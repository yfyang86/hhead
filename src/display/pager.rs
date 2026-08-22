//! Interactive `less`-style pager for terminal output.
//!
//! [`run_pager`] shows pre-formatted text (which may already contain ANSI
//! color escapes from the other display modules) full-screen on an alternate
//! terminal screen and lets the user scroll and search, then restores the
//! terminal on quit — a minimal built-in `less`, so no external pager is
//! needed and the other `--*` options keep working.
//!
//! The pure helpers ([`visible_width`], [`truncate_ansi`], [`find_matches`],
//! [`clamp_offset`]) are kept separate from the I/O loop so they can be unit
//! tested without a terminal.

use std::borrow::Cow;
use std::io::{self, IsTerminal, Write};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size,
};
use crossterm::{execute, queue};

/// Page `content` interactively, `less`-style.
///
/// `title` is shown in the status line (typically the input file path).
///
/// If stdin or stdout is not a terminal there is nothing to page, so the
/// content is written out in full instead — this keeps piped invocations
/// (and integration tests) deterministic.
pub fn run_pager(content: &str, title: &str) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        // Ignore broken-pipe / write errors, matching the other display fns.
        let _ = out.write_all(content.as_bytes());
        return Ok(());
    }

    let lines: Vec<&str> = content.lines().collect();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    enable_raw_mode()?;
    execute!(out, EnterAlternateScreen)?;
    let _guard = TerminalGuard;
    pager_loop(&mut out, &lines, title)
}

/// Restores the terminal (alternate screen + raw mode) no matter how the
/// pager loop exits, including panics and early errors.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let _ = queue!(out, LeaveAlternateScreen);
        let _ = out.flush();
        let _ = disable_raw_mode();
    }
}

/// Active search: the pattern plus every matching line index.
struct SearchState {
    pattern: String,
    matches: Vec<usize>,
    current: usize,
}

