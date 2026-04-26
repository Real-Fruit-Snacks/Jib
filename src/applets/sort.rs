//! `sort` — sort lines of text files.
//!
//! Lexicographic by default. `-n` numeric, `-r` reverse, `-u` unique,
//! `-f` fold case, `-b` skip leading blanks, `-k SPEC` field-key sort,
//! `-t CHAR` separator, `-o FILE` output file. `-k` accepts e.g. `2`,
//! `2,3`, or `2n` for numeric.

use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "sort",
    help: "sort lines of text files",
    aliases: &[],
    main,
};

#[derive(Clone, Copy)]
struct KeySpec {
    start: usize,
    end: Option<usize>,
    numeric: bool,
}

fn parse_key_spec(s: &str) -> Option<KeySpec> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b',') {
        i += 1;
    }
    let field_part = &s[..i];
    let opts = &s[i..];
    if field_part.is_empty() {
        return None;
    }
    let (start, end): (usize, Option<usize>) = if field_part.contains(',') {
        let (a, b) = field_part.split_once(',').unwrap();
        (
            if a.is_empty() { 1 } else { a.parse().ok()? },
            if b.is_empty() {
                None
            } else {
                Some(b.parse().ok()?)
            },
        )
    } else {
        (field_part.parse().ok()?, None)
    };
    Some(KeySpec {
        start,
        end,
        numeric: opts.contains('n'),
    })
}

fn extract_field(line: &str, spec: KeySpec, sep: Option<char>) -> String {
    let fields: Vec<&str> = match sep {
        Some(c) => line.split(c).collect(),
        None => line.split_whitespace().collect(),
    };
    let joiner = match sep {
        Some(c) => c.to_string(),
        None => " ".to_string(),
    };
    let start = spec.start.saturating_sub(1).min(fields.len());
    let end = spec.end.unwrap_or(fields.len()).min(fields.len());
    if start >= end {
        return String::new();
    }
    fields[start..end].join(&joiner)
}

/// Numeric ordering: parse a leading optional sign + digits + optional
/// decimal. Non-numeric strings collate after numeric (matching Python's
/// `(0, val, s)` vs `(1, 0.0, s)` tuple key).
fn numeric_key(s: &str) -> (i32, f64, String) {
    let stripped = s.trim_start();
    let bytes = stripped.as_bytes();
    let mut end = 0;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut saw_digit = false;
    let mut saw_dot = false;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' => {
                saw_digit = true;
                end += 1;
            }
            b'.' if !saw_dot => {
                saw_dot = true;
                end += 1;
            }
            _ => break,
        }
    }
    if !saw_digit {
        return (1, 0.0, s.to_string());
    }
    match stripped[..end].parse::<f64>() {
        Ok(v) => (0, v, s.to_string()),
        Err(_) => (1, 0.0, s.to_string()),
    }
}

