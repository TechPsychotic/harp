//! Harp's 3D audio.
//!
//! - [`hrir`]: the built-in head-related impulse responses (SADIE II, subject D1).
//! - [`render`]: stereo to binaural through two virtual speakers, following the head.
//! - [`head`]: AirPods orientation → how far the head is turned.
//! - Linux only: [`aacp`] (AirPods head tracking over Bluetooth, no root), [`route`] (the
//!   "Harp 3D" output and the PulseAudio/PipeWire streams) and the service that runs them.

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
    /// Head turn (degrees, left positive) being applied.
    pub yaw: f32,
    /// Added delay of the 3D route (recording + playback buffers), milliseconds.
    pub latency_ms: u32,
    pub error: Option<String>,
}

/// Whether this build can run 3D audio at all.
pub const SUPPORTED: bool = cfg!(target_os = "linux");

/// Starts 3D audio (or changes its options); returns the output device for the player.
pub fn start(head_tracking: bool, flip: bool) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        service::start(head_tracking, flip)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (head_tracking, flip);
        Err("3D audio is only available on Linux for now".into())
    }
}

pub fn stop() {
    #[cfg(target_os = "linux")]
    service::stop();
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
    #[cfg(not(target_os = "linux"))]
    {
        Status::default()
    }
}
