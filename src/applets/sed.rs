//! `sed` — stream editor with a small POSIX-compatible command set.
//!
//! Implements `s/PAT/REPL/[flags]`, `d` delete, `p` print, `q` quit,
//! `=` print line number, `y/SRC/DST/` translate. Addresses: line number,
//! `$` last line, `/REGEX/`, plus optional `,` to form a range and `!`
//! to negate. `-n` suppresses default output, `-E`/`-r` extended regex,
//! `-i` in-place edit (writes via `<file>.mainsail_tmp` then atomically
//! renames), `-e SCRIPT` add a script, `-f FILE` read scripts.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use regex::{Regex, RegexBuilder};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "sed",
    help: "stream editor: basic s///, d, p, q, =, y and addresses",
    aliases: &[],
    main,
};

#[derive(Debug)]
struct Command {
    op: char,
    addr1: Option<String>,
    addr2: Option<String>,
    negate: bool,
    pattern: String,
    replacement: String,
    flags: String,
    src: String,
    dst: String,
    compiled: Option<Regex>,
}

const BRE_SWAP: &[char] = &['(', ')', '{', '}', '+', '?', '|'];

/// Translate basic-regex (with escaped meta-chars) into Rust regex syntax.
fn bre_to_rust(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let bytes = p.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '\\' && i + 1 < bytes.len() {
            let nx = bytes[i + 1] as char;
            if BRE_SWAP.contains(&nx) {
                out.push(nx);
                i += 2;
                continue;
            }
            out.push(c);
            out.push(nx);
            i += 2;
            continue;
        }
        if BRE_SWAP.contains(&c) {
            out.push('\\');
            out.push(c);
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn skip_ws(s: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < s.len() && matches!(s[j], b' ' | b'\t' | b'\n' | b';') {
        j += 1;
    }
    j
}

/// Read characters until `delim`, treating `\<x>` as literal `<x>`.
fn read_delim_part(s: &[u8], i: usize, delim: u8) -> (String, usize) {
    let mut j = i;
    while j < s.len() && s[j] != delim {
        if s[j] == b'\\' && j + 1 < s.len() {
            j += 2;
            continue;
        }
        j += 1;
    }
    (String::from_utf8_lossy(&s[i..j]).into_owned(), j)
}

fn parse_address(s: &[u8], i: usize) -> (Option<String>, usize) {
    if i >= s.len() {
        return (None, i);
    }
    let c = s[i];
    if c.is_ascii_digit() {
        let start = i;
        let mut j = i;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        return (Some(String::from_utf8_lossy(&s[start..j]).into_owned()), j);
    }
    if c == b'$' {
        return (Some("$".to_string()), i + 1);
    }
    if c == b'/' {
        let (pat, j) = read_delim_part(s, i + 1, b'/');
        let next_i = if j < s.len() { j + 1 } else { j };
        return (Some(format!("/{pat}/")), next_i);
    }
    (None, i)
}

fn parse_script(script: &str, extended: bool) -> Result<Vec<Command>, String> {
    let s = script.as_bytes();
    let mut cmds: Vec<Command> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        i = skip_ws(s, i);
        if i >= s.len() {
            break;
        }
        let (addr1, j) = parse_address(s, i);
        i = j;
        let mut addr2: Option<String> = None;
        if i < s.len() && s[i] == b',' {
            i += 1;
            let (a2, j) = parse_address(s, i);
            addr2 = a2;
            i = j;
        }
        while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
            i += 1;
        }
        let mut negate = false;
        if i < s.len() && s[i] == b'!' {
            negate = true;
            i += 1;
            while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
                i += 1;
            }
        }
        if i >= s.len() {
            break;
        }
        let op = s[i] as char;
        i += 1;
        let mut cmd = Command {
            op,
            addr1,
            addr2,
            negate,
            pattern: String::new(),
            replacement: String::new(),
            flags: String::new(),
            src: String::new(),
            dst: String::new(),
            compiled: None,
        };
        match op {
            's' => {
                if i >= s.len() {
                    return Err("s command: missing delimiter".to_string());
                }
                let delim = s[i];
                i += 1;
                let (pat, j) = read_delim_part(s, i, delim);
                cmd.pattern = pat;
                i = if j < s.len() { j + 1 } else { j };
                let (repl, j) = read_delim_part(s, i, delim);
                cmd.replacement = repl;
                i = if j < s.len() { j + 1 } else { j };
                let fstart = i;
                while i < s.len() && !matches!(s[i], b' ' | b'\t' | b'\n' | b';') {
                    i += 1;
                }
                cmd.flags = String::from_utf8_lossy(&s[fstart..i]).into_owned();
                let py_pat = if extended {
                    cmd.pattern.clone()
                } else {
                    bre_to_rust(&cmd.pattern)
                };
                let ic = cmd.flags.contains('i') || cmd.flags.contains('I');
                cmd.compiled = Some(
                    RegexBuilder::new(&py_pat)
                        .case_insensitive(ic)
                        .build()
                        .map_err(|e| format!("bad regex '{}': {e}", cmd.pattern))?,
                );
            }
            'y' => {
                if i >= s.len() {
                    return Err("y command: missing delimiter".to_string());
                }
                let delim = s[i];
                i += 1;
                let (src, j) = read_delim_part(s, i, delim);
                cmd.src = src;
                i = if j < s.len() { j + 1 } else { j };
                let (dst, j) = read_delim_part(s, i, delim);
                cmd.dst = dst;
                i = if j < s.len() { j + 1 } else { j };
            }
            'd' | 'p' | 'q' | '=' => {}
            other => return Err(format!("unsupported command: '{other}'")),
        }
        cmds.push(cmd);
    }
    Ok(cmds)
}

