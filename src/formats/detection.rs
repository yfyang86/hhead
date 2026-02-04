//! File format detection by magic numbers

/// Detect file format from the first bytes of data
///
/// Returns a string describing the format, or empty string if unknown.
pub fn detect_file_format(data: &[u8]) -> &'static str {
    // PNG: requires at least 8 bytes
    if data.len() >= 8 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "PNG";
    }
    // JPEG: requires at least 3 bytes
    if data.len() >= 3 && data.starts_with(b"\xff\xd8\xff") {
        return "JPEG";
    }
    // BMP: requires at least 2 bytes
    if data.len() >= 2 && data.starts_with(b"BM") {
        return "BMP";
    }
    // GIF: requires at least 6 bytes
    if data.len() >= 6 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return "GIF";
    }
    // ZIP: requires at least 4 bytes
    if data.len() >= 4 && (data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") || data.starts_with(b"PK\x07\x08")) {
        return "ZIP";
    }
    // GZIP: requires at least 2 bytes
    if data.len() >= 2 && data.starts_with(b"\x1f\x8b") {
        return "GZIP";
    }
    // TIFF: requires at least 4 bytes
    if data.len() >= 4 && (data.starts_with(b"II\x2a\x00") || data.starts_with(b"MM\x00\x2a")) {
        return "TIFF";
    }
    // PDF: requires at least 5 bytes
    if data.len() >= 5 && data.starts_with(b"%PDF-") {
        return "PDF";
    }
    // TAR (USTAR): requires at least 263 bytes
    if data.len() >= 263 && &data[257..263] == b"ustar\0" {
        return "TAR (USTAR)";
    }
    // TAR (GNU): requires at least 263 bytes
    if data.len() >= 263 && &data[257..263] == b"ustar " {
        return "TAR (GNU)";
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_png() {
        let png_header = b"\x89PNG\r\n\x1a\n";
        assert_eq!(detect_file_format(png_header), "PNG");
        // With extra data
        let png_with_data = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00";
        assert_eq!(detect_file_format(png_with_data), "PNG");
    }

    #[test]
    fn test_detect_jpeg() {
        let jpeg_header = b"\xff\xd8\xff";
        assert_eq!(detect_file_format(jpeg_header), "JPEG");
        let jpeg_with_data = b"\xff\xd8\xff\xe0\x00\x10";
        assert_eq!(detect_file_format(jpeg_with_data), "JPEG");
    }

    #[test]
    fn test_detect_bmp() {
        let bmp_header = b"BM";
        assert_eq!(detect_file_format(bmp_header), "BMP");
        let bmp_with_data = b"BM\x00\x00\x00\x00";
        assert_eq!(detect_file_format(bmp_with_data), "BMP");
    }

    #[test]
    fn test_detect_gif() {
        let gif87a = b"GIF87a";
        assert_eq!(detect_file_format(gif87a), "GIF");
        let gif89a = b"GIF89a";
        assert_eq!(detect_file_format(gif89a), "GIF");
    }

    #[test]
    fn test_detect_zip() {
        let zip_header = b"PK\x03\x04";
        assert_eq!(detect_file_format(zip_header), "ZIP");
        let zip_central = b"PK\x05\x06";
        assert_eq!(detect_file_format(zip_central), "ZIP");
        let zip_end = b"PK\x07\x08";
        assert_eq!(detect_file_format(zip_end), "ZIP");
    }

    #[test]
    fn test_detect_gzip() {
        let gzip_header = b"\x1f\x8b";
        assert_eq!(detect_file_format(gzip_header), "GZIP");
    }

    #[test]
    fn test_detect_tiff() {
        let tiff_little = b"II\x2a\x00";
        assert_eq!(detect_file_format(tiff_little), "TIFF");
        let tiff_big = b"MM\x00\x2a";
        assert_eq!(detect_file_format(tiff_big), "TIFF");
    }

    #[test]
    fn test_detect_pdf() {
        let pdf_header = b"%PDF-";
        assert_eq!(detect_file_format(pdf_header), "PDF");
    }

    #[test]
    fn test_detect_tar() {
        // Need at least 263 bytes for TAR detection
        let mut tar_ustar = vec![0u8; 263];
        tar_ustar[257..263].copy_from_slice(b"ustar\0");
        assert_eq!(detect_file_format(&tar_ustar), "TAR (USTAR)");

        let mut tar_gnu = vec![0u8; 263];
        tar_gnu[257..263].copy_from_slice(b"ustar ");
        assert_eq!(detect_file_format(&tar_gnu), "TAR (GNU)");
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_file_format(b""), "");
        assert_eq!(detect_file_format(b"\x00\x00\x00"), "");
        assert_eq!(detect_file_format(b"TEST"), "");
    }

    #[test]
    fn test_detect_insufficient_data() {
        // TAR detection requires 263 bytes
        let short_data = vec![0u8; 100];
        assert_eq!(detect_file_format(&short_data), "");
    }
}