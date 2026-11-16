//! Head tracking where the app itself holds the AirPods connection (Android: Harp's Kotlin side
//! opens it, the way Lyre does): the app sends the packets [`start_packets`] gives, passes every
//! packet the AirPods send to [`feed`], and the 3D engine follows the head.

use crate::head::{Calibrating, Recenter, Tracker, TrackerConfig, UpCalibration};
use std::sync::Mutex;
use std::time::Instant;

/// One packet to send, and how long to wait after it.
pub struct Packet {
    pub data: Vec<u8>,
    pub wait_ms: u32,
}

struct State {
    tracker: Tracker,
    link: (String, String),
    last_sample: Option<Instant>,
    out_of_ears: bool,
    yaw: f32,
    pitch: f32,
    /// Since the link last connected: packets from the AirPods, and those with sensor data.
    packets: u32,
    sensor_packets: u32,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Motion samples asked of the AirPods: 40 a second.
const MOTION_INTERVAL_US: u32 = 25_000;

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap();
    let s = g.get_or_insert_with(|| State {
        tracker: Tracker::new(TrackerConfig::default()),
        link: ("off".into(), String::new()),
        last_sample: None,
        out_of_ears: false,
        yaw: 0.0,
        pitch: 0.0,
        packets: 0,
        sensor_packets: 0,
    });
    f(s)
}

/// Options from the settings (flip, recentring, saved up/down calibration).
pub fn configure(flip: bool, recenter: &str, up_calibration: &[f32]) {
    with(|s| {
        s.tracker.cfg.flip = flip;
        s.tracker.cfg.recenter = Recenter::parse(recenter);
        if !up_calibration.is_empty() || s.tracker.calibrations.is_empty() {
            s.tracker.calibrations = UpCalibration::decode(up_calibration);
        }
    });
}

/// The handshake and the requests that start the motion stream, with the pauses between them.
pub fn start_packets() -> Vec<Packet> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use crate::aacp::*;
        vec![
            Packet { data: HANDSHAKE.to_vec(), wait_ms: 200 },
            Packet { data: FEATURES.to_vec(), wait_ms: 120 },
            Packet { data: NOTIFICATIONS.to_vec(), wait_ms: 120 },
            Packet { data: stream_packet(0x41, SVC_ACTIVITY, 40_000), wait_ms: 250 },
            Packet { data: stream_packet(0x42, SVC_MOTION, MOTION_INTERVAL_US), wait_ms: 0 },
            Packet { data: stream_packet(0x43, SVC_ACTIVITY, 0), wait_ms: 0 },
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        vec![]
    }
}

/// The request that stops the motion stream (send it before closing).
pub fn stop_packets() -> Vec<Vec<u8>> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        vec![crate::aacp::stream_packet(0x44, crate::aacp::SVC_MOTION, 0)]
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        vec![]
    }
}

/// What the app's connection is doing ("searching", "connecting", "error", "off"…), for the
/// settings screen; motion data then makes it "tracking".
pub fn set_link(state: &str, detail: &str) {
    with(|s| {
        s.link = (state.to_string(), detail.to_string());
        if state == "connected" {
            s.packets = 0;
            s.sensor_packets = 0;
        }
        if state != "tracking" {
            s.last_sample = None;
            s.yaw = 0.0;
            s.pitch = 0.0;
        }
    });
    crate::dsp::set_pose(0.0, 0.0);
}

/// One packet from the AirPods.
pub fn feed(pkt: &[u8]) {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let (yaw, pitch) = with(|s| {
            s.packets += 1;
            if pkt.len() > 4 && pkt[4] == 0x17 {
                s.sensor_packets += 1;
            }
            if let Some((l, r)) = crate::aacp::ear_state(pkt) {
                s.out_of_ears = l != 0 && r != 0;
            }
            for m in crate::aacp::motion_samples(pkt) {
                s.last_sample = Some(Instant::now());
                if !s.out_of_ears {
                    (s.yaw, s.pitch) = s.tracker.update(m.q, m.t);
                }
            }
            if s.out_of_ears {
                (s.yaw, s.pitch) = (0.0, 0.0);
            }
            (s.yaw, s.pitch)
        });
        crate::dsp::set_pose(yaw, pitch);
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = pkt;
}

pub fn recenter() {
    with(|s| s.tracker.recenter());
}

pub fn calibrate_up() {
    with(|s| s.tracker.calibrate_up());
}

/// (head state, detail, yaw, pitch, calibrating, saved calibrations) for the status.
pub fn status() -> (String, String, f32, f32, String, Vec<f32>) {
    with(|s| {
        let fresh = s.last_sample.map(|t| t.elapsed().as_secs_f32() < 2.0).unwrap_or(false);
        let (head, detail) = if s.out_of_ears && fresh {
            ("paused".to_string(), "AirPods out of your ears".to_string())
        } else if fresh {
            ("tracking".to_string(), s.link.1.clone())
        } else if s.link.0 == "connected" {
            ("connecting".to_string(), format!("Waiting for motion data · {} packets from the AirPods, {} with sensor data", s.packets, s.sensor_packets))
        } else {
            s.link.clone()
        };
        let calibrating = match &s.tracker.calibrating {
            Calibrating::Idle => String::new(),
            Calibrating::Running(left) => format!("running:{:.0}", left.ceil()),
            Calibrating::Done => "done".into(),
            Calibrating::Failed(why) => format!("failed:{why}"),
        };
        (head, detail, s.yaw, s.pitch, calibrating, UpCalibration::encode(&s.tracker.calibrations))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_packets_turn_the_head() {
        let _g = crate::TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        set_link("connected", "AirPods Pro");
        configure(false, "off", &[]);
        with(|s| s.tracker.cfg.predict_s = 0.0);
        // Two motion blocks: the second turned 30° left about the vertical.
        let block = |deg: f32, ts: u64| {
            let h = deg.to_radians() / 2.0;
            let mut b = [0u8; 58];
            b[0] = 1;
            b[1..9].copy_from_slice(&ts.to_le_bytes());
            // a bud lying on its side (x up), turned about world Z
            let q = crate::head::qmul([h.cos(), 0.0, 0.0, h.sin()], [0.70710677, 0.0, 0.70710677, 0.0]);
            for (k, c) in q[1..].iter().enumerate() {
                b[20 + 2 * k..22 + 2 * k].copy_from_slice(&((c * 32767.0) as i16).to_le_bytes());
            }
            let mut inner = vec![0x08, crate::aacp::SVC_MOTION, 0x1A, 58];
            inner.extend_from_slice(&b);
            let mut body = vec![0x3A, inner.len() as u8];
            body.extend_from_slice(&inner);
            let mut p = crate::aacp::WX_PREFIX.to_vec();
            p.extend_from_slice(&(body.len() as u16).to_le_bytes());
            p.extend_from_slice(&body);
            p
        };
        feed(&block(0.0, 1_000_000_000));
        feed(&block(30.0, 1_025_000_000));
        let (head, _, yaw, _, _, _) = status();
        assert_eq!(head, "tracking");
        assert!((yaw - 30.0).abs() < 2.0, "yaw {yaw}");
        assert_eq!(start_packets().len(), 6);
        assert_eq!(stop_packets().len(), 1);
        set_link("off", "");
    }
}
