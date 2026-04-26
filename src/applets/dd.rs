//! `dd` — convert and copy a file. Supports `if=`, `of=`, `bs=`, `count=`,
//! `skip=`, `seek=`, `conv=...`, `status=...`. The `conv=` operations
//! `notrunc`, `lcase`, `ucase`, `swab`, `noerror`, `excl`, `nocreat` are
//! recognised; the rest are accepted-and-ignored.

use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "dd",
    help: "convert and copy a file",
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
        'T' => (&s[..s.len() - 1], 1024u64.pow(4)),
        'P' => (&s[..s.len() - 1], 1024u64.pow(5)),
        c if c.is_ascii_digit() => (s, 1),
        _ => return None,
    };
    body.parse::<u64>().ok().map(|n| n.saturating_mul(mult))
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut bs: u64 = 512;
    let mut count: Option<u64> = None;
    let mut skip: u64 = 0;
    let mut seek: u64 = 0;
    let mut conv: Vec<String> = Vec::new();
    let mut status: String = "default".to_string();

    for a in args {
        if let Some((k, v)) = a.split_once('=') {
            match k {
                "if" => input_path = Some(v.to_string()),
                "of" => output_path = Some(v.to_string()),
                "bs" => match parse_size(v) {
                    Some(n) if n > 0 => bs = n,
                    _ => {
                        err("dd", &format!("invalid bs: {v}"));
                        return 2;
                    }
                },
                "count" => match v.parse() {
                    Ok(n) => count = Some(n),
                    Err(_) => {
                        err("dd", &format!("invalid count: {v}"));
                        return 2;
                    }
                },
                "skip" => match v.parse() {
                    Ok(n) => skip = n,
                    Err(_) => {
                        err("dd", &format!("invalid skip: {v}"));
                        return 2;
                    }
                },
                "seek" => match v.parse() {
                    Ok(n) => seek = n,
                    Err(_) => {
                        err("dd", &format!("invalid seek: {v}"));
                        return 2;
                    }
                },
                "conv" => conv = v.split(',').map(String::from).collect(),
                "status" => status = v.to_string(),
                _ => {}
            }
        }
    }

    let stdin_lock;
    let stdout_lock;
    let mut input: Box<dyn Read> = match &input_path {
        None => {
            stdin_lock = io::stdin();
            Box::new(stdin_lock.lock())
        }
        Some(p) => match std::fs::File::open(p) {
            Ok(f) => Box::new(f),
            Err(e) => {
                err("dd", &format!("{p}: {e}"));
                return 1;
            }
        },
    };
    let mut output: Box<dyn Write> = match &output_path {
        None => {
            stdout_lock = io::stdout();
            Box::new(stdout_lock.lock())
        }
        Some(p) => {
            let mut o = OpenOptions::new();
            o.write(true);
            if conv.iter().any(|c| c == "nocreat") {
                o.create(false);
            } else {
                o.create(true);
            }
            if conv.iter().any(|c| c == "excl") {
                o.create_new(true);
            }
            if !conv.iter().any(|c| c == "notrunc") {
                o.truncate(true);
            } else {
                o.truncate(false);
            }
            match o.open(p) {
                Ok(mut f) => {
                    if seek > 0 {
                        let _ = f.seek(SeekFrom::Start(seek * bs));
                    }
                    Box::new(f)
                }
                Err(e) => {
                    err("dd", &format!("{p}: {e}"));
                    return 1;
                }
            }
        }
    };

    // Apply skip on input.
    if skip > 0 {
        let mut buf = vec![0u8; bs as usize];
        let mut left = skip;
        while left > 0 {
            match input.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => left -= 1,
                Err(e) => {
                    err("dd", &e.to_string());
                    return 1;
                }
            }
        }
    }

    let mut buf = vec![0u8; bs as usize];
    let mut blocks_in: u64 = 0;
    let mut bytes_total: u64 = 0;
    loop {
        if let Some(c) = count {
            if blocks_in >= c {
                break;
            }
        }
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                if conv.iter().any(|c| c == "noerror") {
                    eprintln!("dd: read error: {e}");
                    continue;
                }
                err("dd", &e.to_string());
                return 1;
            }
        };
        let mut data = buf[..n].to_vec();
        if conv.iter().any(|c| c == "lcase") {
            data = data.iter().map(|b| b.to_ascii_lowercase()).collect();
        }
        if conv.iter().any(|c| c == "ucase") {
            data = data.iter().map(|b| b.to_ascii_uppercase()).collect();
        }
        if conv.iter().any(|c| c == "swab") {
            for chunk in data.chunks_exact_mut(2) {
                chunk.swap(0, 1);
            }
        }
        if let Err(e) = output.write_all(&data) {
            err("dd", &e.to_string());
            return 1;
        }
        blocks_in += 1;
        bytes_total += n as u64;
    }
    let _ = output.flush();
    if status != "none" {
        eprintln!("{blocks_in}+0 records in");
        eprintln!("{blocks_in}+0 records out");
        eprintln!("{bytes_total} bytes copied");
    }
    0
}
