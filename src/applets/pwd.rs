//! `pwd` — print the working directory.
//!
//! `-L` (default) prefers `$PWD` if it exists, is absolute, and points at
//! the same directory as `getcwd()` (so a logical view through symlinks is
//! preserved). `-P` always prints the resolved physical path.

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "pwd",
    help: "print name of current/working directory",
    aliases: &[],
    main,
};

fn main(argv: &[String]) -> i32 {
    let mut physical = false;
    for arg in &argv[1..] {
        match arg.as_str() {
            "-L" => physical = false,
            "-P" => physical = true,
            "--help" | "-h" => {
                println!("usage: pwd [-LP]");
                return 0;
            }
            other => {
                err("pwd", &format!("invalid option: {other}"));
                return 2;
            }
        }
    }

    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            err("pwd", &e.to_string());
            return 1;
        }
    };

    if physical {
        match cwd.canonicalize() {
            Ok(p) => println!("{}", p.display()),
            Err(_) => println!("{}", cwd.display()),
        }
        return 0;
    }

    if let Ok(pwd_env) = std::env::var("PWD") {
        let p = std::path::Path::new(&pwd_env);
        if p.is_absolute() {
            // samefile: compare canonicalized forms, swallow any error.
            if let (Ok(a), Ok(b)) = (p.canonicalize(), cwd.canonicalize()) {
                if a == b {
                    println!("{pwd_env}");
                    return 0;
                }
            }
        }
    }
    println!("{}", cwd.display());
    0
}
