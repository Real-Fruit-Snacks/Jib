//! `rm` — remove files or directories.
//!
//! `-r`/`-R` recurse, `-f` ignore missing/suppress prompts, `-d` remove
//! empty directories, `-v` verbose. Symlinks are removed (never followed).

use std::path::Path;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "rm",
    help: "remove files or directories",
    aliases: &["del", "erase"],
    main,
};

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut recursive = false;
    let mut force = false;
    let mut verbose = false;
    let mut dir_only = false;

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
                'f' => force = true,
                'v' => verbose = true,
                'd' => dir_only = true,
                _ => {
                    err("rm", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let targets: Vec<String> = args[i..].to_vec();
    if targets.is_empty() {
        if force {
            return 0;
        }
        err("rm", "missing operand");
        return 2;
    }

    let mut rc = 0;
    for t in &targets {
        let p = Path::new(t);
        let st = match p.symlink_metadata() {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !force {
                    err_path("rm", t, &e);
                    rc = 1;
                }
                continue;
            }
            Err(e) => {
                err_path("rm", t, &e);
                rc = 1;
                continue;
            }
        };
        let ft = st.file_type();
        let is_dir_real = ft.is_dir() && !ft.is_symlink();
        let res = if is_dir_real {
            if recursive {
                std::fs::remove_dir_all(p)
            } else if dir_only {
                std::fs::remove_dir(p)
            } else {
                err("rm", &format!("cannot remove '{t}': Is a directory"));
                rc = 1;
                continue;
            }
        } else {
            std::fs::remove_file(p)
        };
        match res {
            Ok(()) => {
                if verbose {
                    println!("removed '{t}'");
                }
            }
            Err(e) => {
                if !force {
                    err_path("rm", t, &e);
                    rc = 1;
                }
            }
        }
    }
    rc
}
