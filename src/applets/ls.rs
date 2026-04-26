//! `ls` — list directory contents.
//!
//! Flags: `-l` long format, `-a`/`-A` show dotfiles, `-1` one per line,
//! `-R` recursive, `-F` classify, `-S` sort by size, `-t` sort by mtime,
//! `-r` reverse. Column formatting is enabled when stdout is a TTY and
//! neither `-l` nor `-1` was given.

use std::fs::Metadata;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{err, err_path, filemode, group_name, nlink, uid_gid, unix_mode, user_name};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "ls",
    help: "list directory contents",
    aliases: &["dir"],
    main,
};

#[derive(Default, Clone, Copy)]
struct Flags {
    long_fmt: bool,
    all: bool,
    almost_all: bool,
    one_per_line: bool,
    recursive: bool,
    classify: bool,
    sort_size: bool,
    sort_time: bool,
    reverse: bool,
}

fn classify_suffix(meta: &Metadata, mode: u32) -> &'static str {
    if meta.is_dir() {
        "/"
    } else if meta.file_type().is_symlink() {
        "@"
    } else if mode & 0o170_000 == 0o010_000 {
        "|"
    } else if mode & 0o170_000 == 0o140_000 {
        "="
    } else if meta.is_file() && (mode & 0o111) != 0 {
        "*"
    } else {
        ""
    }
}

/// Date/time formatter for `ls -l`. Matches Python's behavior of using
/// `Mon DD  YYYY` for entries older than 6 months and `Mon DD HH:MM`
/// otherwise.
fn format_time(t: SystemTime) -> String {
    let secs_since_epoch = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day, hour, minute) = ymd_hm_local(secs_since_epoch);
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mname = months.get(month as usize - 1).copied().unwrap_or("Jan");
    if (now - secs_since_epoch).abs() > 180 * 86_400 {
        format!("{mname} {day:>2}  {year:04}")
    } else {
        format!("{mname} {day:>2} {hour:02}:{minute:02}")
    }
}

/// UTC year/month/day/hour/minute (Hinnant). We don't try to compute local
/// time without a TZ database; matches Python's `fromtimestamp` only when
/// the user is in UTC, which is the closest cross-platform default.
fn ymd_hm_local(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    if m <= 2 {
        y += 1;
    }
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    (y, m, d, hour, minute)
}

fn format_long(name: &str, meta: &Metadata, path: &Path) -> String {
    let mode = unix_mode(meta, path);
    let (uid, gid) = uid_gid(meta);
    let fm = filemode(mode);
    let n = nlink(meta);
    let usr = user_name(uid);
    let grp = group_name(gid);
    let size = meta.len();
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let ts = format_time(mtime);
    format!("{fm} {n:>2} {usr} {grp} {size:>8} {ts} {name}")
}

fn format_columns(names: &[String], term_width: usize) -> Vec<String> {
    if names.is_empty() {
        return Vec::new();
    }
    let max_w = names.iter().map(|s| s.len()).max().unwrap_or(0) + 2;
    let cols = (term_width / max_w).max(1);
    let rows = names.len().div_ceil(cols);
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut line = String::new();
        for c in 0..cols {
            let idx = c * rows + r;
            if idx < names.len() {
                line.push_str(&format!("{:<max_w$}", names[idx]));
            }
        }
        out.push(line.trim_end().to_string());
    }
    out
}

fn term_width() -> usize {
    if let Ok(s) = std::env::var("COLUMNS") {
        if let Ok(n) = s.parse::<usize>() {
            return n;
        }
    }
    80
}

fn is_tty_stdout() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        unsafe { libc_isatty(std::io::stdout().as_raw_fd()) }
    }
    #[cfg(windows)]
    {
        // Best-effort: check if FILE_TYPE_CHAR by querying the handle.
        // Without winapi binding we can't easily; fall back to env hint.
        std::env::var("TERM").is_ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(unix)]
unsafe fn libc_isatty(fd: i32) -> bool {
    extern "C" {
        fn isatty(fd: i32) -> i32;
    }
    isatty(fd) != 0
}

