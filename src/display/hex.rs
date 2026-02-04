//! Hex dump display functionality

use colored::{Color, Colorize};

/// Display data as a hex dump with optional colorization
///
/// # Arguments
/// * `data` - The byte data to display
/// * `width` - Number of bytes per line
/// * `color` - Whether to colorize output
/// * `utf8` - Whether to decode as UTF-8 (otherwise ASCII)
pub fn display_hex(data: &[u8], width: usize, color: bool, utf8: bool) {
    // Helper to conditionally colorize text
    let colorize = move |text: &str, col: Color| -> String {
        if color {
            text.color(col).to_string()
        } else {
            text.to_string()
        }
    };

    // Determine grouping (e.g., 8 bytes per group)
    let group_size = 8;
    let num_groups = (width + group_size - 1) / group_size;

    for (i, chunk) in data.chunks(width).enumerate() {
        let offset = i * width;
        // Print offset in hex
        print!("{}:", colorize(&format!("{:08x}", offset), Color::Cyan));

        // Print hex bytes in groups
        for group in 0..num_groups {
            let start = group * group_size;
            let end = std::cmp::min(start + group_size, chunk.len());
            if start < chunk.len() {
                // Print bytes in this group
                for j in start..end {
                    let byte = chunk[j];
                    print!(" {:02x}", byte);
                }
                // Fill missing bytes with spaces
                for _ in end..start + group_size {
                    print!("   ");
                }
                // Extra space between groups
                print!(" ");
            } else {
                // Entire group missing, fill with spaces
                for _ in 0..group_size {
                    print!("   ");
                }
                print!(" ");
            }
        }

        // Print separator
        print!(" {}", colorize("|", Color::Magenta));

        // Character representation
        let repr: String = if utf8 {
            // UTF-8 lossy decoding, replace control characters with '.'
            String::from_utf8_lossy(chunk)
                .chars()
                .map(|c| if c.is_control() { '.' } else { c })
                .collect()
        } else {
            // ASCII only, non-printable as '.'
            chunk.iter().map(|&b| {
                if (32..=126).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            }).collect()
        };
        let repr_len = repr.chars().count();
        print!("{}", repr);
        // Fill missing characters with spaces
        for _ in repr_len..width {
            print!(" ");
        }
        print!(" {}", colorize("|", Color::Magenta));
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_hex_basic() {
        let data = b"Hello, World!";
        // Capture output by redirecting stdout is complex.
        // For now, just ensure it doesn't panic.
        display_hex(data, 16, false, false);
        // If we reach here, test passes
        assert!(true);
    }

    #[test]
    fn test_display_hex_empty() {
        let data = b"";
        display_hex(data, 16, false, false);
        assert!(true);
    }

    #[test]
    fn test_display_hex_utf8() {
        let data = "Hello, 世界!".as_bytes();
        display_hex(data, 16, false, true);
        assert!(true);
    }

    #[test]
    fn test_display_hex_color() {
        let data = b"Test";
        display_hex(data, 16, true, false);
        assert!(true);
    }

    // Note: Testing actual output would require capturing stdout.
    // This is better done with integration tests.
}