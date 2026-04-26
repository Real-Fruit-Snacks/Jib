//! Shared `<algo>sum` implementations: `md5sum`, `sha1sum`, `sha256sum`,
//! `sha512sum`. Mirrors the GNU/BSD coreutils-style output.
//!
//! Default mode: print `<hex>  <file>` (two spaces, GNU form). With `-c`,
//! verify a checksum file produced by either GNU or BSD-tagged output.

use std::fs::File;
use std::io::{self, BufRead, Read, Write};

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const MD5: Applet = Applet {
    name: "md5sum",
    help: "compute and check MD5 message digests",
    aliases: &[],
    main: main_md5,
};
pub const SHA1: Applet = Applet {
    name: "sha1sum",
    help: "compute and check SHA-1 message digests",
    aliases: &[],
    main: main_sha1,
};
pub const SHA256: Applet = Applet {
    name: "sha256sum",
    help: "compute and check SHA-256 message digests",
    aliases: &[],
    main: main_sha256,
};
pub const SHA512: Applet = Applet {
    name: "sha512sum",
    help: "compute and check SHA-512 message digests",
    aliases: &[],
    main: main_sha512,
};

fn main_md5(argv: &[String]) -> i32 {
    run(argv, "md5", "MD5", |buf| compute::<Md5>(buf))
}
fn main_sha1(argv: &[String]) -> i32 {
    run(argv, "sha1", "SHA1", |buf| compute::<Sha1>(buf))
}
fn main_sha256(argv: &[String]) -> i32 {
    run(argv, "sha256", "SHA256", |buf| compute::<Sha256>(buf))
}
fn main_sha512(argv: &[String]) -> i32 {
    run(argv, "sha512", "SHA512", |buf| compute::<Sha512>(buf))
}

fn compute<H: Digest + Default>(reader: &mut dyn Read) -> io::Result<String> {
    let mut h = H::default();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let bytes = h.finalize();
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn parse_check_line(line: &str, label: &str) -> Option<(String, String)> {
    // BSD-tag form: "LABEL (FILE) = HEX"
    let prefix = format!("{label} (");
    if let Some(rest) = line.strip_prefix(&prefix) {
        if let Some(close) = rest.rfind(") = ") {
            return Some((
                rest[close + 4..].to_string(),
                rest[..close].to_string(),
            ));
        }
    }
    // GNU form: HEX  FILE  (with optional ` *` binary marker before file).
    let mut chars = line.chars();
    let hex: String = chars
        .by_ref()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.is_empty() {
        return None;
    }
    let rest: String = chars.collect();
    let trimmed = rest.trim_start_matches([' ', '\t']).trim_start_matches(['*', ' ']);
    if trimmed.is_empty() {
        return None;
    }
    Some((hex, trimmed.to_string()))
}

fn do_check(
    applet: &str,
    algo: &str,
    label: &str,
    files: &[String],
    quiet: bool,
    status: bool,
    warn: bool,
    strict: bool,
) -> i32 {
    let mut ok = 0u64;
    let mut bad = 0u64;
    let mut unreadable = 0u64;
    let mut malformed = 0u64;
    for f in files {
        let reader: Box<dyn std::io::BufRead> = if f == "-" {
            Box::new(std::io::BufReader::new(io::stdin().lock()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(std::io::BufReader::new(fh)),
                Err(e) => {
                    err_path(applet, f, &e);
                    return 1;
                }
            }
        };
        for (lineno_zero, line) in reader.lines().enumerate() {
            let lineno = lineno_zero + 1;
            let line = line.unwrap_or_default();
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parsed = parse_check_line(line, label);
            let (expected, target) = match parsed {
                Some(p) => p,
                None => {
                    malformed += 1;
                    if warn {
                        err(applet, &format!(
                            "{f}:{lineno}: improperly formatted {label} checksum line"
                        ));
                    }
                    continue;
                }
            };
            let mut fh = match File::open(&target) {
                Ok(f) => f,
                Err(_) => {
                    unreadable += 1;
                    if !status {
                        println!("{target}: FAILED open or read");
                    }
                    continue;
                }
            };
            let actual_res = match algo {
                "md5" => compute::<Md5>(&mut fh),
                "sha1" => compute::<Sha1>(&mut fh),
                "sha256" => compute::<Sha256>(&mut fh),
                "sha512" => compute::<Sha512>(&mut fh),
                _ => unreachable!(),
            };
            let actual = match actual_res {
                Ok(s) => s,
                Err(_) => {
                    unreadable += 1;
                    if !status {
                        println!("{target}: FAILED open or read");
                    }
                    continue;
                }
            };
            if actual.eq_ignore_ascii_case(&expected) {
                ok += 1;
                if !status && !quiet {
                    println!("{target}: OK");
                }
            } else {
                bad += 1;
                if !status {
                    println!("{target}: FAILED");
                }
            }
        }
    }
    if strict && malformed > 0 {
        return 1;
    }
    if bad > 0 || unreadable > 0 {
        if !status {
            let mut msgs: Vec<String> = Vec::new();
            if bad > 0 {
                msgs.push(format!("{bad} computed checksum did NOT match"));
            }
            if unreadable > 0 {
                msgs.push(format!("{unreadable} listed file could not be read"));
            }
            for m in msgs {
                eprintln!("{applet}: WARNING: {m}");
            }
        }
        return 1;
    }
    let _ = ok;
    0
}

fn run<F: Fn(&mut dyn Read) -> io::Result<String>>(
    argv: &[String],
    algo: &str,
    label: &str,
    _hash: F,
) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut binary = false;
    let mut tag = false;
    let mut check = false;
    let mut quiet = false;
    let mut status = false;
    let mut warn = false;
    let mut strict = false;
    let applet = label.to_lowercase() + "sum";

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-b" | "--binary" => {
                binary = true;
                i += 1;
            }
            "-t" | "--text" => {
                binary = false;
                i += 1;
            }
            "--tag" => {
                tag = true;
                i += 1;
            }
            "-c" | "--check" => {
                check = true;
                i += 1;
            }
            "--quiet" => {
                quiet = true;
                i += 1;
            }
            "--status" => {
                status = true;
                i += 1;
            }
            "-w" | "--warn" => {
                warn = true;
                i += 1;
            }
            "--strict" => {
                strict = true;
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err(&applet, &format!("invalid option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let files: Vec<String> = args[i..].to_vec();
    if check {
        let files: Vec<String> = if files.is_empty() {
            vec!["-".to_string()]
        } else {
            files
        };
        return do_check(&applet, algo, label, &files, quiet, status, warn, strict);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;
    let targets: Vec<String> = if files.is_empty() {
        vec!["-".to_string()]
    } else {
        files
    };
    for f in &targets {
        let mut reader: Box<dyn Read> = if f == "-" {
            Box::new(io::stdin().lock())
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(fh),
                Err(e) => {
                    err_path(&applet, f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        let hex = match algo {
            "md5" => compute::<Md5>(&mut *reader),
            "sha1" => compute::<Sha1>(&mut *reader),
            "sha256" => compute::<Sha256>(&mut *reader),
            "sha512" => compute::<Sha512>(&mut *reader),
            _ => unreachable!(),
        };
        let hex = match hex {
            Ok(s) => s,
            Err(e) => {
                err_path(&applet, f, &e);
                rc = 1;
                continue;
            }
        };
        if tag {
            let _ = writeln!(out, "{label} ({f}) = {hex}");
        } else {
            let marker = if binary { '*' } else { ' ' };
            let _ = writeln!(out, "{hex} {marker}{f}");
        }
    }
    rc
}
