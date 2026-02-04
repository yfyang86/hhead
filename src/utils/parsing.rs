//! Parsing utility functions

/// Parse a scale string in format "ROWSxCOLS" (e.g., "8x12")
///
/// Returns `Some((rows, cols))` if parsing succeeds, `None` otherwise.
pub fn parse_scale(scale: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = scale.split('x').collect();
    if parts.len() != 2 {
        return None;
    }
    let rows = parts[0].parse().ok()?;
    let cols = parts[1].parse().ok()?;
    if rows == 0 || cols == 0 {
        return None;
    }
    Some((rows, cols))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scale_valid() {
        assert_eq!(parse_scale("8x12"), Some((8, 12)));
        assert_eq!(parse_scale("4x6"), Some((4, 6)));
        assert_eq!(parse_scale("10x20"), Some((10, 20)));
        assert_eq!(parse_scale("1x1"), Some((1, 1)));
    }

    #[test]
    fn test_parse_scale_invalid() {
        assert_eq!(parse_scale(""), None);
        assert_eq!(parse_scale("8"), None);
        assert_eq!(parse_scale("8x12x3"), None);
        assert_eq!(parse_scale("8x"), None);
        assert_eq!(parse_scale("x12"), None);
        assert_eq!(parse_scale("0x12"), None);
        assert_eq!(parse_scale("8x0"), None);
        assert_eq!(parse_scale("0x0"), None);
        assert_eq!(parse_scale("abcxdef"), None);
        assert_eq!(parse_scale("8xabc"), None);
        assert_eq!(parse_scale("abcx12"), None);
    }
}