fn list_one(
    root: &Path,
    flags: &Flags,
    out: &mut impl Write,
    rc: &mut i32,
    show_header: bool,
    use_cols: bool,
    tw: usize,
) {
    if show_header {
        let _ = writeln!(out, "{}:", root.display());
    }
    let st = match root.symlink_metadata() {
        Ok(m) => m,
        Err(e) => {
            err_path("ls", &root.display().to_string(), &e);
            *rc = 1;
            return;
        }
    };

    if !st.is_dir() {
        let name_os = root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
        let suffix = if flags.classify {
            classify_suffix(&st, unix_mode(&st, root))
        } else {
            ""
        };
        if flags.long_fmt {
            let _ = writeln!(out, "{}", format_long(&format!("{name_os}{suffix}"), &st, root));
        } else {
            let _ = writeln!(out, "{name_os}{suffix}");
        }
        return;
    }

    let raw = match std::fs::read_dir(root) {
        Ok(it) => it,
        Err(e) => {
            err_path("ls", &root.display().to_string(), &e);
            *rc = 1;
            return;
        }
    };
    let mut names: Vec<String> = raw
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    if !(flags.all || flags.almost_all) {
        names.retain(|n| !n.starts_with('.'));
    }

    let mut entries: Vec<(String, PathBuf, Option<Metadata>)> = Vec::new();
    if flags.all {
        entries.push((".".to_string(), root.to_path_buf(), None));
        let parent = root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| root.to_path_buf());
        entries.push(("..".to_string(), parent, None));
    }
    for n in names {
        let p = root.join(&n);
        entries.push((n, p, None));
    }

    let need_stat = flags.long_fmt || flags.classify || flags.sort_size || flags.sort_time;
    if need_stat {
        for ent in entries.iter_mut() {
            ent.2 = ent.1.symlink_metadata().ok();
        }
    }

    if flags.sort_size {
        entries.sort_by_key(|e| std::cmp::Reverse(e.2.as_ref().map(|m| m.len()).unwrap_or(0)));
    } else if flags.sort_time {
        entries.sort_by_key(|e| {
            std::cmp::Reverse(
                e.2.as_ref()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH),
            )
        });
    }
    if flags.reverse {
        entries.reverse();
    }

    let display_names: Vec<String> = entries
        .iter()
        .map(|(n, p, m)| {
            if flags.classify {
                if let Some(meta) = m {
                    return format!("{n}{}", classify_suffix(meta, unix_mode(meta, p)));
                }
            }
            n.clone()
        })
        .collect();

    if flags.long_fmt {
        for ((_, p, m), display) in entries.iter().zip(display_names.iter()) {
            if let Some(meta) = m {
                let _ = writeln!(out, "{}", format_long(display, meta, p));
            }
        }
    } else if use_cols && !flags.one_per_line {
        for line in format_columns(&display_names, tw) {
            let _ = writeln!(out, "{line}");
        }
    } else {
        for n in &display_names {
            let _ = writeln!(out, "{n}");
        }
    }

    if flags.recursive {
        for (n, p, m) in &entries {
            if n == "." || n == ".." {
                continue;
            }
            let dir_now = match m.as_ref() {
                Some(meta) => meta.is_dir() && !meta.file_type().is_symlink(),
                None => p.is_dir() && !p.is_symlink(),
            };
            if dir_now {
                let _ = writeln!(out);
                list_one(p, flags, out, rc, true, use_cols, tw);
            }
        }
    }
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut flags = Flags::default();
    let mut paths: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            paths.extend_from_slice(&args[i + 1..]);
            break;
        }
        if a == "-" || !a.starts_with('-') || a.len() < 2 {
            paths.push(a.clone());
        } else {
            for ch in a[1..].chars() {
                match ch {
                    'l' => flags.long_fmt = true,
                    'a' => flags.all = true,
                    'A' => flags.almost_all = true,
                    '1' => flags.one_per_line = true,
                    'R' => flags.recursive = true,
                    'F' => flags.classify = true,
                    'S' => flags.sort_size = true,
                    't' => flags.sort_time = true,
                    'r' => flags.reverse = true,
                    _ => {
                        err("ls", &format!("invalid option: -{ch}"));
                        return 2;
                    }
                }
            }
        }
        i += 1;
    }
    if paths.is_empty() {
        paths.push(".".to_string());
    }

    let multi = paths.len() > 1 || flags.recursive;
    let use_cols = !flags.long_fmt && !flags.one_per_line && is_tty_stdout();
    let tw = term_width();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;

    for (idx, p) in paths.iter().enumerate() {
        if multi && idx > 0 {
            let _ = writeln!(out);
        }
        list_one(Path::new(p), &flags, &mut out, &mut rc, multi, use_cols, tw);
    }
    rc
}
