//! Markdown rendering for the terminal
//!
//! A lightweight, line-based renderer: headings, fenced code blocks,
//! GFM-style pipe tables (aligned and padded), and figures — `![alt](path)`
//! on its own line is drawn as a 256-color minimap. Inline emphasis, code,
//! and link markers are ANSI-styled when color is enabled and stripped
//! otherwise. It is intentionally not a full CommonMark implementation.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use colored::Colorize;

use super::minimap::write_minimap;

/// Render a Markdown file to stdout.
///
/// Reads the whole file: rendering needs the complete document, so the
/// `--bytes` limit does not apply in Markdown mode. Figures are resolved
/// relative to the Markdown file's directory and drawn on an
/// `img_rows` × `img_cols` grid.
pub fn display_markdown(
    path: &Path,
    color: bool,
    img_rows: usize,
    img_cols: usize,
    rainbow: bool,
) -> io::Result<()> {
    let data = std::fs::read(path)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_markdown(
        &mut out,
        &data,
        path.parent(),
        color,
        img_rows,
        img_cols,
        rainbow,
    )
}

/// Same as [`display_markdown`] but writes to an arbitrary [`Write`] and
/// takes the already-read bytes. Exposed for testing.
///
/// `rainbow` paints table columns with the `--csv-rainbow` palette; cells
/// are then rendered with inline markers stripped (no nested ANSI), so the
/// column color runs the full cell.
pub fn write_markdown<W: Write>(
    out: &mut W,
    data: &[u8],
    base_dir: Option<&Path>,
    color: bool,
    img_rows: usize,
    img_cols: usize,
    rainbow: bool,
) -> io::Result<()> {
    let text = String::from_utf8_lossy(data);
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut in_code = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Fenced code blocks: content passes through verbatim.
        if trimmed.starts_with("```") {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if in_code {
            writeln!(out, "{}", line)?;
            i += 1;
            continue;
        }

        if trimmed.is_empty() {
            writeln!(out)?;
            i += 1;
            continue;
        }

        // Figure: a line that is just `![alt](src)`.
        if let Some((alt, src)) = parse_image_line(trimmed) {
            render_figure(out, &alt, &src, base_dir, color, img_rows, img_cols)?;
            i += 1;
            continue;
        }

        // GFM table: header row, separator row, then body rows.
        if trimmed.contains('|') && i + 1 < lines.len() && is_separator_row(lines[i + 1]) {
            let mut body: Vec<&str> = Vec::new();
            let mut j = i + 2;
            while j < lines.len() && lines[j].trim().contains('|') {
                body.push(lines[j]);
                j += 1;
            }
            render_table(out, trimmed, lines[i + 1], &body, color, rainbow)?;
            i = j;
            continue;
        }

        // Heading: 1-6 '#' followed by a space (or nothing).
        if let Some(heading) = parse_heading(trimmed) {
            writeln!(out, "{}", style(&heading, Style::Heading, color))?;
            i += 1;
            continue;
        }

        // Everything else (lists, quotes, rules, paragraphs) passes through
        // the inline renderer.
        writeln!(out, "{}", render_inline(line, color))?;
        i += 1;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Heading,
    Bold,
    Italic,
    Code,
}

fn style(text: &str, s: Style, color: bool) -> String {
    if !color {
        return text.to_string();
    }
    match s {
        Style::Heading => text.bold().cyan().to_string(),
        Style::Bold => text.bold().to_string(),
        Style::Italic => text.italic().to_string(),
        Style::Code => text.yellow().to_string(),
    }
}

/// Parse a line consisting solely of `![alt](src)` (optional `"title"`).
fn parse_image_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = &rest[..close];
    let src = rest[close + 2..].strip_suffix(')')?;
    // Optional title: ![alt](src "title")
    let src = match src.split_once(' ') {
        Some((path, title)) if title.trim_start().starts_with('"') => path,
        _ => src,
    };
    Some((alt.trim().to_string(), src.trim().to_string()))
}

