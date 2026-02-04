//! File reading utilities

use std::fs;
use std::io;
use std::io::Read;
use std::path::Path;

/// Read up to `max_bytes` from a file
///
/// # Arguments
/// * `path` - Path to the file
/// * `max_bytes` - Maximum number of bytes to read
///
/// # Returns
/// `io::Result<Vec<u8>>` - Bytes read from file (up to `max_bytes`)
pub fn read_file(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0; max_bytes];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_file_exists() -> io::Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        let test_data = b"Hello, World!";
        temp_file.write_all(test_data)?;

        let data = read_file(temp_file.path(), 100)?;
        assert_eq!(data, test_data);
        Ok(())
    }

    #[test]
    fn test_read_file_max_bytes() -> io::Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        let test_data = b"Hello, World!";
        temp_file.write_all(test_data)?;

        let data = read_file(temp_file.path(), 5)?;
        assert_eq!(data, b"Hello");
        Ok(())
    }

    #[test]
    fn test_read_file_empty() -> io::Result<()> {
        let temp_file = NamedTempFile::new()?;

        let data = read_file(temp_file.path(), 100)?;
        assert!(data.is_empty());
        Ok(())
    }

    #[test]
    fn test_read_file_nonexistent() {
        let path = Path::new("/nonexistent/file");
        let result = read_file(path, 100);
        assert!(result.is_err());
    }
}