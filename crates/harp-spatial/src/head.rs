//! Head orientation → how far the head is turned (and tilted), for the renderer.
//!
//! AirPods send their own fused orientation (a unit quaternion, bud → world, world Z up).
//! How a bud sits in the ear is arbitrary, so the angles of the quaternion itself mean
//! little:
//! - **Turning** is read from a bud axis that lies close to horizontal: its compass direction
//!   around the vertical, compared with where it pointed at the last recenter. Turning the
//!   head left makes that angle grow.
//! - **Looking up and down** needs to know which way the face points in the bud's frame. A
//!   one-time calibration gives it: face the screen, then look up; the rotation between the
//!   two is about the axis through the ears. From then on the face direction is followed
//!   (for turning too). A calibration belongs to one bud (the one sending motion), so up to
//!   two are kept and the one matching how the current bud sits is used.
//!
//! The scene drifts back to where you face once you hold your head still for a while (as on
//! Apple devices), and the angles are predicted a little ahead to make up for part of the
//! audio delay.

/// Bud → world rotation, (w, x, y, z).
pub type Quat = [f32; 4];

pub fn rotate(q: Quat, v: [f32; 3]) -> [f32; 3] {
    let [w, x, y, z] = q;
    let [vx, vy, vz] = v;
    [
        vx * (1.0 - 2.0 * (y * y + z * z)) + vy * 2.0 * (x * y - w * z) + vz * 2.0 * (x * z + w * y),
        vx * 2.0 * (x * y + w * z) + vy * (1.0 - 2.0 * (x * x + z * z)) + vz * 2.0 * (y * z - w * x),
        vx * 2.0 * (x * z - w * y) + vy * 2.0 * (y * z + w * x) + vz * (1.0 - 2.0 * (x * x + y * y)),
    ]
}

pub fn conj(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], -q[3]]
}

pub fn qmul(a: Quat, b: Quat) -> Quat {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let n = dot(v, v).sqrt().max(1e-9);
    [v[0] / n, v[1] / n, v[2] / n]
}

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// World up seen from the bud.
fn up_in_bud(q: Quat) -> [f32; 3] {
    rotate(conj(q), [0.0, 0.0, 1.0])
}

/// How the scene comes back to where you face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recenter {
    /// Never by itself (Recenter button only).
    Off,
    /// After 8 s held still, over about 15 s.
    Slow,
    /// After 3 s held still, over about 5 s: like AirPods with an iPhone or Mac.
    Dynamic,
}

impl Recenter {
    pub fn parse(s: &str) -> Recenter {
        match s {
            "off" => Recenter::Off,
            "slow" => Recenter::Slow,
            _ => Recenter::Dynamic,
        }
    }
    /// (seconds still before it starts, time constant)
    fn timing(self) -> Option<(f32, f32)> {
        match self {
            Recenter::Off => None,
            Recenter::Slow => Some((8.0, 15.0)),
            Recenter::Dynamic => Some((3.0, 5.0)),
        }
    }
}

/// The up/down calibration of one bud: the bud-frame axis that looking up turns about, and
/// where up was (in the bud's frame) while facing the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpCalibration {
    pub axis: [f32; 3],
    pub up: [f32; 3],
}

impl UpCalibration {
    /// Flattened for saving: 6 numbers per calibration.
    pub fn encode(list: &[UpCalibration]) -> Vec<f32> {
        list.iter().flat_map(|c| c.axis.into_iter().chain(c.up)).collect()
    }
    pub fn decode(v: &[f32]) -> Vec<UpCalibration> {
        v.chunks_exact(6)
            .map(|c| UpCalibration { axis: norm([c[0], c[1], c[2]]), up: norm([c[3], c[4], c[5]]) })
            .filter(|c| dot(c.axis, c.axis) > 0.5)
            .take(2)
            .collect()
    }
    fn forward(&self) -> [f32; 3] {
        norm(cross(self.up, self.axis))
    }
}

/// State of a calibration being taken.
#[derive(Clone, Debug, PartialEq)]
pub enum Calibrating {
    Idle,
    /// Seconds left.
    Running(f32),
    Done,
    Failed(String),
}

const CALIBRATION_S: f64 = 4.0;

struct Capture {
    q0: Option<Quat>,
    t0: f64,
    best: (f32, Quat),
}

