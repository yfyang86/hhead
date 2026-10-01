//! Terminal capability detection for `--tui-graphics` and `--tui-caps`.
//!
//! Detection is purely env-based so it works without ioctl access and can be
//! driven hermetically in tests via [`TerminalCaps::detect_with`].
//!
//! Rules (first match wins for graphics):
//!
//! - `TERM_PROGRAM=iTerm.app` → iTerm2 inline images
//! - `KITTY_WINDOW_ID` set / TERM contains `kitty` → Kitty protocol
//! - `TERM_PROGRAM=WezTerm` / `WEZTERM_*` set → Kitty protocol
//! - `TERM_PROGRAM=ghostty` / `GHOSTTY_*` set → Kitty protocol
//! - TERM contains `foot`/`sixel`/`mlterm`/`contour` → sixel
//! - otherwise → no graphics
//!
//! Inside tmux the auto mode resolves to no graphics (escape passthrough is
//! off by default there); a forced `--tui-graphics=PROTO` still emits, with
//! DCS passthrough wrapping applied by the emitter.
//! Truecolor: `COLORTERM` is `truecolor` or `24bit`.
//!
//! A live query-response probe (DA1 sixel bit, CSI 16t/14t geometry) backs the
//! `--tui-caps` report; it is never used in the render path, so piped output
//! stays deterministic.

use std::io::{self, IsTerminal, Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Terminal graphics protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsProto {
    /// Kitty's APC graphics protocol (`\x1b_G...`).
    Kitty,
    /// iTerm2's OSC 1337 inline images.
    ITerm2,
    /// DEC sixel graphics.
    Sixel,
    /// No graphics protocol.
    #[default]
    None,
}

impl GraphicsProto {
    pub fn name(self) -> &'static str {
        match self {
            GraphicsProto::Kitty => "kitty",
            GraphicsProto::ITerm2 => "iterm2",
            GraphicsProto::Sixel => "sixel",
            GraphicsProto::None => "none",
        }
    }
}

/// Detected terminal capabilities.
#[derive(Debug, Clone)]
pub struct TerminalCaps {
    /// Effective choice for `--tui-graphics=auto` (tmux-aware).
    pub graphics: GraphicsProto,
    /// Raw env detection, ignoring tmux.
    pub detected: GraphicsProto,
    pub truecolor: bool,
    pub in_tmux: bool,
    pub term: String,
    pub term_program: String,
    pub colorterm: String,
}

impl TerminalCaps {
    pub fn detect() -> Self {
        Self::detect_with(&|k| std::env::var(k).ok())
    }

    /// Detect capabilities using an injected environment lookup. Exposed so
    /// tests can simulate any terminal without touching process env.
    pub fn detect_with(get: &dyn Fn(&str) -> Option<String>) -> Self {
        let term = get("TERM").unwrap_or_default();
        let term_program = get("TERM_PROGRAM").unwrap_or_default();
        let colorterm = get("COLORTERM").unwrap_or_default();
        let in_tmux = get("TMUX").map(|v| !v.is_empty()).unwrap_or(false);
        let has = |k: &str| get(k).map(|v| !v.is_empty()).unwrap_or(false);
        let t = term.to_lowercase();

        let detected = if term_program == "iTerm.app" {
            GraphicsProto::ITerm2
        } else if has("KITTY_WINDOW_ID") || t.contains("kitty") {
            GraphicsProto::Kitty
        } else if term_program == "WezTerm"
            || has("WEZTERM_EXECUTABLE")
            || has("WEZTERM_PANE")
            || term_program == "ghostty"
            || has("GHOSTTY_RESOURCES_DIR")
            || has("GHOSTTY_BIN_DIR")
        {
            // wezterm and ghostty both implement the kitty graphics protocol.
            GraphicsProto::Kitty
        } else if t.contains("foot")
            || t.contains("sixel")
            || t.contains("mlterm")
            || t.contains("contour")
        {
            GraphicsProto::Sixel
        } else {
            GraphicsProto::None
        };

        let ct = colorterm.to_lowercase();
        let truecolor = ct == "truecolor" || ct == "24bit";

        TerminalCaps {
            graphics: if in_tmux {
                GraphicsProto::None
            } else {
                detected
            },
            detected,
            truecolor,
            in_tmux,
            term,
            term_program,
            colorterm,
        }
    }
}

/// Human label for the terminal program: TERM_PROGRAM if set, else TERM.
pub fn terminal_program_label(caps: &TerminalCaps) -> String {
    if !caps.term_program.is_empty() {
        caps.term_program.clone()
    } else if !caps.term.is_empty() {
        caps.term.clone()
    } else {
        "unknown".to_string()
    }
}

/// Result of the live query-response probe behind `--tui-caps`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProbeResult {
    /// DA1 self-report of sixel support (parameter `4`), if the terminal
    /// answered at all.
    pub sixel_da1: Option<bool>,
    /// Character cell size in pixels `(width, height)` from CSI 16t.
    pub cell_px: Option<(u16, u16)>,
    /// Window size in pixels `(width, height)` from CSI 14t.
    pub window_px: Option<(u16, u16)>,
}

