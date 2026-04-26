//! `yes` — repeatedly print STRING (or `y`) until the pipe closes.

use std::io::{self, Write};

use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "yes",
    help: "repeatedly output a line with the given STRING (or 'y')",
    aliases: &[],
    main,
};

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let text = if args.is_empty() {
        "y".to_string()
    } else {
        args.join(" ")
    };
    let mut line = text.into_bytes();
    line.push(b'\n');

    // Pre-build a chunk so the syscall rate is reasonable on a fast pipe.
    let mut chunk: Vec<u8> = Vec::with_capacity(line.len() * 64);
    for _ in 0..64 {
        chunk.extend_from_slice(&line);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    loop {
        if out.write_all(&chunk).is_err() {
            break;
        }
        if out.flush().is_err() {
            break;
        }
    }
    0
}
