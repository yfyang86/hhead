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

    /// Input file, or a directory to list as a tree
    /// (with `--meta`, an `ls -lah`/`du`-style block precedes the tree)
    #[arg(long, required_unless_present = "tui_caps")]
    pub input: Option<String>,

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

    /// Render input as Markdown instead of a hex dump (figures use the minimap renderer)
    #[arg(long, default_value_t = false)]
    pub markdown: bool,

    /// Page through the output interactively, like `less`
    /// (works with the other display options; the `--bytes` limit does not apply)
    #[arg(long, default_value_t = false)]
    pub mode_less: bool,

    /// Convert the input to Markdown first (via `anydoc`) and render it like
    /// `--markdown` (which is implied). Text that `anydoc` cannot convert is
    /// rendered as Markdown; other inputs fall back to the hex dump.
    #[arg(long, default_value_t = false)]
    pub mode_anydoc: bool,

    /// Rainbow-colorize columns (implies `--color`): CSV/TSV input is
    /// rendered as text with each column in its own color (the whole file;
    /// `--bytes` does not apply); with `--markdown` or `--mode-anydoc`,
    /// table columns are painted with the same palette
    #[arg(long, default_value_t = false)]
    pub csv_rainbow: bool,

    /// Terminal graphics for images (minimap, Markdown figures, display math):
    /// the bare flag auto-detects the protocol, or force one of
    /// kitty|iterm2|sixel|off. Off by default; ignored under `--mode-less`
    #[arg(long, value_name = "PROTO", num_args = 0..=1, default_missing_value = "auto",
          value_parser = ["auto", "kitty", "iterm2", "sixel", "off"])]
    pub tui_graphics: Option<String>,

    /// Theme for generated graphics (display math): `dark` draws white ink,
    /// `light` black ink, both on a transparent background (sixel composites
    /// onto the matching solid background — the protocol has no alpha).
    /// Only consulted when `--tui-graphics` is active
    #[arg(long, value_name = "THEME", default_value = "dark",
          value_parser = ["dark", "light"])]
    pub tui_graphics_theme: String,

    /// Print detected terminal capabilities (graphics protocol, truecolor,
    /// cell geometry; live-probes the terminal when interactive) and exit
    #[arg(long, default_value_t = false)]
    pub tui_caps: bool,
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
