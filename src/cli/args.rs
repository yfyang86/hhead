use clap::Parser;

/// Command-line arguments for hhead
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Width of hex display (bytes per line)
    #[arg(long, default_value_t = 64)]
    pub width: usize,

    /// Number of bytes to read
    #[arg(long, default_value_t = 256)]
    pub bytes: usize,

    /// Input file
    #[arg(long)]
    pub input: String,

    /// Colorize output
    #[arg(long, default_value_t = false)]
    pub color: bool,

    /// Print file metadata
    #[arg(long, default_value_t = false)]
    pub meta: bool,

    /// Try to decode and align in UTF-8 text mode
    #[arg(long, default_value_t = false)]
    pub utf8: bool,

    /// Display image minimap (for BMP, PNG, JPEG)
    #[arg(long, default_value_t = false)]
    pub minimap: bool,

    /// Minimap scale in format "ROWSxCOLS" (e.g., "8x12")
    #[arg(long, default_value = "8x12")]
    pub minimap_scale: String,
}

impl Args {
    /// Validate command-line arguments
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 {
            return Err("width must be positive".to_string());
        }
        if self.bytes == 0 {
            return Err("bytes must be positive".to_string());
        }
        Ok(())
    }
}