//! AirPods head tracking on Linux: the AirPods' accessory channel (L2CAP PSM 0x1001) opened
//! straight through the kernel's Bluetooth sockets (no root, no BlueZ plugin), the motion
//! stream started, and each sample's orientation passed on.
//!
//! Protocol facts from public research (mousepods, Apache-2.0, and others), written here
//! from scratch:
//! - the link must be encrypted and the receive MTU raised to 2582 bytes, or the buds don't
//!   offer their sensors;
//! - after three setup packets, a SensorDataWX request starts the `devmotion6` stream at a
//!   chosen interval; each 58-byte motion block holds a timestamp (bytes 1..9) and the fused
//!   orientation as the vector part of a unit quaternion in Q15 (bytes 20..26).

use crate::head::Quat;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::Command;
use std::time::Duration;

const AF_BLUETOOTH: i32 = 31;
const BTPROTO_L2CAP: i32 = 0;
const SOL_BLUETOOTH: i32 = 274;
const BT_SECURITY: i32 = 4;
const BT_SECURITY_MEDIUM: u8 = 2;
const SOL_L2CAP: i32 = 6;
const L2CAP_OPTIONS: i32 = 1;
const PSM_AACP: u16 = 0x1001;
const RECEIVE_MTU: u16 = 2582;

const HEADER: [u8; 4] = [0x04, 0x00, 0x04, 0x00];
const HANDSHAKE: [u8; 16] = [0, 0, 4, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const FEATURES: [u8; 14] = [4, 0, 4, 0, 0x4d, 0, 0xd7, 0, 0, 0, 0, 0, 0, 0];
const NOTIFICATIONS: [u8; 10] = [4, 0, 4, 0, 0x0f, 0, 0xff, 0xff, 0xff, 0xff];
/// SensorDataWX messages: header, opcode 0x17, then the RTBuddy service 0x100000.
const WX_PREFIX: [u8; 10] = [4, 0, 4, 0, 0x17, 0, 0, 0, 0x10, 0];
const SVC_ACTIVITY: u8 = 0x0E;
const SVC_MOTION: u8 = 0x10;

#[repr(C)]
struct SockaddrL2 {
    family: u16,
    psm: u16,
    bdaddr: [u8; 6],
    cid: u16,
    addr_type: u8,
    _pad: u8,
}

/// A paired, connected Apple audio device.
#[derive(Clone, Debug)]
pub struct AppleDevice {
    pub address: String,
    pub name: String,
    pub product: u16,
}

impl AppleDevice {
    /// Whether this model has the motion sensors and speaks SensorDataWX. AirPods 3 use a
    /// different protocol (not supported here); AirPods 1 and 2 have no such sensors.
    pub fn head_tracking(&self) -> Result<(), String> {
        match self.product {
            0x2002 | 0x200F => Err(format!("{} has no head-tracking sensors", self.name)),
            0x2013 => Err("AirPods (3rd generation) aren't supported for head tracking yet".into()),
            _ => Ok(()),
        }
    }
}

/// Connected Apple audio devices, through `bluetoothctl` (present wherever BlueZ is).
pub fn connected_apple_devices() -> Vec<AppleDevice> {
    let Ok(out) = Command::new("bluetoothctl").args(["devices", "Connected"]).output() else {
        return vec![];
    };
    let mut found = vec![];
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some("Device"), Some(addr)) = (parts.next(), parts.next()) else { continue };
        let name = parts.next().unwrap_or("").to_string();
        let Ok(info) = Command::new("bluetoothctl").args(["info", addr]).output() else { continue };
        let info = String::from_utf8_lossy(&info.stdout);
        if let Some(p) = info.find("Modalias: bluetooth:v004Cp") {
            let hex = &info[p + 26..(p + 30).min(info.len())];
            if let Ok(product) = u16::from_str_radix(hex, 16) {
                found.push(AppleDevice { address: addr.to_string(), name, product });
            }
        }
    }
    found
}

fn parse_addr(addr: &str) -> io::Result<[u8; 6]> {
    let parts: Vec<u8> = addr.split(':').filter_map(|p| u8::from_str_radix(p, 16).ok()).collect();
    if parts.len() != 6 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad Bluetooth address"));
    }
    let mut b = [0u8; 6];
    for i in 0..6 {
        b[i] = parts[5 - i];
    }
    Ok(b)
}

/// The open accessory channel.
pub struct Channel {
    fd: OwnedFd,
    seq: u8,
}

