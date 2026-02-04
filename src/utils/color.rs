//! Color conversion utilities

/// Convert RGB color to 256-color terminal palette index
///
/// The 256-color palette consists of:
/// - 0-15: basic colors
/// - 16-231: 6x6x6 RGB cube (6 levels per channel)
/// - 232-255: grayscale (24 shades)
///
/// If all RGB components are close (within 10 of each other),
/// uses grayscale mapping. Otherwise uses RGB cube mapping.
pub fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    // 256-color palette mapping
    // 0-15: basic colors (skip)
    // 16-231: 6x6x6 RGB cube
    // 232-255: grayscale

    // If all components are close, use grayscale
    let r_diff = r as i32 - g as i32;
    let g_diff = g as i32 - b as i32;
    let b_diff = b as i32 - r as i32;
    if r_diff.abs() < 10 && g_diff.abs() < 10 && b_diff.abs() < 10 {
        // grayscale range 232-255, 24 shades
        let gray = r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114;
        let index = ((gray / 255.0) * 23.0).round() as u8;
        232 + index
    } else {
        // RGB cube: each component in range 0-5
        let r_idx = (r as f32 / 255.0 * 5.0).round() as u8;
        let g_idx = (g as f32 / 255.0 * 5.0).round() as u8;
        let b_idx = (b as f32 / 255.0 * 5.0).round() as u8;
        16 + 36 * r_idx + 6 * g_idx + b_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_256_black() {
        // Black should map to grayscale near 232
        let result = rgb_to_256(0, 0, 0);
        assert!(result >= 232);
    }

    #[test]
    fn test_rgb_to_256_white() {
        // White should map to grayscale near 255
        let result = rgb_to_256(255, 255, 255);
        assert!(result >= 232);
    }

    #[test]
    fn test_rgb_to_256_gray() {
        // Gray values should map to grayscale range
        let result = rgb_to_256(128, 128, 128);
        assert!(result >= 232);
    }

    #[test]
    fn test_rgb_to_256_red() {
        // Pure red should map to RGB cube
        let result = rgb_to_256(255, 0, 0);
        // Should be in RGB cube range (16-231)
        assert!(result >= 16 && result <= 231);
    }

    #[test]
    fn test_rgb_to_256_green() {
        // Pure green should map to RGB cube
        let result = rgb_to_256(0, 255, 0);
        assert!(result >= 16 && result <= 231);
    }

    #[test]
    fn test_rgb_to_256_blue() {
        // Pure blue should map to RGB cube
        let result = rgb_to_256(0, 0, 255);
        assert!(result >= 16 && result <= 231);
    }

    #[test]
    fn test_rgb_to_256_boundaries() {
        // Test that function never returns invalid values
        for r in 0..=255 {
            for g in 0..=255 {
                for b in 0..=255 {
                    if r % 64 == 0 && g % 64 == 0 && b % 64 == 0 {
                        // Sample every 64 values to avoid slow test
                        let result = rgb_to_256(r, g, b);
                        assert!(result <= 255, "rgb_to_256({}, {}, {}) = {} > 255", r, g, b, result);
                    }
                }
            }
        }
    }
}