#[derive(Clone, Debug)]
pub struct TrackerConfig {
    /// Seconds of look-ahead from the turning speed.
    pub predict_s: f32,
    /// Largest look-ahead correction, degrees.
    pub predict_max_deg: f32,
    pub recenter: Recenter,
    /// Below this turning speed (°/s) the head counts as still.
    pub still_dps: f32,
    /// Turn the other way (if a model reports its frame mirrored).
    pub flip: bool,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        TrackerConfig { predict_s: 0.05, predict_max_deg: 8.0, recenter: Recenter::Dynamic, still_dps: 15.0, flip: false }
    }
}

pub struct Tracker {
    pub cfg: TrackerConfig,
    /// The bud axis followed for turning (the face direction when calibrated).
    axis: Option<[f32; 3]>,
    using_calibration: Option<usize>,
    reference: f32,
    ref_pitch: f32,
    last: Option<(f32, f64)>,
    rate: f32,
    still_s: f32,
    /// The Recenter button also takes the current tilt as level.
    level_from_current: bool,
    yaw: f32,
    pitch: f32,
    pub calibrations: Vec<UpCalibration>,
    capture: Option<Capture>,
    pub calibrating: Calibrating,
}

impl Tracker {
    pub fn new(cfg: TrackerConfig) -> Self {
        Tracker {
            cfg,
            axis: None,
            using_calibration: None,
            reference: 0.0,
            ref_pitch: 0.0,
            last: None,
            rate: 0.0,
            still_s: 0.0,
            level_from_current: false,
            yaw: 0.0,
            pitch: 0.0,
            calibrations: vec![],
            capture: None,
            calibrating: Calibrating::Idle,
        }
    }

    /// Where you face now becomes the front (and your head's tilt now counts as level).
    pub fn recenter(&mut self) {
        self.restart();
        self.level_from_current = true;
    }

    /// Takes the front again on the next sample; up/down stays measured from true level
    /// (horizontal, as during the calibration).
    fn restart(&mut self) {
        self.axis = None;
        self.last = None;
        self.rate = 0.0;
        self.still_s = 0.0;
    }

    /// Starts an up/down calibration: face the screen now, then look up within 4 seconds.
    pub fn calibrate_up(&mut self) {
        self.capture = Some(Capture { q0: None, t0: 0.0, best: (0.0, [1.0, 0.0, 0.0, 0.0]) });
        self.calibrating = Calibrating::Running(CALIBRATION_S as f32);
    }

    fn calibration_step(&mut self, q: Quat, t: f64) {
        let Some(c) = self.capture.as_mut() else { return };
        let q0 = *c.q0.get_or_insert_with(|| {
            c.t0 = t;
            q
        });
        let mut r = qmul(conj(q0), q);
        if r[0] < 0.0 {
            r = [-r[0], -r[1], -r[2], -r[3]];
        }
        let angle = 2.0 * r[0].clamp(-1.0, 1.0).acos().to_degrees();
        if angle > c.best.0 {
            c.best = (angle, r);
        }
        let left = (CALIBRATION_S - (t - c.t0)) as f32;
        if left > 0.0 {
            self.calibrating = Calibrating::Running(left);
            return;
        }
        let (angle, r) = c.best;
        let up = up_in_bud(q0);
        self.capture = None;
        if angle < 12.0 {
            self.calibrating = Calibrating::Failed("Look further up (at the ceiling) and hold it".into());
            return;
        }
        let axis = norm([r[1], r[2], r[3]]);
        if dot(axis, up).abs() > 0.5 {
            self.calibrating = Calibrating::Failed("That was a turn to the side; look straight up instead".into());
            return;
        }
        let axis = norm([axis[0] - up[0] * dot(axis, up), axis[1] - up[1] * dot(axis, up), axis[2] - up[2] * dot(axis, up)]);
        let cal = UpCalibration { axis, up };
        // Replace the calibration of the same bud (it sits the same way), keep the other one.
        self.calibrations.retain(|c| dot(c.up, up) < 0.8);
        self.calibrations.insert(0, cal);
        self.calibrations.truncate(2);
        self.calibrating = Calibrating::Done;
        self.restart();
    }