/// Query the terminal directly (DA1, CSI 16t, CSI 14t) and parse the
/// responses. Returns `None` unless stdin and stdout are both TTYs; the
/// caller (`--tui-caps`) is a diagnostic, so a 250 ms ceiling on waiting is
/// acceptable there.
pub fn probe_terminal(timeout: Duration) -> Option<ProbeResult> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return None;
    }
    crossterm::terminal::enable_raw_mode().ok()?;
    let result = probe_roundtrip(timeout);
    let _ = crossterm::terminal::disable_raw_mode();
    Some(result)
}

fn probe_roundtrip(timeout: Duration) -> ProbeResult {
    let mut stdout = io::stdout();
    if stdout.write_all(b"\x1b[c\x1b[16t\x1b[14t").is_err() || stdout.flush().is_err() {
        return ProbeResult::default();
    }

    // Reply bytes arrive on stdin. Drain them on a helper thread; on timeout
    // the thread may stay parked in read(2), which is harmless for the
    // short-lived `--tui-caps` diagnostic.
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut lock = stdin.lock();
        let mut buf = [0u8; 256];
        loop {
            match lock.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut data = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left) {
            Ok(chunk) => data.extend_from_slice(&chunk),
            Err(_) => break,
        }
    }
    parse_probe(&data)
}

/// Parse probe replies: DA1 is `\x1b[?<params>c` (sixel when a parameter is
/// `4`); geometry replies are `\x1b[6;<h>;<w>t` (cell) and `\x1b[4;<h>;<w>t`
/// (window). Tolerant of interleaved or truncated bytes.
fn parse_probe(data: &[u8]) -> ProbeResult {
    let s = String::from_utf8_lossy(data);
    let mut result = ProbeResult::default();

    if let Some(start) = s.find("\x1b[?") {
        let rest = &s[start + 3..];
        if let Some(end) = rest.find('c') {
            let params = &rest[..end];
            result.sixel_da1 = Some(params.split(';').any(|p| p == "4"));
        }
    }
    if let Some((h, w)) = find_csi_pair(&s, '6') {
        result.cell_px = Some((w, h));
    }
    if let Some((h, w)) = find_csi_pair(&s, '4') {
        result.window_px = Some((w, h));
    }
    result
}