/// The main interactive loop. Runs with raw mode + alternate screen active.
fn pager_loop(out: &mut impl Write, lines: &[&str], title: &str) -> io::Result<()> {
    let mut offset = 0usize;
    let mut search: Option<SearchState> = None;
    let mut input: Option<String> = None;

    loop {
        let (cols, rows) = size().unwrap_or((80, 24));
        let cols = cols.max(1) as usize;
        let view_h = (rows.saturating_sub(1)).max(1) as usize;

        if !lines.is_empty() {
            offset = clamp_offset(offset, lines.len(), view_h);
        }

        // Redraw the visible window and the status line.
        queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;
        let end = (offset + view_h).min(lines.len());
        for (row, line) in lines[offset..end].iter().enumerate() {
            queue!(out, MoveTo(0, row as u16), Print(truncate_ansi(line, cols)))?;
        }
        let status = status_text(
            title,
            offset,
            end,
            lines.len(),
            &search,
            input.as_deref(),
            lines,
        );
        draw_status(out, cols, rows, &status)?;
        out.flush()?;

        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Release => continue,
            Event::Key(k) => {
                if let Some(buf) = input.as_mut() {
                    match k.code {
                        KeyCode::Char(c) => buf.push(c),
                        KeyCode::Backspace => {
                            buf.pop();
                        }
                        KeyCode::Esc => input = None,
                        KeyCode::Enter => {
                            let pat = buf.trim().to_string();
                            if pat.is_empty() {
                                input = None;
                            } else {
                                let matches = find_matches(lines, &pat);
                                if !matches.is_empty() {
                                    offset = matches[0];
                                    search = Some(SearchState {
                                        pattern: pat,
                                        matches,
                                        current: 0,
                                    });
                                    input = None;
                                }
                                // No matches: keep the prompt open so the
                                // pattern can be edited; the status line
                                // flags it.
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                match k.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                        offset = offset.saturating_add(1);
                    }
                    KeyCode::Up | KeyCode::Char('k') => offset = offset.saturating_sub(1),
                    KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => {
                        offset = offset.saturating_add(view_h);
                    }
                    KeyCode::PageUp | KeyCode::Char('b') => {
                        offset = offset.saturating_sub(view_h);
                    }
                    KeyCode::Home | KeyCode::Char('g') => offset = 0,
                    KeyCode::End | KeyCode::Char('G') => offset = usize::MAX,
                    KeyCode::Char('/') => input = Some(String::new()),
                    KeyCode::Char('n') => {
                        if let Some(s) = search.as_mut() {
                            advance_match(s, 1, &mut offset);
                        }
                    }
                    KeyCode::Char('N') => {
                        if let Some(s) = search.as_mut() {
                            advance_match(s, -1, &mut offset);
                        }
                    }
                    _ => {}
                }
            }
            // Resize and everything else: the next loop iteration re-reads
            // the terminal size and repaints.
            _ => {}
        }
    }
    Ok(())
}

/// Move to the next (or previous, when `dir < 0`) search match, wrapping
/// around the match list, and scroll to it.
fn advance_match(search: &mut SearchState, dir: isize, offset: &mut usize) {
    if search.matches.is_empty() {
        return;
    }
    let n = search.matches.len() as isize;
    let mut idx = search.current as isize + dir;
    if idx < 0 {
        idx = n - 1;
    }
    if idx >= n {
        idx = 0;
    }
    search.current = idx as usize;
    *offset = search.matches[search.current];
}

/// Build the status line: file, visible range, percentage, and the active
/// search / prompt state.
fn status_text(
    title: &str,
    offset: usize,
    end: usize,
    total: usize,
    search: &Option<SearchState>,
    input: Option<&str>,
    lines: &[&str],
) -> String {
    if let Some(buf) = input {
        let mut s = format!("hhead: {title}  /{buf}");
        let pat = buf.trim();
        if !pat.is_empty() {
            let lower = pat.to_lowercase();
            let found = lines.iter().any(|l| l.to_lowercase().contains(&lower));
            if !found {
                s.push_str("  (no matches)");
            }
        }
        s.push_str("  [esc] cancel  [enter] search");
        return s;
    }

    if total == 0 {
        return format!("hhead: {title}  (empty)  [q] quit");
    }

    let pct = end * 100 / total;
    let mut s = format!(
        "hhead: {title}  lines {}-{} of {} ({}%)",
        offset + 1,
        end,
        total,
        pct
    );
    if let Some(sr) = search {
        s.push_str(&format!(
            "  /{} {}/{}",
            sr.pattern,
            sr.current + 1,
            sr.matches.len()
        ));
    }
    s.push_str("  [q]uit  [/]search  [j/k]scroll  [g/G]top/bottom  [n/N]match");
    s
}

/// Draw the reverse-video status line on the last row, padded to the full
/// width and truncated to `cols`.
fn draw_status(out: &mut impl Write, cols: usize, rows: u16, status: &str) -> io::Result<()> {
    let status = truncate_ansi(status, cols);
    let pad = cols.saturating_sub(visible_width(&status));
    queue!(
        out,
        MoveTo(0, rows.saturating_sub(1)),
        SetAttribute(Attribute::Reverse),
        Print(&*status),
        Print(" ".repeat(pad)),
        SetAttribute(Attribute::Reset),
    )
}

/// Visible character count of `s`, ignoring ANSI escape sequences.
pub fn visible_width(s: &str) -> usize {
    let mut chars = s.chars().peekable();
    let mut width = 0;
    while let Some(&c) = chars.peek() {
        if c == '\x1b' {
            skip_escape(&mut chars);
        } else {
            chars.next();
            width += 1;
        }
    }
    width
}

/// Truncate `s` to `width` visible characters, never splitting an ANSI
/// escape sequence. Appends a reset (`\x1b[0m`) when content was cut so the
/// terminal cannot be left in a colored state. Returns the input unchanged
/// when no truncation is needed.
pub fn truncate_ansi(s: &str, width: usize) -> Cow<'_, str> {
    let mut chars = s.chars().peekable();
    let mut out = String::with_capacity(s.len());
    let mut visible = 0;
    while let Some(&c) = chars.peek() {
        if c == '\x1b' {
            copy_escape(&mut chars, &mut out);
        } else if visible >= width {
            out.push_str("\x1b[0m");
            return Cow::Owned(out);
        } else {
            out.push(c);
            chars.next();
            visible += 1;
        }
    }
    Cow::Borrowed(s)
}

