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
    pub pitch: f32,
    /// Up/down calibration: `""`, `"waiting"`, `"running:SECONDS"`, `"done"` or `"failed:WHY"`.
    pub calibrating: String,
    /// The up/down calibrations to save (pass them back in `SpatialOptions`).
    pub up_calibration: Vec<f32>,
    pub latency_ms: u32,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SpatialOptions {
    pub head_tracking: bool,
    pub flip: bool,
    pub room: bool,
    pub bounce: bool,
    /// "off", "slow" or "dynamic".
    pub recenter: String,
    pub up_calibration: Vec<f32>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn spatial_supported() -> bool {
    harp_spatial::SUPPORTED
}

/// Starts 3D audio, or changes its options while it runs; returns the output device the
/// player must use.
pub fn spatial_start(opts: SpatialOptions) -> Result<String, String> {
    harp_spatial::start(&harp_spatial::Options {
        head_tracking: opts.head_tracking,
        flip: opts.flip,
        room: opts.room,
        bounce: opts.bounce,
        recenter: opts.recenter,
        up_calibration: opts.up_calibration,
    })
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

/// Up/down calibration: face the screen, call this, then look up within 4 seconds.
#[flutter_rust_bridge::frb(sync)]
pub fn spatial_calibrate_up() {
    harp_spatial::calibrate_up();
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
        pitch: s.pitch,
        calibrating: s.calibrating,
        up_calibration: s.up_calibration,
        latency_ms: s.latency_ms,
        error: s.error,
    }
}
