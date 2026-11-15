//! Runs 3D audio on Linux: the "Harp 3D" sink, the render thread, a thread that follows the
//! default output, and (when wanted) the AirPods head tracker.

use crate::aacp::{self, Channel};
use crate::head::{Tracker, TrackerConfig};
use crate::render::{Binaural, BLOCK};
use crate::route::{self, Pulse, Stream};
use crate::Status;
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
    tracking: AtomicBool,
    flip: AtomicBool,
    recenter: AtomicBool,
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
    fn set_yaw(&self, y: f32) {
        self.yaw.store(y.to_bits(), Relaxed);
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
pub fn start(head_tracking: bool, flip: bool) -> Result<String, String> {
    let mut g = RUNNING.lock().unwrap();
    if let Some(r) = g.as_ref() {
        r.shared.tracking.store(head_tracking, Relaxed);
        r.shared.flip.store(flip, Relaxed);
        return Ok(format!("pulse/{}", route::SINK_NAME));
    }
    Pulse::load()?; // fail early and clearly if the client library is missing
    route::remove_stale_sinks();
    let output = route::real_output().ok_or("no audio output found")?;
    let module = route::create_sink()?;
    let shared = Arc::new(Shared::default());
    shared.tracking.store(head_tracking, Relaxed);
    shared.flip.store(flip, Relaxed);
    *shared.output.lock().unwrap() = output;
    shared.set_head("off", "");
    let threads = vec![
        spawn("harp-3d-audio", &shared, audio_thread),
        spawn("harp-3d-output", &shared, output_thread),
        spawn("harp-3d-head", &shared, head_thread),
    ];
    *g = Some(Running { shared, module, threads });
    Ok(format!("pulse/{}", route::SINK_NAME))
}

/// Stops 3D audio and removes the sink. Harp's player should go back to its usual output first.
pub fn stop() {
    let r = RUNNING.lock().unwrap().take();
    if r.is_none() {
        route::remove_stale_sinks(); // one a crash or a kill left behind
    }
    if let Some(r) = r {
        r.shared.stop.store(true, Relaxed);
        route::remove_sink(&r.module); // ends a blocked read
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
    Status {
        running: true,
        output,
        head,
        head_detail,
        yaw: s.yaw(),
        latency_ms: (s.latency_us.load(Relaxed) / 1000) as u32,
        error,
    }
}

fn spawn(name: &str, shared: &Arc<Shared>, f: fn(Arc<Shared>)) -> JoinHandle<()> {
    let s = shared.clone();
    std::thread::Builder::new().name(name.into()).spawn(move || f(s)).expect("thread")
}

fn audio_thread(s: Arc<Shared>) {
    let Ok(pa) = Pulse::load() else { return };
    let monitor = format!("{}.monitor", route::SINK_NAME);
    let mut r = Binaural::new(true);
    let mut input = vec![0.0f32; BLOCK * 2];
    let mut out = vec![0.0f32; BLOCK * 2];
    while !s.stop.load(Relaxed) {
        let target = s.output.lock().unwrap().clone();
        let opened = Stream::record(&pa, &monitor, BLOCK as u32).and_then(|rec| Ok((rec, Stream::playback(&pa, &target, BLOCK as u32)?)));
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
            if let Err(e) = rec.read(&mut input) {
                *s.error.lock().unwrap() = Some(e);
                break;
            }
            let yaw = if s.tracking.load(Relaxed) { s.yaw() } else { 0.0 };
            r.process(yaw, 0.0, &input, &mut out);
            if let Err(e) = play.write(&out) {
                *s.error.lock().unwrap() = Some(e);
                break;
            }
            n += 1;
            if n % 94 == 0 {
                // about twice a second
                s.latency_us.store(rec.latency_us() + play.latency_us(), Relaxed);
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
            s.set_yaw(0.0);
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
        let started = Instant::now();
        let mut last_sample: Option<Instant> = None;
        let mut out_of_ears = false;
        loop {
            if s.stop.load(Relaxed) || !s.tracking.load(Relaxed) {
                break;
            }
            tracker.cfg.flip = s.flip.load(Relaxed);
            if s.recenter.swap(false, Relaxed) {
                tracker.recenter();
            }
            match ch.recv(&mut buf, Duration::from_millis(200)) {
                Ok(Some(n)) => {
                    let pkt = &buf[..n];
                    if let Some((l, r)) = aacp::ear_state(pkt) {
                        out_of_ears = l != 0 && r != 0;
                        if out_of_ears {
                            s.set_yaw(0.0);
                            s.set_head("paused", "AirPods out of your ears");
                        }
                    }
                    for m in aacp::motion_samples(pkt) {
                        last_sample = Some(Instant::now());
                        backoff = Duration::from_secs(5);
                        if !out_of_ears {
                            let (yaw, _) = tracker.update(m.q, m.t);
                            s.set_yaw(yaw);
                            s.set_head("tracking", dev.name.clone());
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
                s.set_yaw(0.0);
                break;
            }
        }
        ch.stop_motion();
        drop(ch);
        s.set_yaw(0.0);
        let wait = if last_sample.is_some() { Duration::from_secs(3) } else { backoff };
        if last_sample.is_none() {
            backoff = next_backoff(backoff);
        }
        if !s.stop.load(Relaxed) && s.tracking.load(Relaxed) && !s.wait(wait) {
            break;
        }
    }
}
