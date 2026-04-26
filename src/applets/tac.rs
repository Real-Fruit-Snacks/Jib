//! `tac` — concatenate files and print in reverse order.
//!
//! Default separator is `\n` and trails each record. `-b` puts the
//! separator before each record (rare). `-s SEP` sets a custom separator.
//! `-r` (regex) is accepted as a no-op for compatibility.

use std::fs::File;
use std::io::{self, Read, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "tac",
    help: "concatenate and print files in reverse",
    aliases: &[],
    main,
};

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut sep = "\n".to_string();
    let mut before = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-b" | "--before" => {
                before = true;
                i += 1;
            }
            "-s" if i + 1 < args.len() => {
                sep = args[i + 1].clone();
                i += 2;
            }
            s if s.starts_with("--separator=") => {
                sep = s["--separator=".len()..].to_string();
                i += 1;
            }
            "-r" | "--regex" => {
                // Accepted as no-op; GNU tac treats -r without -s as identity.
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("tac", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let raw_files: Vec<String> = args[i..].to_vec();
    let files: Vec<String> = if raw_files.is_empty() {
        vec!["-".to_string()]
    } else {
        raw_files
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;
    let sep_b = sep.into_bytes();

    for f in &files {
        let mut data: Vec<u8> = Vec::new();
        let res = if f == "-" {
            io::stdin().lock().read_to_end(&mut data).map(|_| ())
        } else {
            match File::open(f) {
                Ok(mut fh) => fh.read_to_end(&mut data).map(|_| ()),
                Err(e) => {
                    err_path("tac", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        if let Err(e) = res {
            err_path("tac", f, &e);
            rc = 1;
            continue;
        }
        if data.is_empty() {
            continue;
        }

        // Split into records by separator.
        let mut records: Vec<&[u8]> = data
            .windows(sep_b.len())
            .enumerate()
            .filter(|(_, w)| *w == sep_b.as_slice())
            .map(|(i, _)| i)
            .scan(0usize, |state, i| {
                let part = &data[*state..i];
                *state = i + sep_b.len();
                Some(part)
            })
            .collect();
        // Push the final segment.
        let last_start = records
            .iter()
            .map(|s| s.as_ptr() as usize)
            .max()
            .map(|p| p - data.as_ptr() as usize)
            .map(|off| off + records.last().map(|s| s.len()).unwrap_or(0) + sep_b.len())
            .unwrap_or(0);
        records.push(&data[last_start..]);

        let trailing = data.ends_with(&sep_b);
        if trailing {
            // Drop empty final record so we don't emit a phantom line.
            if let Some(last) = records.last() {
                if last.is_empty() {
                    records.pop();
                }
            }
        }

        let mut buf: Vec<u8> = Vec::with_capacity(data.len());
        if before {
            for r in records.iter().rev() {
                buf.extend_from_slice(&sep_b);
                buf.extend_from_slice(r);
            }
            // Trim leading separator if input didn't start with one.
            if !data.starts_with(&sep_b) && buf.starts_with(&sep_b) {
                buf.drain(..sep_b.len());
            }
        } else {
            let mut first = true;
            for r in records.iter().rev() {
                if !first {
                    buf.extend_from_slice(&sep_b);
                }
                first = false;
                buf.extend_from_slice(r);
            }
            if trailing {
                buf.extend_from_slice(&sep_b);
            }
        }

        if out.write_all(&buf).is_err() {
            return rc;
        }
        let _ = out.flush();
    }
    rc
}
