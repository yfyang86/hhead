use clap::Parser;
use colored::control;
use std::path::Path;

use hhead::cli::Args;
use hhead::display::{display_hex, display_minimap, print_metadata};
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

    // Read file
    let data = read_file(path, args.bytes)?;

    // Display hex and characters
    display_hex(&data, args.width, args.color, args.utf8);

    Ok(())
}