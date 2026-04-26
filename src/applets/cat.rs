//! `cat` — concatenate files (or stdin) to stdout.
//!
//! Supports `-n` (number all lines) and `-b` (number non-blank lines, beats
//! `-n`). A bare `-` means stdin; `--` ends option parsing. Unknown short
//! options return exit 2 to match the Python applet.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "cat",
    help: "concatenate files and print on the standard output",
    aliases: &["type"],
    main,
};

const CHUNK: usize = 64 * 1024;

enum Source {
    Stdin,
    File(String),
}

fn open(src: &Source) -> io::Result<Box<dyn Read>> {
    match src {
        Source::Stdin => Ok(Box::new(io::stdin().lock()) as Box<dyn Read>),
        Source::File(p) => Ok(Box::new(File::open(p)?) as Box<dyn Read>),
    }
}

fn label(src: &Source) -> &str {
    match src {
        Source::Stdin => "-",
        Source::File(p) => p.as_str(),
    }
}

fn copy_raw(srcs: &[Source]) -> i32 {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;
    let mut buf = vec![0u8; CHUNK];
    for s in srcs {
        let mut fh = match open(s) {
            Ok(f) => f,
            Err(e) => {
                err_path("cat", label(s), &e);
                rc = 1;
                continue;
            }
        };
        loop {
            match fh.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out.write_all(&buf[..n]).is_err() {
                        return rc;
                    }
                }
                Err(e) => {
                    err_path("cat", label(s), &e);
                    rc = 1;
                    break;
                }
            }
        }
    }
    let _ = out.flush();
    rc
}

fn copy_numbered(srcs: &[Source], number_all: bool, number_nonblank: bool) -> i32 {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;
    let mut counter = 0u64;
    for s in srcs {
        let fh = match open(s) {
            Ok(f) => f,
            Err(e) => {
                err_path("cat", label(s), &e);
                rc = 1;
                continue;
            }
        };
        let reader = BufReader::new(fh);
        // BufRead::split lets us preserve "no trailing newline" semantics,
        // matching the Python applet which only re-emits '\n' when the
        // source line had one.
        let mut buf = Vec::with_capacity(8 * 1024);
        let mut br = reader;
        loop {
            buf.clear();
            let n = match br.read_until(b'\n', &mut buf) {
                Ok(n) => n,
                Err(e) => {
                    err_path("cat", label(s), &e);
                    rc = 1;
                    break;
                }
            };
            if n == 0 {
                break;
            }
            let ends_nl = buf.ends_with(b"\n");
            let body = if ends_nl { &buf[..buf.len() - 1] } else { &buf[..] };
            let blank = body.is_empty();
            if number_all || (number_nonblank && !blank) {
                counter += 1;
                let _ = write!(out, "{:>6}\t", counter);
            }
            if out.write_all(body).is_err() {
                return rc;
            }
            if ends_nl && out.write_all(b"\n").is_err() {
                return rc;
            }
        }
    }
    let _ = out.flush();
    rc
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut number_all = false;
    let mut number_nonblank = false;
    let mut files: Vec<Source> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            for f in &args[i + 1..] {
                files.push(if f == "-" {
                    Source::Stdin
                } else {
                    Source::File(f.clone())
                });
            }
            break;
        }
        if a == "-" || !a.starts_with('-') || a.len() < 2 {
            files.push(if a == "-" {
                Source::Stdin
            } else {
                Source::File(a.clone())
            });
        } else {
            for ch in a[1..].chars() {
                match ch {
                    'n' => {
                        number_all = true;
                        number_nonblank = false;
                    }
                    'b' => {
                        number_nonblank = true;
                        number_all = false;
                    }
                    _ => {
                        err("cat", &format!("invalid option: -{ch}"));
                        return 2;
                    }
                }
            }
        }
        i += 1;
    }

    if files.is_empty() {
        files.push(Source::Stdin);
    }

    if number_all || number_nonblank {
        copy_numbered(&files, number_all, number_nonblank)
    } else {
        copy_raw(&files)
    }
}
