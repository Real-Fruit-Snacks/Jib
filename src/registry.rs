//! Applet registry: the (name, help, aliases, entry-fn) table.
//!
//! Applets are registered statically in [`crate::applets`] via a `&[Applet]`
//! slice; lookup is a linear scan (the registry is tiny) but builds a map
//! on first call for repeated lookups.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// An entry point for an applet. Receives the full `argv` (with `argv[0]`
/// being the applet name as invoked) and returns a process exit code.
pub type AppletFn = fn(&[String]) -> i32;

/// Static description of one applet.
#[derive(Clone, Copy)]
pub struct Applet {
    /// Canonical name (e.g. `"cat"`).
    pub name: &'static str,
    /// One-line help string shown in `--list` and as the header of `--help`.
    pub help: &'static str,
    /// Alternate names that should also dispatch here (e.g. `cat` ↔ `type`).
    pub aliases: &'static [&'static str],
    /// Entry point. Returns the exit code.
    pub main: AppletFn,
}

fn build_map() -> BTreeMap<&'static str, &'static Applet> {
    let mut m = BTreeMap::new();
    for a in crate::applets::ALL {
        m.insert(a.name, a);
        for alias in a.aliases {
            m.insert(*alias, a);
        }
    }
    m
}

fn map() -> &'static BTreeMap<&'static str, &'static Applet> {
    static MAP: OnceLock<BTreeMap<&'static str, &'static Applet>> = OnceLock::new();
    MAP.get_or_init(build_map)
}

/// Look up an applet by canonical name or alias.
pub fn get(name: &str) -> Option<&'static Applet> {
    map().get(name).copied()
}

/// All registered applets, deduplicated (aliases excluded), sorted by name.
pub fn list() -> Vec<&'static Applet> {
    let mut seen: BTreeMap<&'static str, &'static Applet> = BTreeMap::new();
    for a in crate::applets::ALL {
        seen.insert(a.name, a);
    }
    seen.into_values().collect()
}
