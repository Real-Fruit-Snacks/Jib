//! `gzip` / `gunzip` — gzip compression with flate2.
//!
//! Implements the common subset: `-d` decompress, `-c` write to stdout,
//! `-k` keep input file, `-f` force overwrite, `-1`..`-9` compression
//! level, `-n` ignore name/timestamp (default behavior — we don't write
//! original-name extras).

use std::fs::File;
use std::io::{self};
use std::path::PathBuf;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const GZIP: Applet = Applet {
    name: "gzip",
    help: "compress files",
    aliases: &[],
    main: main_gzip,
};
pub const GUNZIP: Applet = Applet {
    name: "gunzip",
    help: "decompress files",
    aliases: &[],
    main: main_gunzip,
};

fn main_gzip(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut decompress = false;
    let mut to_stdout = false;
    let mut keep = false;
    let mut force = false;
    let mut level = 6u32;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'd' => decompress = true,
                'c' => to_stdout = true,
                'k' => keep = true,
                'f' => force = true,
                '1'..='9' => level = ch.to_digit(10).unwrap(),
                'n' | 'N' | 'q' | 'v' => {}
                _ => {
                    err("gzip", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }
    let files: Vec<String> = args[i..].to_vec();
    if decompress {
        decompress_main(&files, to_stdout, keep, force)
    } else {
        compress_main(&files, to_stdout, keep, force, level)
    }
}

fn main_gunzip(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut to_stdout = false;
    let mut keep = false;
    let mut force = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'c' => to_stdout = true,
                'k' => keep = true,
                'f' => force = true,
                'q' | 'v' | 'n' => {}
                _ => {
                    err("gunzip", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }
    let files: Vec<String> = args[i..].to_vec();
    decompress_main(&files, to_stdout, keep, force)
}

fn compress_main(files: &[String], to_stdout: bool, keep: bool, force: bool, level: u32) -> i32 {
    let mut rc = 0;
    if files.is_empty() {
        let mut enc = GzEncoder::new(io::stdout().lock(), Compression::new(level));
        if let Err(e) = io::copy(&mut io::stdin().lock(), &mut enc) {
            err("gzip", &e.to_string());
            return 1;
        }
        let _ = enc.finish();
        return 0;
    }
    for f in files {
        let mut input = match File::open(f) {
            Ok(fh) => fh,
            Err(e) => {
                err_path("gzip", f, &e);
                rc = 1;
                continue;
            }
        };
        if to_stdout {
            let mut enc = GzEncoder::new(io::stdout().lock(), Compression::new(level));
            if let Err(e) = io::copy(&mut input, &mut enc) {
                err_path("gzip", f, &e);
                rc = 1;
            }
            let _ = enc.finish();
        } else {
            let out = format!("{f}.gz");
            if !force && PathBuf::from(&out).exists() {
                err("gzip", &format!("{out} already exists; use -f to overwrite"));
                rc = 1;
                continue;
            }
            let outf = match File::create(&out) {
                Ok(fh) => fh,
                Err(e) => {
                    err_path("gzip", &out, &e);
                    rc = 1;
                    continue;
                }
            };
            let mut enc = GzEncoder::new(outf, Compression::new(level));
            if let Err(e) = io::copy(&mut input, &mut enc) {
                err_path("gzip", f, &e);
                rc = 1;
                continue;
            }
            if enc.finish().is_ok() && !keep {
                let _ = std::fs::remove_file(f);
            }
        }
    }
    rc
}

fn decompress_main(files: &[String], to_stdout: bool, keep: bool, force: bool) -> i32 {
    let mut rc = 0;
    if files.is_empty() {
        let mut dec = GzDecoder::new(io::stdin().lock());
        if let Err(e) = io::copy(&mut dec, &mut io::stdout().lock()) {
            err("gunzip", &e.to_string());
            return 1;
        }
        return 0;
    }
    for f in files {
        let input = match File::open(f) {
            Ok(fh) => fh,
            Err(e) => {
                err_path("gunzip", f, &e);
                rc = 1;
                continue;
            }
        };
        let mut dec = GzDecoder::new(input);
        if to_stdout {
            if let Err(e) = io::copy(&mut dec, &mut io::stdout().lock()) {
                err_path("gunzip", f, &e);
                rc = 1;
            }
            continue;
        }
        let out = if let Some(stripped) = f.strip_suffix(".gz") {
            stripped.to_string()
        } else if let Some(stripped) = f.strip_suffix(".tgz") {
            format!("{stripped}.tar")
        } else {
            err("gunzip", &format!("{f}: unknown suffix"));
            rc = 1;
            continue;
        };
        if !force && PathBuf::from(&out).exists() {
            err("gunzip", &format!("{out} already exists; use -f to overwrite"));
            rc = 1;
            continue;
        }
        let mut outf = match File::create(&out) {
            Ok(fh) => fh,
            Err(e) => {
                err_path("gunzip", &out, &e);
                rc = 1;
                continue;
            }
        };
        if let Err(e) = io::copy(&mut dec, &mut outf) {
            err_path("gunzip", f, &e);
            rc = 1;
            continue;
        }
        if !keep {
            let _ = std::fs::remove_file(f);
        }
    }
    rc
}

