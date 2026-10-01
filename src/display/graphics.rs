//! Terminal graphics protocol emitters for `--tui-graphics`.
//!
//! Three wire formats, one entry point ([`write_image`]):
//!   - Kitty APC (`\x1b_G...`), base64 PNG in ≤4096-byte chunks
//!   - iTerm2 OSC 1337 inline image, base64 PNG
//!   - DEC sixel, encoded in-process from the decoded pixels (no external
//!     `img2sixel` dependency); the xterm-256 palette doubles as the
//!     quantizer via [`rgb_to_256`]
//!
//! Sizing: kitty/iTerm2 receive the full-resolution PNG plus a cell box
//! (`c`/`r`, resp. `width`/`height` in cells) and scale it themselves, so
//! hi-DPI terminals stay crisp; sixel has no cell concept, so pixels are
//! aspect-fit into `cols`×`rows` cells at an assumed [`CELL_W_PX`]×[`CELL_H_PX`]
//! px per cell. Inside tmux the payload is wrapped in DCS passthrough.

use std::io::{self, Cursor, Write};

use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, RgbImage, RgbaImage, imageops};

use crate::utils::caps::GraphicsProto;
use crate::utils::color::{rgb_to_256, xterm_256_rgb};

/// Assumed character-cell pixel geometry for the sixel pixel box. Only an
/// upper bound for scaling — terminals with larger cells simply show the
/// image smaller.
pub const CELL_W_PX: u32 = 8;
pub const CELL_H_PX: u32 = 16;

/// Ink color for generated graphics (display math). The page background is
/// transparent; sixel composites onto [`GraphicsTheme::bg`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsTheme {
    /// White ink — for dark terminal backgrounds.
    #[default]
    Dark,
    /// Black ink — for light terminal backgrounds.
    Light,
}

impl GraphicsTheme {
    /// `value_parser` on the CLI already restricts input to dark|light.
    pub fn parse(s: &str) -> Self {
        match s {
            "light" => GraphicsTheme::Light,
            _ => GraphicsTheme::Dark,
        }
    }

    /// typst color name for the formula ink.
    pub fn ink(self) -> &'static str {
        match self {
            GraphicsTheme::Dark => "white",
            GraphicsTheme::Light => "black",
        }
    }

    /// Solid background sixel composites transparent sources onto.
    pub fn bg(self) -> Rgb<u8> {
        match self {
            GraphicsTheme::Dark => Rgb([0, 0, 0]),
            GraphicsTheme::Light => Rgb([255, 255, 255]),
        }
    }
}

/// Graphics rendering options threaded into the display renderers.
#[derive(Debug, Clone, Copy, Default)]
pub struct GraphicsOpts {
    pub proto: GraphicsProto,
    pub in_tmux: bool,
    pub theme: GraphicsTheme,
    /// Whether `$...$`/`$$...$$` math spans render in Markdown mode (inline
    /// → Unicode, display → graphics image or Unicode). Separate from
    /// `proto`: with `--tui-graphics` on a graphics-less terminal the spans
    /// still get the Unicode approximation.
    pub math: bool,
}

impl GraphicsOpts {
    pub fn enabled(self) -> bool {
        self.proto != GraphicsProto::None
    }
}

/// Emit `img` at up to `rows`×`cols` cells using `proto`. A `None` protocol
/// writes nothing; callers keep their own text/block fallback.
pub fn write_image<W: Write>(
    out: &mut W,
    img: &DynamicImage,
    opts: GraphicsOpts,
    rows: usize,
    cols: usize,
) -> io::Result<()> {
    let payload = match opts.proto {
        GraphicsProto::Kitty => {
            let (r, c) = aspect_fit_cells(img.dimensions(), rows, cols);
            kitty_escape(&png_bytes(img)?, r, c)
        }
        GraphicsProto::ITerm2 => {
            let (r, c) = aspect_fit_cells(img.dimensions(), rows, cols);
            iterm2_escape(&png_bytes(img)?, r, c)
        }
        GraphicsProto::Sixel => {
            let fitted = fit_to_cells(img, rows, cols);
            let flat = flatten_alpha(&fitted, opts.theme.bg());
            sixel_encode(&flat)
        }
        GraphicsProto::None => return Ok(()),
    };
    let payload = if opts.in_tmux {
        tmux_wrap(&payload)
    } else {
        payload
    };
    out.write_all(payload.as_bytes())
}

