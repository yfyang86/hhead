use clap::Parser;
use colored::control;
use std::path::Path;

use hhead::cli::Args;
use hhead::display::{
    display_hex, display_minimap, display_tree, print_metadata, run_pager, write_dir_listing,
    write_hex, write_markdown, write_metadata, write_minimap,
};
use hhead::io::read_file;
use hhead::utils::parsing::parse_scale;

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    // Enable color override if requested
    if args.color {
        control::set_override(true);
    }

    // Validate parameters using Args::validate method
    if let Err(err) = args.validate() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }

    // Check if file exists
    let path = Path::new(&args.input);
    if !path.exists() {
        eprintln!("Error: File '{}' not found", args.input);
        std::process::exit(1);
    }

    // Directory input: list it as a tree instead of hex-dumping. `--meta`
    // prepends an `ls -lah`/`du`-style block; `--color` paints entry names
    // by type; the other display modes don't apply to directories.
    if path.is_dir() {
        if args.mode_less {
            let mut buf = Vec::new();
            write_dir_listing(&mut buf, path, args.color, args.meta)?;
            let content = String::from_utf8_lossy(&buf);
            return run_pager(&content, &args.input);
        }
        return display_tree(path, args.color, args.meta);
    }

    // Print metadata if requested
    if args.meta {
        print_metadata(path)?;
    }

    // Display minimap if requested
    if args.minimap {
        match parse_scale(&args.minimap_scale) {
            Some((rows, cols)) => {
                if let Err(e) = display_minimap(path, rows, cols) {
                    eprintln!("Warning: Minimap failed: {}", e);
                    // Continue with hex dump
                }
            }
            None => {
                eprintln!(
                    "Warning: Invalid minimap scale format '{}', expected 'ROWSxCOLS' (e.g., '8x12')",
                    args.minimap_scale
                );
            }
        }
    }

    // Document rendering: `--markdown` renders the file itself; `--mode-anydoc`
    // converts it to Markdown first (so the `--markdown` renderer is implied).
    if args.markdown || args.mode_anydoc {
        let (rows, cols) = minimap_scale(&args.minimap_scale);
        if let Some(md) = markdown_source(path, args.mode_anydoc)? {
            if args.mode_less {
                return run_less(path, &args, rows, cols, Some(&md));
            }
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            return write_markdown(&mut out, &md, path.parent(), args.color, rows, cols);
        }
        // anydoc could not convert and the input isn't text: fall through to
        // the hex dump below.
    }

    // Pager mode pages through the whole hex dump (the --bytes cap does not
    // apply: a pager exists precisely to move through the file).
    if args.mode_less {
        let (rows, cols) = minimap_scale(&args.minimap_scale);
        return run_less(path, &args, rows, cols, None);
    }

    // Read file
    let data = read_file(path, args.bytes)?;

    // Display hex and characters
    display_hex(&data, args.width, args.color, args.utf8);

    Ok(())
}

/// Parse "ROWSxCOLS" with the same fallback the Markdown path uses.
fn minimap_scale(scale: &str) -> (usize, usize) {
    match parse_scale(scale) {
        Some(rc) => rc,
        None => {
            eprintln!(
                "Warning: Invalid minimap scale format '{}', expected 'ROWSxCOLS' (e.g., '8x12'); using 8x12",
                scale
            );
            (8, 12)
        }
    }
}

/// Bytes to feed the Markdown renderer: the file itself for `--markdown`, or
/// `anydoc`'s conversion for `--mode-anydoc`.
///
/// For `--mode-anydoc`, `None` means conversion failed and the input isn't
/// text either, so the caller falls back to the hex dump.
fn markdown_source(path: &Path, convert: bool) -> std::io::Result<Option<Vec<u8>>> {
    if !convert {
        return std::fs::read(path).map(Some);
    }

    let bytes = std::fs::read(path)?;
    // The extension names signature-less formats (CSV); `None` falls back to
    // content detection inside anydoc.
    let format = anydoc::Format::from_path(path);
    match anydoc::to_markdown_bytes(&bytes, format) {
        Ok(md) => Ok(Some(md.into_bytes())),
        Err(e) => {
            eprintln!("Warning: anydoc conversion failed: {}", e);
            // Unrecognized or unconvertible input. Text (e.g. already
            // Markdown) still renders as Markdown; anything else falls back
            // to the hex dump.
            if !bytes.is_empty() && std::str::from_utf8(&bytes).is_ok() {
                Ok(Some(bytes))
            } else {
                Ok(None)
            }
        }
    }
}

/// Render the full output (metadata + minimap + Markdown or hex) into a
/// buffer and page through it interactively with the built-in pager.
///
/// `markdown` is `Some(bytes)` to render as Markdown (already converted when
/// `--mode-anydoc` is in play) or `None` for a plain hex dump of the whole
/// file — the pager's job is to move through it, so the `--bytes` limit does
/// not apply (mirroring Markdown mode).
fn run_less(
    path: &Path,
    args: &Args,
    rows: usize,
    cols: usize,
    markdown: Option<&[u8]>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();

    if args.meta {
        write_metadata(&mut buf, path)?;
    }
    if args.minimap {
        // Bound the write in its own statement so the error can be reported
        // without an early return.
        let result = write_minimap(&mut buf, path, rows, cols);
        if let Err(e) = result {
            eprintln!("Warning: Minimap failed: {}", e);
            // Continue with the rest of the output
        }
    }

    match markdown {
        Some(md) => write_markdown(&mut buf, md, path.parent(), args.color, rows, cols)?,
        None => {
            let data = std::fs::read(path)?;
            write_hex(&mut buf, &data, args.width, args.color, args.utf8)?;
        }
    }

    let content = String::from_utf8_lossy(&buf);
    run_pager(&content, &args.input)
}
