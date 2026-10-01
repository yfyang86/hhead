//! TeX math rendering for `--markdown` under `--tui-graphics`.
//!
//! Display math (`$$...$$`) becomes a PNG via external `typst` when it is on
//! PATH and is emitted through the terminal graphics protocol; every failure
//! degrades to a pure-Unicode approximation. The degradation chain is:
//!
//! ```text
//! typst (LaTeX-ish -> typst translation, `typst compile` -> transparent PNG)
//!   -> Unicode approximation (always available)
//! ```
//!
//! The typst page uses `fill: none` and a theme ink (`--tui-graphics-theme`):
//! dark = white ink for dark terminals, light = black ink. Kitty/iTerm2
//! composite the transparent PNG over the real terminal background; sixel
//! flattens onto the theme background (see `display::graphics`).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::graphics::{GraphicsOpts, GraphicsTheme, write_image};

/// Display-math cell box: aspect-fit target for formula images.
pub const MATH_ROWS: usize = 12;
pub const MATH_COLS: usize = 72;

/// Render a display-math fragment: graphics protocol + typst PNG when
/// possible, else the Unicode approximation as a line of text.
pub fn write_display_math<W: Write>(out: &mut W, tex: &str, opts: GraphicsOpts) -> io::Result<()> {
    if opts.enabled()
        && let Some(img) =
            render_math_png(tex, opts.theme).and_then(|png| image::load_from_memory(&png).ok())
    {
        return write_image(out, &img, opts, MATH_ROWS, MATH_COLS);
    }
    writeln!(out, "{}", unicode_math(tex))
}

// ---------------------------------------------------------------------------
// external renderer
// ---------------------------------------------------------------------------

/// typst math syntax is not LaTeX; do a best-effort translation of common
/// LaTeX constructs before compiling a one-line math doc. The page has no
/// fill so the PNG keeps a transparent background; the ink follows the theme.
///
/// PNG export needs typst ≥ 0.4: the modern `--format png` is tried first,
/// then a bare compile (old typst silently writes a PDF at the .png path,
/// which the magic-byte check rejects — degrading to the Unicode fallback).
fn render_math_png(tex: &str, theme: GraphicsTheme) -> Option<Vec<u8>> {
    which("typst")?;
    let dir = make_temp_dir()?;
    let doc = format!(
        "#set page(width: auto, height: auto, margin: 4pt, fill: none)\n#set text(fill: {})\n$ {} $\n",
        theme.ink(),
        latex_to_typst(tex)
    );
    let src = dir.join("math.typ");
    let out = dir.join("math.png");
    let ok = std::fs::write(&src, doc).is_ok() && typst_compile(&src, &out);
    let png = if ok { std::fs::read(&out).ok() } else { None };
    let _ = std::fs::remove_dir_all(&dir);
    png.filter(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
}

fn typst_compile(src: &Path, out: &Path) -> bool {
    let run = |extra: &[&str]| {
        Command::new("typst")
            .arg("compile")
            .args(extra)
            .arg(src)
            .arg(out)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    run(&["--format", "png"]) || (run(&[]) && out.is_file())
}

/// Scratch dir for the typst compile (the `tempfile` crate is a dev-only
/// dependency, so roll the trivial version here).
fn make_temp_dir() -> Option<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("hhead-math-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir).ok()?;
    Some(dir)
}

/// Minimal `which(1)`: executable named `name` on PATH.
fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|dir| {
        let candidate = dir.join(name);
        let exec = candidate.is_file() && is_executable(&candidate);
        exec.then_some(candidate)
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

// ---------------------------------------------------------------------------
// LaTeX -> typst translation (best effort)
// ---------------------------------------------------------------------------

/// Translate common LaTeX math into typst math syntax. Unknown commands are
/// passed through with the backslash stripped (typst often has a same-named
/// symbol); anything that still fails to compile just degrades to the
/// Unicode approximation.
fn latex_to_typst(tex: &str) -> String {
    unicode_transform(tex, &TypstSink)
}

// ---------------------------------------------------------------------------
// Unicode approximation
// ---------------------------------------------------------------------------

/// Pure-Rust Unicode approximation of a TeX math fragment. Handles greek
/// letters, common operators/relations, `\frac{a}{b}` -> `a/b`, `\sqrt{x}`
/// -> `√(x)`, super/subscripts (with real Unicode sup/sub characters where
/// they exist), and drops sizing/spacing commands. Anything unrecognized is
/// passed through with the backslash stripped.
pub fn unicode_math(tex: &str) -> String {
    unicode_transform(tex, &UnicodeSink)
}

/// Output-flavor abstraction shared by the typst translator and the
/// Unicode approximation: both walk the same token stream but emit either
/// typst names or Unicode glyphs.
trait MathSink {
    fn command(&self, name: &str) -> Option<String>;
    fn frac(&self, num: &str, den: &str) -> String;
    fn sqrt(&self, arg: &str) -> String;
    fn sup(&self, base_end: &mut String, arg: &str) -> String;
    fn sub(&self, base_end: &mut String, arg: &str) -> String;
}

struct UnicodeSink;
struct TypstSink;

/// Map a single char to its Unicode superscript form.
fn sup_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        _ => return None,
    })
}