/// Aspect-fit pixel dimensions into a `rows`×`cols` cell box and return the
/// cell dims `(rows, cols)` to advertise to the terminal. Computing them
/// here (instead of handing the terminal the bare box) keeps the aspect
/// exact even on terminals that stretch to fill `c`×`r`, and matches the
/// sixel pixel box. Never upscales beyond native pixels.
fn aspect_fit_cells((px_w, px_h): (u32, u32), max_rows: usize, max_cols: usize) -> (usize, usize) {
    if px_w == 0 || px_h == 0 {
        return (max_rows.max(1), max_cols.max(1));
    }
    let box_w = (max_cols as u32 * CELL_W_PX).max(1) as f32;
    let box_h = (max_rows as u32 * CELL_H_PX).max(1) as f32;
    let scale = (box_w / px_w as f32).min(box_h / px_h as f32).min(1.0);
    let cols = ((px_w as f32 * scale) / CELL_W_PX as f32).floor().max(1.0) as usize;
    let rows = ((px_h as f32 * scale) / CELL_H_PX as f32).floor().max(1.0) as usize;
    (rows.min(max_rows.max(1)), cols.min(max_cols.max(1)))
}

/// Wrap a graphics escape in tmux DCS passthrough (every ESC doubled).
/// Only used for forced protocols; auto detection already resolves to none
/// inside tmux.
fn tmux_wrap(payload: &str) -> String {
    format!("\x1bPtmux;{}\x1b\\", payload.replace('\x1b', "\x1b\x1b"))
}

/// Kitty graphics protocol: transmit-and-display (`a=T`), PNG (`f=100`),
/// scaled by the terminal into `c`×`r` cells, base64 payload chunked into
/// ≤4096-byte segments with `m=` continuation (only the first chunk carries
/// the action keys). `q=2` suppresses terminal responses.
fn kitty_escape(png: &[u8], rows: usize, cols: usize) -> String {
    let b64 = b64_encode(png);
    let mut out = String::new();
    let chunks: Vec<&str> = b64
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    for (i, chunk) in chunks.iter().enumerate() {
        if i == 0 {
            let more = if chunks.len() > 1 { ",m=1" } else { "" };
            out.push_str(&format!(
                "\x1b_Gq=2,a=T,f=100,c={cols},r={rows}{more};{chunk}\x1b\\"
            ));
        } else {
            let more = if i + 1 < chunks.len() { 1 } else { 0 };
            out.push_str(&format!("\x1b_Gq=2,m={more};{chunk}\x1b\\"));
        }
    }
    out.push('\n');
    out
}

/// iTerm2 OSC 1337 inline image; width/height in cells bound the box and
/// `preserveAspectRatio=1` keeps the aspect inside it.
fn iterm2_escape(png: &[u8], rows: usize, cols: usize) -> String {
    format!(
        "\x1b]1337;File=inline=1;width={cols};height={rows};preserveAspectRatio=1:{}\x07\n",
        b64_encode(png)
    )
}

fn png_bytes(img: &DynamicImage) -> io::Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| io::Error::other(format!("failed to encode PNG: {e}")))?;
    Ok(cursor.into_inner())
}

/// Aspect-fit into `(cols × CELL_W_PX) × (rows × CELL_H_PX)` px; never
/// upscales.
fn fit_to_cells(img: &DynamicImage, rows: usize, cols: usize) -> RgbaImage {
    let src = img.to_rgba8();
    let (w, h) = (src.width(), src.height());
    if w == 0 || h == 0 {
        return src;
    }
    let box_w = (cols as u32 * CELL_W_PX).max(1);
    let box_h = (rows as u32 * CELL_H_PX).max(1);
    let scale = (box_w as f32 / w as f32)
        .min(box_h as f32 / h as f32)
        .min(1.0);
    if scale >= 1.0 {
        return src;
    }
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    imageops::resize(&src, nw, nh, imageops::FilterType::Triangle)
}

/// Composite an RGBA image onto a solid background. Sixel has no alpha
/// channel; opaque pixels pass through unchanged.
fn flatten_alpha(img: &RgbaImage, bg: Rgb<u8>) -> RgbImage {
    RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        let a = p[3] as u32;
        let blend = |fg: u8, bg: u8| ((fg as u32 * a + bg as u32 * (255 - a) + 127) / 255) as u8;
        Rgb([blend(p[0], bg[0]), blend(p[1], bg[1]), blend(p[2], bg[2])])
    })
}

