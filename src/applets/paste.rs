//! `paste` — merge corresponding lines of files.
//!
//! Default: round-robin one line per file with TAB separator. `-d STR`
//! cycles through the chars of STR as separators. `-s` ("serial") joins
//! all lines from each file into one output line per file.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "paste",
    help: "merge corresponding lines of files",
    aliases: &[],
    main,
};

fn join_with_delims(parts: &[String], delims: &[char]) -> String {
    let mut out = parts[0].clone();
    for (j, p) in parts.iter().skip(1).enumerate() {
        out.push(delims[j % delims.len()]);
        out.push_str(p);
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut delims: Vec<char> = vec!['\t'];
    let mut serial = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-d" | "--delimiters" if i + 1 < args.len() => {
                let d: Vec<char> = args[i + 1].chars().collect();
                delims = if d.is_empty() { vec!['\t'] } else { d };
                i += 2;
            }
            s if s.starts_with("-d") && s.len() > 2 => {
                let d: Vec<char> = s[2..].chars().collect();
                delims = if d.is_empty() { vec!['\t'] } else { d };
                i += 1;
            }
            "-s" | "--serial" => {
                serial = true;
                i += 1;
            }
            "-z" | "--zero-terminated" => {
                err("paste", "-z is not supported");
                return 2;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("paste", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let raw: Vec<String> = args[i..].to_vec();
    let files: Vec<String> = if raw.is_empty() {
        vec!["-".to_string()]
    } else {
        raw
    };

    // Eagerly read all input. paste isn't usually streamed and this matches
    // the Python applet's behavior (which holds files open and reads
    // line-by-line in lockstep).
    let mut lists: Vec<Vec<String>> = Vec::new();
    for f in &files {
        let mut buf: Vec<String> = Vec::new();
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    err_path("paste", f, &e);
                    return 1;
                }
            }
        };
        for line in reader.lines() {
            buf.push(line.unwrap_or_default());
        }
        lists.push(buf);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if serial {
        for lines in &lists {
            if !lines.is_empty() {
                let _ = writeln!(out, "{}", join_with_delims(lines, &delims));
            }
        }
    } else {
        let max_rows = lists.iter().map(|v| v.len()).max().unwrap_or(0);
        for r in 0..max_rows {
            let row: Vec<String> = lists
                .iter()
                .map(|v| v.get(r).cloned().unwrap_or_default())
                .collect();
            let _ = writeln!(out, "{}", join_with_delims(&row, &delims));
        }
    }
    0
}
