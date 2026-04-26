//! `du` — estimate file space usage.
//!
//! Defaults to recursive byte sums. `-s` prints only totals; `-h` human-
//! readable; `-a` includes files (not just directories); `-c` produces a
//! grand total. Sizes are reported in bytes by default (use `-h` for the
//! `1.5K` / `42M` form).

use std::path::Path;

use crate::common::err_path;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "du",
    help: "estimate file space usage",
    aliases: &[],
    main,
};

fn human(n: u64) -> String {
    const UNITS: &[(&str, u64)] = &[
        ("P", 1 << 50),
        ("T", 1 << 40),
        ("G", 1 << 30),
        ("M", 1 << 20),
        ("K", 1 << 10),
    ];
    for (suf, base) in UNITS {
        if n >= *base {
            let v = n as f64 / *base as f64;
            return format!("{v:.1}{suf}");
        }
    }
    n.to_string()
}

fn walk(
    path: &Path,
    all: bool,
    human_fmt: bool,
    depth: usize,
    max_depth: usize,
    only_summary: bool,
    out: &mut Vec<(u64, String)>,
) -> u64 {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            err_path("du", &path.display().to_string(), &e);
            return 0;
        }
    };
    if meta.is_file() {
        let s = meta.len();
        if all {
            out.push((s, path.display().to_string()));
        }
        return s;
    }
    if meta.file_type().is_symlink() {
        return 0;
    }
    let mut total = 0u64;
    if let Ok(d) = std::fs::read_dir(path) {
        for entry in d.flatten() {
            total += walk(
                &entry.path(),
                all,
                human_fmt,
                depth + 1,
                max_depth,
                only_summary,
                out,
            );
        }
    }
    if !only_summary || depth == 0 {
        out.push((total, path.display().to_string()));
    }
    total
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut summary = false;
    let mut human_fmt = false;
    let mut all = false;
    let mut grand = false;

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
                's' => summary = true,
                'h' => human_fmt = true,
                'a' => all = true,
                'c' => grand = true,
                _ => {}
            }
        }
        i += 1;
    }
    let paths: Vec<String> = if i >= args.len() {
        vec![".".to_string()]
    } else {
        args[i..].to_vec()
    };

    let mut grand_total = 0u64;
    for p in &paths {
        let mut out: Vec<(u64, String)> = Vec::new();
        let total = walk(
            Path::new(p),
            all,
            human_fmt,
            0,
            usize::MAX,
            summary,
            &mut out,
        );
        grand_total += total;
        if summary {
            let size = if human_fmt {
                human(total)
            } else {
                total.to_string()
            };
            println!("{size}\t{p}");
        } else {
            for (s, name) in out {
                let size = if human_fmt { human(s) } else { s.to_string() };
                println!("{size}\t{name}");
            }
        }
    }
    if grand {
        let size = if human_fmt {
            human(grand_total)
        } else {
            grand_total.to_string()
        };
        println!("{size}\ttotal");
    }
    0
}
