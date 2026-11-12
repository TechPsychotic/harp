//! A loopback HTTP proxy with random credentials, for Android.
//!
//! The static (musl) Python that runs yt-dlp there cannot resolve names (no
//! /etc/resolv.conf), so yt-dlp connects through this proxy, which dials with
//! the app's own resolver. Other apps can reach loopback ports, hence the
//! credentials, which only yt-dlp's command line holds.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

pub struct Proxy {
    pub url: String,
    // Dropping stops nothing (threads end with the process); kept for the port.
    pub port: u16,
}

fn b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::new();
    for c in input.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}

fn secret() -> String {
    let mut b = [0u8; 16];
    // /dev/urandom exists on Linux, Android and macOS; Windows is not a proxy target.
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = n.to_le_bytes();
    }
    hex::encode(b)
}

impl Proxy {
    pub fn start() -> std::io::Result<Proxy> {
        let ln = TcpListener::bind("127.0.0.1:0")?;
        let port = ln.local_addr()?.port();
        let user = "harp";
        let pass = secret();
        let expect: Arc<String> = Arc::new(format!("Basic {}", b64(format!("{user}:{pass}").as_bytes())));
        std::thread::spawn(move || {
            for c in ln.incoming().flatten() {
                let expect = expect.clone();
                std::thread::spawn(move || {
                    let _ = handle(c, &expect);
                });
            }
        });
        Ok(Proxy { url: format!("http://{user}:{pass}@127.0.0.1:{port}"), port })
    }
}

static PREFER_V6: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Which address family goes first. IPv4 normally; IPv6 after YouTube refused
/// the IPv4 route (a mobile carrier's shared IPv4 address is what YouTube flags
/// as a bot; a phone's IPv6 address is its own).
pub fn set_prefer_v6(on: bool) {
    PREFER_V6.store(on, std::sync::atomic::Ordering::Relaxed);
}
pub fn prefer_v6() -> bool {
    PREFER_V6.load(std::sync::atomic::Ordering::Relaxed)
}

/// The preferred family first (IPv4 unless switched; many phones resolve an
/// IPv6 address they cannot reach), alternating so one dead family never
/// blocks the other.
pub(crate) fn order_addrs(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    let (v4, v6): (Vec<SocketAddr>, Vec<SocketAddr>) = addrs.iter().copied().partition(|a| a.is_ipv4());
    let (first, second) = if prefer_v6() { (v6, v4) } else { (v4, v6) };
    let mut out = Vec::new();
    let (mut i4, mut i6) = (first.into_iter(), second.into_iter());
    loop {
        match (i4.next(), i6.next()) {
            (None, None) => break,
            (a, b) => out.extend(a.into_iter().chain(b)),
        }
    }
    out
}

/// Tries every address (each with a short timeout) until one connects.
pub(crate) fn connect_any(addrs: &[SocketAddr]) -> Option<TcpStream> {
    let started = std::time::Instant::now();
    for a in order_addrs(addrs) {
        if started.elapsed() > Duration::from_secs(25) {
            break;
        }
        if let Ok(s) = TcpStream::connect_timeout(&a, Duration::from_secs(7)) {
            return Some(s);
        }
    }
    None
}

fn reply(c: &mut TcpStream, status: &str) {
    let _ = write!(c, "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}

fn handle(mut client: TcpStream, expect: &str) -> std::io::Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut rd = BufReader::new(client.try_clone()?);
    let mut line = String::new();
    rd.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").to_string());
    let mut auth = String::new();
    loop {
        let mut h = String::new();
        if rd.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("proxy-authorization") {
                auth = v.trim().to_string();
            }
        }
    }
    if auth != expect {
        reply(&mut client, "407 Proxy Authentication Required");
        return Ok(());
    }
    // Only HTTPS tunnels: yt-dlp talks TLS to YouTube; plain http is refused.
    if method != "CONNECT" || target.contains(['/', '@']) {
        reply(&mut client, "400 Bad Request");
        return Ok(());
    }
    let addrs: Vec<SocketAddr> = target.to_socket_addrs().map(|a| a.collect()).unwrap_or_default();
    let up = match connect_any(&addrs) {
        Some(u) => u,
        None => {
            reply(&mut client, "502 Bad Gateway");
            return Ok(());
        }
    };
    client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")?;
    client.set_read_timeout(None)?;
    // Bytes the client sent after its request are already in the BufReader.
    let buffered = rd.buffer().to_vec();
    let mut up_w = up.try_clone()?;
    up_w.write_all(&buffered)?;
    let mut c_r = client.try_clone()?;
    let mut up_r = up.try_clone()?;
    let t = std::thread::spawn(move || {
        let _ = std::io::copy(&mut c_r, &mut up_w);
        let _ = up_w.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut up_r, &mut client);
    let _ = client.shutdown(Shutdown::Write);
    let _ = t.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        assert_eq!(b64(b"harp:x"), "aGFycDp4");
        assert_eq!(b64(b"a"), "YQ==");
        assert_eq!(b64(b"ab"), "YWI=");
    }

    fn echo_server() -> u16 {
        let ln = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ln.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in ln.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut buf = [0u8; 64];
                    while let Ok(n) = s.read(&mut buf) {
                        if n == 0 || s.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                });
            }
        });
        port
    }

    fn connect_via(p: &Proxy, auth: Option<&str>, target: &str) -> String {
        let mut c = TcpStream::connect(("127.0.0.1", p.port)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let a = auth.map(|a| format!("Proxy-Authorization: Basic {}\r\n", b64(a.as_bytes()))).unwrap_or_default();
        write!(c, "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n{a}\r\n").unwrap();
        let mut rd = BufReader::new(c.try_clone().unwrap());
        let mut status = String::new();
        rd.read_line(&mut status).unwrap();
        if status.contains("200") {
            let mut l = String::new();
            rd.read_line(&mut l).unwrap(); // blank line
            c.write_all(b"ping\n").unwrap();
            let mut echo = String::new();
            rd.read_line(&mut echo).unwrap();
            return format!("200 {}", echo.trim());
        }
        status.trim().to_string()
    }

    #[test]
    fn ipv4_first_and_falls_through_dead_addresses() {
        let v6: SocketAddr = "[::1]:9".parse().unwrap();
        let v4a: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let v4b: SocketAddr = "127.0.0.2:9".parse().unwrap();
        let o = order_addrs(&[v6, v4a, v4b]);
        assert_eq!(o, vec![v4a, v6, v4b]);
        // First address is dead, second is alive: the connection still succeeds.
        let ln = TcpListener::bind("127.0.0.1:0").unwrap();
        let live = ln.local_addr().unwrap();
        let dead: SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert!(connect_any(&[dead, live]).is_some());
        assert!(connect_any(&[dead]).is_none());
        assert!(connect_any(&[]).is_none());
    }

    #[test]
    fn tunnels_with_credentials_and_refuses_without() {
        let echo = echo_server();
        let p = Proxy::start().unwrap();
        let cred = p.url.strip_prefix("http://").unwrap().split('@').next().unwrap().to_string();
        let target = format!("127.0.0.1:{echo}");
        assert_eq!(connect_via(&p, Some(&cred), &target), "200 ping");
        assert!(connect_via(&p, None, &target).contains("407"));
        assert!(connect_via(&p, Some("harp:wrong"), &target).contains("407"));
        assert!(connect_via(&p, Some(&cred), "a/b:1").contains("400"));
    }
}
