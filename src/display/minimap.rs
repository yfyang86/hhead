//! Image minimap display functionality

use std::io::{self, Write};
use std::path::Path;

use image::{GenericImageView, ImageReader};

use crate::utils::color::rgb_to_256;

/// Display a minimap of an image file
///
/// # Arguments
/// * `path` - Path to the image file
/// * `rows` - Number of rows in the minimap
/// * `cols` - Number of columns in the minimap
///
/// # Returns
/// `io::Result<()>` - Ok on success, Err if image cannot be decoded
pub fn display_minimap(path: &Path, rows: usize, cols: usize) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_minimap(&mut out, path, rows, cols)
}

/// Same as [`display_minimap`] but writes to an arbitrary [`Write`]. Exposed
/// so other renderers (e.g. Markdown figures) can embed a minimap, and for
/// testing.
pub fn write_minimap<W: Write>(
    out: &mut W,
    path: &Path,
    rows: usize,
    cols: usize,
) -> io::Result<()> {
    let img = ImageReader::open(path)?
        .decode()
        .map_err(|e| io::Error::other(format!("Failed to decode image: {}", e)))?;

    let (width, height) = img.dimensions();

    // Sample pixels
    for row in 0..rows {
        for col in 0..cols {
            let x = (col * width as usize) / cols;
            let y = (row * height as usize) / rows;
            let pixel = img.get_pixel(x as u32, y as u32);
            let color_idx = rgb_to_256(pixel[0], pixel[1], pixel[2]);
            // Use ANSI 256-color escape sequence: \x1b[38;5;{index}m
            write!(out, "\x1b[38;5;{}m█\x1b[0m", color_idx)?;
        }
        writeln!(out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_minimap_invalid_path() {
        let path = Path::new("/nonexistent/file.png");
        let result = display_minimap(path, 8, 12);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_minimap_invalid_path() {
        let mut buf = Vec::new();
        let result = write_minimap(&mut buf, Path::new("/nonexistent/file.png"), 8, 12);
        assert!(result.is_err());
        assert!(buf.is_empty());
    }

    // Note: Testing with actual image files would require test images.
    // This is better done with integration tests.
}
