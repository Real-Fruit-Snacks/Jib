//! `cut` — remove sections from each line of files.
//!
//! `-d DELIM -f LIST` selects fields; `-c LIST` selects character columns.
//! `-s` suppresses lines without the delimiter (field mode only).

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "cut",
    help: "remove sections from each line of files",
    aliases: &[],
    main,
};

#[derive(Clone, Copy)]
struct Range {
    start: usize, // 1-based
    end: usize,   // inclusive; 0 means "open" (to end-of-line)
}

fn parse_list(s: &str) -> Result<Vec<Range>, String> {
    let mut out = Vec::new();
    for raw in s.split(',') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        let (start, end): (usize, usize) = if let Some(idx) = part.find('-') {
            let (a, b) = part.split_at(idx);
            let b = &b[1..];
            let s = if a.is_empty() {
                1
            } else {
                a.parse().map_err(|_| format!("invalid range: '{part}'"))?
            };
            let e = if b.is_empty() {
                0
            } else {
                b.parse().map_err(|_| format!("invalid range: '{part}'"))?
            };
            (s, e)
        } else {
            let n: usize = part.parse().map_err(|_| format!("invalid position: '{part}'"))?;
            (n, n)
        };
        if start < 1 {
            return Err(format!("position must be >= 1: {part}"));
        }
        out.push(Range { start, end });
    }
    if out.is_empty() {
        return Err("empty list".to_string());
    }
    Ok(out)
}

fn positions(n: usize, ranges: &[Range]) -> Vec<usize> {
    let mut sel: BTreeSet<usize> = BTreeSet::new();
    for r in ranges {
        let stop = if r.end == 0 { n } else { r.end.min(n) };
        for p in r.start..=stop {
            sel.insert(p);
        }
    }
    sel.into_iter().collect()
}

fn take_value(flag: &str, args: &[String], idx: usize) -> Option<(String, usize)> {
    let a = &args[idx];
    if a.len() > flag.len() {
        return Some((a[flag.len()..].to_string(), idx + 1));
    }
    if idx + 1 >= args.len() {
        err("cut", &format!("{flag}: missing argument"));
        return None;
    }
    Some((args[idx + 1].clone(), idx + 2))
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut delim = "\t".to_string();
    let mut suppress = false;
    let mut mode: Option<char> = None;
    let mut list_spec: Option<String> = None;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        if a == "-d" || a.starts_with("-d") {
            match take_value("-d", &args, i) {
                Some((v, ni)) => {
                    delim = v;
                    i = ni;
                    continue;
                }
                None => return 2,
            }
        }
        if a == "-f" || a.starts_with("-f") {
            match take_value("-f", &args, i) {
                Some((v, ni)) => {
                    mode = Some('f');
                    list_spec = Some(v);
                    i = ni;
                    continue;
                }
                None => return 2,
            }
        }
        if a == "-c" || a.starts_with("-c") {
            match take_value("-c", &args, i) {
                Some((v, ni)) => {
                    mode = Some('c');
                    list_spec = Some(v);
                    i = ni;
                    continue;
                }
                None => return 2,
            }
        }
        if a == "-s" {
            suppress = true;
            i += 1;
            continue;
        }
        if a == "-n" {
            i += 1;
            continue;
        }
        if a == "-" || !a.starts_with('-') {
            files.push(a);
            i += 1;
            continue;
        }
        err("cut", &format!("invalid option: {a}"));
        return 2;
    }

    let (mode, list_spec) = match (mode, list_spec) {
        (Some(m), Some(l)) => (m, l),
        _ => {
            err("cut", "must specify -f or -c");
            return 2;
        }
    };
    let ranges = match parse_list(&list_spec) {
        Ok(r) => r,
        Err(e) => {
            err("cut", &format!("invalid list '{list_spec}': {e}"));
            return 2;
        }
    };
    if files.is_empty() {
        files.push("-".to_string());
    }

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
                    err_path("cut", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            if mode == 'f' {
                if !line.contains(&delim) {
                    if suppress {
                        continue;
                    }
                    let _ = writeln!(out, "{line}");
                    continue;
                }
                let fields: Vec<&str> = line.split(&delim as &str).collect();
                let pos = positions(fields.len(), &ranges);
                let parts: Vec<&str> = pos.iter().map(|&p| fields[p - 1]).collect();
                let _ = writeln!(out, "{}", parts.join(&delim));
            } else {
                // Character mode: index by chars (UTF-8 safe).
                let chars: Vec<char> = line.chars().collect();
                let pos = positions(chars.len(), &ranges);
                let s: String = pos.iter().map(|&p| chars[p - 1]).collect();
                let _ = writeln!(out, "{s}");
            }
        }
    }
    rc
}
