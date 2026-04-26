//! `mv` — move (rename) files. `-f`/`-i`/`-n`/`-u`/`-v` modulate the
//! overwrite policy. Cross-device moves fall back to copy+remove.

use std::path::{Path, PathBuf};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "mv",
    help: "move (rename) files",
    aliases: &["move", "ren", "rename"],
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
        eprint!("mv: overwrite '{}'? ", target.display());
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
        if !s.trim().to_ascii_lowercase().starts_with('y') {
            return false;
        }
    }
    true
}

fn move_one(src: &Path, target: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, target) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Cross-device or "not a regular rename" — fall back to copy+remove.
            if e.raw_os_error().is_some() {
                if src.is_dir() {
                    copy_dir_recursive(src, target)?;
                    std::fs::remove_dir_all(src)?;
                } else {
                    std::fs::copy(src, target)?;
                    std::fs::remove_file(src)?;
                }
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut force = false;
    let mut verbose = false;
    let mut no_clobber = false;
    let mut interactive = false;
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
                'f' => {
                    force = true;
                    no_clobber = false;
                    interactive = false;
                }
                'n' => {
                    no_clobber = true;
                    force = false;
                    interactive = false;
                }
                'i' => {
                    interactive = true;
                    force = false;
                    no_clobber = false;
                }
                'u' => update = true,
                'v' => verbose = true,
                _ => {
                    err("mv", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let positional: Vec<String> = args[i..].to_vec();
    if positional.len() < 2 {
        err("mv", "missing file operand");
        return 2;
    }
    let dest = PathBuf::from(positional.last().unwrap());
    let sources = &positional[..positional.len() - 1];
    let dest_is_dir = dest.is_dir();
    if sources.len() > 1 && !dest_is_dir {
        err(
            "mv",
            &format!("target '{}' is not a directory", dest.display()),
        );
        return 1;
    }

    let mut rc = 0;
    for s in sources {
        let src_path = PathBuf::from(s);
        if !src_path.exists() && !src_path.is_symlink() {
            err_path(
                "mv",
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
        match move_one(&src_path, &target) {
            Ok(()) => {
                if verbose {
                    println!("'{s}' -> '{}'", target.display());
                }
            }
            Err(e) => {
                err_path("mv", s, &e);
                rc = 1;
            }
        }
    }
    rc
}