fn match_addr(addr: &str, lineno: usize, line: &str, last: usize) -> bool {
    if addr == "$" {
        return lineno == last;
    }
    if addr.bytes().all(|b| b.is_ascii_digit()) {
        return lineno == addr.parse::<usize>().unwrap_or(0);
    }
    if addr.starts_with('/') && addr.ends_with('/') && addr.len() >= 2 {
        let pat = &addr[1..addr.len() - 1];
        return Regex::new(pat).map(|rx| rx.is_match(line)).unwrap_or(false);
    }
    false
}

fn active_for(
    cmd: &Command,
    lineno: usize,
    line: &str,
    last: usize,
    state: &mut std::collections::HashMap<usize, bool>,
    cmd_idx: usize,
) -> bool {
    let base = if cmd.addr1.is_none() {
        true
    } else if cmd.addr2.is_none() {
        match_addr(cmd.addr1.as_deref().unwrap(), lineno, line, last)
    } else {
        let mut in_range = state.get(&cmd_idx).copied().unwrap_or(false);
        if !in_range && match_addr(cmd.addr1.as_deref().unwrap(), lineno, line, last) {
            in_range = true;
            state.insert(cmd_idx, true);
        }
        let result = in_range;
        if in_range && match_addr(cmd.addr2.as_deref().unwrap(), lineno, line, last) {
            state.insert(cmd_idx, false);
        }
        result
    };
    if cmd.negate { !base } else { base }
}

