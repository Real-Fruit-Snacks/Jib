//! `realpath` — resolve a path to its canonical absolute form.
//!
//! `-e` requires the path to exist. `-m` is accepted as a no-op (Rust's
//! [`Path::canonicalize`] requires existence, so for `-m` we fall back to
//! `absolutize`). `-s`/`-L` skip symlink resolution. `--relative-to=DIR`
//! prints the result relative to DIR.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "realpath",
    help: "resolve a path to its canonical absolute form",
    aliases: &[],
    main,
};

/// Lexical absolutize without touching the filesystem (mirrors Python's
/// `os.path.abspath`). Joins relative paths against the cwd, then collapses
/// `.`/`..` components.
fn lexical_abs(p: &Path) -> std::io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

/// Compute a relative path from `base` to `path`. Returns the original if
/// roots differ (e.g. different Windows drives).
fn relative_to(path: &Path, base: &Path) -> PathBuf {
    let path = path.components().collect::<Vec<_>>();
    let base = base.components().collect::<Vec<_>>();

    // Find common prefix
    let mut i = 0;
    while i < path.len() && i < base.len() && path[i] == base[i] {
        i += 1;
    }
    let mut out = PathBuf::new();
    for _ in 0..(base.len() - i) {
        out.push("..");
    }
    for c in &path[i..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut require_exist = false;
    let mut no_symlink = false;
    let mut zero = false;
    let mut rel_to: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-e" | "--canonicalize-existing" => {
                require_exist = true;
                i += 1;
            }
            "-m" | "--canonicalize-missing" => {
                // No-op; Rust's lexical abs already tolerates missing parts.
                i += 1;
            }
            "-s" | "-L" | "--strip" | "--no-symlinks" => {
                no_symlink = true;
                i += 1;
            }
            "-z" | "--zero" => {
                zero = true;
                i += 1;
            }
            "--relative-to" if i + 1 < args.len() => {
                rel_to = Some(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with("--relative-to=") => {
                rel_to = Some(s["--relative-to=".len()..].to_string());
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("realpath", &format!("invalid option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let paths = &args[i..];
    if paths.is_empty() {
        err("realpath", "missing operand");
        return 2;
    }

    let end: u8 = if zero { 0 } else { b'\n' };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;

    for p in paths {
        let resolved = if no_symlink {
            match lexical_abs(Path::new(p)) {
                Ok(r) => r,
                Err(e) => {
                    err_path("realpath", p, &e);
                    rc = 1;
                    continue;
                }
            }
        } else {
            match Path::new(p).canonicalize() {
                Ok(r) => r,
                Err(_) => {
                    // Mirrors Python's os.path.realpath which tolerates
                    // missing parts: fall back to lexical abs.
                    match lexical_abs(Path::new(p)) {
                        Ok(r) => r,
                        Err(e) => {
                            err_path("realpath", p, &e);
                            rc = 1;
                            continue;
                        }
                    }
                }
            }
        };

        if require_exist && !resolved.exists() {
            err_path(
                "realpath",
                p,
                &std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory"),
            );
            rc = 1;
            continue;
        }

        let final_path = if let Some(base) = &rel_to {
            let base_resolved = match Path::new(base).canonicalize() {
                Ok(b) => b,
                Err(_) => match lexical_abs(Path::new(base)) {
                    Ok(b) => b,
                    Err(e) => {
                        err("realpath", &e.to_string());
                        rc = 1;
                        continue;
                    }
                },
            };
            relative_to(&resolved, &base_resolved)
        } else {
            resolved
        };

        // On Windows, canonicalize() returns a `\\?\` UNC-prefixed path.
        // Strip it for friendly output, mirroring most coreutils-style tools.
        let display = final_path.display().to_string();
        let stripped = display.strip_prefix(r"\\?\").unwrap_or(&display);
        let _ = out.write_all(stripped.as_bytes());
        let _ = out.write_all(&[end]);
    }
    rc
}
