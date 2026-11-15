//! The built-in HRIR table (SADIE II, subject D1; see `data/README.md`) and lookups between
//! its grid points.

use std::sync::OnceLock;

static BUILTIN: &[u8] = include_bytes!("../data/sadie_d1_48k.hrir");

/// Head-related impulse responses on a regular azimuth × elevation grid.
pub struct HrirSet {
    pub rate: u32,
    pub taps: usize,
    n_az: usize,
    az_step: f32,
    els: Vec<f32>,
    /// Per direction (elevation-major): `taps` left samples, then `taps` right samples.
    data: Vec<f32>,
}

impl HrirSet {
    /// The table bundled with Harp as measured (48 kHz, 256 taps).
    pub fn raw() -> &'static HrirSet {
        static SET: OnceLock<HrirSet> = OnceLock::new();
        SET.get_or_init(|| HrirSet::parse(BUILTIN).expect("built-in HRIR table"))
    }

    /// The table Harp renders with: the measured one with its tone corrected so a centred
    /// sound comes out flat (see `eq.rs`); 512 taps.
    pub fn builtin() -> &'static HrirSet {
        static SET: OnceLock<HrirSet> = OnceLock::new();
        SET.get_or_init(|| HrirSet::raw().equalized(crate::render::SPEAKER_AZ))
    }

    /// The built-in table at another sample rate (an Android output often runs at 44.1 kHz):
    /// every response resampled (windowed-sinc), the tone correction designed again.
    pub fn builtin_at(rate: u32) -> &'static HrirSet {
        if rate == HrirSet::raw().rate {
            return HrirSet::builtin();
        }
        static SETS: std::sync::Mutex<Vec<(u32, &'static HrirSet)>> = std::sync::Mutex::new(Vec::new());
        let mut sets = SETS.lock().unwrap();
        if let Some(&(_, s)) = sets.iter().find(|(r, _)| *r == rate) {
            return s;
        }
        let s: &'static HrirSet = Box::leak(Box::new(HrirSet::raw().resampled(rate).equalized(crate::render::SPEAKER_AZ)));
        sets.push((rate, s));
        s
    }

    /// A copy at `rate`, each response resampled with a Hann-windowed sinc (cut-off at the
    /// lower Nyquist, so nothing folds back).
    pub fn resampled(&self, rate: u32) -> HrirSet {
        let ratio = rate as f64 / self.rate as f64;
        let taps = ((self.taps as f64 * ratio).ceil() as usize).max(1);
        let cutoff = ratio.min(1.0);
        const HALF: i64 = 16;
        let mut data = Vec::with_capacity(self.data.len() / self.taps * taps);
        for ir in self.data.chunks(self.taps) {
            for n in 0..taps {
                let t = n as f64 / ratio; // position in the source
                let c = t.floor() as i64;
                let mut acc = 0.0f64;
                for k in (c - HALF + 1)..=(c + HALF) {
                    if k < 0 || k as usize >= self.taps {
                        continue;
                    }
                    let x = t - k as f64;
                    let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x) };
                    let w = 0.5 + 0.5 * (std::f64::consts::PI * x / HALF as f64).cos();
                    acc += ir[k as usize] as f64 * sinc * w;
                }
                data.push(acc as f32);
            }
        }
        HrirSet { rate, taps, n_az: self.n_az, az_step: self.az_step, els: self.els.clone(), data }
    }

    /// The left and right ears' response to the same signal from speakers at ±`spk_az`.
    pub fn centre_response(&self, spk_az: f32) -> (Vec<f32>, Vec<f32>) {
        let (mut l1, mut r1, mut l2, mut r2) = (vec![0.0; self.taps], vec![0.0; self.taps], vec![0.0; self.taps], vec![0.0; self.taps]);
        self.lookup(spk_az, 0.0, &mut l1, &mut r1);
        self.lookup(-spk_az, 0.0, &mut l2, &mut r2);
        let l: Vec<f32> = l1.iter().zip(&l2).map(|(a, b)| a + b).collect();
        let r: Vec<f32> = r1.iter().zip(&r2).map(|(a, b)| a + b).collect();
        (l, r)
    }

    /// A copy with every response filtered so a centred sound from ±`spk_az` is flat.
    pub fn equalized(&self, spk_az: f32) -> HrirSet {
        let (l, r) = self.centre_response(spk_az);
        let (ml, mr) = (crate::eq::magnitude(&l), crate::eq::magnitude(&r));
        // Both ears together (the head is nearly symmetric): the power average.
        let avg: Vec<f32> = ml.iter().zip(&mr).map(|(a, b)| ((a * a + b * b) / 2.0).sqrt()).collect();
        let fir = crate::eq::flattening_filter(&avg, self.rate as f32, self.taps);
        // Whole partitions for the renderer.
        let taps = (self.taps * 2).div_ceil(crate::render::BLOCK) * crate::render::BLOCK;
        let mut data = Vec::with_capacity(self.data.len() * 2);
        for d in self.data.chunks(self.taps) {
            data.extend(crate::eq::convolve(d, &fir, taps));
        }
        HrirSet { rate: self.rate, taps, n_az: self.n_az, az_step: self.az_step, els: self.els.clone(), data }
    }

    pub fn parse(b: &[u8]) -> Result<HrirSet, String> {
        let mut r = Reader { b, i: 0 };
        if r.take(8)? != b"HRIRGRD1" {
            return Err("not an HRIR grid file".into());
        }
        let rate = r.u32()?;
        let taps = r.u32()? as usize;
        let n_az = r.u32()? as usize;
        let n_el = r.u32()? as usize;
        if taps == 0 || taps > 4096 || n_az < 4 || n_el == 0 || n_el > 64 {
            return Err("bad HRIR grid header".into());
        }
        let az_step = r.f32()?;
        let els = (0..n_el).map(|_| r.f32()).collect::<Result<Vec<_>, _>>()?;
        let scale = r.f32()?;
        let mut data = Vec::with_capacity(n_az * n_el * taps * 2);
        for _ in 0..n_az * n_el {
            let _ldelay = r.f32()?;
            let _rdelay = r.f32()?;
            for _ in 0..taps * 2 {
                data.push(r.i16()? as f32 * scale);
            }
        }
        Ok(HrirSet { rate, taps, n_az, az_step, els, data })
    }

    fn dir(&self, el_i: usize, az_i: usize) -> (&[f32], &[f32]) {
        let k = (el_i * self.n_az + az_i % self.n_az) * self.taps * 2;
        (&self.data[k..k + self.taps], &self.data[k + self.taps..k + 2 * self.taps])
    }

    /// The impulse responses for a source at `az` degrees (0 = front, counter-clockwise, so
    /// +90 = left) and `el` degrees (up positive), blended from the four nearest grid points.
    pub fn lookup(&self, az: f32, el: f32, left: &mut [f32], right: &mut [f32]) {
        let az = az.rem_euclid(360.0) / self.az_step;
        let a0 = az.floor() as usize % self.n_az;
        let a1 = (a0 + 1) % self.n_az;
        let fa = az - az.floor();
        let el = el.clamp(self.els[0], *self.els.last().unwrap());
        let mut e0 = 0;
        while e0 + 1 < self.els.len() && self.els[e0 + 1] <= el {
            e0 += 1;
        }
        let e1 = (e0 + 1).min(self.els.len() - 1);
        let fe = if e1 == e0 { 0.0 } else { (el - self.els[e0]) / (self.els[e1] - self.els[e0]) };
        let w = [(1.0 - fa) * (1.0 - fe), fa * (1.0 - fe), (1.0 - fa) * fe, fa * fe];
        let pts = [(e0, a0), (e0, a1), (e1, a0), (e1, a1)];
        left.iter_mut().for_each(|s| *s = 0.0);
        right.iter_mut().for_each(|s| *s = 0.0);
        for (&(e, a), &wk) in pts.iter().zip(&w) {
            if wk == 0.0 {
                continue;
            }
            let (l, r) = self.dir(e, a);
            for i in 0..self.taps {
                left[i] += wk * l[i];
                right[i] += wk * r[i];
            }
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.i + n > self.b.len() {
            return Err("HRIR file too short".into());
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum()
    }

    #[test]
    fn builtin_table_loads_and_left_is_left() {
        assert_eq!((HrirSet::raw().rate, HrirSet::raw().taps), (48000, 256));
        let s = HrirSet::builtin();
        assert_eq!((s.rate, s.taps), (48000, 512));
        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        s.lookup(90.0, 0.0, &mut l, &mut r);
        assert!(energy(&l) > 10.0 * energy(&r), "a source at +90° is louder in the left ear");
        s.lookup(-90.0, 0.0, &mut l, &mut r);
        assert!(energy(&r) > 10.0 * energy(&l), "a source at -90° is louder in the right ear");
    }

    #[test]
    fn lookup_on_a_grid_point_is_exact_and_between_points_blends() {
        let s = HrirSet::raw();
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        s.lookup(30.0, 0.0, &mut l, &mut r);
        let (gl, _) = s.dir(2, 6);
        assert_eq!(&l[..], gl);
        let (mut l2, mut r2) = (vec![0.0; 256], vec![0.0; 256]);
        s.lookup(32.5, 0.0, &mut l2, &mut r2);
        let (a, _) = s.dir(2, 6);
        let (b, _) = s.dir(2, 7);
        for i in 0..256 {
            assert!((l2[i] - 0.5 * (a[i] + b[i])).abs() < 1e-6);
        }
        // Angles wrap.
        s.lookup(390.0, 0.0, &mut l2, &mut r2);
        assert_eq!(l2, l);
        let _ = r;
    }

    /// Level of a response at `f` Hz, in dB.
    fn db_at(ir: &[f32], f: f32) -> f32 {
        let w = 2.0 * std::f32::consts::PI * f / 48000.0;
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (k, &h) in ir.iter().enumerate() {
            re += h * (w * k as f32).cos();
            im -= h * (w * k as f32).sin();
        }
        10.0 * (re * re + im * im).log10()
    }

    #[test]
    fn the_table_at_44_1_khz_keeps_direction_and_level() {
        let s = HrirSet::builtin_at(44100);
        assert_eq!(s.rate, 44100);
        assert_eq!(s.taps % crate::render::BLOCK, 0);
        let (mut l, mut r) = (vec![0.0; s.taps], vec![0.0; s.taps]);
        s.lookup(90.0, 0.0, &mut l, &mut r);
        assert!(energy(&l) > 10.0 * energy(&r));
        // About the same centred level as at 48 kHz (energy scales with the rate).
        let (c44, _) = s.centre_response(30.0);
        let (c48, _) = HrirSet::builtin().centre_response(30.0);
        let ratio = energy(&c44) / energy(&c48) * 48000.0 / 44100.0;
        assert!((0.7..1.4).contains(&ratio), "level ratio {ratio}");
    }

    #[test]
    fn a_centred_sound_comes_out_flat_after_correction() {
        // Before: +4 dB at 4 kHz and -7.7 dB at 8 kHz for a centred voice (measured).
        let (raw, _) = HrirSet::raw().centre_response(30.0);
        let (eq, _) = HrirSet::builtin().centre_response(30.0);
        let bands = [300.0f32, 500.0, 1000.0, 2000.0, 3000.0, 4000.0, 6000.0, 8000.0, 10000.0, 12000.0];
        let spread = |ir: &[f32]| {
            let v: Vec<f32> = bands.iter().map(|&f| db_at(ir, f)).collect();
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        let (before, after) = (spread(&raw), spread(&eq));
        eprintln!("centre response spread 300 Hz-12 kHz: {before:.1} dB before, {after:.1} dB after");
        assert!(after < 4.0 && after < before / 2.0, "before {before} after {after}");
        let peak = bands.iter().map(|&f| db_at(&eq, f)).fold(f32::MIN, f32::max);
        assert!(peak < 2.5, "a centred voice must not come out louder than it went in: {peak} dB");
    }
}
