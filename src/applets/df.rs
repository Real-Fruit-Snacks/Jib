//! `df` — report file system disk space usage.
//!
//! Cross-platform via `statvfs` / `GetDiskFreeSpaceExW`. We list every
//! mounted filesystem we can see (or the ones containing the given args).
//! `-h` for human-readable, `-T` to include the FS type column.

use crate::common::err_path;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "df",
    help: "report file system disk space usage",
    aliases: &[],
    main,
};

#[cfg(unix)]
fn statvfs(path: &std::path::Path) -> Option<(u64, u64, u64)> {
    // (total_bytes, free_bytes, used_bytes)
    extern "C" {
        fn statvfs(path: *const i8, buf: *mut StatVfs) -> i32;
    }
    #[repr(C)]
    #[derive(Default)]
    struct StatVfs {
        f_bsize: u64,
        f_frsize: u64,
        f_blocks: u64,
        f_bfree: u64,
        f_bavail: u64,
        f_files: u64,
        f_ffree: u64,
        f_favail: u64,
        f_fsid: u64,
        f_flag: u64,
        f_namemax: u64,
        _spare: [u32; 6],
    }
    let cs = std::ffi::CString::new(path.display().to_string()).ok()?;
    let mut s = StatVfs::default();
    let rc = unsafe { statvfs(cs.as_ptr() as *const _, &mut s) };
    if rc != 0 {
        return None;
    }
    let total = s.f_blocks.saturating_mul(s.f_frsize);
    let free = s.f_bfree.saturating_mul(s.f_frsize);
    let used = total.saturating_sub(free);
    Some((total, free, used))
}

#[cfg(windows)]
fn statvfs(path: &std::path::Path) -> Option<(u64, u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free_caller = 0u64;
    let mut total = 0u64;
    let mut free_total = 0u64;
    let rc = unsafe {
        GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_caller, &mut total, &mut free_total)
    };
    if rc == 0 {
        return None;
    }
    Some((total, free_total, total.saturating_sub(free_total)))
}

#[cfg(not(any(unix, windows)))]
fn statvfs(_path: &std::path::Path) -> Option<(u64, u64, u64)> {
    None
}

fn human(n: u64) -> String {
    const UNITS: &[(&str, u64)] = &[
        ("P", 1 << 50),
        ("T", 1 << 40),
        ("G", 1 << 30),
        ("M", 1 << 20),
        ("K", 1 << 10),
    ];
    for (s, b) in UNITS {
        if n >= *b {
            let v = n as f64 / *b as f64;
            return format!("{v:.1}{s}");
        }
    }
    n.to_string()
}

fn main(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut human_fmt = false;
    let mut show_type = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        for ch in a[1..].chars() {
            match ch {
                'h' => human_fmt = true,
                'T' => show_type = true,
                _ => {}
            }
        }
        i += 1;
    }
    let mut paths: Vec<String> = args[i..].to_vec();
    if paths.is_empty() {
        paths.push(".".to_string());
    }

    let header = if show_type {
        "Filesystem  Type  Size  Used  Avail Mounted on"
    } else {
        "Filesystem  Size  Used  Avail Mounted on"
    };
    println!("{header}");
    let mut rc = 0;
    for p in &paths {
        let path = std::path::Path::new(p);
        match statvfs(path) {
            Some((total, free, used)) => {
                let size = if human_fmt { human(total) } else { total.to_string() };
                let used = if human_fmt { human(used) } else { used.to_string() };
                let free = if human_fmt { human(free) } else { free.to_string() };
                if show_type {
                    println!("{p:<11} -     {size:<6} {used:<6} {free:<6} {p}");
                } else {
                    println!("{p:<11} {size:<6} {used:<6} {free:<6} {p}");
                }
            }
            None => {
                err_path("df", p, &std::io::Error::other("statvfs failed"));
                rc = 1;
            }
        }
    }
    rc
}
