//! Directory listing: `tree`-style output, plus an optional metadata block
//! in the spirit of `ls -lah` and `du -sh` when `--meta` is enabled.
//!
//! The directory is scanned exactly once into an in-memory snapshot (an
//! index-based arena, built and rendered iteratively so arbitrarily deep
//! trees cannot overflow the stack); the `ls`/`du` blocks and the tree are
//! all rendered from that snapshot, so they always agree on the directory's
//! contents.
//!
//! Entry names are colorized by type (directories blue/bold, symlinks cyan,
//! executables green) when `--color` is on; sizes in the metadata block are
//! yellow. Timestamps are UTC — the standard library has no timezone data.

use colored::{Color, Colorize};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// How an entry should be displayed (and colored).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryKind {
    Dir,
    Symlink,
    Executable,
    File,
}

fn classify(md: &fs::Metadata) -> EntryKind {
    if md.file_type().is_symlink() {
        EntryKind::Symlink
    } else if md.is_dir() {
        EntryKind::Dir
    } else if is_executable(md) {
        EntryKind::Executable
    } else {
        EntryKind::File
    }
}

#[cfg(unix)]
fn is_executable(md: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    md.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_md: &fs::Metadata) -> bool {
    false
}

/// Colorize an entry name by kind, mirroring `tree`/`ls` conventions.
fn paint(name: &str, kind: EntryKind, color: bool) -> String {
    if !color {
        return name.to_string();
    }
    match kind {
        EntryKind::Dir => name.color(Color::Blue).bold().to_string(),
        EntryKind::Symlink => name.color(Color::Cyan).to_string(),
        EntryKind::Executable => name.color(Color::Green).to_string(),
        EntryKind::File => name.to_string(),
    }
}

fn paint_size(size: &str, color: bool) -> String {
    if color {
        size.color(Color::Yellow).to_string()
    } else {
        size.to_string()
    }
}

/// Directory entries sorted by name, so output is deterministic.
fn sorted_entries(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

/// Human-readable size like `ls -h` / `du -h`: `804`, `4.0K`, `64K`, `1.2M`.
/// Values are ceiled the way coreutils does, and promoted to the next unit
/// when the ceiled value would reach 1024 (`1.0M`, never `1024K`).
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return bytes.to_string();
    }
    const UNITS: [char; 6] = ['K', 'M', 'G', 'T', 'P', 'E'];
    let mut value = bytes as f64;
    let mut unit = 0;
    while unit < UNITS.len() {
        value /= 1024.0;
        unit += 1;
        // Ceil to the precision we will print before deciding whether the
        // value still spills into the next unit.
        let shown = if value < 10.0 {
            (value * 10.0).ceil() / 10.0
        } else {
            value.ceil()
        };
        if shown < 1024.0 || unit == UNITS.len() {
            return if shown < 10.0 {
                format!("{:.1}{}", shown, UNITS[unit - 1])
            } else {
                format!("{}{}", shown as u64, UNITS[unit - 1])
            };
        }
    }
    unreachable!("u64 sizes always fit within the exbibyte unit")
}

/// `ls -l`-style mode string, e.g. `drwxr-xr-x`.
#[cfg(unix)]
fn format_mode(md: &fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    let mode = md.permissions().mode();
    let kind = if md.file_type().is_symlink() {
        'l'
    } else if md.is_dir() {
        'd'
    } else {
        '-'
    };
    let mut s = String::with_capacity(10);
    s.push(kind);
    for shift in [6u32, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

#[cfg(not(unix))]
fn format_mode(md: &fs::Metadata) -> String {
    let kind = if md.is_dir() { 'd' } else { '-' };
    let write = if md.permissions().readonly() {
        '-'
    } else {
        'w'
    };
    format!("{}r{}-", kind, write)
}

/// Gregorian date from days since the Unix epoch (Howard Hinnant's
/// `civil_from_days` algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `ls -l`-style modification time (UTC): `Sep  1 08:48` for recent files,
/// `Sep  1  2024` for files older than about six months.
fn format_mtime(t: io::Result<SystemTime>) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // ls switches from clock time to year at six months (in either direction).
    const SIX_MONTHS_SECS: i64 = 15_552_000;

    let secs = match t {
        Ok(ts) => match ts.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(_) => return "<pre-epoch>".to_string(),
        },
        Err(_) => return "<unavailable>".to_string(),
    };
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let month = MONTHS[(m - 1) as usize];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(secs);
    if (now - secs).abs() < SIX_MONTHS_SECS {
        let rem = secs.rem_euclid(86_400);
        format!(
            "{} {:>2} {:02}:{:02}",
            month,
            d,
            rem / 3600,
            (rem % 3600) / 60
        )
    } else {
        format!("{} {:>2}  {}", month, d, y)
    }
}

