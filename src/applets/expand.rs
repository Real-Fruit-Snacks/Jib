//! `expand` — convert tabs to spaces.
//!
//! `-t N` for a fixed step; `-t N1,N2,...` for explicit stops past which
//! we emit a single space. `-i` only converts leading tabs.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "expand",
    help: "convert tabs to spaces",
    aliases: &[],
    main,
};

fn parse_tabs(spec: &str) -> Option<Vec<usize>> {
    let parts: Vec<&str> = spec
        .replace(' ', ",")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .map(|s| s.as_str())
        .collect();
    let mut out = Vec::new();
    for p in parts {
        out.push(p.parse().ok()?);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut tabs: Vec<usize> = vec![8];
    let mut initial_only = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-i" | "--initial" => {
                initial_only = true;
                i += 1;
            }
            "-t" | "--tabs" if i + 1 < args.len() => match parse_tabs(&args[i + 1]) {
                Some(t) => {
                    tabs = t;
                    i += 2;
                }
                None => {
                    err("expand", &format!("invalid tabs: {}", args[i + 1]));
                    return 2;
                }
            },
            s if s.starts_with("-t") => match parse_tabs(&s[2..]) {
                Some(t) => {
                    tabs = t;
                    i += 1;
                }
                None => {
                    err("expand", &format!("invalid tabs: {}", &s[2..]));
                    return 2;
                }
            },
            s if s.starts_with('-') && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit()) => {
                tabs = vec![s[1..].parse().unwrap_or(8)];
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("expand", &format!("unknown option: {s}"));
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
                    err_path("expand", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            // Detect trailing CR for round-tripping.
            let (body, tail) = if let Some(s) = line.strip_suffix('\r') {
                (s, "\r\n")
            } else {
                (line.as_str(), "\n")
            };
            let mut col = 0usize;
            let mut seen_non_blank = false;
            let mut buf = String::with_capacity(body.len());
            for ch in body.chars() {
                if ch == '\t' {
                    if initial_only && seen_non_blank {
                        buf.push('\t');
                        col += 1;
                        continue;
                    }
                    let n = if tabs.len() == 1 {
                        let step = tabs[0];
                        step - (col % step)
                    } else {
                        match tabs.iter().copied().find(|&t| t > col) {
                            Some(stop) => stop - col,
                            None => 1,
                        }
                    };
                    for _ in 0..n {
                        buf.push(' ');
                    }
                    col += n;
                } else {
                    buf.push(ch);
                    col += 1;
                    if ch != ' ' {
                        seen_non_blank = true;
                    }
                }
            }
            let _ = out.write_all(buf.as_bytes());
            let _ = out.write_all(tail.as_bytes());
        }
        let _ = out.flush();
    }
    rc
}
