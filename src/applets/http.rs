//! `http` — minimal HTTP/1.1 client with HTTPS via `rustls`.
//!
//! Mirrors the Python applet's surface where practical:
//! `http [-X METHOD] [-H KEY:VALUE] [-d BODY] [-i] [-I] [-o FILE] [-f]
//! [--timeout N] [--json BODY] URL`.
//!
//! HTTPS uses `rustls` with the bundled `webpki-roots` trust store —
//! no system trust dep, deterministic across platforms.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::common::err;
use crate::registry::Applet;

pub const APPLET: Applet = Applet {
    name: "http",
    help: "HTTP/1.1 client with HTTPS via rustls",
    aliases: &[],
    main,
};

struct Url {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(u: &str) -> Option<Url> {
    let (scheme, rest) = u.split_once("://")?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = if let Some((h, p)) = host_port.rsplit_once(':') {
        (h.to_string(), p.parse().ok()?)
    } else {
        (
            host_port.to_string(),
            if scheme == "https" { 443 } else { 80 },
        )
    };
    Some(Url {
        scheme: scheme.to_string(),
        host,
        port,
        path: path.to_string(),
    })
}

/// Build the rustls config once per process. We use the Mozilla CA bundle
/// (`webpki-roots`) instead of the system trust store so HTTPS works the
/// same on Linux/macOS/Windows without OS-specific plumbing.
fn tls_config() -> Arc<ClientConfig> {
    use std::sync::OnceLock;
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// Format the HTTP/1.1 request line + headers + body into one buffer ready
/// to ship over either a plain TCP stream or a TLS stream.
fn build_request(
    url: &Url,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Vec<u8> {
    let mut req = String::new();
    req.push_str(&format!("{method} {} HTTP/1.1\r\n", url.path));
    // Omit the explicit port for default ports — some origins (notably
    // GitHub/Cloudflare) reject the explicit form on TLS.
    let default_port = if url.scheme == "https" { 443 } else { 80 };
    if url.port == default_port {
        req.push_str(&format!("Host: {}\r\n", url.host));
    } else {
        req.push_str(&format!("Host: {}:{}\r\n", url.host, url.port));
    }
    req.push_str("Connection: close\r\n");
    let mut have_ct = false;
    let mut have_cl = false;
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("Content-Type") {
            have_ct = true;
        }
        if k.eq_ignore_ascii_case("Content-Length") {
            have_cl = true;
        }
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        if !have_ct {
            req.push_str("Content-Type: application/octet-stream\r\n");
        }
        if !have_cl {
            req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        }
    }
    req.push_str("\r\n");
    let mut buf = req.into_bytes();
    if let Some(b) = body {
        buf.extend_from_slice(b);
    }
    buf
}

/// Split a raw HTTP/1.1 response into `(status, headers, body)`. Used by
/// both the TCP and TLS paths.
fn parse_response(raw: &[u8]) -> std::io::Result<(u16, Vec<(String, String)>, Vec<u8>)> {
    let split_at = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| std::io::Error::other("malformed response"))?;
    let head = &raw[..split_at];
    let body = raw[split_at + 4..].to_vec();
    let head_text = String::from_utf8_lossy(head);
    let mut lines = head_text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut hdrs: Vec<(String, String)> = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            hdrs.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok((status, hdrs, body))
}

fn send_request(
    url: &Url,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> std::io::Result<(u16, Vec<(String, String)>, Vec<u8>)> {
    let sa = (url.host.as_str(), url.port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("resolve failed"))?;
    let mut sock = TcpStream::connect_timeout(&sa, timeout)?;
    sock.set_read_timeout(Some(timeout))?;
    sock.set_write_timeout(Some(timeout))?;
    let req = build_request(url, method, headers, body);

    let mut raw = Vec::new();
    if url.scheme == "https" {
        let server_name = ServerName::try_from(url.host.clone())
            .map_err(|e| std::io::Error::other(format!("invalid server name: {e}")))?;
        let conn = ClientConnection::new(tls_config(), server_name)
            .map_err(|e| std::io::Error::other(format!("tls setup: {e}")))?;
        let mut tls = StreamOwned::new(conn, sock);
        tls.write_all(&req)?;
        tls.read_to_end(&mut raw)?;
    } else {
        sock.write_all(&req)?;
        sock.read_to_end(&mut raw)?;
    }
    parse_response(&raw)
}

fn main(argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut method = "GET".to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut body: Option<Vec<u8>> = None;
    let mut include = false;
    let mut head_only = false;
    let mut output: Option<String> = None;
    let mut fail_on_error = false;
    let mut timeout_secs: u64 = 30;
    let mut url_str: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-X" if i + 1 < args.len() => {
                method = args[i + 1].clone();
                i += 2;
            }
            "-H" if i + 1 < args.len() => {
                if let Some((k, v)) = args[i + 1].split_once(':') {
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                }
                i += 2;
            }
            "-d" if i + 1 < args.len() => {
                let v = &args[i + 1];
                if let Some(path) = v.strip_prefix('@') {
                    body = std::fs::read(path).ok();
                } else {
                    body = Some(v.as_bytes().to_vec());
                }
                if method == "GET" {
                    method = "POST".to_string();
                }
                i += 2;
            }
            "--json" if i + 1 < args.len() => {
                let v = &args[i + 1];
                body = Some(v.as_bytes().to_vec());
                headers.push(("Content-Type".to_string(), "application/json".to_string()));
                if method == "GET" {
                    method = "POST".to_string();
                }
                i += 2;
            }
            "-i" => {
                include = true;
                i += 1;
            }
            "-I" => {
                head_only = true;
                method = "HEAD".to_string();
                i += 1;
            }
            "-o" if i + 1 < args.len() => {
                output = Some(args[i + 1].clone());
                i += 2;
            }
            "-f" => {
                fail_on_error = true;
                i += 1;
            }
            "--timeout" if i + 1 < args.len() => {
                timeout_secs = args[i + 1].parse().unwrap_or(30);
                i += 2;
            }
            "-A" if i + 1 < args.len() => {
                headers.push(("User-Agent".to_string(), args[i + 1].clone()));
                i += 2;
            }
            "-s" => {
                i += 1;
            }
            s if s.starts_with('-') && s != "-" && s.len() > 1 => {
                err("http", &format!("unknown option: {s}"));
                return 2;
            }
            _ => {
                url_str = Some(a.clone());
                i += 1;
            }
        }
    }
    let url_str = match url_str {
        Some(u) => u,
        None => {
            err("http", "missing URL");
            return 2;
        }
    };
    let url = match parse_url(&url_str) {
        Some(u) => u,
        None => {
            err("http", &format!("invalid URL: {url_str}"));
            return 2;
        }
    };
    if url.scheme != "http" && url.scheme != "https" {
        err("http", &format!("unsupported scheme: {}", url.scheme));
        return 2;
    }
    let to = Duration::from_secs(timeout_secs);
    let (status, hdrs, body_bytes) =
        match send_request(&url, &method, &headers, body.as_deref(), to) {
            Ok(t) => t,
            Err(e) => {
                err("http", &e.to_string());
                return 1;
            }
        };
    if fail_on_error && status >= 400 {
        eprintln!("http: HTTP {status}");
        return 1;
    }
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    if include || head_only {
        let _ = writeln!(out, "HTTP/1.1 {status}");
        for (k, v) in &hdrs {
            let _ = writeln!(out, "{k}: {v}");
        }
        let _ = writeln!(out);
    }
    if !head_only {
        if let Some(p) = output {
            if std::fs::write(&p, &body_bytes).is_err() {
                err("http", &format!("write {p}"));
                return 1;
            }
        } else {
            let _ = out.write_all(&body_bytes);
        }
    }
    0
}
