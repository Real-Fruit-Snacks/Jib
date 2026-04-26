//! `tar` — minimal create/extract/list with optional gzip via `-z`.
//!
//! Modes: `-c` create, `-x` extract, `-t` list. With `-z` the stream is
//! gzip-wrapped (auto-detected on extract). `-f FILE` archive file
//! (default stdout/stdin). `-v` verbose. `-C DIR` change directory before
//! the operation.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "tar",
    help: "create, extract, or list tar archives",
    aliases: &[],
    main,
};

fn parse_short_options(s: &str) -> (bool, bool, bool, bool, bool, bool) {
    let mut create = false;
    let mut extract = false;
    let mut list = false;
    let mut gz = false;
    let mut verbose = false;
    let mut needs_arg = false;
    for ch in s.chars() {
        match ch {
            'c' => create = true,
            'x' => extract = true,
            't' => list = true,
            'z' => gz = true,
            'v' => verbose = true,
            'f' => needs_arg = true,
            _ => {}
        }
    }
    (create, extract, list, gz, verbose, needs_arg)
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut create = false;
    let mut extract = false;
    let mut list = false;
    let mut gz = false;
    let mut verbose = false;
    let mut archive: Option<String> = None;
    let mut chdir: Option<String> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        let body = &a[1..];
        if body.chars().all(|c| matches!(c, 'c' | 'x' | 't' | 'z' | 'v' | 'f')) {
            let (c, x, t, z, v, needs_f) = parse_short_options(body);
            create |= c;
            extract |= x;
            list |= t;
            gz |= z;
            verbose |= v;
            if needs_f {
                if i + 1 >= args.len() {
                    err("tar", "-f: missing argument");
                    return 2;
                }
                archive = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        match a.as_str() {
            "-C" if i + 1 < args.len() => {
                chdir = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                err("tar", &format!("unknown option: {a}"));
                return 2;
            }
        }
    }

    let inputs: Vec<String> = args[i..].to_vec();

    if let Some(d) = &chdir {
        if let Err(e) = std::env::set_current_dir(d) {
            err_path("tar", d, &e);
            return 1;
        }
    }

    let modes = (create as u8) + (extract as u8) + (list as u8);
    if modes == 0 {
        err("tar", "you must specify -c, -x, or -t");
        return 2;
    }
    if modes > 1 {
        err("tar", "only one of -c, -x, -t allowed");
        return 2;
    }

    if create {
        return do_create(&inputs, archive.as_deref(), gz, verbose);
    }
    if extract {
        return do_extract(archive.as_deref(), gz, verbose);
    }
    if list {
        return do_list(archive.as_deref(), gz, verbose);
    }
    0
}

fn do_create(inputs: &[String], archive: Option<&str>, gz: bool, verbose: bool) -> i32 {
    if inputs.is_empty() {
        err("tar", "no files specified for archive creation");
        return 2;
    }
    let stdout_lock;
    let writer: Box<dyn Write> = match archive {
        None | Some("-") => {
            stdout_lock = io::stdout();
            Box::new(stdout_lock.lock())
        }
        Some(p) => match File::create(p) {
            Ok(f) => Box::new(f),
            Err(e) => {
                err_path("tar", p, &e);
                return 1;
            }
        },
    };
    let writer: Box<dyn Write> = if gz {
        Box::new(GzEncoder::new(writer, Compression::default()))
    } else {
        writer
    };
    let mut builder = tar::Builder::new(writer);
    for inp in inputs {
        let p = Path::new(inp);
        let res = if p.is_dir() {
            builder.append_dir_all(inp, p)
        } else {
            builder.append_path(p)
        };
        if let Err(e) = res {
            err_path("tar", inp, &io::Error::other(e));
            return 1;
        }
        if verbose {
            eprintln!("{inp}");
        }
    }
    if let Err(e) = builder.finish() {
        err("tar", &e.to_string());
        return 1;
    }
    0
}

fn open_input(archive: Option<&str>, force_gz: bool) -> io::Result<Box<dyn Read>> {
    let stdin_lock;
    let raw: Box<dyn Read> = match archive {
        None | Some("-") => {
            stdin_lock = io::stdin();
            Box::new(stdin_lock.lock())
        }
        Some(p) => Box::new(File::open(p)?),
    };
    if force_gz || archive.map(is_gz_path).unwrap_or(false) {
        Ok(Box::new(GzDecoder::new(raw)))
    } else {
        Ok(raw)
    }
}

fn is_gz_path(p: &str) -> bool {
    p.ends_with(".gz") || p.ends_with(".tgz")
}

fn do_extract(archive: Option<&str>, gz: bool, verbose: bool) -> i32 {
    let reader = match open_input(archive, gz) {
        Ok(r) => r,
        Err(e) => {
            err("tar", &e.to_string());
            return 1;
        }
    };
    let mut a = tar::Archive::new(reader);
    let entries = match a.entries() {
        Ok(e) => e,
        Err(e) => {
            err("tar", &e.to_string());
            return 1;
        }
    };
    for entry in entries {
        let mut entry = match entry {
            Ok(e) => e,
            Err(e) => {
                err("tar", &e.to_string());
                return 1;
            }
        };
        let path = entry
            .path()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| PathBuf::from("?"));
        if verbose {
            eprintln!("{}", path.display());
        }
        if let Err(e) = entry.unpack_in(".") {
            err_path("tar", &path.display().to_string(), &e);
            return 1;
        }
    }
    0
}

fn do_list(archive: Option<&str>, gz: bool, _verbose: bool) -> i32 {
    let reader = match open_input(archive, gz) {
        Ok(r) => r,
        Err(e) => {
            err("tar", &e.to_string());
            return 1;
        }
    };
    let mut a = tar::Archive::new(reader);
    let entries = match a.entries() {
        Ok(e) => e,
        Err(e) => {
            err("tar", &e.to_string());
            return 1;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                err("tar", &e.to_string());
                return 1;
            }
        };
        let path = entry
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "?".to_string());
        println!("{path}");
    }
    0
}
