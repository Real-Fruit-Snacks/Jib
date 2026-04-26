//! `tr` — translate, delete, or squeeze characters.
//!
//! Operates on bytes. Supports backslash escapes (`\n`, `\t`, ...),
//! character ranges (`a-z`), and POSIX character classes
//! (`[:alpha:]`, `[:digit:]`, etc.). Flags: `-d` delete, `-s` squeeze,
//! `-c`/`-C` complement, `-t` truncate first set to second's length.

use std::io::{self, Read, Write};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "tr",
    help: "translate, delete, or squeeze characters",
    aliases: &[],
    main,
};

fn class_bytes(name: &str) -> Option<Vec<u8>> {
    let v: Vec<u8> = match name {
        "alpha" => (b'A'..=b'Z').chain(b'a'..=b'z').collect(),
        "upper" => (b'A'..=b'Z').collect(),
        "lower" => (b'a'..=b'z').collect(),
        "digit" => (b'0'..=b'9').collect(),
        "alnum" => (b'A'..=b'Z')
            .chain(b'a'..=b'z')
            .chain(b'0'..=b'9')
            .collect(),
        "space" => vec![b' ', b'\t', b'\n', b'\r', 0x0b, 0x0c],
        "blank" => vec![b' ', b'\t'],
        "print" => (32u8..=126).chain(std::iter::once(b'\t')).collect(),
        "cntrl" => (0u8..=31).chain(std::iter::once(0x7f)).collect(),
        "punct" => b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~".to_vec(),
        "xdigit" => (b'0'..=b'9')
            .chain(b'A'..=b'F')
            .chain(b'a'..=b'f')
            .collect(),
        _ => return None,
    };
    Some(v)
}

/// Expand a SET argument into its byte sequence, handling escapes,
/// `[:class:]`, and `a-z` ranges.
fn expand(s: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // \X escape
        if c == b'\\' && i + 1 < bytes.len() {
            let nx = bytes[i + 1];
            let mapped: Option<u8> = match nx {
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'r' => Some(b'\r'),
                b'\\' => Some(b'\\'),
                b'0' => Some(0),
                b'a' => Some(0x07),
                b'b' => Some(0x08),
                b'f' => Some(0x0c),
                b'v' => Some(0x0b),
                _ => None,
            };
            if let Some(m) = mapped {
                out.push(m);
                i += 2;
                continue;
            }
            out.push(nx);
            i += 2;
            continue;
        }
        // [:class:]
        if c == b'[' && i + 3 < bytes.len() && bytes[i + 1] == b':' {
            if let Some(end) = s[i + 2..].find(":]") {
                let cls = &s[i + 2..i + 2 + end];
                if let Some(bs) = class_bytes(cls) {
                    out.extend_from_slice(&bs);
                    i = i + 2 + end + 2;
                    continue;
                }
            }
        }
        // a-z range
        if i + 2 < bytes.len() && bytes[i + 1] == b'-' && bytes[i + 2] != b']' {
            let a = bytes[i];
            let b = bytes[i + 2];
            if a <= b {
                for v in a..=b {
                    out.push(v);
                }
                i += 3;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut delete = false;
    let mut squeeze = false;
    let mut complement = false;
    let mut truncate = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a.starts_with('-')
            && a.len() > 1
            && a[1..]
                .chars()
                .all(|c| matches!(c, 'd' | 's' | 'c' | 'C' | 't'))
        {
            for ch in a[1..].chars() {
                match ch {
                    'd' => delete = true,
                    's' => squeeze = true,
                    'c' | 'C' => complement = true,
                    't' => truncate = true,
                    _ => unreachable!(),
                }
            }
            i += 1;
        } else {
            break;
        }
    }

    let positional: Vec<String> = args[i..].to_vec();
    if positional.is_empty() {
        err("tr", "missing operand");
        return 2;
    }
    if !delete && !squeeze && positional.len() < 2 {
        err(
            "tr",
            "when not deleting or squeezing, two arguments are required",
        );
        return 2;
    }

    let set1 = expand(&positional[0]);
    let set2 = if positional.len() > 1 {
        expand(&positional[1])
    } else {
        Vec::new()
    };

    let mut data: Vec<u8> = Vec::new();
    let _ = io::stdin().lock().read_to_end(&mut data);

    if delete {
        let in_set1: std::collections::HashSet<u8> = set1.iter().copied().collect();
        data = data
            .into_iter()
            .filter(|b| {
                if complement {
                    in_set1.contains(b)
                } else {
                    !in_set1.contains(b)
                }
            })
            .collect();
    } else if positional.len() >= 2 {
        let mut src = set1.clone();
        let mut dst = set2.clone();
        if truncate {
            src.truncate(dst.len());
        } else if dst.len() < src.len() {
            let last = *dst.last().unwrap_or(&0);
            while dst.len() < src.len() {
                dst.push(last);
            }
        }
        if complement {
            let replacement = *dst.last().unwrap_or(&0);
            let keep: std::collections::HashSet<u8> = src.iter().copied().collect();
            data = data
                .into_iter()
                .map(|b| if keep.contains(&b) { b } else { replacement })
                .collect();
        } else {
            let mut tbl: [u8; 256] = std::array::from_fn(|i| i as u8);
            for (s, d) in src.iter().zip(dst.iter()) {
                tbl[*s as usize] = *d;
            }
            data = data.into_iter().map(|b| tbl[b as usize]).collect();
        }
    }

    if squeeze {
        let sq_bytes = if delete && !set2.is_empty() {
            &set2
        } else {
            &set1
        };
        let mut sq_set: std::collections::HashSet<u8> = sq_bytes.iter().copied().collect();
        if complement && !delete {
            sq_set = (0..=255u8).filter(|b| !sq_set.contains(b)).collect();
        }
        let mut out = Vec::with_capacity(data.len());
        let mut prev: i32 = -1;
        for b in data {
            if i32::from(b) == prev && sq_set.contains(&b) {
                continue;
            }
            out.push(b);
            prev = i32::from(b);
        }
        data = out;
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(&data);
    let _ = out.flush();
    0
}
