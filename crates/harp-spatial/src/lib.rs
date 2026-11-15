//! Harp's 3D audio.
//!
//! - [`hrir`]: the built-in head-related impulse responses (SADIE II, subject D1).
//! - [`render`]: stereo to binaural through two virtual speakers, following the head.
//! - [`head`]: AirPods orientation → how far the head is turned.
//! - Linux only: [`aacp`] (AirPods head tracking over Bluetooth, no root), [`route`] (the
//!   "Harp 3D" output and the PulseAudio/PipeWire streams) and the service that runs them.

pub mod dsp;
pub mod eq;
pub mod head;
pub mod hrir;
pub mod render;

#[cfg(target_os = "linux")]
pub mod aacp;
#[cfg(target_os = "linux")]
pub mod route;
#[cfg(target_os = "linux")]
mod service;

/// What 3D audio is doing.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub running: bool,
    /// The real output the 3D sound plays on.
    pub output: String,
    /// Head tracking: "off", "searching", "connecting", "tracking", "paused", "in_use",
    /// "unsupported" or "error".
    pub head: String,
    pub head_detail: String,
    /// Head turn (degrees, left positive) and tilt (up positive) being applied.
    pub yaw: f32,
    pub pitch: f32,
    /// Up/down calibration: "", "waiting", "running:<seconds left>", "done" or "failed:<why>".
    pub calibrating: String,
    /// The saved up/down calibrations (keep them and pass them back in [`Options`]).
    pub up_calibration: Vec<f32>,
    /// Added delay of the 3D route (recording + playback buffers), milliseconds.
    pub latency_ms: u32,
    pub error: Option<String>,
    /// Android: what the audio engine's 3D hook is doing (found, chunks, output format).
    pub engine: String,
}

/// Whether 3D audio can run here: on Linux always (through Harp's own output), on Android
/// when the audio engine is Harp's build with the 3D hook.
pub fn supported() -> bool {
    cfg!(target_os = "linux") || (cfg!(target_os = "android") && dsp::available())
}

#[cfg(target_os = "android")]
static ANDROID_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How 3D audio should behave.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub head_tracking: bool,
    pub flip: bool,
    /// The virtual room (reflections and reverb).
    pub room: bool,
    /// "3D Bounce": the stage glides around you.
    pub bounce: bool,
    /// "off", "slow" or "dynamic" (see [`head::Recenter`]).
    pub recenter: String,
    /// Saved up/down calibrations (from [`Status::up_calibration`]).
    pub up_calibration: Vec<f32>,
}

/// Starts 3D audio (or changes its options); returns the output device for the player.
pub fn start(opts: &Options) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        service::start(opts)
    }
    #[cfg(target_os = "android")]
    {
        // Inside the engine: no output to switch to (an empty device keeps the usual one).
        dsp::enable(opts.room, opts.bounce)?;
        ANDROID_ON.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(String::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = opts;
        Err("3D audio isn't available on this platform yet".into())
    }
}

/// Up/down calibration: face the screen, call this, then look up within 4 seconds.
pub fn calibrate_up() {
    #[cfg(target_os = "linux")]
    service::calibrate_up();
}

pub fn stop() {
    #[cfg(target_os = "linux")]
    service::stop();
    #[cfg(target_os = "android")]
    {
        dsp::disable();
        ANDROID_ON.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn recenter() {
    #[cfg(target_os = "linux")]
    service::recenter();
}

pub fn status() -> Status {
    #[cfg(target_os = "linux")]
    {
        service::status()
    }
    #[cfg(target_os = "android")]
    {
        Status {
            running: ANDROID_ON.load(std::sync::atomic::Ordering::Relaxed),
            head: "unsupported".into(),
            head_detail: "Head tracking isn't on Android yet".into(),
            engine: dsp::diagnostics(),
            ..Status::default()
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        Status::default()
    }
}
