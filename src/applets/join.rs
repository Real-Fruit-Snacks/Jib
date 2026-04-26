//! `join` — relational join of two pre-sorted files.
//!
//! `-1 N`/`-2 N`/`-j N` field selectors (1-based; default 1). `-t CHAR`
//! field separator. `-a {1|2}` print unpaired lines from a file. `-e STR`
//! supply EMPTY for missing fields. `-i` case-insensitive. `-o FORMAT`
//! explicit output spec like `1.1,2.2` or `0`.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "join",
    help: "relational join of two pre-sorted files",
    aliases: &[],
    main,
};

fn split_fields(line: &str, sep: Option<char>) -> Vec<String> {
    match sep {
        Some(c) => line.split(c).map(String::from).collect(),
        None => line.split_whitespace().map(String::from).collect(),
    }
}

fn key_of(fields: &[String], idx: usize, ignore_case: bool) -> String {
    let s = fields.get(idx).cloned().unwrap_or_default();
    if ignore_case {
        s.to_lowercase()
    } else {
        s
    }
}

fn read_lines(path: &str) -> io::Result<Vec<String>> {
    let fh: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        Box::new(BufReader::new(File::open(path)?))
    };
    let mut out = Vec::new();
    for line in fh.lines() {
        out.push(line?);
    }
    Ok(out)
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut field1: usize = 0;
    let mut field2: usize = 0;
    let mut sep: Option<char> = None;
    let mut print_unpaired = [false; 3]; // 1 and 2 used
    let mut empty: Option<String> = None;
    let mut ignore_case = false;
    let mut output_spec: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        match a.as_str() {
            "-1" if i + 1 < args.len() => {
                field1 = args[i + 1].parse::<usize>().unwrap_or(1).saturating_sub(1);
                i += 2;
            }
            "-2" if i + 1 < args.len() => {
                field2 = args[i + 1].parse::<usize>().unwrap_or(1).saturating_sub(1);
                i += 2;
            }
            "-j" if i + 1 < args.len() => {
                let n = args[i + 1].parse::<usize>().unwrap_or(1).saturating_sub(1);
                field1 = n;
                field2 = n;
                i += 2;
            }
            "-t" if i + 1 < args.len() => {
                sep = args[i + 1].chars().next();
                i += 2;
            }
            "-a" if i + 1 < args.len() => {
                if let Ok(n) = args[i + 1].parse::<usize>() {
                    if n == 1 || n == 2 {
                        print_unpaired[n] = true;
                    }
                }
                i += 2;
            }
            "-e" if i + 1 < args.len() => {
                empty = Some(args[i + 1].clone());
                i += 2;
            }
            "-i" => {
                ignore_case = true;
                i += 1;
            }
            "-o" if i + 1 < args.len() => {
                output_spec = Some(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("join", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let rest = &args[i..];
    if rest.len() < 2 {
        err("join", "missing file operands");
        return 2;
    }
    let lines1 = match read_lines(&rest[0]) {
        Ok(v) => v,
        Err(e) => {
            err_path("join", &rest[0], &e);
            return 1;
        }
    };
    let lines2 = match read_lines(&rest[1]) {
        Ok(v) => v,
        Err(e) => {
            err_path("join", &rest[1], &e);
            return 1;
        }
    };

    let parsed1: Vec<Vec<String>> = lines1.iter().map(|l| split_fields(l, sep)).collect();
    let parsed2: Vec<Vec<String>> = lines2.iter().map(|l| split_fields(l, sep)).collect();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let join_sep = sep.unwrap_or(' ');

    let mut i = 0;
    let mut j = 0;
    while i < parsed1.len() && j < parsed2.len() {
        let k1 = key_of(&parsed1[i], field1, ignore_case);
        let k2 = key_of(&parsed2[j], field2, ignore_case);
        if k1 == k2 {
            // Find run of matching keys on both sides.
            let mut ie = i;
            while ie < parsed1.len() && key_of(&parsed1[ie], field1, ignore_case) == k1 {
                ie += 1;
            }
            let mut je = j;
            while je < parsed2.len() && key_of(&parsed2[je], field2, ignore_case) == k1 {
                je += 1;
            }
            for a in &parsed1[i..ie] {
                for b in &parsed2[j..je] {
                    let row = match &output_spec {
                        Some(spec) => render_spec(spec, a, b, field1, field2, empty.as_deref()),
                        None => render_default(a, b, field1, field2, join_sep),
                    };
                    let _ = writeln!(out, "{row}");
                }
            }
            i = ie;
            j = je;
        } else if k1 < k2 {
            if print_unpaired[1] {
                let _ = writeln!(out, "{}", parsed1[i].join(&join_sep.to_string()));
            }
            i += 1;
        } else {
            if print_unpaired[2] {
                let _ = writeln!(out, "{}", parsed2[j].join(&join_sep.to_string()));
            }
            j += 1;
        }
    }
    while i < parsed1.len() && print_unpaired[1] {
        let _ = writeln!(out, "{}", parsed1[i].join(&join_sep.to_string()));
        i += 1;
    }
    while j < parsed2.len() && print_unpaired[2] {
        let _ = writeln!(out, "{}", parsed2[j].join(&join_sep.to_string()));
        j += 1;
    }
    0
}

fn render_default(a: &[String], b: &[String], f1: usize, f2: usize, sep: char) -> String {
    let key = a.get(f1).cloned().unwrap_or_default();
    let mut parts: Vec<String> = vec![key];
    for (i, x) in a.iter().enumerate() {
        if i != f1 {
            parts.push(x.clone());
        }
    }
    for (i, x) in b.iter().enumerate() {
        if i != f2 {
            parts.push(x.clone());
        }
    }
    parts.join(&sep.to_string())
}

fn render_spec(
    spec: &str,
    a: &[String],
    b: &[String],
    f1: usize,
    f2: usize,
    empty: Option<&str>,
) -> String {
    spec.split(',')
        .map(|tok| {
            let tok = tok.trim();
            if tok == "0" {
                return a
                    .get(f1)
                    .cloned()
                    .or_else(|| b.get(f2).cloned())
                    .unwrap_or_default();
            }
            if let Some((file, idx)) = tok.split_once('.') {
                let idx = idx.parse::<usize>().unwrap_or(1).saturating_sub(1);
                let value = match file {
                    "1" => a.get(idx).cloned(),
                    "2" => b.get(idx).cloned(),
                    _ => None,
                };
                value.unwrap_or_else(|| empty.unwrap_or("").to_string())
            } else {
                String::new()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
