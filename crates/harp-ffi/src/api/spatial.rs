//! 3D audio (HRTF, with AirPods head tracking) for Flutter. Linux only for now; elsewhere
//! [`spatial_supported`] is false and starting fails.

/// What 3D audio is doing (see `harp_spatial::Status`).
#[derive(Clone, Debug)]
pub struct SpatialStatus {
    pub running: bool,
    pub output: String,
    /// "off", "searching", "connecting", "tracking", "paused", "in_use", "unsupported", "error".
    pub head: String,
    pub head_detail: String,
    pub yaw: f32,
    pub latency_ms: u32,
    pub error: Option<String>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn spatial_supported() -> bool {
    harp_spatial::SUPPORTED
}

/// Starts 3D audio, or changes its options while it runs; returns the output device the
/// player must use.
pub fn spatial_start(head_tracking: bool, flip: bool) -> Result<String, String> {
    harp_spatial::start(head_tracking, flip)
}

/// Stops 3D audio and removes its output. Point the player back at "auto" first.
pub fn spatial_stop() {
    harp_spatial::stop();
}

/// The current head direction becomes the front.
#[flutter_rust_bridge::frb(sync)]
pub fn spatial_recenter() {
    harp_spatial::recenter();
}

#[flutter_rust_bridge::frb(sync)]
pub fn spatial_status() -> SpatialStatus {
    let s = harp_spatial::status();
    SpatialStatus {
        running: s.running,
        output: s.output,
        head: s.head,
        head_detail: s.head_detail,
        yaw: s.yaw,
        latency_ms: s.latency_ms,
        error: s.error,
    }
}
