//! `xargs` — build and execute command lines from standard input.
//!
//! Default tokenizer is shell-like (single/double quotes, backslash
//! escape). `-0`/`--null` reads NUL-separated; `-d C` uses a custom
//! single-char delimiter; `-L N` reads N lines per command. `-n N`
//! caps the number of args per invocation. `-I REPL` substitutes REPL
//! in the command template (one invocation per token). `-r` skips empty
//! input. `-t` traces.

use std::io::Read;
use std::process::Command;

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "xargs",
    help: "build and execute command lines from standard input",
    aliases: &[],
    main,
};

fn tokenize_shell_like(data: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let bytes = data.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                cur.push(c);
            }
        } else if in_double {
            if c == '"' {
                in_double = false;
            } else if c == '\\' && i + 1 < bytes.len() {
                cur.push(bytes[i + 1] as char);
                i += 1;
            } else {
                cur.push(c);
            }
        } else {
            match c {
                ' ' | '\t' | '\n' => {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                }
                '\'' => in_single = true,
                '"' => in_double = true,
                '\\' if i + 1 < bytes.len() => {
                    cur.push(bytes[i + 1] as char);
                    i += 1;
                }
                _ => cur.push(c),
            }
        }
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn take_value(flag: &str, args: &[String], idx: usize) -> Option<(String, usize)> {
    let a = &args[idx];
    if a.len() > flag.len() {
        return Some((a[flag.len()..].to_string(), idx + 1));
    }
    if idx + 1 >= args.len() {
        err("xargs", &format!("{flag}: missing argument"));
        return None;
    }
    Some((args[idx + 1].clone(), idx + 2))
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut n_per_call: Option<usize> = None;
    let mut lines_per_call: Option<usize> = None;
    let mut replace_str: Option<String> = None;
    let mut null_sep = false;
    let mut delimiter: Option<char> = None;
    let mut no_run_empty = false;
    let mut trace = false;
    let mut input_file: Option<String> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            i += 1;
            break;
        }
        match a.as_str() {
            s if s == "-n" || (s.starts_with("-n") && s[2..].chars().all(|c| c.is_ascii_digit())) => {
                match take_value("-n", &args, i) {
                    Some((v, ni)) => match v.parse() {
                        Ok(n) => {
                            n_per_call = Some(n);
                            i = ni;
                        }
                        Err(_) => {
                            err("xargs", &format!("-n: invalid value '{v}'"));
                            return 2;
                        }
                    },
                    None => return 2,
                }
            }
            s if s == "-L" || (s.starts_with("-L") && s[2..].chars().all(|c| c.is_ascii_digit())) => {
                match take_value("-L", &args, i) {
                    Some((v, ni)) => match v.parse() {
                        Ok(n) => {
                            lines_per_call = Some(n);
                            i = ni;
                        }
                        Err(_) => {
                            err("xargs", &format!("-L: invalid value '{v}'"));
                            return 2;
                        }
                    },
                    None => return 2,
                }
            }
            "-I" => match take_value("-I", &args, i) {
                Some((v, ni)) => {
                    replace_str = Some(v);
                    i = ni;
                }
                None => return 2,
            },
            s if s == "-d" || s.starts_with("-d") => match take_value("-d", &args, i) {
                Some((v, ni)) => {
                    delimiter = v.chars().next();
                    i = ni;
                }
                None => return 2,
            },
            "-a" => match take_value("-a", &args, i) {
                Some((v, ni)) => {
                    input_file = Some(v);
                    i = ni;
                }
                None => return 2,
            },
            "-0" | "--null" => {
                null_sep = true;
                i += 1;
            }
            "-r" | "--no-run-if-empty" => {
                no_run_empty = true;
                i += 1;
            }
            "-t" => {
                trace = true;
                i += 1;
            }
            s if !s.starts_with('-') => break,
            s => {
                err("xargs", &format!("invalid option: {s}"));
                return 2;
            }
        }
    }

    let cmd_template: Vec<String> = if i < args.len() {
        args[i..].to_vec()
    } else {
        vec!["echo".to_string()]
    };

    // Read input.
    let mut data = String::new();
    if let Some(p) = &input_file {
        match std::fs::read_to_string(p) {
            Ok(s) => data = s,
            Err(e) => {
                err_path("xargs", p, &e);
                return 1;
            }
        }
    } else if let Err(e) = std::io::stdin().read_to_string(&mut data) {
        err("xargs", &e.to_string());
        return 1;
    }

    let tokens: Vec<String> = if null_sep {
        data.split('\0').filter(|s| !s.is_empty()).map(String::from).collect()
    } else if let Some(d) = delimiter {
        data.split(d).filter(|s| !s.is_empty()).map(String::from).collect()
    } else if lines_per_call.is_some() {
        data.lines().filter(|s| !s.is_empty()).map(String::from).collect()
    } else {
        tokenize_shell_like(&data)
    };

    if tokens.is_empty() && no_run_empty {
        return 0;
    }

    // -I: one invocation per token, with substitution.
    if let Some(repl) = replace_str {
        let mut rc = 0;
        for tok in &tokens {
            let argv: Vec<String> = cmd_template
                .iter()
                .map(|a| a.replace(&repl, tok))
                .collect();
            if trace {
                eprintln!("{}", argv.join(" "));
            }
            match Command::new(&argv[0]).args(&argv[1..]).status() {
                Ok(s) => {
                    let r = s.code().unwrap_or(1);
                    if r != 0 {
                        rc = r;
                    }
                }
                Err(e) => {
                    err("xargs", &format!("{}: {}", argv[0], e));
                    return 127;
                }
            }
        }
        return rc;
    }

    // Group by -n or -L.
    let batch_size = n_per_call.or(lines_per_call);
    let groups: Vec<Vec<String>> = match batch_size {
        Some(b) if b > 0 => tokens.chunks(b).map(|c| c.to_vec()).collect(),
        _ => {
            if tokens.is_empty() {
                if no_run_empty {
                    Vec::new()
                } else {
                    vec![Vec::new()]
                }
            } else {
                vec![tokens]
            }
        }
    };

    let mut rc = 0;
    for group in groups {
        let mut argv = cmd_template.clone();
        argv.extend(group);
        if trace {
            eprintln!("{}", argv.join(" "));
        }
        match Command::new(&argv[0]).args(&argv[1..]).status() {
            Ok(s) => {
                let r = s.code().unwrap_or(1);
                if r != 0 {
                    rc = r;
                }
            }
            Err(e) => {
                err("xargs", &format!("{}: {}", argv[0], e));
                return 127;
            }
        }
    }
    rc
}
