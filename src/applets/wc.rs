//! `wc` — count newlines, words, and bytes (or chars) per file.
//!
//! With no flags: prints lines, words, bytes. Flag selectors `-l`, `-w`,
//! `-c` (bytes), `-m` (chars). Multiple files produce a `total` row.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "wc",
    help: "print newline, word, and byte counts for each file",
    aliases: &[],
    main,
};

#[derive(Default, Clone, Copy)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
    chars: u64,
}

impl std::ops::AddAssign for Counts {
    fn add_assign(&mut self, rhs: Self) {
        self.lines += rhs.lines;
        self.words += rhs.words;
        self.bytes += rhs.bytes;
        self.chars += rhs.chars;
    }
}

fn count_one<R: Read>(r: R) -> Counts {
    let mut br = BufReader::new(r);
    let mut buf: Vec<u8> = Vec::new();
    let mut c = Counts::default();
    loop {
        buf.clear();
        let n = match br.read_until(b'\n', &mut buf) {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        c.bytes += n as u64;
        if buf.ends_with(b"\n") {
            c.lines += 1;
        }
        // chars: best-effort utf-8 with replacement.
        let s = String::from_utf8_lossy(&buf);
        c.chars += s.chars().count() as u64;
        c.words += s.split_whitespace().count() as u64;
    }
    c
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut want_lines = false;
    let mut want_words = false;
    let mut want_bytes = false;
    let mut want_chars = false;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        if a == "-" || !a.starts_with('-') || a.len() < 2 {
            files.push(a.clone());
        } else {
            for ch in a[1..].chars() {
                match ch {
                    'l' => want_lines = true,
                    'w' => want_words = true,
                    'c' => want_bytes = true,
                    'm' => want_chars = true,
                    _ => {
                        err("wc", &format!("invalid option: -{ch}"));
                        return 2;
                    }
                }
            }
        }
        i += 1;
    }

    if !(want_lines || want_words || want_bytes || want_chars) {
        want_lines = true;
        want_words = true;
        want_bytes = true;
    }

    if files.is_empty() {
        files.push("-".to_string());
    }

    let mut totals = Counts::default();
    let mut rc = 0;
    let mut results: Vec<(Counts, String)> = Vec::new();

    for f in &files {
        let counts = if f == "-" {
            count_one(io::stdin().lock())
        } else {
            match File::open(f) {
                Ok(fh) => count_one(fh),
                Err(e) => {
                    err_path("wc", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        results.push((counts, f.clone()));
        totals += counts;
    }

    let format_row = |c: &Counts, label: &str| -> String {
        let mut parts: Vec<String> = Vec::new();
        if want_lines {
            parts.push(format!("{:>7}", c.lines));
        }
        if want_words {
            parts.push(format!("{:>7}", c.words));
        }
        if want_bytes {
            parts.push(format!("{:>7}", c.bytes));
        }
        if want_chars {
            parts.push(format!("{:>7}", c.chars));
        }
        if !label.is_empty() && label != "-" {
            parts.push(label.to_string());
        }
        parts.join(" ")
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for (c, label) in &results {
        let _ = writeln!(out, "{}", format_row(c, label));
    }
    if results.len() > 1 {
        let _ = writeln!(out, "{}", format_row(&totals, "total"));
    }
    rc
}
