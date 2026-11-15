//! Prints what the AirPods send after the motion stream is started (debugging aid).
use harp_spatial::aacp::{self, Channel};
use std::time::{Duration, Instant};
fn main() {
    let d = aacp::connected_apple_devices();
    println!("devices: {d:?}");
    let dev = d.first().expect("no AirPods connected");
    let mut ch = Channel::open(&dev.address).expect("open");
    let us: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(25_000);
    ch.start_motion(us).expect("start");
    let mut buf = vec![0u8; 4096];
    let t = Instant::now();
    let (mut pk, mut wx, mut samples) = (0, 0, 0);
    while t.elapsed() < Duration::from_secs(4) {
        if let Ok(Some(n)) = ch.recv(&mut buf, Duration::from_millis(200)) {
            pk += 1;
            let p = &buf[..n];
            if p.len() > 10 && p[4] == 0x17 {
                wx += 1;
                if wx <= 3 { println!("wx {} bytes: {:02x?}", n, &p[..n.min(48)]); }
            }
            samples += aacp::motion_samples(p).len();
        }
    }
    ch.stop_motion();
    println!("packets={pk} sensor-packets={wx} motion-samples={samples}");
}
