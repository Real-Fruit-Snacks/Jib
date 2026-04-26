//! `diff` — line-by-line file comparison.
//!
//! Defaults to a normal (ed-style) diff. `-u`/`-U N` for unified diff,
//! `-q` for "files differ" only, `-i` case-insensitive, `-w` ignore all
//! whitespace, `-B` ignore blank lines.

use std::fs::File;
use std::io::{self, BufRead, BufReader};

use similar::{ChangeTag, TextDiff};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "diff",
    help: "compare files line by line",
    aliases: &[],
    main,
};

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

fn normalize(s: &str, ignore_case: bool, ignore_ws: bool) -> String {
    let mut s = s.to_string();
    if ignore_case {
        s = s.to_lowercase();
    }
    if ignore_ws {
        s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    s
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut unified: Option<usize> = None;
    let mut brief = false;
    let mut ignore_case = false;
    let mut ignore_ws = false;
    let mut ignore_blank = false;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            i += 1;
            break;
        }
        match a.as_str() {
            "-u" => {
                unified = Some(3);
                i += 1;
            }
            "-U" if i + 1 < args.len() => {
                unified = args[i + 1].parse().ok();
                i += 2;
            }
            "-q" => {
                brief = true;
                i += 1;
            }
            "-i" => {
                ignore_case = true;
                i += 1;
            }
            "-w" => {
                ignore_ws = true;
                i += 1;
            }
            "-B" => {
                ignore_blank = true;
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("diff", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let rest = &args[i..];
    if rest.len() < 2 {
        err("diff", "missing file operands");
        return 2;
    }
    let f1 = rest[0].clone();
    let f2 = rest[1].clone();
    let lines1 = match read_lines(&f1) {
        Ok(v) => v,
        Err(e) => {
            err_path("diff", &f1, &e);
            return 2;
        }
    };
    let lines2 = match read_lines(&f2) {
        Ok(v) => v,
        Err(e) => {
            err_path("diff", &f2, &e);
            return 2;
        }
    };

    // Apply normalization for comparison only.
    let norm1: Vec<String> = lines1
        .iter()
        .filter(|l| !ignore_blank || !l.trim().is_empty())
        .map(|l| normalize(l, ignore_case, ignore_ws))
        .collect();
    let norm2: Vec<String> = lines2
        .iter()
        .filter(|l| !ignore_blank || !l.trim().is_empty())
        .map(|l| normalize(l, ignore_case, ignore_ws))
        .collect();

    if norm1 == norm2 {
        return 0;
    }
    if brief {
        println!("Files {f1} and {f2} differ");
        return 1;
    }

    let s1: Vec<&str> = norm1.iter().map(String::as_str).collect();
    let s2: Vec<&str> = norm2.iter().map(String::as_str).collect();
    let diff = TextDiff::from_slices(&s1, &s2);
    if let Some(ctx) = unified {
        println!("--- {f1}");
        println!("+++ {f2}");
        for hunk in diff.unified_diff().context_radius(ctx).iter_hunks() {
            print!("{hunk}");
        }
    } else {
        for change in diff.iter_all_changes() {
            let prefix = match change.tag() {
                ChangeTag::Equal => " ",
                ChangeTag::Insert => "> ",
                ChangeTag::Delete => "< ",
            };
            print!("{prefix}{change}");
        }
    }
    1
}
