//! Mix audio engine for Pacemaker Revival: renders a `pm_formats::mix::Mix` to PCM (decode,
//! time-stretch to the mix tempo, key shift, transitions, tools/effects, gain envelope, master
//! limiter) for preview and export. Design and parameter mappings: `notes/mix-audio.md`.
//!
//! ```text
//! file -> decode (stereo, 44.1 kHz) -> tempo/key (WSOLA | vinyl resample) -> loop/reverse/ignore
//!      -> tools -> clip gain -> audible window -> crossfader -> sum -> limiter -> WAV / Ogg
//! ```
//!
//! Entry points: [`engine::render`] (streaming), [`engine::render_to_vecs`] (memory),
//! [`output::render_to_file`] (WAV or Ogg Vorbis). The caller supplies a [`engine::Resolver`] that
//! turns a mix track into decoded audio.

pub mod audio;
pub mod clip;
pub mod engine;
pub mod env;
pub mod fx;
pub mod master;
pub mod output;
pub mod sinc;
pub mod stretch;

pub use engine::{render, render_to_vecs, DirResolver, Options, Rendered, Resolver};
pub use output::{render_to_file, Format};

/// The engine's sample rate; every position in a mix is a sample count at this rate.
pub const RATE: u32 = 44_100;

#[derive(Debug)]
pub enum Error {
    Decode(String),
    Io(String),
    Encode(String),
    Invalid(String),
    Cancelled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Decode(s) => write!(f, "cannot decode audio: {s}"),
            Error::Io(s) => write!(f, "file error: {s}"),
            Error::Encode(s) => write!(f, "cannot encode audio: {s}"),
            Error::Invalid(s) => f.write_str(s),
            Error::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}
