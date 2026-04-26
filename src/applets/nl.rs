//! `nl` — number lines of files.
//!
//! Body styles: `-ba` number all lines, `-bt` (default) skip blanks, `-bn`
//! number none. Knobs: `-w` width, `-s` separator, `-v` start, `-i` step.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "nl",
    help: "number lines of files",
    aliases: &[],
    main,
};

#[derive(Clone, Copy)]
enum Body {
    All,
    NonEmpty,
    None,
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut width: usize = 6;
    let mut sep = "\t".to_string();
    let mut start: i64 = 1;
    let mut increment: i64 = 1;
    let mut body = Body::NonEmpty;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-b" | "--body-numbering" if i + 1 < args.len() => {
                body = match args[i + 1].as_str() {
                    "a" => Body::All,
                    "t" => Body::NonEmpty,
                    "n" => Body::None,
                    other => {
                        err("nl", &format!("invalid body-numbering style: {other}"));
                        return 2;
                    }
                };
                i += 2;
            }
            "-ba" => {
                body = Body::All;
                i += 1;
            }
            "-bt" => {
                body = Body::NonEmpty;
                i += 1;
            }
            "-bn" => {
                body = Body::None;
                i += 1;
            }
            "-w" | "--number-width" if i + 1 < args.len() => match args[i + 1].parse() {
                Ok(n) => {
                    width = n;
                    i += 2;
                }
                Err(_) => {
                    err("nl", &format!("invalid width: {}", args[i + 1]));
                    return 2;
                }
            },
            "-s" | "--number-separator" if i + 1 < args.len() => {
                sep = args[i + 1].clone();
                i += 2;
            }
            "-v" | "--starting-line-number" if i + 1 < args.len() => match args[i + 1].parse() {
                Ok(n) => {
                    start = n;
                    i += 2;
                }
                Err(_) => {
                    err("nl", &format!("invalid starting line: {}", args[i + 1]));
                    return 2;
                }
            },
            "-i" | "--line-increment" if i + 1 < args.len() => match args[i + 1].parse() {
                Ok(n) => {
                    increment = n;
                    i += 2;
                }
                Err(_) => {
                    err("nl", &format!("invalid increment: {}", args[i + 1]));
                    return 2;
                }
            },
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("nl", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let raw_files: Vec<String> = args[i..].to_vec();
    let files: Vec<String> = if raw_files.is_empty() {
        vec!["-".to_string()]
    } else {
        raw_files
    };
    let mut n = start;
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
                    err_path("nl", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            // strip trailing \r (incoming \r\n on Windows)
            let stripped: &str = line.strip_suffix('\r').unwrap_or(line.as_str());
            let emit = match body {
                Body::None => false,
                Body::NonEmpty => !stripped.is_empty(),
                Body::All => true,
            };
            if emit {
                let _ = writeln!(out, "{:>width$}{sep}{stripped}", n, width = width);
                n += increment;
            } else {
                let _ = writeln!(out, "{stripped}");
            }
        }
        let _ = out.flush();
    }
    rc
}
