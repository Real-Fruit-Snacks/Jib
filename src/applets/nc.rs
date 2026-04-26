//! `nc` — minimal TCP netcat. Supports client mode (`HOST PORT`), listen
//! mode (`-l -p PORT`), and a port-scan mode (`-z HOST PORT[-PORT]`).
//! UDP (`-u`) is intentionally rejected. Bidirectional pumping uses two
//! threads: stdin → socket and socket → stdout.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "nc",
    help: "TCP netcat — client, listener, and port scanner",
    aliases: &[],
    main,
};

fn pump_to_eof<R: Read + Send + 'static, W: Write + Send + 'static>(mut r: R, mut w: W) {
    let mut buf = [0u8; 64 * 1024];
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if w.write_all(&buf[..n]).is_err() {
                    break;
                }
                let _ = w.flush();
            }
            Err(_) => break,
        }
    }
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut listen = false;
    let mut port: Option<u16> = None;
    let mut scan = false;
    let mut timeout_secs: Option<u64> = None;

    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-l" => {
                listen = true;
                i += 1;
            }
            "-p" if i + 1 < args.len() => {
                port = args[i + 1].parse().ok();
                i += 2;
            }
            "-z" => {
                scan = true;
                i += 1;
            }
            "-u" => {
                err("nc", "UDP is not supported");
                return 2;
            }
            "-w" if i + 1 < args.len() => {
                timeout_secs = args[i + 1].parse().ok();
                i += 2;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("nc", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                positional.push(a.clone());
                i += 1;
            }
        }
    }

    if listen {
        let p = match port.or_else(|| positional.first().and_then(|s| s.parse().ok())) {
            Some(p) => p,
            None => {
                err("nc", "listen mode requires -p PORT");
                return 2;
            }
        };
        let addr = format!("0.0.0.0:{p}");
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                err("nc", &format!("bind {addr}: {e}"));
                return 1;
            }
        };
        let (sock, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) => {
                err("nc", &e.to_string());
                return 1;
            }
        };
        let sock_in = sock.try_clone().unwrap();
        let sock_out = sock;
        let h1 = thread::spawn(move || pump_to_eof(io::stdin(), sock_out));
        pump_to_eof(sock_in, io::stdout());
        let _ = h1.join();
        return 0;
    }

    if scan {
        let host = positional.first().cloned().unwrap_or_default();
        let spec = positional.get(1).cloned().unwrap_or_default();
        let (lo, hi) = if let Some((a, b)) = spec.split_once('-') {
            match (a.parse::<u16>(), b.parse::<u16>()) {
                (Ok(a), Ok(b)) => (a, b),
                _ => {
                    err("nc", &format!("invalid port range: {spec}"));
                    return 2;
                }
            }
        } else {
            match spec.parse::<u16>() {
                Ok(p) => (p, p),
                Err(_) => {
                    err("nc", &format!("invalid port: {spec}"));
                    return 2;
                }
            }
        };
        for p in lo..=hi {
            let addr = format!("{host}:{p}");
            let to = Duration::from_secs(timeout_secs.unwrap_or(2));
            if let Ok(mut iter) = (host.as_str(), p).to_socket_addrs() {
                if let Some(sa) = iter.next() {
                    if TcpStream::connect_timeout(&sa, to).is_ok() {
                        println!("Connection to {addr} succeeded");
                    }
                }
            }
        }
        return 0;
    }

    // Client mode.
    if positional.len() < 2 {
        err("nc", "usage: nc [OPTIONS] HOST PORT");
        return 2;
    }
    let host = positional[0].clone();
    let p: u16 = match positional[1].parse() {
        Ok(p) => p,
        Err(_) => {
            err("nc", &format!("invalid port: {}", positional[1]));
            return 2;
        }
    };
    let to = Duration::from_secs(timeout_secs.unwrap_or(10));
    let sa = match (host.as_str(), p)
        .to_socket_addrs()
        .ok()
        .and_then(|mut it| it.next())
    {
        Some(s) => s,
        None => {
            err("nc", &format!("could not resolve {host}"));
            return 1;
        }
    };
    let sock = match TcpStream::connect_timeout(&sa, to) {
        Ok(s) => s,
        Err(e) => {
            err("nc", &format!("connect {host}:{p}: {e}"));
            return 1;
        }
    };
    let sock_in = sock.try_clone().unwrap();
    let sock_out = sock;
    let h1 = thread::spawn(move || pump_to_eof(io::stdin(), sock_out));
    pump_to_eof(sock_in, io::stdout());
    let _ = h1.join();
    0
}
