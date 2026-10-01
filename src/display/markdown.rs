//! Markdown rendering for the terminal
//!
//! A lightweight, line-based renderer: headings, fenced code blocks, tables
//! (GFM pipe tables plus pandoc's grid and simple/multiline dash tables with
//! `: caption` lines), and figures — `![alt](path)` on its own line is drawn
//! as a 256-color minimap (or a terminal-graphics image). With
//! `--tui-graphics`, TeX math renders too: `$...$`, pandoc-gfm `` `$`...`$` ``
//! inline spans, `$$...$$` lines/blocks, and ``` math fenced blocks. Inline
//! emphasis, code, and link markers are ANSI-styled when color is enabled
//! and stripped otherwise. It is intentionally not a full CommonMark
//! implementation.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use colored::Colorize;

use super::graphics::GraphicsOpts;
use super::minimap::{write_minimap, write_minimap_graphics};

/// Rendering options for Markdown mode.
#[derive(Debug, Clone, Copy)]
pub struct MarkdownOpts {
    /// ANSI-style inline markers and headings.
    pub color: bool,
    /// Figure box in terminal cells (rows × cols).
    pub img_rows: usize,
    pub img_cols: usize,
    /// Paint table columns with the `--csv-rainbow` palette.
    pub rainbow: bool,
    /// Terminal graphics for figures and math spans.
    pub graphics: GraphicsOpts,
}

impl Default for MarkdownOpts {
    fn default() -> Self {
        MarkdownOpts {
            color: false,
            img_rows: 8,
            img_cols: 12,
            rainbow: false,
            graphics: GraphicsOpts::default(),
        }
    }
}

/// Render a Markdown file to stdout.
///
/// Reads the whole file: rendering needs the complete document, so the
/// `--bytes` limit does not apply in Markdown mode. Figures are resolved
/// relative to the Markdown file's directory and drawn on an
/// `img_rows` × `img_cols` grid — as a 256-color minimap, or through a
/// terminal graphics protocol when `graphics` enables one.
pub fn display_markdown(path: &Path, opts: MarkdownOpts) -> io::Result<()> {
    let data = std::fs::read(path)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_markdown(&mut out, &data, path.parent(), opts)
}

/// Same as [`display_markdown`] but writes to an arbitrary [`Write`] and
/// takes the already-read bytes. Exposed for testing.
///
/// `opts.rainbow` paints table columns with the `--csv-rainbow` palette;
/// cells are then rendered with inline markers stripped (no nested ANSI), so
/// the column color runs the full cell.
pub fn write_markdown<W: Write>(
    out: &mut W,
    data: &[u8],
    base_dir: Option<&Path>,
    opts: MarkdownOpts,
) -> io::Result<()> {
    let text = String::from_utf8_lossy(data);
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut in_code = false;
    let mut table_just_ended = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // GFM display math: pandoc's gfm writer emits ``` math fenced
        // blocks. With math disabled they stay verbatim code blocks.
        if !in_code && opts.graphics.math && is_math_fence(trimmed) {
            let mut j = i + 1;
            let mut parts: Vec<&str> = Vec::new();
            while j < lines.len() && !lines[j].trim().starts_with("```") {
                parts.push(lines[j].trim());
                j += 1;
            }
            if j < lines.len() {
                super::math::write_display_math(out, &parts.join(" "), opts.graphics)?;
                table_just_ended = false;
                i = j + 1;
                continue;
            }
            // Unterminated fence: fall through to the generic code handling.
        }

        // Fenced code blocks: content passes through verbatim.
        if trimmed.starts_with("```") {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if in_code {
            writeln!(out, "{}", line)?;
            i += 1;
            continue;
        }

        if trimmed.is_empty() {
            writeln!(out)?;
            i += 1;
            continue;
        }

        // Pandoc table caption (`: caption` after a table, optionally with
        // one blank line between).
        if table_just_ended && trimmed.starts_with(": ") {
            writeln!(out, "{}", render_inline(trimmed[2..].trim(), opts.color))?;
            table_just_ended = false;
            i += 1;
            continue;
        }

        // Figure: a line that is just `![alt](src)`.
        if let Some((alt, src)) = parse_image_line(trimmed) {
            render_figure(out, &alt, &src, base_dir, opts)?;
            table_just_ended = false;
            i += 1;
            continue;
        }

        // Display math: `$$`-fenced block, or one or more `$$...$$` spans on
        // the line (pandoc joins consecutive display equations onto one
        // line). Only with --tui-graphics (graphics.math); otherwise literal.
        if opts.graphics.math && trimmed.starts_with("$$") {
            if trimmed == "$$" {
                let mut j = i + 1;
                let mut parts: Vec<&str> = Vec::new();
                while j < lines.len() && lines[j].trim() != "$$" {
                    parts.push(lines[j].trim());
                    j += 1;
                }
                if j < lines.len() {
                    super::math::write_display_math(out, &parts.join(" "), opts.graphics)?;
                    table_just_ended = false;
                    i = j + 1;
                    continue;
                }
                // Unterminated `$$`: fall through and render literally.
            } else {
                let dchars: Vec<char> = trimmed.chars().collect();
                let mut spans: Vec<String> = Vec::new();
                let mut pos = 0;
                let mut whole_line = true;
                while pos < dchars.len() {
                    if dchars[pos].is_whitespace() {
                        pos += 1;
                        continue;
                    }
                    match scan_math_span(&dchars, pos) {
                        Some((tex, next))
                            if dchars[pos] == '$' && dchars.get(pos + 1) == Some(&'$') =>
                        {
                            spans.push(tex);
                            pos = next;
                        }
                        _ => {
                            whole_line = false;
                            break;
                        }
                    }
                }
                if whole_line && !spans.is_empty() {
                    for tex in &spans {
                        super::math::write_display_math(out, tex, opts.graphics)?;
                    }
                    table_just_ended = false;
                    i += 1;
                    continue;
                }
                // Mixed content: the paragraph path renders spans inline.
            }
        }

        // GFM pipe table: header row, separator row, then body rows.
        if trimmed.contains('|') && i + 1 < lines.len() && is_separator_row(lines[i + 1]) {
            let mut body: Vec<&str> = Vec::new();
            let mut j = i + 2;
            while j < lines.len() && lines[j].trim().contains('|') {
                body.push(lines[j]);
                j += 1;
            }
            render_pipe_table(out, trimmed, lines[i + 1], &body, opts.color, opts.rainbow)?;
            table_just_ended = true;
            i = j;
            continue;
        }

        // Pandoc grid table: `+---+`/`+===+` borders.
        if is_grid_border(trimmed)
            && let Some((table, next)) = parse_grid_table(&lines, i)
        {
            render_table_data(out, &table, opts.color, opts.rainbow)?;
            table_just_ended = true;
            i = next;
            continue;
        }

        // Pandoc simple/multiline table: dash separator under the header, or
        // a top dash border.
        if let Some((table, next)) = parse_dash_table(&lines, i) {
            render_table_data(out, &table, opts.color, opts.rainbow)?;
            table_just_ended = true;
            i = next;
            continue;
        }

        // Heading: 1-6 '#' followed by a space (or nothing).
        if let Some(heading) = parse_heading(trimmed) {
            writeln!(out, "{}", style(&heading, Style::Heading, opts.color))?;
            table_just_ended = false;
            i += 1;
            continue;
        }

        // Everything else (lists, quotes, rules, paragraphs) passes through
        // the inline renderer.
        writeln!(
            out,
            "{}",
            render_inline_math(line, opts.color, opts.graphics)
        )?;
        table_just_ended = false;
        i += 1;
    }
    Ok(())
}

