//! `chmod` — change file mode bits.
//!
//! Accepts an octal mode (`644`, `0755`, `1777`) or symbolic clauses like
//! `u+x`, `go-w`, `a=r`, joined by commas. On Windows we can only toggle
//! the read-only bit; we set it from the resulting `0o200` (write) bit
//! across user/group/other for parity with Python's `os.chmod`.

use std::path::Path;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "chmod",
    help: "change file mode bits",
    aliases: &[],
    main,
};

fn parse_octal(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 8).ok()
}

fn apply_clause(mut mode: u32, clause: &str) -> Result<u32, String> {
    let mut op_idx = None;
    for (i, c) in clause.char_indices() {
        if c == '+' || c == '-' || c == '=' {
            op_idx = Some((i, c));
            break;
        }
    }
    let (idx, op) = op_idx.ok_or_else(|| format!("no operator in clause '{clause}'"))?;
    let who = &clause[..idx];
    let perms = &clause[idx + 1..];

    let (who_mask, has_u, has_g): (u32, bool, bool) = if who.is_empty() || who.contains('a') {
        (0o777, true, true)
    } else {
        let mut m = 0u32;
        if who.contains('u') {
            m |= 0o700;
        }
        if who.contains('g') {
            m |= 0o070;
        }
        if who.contains('o') {
            m |= 0o007;
        }
        (m, who.contains('u'), who.contains('g'))
    };

    let mut perm_bits = 0u32;
    if perms.contains('r') {
        perm_bits |= 0o444;
    }
    if perms.contains('w') {
        perm_bits |= 0o222;
    }
    if perms.contains('x') {
        perm_bits |= 0o111;
    }
    let mut effective = perm_bits & who_mask;
    if perms.contains('s') {
        if has_u {
            effective |= 0o4000;
        }
        if has_g {
            effective |= 0o2000;
        }
    }
    if perms.contains('t') {
        effective |= 0o1000;
    }

    mode = match op {
        '+' => mode | effective,
        '-' => mode & !effective,
        '=' => {
            let mut clear = who_mask;
            if has_u {
                clear |= 0o4000;
            }
            if has_g {
                clear |= 0o2000;
            }
            (mode & !clear) | effective
        }
        _ => unreachable!(),
    };
    Ok(mode)
}

fn compute_new_mode(current: u32, spec: &str) -> Result<u32, String> {
    if let Some(o) = parse_octal(spec) {
        return Ok(o);
    }
    let mut mode = current;
    for clause in spec.split(',') {
        mode = apply_clause(mode, clause.trim())?;
    }
    Ok(mode)
}

#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(p: &Path, mode: u32) -> std::io::Result<()> {
    // Windows: only toggle the read-only attribute.
    let mut perms = std::fs::metadata(p)?.permissions();
    perms.set_readonly(mode & 0o222 == 0);
    std::fs::set_permissions(p, perms)
}

fn walk_apply(p: &Path, spec: &str, verbose: bool, changes_only: bool, silent: bool, rc: &mut i32) {
    let st = match std::fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) => {
            if !silent {
                err_path("chmod", &p.display().to_string(), &e);
            }
            *rc = 1;
            return;
        }
    };
    if st.file_type().is_symlink() {
        // Don't dereference symlinks; the GNU chmod default is also to skip.
        return;
    }
    let current = crate::common::unix_mode(&st, p) & 0o7777;
    let new_mode = match compute_new_mode(current, spec) {
        Ok(m) => m,
        Err(e) => {
            err("chmod", &e);
            *rc = 2;
            return;
        }
    };
    if new_mode == current {
        if verbose && !changes_only {
            println!("mode of '{}' retained as {:04o}", p.display(), current);
        }
    } else if let Err(e) = set_mode(p, new_mode) {
        if !silent {
            err_path("chmod", &p.display().to_string(), &e);
        }
        *rc = 1;
        return;
    } else if verbose || changes_only {
        println!(
            "mode of '{}' changed from {:04o} to {:04o}",
            p.display(),
            current,
            new_mode
        );
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut recursive = false;
    let mut verbose = false;
    let mut changes_only = false;
    let mut silent = false;

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
        // Stop flag parsing at anything that isn't a recognized chmod flag.
        if !a[1..].chars().all(|c| matches!(c, 'R' | 'r' | 'v' | 'c' | 'f')) {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'R' | 'r' => recursive = true,
                'v' => verbose = true,
                'c' => changes_only = true,
                'f' => silent = true,
                _ => unreachable!(),
            }
        }
        i += 1;
    }

    let remaining: Vec<String> = args[i..].to_vec();
    if remaining.len() < 2 {
        err("chmod", "missing operand");
        return 2;
    }
    let mode_spec = remaining[0].clone();
    let paths = &remaining[1..];

    let mut rc = 0;
    for path in paths {
        let p = Path::new(path);
        if !p.exists() && !p.is_symlink() {
            if !silent {
                err_path(
                    "chmod",
                    path,
                    &std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No such file or directory",
                    ),
                );
            }
            rc = 1;
            continue;
        }
        walk_apply(p, &mode_spec, verbose, changes_only, silent, &mut rc);
        if recursive && p.is_dir() && !p.is_symlink() {
            walk(p, &mode_spec, verbose, changes_only, silent, &mut rc);
        }
    }
    rc
}

fn walk(dir: &Path, spec: &str, verbose: bool, changes_only: bool, silent: bool, rc: &mut i32) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            if !silent {
                err_path("chmod", &dir.display().to_string(), &e);
            }
            *rc = 1;
            return;
        }
    };
    for entry in entries.flatten() {
        let p = entry.path();
        walk_apply(&p, spec, verbose, changes_only, silent, rc);
        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() && !ft.is_symlink() {
                walk(&p, spec, verbose, changes_only, silent, rc);
            }
        }
    }
}
