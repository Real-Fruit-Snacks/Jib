//! `tail` — print the last part of files.
//!
//! Static tail with `-n N` lines (default 10) or `-c N` bytes; `-f`/`-F`
//! polls for appended data with a `-s SECS` interval (default 1.0). Multi-
//! file mode prints `==> NAME <==` headers.

use std::collections::VecDeque;
use std::fs::{File, Metadata};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::time::Duration;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "tail",
    help: "output the last part of files",
    aliases: &[],
    main,
};

fn parse_count(s: &str) -> Option<i64> {
    s.parse::<i64>().ok()
}

enum Source {
    Stdin,
    File(String),
}

/// Read all of `r`, then write the trailing `lines` lines or `byte_count`
/// bytes to stdout. For lines we keep a deque of size `lines`; for bytes we
/// just slice the tail of the buffer. Matches `_initial_tail` in Python.
fn initial_tail<R: Read>(
    r: R,
    bytes_mode: bool,
    byte_count: i64,
    lines: i64,
    out: &mut impl Write,
) {
    if bytes_mode {
        let mut all = Vec::new();
        if BufReader::new(r).read_to_end(&mut all).is_err() {
            return;
        }
        if byte_count <= 0 {
            return;
        }
        let n = byte_count as usize;
        let start = all.len().saturating_sub(n);
        let _ = out.write_all(&all[start..]);
    } else {
        let mut br = BufReader::new(r);
        let mut dq: VecDeque<Vec<u8>> = VecDeque::with_capacity(lines.max(0) as usize + 1);
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            let n = match br.read_until(b'\n', &mut buf) {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            if lines <= 0 {
                continue;
            }
            if dq.len() == lines as usize {
                dq.pop_front();
            }
            dq.push_back(buf.clone());
        }
        for line in dq {
            let _ = out.write_all(&line);
        }
    }
}

#[allow(dead_code)]
fn inode_of(meta: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return meta.ino();
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}

fn follow(files: &[String], multi: bool, sleep: f64) -> i32 {
    let mut handles: Vec<(String, File, u64)> = Vec::new();
    let mut rc = 0;
    for f in files {
        match File::open(f) {
            Ok(mut fh) => {
                let _ = fh.seek(SeekFrom::End(0));
                let ino = fh.metadata().as_ref().map(inode_of).unwrap_or(0);
                handles.push((f.clone(), fh, ino));
            }
            Err(e) => {
                err_path("tail", f, &e);
                rc = 1;
            }
        }
    }
    if handles.is_empty() {
        return rc;
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut last_file = handles
        .last()
        .map(|(n, _, _)| n.clone())
        .unwrap_or_default();
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let mut any = false;
        for (path, fh, ino) in handles.iter_mut() {
            // Detect truncation/rotation.
            if let Ok(st) = std::fs::metadata(path.as_str()) {
                if let Ok(pos) = fh.stream_position() {
                    if st.len() < pos {
                        let _ = fh.seek(SeekFrom::Start(0));
                    }
                    let new_ino = inode_of(&st);
                    if *ino != 0 && new_ino != 0 && new_ino != *ino {
                        if let Ok(nfh) = File::open(path.as_str()) {
                            *fh = nfh;
                            *ino = new_ino;
                        }
                    }
                }
            }
            loop {
                match fh.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if multi && last_file != *path {
                            let _ = writeln!(out, "\n==> {path} <==");
                            last_file = path.clone();
                        }
                        let _ = out.write_all(&buf[..n]);
                        let _ = out.flush();
                        any = true;
                    }
                    Err(_) => break,
                }
            }
        }
        if !any {
            std::thread::sleep(Duration::from_secs_f64(sleep));
        }
    }
    // unreachable except by Ctrl-C; the runtime tears us down.
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut lines: i64 = 10;
    let mut bytes_mode = false;
    let mut byte_count: i64 = 0;
    let mut do_follow = false;
    let mut sleep_interval = 1.0_f64;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a == "-n" && i + 1 < args.len() {
            match parse_count(&args[i + 1]) {
                Some(n) => {
                    lines = n;
                    i += 2;
                    continue;
                }
                None => {
                    err("tail", &format!("invalid line count: {}", args[i + 1]));
                    return 2;
                }
            }
        }
        if a == "-c" && i + 1 < args.len() {
            match parse_count(&args[i + 1]) {
                Some(n) => {
                    bytes_mode = true;
                    byte_count = n;
                    i += 2;
                    continue;
                }
                None => {
                    err("tail", &format!("invalid byte count: {}", args[i + 1]));
                    return 2;
                }
            }
        }
        if a == "-s" && i + 1 < args.len() {
            match args[i + 1].parse::<f64>() {
                Ok(f) => {
                    sleep_interval = f;
                    i += 2;
                    continue;
                }
                Err(_) => {
                    err("tail", &format!("invalid sleep interval: {}", args[i + 1]));
                    return 2;
                }
            }
        }
        if a.starts_with('-') && a.len() > 1 && a[1..].chars().all(|c| c.is_ascii_digit()) {
            lines = a[1..].parse().unwrap_or(10);
            i += 1;
            continue;
        }
        if a.starts_with('-') && a.len() > 1 {
            let body = &a[1..];
            for ch in body.chars() {
                match ch {
                    'f' | 'F' => do_follow = true,
                    _ => {
                        err("tail", &format!("invalid option: -{ch}"));
                        return 2;
                    }
                }
            }
            i += 1;
            continue;
        }
        break;
    }

    let raw_files: Vec<String> = args[i..].to_vec();
    let files_vec: Vec<Source> = if raw_files.is_empty() {
        vec![Source::Stdin]
    } else {
        raw_files
            .iter()
            .map(|f| {
                if f == "-" {
                    Source::Stdin
                } else {
                    Source::File(f.clone())
                }
            })
            .collect()
    };
    let multi = files_vec.len() > 1;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut rc = 0;

    for (idx, src) in files_vec.iter().enumerate() {
        let label = match src {
            Source::Stdin => "-".to_string(),
            Source::File(p) => p.clone(),
        };
        if multi {
            if idx > 0 {
                let _ = out.write_all(b"\n");
            }
            let _ = writeln!(out, "==> {label} <==");
            let _ = out.flush();
        }
        match src {
            Source::Stdin => {
                initial_tail(io::stdin().lock(), bytes_mode, byte_count, lines, &mut out)
            }
            Source::File(p) => match File::open(p) {
                Ok(fh) => initial_tail(fh, bytes_mode, byte_count, lines, &mut out),
                Err(e) => {
                    err_path("tail", p, &e);
                    rc = 1;
                    continue;
                }
            },
        }
        let _ = out.flush();
    }

    if !do_follow {
        return rc;
    }
    drop(out);
    let follow_files: Vec<String> = raw_files.into_iter().filter(|f| f != "-").collect();
    if follow_files.is_empty() {
        return rc;
    }
    follow(&follow_files, multi, sleep_interval);
    rc
}
