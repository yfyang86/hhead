//! Integration tests for hhead CLI

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hhead"))
        .stdout(predicate::str::contains("--input"));
}

#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hhead"));
}

#[test]
fn test_cli_file_not_found() {
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg("nonexistent.txt");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_cli_basic_hex_dump() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    let test_data = b"Hello, World!";
    temp_file.write_all(test_data)?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("00000000:"))
        .stdout(predicate::str::contains("Hello, World!"));

    Ok(())
}

#[test]
fn test_cli_with_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    let test_data = b"Hello";
    temp_file.write_all(test_data)?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--meta");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("File:"))
        .stdout(predicate::str::contains("Size:"));

    Ok(())
}

#[test]
fn test_cli_with_color() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    let test_data = b"Test";
    temp_file.write_all(test_data)?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--color");
    cmd.assert().success();
    // Can't easily test color output in CI, just ensure it doesn't crash
    Ok(())
}

#[test]
fn test_cli_with_utf8() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    let test_data = "Hello, 世界!";
    temp_file.write_all(test_data.as_bytes())?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--utf8");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("世界"));

    Ok(())
}

#[test]
fn test_cli_invalid_arguments() {
    // Zero width
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg("test.txt").arg("--width").arg("0");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("width must be positive"));

    // Zero bytes
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg("test.txt").arg("--bytes").arg("0");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("bytes must be positive"));
}

#[test]
fn test_cli_minimap_invalid_scale() -> Result<(), Box<dyn std::error::Error>> {
    // Create a small PNG file for testing
    let mut temp_file = NamedTempFile::new()?;
    // Minimal PNG: 1x1 transparent pixel
    let png_data = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk length
        0x49, 0x48, 0x44, 0x52, // IHDR type
        0x00, 0x00, 0x00, 0x01, // width
        0x00, 0x00, 0x00, 0x01, // height
        0x08, 0x02, 0x00, 0x00, 0x00, // bit depth, color type, etc.
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
    ];
    temp_file.write_all(&png_data)?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--minimap")
        .arg("--minimap-scale")
        .arg("invalid");
    cmd.assert()
        .success() // Should still succeed with warning
        .stderr(predicate::str::contains("Warning"));

    Ok(())
}

#[test]
fn test_cli_markdown_renders_table_not_hex() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(b"# Doc\n\n| a | b |\n|---|---|\n| 1 | 2 |\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--markdown");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Doc"))
        .stdout(predicate::str::contains("| a | b |"))
        // Markdown mode overrides the hex dump entirely.
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_markdown_reads_whole_file() -> Result<(), Box<dyn std::error::Error>> {
    // The default --bytes limit (256) must not truncate Markdown rendering.
    let mut temp_file = NamedTempFile::new()?;
    let mut content = String::new();
    for _ in 0..100 {
        content.push_str("filler line\n");
    }
    content.push_str("# END MARKER\n");
    temp_file.write_all(content.as_bytes())?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--markdown");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("END MARKER"));

    Ok(())
}

#[test]
fn test_cli_png_metadata() -> Result<(), Box<dyn std::error::Error>> {
    // Create a minimal PNG file
    let mut temp_file = NamedTempFile::new()?;
    let png_data = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk length
        0x49, 0x48, 0x44, 0x52, // IHDR type
        0x00, 0x00, 0x00, 0x01, // width
        0x00, 0x00, 0x00, 0x01, // height
        0x08, 0x02, 0x00, 0x00, 0x00, // bit depth, color type, etc.
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
    ];
    temp_file.write_all(&png_data)?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--meta");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Format: PNG"))
        .stdout(predicate::str::contains("Dimensions"));

    Ok(())
}

#[test]
fn test_cli_mode_less_pages_whole_file_when_not_a_tty() -> Result<(), Box<dyn std::error::Error>> {
    // With piped stdio there is no terminal to page on, so --mode-less must
    // fall back to dumping the whole output — and, like a real pager, the
    // default --bytes cap (256) must not apply. The fixture is 500 lines of
    // 22 bytes + a 13-byte marker = 11013 bytes, so the final hex row starts
    // at 0x2b00; the marker text itself is split across two rows in the
    // ASCII column, so assert on a contiguous fragment instead.
    let mut temp_file = NamedTempFile::new()?;
    let mut content = String::new();
    for i in 0..500 {
        content.push_str(&format!("line {i:04} of the file\n"));
    }
    content.push_str("# END MARKER\n");
    temp_file.write_all(content.as_bytes())?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--mode-less");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("line 0000"))
        .stdout(predicate::str::contains("00002b00:"))
        .stdout(predicate::str::contains("# END MA"));

    // The pager must respect the other options too: page the hex dump at the
    // requested width.
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--mode-less")
        .arg("--width")
        .arg("16");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("00000000:"))
        .stdout(predicate::str::contains("00002b00:"));

    Ok(())
}