/// Paragraph inline rendering. With math enabled, `$...$`/`$$...$$` spans
/// outside code spans become their Unicode approximation; each span is
/// protected with a private-use placeholder while `render_inline` runs so
/// math content is never mistaken for emphasis/link markers.
fn render_inline_math(text: &str, color: bool, graphics: GraphicsOpts) -> String {
    if !graphics.math {
        return render_inline(text, color);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<String> = Vec::new();
    let mut protected = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_code = false;
    while i < chars.len() {
        // GFM inline math (pandoc's gfm writer): `$`...`$` — check before
        // the backtick toggle so the span markers aren't read as code.
        if !in_code
            && chars[i] == '$'
            && chars.get(i + 1) == Some(&'`')
            && let Some((tex, next)) = scan_gfm_math_span(&chars, i)
        {
            spans.push(super::math::unicode_math(&tex));
            protected.push_str(&format!("\u{E000}{}\u{E001}", spans.len() - 1));
            i = next;
            continue;
        }
        if chars[i] == '`' {
            in_code = !in_code;
            protected.push('`');
            i += 1;
            continue;
        }
        if !in_code
            && chars[i] == '$'
            && let Some((tex, next)) = scan_math_span(&chars, i)
        {
            spans.push(super::math::unicode_math(&tex));
            protected.push_str(&format!("\u{E000}{}\u{E001}", spans.len() - 1));
            i = next;
            continue;
        }
        protected.push(chars[i]);
        i += 1;
    }
    let mut rendered = render_inline(&protected, color);
    for (idx, span) in spans.iter().enumerate() {
        rendered = rendered.replace(&format!("\u{E000}{idx}\u{E001}"), span);
    }
    rendered
}

/// Scan a `$...$` or `$$...$$` span starting at `chars[i] == '$'`. Pandoc
/// guard rules keep currency literal: the opening `$` is not followed by
/// whitespace, the closing `$` is not preceded by whitespace and not
/// followed by a digit. Returns the TeX source and the index just past the
/// closing marker.
fn scan_math_span(chars: &[char], i: usize) -> Option<(String, usize)> {
    let mut j = i + 1;
    let double = chars.get(j) == Some(&'$');
    if double {
        j += 1;
    }
    if chars.get(j).map(|c| c.is_whitespace()).unwrap_or(true) {
        return None;
    }
    let mut k = j;
    while k < chars.len() {
        if chars[k] == '$' {
            if chars[k - 1].is_whitespace() {
                return None;
            }
            if double {
                if chars.get(k + 1) == Some(&'$') {
                    return Some((chars[j..k].iter().collect(), k + 2));
                }
            } else {
                if chars
                    .get(k + 1)
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    return None;
                }
                return Some((chars[j..k].iter().collect(), k + 1));
            }
        }
        k += 1;
    }
    None
}

