//! Runs 3D audio on Linux: the "Harp 3D" sink, the render thread, a thread that follows the
//! default output, and (when wanted) the AirPods head tracker.

use crate::aacp::{self, Channel};
use crate::head::{self, Calibrating, Recenter, Tracker, TrackerConfig, UpCalibration};
use crate::render::{Binaural, BLOCK};
use crate::route::{self, Pulse, Stream};
use crate::{Options, Status};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Sensor interval asked of the AirPods (40 per second: plenty for audio, gentle on battery).
const MOTION_INTERVAL_US: u32 = 25_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_to_a_minute() {
        let mut d = Duration::from_secs(5);
        let mut seen = vec![];
        for _ in 0..5 {
            d = next_backoff(d);
            seen.push(d.as_secs());
        }
        assert_eq!(seen, [15, 30, 60, 60, 60]);
    }
}

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    yaw: AtomicU32,
    pitch: AtomicU32,
    tracking: AtomicBool,
    flip: AtomicBool,
    room: AtomicBool,
    bounce: AtomicBool,
    recenter: AtomicBool,
    recenter_mode: Mutex<Option<Recenter>>,
    calibrate: AtomicBool,
    calibrating: Mutex<String>,
    calibrations: Mutex<Vec<f32>>,
    latency_us: AtomicU64,
    output: Mutex<String>,
    head_state: Mutex<(String, String)>,
    error: Mutex<Option<String>>,
}

impl Shared {
    fn set_head(&self, state: &str, detail: impl Into<String>) {
        *self.head_state.lock().unwrap() = (state.to_string(), detail.into());
    }
    fn yaw(&self) -> f32 {
        f32::from_bits(self.yaw.load(Relaxed))
    }
    fn pitch(&self) -> f32 {
        f32::from_bits(self.pitch.load(Relaxed))
    }
    fn set_pose(&self, yaw: f32, pitch: f32) {
        self.yaw.store(yaw.to_bits(), Relaxed);
        self.pitch.store(pitch.to_bits(), Relaxed);
    }
    fn apply(&self, o: &Options) {
        self.tracking.store(o.head_tracking, Relaxed);
        self.flip.store(o.flip, Relaxed);
        self.room.store(o.room, Relaxed);
        self.bounce.store(o.bounce, Relaxed);
        *self.recenter_mode.lock().unwrap() = Some(Recenter::parse(&o.recenter));
        let mut c = self.calibrations.lock().unwrap();
        if !o.up_calibration.is_empty() || c.is_empty() {
            *c = o.up_calibration.clone();
        }
    }
    fn wait(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.stop.load(Relaxed) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        !self.stop.load(Relaxed)
    }
}