/// Map a single char to its Unicode subscript form.
fn sub_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

fn to_sup(s: &str) -> Option<String> {
    s.chars().map(sup_char).collect()
}

fn to_sub(s: &str) -> Option<String> {
    s.chars().map(sub_char).collect()
}

/// Parenthesize a group when it contains more than one "atom".
fn paren_if_complex(s: &str) -> String {
    if s.chars().count() <= 1 {
        s.to_string()
    } else {
        format!("({s})")
    }
}

impl MathSink for UnicodeSink {
    fn command(&self, name: &str) -> Option<String> {
        Some(
            match name {
                // greek lowercase
                "alpha" => "α",
                "beta" => "β",
                "gamma" => "γ",
                "delta" => "δ",
                "epsilon" | "varepsilon" => "ε",
                "zeta" => "ζ",
                "eta" => "η",
                "theta" | "vartheta" => "θ",
                "iota" => "ι",
                "kappa" => "κ",
                "lambda" => "λ",
                "mu" => "μ",
                "nu" => "ν",
                "xi" => "ξ",
                "pi" => "π",
                "rho" | "varrho" => "ρ",
                "sigma" | "varsigma" => "σ",
                "tau" => "τ",
                "upsilon" => "υ",
                "phi" | "varphi" => "φ",
                "chi" => "χ",
                "psi" => "ψ",
                "omega" => "ω",
                // greek uppercase
                "Gamma" => "Γ",
                "Delta" => "Δ",
                "Theta" => "Θ",
                "Lambda" => "Λ",
                "Xi" => "Ξ",
                "Pi" => "Π",
                "Sigma" => "Σ",
                "Phi" => "Φ",
                "Psi" => "Ψ",
                "Omega" => "Ω",
                // big operators & relations
                "sum" => "∑",
                "prod" => "∏",
                "int" => "∫",
                "iint" => "∬",
                "oint" => "∮",
                "infty" => "∞",
                "partial" => "∂",
                "nabla" => "∇",
                "pm" => "±",
                "mp" => "∓",
                "times" => "×",
                "div" => "÷",
                "cdot" => "·",
                "ast" => "∗",
                "circ" => "∘",
                "bullet" => "∙",
                "le" | "leq" => "≤",
                "ge" | "geq" => "≥",
                "ne" | "neq" => "≠",
                "equiv" => "≡",
                "approx" => "≈",
                "sim" => "∼",
                "simeq" => "≃",
                "propto" => "∝",
                "ll" => "≪",
                "gg" => "≫",
                "in" => "∈",
                "notin" => "∉",
                "ni" => "∋",
                "subset" => "⊂",
                "supset" => "⊃",
                "subseteq" => "⊆",
                "supseteq" => "⊇",
                "cup" => "∪",
                "cap" => "∩",
                "setminus" => "∖",
                "emptyset" => "∅",
                "forall" => "∀",
                "exists" => "∃",
                "neg" | "lnot" => "¬",
                "land" | "wedge" => "∧",
                "lor" | "vee" => "∨",
                "oplus" => "⊕",
                "otimes" => "⊗",
                "perp" => "⊥",
                "parallel" => "∥",
                "angle" => "∠",
                "degree" => "°",
                "prime" => "′",
                "rightarrow" | "to" => "→",
                "leftarrow" | "gets" => "←",
                "leftrightarrow" => "↔",
                "Rightarrow" => "⇒",
                "Leftarrow" => "⇐",
                "Leftrightarrow" => "⇔",
                "mapsto" => "↦",
                "uparrow" => "↑",
                "downarrow" => "↓",
                "ldots" | "dots" => "…",
                "cdots" => "⋯",
                "vdots" => "⋮",
                "ddots" => "⋱",
                "hbar" => "ℏ",
                "ell" => "ℓ",
                "Re" => "ℜ",
                "Im" => "ℑ",
                "aleph" => "ℵ",
                "mathbb{R}" => "ℝ",
                "mathbb{Z}" => "ℤ",
                "mathbb{N}" => "ℕ",
                "mathbb{Q}" => "ℚ",
                "mathbb{C}" => "ℂ",
                // spacing / styling: drop
                "," | ";" | "!" | " " | "quad" | "qquad" | "displaystyle" | "limits" | "left"
                | "right" | "big" | "Big" | "bigg" | "Bigg" | "textstyle" => "",
                _ => return None,
            }
            .to_string(),
        )
    }

