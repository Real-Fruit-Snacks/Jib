//! Per-applet `--help` body text, indexed by applet name.
//!
//! These map 1:1 with `mainsail/usage.py` in the Python project. Each entry
//! is the *body* of `mainsail <applet> --help`; the CLI prints a header line
//! (`<name> - <one-line help>`) above it.

/// Return the usage body for `name`, or `None` if no entry exists.
pub fn get(name: &str) -> Option<&'static str> {
    USAGE
        .iter()
        .find_map(|(k, v)| if *k == name { Some(*v) } else { None })
}

const USAGE: &[(&str, &str)] = &[
    (
        "cat",
        "\
Usage: cat [OPTION]... [FILE]...

Concatenate FILE(s) to standard output (stdin if no FILE or FILE is '-').

Options:
  -n            number every output line (right-justified, tab)
  -b            like -n but skip blank lines (wins over -n)
",
    ),
    (
        "echo",
        "\
Usage: echo [-neE] [STRING]...

Write STRING(s) to standard output, separated by spaces, terminated by a
newline.

Options:
  -n            do not output the trailing newline
  -e            interpret backslash escapes (\\n, \\t, \\\\, ...)
  -E            do not interpret backslash escapes (default)
",
    ),
    (
        "false",
        "\
Usage: false

Exit with status 1.
",
    ),
    (
        "hostname",
        "\
Usage: hostname [OPTION]

Show the system's hostname. Setting the hostname is not supported.

Options:
  -s, --short        strip the domain part
  -f, --fqdn, --long fully-qualified domain name
  -I                 print all configured IP addresses
",
    ),
    (
        "pwd",
        "\
Usage: pwd [-LP]

Print the name of the current working directory.

Options:
  -L            use $PWD even if it contains symlinks (default)
  -P            print the physical (resolved) directory
",
    ),
    (
        "sleep",
        "\
Usage: sleep DURATION...

Pause for the sum of all DURATIONs. Each DURATION is a non-negative number
followed by an optional unit: s (seconds, default), m (minutes), h (hours),
d (days).
",
    ),
    (
        "true",
        "\
Usage: true

Exit with status 0.
",
    ),
    (
        "uname",
        "\
Usage: uname [OPTION]...

Print system information. With no options, prints the kernel name (-s).

Options:
  -a, --all           all information, in this order: -snrvmpio
  -s, --kernel-name   kernel name
  -n, --nodename      network node hostname
  -r, --kernel-release
  -v, --kernel-version
  -m, --machine       machine hardware name
  -p, --processor     processor type (or 'unknown')
  -i, --hardware-platform
  -o, --operating-system
",
    ),
    (
        "whoami",
        "\
Usage: whoami

Print the effective user name.
",
    ),
    (
        "yes",
        "\
Usage: yes [STRING]...

Repeatedly print STRING (or 'y' if no STRING is given) until killed or the
output pipe is closed.
",
    ),
];
