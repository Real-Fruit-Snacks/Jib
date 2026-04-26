//! Top-level CLI: argv parsing, multi-call dispatch, top-level flags.
//!
//! Mirrors `mainsail/cli.py` in the Python project as closely as practical.

use std::path::Path;

use crate::registry::{self, Applet};
use crate::usage;

/// Stem of `argv[0]` lowercased — e.g. `/usr/local/bin/CAT.exe` → `cat`.
fn program_stem(argv0: &str) -> String {
    Path::new(argv0)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("mainsail")
        .to_lowercase()
}

fn print_top_help() {
    println!(
        "mainsail {} - cross-platform multi-call utility binary",
        crate::VERSION
    );
    println!();
    println!("Usage:");
    println!("  mainsail <applet> [args...]");
    println!("  mainsail <applet> --help       show help for <applet>");
    println!("  <applet> [args...]          (when installed as hardlink/symlink)");
    println!();
    println!("Top-level options:");
    println!("  --list           list available applets");
    println!("  --help, -h       show this help");
    println!("  --version        show version");
}

fn print_applet_help(applet: &Applet) {
    println!("{} - {}", applet.name, applet.help);
    if let Some(body) = usage::get(applet.name) {
        println!();
        // Trim a single trailing newline so println! doesn't double it.
        print!("{}", body.trim_end_matches('\n'));
        println!();
    }
    if !applet.aliases.is_empty() {
        println!();
        println!("Aliases: {}", applet.aliases.join(", "));
    }
}

fn print_list() {
    let rows = registry::list();
    if rows.is_empty() {
        return;
    }
    let width = rows.iter().map(|a| a.name.len()).max().unwrap_or(0);
    for a in rows {
        let suffix = if a.aliases.is_empty() {
            String::new()
        } else {
            format!("  (aliases: {})", a.aliases.join(", "))
        };
        println!("  {:<width$}  {}{}", a.name, a.help, suffix, width = width);
    }
}

/// Entry point. Returns the exit code; the binary clamps to a `u8`.
pub fn run(argv: &[String]) -> i32 {
    let argv0 = argv.first().map(String::as_str).unwrap_or("mainsail");
    let stem = program_stem(argv0);

    // Multi-call mode: argv[0] basename matches a known applet.
    if stem != "mainsail" {
        if let Some(applet) = registry::get(&stem) {
            // Intercept `--help` only — `-h` is overloaded by many applets
            // (df/du/sort use it for human-readable sizes).
            if argv.len() >= 2 && argv[1] == "--help" {
                print_applet_help(applet);
                return 0;
            }
            // Build child argv with the canonical-or-alias name as argv[0].
            let mut child = Vec::with_capacity(argv.len());
            child.push(stem);
            child.extend_from_slice(&argv[1..]);
            return (applet.main)(&child);
        }
    }

    // Wrapper mode: `mainsail <applet> [args...]`.
    if argv.len() < 2 {
        print_top_help();
        return 0;
    }

    let first = argv[1].as_str();
    match first {
        "--help" | "-h" => {
            // `mainsail --help <applet>` prints that applet's help.
            if argv.len() >= 3 {
                if let Some(applet) = registry::get(&argv[2]) {
                    print_applet_help(applet);
                    return 0;
                }
            }
            print_top_help();
            0
        }
        "--version" => {
            println!("mainsail {}", crate::VERSION);
            0
        }
        "--list" => {
            print_list();
            0
        }
        _ => {
            let Some(applet) = registry::get(first) else {
                eprintln!("mainsail: unknown applet '{first}'");
                eprintln!("try 'mainsail --list' to see all applets");
                return 1;
            };
            // `mainsail <applet> --help` -> applet help.
            if argv.len() >= 3 && argv[2] == "--help" {
                print_applet_help(applet);
                return 0;
            }
            let mut child = Vec::with_capacity(argv.len() - 1);
            child.push(first.to_string());
            child.extend_from_slice(&argv[2..]);
            (applet.main)(&child)
        }
    }
}
