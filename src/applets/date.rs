//! `date` — print or format the date and time.
//!
//! Default output: `Sat Apr 26 12:00:00 2026`. `+FORMAT` uses strftime-
//! style codes; `-u` switches to UTC. `-d STR` parses an ISO 8601-ish
//! input; `-r FILE` uses the file's mtime; `-R` prints RFC 2822; `-I`
//! prints ISO 8601 to a specified precision (`date`, `hours`, `minutes`,
//! `seconds`).
//!
//! Backed by `chrono` for strftime, calendar math, and local-tz lookup.

use std::time::UNIX_EPOCH;

use chrono::{DateTime, FixedOffset, Local, Offset, TimeZone, Utc};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "date",
    help: "print or format the date and time",
    aliases: &[],
    main,
};

/// Either UTC or the user's local timezone, retaining the right offset
/// for `%z`/`%Z` formatting and arithmetic. We keep the original instant
/// (`when`) plus the chosen zone so we can re-evaluate either formatter.
enum Zoned {
    Utc(DateTime<Utc>),
    Local(DateTime<Local>),
    Fixed(DateTime<FixedOffset>),
}

impl Zoned {
    fn format(&self, fmt: &str) -> String {
        match self {
            Zoned::Utc(d) => d.format(fmt).to_string(),
            Zoned::Local(d) => d.format(fmt).to_string(),
            Zoned::Fixed(d) => d.format(fmt).to_string(),
        }
    }
}

/// Parse a `-d` argument. Accepts:
/// - `YYYY-MM-DD` (treated as midnight in the chosen zone)
/// - `YYYY-MM-DD HH:MM[:SS]` (zone-naive — chosen zone applies)
/// - `YYYY-MM-DDTHH:MM:SS[Z|±HH:MM]` (RFC 3339)
/// - `@<unix>` (numeric epoch, GNU extension)
fn parse_d(s: &str, utc: bool) -> Option<Zoned> {
    let s = s.trim();
    if let Some(epoch_s) = s.strip_prefix('@') {
        let n: i64 = epoch_s.parse().ok()?;
        let inst = Utc.timestamp_opt(n, 0).single()?;
        return Some(if utc {
            Zoned::Utc(inst)
        } else {
            Zoned::Local(inst.with_timezone(&Local))
        });
    }
    // Try RFC 3339 first — it carries its own offset.
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(if utc {
            Zoned::Utc(d.with_timezone(&Utc))
        } else {
            Zoned::Fixed(d)
        });
    }
    // Also accept the variant with a space instead of `T`.
    if s.len() >= 19 && s.as_bytes()[10] == b' ' {
        let mut owned = s.to_string();
        unsafe {
            owned.as_bytes_mut()[10] = b'T';
        }
        if let Ok(d) = DateTime::parse_from_rfc3339(&owned) {
            return Some(if utc {
                Zoned::Utc(d.with_timezone(&Utc))
            } else {
                Zoned::Fixed(d)
            });
        }
    }
    // Date-only or zone-naive forms — interpret in the chosen zone.
    use chrono::NaiveDateTime;
    let formats = [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ];
    for fmt in formats {
        let parsed = if fmt == "%Y-%m-%d" {
            chrono::NaiveDate::parse_from_str(s, fmt)
                .ok()
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        } else {
            NaiveDateTime::parse_from_str(s, fmt).ok()
        };
        if let Some(naive) = parsed {
            return Some(if utc {
                Zoned::Utc(Utc.from_utc_datetime(&naive))
            } else {
                let local = Local.from_local_datetime(&naive).single()?;
                Zoned::Local(local)
            });
        }
    }
    None
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

    // Resolve the timestamp into a Zoned value.
    let zoned: Zoned = if let Some(rp) = &r_arg {
        match std::fs::metadata(rp).and_then(|m| m.modified()) {
            Ok(t) => {
                let secs = t
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let nanos = t
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                let inst = match Utc.timestamp_opt(secs, nanos).single() {
                    Some(d) => d,
                    None => {
                        err("date", "invalid file timestamp");
                        return 1;
                    }
                };
                if utc {
                    Zoned::Utc(inst)
                } else {
                    Zoned::Local(inst.with_timezone(&Local))
                }
            }
            Err(e) => {
                err_path("date", rp, &e);
                return 1;
            }
        }
    } else if let Some(s) = &d_arg {
        match parse_d(s, utc) {
            Some(z) => z,
            None => {
                err("date", &format!("invalid date: '{s}'"));
                return 1;
            }
        }
    } else {
        let now = Utc::now();
        if utc {
            Zoned::Utc(now)
        } else {
            Zoned::Local(now.with_timezone(&Local))
        }
    };

    let output = if let Some(f) = &fmt {
        zoned.format(f)
    } else if rfc_2822 {
        zoned.format("%a, %d %b %Y %H:%M:%S %z")
    } else if let Some(spec) = &iso_spec {
        match spec.as_str() {
            "date" => zoned.format("%Y-%m-%d"),
            "hours" => zoned.format("%Y-%m-%dT%H%z"),
            "minutes" => zoned.format("%Y-%m-%dT%H:%M%z"),
            "seconds" => zoned.format("%Y-%m-%dT%H:%M:%S%z"),
            "ns" => zoned.format("%Y-%m-%dT%H:%M:%S.%9f%z"),
            _ => unreachable!(),
        }
    } else {
        // GNU default layout. The trailing "UTC"/local-tz-abbrev mirrors
        // what coreutils prints when no format is requested.
        let tz_label = match &zoned {
            Zoned::Utc(_) => "UTC".to_string(),
            Zoned::Local(d) => d.offset().fix().to_string(),
            Zoned::Fixed(d) => d.offset().to_string(),
        };
        let _ = tz_label; // currently unused — we emit the classic 5-field form
        zoned.format("%a %b %e %H:%M:%S %Y")
    };
    println!("{output}");
    0
}