/// Consume one ANSI escape sequence from a peekable char iterator.
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let mut consumed = String::new();
    copy_escape(chars, &mut consumed);
}

/// Copy one ANSI escape sequence (ESC, optional `[`…final byte) into `out`.
fn copy_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    if chars.next() != Some('\x1b') {
        return;
    }
    out.push('\x1b');
    if chars.peek() == Some(&'[') {
        out.push('[');
        chars.next();
        // CSI: parameter/intermediate bytes (0x20–0x3f) then one final
        // byte (0x40–0x7e).
        while let Some(&c) = chars.peek() {
            out.push(c);
            chars.next();
            if ('\x40'..='\x7e').contains(&c) {
                break;
            }
        }
    } else if let Some(&c) = chars.peek().filter(|&&c| !c.is_control()) {
        out.push(c);
        chars.next();
    }
}

/// Indices of lines containing `pattern`, case-insensitive.
pub fn find_matches(lines: &[&str], pattern: &str) -> Vec<usize> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let lower = pattern.to_lowercase();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.to_lowercase().contains(&lower))
        .map(|(i, _)| i)
        .collect()
}

/// Clamp a scroll offset so the last `view_h` lines stay reachable.
pub fn clamp_offset(offset: usize, total: usize, view_h: usize) -> usize {
    if view_h == 0 || total <= view_h {
        return 0;
    }
    offset.min(total - view_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visible_width_plain() {
        assert_eq!(visible_width("abc"), 3);
        assert_eq!(visible_width(""), 0);
        assert_eq!(visible_width("héllo"), 5);
    }

    #[test]
    fn test_visible_width_ignores_ansi() {
        let s = "\x1b[31mred\x1b[0m";
        assert_eq!(visible_width(s), 3);
        let s2 = "\x1b[38;5;196m█\x1b[0m";
        assert_eq!(visible_width(s2), 1);
    }

    #[test]
    fn test_truncate_ansi_no_truncation_borrows() {
        let s = "abc";
        assert_eq!(truncate_ansi(s, 10), Cow::Borrowed(s));
    }

    #[test]
    fn test_truncate_ansi_cuts_visible_chars() {
        assert_eq!(
            truncate_ansi("abcdef", 3),
            Cow::<'_, str>::Owned("abc\x1b[0m".to_string())
        );
    }

    #[test]
    fn test_truncate_ansi_preserves_escape_sequences() {
        // Cut inside a colored run: the escape must survive intact and a
        // reset is appended so the terminal is not left colored.
        let s = "ab\x1b[31mcd";
        assert_eq!(
            truncate_ansi(s, 3),
            Cow::<'_, str>::Owned("ab\x1b[31mc\x1b[0m".to_string())
        );
    }

    #[test]
    fn test_truncate_ansi_zero_width() {
        assert_eq!(
            truncate_ansi("abc", 0),
            Cow::<'_, str>::Owned("\x1b[0m".to_string())
        );
    }

    #[test]
    fn test_find_matches_case_insensitive() {
        let lines: Vec<&str> = vec!["Hello World", "goodbye", "hello there", "HELLO!"];
        assert_eq!(find_matches(&lines, "hello"), vec![0, 2, 3]);
        assert_eq!(find_matches(&lines, "zzz"), Vec::<usize>::new());
        assert_eq!(find_matches(&lines, ""), Vec::<usize>::new());
    }

    #[test]
    fn test_find_matches_ignores_ansi_codes_in_pattern() {
        // The pattern matches against the raw (escaped) line; ANSI sequences
        // are part of the line but a plain text pattern still finds text.
        let lines: Vec<&str> = vec!["\x1b[31mred alert\x1b[0m"];
        assert_eq!(find_matches(&lines, "alert"), vec![0]);
    }

    #[test]
    fn test_clamp_offset_bounds() {
        assert_eq!(clamp_offset(0, 10, 5), 0);
        assert_eq!(clamp_offset(3, 10, 5), 3);
        assert_eq!(clamp_offset(100, 10, 5), 5);
        assert_eq!(clamp_offset(100, 10, 0), 0);
        assert_eq!(clamp_offset(0, 3, 10), 0);
        assert_eq!(clamp_offset(5, 0, 10), 0);
    }
}
