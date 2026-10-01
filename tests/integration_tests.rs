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

#[test]
fn test_cli_csv_rainbow_colors_columns() -> Result<(), Box<dyn std::error::Error>> {
    // --csv-rainbow implies color, so ANSI shows up even in a pipe, with
    // each column in a different color and the layout untouched.
    let mut temp_file = tempfile::Builder::new().suffix(".csv").tempfile()?;
    temp_file.write_all(b"name,count\napple,3\nbanana,5\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--csv-rainbow");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\x1b[36mname\x1b[0m,"))
        .stdout(predicate::str::contains(",\x1b[33mcount\x1b[0m"))
        .stdout(predicate::str::contains("apple"))
        .stdout(predicate::str::contains("00000000:").not());

    Ok(())
}

#[test]
fn test_cli_csv_rainbow_markdown_table() -> Result<(), Box<dyn std::error::Error>> {
    // With --markdown, table columns get the rainbow palette.
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(b"| Name | Age |\n| ---- | --- |\n| Al | 9 |\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--markdown")
        .arg("--csv-rainbow");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\x1b[36m"))
        .stdout(predicate::str::contains("\x1b[33m"))
        .stdout(predicate::str::contains("Al"));

    Ok(())
}

#[test]
fn test_cli_csv_rainbow_binary_falls_back_to_hex() -> Result<(), Box<dyn std::error::Error>> {
    // Non-UTF-8 input warns and falls back to the hex dump.
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(&[0x00, 0xFF, 0xFE, 0x01])?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--csv-rainbow");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("00000000:"))
        .stderr(predicate::str::contains("UTF-8"));

    Ok(())
}

// ---------------------------------------------------------------------------
// --tui-caps / --tui-graphics / --tui-graphics-theme
// ---------------------------------------------------------------------------

/// Minimal valid 2x2 red PNG fixture.
const PNG_2X2_RED: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0,
    0, 0, 253, 212, 154, 115, 0, 0, 0, 16, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 68, 12,
    16, 10, 0, 31, 238, 3, 253, 139, 95, 20, 212, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn png_fixture() -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    // ImageReader guesses the format from the extension, so keep a .png one.
    let mut f = tempfile::Builder::new().suffix(".png").tempfile()?;
    f.write_all(PNG_2X2_RED)?;
    Ok(f)
}

#[test]
fn test_cli_tui_caps_without_input() {
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--tui-caps");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("graphics protocol:"))
        .stdout(predicate::str::contains("truecolor:"))
        // Piped stdio never probes.
        .stdout(predicate::str::contains("probe: skipped (not a TTY)"));
}

#[test]
fn test_cli_tui_graphics_invalid_value() {
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--tui-graphics=bogus");
    cmd.assert().failure();
}

#[test]
fn test_cli_tui_graphics_theme_invalid() {
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--tui-graphics-theme").arg("blue");
    cmd.assert().failure();
}

#[test]
fn test_cli_tui_graphics_sixel_minimap() -> Result<(), Box<dyn std::error::Error>> {
    let img = png_fixture()?;
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(img.path())
        .arg("--minimap")
        .arg("--tui-graphics=sixel")
        .arg("--bytes")
        .arg("8");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\x1bPq"))
        .stdout(predicate::str::contains("\x1b\\"))
        // The block minimap is replaced, and the hex dump still follows.
        .stdout(predicate::str::contains('█').not())
        .stdout(predicate::str::contains("00000000:"));
    Ok(())
}

#[test]
fn test_cli_tui_graphics_kitty_minimap() -> Result<(), Box<dyn std::error::Error>> {
    let img = png_fixture()?;
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(img.path())
        .arg("--minimap")
        .arg("--tui-graphics=kitty")
        .arg("--bytes")
        .arg("8");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\x1b_Gq=2,a=T,f=100"))
        .stdout(predicate::str::contains('█').not());
    Ok(())
}

#[test]
fn test_cli_tui_graphics_off_keeps_block_minimap() -> Result<(), Box<dyn std::error::Error>> {
    let img = png_fixture()?;
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(img.path())
        .arg("--minimap")
        .arg("--tui-graphics=off")
        .arg("--bytes")
        .arg("8");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains('█'))
        .stdout(predicate::str::contains("\x1bPq").not());
    Ok(())
}

#[test]
fn test_cli_tui_graphics_markdown_figure() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut img_file = tempfile::Builder::new()
        .suffix(".png")
        .tempfile_in(dir.path())?;
    img_file.write_all(PNG_2X2_RED)?;
    let md_path = dir.path().join("doc.md");
    std::fs::write(
        &md_path,
        format!(
            "![cap]({})\n",
            img_file.path().file_name().unwrap().to_string_lossy()
        ),
    )?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(&md_path)
        .arg("--markdown")
        .arg("--tui-graphics=iterm2");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("cap"))
        .stdout(predicate::str::contains("\x1b]1337;File=inline=1"));
    Ok(())
}

#[test]
fn test_cli_tui_graphics_math_unicode_fallback() -> Result<(), Box<dyn std::error::Error>> {
    // Piped stdout means auto resolves to no protocol; math spans still get
    // the Unicode approximation.
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(b"Euler: $e^{i\\pi} + 1 = 0$\n")?;

    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input")
        .arg(temp_file.path())
        .arg("--markdown")
        .arg("--tui-graphics");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Euler: e^(iπ) + 1 = 0"));

    // Without the flag the span stays literal.
    let mut cmd = Command::cargo_bin("hhead").unwrap();
    cmd.arg("--input").arg(temp_file.path()).arg("--markdown");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Euler: $e^{i\\pi} + 1 = 0$"));

    Ok(())
}
