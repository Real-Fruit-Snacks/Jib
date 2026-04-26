//! `dig` — minimal DNS resolver (UDP).
//!
//! Sends a single A/AAAA/MX/TXT/CNAME/NS/PTR query and prints the answers
//! GNU-style. Supports `@server`, `+short`, `-t TYPE`, `-x ADDR`,
//! `--timeout N`. The wire format is hand-rolled — no `hickory` dep.

use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "dig",
    help: "DNS resolver (UDP, hand-rolled wire format)",
    aliases: &[],
    main,
};

fn type_to_code(t: &str) -> Option<u16> {
    Some(match t.to_ascii_uppercase().as_str() {
        "A" => 1,
        "NS" => 2,
        "CNAME" => 5,
        "SOA" => 6,
        "PTR" => 12,
        "MX" => 15,
        "TXT" => 16,
        "AAAA" => 28,
        "ANY" => 255,
        _ => return None,
    })
}

fn code_to_type(c: u16) -> &'static str {
    match c {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        6 => "SOA",
        12 => "PTR",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        _ => "?",
    }
}

fn encode_qname(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() {
            continue;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

fn build_query(name: &str, qtype: u16) -> Vec<u8> {
    let mut buf = Vec::new();
    let id = std::process::id() as u16;
    buf.extend_from_slice(&id.to_be_bytes());
    buf.extend_from_slice(&0x0100u16.to_be_bytes()); // RD=1
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&encode_qname(name));
    buf.extend_from_slice(&qtype.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes()); // class IN
    buf
}

fn parse_name(buf: &[u8], mut pos: usize) -> (String, usize) {
    let mut out = String::new();
    let mut jumped = false;
    let mut consumed = 0usize;
    let start = pos;
    let mut hops = 0;
    loop {
        if pos >= buf.len() || hops > 50 {
            break;
        }
        let len = buf[pos];
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xc0 == 0xc0 {
            if pos + 1 >= buf.len() {
                break;
            }
            let off = (((len & 0x3f) as usize) << 8) | (buf[pos + 1] as usize);
            if !jumped {
                consumed = pos + 2 - start;
            }
            pos = off;
            jumped = true;
            hops += 1;
            continue;
        }
        let lab_end = pos + 1 + len as usize;
        if lab_end > buf.len() {
            break;
        }
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(&String::from_utf8_lossy(&buf[pos + 1..lab_end]));
        pos = lab_end;
    }
    if !jumped {
        consumed = pos - start;
    }
    (out, start + consumed)
}

fn read_u16(buf: &[u8], pos: usize) -> u16 {
    u16::from_be_bytes([buf[pos], buf[pos + 1]])
}

fn read_u32(buf: &[u8], pos: usize) -> u32 {
    u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]])
}

fn parse_response(buf: &[u8], short: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if buf.len() < 12 {
        return out;
    }
    let qdcount = read_u16(buf, 4);
    let ancount = read_u16(buf, 6);
    let mut pos = 12usize;
    for _ in 0..qdcount {
        let (_n, np) = parse_name(buf, pos);
        pos = np + 4;
    }
    for _ in 0..ancount {
        let (name, np) = parse_name(buf, pos);
        pos = np;
        if pos + 10 > buf.len() {
            break;
        }
        let rtype = read_u16(buf, pos);
        let _rclass = read_u16(buf, pos + 2);
        let ttl = read_u32(buf, pos + 4);
        let rdlen = read_u16(buf, pos + 8) as usize;
        pos += 10;
        if pos + rdlen > buf.len() {
            break;
        }
        let rdata = &buf[pos..pos + rdlen];
        pos += rdlen;
        let value = match rtype {
            1 if rdlen == 4 => format!("{}.{}.{}.{}", rdata[0], rdata[1], rdata[2], rdata[3]),
            28 if rdlen == 16 => {
                let mut parts = Vec::new();
                for i in (0..16).step_by(2) {
                    parts.push(format!(
                        "{:x}",
                        u16::from_be_bytes([rdata[i], rdata[i + 1]])
                    ));
                }
                parts.join(":")
            }
            5 | 2 | 12 => parse_name(buf, pos - rdlen).0,
            15 if rdlen >= 3 => {
                let pref = read_u16(buf, pos - rdlen);
                let (target, _) = parse_name(buf, pos - rdlen + 2);
                format!("{pref} {target}")
            }
            16 => {
                let mut s = String::new();
                let mut p = 0;
                while p < rdlen {
                    let l = rdata[p] as usize;
                    p += 1;
                    s.push_str(&String::from_utf8_lossy(&rdata[p..p + l]));
                    p += l;
                }
                format!("\"{s}\"")
            }
            _ => format!("<rdata {rdlen} bytes>"),
        };
        if short {
            out.push(value);
        } else {
            out.push(format!(
                "{name}\t{ttl}\tIN\t{}\t{}",
                code_to_type(rtype),
                value
            ));
        }
    }
    out
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut server = "1.1.1.1".to_string();
    let mut qtype = "A".to_string();
    let mut name: Option<String> = None;
    let mut short = false;
    let mut reverse_addr: Option<String> = None;
    let mut timeout_secs: u64 = 5;

    for a in args {
        if let Some(rest) = a.strip_prefix('@') {
            server = rest.to_string();
            continue;
        }
        if a == "+short" {
            short = true;
            continue;
        }
        if let Some(rest) = a.strip_prefix("--timeout=") {
            timeout_secs = rest.parse().unwrap_or(5);
            continue;
        }
    }
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-t" if i + 1 < args.len() => {
                qtype = args[i + 1].clone();
                i += 2;
            }
            "-x" if i + 1 < args.len() => {
                reverse_addr = Some(args[i + 1].clone());
                qtype = "PTR".to_string();
                i += 2;
            }
            s if s.starts_with('@') || s == "+short" || s.starts_with("--timeout") => {
                i += 1;
            }
            _ => {
                if !a.starts_with('-') && name.is_none() {
                    name = Some(a.clone());
                }
                i += 1;
            }
        }
    }

    let qname = match (reverse_addr, name) {
        (Some(addr), _) => {
            let parts: Vec<&str> = addr.split('.').collect();
            if parts.len() == 4 {
                let rev: Vec<String> = parts.iter().rev().map(|s| s.to_string()).collect();
                format!("{}.in-addr.arpa", rev.join("."))
            } else {
                addr
            }
        }
        (None, Some(n)) => n,
        (None, None) => {
            err("dig", "missing name");
            return 2;
        }
    };
    let code = match type_to_code(&qtype) {
        Some(c) => c,
        None => {
            err("dig", &format!("unknown type: {qtype}"));
            return 2;
        }
    };

    let sock = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            err("dig", &e.to_string());
            return 1;
        }
    };
    let _ = sock.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
    let dest_str = format!("{server}:53");
    let dest = match dest_str.to_socket_addrs().ok().and_then(|mut it| it.next()) {
        Some(s) => s,
        None => {
            err("dig", &format!("could not resolve {server}"));
            return 1;
        }
    };
    let q = build_query(&qname, code);
    if let Err(e) = sock.send_to(&q, dest) {
        err("dig", &e.to_string());
        return 1;
    }
    let mut buf = vec![0u8; 4096];
    let n = match sock.recv(&mut buf) {
        Ok(n) => n,
        Err(e) => {
            err("dig", &e.to_string());
            return 1;
        }
    };
    for line in parse_response(&buf[..n], short) {
        println!("{line}");
    }
    0
}
