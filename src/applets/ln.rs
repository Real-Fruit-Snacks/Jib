//! `ln` — create links between files.
//!
//! `-s` symbolic, `-r` relative target (implies `-s`), `-f` remove existing
//! destination first, `-T` treat the destination as never-a-directory,
//! `-v` print created links. On Windows, `-s` requires Developer Mode or
//! admin; on failure we report the OS error verbatim.

use std::path::{Path, PathBuf};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "ln",
    help: "create links between files",
    aliases: &[],
    main,
};

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path, _is_dir: bool) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path, is_dir: bool) -> std::io::Result<()> {
    if is_dir {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

#[cfg(not(any(unix, windows)))]
fn make_symlink(_t: &Path, _l: &Path, _d: bool) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks not supported on this platform",
    ))
}

fn relative_to(target: &Path, link_dir: &Path) -> PathBuf {
    let target = match target.canonicalize() {
        Ok(p) => p,
        Err(_) => target.to_path_buf(),
    };
    let link_dir = match link_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => link_dir.to_path_buf(),
    };
    let t: Vec<_> = target.components().collect();
    let l: Vec<_> = link_dir.components().collect();
    let mut i = 0;
    while i < t.len() && i < l.len() && t[i] == l[i] {
        i += 1;
    }
    let mut out = PathBuf::new();
    for _ in 0..(l.len() - i) {
        out.push("..");
    }
    for c in &t[i..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut symbolic = false;
    let mut force = false;
    let mut verbose = false;
    let mut relative = false;
    let mut no_target_dir = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if !a.starts_with('-') || a.len() < 2 || a == "-" {
            break;
        }
        if !a[1..]
            .chars()
            .all(|c| matches!(c, 's' | 'f' | 'v' | 'r' | 'T'))
        {
            err("ln", &format!("invalid option: {a}"));
            return 2;
        }
        for ch in a[1..].chars() {
            match ch {
                's' => symbolic = true,
                'f' => force = true,
                'v' => verbose = true,
                'r' => {
                    relative = true;
                    symbolic = true;
                }
                'T' => no_target_dir = true,
                _ => unreachable!(),
            }
        }
        i += 1;
    }

    let positional: Vec<String> = args[i..].to_vec();
    if positional.is_empty() {
        err("ln", "missing operand");
        return 2;
    }

    // (target, link_path) pairs.
    let mut pairs: Vec<(String, PathBuf)> = Vec::new();
    if positional.len() == 1 {
        let target = positional[0].clone();
        let basename = Path::new(&target)
            .file_name()
            .map(|s| s.to_os_string())
            .unwrap_or_else(|| target.clone().into());
        pairs.push((target, PathBuf::from(".").join(basename)));
    } else if positional.len() == 2 && (no_target_dir || !Path::new(&positional[1]).is_dir()) {
        pairs.push((positional[0].clone(), PathBuf::from(&positional[1])));
    } else {
        let dest = PathBuf::from(positional.last().unwrap());
        if !dest.is_dir() {
            err(
                "ln",
                &format!("target '{}' is not a directory", dest.display()),
            );
            return 1;
        }
        for t in &positional[..positional.len() - 1] {
            let base = Path::new(t)
                .file_name()
                .map(|s| s.to_os_string())
                .unwrap_or_else(|| t.clone().into());
            pairs.push((t.clone(), dest.join(base)));
        }
    }

    let mut rc = 0;
    for (target, link) in pairs {
        if link.exists() || link.is_symlink() {
            if force {
                if let Err(e) = std::fs::remove_file(&link) {
                    err_path("ln", &link.display().to_string(), &e);
                    rc = 1;
                    continue;
                }
            } else {
                err(
                    "ln",
                    &format!("failed to create link '{}': File exists", link.display()),
                );
                rc = 1;
                continue;
            }
        }
        let effective_target: PathBuf = if symbolic && relative {
            let abs = Path::new(&target)
                .canonicalize()
                .unwrap_or_else(|_| Path::new(&target).to_path_buf());
            let link_dir = link
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let link_dir_abs = link_dir.canonicalize().unwrap_or(link_dir);
            relative_to(&abs, &link_dir_abs)
        } else {
            PathBuf::from(&target)
        };
        let res = if symbolic {
            let is_dir_hint = Path::new(&target).is_dir();
            make_symlink(&effective_target, &link, is_dir_hint)
        } else {
            std::fs::hard_link(&target, &link)
        };
        match res {
            Ok(()) => {
                if verbose {
                    let arrow = if symbolic { " -> " } else { " => " };
                    println!(
                        "'{}'{arrow}'{}'",
                        link.display(),
                        effective_target.display()
                    );
                }
            }
            Err(e) => {
                err_path("ln", &link.display().to_string(), &e);
                rc = 1;
            }
        }
    }
    rc
}
