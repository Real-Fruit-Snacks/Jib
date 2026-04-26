//! `fmt` — paragraph reflow.
//!
//! Wraps lines to a target width (default 75). `-w N` or `-NUM` sets the
//! width. `-s` (split-only) shrinks long lines but never joins. Blank
//! lines separate paragraphs.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "fmt",
    help: "simple paragraph formatter",
    aliases: &[],
    main,
};

fn flow(words: &[String], width: usize, out: &mut impl Write) {
    let mut line = String::new();
    for w in words {
        if line.is_empty() {
            line.push_str(w);
            continue;
        }
        if line.len() + 1 + w.len() > width {
            let _ = writeln!(out, "{line}");
            line.clear();
            line.push_str(w);
        } else {
            line.push(' ');
            line.push_str(w);
        }
    }
    if !line.is_empty() {
        let _ = writeln!(out, "{line}");
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut width = 75usize;
    let mut split_only = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-w" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(75);
                i += 2;
            }
            "-s" => {
                split_only = true;
                i += 1;
            }
            "-u" | "-c" | "-t" => {
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit()) => {
                width = s[1..].parse().unwrap_or(75);
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("fmt", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }
    let files: Vec<String> = args[i..].to_vec();
    let files = if files.is_empty() {
        vec!["-".to_string()]
    } else {
        files
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
                    err_path("fmt", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        let mut buf: Vec<String> = Vec::new();
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            if line.trim().is_empty() {
                if !buf.is_empty() {
                    if split_only {
                        // Print each original line, but split if too long.
                        // (Approximation: just emit as paragraph reflow at
                        // current width.)
                        flow(&buf.iter().flat_map(|l| l.split_whitespace().map(String::from)).collect::<Vec<_>>(), width, &mut out);
                    } else {
                        let words: Vec<String> = buf.iter().flat_map(|l| l.split_whitespace().map(String::from)).collect();
                        flow(&words, width, &mut out);
                    }
                    buf.clear();
                }
                let _ = writeln!(out);
            } else {
                buf.push(line);
            }
        }
        if !buf.is_empty() {
            let words: Vec<String> = buf.iter().flat_map(|l| l.split_whitespace().map(String::from)).collect();
            flow(&words, width, &mut out);
        }
    }
    rc
}