#[test]
fn test_cli_mode_less_with_meta_and_markdown() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(b"# Doc\n\n| a | b |\n|---|---|\n| 1 | 2 |\n")?;

    // --mode-less combines with --markdown and --meta.
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--mode-less")
        .arg("--markdown")
        .arg("--meta");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("File:"))
        .stdout(predicate::str::contains("| a | b |"))
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_mode_anydoc_converts_csv_to_markdown() -> Result<(), Box<dyn std::error::Error>> {
    // anydoc turns a CSV into a GFM table; --markdown rendering is implied,
    // so no hex dump is produced. CSV carries no signature, so the file needs
    // a .csv extension for anydoc to name the format.
    let mut temp_file = tempfile::Builder::new().suffix(".csv").tempfile()?;
    temp_file.write_all(b"name,count\napple,3\nbanana,5\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--mode-anydoc");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("| name"))
        .stdout(predicate::str::contains("| count |"))
        .stdout(predicate::str::contains("banana"))
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_mode_anydoc_text_falls_back_to_markdown() -> Result<(), Box<dyn std::error::Error>> {
    // anydoc can't convert plain Markdown, so the input is rendered as
    // Markdown instead of falling back to the hex dump.
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(b"# Doc\n\n| a | b |\n|---|---|\n| 1 | 2 |\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--mode-anydoc");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Doc"))
        .stdout(predicate::str::contains("| a | b |"))
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_mode_anydoc_binary_falls_back_to_hex() -> Result<(), Box<dyn std::error::Error>> {
    // A binary anydoc can't convert (and that isn't text) falls back to the
    // hex dump.
    let mut temp_file = NamedTempFile::new()?;
    let png_data = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk length
        0x49, 0x48, 0x44, 0x52, // IHDR type
        0x00, 0x00, 0x00, 0x01, // width
        0x00, 0x00, 0x00, 0x01, // height
        0x08, 0x02, 0x00, 0x00, 0x00, // bit depth, color type, etc.
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
    ];
    temp_file.write_all(&png_data)?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--mode-anydoc");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("00000000:"))
        .stderr(predicate::str::contains("anydoc conversion failed"));

    Ok(())
}

#[test]
fn test_cli_mode_anydoc_with_mode_less() -> Result<(), Box<dyn std::error::Error>> {
    // --mode-anydoc pages the converted Markdown when combined with
    // --mode-less (piped stdio falls back to a full dump).
    let mut temp_file = tempfile::Builder::new().suffix(".csv").tempfile()?;
    temp_file.write_all(b"name,count\napple,3\nbanana,5\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--mode-anydoc")
        .arg("--mode-less");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("| name"))
        .stdout(predicate::str::contains("banana"))
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_directory_input_tree() -> Result<(), Box<dyn std::error::Error>> {
    // A directory input is listed as a tree instead of hex-dumped.
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("sub"))?;
    std::fs::write(dir.path().join("alpha.txt"), b"hello")?;
    std::fs::write(dir.path().join("sub").join("beta.bin"), vec![0u8; 2048])?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(dir.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("alpha.txt"))
        .stdout(predicate::str::contains("└──"))
        .stdout(predicate::str::contains("1 directory, 2 files"))
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_directory_input_with_meta() -> Result<(), Box<dyn std::error::Error>> {
    // --meta on a directory prepends an ls -lah / du -sh style block.
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("sub"))?;
    std::fs::write(dir.path().join("sub").join("beta.bin"), vec![0u8; 2048])?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(dir.path()).arg("--meta");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Directory:"))
        .stdout(predicate::str::contains("2.0K\tsub"))
        .stdout(predicate::str::contains("\ttotal"))
        .stdout(predicate::str::contains("└──"));

    Ok(())
}

#[test]
fn test_cli_directory_input_with_color() -> Result<(), Box<dyn std::error::Error>> {
    // --color forces ANSI escapes even though stdout is piped.
    let dir = tempfile::tempdir()?;
    std::fs::create_dir(dir.path().join("sub"))?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(dir.path()).arg("--color");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\x1b["));

    Ok(())
}
