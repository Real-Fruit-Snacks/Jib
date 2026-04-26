//! `uniq` — report or omit repeated *adjacent* lines.
//!
//! `-c` count, `-d` only duplicates, `-u` only uniques, `-i` case-insens.
//! `-f N` skip first N fields, `-s N` skip first N chars, `-w N` compare
//! first N chars of the remainder.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "uniq",
    help: "report or omit repeated adjacent lines",
    aliases: &[],
    main,
};

fn key(line: &str, skip_fields: usize, skip_chars: usize, width: Option<usize>, ic: bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let mut skipped = 0usize;
    while skipped < skip_fields && i < chars.len() {
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        while i < chars.len() && !(chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        skipped += 1;
    }
    let mut s: String = chars[i..].iter().collect();
    if skip_chars > 0 && skip_chars <= s.chars().count() {
        s = s.chars().skip(skip_chars).collect();
    }
    if let Some(w) = width {
        s = s.chars().take(w).collect();
    }
    if ic {
        s = s.to_lowercase();
    }
    s
}

fn take_value(flag: &str, args: &[String], idx: usize) -> Option<(String, usize)> {
    let a = &args[idx];
    if a.len() > flag.len() {
        return Some((a[flag.len()..].to_string(), idx + 1));
    }
    if idx + 1 >= args.len() {
        err("uniq", &format!("{flag}: missing argument"));
        return None;
    }
    Some((args[idx + 1].clone(), idx + 2))
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut count = false;
    let mut only_dup = false;
    let mut only_unique = false;
    let mut ignore_case = false;
    let mut skip_fields: usize = 0;
    let mut skip_chars: usize = 0;
    let mut width: Option<usize> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-c" => {
                count = true;
                i += 1;
            }
            "-d" => {
                only_dup = true;
                i += 1;
            }
            "-u" => {
                only_unique = true;
                i += 1;
            }
            "-i" => {
                ignore_case = true;
                i += 1;
            }
            s if s.starts_with("-f") => match take_value("-f", &args, i) {
                Some((v, ni)) => match v.parse() {
                    Ok(n) => {
                        skip_fields = n;
                        i = ni;
                    }
                    Err(_) => {
                        err("uniq", &format!("-f: invalid value '{v}'"));
                        return 2;
                    }
                },
                None => return 2,
            },
            s if s.starts_with("-s") => match take_value("-s", &args, i) {
                Some((v, ni)) => match v.parse() {
                    Ok(n) => {
                        skip_chars = n;
                        i = ni;
                    }
                    Err(_) => {
                        err("uniq", &format!("-s: invalid value '{v}'"));
                        return 2;
                    }
                },
                None => return 2,
            },
            s if s.starts_with("-w") => match take_value("-w", &args, i) {
                Some((v, ni)) => match v.parse() {
                    Ok(n) => {
                        width = Some(n);
                        i = ni;
                    }
                    Err(_) => {
                        err("uniq", &format!("-w: invalid value '{v}'"));
                        return 2;
                    }
                },
                None => return 2,
            },
            s if s.starts_with('-') && s != "-" && s.len() > 1 && s[1..].chars().all(|c| c.is_ascii_digit()) => {
                skip_fields = s[1..].parse().unwrap_or(0);
                i += 1;
            }
            _ => break,
        }
    }

    let positional: Vec<String> = args[i..].to_vec();
    let input = positional.first().cloned().unwrap_or_else(|| "-".to_string());
    let output = positional.get(1).cloned().unwrap_or_else(|| "-".to_string());

    let reader: Box<dyn BufRead> = if input == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        match File::open(&input) {
            Ok(fh) => Box::new(BufReader::new(fh)),
            Err(e) => {
                err_path("uniq", &input, &e);
                return 1;
            }
        }
    };
    let mut writer: Box<dyn Write> = if output == "-" {
        Box::new(io::stdout().lock())
    } else {
        match File::create(&output) {
            Ok(fh) => Box::new(fh),
            Err(e) => {
                err_path("uniq", &output, &e);
                return 1;
            }
        }
    };

    let emit = |out: &mut dyn Write, line: &str, cnt: usize, count: bool, dup_only: bool, uniq_only: bool| {
        if dup_only && cnt < 2 {
            return;
        }
        if uniq_only && cnt != 1 {
            return;
        }
        if count {
            let _ = writeln!(out, "{cnt:>7} {line}");
        } else {
            let _ = writeln!(out, "{line}");
        }
    };

    let mut prev_line: Option<String> = None;
    let mut prev_key: Option<String> = None;
    let mut cnt: usize = 0;
    for line in reader.lines() {
        let line = line.unwrap_or_default();
        let k = key(&line, skip_fields, skip_chars, width, ignore_case);
        match prev_key.as_ref() {
            None => {
                prev_line = Some(line);
                prev_key = Some(k);
                cnt = 1;
            }
            Some(pk) if pk == &k => {
                cnt += 1;
            }
            _ => {
                if let Some(pl) = prev_line.as_deref() {
                    emit(writer.as_mut(), pl, cnt, count, only_dup, only_unique);
                }
                prev_line = Some(line);
                prev_key = Some(k);
                cnt = 1;
            }
        }
    }
    if let Some(pl) = prev_line.as_deref() {
        emit(writer.as_mut(), pl, cnt, count, only_dup, only_unique);
    }
    0
}