/// Find `\x1b[<code>;<a>;<b>t` and return `(a, b)`.
fn find_csi_pair(s: &str, code: char) -> Option<(u16, u16)> {
    let marker = format!("\x1b[{};", code);
    let start = s.find(&marker)? + marker.len();
    let rest = &s[start..];
    let end = rest.find('t')?;
    let (a, b) = rest[..end].split_once(';')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Print the `--tui-caps` diagnostic report.
pub fn print_caps_report() {
    let caps = TerminalCaps::detect();
    let yes_no = |b: bool| if b { "yes" } else { "no" };

    println!("hhead tui caps: detected terminal capabilities");
    println!("  terminal program: {}", terminal_program_label(&caps));
    println!("  graphics protocol: {}", caps.detected.name());
    if caps.in_tmux && caps.detected != GraphicsProto::None {
        println!(
            "  auto mode resolves to: none (inside tmux; force with --tui-graphics={})",
            caps.detected.name()
        );
    } else {
        println!("  auto mode resolves to: {}", caps.graphics.name());
    }
    println!("  truecolor: {}", yes_no(caps.truecolor));

    match crossterm::terminal::size() {
        Ok((cols, rows)) => println!("  window size: {cols}x{rows} cells"),
        Err(_) => println!("  window size: unknown"),
    }
    match crossterm::terminal::window_size() {
        Ok(ws) if ws.width > 0 && ws.height > 0 => {
            println!("  window pixels: {}x{}", ws.width, ws.height)
        }
        _ => println!("  window pixels: unknown"),
    }

    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        match probe_terminal(Duration::from_millis(250)) {
            Some(probe) => {
                let sixel = match probe.sixel_da1 {
                    Some(b) => yes_no(b).to_string(),
                    None => "no reply".to_string(),
                };
                println!("  probe DA1 sixel: {sixel}");
                match probe.cell_px {
                    Some((w, h)) => println!("  probe cell size: {w}x{h} px"),
                    None => println!("  probe cell size: no reply"),
                }
                match probe.window_px {
                    Some((w, h)) => println!("  probe window size: {w}x{h} px"),
                    None => println!("  probe window size: no reply"),
                }
            }
            None => println!("  probe: unavailable"),
        }
    } else {
        println!("  probe: skipped (not a TTY)");
    }

    fn show(v: &str) -> &str {
        if v.is_empty() { "<unset>" } else { v }
    }
    println!(
        "  env: TERM={} TERM_PROGRAM={} COLORTERM={} TMUX={}",
        show(&caps.term),
        show(&caps.term_program),
        show(&caps.colorterm),
        if caps.in_tmux { "<set>" } else { "<unset>" },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn test_detect_iterm2() {
        let caps = TerminalCaps::detect_with(&env(&[
            ("TERM_PROGRAM", "iTerm.app"),
            ("TERM", "xterm-256color"),
        ]));
        assert_eq!(caps.graphics, GraphicsProto::ITerm2);
        assert_eq!(caps.detected, GraphicsProto::ITerm2);
    }

    #[test]
    fn test_detect_kitty_by_env() {
        let caps = TerminalCaps::detect_with(&env(&[("KITTY_WINDOW_ID", "1")]));
        assert_eq!(caps.graphics, GraphicsProto::Kitty);
    }

    #[test]
    fn test_detect_kitty_by_term() {
        let caps = TerminalCaps::detect_with(&env(&[("TERM", "xterm-kitty")]));
        assert_eq!(caps.graphics, GraphicsProto::Kitty);
    }

    #[test]
    fn test_detect_wezterm_is_kitty_protocol() {
        let caps = TerminalCaps::detect_with(&env(&[("TERM_PROGRAM", "WezTerm")]));
        assert_eq!(caps.graphics, GraphicsProto::Kitty);
        let caps = TerminalCaps::detect_with(&env(&[("WEZTERM_PANE", "0")]));
        assert_eq!(caps.graphics, GraphicsProto::Kitty);
    }

    #[test]
    fn test_detect_ghostty_is_kitty_protocol() {
        let caps = TerminalCaps::detect_with(&env(&[("TERM_PROGRAM", "ghostty")]));
        assert_eq!(caps.graphics, GraphicsProto::Kitty);
        let caps = TerminalCaps::detect_with(&env(&[("GHOSTTY_BIN_DIR", "/x")]));
        assert_eq!(caps.graphics, GraphicsProto::Kitty);
    }

    #[test]
    fn test_detect_sixel_terminals() {
        for term in ["foot", "foot-extra", "xterm-sixel", "mlterm", "contour"] {
            let caps = TerminalCaps::detect_with(&env(&[("TERM", term)]));
            assert_eq!(caps.graphics, GraphicsProto::Sixel, "TERM={term}");
        }
    }

    #[test]
    fn test_detect_plain_terminal_has_no_graphics() {
        let caps = TerminalCaps::detect_with(&env(&[("TERM", "xterm-256color")]));
        assert_eq!(caps.graphics, GraphicsProto::None);
    }

    #[test]
    fn test_tmux_disables_auto_graphics() {
        let caps = TerminalCaps::detect_with(&env(&[
            ("TERM_PROGRAM", "iTerm.app"),
            ("TMUX", "/tmp/tmux-1000/default,1234,0"),
        ]));
        assert_eq!(caps.detected, GraphicsProto::ITerm2);
        assert_eq!(caps.graphics, GraphicsProto::None);
        assert!(caps.in_tmux);
    }

    #[test]
    fn test_truecolor() {
        let caps = TerminalCaps::detect_with(&env(&[("COLORTERM", "truecolor")]));
        assert!(caps.truecolor);
        let caps = TerminalCaps::detect_with(&env(&[("COLORTERM", "24bit")]));
        assert!(caps.truecolor);
        let caps = TerminalCaps::detect_with(&env(&[("COLORTERM", "yes")]));
        assert!(!caps.truecolor);
        let caps = TerminalCaps::detect_with(&env(&[]));
        assert!(!caps.truecolor);
    }

    #[test]
    fn test_terminal_program_label() {
        let caps = TerminalCaps::detect_with(&env(&[("TERM_PROGRAM", "WezTerm")]));
        assert_eq!(terminal_program_label(&caps), "WezTerm");
        let caps = TerminalCaps::detect_with(&env(&[("TERM", "foot")]));
        assert_eq!(terminal_program_label(&caps), "foot");
        let caps = TerminalCaps::detect_with(&env(&[]));
        assert_eq!(terminal_program_label(&caps), "unknown");
    }

    #[test]
    fn test_parse_probe_sixel_da1() {
        let r = parse_probe(b"\x1b[?62;4;22c");
        assert_eq!(r.sixel_da1, Some(true));
        let r = parse_probe(b"\x1b[?62;22c");
        assert_eq!(r.sixel_da1, Some(false));
        let r = parse_probe(b"garbage");
        assert_eq!(r.sixel_da1, None);
    }

    #[test]
    fn test_parse_probe_geometry() {
        let r = parse_probe(b"\x1b[6;32;16t\x1b[4;1024;768t");
        assert_eq!(r.cell_px, Some((16, 32)));
        assert_eq!(r.window_px, Some((768, 1024)));
    }

    #[test]
    fn test_parse_probe_interleaved() {
        let r = parse_probe(b"\x1b[?1;2c\x1b[6;20;10t");
        assert_eq!(r.sixel_da1, Some(false));
        assert_eq!(r.cell_px, Some((10, 20)));
        assert_eq!(r.window_px, None);
    }
}
