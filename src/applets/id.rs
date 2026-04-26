//! `id` — print real and effective user/group IDs.
//!
//! Without libc we can't fetch the real uid/gid table; this is a
//! best-effort implementation that mirrors `whoami`'s env-var fallback
//! chain for the user name and reports `0` for the IDs and a single
//! "users"-style group on platforms where we don't have better info.
//!
//! The behaviour is sufficient for the common scripting cases
//! (`id -u`, `id -un`, `id -g`, `id -Gn`); anything that needs the real
//! kernel-reported IDs should fall back to the system `id`.

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "id",
    help: "print user/group IDs",
    aliases: &[],
    main,
};

fn current_user() -> String {
    for var in ["LOGNAME", "USER", "LNAME", "USERNAME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    "unknown".to_string()
}

fn current_groups() -> Vec<String> {
    // Best-effort: a single group equal to the user name on Unix
    // (matches util.linux behaviour when `getgroups` isn't available),
    // or "Users" on Windows where no env var exposes group membership.
    if cfg!(windows) {
        vec!["Users".to_string()]
    } else {
        vec![current_user()]
    }
}

fn main(argv: &[String]) -> i32 {
    let args: &[String] = &argv[1..];
    let mut want_uid = false;
    let mut want_gid = false;
    let mut want_groups = false;
    let mut name_only = false;

    for a in args {
        match a.as_str() {
            "--user" => want_uid = true,
            "--group" => want_gid = true,
            "--groups" => want_groups = true,
            "--name" => name_only = true,
            s if s.starts_with('-') && s.len() > 1 && !s.starts_with("--") => {
                // Short flag block: `-u`, `-Gn`, etc.
                for ch in s[1..].chars() {
                    match ch {
                        'u' => want_uid = true,
                        'g' => want_gid = true,
                        'G' => want_groups = true,
                        'n' => name_only = true,
                        _ => {
                            err("id", &format!("unknown option: -{ch}"));
                            return 2;
                        }
                    }
                }
            }
            s if s.starts_with("--") => {
                err("id", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                err("id", "operands not supported");
                return 2;
            }
        }
    }

    let user = current_user();
    let groups = current_groups();
    let primary = groups.first().cloned().unwrap_or_else(|| user.clone());

    if want_uid {
        if name_only {
            println!("{user}");
        } else {
            println!("0");
        }
        return 0;
    }
    if want_gid {
        if name_only {
            println!("{primary}");
        } else {
            println!("0");
        }
        return 0;
    }
    if want_groups {
        if name_only {
            println!("{}", groups.join(" "));
        } else {
            // One ID per group; we don't have real numbers, so emit zeros.
            let z: Vec<String> = groups.iter().map(|_| "0".to_string()).collect();
            println!("{}", z.join(" "));
        }
        return 0;
    }

    // Default: full id line.
    println!(
        "uid=0({user}) gid=0({primary}) groups={}",
        groups
            .iter()
            .map(|g| format!("0({g})"))
            .collect::<Vec<_>>()
            .join(",")
    );
    0
}