/// Encode an RGB image as a sixel stream: `\x1bPq`, raster attributes, the
/// used part of the xterm-256 palette as `#Pc;2;r;g;b` (0-100 scale), then
/// per 6-row band one color pass per used color (`$` rewinds between passes,
/// `-` advances bands), run-length `!N<char>` for runs ≥ 3, `\x1b\\` to end.
fn sixel_encode(img: &RgbImage) -> String {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let idx: Vec<u8> = img.pixels().map(|p| rgb_to_256(p[0], p[1], p[2])).collect();

    let mut used = [false; 256];
    for &i in &idx {
        used[i as usize] = true;
    }

    let mut out = String::from("\x1bPq");
    out.push_str(&format!("\"1;1;{w};{h}"));
    for (c, &is_used) in used.iter().enumerate() {
        if is_used {
            let (r, g, b) = xterm_256_rgb(c as u8);
            out.push_str(&format!("#{};2;{};{};{}", c, pct(r), pct(g), pct(b)));
        }
    }

    let bands = h.div_ceil(6);
    for band in 0..bands {
        let y0 = band * 6;
        let band_h = (h - y0).min(6);

        let mut band_used = [false; 256];
        for y in y0..y0 + band_h {
            for x in 0..w {
                band_used[idx[y * w + x] as usize] = true;
            }
        }

        let mut first_pass = true;
        for (c, &in_band) in band_used.iter().enumerate() {
            if !in_band {
                continue;
            }
            if !first_pass {
                out.push('$');
            }
            first_pass = false;
            out.push_str(&format!("#{c}"));

            let mut x = 0;
            while x < w {
                let mask = column_mask(&idx, w, y0, band_h, x, c as u8);
                let mut run = 1;
                while x + run < w && column_mask(&idx, w, y0, band_h, x + run, c as u8) == mask {
                    run += 1;
                }
                let ch = char::from(0x3f + mask);
                if run >= 3 {
                    out.push_str(&format!("!{run}{ch}"));
                } else {
                    for _ in 0..run {
                        out.push(ch);
                    }
                }
                x += run;
            }
        }
        if band + 1 < bands {
            out.push('-');
        }
    }
    out.push_str("\x1b\\");
    out
}

/// The sixel character for column `x` of a band: bit `p` set when the pixel
/// `p` rows below the band start uses color `c`.
fn column_mask(idx: &[u8], w: usize, y0: usize, band_h: usize, x: usize, c: u8) -> u8 {
    let mut bits = 0u8;
    for p in 0..band_h {
        if idx[(y0 + p) * w + x] == c {
            bits |= 1 << p;
        }
    }
    bits
}

/// 0-255 channel value to the 0-100 scale sixel palette definitions use.
fn pct(v: u8) -> u32 {
    (v as u32 * 100 + 127) / 255
}