    /// The calibration that fits how the bud sits now, if any.
    fn matching_calibration(&self, q: Quat) -> Option<usize> {
        let up = up_in_bud(q);
        self.calibrations
            .iter()
            .enumerate()
            .map(|(i, c)| (i, dot(c.up, up)))
            .filter(|&(_, d)| d > 0.8)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(i, _)| i)
    }

    /// Feeds one orientation sample taken at `t` seconds; returns (yaw, pitch) in degrees,
    /// yaw positive when the head is turned left of where it was recentred, pitch positive up.
    pub fn update(&mut self, q: Quat, t: f64) -> (f32, f32) {
        if self.capture.is_some() {
            self.calibration_step(q, t);
        }
        let cal = self.matching_calibration(q);
        if cal != self.using_calibration {
            self.using_calibration = cal;
            self.restart();
        }
        if self.axis.is_none() {
            let axis = match cal {
                Some(i) => self.calibrations[i].forward(),
                None => {
                    // The bud axis that is most horizontal right now: the most reliable compass.
                    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                    axes.iter().copied().min_by(|a, b| rotate(q, *a)[2].abs().partial_cmp(&rotate(q, *b)[2].abs()).unwrap()).unwrap()
                }
            };
            self.axis = Some(axis);
            let v = rotate(q, axis);
            self.reference = v[1].atan2(v[0]).to_degrees();
            // The face direction is horizontal when the head is as level as during calibration.
            self.ref_pitch = if cal.is_some() && self.level_from_current { v[2].clamp(-1.0, 1.0).asin().to_degrees() } else { 0.0 };
            self.level_from_current = false;
        }
        let v = rotate(q, self.axis.unwrap());
        let az = v[1].atan2(v[0]).to_degrees();
        let mut yaw = wrap(az - self.reference);
        let mut pitch = if cal.is_some() { v[2].clamp(-1.0, 1.0).asin().to_degrees() - self.ref_pitch } else { 0.0 };
        if let Some((prev, pt)) = self.last {
            let dt = (t - pt) as f32;
            if dt > 0.0 && dt < 0.5 {
                let inst = wrap(yaw - prev) / dt;
                self.rate += (inst - self.rate) * (dt / 0.05).min(1.0);
                if self.rate.abs() < self.cfg.still_dps {
                    self.still_s += dt;
                } else {
                    self.still_s = 0.0;
                }
                if let Some((after, tau)) = self.cfg.recenter.timing() {
                    if self.still_s > after {
                        let k = (dt / tau).min(1.0);
                        self.reference = wrap(self.reference + yaw * k);
                        yaw -= yaw * k;
                        self.ref_pitch += pitch * k;
                        pitch -= pitch * k;
                    }
                }
            }
        }
        self.last = Some((yaw, t));
        let ahead = (self.rate * self.cfg.predict_s).clamp(-self.cfg.predict_max_deg, self.cfg.predict_max_deg);
        let out = wrap(yaw + ahead);
        self.yaw = if self.cfg.flip { -out } else { out };
        self.pitch = pitch.clamp(-60.0, 60.0);
        (self.yaw, self.pitch)
    }

    pub fn yaw(&self) -> f32 {
        self.yaw
    }
}

