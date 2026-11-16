//! 3D audio inside the audio engine: Harp's libmpv build (the `dsp_hook` patch in Harp's
//! fork of libmpv-scripts) hands every chunk of output audio to [`process`] just before the
//! device. This is how 3D runs where there is no Harp 3D sink to play into (Android).
//!
//! The engine's chunks have any length; the renderer works in fixed blocks, so each output
//! (one per player: crossfades have two) keeps a small queue in and out. That adds one block
//! of delay (5.3 ms at 48 kHz).

use crate::bounce::{BeatDetector, Bounce};
use crate::render::{Binaural, BLOCK};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Instant;

type DspFn = unsafe extern "C" fn(*mut c_void, *const c_void, *mut f32, i32, i32, i32);
type SetDsp = unsafe extern "C" fn(Option<DspFn>, *mut c_void);

/// What the audio thread reads (set from the app).
struct Settings {
    room: AtomicBool,
    bounce: AtomicBool,
    yaw: AtomicU32,
    pitch: AtomicU32,
}

static SETTINGS: Settings = Settings { room: AtomicBool::new(true), bounce: AtomicBool::new(false), yaw: AtomicU32::new(0), pitch: AtomicU32::new(0) };

struct Output {
    rate: i32,
    room: bool,
    /// Built off the audio thread; until then the audio plays as it is.
    r: Option<Binaural>,
    inq: VecDeque<f32>,
    outq: VecDeque<f32>,
    block_in: Vec<f32>,
    block_out: Vec<f32>,
}

impl Output {
    fn new(rate: i32, room: bool) -> Output {
        Output {
            rate,
            room,
            r: None,
            inq: VecDeque::with_capacity(8192),
            outq: VecDeque::with_capacity(8192),
            block_in: vec![0.0; BLOCK * 2],
            block_out: vec![0.0; BLOCK * 2],
        }
    }

    /// The renderer is ready: start its queues (one block of silence first: the output is
    /// always a block behind the input).
    fn install(&mut self, r: Binaural) {
        self.r = Some(r);
        self.inq.clear();
        self.outq.clear();
        self.outq.extend(std::iter::repeat_n(0.0, BLOCK * 2));
    }
}

type Slot = Arc<Mutex<Output>>;

/// An output's key and when it was last heard, kept outside its lock so finding a slot never
/// waits on another player's processing.
struct Entry {
    stream: usize,
    last_ms: AtomicU64,
    slot: Slot,
}

/// One slot per engine output (per player). The list is locked only to find or add a slot;
/// each output's own lock is only taken by its audio thread (and once by the thread that
/// builds its renderer), so two players never wait on each other.
static OUTPUTS: Mutex<Vec<Arc<Entry>>> = Mutex::new(Vec::new());
/// Bounce is one motion for every output (two players in a crossfade move together), advanced
/// by the clock, with beats from whichever output is playing.
static MOTION: Mutex<Option<(Bounce, BeatDetector, Instant)>> = Mutex::new(None);
static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Diagnostics for the settings screen.
static CHUNKS: AtomicU64 = AtomicU64::new(0);
static LAST_RATE: AtomicU32 = AtomicU32::new(0);
static LAST_CHANNELS: AtomicU32 = AtomicU32::new(0);

fn slot_for(stream: usize, rate: i32, room: bool) -> Slot {
    let now = now_ms();
    let mut outs = OUTPUTS.lock().unwrap();
    // Players that went away (nothing for 30 s).
    outs.retain(|e| now.saturating_sub(e.last_ms.load(Relaxed)) < 30_000);
    if let Some(e) = outs.iter().find(|e| e.stream == stream) {
        e.last_ms.store(now, Relaxed);
        return e.slot.clone();
    }
    let slot: Slot = Arc::new(Mutex::new(Output::new(rate, room)));
    outs.push(Arc::new(Entry { stream, last_ms: AtomicU64::new(now), slot: slot.clone() }));
    build_later(&slot, rate, room);
    slot
}

/// Builds a renderer on its own thread (it can take a moment: a new sample rate resamples the
/// whole table) and hands it to the output.
fn build_later(slot: &Slot, rate: i32, room: bool) {
    let slot = slot.clone();
    let _ = std::thread::Builder::new().name("harp-3d-build".into()).spawn(move || {
        let r = Binaural::new_at(rate as u32, true, room);
        if let Ok(mut o) = slot.lock() {
            if o.rate == rate && o.room == room {
                o.install(r);
            }
        }
    });
}

