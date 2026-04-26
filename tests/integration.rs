//! Integration tests for the `jib` binary.
//!
//! These mirror a subset of the Python project's `tests/test_applets.py`
//! and cover top-level flags plus the trivial applets ported in this slice.

use std::env;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

/// Path to the freshly-built `jib` binary that Cargo built for tests.
fn bin_path() -> PathBuf {
    // Cargo sets CARGO_BIN_EXE_<name> for integration tests. See
    // https://doc.rust-lang.org/cargo/reference/environment-variables.html
    PathBuf::from(env!("CARGO_BIN_EXE_jib"))
}

struct Out {
    rc: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str]) -> Out {
    let output = Command::new(bin_path())
        .args(args)
        .output()
        .expect("failed to spawn jib");
    Out {
        rc: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// --- top-level --------------------------------------------------------------

#[test]
fn version_prints_program_and_version() {
    let r = run(&["--version"]);
    assert_eq!(r.rc, 0);
    assert!(r.stdout.starts_with("jib "), "got: {}", r.stdout);
}

#[test]
fn list_includes_ported_applets() {
    let r = run(&["--list"]);
    assert_eq!(r.rc, 0);
    for name in [
        "cat", "echo", "false", "hostname", "pwd", "sleep", "true", "uname", "whoami", "yes",
    ] {
        let needle1 = format!("  {name} ");
        let needle2 = format!("  {name}  ");
        assert!(
            r.stdout.contains(&needle1) || r.stdout.contains(&needle2),
            "missing applet `{name}` in --list output:\n{}",
            r.stdout
        );
    }
}

#[test]
fn unknown_applet_errors() {
    let r = run(&["bogus_xyz"]);
    assert_eq!(r.rc, 1);
    assert!(r.stderr.contains("unknown applet"));
}

#[test]
fn applet_help_via_subcommand() {
    // `jib cat --help` should print the usage block.
    let r = run(&["cat", "--help"]);
    assert_eq!(r.rc, 0);
    assert!(r.stdout.contains("cat - "));
    assert!(r.stdout.contains("Usage: cat"));
    assert!(r.stdout.contains("Aliases: type"));
}

#[test]
fn help_applet_form_works() {
    // `jib --help cat` should equal `jib cat --help`.
    let r = run(&["--help", "cat"]);
    assert_eq!(r.rc, 0);
    assert!(r.stdout.contains("cat - "));
    assert!(r.stdout.contains("Usage: cat"));
}

#[test]
fn every_listed_applet_has_help() {
    let r = run(&["--list"]);
    assert_eq!(r.rc, 0);
    for line in r.stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let name = trimmed.split_whitespace().next().unwrap();
        let h = run(&[name, "--help"]);
        assert_eq!(h.rc, 0, "{name} --help rc={} stderr={}", h.rc, h.stderr);
        let header = format!("{name} - ");
        assert!(
            h.stdout.contains(&header),
            "{name} --help missing header:\n{}",
            h.stdout
        );
    }
}

// --- trivial applets --------------------------------------------------------

#[test]
fn true_and_false_exit_codes() {
    assert_eq!(run(&["true"]).rc, 0);
    assert_eq!(run(&["false"]).rc, 1);
}

#[test]
fn echo_basic() {
    let r = run(&["echo", "hello", "world"]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "hello world\n");
}

#[test]
fn echo_no_newline() {
    let r = run(&["echo", "-n", "hi"]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "hi");
}

#[test]
fn echo_interpret_escapes() {
    let r = run(&["echo", "-e", "a\\nb"]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "a\nb\n");
}

#[test]
fn echo_uppercase_e_disables_escapes() {
    let r = run(&["echo", "-E", "a\\nb"]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "a\\nb\n");
}

#[test]
fn echo_combined_flag_block() {
    // `-ne` should set both no-newline and interpret-escapes.
    let r = run(&["echo", "-ne", "a\\tb"]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "a\tb");
}

// --- pwd --------------------------------------------------------------------

#[test]
fn pwd_prints_a_directory() {
    let r = run(&["pwd"]);
    assert_eq!(r.rc, 0);
    let p = PathBuf::from(r.stdout.trim());
    assert!(p.is_absolute(), "pwd output not absolute: {}", r.stdout);
}

#[test]
fn pwd_invalid_option_errors() {
    let r = run(&["pwd", "--no-such-flag"]);
    assert_eq!(r.rc, 2);
    assert!(r.stderr.contains("invalid option"));
}

// --- sleep ------------------------------------------------------------------

#[test]
fn sleep_zero_returns_quickly() {
    let r = run(&["sleep", "0"]);
    assert_eq!(r.rc, 0);
}

#[test]
fn sleep_invalid_returns_2() {
    let r = run(&["sleep", "abc"]);
    assert_eq!(r.rc, 2);
    assert!(r.stderr.contains("invalid time interval"));
}

#[test]
fn sleep_missing_operand() {
    let r = run(&["sleep"]);
    assert_eq!(r.rc, 2);
    assert!(r.stderr.contains("missing operand"));
}

#[test]
fn sleep_unit_suffix_parses() {
    // 10 milliseconds expressed as 0.01s should not error.
    let r = run(&["sleep", "0.01s"]);
    assert_eq!(r.rc, 0);
}

// --- uname / hostname / whoami ---------------------------------------------

#[test]
fn uname_default_is_kernel_name() {
    let r = run(&["uname"]);
    assert_eq!(r.rc, 0);
    let s = r.stdout.trim();
    assert!(!s.is_empty());
    assert!(
        !s.contains(' '),
        "default uname should be a single token, got `{s}`"
    );
}

#[test]
fn uname_dash_a_has_8_fields() {
    let r = run(&["uname", "-a"]);
    assert_eq!(r.rc, 0);
    let fields: Vec<&str> = r.stdout.split_whitespace().collect();
    assert_eq!(fields.len(), 8, "uname -a fields: {:?}", fields);
}

#[test]
fn hostname_prints_something() {
    let r = run(&["hostname"]);
    assert_eq!(r.rc, 0);
    assert!(!r.stdout.trim().is_empty());
}

#[test]
fn whoami_prints_something() {
    let r = run(&["whoami"]);
    // Skip if neither USER nor USERNAME is available — this happens in
    // some sandboxed CI environments. Treat it as "test inapplicable".
    if r.rc != 0 {
        assert!(r.stderr.contains("could not determine"));
        return;
    }
    assert!(!r.stdout.trim().is_empty());
}

// --- cat --------------------------------------------------------------------

#[test]
fn cat_concatenates_files() {
    let dir = TempDir::new().expect("tempdir");
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, b"hello\n").unwrap();
    std::fs::write(&b, b"world\n").unwrap();

    let r = run(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "hello\nworld\n");
}

