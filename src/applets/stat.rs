//! `stat` — display file or filesystem status.
//!
//! `-c FMT` / `--format=FMT` prints a custom format. `-t`/`--terse` prints
//! a single space-separated line. `-L`/`--dereference` follows symlinks.

use std::fs::Metadata;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{
    err, err_path, filemode, group_name, inode, nlink, uid_gid, unix_mode, user_name,
};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "stat",
    help: "display file or file system status",
    aliases: &[],
    main,
};

fn type_string(mode: u32) -> &'static str {
    match mode & 0o170_000 {
        0o100_000 => "regular file",
        0o040_000 => "directory",
        0o120_000 => "symbolic link",
        0o020_000 => "character special file",
        0o060_000 => "block special file",
        0o010_000 => "fifo",
        0o140_000 => "socket",
        _ => "unknown",
    }
}

fn epoch_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn fmt_time(t: SystemTime) -> String {
    let s = epoch_secs(t);
    let (y, mo, d, hh, mm) = ymd_hm(s);
    let ss = (s.rem_euclid(60)) as u32;
    format!("{y:04}-{mo:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

fn ymd_hm(secs: i64) -> (i64, u32, u32, u32, u32) {
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
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32)
}

fn apply_format(p: &Path, st: &Metadata, fmt: &str) -> String {
    let mode = unix_mode(st, p);
    let (uid, gid) = uid_gid(st);
    let mtime = st.modified().unwrap_or(UNIX_EPOCH);
    let atime = st.accessed().unwrap_or(UNIX_EPOCH);
    let ctime = st.created().unwrap_or(mtime);
    let mut out = String::with_capacity(fmt.len());
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '%' && i + 1 < bytes.len() {
            match bytes[i + 1] as char {
                'n' => out.push_str(&p.display().to_string()),
                's' => out.push_str(&st.len().to_string()),
                'a' => out.push_str(&format!("{:o}", mode & 0o7777)),
                'A' => out.push_str(&filemode(mode)),
                'u' => out.push_str(&uid.to_string()),
                'U' => out.push_str(&user_name(uid)),
                'g' => out.push_str(&gid.to_string()),
                'G' => out.push_str(&group_name(gid)),
                'F' => out.push_str(type_string(mode)),
                'Y' => out.push_str(&epoch_secs(mtime).to_string()),
                'X' => out.push_str(&epoch_secs(atime).to_string()),
                'Z' => out.push_str(&epoch_secs(ctime).to_string()),
                'y' => out.push_str(&fmt_time(mtime)),
                'x' => out.push_str(&fmt_time(atime)),
                'z' => out.push_str(&fmt_time(ctime)),
                'h' => out.push_str(&nlink(st).to_string()),
                'i' => out.push_str(&inode(st).to_string()),
                '%' => out.push('%'),
                other => {
                    out.push('%');
                    out.push(other);
                }
            }
            i += 2;
            continue;
        }
        if c == '\\' && i + 1 < bytes.len() {
            match bytes[i + 1] as char {
                'n' => {
                    out.push('\n');
                    i += 2;
                    continue;
                }
                't' => {
                    out.push('\t');
                    i += 2;
                    continue;
                }
                'r' => {
                    out.push('\r');
                    i += 2;
                    continue;
                }
                '\\' => {
                    out.push('\\');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn default_output(p: &Path, st: &Metadata) -> String {
    let mode = unix_mode(st, p);
    let (uid, gid) = uid_gid(st);
    let mtime = st.modified().unwrap_or(UNIX_EPOCH);
    let atime = st.accessed().unwrap_or(UNIX_EPOCH);
    let ctime = st.created().unwrap_or(mtime);
    format!(
        "  File: {}\n  Size: {:<12}  Type: {}\n  Mode: ({:04o}/{})  Uid: ({:>4}/{})  Gid: ({:>4}/{})\nAccess: {}\nModify: {}\nChange: {}",
        p.display(),
        st.len(),
        type_string(mode),
        mode & 0o7777,
        filemode(mode),
        uid,
        user_name(uid),
        gid,
        group_name(gid),
        fmt_time(atime),
        fmt_time(mtime),
        fmt_time(ctime),
    )
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut fmt: Option<String> = None;
    let mut terse = false;
    let mut deref = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a == "-c" && i + 1 < args.len() {
            fmt = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if let Some(rest) = a.strip_prefix("--format=") {
            fmt = Some(rest.to_string());
            i += 1;
            continue;
        }
        if a == "-t" || a == "--terse" {
            terse = true;
            i += 1;
            continue;
        }
        if a == "-L" || a == "--dereference" {
            deref = true;
            i += 1;
            continue;
        }
        if a.starts_with('-') && a.len() > 1 && a != "-" {
            err("stat", &format!("invalid option: {a}"));
            return 2;
        }
        break;
    }

    let paths = &args[i..];
    if paths.is_empty() {
        err("stat", "missing operand");
        return 2;
    }

    let mut rc = 0;
    for path in paths {
        let p = Path::new(path);
        let st = if deref {
            std::fs::metadata(p)
        } else {
            std::fs::symlink_metadata(p)
        };
        let st = match st {
            Ok(m) => m,
            Err(e) => {
                err_path("stat", path, &e);
                rc = 1;
                continue;
            }
        };
        if let Some(f) = &fmt {
            println!("{}", apply_format(p, &st, f));
        } else if terse {
            let mode = unix_mode(&st, p);
            let (uid, gid) = uid_gid(&st);
            let mtime = st.modified().unwrap_or(UNIX_EPOCH);
            let atime = st.accessed().unwrap_or(UNIX_EPOCH);
            let ctime = st.created().unwrap_or(mtime);
            println!(
                "{} {} {} {:o} {} {} {} {} {}",
                p.display(),
                st.len(),
                nlink(&st),
                mode,
                uid,
                gid,
                epoch_secs(mtime),
                epoch_secs(atime),
                epoch_secs(ctime)
            );
        } else {
            println!("{}", default_output(p, &st));
        }
    }
    rc
}
