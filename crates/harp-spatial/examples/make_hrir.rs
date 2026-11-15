// Extracts a compact HRIR grid from a SOFA file (build-time tool, not shipped).
use sofar::reader::{Filter, OpenOptions};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sofa = OpenOptions::new().sample_rate(48000.0).normalized(true).open(&args[1]).expect("open");
    let n = sofa.filter_len();
    eprintln!("measurements={} filter_len={} rate={}", sofa.num_measurements(), n, sofa.sample_rate());
    let az_step = 5.0f32;
    let n_az = 72usize;
    let els: [f32; 7] = [-30.0, -15.0, 0.0, 15.0, 30.0, 45.0, 60.0];
    let mut f = Filter::new(n);
    let mut all: Vec<(f32, f32, Vec<f32>, Vec<f32>)> = Vec::new();
    let mut peak = 0f32;
    for &el in &els {
        for i in 0..n_az {
            let az = i as f32 * az_step; // degrees, counter-clockwise from front (SOFA): +90 = left
            let (a, e) = (az.to_radians(), el.to_radians());
            sofa.filter(e.cos() * a.cos(), e.cos() * a.sin(), e.sin(), &mut f);
            for s in f.left.iter().chain(f.right.iter()) { peak = peak.max(s.abs()); }
            all.push((f.ldelay, f.rdelay, f.left.to_vec(), f.right.to_vec()));
        }
    }
    let energy = |v: &Vec<f32>| v.iter().map(|x| x * x).sum::<f32>();
    let onset = |v: &Vec<f32>| { let m = v.iter().fold(0f32, |a, x| a.max(x.abs())); v.iter().position(|x| x.abs() > 0.3 * m).unwrap_or(0) };
    for (label, idx) in [("front", 2 * n_az), ("left90", 2 * n_az + 18), ("right90", 2 * n_az + 54), ("back", 2 * n_az + 36)] {
        let (ld, rd, l, r) = &all[idx];
        eprintln!("{label}: dl={ld:.5} dr={rd:.5} El={:.3} Er={:.3} onsetL={} onsetR={}", energy(l), energy(r), onset(l), onset(r));
    }
    let scale = peak / 32767.0;
    let mut out = std::fs::File::create(&args[2]).unwrap();
    out.write_all(b"HRIRGRD1").unwrap();
    for v in [48000u32, n as u32, n_az as u32, els.len() as u32] { out.write_all(&v.to_le_bytes()).unwrap(); }
    out.write_all(&az_step.to_le_bytes()).unwrap();
    for e in els { out.write_all(&e.to_le_bytes()).unwrap(); }
    out.write_all(&scale.to_le_bytes()).unwrap();
    for (ld, rd, l, r) in &all {
        out.write_all(&ld.to_le_bytes()).unwrap();
        out.write_all(&rd.to_le_bytes()).unwrap();
        for s in l.iter().chain(r.iter()) { out.write_all(&((s / scale).round() as i16).to_le_bytes()).unwrap(); }
    }
    eprintln!("peak={peak} wrote {}", args[2]);
}