/// Scan a GFM inline math span `$`...`$` starting at `chars[i] == '$'`
/// (pandoc's gfm writer wraps TeX in dollar-backticks). Same whitespace
/// guards as [`scan_math_span`].
fn scan_gfm_math_span(chars: &[char], i: usize) -> Option<(String, usize)> {
    let content_start = i + 2; // past "$`"
    if chars
        .get(content_start)
        .map(|c| c.is_whitespace())
        .unwrap_or(true)
    {
        return None;
    }
    let mut k = content_start;
    while k + 1 < chars.len() {
        if chars[k] == '`' && chars[k + 1] == '$' {
            if chars[k - 1].is_whitespace() {
                return None;
            }
            return Some((chars[content_start..k].iter().collect(), k + 2));
        }
        k += 1;
    }
    None
}

/// A ``` math fenced block line (pandoc gfm display math).
fn is_math_fence(line: &str) -> bool {
    line.strip_prefix("```")
        .map(|info| info.trim().eq_ignore_ascii_case("math"))
        .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Heading,
    Bold,
    Italic,
    Code,
}

fn style(text: &str, s: Style, color: bool) -> String {
    if !color {
        return text.to_string();
    }
    match s {
        Style::Heading => text.bold().cyan().to_string(),
        Style::Bold => text.bold().to_string(),
        Style::Italic => text.italic().to_string(),
        Style::Code => text.yellow().to_string(),
    }
}

/// Parse a line consisting solely of `![alt](src)` (optional `"title"`).
fn parse_image_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = &rest[..close];
    let src = rest[close + 2..].strip_suffix(')')?;
    // Optional title: ![alt](src "title")
    let src = match src.split_once(' ') {
        Some((path, title)) if title.trim_start().starts_with('"') => path,
        _ => src,
    };
    Some((alt.trim().to_string(), src.trim().to_string()))
}

fn render_figure<W: Write>(
    out: &mut W,
    alt: &str,
    src: &str,
    base_dir: Option<&Path>,
    opts: MarkdownOpts,
) -> io::Result<()> {
    let caption = if alt.is_empty() { src } else { alt };
    writeln!(out, "{}", render_inline(caption, opts.color))?;

    if src.starts_with("http://") || src.starts_with("https://") {
        return writeln!(out, "[remote image not rendered: {}]", src);
    }

    let resolved = match base_dir {
        Some(dir) => dir.join(src),
        None => PathBuf::from(src),
    };
    // With --tui-graphics, try the protocol emitter first; a decode failure
    // falls back to the block minimap, which reports the placeholder.
    if opts.graphics.enabled()
        && write_minimap_graphics(out, &resolved, opts.img_rows, opts.img_cols, opts.graphics)
            .is_ok()
    {
        return Ok(());
    }
    match write_minimap(out, &resolved, opts.img_rows, opts.img_cols) {
        Ok(()) => Ok(()),
        Err(_) => writeln!(out, "[image not rendered: {}]", src),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
    Center,
}

/// A parsed table, source-format agnostic (pipe, grid, or pandoc dash
/// tables all reduce to this).
struct TableData {
    header: Vec<String>,
    aligns: Vec<Align>,
    body: Vec<Vec<String>>,
}

/// A separator row like `| --- | :-: | --: |` (also a bare `---`).
fn is_separator_row(line: &str) -> bool {
    let t = line.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '-' | ':' | ' ' | '|' | '\t'))
}

/// Split a pipe-table row into trimmed cells. `\|` stays a literal pipe.
fn split_row(line: &str) -> Vec<String> {
    const ESC: &str = "\u{0}";
    let mut t = line.trim().replace("\\|", ESC);
    if t.starts_with('|') {
        t.remove(0);
    }
    if t.ends_with('|') {
        t.pop();
    }
    t.split('|').map(|c| c.trim().replace(ESC, "|")).collect()
}

fn parse_aligns(sep_line: &str, ncols: usize) -> Vec<Align> {
    let cells = split_row(sep_line);
    (0..ncols)
        .map(|j| {
            let cell = cells.get(j).map(|s| s.trim()).unwrap_or("");
            match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            }
        })
        .collect()
}

fn render_pipe_table<W: Write>(
    out: &mut W,
    header_line: &str,
    sep_line: &str,
    body_lines: &[&str],
    color: bool,
    rainbow: bool,
) -> io::Result<()> {
    let header = split_row(header_line);
    let ncols = std::iter::once(header.len())
        .chain(body_lines.iter().map(|l| split_row(l).len()))
        .max()
        .unwrap_or(0);
    let table = TableData {
        header,
        aligns: parse_aligns(sep_line, ncols),
        body: body_lines.iter().map(|l| split_row(l)).collect(),
    };
    render_table_data(out, &table, color, rainbow)
}

/// A grid-table border like `+---+---+` or `+===+===+` (pandoc's default
/// markdown writer uses grid tables when cells contain block content).
fn is_grid_border(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('+') && t.len() > 1 && t.chars().all(|c| matches!(c, '+' | '-' | '=' | ':'))
}

