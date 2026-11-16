//! Stereo to binaural.
//!
//! The two channels play from virtual speakers at ±30° in front of the listener, in a small
//! virtual room: each speaker's sound reaches the ears straight (filtered with the measured
//! HRIRs for its direction relative to the head) and by six first reflections (walls, floor,
//! ceiling), each placed with a spherical-head model (the delay between the ears and the
//! head's shadow), which is cheap and changes smoothly. A light late reverb follows. The reflections are what put the sound outside the head;
//! that they turn with the room (not with the head) is much of what makes head tracking
//! noticeable.
//!
//! When the head turns, filters change and the output crossfades from the old to the new
//! filters over one block, so there is no click. Bass below the crossover skips the HRTF. A
//! look-ahead limiter catches peaks (no clipping, no waveshaping).

use crate::hrir::HrirSet;
use sofar::reader::Filter;
use sofar::render::Renderer;

/// Frames per processing block (5.3 ms at 48 kHz).
pub const BLOCK: usize = 256;
/// Where the virtual speakers stand (degrees left of front).
pub const SPEAKER_AZ: f32 = 30.0;
const CROSSOVER_HZ: f32 = 150.0;
/// Filters are only swapped when a source's direction moved at least this much.
const MIN_STEP_DEG: f32 = 0.75;
/// Input history kept for the reflections' delays (≈ 42 ms).
const HISTORY: usize = 2048;
/// Limiter look-ahead (1.3 ms) and ceiling (−1 dBFS).
pub const LOOKAHEAD: usize = 64;
const CEILING: f32 = 0.891;
const SOUND_MPS: f32 = 343.0;
/// How far the virtual speakers stand (metres).
pub const SPEAKER_DIST: f32 = 1.8;
/// Height and behind cues: sound from above is brighter around 8 kHz, from below duller, from
/// behind darker above 4.5 kHz (the "directional bands" of human hearing). Generic HRTFs carry
/// these only weakly, so they are emphasised.
const HEIGHT_UP_DB: f32 = 9.0;
const HEIGHT_DOWN_DB: f32 = 6.0;
const BEHIND_DB: f32 = 8.0;
/// Behind also gets a little more around 1 kHz (the "behind" band); the front keeps its tone.
const BEHIND_MID_DB: f32 = 3.0;

/// One sound arriving at the listener: a speaker's direct sound or one of its reflections.
struct Source {
    ch: usize,
    az: f32,
    el: f32,
    delay: usize,
    gain: f32,
    step: f32,
    r: [Renderer; 2],
    active: usize,
    dir: Option<(f32, f32)>,
    filt: Filter,
    input: Vec<f32>,
    out: [[Vec<f32>; 2]; 2],
}

const HEAD_RADIUS_M: f32 = 0.0875;

/// One early reflection, rendered with a spherical-head model (Brown & Duda 1998): per ear a
/// delay (the interaural time difference) and a one-pole head-shadow filter, both following
/// the head smoothly.
struct Reflection {
    ch: usize,
    az: f32,
    el: f32,
    delay: f32,
    gain: f32,
    /// Per ear: current extra delay (samples), filter state.
    ear_delay: [f32; 2],
    z: [(f32, f32); 2],
}

