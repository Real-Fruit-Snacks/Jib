//! `hexdump` — canonical hex+ASCII dump (`-C`), 2-byte word formats.

use std::io::{self, Read, Write};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "hexdump",
    help: "ASCII, decimal, hexadecimal, octal dump",
    aliases: &[],
    main,
};

#[derive(Clone, Copy)]
enum Mode {
    Canonical,
    HexShorts,
    DecShorts,
    OctShorts,
    OctBytes,
    CharBytes,
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut mode = Mode::HexShorts;
    let mut skip: u64 = 0;
    let mut max: Option<u64> = None;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-C" => {
                mode = Mode::Canonical;
                i += 1;
            }
            "-x" => {
                mode = Mode::HexShorts;
                i += 1;
            }
            "-d" => {
                mode = Mode::DecShorts;
                i += 1;
            }
            "-o" => {
                mode = Mode::OctShorts;
                i += 1;
            }
            "-b" => {
                mode = Mode::OctBytes;
                i += 1;
            }
            "-c" => {
                mode = Mode::CharBytes;
                i += 1;
            }
            "-s" if i + 1 < args.len() => {
                skip = args[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            "-n" if i + 1 < args.len() => {
                max = args[i + 1].parse().ok();
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
                    err("hexdump", &format!("{f}: {e}"));
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
    let row = match mode {
        Mode::Canonical => 16,
        _ => 16,
    };
    while !data.is_empty() {
        let n = row.min(data.len());
        let _ = write!(out, "{offset:08x} ");
        match mode {
            Mode::Canonical => {
                let bytes = &data[..n];
                for (i, b) in bytes.iter().enumerate() {
                    if i == 8 {
                        let _ = write!(out, " ");
                    }
                    let _ = write!(out, " {b:02x}");
                }
                for _ in bytes.len()..16 {
                    let _ = write!(out, "   ");
                }
                let _ = write!(out, "  |");
                for b in bytes {
                    let c = if (32..127).contains(b) {
                        *b as char
                    } else {
                        '.'
                    };
                    let _ = write!(out, "{c}");
                }
                let _ = writeln!(out, "|");
            }
            Mode::HexShorts | Mode::DecShorts | Mode::OctShorts => {
                for chunk in data[..n].chunks(2) {
                    let v = chunk
                        .iter()
                        .enumerate()
                        .fold(0u16, |acc, (j, b)| acc | ((*b as u16) << (j * 8)));
                    match mode {
                        Mode::HexShorts => {
                            let _ = write!(out, " {v:04x}");
                        }
                        Mode::DecShorts => {
                            let _ = write!(out, " {v:5}");
                        }
                        Mode::OctShorts => {
                            let _ = write!(out, " {v:06o}");
                        }
                        _ => {}
                    }
                }
                let _ = writeln!(out);
            }
            Mode::OctBytes => {
                for b in &data[..n] {
                    let _ = write!(out, " {b:03o}");
                }
                let _ = writeln!(out);
            }
            Mode::CharBytes => {
                for b in &data[..n] {
                    let s = if *b < 32 || *b == 127 {
                        format!("{b:03o}")
                    } else {
                        format!("  {}", *b as char)
                    };
                    let _ = write!(out, " {s}");
                }
                let _ = writeln!(out);
            }
        }
        offset += n as u64;
        data = &data[n..];
    }
    let _ = writeln!(out, "{offset:08x}");
    0
}
