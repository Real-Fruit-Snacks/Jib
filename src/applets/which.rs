//! `which` — locate a command on `PATH`.
//!
//! On Windows, also tries each `PATHEXT` suffix. `-a` prints every match,
//! not just the first.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "which",
    help: "locate a command on PATH",
    aliases: &["where"],
    main,
};

#[cfg(windows)]
fn pathexts() -> Vec<String> {
    let mut out: Vec<String> = vec![String::new()];
    let pe = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.BAT;.CMD;.COM".to_string());
    for e in pe.split(';') {
        if !e.is_empty() {
            out.push(e.to_lowercase());
        }
    }
    out
}

#[cfg(not(windows))]
fn pathexts() -> Vec<String> {
    vec![String::new()]
}

fn is_executable(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = p.metadata() {
            return meta.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut all_matches = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'a' => all_matches = true,
                _ => {
                    err("which", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let names: Vec<String> = args[i..].to_vec();
    if names.is_empty() {
        err("which", "missing command name");
        return 2;
    }

    let path_env = std::env::var("PATH").unwrap_or_default();
    let path_dirs: Vec<PathBuf> = std::env::split_paths(&path_env).collect();
    let exts = pathexts();

    let mut rc = 0;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for name in &names {
        let mut found: Vec<PathBuf> = Vec::new();
        let has_sep = name.contains('/') || name.contains('\\');
        if has_sep {
            let p = PathBuf::from(name);
            if is_executable(&p) {
                found.push(p);
            }
        } else {
            'outer: for d in &path_dirs {
                if d.as_os_str().is_empty() {
                    continue;
                }
                for ext in &exts {
                    let cand = d.join(format!("{name}{ext}"));
                    if cand.is_file() {
                        found.push(cand);
                        if !all_matches {
                            break 'outer;
                        }
                    }
                }
            }
        }

        if found.is_empty() {
            rc = 1;
            continue;
        }
        for m in found {
            let _ = writeln!(out, "{}", m.display());
            if !all_matches {
                break;
            }
        }
    }
    rc
}
