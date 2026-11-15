//! Head orientation → how far the head is turned, for the renderer.
//!
//! AirPods send their own fused orientation (a unit quaternion, bud → world, world Z up).
//! How a bud sits in the ear is arbitrary, so the angles of the quaternion itself mean
//! little. Instead one of the bud's axes that lies close to horizontal is followed, and its
//! compass direction around the vertical is compared with where it pointed at the last
//! recenter. Turning the head left makes that angle grow.
//!
//! The scene also drifts slowly back to the front while the head is held still (so a
//! slouch or a turned chair doesn't leave the music to one side, and the buds' own slow
//! drift is absorbed), and the angle is predicted a little ahead to make up for part of the
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

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

#[derive(Clone, Debug)]
pub struct TrackerConfig {
    /// Seconds of look-ahead from the turning speed.
    pub predict_s: f32,
    /// Largest look-ahead correction, degrees.
    pub predict_max_deg: f32,
    /// Time constant (s) of the drift back to the front while the head is still.
    pub recenter_tau_s: f32,
    /// Below this turning speed (°/s) the head counts as still.
    pub still_dps: f32,
    /// Turn the other way (if a model reports its frame mirrored).
    pub flip: bool,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        TrackerConfig { predict_s: 0.06, predict_max_deg: 12.0, recenter_tau_s: 12.0, still_dps: 15.0, flip: false }
    }
}

pub struct Tracker {
    pub cfg: TrackerConfig,
    axis: Option<[f32; 3]>,
    reference: f32,
    last: Option<(f32, f64)>,
    rate: f32,
    yaw: f32,
    pitch: f32,
}

impl Tracker {
    pub fn new(cfg: TrackerConfig) -> Self {
        Tracker { cfg, axis: None, reference: 0.0, last: None, rate: 0.0, yaw: 0.0, pitch: 0.0 }
    }

    /// The next recenter happens on the next sample.
    pub fn recenter(&mut self) {
        self.axis = None;
        self.last = None;
        self.rate = 0.0;
    }

    /// Feeds one orientation sample taken at `t` seconds; returns (yaw, pitch) in degrees,
    /// yaw positive when the head is turned left of where it was recentred, pitch positive up.
    pub fn update(&mut self, q: Quat, t: f64) -> (f32, f32) {
        if self.axis.is_none() {
            // The bud axis that is most horizontal right now: the most reliable compass.
            let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            let best = axes
                .iter()
                .copied()
                .min_by(|a, b| rotate(q, *a)[2].abs().partial_cmp(&rotate(q, *b)[2].abs()).unwrap())
                .unwrap();
            self.axis = Some(best);
            let v = rotate(q, best);
            self.reference = v[1].atan2(v[0]).to_degrees();
        }
        let v = rotate(q, self.axis.unwrap());
        let az = v[1].atan2(v[0]).to_degrees();
        let mut yaw = wrap(az - self.reference);
        if let Some((prev, pt)) = self.last {
            let dt = (t - pt) as f32;
            if dt > 0.0 && dt < 0.5 {
                let inst = wrap(yaw - prev) / dt;
                self.rate += (inst - self.rate) * (dt / 0.05).min(1.0);
                if self.rate.abs() < self.cfg.still_dps && self.cfg.recenter_tau_s > 0.0 {
                    let k = (dt / self.cfg.recenter_tau_s).min(1.0);
                    self.reference = wrap(self.reference + yaw * k);
                    yaw -= yaw * k;
                }
            }
        }
        self.last = Some((yaw, t));
        // Only turning is followed for now: nodding needs the face direction, which the
        // followed axis doesn't give for every way a bud can sit in the ear.
        self.pitch = 0.0;
        let ahead = (self.rate * self.cfg.predict_s).clamp(-self.cfg.predict_max_deg, self.cfg.predict_max_deg);
        let out = wrap(yaw + ahead);
        self.yaw = if self.cfg.flip { -out } else { out };
        (self.yaw, self.pitch)
    }

    pub fn yaw(&self) -> f32 {
        self.yaw
    }
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
        let cfg = TrackerConfig { predict_s: 0.0, recenter_tau_s: 0.0, ..Default::default() };
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
        let cfg = TrackerConfig { predict_s: 0.0, recenter_tau_s: 2.0, ..Default::default() };
        let mut t = Tracker::new(cfg);
        let q0 = axis_angle([1.0, 0.0, 0.0], 90.0);
        t.update(q0, 0.0);
        let q = mul(axis_angle([0.0, 0.0, 1.0], 30.0), q0);
        let mut yaw = 0.0;
        for i in 1..=1000 {
            yaw = t.update(q, i as f64 * 0.01).0; // 10 s still
        }
        assert!(yaw > 0.0 && yaw < 3.0, "after 10 s held, yaw {yaw}");
    }

    #[test]
    fn prediction_leads_a_steady_turn_and_flip_mirrors() {
        let cfg = TrackerConfig { predict_s: 0.1, recenter_tau_s: 0.0, ..Default::default() };
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
}
