//! Integration tests for hhead CLI

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs::File;
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
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82 // IEND
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
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82 // IEND
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