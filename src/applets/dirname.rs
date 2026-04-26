//! `dirname` — strip the last component from a path.

use std::io::Write;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "dirname",
    help: "strip last component from file name",
    aliases: &[],
    main,
};

fn dirname(s: &str) -> String {
    let stripped = s.trim_end_matches(|c| c == '/' || c == '\\');
    if stripped.is_empty() {
        return if s.is_empty() {
            ".".to_string()
        } else {
            s.to_string()
        };
    }
    match stripped.rfind(|c| c == '/' || c == '\\') {
        Some(0) => stripped[..1].to_string(),
        Some(i) => stripped[..i].to_string(),
        None => ".".to_string(),
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut zero = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-z" | "--zero" => {
                zero = true;
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("dirname", &format!("invalid option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let paths = &args[i..];
    if paths.is_empty() {
        err("dirname", "missing operand");
        return 2;
    }

    let end: u8 = if zero { 0 } else { b'\n' };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for p in paths {
        let _ = out.write_all(dirname(p).as_bytes());
        let _ = out.write_all(&[end]);
    }
    0
}