impl Reflection {
    /// Delay (samples) and head-shadow filter coefficients for one ear at angle `theta`
    /// (radians) between the source and that ear's axis.
    fn ear_model(theta: f32, rate: f32) -> (f32, [f32; 3]) {
        let a_c = HEAD_RADIUS_M / SOUND_MPS;
        let t = if theta < std::f32::consts::FRAC_PI_2 { -a_c * theta.cos() } else { a_c * (theta - std::f32::consts::FRAC_PI_2) };
        let delay = (t + a_c) * rate;
        let theta_min = 150f32.to_radians();
        let alpha = (1.0 + 0.05) + (1.0 - 0.05) * (theta * std::f32::consts::PI / theta_min).cos().max(-1.0);
        let tau_k = (rate * 2.0) / (2.0 * SOUND_MPS / HEAD_RADIUS_M);
        let d = 1.0 + tau_k;
        (delay, [(1.0 + alpha * tau_k) / d, (1.0 - alpha * tau_k) / d, (1.0 - tau_k) / d])
    }
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
        let b = if high { [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0] } else { [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0] };
        Biquad { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [-2.0 * c / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }
    /// RBJ peaking EQ.
    fn peak(rate: f32, f0: f32, q: f32, db: f32) -> Self {
        let a = 10f32.powf(db / 40.0);
        let w = 2.0 * std::f32::consts::PI * f0 / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Biquad { b: [(1.0 + alpha * a) / a0, -2.0 * c / a0, (1.0 - alpha * a) / a0], a: [-2.0 * c / a0, (1.0 - alpha / a) / a0], z: [0.0; 2] }
    }
    /// RBJ high shelf (slope 1).
    fn high_shelf(rate: f32, f0: f32, db: f32) -> Self {
        let a = 10f32.powf(db / 40.0);
        let w = 2.0 * std::f32::consts::PI * f0 / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / 2.0 * 2f32.sqrt();
        let sa = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) - (a - 1.0) * c + sa;
        Biquad {
            b: [a * ((a + 1.0) + (a - 1.0) * c + sa) / a0, -2.0 * a * ((a - 1.0) + (a + 1.0) * c) / a0, a * ((a + 1.0) + (a - 1.0) * c - sa) / a0],
            a: [2.0 * ((a - 1.0) - (a + 1.0) * c) / a0, ((a + 1.0) - (a - 1.0) * c - sa) / a0],
            z: [0.0; 2],
        }
    }
    /// New coefficients, same state (for gains that move slowly).
    fn retune(&mut self, to: Biquad) {
        self.b = to.b;
        self.a = to.a;
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// Linkwitz-Riley 4th-order crossover: low + high sum to a flat all-pass.
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

/// A small feedback delay network: the room's late, diffuse sound (not head-tracked).
struct Reverb {
    lines: [Vec<f32>; 4],
    pos: [usize; 4],
    fb: [f32; 4],
    lp: [f32; 4],
    pre: Vec<f32>,
    pre_pos: usize,
    level: f32,
}

impl Reverb {
    fn new(rate: f32, rt60: f32, level: f32) -> Self {
        let lens = [1123usize, 1361, 1597, 1931];
        let fb = lens.map(|n| 10f32.powf(-3.0 * n as f32 / (rt60 * rate)));
        Reverb { lines: lens.map(|n| vec![0.0; n]), pos: [0; 4], fb, lp: [0.0; 4], pre: vec![0.0; (0.012 * rate) as usize], pre_pos: 0, level }
    }
    #[inline]
    fn run(&mut self, x: f32) -> (f32, f32) {
        let x = std::mem::replace(&mut self.pre[self.pre_pos], x);
        self.pre_pos = (self.pre_pos + 1) % self.pre.len();
        let o: [f32; 4] = std::array::from_fn(|i| self.lines[i][self.pos[i]]);
        for i in 0..4 {
            self.lp[i] += 0.55 * (o[i] - self.lp[i]); // the air and walls soak up the highs
        }
        let l = &self.lp;
        let m = [l[0] + l[1] + l[2] + l[3], l[0] - l[1] + l[2] - l[3], l[0] + l[1] - l[2] - l[3], l[0] - l[1] - l[2] + l[3]];
        for i in 0..4 {
            self.lines[i][self.pos[i]] = x + self.fb[i] * 0.5 * m[i];
            self.pos[i] = (self.pos[i] + 1) % self.lines[i].len();
        }
        (self.level * (o[0] - o[1] + o[2] - o[3]), self.level * (o[0] + o[1] - o[2] - o[3]))
    }
}

/// Look-ahead peak limiter: gain comes down smoothly before a peak arrives, so the output
/// never goes over the ceiling and nothing is clipped or waveshaped.
struct Limiter {
    buf: Vec<[f32; 2]>,
    want: Vec<f32>,
    pos: usize,
    env: f32,
    att: f32,
    rel: f32,
}

impl Limiter {
    fn new(rate: f32) -> Self {
        Limiter {
            buf: vec![[0.0; 2]; LOOKAHEAD],
            want: vec![1.0; LOOKAHEAD],
            pos: 0,
            env: 1.0,
            att: 1.0 - (-5.0 / LOOKAHEAD as f32).exp(),
            rel: 1.0 - (-1.0 / (0.15 * rate)).exp(),
        }
    }
    #[inline]
    fn run(&mut self, x: [f32; 2]) -> [f32; 2] {
        let peak = x[0].abs().max(x[1].abs());
        self.want[self.pos] = if peak > CEILING { CEILING / peak } else { 1.0 };
        let target = self.want.iter().cloned().fold(1.0f32, f32::min);
        let k = if target < self.env { self.att } else { self.rel };
        self.env += (target - self.env) * k;
        let d = std::mem::replace(&mut self.buf[self.pos], x);
        self.pos = (self.pos + 1) % LOOKAHEAD;
        [(d[0] * self.env).clamp(-1.0, 1.0), (d[1] * self.env).clamp(-1.0, 1.0)]
    }
}

/// The first reflections of a speaker at `az` degrees, `dist` metres away, in a 5 × 4 ×
/// 2.6 m room: (azimuth, elevation, extra delay in seconds, gain relative to the direct sound).
fn reflections(az: f32, dist: f32) -> Vec<(f32, f32, f32, f32)> {
    let (s, c) = az.to_radians().sin_cos();
    let sp = [dist * c, dist * s, 0.0f32];
    // Walls relative to the listener's ears: front, back, left, right, floor, ceiling, and how
    // much of the sound each reflects.
    let walls: [(usize, f32, f32); 6] = [(0, 2.2, 0.6), (0, -2.8, 0.55), (1, 2.0, 0.6), (1, -2.0, 0.6), (2, -1.2, 0.45), (2, 1.4, 0.5)];
    let d0 = dist;
    walls
        .iter()
        .map(|&(axis, at, refl)| {
            let mut p = sp;
            p[axis] = 2.0 * at - p[axis];
            let d = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            let az = p[1].atan2(p[0]).to_degrees();
            let el = p[2].atan2((p[0] * p[0] + p[1] * p[1]).sqrt()).to_degrees();
            (az, el, (d - d0) / SOUND_MPS, refl * d0 / d)
        })
        .collect()
}

pub struct Binaural {
    set: &'static HrirSet,
    sources: Vec<Source>,
    reflections: Vec<Reflection>,
    rate: f32,
    bass_bypass: bool,
    xo: [Crossover; 2],
    history: [Vec<f32>; 2],
    hist_pos: usize,
    low_delay: Vec<[f32; 2]>,
    low_pos: usize,
    gain: f32,
    reverb: Option<Reverb>,
    limiter: Limiter,
    lo_buf: Vec<[f32; 2]>,
    hi_buf: [Vec<f32>; 2],
    /// Direction cues on the direct sound, per ear: (8 kHz peak, 4.5 kHz shelf), and their
    /// current gains in dB.
    cues: [(Biquad, Biquad, Biquad); 2],
    cue_db: (f32, f32, f32),
    /// Distance of the stage (metres), as applied, and the air's low-pass state per ear.
    dist: f32,
    air: [f32; 2],
    /// Tests only: swap filters without the crossfade, to show the fade matters.
    #[cfg(test)]
    hard_swap: bool,
    pub yaw: f32,
    pub pitch: f32,
}

impl Binaural {
    /// A renderer at the built-in table's rate (48 kHz). `room` adds the virtual room's
    /// reflections and reverb.
    pub fn new(bass_bypass: bool, room: bool) -> Self {
        Binaural::new_at(48000, bass_bypass, room)
    }

    /// A renderer for audio at `rate` Hz.
    pub fn new_at(rate: u32, bass_bypass: bool, room: bool) -> Self {
        let set = HrirSet::builtin_at(rate);
        let rate = set.rate as f32;
        let mk = || Renderer::builder(set.taps).with_sample_rate(rate).with_partition_len(BLOCK).build().expect("renderer");
        let source = |ch: usize, az: f32, el: f32, delay: usize, gain: f32, step: f32| Source {
            ch,
            az,
            el,
            delay,
            gain,
            step,
            r: [mk(), mk()],
            active: 0,
            dir: None,
            filt: Filter::new(set.taps),
            input: vec![0.0; BLOCK],
            out: [[vec![0.0; BLOCK], vec![0.0; BLOCK]], [vec![0.0; BLOCK], vec![0.0; BLOCK]]],
        };
        let sources = vec![source(0, SPEAKER_AZ, 0.0, 0, 1.0, MIN_STEP_DEG), source(1, -SPEAKER_AZ, 0.0, 0, 1.0, MIN_STEP_DEG)];
        let mut refl_energy = 0.0;
        let mut refl = vec![];
        if room {
            for (ch, az) in [(0usize, SPEAKER_AZ), (1, -SPEAKER_AZ)] {
                for (raz, rel, dt, g) in reflections(az, 1.8) {
                    let delay = (dt * rate).min((HISTORY - 2 * BLOCK) as f32);
                    refl.push(Reflection { ch, az: raz, el: rel, delay, gain: g, ear_delay: [-1.0; 2], z: [(0.0, 0.0); 2] });
                    refl_energy += g * g;
                }
            }
        }
        // Bass lines up with the main peak of the front response.
        let (mut l, mut r) = (vec![0.0; set.taps], vec![0.0; set.taps]);
        set.lookup(0.0, 0.0, &mut l, &mut r);
        let peak = l.iter().enumerate().fold((0, 0.0f32), |m, (i, x)| if x.abs() > m.1 { (i, x.abs()) } else { m }).0;
        let mut b = Binaural {
            set,
            sources,
            reflections: refl,
            rate,
            bass_bypass,
            xo: [Crossover::new(rate); 2],
            history: [vec![0.0; HISTORY], vec![0.0; HISTORY]],
            hist_pos: 0,
            low_delay: vec![[0.0; 2]; peak.max(1)],
            low_pos: 0,
            // The table is corrected so a centred sound is flat; the room's extra energy
            // (per speaker) is taken back so the level stays the same.
            gain: 1.0 / (1.0 + refl_energy / 2.0).sqrt(),
            reverb: room.then(|| Reverb::new(rate, 0.4, 0.06)),
            limiter: Limiter::new(rate),
            lo_buf: vec![[0.0; 2]; BLOCK],
            hi_buf: [vec![0.0; BLOCK], vec![0.0; BLOCK]],
            cues: [(Biquad::peak(rate, 8000.0, 2.0, 0.0), Biquad::high_shelf(rate, 4500.0, 0.0), Biquad::peak(rate, 1000.0, 1.0, 0.0)); 2],
            cue_db: (0.0, 0.0, 0.0),
            dist: SPEAKER_DIST,
            air: [0.0; 2],
            #[cfg(test)]
            hard_swap: false,
            yaw: 0.0,
            pitch: 0.0,
        };
        b.steer(0.0, 0.0);
        for s in &mut b.sources {
            s.active = 1 - s.active; // the first filters are used straight away
        }
        b
    }

    /// Points every source's filters at the given head pose; marks which ones got a new
    /// filter on their inactive renderer (to be faded in).
    fn steer(&mut self, yaw: f32, pitch: f32) -> Vec<bool> {
        let set = self.set;
        self.sources
            .iter_mut()
            .map(|s| {
                let want = (s.az - yaw, s.el - pitch);
                let moved = match s.dir {
                    None => true,
                    Some((a, e)) => angle_diff(a, want.0).abs() >= s.step || (e - want.1).abs() >= s.step,
                };
                if moved {
                    set.lookup(want.0, want.1, &mut s.filt.left, &mut s.filt.right);
                    let next = 1 - s.active;
                    s.r[next].set_filter(&s.filt).expect("filter length");
                    s.dir = Some(want);
                }
                moved
            })
            .collect()
    }

    /// Renders one block: `input` and `out` are interleaved stereo, `BLOCK` frames.
    /// `yaw` is the head's turn to the left in degrees, `pitch` its tilt up.
    pub fn process(&mut self, yaw: f32, pitch: f32, input: &[f32], out: &mut [f32]) {
        self.process_at(yaw, pitch, SPEAKER_DIST, input, out)
    }

    /// The same with the stage `distance` metres away (3D Bounce moves it near and far): the
    /// direct sound gets louder or quieter and duller with distance while the room stays, the
    /// way distance sounds in a real room.
    pub fn process_at(&mut self, yaw: f32, pitch: f32, distance: f32, input: &[f32], out: &mut [f32]) {
        assert_eq!(input.len(), BLOCK * 2);
        assert_eq!(out.len(), BLOCK * 2);
        self.yaw = yaw;
        self.pitch = pitch;
        let start = self.hist_pos;
        for i in 0..BLOCK {
            for c in 0..2 {
                let x = input[2 * i + c];
                let (lo, hi) = if self.bass_bypass { self.xo[c].split(x) } else { (0.0, x) };
                self.history[c][(start + i) % HISTORY] = hi;
                self.hi_buf[c][i] = hi;
                self.lo_buf[i][c] = lo;
            }
        }
        self.hist_pos = (start + BLOCK) % HISTORY;
        let fade = self.steer(yaw, pitch);
        out.iter_mut().for_each(|s| *s = 0.0);
        for (k, s) in self.sources.iter_mut().enumerate() {
            let h = &self.history[s.ch];
            for i in 0..BLOCK {
                s.input[i] = h[(start + HISTORY + i - s.delay) % HISTORY];
            }
            // Both renderers always see the input, so either has the right history when its
            // filter is faded in.
            for j in 0..2 {
                let [ref mut ol, ref mut or] = s.out[j];
                s.r[j].process_block(&s.input, &mut ol[..], &mut or[..]).expect("block");
            }
            let a = s.active;
            let g = s.gain;
            #[cfg(test)]
            if self.hard_swap && fade[k] {
                s.active = 1 - a;
                for i in 0..BLOCK {
                    out[2 * i] += g * s.out[1 - a][0][i];
                    out[2 * i + 1] += g * s.out[1 - a][1][i];
                }
                continue;
            }
            if fade[k] {
                let b = 1 - a;
                for i in 0..BLOCK {
                    let w = 0.5 - 0.5 * (std::f32::consts::PI * (i as f32 + 0.5) / BLOCK as f32).cos();
                    out[2 * i] += g * ((1.0 - w) * s.out[a][0][i] + w * s.out[b][0][i]);
                    out[2 * i + 1] += g * ((1.0 - w) * s.out[a][1][i] + w * s.out[b][1][i]);
                }
                s.active = b;
            } else {
                for i in 0..BLOCK {
                    out[2 * i] += g * s.out[a][0][i];
                    out[2 * i + 1] += g * s.out[a][1][i];
                }
            }
        }
        // Direction cues and distance on the direct sound.
        let (az_c, el_c) = ((-yaw).to_radians(), (-pitch).to_radians());
        let up = el_c.sin();
        let behind = (-az_c.cos()).max(0.0) * el_c.cos();
        let target = (if up > 0.0 { HEIGHT_UP_DB * up } else { HEIGHT_DOWN_DB * up }, -BEHIND_DB * behind, BEHIND_MID_DB * behind);
        let step = |cur: f32, want: f32| cur + (want - cur).clamp(-0.4, 0.4);
        self.cue_db = (step(self.cue_db.0, target.0), step(self.cue_db.1, target.1), step(self.cue_db.2, target.2));
        let rate = self.rate;
        for (pk, sh, mid) in self.cues.iter_mut() {
            pk.retune(Biquad::peak(rate, 8000.0, 2.0, self.cue_db.0));
            sh.retune(Biquad::high_shelf(rate, 4500.0, self.cue_db.1));
            mid.retune(Biquad::peak(rate, 1000.0, 1.0, self.cue_db.2));
        }
        let d0 = self.dist;
        let d1 = distance.clamp(0.5, 8.0);
        self.dist = d1;
        for i in 0..BLOCK {
            let d = d0 + (d1 - d0) * (i as f32 / BLOCK as f32);
            let g = (SPEAKER_DIST / d).clamp(0.3, 2.2);
            // Air dulls only what is farther than the speakers' usual place (the usual tone
            // stays untouched).
            let a = if d <= SPEAKER_DIST {
                0.0
            } else {
                let fc = 20_000.0 / (1.0 + 0.5 * (d - SPEAKER_DIST));
                (-2.0 * std::f32::consts::PI * fc / rate).exp()
            };
            for c in 0..2 {
                let (pk, sh, mid) = &mut self.cues[c];
                let x = mid.run(sh.run(pk.run(out[2 * i + c])));
                self.air[c] = (1.0 - a) * x + a * self.air[c];
                out[2 * i + c] = g * self.air[c];
            }
        }
        let refl_gain = (SPEAKER_DIST / d1).sqrt().clamp(0.6, 1.5);
        // Early reflections, placed relative to the head.
        for rf in self.reflections.iter_mut() {
            let (az, el) = ((rf.az - yaw).to_radians(), (rf.el - pitch).to_radians());
            let dir = [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()];
            let h = &self.history[rf.ch];
            for (ear, axis_y) in [(0usize, 1.0f32), (1, -1.0)] {
                let theta = (dir[1] * axis_y).clamp(-1.0, 1.0).acos();
                let (d_target, c) = Reflection::ear_model(theta, rate);
                let d0 = if rf.ear_delay[ear] < 0.0 { d_target } else { rf.ear_delay[ear] };
                let (mut x1, mut y1) = rf.z[ear];
                for i in 0..BLOCK {
                    // The delay glides across the block: no clicks when the head moves.
                    let d = rf.delay + d0 + (d_target - d0) * (i as f32 / BLOCK as f32);
                    let pos = (start + HISTORY + i) as f32 - d;
                    let k = pos.floor();
                    let frac = pos - k;
                    let k = k as usize;
                    let x = h[k % HISTORY] * (1.0 - frac) + h[(k + 1) % HISTORY] * frac;
                    let y = c[0] * x + c[1] * x1 - c[2] * y1;
                    x1 = x;
                    y1 = y;
                    out[2 * i + ear] += refl_gain * rf.gain * y;
                }
                rf.z[ear] = (x1, y1);
                rf.ear_delay[ear] = d_target;
            }
        }
        let n = self.low_delay.len();
        for i in 0..BLOCK {
            let (mut wl, mut wr) = (0.0, 0.0);
            if let Some(rv) = self.reverb.as_mut() {
                (wl, wr) = rv.run(0.5 * (self.hi_buf[0][i] + self.hi_buf[1][i]));
            }
            let lo = std::mem::replace(&mut self.low_delay[self.low_pos], self.lo_buf[i]);
            self.low_pos = (self.low_pos + 1) % n;
            let y = [out[2 * i] * self.gain + wl + lo[0], out[2 * i + 1] * self.gain + wr + lo[1]];
            let z = self.limiter.run(y);
            out[2 * i] = z[0];
            out[2 * i + 1] = z[1];
        }
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    #[test]
    fn impulse_in_the_left_channel_gives_the_left_speakers_hrirs() {
        let mut b = Binaural::new(false, false);
        let set = HrirSet::builtin();
        let (mut l, mut r) = (vec![0.0; set.taps], vec![0.0; set.taps]);
        set.lookup(SPEAKER_AZ, 0.0, &mut l, &mut r);
        let g = b.gain();
        let mut got = vec![];
        for blk in 0..4 {
            let mut input = vec![0.0; BLOCK * 2];
            if blk == 0 {
                input[0] = 0.25;
            }
            let mut out = vec![0.0; BLOCK * 2];
            b.process(0.0, 0.0, &input, &mut out);
            got.extend(out);
        }
        for k in 0..set.taps {
            let (ol, or) = (got[2 * (k + LOOKAHEAD)], got[2 * (k + LOOKAHEAD) + 1]);
            assert!((ol - 0.25 * g * l[k]).abs() < 1e-4, "left ear sample {k}: {ol} vs {}", 0.25 * g * l[k]);
            assert!((or - 0.25 * g * r[k]).abs() < 1e-4, "right ear sample {k}");
        }
    }

    #[test]
    fn turning_the_head_left_moves_the_sound_to_the_right_ear() {
        for room in [false, true] {
            let mut b = Binaural::new(false, room);
            let mut seed = 1u32;
            let mut e = [0.0f32; 2];
            for blk in 0..40 {
                let mut input = vec![0.0; BLOCK * 2];
                for i in 0..BLOCK {
                    let x = 0.2 * noise(&mut seed);
                    input[2 * i] = x;
                    input[2 * i + 1] = x;
                }
                let mut out = vec![0.0; BLOCK * 2];
                b.process(60.0, 0.0, &input, &mut out);
                if blk > 8 {
                    for i in 0..BLOCK {
                        e[0] += out[2 * i] * out[2 * i];
                        e[1] += out[2 * i + 1] * out[2 * i + 1];
                    }
                }
            }
            let need = if room { 1.5 } else { 2.0 };
            assert!(e[1] > need * e[0], "room {room}: right ear {} vs left ear {}", e[1], e[0]);
        }
    }

    #[test]
    fn loud_centred_music_is_never_clipped() {
        // What cracked voices before: a loud centred signal summed over full scale and hit a
        // waveshaper. Now the peaks stay under the ceiling through the limiter, room on.
        let mut b = Binaural::new(true, true);
        let mut seed = 7u32;
        let mut peak = 0.0f32;
        let mut t = 0usize;
        for _ in 0..200 {
            let mut input = vec![0.0; BLOCK * 2];
            for i in 0..BLOCK {
                let v = 0.6 * (2.0 * std::f32::consts::PI * 3500.0 * t as f32 / 48000.0).sin() + 0.35 * noise(&mut seed);
                input[2 * i] = v;
                input[2 * i + 1] = v;
                t += 1;
            }
            let mut out = vec![0.0; BLOCK * 2];
            b.process(0.0, 0.0, &input, &mut out);
            peak = out.iter().fold(peak, |m, x| m.max(x.abs()));
        }
        assert!(peak <= CEILING + 0.01, "peak {peak}");
    }

    #[test]
    fn the_room_leaves_a_short_tail() {
        let mut dry = Binaural::new(true, false);
        let mut wet = Binaural::new(true, true);
        let mut seed = 3u32;
        let (mut tail_dry, mut tail_wet) = (0.0f32, 0.0f32);
        for blk in 0..60 {
            let mut input = vec![0.0; BLOCK * 2];
            if blk < 20 {
                for i in 0..BLOCK {
                    let x = 0.3 * noise(&mut seed);
                    input[2 * i] = x;
                    input[2 * i + 1] = x;
                }
            }
            let (mut o1, mut o2) = (vec![0.0; BLOCK * 2], vec![0.0; BLOCK * 2]);
            dry.process(0.0, 0.0, &input, &mut o1);
            wet.process(0.0, 0.0, &input, &mut o2);
            if (24..40).contains(&blk) {
                tail_dry += o1.iter().map(|x| x * x).sum::<f32>();
                tail_wet += o2.iter().map(|x| x * x).sum::<f32>();
            }
        }
        assert!(tail_wet > 100.0 * tail_dry.max(1e-9), "wet tail {tail_wet} vs dry {tail_dry}");
    }

    fn energy_in_band(b: &mut Binaural, yaw: f32, pitch: f32, dist: f32, f0: f32) -> f32 {
        let mut seed = 11u32;
        let mut bp = [Biquad::peak(48000.0, f0, 4.0, 24.0), Biquad::peak(48000.0, f0, 4.0, 24.0)];
        let mut e = 0.0;
        for blk in 0..120 {
            let mut input = vec![0.0; BLOCK * 2];
            for i in 0..BLOCK {
                let x = 0.1 * noise(&mut seed);
                input[2 * i] = x;
                input[2 * i + 1] = x;
            }
            let mut out = vec![0.0; BLOCK * 2];
            b.process_at(yaw, pitch, dist, &input, &mut out);
            if blk > 60 {
                for i in 0..BLOCK {
                    let y = bp[0].run(out[2 * i]) + bp[1].run(out[2 * i + 1]);
                    e += y * y;
                }
            }
        }
        e
    }

    #[test]
    fn sound_from_above_is_brighter_and_from_behind_darker() {
        // The stage up 45° (head looking down): more 8 kHz than level; behind: less 9 kHz.
        let level = energy_in_band(&mut Binaural::new(true, false), 0.0, 0.0, SPEAKER_DIST, 8000.0);
        let above = energy_in_band(&mut Binaural::new(true, false), 0.0, -45.0, SPEAKER_DIST, 8000.0);
        assert!(above > 1.6 * level, "above {above} vs level {level}");
        let front = energy_in_band(&mut Binaural::new(true, false), 0.0, 0.0, SPEAKER_DIST, 9000.0);
        let back = energy_in_band(&mut Binaural::new(true, false), 180.0, 0.0, SPEAKER_DIST, 9000.0);
        assert!(back < 0.6 * front, "behind {back} vs front {front}");
    }

    #[test]
    fn far_away_is_quieter_with_more_room() {
        // Direct-to-room ratio: compare the dry and room renderers at two distances.
        let e = |room: bool, d: f32| energy_in_band(&mut Binaural::new(true, room), 0.0, 0.0, d, 1000.0);
        let (near_dry, far_dry) = (e(false, 0.8), e(false, 4.0));
        let (near_wet, far_wet) = (e(true, 0.8), e(true, 4.0));
        assert!(near_dry > 8.0 * far_dry, "direct sound: near {near_dry} vs far {far_dry}");
        let room_share = |wet: f32, dry: f32| (wet - dry).abs() / wet;
        assert!(room_share(far_wet, far_dry) > room_share(near_wet, near_dry), "more room when far");
    }

    /// Energy above ~6 kHz in the output for a 1 kHz tone while `yaw(block)` moves: the
    /// tone itself has none there, so this is what filter changes add (clicks, zipper noise).
    fn click_energy(blocks: usize, hard: bool, yaw: impl Fn(usize) -> f32) -> f32 {
        let mut b = Binaural::new(true, false);
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
        // noise it adds well below what swapping filters outright does.
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
