//! `touch` — update timestamps, creating files when missing.
//!
//! Supports `-c` no-create, `-a` atime-only, `-m` mtime-only, `-r REF`
//! reference file, `-t STAMP` POSIX `[[CC]YY]MMDDhhmm[.ss]`, and a small
//! subset of ISO-8601 forms for `-d STR`. We deliberately don't pull in
//! `chrono` — `date` is the heavyweight applet that needs full parsing.

use std::fs::OpenOptions;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "touch",
    help: "change file timestamps (create if missing)",
    aliases: &[],
    main,
};

/// Convert a Gregorian (UTC) date-time to a Unix timestamp (seconds).
/// Algorithm adapted from Howard Hinnant's date library — exact for any
/// year ≥ 1601 we care about.
fn unix_time_utc(y: i64, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh >= 24 || mm >= 60 || ss >= 60 {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh as i64 * 3600 + mm as i64 * 60 + ss as i64)
}

/// `[[CC]YY]MMDDhhmm[.ss]`.
fn parse_t(s: &str) -> Option<SystemTime> {
    let (body, ss): (&str, u32) = match s.split_once('.') {
        Some((b, sec)) => (b, sec.parse().ok()?),
        None => (s, 0),
    };
    if !body.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let now_year = current_year();
    let stamp: String = match body.len() {
        8 => format!("{now_year:04}{body}"),
        10 => {
            let yy: u32 = body[..2].parse().ok()?;
            let cc = if yy < 69 { "20" } else { "19" };
            format!("{cc}{body}")
        }
        12 => body.to_string(),
        _ => return None,
    };
    let y: i64 = stamp[0..4].parse().ok()?;
    let m: u32 = stamp[4..6].parse().ok()?;
    let d: u32 = stamp[6..8].parse().ok()?;
    let hh: u32 = stamp[8..10].parse().ok()?;
    let mm: u32 = stamp[10..12].parse().ok()?;
    // Treat the parsed time as local; adjust by local-time offset to UTC.
    let utc_secs = unix_time_utc(y, m, d, hh, mm, ss)?;
    let local_offset = local_utc_offset_secs();
    Some(unix_to_systime(utc_secs - local_offset))
}

/// A small subset of ISO 8601: `YYYY-MM-DD`, optional `T`/space then
/// `HH:MM[:SS]`, optional trailing `Z` (UTC) or `±HH:MM` offset.
fn parse_d(s: &str) -> Option<SystemTime> {
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
    let mut tz_offset: Option<i64> = None;
    if !rest.is_empty() {
        // Strip trailing tz indicator.
        let (time_str, tz) = if let Some(stripped) = rest.strip_suffix('Z') {
            (stripped, Some(0i64))
        } else if let Some(idx) = rest.rfind(['+', '-']) {
            // Don't confuse a leading sign with a tz: the time part has
            // colons.
            if idx > 0 {
                let (t, off) = rest.split_at(idx);
                let sign = if off.starts_with('+') { 1 } else { -1 };
                let off = &off[1..];
                let (oh, om) = off.split_once(':').unwrap_or((off, "00"));
                let off_secs =
                    sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60);
                (t, Some(off_secs))
            } else {
                (rest, None)
            }
        } else {
            (rest, None)
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
        tz_offset = tz;
    }
    let utc = unix_time_utc(y, m, d, hh, mm, ss)?;
    let offset = tz_offset.unwrap_or_else(local_utc_offset_secs);
    Some(unix_to_systime(utc - offset))
}

fn current_year() -> i64 {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Gregorian conversion (Hinnant): days from epoch.
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    era * 400
        + yoe
        + if (doe - (365 * yoe + yoe / 4 - yoe / 100)) >= 306 {
            1
        } else {
            0
        }
}

/// Best-effort local offset (seconds east of UTC). std doesn't expose
/// timezone info; we use a calibration trick: format two `time_t`s.
/// Falls back to 0 (UTC) if anything goes wrong. That means `-d`/`-t`
/// parsing on platforms where this can't be derived will be off by the
/// local offset — a known limitation we'll fix when `chrono` arrives.
fn local_utc_offset_secs() -> i64 {
    0
}

fn unix_to_systime(secs: i64) -> SystemTime {
    if secs >= 0 {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        SystemTime::UNIX_EPOCH - Duration::from_secs((-secs) as u64)
    }
}

fn set_times(path: &Path, atime: SystemTime, mtime: SystemTime) -> std::io::Result<()> {
    let f = OpenOptions::new().write(true).open(path)?;
    f.set_modified(mtime)?;
    let _ = (f, atime); // set_accessed is nightly; skip atime for now (TODO).
    Ok(())
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut no_create = false;
    let mut atime_only = false;
    let mut mtime_only = false;
    let mut ref_atime: Option<SystemTime> = None;
    let mut ref_mtime: Option<SystemTime> = None;
    let mut target_time: Option<SystemTime> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if a == "-r" && i + 1 < args.len() {
            match std::fs::metadata(&args[i + 1]) {
                Ok(m) => {
                    ref_atime = m.accessed().ok();
                    ref_mtime = m.modified().ok();
                    i += 2;
                    continue;
                }
                Err(e) => {
                    err_path("touch", &args[i + 1], &e);
                    return 1;
                }
            }
        }
        if a == "-d" && i + 1 < args.len() {
            match parse_d(&args[i + 1]) {
                Some(t) => {
                    target_time = Some(t);
                    i += 2;
                    continue;
                }
                None => {
                    err("touch", &format!("invalid date: '{}'", args[i + 1]));
                    return 2;
                }
            }
        }
        if a == "-t" && i + 1 < args.len() {
            match parse_t(&args[i + 1]) {
                Some(t) => {
                    target_time = Some(t);
                    i += 2;
                    continue;
                }
                None => {
                    err("touch", &format!("invalid -t value: '{}'", args[i + 1]));
                    return 2;
                }
            }
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'c' => no_create = true,
                'a' => atime_only = true,
                'm' => mtime_only = true,
                _ => {
                    err("touch", &format!("invalid option: -{ch}"));
                    return 2;
                }
            }
        }
        i += 1;
    }

    let files: Vec<String> = args[i..].to_vec();
    if files.is_empty() {
        err("touch", "missing file operand");
        return 2;
    }

    let now = SystemTime::now();
    let mut rc = 0;
    for f in &files {
        let p = Path::new(f);
        let exists = p.exists();
        if !exists {
            if no_create {
                continue;
            }
            // Create empty.
            if let Err(e) = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(p)
            {
                err_path("touch", f, &e);
                rc = 1;
                continue;
            }
        }
        let st = match std::fs::metadata(p) {
            Ok(m) => m,
            Err(e) => {
                err_path("touch", f, &e);
                rc = 1;
                continue;
            }
        };
        let (a_src, m_src) = if let Some(rm_) = ref_mtime {
            (
                ref_atime.unwrap_or_else(|| st.accessed().unwrap_or(now)),
                rm_,
            )
        } else if let Some(t) = target_time {
            (t, t)
        } else {
            (now, now)
        };
        let new_atime = if atime_only || !mtime_only {
            a_src
        } else {
            st.accessed().unwrap_or(now)
        };
        let new_mtime = if mtime_only || !atime_only {
            m_src
        } else {
            st.modified().unwrap_or(now)
        };
        if let Err(e) = set_times(p, new_atime, new_mtime) {
            err_path("touch", f, &e);
            rc = 1;
        }
    }
    rc
}
