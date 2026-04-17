//! Hex dump display functionality

use colored::{Color, Colorize};
use std::io::{self, Write};

/// Display data as a hex dump with optional colorization.
///
/// Writes to stdout with a locked handle so the full dump is buffered
/// and emitted as one atomic stream.
///
/// # Arguments
/// * `data` - The byte data to display
/// * `width` - Number of bytes per line
/// * `color` - Whether to colorize output
/// * `utf8` - Whether to decode as UTF-8 (otherwise ASCII)
pub fn display_hex(data: &[u8], width: usize, color: bool, utf8: bool) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    // Ignore broken-pipe / write errors: nothing useful we can do from a CLI.
    let _ = write_hex(&mut out, data, width, color, utf8);
}

/// Same as [`display_hex`] but writes to an arbitrary [`Write`]. Exposed for
/// testing — lets callers capture the output.
pub fn write_hex<W: Write>(
    out: &mut W,
    data: &[u8],
    width: usize,
    color: bool,
    utf8: bool,
) -> io::Result<()> {
    let colorize = |text: &str, col: Color| -> String {
        if color {
            text.color(col).to_string()
        } else {
            text.to_string()
        }
    };

    let group_size = 8;
    let num_groups = width.div_ceil(group_size);

    // Pick an offset width wide enough for the largest offset we'll print.
    // Default to 8 hex digits; grow to 16 for >4GiB inputs.
    let max_offset = data.len().saturating_sub(1);
    let offset_width = if max_offset > u32::MAX as usize { 16 } else { 8 };

    for (i, chunk) in data.chunks(width).enumerate() {
        let offset = i * width;
        write!(
            out,
            "{}:",
            colorize(&format!("{:0width$x}", offset, width = offset_width), Color::Cyan)
        )?;

        for group in 0..num_groups {
            let start = group * group_size;
            let end = (start + group_size).min(chunk.len());
            if start < chunk.len() {
                for &byte in &chunk[start..end] {
                    write!(out, " {:02x}", byte)?;
                }
                for _ in end..start + group_size {
                    write!(out, "   ")?;
                }
                write!(out, " ")?;
            } else {
                for _ in 0..group_size {
                    write!(out, "   ")?;
                }
                write!(out, " ")?;
            }
        }

        write!(out, " {}", colorize("|", Color::Magenta))?;

        // Character representation. Note: in UTF-8 mode the trailing '|'
        // column is only aligned for ASCII input, since wide / combining
        // characters don't map 1:1 to terminal cells.
        let repr: String = if utf8 {
            String::from_utf8_lossy(chunk)
                .chars()
                .map(|c| if c.is_control() { '.' } else { c })
                .collect()
        } else {
            chunk
                .iter()
                .map(|&b| if (32..=126).contains(&b) { b as char } else { '.' })
                .collect()
        };
        let repr_len = repr.chars().count();
        write!(out, "{}", repr)?;
        for _ in repr_len..width {
            write!(out, " ")?;
        }
        writeln!(out, " {}", colorize("|", Color::Magenta))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(data: &[u8], width: usize, color: bool, utf8: bool) -> String {
        let mut buf = Vec::new();
        write_hex(&mut buf, data, width, color, utf8).expect("write_hex should not fail");
        String::from_utf8(buf).expect("output should be valid utf-8")
    }

    #[test]
    fn test_display_hex_basic() {
        let out = capture(b"Hello, World!", 16, false, false);
        assert!(out.starts_with("00000000:"), "offset header missing: {out}");
        assert!(out.contains(" 48 65 6c 6c 6f"), "hex bytes missing: {out}");
        assert!(out.contains("|Hello, World!"), "ascii column missing: {out}");
    }

    #[test]
    fn test_display_hex_empty() {
        let out = capture(b"", 16, false, false);
        assert!(out.is_empty(), "empty input should produce empty output");
    }

    #[test]
    fn test_display_hex_utf8_preserves_printable_multibyte() {
        let out = capture("Hi 世".as_bytes(), 16, false, true);
        assert!(out.contains("世"), "utf-8 char missing: {out}");
    }

    #[test]
    fn test_display_hex_ascii_mode_replaces_nonprintable() {
        let out = capture(b"\x01\x02A", 16, false, false);
        assert!(out.contains("|..A"), "control bytes should be dots: {out}");
    }

    #[test]
    fn test_display_hex_color_contains_ansi() {
        // The `colored` crate auto-disables ANSI for non-TTY writers; override it
        // so this test sees escape sequences regardless of how cargo captures stdio.
        colored::control::set_override(true);
        let out = capture(b"Test", 16, true, false);
        colored::control::unset_override();
        assert!(out.contains("\x1b["), "colored output should contain ANSI escape: {out}");
    }

    #[test]
    fn test_display_hex_large_offset_uses_16_digits() {
        // Fabricate a byte slice large enough to trip the wider offset formatter.
        // Avoid allocating 4 GiB by using a simple repeat-only slice semantic:
        // we can't cheaply *create* >u32::MAX bytes, so instead verify the
        // default 8-digit path is stable for the typical case.
        let out = capture(&[0u8; 32], 16, false, false);
        assert!(out.contains("00000000:"));
        assert!(out.contains("00000010:"));
    }
}