/// The renderer as the engine calls it, on its audio thread.
unsafe extern "C" fn process(_ud: *mut c_void, stream: *const c_void, samples: *mut f32, frames: i32, channels: i32, rate: i32) {
    LAST_RATE.store(rate.max(0) as u32, Relaxed);
    LAST_CHANNELS.store(channels.max(0) as u32, Relaxed);
    if samples.is_null() || frames <= 0 || channels != 2 || !(8000..=192_000).contains(&rate) {
        return; // only stereo is rendered; anything else plays as it is
    }
    // SAFETY: the engine passes a buffer of frames * channels floats, valid for this call.
    let buf = unsafe { std::slice::from_raw_parts_mut(samples, frames as usize * 2) };
    let room = SETTINGS.room.load(Relaxed);
    let slot = slot_for(stream as usize, rate, room);
    let Ok(mut o) = slot.lock() else { return };
    let now = Instant::now();
    if o.rate != rate || o.room != room {
        // The output changed rate (another song) or the room was switched: rebuild aside.
        o.rate = rate;
        o.room = room;
        o.r = None;
        build_later(&slot, rate, room);
    }
    if o.r.is_none() {
        return; // the renderer is still being built: plays as it is for a moment
    }
    let (mut yaw, mut pitch, mut dist) = (f32::from_bits(SETTINGS.yaw.load(Relaxed)), f32::from_bits(SETTINGS.pitch.load(Relaxed)), crate::render::SPEAKER_DIST);
    if SETTINGS.bounce.load(Relaxed) {
        let mut m = MOTION.lock().unwrap();
        let (motion, beats, last) = m.get_or_insert_with(|| {
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
            (Bounce::new(seed), BeatDetector::new(rate as f32), now)
        });
        let beat = beats.feed(buf);
        let dt = now.duration_since(*last).as_secs_f32();
        *last = now;
        let (az, el, d) = motion.step(dt, beat);
        yaw -= az;
        pitch -= el;
        dist = d;
    }
    let o = &mut *o;
    o.inq.extend(buf.iter().copied());
    while o.inq.len() >= BLOCK * 2 {
        for (k, x) in o.inq.drain(..BLOCK * 2).enumerate() {
            o.block_in[k] = x;
        }
        if let Some(r) = o.r.as_mut() {
            r.process_at(yaw, pitch, dist, &o.block_in, &mut o.block_out);
        }
        o.outq.extend(o.block_out.iter().copied());
    }
    for s in buf.iter_mut() {
        *s = o.outq.pop_front().unwrap_or(0.0);
    }
    CHUNKS.fetch_add(1, Relaxed);
}

/// What the engine side is doing, for the settings screen: whether the hook was found, how
/// many chunks were rendered, and the output's rate and channels.
pub fn diagnostics() -> String {
    if set_dsp().is_none() {
        return "Audio engine: no 3D hook found".into();
    }
    let rate = LAST_RATE.load(Relaxed);
    let ch = LAST_CHANNELS.load(Relaxed);
    let n = CHUNKS.load(Relaxed);
    if rate == 0 {
        "Audio engine: 3D hook found, no audio through it yet".into()
    } else {
        format!("Audio engine: 3D hook found · {n} chunks rendered · output {rate} Hz, {ch} ch")
    }
}

/// The engine's `mak_pcm_set_dsp`, if this build has the hook.
fn set_dsp() -> Option<SetDsp> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        for name in [c"libmpv_audio_kit.so", c"libmpv.so", c"libmpv.so.2"] {
            // SAFETY: looks up an already loaded library (RTLD_NOLOAD) and one symbol.
            unsafe {
                let h = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_NOLOAD);
                if h.is_null() {
                    continue;
                }
                let f = libc::dlsym(h, c"mak_pcm_set_dsp".as_ptr());
                if !f.is_null() {
                    return Some(std::mem::transmute::<*mut c_void, SetDsp>(f));
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        None
    }
}

/// Whether the loaded engine can run 3D audio inside it.
pub fn available() -> bool {
    set_dsp().is_some()
}

