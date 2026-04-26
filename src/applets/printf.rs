//! `printf` — format and print data.
//!
//! Subset of the standard C/Python printf surface that the Python applet
//! supports: `%d/i/o/u/x/X/e/E/f/g/G/c/s/b/%`, optional flags `-+ 0#`,
//! optional width and precision. Backslash escapes (`\n`, `\t`, etc.) in
//! the format string are processed once. The format is reused (with
//! remaining arguments) until no more `%`-specs match.

use std::io::Write;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "printf",
    help: "format and print data",
    aliases: &[],
    main,
};

fn process_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let nx = bytes[i + 1];
            let mapped: Option<char> = match nx {
                b'n' => Some('\n'),
                b't' => Some('\t'),
                b'r' => Some('\r'),
                b'\\' => Some('\\'),
                b'a' => Some('\x07'),
                b'b' => Some('\x08'),
                b'f' => Some('\x0c'),
                b'v' => Some('\x0b'),
                b'0' => Some('\0'),
                b'\'' => Some('\''),
                b'"' => Some('"'),
                _ => None,
            };
            if let Some(c) = mapped {
                out.push(c);
                i += 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn coerce_int(arg: &str) -> i64 {
    if arg.is_empty() {
        return 0;
    }
    if let Ok(n) = arg.parse::<i64>() {
        return n;
    }
    if let Some(rest) = arg.strip_prefix("0x").or_else(|| arg.strip_prefix("0X")) {
        if let Ok(n) = i64::from_str_radix(rest, 16) {
            return n;
        }
    }
    if let Some(rest) = arg.strip_prefix("0o") {
        if let Ok(n) = i64::from_str_radix(rest, 8) {
            return n;
        }
    }
    arg.parse::<f64>().map(|f| f as i64).unwrap_or(0)
}

fn coerce_float(arg: &str) -> f64 {
    if arg.is_empty() {
        return 0.0;
    }
    arg.parse().unwrap_or(0.0)
}

#[derive(Default, Clone)]
struct Spec {
    left_align: bool,
    zero_pad: bool,
    plus: bool,
    space: bool,
    alt: bool,
    width: usize,
    precision: Option<usize>,
}

/// Parse a single conversion at fmt[start..]. Returns `(spec, conv_char,
/// new_index)` or None if no valid conversion is found.
fn parse_spec(fmt: &str, start: usize) -> Option<(Spec, char, usize)> {
    let bytes = fmt.as_bytes();
    let mut i = start + 1; // step past '%'
    let mut spec = Spec::default();
    while i < bytes.len() {
        match bytes[i] {
            b'-' => spec.left_align = true,
            b'+' => spec.plus = true,
            b' ' => spec.space = true,
            b'0' => spec.zero_pad = true,
            b'#' => spec.alt = true,
            _ => break,
        }
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        spec.width = spec.width * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let mut p = 0usize;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            p = p * 10 + (bytes[i] - b'0') as usize;
            i += 1;
        }
        spec.precision = Some(p);
    }
    if i >= bytes.len() {
        return None;
    }
    let conv = bytes[i] as char;
    Some((spec, conv, i + 1))
}

fn pad(out: &mut String, body: &str, spec: &Spec) {
    if spec.width <= body.len() {
        out.push_str(body);
        return;
    }
    let pad = spec.width - body.len();
    if spec.left_align {
        out.push_str(body);
        for _ in 0..pad {
            out.push(' ');
        }
    } else if spec.zero_pad && spec.precision.is_none() {
        let (sign, rest) = if body.starts_with('-') || body.starts_with('+') {
            (&body[..1], &body[1..])
        } else {
            ("", body)
        };
        out.push_str(sign);
        for _ in 0..(spec.width - body.len()) {
            out.push('0');
        }
        out.push_str(rest);
    } else {
        for _ in 0..pad {
            out.push(' ');
        }
        out.push_str(body);
    }
}

fn render(out: &mut String, spec: &Spec, conv: char, arg: &str) {
    let body = match conv {
        'd' | 'i' => format!("{}", coerce_int(arg)),
        'o' => format!("{:o}", coerce_int(arg) as u64),
        'u' => format!("{}", coerce_int(arg) as u64),
        'x' => format!("{:x}", coerce_int(arg) as u64),
        'X' => format!("{:X}", coerce_int(arg) as u64),
        'e' => match spec.precision {
            Some(p) => format!("{:.*e}", p, coerce_float(arg)),
            None => format!("{:.6e}", coerce_float(arg)),
        },
        'E' => match spec.precision {
            Some(p) => format!("{:.*E}", p, coerce_float(arg)),
            None => format!("{:.6E}", coerce_float(arg)),
        },
        'f' => match spec.precision {
            Some(p) => format!("{:.*}", p, coerce_float(arg)),
            None => format!("{:.6}", coerce_float(arg)),
        },
        'g' | 'G' => {
            let v = coerce_float(arg);
            let s = match spec.precision {
                Some(0) | None => format!("{v}"),
                Some(p) => format!("{:.*}", p, v),
            };
            if conv == 'G' {
                s.to_uppercase()
            } else {
                s
            }
        }
        'c' => arg
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_default(),
        's' => match spec.precision {
            Some(p) => arg.chars().take(p).collect::<String>(),
            None => arg.to_string(),
        },
        'b' => process_escapes(arg),
        _ => arg.to_string(),
    };
    let signed = if matches!(conv, 'd' | 'i' | 'e' | 'E' | 'f' | 'g' | 'G') {
        if spec.plus && !body.starts_with('-') && !body.starts_with('+') {
            format!("+{body}")
        } else if spec.space && !body.starts_with('-') && !body.starts_with('+') {
            format!(" {body}")
        } else {
            body
        }
    } else {
        body
    };
    pad(out, &signed, spec);
}

/// Apply `fmt` once to the argument vector. Returns `(text, had_spec,
/// consumed)`.
fn apply_once(fmt: &str, values: &[String]) -> (String, bool, usize) {
    let mut out = String::with_capacity(fmt.len());
    let mut had_spec = false;
    let mut consumed = 0usize;
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some((spec, conv, ni)) = parse_spec(fmt, i) {
                if conv == '%' {
                    out.push('%');
                    i = ni;
                    continue;
                }
                had_spec = true;
                let arg: &str = if consumed < values.len() {
                    let v = values[consumed].as_str();
                    consumed += 1;
                    v
                } else {
                    ""
                };
                render(&mut out, &spec, conv, arg);
                i = ni;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    (out, had_spec, consumed)
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    if args.is_empty() {
        err("printf", "missing format");
        return 2;
    }
    let fmt = process_escapes(&args[0]);
    let mut values: Vec<String> = args[1..].to_vec();

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let (text, mut had_spec, mut consumed) = apply_once(&fmt, &values);
    let _ = out.write_all(text.as_bytes());
    if consumed < values.len() {
        values.drain(..consumed);
    } else {
        values.clear();
    }

    while had_spec && !values.is_empty() {
        let (text, hs, c) = apply_once(&fmt, &values);
        let _ = out.write_all(text.as_bytes());
        had_spec = hs;
        consumed = c;
        if consumed == 0 {
            break;
        }
        values.drain(..consumed);
    }
    let _ = out.flush();
    0
}
