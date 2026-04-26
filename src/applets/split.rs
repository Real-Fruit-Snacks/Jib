//! `split` — split a file into pieces.
//!
//! Defaults to 1000 lines per chunk; `-l N` sets line count, `-b N` sets
//! byte count. `-d` produces numeric suffixes; `-a N` sets suffix length
//! (default 2). The third positional is the prefix (default `x`).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "split",
    help: "split a file into pieces",
    aliases: &[],
    main,
};

fn parse_size(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    let last = s.chars().last().unwrap();
    let (body, mult): (&str, u64) = match last.to_ascii_uppercase() {
        'K' => (&s[..s.len() - 1], 1024),
        'M' => (&s[..s.len() - 1], 1024 * 1024),
        'G' => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        c if c.is_ascii_digit() => (s, 1),
        _ => return None,
    };
    body.parse::<u64>().ok().map(|n| n.saturating_mul(mult))
}

fn alpha_suffix(n: usize, len: usize) -> String {
    let mut s = vec!['a'; len];
    let mut x = n;
    for i in (0..len).rev() {
        s[i] = (b'a' + (x % 26) as u8) as char;
        x /= 26;
    }
    s.into_iter().collect()
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut line_count: Option<u64> = None;
    let mut byte_count: Option<u64> = None;
    let mut numeric = false;
    let mut suffix_len = 2usize;
    let mut additional: String = String::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-l" if i + 1 < args.len() => {
                line_count = args[i + 1].parse().ok();
                i += 2;
            }
            "-b" if i + 1 < args.len() => {
                byte_count = parse_size(&args[i + 1]);
                i += 2;
            }
            "-d" => {
                numeric = true;
                i += 1;
            }
            "-a" if i + 1 < args.len() => {
                suffix_len = args[i + 1].parse().unwrap_or(2);
                i += 2;
            }
            s if s.starts_with("--additional-suffix=") => {
                additional = s["--additional-suffix=".len()..].to_string();
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("split", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }
    if line_count.is_none() && byte_count.is_none() {
        line_count = Some(1000);
    }
    let positional: Vec<String> = args[i..].to_vec();
    let input = positional.first().cloned().unwrap_or_else(|| "-".to_string());
    let prefix = positional.get(1).cloned().unwrap_or_else(|| "x".to_string());

    let make_name = |idx: usize| -> String {
        let suf = if numeric {
            format!("{idx:0>width$}", width = suffix_len)
        } else {
            alpha_suffix(idx, suffix_len)
        };
        format!("{prefix}{suf}{additional}")
    };

    if let Some(n) = byte_count {
        let mut reader: Box<dyn Read> = if input == "-" {
            Box::new(io::stdin().lock())
        } else {
            match File::open(&input) {
                Ok(fh) => Box::new(fh),
                Err(e) => {
                    err_path("split", &input, &e);
                    return 1;
                }
            }
        };
        let mut buf = vec![0u8; n.min(64 * 1024) as usize];
        let mut idx = 0usize;
        loop {
            let mut wrote = 0u64;
            let name = make_name(idx);
            let mut wh = match File::create(&name) {
                Ok(f) => f,
                Err(e) => {
                    err_path("split", &name, &e);
                    return 1;
                }
            };
            while wrote < n {
                let want = (n - wrote).min(buf.len() as u64) as usize;
                let r = reader.read(&mut buf[..want]).unwrap_or(0);
                if r == 0 {
                    if wrote == 0 {
                        let _ = std::fs::remove_file(&name);
                        return 0;
                    }
                    return 0;
                }
                let _ = wh.write_all(&buf[..r]);
                wrote += r as u64;
            }
            idx += 1;
        }
    }

    let n = line_count.unwrap();
    let reader: Box<dyn BufRead> = if input == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        match File::open(&input) {
            Ok(fh) => Box::new(BufReader::new(fh)),
            Err(e) => {
                err_path("split", &input, &e);
                return 1;
            }
        }
    };
    let mut idx = 0usize;
    let mut count = 0u64;
    let mut wh: Option<File> = None;
    let mut buf = Vec::new();
    let mut br = reader;
    loop {
        buf.clear();
        let read = match br.read_until(b'\n', &mut buf) {
            Ok(n) => n,
            Err(e) => {
                err("split", &e.to_string());
                return 1;
            }
        };
        if read == 0 {
            break;
        }
        if wh.is_none() || count >= n {
            count = 0;
            let name = make_name(idx);
            wh = match File::create(&name) {
                Ok(f) => Some(f),
                Err(e) => {
                    err_path("split", &name, &e);
                    return 1;
                }
            };
            idx += 1;
        }
        let _ = wh.as_mut().unwrap().write_all(&buf);
        count += 1;
    }
    0
}
