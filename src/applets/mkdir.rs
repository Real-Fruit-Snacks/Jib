//! `mkdir` — make directories.
//!
//! `-p` creates intermediate components; `-m MODE` sets the resulting
//! permission (Unix only; ignored on Windows); `-v` prints a line per
//! created directory.

use std::path::Path;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "mkdir",
    help: "make directories",
    aliases: &["md"],
    main,
};

fn parse_octal(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok()
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    // Windows doesn't have POSIX mode bits; -m is a parity no-op.
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut parents = false;
    let mut verbose = false;
    let mut mode: Option<u32> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a == "-m" && i + 1 < args.len() {
            match parse_octal(&args[i + 1]) {
                Some(m) => {
                    mode = Some(m);
                    i += 2;
                    continue;
                }
                None => {
                    err("mkdir", &format!("invalid mode: '{}'", args[i + 1]));
                    return 2;
                }
            }
        }
        if let Some(rest) = a.strip_prefix("--mode=") {
            match parse_octal(rest) {
                Some(m) => {
                    mode = Some(m);
                    i += 1;
                    continue;
                }
                None => {
                    err("mkdir", &format!("invalid mode: '{rest}'"));
                    return 2;
                }
            }
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'p' => parents = true,
                'v' => verbose = true,
                _ => {
                    err("mkdir", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let dirs: Vec<String> = args[i..].to_vec();
    if dirs.is_empty() {
        err("mkdir", "missing operand");
        return 2;
    }

    let mut rc = 0;
    for d in &dirs {
        let p = Path::new(d);
        let res = if parents {
            std::fs::create_dir_all(p)
        } else {
            std::fs::create_dir(p)
        };
        match res {
            Ok(()) => {
                if let Some(m) = mode {
                    if let Err(e) = set_mode(p, m) {
                        err_path("mkdir", d, &e);
                        rc = 1;
                        continue;
                    }
                }
                if verbose {
                    println!("mkdir: created directory '{d}'");
                }
            }
            Err(e) => {
                // -p should swallow "already exists" errors but Rust's
                // create_dir_all does that itself; we only see other ones.
                err_path("mkdir", d, &e);
                rc = 1;
            }
        }
    }
    rc
}
