//! Rainbow CSV rendering: each column in its own color.
//!
//! The input text is emitted with its layout untouched — no realignment —
//! but every field is painted by its column index, cycling the [`RAINBOW`]
//! palette (the same palette the Markdown table renderer uses for
//! `--csv-rainbow`). Delimiters and newlines stay unpainted. Quoting is
//! CSV-aware: a quoted field keeps its delimiters, doubled quotes, and even
//! embedded newlines in one field's color. The delimiter is sniffed from
//! the first line (`,`, tab, or `;`).

use colored::{Color, Colorize};
use std::io::{self, Write};
use std::path::Path;

/// Per-column palette, cycled by column index.
pub const RAINBOW: [Color; 6] = [
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Magenta,
    Color::Blue,
    Color::Red,
];

/// The palette color for a column index (wraps around).
pub fn rainbow_color(col: usize) -> Color {
    RAINBOW[col % RAINBOW.len()]
}

/// Sniff the field delimiter from the first line: whichever of `,`, tab,
/// `;` occurs most often outside quotes. Comma wins ties and empty input.
fn sniff_delimiter(text: &str) -> char {
    const DELIMS: [char; 3] = [',', '\t', ';'];
    let first = text.lines().next().unwrap_or("");
    let mut counts = [0usize; 3];
    let mut in_quotes = false;
    for c in first.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && let Some(i) = DELIMS.iter().position(|&d| d == c) {
            counts[i] += 1;
        }
    }
    let best = (0..DELIMS.len()).max_by_key(|&i| counts[i]).unwrap_or(0);
    if counts[best] == 0 { ',' } else { DELIMS[best] }
}

/// Render a whole CSV/TSV file with rainbow columns. Reads the whole file
/// (like Markdown mode, the `--bytes` limit does not apply) and locks
/// stdout so the output is one atomic stream.
pub fn display_csv_rainbow(path: &Path) -> io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_csv_rainbow(&mut out, &text)
}

/// Same as [`display_csv_rainbow`] but writes to an arbitrary [`Write`] and
/// takes the already-read text. Exposed for testing and the pager path.
pub fn write_csv_rainbow<W: Write>(out: &mut W, text: &str) -> io::Result<()> {
    let delim = sniff_delimiter(text);
    let mut col = 0usize;
    let mut in_quotes = false;
    let mut field = String::new();

    fn flush<W: Write>(out: &mut W, field: &mut String, col: usize) -> io::Result<()> {
        if !field.is_empty() {
            write!(out, "{}", field.color(rainbow_color(col)))?;
            field.clear();
        }
        Ok(())
    }

    for c in text.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                field.push(c);
            }
            c if c == delim && !in_quotes => {
                flush(out, &mut field, col)?;
                write!(out, "{}", c)?;
                col += 1;
            }
            '\n' if !in_quotes => {
                flush(out, &mut field, col)?;
                writeln!(out)?;
                col = 0;
            }
            '\r' if !in_quotes => {
                flush(out, &mut field, col)?;
                write!(out, "\r")?;
            }
            _ => field.push(c),
        }
    }
    flush(out, &mut field, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for d in chars.by_ref() {
                    if d == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn test_sniff_delimiter() {
        assert_eq!(sniff_delimiter("a,b,c\n1,2,3"), ',');
        assert_eq!(sniff_delimiter("a\tb\tc"), '\t');
        assert_eq!(sniff_delimiter("a;b;c"), ';');
        // Delimiters inside quotes don't count.
        assert_eq!(sniff_delimiter("\"a;b;c\",x\t1"), '\t');
        // No delimiter at all falls back to comma.
        assert_eq!(sniff_delimiter("plain text"), ',');
        assert_eq!(sniff_delimiter(""), ',');
    }

    #[test]
    fn test_rainbow_color_cycles() {
        assert_eq!(rainbow_color(0), rainbow_color(RAINBOW.len()));
        assert_ne!(rainbow_color(0), rainbow_color(1));
    }

    #[test]
    fn test_write_csv_rainbow_preserves_text() {
        // Without a color override (non-TTY test run), `colored` emits no
        // ANSI, so the output must round-trip the input exactly.
        let input = "name,count\napple,3\n\"q,uoted\",\"line\nbreak\"\n";
        let mut buf = Vec::new();
        write_csv_rainbow(&mut buf, input).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(strip_ansi(&out), input);
    }

    #[test]
    fn test_write_csv_rainbow_colors_columns() {
        let _guard = crate::COLOR_TEST_LOCK.lock().unwrap();
        colored::control::set_override(true);
        let mut buf = Vec::new();
        write_csv_rainbow(&mut buf, "a,b\n\"x,y\",z\n").unwrap();
        colored::control::unset_override();
        let out = String::from_utf8(buf).unwrap();

        // Column 0 cyan (36), column 1 yellow (33); delimiters unpainted.
        assert!(out.contains("\x1b[36ma\x1b[0m,"), "col 0 cyan: {out:?}");
        assert!(out.contains(",\x1b[33mb\x1b[0m"), "col 1 yellow: {out:?}");
        // The quoted comma stays inside one cyan field.
        assert!(out.contains("\x1b[36m\"x,y\"\x1b[0m,"), "quoted: {out:?}");
        // Layout survives coloring.
        assert_eq!(strip_ansi(&out), "a,b\n\"x,y\",z\n");
    }
}