fn render_figure<W: Write>(
    out: &mut W,
    alt: &str,
    src: &str,
    base_dir: Option<&Path>,
    color: bool,
    rows: usize,
    cols: usize,
) -> io::Result<()> {
    let caption = if alt.is_empty() { src } else { alt };
    writeln!(out, "{}", render_inline(caption, color))?;

    if src.starts_with("http://") || src.starts_with("https://") {
        return writeln!(out, "[remote image not rendered: {}]", src);
    }

    let resolved = match base_dir {
        Some(dir) => dir.join(src),
        None => PathBuf::from(src),
    };
    match write_minimap(out, &resolved, rows, cols) {
        Ok(()) => Ok(()),
        Err(_) => writeln!(out, "[image not rendered: {}]", src),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
    Center,
}

/// A separator row like `| --- | :-: | --: |` (also a bare `---`).
fn is_separator_row(line: &str) -> bool {
    let t = line.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '-' | ':' | ' ' | '|' | '\t'))
}

/// Split a pipe-table row into trimmed cells. `\|` stays a literal pipe.
fn split_row(line: &str) -> Vec<String> {
    const ESC: &str = "\u{0}";
    let mut t = line.trim().replace("\\|", ESC);
    if t.starts_with('|') {
        t.remove(0);
    }
    if t.ends_with('|') {
        t.pop();
    }
    t.split('|').map(|c| c.trim().replace(ESC, "|")).collect()
}

fn parse_aligns(sep_line: &str, ncols: usize) -> Vec<Align> {
    let cells = split_row(sep_line);
    (0..ncols)
        .map(|j| {
            let cell = cells.get(j).map(|s| s.trim()).unwrap_or("");
            match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            }
        })
        .collect()
}

fn render_table<W: Write>(
    out: &mut W,
    header_line: &str,
    sep_line: &str,
    body_lines: &[&str],
    color: bool,
    rainbow: bool,
) -> io::Result<()> {
    let header = split_row(header_line);
    let body: Vec<Vec<String>> = body_lines.iter().map(|l| split_row(l)).collect();

    let ncols = std::iter::once(header.len())
        .chain(body.iter().map(|r| r.len()))
        .max()
        .unwrap_or(0);
    if ncols == 0 {
        return Ok(());
    }

    // Column widths come from the *plain* text (markers stripped, no ANSI),
    // so styling never throws off the padding.
    let mut widths = vec![0usize; ncols];
    for row in std::iter::once(&header).chain(body.iter()) {
        for (j, cell) in row.iter().enumerate() {
            widths[j] = widths[j].max(render_inline(cell, false).chars().count());
        }
    }
    let aligns = parse_aligns(sep_line, ncols);

    let render_row = |cells: &[String], plain: bool, header_row: bool| -> String {
        let mut line = String::from("|");
        for (j, width) in widths.iter().enumerate() {
            let cell = cells.get(j).map(String::as_str).unwrap_or("");
            let plain_len = render_inline(cell, false).chars().count();
            // Rainbow paints the whole (marker-stripped) cell in its column
            // color — inline ANSI inside the cell would reset it mid-cell.
            let rendered = if rainbow {
                let painted = render_inline(cell, false).color(super::csv::rainbow_color(j));
                let painted = if header_row { painted.bold() } else { painted };
                painted.to_string()
            } else if plain {
                render_inline(cell, false)
            } else {
                render_inline(cell, color)
            };
            line.push(' ');
            line.push_str(&pad_cell(&rendered, plain_len, *width, aligns[j]));
            line.push_str(" |");
        }
        line
    };

    // Header cells are rendered plain so the whole line can be bolded
    // without nested ANSI resets cancelling the style mid-row; in rainbow
    // mode each cell is bolded individually inside its column color instead.
    let header_out = render_row(&header, true, true);
    if rainbow {
        writeln!(out, "{}", header_out)?;
    } else {
        writeln!(out, "{}", style(&header_out, Style::Bold, color))?;
    }

    let mut divider = String::from("|");
    for width in &widths {
        divider.push_str(&"-".repeat(width + 2));
        divider.push('|');
    }
    writeln!(out, "{}", divider)?;

    for row in &body {
        writeln!(out, "{}", render_row(row, false, false))?;
    }
    Ok(())
}

fn pad_cell(rendered: &str, plain_len: usize, width: usize, align: Align) -> String {
    let pad = width.saturating_sub(plain_len);
    match align {
        Align::Left => format!("{}{}", rendered, " ".repeat(pad)),
        Align::Right => format!("{}{}", " ".repeat(pad), rendered),
        Align::Center => {
            let left = pad / 2;
            format!("{}{}{}", " ".repeat(left), rendered, " ".repeat(pad - left))
        }
    }
}