/// Parse a pandoc grid table starting at a border line. Returns the table
/// and the index of the first line past it. Column boundaries are the `+`
/// positions; alignment colons sit just inside them. The first row block is
/// the header; wrapped cell lines within a block join with a space.
fn parse_grid_table(lines: &[&str], start: usize) -> Option<(TableData, usize)> {
    let border: Vec<char> = lines[start].chars().collect();
    let bounds: Vec<usize> = border
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| (c == '+').then_some(i))
        .collect();
    if bounds.len() < 2 {
        return None;
    }
    let aligns: Vec<Align> = bounds
        .windows(2)
        .map(|w| {
            let left_colon = border.get(w[0] + 1) == Some(&':');
            let right_colon = w[1] > 0 && border.get(w[1] - 1) == Some(&':');
            match (left_colon, right_colon) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            }
        })
        .collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = vec![String::new(); bounds.len() - 1];
    let mut have_current = false;
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if is_grid_border(line) {
            if have_current {
                rows.push(std::mem::take(&mut current));
                current = vec![String::new(); bounds.len() - 1];
                have_current = false;
            }
            i += 1;
            if i >= lines.len() || !lines[i].trim_start().starts_with('|') {
                break; // bottom border: table ends
            }
        } else if trimmed.starts_with('|') {
            let cells: Vec<char> = line.chars().collect();
            for (k, w) in bounds.windows(2).enumerate() {
                let from = (w[0] + 1).min(cells.len());
                let to = w[1].min(cells.len());
                let fragment: String = cells[from..to]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                if fragment.is_empty() {
                    continue;
                }
                if !current[k].is_empty() {
                    current[k].push(' ');
                }
                current[k].push_str(&fragment);
            }
            have_current = true;
            i += 1;
        } else {
            break; // unterminated; keep what we have
        }
    }
    if have_current {
        rows.push(current);
    }
    if rows.is_empty() {
        return None;
    }
    let header = rows.remove(0);
    Some((
        TableData {
            header,
            aligns,
            body: rows,
        },
        i,
    ))
}

/// A dash-only line like `  ----- -----` (pandoc simple/multiline table
/// part). Guarded against setext heading underlines and `---` rules: those
/// start at column 0 as one dash group, while pandoc tables are indented or
/// have multiple groups.
fn is_dash_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || !t.contains('-') {
        return false;
    }
    if !t.chars().all(|c| c == '-' || c == ' ') {
        return false;
    }
    line.starts_with(' ') || line.starts_with('\t') || t.contains(' ')
}

