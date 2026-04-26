//! `comm` — compare two sorted files line by line.
//!
//! Three columns: lines only in file 1, only in file 2, in both. `-1`,
//! `-2`, `-3` (and combinations like `-12`, `-23`, `-123`) suppress those
//! columns. `--output-delimiter STR` swaps the default TAB.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "comm",
    help: "compare two sorted files line by line",
    aliases: &[],
    main,
};

fn open(path: &str) -> io::Result<Box<dyn BufRead>> {
    if path == "-" {
        Ok(Box::new(BufReader::new(io::stdin().lock())))
    } else {
        Ok(Box::new(BufReader::new(File::open(path)?)))
    }
}

fn read_line(r: &mut dyn BufRead) -> Option<String> {
    let mut s = String::new();
    match r.read_line(&mut s) {
        Ok(0) => None,
        Ok(_) => {
            // Strip trailing newline (and CR) for comparison.
            while s.ends_with('\n') || s.ends_with('\r') {
                s.pop();
            }
            Some(s)
        }
        Err(_) => None,
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut suppress = [false; 4]; // 1, 2, 3 used
    let mut sep = "\t".to_string();
    let mut check_order = true;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a.starts_with('-')
            && a.len() >= 2
            && a[1..].chars().all(|c| c.is_ascii_digit())
            && a[1..].chars().all(|c| matches!(c, '1' | '2' | '3'))
        {
            for c in a[1..].chars() {
                suppress[c.to_digit(10).unwrap() as usize] = true;
            }
            i += 1;
            continue;
        }
        match a.as_str() {
            "--nocheck-order" => {
                check_order = false;
                i += 1;
            }
            "--check-order" => {
                check_order = true;
                i += 1;
            }
            "--output-delimiter" if i + 1 < args.len() => {
                sep = args[i + 1].clone();
                i += 2;
            }
            s if s.starts_with("--output-delimiter=") => {
                sep = s["--output-delimiter=".len()..].to_string();
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("comm", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let rest: Vec<String> = args[i..].to_vec();
    if rest.len() != 2 {
        err("comm", "two file operands required");
        return 2;
    }
    let mut h1 = match open(&rest[0]) {
        Ok(h) => h,
        Err(e) => {
            err_path("comm", &rest[0], &e);
            return 1;
        }
    };
    let mut h2 = match open(&rest[1]) {
        Ok(h) => h,
        Err(e) => {
            err_path("comm", &rest[1], &e);
            return 1;
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;
    let mut a_line = read_line(&mut h1);
    let mut b_line = read_line(&mut h2);
    let mut prev_a: Option<String> = None;
    let mut prev_b: Option<String> = None;

    while a_line.is_some() || b_line.is_some() {
        if check_order {
            if let (Some(p), Some(c)) = (prev_a.as_ref(), a_line.as_ref()) {
                if c < p {
                    err("comm", "file 1 is not in sorted order");
                    rc = 1;
                }
            }
            if let (Some(p), Some(c)) = (prev_b.as_ref(), b_line.as_ref()) {
                if c < p {
                    err("comm", "file 2 is not in sorted order");
                    rc = 1;
                }
            }
        }
        let (col, line): (usize, String) = match (&a_line, &b_line) {
            (None, Some(b)) => {
                let l = b.clone();
                prev_b = b_line.take();
                b_line = read_line(&mut h2);
                (2, l)
            }
            (Some(a), None) => {
                let l = a.clone();
                prev_a = a_line.take();
                a_line = read_line(&mut h1);
                (1, l)
            }
            (Some(a), Some(b)) if a == b => {
                let l = a.clone();
                prev_a = a_line.take();
                prev_b = b_line.take();
                a_line = read_line(&mut h1);
                b_line = read_line(&mut h2);
                (3, l)
            }
            (Some(a), Some(b)) if a < b => {
                let l = a.clone();
                prev_a = a_line.take();
                a_line = read_line(&mut h1);
                (1, l)
            }
            (Some(_), Some(b)) => {
                let l = b.clone();
                prev_b = b_line.take();
                b_line = read_line(&mut h2);
                (2, l)
            }
            (None, None) => break,
        };
        if suppress[col] {
            continue;
        }
        let shown_below = (1..col).filter(|&c| !suppress[c]).count();
        let prefix: String = sep.repeat(shown_below);
        let _ = writeln!(out, "{prefix}{line}");
    }
    rc
}