    fn frac(&self, num: &str, den: &str) -> String {
        format!("{}/{}", paren_if_complex(num), paren_if_complex(den))
    }

    fn sqrt(&self, arg: &str) -> String {
        format!("√{}", paren_if_complex(arg))
    }

    fn sup(&self, _base: &mut String, arg: &str) -> String {
        match to_sup(arg) {
            Some(s) => s,
            None => format!("^{}", paren_if_complex(arg)),
        }
    }

    fn sub(&self, _base: &mut String, arg: &str) -> String {
        match to_sub(arg) {
            Some(s) => s,
            None => format!("_{}", paren_if_complex(arg)),
        }
    }
}

impl MathSink for TypstSink {
    fn command(&self, name: &str) -> Option<String> {
        Some(
            match name {
                // typst uses the same names for greek and most operators.
                "varepsilon" => "epsilon",
                "vartheta" => "theta",
                "le" | "leq" => "<=",
                "ge" | "geq" => ">=",
                "ne" | "neq" => "!=",
                "cdot" => "dot",
                "rightarrow" | "to" => "->",
                "leftarrow" | "gets" => "<-",
                "Rightarrow" => "=>",
                "Leftarrow" => "<=",
                "mathbb{R}" => "RR",
                "mathbb{Z}" => "ZZ",
                "mathbb{N}" => "NN",
                "mathbb{Q}" => "QQ",
                "mathbb{C}" => "CC",
                "," | ";" | " " | "quad" => " ",
                "!" | "displaystyle" | "limits" | "left" | "right" | "big" | "Big" | "bigg"
                | "Bigg" | "textstyle" | "qquad" => "",
                other => return Some(other.to_string()),
            }
            .to_string(),
        )
    }

    fn frac(&self, num: &str, den: &str) -> String {
        format!("frac({num}, {den})")
    }

    fn sqrt(&self, arg: &str) -> String {
        format!("sqrt({arg})")
    }

    fn sup(&self, _base: &mut String, arg: &str) -> String {
        format!("^({arg})")
    }

    fn sub(&self, _base: &mut String, arg: &str) -> String {
        format!("_({arg})")
    }
}

// ---------------------------------------------------------------------------
// shared TeX-ish token walker
// ---------------------------------------------------------------------------

/// One-level recursive walker over a TeX-ish token stream, emitting via
/// `sink`. Handles `\command`, `{groups}`, `^{...}`/`_{...}` scripts,
/// `\frac`, `\sqrt`, `\mathbb{X}`, and passes anything else through.
fn unicode_transform(tex: &str, sink: &dyn MathSink) -> String {
    let chars: Vec<char> = tex.chars().collect();
    let mut pos = 0usize;
    transform_group(&chars, &mut pos, sink, false)
}

/// Transform tokens until end of input or (when `in_group`) the closing
/// brace that ends the current group.
fn transform_group(chars: &[char], pos: &mut usize, sink: &dyn MathSink, in_group: bool) -> String {
    let mut out = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        match c {
            '}' if in_group => {
                *pos += 1;
                return out;
            }
            '{' => {
                *pos += 1;
                out.push_str(&transform_group(chars, pos, sink, true));
            }
            '}' => {
                *pos += 1; // stray brace: drop
            }
            '^' | '_' => {
                *pos += 1;
                let arg = read_script_arg(chars, pos, sink);
                let rendered = if c == '^' {
                    sink.sup(&mut out, &arg)
                } else {
                    sink.sub(&mut out, &arg)
                };
                out.push_str(&rendered);
            }
            '\\' => {
                *pos += 1;
                transform_command(chars, pos, sink, &mut out);
            }
            c if c.is_whitespace() => {
                *pos += 1;
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
            }
            _ => {
                *pos += 1;
                out.push(c);
            }
        }
    }
    out
}

/// Read a script argument: either `{...}` or a single token.
fn read_script_arg(chars: &[char], pos: &mut usize, sink: &dyn MathSink) -> String {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
    if *pos >= chars.len() {
        return String::new();
    }
    if chars[*pos] == '{' {
        *pos += 1;
        transform_group(chars, pos, sink, true)
    } else {
        // single token (char or command)
        let mut one = String::new();
        if chars[*pos] == '\\' {
            *pos += 1;
            transform_command(chars, pos, sink, &mut one);
        } else {
            one.push(chars[*pos]);
            *pos += 1;
        }
        one
    }
}

