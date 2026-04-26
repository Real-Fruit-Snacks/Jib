//! `zip` and `unzip` — built on the `zip` crate.
//!
//! `zip ARCHIVE FILE...` adds files (recursing into directories).
//! `unzip [-l] [-d DIR] ARCHIVE` extracts (or with `-l`, lists).

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::common::{err, err_path};
use crate::registry::Applet;

pub const ZIP: Applet = Applet {
    name: "zip",
    help: "package and compress files into a zip archive",
    aliases: &[],
    main: main_zip,
};
pub const UNZIP: Applet = Applet {
    name: "unzip",
    help: "extract files from a zip archive",
    aliases: &[],
    main: main_unzip,
};

fn main_zip(argv: &[String]) -> i32 {
    let args = &argv[1..];
    if args.len() < 2 {
        err("zip", "usage: zip ARCHIVE FILE...");
        return 2;
    }
    let archive = &args[0];
    let files = &args[1..];

    let outf = match File::create(archive) {
        Ok(f) => f,
        Err(e) => {
            err_path("zip", archive, &e);
            return 1;
        }
    };
    let mut zw = zip::ZipWriter::new(outf);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for f in files {
        if let Err(e) = add_path(&mut zw, Path::new(f), Path::new(""), opts) {
            err_path("zip", f, &io::Error::other(e));
            return 1;
        }
    }
    if let Err(e) = zw.finish() {
        err("zip", &e.to_string());
        return 1;
    }
    0
}

fn add_path(
    zw: &mut zip::ZipWriter<File>,
    path: &Path,
    rel_root: &Path,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let rel = if rel_root.as_os_str().is_empty() {
        path.to_path_buf()
    } else {
        rel_root.join(path.file_name().unwrap_or_default())
    };
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        let entry_name = format!("{}/", rel.display()).replace('\\', "/");
        zw.add_directory(entry_name, opts)
            .map_err(|e| e.to_string())?;
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            add_path(zw, &entry.path(), &rel, opts)?;
        }
        return Ok(());
    }
    let name = rel.display().to_string().replace('\\', "/");
    zw.start_file(name, opts).map_err(|e| e.to_string())?;
    let mut fh = File::open(path).map_err(|e| e.to_string())?;
    io::copy(&mut fh, zw).map_err(|e| e.to_string())?;
    Ok(())
}

fn main_unzip(argv: &[String]) -> i32 {
    let mut args: Vec<String> = argv[1..].to_vec();
    let mut list_only = false;
    let mut dest: Option<String> = None;

    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--" {
            args.remove(i);
            break;
        }
        match a.as_str() {
            "-l" => {
                list_only = true;
                i += 1;
            }
            "-d" if i + 1 < args.len() => {
                dest = Some(args[i + 1].clone());
                i += 2;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("unzip", &format!("unknown option: {s}"));
                return 2;
            }
            _ => break,
        }
    }
    if i >= args.len() {
        err("unzip", "missing archive");
        return 2;
    }
    let archive = &args[i];
    let fh = match File::open(archive) {
        Ok(f) => f,
        Err(e) => {
            err_path("unzip", archive, &e);
            return 1;
        }
    };
    let mut zr = match zip::ZipArchive::new(fh) {
        Ok(z) => z,
        Err(e) => {
            err("unzip", &e.to_string());
            return 1;
        }
    };
    let dest_root = dest
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if list_only {
        for i in 0..zr.len() {
            let entry = match zr.by_index(i) {
                Ok(e) => e,
                Err(e) => {
                    err("unzip", &e.to_string());
                    return 1;
                }
            };
            println!("{}", entry.name());
        }
        return 0;
    }
    for i in 0..zr.len() {
        let mut entry = match zr.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                err("unzip", &e.to_string());
                return 1;
            }
        };
        let outpath = match entry.enclosed_name() {
            Some(p) => dest_root.join(p),
            None => continue,
        };
        if entry.is_dir() {
            if let Err(e) = std::fs::create_dir_all(&outpath) {
                err_path("unzip", &outpath.display().to_string(), &e);
                return 1;
            }
            continue;
        }
        if let Some(parent) = outpath.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut wh = match File::create(&outpath) {
            Ok(f) => f,
            Err(e) => {
                err_path("unzip", &outpath.display().to_string(), &e);
                return 1;
            }
        };
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = match entry.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    err("unzip", &e.to_string());
                    return 1;
                }
            };
            if let Err(e) = wh.write_all(&buf[..n]) {
                err("unzip", &e.to_string());
                return 1;
            }
        }
    }
    0
}