/// Apply backslash references and `&` in a replacement string. We rebuild
/// the output by hand because `regex::Captures::expand` uses `$N` notation,
/// not the sed `\N` notation.
fn replace_apply(caps: &regex::Captures, repl: &str) -> String {
    let bytes = repl.as_bytes();
    let mut out = String::with_capacity(repl.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            let nx = bytes[i + 1];
            if nx.is_ascii_digit() {
                let idx = (nx - b'0') as usize;
                if idx == 0 {
                    out.push_str(caps.get(0).map(|m| m.as_str()).unwrap_or(""));
                } else if let Some(g) = caps.get(idx) {
                    out.push_str(g.as_str());
                }
                i += 2;
                continue;
            }
            match nx {
                b'n' => {
                    out.push('\n');
                    i += 2;
                    continue;
                }
                b't' => {
                    out.push('\t');
                    i += 2;
                    continue;
                }
                b'r' => {
                    out.push('\r');
                    i += 2;
                    continue;
                }
                b'\\' => {
                    out.push('\\');
                    i += 2;
                    continue;
                }
                b'&' => {
                    out.push('&');
                    i += 2;
                    continue;
                }
                _ => {
                    out.push(nx as char);
                    i += 2;
                    continue;
                }
            }
        }
        if c == b'&' {
            out.push_str(caps.get(0).map(|m| m.as_str()).unwrap_or(""));
            i += 1;
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

fn run(cmds: &[Command], lines: &[String], quiet: bool) -> Result<Vec<String>, String> {
    let mut output: Vec<String> = Vec::new();
    let mut state: std::collections::HashMap<usize, bool> = Default::default();
    let total = lines.len();
    let mut quitting = false;

    for (idx, raw) in lines.iter().enumerate() {
        if quitting {
            break;
        }
        let lineno = idx + 1;
        let (mut pattern_space, had_nl) = if let Some(s) = raw.strip_suffix('\n') {
            (s.to_string(), true)
        } else {
            (raw.clone(), false)
        };
        let mut deleted = false;
        for (ci, cmd) in cmds.iter().enumerate() {
            if deleted || quitting {
                break;
            }
            if !active_for(cmd, lineno, &pattern_space, total, &mut state, ci) {
                continue;
            }
            match cmd.op {
                's' => {
                    let rx = cmd.compiled.as_ref().unwrap();
                    let global = cmd.flags.contains('g');
                    let mut nsubs = 0usize;
                    let new_space: String = if global {
                        rx.replace_all(&pattern_space, |caps: &regex::Captures| {
                            nsubs += 1;
                            replace_apply(caps, &cmd.replacement)
                        })
                        .into_owned()
                    } else {
                        // Replace only the first match.
                        if let Some(caps) = rx.captures(&pattern_space) {
                            nsubs = 1;
                            let m = caps.get(0).unwrap();
                            let repl = replace_apply(&caps, &cmd.replacement);
                            let mut s = String::with_capacity(pattern_space.len() + repl.len());
                            s.push_str(&pattern_space[..m.start()]);
                            s.push_str(&repl);
                            s.push_str(&pattern_space[m.end()..]);
                            s
                        } else {
                            pattern_space.clone()
                        }
                    };
                    pattern_space = new_space;
                    if cmd.flags.contains('p') && nsubs > 0 {
                        output.push(format!("{pattern_space}\n"));
                    }
                }
                'd' => {
                    deleted = true;
                    break;
                }
                'p' => {
                    output.push(format!("{pattern_space}\n"));
                }
                'q' => {
                    if !quiet {
                        output.push(if had_nl {
                            format!("{pattern_space}\n")
                        } else {
                            pattern_space.clone()
                        });
                    }
                    quitting = true;
                    break;
                }
                '=' => {
                    output.push(format!("{lineno}\n"));
                }
                'y' => {
                    if cmd.src.chars().count() != cmd.dst.chars().count() {
                        return Err("y: source and destination differ in length".to_string());
                    }
                    let table: std::collections::HashMap<char, char> =
                        cmd.src.chars().zip(cmd.dst.chars()).collect();
                    pattern_space = pattern_space
                        .chars()
                        .map(|c| table.get(&c).copied().unwrap_or(c))
                        .collect();
                }
                _ => {}
            }
        }
        if !deleted && !quitting && !quiet {
            output.push(if had_nl {
                format!("{pattern_space}\n")
            } else {
                pattern_space
            });
        }
    }
    Ok(output)
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut quiet = false;
    let mut in_place = false;
    let mut extended = false;
    let mut scripts: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            files.extend_from_slice(&args[i + 1..]);
            break;
        }
        match a.as_str() {
            "-n" | "--quiet" | "--silent" => {
                quiet = true;
                i += 1;
            }
            "-E" | "-r" | "--regexp-extended" => {
                extended = true;
                i += 1;
            }
            "-i" | "--in-place" => {
                in_place = true;
                i += 1;
            }
            "-e" if i + 1 < args.len() => {
                scripts.push(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with("-e") => {
                scripts.push(s[2..].to_string());
                i += 1;
            }
            "-f" if i + 1 < args.len() => {
                match std::fs::read_to_string(&args[i + 1]) {
                    Ok(s) => scripts.push(s),
                    Err(e) => {
                        err_path("sed", &args[i + 1], &e);
                        return 1;
                    }
                }
                i += 2;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("sed", &format!("invalid option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    let mut positional: Vec<String> = args[i..].to_vec();
    if scripts.is_empty() {
        if positional.is_empty() {
            err("sed", "missing script");
            return 2;
        }
        scripts.push(positional.remove(0));
    }
    files.extend(positional);

    let script = scripts.join("\n");
    let cmds = match parse_script(&script, extended) {
        Ok(c) => c,
        Err(e) => {
            err("sed", &e);
            return 2;
        }
    };

    if files.is_empty() {
        files.push("-".to_string());
    }
    if in_place && files.iter().any(|f| f == "-") {
        err("sed", "-i cannot be used with stdin");
        return 2;
    }

    let mut rc = 0;
    for f in &files {
        let data = if f == "-" {
            let mut s = String::new();
            let _ = io::stdin().lock().read_to_string(&mut s);
            s
        } else {
            match std::fs::read_to_string(f) {
                Ok(s) => s,
                Err(e) => {
                    err_path("sed", f, &e);
                    rc = 1;
                    continue;
                }
            }
        };
        // Split into newline-terminated lines (keeping the trailing newlines).
        let mut lines: Vec<String> = Vec::new();
        let mut start = 0usize;
        for (idx, b) in data.bytes().enumerate() {
            if b == b'\n' {
                lines.push(data[start..=idx].to_string());
                start = idx + 1;
            }
        }
        if start < data.len() {
            lines.push(data[start..].to_string());
        }
        let out_lines = match run(&cmds, &lines, quiet) {
            Ok(v) => v,
            Err(e) => {
                err("sed", &e);
                return 2;
            }
        };
        if in_place {
            let tmp = PathBuf::from(format!("{f}.mainsail_tmp"));
            match File::create(&tmp) {
                Ok(mut wh) => {
                    for line in &out_lines {
                        let _ = wh.write_all(line.as_bytes());
                    }
                    if let Err(e) = std::fs::rename(&tmp, f) {
                        err_path("sed", f, &e);
                        let _ = std::fs::remove_file(&tmp);
                        rc = 1;
                    }
                }
                Err(e) => {
                    err_path("sed", &tmp.display().to_string(), &e);
                    rc = 1;
                }
            }
        } else {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            for line in &out_lines {
                let _ = out.write_all(line.as_bytes());
            }
        }
    }
    rc
}

