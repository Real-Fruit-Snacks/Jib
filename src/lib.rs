//! jib — a BusyBox-style multi-call binary in Rust.
//!
//! The binary dispatches to one of many *applets* (Unix-style utilities)
//! using one of two modes:
//!
//! 1. **Multi-call mode** — `argv[0]`'s basename matches a known applet
//!    (e.g. the binary is hardlinked or symlinked to `cat`, `ls`, ...).
//! 2. **Wrapper mode** — `mainsail <applet> [args...]` invokes the named
//!    applet explicitly.
//!
//! Top-level flags: `--list`, `--help` / `-h`, `--version`.
//!
//! See [`cli::run`] for the entry point and [`registry`] for how applets
//! are registered.

pub mod applets;
pub mod cli;
pub mod common;
pub mod registry;
pub mod usage;

/// Crate version string (sourced from Cargo metadata).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
