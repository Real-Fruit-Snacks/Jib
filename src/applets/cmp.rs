//! `cmp` — compare two files byte by byte. Exit 0 on identical, 1 on diff,
//! 2 on error.

use std::fs::File;
use std::io::{self, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "cmp",
    help: "compare two files byte by byte",
    aliases: &[],
    main,
};

fn open(path: &str) -> io::Result<Box<dyn Read>> {
    if path == "-" {
        Ok(Box::new(io::stdin().lock()))
    } else {
        Ok(Box::new(File::open(path)?))
    }
}

fn skip(r: &mut dyn Read, n: u64) -> io::Result<()> {
    let mut buf = [0u8; 4096];
    let mut left = n;
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let n = r.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "skip past EOF",
            ));
        }
        left -= n as u64;
    }
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut silent = false;
    let mut print_chars = false;
    let mut print_all = false;
    let mut skip1: u64 = 0;
    let mut skip2: u64 = 0;
    let mut bytes_limit: Option<u64> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-s" | "--quiet" | "--silent" => {
                silent = true;
                i += 1;
            }
            "-b" | "--print-bytes" => {
                print_chars = true;
                i += 1;
            }
            "-l" | "--verbose" => {
                print_all = true;
                i += 1;
            }
            "-n" | "--bytes" if i + 1 < args.len() => match args[i + 1].parse() {
                Ok(n) => {
                    bytes_limit = Some(n);
                    i += 2;
                }
                Err(_) => {
                    err("cmp", &format!("invalid byte count: {}", args[i + 1]));
                    return 2;
                }
            },
            "-i" if i + 1 < args.len() => {
                let spec = args[i + 1].clone();
                if let Some((a, b)) = spec.split_once(':') {
                    match (a.parse(), b.parse()) {
                        (Ok(x), Ok(y)) => {
                            skip1 = x;
                            skip2 = y;
                            i += 2;
                        }
                        _ => {
                            err("cmp", &format!("invalid skip: {spec}"));
                            return 2;
                        }
                    }
                } else {
                    match spec.parse() {
                        Ok(x) => {
                            skip1 = x;
                            skip2 = x;
                            i += 2;
                        }
                        Err(_) => {
                            err("cmp", &format!("invalid skip: {spec}"));
                            return 2;
                        }
                    }
                }
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("cmp", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let rest: Vec<String> = args[i..].to_vec();
    if rest.len() < 2 {
        err("cmp", "missing operand");
        return 2;
    }
    let f1 = rest[0].clone();
    let f2 = rest[1].clone();
    if let Some(s) = rest.get(2) {
        match s.parse() {
            Ok(x) => skip1 = x,
            Err(_) => {
                err("cmp", &format!("invalid skip: {s}"));
                return 2;
            }
        }
    }
    if let Some(s) = rest.get(3) {
        match s.parse() {
            Ok(x) => skip2 = x,
            Err(_) => {
                err("cmp", &format!("invalid skip: {s}"));
                return 2;
            }
        }
    }

    let mut h1 = match open(&f1) {
        Ok(h) => h,
        Err(e) => {
            err_path("cmp", &f1, &e);
            return 2;
        }
    };
    let mut h2 = match open(&f2) {
        Ok(h) => h,
        Err(e) => {
            err_path("cmp", &f2, &e);
            return 2;
        }
    };

    if skip1 > 0 {
        if let Err(e) = skip(&mut h1, skip1) {
            err_path("cmp", &f1, &e);
            return 2;
        }
    }
    if skip2 > 0 {
        if let Err(e) = skip(&mut h2, skip2) {
            err_path("cmp", &f2, &e);
            return 2;
        }
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut offset: u64 = 0;
    let mut line: u64 = 1;
    let mut differ = false;
    let mut buf1 = [0u8; 1];
    let mut buf2 = [0u8; 1];

    loop {
        if let Some(lim) = bytes_limit {
            if offset >= lim {
                break;
            }
        }
        let n1 = h1.read(&mut buf1).unwrap_or(0);
        let n2 = h2.read(&mut buf2).unwrap_or(0);
        if n1 == 0 && n2 == 0 {
            break;
        }
        if n1 == 0 {
            if !silent {
                err("cmp", &format!("EOF on {f1} after byte {offset}"));
            }
            return 1;
        }
        if n2 == 0 {
            if !silent {
                err("cmp", &format!("EOF on {f2} after byte {offset}"));
            }
            return 1;
        }
        if buf1[0] != buf2[0] {
            differ = true;
            if silent {
                return 1;
            }
            if print_all {
                let _ = writeln!(out, "{} {:>3o} {:>3o}", offset + 1, buf1[0], buf2[0]);
            } else if print_chars {
                let c1 = if (32..127).contains(&buf1[0]) {
                    buf1[0] as char
                } else {
                    '.'
                };
                let c2 = if (32..127).contains(&buf2[0]) {
                    buf2[0] as char
                } else {
                    '.'
                };
                let _ = writeln!(
                    out,
                    "{f1} {f2} differ: byte {}, line {} is {:>3o} {} {:>3o} {}",
                    offset + 1,
                    line,
                    buf1[0],
                    c1,
                    buf2[0],
                    c2
                );
                return 1;
            } else {
                let _ = writeln!(out, "{f1} {f2} differ: byte {}, line {}", offset + 1, line);
                return 1;
            }
        }
        if buf1[0] == b'\n' {
            line += 1;
        }
        offset += 1;
    }
    if differ && print_all {
        1
    } else {
        0
    }
}
