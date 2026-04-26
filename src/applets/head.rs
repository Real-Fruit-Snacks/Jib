//! `head` — print the first part of files.
//!
//! `-n N` (default 10) lines, `-c N` bytes. The legacy `-N` shorthand for
//! `-n N` is supported. With multiple files, prints `==> NAME <==` headers.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "head",
    help: "output the first part of files",
    aliases: &[],
    main,
};

fn parse_count(s: &str) -> Option<i64> {
    s.parse::<i64>().ok()
}

enum Source {
    Stdin,
    File(String),
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut lines: i64 = 10;
    let mut bytes_mode = false;
    let mut byte_count: i64 = 0;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a == "-n" && i + 1 < args.len() {
            match parse_count(&args[i + 1]) {
                Some(n) => {
                    lines = n;
                    i += 2;
                    continue;
                }
                None => {
                    err("head", &format!("invalid line count: {}", args[i + 1]));
                    return 2;
                }
            }
        }
        if a == "-c" && i + 1 < args.len() {
            match parse_count(&args[i + 1]) {
                Some(n) => {
                    bytes_mode = true;
                    byte_count = n;
                    i += 2;
                    continue;
                }
                None => {
                    err("head", &format!("invalid byte count: {}", args[i + 1]));
                    return 2;
                }
            }
        }
        if a.starts_with('-') && a.len() > 1 && a[1..].chars().all(|c| c.is_ascii_digit()) {
            lines = a[1..].parse().unwrap_or(10);
            i += 1;
            continue;
        }
        break;
    }

    let raw_files: Vec<String> = args[i..].to_vec();
    let files: Vec<Source> = if raw_files.is_empty() {
        vec![Source::Stdin]
    } else {
        raw_files
            .iter()
            .map(|f| {
                if f == "-" {
                    Source::Stdin
                } else {
                    Source::File(f.clone())
                }
            })
            .collect()
    };
    let multi = files.len() > 1;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;

    for (idx, src) in files.iter().enumerate() {
        let label = match src {
            Source::Stdin => "-".to_string(),
            Source::File(p) => p.clone(),
        };
        let fh: Box<dyn Read> = match src {
            Source::Stdin => Box::new(io::stdin().lock()),
            Source::File(p) => match File::open(p) {
                Ok(f) => Box::new(f),
                Err(e) => {
                    err_path("head", p, &e);
                    rc = 1;
                    continue;
                }
            },
        };
        if multi {
            if idx > 0 {
                let _ = out.write_all(b"\n");
            }
            let _ = writeln!(out, "==> {label} <==");
            let _ = out.flush();
        }
        if bytes_mode {
            let mut take = fh.take(byte_count.max(0) as u64);
            let mut buf = [0u8; 64 * 1024];
            loop {
                match take.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if out.write_all(&buf[..n]).is_err() {
                            return rc;
                        }
                    }
                    Err(e) => {
                        err_path("head", &label, &e);
                        rc = 1;
                        break;
                    }
                }
            }
        } else {
            let reader = BufReader::new(fh);
            let mut count: i64 = 0;
            let mut br = reader;
            let mut buf: Vec<u8> = Vec::new();
            loop {
                if count >= lines {
                    break;
                }
                buf.clear();
                let n = match br.read_until(b'\n', &mut buf) {
                    Ok(n) => n,
                    Err(e) => {
                        err_path("head", &label, &e);
                        rc = 1;
                        break;
                    }
                };
                if n == 0 {
                    break;
                }
                if out.write_all(&buf).is_err() {
                    return rc;
                }
                count += 1;
            }
        }
        let _ = out.flush();
    }
    rc
}