/// Handle a `\command` at `pos` (just past the backslash).
fn transform_command(chars: &[char], pos: &mut usize, sink: &dyn MathSink, out: &mut String) {
    if *pos >= chars.len() {
        return;
    }
    // Control symbols like \, \; \{ \} are single non-letter chars.
    if !chars[*pos].is_ascii_alphabetic() {
        let sym = chars[*pos];
        *pos += 1;
        match sym {
            '{' => out.push('{'),
            '}' => out.push('}'),
            '$' | '%' | '&' | '#' | '_' => out.push(sym),
            other => {
                if let Some(s) = sink.command(&other.to_string()) {
                    out.push_str(&s);
                }
            }
        }
        return;
    }
    let start = *pos;
    while *pos < chars.len() && chars[*pos].is_ascii_alphabetic() {
        *pos += 1;
    }
    let name: String = chars[start..*pos].iter().collect();
    match name.as_str() {
        "frac" | "dfrac" | "tfrac" => {
            let num = read_braced_group(chars, pos, sink);
            let den = read_braced_group(chars, pos, sink);
            out.push_str(&sink.frac(&num, &den));
        }
        "sqrt" => {
            skip_optional_bracket(chars, pos);
            let arg = read_braced_group(chars, pos, sink);
            out.push_str(&sink.sqrt(&arg));
        }
        "mathbb" | "mathcal" | "mathrm" | "mathbf" | "mathit" | "operatorname" | "text"
        | "textrm" | "textbf" => {
            let arg = read_braced_group(chars, pos, sink);
            let key = format!("mathbb{{{arg}}}");
            if name == "mathbb"
                && let Some(s) = sink.command(&key)
            {
                out.push_str(&s);
                return;
            }
            out.push_str(&arg);
        }
        _ => match sink.command(&name) {
            Some(s) => out.push_str(&s),
            None => out.push_str(&name),
        },
    }
}

/// Read a `{...}` group (or a single token) as an argument.
fn read_braced_group(chars: &[char], pos: &mut usize, sink: &dyn MathSink) -> String {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
    if *pos < chars.len() && chars[*pos] == '{' {
        *pos += 1;
        transform_group(chars, pos, sink, true)
    } else {
        read_script_arg(chars, pos, sink)
    }
}

/// Skip an optional `[...]` argument (e.g. `\sqrt[3]{x}` — the index is
/// dropped in the approximation).
fn skip_optional_bracket(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
    if *pos < chars.len() && chars[*pos] == '[' {
        let mut depth = 0usize;
        while *pos < chars.len() {
            match chars[*pos] {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        *pos += 1;
                        return;
                    }
                }
                _ => {}
            }
            *pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unicode_math_greek_and_frac() {
        assert_eq!(
            unicode_math("\\alpha + \\frac{1}{2} = \\sum_{i=1}^{n} x_i"),
            "α + 1/2 = ∑ᵢ₌₁ⁿ xᵢ"
        );
    }

    #[test]
    fn test_unicode_math_sqrt_and_scripts() {
        assert_eq!(unicode_math("\\sqrt{x+1}"), "√(x+1)");
        assert_eq!(unicode_math("e^{i\\pi}"), "e^(iπ)"); // π has no superscript
        assert_eq!(unicode_math("x^2"), "x²");
        assert_eq!(unicode_math("a_n"), "aₙ");
    }

    #[test]
    fn test_unicode_math_misc_commands() {
        assert_eq!(unicode_math("\\mathbb{R} \\times \\infty"), "ℝ × ∞");
        assert_eq!(unicode_math("x \\le y \\ne z"), "x ≤ y ≠ z");
        // unknown commands pass through with the backslash stripped
        assert_eq!(unicode_math("\\unknowncmd{x}"), "unknowncmdx");
    }

    #[test]
    fn test_latex_to_typst() {
        assert_eq!(latex_to_typst("\\frac{1}{2}"), "frac(1, 2)");
        assert_eq!(latex_to_typst("x \\le 1"), "x <= 1");
        assert_eq!(latex_to_typst("\\sqrt{\\alpha}"), "sqrt(alpha)");
        assert_eq!(latex_to_typst("x^2"), "x^(2)");
    }

    #[test]
    fn test_typst_render_theme_and_transparency() {
        if which("typst").is_none() {
            eprintln!("typst not on PATH; skipping render test");
            return;
        }
        // typst < 0.4 has no PNG export; the renderer degrades to None.
        let Some(png) = render_math_png("\\frac{1}{2}", GraphicsTheme::Dark) else {
            eprintln!("typst too old for PNG export; skipping render test");
            return;
        };
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG magic: {png:?}");
        let img = image::load_from_memory(&png).expect("decode PNG");
        let rgba = img.to_rgba8();
        let corner = rgba.get_pixel(0, 0);
        assert_eq!(corner[3], 0, "page background should be transparent");
    }
}