/// One scanned entry. Nodes live in [`Snapshot::nodes`] and refer to each
/// other by index; children always have a higher index than their parent.
struct Node {
    name: String,
    kind: EntryKind,
    /// `symlink_metadata` result; `None` when the entry could not be stat'ed
    /// (both the `ls` block and the tree render it as unavailable).
    md: Option<fs::Metadata>,
    /// Symlink target, for `name -> target` display.
    link_target: Option<String>,
    parent: usize,
    children: Vec<usize>,
    /// The directory's entries could not be read (`tree` shows
    /// `[error opening dir]` in place).
    read_error: bool,
    /// Recursive content size in bytes, `du`-style: file sizes summed through
    /// subdirectories; symlinks and unreadable entries count as 0.
    du: u64,
}

/// The whole directory scanned once; everything renders from this.
struct Snapshot {
    nodes: Vec<Node>,
}

/// Scan `root` breadth-agnostically with an explicit work stack (no
/// recursion, so depth is bounded by memory, not the call stack).
fn scan(root: &Path) -> Snapshot {
    let mut nodes = vec![Node {
        name: root.display().to_string(),
        kind: EntryKind::Dir,
        md: None,
        link_target: None,
        parent: 0,
        children: Vec::new(),
        read_error: false,
        du: 0,
    }];

    let mut stack = vec![(0usize, root.to_path_buf())];
    while let Some((idx, path)) = stack.pop() {
        let entries = match sorted_entries(&path) {
            Ok(entries) => entries,
            Err(_) => {
                nodes[idx].read_error = true;
                continue;
            }
        };
        for entry in entries {
            let md = entry.path().symlink_metadata().ok();
            let kind = md.as_ref().map_or(EntryKind::File, classify);
            let link_target = (kind == EntryKind::Symlink).then(|| {
                fs::read_link(entry.path())
                    .map(|t| t.display().to_string())
                    .unwrap_or_else(|_| "?".to_string())
            });
            // Symlinks contribute nothing and are not followed, so a link
            // cycle cannot make the scan loop.
            let du = match kind {
                EntryKind::Dir | EntryKind::Symlink => 0,
                _ => md.as_ref().map_or(0, |m| m.len()),
            };
            let child = nodes.len();
            nodes.push(Node {
                name: entry.file_name().to_string_lossy().into_owned(),
                kind,
                md,
                link_target,
                parent: idx,
                children: Vec::new(),
                read_error: false,
                du,
            });
            nodes[idx].children.push(child);
            if kind == EntryKind::Dir {
                stack.push((child, entry.path()));
            }
        }
    }

    // Children always have a higher index than their parent, so one reverse
    // pass accumulates every subtree size before it is added to its parent.
    for i in (1..nodes.len()).rev() {
        let (du, parent) = (nodes[i].du, nodes[i].parent);
        nodes[parent].du += du;
    }

    Snapshot { nodes }
}

/// Print a directory as a tree (and, with `meta`, an `ls -lah`/`du`-style
/// block first). Locks stdout so the listing is emitted as one atomic stream.
pub fn display_tree(path: &Path, color: bool, meta: bool) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_dir_listing(&mut out, path, color, meta)
}

/// Full directory output — the optional metadata block followed by the tree —
/// from a single scan. Used by [`display_tree`] and by the pager path in
/// `main`, so the composition lives in one place.
pub fn write_dir_listing<W: Write>(
    out: &mut W,
    path: &Path,
    color: bool,
    meta: bool,
) -> io::Result<()> {
    let snapshot = scan(path);
    if meta {
        render_meta(out, &snapshot, path, color)?;
    }
    render_tree(out, &snapshot, color)
}

/// The tree part alone. Exposed for testing.
pub fn write_tree<W: Write>(out: &mut W, path: &Path, color: bool) -> io::Result<()> {
    render_tree(out, &scan(path), color)
}

