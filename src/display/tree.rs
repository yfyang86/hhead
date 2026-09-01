//! Directory listing: `tree`-style output, plus an optional metadata block
//! in the spirit of `ls -lah` and `du -sh` when `--meta` is enabled.
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
/// Values are ceiled the way coreutils does.
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return bytes.to_string();
    }
    const UNITS: [char; 6] = ['K', 'M', 'G', 'T', 'P', 'E'];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let tenths = (value * 10.0).ceil() / 10.0;
    if tenths < 10.0 {
        format!("{:.1}{}", tenths, UNITS[unit - 1])
    } else {
        format!("{}{}", value.ceil() as u64, UNITS[unit - 1])
    }
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

/// Recursive size of a path in bytes, `du`-style: file sizes summed through
/// subdirectories; symlinks are not followed; unreadable entries count as 0.
fn du_size(path: &Path) -> u64 {
    let Ok(md) = path.symlink_metadata() else {
        return 0;
    };
    if md.file_type().is_symlink() {
        return 0;
    }
    if !md.is_dir() {
        return md.len();
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().map(|entry| du_size(&entry.path())).sum()
}

#[derive(Default)]
struct Counts {
    dirs: usize,
    files: usize,
}

/// Print a directory as a tree (and, with `meta`, an `ls -lah`/`du`-style
/// block first). Locks stdout so the listing is emitted as one atomic stream.
pub fn display_tree(path: &Path, color: bool, meta: bool) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if meta {
        write_dir_meta(&mut out, path, color)?;
    }
    write_tree(&mut out, path, color)
}

/// Same as the tree part of [`display_tree`] but writes to an arbitrary
/// [`Write`]. Exposed for testing and so the pager can capture the listing.
pub fn write_tree<W: Write>(out: &mut W, path: &Path, color: bool) -> io::Result<()> {
    writeln!(
        out,
        "{}",
        paint(&path.display().to_string(), EntryKind::Dir, color)
    )?;
    let mut counts = Counts::default();
    walk(out, path, "", color, &mut counts)?;
    writeln!(out)?;
    writeln!(out, "{} directories, {} files", counts.dirs, counts.files)?;
    Ok(())
}

fn walk<W: Write>(
    out: &mut W,
    dir: &Path,
    prefix: &str,
    color: bool,
    counts: &mut Counts,
) -> io::Result<()> {
    let entries = match sorted_entries(dir) {
        Ok(entries) => entries,
        Err(_) => {
            // Same shape as `tree`: report the unreadable directory in place.
            return writeln!(out, "{}└── [error opening dir]", prefix);
        }
    };
    let last = entries.len().saturating_sub(1);
    for (i, entry) in entries.iter().enumerate() {
        let connector = if i == last {
            "└── "
        } else {
            "├── "
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(md) = entry.path().symlink_metadata() else {
            counts.files += 1;
            writeln!(out, "{}{}{}", prefix, connector, name)?;
            continue;
        };
        let kind = classify(&md);
        match kind {
            EntryKind::Symlink => {
                counts.files += 1;
                let target = fs::read_link(entry.path())
                    .map(|t| t.display().to_string())
                    .unwrap_or_else(|_| "?".to_string());
                writeln!(
                    out,
                    "{}{}{} -> {}",
                    prefix,
                    connector,
                    paint(&name, kind, color),
                    target
                )?;
            }
            EntryKind::Dir => {
                counts.dirs += 1;
                writeln!(out, "{}{}{}", prefix, connector, paint(&name, kind, color))?;
                let child_prefix = format!("{}{}", prefix, if i == last { "    " } else { "│   " });
                walk(out, &entry.path(), &child_prefix, color, counts)?;
            }
            EntryKind::Executable | EntryKind::File => {
                counts.files += 1;
                writeln!(out, "{}{}{}", prefix, connector, paint(&name, kind, color))?;
            }
        }
    }
    Ok(())
}

/// Metadata block for a directory: an `ls -lah`-style listing of its
/// immediate entries, then `du -sh *`-style recursive sizes and a total.
/// Exposed for testing and so the pager can capture the block.
pub fn write_dir_meta<W: Write>(out: &mut W, path: &Path, color: bool) -> io::Result<()> {
    writeln!(out, "Directory: {}", path.display())?;

    let entries = sorted_entries(path)?;

    // ls -lah: mode, human size, mtime, name (all entries, dotfiles included).
    for entry in &entries {
        let Ok(md) = entry.path().symlink_metadata() else {
            continue;
        };
        let kind = classify(&md);
        let name = entry.file_name().to_string_lossy().into_owned();
        let name = if kind == EntryKind::Symlink {
            let target = fs::read_link(entry.path())
                .map(|t| t.display().to_string())
                .unwrap_or_else(|_| "?".to_string());
            format!("{} -> {}", paint(&name, kind, color), target)
        } else {
            paint(&name, kind, color)
        };
        // Pad the size before painting: ANSI escapes count toward `{:>6}`
        // and would break the column alignment otherwise.
        writeln!(
            out,
            "{} {} {} {}",
            format_mode(&md),
            paint_size(&format!("{:>6}", human_size(md.len())), color),
            format_mtime(md.modified()),
            name
        )?;
    }

    // du -sh *: recursive size per entry, then the directory total.
    writeln!(out)?;
    let mut total: u64 = 0;
    for entry in &entries {
        let size = du_size(&entry.path());
        total += size;
        writeln!(
            out,
            "{}\t{}",
            paint_size(&format!("{:>6}", human_size(size)), color),
            entry.file_name().to_string_lossy()
        )?;
    }
    writeln!(
        out,
        "{}\ttotal",
        paint_size(&format!("{:>6}", human_size(total)), color)
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
        assert!(out.contains("1 directories, 2 files"));
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
    fn test_du_size_recursive() {
        let dir = build_fixture();
        assert_eq!(du_size(&dir.path().join("sub")), 2048);
        assert_eq!(du_size(dir.path()), 2048 + 5);
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
