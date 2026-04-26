//! `basename` — strip directory components (and optional suffix) from a path.

use std::io::Write;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "basename",
    help: "strip directory components from a filename",
    aliases: &[],
    main,
};

fn basename(s: &str) -> &str {
    // Strip trailing separators, then return everything after the last
    // remaining separator. Mirrors `_basename` in the Python applet, which
    // accepts both `/` and `\` regardless of platform.
    let stripped = s.trim_end_matches(|c| c == '/' || c == '\\');
    if stripped.is_empty() {
        // "/"  -> "/"
        // "\\" -> "\\"
        // ""   -> ""
        return s
            .get(0..s.chars().next().map_or(0, char::len_utf8))
            .unwrap_or("");
    }
    match stripped.rfind(|c| c == '/' || c == '\\') {
        Some(i) => &stripped[i + 1..],
        None => stripped,
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut multiple = false;
    let mut suffix_all: Option<String> = None;
    let mut zero = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-a" | "--multiple" => {
                multiple = true;
                i += 1;
            }
            "-s" if i + 1 < args.len() => {
                suffix_all = Some(args[i + 1].clone());
                multiple = true;
                i += 2;
            }
            s if s.starts_with("--suffix=") => {
                suffix_all = Some(s["--suffix=".len()..].to_string());
                multiple = true;
                i += 1;
            }
            "-z" | "--zero" => {
                zero = true;
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("basename", &format!("invalid option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let remaining: Vec<String> = args[i..].to_vec();
    if remaining.is_empty() {
        err("basename", "missing operand");
        return 2;
    }

    let end: u8 = if zero { 0 } else { b'\n' };
    let (paths, suffix): (Vec<&String>, String) = if multiple {
        (remaining.iter().collect(), suffix_all.unwrap_or_default())
    } else {
        let s = remaining.get(1).cloned().unwrap_or_default();
        (vec![&remaining[0]], s)
    };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for p in paths {
        let mut name = basename(p).to_string();
        if !suffix.is_empty() && name.ends_with(&suffix) && name != suffix {
            name.truncate(name.len() - suffix.len());
        }
        let _ = out.write_all(name.as_bytes());
        let _ = out.write_all(&[end]);
    }
    0
}