/// The metadata block alone. Exposed for testing.
pub fn write_dir_meta<W: Write>(out: &mut W, path: &Path, color: bool) -> io::Result<()> {
    render_meta(out, &scan(path), path, color)
}

/// Render the `tree`-style listing from a snapshot, iteratively (a work stack
/// of `(node, next child, prefix)` frames instead of recursion).
fn render_tree<W: Write>(out: &mut W, snapshot: &Snapshot, color: bool) -> io::Result<()> {
    let nodes = &snapshot.nodes;
    writeln!(out, "{}", paint(&nodes[0].name, EntryKind::Dir, color))?;

    let mut stack: Vec<(usize, usize, String)> = vec![(0, 0, String::new())];
    while let Some((idx, mut child_i, prefix)) = stack.pop() {
        let node = &nodes[idx];
        if node.read_error {
            // Same shape as `tree`: report the unreadable directory in place.
            writeln!(out, "{}└── [error opening dir]", prefix)?;
            continue;
        }
        while child_i < node.children.len() {
            let child = &nodes[node.children[child_i]];
            let is_last = child_i + 1 == node.children.len();
            let connector = if is_last { "└── " } else { "├── " };
            match child.kind {
                EntryKind::Symlink => {
                    let target = child.link_target.as_deref().unwrap_or("?");
                    writeln!(
                        out,
                        "{}{}{} -> {}",
                        prefix,
                        connector,
                        paint(&child.name, child.kind, color),
                        target
                    )?;
                }
                _ => {
                    writeln!(
                        out,
                        "{}{}{}",
                        prefix,
                        connector,
                        paint(&child.name, child.kind, color)
                    )?;
                }
            }
            if child.kind == EntryKind::Dir {
                let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                // Come back to the remaining siblings after the subtree.
                stack.push((idx, child_i + 1, prefix));
                stack.push((node.children[child_i], 0, child_prefix));
                break;
            }
            child_i += 1;
        }
    }

    let dirs = nodes
        .iter()
        .skip(1)
        .filter(|n| n.kind == EntryKind::Dir)
        .count();
    let files = nodes.len() - 1 - dirs;
    writeln!(out)?;
    writeln!(
        out,
        "{} {}, {} {}",
        dirs,
        if dirs == 1 {
            "directory"
        } else {
            "directories"
        },
        files,
        if files == 1 { "file" } else { "files" }
    )?;
    Ok(())
}

