//! Tone correction for the HRTFs: the dummy head's ears colour everything (a −7.7 dB dip at
//! 8 kHz, +4 dB at 4 kHz for a centred voice). Harp corrects the HRIRs so that a centred
//! sound comes out flat, as on a good pair of headphones without 3D, while sounds from other
//! directions keep their direction cues. The correction is a minimum-phase filter (no
//! pre-echo), smoothed over a third of an octave, and limited to ±10 dB.

use rustfft::{num_complex::Complex32, FftPlanner};

const N: usize = 4096;

fn fft(buf: &mut [Complex32], inverse: bool) {
    let mut p = FftPlanner::<f32>::new();
    let f = if inverse { p.plan_fft_inverse(buf.len()) } else { p.plan_fft_forward(buf.len()) };
    f.process(buf);
    if inverse {
        let n = buf.len() as f32;
        buf.iter_mut().for_each(|x| *x /= n);
    }
}

/// Magnitude response of an impulse response on `N` bins.
pub fn magnitude(ir: &[f32]) -> Vec<f32> {
    let mut b: Vec<Complex32> = (0..N).map(|i| Complex32::new(ir.get(i).copied().unwrap_or(0.0), 0.0)).collect();
    fft(&mut b, false);
    b.iter().map(|c| c.norm()).collect()
}

/// A minimum-phase FIR (`taps` long) whose magnitude makes `response` (on `N` bins at
/// `rate`) flat between 150 Hz and 14 kHz.
pub fn flattening_filter(response: &[f32], rate: f32, taps: usize) -> Vec<f32> {
    let hz = |k: usize| k as f32 * rate / N as f32;
    let bin = |f: f32| ((f * N as f32 / rate).round() as usize).clamp(1, N / 2);
    // Third-octave smoothing of the power, then the inverse, limited.
    let mut gain_db = vec![0.0f32; N / 2 + 1];
    for k in 1..=N / 2 {
        let f = hz(k).clamp(150.0, 14_000.0);
        let (lo, hi) = (bin(f / 2f32.powf(1.0 / 6.0)), bin(f * 2f32.powf(1.0 / 6.0)));
        let p: f32 = (lo..=hi).map(|j| response[j] * response[j]).sum::<f32>() / (hi - lo + 1) as f32;
        gain_db[k] = (-10.0 * p.max(1e-12).log10()).clamp(-10.0, 10.0);
    }
    gain_db[0] = gain_db[1];
    // Minimum phase through the real cepstrum.
    let mut c: Vec<Complex32> = (0..N)
        .map(|k| {
            let k = if k <= N / 2 { k } else { N - k };
            Complex32::new(gain_db[k] / 20.0 * std::f32::consts::LN_10, 0.0)
        })
        .collect();
    fft(&mut c, true);
    for (n, x) in c.iter_mut().enumerate() {
        *x = if n == 0 || n == N / 2 {
            Complex32::new(x.re, 0.0)
        } else if n < N / 2 {
            Complex32::new(2.0 * x.re, 0.0)
        } else {
            Complex32::new(0.0, 0.0)
        };
    }
    fft(&mut c, false);
    let mut h: Vec<Complex32> = c.iter().map(|z| z.exp()).collect();
    fft(&mut h, true);
    let fade = taps / 4;
    (0..taps)
        .map(|i| {
            let w = if i + fade >= taps { 0.5 + 0.5 * (std::f32::consts::PI * (i + fade - taps) as f32 / fade as f32).cos() } else { 1.0 };
            h[i].re * w
        })
        .collect()
}

pub fn convolve(a: &[f32], b: &[f32], out_len: usize) -> Vec<f32> {
    let mut o = vec![0.0f32; out_len];
    for (i, &x) in a.iter().enumerate() {
        if x == 0.0 {
            continue;
        }
        for (j, &y) in b.iter().enumerate() {
            if i + j < out_len {
                o[i + j] += x * y;
            }
        }
    }
    o
}

pub fn n_bins() -> usize {
    N
}