/// Turns 3D on in the engine (or updates its options).
pub fn enable(room: bool, bounce: bool) -> Result<(), String> {
    let set = set_dsp().ok_or("this build of the audio engine has no 3D hook")?;
    // The tables for the usual output rates, ready before any audio asks for them.
    crate::hrir::HrirSet::builtin_at(48000);
    crate::hrir::HrirSet::builtin_at(44100);
    SETTINGS.room.store(room, Relaxed);
    if bounce && !SETTINGS.bounce.load(Relaxed) {
        *MOTION.lock().unwrap() = None; // a new Bounce each time it's turned on
    }
    SETTINGS.bounce.store(bounce, Relaxed);
    // SAFETY: `process` lives for the whole program; no user data is used.
    unsafe { set(Some(process), std::ptr::null_mut()) };
    Ok(())
}

/// Turns 3D off in the engine (audio plays as it is again).
pub fn disable() {
    if let Some(set) = set_dsp() {
        // SAFETY: removing the function; the engine stops calling it.
        unsafe { set(None, std::ptr::null_mut()) };
    }
    if let Ok(mut o) = OUTPUTS.lock() {
        o.clear();
    }
}

/// The head pose to render with (from head tracking, where there is some).
pub fn set_pose(yaw: f32, pitch: f32) {
    SETTINGS.yaw.store(yaw.to_bits(), Relaxed);
    SETTINGS.pitch.store(pitch.to_bits(), Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a 440 Hz tone (offset per stream) through `process` for `stream`, in uneven chunks,
    /// once its renderer is ready; returns what came out and the reference rendering.
    fn run_stream(key: usize, seconds: usize) -> (Vec<f32>, Vec<f32>) {
        let stream = key as *const c_void;
        // The first chunk starts building the renderer (it plays as it is); then wait for it
        // without feeding more, so the stream below starts exactly at the installed queue.
        let mut z = vec![0.0f32; 512];
        unsafe { process(std::ptr::null_mut(), stream, z.as_mut_ptr(), 256, 2, 48000) };
        while !OUTPUTS.lock().unwrap().iter().find(|e| e.stream == key).map(|e| e.slot.lock().unwrap().r.is_some()).unwrap_or(false) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let f = 220.0 + key as f32;
        let tone: Vec<f32> = (0..48000 * seconds).flat_map(|t| {
            let x = 0.3 * (2.0 * std::f32::consts::PI * f * t as f32 / 48000.0).sin();
            [x, x]
        }).collect();
        let mut got = tone.clone();
        let mut at = 0;
        for n in [480usize, 17, 1024, 333, 256, 4096].iter().cycle() {
            let n = (*n).min((got.len() - at) / 2);
            if n == 0 {
                break;
            }
            unsafe { process(std::ptr::null_mut(), stream, got[at..].as_mut_ptr(), n as i32, 2, 48000) };
            at += n * 2;
        }
        let mut reference = Binaural::new_at(48000, true, false);
        let mut want = vec![0.0f32; BLOCK * 2]; // the queue's first block of silence
        let mut out = vec![0.0f32; BLOCK * 2];
        for b in 0..(tone.len() / (BLOCK * 2)) - 1 {
            reference.process_at(0.0, 0.0, crate::render::SPEAKER_DIST, &tone[b * BLOCK * 2..(b + 1) * BLOCK * 2], &mut out);
            want.extend_from_slice(&out);
        }
        (got, want)
    }

    #[test]
    fn two_players_at_once_are_both_fully_rendered() {
        // A crossfade: two outputs on two threads. Every chunk of both must be rendered (a
        // chunk left as it is would click), and each stream must match its own reference.
        let _g = crate::TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        SETTINGS.room.store(false, Relaxed);
        SETTINGS.bounce.store(false, Relaxed);
        set_pose(0.0, 0.0);
        let a = std::thread::spawn(|| run_stream(0xA0, 2));
        let b = std::thread::spawn(|| run_stream(0xB0, 2));
        for (got, want) in [a.join().unwrap(), b.join().unwrap()] {
            let n = want.len().min(got.len());
            assert!(n > 48000 * 3);
            for k in 0..n {
                assert!((got[k] - want[k]).abs() < 1e-5, "sample {k}: {} vs {}", got[k], want[k]);
            }
        }
        // Mono and odd formats pass through untouched.
        let mut mono = vec![0.5f32; 100];
        unsafe { process(std::ptr::null_mut(), 0x1234usize as *const c_void, mono.as_mut_ptr(), 100, 1, 48000) };
        assert!(mono.iter().all(|&x| x == 0.5));
    }
}
