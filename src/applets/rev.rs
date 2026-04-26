//! `rev` — reverse lines characterwise. Trailing newline structure is
//! preserved (`\n` stays `\n`; `\r\n` stays `\r\n`).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::err_path;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "rev",
    help: "reverse lines characterwise",
    aliases: &[],
    main,
};

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let files: Vec<String> = if args.is_empty() {
        vec!["-".to_string()]
    } else {
        args
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;

    for f in &files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    err_path("rev", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        let mut buf: Vec<u8> = Vec::new();
        let mut br = reader;
        loop {
            buf.clear();
            let n = match br.read_until(b'\n', &mut buf) {
                Ok(n) => n,
                Err(e) => {
                    err_path("rev", f, &e);
                    rc = 1;
                    break;
                }
            };
            if n == 0 {
                break;
            }
            // Determine trailing newline shape.
            let (body, tail): (&[u8], &[u8]) = if buf.ends_with(b"\r\n") {
                (&buf[..buf.len() - 2], b"\r\n")
            } else if buf.ends_with(b"\n") {
                (&buf[..buf.len() - 1], b"\n")
            } else {
                (&buf[..], b"")
            };
            // Reverse char-by-char (UTF-8 safe).
            let s = String::from_utf8_lossy(body);
            let reversed: String = s.chars().rev().collect();
            let _ = out.write_all(reversed.as_bytes());
            let _ = out.write_all(tail);
        }
        let _ = out.flush();
    }
    rc
}
