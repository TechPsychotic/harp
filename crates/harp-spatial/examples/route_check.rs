//! Runs 3D audio for a few seconds and prints its status (a live check of the route).
//! `cargo run -p harp-spatial --example route_check -- <seconds> [track]`
fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let track = std::env::args().nth(2).as_deref() == Some("track");
    let dev = harp_spatial::start(track, false).expect("start");
    println!("player device: {dev}");
    for _ in 0..secs * 2 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let s = harp_spatial::status();
        println!("output={} head={} ({}) yaw={:.1} latency={}ms error={:?}", s.output, s.head, s.head_detail, s.yaw, s.latency_ms, s.error);
    }
    harp_spatial::stop();
    println!("stopped");
}
