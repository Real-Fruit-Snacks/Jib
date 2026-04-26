//! `mktemp` — create a unique temporary file or directory.
//!
//! Tries random suffixes (matching Python's stdlib heuristic of 8 hex
//! chars) until a non-existing path is found. Mirrors the GNU/BusyBox
//! flag set: `-d` directory, `-u` dry-run, `-q` silent, `-t` use TMPDIR,
//! `-p DIR` / `--tmpdir[=DIR]` explicit dir.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "mktemp",
    help: "create a unique temporary file or directory",
    aliases: &[],
    main,
};

fn split_template(tmpl: &str) -> (PathBuf, String, String, usize) {
    let p = std::path::Path::new(tmpl);
    let parent = p.parent().map(|q| q.to_path_buf()).unwrap_or_default();
    let base = p.file_name().and_then(|s| s.to_str()).unwrap_or(tmpl).to_string();
    // Find longest run of trailing X's anywhere in basename.
    let bytes = base.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] != b'X' {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && bytes[start - 1] == b'X' {
        start -= 1;
    }
    let n = end - start;
    let prefix = base[..start].to_string();
    let suffix = base[end..].to_string();
    (parent, prefix, suffix, n)
}

fn rand_chars(n: usize) -> String {
    // Cheap pseudo-random based on system time + addr. Sufficient since
    // we re-roll on collision and the OS provides actual atomicity for
    // create-exclusive open / mkdir.
    let mut t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    t ^= &t as *const _ as u64;
    let mut s = String::with_capacity(n);
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    for _ in 0..n {
        // xorshift64
        t ^= t << 13;
        t ^= t >> 7;
        t ^= t << 17;
        let idx = (t % alphabet.len() as u64) as usize;
        s.push(alphabet[idx] as char);
    }
    s
}

fn try_create_file(path: &std::path::Path) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .truncate(true)
        .open(path)
        .map(|_| ())
}

fn try_create_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut make_dir = false;
    let mut dry_run = false;
    let mut quiet = false;
    let mut use_tmpdir = false;
    let mut explicit_dir: Option<String> = None;
    let mut template: Option<String> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-d" | "--directory" => {
                make_dir = true;
                i += 1;
            }
            "-u" | "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "-q" | "--quiet" => {
                quiet = true;
                i += 1;
            }
            "-t" => {
                use_tmpdir = true;
                i += 1;
            }
            "-p" if i + 1 < args.len() => {
                explicit_dir = Some(args[i + 1].clone());
                i += 2;
            }
            "--tmpdir" => {
                explicit_dir = Some(std::env::temp_dir().display().to_string());
                i += 1;
            }
            s if s.starts_with("--tmpdir=") => {
                explicit_dir = Some(s["--tmpdir=".len()..].to_string());
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                if quiet {
                    return 1;
                }
                err("mktemp", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                if template.is_none() {
                    template = Some(args[i].clone());
                    i += 1;
                } else {
                    err("mktemp", "too many arguments");
                    return 2;
                }
            }
        }
    }

    let tmpl = template.unwrap_or_else(|| "tmp.XXXXXXXXXX".to_string());
    let (parent, prefix, suffix, n) = split_template(&tmpl);
    if n < 3 {
        if !quiet {
            err("mktemp", &format!("too few X's in template '{tmpl}'"));
        }
        return 1;
    }

    let target_dir: Option<PathBuf> = if use_tmpdir {
        Some(
            explicit_dir
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir),
        )
    } else if let Some(d) = &explicit_dir {
        Some(PathBuf::from(d))
    } else if !parent.as_os_str().is_empty() {
        None
    } else {
        None
    };

    for _attempt in 0..1000 {
        let mut name = String::new();
        name.push_str(&prefix);
        name.push_str(&rand_chars(n));
        name.push_str(&suffix);
        let path: PathBuf = match &target_dir {
            Some(d) => d.join(&name),
            None => parent.join(&name),
        };
        let res = if make_dir {
            try_create_dir(&path)
        } else {
            try_create_file(&path)
        };
        match res {
            Ok(()) => {
                if dry_run {
                    let _ = if make_dir {
                        std::fs::remove_dir(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                }
                println!("{}", path.display());
                return 0;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                continue; // collision; re-roll
            }
            Err(e) => {
                if !quiet {
                    err("mktemp", &e.to_string());
                }
                return 1;
            }
        }
    }
    if !quiet {
        err("mktemp", "could not generate a unique name");
    }
    1
}