/// "3D Bounce": where the whole stage is (degrees left of front, degrees up) at `t` seconds.
/// It glides around you through the full circle from one spot to the next with a small
/// springy overshoot, rests a moment, and bobs gently up and down.
pub fn bounce(t: f64) -> (f32, f32) {
    const SPOTS: [(f32, f32); 8] = [(0.0, 0.0), (95.0, 12.0), (200.0, -8.0), (290.0, 18.0), (150.0, 0.0), (35.0, -12.0), (250.0, 10.0), (330.0, 5.0)];
    const SEG: f64 = 4.5;
    const GLIDE: f64 = 2.4;
    let k = (t / SEG).floor() as usize;
    let into = t - k as f64 * SEG;
    let (a0, e0) = SPOTS[k % SPOTS.len()];
    let (a1, e1) = SPOTS[(k + 1) % SPOTS.len()];
    let x = (into / GLIDE).min(1.0) as f32;
    // ease-in-out-back: starts gently, overshoots a little, then settles (the "bounce")
    let c2 = 1.0f32;
    let e = if x < 0.5 {
        (2.0 * x).powi(2) * ((c2 + 1.0) * 2.0 * x - c2) / 2.0
    } else {
        ((2.0 * x - 2.0).powi(2) * ((c2 + 1.0) * (2.0 * x - 2.0) + c2) + 2.0) / 2.0
    };
    let da = wrap(a1 - a0);
    let az = wrap(a0 + da * e);
    let bob = 4.0 * (2.0 * std::f64::consts::PI * t / 2.7).sin() as f32;
    (az, e0 + (e1 - e0) * x + bob)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_angle(axis: [f32; 3], deg: f32) -> Quat {
        let h = deg.to_radians() / 2.0;
        let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        [h.cos(), axis[0] / n * h.sin(), axis[1] / n * h.sin(), axis[2] / n * h.sin()]
    }

    fn mul(a: Quat, b: Quat) -> Quat {
        [
            a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
            a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
            a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
            a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
        ]
    }

    #[test]
    fn a_tilted_bud_still_reads_the_head_turn() {
        // The bud sits rolled 120° and pitched 25° in the ear (like the real readings); the
        // head then turns 40° left about the vertical.
        let mount = mul(axis_angle([1.0, 0.0, 0.0], -120.0), axis_angle([0.0, 1.0, 0.0], 25.0));
        let start = mul(axis_angle([0.0, 0.0, 1.0], 17.0), mount);
        let cfg = TrackerConfig { predict_s: 0.0, recenter: Recenter::Off, ..Default::default() };
        let mut t = Tracker::new(cfg);
        assert!(t.update(start, 0.0).0.abs() < 0.01);
        let turned = mul(axis_angle([0.0, 0.0, 1.0], 40.0), start);
        let (yaw, _) = t.update(turned, 0.01);
        assert!((yaw - 40.0).abs() < 0.1, "yaw {yaw}");
        let right = mul(axis_angle([0.0, 0.0, 1.0], -70.0), start);
        assert!((t.update(right, 0.02).0 + 70.0).abs() < 0.1);
    }

    #[test]
    fn holding_the_head_turned_drifts_back_to_the_front() {
        let cfg = TrackerConfig { predict_s: 0.0, ..Default::default() };
        let mut t = Tracker::new(cfg);
        let q0 = axis_angle([1.0, 0.0, 0.0], 90.0);
        t.update(q0, 0.0);
        let q = mul(axis_angle([0.0, 0.0, 1.0], 30.0), q0);
        let mut yaw = 0.0;
        for i in 1..=6000 {
            yaw = t.update(q, i as f64 * 0.01).0; // 60 s held
        }
        assert!(yaw > 0.0 && yaw < 3.0, "after a minute held, yaw {yaw}");
    }

    #[test]
    fn a_look_to_the_side_and_back_keeps_the_front() {
        // Seen live: a 50° turn held 4 s used to move the front by ~14°. With "Slow" a short
        // look doesn't move it at all.
        let cfg = TrackerConfig { predict_s: 0.0, recenter: Recenter::Slow, ..Default::default() };
        let mut t = Tracker::new(cfg);
        let q0 = axis_angle([1.0, 0.0, 0.0], 90.0);
        let mut time = 0.0;
        let mut step = |t: &mut Tracker, deg: f32| {
            time += 0.025;
            t.update(mul(axis_angle([0.0, 0.0, 1.0], deg), q0), time).0
        };
        for _ in 0..200 {
            step(&mut t, 0.0); // 5 s facing front
        }
        for i in 0..20 {
            step(&mut t, 2.5 * i as f32); // turn left over 0.5 s
        }
        for _ in 0..160 {
            step(&mut t, 50.0); // hold 4 s
        }
        for i in 0..20 {
            step(&mut t, 50.0 - 2.5 * i as f32);
        }
        let mut yaw = 0.0;
        for _ in 0..40 {
            yaw = step(&mut t, 0.0);
        }
        assert!(yaw.abs() < 1.0, "back at the front, yaw {yaw}");
    }

    #[test]
    fn prediction_leads_a_steady_turn_and_flip_mirrors() {
        let cfg = TrackerConfig { predict_s: 0.1, recenter: Recenter::Off, ..Default::default() };
        let mut t = Tracker::new(cfg.clone());
        let q0 = axis_angle([1.0, 0.0, 0.0], 90.0);
        let mut yaw = 0.0;
        for i in 0..50 {
            // 50 °/s to the left
            let q = mul(axis_angle([0.0, 0.0, 1.0], i as f32 * 0.5), q0);
            yaw = t.update(q, i as f64 * 0.01).0;
        }
        let truth = 49.0 * 0.5;
        assert!((yaw - (truth + 5.0)).abs() < 0.6, "yaw {yaw} vs {truth}+5");
        let mut f = Tracker::new(TrackerConfig { flip: true, ..cfg });
        f.update(q0, 0.0);
        let q = mul(axis_angle([0.0, 0.0, 1.0], 20.0), q0);
        assert!(f.update(q, 0.5).0 < -19.0);
    }

    #[test]
    fn dynamic_recenter_follows_a_new_facing_within_seconds() {
        let mut t = Tracker::new(TrackerConfig { predict_s: 0.0, ..Default::default() });
        let q0 = axis_angle([1.0, 0.0, 0.0], 90.0);
        t.update(q0, 0.0);
        let q = mul(axis_angle([0.0, 0.0, 1.0], 40.0), q0);
        let mut yaw = 0.0;
        for i in 1..=480 {
            yaw = t.update(q, i as f64 * 0.025).0; // 12 s facing 40° left
        }
        assert!(yaw.abs() < 8.0, "after 12 s the scene has mostly followed: {yaw}");
    }

    /// A bud sitting rolled and pitched in the ear (like the real readings), on a head that
    /// turns `yaw` left and looks `pitch` up: bud → world.
    fn worn(yaw: f32, pitch: f32) -> Quat {
        let mount = mul(axis_angle([1.0, 0.0, 0.0], -120.0), axis_angle([0.0, 1.0, 0.0], 25.0));
        let head = mul(axis_angle([0.0, 0.0, 1.0], yaw), axis_angle([0.0, 1.0, 0.0], -pitch));
        mul(head, mount)
    }

    #[test]
    fn looking_up_and_down_after_calibration() {
        let mut t = Tracker::new(TrackerConfig { predict_s: 0.0, recenter: Recenter::Off, ..Default::default() });
        t.update(worn(0.0, 0.0), 0.0);
        t.calibrate_up();
        let mut time = 0.0;
        for i in 0..200 {
            time += 0.025;
            let p = if i < 40 { 0.0 } else { 35.0f32.min((i - 40) as f32) }; // look up
            t.update(worn(0.0, p), time);
        }
        assert_eq!(t.calibrating, Calibrating::Done);
        // Still looking up when it finishes: that must not become "level".
        let (_, p_up) = t.update(worn(0.0, 35.0), time + 0.0125);
        assert!((p_up - 35.0).abs() < 1.0, "still looking up: {p_up}");
        t.update(worn(0.0, 0.0), time + 0.025);
        let (yaw, pitch) = t.update(worn(0.0, 20.0), time + 0.05);
        assert!((pitch - 20.0).abs() < 1.0 && yaw.abs() < 1.0, "up 20: yaw {yaw} pitch {pitch}");
        let (yaw, pitch) = t.update(worn(30.0, -15.0), time + 0.075);
        assert!((pitch + 15.0).abs() < 1.5 && (yaw - 30.0).abs() < 1.5, "left 30 down 15: yaw {yaw} pitch {pitch}");
        // Saved and loaded.
        let saved = UpCalibration::encode(&t.calibrations);
        let back = UpCalibration::decode(&saved);
        assert_eq!(back.len(), 1);
        assert!(dot(back[0].axis, t.calibrations[0].axis) > 0.9999 && dot(back[0].up, t.calibrations[0].up) > 0.9999);
    }

    #[test]
    fn a_turn_is_not_taken_as_looking_up() {
        let mut t = Tracker::new(TrackerConfig { predict_s: 0.0, ..Default::default() });
        t.calibrate_up();
        for i in 0..200 {
            t.update(worn(30.0f32.min(i as f32), 0.0), i as f64 * 0.025);
        }
        assert!(matches!(t.calibrating, Calibrating::Failed(_)), "{:?}", t.calibrating);
        assert!(t.calibrations.is_empty());
    }

    #[test]
    fn bounce_goes_all_the_way_round_smoothly() {
        let mut seen = [false; 8];
        let mut prev = bounce(0.0);
        let mut max_step = 0.0f32;
        for i in 1..(45 * 100) {
            let t = i as f64 * 0.01;
            let (az, el) = bounce(t);
            seen[(az.rem_euclid(360.0) / 45.0) as usize % 8] = true;
            max_step = max_step.max(wrap(az - prev.0).abs()).max((el - prev.1).abs());
            assert!(el.abs() < 30.0);
            prev = (az, el);
        }
        assert!(seen.iter().all(|&s| s), "every direction is visited: {seen:?}");
        assert!(max_step < 3.0, "no jumps (largest step in 10 ms: {max_step}°)");
    }
}
