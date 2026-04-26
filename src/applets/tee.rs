//! `tee` — copy stdin to stdout and to one or more files.
//!
//! `-a` appends instead of truncating. `-i` ignores SIGINT (no-op on
//! Windows; needs platform plumbing on Unix that we accept-and-ignore for
//! parity — TODO).

use std::fs::OpenOptions;
use std::io::{self, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "tee",
    help: "read from stdin and write to stdout and files",
    aliases: &[],
    main,
};

const CHUNK: usize = 64 * 1024;

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut append = false;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        if a == "--append" {
            append = true;
            i += 1;
            continue;
        }
        if a == "-" || !a.starts_with('-') || a.len() < 2 {
            files.push(a.clone());
            i += 1;
            continue;
        }
        if a[1..].chars().all(|c| c == 'a' || c == 'i') {
            for ch in a[1..].chars() {
                if ch == 'a' {
                    append = true;
                }
                // 'i' SIGINT-ignore: no-op for now (parity).
            }
            i += 1;
            continue;
        }
        err("tee", &format!("invalid option: {a}"));
        return 2;
    }

    let mut handles: Vec<(String, std::fs::File)> = Vec::new();
    let mut rc = 0;
    for f in &files {
        let res = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(!append)
            .append(append)
            .open(f);
        match res {
            Ok(fh) => handles.push((f.clone(), fh)),
            Err(e) => {
                err_path("tee", f, &e);
                rc = 1;
            }
        }
    }

    let stdin = io::stdin();
    let mut sin = stdin.lock();
    let stdout = io::stdout();
    let mut sout = stdout.lock();
    let mut buf = vec![0u8; CHUNK];

    loop {
        match sin.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = sout.write_all(&buf[..n]) {
                    err("tee", &format!("stdout: {e}"));
                    rc = 1;
                }
                if let Err(e) = sout.flush() {
                    err("tee", &format!("stdout: {e}"));
                    rc = 1;
                }
                for (name, fh) in handles.iter_mut() {
                    if let Err(e) = fh.write_all(&buf[..n]) {
                        err_path("tee", name, &e);
                        rc = 1;
                    }
                }
            }
            Err(e) => {
                err("tee", &e.to_string());
                rc = 1;
                break;
            }
        }
    }
    rc
}
