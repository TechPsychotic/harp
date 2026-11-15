//! The audio route on Linux: Harp plays into its own virtual output ("Harp 3D", a pipe sink:
//! the sound server writes what plays on it into a FIFO, in real time), this module reads it,
//! renders it binaurally and plays the result on the real output.
//!
//! A pipe sink rather than recording a null sink's monitor: no recording stream is opened, so
//! the desktop shows no microphone-in-use indicator (it did before, for a stream that never
//! touched the microphone).
//!
//! Each Harp process gets its own sink (`harp_spatial_<pid>`), so two copies (or a test) never
//! take each other's output; leftovers are only removed when their process is gone.
//!
//! Playback goes through the PulseAudio "simple" API, loaded at run time (`dlopen`), so Harp
//! still starts on systems without it. The sink is created and removed with `pactl`.

use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CString};
use std::process::Command;

const SINK_PREFIX: &str = "harp_spatial";

/// This process's sink.
pub fn sink_name() -> String {
    format!("{SINK_PREFIX}_{}", std::process::id())
}

/// Where the sink writes its sound.
pub fn fifo_path() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    dir.join(format!("harp-3d-{}.fifo", std::process::id()))
}

fn is_ours(name: &str) -> bool {
    name.starts_with(SINK_PREFIX)
}

const PA_STREAM_PLAYBACK: c_int = 1;
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
type WriteFn = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type LatencyFn = unsafe extern "C" fn(*mut c_void, *mut c_int) -> u64;

/// The loaded library and its functions.
pub struct Pulse {
    _lib: Library,
    new: NewFn,
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
            let write = *lib.get::<WriteFn>(b"pa_simple_write\0").map_err(|e| e.to_string())?;
            let free = *lib.get::<FreeFn>(b"pa_simple_free\0").map_err(|e| e.to_string())?;
            let latency = *lib.get::<LatencyFn>(b"pa_simple_get_latency\0").map_err(|e| e.to_string())?;
            Ok(Pulse { _lib: lib, new, write, free, latency })
        }
    }
}

/// One recording or playback stream (48 kHz float stereo).
pub struct Stream<'a> {
    pa: &'a Pulse,
    s: *mut c_void,
}

impl<'a> Stream<'a> {
    /// A playback stream on `device`. Its buffer is small (head-tracking lag) but keeps
    /// ~64 ms so a busy moment doesn't run it dry (a crackle).
    pub fn playback(pa: &'a Pulse, device: &str, frames: u32) -> Result<Stream<'a>, String> {
        let ss = SampleSpec { format: PA_SAMPLE_FLOAT32LE, rate: 48000, channels: 2 };
        let bytes = frames * 8;
        let attr = BufferAttr { maxlength: bytes * 24, tlength: bytes * 12, prebuf: bytes * 4, minreq: bytes, fragsize: u32::MAX };
        let app = CString::new("Harp").unwrap();
        let dev = CString::new(device).map_err(|e| e.to_string())?;
        let sname = CString::new("3D audio").unwrap();
        let mut err: c_int = 0;
        // SAFETY: all pointers are valid for the call; the returned handle is owned here.
        let s = unsafe { (pa.new)(std::ptr::null(), app.as_ptr(), PA_STREAM_PLAYBACK, dev.as_ptr(), sname.as_ptr(), &ss, std::ptr::null(), &attr, &mut err) };
        if s.is_null() {
            return Err(format!("couldn't open a playback stream on {device} (error {err})"));
        }
        Ok(Stream { pa, s })
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
        Some(s) if !is_ours(&s) => Some(s),
        _ => pactl(&["list", "sinks", "short"])
            .ok()?
            .lines()
            .filter_map(|l| l.split('\t').nth(1).map(str::to_string))
            .find(|n| !is_ours(n)),
    }
}

fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Other running Harp processes (by name).
fn other_harps() -> usize {
    let me = std::process::id().to_string();
    std::fs::read_dir("/proc")
        .map(|d| {
            d.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy() != me.as_str())
                .filter(|e| std::fs::read_to_string(e.path().join("comm")).map(|c| c.trim() == "harp").unwrap_or(false))
                .count()
        })
        .unwrap_or(0)
}

/// Whether a sink name found in the module list is a leftover: its Harp process is gone (or,
/// for the older shared name, no other Harp is running).
fn stale(sink: &str) -> bool {
    if sink == SINK_PREFIX {
        return other_harps() == 0;
    }
    match sink.strip_prefix(SINK_PREFIX).and_then(|r| r.strip_prefix('_')).and_then(|p| p.parse::<u32>().ok()) {
        Some(pid) => pid != std::process::id() && !process_alive(pid),
        None => false,
    }
}

/// Removes Harp sinks left behind by a crash or a kill, never another running Harp's.
pub fn remove_stale_sinks() {
    if let Ok(list) = pactl(&["list", "modules", "short"]) {
        for line in list.lines() {
            let Some(sink) = line.split_whitespace().find_map(|a| a.strip_prefix("sink_name=")) else { continue };
            if is_ours(sink) && (line.contains("module-null-sink") || line.contains("module-pipe-sink")) && stale(sink) {
                if let Some(id) = line.split('\t').next() {
                    let _ = pactl(&["unload-module", id]);
                }
            }
        }
    }
}

/// Creates this process's "Harp 3D" sink; returns the module id. The default output is put
/// back if the new sink took its place.
pub fn create_sink() -> Result<String, String> {
    let before = default_sink();
    let fifo = fifo_path();
    let _ = std::fs::remove_file(&fifo);
    let id = pactl(&[
        "load-module",
        "module-pipe-sink",
        &format!("file={}", fifo.display()),
        &format!("sink_name={}", sink_name()),
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

/// Reads the sink's FIFO in whole blocks.
pub struct FifoReader {
    fd: std::os::fd::OwnedFd,
    buf: Vec<u8>,
}

impl FifoReader {
    pub fn open() -> Result<FifoReader, String> {
        use std::os::fd::FromRawFd;
        let path = std::ffi::CString::new(fifo_path().as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: opening a path; the descriptor is owned here.
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(format!("couldn't open the Harp 3D pipe: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: fd is valid and owned from here on.
        Ok(FifoReader { fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) }, buf: Vec::with_capacity(1 << 16) })
    }

    /// Fills `out` (interleaved f32) and returns true, or returns false after `timeout` with
    /// nothing complete (nothing playing).
    pub fn read(&mut self, out: &mut [f32], timeout: std::time::Duration) -> Result<bool, String> {
        use std::os::fd::AsRawFd;
        let need = std::mem::size_of_val(out);
        let end = std::time::Instant::now() + timeout;
        while self.buf.len() < need {
            let left = end.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            let mut p = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd.
            let r = unsafe { libc::poll(&mut p, 1, left.as_millis().max(1) as i32) };
            if r < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e.to_string());
            }
            if r == 0 {
                return Ok(false);
            }
            let mut tmp = [0u8; 16384];
            // SAFETY: valid descriptor and buffer.
            let n = unsafe { libc::read(self.fd.as_raw_fd(), tmp.as_mut_ptr().cast(), tmp.len()) };
            if n > 0 {
                self.buf.extend_from_slice(&tmp[..n as usize]);
            } else if n == 0 {
                // No writer right now: wait a little instead of spinning.
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        for (i, c) in self.buf[..need].chunks_exact(4).enumerate() {
            out[i] = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        }
        self.buf.drain(..need);
        Ok(true)
    }
}

pub fn remove_sink(module: &str) {
    let _ = pactl(&["unload-module", module]);
    let _ = std::fs::remove_file(fifo_path());
}
