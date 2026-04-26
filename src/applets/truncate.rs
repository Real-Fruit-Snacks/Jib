//! `truncate` — set a file's length.
//!
//! Size operators on `--size`: `+N` extend, `-N` shrink, `<N` cap, `>N`
//! floor, `/N` round down to multiple, `%N` round up. Suffixes K/M/G/T/P
//! use 1024 bases.

use std::fs::OpenOptions;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "truncate",
    help: "shrink or extend the size of a file to the specified size",
    aliases: &[],
    main,
};

#[derive(Clone, Copy)]
enum Op {
    Set,
    Add,
    Sub,
    Cap,
    Floor,
    RoundDown,
    RoundUp,
}

fn parse_size(s: &str) -> Option<(Op, u64)> {
    if s.is_empty() {
        return None;
    }
    let (op, rest): (Op, &str) = match s.as_bytes()[0] {
        b'+' => (Op::Add, &s[1..]),
        b'-' => (Op::Sub, &s[1..]),
        b'<' => (Op::Cap, &s[1..]),
        b'>' => (Op::Floor, &s[1..]),
        b'/' => (Op::RoundDown, &s[1..]),
        b'%' => (Op::RoundUp, &s[1..]),
        _ => (Op::Set, s),
    };
    if rest.is_empty() {
        return None;
    }
    let (body, mult) = match rest.chars().last().unwrap() {
        c @ ('K' | 'M' | 'G' | 'T' | 'P' | 'k' | 'm' | 'g' | 't' | 'p') => {
            let m: u64 = match c.to_ascii_uppercase() {
                'K' => 1u64 << 10,
                'M' => 1u64 << 20,
                'G' => 1u64 << 30,
                'T' => 1u64 << 40,
                'P' => 1u64 << 50,
                _ => 1,
            };
            (&rest[..rest.len() - 1], m)
        }
        _ => (rest, 1u64),
    };
    let n: u64 = body.parse().ok()?;
    Some((op, n.checked_mul(mult)?))
}

fn new_size(op: Op, val: u64, current: u64) -> Option<u64> {
    Some(match op {
        Op::Set => val,
        Op::Add => current.saturating_add(val),
        Op::Sub => current.saturating_sub(val),
        Op::Cap => current.min(val),
        Op::Floor => current.max(val),
        Op::RoundDown => {
            if val == 0 {
                return None;
            }
            (current / val) * val
        }
        Op::RoundUp => {
            if val == 0 {
                return None;
            }
            current.div_ceil(val) * val
        }
    })
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut size_arg: Option<String> = None;
    let mut reference: Option<String> = None;
    let mut no_create = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-s" | "--size" if i + 1 < args.len() => {
                size_arg = Some(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with("--size=") => {
                size_arg = Some(s["--size=".len()..].to_string());
                i += 1;
            }
            "-r" | "--reference" if i + 1 < args.len() => {
                reference = Some(args[i + 1].clone());
                i += 2;
            }
            "-c" | "--no-create" => {
                no_create = true;
                i += 1;
            }
            "-o" | "--io-blocks" => {
                // No-op for parity (we don't track block size).
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("truncate", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let files: Vec<String> = args[i..].to_vec();
    if files.is_empty() {
        err("truncate", "missing FILE operand");
        return 2;
    }
    if size_arg.is_none() && reference.is_none() {
        err("truncate", "you must specify either '--size' or '--reference'");
        return 2;
    }

    let parsed = if let Some(sa) = &size_arg {
        match parse_size(sa) {
            Some(p) => Some(p),
            None => {
                err("truncate", &format!("invalid size: {sa}"));
                return 2;
            }
        }
    } else {
        None
    };

    let ref_size = if let Some(r) = &reference {
        match std::fs::metadata(r) {
            Ok(m) => m.len(),
            Err(e) => {
                err_path("truncate", r, &e);
                return 1;
            }
        }
    } else {
        0
    };

    let mut rc = 0;
    for f in &files {
        let exists = std::path::Path::new(f).exists();
        if !exists && no_create {
            continue;
        }
        let current: u64 = if exists {
            match std::fs::metadata(f) {
                Ok(m) => m.len(),
                Err(e) => {
                    err_path("truncate", f, &e);
                    rc = 1;
                    continue;
                }
            }
        } else {
            0
        };

        let target: u64 = if let Some((op, val)) = parsed {
            match new_size(op, val, current) {
                Some(t) => t,
                None => {
                    err(
                        "truncate",
                        &format!(
                            "division by zero in size operator: {}",
                            size_arg.as_deref().unwrap_or("")
                        ),
                    );
                    rc = 1;
                    continue;
                }
            }
        } else {
            ref_size
        };

        // Open with create+write (no truncate flag) so the file exists, then
        // resize. set_len both grows (zero-fills) and shrinks.
        let res = OpenOptions::new().write(true).create(true).truncate(false).open(f);
        let fh = match res {
            Ok(fh) => fh,
            Err(e) => {
                err_path("truncate", f, &e);
                rc = 1;
                continue;
            }
        };
        if let Err(e) = fh.set_len(target) {
            err_path("truncate", f, &e);
            rc = 1;
        }
    }
    rc
}