/// RFC 4648 base64 with padding (no dependency; the payload for kitty and
/// iTerm2 escapes).
fn b64_encode(data: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_green_image(w: u32, h: u32) -> RgbImage {
        // Top six rows red, the rest green — deterministic indices 196/46.
        RgbImage::from_fn(w, h, |_, y| {
            if y < 6.min(h) {
                Rgb([255, 0, 0])
            } else {
                Rgb([0, 255, 0])
            }
        })
    }

    #[test]
    fn test_b64_rfc4648_vectors() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(b64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_sixel_encode_single_band_two_colors() {
        // 2x1: green left, red right → indices 46 and 196.
        let img = RgbImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgb([0, 255, 0])
            } else {
                Rgb([255, 0, 0])
            }
        });
        let s = sixel_encode(&img);
        assert_eq!(
            s,
            "\x1bPq\"1;1;2;1#46;2;0;100;0#196;2;100;0;0#46@?$#196?@\x1b\\"
        );
    }

    #[test]
    fn test_sixel_encode_two_bands_run_lengths() {
        let img = red_green_image(3, 7);
        let s = sixel_encode(&img);
        // Band 0: six red rows → mask 0b111111='~', run of 3 → "!3~".
        // Band 1: one green row → mask 0b000001='@', run of 3 → "!3@".
        assert_eq!(
            s,
            "\x1bPq\"1;1;3;7#46;2;0;100;0#196;2;100;0;0#196!3~-#46!3@\x1b\\"
        );
    }

    #[test]
    fn test_sixel_encode_short_runs_stay_literal() {
        let img = red_green_image(2, 7);
        let s = sixel_encode(&img);
        assert_eq!(
            s,
            "\x1bPq\"1;1;2;7#46;2;0;100;0#196;2;100;0;0#196~~-#46@@\x1b\\"
        );
    }

    #[test]
    fn test_kitty_single_chunk() {
        let out = kitty_escape(b"png-bytes", 8, 12);
        assert!(out.starts_with("\x1b_Gq=2,a=T,f=100,c=12,r=8;"), "{out:?}");
        assert!(out.ends_with("\x1b\\\n"), "{out:?}");
        assert!(!out.contains("m="), "{out:?}");
    }

    #[test]
    fn test_kitty_chunking_continuation() {
        // 4000 bytes → 5336 base64 chars → two chunks.
        let out = kitty_escape(&vec![7u8; 4000], 4, 2);
        assert!(
            out.starts_with("\x1b_Gq=2,a=T,f=100,c=2,r=4,m=1;"),
            "{out:?}"
        );
        assert!(out.contains("\x1b\\\x1b_Gq=2,m=0;"), "{out:?}");
        assert!(out.ends_with("\x1b\\\n"), "{out:?}");
        // First chunk carries exactly 4096 base64 chars.
        let first = &out[out.find(';').unwrap() + 1..out.find("\x1b\\").unwrap()];
        assert_eq!(first.len(), 4096);
    }

    #[test]
    fn test_iterm2_escape_shape() {
        let out = iterm2_escape(b"foo", 8, 12);
        assert_eq!(
            out,
            "\x1b]1337;File=inline=1;width=12;height=8;preserveAspectRatio=1:Zm9v\x07\n"
        );
    }

    #[test]
    fn test_tmux_wrap_doubles_escapes() {
        assert_eq!(tmux_wrap("a\x1bb"), "\x1bPtmux;a\x1b\x1bb\x1b\\");
    }

    #[test]
    fn test_fit_to_cells_shrinks_preserving_aspect() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 100, image::Rgba([0; 4])));
        let fitted = fit_to_cells(&img, 8, 12); // box 96x128 px
        assert_eq!(fitted.dimensions(), (96, 96));
    }

    #[test]
    fn test_fit_to_cells_never_upscales() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(10, 5, image::Rgba([0; 4])));
        let fitted = fit_to_cells(&img, 8, 12);
        assert_eq!(fitted.dimensions(), (10, 5));
    }

    #[test]
    fn test_flatten_alpha_blends() {
        let mut img = RgbaImage::new(3, 1);
        img.put_pixel(0, 0, image::Rgba([255, 255, 255, 128]));
        img.put_pixel(1, 0, image::Rgba([200, 100, 50, 255]));
        img.put_pixel(2, 0, image::Rgba([9, 9, 9, 0]));
        let flat = flatten_alpha(&img, Rgb([0, 0, 0]));
        assert_eq!(flat.get_pixel(0, 0).0, [128, 128, 128]);
        assert_eq!(flat.get_pixel(1, 0).0, [200, 100, 50]);
        assert_eq!(flat.get_pixel(2, 0).0, [0, 0, 0]);
    }

    #[test]
    fn test_theme_defaults_and_values() {
        assert_eq!(GraphicsTheme::default(), GraphicsTheme::Dark);
        assert_eq!(GraphicsTheme::Dark.ink(), "white");
        assert_eq!(GraphicsTheme::Light.ink(), "black");
        assert_eq!(GraphicsTheme::Dark.bg().0, [0, 0, 0]);
        assert_eq!(GraphicsTheme::Light.bg().0, [255, 255, 255]);
        assert_eq!(GraphicsTheme::parse("light"), GraphicsTheme::Light);
        assert_eq!(GraphicsTheme::parse("dark"), GraphicsTheme::Dark);
    }

    #[test]
    fn test_write_image_none_writes_nothing() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(1, 1));
        let mut buf = Vec::new();
        write_image(&mut buf, &img, GraphicsOpts::default(), 8, 12).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_write_image_sixel_bytes() {
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])));
        let mut buf = Vec::new();
        let opts = GraphicsOpts {
            proto: GraphicsProto::Sixel,
            in_tmux: false,
            theme: GraphicsTheme::Dark,
            math: false,
        };
        write_image(&mut buf, &img, opts, 8, 12).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("\x1bPq"), "{s:?}");
        assert!(s.ends_with("\x1b\\"), "{s:?}");
    }

    #[test]
    fn test_aspect_fit_cells() {
        // 512x460 into a 2x4-cell box (32x64 px): width-limited.
        assert_eq!(aspect_fit_cells((512, 460), 2, 4), (1, 4));
        // Small images keep native size: 10x5 px ≈ 1x1 cell.
        assert_eq!(aspect_fit_cells((10, 5), 8, 12), (1, 1));
        // A wide flat formula: 400x50 into 12x72 cells (576x192 px).
        assert_eq!(aspect_fit_cells((400, 50), 12, 72), (3, 50));
        // Never exceeds the box: square into 8x12 cells fits at 6x12.
        assert_eq!(aspect_fit_cells((10000, 10000), 8, 12), (6, 12));
        assert_eq!(aspect_fit_cells((0, 0), 8, 12), (8, 12));
    }
}
