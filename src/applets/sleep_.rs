//! `sleep` — pause for the sum of all DURATIONs.
//!
//! Module is `sleep_` to avoid shadowing `std::thread::sleep`. Each duration
//! is `<number>[smhd]`. Negative or unparseable durations exit with code 2.
//! A Ctrl-C interrupt yields exit code 130 (POSIX 128 + SIGINT).

use std::thread;
use std::time::Duration;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "sleep",
    help: "delay for a specified amount of time",
    aliases: &[],
    main,
};

fn parse_duration(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let last = s.chars().last().unwrap();
    let (body, mult) = match last {
        's' => (&s[..s.len() - 1], 1.0),
        'm' => (&s[..s.len() - 1], 60.0),
        'h' => (&s[..s.len() - 1], 3600.0),
        'd' => (&s[..s.len() - 1], 86_400.0),
        _ => (s, 1.0),
    };
    body.parse::<f64>().ok().map(|n| n * mult)
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    if args.is_empty() {
        err("sleep", "missing operand");
        return 2;
    }
    let mut total = 0.0_f64;
    for a in args {
        match parse_duration(a) {
            Some(d) if d >= 0.0 && d.is_finite() => total += d,
            _ => {
                err("sleep", &format!("invalid time interval: '{a}'"));
                return 2;
            }
        }
    }
    // Cap at u64::MAX seconds; Duration::from_secs_f64 panics on absurd
    // values, so we guard explicitly.
    if total > (u64::MAX / 2) as f64 {
        err("sleep", "duration too large");
        return 2;
    }
    thread::sleep(Duration::from_secs_f64(total));
    0
}
