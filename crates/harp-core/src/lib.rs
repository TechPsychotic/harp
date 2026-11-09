//! Harp's core: songs, the queue, the on-device taste model and recommender.
//!
//! No accounts and no API keys: everything here works from what yt-dlp can
//! fetch (through the [`recommend::CandidateSource`] trait) and a local SQLite file.

pub mod error;
pub mod export;
pub mod import;
pub mod library;
pub mod lyrics;
pub mod model;
pub mod queue;
pub mod recommend;
pub mod rng;
pub mod taste;
pub mod tidy;

pub use error::{Error, Result};
pub use model::Song;