/// Dash runs of a dash line as `(start, end)` char indices (end exclusive);
/// they define the column spans of a simple/multiline table.
fn dash_groups(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut groups = Vec::new();
    let mut start = None;
    for (i, &c) in chars.iter().enumerate() {
        match (c == '-', start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                groups.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        groups.push((s, chars.len()));
    }
    groups
}

/// Slice one table row line by column spans; wrapped fragments (multiline
/// tables) join with a space.
fn dash_row_cells(lines: &[&str], cols: &[(usize, usize)]) -> Vec<String> {
    cols.iter()
        .map(|&(s, e)| {
            lines
                .iter()
                .map(|l| {
                    let chars: Vec<char> = l.chars().collect();
                    let from = s.min(chars.len());
                    let to = e.min(chars.len());
                    chars[from..to]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string()
                })
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Column alignment from the header text's position within its dash span
/// (pandoc's rule: flush right only → right, flush left only → left,
/// extends on both sides → center, flush both → left).
fn dash_aligns(header_lines: &[&str], cols: &[(usize, usize)]) -> Vec<Align> {
    let header: Vec<char> = header_lines.first().unwrap_or(&"").chars().collect();
    cols.iter()
        .map(|&(s, e)| {
            let end = e.min(header.len());
            let start = s.min(end);
            let mut first = None;
            let mut last = start;
            for (k, &c) in header.iter().enumerate().take(end).skip(start) {
                if !c.is_whitespace() {
                    first.get_or_insert(k);
                    last = k;
                }
            }
            match first {
                None => Align::Left,
                Some(f) => {
                    let flush_left = f == s;
                    let flush_right = last + 1 >= end;
                    match (flush_left, flush_right) {
                        (false, true) => Align::Right,
                        (false, false) => Align::Center,
                        _ => Align::Left,
                    }
                }
            }
        })
        .collect()
}

/// Parse a pandoc simple or multiline table starting at `start`.
///
/// Multiline: top dash border, header block, separator, blank-separated
/// body row groups, bottom border. Simple: header line, dash separator
/// (≥2 groups), one-line body rows until a blank line. Returns the table
/// and the index of the first line past it.
fn parse_dash_table(lines: &[&str], start: usize) -> Option<(TableData, usize)> {
    if is_dash_line(lines[start]) {
        // Multiline form.
        let cols = dash_groups(lines[start]);
        if cols.is_empty() {
            return None;
        }
        // Content blocks between dash borders; blanks separate row groups.
        let mut blocks: Vec<Vec<&str>> = Vec::new();
        let mut current: Vec<&str> = Vec::new();
        let mut j = start + 1;
        let closed = loop {
            if j >= lines.len() {
                break false;
            }
            let line = lines[j];
            if is_dash_line(line) {
                if !current.is_empty() {
                    blocks.push(std::mem::take(&mut current));
                }
                j += 1;
                // A border followed by content is an inner separator;
                // anything else makes it the bottom border.
                if j >= lines.len()
                    || lines[j].trim().is_empty()
                    || is_dash_line(lines[j])
                    || is_grid_border(lines[j])
                {
                    break true;
                }
            } else {
                current.push(line);
                j += 1;
            }
        };
        if !closed || blocks.is_empty() {
            return None;
        }
        let (header_lines, body_blocks): (Vec<&str>, &[Vec<&str>]) = if blocks.len() >= 2 {
            (blocks[0].clone(), &blocks[1..])
        } else {
            (Vec::new(), &blocks[..]) // headerless: single block is the body
        };
        let aligns = dash_aligns(&header_lines, &cols);
        let header = dash_row_cells(&header_lines, &cols);
        let mut body = Vec::new();
        for block in body_blocks {
            // pandoc separates wrapped multiline rows with blank lines;
            // compact tables have none, so each line is its own row.
            if block.iter().any(|l| l.trim().is_empty()) {
                for group in block.split(|l| l.trim().is_empty()) {
                    if !group.is_empty() {
                        body.push(dash_row_cells(group, &cols));
                    }
                }
            } else {
                for line in block.iter().filter(|l| !l.trim().is_empty()) {
                    body.push(dash_row_cells(&[line], &cols));
                }
            }
        }
        return Some((
            TableData {
                header,
                aligns,
                body,
            },
            j,
        ));
    }

    // Simple form: header line, then a dash separator with ≥2 groups.
    if lines[start].trim().is_empty() || start + 1 >= lines.len() || !is_dash_line(lines[start + 1])
    {
        return None;
    }
    let cols = dash_groups(lines[start + 1]);
    if cols.len() < 2 {
        return None;
    }
    let header = dash_row_cells(&[lines[start]], &cols);
    let aligns = dash_aligns(&[lines[start]], &cols);
    let mut body = Vec::new();
    let mut j = start + 2;
    while j < lines.len() {
        let line = lines[j];
        if line.trim().is_empty() || is_dash_line(line) {
            break;
        }
        body.push(dash_row_cells(&[line], &cols));
        j += 1;
    }
    Some((
        TableData {
            header,
            aligns,
            body,
        },
        j,
    ))
}

fn render_table_data<W: Write>(
    out: &mut W,
    table: &TableData,
    color: bool,
    rainbow: bool,
) -> io::Result<()> {
    let ncols = std::iter::once(table.header.len())
        .chain(table.body.iter().map(|r| r.len()))
        .max()
        .unwrap_or(0);
    if ncols == 0 {
        return Ok(());
    }
    let aligns = &table.aligns;

    // Column widths come from the *plain* text (markers stripped, no ANSI),
    // so styling never throws off the padding.
    let mut widths = vec![0usize; ncols];
    for row in std::iter::once(&table.header).chain(table.body.iter()) {
        for (j, cell) in row.iter().enumerate() {
            widths[j] = widths[j].max(render_inline(cell, false).chars().count());
        }
    }

    let render_row = |cells: &[String], plain: bool, header_row: bool| -> String {
        let mut line = String::from("|");
        for (j, width) in widths.iter().enumerate() {
            let cell = cells.get(j).map(String::as_str).unwrap_or("");
            let plain_len = render_inline(cell, false).chars().count();
            // Rainbow paints the whole (marker-stripped) cell in its column
            // color — inline ANSI inside the cell would reset it mid-cell.
            let rendered = if rainbow {
                let painted = render_inline(cell, false).color(super::csv::rainbow_color(j));
                let painted = if header_row { painted.bold() } else { painted };
                painted.to_string()
            } else if plain {
                render_inline(cell, false)
            } else {
                render_inline(cell, color)
            };
            let align = aligns.get(j).copied().unwrap_or(Align::Left);
            line.push(' ');
            line.push_str(&pad_cell(&rendered, plain_len, *width, align));
            line.push_str(" |");
        }
        line
    };

    let divider = |widths: &[usize]| -> String {
        let mut d = String::from("|");
        for width in widths {
            d.push_str(&"-".repeat(width + 2));
            d.push('|');
        }
        d
    };

    // Headerless tables (pandoc emits them) get a framing divider instead of
    // a header row.
    if table.header.iter().all(|c| c.is_empty()) {
        writeln!(out, "{}", divider(&widths))?;
    } else {
        // Header cells are rendered plain so the whole line can be bolded
        // without nested ANSI resets cancelling the style mid-row; in rainbow
        // mode each cell is bolded individually inside its column color instead.
        let header_out = render_row(&table.header, true, true);
        if rainbow {
            writeln!(out, "{}", header_out)?;
        } else {
            writeln!(out, "{}", style(&header_out, Style::Bold, color))?;
        }
        writeln!(out, "{}", divider(&widths))?;
    }

    for row in &table.body {
        writeln!(out, "{}", render_row(row, false, false))?;
    }
    if table.header.iter().all(|c| c.is_empty()) {
        writeln!(out, "{}", divider(&widths))?;
    }
    Ok(())
}

fn pad_cell(rendered: &str, plain_len: usize, width: usize, align: Align) -> String {
    let pad = width.saturating_sub(plain_len);
    match align {
        Align::Left => format!("{}{}", rendered, " ".repeat(pad)),
        Align::Right => format!("{}{}", " ".repeat(pad), rendered),
        Align::Center => {
            let left = pad / 2;
            format!("{}{}{}", " ".repeat(left), rendered, " ".repeat(pad - left))
        }
    }
}

/// Heading text without the leading `#`s, or `None` if not a heading.
fn parse_heading(line: &str) -> Option<String> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None; // "#tag" is not a heading
    }
    let text = rest.trim();
    // Strip optional closing hashes: "## Title ##"
    Some(text.trim_end_matches('#').trim_end().to_string())
}

/// Render inline Markdown: code spans, links, images, bold, italic.
/// With `color` the text is ANSI-styled; without it markers are stripped.
fn render_inline(text: &str, color: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        // Code span: `...`
        if chars[i] == '`'
            && let Some(j) = find_char(&chars, i + 1, '`')
        {
            let content: String = chars[i + 1..j].iter().collect();
            out.push_str(&style(&content, Style::Code, color));
            i = j + 1;
            continue;
        }

        // Inline image: ![alt](src) renders as its alt text + source.
        if chars[i] == '!'
            && chars.get(i + 1) == Some(&'[')
            && let Some((label, url, next)) = parse_link_at(&chars, i + 1)
        {
            out.push_str(&format_link(&label, &url, color));
            i = next;
            continue;
        }

        // Link: [text](url)
        if chars[i] == '['
            && let Some((label, url, next)) = parse_link_at(&chars, i)
        {
            out.push_str(&format_link(&label, &url, color));
            i = next;
            continue;
        }

        // Bold: **...** or __...__ (checked before single-marker italic).
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*')
            || chars[i] == '_' && chars.get(i + 1) == Some(&'_')
        {
            let marker = [chars[i], chars[i + 1]];
            if let Some(j) = find_seq(&chars, i + 2, &marker)
                && j > i + 2
            {
                let content: String = chars[i + 2..j].iter().collect();
                out.push_str(&style(&render_inline(&content, color), Style::Bold, color));
                i = j + 2;
                continue;
            }
        }

        // Italic: *...* or _..._. '_' must sit on a word boundary so
        // snake_case identifiers stay literal.
        if chars[i] == '*' || chars[i] == '_' {
            let opener_ok = chars[i] == '*' || i == 0 || !chars[i - 1].is_alphanumeric();
            if opener_ok && let Some(j) = find_char(&chars, i + 1, chars[i]) {
                let closer_ok =
                    chars[i] == '*' || j + 1 == chars.len() || !chars[j + 1].is_alphanumeric();
                if j > i + 1 && closer_ok {
                    let content: String = chars[i + 1..j].iter().collect();
                    out.push_str(&style(
                        &render_inline(&content, color),
                        Style::Italic,
                        color,
                    ));
                    i = j + 1;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Parse `[label](url)` with chars[i] == '['.
/// Returns (label, url, index just past the closing ')').
fn parse_link_at(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    let close = find_char(chars, i + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find_char(chars, close + 2, ')')?;
    let label: String = chars[i + 1..close].iter().collect();
    let url: String = chars[close + 2..end].iter().collect();
    Some((label, url, end + 1))
}

fn format_link(label: &str, url: &str, color: bool) -> String {
    let show_url = !url.is_empty() && url != label;
    let label = render_inline(label, color);
    let label = if color {
        label.underline().to_string()
    } else {
        label
    };
    if show_url {
        format!("{} ({})", label, url)
    } else {
        label
    }
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    (start..chars.len()).find(|&k| chars[k] == target)
}

fn find_seq(chars: &[char], start: usize, seq: &[char]) -> Option<usize> {
    if seq.is_empty() || chars.len() < seq.len() {
        return None;
    }
    (start..=chars.len() - seq.len()).find(|&k| chars[k..k + seq.len()] == *seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(text: &str, color: bool) -> String {
        capture_with(
            text,
            MarkdownOpts {
                color,
                img_rows: 4,
                img_cols: 6,
                ..MarkdownOpts::default()
            },
        )
    }

    fn capture_with(text: &str, opts: MarkdownOpts) -> String {
        let mut buf = Vec::new();
        write_markdown(&mut buf, text.as_bytes(), None, opts)
            .expect("write_markdown should not fail");
        String::from_utf8(buf).expect("output should be valid utf-8")
    }

    #[test]
    fn test_table_rainbow_columns() {
        let _guard = crate::COLOR_TEST_LOCK.lock().unwrap();
        colored::control::set_override(true);
        let md = "| Name | Age |\n| ---- | --- |\n| Al | 9 |\n";
        let mut buf = Vec::new();
        write_markdown(
            &mut buf,
            md.as_bytes(),
            None,
            MarkdownOpts {
                img_rows: 4,
                img_cols: 6,
                rainbow: true,
                ..MarkdownOpts::default()
            },
        )
        .expect("write_markdown should not fail");
        colored::control::unset_override();
        let out = String::from_utf8(buf).unwrap();
        // Column 0 cyan (36), column 1 yellow (33), in header and body.
        assert!(out.contains("\x1b[36m"), "cyan column: {out:?}");
        assert!(out.contains("\x1b[33m"), "yellow column: {out:?}");
        assert!(out.contains("Al"));
        // The divider row stays unpainted.
        assert!(
            out.lines()
                .any(|l| l.starts_with("|--") || l.starts_with("|-"))
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(capture("", false), "");
    }

    #[test]
    fn test_heading_markers_stripped() {
        let out = capture("# Title\n## Sub\n#tag is not a heading\n", false);
        assert!(out.contains("Title\n"));
        assert!(out.contains("Sub\n"));
        assert!(out.contains("#tag is not a heading\n"));
    }

    #[test]
    fn test_paragraph_passthrough() {
        assert_eq!(capture("hello world\n", false), "hello world\n");
    }

    #[test]
    fn test_code_fence_verbatim() {
        let out = capture("```\n# not a heading\n**raw**\n```\nafter\n", false);
        assert!(out.contains("# not a heading\n"));
        assert!(out.contains("**raw**\n"));
        assert!(out.ends_with("after\n"));
        assert!(!out.contains("```"));
    }

    #[test]
    fn test_inline_markers_stripped_without_color() {
        let out = capture(
            "a **bold** and *ital* and `code` and [lbl](http://x)\n",
            false,
        );
        assert!(out.contains("a bold and ital and code and lbl (http://x)\n"));
    }

    #[test]
    fn test_snake_case_not_italic() {
        let out = capture("use foo_bar_baz here\n", false);
        assert!(out.contains("foo_bar_baz"));
    }

    #[test]
    fn test_table_alignment_and_padding() {
        let md = "| Name | Age |\n| ---- | --: |\n| Al | 9 |\n| Bob | 10 |\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| Name | Age |");
        assert_eq!(lines[1], "|------|-----|");
        assert_eq!(lines[2], "| Al   |   9 |");
        assert_eq!(lines[3], "| Bob  |  10 |");
    }

    #[test]
    fn test_table_center_alignment() {
        let md = "| a |\n| :-: |\n| bb |\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| a  |");
        assert_eq!(lines[1], "|----|");
        assert_eq!(lines[2], "| bb |");
    }

    #[test]
    fn test_table_escaped_pipe() {
        let md = "| a \\| b |\n| --- |\n| c |\n";
        let out = capture(md, false);
        assert!(out.contains("a | b"));
    }

    #[test]
    fn test_table_requires_separator() {
        let out = capture("a | b\nnot a table\n", false);
        assert_eq!(out, "a | b\nnot a table\n");
    }

    #[test]
    fn test_table_strips_inline_markers_in_cells() {
        let md = "| h |\n| --- |\n| **b** |\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| h |");
        assert_eq!(lines[2], "| b |");
    }

    #[test]
    fn test_missing_image_placeholder() {
        let out = capture("![pic](nope.png)\n", false);
        assert!(out.contains("pic\n"));
        assert!(out.contains("[image not rendered: nope.png]"));
    }

    #[test]
    fn test_remote_image_not_rendered() {
        let out = capture("![alt](https://example.com/x.png)\n", false);
        assert!(out.contains("[remote image not rendered: https://example.com/x.png]"));
    }

    #[test]
    fn test_image_rendered_with_minimap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let img_path = dir.path().join("pic.png");
        let mut img = image::RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.save(&img_path).expect("save png");

        let mut buf = Vec::new();
        write_markdown(
            &mut buf,
            b"![cap](pic.png)\n",
            Some(dir.path()),
            MarkdownOpts {
                img_rows: 2,
                img_cols: 2,
                ..MarkdownOpts::default()
            },
        )
        .expect("write_markdown should not fail");
        let out = String::from_utf8(buf).expect("output should be valid utf-8");
        assert!(out.contains("cap\n"));
        assert!(out.contains("\x1b[38;5;"), "minimap ANSI missing: {out}");
        assert!(out.contains('█'));
    }

    #[test]
    fn test_image_rendered_with_graphics_protocol() {
        let dir = tempfile::tempdir().expect("tempdir");
        let img_path = dir.path().join("pic.png");
        let mut img = image::RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.save(&img_path).expect("save png");

        let mut buf = Vec::new();
        write_markdown(
            &mut buf,
            b"![cap](pic.png)\n",
            Some(dir.path()),
            MarkdownOpts {
                img_rows: 2,
                img_cols: 2,
                graphics: GraphicsOpts {
                    proto: crate::utils::caps::GraphicsProto::Kitty,
                    ..GraphicsOpts::default()
                },
                ..MarkdownOpts::default()
            },
        )
        .expect("write_markdown should not fail");
        let out = String::from_utf8(buf).expect("output should be valid utf-8");
        assert!(out.contains("cap\n"));
        assert!(
            out.contains("\x1b_Gq=2,a=T,f=100"),
            "kitty APC missing: {out:?}"
        );
        assert!(
            !out.contains('█'),
            "block minimap should not be used: {out:?}"
        );
    }

    fn capture_math(text: &str) -> String {
        capture_with(
            text,
            MarkdownOpts {
                graphics: GraphicsOpts {
                    math: true,
                    ..GraphicsOpts::default()
                },
                ..MarkdownOpts::default()
            },
        )
    }

    #[test]
    fn test_gfm_inline_math_dollar_backtick() {
        // pandoc `-t gfm` writes inline math as `$`...`$`.
        let out = capture_math("Euler: $`e^{i\\pi} + 1 = 0`$ and $`\\alpha`$ here\n");
        assert_eq!(out, "Euler: e^(iπ) + 1 = 0 and α here\n");
        // Without math enabled the spans render as ordinary code spans.
        let out = capture("Euler: $`e^{i\\pi}`$\n", false);
        assert!(out.contains("$e^{i\\pi}$"), "{out:?}");
    }

    #[test]
    fn test_gfm_math_fence_block() {
        // pandoc `-t gfm` writes display math as ``` math fenced blocks.
        let out = capture_math("before\n``` math\nE = mc^2\n```\nafter\n");
        assert_eq!(out, "before\nE = mc²\nafter\n");
        // Without math, the fence stays a verbatim code block.
        let out = capture("``` math\nE = mc^2\n```\n", false);
        assert!(out.contains("E = mc^2\n"), "{out:?}");
    }

    #[test]
    fn test_display_math_multiple_spans_one_line() {
        // pandoc joins consecutive display equations onto one line.
        let out = capture_math("$$E = mc^2$$ $$\\frac{1}{2}$$\n");
        assert_eq!(out, "E = mc²\n1/2\n");
    }

    #[test]
    fn test_pandoc_simple_table_with_caption() {
        // pandoc's default markdown: space-aligned columns, dash separator
        // under the header, `: caption` after a blank line.
        let md = "  Name     Age    Score\n  ------- ----- -------\n  Alice    30      91.5\n  Bob      25      88.0\n\n  : Scores\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| Name  | Age | Score |");
        assert_eq!(lines[1], "|-------|-----|-------|");
        // Age centers, Score right-aligns (header flush rules).
        assert_eq!(lines[2], "| Alice | 30  |  91.5 |");
        assert_eq!(lines[3], "| Bob   | 25  |  88.0 |");
        assert_eq!(lines[5], "Scores");
    }

    #[test]
    fn test_pandoc_simple_table_alignment() {
        // lcr tabular → header flush-left / centered / flush-right.
        let md = "  Name     Age    Score\n  ------- ----- -------\n  Alice    30      91.5\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[2], "| Alice | 30  |  91.5 |");
    }

    #[test]
    fn test_pandoc_multiline_table_bordered() {
        // Headerless multiline form: top/bottom dash borders, one line per
        // row, framed with dividers.
        let md = "  ----- -----------\n  Alice long text\n  Bob   short\n  ----- -----------\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "|-------|-----------|");
        assert_eq!(lines[1], "| Alice | long text |");
        assert_eq!(lines[2], "| Bob   | short     |");
        assert_eq!(lines[3], "|-------|-----------|");
    }

    #[test]
    fn test_pandoc_multiline_wrapped_rows() {
        // Wrapped rows are blank-line separated; continuation lines join
        // into their row's cell.
        let md = "  ----- -----------\n  Name  Description\n  ----- -----------\n  Alice first part\n        second part\n\n  Bob   short\n\n  ----- -----------\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| Name  | Description            |");
        assert!(
            out.contains("| Alice | first part second part |"),
            "{out:?}"
        );
        assert!(out.contains("| Bob   | short"), "{out:?}");
    }

    #[test]
    fn test_pandoc_grid_table() {
        let md = "+:-----+:--------+\n| Term | Details |\n+------+---------+\n| X    | - a     |\n|      |         |\n|      | - b     |\n+------+---------+\n";
        let out = capture(md, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "| Term | Details |");
        assert_eq!(lines[1], "|------|---------|");
        assert_eq!(lines[2], "| X    | - a - b |");
    }

    #[test]
    fn test_dash_line_guards_keep_setext_and_rules() {
        // Setext H2 and a horizontal rule are not tables.
        let out = capture("Title\n-----\n", false);
        assert_eq!(out, "Title\n-----\n");
        let out = capture("some text\n\n---\n", false);
        assert_eq!(out, "some text\n\n---\n");
    }

    #[test]
    fn test_math_disabled_stays_literal() {
        let out = capture("Euler: $e^{i\\pi} + 1 = 0$\n", false);
        assert!(out.contains("Euler: $e^{i\\pi} + 1 = 0$\n"), "{out:?}");
    }

    #[test]
    fn test_inline_math_unicode() {
        let out = capture_math("Euler: $e^{i\\pi} + 1 = 0$ and $\\alpha \\le \\beta$\n");
        assert!(out.contains("Euler: e^(iπ) + 1 = 0 and α ≤ β\n"), "{out:?}");
    }

    #[test]
    fn test_inline_math_currency_stays_literal() {
        let out = capture_math("it costs $5 and $10 today\n");
        assert!(out.contains("it costs $5 and $10 today\n"), "{out:?}");
    }

    #[test]
    fn test_inline_math_code_span_untouched() {
        let out = capture_math("literal `$x$` but real $x^2$\n");
        assert!(out.contains("literal $x$ but real x²\n"), "{out:?}");
    }

    #[test]
    fn test_display_math_block_unicode_without_protocol() {
        let out = capture_math("before\n$$\n\\frac{1}{2} + \\alpha\n$$\nafter\n");
        assert_eq!(out, "before\n1/2 + α\nafter\n");
    }

    #[test]
    fn test_display_math_single_line() {
        let out = capture_math("$$\\sum_{i=1}^{n} i$$\n");
        assert_eq!(out, "∑ᵢ₌₁ⁿ i\n");
    }

    #[test]
    fn test_color_output_contains_ansi() {
        // The `colored` crate auto-disables ANSI for non-TTY writers; override
        // it so this test sees escape sequences regardless of stdio capture.
        let _guard = crate::COLOR_TEST_LOCK.lock().unwrap();
        colored::control::set_override(true);
        let out = capture("# Title\n**b** and `c`\n", true);
        colored::control::unset_override();
        assert!(
            out.contains("\x1b["),
            "colored output should contain ANSI escape: {out}"
        );
    }
}
