//! `date` — print or format the date and time.
//!
//! Default output: `Sat Apr 26 12:00:00 2026`. `+FORMAT` uses strftime-
//! style codes; `-u` switches to UTC. `-d STR` parses an ISO 8601-ish
//! input; `-r FILE` uses the file's mtime; `-R` prints RFC 2822; `-I`
//! prints ISO 8601 to a specified precision (`date`, `hours`, `minutes`,
//! `seconds`).

use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "date",
    help: "print or format the date and time",
    aliases: &[],
    main,
};

const MONTH_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const DAY_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const DAY_ABBR: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, Copy)]
struct DateTime {
    year: i64,
    month: u32, // 1-12
    day: u32,   // 1-31
    hour: u32,
    minute: u32,
    second: u32,
    /// Day of week: 0 = Sunday.
    dow: u32,
    /// Day of year (1-366).
    doy: u32,
    /// Offset east of UTC, in seconds. We don't have a TZ DB, so we treat
    /// non-UTC as "unknown offset" and print `+0000` for parity. Setting
    /// `-u` makes that explicitly correct.
    utc_offset_secs: i64,
    is_utc: bool,
}

fn from_unix_secs(secs: i64, is_utc: bool) -> DateTime {
    // Always interpret as UTC since we have no TZ DB. `-u` is a label.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy_civil = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy_civil + 2) / 153;
    let d = (doy_civil - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    if m <= 2 {
        y += 1;
    }
    // Day of week: 1970-01-01 was Thursday (4).
    let dow = ((days % 7 + 4) % 7 + 7) % 7;
    // Day of year.
    let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let mdays = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut doy = d as i64;
    for i in 0..(m as usize - 1) {
        doy += mdays[i];
    }
    DateTime {
        year: y,
        month: m,
        day: d,
        hour: (rem / 3600) as u32,
        minute: ((rem % 3600) / 60) as u32,
        second: (rem % 60) as u32,
        dow: dow as u32,
        doy: doy as u32,
        utc_offset_secs: 0,
        is_utc,
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_iso(s: &str) -> Option<i64> {
    // Accept: YYYY-MM-DD, YYYY-MM-DD HH:MM[:SS], YYYY-MM-DDTHH:MM[:SS][Z|±HH:MM].
    let s = s.trim();
    let (date, rest): (&str, &str) = match s.find(['T', ' ']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    if date.len() != 10 || date.as_bytes()[4] != b'-' || date.as_bytes()[7] != b'-' {
        return None;
    }
    let y: i64 = date[0..4].parse().ok()?;
    let m: u32 = date[5..7].parse().ok()?;
    let d: u32 = date[8..10].parse().ok()?;
    let mut hh = 0u32;
    let mut mm = 0u32;
    let mut ss = 0u32;
    let mut offset_secs = 0i64;
    if !rest.is_empty() {
        let (time_str, off): (&str, i64) = if let Some(stripped) = rest.strip_suffix('Z') {
            (stripped, 0)
        } else if let Some(idx) = rest.rfind(['+', '-']).filter(|&i| i > 0) {
            let (t, o) = rest.split_at(idx);
            let sign = if o.starts_with('+') { 1 } else { -1 };
            let (oh, om) = o[1..].split_once(':').unwrap_or((&o[1..], "00"));
            let off_secs = sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60);
            (t, off_secs)
        } else {
            (rest, 0)
        };
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() < 2 {
            return None;
        }
        hh = parts[0].parse().ok()?;
        mm = parts[1].parse().ok()?;
        if let Some(s) = parts.get(2) {
            ss = s.parse().ok()?;
        }
        offset_secs = off;
    }
    let utc = unix_time(y, m, d, hh, mm, ss)?;
    Some(utc - offset_secs)
}

fn unix_time(y: i64, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh >= 24 || mm >= 60 || ss >= 60 {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh as i64 * 3600 + mm as i64 * 60 + ss as i64)
}

fn pad2(n: u32) -> String {
    format!("{n:02}")
}

fn strftime(dt: &DateTime, fmt: &str) -> String {
    let mut out = String::with_capacity(fmt.len());
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            let c = bytes[i + 1];
            i += 2;
            match c {
                b'Y' => out.push_str(&format!("{:04}", dt.year)),
                b'y' => out.push_str(&format!("{:02}", dt.year.rem_euclid(100))),
                b'C' => out.push_str(&format!("{:02}", dt.year.div_euclid(100))),
                b'm' => out.push_str(&pad2(dt.month)),
                b'd' => out.push_str(&pad2(dt.day)),
                b'e' => out.push_str(&format!("{:>2}", dt.day)),
                b'H' => out.push_str(&pad2(dt.hour)),
                b'I' => {
                    let h12 = match dt.hour {
                        0 => 12,
                        h if h > 12 => h - 12,
                        h => h,
                    };
                    out.push_str(&pad2(h12));
                }
                b'M' => out.push_str(&pad2(dt.minute)),
                b'S' => out.push_str(&pad2(dt.second)),
                b'p' => out.push_str(if dt.hour < 12 { "AM" } else { "PM" }),
                b'a' => out.push_str(DAY_ABBR[dt.dow as usize]),
                b'A' => out.push_str(DAY_FULL[dt.dow as usize]),
                b'b' | b'h' => out.push_str(MONTH_ABBR[dt.month as usize - 1]),
                b'B' => out.push_str(MONTH_FULL[dt.month as usize - 1]),
                b'j' => out.push_str(&format!("{:03}", dt.doy)),
                b'w' => out.push_str(&dt.dow.to_string()),
                b'u' => {
                    let iso = if dt.dow == 0 { 7 } else { dt.dow };
                    out.push_str(&iso.to_string());
                }
                b'T' => out.push_str(&format!("{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second)),
                b'R' => out.push_str(&format!("{:02}:{:02}", dt.hour, dt.minute)),
                b'D' => out.push_str(&format!(
                    "{:02}/{:02}/{:02}",
                    dt.month,
                    dt.day,
                    dt.year.rem_euclid(100)
                )),
                b'F' => out.push_str(&format!("{:04}-{:02}-{:02}", dt.year, dt.month, dt.day)),
                b'z' => {
                    let sign = if dt.utc_offset_secs >= 0 { '+' } else { '-' };
                    let mag = dt.utc_offset_secs.abs();
                    out.push_str(&format!("{sign}{:02}{:02}", mag / 3600, (mag / 60) % 60));
                }
                b'Z' => out.push_str(if dt.is_utc { "UTC" } else { "" }),
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'%' => out.push('%'),
                other => {
                    out.push('%');
                    out.push(other as char);
                }
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut utc = false;
    let mut d_arg: Option<String> = None;
    let mut r_arg: Option<String> = None;
    let mut fmt: Option<String> = None;
    let mut iso_spec: Option<String> = None;
    let mut rfc_2822 = false;

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            break;
        }
        match a.as_str() {
            "-u" | "--utc" | "--universal" => {
                utc = true;
                i += 1;
            }
            "-d" | "--date" if i + 1 < args.len() => {
                d_arg = Some(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with("--date=") => {
                d_arg = Some(s["--date=".len()..].to_string());
                i += 1;
            }
            "-r" if i + 1 < args.len() => {
                r_arg = Some(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with("--reference=") => {
                r_arg = Some(s["--reference=".len()..].to_string());
                i += 1;
            }
            "-R" | "--rfc-2822" | "--rfc-email" => {
                rfc_2822 = true;
                i += 1;
            }
            "-I" => {
                iso_spec = Some("date".to_string());
                i += 1;
            }
            s if s.starts_with("-I") => {
                let spec = &s[2..];
                if !matches!(spec, "date" | "hours" | "minutes" | "seconds" | "ns") {
                    err("date", &format!("invalid --iso-8601 arg: {spec}"));
                    return 2;
                }
                iso_spec = Some(spec.to_string());
                i += 1;
            }
            s if s.starts_with("--iso-8601") => {
                let spec = if let Some(eq) = s.find('=') {
                    &s[eq + 1..]
                } else {
                    "date"
                };
                if !matches!(spec, "date" | "hours" | "minutes" | "seconds" | "ns") {
                    err("date", &format!("invalid --iso-8601 arg: {spec}"));
                    return 2;
                }
                iso_spec = Some(spec.to_string());
                i += 1;
            }
            s if s.starts_with('+') => {
                fmt = Some(s[1..].to_string());
                i += 1;
            }
            s if s.starts_with('-') => {
                err("date", &format!("invalid option: {s}"));
                return 2;
            }
            _ => break,
        }
    }

    // Pick the timestamp.
    let secs: i64 = if let Some(rp) = &r_arg {
        match std::fs::metadata(rp).and_then(|m| m.modified()) {
            Ok(t) => t
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            Err(e) => {
                err_path("date", rp, &e);
                return 1;
            }
        }
    } else if let Some(s) = &d_arg {
        match parse_iso(s) {
            Some(n) => n,
            None => {
                err("date", &format!("invalid date: '{s}'"));
                return 1;
            }
        }
    } else {
        now_unix()
    };

    let dt = from_unix_secs(secs, utc);
    let output = if let Some(f) = &fmt {
        strftime(&dt, f)
    } else if rfc_2822 {
        // %a, %d %b %Y %H:%M:%S %z
        format!(
            "{}, {} {} {:04} {:02}:{:02}:{:02} +0000",
            DAY_ABBR[dt.dow as usize],
            dt.day,
            MONTH_ABBR[dt.month as usize - 1],
            dt.year,
            dt.hour,
            dt.minute,
            dt.second
        )
    } else if let Some(spec) = &iso_spec {
        match spec.as_str() {
            "date" => format!("{:04}-{:02}-{:02}", dt.year, dt.month, dt.day),
            "hours" => format!(
                "{:04}-{:02}-{:02}T{:02}+0000",
                dt.year, dt.month, dt.day, dt.hour
            ),
            "minutes" => format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}+0000",
                dt.year, dt.month, dt.day, dt.hour, dt.minute
            ),
            "seconds" => format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+0000",
                dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
            ),
            "ns" => format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000000000+0000",
                dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
            ),
            _ => unreachable!(),
        }
    } else {
        // Default GNU layout: "Sat Apr 26 12:34:56 2026".
        format!(
            "{} {} {:>2} {:02}:{:02}:{:02} {:04}",
            DAY_ABBR[dt.dow as usize],
            MONTH_ABBR[dt.month as usize - 1],
            dt.day,
            dt.hour,
            dt.minute,
            dt.second,
            dt.year
        )
    };
    println!("{output}");
    0
}
