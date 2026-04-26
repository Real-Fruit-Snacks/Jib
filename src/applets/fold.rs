//! `fold` — wrap each input line to fit a given width.
//!
//! Default width is 80. With `-s`, breaks happen at the last whitespace
//! before the limit when one is available; otherwise it cuts mid-word.
//! Width counts bytes by default (matching coreutils when locale is C).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "fold",
    help: "wrap input lines to fit width",
    aliases: &[],
    main,
};

fn fold_line(line: &str, width: usize, spaces: bool, out: &mut impl Write) -> io::Result<()> {
    if line.is_empty() {
        out.write_all(b"\n")?;
        return Ok(());
    }
    let bytes = line.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        let mut end = (start + width).min(bytes.len());
        if spaces && end < bytes.len() {
            // Walk back to the last space at or before end (but after start).
            if let Some(p) = bytes[start..end]
                .iter()
                .rposition(|&b| b == b' ' || b == b'\t')
            {
                end = start + p + 1;
            }
        }
        out.write_all(&bytes[start..end])?;
        out.write_all(b"\n")?;
        start = end;
    }
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut width: usize = 80;
    let mut spaces = false;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-s" | "--spaces" => {
                spaces = true;
                i += 1;
            }
            "-w" | "--width" => {
                if i + 1 >= args.len() {
                    err("fold", "option requires argument: -w");
                    return 2;
                }
                match args[i + 1].parse::<usize>() {
                    Ok(n) if n > 0 => {
                        width = n;
                        i += 2;
                    }
                    _ => {
                        err("fold", &format!("invalid width: {}", args[i + 1]));
                        return 2;
                    }
                }
            }
            s if s.starts_with("-w") && s.len() > 2 => {
                match s[2..].parse::<usize>() {
                    Ok(n) if n > 0 => width = n,
                    _ => {
                        err("fold", &format!("invalid width: {}", &s[2..]));
                        return 2;
                    }
                }
                i += 1;
            }
            s if s.starts_with('-')
                && s.len() > 1
                && s != "-"
                && s.chars().skip(1).all(|c| c.is_ascii_digit()) =>
            {
                // -N as shorthand for -w N (POSIX form).
                match s[1..].parse::<usize>() {
                    Ok(n) if n > 0 => width = n,
                    _ => {
                        err("fold", &format!("invalid width: {s}"));
                        return 2;
                    }
                }
                i += 1;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("fold", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                files.push(a.clone());
                i += 1;
            }
        }
    }

    if files.is_empty() {
        files.push("-".to_string());
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;
    for f in &files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    err_path("fold", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        let mut buf: Vec<u8> = Vec::new();
        let mut br = reader;
        loop {
            buf.clear();
            let n = match br.read_until(b'\n', &mut buf) {
                Ok(n) => n,
                Err(e) => {
                    err_path("fold", f, &e);
                    rc = 1;
                    break;
                }
            };
            if n == 0 {
                break;
            }
            // Strip the trailing \n (preserved when we re-emit), but keep
            // bytes lossy-decoded so width counts UTF-8 byte length.
            let body = if buf.ends_with(b"\n") {
                &buf[..buf.len() - 1]
            } else {
                &buf[..]
            };
            let s = String::from_utf8_lossy(body);
            if let Err(e) = fold_line(&s, width, spaces, &mut out) {
                err("fold", &e.to_string());
                rc = 1;
                break;
            }
        }
        let _ = out.flush();
    }
    rc
}
