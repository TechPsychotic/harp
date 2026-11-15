//! 3D audio inside the audio engine: Harp's libmpv build (the `dsp_hook` patch in Harp's
//! fork of libmpv-scripts) hands every chunk of output audio to [`process`] just before the
//! device. This is how 3D runs where there is no Harp 3D sink to play into (Android).
//!
//! The engine's chunks have any length; the renderer works in fixed blocks, so each output
//! (one per player: crossfades have two) keeps a small queue in and out. That adds one block
//! of delay (5.3 ms at 48 kHz).

use crate::head;
use crate::render::{Binaural, BLOCK};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;
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
    stream: usize,
    rate: i32,
    room: bool,
    r: Binaural,
    inq: VecDeque<f32>,
    outq: VecDeque<f32>,
    block_in: Vec<f32>,
    block_out: Vec<f32>,
    last: Instant,
}

impl Output {
    fn new(stream: usize, rate: i32, room: bool) -> Output {
        Output {
            stream,
            rate,
            room,
            r: Binaural::new_at(rate as u32, true, room),
            inq: VecDeque::with_capacity(8192),
            // One block of silence first: the output is always a block behind the input.
            outq: std::iter::repeat_n(0.0, BLOCK * 2).collect(),
            block_in: vec![0.0; BLOCK * 2],
            block_out: vec![0.0; BLOCK * 2],
            last: Instant::now(),
        }
    }
}

static OUTPUTS: Mutex<Vec<Output>> = Mutex::new(Vec::new());
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);

/// The renderer as the engine calls it, on its audio thread.
unsafe extern "C" fn process(_ud: *mut c_void, stream: *const c_void, samples: *mut f32, frames: i32, channels: i32, rate: i32) {
    if samples.is_null() || frames <= 0 || channels != 2 || !(8000..=192_000).contains(&rate) {
        return; // only stereo is rendered; anything else plays as it is
    }
    // SAFETY: the engine passes a buffer of frames * channels floats, valid for this call.
    let buf = unsafe { std::slice::from_raw_parts_mut(samples, frames as usize * 2) };
    let Ok(mut outs) = OUTPUTS.try_lock() else { return }; // never wait on the audio thread
    let now = Instant::now();
    outs.retain(|o| now.duration_since(o.last).as_secs() < 30); // players that went away
    let room = SETTINGS.room.load(Relaxed);
    let key = stream as usize;
    let i = match outs.iter().position(|o| o.stream == key) {
        Some(i) if outs[i].rate == rate && outs[i].room == room => i,
        Some(i) => {
            outs[i] = Output::new(key, rate, room);
            i
        }
        None => {
            outs.push(Output::new(key, rate, room));
            outs.len() - 1
        }
    };
    let o = &mut outs[i];
    o.last = now;
    let (mut yaw, mut pitch, mut dist) = (f32::from_bits(SETTINGS.yaw.load(Relaxed)), f32::from_bits(SETTINGS.pitch.load(Relaxed)), crate::render::SPEAKER_DIST);
    if SETTINGS.bounce.load(Relaxed) {
        let t0 = *STARTED.lock().unwrap().get_or_insert(now);
        let (az, el, d) = head::bounce(now.duration_since(t0).as_secs_f64());
        yaw -= az;
        pitch -= el;
        dist = d;
    }
    o.inq.extend(buf.iter().copied());
    while o.inq.len() >= BLOCK * 2 {
        for (k, x) in o.inq.drain(..BLOCK * 2).enumerate() {
            o.block_in[k] = x;
        }
        o.r.process_at(yaw, pitch, dist, &o.block_in, &mut o.block_out);
        o.outq.extend(o.block_out.iter().copied());
    }
    for s in buf.iter_mut() {
        *s = o.outq.pop_front().unwrap_or(0.0);
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
    SETTINGS.room.store(room, Relaxed);
    if bounce && !SETTINGS.bounce.load(Relaxed) {
        *STARTED.lock().unwrap() = None;
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

    #[test]
    fn odd_chunk_sizes_come_out_as_one_continuous_stream() {
        // Feed a tone in uneven chunks (as an audio device asks) and compare with the same tone
        // rendered in whole blocks: identical, one block later.
        SETTINGS.room.store(false, Relaxed);
        SETTINGS.bounce.store(false, Relaxed);
        let tone: Vec<f32> = (0..48000).flat_map(|t| {
            let x = 0.3 * (2.0 * std::f32::consts::PI * 440.0 * t as f32 / 48000.0).sin();
            [x, x]
        }).collect();
        let mut got = tone.clone();
        let stream = 0x1234usize as *const c_void;
        let mut at = 0;
        for (k, n) in [480usize, 17, 1024, 333, 256, 4096].iter().cycle().enumerate() {
            let n = (*n).min((got.len() - at) / 2);
            if n == 0 || k > 10_000 {
                break;
            }
            unsafe { process(std::ptr::null_mut(), stream, got[at..].as_mut_ptr(), n as i32, 2, 48000) };
            at += n * 2;
        }
        let mut reference = Binaural::new_at(48000, true, false);
        let mut want = vec![0.0f32; BLOCK * 2];
        let mut out = vec![0.0f32; BLOCK * 2];
        for b in 0..(tone.len() / (BLOCK * 2)) - 1 {
            reference.process_at(0.0, 0.0, crate::render::SPEAKER_DIST, &tone[b * BLOCK * 2..(b + 1) * BLOCK * 2], &mut out);
            want.extend_from_slice(&out);
        }
        for k in 0..want.len().min(got.len()) {
            assert!((got[k] - want[k]).abs() < 1e-5, "sample {k}: {} vs {}", got[k], want[k]);
        }
        // Mono and odd formats pass through untouched.
        let mut mono = vec![0.5f32; 100];
        unsafe { process(std::ptr::null_mut(), stream, mono.as_mut_ptr(), 100, 1, 48000) };
        assert!(mono.iter().all(|&x| x == 0.5));
    }
}
