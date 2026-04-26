//! `column` — pretty-print whitespace-separated input as aligned columns.
//!
//! `-t` enables table mode, the 80% case: each line is split on whitespace
//! into fields, then every field is left-padded to the per-column maximum.
//! Without `-t` we just pass the input through (the full util.linux
//! `column` has flow modes we don't need).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "column",
    help: "format input into columns",
    aliases: &[],
    main,
};

fn run_table(lines: &[String], separator: &str, out: &mut impl Write) -> io::Result<()> {
    // Parse rows.
    let rows: Vec<Vec<&str>> = lines
        .iter()
        .map(|l| {
            if separator == " " || separator.is_empty() {
                l.split_whitespace().collect()
            } else {
                l.split(separator).collect()
            }
        })
        .collect();
    if rows.is_empty() {
        return Ok(());
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; cols];
    for r in &rows {
        for (i, f) in r.iter().enumerate() {
            widths[i] = widths[i].max(f.chars().count());
        }
    }
    for r in &rows {
        for (i, f) in r.iter().enumerate() {
            if i + 1 == r.len() {
                // Last field on the line: no trailing padding.
                out.write_all(f.as_bytes())?;
            } else {
                let pad = widths[i].saturating_sub(f.chars().count());
                out.write_all(f.as_bytes())?;
                for _ in 0..pad + 2 {
                    out.write_all(b" ")?;
                }
            }
        }
        out.write_all(b"\n")?;
    }
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut table = false;
    let mut separator: String = " ".to_string();
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-t" | "--table" => {
                table = true;
                i += 1;
            }
            "-s" | "--separator" => {
                if i + 1 >= args.len() {
                    err("column", "option requires argument: -s");
                    return 2;
                }
                separator = args[i + 1].clone();
                i += 2;
            }
            s if s.starts_with('-') && s.len() > 1 && s != "-" => {
                err("column", &format!("unknown option: {s}"));
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

    // Slurp all input lines first — column needs to know column widths
    // before it can emit any row.
    let mut lines: Vec<String> = Vec::new();
    let mut rc = 0;
    for f in &files {
        let reader: Box<dyn BufRead> = if f == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(f) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    err_path("column", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        for ln in reader.lines() {
            match ln {
                Ok(s) => lines.push(s),
                Err(e) => {
                    err_path("column", f, &e);
                    rc = 1;
                    break;
                }
            }
        }
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let res = if table {
        run_table(&lines, &separator, &mut out)
    } else {
        // Plain mode: passthrough, one line per input line.
        let mut r = Ok(());
        for l in &lines {
            if let Err(e) = writeln!(out, "{l}") {
                r = Err(e);
                break;
            }
        }
        r
    };
    if let Err(e) = res {
        err("column", &e.to_string());
        rc = 1;
    }
    rc
}
