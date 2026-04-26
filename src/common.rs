//! Shared helpers used by multiple applets.

use std::fs::Metadata;
use std::io::{self, Write};

/// Write `<applet>: <msg>\n` to stderr.
pub fn err(applet: &str, msg: &str) {
    let _ = writeln!(io::stderr(), "{applet}: {msg}");
}

/// Write `<applet>: <path>: <io-error>\n` to stderr.
pub fn err_path(applet: &str, path: &str, e: &io::Error) {
    let _ = writeln!(io::stderr(), "{applet}: {path}: {e}");
}

/// 16-bit Unix mode for `meta`, faked on Windows from file attributes.
///
/// On Unix returns `st_mode & 0xFFFF`. On Windows we synthesize a mode the
/// way Python's `os.stat` does: directory → `0o40555` (or `0o40755` if
/// writable), regular file → `0o100444` (or `0o100644` if writable),
/// symlink → `0o120777`. Executable bit is set on `.exe`/`.bat`/`.cmd`/
/// `.com`/`.ps1`.
#[allow(clippy::needless_return)]
pub fn unix_mode(meta: &Metadata, path: &std::path::Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return meta.mode();
    }
    #[cfg(windows)]
    {
        let mut mode: u32 = 0;
        let ro = meta.permissions().readonly();
        if meta.file_type().is_symlink() {
            return 0o120_777;
        }
        if meta.is_dir() {
            mode |= 0o040_000; // S_IFDIR
            mode |= if ro { 0o555 } else { 0o755 };
        } else {
            mode |= 0o100_000; // S_IFREG
            mode |= if ro { 0o444 } else { 0o644 };
            // Mark Windows-executables as +x in all classes for parity.
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let e = ext.to_ascii_lowercase();
                if matches!(e.as_str(), "exe" | "bat" | "cmd" | "com" | "ps1") {
                    mode |= 0o111;
                }
            }
        }
        mode
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (meta, path);
        0
    }
}

/// Render a Unix mode word as the canonical 10-character string used by
/// `ls -l` and `stat` (e.g. `drwxr-xr-x`). Matches Python's
/// `stat.filemode`.
pub fn filemode(mode: u32) -> String {
    let kind = match mode & 0o170_000 {
        0o040_000 => 'd',
        0o020_000 => 'c',
        0o060_000 => 'b',
        0o010_000 => 'p',
        0o140_000 => 's',
        0o120_000 => 'l',
        0o100_000 => '-',
        _ => '?',
    };
    fn rwx(m: u32, shift: u32, sticky_bit: u32, sticky_char_lower: char) -> String {
        let r = if m & (0o400 >> shift) != 0 { 'r' } else { '-' };
        let w = if m & (0o200 >> shift) != 0 { 'w' } else { '-' };
        let x_set = m & (0o100 >> shift) != 0;
        let sticky_set = m & sticky_bit != 0;
        let x = match (x_set, sticky_set) {
            (true, true) => sticky_char_lower,
            (false, true) => sticky_char_lower.to_ascii_uppercase(),
            (true, false) => 'x',
            (false, false) => '-',
        };
        format!("{r}{w}{x}")
    }
    let mut out = String::with_capacity(10);
    out.push(kind);
    out.push_str(&rwx(mode, 0, 0o4000, 's')); // user (suid)
    out.push_str(&rwx(mode, 3, 0o2000, 's')); // group (sgid)
    out.push_str(&rwx(mode, 6, 0o1000, 't')); // other (sticky)
    out
}

/// Best-effort user-name lookup. Without a libc/winapi dep we just
/// stringify the numeric UID. TODO: add `users` crate or libc behind a
/// feature flag for real name resolution.
pub fn user_name(uid: u32) -> String {
    uid.to_string()
}

/// Best-effort group-name lookup. See [`user_name`].
pub fn group_name(gid: u32) -> String {
    gid.to_string()
}

/// Return `(uid, gid)` for `meta`; on Windows we always return `(0, 0)`
/// since Windows doesn't have POSIX UIDs.
pub fn uid_gid(meta: &Metadata) -> (u32, u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (meta.uid(), meta.gid())
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        (0, 0)
    }
}

/// Number of hard links to `meta`. 1 if the platform doesn't expose it.
pub fn nlink(meta: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.nlink()
    }
    #[cfg(not(unix))]
    {
        // Windows exposes link count only behind nightly's `windows_by_handle`.
        // Stable Rust has no portable way; report 1.
        let _ = meta;
        1
    }
}

/// Inode (or 0 if the platform doesn't expose one).
pub fn inode(meta: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0
    }
}
