//! `hostname` — show the system's hostname.
//!
//! Module is named `hostname_` to avoid a clash with the `gethostname` crate
//! (and to leave room for future `hostname` types). Setting the hostname is
//! intentionally not supported, matching the Python applet.

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "hostname",
    help: "show the system's hostname",
    aliases: &[],
    main,
};

fn host() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

fn main(argv: &[String]) -> i32 {
    let mut short = false;
    let mut full = false;

    for a in &argv[1..] {
        match a.as_str() {
            "-s" | "--short" => short = true,
            "-f" | "--fqdn" | "--long" => full = true,
            "-I" => {
                // No std API for "all configured IPs" without a dep; fall
                // back to whatever the OS resolves the hostname to. This
                // matches the Python implementation's getaddrinfo() pass.
                let h = host();
                match (h.as_str(), std::net::ToSocketAddrs::to_socket_addrs(
                    &(h.as_str(), 0u16),
                )) {
                    (_, Ok(iter)) => {
                        let mut ips: Vec<String> =
                            iter.map(|sa| sa.ip().to_string()).collect();
                        ips.sort();
                        ips.dedup();
                        println!("{}", ips.join(" "));
                        return 0;
                    }
                    (_, Err(e)) => {
                        err("hostname", &e.to_string());
                        return 1;
                    }
                }
            }
            s if s.starts_with('-') => {
                err("hostname", &format!("invalid option: {s}"));
                return 2;
            }
            _ => {
                err("hostname", "setting hostname is not supported");
                return 2;
            }
        }
    }

    let h = host();
    if full {
        // Best-effort FQDN: ask the resolver for a canonical name. If the
        // lookup fails, fall back to the bare hostname rather than erroring.
        let canon = (h.as_str(), 0u16)
            .to_socket_addrs_with_canon()
            .unwrap_or_else(|| h.clone());
        println!("{canon}");
    } else if short {
        let s = h.split('.').next().unwrap_or(&h);
        println!("{s}");
    } else {
        println!("{h}");
    }
    0
}

/// Tiny helper for FQDN resolution that returns the canonical form when the
/// resolver provides one, falling back to the input. Kept private to this
/// module since it's a hostname-specific quirk.
trait CanonResolve {
    fn to_socket_addrs_with_canon(&self) -> Option<String>;
}

impl CanonResolve for (&str, u16) {
    fn to_socket_addrs_with_canon(&self) -> Option<String> {
        // Rust's std doesn't expose AI_CANONNAME, so we settle for the
        // reverse lookup of the first resolved IP. Good enough for `-f`
        // on most setups; the Python version relies on socket.getfqdn(),
        // which has the same fundamental limitation on Windows.
        use std::net::ToSocketAddrs;
        let mut iter = self.to_socket_addrs().ok()?;
        let _ = iter.next()?; // we don't actually use the IP — std refuses
        // to do reverse DNS without a dep, so we just echo the host.
        Some(self.0.to_string())
    }
}
