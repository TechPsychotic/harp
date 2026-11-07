//! Runs yt-dlp for Harp: search, resolve an audio stream, "watch next".
//!
//! Desktop runs the yt-dlp single-file binary; Android runs the yt-dlp zipapp on
//! a static Python shipped as a native library (same technique as zevro) behind
//! a loopback proxy ([`proxy`]) because that Python cannot resolve DNS names.

pub mod android;
pub mod config;
pub mod extractor;
pub mod install;
pub mod parse;
pub mod proxy;

pub use config::Tool;
pub use extractor::{Stream, YtDlp};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not run yt-dlp: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("yt-dlp failed: {0}")]
    Failed(String),
    #[error("yt-dlp timed out")]
    Timeout,
    #[error("unexpected yt-dlp output: {0}")]
    Parse(String),
    #[error("download: {0}")]
    Download(String),
}

pub type Result<T> = std::result::Result<T, Error>;
