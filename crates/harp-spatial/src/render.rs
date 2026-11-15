//! Stereo to binaural: the two channels play from virtual speakers at ±30° in front of the
//! listener, each filtered with the HRIRs for its direction relative to the head. When the
//! head turns, the filters change and the output crossfades from the old to the new filters
//! over one block, so there is no click.
//!
//! Bass below the crossover skips the HRTF (it carries almost no direction and the filters
//! would only colour it); it is delayed to line up with the filtered band.

use crate::hrir::HrirSet;
use sofar::reader::Filter;
use sofar::render::Renderer;

/// Frames per processing block (5.3 ms at 48 kHz).
pub const BLOCK: usize = 256;
/// Where the virtual speakers stand (degrees left of front).
pub const SPEAKER_AZ: f32 = 30.0;
const CROSSOVER_HZ: f32 = 150.0;
/// Filters are only swapped when a speaker's direction moved at least this much.
const MIN_STEP_DEG: f32 = 0.75;

struct Speaker {
    az: f32,
    r: [Renderer; 2],
    active: usize,
    dir: Option<(f32, f32)>,
    filt: Filter,
    out: [[Vec<f32>; 2]; 2],
}

/// A 2nd-order Butterworth section (RBJ cookbook), transposed direct form II.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(rate: f32, f0: f32, high: bool) -> Self {
        let w = 2.0 * std::f32::consts::PI * f0 / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b = if high {
            [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0]
        } else {
            [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0]
        };
        Biquad { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [-2.0 * c / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// Linkwitz-Riley 4th-order crossover (two Butterworth sections per side): low + high sum
/// to a flat all-pass.
#[derive(Clone, Copy)]
struct Crossover {
    lo: [Biquad; 2],
    hi: [Biquad; 2],
}

impl Crossover {
    fn new(rate: f32) -> Self {
        let l = Biquad::new(rate, CROSSOVER_HZ, false);
        let h = Biquad::new(rate, CROSSOVER_HZ, true);
        Crossover { lo: [l, l], hi: [h, h] }
    }
    #[inline]
    fn split(&mut self, x: f32) -> (f32, f32) {
        let l0 = self.lo[0].run(x);
        let lo = self.lo[1].run(l0);
        let h0 = self.hi[0].run(x);
        let hi = self.hi[1].run(h0);
        (lo, hi)
    }
}

pub struct Binaural {
    set: &'static HrirSet,
    spk: [Speaker; 2],
    bass_bypass: bool,
    xo: [Crossover; 2],
    /// Delay line for the bass band, per channel.
    low_delay: Vec<[f32; 2]>,
    low_pos: usize,
    gain: f32,
    hi_in: [Vec<f32>; 2],
    lo_buf: Vec<[f32; 2]>,
    /// Tests only: swap filters without the crossfade, to show the fade matters.
    #[cfg(test)]
    hard_swap: bool,
    /// Last applied head pose.
    pub yaw: f32,
    pub pitch: f32,
}

impl Binaural {
    /// A renderer at the built-in table's rate (48 kHz).
    pub fn new(bass_bypass: bool) -> Self {
        let set = HrirSet::builtin();
        let rate = set.rate as f32;
        let mk = || {
            Renderer::builder(set.taps).with_sample_rate(rate).with_partition_len(BLOCK).build().expect("renderer")
        };
        let speaker = |az: f32| Speaker {
            az,
            r: [mk(), mk()],
            active: 0,
            dir: None,
            filt: Filter::new(set.taps),
            out: [[vec![0.0; BLOCK], vec![0.0; BLOCK]], [vec![0.0; BLOCK], vec![0.0; BLOCK]]],
        };
        // Level: a speaker's direct + cross-ear energy, so loudness stays close to the
        // unprocessed signal.
        let (mut l, mut r) = (vec![0.0; set.taps], vec![0.0; set.taps]);
        set.lookup(SPEAKER_AZ, 0.0, &mut l, &mut r);
        let e: f32 = l.iter().chain(&r).map(|x| x * x).sum();
        // Bass lines up with the main peak of the front response.
        set.lookup(0.0, 0.0, &mut l, &mut r);
        let peak = l.iter().enumerate().fold((0, 0.0f32), |m, (i, x)| if x.abs() > m.1 { (i, x.abs()) } else { m }).0;
        let mut b = Binaural {
            set,
            spk: [speaker(SPEAKER_AZ), speaker(-SPEAKER_AZ)],
            bass_bypass,
            xo: [Crossover::new(rate); 2],
            low_delay: vec![[0.0; 2]; peak.max(1)],
            low_pos: 0,
            gain: 1.0 / e.sqrt().max(1e-6),
            hi_in: [vec![0.0; BLOCK], vec![0.0; BLOCK]],
            lo_buf: vec![[0.0; 2]; BLOCK],
            #[cfg(test)]
            hard_swap: false,
            yaw: 0.0,
            pitch: 0.0,
        };
        b.steer(0.0, 0.0);
        for s in &mut b.spk {
            s.active = 1 - s.active; // the first filter set is used straight away
        }
        b
    }

    /// Points the speakers' filters at the given head pose; returns per speaker whether the
    /// inactive renderer got a new filter (and should be faded in).
    fn steer(&mut self, yaw: f32, pitch: f32) -> [bool; 2] {
        let mut changed = [false; 2];
        for (k, s) in self.spk.iter_mut().enumerate() {
            let want = (s.az - yaw, -pitch);
            let moved = match s.dir {
                None => true,
                Some((a, e)) => angle_diff(a, want.0).abs() >= MIN_STEP_DEG || (e - want.1).abs() >= MIN_STEP_DEG,
            };
            if moved {
                self.set.lookup(want.0, want.1, &mut s.filt.left, &mut s.filt.right);
                let next = 1 - s.active;
                s.r[next].set_filter(&s.filt).expect("filter length");
                s.dir = Some(want);
                changed[k] = true;
            }
        }
        changed
    }

    /// Renders one block: `input` and `out` are interleaved stereo, `BLOCK` frames.
    /// `yaw` is the head's turn to the left in degrees, `pitch` its tilt up.
    pub fn process(&mut self, yaw: f32, pitch: f32, input: &[f32], out: &mut [f32]) {
        assert_eq!(input.len(), BLOCK * 2);
        assert_eq!(out.len(), BLOCK * 2);
        self.yaw = yaw;
        self.pitch = pitch;
        for i in 0..BLOCK {
            for c in 0..2 {
                let x = input[2 * i + c];
                if self.bass_bypass {
                    let (lo, hi) = self.xo[c].split(x);
                    self.hi_in[c][i] = hi;
                    self.lo_buf[i][c] = lo;
                } else {
                    self.hi_in[c][i] = x;
                    self.lo_buf[i][c] = 0.0;
                }
            }
        }
        let fade = self.steer(yaw, pitch);
        out.iter_mut().for_each(|s| *s = 0.0);
        for (k, s) in self.spk.iter_mut().enumerate() {
            // Both renderers always see the input, so either has the right history when its
            // filter is faded in.
            for j in 0..2 {
                let [ref mut ol, ref mut or] = s.out[j];
                s.r[j].process_block(&self.hi_in[k], &mut ol[..], &mut or[..]).expect("block");
            }
            let a = s.active;
            #[cfg(test)]
            if self.hard_swap && fade[k] {
                s.active = 1 - a;
                for i in 0..BLOCK {
                    out[2 * i] += s.out[1 - a][0][i];
                    out[2 * i + 1] += s.out[1 - a][1][i];
                }
                continue;
            }
            if fade[k] {
                let b = 1 - a;
                for i in 0..BLOCK {
                    let w = 0.5 - 0.5 * (std::f32::consts::PI * (i as f32 + 0.5) / BLOCK as f32).cos();
                    out[2 * i] += (1.0 - w) * s.out[a][0][i] + w * s.out[b][0][i];
                    out[2 * i + 1] += (1.0 - w) * s.out[a][1][i] + w * s.out[b][1][i];
                }
                s.active = b;
            } else {
                for i in 0..BLOCK {
                    out[2 * i] += s.out[a][0][i];
                    out[2 * i + 1] += s.out[a][1][i];
                }
            }
        }
        let n = self.low_delay.len();
        for i in 0..BLOCK {
            let lo = std::mem::replace(&mut self.low_delay[self.low_pos], self.lo_buf[i]);
            self.low_pos = (self.low_pos + 1) % n;
            for c in 0..2 {
                out[2 * i + c] = soft_clip(out[2 * i + c] * self.gain + lo[c]);
            }
        }
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

/// Leaves everything below 0.9 alone and rounds peaks off instead of wrapping or clipping hard.
#[inline]
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.9 {
        x
    } else {
        x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_in_the_left_channel_gives_the_left_speakers_hrirs() {
        let mut b = Binaural::new(false);
        let set = HrirSet::builtin();
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        set.lookup(SPEAKER_AZ, 0.0, &mut l, &mut r);
        let mut input = vec![0.0; BLOCK * 2];
        input[0] = 0.5;
        let mut out = vec![0.0; BLOCK * 2];
        b.process(0.0, 0.0, &input, &mut out);
        let g = b.gain();
        for i in 0..BLOCK {
            let (el, er) = (soft_clip(0.5 * g * l[i]), soft_clip(0.5 * g * r[i]));
            assert!((out[2 * i] - el).abs() < 1e-4, "left ear sample {i}: {} vs {el}", out[2 * i]);
            assert!((out[2 * i + 1] - er).abs() < 1e-4, "right ear sample {i}");
        }
    }

    #[test]
    fn turning_the_head_left_moves_the_sound_to_the_right_ear() {
        // A source dead ahead (both channels equal); with the head turned 60° left it sits at
        // the listener's right.
        let mut b = Binaural::new(false);
        let mut rng = 1u32;
        let mut e = [0.0f32; 2];
        for blk in 0..40 {
            let mut input = vec![0.0; BLOCK * 2];
            for i in 0..BLOCK {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let x = (rng >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                input[2 * i] = 0.2 * x;
                input[2 * i + 1] = 0.2 * x;
            }
            let mut out = vec![0.0; BLOCK * 2];
            b.process(60.0, 0.0, &input, &mut out);
            if blk > 4 {
                for i in 0..BLOCK {
                    e[0] += out[2 * i] * out[2 * i];
                    e[1] += out[2 * i + 1] * out[2 * i + 1];
                }
            }
        }
        assert!(e[1] > 2.0 * e[0], "right ear {} vs left ear {}", e[1], e[0]);
    }

    /// Energy above ~6 kHz in the output for a 1 kHz tone while `yaw(block)` moves: the
    /// tone itself has none there, so this is what filter changes add (clicks, zipper noise).
    fn click_energy(blocks: usize, hard: bool, yaw: impl Fn(usize) -> f32) -> f32 {
        let mut b = Binaural::new(true);
        b.hard_swap = hard;
        let mut hp = [[Biquad::new(48000.0, 6000.0, true); 3]; 2];
        let mut e = 0.0f32;
        let mut t = 0usize;
        for blk in 0..blocks {
            let mut input = vec![0.0; BLOCK * 2];
            for i in 0..BLOCK {
                let x = 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * t as f32 / 48000.0).sin();
                input[2 * i] = x;
                input[2 * i + 1] = x;
                t += 1;
            }
            let mut out = vec![0.0; BLOCK * 2];
            b.process(yaw(blk), 0.0, &input, &mut out);
            for i in 0..BLOCK {
                for c in 0..2 {
                    let y = hp[c].iter_mut().fold(out[2 * i + c], |x, f| f.run(x));
                    if blk > 20 {
                        e += y * y;
                    }
                }
            }
        }
        e
    }

    #[test]
    fn moving_the_head_does_not_click() {
        // A fast but human head swing (±80°, peak about 250°/s): the crossfade must keep the
        // noise it adds well below what swapping filters outright does, and near a head held
        // still.
        let block_s = BLOCK as f32 / 48000.0;
        let swing = |blk: usize| 80.0 * (2.0 * std::f32::consts::PI * 0.5 * blk as f32 * block_s).sin();
        let still = click_energy(400, false, |_| 40.0);
        let faded = click_energy(400, false, swing);
        let hard = click_energy(400, true, swing);
        eprintln!("click energy: still {still:.3e}, crossfaded {faded:.3e}, hard swap {hard:.3e}");
        assert!(faded < 0.25 * hard, "crossfade {faded} vs hard swap {hard}");
    }

    #[test]
    fn crossover_bands_sum_flat_in_level() {
        let mut xo = Crossover::new(48000.0);
        for f in [50.0f32, 150.0, 1000.0] {
            let (mut ei, mut eo) = (0.0, 0.0);
            for t in 0..48000 {
                let x = (2.0 * std::f32::consts::PI * f * t as f32 / 48000.0).sin();
                let (lo, hi) = xo.split(x);
                if t > 24000 {
                    ei += x * x;
                    eo += (lo + hi) * (lo + hi);
                }
            }
            let db = 10.0 * (eo / ei).log10();
            assert!(db.abs() < 0.2, "{f} Hz: {db} dB");
        }
    }
}
