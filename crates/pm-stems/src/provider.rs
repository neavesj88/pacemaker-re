//! The model-provider trait. A provider turns ONE fixed-length stereo segment into a vocals
//! estimate and an estimate of everything else; chunking, normalising, overlap-add and file
//! writing are the pipeline's job, so a different model (Open-Unmix, a RoFormer) is a new
//! implementation of this trait and nothing else.

use crate::Error;

/// Two equally long channels.
pub type Stereo = [Vec<f32>; 2];

/// What a provider returns for one segment.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentStems {
    pub vocals: Stereo,
    /// Everything that is not vocals (for HTDemucs: drums + bass + other).
    pub rest: Stereo,
}

/// Stable facts about a loaded model, shown to the user and logged with a run.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    /// Short id, e.g. `htdemucs-ort`.
    pub id: String,
    pub name: String,
    /// Sample rate the model works at; the pipeline hands it audio at this rate.
    pub sample_rate: u32,
    /// Frames per call.
    pub segment_len: usize,
    /// Where it runs, e.g. `CPU` or `GPU (DirectML)`; for the UI.
    pub device: String,
}

pub trait StemModel {
    fn info(&self) -> &ModelInfo;

    /// Separates one segment of `info().segment_len` frames at `info().sample_rate`. The input
    /// has already been normalised by the pipeline; the output must be on the same scale and
    /// the same length.
    fn separate(&mut self, segment: &Stereo) -> Result<SegmentStems, Error>;
}

/// Checks a provider's answer against the request so a misbehaving model fails loudly instead
/// of corrupting the overlap-add.
pub fn check_output(out: &SegmentStems, len: usize) -> Result<(), Error> {
    let ok = [&out.vocals, &out.rest].iter().all(|s| s[0].len() == len && s[1].len() == len);
    if ok { Ok(()) } else { Err(Error::Model(format!("the model returned segments that are not {len} frames long"))) }
}
