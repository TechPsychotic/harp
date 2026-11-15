//! The audio route on Linux: Harp plays into its own virtual output ("Harp 3D", a null
//! sink), this module records that sink's monitor, renders it binaurally and plays the result
//! on the real output.
//!
//! It speaks to PipeWire or PulseAudio through the PulseAudio "simple" API, loaded at run
//! time (`dlopen`), so Harp still starts on systems without it. The sink is created and
//! removed with `pactl`.

use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CString};
use std::process::Command;

pub const SINK_NAME: &str = "harp_spatial";

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_STREAM_RECORD: c_int = 2;
const PA_SAMPLE_FLOAT32LE: c_int = 5;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

type NewFn = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    c_int,
    *const c_char,
    *const c_char,
    *const SampleSpec,
    *const c_void,
    *const BufferAttr,
    *mut c_int,
) -> *mut c_void;
type IoFn = unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *mut c_int) -> c_int;
type WriteFn = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type LatencyFn = unsafe extern "C" fn(*mut c_void, *mut c_int) -> u64;

/// The loaded library and its functions.
pub struct Pulse {
    _lib: Library,
    new: NewFn,
    read: IoFn,
    write: WriteFn,
    free: FreeFn,
    latency: LatencyFn,
}

impl Pulse {
    pub fn load() -> Result<Pulse, String> {
        // SAFETY: loading a system library and looking up functions with their documented
        // C signatures.
        unsafe {
            let lib = Library::new("libpulse-simple.so.0").map_err(|e| format!("PulseAudio/PipeWire client library not found ({e})"))?;
            let new = *lib.get::<NewFn>(b"pa_simple_new\0").map_err(|e| e.to_string())?;
            let read = *lib.get::<IoFn>(b"pa_simple_read\0").map_err(|e| e.to_string())?;
            let write = *lib.get::<WriteFn>(b"pa_simple_write\0").map_err(|e| e.to_string())?;
            let free = *lib.get::<FreeFn>(b"pa_simple_free\0").map_err(|e| e.to_string())?;
            let latency = *lib.get::<LatencyFn>(b"pa_simple_get_latency\0").map_err(|e| e.to_string())?;
            Ok(Pulse { _lib: lib, new, read, write, free, latency })
        }
    }
}

/// One recording or playback stream (48 kHz float stereo).
pub struct Stream<'a> {
    pa: &'a Pulse,
    s: *mut c_void,
}

impl<'a> Stream<'a> {
    fn open(pa: &'a Pulse, record: bool, device: &str, name: &str, frames: u32) -> Result<Stream<'a>, String> {
        let ss = SampleSpec { format: PA_SAMPLE_FLOAT32LE, rate: 48000, channels: 2 };
        let bytes = frames * 8;
        // Small buffers on purpose: every millisecond here is a millisecond of head-tracking lag.
        let attr = if record {
            BufferAttr { maxlength: bytes * 8, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: bytes }
        } else {
            BufferAttr { maxlength: bytes * 16, tlength: bytes * 6, prebuf: bytes * 2, minreq: bytes, fragsize: u32::MAX }
        };
        let app = CString::new("Harp").unwrap();
        let dev = CString::new(device).map_err(|e| e.to_string())?;
        let sname = CString::new(name).unwrap();
        let mut err: c_int = 0;
        let dir = if record { PA_STREAM_RECORD } else { PA_STREAM_PLAYBACK };
        // SAFETY: all pointers are valid for the call; the returned handle is owned here.
        let s = unsafe { (pa.new)(std::ptr::null(), app.as_ptr(), dir, dev.as_ptr(), sname.as_ptr(), &ss, std::ptr::null(), &attr, &mut err) };
        if s.is_null() {
            return Err(format!("couldn't open {} stream on {device} (error {err})", if record { "a recording" } else { "a playback" }));
        }
        Ok(Stream { pa, s })
    }

    pub fn record(pa: &'a Pulse, device: &str, frames: u32) -> Result<Stream<'a>, String> {
        Stream::open(pa, true, device, "3D input", frames)
    }

    pub fn playback(pa: &'a Pulse, device: &str, frames: u32) -> Result<Stream<'a>, String> {
        Stream::open(pa, false, device, "3D audio", frames)
    }

    pub fn read(&mut self, buf: &mut [f32]) -> Result<(), String> {
        let mut err = 0;
        // SAFETY: buf is valid for its byte length.
        let r = unsafe { (self.pa.read)(self.s, buf.as_mut_ptr().cast(), std::mem::size_of_val(buf), &mut err) };
        if r < 0 { Err(format!("recording stopped (error {err})")) } else { Ok(()) }
    }

    pub fn write(&mut self, buf: &[f32]) -> Result<(), String> {
        let mut err = 0;
        // SAFETY: buf is valid for its byte length.
        let r = unsafe { (self.pa.write)(self.s, buf.as_ptr().cast(), std::mem::size_of_val(buf), &mut err) };
        if r < 0 { Err(format!("playback stopped (error {err})")) } else { Ok(()) }
    }

    /// The stream's current latency in microseconds.
    pub fn latency_us(&mut self) -> u64 {
        let mut err = 0;
        // SAFETY: valid handle.
        unsafe { (self.pa.latency)(self.s, &mut err) }
    }
}

impl Drop for Stream<'_> {
    fn drop(&mut self) {
        // SAFETY: the handle came from pa_simple_new and is freed once.
        unsafe { (self.pa.free)(self.s) }
    }
}

fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl").args(args).output().map_err(|e| format!("pactl not found ({e})"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn default_sink() -> Option<String> {
    pactl(&["get-default-sink"]).ok().filter(|s| !s.is_empty())
}

/// The output to play on: the default one, unless that is Harp's own sink.
/// (`HARP_SPATIAL_OUTPUT` overrides it, for tests.)
pub fn real_output() -> Option<String> {
    if let Ok(o) = std::env::var("HARP_SPATIAL_OUTPUT") {
        return Some(o);
    }
    match default_sink() {
        Some(s) if s != SINK_NAME => Some(s),
        _ => pactl(&["list", "sinks", "short"])
            .ok()?
            .lines()
            .filter_map(|l| l.split('\t').nth(1).map(str::to_string))
            .find(|n| n != SINK_NAME),
    }
}

/// Removes any Harp sink left behind (a crash, a kill).
pub fn remove_stale_sinks() {
    if let Ok(list) = pactl(&["list", "modules", "short"]) {
        for line in list.lines() {
            if line.contains("module-null-sink") && line.contains(&format!("sink_name={SINK_NAME}")) {
                if let Some(id) = line.split('\t').next() {
                    let _ = pactl(&["unload-module", id]);
                }
            }
        }
    }
}

/// Creates the "Harp 3D" sink; returns the module id. The default output is put back if
/// the new sink took its place.
pub fn create_sink() -> Result<String, String> {
    let before = default_sink();
    let id = pactl(&[
        "load-module",
        "module-null-sink",
        &format!("sink_name={SINK_NAME}"),
        "sink_properties=device.description=\"Harp 3D\"",
        "rate=48000",
        "channels=2",
        "format=float32le",
    ])?;
    if let Some(b) = before {
        if default_sink().as_deref() != Some(b.as_str()) {
            let _ = pactl(&["set-default-sink", &b]);
        }
    }
    Ok(id)
}

pub fn remove_sink(module: &str) {
    let _ = pactl(&["unload-module", module]);
}
