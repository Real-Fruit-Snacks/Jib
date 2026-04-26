//! `env` — run a program in a modified environment, or print the environment.
//!
//! Module name is `env_` to avoid shadowing `std::env`. `-i` ignores the
//! inherited env; `-u VAR` removes a single variable; `KEY=VAL` pairs after
//! flags add to the environment for the spawned command. With no command,
//! prints the resulting environment as `KEY=VAL` lines, sorted by key.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::Command;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "env",
    help: "run a program in a modified environment, or print the environment",
    aliases: &[],
    main,
};

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut ignore_env = false;
    let mut unsets: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        match a.as_str() {
            "-i" | "--ignore-environment" => {
                ignore_env = true;
                i += 1;
            }
            "-u" | "--unset" if i + 1 < args.len() => {
                unsets.push(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 && !s.starts_with("--") => {
                for ch in s[1..].chars() {
                    if ch == 'i' {
                        ignore_env = true;
                    } else {
                        err("env", &format!("invalid option: -{ch}"));
                        return 2;
                    }
                }
                i += 1;
            }
            _ => break,
        }
    }

    let mut env: BTreeMap<String, String> = if ignore_env {
        BTreeMap::new()
    } else {
        std::env::vars().collect()
    };
    for u in &unsets {
        env.remove(u);
    }

    // KEY=VAL pairs come right before the command (if any).
    while i < args.len() {
        let a = &args[i];
        if a.starts_with('=') {
            break;
        }
        if let Some(eq) = a.find('=') {
            env.insert(a[..eq].to_string(), a[eq + 1..].to_string());
            i += 1;
        } else {
            break;
        }
    }

    let remaining = &args[i..];
    if remaining.is_empty() {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        for (k, v) in &env {
            let _ = writeln!(out, "{k}={v}");
        }
        return 0;
    }

    let prog = &remaining[0];
    let rest = &remaining[1..];
    let mut cmd = Command::new(prog);
    cmd.args(rest);
    if ignore_env {
        cmd.env_clear();
    } else {
        for u in &unsets {
            cmd.env_remove(u);
        }
    }
    for (k, v) in &env {
        cmd.env(k, v);
    }
    match cmd.status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            err("env", &format!("{prog}: No such file or directory"));
            127
        }
        Err(e) => {
            err("env", &format!("{prog}: {e}"));
            126
        }
    }
}