/// Render the metadata block from a snapshot: an `ls -lah`-style listing of
/// the immediate entries, then `du -sh *`-style recursive sizes and a total.
fn render_meta<W: Write>(
    out: &mut W,
    snapshot: &Snapshot,
    path: &Path,
    color: bool,
) -> io::Result<()> {
    let nodes = &snapshot.nodes;
    writeln!(out, "Directory: {}", path.display())?;

    // ls -lah: mode, human size, mtime, name (all entries, dotfiles
    // included; an entry that could not be stat'ed still gets a line, so
    // this block and the du block below always list the same entries).
    for &ci in &nodes[0].children {
        let child = &nodes[ci];
        let name = match (&child.link_target, child.kind) {
            (Some(target), EntryKind::Symlink) => {
                format!("{} -> {}", paint(&child.name, child.kind, color), target)
            }
            _ => paint(&child.name, child.kind, color),
        };
        match &child.md {
            // Pad the size before painting: ANSI escapes count toward `{:>6}`
            // and would break the column alignment otherwise.
            Some(md) => writeln!(
                out,
                "{} {} {} {}",
                format_mode(md),
                paint_size(&format!("{:>6}", human_size(md.len())), color),
                format_mtime(md.modified()),
                name
            )?,
            None => writeln!(out, "?????????? {:>6} <unavailable> {}", "-", name)?,
        }
    }

    // du -sh *: recursive size per entry, then the directory total.
    writeln!(out)?;
    for &ci in &nodes[0].children {
        let child = &nodes[ci];
        writeln!(
            out,
            "{}\t{}",
            paint_size(&format!("{:>6}", human_size(child.du)), color),
            child.name
        )?;
    }
    writeln!(
        out,
        "{}\ttotal",
        paint_size(&format!("{:>6}", human_size(nodes[0].du)), color)
    )?;
    writeln!(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn build_fixture() -> tempfile::TempDir {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("sub")).unwrap();
        let mut f = fs::File::create(dir.path().join("alpha.txt")).unwrap();
        f.write_all(b"hello").unwrap();
        let mut g = fs::File::create(dir.path().join("sub").join("beta.bin")).unwrap();
        g.write_all(&vec![0u8; 2048]).unwrap();
        dir
    }

    #[test]
    fn test_human_size() {
        assert_eq!(human_size(0), "0");
        assert_eq!(human_size(804), "804");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(10 * 1024), "10K");
        assert_eq!(human_size(1024 * 1024), "1.0M");
        // 1.15M must round up, like coreutils.
        assert_eq!(human_size(1_205_862), "1.2M");
        // A ceiled value that reaches 1024 promotes to the next unit…
        assert_eq!(human_size(1024 * 1024 - 1), "1.0M");
        // …and the top unit is reachable.
        assert_eq!(human_size(1 << 60), "1.0E");
        assert_eq!(human_size(u64::MAX), "16E");
    }

    #[test]
    fn test_civil_from_days_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn test_write_tree_structure_and_counts() {
        let dir = build_fixture();
        let mut buf = Vec::new();
        write_tree(&mut buf, dir.path(), false).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("├── alpha.txt") || out.contains("└── alpha.txt"));
        assert!(out.contains("sub"));
        assert!(out.contains("│   └── beta.bin") || out.contains("    └── beta.bin"));
        assert!(out.contains("1 directory, 2 files"));
        // No ANSI escapes without --color.
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn test_write_tree_color_contains_ansi() {
        // The `colored` crate auto-disables ANSI for non-TTY writers; override it
        // so this test sees escape sequences regardless of how cargo captures stdio.
        let _guard = crate::COLOR_TEST_LOCK.lock().unwrap();
        colored::control::set_override(true);
        let dir = build_fixture();
        let mut buf = Vec::new();
        write_tree(&mut buf, dir.path(), true).unwrap();
        colored::control::unset_override();
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains("\x1b["),
            "colored output should contain ANSI escape: {out}"
        );
    }

    #[test]
    fn test_write_dir_meta_lists_and_totals() {
        let dir = build_fixture();
        let mut buf = Vec::new();
        write_dir_meta(&mut buf, dir.path(), false).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("Directory:"));
        // ls-style line for the subdirectory starts with 'd'.
        assert!(out.lines().any(|l| l.starts_with('d') && l.contains("sub")));
        // du block: recursive size of `sub` (2048 bytes -> 2.0K) and a total.
        assert!(out.contains("2.0K\tsub"));
        assert!(out.contains("\ttotal"));
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn test_scan_du_recursive_and_deep_tree() {
        let dir = build_fixture();
        // Recursive sizes accumulate bottom-up in the snapshot.
        let snapshot = scan(dir.path());
        assert_eq!(snapshot.nodes[0].du, 2048 + 5);
        let sub = snapshot.nodes[0]
            .children
            .iter()
            .map(|&ci| &snapshot.nodes[ci])
            .find(|n| n.name == "sub")
            .expect("sub should be scanned");
        assert_eq!(sub.du, 2048);

        // A deeply nested chain renders fine (scan and render are both
        // iterative, so depth is bounded by memory, not the call stack).
        // PATH_MAX caps how deep a test fixture can practically go: it is
        // 1024 bytes on macOS (vs 4096 on Linux), so with 2 bytes per level
        // plus the tempdir prefix, 300 levels is the safe portable depth.
        let deep = tempdir().expect("tempdir");
        let mut p = deep.path().to_path_buf();
        for _ in 0..300 {
            p.push("d");
            fs::create_dir(&p).unwrap();
        }
        let mut buf = Vec::new();
        write_dir_listing(&mut buf, deep.path(), false, true).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("300 directories, 0 files"));
    }

    #[test]
    fn test_format_mtime_recent_and_old() {
        let now = SystemTime::now();
        let recent = format_mtime(Ok(now));
        // Recent timestamps show HH:MM, not a year.
        assert!(recent.contains(':'), "expected clock time: {recent}");
        let old = format_mtime(Ok(UNIX_EPOCH));
        assert!(old.contains("1970"), "expected year: {old}");
    }
}