impl Channel {
    pub fn open(addr: &str) -> io::Result<Channel> {
        let bdaddr = parse_addr(addr)?;
        // SAFETY: plain socket calls on a descriptor owned here; buffers outlive the calls.
        unsafe {
            let raw = libc::socket(AF_BLUETOOTH, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, BTPROTO_L2CAP);
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }
            let fd = OwnedFd::from_raw_fd(raw);
            let sec: [u8; 2] = [BT_SECURITY_MEDIUM, 0];
            libc::setsockopt(raw, SOL_BLUETOOTH, BT_SECURITY, sec.as_ptr().cast(), 2);
            let mut opts = [0u8; 12];
            let mut len: libc::socklen_t = 12;
            if libc::getsockopt(raw, SOL_L2CAP, L2CAP_OPTIONS, opts.as_mut_ptr().cast(), &mut len) == 0 {
                opts[2..4].copy_from_slice(&RECEIVE_MTU.to_le_bytes());
                libc::setsockopt(raw, SOL_L2CAP, L2CAP_OPTIONS, opts.as_ptr().cast(), len);
            }
            let sa = SockaddrL2 { family: AF_BLUETOOTH as u16, psm: PSM_AACP.to_le(), bdaddr, cid: 0, addr_type: 0, _pad: 0 };
            if libc::connect(raw, (&sa as *const SockaddrL2).cast(), std::mem::size_of::<SockaddrL2>() as u32) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut ch = Channel { fd, seq: 0x40 };
            for (p, wait) in [(&HANDSHAKE[..], 200), (&FEATURES[..], 120), (&NOTIFICATIONS[..], 120)] {
                ch.send(p)?;
                std::thread::sleep(Duration::from_millis(wait));
            }
            Ok(ch)
        }
    }

    pub fn send(&mut self, pkt: &[u8]) -> io::Result<()> {
        // SAFETY: valid descriptor and buffer.
        let n = unsafe { libc::send(self.fd.as_raw_fd(), pkt.as_ptr().cast(), pkt.len(), libc::MSG_NOSIGNAL) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Starts (interval > 0) or stops (0) a SensorDataWX service stream.
    pub fn stream(&mut self, service: u8, interval_us: u32) -> io::Result<()> {
        self.seq = (self.seq + 1) & 0x7F;
        let mut body = vec![0x08, self.seq, 0x42, 0x0B, 0x08, service, 0x10, 0x02, 0x1A, 0x05, 0x01];
        body.extend_from_slice(&interval_us.to_le_bytes());
        let mut pkt = WX_PREFIX.to_vec();
        pkt.extend_from_slice(&(body.len() as u16).to_le_bytes());
        pkt.extend_from_slice(&body);
        self.send(&pkt)
    }

    pub fn start_motion(&mut self, interval_us: u32) -> io::Result<()> {
        self.stream(SVC_ACTIVITY, 40_000)?;
        std::thread::sleep(Duration::from_millis(250));
        self.stream(SVC_MOTION, interval_us)?;
        self.stream(SVC_ACTIVITY, 0)
    }

    pub fn stop_motion(&mut self) {
        let _ = self.stream(SVC_MOTION, 0);
    }

    /// Waits up to `timeout` for one packet; `Ok(None)` on timeout.
    pub fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let mut p = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd.
        let r = unsafe { libc::poll(&mut p, 1, timeout.as_millis() as i32) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        if r == 0 {
            return Ok(None);
        }
        if p.revents & (libc::POLLHUP | libc::POLLERR) != 0 && p.revents & libc::POLLIN == 0 {
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, "AirPods disconnected"));
        }
        // SAFETY: valid descriptor and buffer.
        let n = unsafe { libc::recv(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, "AirPods disconnected"));
        }
        Ok(Some(n as usize))
    }
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0);
    while *i < b.len() && shift < 64 {
        let c = b[*i];
        *i += 1;
        v |= ((c & 0x7F) as u64) << shift;
        if c & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
    }
    None
}

/// Protobuf fields of a message: (field number, varint value or bytes).
fn fields(b: &[u8]) -> Vec<(u64, Result<u64, &[u8]>)> {
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        let (f, wt) = (key >> 3, key & 7);
        match wt {
            0 => match varint(b, &mut i) {
                Some(v) => out.push((f, Ok(v))),
                None => break,
            },
            2 => {
                let Some(n) = varint(b, &mut i) else { break };
                let n = n as usize;
                if i + n > b.len() {
                    break;
                }
                out.push((f, Err(&b[i..i + n])));
                i += n;
            }
            5 => i += 4,
            1 => i += 8,
            _ => break,
        }
    }
    out
}

