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
    /// The table bundled with Harp (48 kHz, 256 taps).
    pub fn builtin() -> &'static HrirSet {
        static SET: OnceLock<HrirSet> = OnceLock::new();
        SET.get_or_init(|| HrirSet::parse(BUILTIN).expect("built-in HRIR table"))
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
        let s = HrirSet::builtin();
        assert_eq!((s.rate, s.taps), (48000, 256));
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        s.lookup(90.0, 0.0, &mut l, &mut r);
        assert!(energy(&l) > 10.0 * energy(&r), "a source at +90° is louder in the left ear");
        s.lookup(-90.0, 0.0, &mut l, &mut r);
        assert!(energy(&r) > 10.0 * energy(&l), "a source at -90° is louder in the right ear");
    }

    #[test]
    fn lookup_on_a_grid_point_is_exact_and_between_points_blends() {
        let s = HrirSet::builtin();
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
}
