use clap::Parser;
use colored::control;
use std::path::Path;

use hhead::cli::Args;
use hhead::display::{
    display_hex, display_markdown, display_minimap, print_metadata, run_pager, write_hex,
    write_markdown, write_metadata, write_minimap,
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

    // Markdown mode renders the document instead of the hex dump.
    if args.markdown {
        let (rows, cols) = minimap_scale(&args.minimap_scale);
        if args.mode_less {
            return run_less(path, &args, rows, cols, true);
        }
        return display_markdown(path, args.color, rows, cols);
    }

    // Pager mode pages through the whole hex dump (the --bytes cap does not
    // apply: a pager exists precisely to move through the file).
    if args.mode_less {
        let (rows, cols) = minimap_scale(&args.minimap_scale);
        return run_less(path, &args, rows, cols, false);
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

/// Render the full output (metadata + minimap + Markdown or hex) into a
/// buffer and page through it interactively with the built-in pager.
fn run_less(
    path: &Path,
    args: &Args,
    rows: usize,
    cols: usize,
    markdown: bool,
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

    if markdown {
        let data = std::fs::read(path)?;
        write_markdown(&mut buf, &data, path.parent(), args.color, rows, cols)?;
    } else {
        // Read the whole file: the pager's job is to move through it, so the
        // --bytes limit does not apply (mirroring Markdown mode).
        let data = std::fs::read(path)?;
        write_hex(&mut buf, &data, args.width, args.color, args.utf8)?;
    }

    let content = String::from_utf8_lossy(&buf);
    run_pager(&content, &args.input)
}
