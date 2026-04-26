//! `unexpand` — convert runs of spaces (aligned to tab stops) into tabs.
//!
//! By default only leading runs are converted. `-a` converts everywhere.
//! `-t SPEC` sets a fixed step or explicit list of stops; with `-t`,
//! `-a` is implied (per GNU).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "unexpand",
    help: "convert spaces to tabs",
    aliases: &[],
    main,
};

fn parse_tabs(spec: &str) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    for p in spec.replace(' ', ",").split(',').filter(|s| !s.is_empty()) {
        out.push(p.parse().ok()?);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Collapse `count` consecutive spaces starting at column `start_col` into
/// the smallest sequence of tabs and spaces that aligns to the configured
/// stops. Mirrors `_compact_spaces` in the Python applet.
fn compact_spaces(start_col: usize, count: usize, tabs: &[usize], step: usize) -> String {
    let mut out = String::new();
    let mut col = start_col;
    let mut remaining = count;
    while remaining > 0 {
        let next_stop = if step > 0 {
            ((col / step) + 1) * step
        } else {
            tabs.iter().copied().find(|&t| t > col).unwrap_or(col + remaining)
        };
        let gap = next_stop - col;
        if gap <= remaining {
            out.push('\t');
            remaining -= gap;
            col = next_stop;
        } else {
            for _ in 0..remaining {
                out.push(' ');
            }
            remaining = 0;
        }
    }
    out
}

fn convert(line: &str, tabs: &[usize], step: usize, all_blanks: bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0usize;
    let mut pending = 0usize;
    let mut seen_non_blank = false;
    for ch in line.chars() {
        if ch == ' ' {
            pending += 1;
            continue;
        }
        if pending > 0 {
            if !seen_non_blank || all_blanks {
                out.push_str(&compact_spaces(col, pending, tabs, step));
            } else {
                for _ in 0..pending {
                    out.push(' ');
                }
            }
            col += pending;
            pending = 0;
        }
        out.push(ch);
        col += 1;
        if ch != '\t' {
            seen_non_blank = true;
        }
        if ch == '\t' {
            if step > 0 {
                col = ((col / step) + 1) * step;
            } else {
                col = tabs.iter().copied().find(|&t| t > col).unwrap_or(col + 1);
            }
        }
    }
    if pending > 0 {
        if !seen_non_blank || all_blanks {
            out.push_str(&compact_spaces(col, pending, tabs, step));
        } else {
            for _ in 0..pending {
                out.push(' ');
            }
        }
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut tabs: Vec<usize> = vec![8];
    let mut all_blanks = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-a" | "--all" => {
                all_blanks = true;
                i += 1;
            }
            "--first-only" => {
                all_blanks = false;
                i += 1;
            }
            "-t" | "--tabs" if i + 1 < args.len() => match parse_tabs(&args[i + 1]) {
                Some(t) => {
                    tabs = t;
                    all_blanks = true;
                    i += 2;
                }
                None => {
                    err("unexpand", &format!("invalid tabs: {}", args[i + 1]));
                    return 2;
                }
            },
            s if s.starts_with("-t") => match parse_tabs(&s[2..]) {
                Some(t) => {
                    tabs = t;
                    all_blanks = true;
                    i += 1;
                }
                None => {
                    err("unexpand", &format!("invalid tabs: {}", &s[2..]));
                    return 2;
                }
            },
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("unexpand", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let step = if tabs.len() == 1 { tabs[0] } else { 0 };
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
                    err_path("unexpand", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            let (body, tail) = if let Some(s) = line.strip_suffix('\r') {
                (s, "\r\n")
            } else {
                (line.as_str(), "\n")
            };
            let s = convert(body, &tabs, step, all_blanks);
            let _ = out.write_all(s.as_bytes());
            let _ = out.write_all(tail.as_bytes());
        }
        let _ = out.flush();
    }
    rc
}