/// Heading text without the leading `#`s, or `None` if not a heading.
fn parse_heading(line: &str) -> Option<String> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None; // "#tag" is not a heading
    }
    let text = rest.trim();
    // Strip optional closing hashes: "## Title ##"
    Some(text.trim_end_matches('#').trim_end().to_string())
}

/// Render inline Markdown: code spans, links, images, bold, italic.
/// With `color` the text is ANSI-styled; without it markers are stripped.
fn render_inline(text: &str, color: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        // Code span: `...`
        if chars[i] == '`'
            && let Some(j) = find_char(&chars, i + 1, '`')
        {
            let content: String = chars[i + 1..j].iter().collect();
            out.push_str(&style(&content, Style::Code, color));
            i = j + 1;
            continue;
        }

        // Inline image: ![alt](src) renders as its alt text + source.
        if chars[i] == '!'
            && chars.get(i + 1) == Some(&'[')
            && let Some((label, url, next)) = parse_link_at(&chars, i + 1)
        {
            out.push_str(&format_link(&label, &url, color));
            i = next;
            continue;
        }

        // Link: [text](url)
        if chars[i] == '['
            && let Some((label, url, next)) = parse_link_at(&chars, i)
        {
            out.push_str(&format_link(&label, &url, color));
            i = next;
            continue;
        }

        // Bold: **...** or __...__ (checked before single-marker italic).
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*')
            || chars[i] == '_' && chars.get(i + 1) == Some(&'_')
        {
            let marker = [chars[i], chars[i + 1]];
            if let Some(j) = find_seq(&chars, i + 2, &marker)
                && j > i + 2
            {
                let content: String = chars[i + 2..j].iter().collect();
                out.push_str(&style(&render_inline(&content, color), Style::Bold, color));
                i = j + 2;
                continue;
            }
        }

        // Italic: *...* or _..._. '_' must sit on a word boundary so
        // snake_case identifiers stay literal.
        if chars[i] == '*' || chars[i] == '_' {
            let opener_ok = chars[i] == '*' || i == 0 || !chars[i - 1].is_alphanumeric();
            if opener_ok && let Some(j) = find_char(&chars, i + 1, chars[i]) {
                let closer_ok =
                    chars[i] == '*' || j + 1 == chars.len() || !chars[j + 1].is_alphanumeric();
                if j > i + 1 && closer_ok {
                    let content: String = chars[i + 1..j].iter().collect();
                    out.push_str(&style(
                        &render_inline(&content, color),
                        Style::Italic,
                        color,
                    ));
                    i = j + 1;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Parse `[label](url)` with chars[i] == '['.
/// Returns (label, url, index just past the closing ')').
fn parse_link_at(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    let close = find_char(chars, i + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find_char(chars, close + 2, ')')?;
    let label: String = chars[i + 1..close].iter().collect();
    let url: String = chars[close + 2..end].iter().collect();
    Some((label, url, end + 1))
}

fn format_link(label: &str, url: &str, color: bool) -> String {
    let show_url = !url.is_empty() && url != label;
    let label = render_inline(label, color);
    let label = if color {
        label.underline().to_string()
    } else {
        label
    };
    if show_url {
        format!("{} ({})", label, url)
    } else {
        label
    }
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    (start..chars.len()).find(|&k| chars[k] == target)
}

fn find_seq(chars: &[char], start: usize, seq: &[char]) -> Option<usize> {
    if seq.is_empty() || chars.len() < seq.len() {
        return None;
    }
    (start..=chars.len() - seq.len()).find(|&k| chars[k..k + seq.len()] == *seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(text: &str, color: bool) -> String {
        let mut buf = Vec::new();
        write_markdown(&mut buf, text.as_bytes(), None, color, 4, 6, false)
            .expect("write_markdown should not fail");
        String::from_utf8(buf).expect("output should be valid utf-8")
    }

    #[test]
    fn test_table_rainbow_columns() {
        let _guard = crate::COLOR_TEST_LOCK.lock().unwrap();
        colored::control::set_override(true);
        let md = "| Name | Age |\n| ---- | --- |\n| Al | 9 |\n";
        let mut buf = Vec::new();
        write_markdown(&mut buf, md.as_bytes(), None, false, 4, 6, true)
            .expect("write_markdown should not fail");
        colored::control::unset_override();
        let out = String::from_utf8(buf).unwrap();
        // Column 0 cyan (36), column 1 yellow (33), in header and body.
        assert!(out.contains("\x1b[36m"), "cyan column: {out:?}");
        assert!(out.contains("\x1b[33m"), "yellow column: {out:?}");
        assert!(out.contains("Al"));
        // The divider row stays unpainted.
        assert!(
            out.lines()
                .any(|l| l.starts_with("|--") || l.starts_with("|-"))
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(capture("", false), "");
    }

    #[test]
    fn test_heading_markers_stripped() {
        let out = capture("# Title\n## Sub\n#tag is not a heading\n", false);
        assert!(out.contains("Title\n"));
        assert!(out.contains("Sub\n"));
        assert!(out.contains("#tag is not a heading\n"));
    }

    #[test]
    fn test_paragraph_passthrough() {
        assert_eq!(capture("hello world\n", false), "hello world\n");
    }

    #[test]
    fn test_code_fence_verbatim() {
        let out = capture("```\n# not a heading\n**raw**\n```\nafter\n", false);
        assert!(out.contains("# not a heading\n"));
        assert!(out.contains("**raw**\n"));
        assert!(out.ends_with("after\n"));
        assert!(!out.contains("```"));
    }

    #[test]
    fn test_inline_markers_stripped_without_color() {
        let out = capture(
            "a **bold** and *ital* and `code` and [lbl](http://x)\n",
            false,
        );
        assert!(out.contains("a bold and ital and code and lbl (http://x)\n"));
    }

    #[test]
    fn test_snake_case_not_italic() {
        let out = capture("use foo_bar_baz here\n", false);
        assert!(out.contains("foo_bar_baz"));
    }

    #[test]
    fn test_table_alignment_and_padding() {
        let md = "| Name | Age |\n| ---- | --: |\n| Al | 9 |\n| Bob | 10 |\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| Name | Age |");
        assert_eq!(lines[1], "|------|-----|");
        assert_eq!(lines[2], "| Al   |   9 |");
        assert_eq!(lines[3], "| Bob  |  10 |");
    }

    #[test]
    fn test_table_center_alignment() {
        let md = "| a |\n| :-: |\n| bb |\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| a  |");
        assert_eq!(lines[1], "|----|");
        assert_eq!(lines[2], "| bb |");
    }

    #[test]
    fn test_table_escaped_pipe() {
        let md = "| a \\| b |\n| --- |\n| c |\n";
        let out = capture(md, false);
        assert!(out.contains("a | b"));
    }

    #[test]
    fn test_table_requires_separator() {
        let out = capture("a | b\nnot a table\n", false);
        assert_eq!(out, "a | b\nnot a table\n");
    }

    #[test]
    fn test_table_strips_inline_markers_in_cells() {
        let md = "| h |\n| --- |\n| **b** |\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| h |");
        assert_eq!(lines[2], "| b |");
    }

    #[test]
    fn test_missing_image_placeholder() {
        let out = capture("![pic](nope.png)\n", false);
        assert!(out.contains("pic\n"));
        assert!(out.contains("[image not rendered: nope.png]"));
    }

    #[test]
    fn test_remote_image_not_rendered() {
        let out = capture("![alt](https://example.com/x.png)\n", false);
        assert!(out.contains("[remote image not rendered: https://example.com/x.png]"));
    }

    #[test]
    fn test_image_rendered_with_minimap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let img_path = dir.path().join("pic.png");
        let mut img = image::RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.save(&img_path).expect("save png");

        let mut buf = Vec::new();
        write_markdown(
            &mut buf,
            b"![cap](pic.png)\n",
            Some(dir.path()),
            false,
            2,
            2,
            false,
        )
        .expect("write_markdown should not fail");
        let out = String::from_utf8(buf).expect("output should be valid utf-8");
        assert!(out.contains("cap\n"));
        assert!(out.contains("\x1b[38;5;"), "minimap ANSI missing: {out}");
        assert!(out.contains('█'));
    }

    #[test]
    fn test_color_output_contains_ansi() {
        // The `colored` crate auto-disables ANSI for non-TTY writers; override
        // it so this test sees escape sequences regardless of stdio capture.
        let _guard = crate::COLOR_TEST_LOCK.lock().unwrap();
        colored::control::set_override(true);
        let out = capture("# Title\n**b** and `c`\n", true);
        colored::control::unset_override();
        assert!(
            out.contains("\x1b["),
            "colored output should contain ANSI escape: {out}"
        );
    }
}
