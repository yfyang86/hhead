//! Format-specific metadata extraction

use super::detection::detect_file_format;

/// Extract format-specific metadata from file data
///
/// Returns a vector of formatted metadata strings.
pub fn extract_format_metadata(data: &[u8]) -> Vec<String> {
    let mut metadata = Vec::new();
    let format = detect_file_format(data);

    match format {
        "PNG" => {
            if data.len() >= 24 {
                // PNG IHDR chunk starts at offset 8
                let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
                let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
                let bit_depth = data[24];
                let color_type = data[25];
                let color_type_str = match color_type {
                    0 => "Grayscale",
                    2 => "RGB",
                    3 => "Indexed",
                    4 => "Grayscale+Alpha",
                    6 => "RGB+Alpha",
                    _ => "Unknown",
                };
                metadata.push(format!("  Dimensions: {} x {}", width, height));
                metadata.push(format!("  Bit depth: {}", bit_depth));
                metadata.push(format!("  Color type: {}", color_type_str));
            }
        }
        "BMP" => {
            if data.len() >= 54 {
                // BITMAPINFOHEADER starts at offset 14, width at offset 18, height at offset 22
                let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
                let height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
                let bits_per_pixel = u16::from_le_bytes([data[28], data[29]]);
                let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);
                let compression_str = match compression {
                    0 => "BI_RGB",
                    1 => "BI_RLE8",
                    2 => "BI_RLE4",
                    3 => "BI_BITFIELDS",
                    4 => "BI_JPEG",
                    5 => "BI_PNG",
                    _ => "Unknown",
                };
                metadata.push(format!("  Dimensions: {} x {}", width.abs(), height.abs()));
                metadata.push(format!("  Bits per pixel: {}", bits_per_pixel));
                metadata.push(format!("  Compression: {}", compression_str));
                // Height can be negative for top-down DIB
                if height < 0 {
                    metadata.push("  Orientation: Top-down".to_string());
                } else {
                    metadata.push("  Orientation: Bottom-up".to_string());
                }
            }
        }
        "JPEG" => {
            // JPEG parsing is more complex due to segmented format
            // Look for SOF0 (Start Of Frame 0) marker: FF C0 to FF CF
            let mut i = 2; // Skip initial FF D8
            while i + 9 < data.len() {
                if data[i] == 0xFF && data[i+1] >= 0xC0 && data[i+1] <= 0xCF {
                    let height = (u16::from(data[i+5]) << 8) | u16::from(data[i+6]);
                    let width = (u16::from(data[i+7]) << 8) | u16::from(data[i+8]);
                    let components = data[i+9];
                    metadata.push(format!("  Dimensions: {} x {}", width, height));
                    metadata.push(format!("  Components: {}", components));
                    break;
                }
                if data[i] == 0xFF {
                    let segment_len = (u16::from(data[i+2]) << 8) | u16::from(data[i+3]);
                    i += usize::from(segment_len) + 2;
                } else {
                    i += 1;
                }
            }
        }
        "GIF" => {
            if data.len() >= 10 {
                let width = u16::from_le_bytes([data[6], data[7]]);
                let height = u16::from_le_bytes([data[8], data[9]]);
                let packed = data[10];
                let global_color_table = (packed & 0x80) != 0;
                let color_resolution = ((packed >> 4) & 0x07) + 1;
                let _sorted = (packed & 0x08) != 0;
                let global_color_table_size = 1 << ((packed & 0x07) + 1);
                let version = if data.starts_with(b"GIF87a") { "87a" } else { "89a" };
                metadata.push(format!("  Version: GIF{}", version));
                metadata.push(format!("  Dimensions: {} x {}", width, height));
                metadata.push(format!("  Global color table: {}", global_color_table));
                if global_color_table {
                    metadata.push(format!("  Color table size: {}", global_color_table_size));
                }
                metadata.push(format!("  Color resolution: {} bits", color_resolution));
            }
        }
        "ZIP" => {
            if data.len() >= 30 && data.starts_with(b"PK\x03\x04") {
                let compressed_size = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
                let uncompressed_size = u32::from_le_bytes([data[22], data[23], data[24], data[25]]);
                let compression_method = u16::from_le_bytes([data[8], data[9]]);
                let compression_method_str = match compression_method {
                    0 => "Stored",
                    8 => "Deflated",
                    9 => "Enhanced Deflated",
                    12 => "BZIP2",
                    14 => "LZMA",
                    19 => "LZ77",
                    98 => "PPMd",
                    _ => "Unknown",
                };
                metadata.push(format!("  Compression: {}", compression_method_str));
                metadata.push(format!("  Compressed size: {} bytes", compressed_size));
                metadata.push(format!("  Uncompressed size: {} bytes", uncompressed_size));
                // File name length at offset 26
                let name_len = u16::from_le_bytes([data[26], data[27]]) as usize;
                let _extra_len = u16::from_le_bytes([data[28], data[29]]) as usize;
                if data.len() >= 30 + name_len {
                    let name_bytes = &data[30..30 + name_len];
                    if let Ok(name) = String::from_utf8(name_bytes.to_vec()) {
                        metadata.push(format!("  First file: {}", name));
                    }
                }
            }
        }
        "GZIP" => {
            if data.len() >= 10 {
                let compression_method = data[2];
                let flags = data[3];
                let mtime = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                let extra_flags = data[8];
                let os = data[9];
                let os_str = match os {
                    0 => "FAT filesystem (MS-DOS, OS/2, NT/Win32)",
                    1 => "Amiga",
                    2 => "VMS (or OpenVMS)",
                    3 => "Unix",
                    4 => "VM/CMS",
                    5 => "Atari TOS",
                    6 => "HPFS filesystem (OS/2, NT)",
                    7 => "Macintosh",
                    8 => "Z-System",
                    9 => "CP/M",
                    10 => "TOPS-20",
                    11 => "NTFS filesystem (NT)",
                    12 => "QDOS",
                    13 => "Acorn RISCOS",
                    255 => "unknown",
                    _ => "Other",
                };
                let method_str = match compression_method {
                    8 => "Deflate",
                    _ => "Unknown",
                };
                metadata.push(format!("  Compression: {}", method_str));
                if mtime != 0 {
                    metadata.push(format!("  Modification time: {} Unix timestamp", mtime));
                }
                metadata.push(format!("  OS: {}", os_str));
                metadata.push(format!("  Flags: 0x{:02x}", flags));
                metadata.push(format!("  Extra flags: 0x{:02x}", extra_flags));
            }
        }
        "TIFF" => {
            if data.len() >= 16 {
                let is_little_endian = data.starts_with(b"II");
                // First IFD offset at bytes 4-7
                let ifd_offset = if is_little_endian {
                    u32::from_le_bytes([data[4], data[5], data[6], data[7]])
                } else {
                    u32::from_be_bytes([data[4], data[5], data[6], data[7]])
                };
                metadata.push(format!("  Endianness: {}", if is_little_endian { "Little" } else { "Big" }));
                metadata.push(format!("  IFD offset: {}", ifd_offset));
                // Try to read first IFD for basic image info
                // This is simplified - full TIFF parsing is complex
            }
        }
        "TAR (USTAR)" | "TAR (GNU)" => {
            if data.len() >= 512 {
                // Parse TAR header
                let name = String::from_utf8_lossy(&data[0..100]).trim_end_matches('\0').to_string();
                let _mode = String::from_utf8_lossy(&data[100..108]).trim_end_matches('\0').to_string();
                let size_str = String::from_utf8_lossy(&data[124..136]).trim_end_matches('\0').to_string();
                let mtime_str = String::from_utf8_lossy(&data[136..148]).trim_end_matches('\0').to_string();
                let typeflag = data[156];
                let linkname = String::from_utf8_lossy(&data[157..257]).trim_end_matches('\0').to_string();
                let type_str = match typeflag as char {
                    '0' | '\0' => "Regular file",
                    '1' => "Hard link",
                    '2' => "Symbolic link",
                    '3' => "Character device",
                    '4' => "Block device",
                    '5' => "Directory",
                    '6' => "FIFO",
                    '7' => "Contiguous file",
                    _ => "Unknown",
                };
                if !name.is_empty() {
                    metadata.push(format!("  First entry: {}", name));
                }
                if let Ok(size) = u64::from_str_radix(&size_str, 8) {
                    metadata.push(format!("  Size: {} bytes", size));
                }
                if let Ok(mtime) = u64::from_str_radix(&mtime_str, 8) {
                    metadata.push(format!("  Modification time: {} Unix timestamp", mtime));
                }
                metadata.push(format!("  Type: {}", type_str));
                if !linkname.is_empty() {
                    metadata.push(format!("  Link name: {}", linkname));
                }
            }
        }
        "PDF" => {
            if data.len() >= 8 {
                // PDF version is in bytes 5-7 (e.g., "1.4" or "2.0")
                let version = String::from_utf8_lossy(&data[5..8]);
                metadata.push(format!("  Version: {}", version));
            }
        }
        _ => {}
    }

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_png_metadata() {
        // Create minimal PNG header
        let mut png_data = vec![0u8; 30];
        // PNG signature
        png_data[0..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        // IHDR chunk (length 13)
        png_data[8..12].copy_from_slice(&13u32.to_be_bytes());
        png_data[12..16].copy_from_slice(b"IHDR");
        // Width: 800, Height: 600
        png_data[16..20].copy_from_slice(&800u32.to_be_bytes());
        png_data[20..24].copy_from_slice(&600u32.to_be_bytes());
        // Bit depth: 8, Color type: 2 (RGB)
        png_data[24] = 8;
        png_data[25] = 2;

        let metadata = extract_format_metadata(&png_data);
        assert!(!metadata.is_empty());
        assert!(metadata.iter().any(|s| s.contains("Dimensions: 800 x 600")));
        assert!(metadata.iter().any(|s| s.contains("Bit depth: 8")));
        assert!(metadata.iter().any(|s| s.contains("Color type: RGB")));
    }

    #[test]
    fn test_extract_bmp_metadata() {
        // Create minimal BMP header
        let mut bmp_data = vec![0u8; 54];
        // BMP signature
        bmp_data[0..2].copy_from_slice(b"BM");
        // BITMAPINFOHEADER size (40)
        bmp_data[14..18].copy_from_slice(&40u32.to_le_bytes());
        // Width: 800, Height: 600
        bmp_data[18..22].copy_from_slice(&800i32.to_le_bytes());
        bmp_data[22..26].copy_from_slice(&600i32.to_le_bytes());
        // Bits per pixel: 24
        bmp_data[28..30].copy_from_slice(&24u16.to_le_bytes());
        // Compression: 0 (BI_RGB)
        bmp_data[30..34].copy_from_slice(&0u32.to_le_bytes());

        let metadata = extract_format_metadata(&bmp_data);
        assert!(!metadata.is_empty());
        assert!(metadata.iter().any(|s| s.contains("Dimensions: 800 x 600")));
        assert!(metadata.iter().any(|s| s.contains("Bits per pixel: 24")));
        assert!(metadata.iter().any(|s| s.contains("Compression: BI_RGB")));
        assert!(metadata.iter().any(|s| s.contains("Orientation: Bottom-up")));
    }

    #[test]
    fn test_extract_gif_metadata() {
        // Create GIF header
        let mut gif_data = vec![0u8; 20];
        // GIF signature
        gif_data[0..6].copy_from_slice(b"GIF89a");
        // Width: 320, Height: 240
        gif_data[6..8].copy_from_slice(&320u16.to_le_bytes());
        gif_data[8..10].copy_from_slice(&240u16.to_le_bytes());
        // Packed byte: global color table present (0x80), color resolution 7 (0x70)
        gif_data[10] = 0xF0; // 0x80 (global color table) + 0x70 (color resolution 8)

        let metadata = extract_format_metadata(&gif_data);
        assert!(!metadata.is_empty());
        assert!(metadata.iter().any(|s| s.contains("Version: GIF89a")));
        assert!(metadata.iter().any(|s| s.contains("Dimensions: 320 x 240")));
        assert!(metadata.iter().any(|s| s.contains("Global color table: true")));
        assert!(metadata.iter().any(|s| s.contains("Color resolution: 8 bits")));
    }

    #[test]
    fn test_extract_unknown_format() {
        let unknown_data = b"UNKNOWN";
        let metadata = extract_format_metadata(unknown_data);
        assert!(metadata.is_empty());
    }
}