fn cmp_numeric_key(a: &(i32, f64, String), b: &(i32, f64, String)) -> Ordering {
    a.0.cmp(&b.0)
        .then(a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
        .then_with(|| a.2.cmp(&b.2))
}

fn take_value(flag: &str, args: &[String], idx: usize) -> Option<(String, usize)> {
    let a = &args[idx];
    if a.len() > flag.len() {
        return Some((a[flag.len()..].to_string(), idx + 1));
    }
    if idx + 1 >= args.len() {
        err("sort", &format!("{flag}: missing argument"));
        return None;
    }
    Some((args[idx + 1].clone(), idx + 2))
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut reverse = false;
    let mut numeric = false;
    let mut unique = false;
    let mut ignore_case = false;
    let mut ignore_leading_blanks = false;
    let mut separator: Option<char> = None;
    let mut output_path: Option<String> = None;
    let mut key_specs: Vec<KeySpec> = Vec::new();
    let mut files: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        if a == "-" || !a.starts_with('-') || a.len() < 2 {
            files.push(a);
            i += 1;
            continue;
        }
        if a == "-k" || a.starts_with("-k") {
            match take_value("-k", &args, i) {
                Some((v, ni)) => match parse_key_spec(&v) {
                    Some(spec) => {
                        key_specs.push(spec);
                        i = ni;
                        continue;
                    }
                    None => {
                        err("sort", &format!("invalid -k spec: '{v}'"));
                        return 2;
                    }
                },
                None => return 2,
            }
        }
        if a == "-t" || a.starts_with("-t") {
            match take_value("-t", &args, i) {
                Some((v, ni)) => {
                    if v.chars().count() != 1 {
                        err("sort", &format!("separator must be a single character: '{v}'"));
                        return 2;
                    }
                    separator = v.chars().next();
                    i = ni;
                    continue;
                }
                None => return 2,
            }
        }
        if a == "-o" || a.starts_with("-o") {
            match take_value("-o", &args, i) {
                Some((v, ni)) => {
                    output_path = Some(v);
                    i = ni;
                    continue;
                }
                None => return 2,
            }
        }
        for ch in a[1..].chars() {
            match ch {
                'r' => reverse = true,
                'n' => numeric = true,
                'u' => unique = true,
                'f' => ignore_case = true,
                'b' => ignore_leading_blanks = true,
                _ => {
                    err("sort", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    if files.is_empty() {
        files.push("-".to_string());
    }
    let mut lines: Vec<String> = Vec::new();
    let mut rc = 0;
    for f in &files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    err_path("sort", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        for line in reader.lines() {
            lines.push(line.unwrap_or_default());
        }
    }

    let base_transform = |s: &str| -> String {
        let mut out = if ignore_leading_blanks {
            s.trim_start().to_string()
        } else {
            s.to_string()
        };
        if ignore_case {
            out = out.to_lowercase();
        }
        out
    };

    enum Key {
        Str(String),
        Num((i32, f64, String)),
        Multi(Vec<Key>),
    }

    let key_fn = |s: &str| -> Key {
        if !key_specs.is_empty() {
            let parts: Vec<Key> = key_specs
                .iter()
                .map(|spec| {
                    let v = base_transform(&extract_field(s, *spec, separator));
                    if spec.numeric {
                        Key::Num(numeric_key(&v))
                    } else {
                        Key::Str(v)
                    }
                })
                .collect();
            Key::Multi(parts)
        } else if numeric {
            Key::Num(numeric_key(&base_transform(s)))
        } else {
            Key::Str(base_transform(s))
        }
    };

    fn cmp_key(a: &Key, b: &Key) -> Ordering {
        match (a, b) {
            (Key::Str(x), Key::Str(y)) => x.cmp(y),
            (Key::Num(x), Key::Num(y)) => cmp_numeric_key(x, y),
            (Key::Multi(xs), Key::Multi(ys)) => {
                for (x, y) in xs.iter().zip(ys.iter()) {
                    match cmp_key(x, y) {
                        Ordering::Equal => continue,
                        o => return o,
                    }
                }
                Ordering::Equal
            }
            _ => Ordering::Equal,
        }
    }

    let mut keyed: Vec<(Key, String)> = lines.into_iter().map(|s| (key_fn(&s), s)).collect();
    keyed.sort_by(|a, b| {
        let c = cmp_key(&a.0, &b.0);
        if reverse { c.reverse() } else { c }
    });

    let final_lines: Vec<String> = if unique {
        let mut out: Vec<(Key, String)> = Vec::with_capacity(keyed.len());
        for entry in keyed {
            if let Some(last) = out.last() {
                if cmp_key(&last.0, &entry.0) == Ordering::Equal {
                    continue;
                }
            }
            out.push(entry);
        }
        out.into_iter().map(|(_, s)| s).collect()
    } else {
        keyed.into_iter().map(|(_, s)| s).collect()
    };

    if let Some(p) = output_path {
        match File::create(&p) {
            Ok(mut fh) => {
                for line in &final_lines {
                    let _ = writeln!(fh, "{line}");
                }
            }
            Err(e) => {
                err_path("sort", &p, &e);
                return 1;
            }
        }
    } else {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        for line in &final_lines {
            let _ = writeln!(out, "{line}");
        }
    }
    rc
}
