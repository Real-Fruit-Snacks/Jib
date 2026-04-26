//! `getopt` — POSIX/GNU shell-script option parser.
//!
//! Pattern: `getopt [OPTIONS] -o SHORTOPTS [--longoptions LONGOPTS] -- ARGS`
//! Output is shell-quoted so `eval set -- $(getopt ...)` works in scripts.

use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "getopt",
    help: "parse command options for shell scripts",
    aliases: &[],
    main,
};

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/.-_=:".contains(c)) {
        return s.to_string();
    }
    let escaped = s.replace('\'', r#"'\''"#);
    format!("'{escaped}'")
}

fn parse_long_spec(spec: &str) -> Vec<(String, u8)> {
    spec.split(',')
        .filter(|s| !s.is_empty())
        .map(|name| {
            if let Some(stripped) = name.strip_suffix("::") {
                (stripped.to_string(), 2)
            } else if let Some(stripped) = name.strip_suffix(':') {
                (stripped.to_string(), 1)
            } else {
                (name.to_string(), 0)
            }
        })
        .collect()
}

fn parse_short_spec(spec: &str) -> std::collections::HashMap<char, u8> {
    let mut out = std::collections::HashMap::new();
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        let mut needs = 0u8;
        if i < chars.len() && chars[i] == ':' {
            i += 1;
            needs = 1;
            if i < chars.len() && chars[i] == ':' {
                i += 1;
                needs = 2;
            }
        }
        out.insert(c, needs);
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut shortspec = String::new();
    let mut longspec: Vec<(String, u8)> = Vec::new();
    let mut quiet = false;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-o" | "--options" if i + 1 < args.len() => {
                shortspec = args[i + 1].clone();
                i += 2;
            }
            "-l" | "--longoptions" if i + 1 < args.len() => {
                longspec.extend(parse_long_spec(&args[i + 1]));
                i += 2;
            }
            "-q" | "--quiet" => {
                quiet = true;
                i += 1;
            }
            "-n" if i + 1 < args.len() => {
                // Program name for error messages — accept-and-ignore.
                i += 2;
            }
            "--" => {
                i += 1;
                break;
            }
            _ => break,
        }
    }
    let positional: Vec<String> = args[i..].to_vec();
    let shortmap = parse_short_spec(&shortspec);

    let mut output: Vec<String> = Vec::new();
    let mut residual: Vec<String> = Vec::new();
    let mut j = 0;
    while j < positional.len() {
        let a = positional[j].clone();
        if a == "--" {
            residual.extend_from_slice(&positional[j + 1..]);
            break;
        }
        if let Some(rest) = a.strip_prefix("--") {
            let (name, attached): (&str, Option<&str>) = match rest.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            };
            if let Some((spec, needs)) = longspec.iter().find(|(n, _)| n == name) {
                output.push(format!("--{spec}"));
                if *needs == 1 {
                    let v = if let Some(v) = attached {
                        v.to_string()
                    } else {
                        j += 1;
                        positional.get(j).cloned().unwrap_or_default()
                    };
                    output.push(shell_quote(&v));
                } else if *needs == 2 {
                    if let Some(v) = attached {
                        output.push(shell_quote(v));
                    } else {
                        output.push("''".to_string());
                    }
                }
                j += 1;
                continue;
            }
            if !quiet {
                eprintln!("getopt: unknown option: --{name}");
            }
            j += 1;
            continue;
        }
        if let Some(rest) = a.strip_prefix('-') {
            if rest.is_empty() {
                residual.push(a);
                j += 1;
                continue;
            }
            let mut chars = rest.chars().peekable();
            while let Some(c) = chars.next() {
                let needs = shortmap.get(&c).copied().unwrap_or(255);
                if needs == 255 {
                    if !quiet {
                        eprintln!("getopt: unknown option: -{c}");
                    }
                    continue;
                }
                output.push(format!("-{c}"));
                if needs == 1 {
                    let attached: String = chars.by_ref().collect();
                    if !attached.is_empty() {
                        output.push(shell_quote(&attached));
                    } else {
                        j += 1;
                        let v = positional.get(j).cloned().unwrap_or_default();
                        output.push(shell_quote(&v));
                    }
                    break;
                }
                if needs == 2 {
                    let attached: String = chars.by_ref().collect();
                    if attached.is_empty() {
                        output.push("''".to_string());
                    } else {
                        output.push(shell_quote(&attached));
                    }
                    break;
                }
            }
            j += 1;
            continue;
        }
        residual.push(a);
        j += 1;
    }
    output.push("--".to_string());
    output.extend(residual.into_iter().map(|a| shell_quote(&a)));
    println!("{}", output.join(" "));
    0
}
