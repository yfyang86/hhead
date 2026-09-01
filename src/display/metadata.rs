//! Metadata display functionality

use crate::formats::detection::detect_file_format;
use crate::formats::metadata::extract_format_metadata;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn format_system_time(t: io::Result<SystemTime>) -> String {
    match t {
        Ok(ts) => match ts.duration_since(UNIX_EPOCH) {
            Ok(d) => format!("{} (unix)", d.as_secs()),
            Err(_) => "<pre-epoch>".to_string(),
        },
        Err(_) => "<unavailable>".to_string(),
    }
}

#[cfg(unix)]
fn format_permissions(perm: &fs::Permissions) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!("{:04o}", perm.mode() & 0o7777)
}

#[cfg(not(unix))]
fn format_permissions(perm: &fs::Permissions) -> String {
    if perm.readonly() {
        "read-only".to_string()
    } else {
        "read-write".to_string()
    }
}

/// Print file metadata including format information
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// `io::Result<()>` - Ok on success, Err on I/O error
pub fn print_metadata(path: &Path) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_metadata(&mut out, path)
}

/// Same as [`print_metadata`] but writes to an arbitrary [`Write`]. Exposed
/// for testing and so the pager can capture the metadata block.
pub fn write_metadata<W: Write>(out: &mut W, path: &Path) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    writeln!(out, "File: {}", path.display())?;
    writeln!(out, "Size: {} bytes", metadata.len())?;
    writeln!(out, "Created: {}", format_system_time(metadata.created()))?;
    writeln!(out, "Modified: {}", format_system_time(metadata.modified()))?;
    writeln!(out, "Accessed: {}", format_system_time(metadata.accessed()))?;
    writeln!(
        out,
        "Permissions: {}",
        format_permissions(&metadata.permissions())
    )?;

    // Read first 1024 bytes for format detection
    let mut file = fs::File::open(path)?;
    let mut buffer = [0; 1024];
    let bytes_read = file.read(&mut buffer)?;

    if bytes_read > 0 {
        let format_info = detect_file_format(&buffer[..bytes_read]);
        if !format_info.is_empty() {
            writeln!(out, "Format: {}", format_info)?;
        }

        // Extract additional format-specific metadata
        let additional_meta = extract_format_metadata(&buffer[..bytes_read]);
        for line in additional_meta {
            writeln!(out, "{}", line)?;
        }
    }

    writeln!(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_print_metadata_file_exists() -> io::Result<()> {
        let temp_file = NamedTempFile::new()?;
        // Just ensure it doesn't panic
        let result = print_metadata(temp_file.path());
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_print_metadata_nonexistent() {
        let path = Path::new("/nonexistent/file");
        let result = print_metadata(path);
        assert!(result.is_err());
    }
}
