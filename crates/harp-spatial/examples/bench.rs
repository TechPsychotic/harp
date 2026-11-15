//! How much of one CPU core 3D audio takes (release build).
use harp_spatial::render::{Binaural, BLOCK};
fn main() {
    for room in [false, true] {
        let mut b = Binaural::new(true, room);
        let input: Vec<f32> = (0..BLOCK * 2).map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5).collect();
        let mut out = vec![0.0; BLOCK * 2];
        let blocks = 48000 * 20 / BLOCK; // 20 s of audio
        let t = std::time::Instant::now();
        for k in 0..blocks {
            b.process((k as f32 * 0.7).sin() * 60.0, 0.0, &input, &mut out);
        }
        let used = t.elapsed().as_secs_f64();
        println!("room {room}: 20 s of audio in {used:.2} s = {:.1}% of one core", used / 20.0 * 100.0);
    }
}
