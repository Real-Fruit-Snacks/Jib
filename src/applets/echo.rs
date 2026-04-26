//! `echo` — write arguments to stdout, with optional escape interpretation.
//!
//! Mirrors the Python applet: BSD-style `-n`/`-e`/`-E` are recognized as a
//! merged flag block (`-ne`, `-En`, ...). The first non-flag (or `--`) ends
//! flag parsing. Anything that looks like a flag but contains an unknown
//! letter is treated as a literal argument (matching coreutils behavior).

use std::io::Write;

use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "echo",
    help: "display a line of text",
    aliases: &[],
    main,
};

fn interpret(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                let mapped = match next {
                    '\\' => Some('\\'),
                    'a' => Some('\x07'),
                    'b' => Some('\x08'),
                    'f' => Some('\x0c'),
                    'n' => Some('\n'),
                    'r' => Some('\r'),
                    't' => Some('\t'),
                    'v' => Some('\x0b'),
                    '0' => Some('\0'),
                    _ => None,
                };
                if let Some(m) = mapped {
                    out.push(m);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut newline = true;
    let mut do_interpret = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        if a.len() < 2 || !a.starts_with('-') {
            break;
        }
        // Only consume as flag block if every char after '-' is one of n/e/E.
        let body = &a[1..];
        if !body.chars().all(|c| matches!(c, 'n' | 'e' | 'E')) {
            break;
        }
        for ch in body.chars() {
            match ch {
                'n' => newline = false,
                'e' => do_interpret = true,
                'E' => do_interpret = false,
                _ => unreachable!(),
            }
        }
        i += 1;
    }

    let text = args[i..].join(" ");
    let text = if do_interpret { interpret(&text) } else { text };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(text.as_bytes());
    if newline {
        let _ = out.write_all(b"\n");
    }
    0
}
