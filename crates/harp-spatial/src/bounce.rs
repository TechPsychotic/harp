//! "3D Bounce": the whole stage moves around the listener, made up as it goes, never the same
//! twice, and in time with the music.
//!
//! Moves follow one another at random (never the same kind twice in a row, except hops):
//! - **hop**: a glide to a new spot with a small springy overshoot;
//! - **orbit**: a sweep of ¾ to 1½ turns around the head, rising and falling;
//! - **swoop**: up over the head and down behind (or the other way);
//! - **whisper**: in close to one ear, then away;
//! - **drift**: a slow wander nearby.
//!
//! Between moves the stage rests a moment; a beat in the music (a jump in the bass) ends the rest
//! early, so moves start on the beat. Positions: (degrees left of front, degrees up, metres).

const EL_MIN: f32 = -30.0;
const EL_MAX: f32 = 70.0;
const D_MIN: f32 = 0.6;
const D_MAX: f32 = 4.5;

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Ease-in-out with a little overshoot at the end (the "bounce").
fn springy(u: f32) -> f32 {
    let x = u.clamp(0.0, 1.0);
    let c = 1.0f32;
    if x < 0.5 {
        (2.0 * x).powi(2) * ((c + 1.0) * 2.0 * x - c) / 2.0
    } else {
        ((2.0 * x - 2.0).powi(2) * ((c + 1.0) * (2.0 * x - 2.0) + c) + 2.0) / 2.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Hop,
    Orbit,
    Swoop,
    Whisper,
    Drift,
}

type Pos = (f32, f32, f32);

#[derive(Clone, Copy, Debug)]
struct Move {
    kind: Kind,
    from: Pos,
    to: Pos,
    /// Orbit: degrees to sweep (signed). Swoop: the height of the arc.
    extra: f32,
    dur: f32,
}

impl Move {
    fn at(&self, u: f32) -> Pos {
        let (a0, e0, d0) = self.from;
        let (a1, e1, d1) = self.to;
        match self.kind {
            Kind::Hop => {
                let k = springy(u);
                (wrap(a0 + wrap(a1 - a0) * k), e0 + (e1 - e0) * k, d0 + (d1 - d0) * smooth(u))
            }
            Kind::Orbit => {
                let k = smooth(u);
                let rise = 18.0 * (std::f32::consts::PI * 2.0 * u).sin() * (std::f32::consts::PI * u).sin();
                (wrap(a0 + self.extra * k), e0 + (e1 - e0) * k + rise, d0 + (d1 - d0) * k)
            }
            Kind::Swoop => {
                let k = smooth(u);
                let arc = self.extra * (std::f32::consts::PI * k).sin();
                (wrap(a0 + wrap(a1 - a0) * k), (e0 + (e1 - e0) * k + arc).min(EL_MAX), d0 + (d1 - d0) * k)
            }
            Kind::Whisper | Kind::Drift => {
                let k = smooth(u);
                (wrap(a0 + wrap(a1 - a0) * k), e0 + (e1 - e0) * k, d0 + (d1 - d0) * k)
            }
        }
    }
}

pub struct Bounce {
    rng: u64,
    pos: Pos,
    mv: Option<Move>,
    t: f32,
    rest: f32,
    rested: f32,
    last: Option<Kind>,
    clock: f32,
}

impl Bounce {
    pub fn new(seed: u64) -> Bounce {
        Bounce { rng: seed | 1, pos: (0.0, 0.0, crate::render::SPEAKER_DIST), mv: None, t: 0.0, rest: 1.0, rested: 0.0, last: None, clock: 0.0 }
    }

    fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        ((self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    /// A direction, in front of the listener about two times in three: generic HRTFs make the
    /// front the hardest place to hear, so the stage spends more time there.
    fn front_az(&mut self) -> f32 {
        if self.rand() < 0.65 {
            self.range(-75.0, 75.0)
        } else {
            self.range(-180.0, 180.0)
        }
    }

    /// Long enough that the fastest moment of a glide stays under ~550°/s.
    fn glide_dur(&mut self, deg: f32, lo: f32, hi: f32, peak_factor: f32) -> f32 {
        self.range(lo, hi).max(peak_factor * deg.abs() / 550.0)
    }

    fn pick(&mut self) -> Kind {
        loop {
            let r = self.rand();
            let k = match r {
                r if r < 0.38 => Kind::Hop,
                r if r < 0.58 => Kind::Orbit,
                r if r < 0.73 => Kind::Swoop,
                r if r < 0.86 => Kind::Whisper,
                _ => Kind::Drift,
            };
            if k == Kind::Hop || Some(k) != self.last {
                return k;
            }
        }
    }

    fn start(&mut self) {
        let kind = self.pick();
        let from = self.pos;
        let side = if self.rand() < 0.5 { 1.0 } else { -1.0 };
        let mv = match kind {
            Kind::Hop => {
                // Somewhere at least 50° away, in front more often than not.
                let mut az = self.front_az();
                for _ in 0..6 {
                    if wrap(az - from.0).abs() >= 50.0 {
                        break;
                    }
                    az = self.front_az();
                }
                let to = (az, self.range(-25.0, 60.0), self.range(0.8, 3.8));
                let span = wrap(to.0 - from.0).abs().max((to.1 - from.1).abs());
                let dur = self.glide_dur(span, 0.8, 1.8, 4.0); // the springy ease peaks at 4x the average speed
                Move { kind, from, to, extra: 0.0, dur }
            }
            Kind::Orbit => {
                // Most of a turn or more, ending in front more often than not.
                let end = self.front_az();
                let sweep = wrap(end - from.0) + side * 360.0;
                let to = (end, self.range(-15.0, 35.0), self.range(1.0, 3.0));
                let dur = self.glide_dur(sweep, 3.5, 7.0, 1.5);
                Move { kind, from, to, extra: sweep, dur }
            }
            Kind::Swoop => {
                // Over the top to roughly the other side (behind to front, or front to behind).
                let to = (wrap(from.0 + 180.0 + self.range(-40.0, 40.0)), self.range(-20.0, 10.0), self.range(1.2, 3.0));
                let peak = EL_MAX - from.1.max(to.1);
                Move { kind, from, to, extra: peak.max(20.0), dur: self.range(2.4, 4.0) }
            }
            Kind::Whisper => {
                let to = (side * self.range(70.0, 110.0), self.range(-5.0, 15.0), self.range(D_MIN, 0.8));
                let dur = self.glide_dur(wrap(to.0 - from.0), 1.5, 2.8, 1.5);
                Move { kind, from, to, extra: 0.0, dur }
            }
            Kind::Drift => {
                // A slow wander that leans back toward the front.
                let to = (wrap(from.0 * 0.6 + self.range(-40.0, 40.0)), (from.1 + self.range(-15.0, 15.0)).clamp(EL_MIN, 45.0), self.range(1.2, 2.5));
                Move { kind, from, to, extra: 0.0, dur: self.range(3.0, 6.0) }
            }
        };
        self.mv = Some(mv);
        self.t = 0.0;
        self.last = Some(kind);
    }

    /// Advances by `dt` seconds; `beat` when the music just hit a beat. Returns where the stage
    /// is: (degrees left of front, degrees up, metres away).
    pub fn step(&mut self, dt: f32, beat: bool) -> Pos {
        let dt = dt.clamp(0.0, 0.25);
        self.clock += dt;
        match self.mv {
            Some(mv) => {
                self.t += dt;
                let u = self.t / mv.dur;
                self.pos = mv.at(u);
                if u >= 1.0 {
                    self.pos = mv.at(1.0);
                    self.mv = None;
                    self.rested = 0.0;
                    // A whisper goes on at once; otherwise a short rest (a beat may end it).
                    self.rest = if mv.kind == Kind::Whisper { self.range(0.3, 0.8) } else { self.range(0.4, 2.0) };
                }
            }
            None => {
                self.rested += dt;
                if self.rested >= self.rest || (beat && self.rested >= 0.25) {
                    self.start();
                }
            }
        }
        // A gentle bob on top, and the limits.
        let bob = 3.0 * (2.0 * std::f32::consts::PI * self.clock / 2.7).sin();
        (self.pos.0, (self.pos.1 + bob).clamp(EL_MIN, EL_MAX), self.pos.2.clamp(D_MIN, D_MAX))
    }
}

/// Finds beats: a jump in the bass energy of a block above its recent average.
pub struct BeatDetector {
    lp: f32,
    a: f32,
    avg: f32,
    since: f32,
    rate: f32,
}

impl BeatDetector {
    pub fn new(rate: f32) -> BeatDetector {
        BeatDetector { lp: 0.0, a: (-2.0 * std::f32::consts::PI * 150.0 / rate).exp(), avg: 0.0, since: 1.0, rate }
    }

    /// A chunk of interleaved stereo (any length); true on a beat (at most about four a second).
    pub fn feed(&mut self, block: &[f32]) -> bool {
        let block_s = (block.len() / 2) as f32 / self.rate;
        let mut e = 0.0;
        for f in block.chunks_exact(2) {
            let x = 0.5 * (f[0] + f[1]);
            self.lp = (1.0 - self.a) * x + self.a * self.lp;
            e += self.lp * self.lp;
        }
        e /= (block.len() / 2).max(1) as f32;
        self.since += block_s;
        let k = (block_s / 1.0).min(1.0);
        let beat = e > 1e-5 && e > 1.7 * self.avg && self.since > 0.25;
        self.avg += (e - self.avg) * k;
        if beat {
            self.since = 0.0;
        }
        beat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_everywhere_smoothly_and_never_repeats() {
        let mut b = Bounce::new(42);
        let dt = 256.0 / 48000.0;
        let mut seen = [false; 8];
        let (mut hi, mut lo, mut near, mut far) = (f32::MIN, f32::MAX, f32::MAX, f32::MIN);
        let mut prev = b.step(dt, false);
        let mut max_step = 0.0f32;
        let mut kinds = vec![];
        let (mut front, mut total) = (0usize, 0usize);
        for _ in 0..(600.0 / dt) as usize {
            let p = b.step(dt, false);
            total += 1;
            if p.0.abs() < 90.0 {
                front += 1;
            }
            seen[(p.0.rem_euclid(360.0) / 45.0) as usize % 8] = true;
            (hi, lo, near, far) = (hi.max(p.1), lo.min(p.1), near.min(p.2), far.max(p.2));
            max_step = max_step.max(wrap(p.0 - prev.0).abs()).max((p.1 - prev.1).abs());
            prev = p;
            if let Some(mv) = b.mv {
                if b.t == dt.min(mv.dur) || kinds.is_empty() {
                    kinds.push((mv.kind, (mv.to.0 / 10.0).round() as i32, (mv.to.1 / 10.0).round() as i32));
                }
            }
        }
        assert!(seen.iter().all(|&s| s), "every direction: {seen:?}");
        assert!(hi > 50.0 && lo < -15.0, "up and down: {lo}..{hi}");
        assert!(near < 0.8 && far > 3.5, "near and far: {near}..{far}");
        assert!(max_step < 3.5, "no jumps: {max_step}° in one block");
        assert!(front as f32 > 0.55 * total as f32, "in front {front} of {total} blocks");
        for k in [Kind::Hop, Kind::Orbit, Kind::Swoop, Kind::Whisper, Kind::Drift] {
            assert!(kinds.iter().any(|x| x.0 == k), "{k:?} happens");
        }
        // No cycle: the sequence of moves doesn't repeat with any short period.
        for period in 1..=30 {
            let repeats = kinds.windows(period + 1).all(|w| w[0] == w[period]);
            assert!(!repeats, "repeats every {period} moves");
        }
    }

    #[test]
    fn beats_start_the_next_move() {
        // While resting, a beat starts the next move at once.
        let mut b = Bounce::new(7);
        let dt = 0.01;
        while b.mv.is_none() {
            b.step(dt, false);
        }
        while b.mv.is_some() {
            b.step(dt, false);
        }
        b.rest = 5.0; // a long rest...
        for _ in 0..30 {
            b.step(dt, false);
        }
        assert!(b.mv.is_none());
        b.step(dt, true); // ...cut short by a beat
        assert!(b.mv.is_some());
    }

    #[test]
    fn finds_kick_drums() {
        let mut d = BeatDetector::new(48000.0);
        let mut hits = vec![];
        let mut t = 0usize;
        for blk in 0..(48000 * 4 / 256) {
            let mut block = vec![0.0f32; 512];
            for i in 0..256 {
                let s = (t % 24000) as f32 / 48000.0; // a kick every 0.5 s
                let x = 0.6 * (2.0 * std::f32::consts::PI * 60.0 * s).sin() * (-s * 18.0).exp() + 0.02 * ((t * 7919 % 1000) as f32 / 1000.0 - 0.5);
                block[2 * i] = x;
                block[2 * i + 1] = x;
                t += 1;
            }
            if d.feed(&block) {
                hits.push(blk);
            }
        }
        assert!((6..=10).contains(&hits.len()), "beats found: {}", hits.len());
    }
}