#[test]
fn cat_n_numbers_lines() {
    let dir = TempDir::new().expect("tempdir");
    let p = dir.path().join("f.txt");
    std::fs::write(&p, b"foo\nbar\n").unwrap();

    let r = run(&["cat", "-n", p.to_str().unwrap()]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "     1\tfoo\n     2\tbar\n");
}

#[test]
fn cat_b_skips_blank_lines() {
    let dir = TempDir::new().expect("tempdir");
    let p = dir.path().join("f.txt");
    std::fs::write(&p, b"foo\n\nbar\n").unwrap();

    let r = run(&["cat", "-b", p.to_str().unwrap()]);
    assert_eq!(r.rc, 0);
    assert_eq!(r.stdout, "     1\tfoo\n\n     2\tbar\n");
}

#[test]
fn cat_missing_file_returns_1() {
    let r = run(&["cat", "this_file_definitely_does_not_exist.txt"]);
    assert_eq!(r.rc, 1);
    assert!(r
        .stderr
        .contains("cat: this_file_definitely_does_not_exist.txt"));
}

#[test]
fn cat_invalid_option_returns_2() {
    let r = run(&["cat", "-Z"]);
    assert_eq!(r.rc, 2);
    assert!(r.stderr.contains("invalid option"));
}

// --- multi-call dispatch ----------------------------------------------------

/// Copy the binary to `<tempdir>/<applet>(.exe)` and exec it. This proves
/// that `argv[0]` basename routing works.
#[test]
fn multicall_basename_dispatch() {
    let dir = TempDir::new().expect("tempdir");
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let dest = dir.path().join(format!("echo{suffix}"));
    std::fs::copy(bin_path(), &dest).expect("copy binary");

    let out = Command::new(&dest)
        .args(["hi", "there"])
        .output()
        .expect("spawn copied binary");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi there\n");
}

#[test]
fn multicall_alias_is_resolved() {
    // `type` is an alias for `cat`. Copying the binary as `type(.exe)` and
    // running it with a file should behave like `cat`.
    let dir = TempDir::new().expect("tempdir");
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let dest = dir.path().join(format!("type{suffix}"));
    std::fs::copy(bin_path(), &dest).expect("copy binary");

    let p = dir.path().join("hello.txt");
    std::fs::write(&p, b"hello world\n").unwrap();

    let out = Command::new(&dest)
        .arg(p.to_str().unwrap())
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello world\n");
}
