//! Prints the response a centred (mono) signal gets at the left ear, by band, with the
//! renderer's gain, to see how loud vocals come out.
use harp_spatial::hrir::HrirSet;
fn main() {
    let s = HrirSet::builtin();
    let (mut l1, mut r1, mut l2, mut r2) = (vec![0.0; 256], vec![0.0; 256], vec![0.0; 256], vec![0.0; 256]);
    s.lookup(30.0, 0.0, &mut l1, &mut r1);
    s.lookup(-30.0, 0.0, &mut l2, &mut r2);
    let e: f32 = l1.iter().chain(&r1).map(|x| x * x).sum();
    let g = 1.0 / e.sqrt();
    let n = 256;
    for f in [60.0f32, 125.0, 250.0, 500.0, 1000.0, 2000.0, 3000.0, 4000.0, 6000.0, 8000.0, 10000.0, 12000.0, 16000.0] {
        let w = 2.0 * std::f32::consts::PI * f / 48000.0;
        let resp = |h: &[f32]| {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for k in 0..n { re += h[k] * (w * k as f32).cos(); im -= h[k] * (w * k as f32).sin(); }
            (re, im)
        };
        let (a, b) = resp(&l1);
        let (c, d) = resp(&l2);
        let center = ((a + c).powi(2) + (b + d).powi(2)).sqrt() * g;
        let side = (a * a + b * b).sqrt() * g;
        println!("{f:>6} Hz  centre {:+6.1} dB   one speaker {:+6.1} dB", 20.0 * center.log10(), 20.0 * side.log10());
    }
}
