//! `grep` — print lines matching a pattern.
//!
//! Built on the `regex` crate. Flags: `-i` ignore case, `-v` invert,
//! `-n` line numbers, `-r/-R` recurse, `-F` fixed-string, `-l` list files,
//! `-c` count, `-w` whole word, `-o` only matching, `-q` quiet,
//! `-A`/`-B`/`-C` context.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use regex::{Regex, RegexBuilder};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "grep",
    help: "print lines matching a pattern",
    aliases: &[],
    main,
};

fn parse_count(flag: &str, value: &str) -> Option<usize> {
    match value.parse::<i64>() {
        Ok(n) if n >= 0 => Some(n as usize),
        _ => {
            err("grep", &format!("{flag}: invalid number '{value}'"));
            None
        }
    }
}

fn collect_recursive(t: &str, out: &mut Vec<String>) {
    if let Ok(meta) = std::fs::metadata(t) {
        if meta.is_dir() {
            let mut entries: Vec<_> = match std::fs::read_dir(t) {
                Ok(d) => d.flatten().collect(),
                Err(_) => return,
            };
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let p = e.path();
                if p.is_dir() {
                    collect_recursive(&p.display().to_string(), out);
                } else {
                    out.push(p.display().to_string());
                }
            }
            return;
        }
    }
    out.push(t.to_string());
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut ignore_case = false;
    let mut invert = false;
    let mut show_line_num = false;
    let mut recursive = false;
    let mut fixed_string = false;
    let mut list_files = false;
    let mut count_only = false;
    let mut word_match = false;
    let mut only_matching = false;
    let mut quiet = false;
    let mut before_n = 0usize;
    let mut after_n = 0usize;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if matches!(a.as_str(), "-A" | "-B" | "-C") {
            if i + 1 >= args.len() {
                err("grep", &format!("{a}: missing argument"));
                return 2;
            }
            match parse_count(&a, &args[i + 1]) {
                Some(n) => match a.as_str() {
                    "-A" => after_n = after_n.max(n),
                    "-B" => before_n = before_n.max(n),
                    _ => {
                        before_n = before_n.max(n);
                        after_n = after_n.max(n);
                    }
                },
                None => return 2,
            }
            i += 2;
            continue;
        }
        if a.len() > 2
            && matches!(&a[..2], "-A" | "-B" | "-C")
            && a[2..].trim_start_matches('-').chars().all(|c| c.is_ascii_digit())
        {
            let prefix = a[..2].to_string();
            match parse_count(&prefix, &a[2..]) {
                Some(n) => match prefix.as_str() {
                    "-A" => after_n = after_n.max(n),
                    "-B" => before_n = before_n.max(n),
                    _ => {
                        before_n = before_n.max(n);
                        after_n = after_n.max(n);
                    }
                },
                None => return 2,
            }
            i += 1;
            continue;
        }
        if !a.starts_with('-') || a.len() < 2 || a == "-" {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'i' => ignore_case = true,
                'v' => invert = true,
                'n' => show_line_num = true,
                'r' | 'R' => recursive = true,
                'F' => fixed_string = true,
                'l' => list_files = true,
                'c' => count_only = true,
                'w' => word_match = true,
                'o' => only_matching = true,
                'q' => quiet = true,
                'E' => {}
                _ => {
                    err("grep", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let remaining: Vec<String> = args[i..].to_vec();
    if remaining.is_empty() {
        err("grep", "missing pattern");
        return 2;
    }
    let mut pattern = remaining[0].clone();
    let mut targets: Vec<String> = remaining[1..].to_vec();

    if fixed_string {
        pattern = regex::escape(&pattern);
    }
    if word_match {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    let rx: Regex = match RegexBuilder::new(&pattern)
        .case_insensitive(ignore_case)
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            err("grep", &format!("bad pattern: {e}"));
            return 2;
        }
    };

    if targets.is_empty() {
        targets.push("-".to_string());
    }

    if recursive {
        let mut expanded: Vec<String> = Vec::new();
        for t in &targets {
            collect_recursive(t, &mut expanded);
        }
        targets = expanded;
    }

    let show_filename = targets.len() > 1 || recursive;
    let mut matched_any = false;

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for t in &targets {
        let reader: Box<dyn BufRead> = if t == "-" {
            Box::new(BufReader::new(io::stdin().lock()))
        } else {
            match File::open(Path::new(t)) {
                Ok(fh) => Box::new(BufReader::new(fh)),
                Err(e) => {
                    err_path("grep", t, &e);
                    continue;
                }
            }
        };
        let lines: Vec<(usize, String)> = reader
            .lines()
            .enumerate()
            .filter_map(|(n, l)| l.ok().map(|s| (n + 1, s)))
            .collect();

        let mut match_lines: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
        for (n, text) in &lines {
            let found: Vec<(usize, usize)> = rx.find_iter(text).map(|m| (m.start(), m.end())).collect();
            let is_match = !found.is_empty() != invert;
            if is_match {
                let toks = if invert { Vec::new() } else { found };
                match_lines.push((*n, toks));
            }
        }

        if quiet {
            if !match_lines.is_empty() {
                return 0;
            }
            continue;
        }

        if list_files {
            if !match_lines.is_empty() {
                matched_any = true;
                let _ = writeln!(out, "{t}");
            }
            continue;
        }

        if count_only {
            let cnt = match_lines.len();
            if show_filename {
                let _ = writeln!(out, "{t}:{cnt}");
            } else {
                let _ = writeln!(out, "{cnt}");
            }
            if cnt > 0 {
                matched_any = true;
            }
            continue;
        }

        let write_line = |out: &mut dyn Write, path: &str, lineno: usize, text: &str, is_match: bool| {
            let mut parts: Vec<String> = Vec::new();
            if show_filename {
                parts.push(path.to_string());
            }
            if show_line_num {
                parts.push(lineno.to_string());
            }
            parts.push(text.to_string());
            let sep = if is_match { ":" } else { "-" };
            let _ = writeln!(out, "{}", parts.join(sep));
        };

        if only_matching {
            for (n, ranges) in &match_lines {
                let line = &lines.iter().find(|(ln, _)| ln == n).unwrap().1;
                for (s, e) in ranges {
                    write_line(&mut out, t, *n, &line[*s..*e], true);
                }
            }
            if !match_lines.is_empty() {
                matched_any = true;
            }
            continue;
        }

        if match_lines.is_empty() {
            continue;
        }
        matched_any = true;

        // Build set of lines to print, with context.
        let mut to_print: std::collections::BTreeMap<usize, bool> = std::collections::BTreeMap::new();
        for (n, _) in &match_lines {
            to_print.insert(*n, true);
        }
        if before_n > 0 || after_n > 0 {
            for (n, _) in match_lines.iter().map(|x| (x.0, &x.1)).collect::<Vec<_>>() {
                let lo = n.saturating_sub(before_n).max(1);
                let hi = (n + after_n).min(lines.len());
                for k in lo..=hi {
                    to_print.entry(k).or_insert(false);
                }
            }
        }

        // Mirror the Python applet: print `--` between non-contiguous
        // output groups regardless of whether context is enabled. (This
        // is more aggressive than GNU grep, which gates `--` on -A/-B/-C.)
        let mut prev_printed: Option<usize> = None;
        for (n, text) in &lines {
            if let Some(&is_match) = to_print.get(n) {
                if let Some(p) = prev_printed {
                    if n - p > 1 {
                        let _ = writeln!(out, "--");
                    }
                }
                write_line(&mut out, t, *n, text, is_match);
                prev_printed = Some(*n);
            }
        }
    }

    if matched_any { 0 } else { 1 }
}
