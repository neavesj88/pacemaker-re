//! Vocal / instrumental stem separation.
//!
//! The pieces, from the model outwards:
//! - [`provider::StemModel`]: what a separation model must do for ONE fixed-length stereo
//!   segment. Real models (HTDemucs through ONNX Runtime, `htdemucs`, feature `ort`) and the
//!   test models in [`mock`] both implement it.
//! - [`stft`] / [`demucs`]: the STFT, iSTFT and Demucs spectrogram packing the model graph
//!   expects, done here in Rust (the ONNX graph cannot contain them).
//! - [`overlap`]: cuts a whole track into overlapping segments and joins the model's answers
//!   again with a weighted overlap-add, finishing frames as soon as no later segment touches them.
//! - [`pipeline::separate`]: the whole job for one track, streaming the two results to writers.
//! - [`sink`]: the file writers (MP3, Ogg Vorbis, WAV), each writing `<name>.partial` and
//!   renaming on success.
//!
//! Nothing here touches the network or the device; the app decides where models come from.

pub mod demucs;
pub mod mock;
pub mod overlap;
pub mod pipeline;
pub mod provider;
pub mod sink;
pub mod stft;

#[cfg(feature = "ort")]
pub mod htdemucs;

/// The sample rate every model and every output file here uses.
pub const RATE: u32 = 44_100;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The request itself is wrong (lengths, parameters).
    Invalid(String),
    Io(String),
    /// The model failed to load or to run.
    Model(String),
    Encode(String),
    /// The caller asked to stop.
    Cancelled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Invalid(s) | Error::Io(s) | Error::Model(s) | Error::Encode(s) => f.write_str(s),
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
