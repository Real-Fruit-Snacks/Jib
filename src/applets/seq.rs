//! `seq` — print a sequence of numbers.
//!
//! `seq LAST` → 1..LAST. `seq FIRST LAST` → FIRST..LAST step 1. `seq FIRST
//! INCR LAST` → FIRST..LAST step INCR. `-s SEP`, `-w` for equal-width zero
//! padding, `-f FMT` for printf-style format (limited to integer-and-float
//! conversions — no escape parsing).

use std::io::Write;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "seq",
    help: "print a sequence of numbers",
    aliases: &[],
    main,
};

fn is_int_literal(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let start = if bytes[0] == b'+' || bytes[0] == b'-' {
        1
    } else {
        0
    };
    if start == s.len() {
        return false;
    }
    s[start..].chars().all(|c| c.is_ascii_digit())
}

fn take_value(flag: &str, args: &[String], idx: usize) -> Option<(String, usize)> {
    let a = &args[idx];
    if a.len() > flag.len() {
        return Some((a[flag.len()..].to_string(), idx + 1));
    }
    if idx + 1 >= args.len() {
        err("seq", &format!("{flag}: missing argument"));
        return None;
    }
    Some((args[idx + 1].clone(), idx + 2))
}

/// Apply a printf-style format specifier to one float value. We support
/// the common conversions the GNU seq accepts: `d/i/o/u/x/X/e/E/f/g/G`.
/// Anything we don't understand is returned as-is, matching the Python
/// fallback behavior.
fn apply_format(fmt: &str, v: f64) -> String {
    // Find the first `%` ... letter pair that isn't `%%`.
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'%' {
                i += 2;
                continue;
            }
            // Find conversion letter.
            let start = i;
            i += 1;
            while i < bytes.len()
                && !matches!(
                    bytes[i],
                    b'd' | b'i' | b'o' | b'u' | b'x' | b'X'
                        | b'e' | b'E' | b'f' | b'g' | b'G' | b's'
                )
            {
                i += 1;
            }
            if i >= bytes.len() {
                return fmt.to_string();
            }
            let conv = bytes[i] as char;
            let spec = &fmt[start..=i];
            let head = &fmt[..start];
            let tail = &fmt[i + 1..];
            // Evaluate. Use a tiny printf-like for the supported letters.
            let body = format_one(spec, conv, v);
            return format!("{head}{body}{tail}");
        }
        i += 1;
    }
    fmt.to_string()
}

fn format_one(spec: &str, conv: char, v: f64) -> String {
    // Parse flags/width/precision from spec like "%-10.3f".
    // Strip leading `%` and trailing conversion.
    let inner = &spec[1..spec.len() - 1];
    let mut chars = inner.chars().peekable();
    let mut left_align = false;
    let mut zero_pad = false;
    let mut plus = false;
    let mut space = false;
    let mut alt = false;
    while let Some(&c) = chars.peek() {
        match c {
            '-' => {
                left_align = true;
                chars.next();
            }
            '0' => {
                zero_pad = true;
                chars.next();
            }
            '+' => {
                plus = true;
                chars.next();
            }
            ' ' => {
                space = true;
                chars.next();
            }
            '#' => {
                alt = true;
                chars.next();
            }
            _ => break,
        }
    }
    let mut width = 0usize;
    while let Some(&c) = chars.peek() {
        if let Some(d) = c.to_digit(10) {
            width = width * 10 + d as usize;
            chars.next();
        } else {
            break;
        }
    }
    let mut precision: Option<usize> = None;
    if chars.peek() == Some(&'.') {
        chars.next();
        let mut p = 0usize;
        while let Some(&c) = chars.peek() {
            if let Some(d) = c.to_digit(10) {
                p = p * 10 + d as usize;
                chars.next();
            } else {
                break;
            }
        }
        precision = Some(p);
    }

    let _ = (left_align, plus, space, alt); // referenced below

    let body = match conv {
        'd' | 'i' => format!("{}", v as i64),
        'o' => format!("{:o}", v as i64),
        'u' => format!("{}", v as i64),
        'x' => format!("{:x}", v as i64),
        'X' => format!("{:X}", v as i64),
        'f' => match precision {
            Some(p) => format!("{:.*}", p, v),
            None => format!("{:.6}", v),
        },
        'e' => match precision {
            Some(p) => format!("{:.*e}", p, v),
            None => format!("{:.6e}", v),
        },
        'E' => match precision {
            Some(p) => format!("{:.*E}", p, v),
            None => format!("{:.6E}", v),
        },
        'g' | 'G' => {
            let body = match precision {
                Some(p) if p > 0 => format!("{:.*}", p, v),
                _ => format!("{}", v),
            };
            if conv == 'G' { body.to_uppercase() } else { body }
        }
        's' => format!("{v}"),
        _ => format!("{v}"),
    };

    let mut signed = body;
    if plus && !signed.starts_with('-') && !signed.starts_with('+') {
        signed.insert(0, '+');
    } else if space && !signed.starts_with('-') && !signed.starts_with('+') {
        signed.insert(0, ' ');
    }

    if width > signed.len() {
        let pad = width - signed.len();
        if left_align {
            format!("{signed}{:width$}", "", width = pad)
        } else if zero_pad && precision.is_none() {
            // Zero-pad numerically, after the sign character.
            let (sign, rest) = if signed.starts_with('-') || signed.starts_with('+') {
                (&signed[..1], &signed[1..])
            } else {
                ("", signed.as_str())
            };
            format!("{sign}{:0>width$}", rest, width = width - sign.len())
        } else {
            format!("{:width$}{signed}", "", width = pad)
        }
    } else {
        signed
    }
}

