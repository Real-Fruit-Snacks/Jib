//! `whoami` — print the effective user name.
//!
//! Mirrors `getpass.getuser()`'s fallback chain: `LOGNAME`, `USER`,
//! `LNAME`, `USERNAME`. On Windows that lands on `USERNAME`.

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "whoami",
    help: "print effective user name",
    aliases: &[],
    main,
};

fn current_user() -> Option<String> {
    for var in ["LOGNAME", "USER", "LNAME", "USERNAME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn main(_argv: &[String]) -> i32 {
    match current_user() {
        Some(u) => {
            println!("{u}");
            0
        }
        None => {
            err("whoami", "could not determine user name");
            1
        }
    }
}