/// One motion sample: the AirPods' clock in seconds and their orientation.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub t: f64,
    pub q: Quat,
}

/// The motion samples in one SensorDataWX packet (often several).
pub fn motion_samples(pkt: &[u8]) -> Vec<Motion> {
    if pkt.len() < 12 || pkt[..10] != WX_PREFIX {
        return vec![];
    }
    let n = u16::from_le_bytes([pkt[10], pkt[11]]) as usize;
    let body = &pkt[12..(12 + n).min(pkt.len())];
    let mut out = vec![];
    for (f, v) in fields(body) {
        let (7, Err(inner)) = (f, v) else { continue };
        let mut svc = None;
        let mut blk = None;
        for (f2, v2) in fields(inner) {
            match (f2, v2) {
                (1, Ok(s)) => svc = Some(s),
                (3, Err(b)) => blk = Some(b),
                _ => {}
            }
        }
        let (Some(s), Some(b)) = (svc, blk) else { continue };
        if b.len() < 50 || !(s == SVC_MOTION as u64 || s == SVC_ACTIVITY as u64 || s == 15) {
            continue;
        }
        let ts = u64::from_le_bytes(b[1..9].try_into().unwrap());
        let c = |k: usize| i16::from_le_bytes([b[20 + 2 * k], b[21 + 2 * k]]) as f32 / 32768.0;
        let (x, y, z) = (c(0), c(1), c(2));
        let n2 = x * x + y * y + z * z;
        if !(0.0..=1.0001).contains(&n2) {
            continue;
        }
        out.push(Motion { t: ts as f64 / 1e9, q: [(1.0 - n2).max(0.0).sqrt(), x, y, z] });
    }
    out
}

/// For an AirPods packet saying whether the buds are in the ears: (left, right) with
/// 0 = in the ear, 1 = out, 2 = in the case.
pub fn ear_state(pkt: &[u8]) -> Option<(u8, u8)> {
    (pkt.len() >= 8 && pkt[..4] == HEADER && pkt[4] == 0x06).then(|| (pkt[6], pkt[7]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wx_packet(blocks: &[[u8; 58]]) -> Vec<u8> {
        let mut body = vec![];
        for b in blocks {
            let mut inner = vec![0x08, SVC_MOTION, 0x1A, 58];
            inner.extend_from_slice(b);
            body.push(0x3A); // field 7, bytes
            body.push(inner.len() as u8);
            body.extend_from_slice(&inner);
        }
        let mut p = WX_PREFIX.to_vec();
        p.extend_from_slice(&(body.len() as u16).to_le_bytes());
        p.extend_from_slice(&body);
        p
    }

    #[test]
    fn parses_motion_blocks() {
        let mut b = [0u8; 58];
        b[0] = 1;
        b[1..9].copy_from_slice(&2_500_000_000u64.to_le_bytes());
        // a 90° turn about Z: (cos 45°, 0, 0, sin 45°)
        b[24..26].copy_from_slice(&((0.70710677f32 * 32768.0) as i16).to_le_bytes());
        let p = wx_packet(&[b, b]);
        let m = motion_samples(&p);
        assert_eq!(m.len(), 2);
        assert!((m[0].t - 2.5).abs() < 1e-9);
        assert!((m[0].q[0] - 0.7071).abs() < 1e-3 && (m[0].q[3] - 0.7071).abs() < 1e-3);
        assert!(motion_samples(&[4, 0, 4, 0, 6, 0, 0, 0]).is_empty());
    }

    #[test]
    fn ear_packets_and_addresses() {
        assert_eq!(ear_state(&[4, 0, 4, 0, 6, 0, 0, 2]), Some((0, 2)));
        assert_eq!(parse_addr("CC:08:FA:B8:0F:2C").unwrap(), [0x2C, 0x0F, 0xB8, 0xFA, 0x08, 0xCC]);
        assert!(parse_addr("nope").is_err());
        let no = AppleDevice { address: String::new(), name: "AirPods".into(), product: 0x200F };
        assert!(no.head_tracking().is_err());
        let pro2 = AppleDevice { address: String::new(), name: "AirPods Pro".into(), product: 0x2014 };
        assert!(pro2.head_tracking().is_ok());
    }
}
