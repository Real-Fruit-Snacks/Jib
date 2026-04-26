//! `od` — octal/hex/decimal/character dump.
//!
//! Format flags: `-c` chars, `-d` decimal shorts, `-o` octal shorts (POSIX
//! default), `-x` hex shorts. Address: `-A {d,o,x,n}`. Bytes: `-j skip`,
//! `-N max`, `-w width` (default 16).

use std::io::{self, Read, Write};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "od",
    help: "dump files in octal and other formats",
    aliases: &[],
    main,
};

#[derive(Clone, Copy)]
enum Fmt {
    Octal,
    Decimal,
    Hex,
    Char,
}

fn format_byte_char(b: u8) -> &'static str {
    const NAMES: [&str; 32] = [
        "nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", " bs", " ht",
        " nl", " vt", " ff", " cr", " so", " si", "dle", "dc1", "dc2", "dc3",
        "dc4", "nak", "syn", "etb", "can", " em", "sub", "esc", " fs", " gs",
        " rs", " us",
    ];
    if b < 32 {
        NAMES[b as usize]
    } else if b == 127 {
        "del"
    } else {
        ""
    }
}

fn render_byte_char(b: u8) -> String {
    if b < 32 || b == 127 {
        format_byte_char(b).to_string()
    } else {
        format!("  {}", b as char)
    }
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut fmt = Fmt::Octal;
    let mut addr_radix: char = 'o';
    let mut skip: u64 = 0;
    let mut max: Option<u64> = None;
    let mut width: usize = 16;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-c" => {
                fmt = Fmt::Char;
                i += 1;
            }
            "-d" => {
                fmt = Fmt::Decimal;
                i += 1;
            }
            "-o" => {
                fmt = Fmt::Octal;
                i += 1;
            }
            "-x" => {
                fmt = Fmt::Hex;
                i += 1;
            }
            "-A" if i + 1 < args.len() => {
                addr_radix = args[i + 1].chars().next().unwrap_or('o');
                i += 2;
            }
            s if s.starts_with("-A") && s.len() == 3 => {
                addr_radix = s.chars().nth(2).unwrap();
                i += 1;
            }
            "-j" if i + 1 < args.len() => {
                skip = args[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            "-N" if i + 1 < args.len() => {
                max = args[i + 1].parse().ok();
                i += 2;
            }
            "-w" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(16);
                i += 2;
            }
            _ => break,
        }
    }
    let files: Vec<String> = args[i..].to_vec();
    let stdin_lock;
    let mut data: Vec<u8> = Vec::new();
    if files.is_empty() {
        stdin_lock = io::stdin();
        let _ = stdin_lock.lock().read_to_end(&mut data);
    } else {
        for f in &files {
            match std::fs::read(f) {
                Ok(b) => data.extend_from_slice(&b),
                Err(e) => {
                    err("od", &format!("{f}: {e}"));
                    return 1;
                }
            }
        }
    }
    if (skip as usize) >= data.len() {
        return 0;
    }
    let mut data = &data[skip as usize..];
    if let Some(m) = max {
        let m = (m as usize).min(data.len());
        data = &data[..m];
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut offset = skip;
    while !data.is_empty() {
        let line_len = width.min(data.len());
        if addr_radix != 'n' {
            let prefix = match addr_radix {
                'd' => format!("{offset:07}"),
                'x' => format!("{offset:06x}"),
                _ => format!("{offset:07o}"),
            };
            let _ = write!(out, "{prefix} ");
        }
        match fmt {
            Fmt::Char => {
                for b in &data[..line_len] {
                    let _ = write!(out, "{:>4}", render_byte_char(*b));
                }
            }
            Fmt::Octal => {
                for chunk in data[..line_len].chunks(2) {
                    let v = chunk.iter().enumerate().fold(0u16, |acc, (i, b)| acc | ((*b as u16) << (i * 8)));
                    let _ = write!(out, " {v:06o}");
                }
            }
            Fmt::Decimal => {
                for chunk in data[..line_len].chunks(2) {
                    let v = chunk.iter().enumerate().fold(0u16, |acc, (i, b)| acc | ((*b as u16) << (i * 8)));
                    let _ = write!(out, " {v:5}");
                }
            }
            Fmt::Hex => {
                for chunk in data[..line_len].chunks(2) {
                    let v = chunk.iter().enumerate().fold(0u16, |acc, (i, b)| acc | ((*b as u16) << (i * 8)));
                    let _ = write!(out, " {v:04x}");
                }
            }
        }
        let _ = writeln!(out);
        offset += line_len as u64;
        data = &data[line_len..];
    }
    if addr_radix != 'n' {
        let prefix = match addr_radix {
            'd' => format!("{offset:07}"),
            'x' => format!("{offset:06x}"),
            _ => format!("{offset:07o}"),
        };
        let _ = writeln!(out, "{prefix}");
    }
    0
}
