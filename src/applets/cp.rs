//! `cp` — copy files and directories.
//!
//! `-r`/`-R` recurse, `-p` preserve mtime, `-a` archive (≡ `-r -p`), and
//! the `-f`/`-i`/`-n`/`-u` overwrite-policy quartet.

use std::path::{Path, PathBuf};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "cp",
    help: "copy files and directories",
    aliases: &["copy"],
    main,
};

fn should_overwrite(
    target: &Path,
    src: &Path,
    interactive: bool,
    no_clobber: bool,
    update: bool,
    force: bool,
) -> bool {
    if !target.exists() && !target.is_symlink() {
        return true;
    }
    if no_clobber {
        return false;
    }
    if update {
        if let (Ok(s), Ok(t)) = (
            src.metadata().and_then(|m| m.modified()),
            target.metadata().and_then(|m| m.modified()),
        ) {
            if s <= t {
                return false;
            }
        }
    }
    if interactive && !force {
        eprint!("cp: overwrite '{}'? ", target.display());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
        if !s.trim().to_ascii_lowercase().starts_with('y') {
            return false;
        }
    }
    true
}

fn copy_dir_recursive(src: &Path, dst: &Path, preserve: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_symlink() {
            #[cfg(unix)]
            {
                let target = std::fs::read_link(&from)?;
                std::os::unix::fs::symlink(target, &to)?;
            }
            #[cfg(windows)]
            {
                let target = std::fs::read_link(&from)?;
                if target.is_dir() {
                    std::os::windows::fs::symlink_dir(&target, &to)?;
                } else {
                    std::os::windows::fs::symlink_file(&target, &to)?;
                }
            }
        } else if ft.is_dir() {
            copy_dir_recursive(&from, &to, preserve)?;
        } else {
            std::fs::copy(&from, &to)?;
            if preserve {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&to) {
                            let _ = f.set_modified(mtime);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut recursive = false;
    let mut force = false;
    let mut verbose = false;
    let mut preserve_meta = false;
    let mut interactive = false;
    let mut no_clobber = false;
    let mut update = false;

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
                'r' | 'R' => recursive = true,
                'f' => {
                    force = true;
                    interactive = false;
                    no_clobber = false;
                }
                'v' => verbose = true,
                'p' => preserve_meta = true,
                'a' => {
                    recursive = true;
                    preserve_meta = true;
                }
                'i' => {
                    interactive = true;
                    no_clobber = false;
                    force = false;
                }
                'n' => {
                    no_clobber = true;
                    interactive = false;
                }
                'u' => update = true,
                _ => {
                    err("cp", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let positional: Vec<String> = args[i..].to_vec();
    if positional.len() < 2 {
        err("cp", "missing file operand");
        return 2;
    }
    let dest = PathBuf::from(positional.last().unwrap());
    let sources = &positional[..positional.len() - 1];
    let dest_is_dir = dest.is_dir();
    if sources.len() > 1 && !dest_is_dir {
        err("cp", &format!("target '{}' is not a directory", dest.display()));
        return 1;
    }

    let mut rc = 0;
    for s in sources {
        let src_path = PathBuf::from(s);
        if !src_path.exists() && !src_path.is_symlink() {
            err_path(
                "cp",
                s,
                &std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory"),
            );
            rc = 1;
            continue;
        }
        let target: PathBuf = if dest_is_dir {
            let name = src_path.file_name().unwrap_or_default();
            dest.join(name)
        } else {
            dest.clone()
        };
        if !should_overwrite(&target, &src_path, interactive, no_clobber, update, force) {
            continue;
        }
        let res: std::io::Result<()> = if src_path.is_dir() && !src_path.is_symlink() {
            if !recursive {
                err("cp", &format!("-r not specified; omitting directory '{s}'"));
                rc = 1;
                continue;
            }
            if target.exists() {
                if target.is_dir() && !target.is_symlink() {
                    let _ = std::fs::remove_dir_all(&target);
                } else {
                    let _ = std::fs::remove_file(&target);
                }
            }
            copy_dir_recursive(&src_path, &target, preserve_meta)
        } else {
            if target.exists() || target.is_symlink() {
                let _ = std::fs::remove_file(&target);
            }
            std::fs::copy(&src_path, &target).and_then(|_| {
                if preserve_meta {
                    if let Ok(meta) = src_path.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if let Ok(f) =
                                std::fs::OpenOptions::new().write(true).open(&target)
                            {
                                let _ = f.set_modified(mtime);
                            }
                        }
                    }
                }
                Ok(())
            })
        };
        match res {
            Ok(()) => {
                if verbose {
                    println!("'{s}' -> '{}'", target.display());
                }
            }
            Err(e) => {
                err_path("cp", s, &e);
                rc = 1;
            }
        }
    }
    rc
}
