//! `uname` — print system information.
//!
//! Best-effort cross-platform: the kernel-name (`-s`), nodename (`-n`),
//! and machine (`-m`) come from `std`/the host. Release/version (`-r`/`-v`)
//! and processor (`-p`) are reported as `unknown` for now — wiring those up
//! requires `libc::uname()` on Unix and registry/WMI on Windows, which we'll
//! add in a follow-up to keep the vertical slice dep-light.

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "uname",
    help: "print system information",
    aliases: &[],
    main,
};

fn kernel_name() -> &'static str {
    // Match `platform.system()` capitalization: "Linux", "Darwin", "Windows".
    match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "Darwin",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "netbsd" => "NetBSD",
        "openbsd" => "OpenBSD",
        "dragonfly" => "DragonFly",
        "solaris" | "illumos" => "SunOS",
        "android" => "Linux",
        other => other, // unknown — fall through with the lowercase name
    }
}

fn machine() -> &'static str {
    // Map Rust's ARCH names onto the `uname -m` strings GNU coreutils prints.
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" => "i686",
        "aarch64" => "aarch64",
        "arm" => "armv7l",
        "powerpc64" => "ppc64",
        "powerpc" => "ppc",
        "riscv64" => "riscv64",
        "s390x" => "s390x",
        "mips" => "mips",
        "mips64" => "mips64",
        other => other,
    }
}

fn os_string() -> &'static str {
    if std::env::consts::OS == "linux" || std::env::consts::OS == "android" {
        "GNU/Linux"
    } else {
        kernel_name()
    }
}

fn add_unique(out: &mut Vec<char>, ch: char) {
    if !out.contains(&ch) {
        out.push(ch);
    }
}

fn add_all(out: &mut Vec<char>) {
    for ch in ['s', 'n', 'r', 'v', 'm', 'p', 'i', 'o'] {
        add_unique(out, ch);
    }
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut wanted: Vec<char> = Vec::new();

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            break;
        }
        match a.as_str() {
            "-a" | "--all" => add_all(&mut wanted),
            "--kernel-name" => add_unique(&mut wanted, 's'),
            "--nodename" => add_unique(&mut wanted, 'n'),
            "--kernel-release" => add_unique(&mut wanted, 'r'),
            "--kernel-version" => add_unique(&mut wanted, 'v'),
            "--machine" => add_unique(&mut wanted, 'm'),
            "--processor" => add_unique(&mut wanted, 'p'),
            "--hardware-platform" => add_unique(&mut wanted, 'i'),
            "--operating-system" => add_unique(&mut wanted, 'o'),
            s if s.starts_with('-') && s.len() > 1 => {
                let body = &s[1..];
                if !body.chars().all(|c| matches!(c, 's' | 'n' | 'r' | 'v' | 'm' | 'p' | 'i' | 'o' | 'a'))
                {
                    err("uname", &format!("invalid option: {s}"));
                    return 2;
                }
                for ch in body.chars() {
                    if ch == 'a' {
                        add_all(&mut wanted);
                    } else {
                        add_unique(&mut wanted, ch);
                    }
                }
            }
            _ => break,
        }
        i += 1;
    }

    if wanted.is_empty() {
        wanted.push('s');
    }

    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let parts: Vec<String> = wanted
        .iter()
        .map(|&ch| match ch {
            's' => kernel_name().to_string(),
            'n' => host.clone(),
            'r' => "unknown".to_string(),
            'v' => "unknown".to_string(),
            'm' => machine().to_string(),
            'p' => "unknown".to_string(),
            'i' => machine().to_string(),
            'o' => os_string().to_string(),
            _ => "unknown".to_string(),
        })
        .collect();

    println!("{}", parts.join(" "));
    0
}