struct Running {
    shared: Arc<Shared>,
    module: String,
    threads: Vec<JoinHandle<()>>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// Starts 3D audio (or updates its options); returns the output device Harp's player should
/// use (`pulse/harp_spatial`).
pub fn start(opts: &Options) -> Result<String, String> {
    let mut g = RUNNING.lock().unwrap();
    if let Some(r) = g.as_ref() {
        r.shared.apply(opts);
        return Ok(format!("pulse/{}", route::sink_name()));
    }
    Pulse::load()?; // fail early and clearly if the client library is missing
    route::remove_stale_sinks();
    let output = route::real_output().ok_or("no audio output found")?;
    let module = route::create_sink()?;
    let shared = Arc::new(Shared::default());
    shared.apply(opts);
    *shared.output.lock().unwrap() = output;
    shared.set_head("off", "");
    let threads = vec![
        spawn("harp-3d-audio", &shared, audio_thread),
        spawn("harp-3d-output", &shared, output_thread),
        spawn("harp-3d-head", &shared, head_thread),
    ];
    *g = Some(Running { shared, module, threads });
    Ok(format!("pulse/{}", route::sink_name()))
}

/// Starts the up/down calibration on the next motion sample.
pub fn calibrate_up() {
    if let Some(r) = RUNNING.lock().unwrap().as_ref() {
        r.shared.calibrate.store(true, Relaxed);
        *r.shared.calibrating.lock().unwrap() = "waiting".into();
    }
}

/// Stops 3D audio and removes the sink. Harp's player should go back to its usual output first.
pub fn stop() {
    let r = RUNNING.lock().unwrap().take();
    if r.is_none() {
        route::remove_stale_sinks(); // one a crash or a kill left behind
    }
    if let Some(r) = r {
        r.shared.stop.store(true, Relaxed);
        route::remove_sink(&r.module);
        for t in r.threads {
            let _ = t.join();
        }
    }
}

pub fn recenter() {
    if let Some(r) = RUNNING.lock().unwrap().as_ref() {
        r.shared.recenter.store(true, Relaxed);
    }
}

pub fn status() -> Status {
    let g = RUNNING.lock().unwrap();
    let Some(r) = g.as_ref() else { return Status::default() };
    let s = &r.shared;
    let (head, head_detail) = s.head_state.lock().unwrap().clone();
    let output = s.output.lock().unwrap().clone();
    let error = s.error.lock().unwrap().clone();
    let calibrating = s.calibrating.lock().unwrap().clone();
    let up_calibration = s.calibrations.lock().unwrap().clone();
    Status {
        running: true,
        output,
        head,
        head_detail,
        yaw: s.yaw(),
        pitch: s.pitch(),
        calibrating,
        up_calibration,
        latency_ms: (s.latency_us.load(Relaxed) / 1000) as u32,
        error,
        engine: String::new(),
    }
}

fn spawn(name: &str, shared: &Arc<Shared>, f: fn(Arc<Shared>)) -> JoinHandle<()> {
    let s = shared.clone();
    std::thread::Builder::new().name(name.into()).spawn(move || f(s)).expect("thread")
}

fn audio_thread(s: Arc<Shared>) {
    let Ok(pa) = Pulse::load() else { return };
    let mut room = s.room.load(Relaxed);
    let mut r = Binaural::new(true, room);
    let mut input = vec![0.0f32; BLOCK * 2];
    let mut out = vec![0.0f32; BLOCK * 2];
    let began = Instant::now();
    while !s.stop.load(Relaxed) {
        let target = s.output.lock().unwrap().clone();
        let opened = route::FifoReader::open().and_then(|rec| Ok((rec, Stream::playback(&pa, &target, BLOCK as u32)?)));
        let (mut rec, mut play) = match opened {
            Ok(p) => p,
            Err(e) => {
                *s.error.lock().unwrap() = Some(e);
                if !s.wait(Duration::from_millis(700)) {
                    break;
                }
                continue;
            }
        };
        *s.error.lock().unwrap() = None;
        let mut n = 0u64;
        while !s.stop.load(Relaxed) {
            match rec.read(&mut input, Duration::from_millis(100)) {
                Ok(true) => {}
                // Nothing playing: nothing to render (the output stays quiet).
                Ok(false) => continue,
                Err(e) => {
                    *s.error.lock().unwrap() = Some(e);
                    break;
                }
            }
            let want_room = s.room.load(Relaxed);
            if want_room != room {
                room = want_room;
                r = Binaural::new(true, room);
            }
            let (mut yaw, mut pitch) = if s.tracking.load(Relaxed) { (s.yaw(), s.pitch()) } else { (0.0, 0.0) };
            let mut dist = crate::render::SPEAKER_DIST;
            if s.bounce.load(Relaxed) {
                // The stage moves around you: the same as the head turning the other way.
                let (az, el, d) = head::bounce(began.elapsed().as_secs_f64());
                yaw -= az;
                pitch -= el;
                dist = d;
            }
            r.process_at(yaw, pitch, dist, &input, &mut out);
            if let Err(e) = play.write(&out) {
                *s.error.lock().unwrap() = Some(e);
                break;
            }
            n += 1;
            if n % 94 == 0 {
                // about twice a second
                s.latency_us.store(play.latency_us(), Relaxed);
                if *s.output.lock().unwrap() != target {
                    break; // the default output changed: reopen on the new one
                }
            }
        }
    }
}

fn output_thread(s: Arc<Shared>) {
    while s.wait(Duration::from_secs(2)) {
        if let Some(o) = route::real_output() {
            let mut cur = s.output.lock().unwrap();
            if *cur != o {
                *cur = o;
            }
        }
    }
}

/// Waits between attempts to open the AirPods' channel when they don't give motion data:
/// 5 s, then 15, 30, then every 60 s. Every attempt is a new session with the buds, and after
/// many sessions in a row they stop offering their sensors (seen live), so retrying fast would
/// cause the very problem it waits out.
fn next_backoff(cur: Duration) -> Duration {
    match cur.as_secs() {
        0..=5 => Duration::from_secs(15),
        6..=15 => Duration::from_secs(30),
        _ => Duration::from_secs(60),
    }
}

fn head_thread(s: Arc<Shared>) {
    let mut buf = vec![0u8; 4096];
    let mut backoff = Duration::from_secs(5);
    while !s.stop.load(Relaxed) {
        if !s.tracking.load(Relaxed) {
            backoff = Duration::from_secs(5);
            s.set_pose(0.0, 0.0);
            s.set_head("off", "");
            if !s.wait(Duration::from_millis(300)) {
                break;
            }
            continue;
        }
        s.set_head("searching", "Looking for connected AirPods");
        let devices = aacp::connected_apple_devices();
        let Some(dev) = devices.iter().find(|d| d.head_tracking().is_ok()).cloned() else {
            let detail = devices.first().and_then(|d| d.head_tracking().err()).unwrap_or_else(|| "No AirPods connected".into());
            s.set_head(if devices.is_empty() { "searching" } else { "unsupported" }, detail);
            if !s.wait(Duration::from_secs(3)) {
                break;
            }
            continue;
        };
        s.set_head("connecting", dev.name.clone());
        let mut ch = match Channel::open(&dev.address) {
            Ok(c) => c,
            Err(e) => {
                s.set_head("error", format!("Couldn't open the AirPods' control channel: {e}"));
                let wait = backoff;
                backoff = next_backoff(backoff);
                if !s.wait(wait) {
                    break;
                }
                continue;
            }
        };
        if let Err(e) = ch.start_motion(MOTION_INTERVAL_US) {
            s.set_head("error", e.to_string());
            continue;
        }
        let mut tracker = Tracker::new(TrackerConfig::default());
        tracker.calibrations = UpCalibration::decode(&s.calibrations.lock().unwrap());
        let started = Instant::now();
        let mut last_sample: Option<Instant> = None;
        let mut out_of_ears = false;
        loop {
            if s.stop.load(Relaxed) || !s.tracking.load(Relaxed) {
                break;
            }
            tracker.cfg.flip = s.flip.load(Relaxed);
            if let Some(m) = *s.recenter_mode.lock().unwrap() {
                tracker.cfg.recenter = m;
            }
            if s.recenter.swap(false, Relaxed) {
                tracker.recenter();
            }
            if s.calibrate.swap(false, Relaxed) {
                tracker.calibrate_up();
            }
            match ch.recv(&mut buf, Duration::from_millis(200)) {
                Ok(Some(n)) => {
                    let pkt = &buf[..n];
                    if let Some((l, r)) = aacp::ear_state(pkt) {
                        out_of_ears = l != 0 && r != 0;
                        if out_of_ears {
                            s.set_pose(0.0, 0.0);
                            s.set_head("paused", "AirPods out of your ears");
                        }
                    }
                    for m in aacp::motion_samples(pkt) {
                        last_sample = Some(Instant::now());
                        backoff = Duration::from_secs(5);
                        if !out_of_ears {
                            let was = tracker.calibrating.clone();
                            let (yaw, pitch) = tracker.update(m.q, m.t);
                            s.set_pose(yaw, pitch);
                            s.set_head("tracking", dev.name.clone());
                            if tracker.calibrating != was {
                                *s.calibrating.lock().unwrap() = match &tracker.calibrating {
                                    Calibrating::Idle => String::new(),
                                    Calibrating::Running(left) => format!("running:{:.0}", left.ceil()),
                                    Calibrating::Done => {
                                        *s.calibrations.lock().unwrap() = UpCalibration::encode(&tracker.calibrations);
                                        "done".into()
                                    }
                                    Calibrating::Failed(why) => format!("failed:{why}"),
                                };
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    s.set_head("searching", format!("{e}"));
                    break;
                }
            }
            let quiet = last_sample.map_or(started.elapsed(), |t| t.elapsed());
            if quiet > Duration::from_secs(4) && !out_of_ears {
                // Connected but no data: another app (Lyre, LibrePods, mousepods…) holds the
                // AirPods' control channel, which only one app can use at a time; or the buds
                // stopped offering their sensors after many sessions (seen live; a Bluetooth
                // reconnect brings them back).
                s.set_head("in_use", "No motion data from the AirPods. Another app may be using their controls (close it), or the buds need reconnecting: turn Bluetooth off and on.");
                s.set_pose(0.0, 0.0);
                break;
            }
        }
        ch.stop_motion();
        drop(ch);
        s.set_pose(0.0, 0.0);
        let wait = if last_sample.is_some() { Duration::from_secs(3) } else { backoff };
        if last_sample.is_none() {
            backoff = next_backoff(backoff);
        }
        if !s.stop.load(Relaxed) && s.tracking.load(Relaxed) && !s.wait(wait) {
            break;
        }
    }
}
