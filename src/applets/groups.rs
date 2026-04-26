//! `groups` — list the user's group memberships, space-separated.
//!
//! Equivalent to `id -Gn`. With no libc we don't have the real kernel
//! group table, so we report a single "users-like" group; this matches
//! how `id` in this binary behaves and is enough for the scripting
//! cases we care about.

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "groups",
    help: "print group memberships",
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
    if cfg!(windows) {
        vec!["Users".to_string()]
    } else {
        vec![current_user()]
    }
}

fn main(argv: &[String]) -> i32 {
    for a in &argv[1..] {
        if a.starts_with('-') && a.len() > 1 {
            err("groups", &format!("unknown option: {a}"));
            return 2;
        }
    }
    let groups = current_groups();
    println!("{}", groups.join(" "));
    0
}