fn main(argv: &[String]) -> i32 {
    let args: Vec<String> = argv[1..].to_vec();
    let mut separator = "\n".to_string();
    let mut fmt: Option<String> = None;
    let mut equal_width = false;
    let terminator = "\n";

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            i += 1;
            break;
        }
        if a == "-s" || (a.starts_with("-s") && !is_int_literal(&a)) {
            match take_value("-s", &args, i) {
                Some((v, ni)) => {
                    separator = v;
                    i = ni;
                }
                None => return 2,
            }
            continue;
        }
        if a == "-f" || (a.starts_with("-f") && !is_int_literal(&a)) {
            match take_value("-f", &args, i) {
                Some((v, ni)) => {
                    fmt = Some(v);
                    i = ni;
                }
                None => return 2,
            }
            continue;
        }
        if a == "-w" || a == "--equal-width" {
            equal_width = true;
            i += 1;
            continue;
        }
        break;
    }

    let nums = &args[i..];
    let (start_s, incr_s, end_s) = match nums.len() {
        1 => ("1", "1", nums[0].as_str()),
        2 => (nums[0].as_str(), "1", nums[1].as_str()),
        3 => (nums[0].as_str(), nums[1].as_str(), nums[2].as_str()),
        _ => {
            err("seq", "usage: seq [-s SEP] [-f FMT] [-w] [FIRST [INCR]] LAST");
            return 2;
        }
    };

    let (start, incr, end) = match (
        start_s.parse::<f64>(),
        incr_s.parse::<f64>(),
        end_s.parse::<f64>(),
    ) {
        (Ok(s), Ok(i_), Ok(e)) => (s, i_, e),
        _ => {
            err("seq", "invalid numeric argument");
            return 2;
        }
    };
    if incr == 0.0 {
        err("seq", "increment must be non-zero");
        return 2;
    }
    let all_int = is_int_literal(start_s) && is_int_literal(incr_s) && is_int_literal(end_s);

    let mut values: Vec<f64> = Vec::new();
    let mut current = start;
    if incr > 0.0 {
        while current <= end + 1e-12 {
            values.push(current);
            current += incr;
        }
    } else {
        while current >= end - 1e-12 {
            values.push(current);
            current += incr;
        }
    }

    let format_one_value = |v: f64| -> String {
        if let Some(f) = &fmt {
            return apply_format(f, v);
        }
        if all_int {
            return (v.round() as i64).to_string();
        }
        if v == v.trunc() {
            return (v as i64).to_string();
        }
        // {:g}-equivalent — strip trailing zeros from a fixed-point form.
        let s = format!("{v}");
        s
    };

    let mut formatted: Vec<String> = values.iter().copied().map(format_one_value).collect();

    if equal_width && !formatted.is_empty() && fmt.is_none() {
        let max_len = formatted
            .iter()
            .map(|s| s.trim_start_matches('-').len())
            .max()
            .unwrap_or(0);
        for s in formatted.iter_mut() {
            if let Some(rest) = s.strip_prefix('-') {
                *s = format!("-{:0>width$}", rest, width = max_len);
            } else {
                *s = format!("{s:0>max_len$}");
            }
        }
    }

    if !formatted.is_empty() {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let _ = out.write_all(formatted.join(&separator).as_bytes());
        let _ = out.write_all(terminator.as_bytes());
    }
    0
}
