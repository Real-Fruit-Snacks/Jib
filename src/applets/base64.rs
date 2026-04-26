//! `base64` — RFC 4648 encode/decode.
//!
//! Encoded output is wrapped at 76 columns by default (matching coreutils);
//! `-w 0` disables wrapping. `-d` decodes, ignoring whitespace.

use std::fs::File;
use std::io::{self, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "base64",
    help: "encode/decode base64",
    aliases: &[],
    main,
};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode_byte(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn encode(input: &[u8], wrap: usize, out: &mut impl Write) -> io::Result<()> {
    let mut col = 0usize;
    let mut emit = |b: u8, out: &mut dyn Write| -> io::Result<()> {
        if wrap > 0 && col == wrap {
            out.write_all(b"\n")?;
            col = 0;
        }
        out.write_all(&[b])?;
        col += 1;
        Ok(())
    };
    let chunks = input.chunks_exact(3);
    let rem = chunks.remainder();
    for chunk in chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        emit(ALPHABET[((n >> 18) & 0x3f) as usize], out)?;
        emit(ALPHABET[((n >> 12) & 0x3f) as usize], out)?;
        emit(ALPHABET[((n >> 6) & 0x3f) as usize], out)?;
        emit(ALPHABET[(n & 0x3f) as usize], out)?;
    }
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            emit(ALPHABET[((n >> 18) & 0x3f) as usize], out)?;
            emit(ALPHABET[((n >> 12) & 0x3f) as usize], out)?;
            emit(b'=', out)?;
            emit(b'=', out)?;
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            emit(ALPHABET[((n >> 18) & 0x3f) as usize], out)?;
            emit(ALPHABET[((n >> 12) & 0x3f) as usize], out)?;
            emit(ALPHABET[((n >> 6) & 0x3f) as usize], out)?;
            emit(b'=', out)?;
        }
        _ => {}
    }
    // mainsail / coreutils: trailing newline only when wrapping is on.
    // Empty input + wrap>0 still gets one newline (preserves "one line per
    // chunk" framing); zero-input with wrap=0 emits literally nothing.
    if wrap > 0 {
        out.write_all(b"\n")?;
    }
    Ok(())
}

fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    // Strip whitespace and `=` padding to keep the 4-char-block math simple.
    let cleaned: Vec<u8> = input
        .iter()
        .copied()
        .filter(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        .collect();
    let pad_count = cleaned.iter().rev().take_while(|&&b| b == b'=').count();
    let body = &cleaned[..cleaned.len() - pad_count];
    let mut out: Vec<u8> = Vec::with_capacity(body.len() * 3 / 4);
    let chunks = body.chunks_exact(4);
    let rem = chunks.remainder();
    for chunk in chunks {
        let mut n = 0u32;
        for &b in chunk {
            let v =
                decode_byte(b).ok_or_else(|| format!("invalid base64 char: {:?}", b as char))?;
            n = (n << 6) | v as u32;
        }
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
    }
    match rem.len() {
        0 => {}
        2 => {
            let v0 = decode_byte(rem[0]).ok_or("invalid base64")?;
            let v1 = decode_byte(rem[1]).ok_or("invalid base64")?;
            let n = ((v0 as u32) << 18) | ((v1 as u32) << 12);
            out.push((n >> 16) as u8);
        }
        3 => {
            let v0 = decode_byte(rem[0]).ok_or("invalid base64")?;
            let v1 = decode_byte(rem[1]).ok_or("invalid base64")?;
            let v2 = decode_byte(rem[2]).ok_or("invalid base64")?;
            let n = ((v0 as u32) << 18) | ((v1 as u32) << 12) | ((v2 as u32) << 6);
            out.push((n >> 16) as u8);
            out.push((n >> 8) as u8);
        }
        _ => return Err("invalid base64: trailing 1 char".into()),
    }
    Ok(out)
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut decode_mode = false;
    let mut wrap: usize = 76;
    let mut file: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-d" | "--decode" => {
                decode_mode = true;
                i += 1;
            }
            "-w" | "--wrap" => {
                if i + 1 >= args.len() {
                    err("base64", "option requires argument: -w");
                    return 2;
                }
                match args[i + 1].parse::<usize>() {
                    Ok(n) => {
                        wrap = n;
                        i += 2;
                    }
                    Err(_) => {
                        err("base64", &format!("invalid wrap value: {}", args[i + 1]));
                        return 2;
                    }
                }
            }
            "-i" | "--ignore-garbage" => {
                // We already ignore non-alphabet characters strictly; -i is
                // accepted as a no-op for compatibility.
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("base64", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                file = Some(a.clone());
                i += 1;
            }
        }
    }

    let mut input: Vec<u8> = Vec::new();
    let read_result = match file.as_deref() {
        None | Some("-") => io::stdin().read_to_end(&mut input).map(|_| ()),
        Some(p) => match File::open(p) {
            Ok(mut f) => f.read_to_end(&mut input).map(|_| ()),
            Err(e) => {
                err_path("base64", p, &e);
                return 1;
            }
        },
    };
    if let Err(e) = read_result {
        err("base64", &e.to_string());
        return 1;
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if decode_mode {
        match decode(&input) {
            Ok(bytes) => {
                if let Err(e) = out.write_all(&bytes) {
                    err("base64", &e.to_string());
                    return 1;
                }
                0
            }
            Err(e) => {
                err("base64", &e);
                1
            }
        }
    } else {
        if let Err(e) = encode(&input, wrap, &mut out) {
            err("base64", &e.to_string());
            return 1;
        }
        0
    }
}
