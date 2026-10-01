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

/// The RGB triple the xterm-256 palette assigns to `idx`.
///
/// Only indices 16..=255 are meaningful here — those are the ones
/// [`rgb_to_256`] produces (cube + grayscale). Used to define the sixel
/// palette (`#Pc;2;r;g;b`); indices 0-15 fall back to black.
pub fn xterm_256_rgb(idx: u8) -> (u8, u8, u8) {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match idx {
        16..=231 => {
            let v = idx - 16;
            (
                LEVELS[(v / 36) as usize],
                LEVELS[((v % 36) / 6) as usize],
                LEVELS[(v % 6) as usize],
            )
        }
        232..=255 => {
            let g = 8 + 10 * (idx - 232);
            (g, g, g)
        }
        _ => (0, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xterm_256_rgb_cube_corners() {
        assert_eq!(xterm_256_rgb(16), (0, 0, 0)); // cube origin
        assert_eq!(xterm_256_rgb(196), (255, 0, 0)); // 196-16=180 → (5,0,0)
        assert_eq!(xterm_256_rgb(21), (0, 0, 255)); // 21-16=5 → (0,0,5)
        assert_eq!(xterm_256_rgb(231), (255, 255, 255)); // (5,5,5)
    }

    #[test]
    fn test_xterm_256_rgb_grayscale() {
        assert_eq!(xterm_256_rgb(232), (8, 8, 8));
        assert_eq!(xterm_256_rgb(255), (238, 238, 238));
    }

    #[test]
    fn test_xterm_256_rgb_basic_indices_fall_back_to_black() {
        assert_eq!(xterm_256_rgb(0), (0, 0, 0));
        assert_eq!(xterm_256_rgb(15), (0, 0, 0));
    }

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
        assert!((16..=231).contains(&result));
    }

    #[test]
    fn test_rgb_to_256_green() {
        // Pure green should map to RGB cube
        let result = rgb_to_256(0, 255, 0);
        assert!((16..=231).contains(&result));
    }

    #[test]
    fn test_rgb_to_256_blue() {
        // Pure blue should map to RGB cube
        let result = rgb_to_256(0, 0, 255);
        assert!((16..=231).contains(&result));
    }

    #[test]
    fn test_rgb_to_256_boundaries() {
        // Sample a sparse grid of the RGB cube and verify the returned palette
        // index is always in the RGB-cube or grayscale region (16..=255).
        for r in (0..=255u8).step_by(64) {
            for g in (0..=255u8).step_by(64) {
                for b in (0..=255u8).step_by(64) {
                    let result = rgb_to_256(r, g, b);
                    assert!(
                        (16..=255).contains(&result),
                        "rgb_to_256({r}, {g}, {b}) = {result} out of range"
                    );
                }
            }
        }
    }
}
