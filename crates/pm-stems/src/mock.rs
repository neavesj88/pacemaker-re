//! Models that need no weights, for tests and for exercising the pipeline end to end.
//! They are not offered to users: a separation that is not one would only mislead.

use crate::Error;
use crate::provider::{ModelInfo, SegmentStems, Stereo, StemModel};

fn info(id: &str, segment_len: usize) -> ModelInfo {
    ModelInfo { id: id.into(), name: format!("{id} (test model)"), sample_rate: crate::RATE, segment_len, device: "CPU".into() }
}

/// Vocals are the input, the rest is silence: `vocals + rest == input` exactly.
pub struct IdentityModel(ModelInfo);

impl IdentityModel {
    pub fn new(segment_len: usize) -> IdentityModel {
        IdentityModel(info("identity", segment_len))
    }
}

impl StemModel for IdentityModel {
    fn info(&self) -> &ModelInfo {
        &self.0
    }

    fn separate(&mut self, seg: &Stereo) -> Result<SegmentStems, Error> {
        Ok(SegmentStems { vocals: seg.clone(), rest: [vec![0.0; seg[0].len()], vec![0.0; seg[1].len()]] })
    }
}

/// "Vocals" are the centre (mid) of the stereo image and the rest is the side signal: a crude
/// but deterministic split that differs per channel, so channel mix-ups show in tests.
pub struct MidSideModel(ModelInfo);

impl MidSideModel {
    pub fn new(segment_len: usize) -> MidSideModel {
        MidSideModel(info("mid-side", segment_len))
    }
}

impl StemModel for MidSideModel {
    fn info(&self) -> &ModelInfo {
        &self.0
    }

    fn separate(&mut self, seg: &Stereo) -> Result<SegmentStems, Error> {
        let mid: Vec<f32> = seg[0].iter().zip(&seg[1]).map(|(l, r)| 0.5 * (l + r)).collect();
        let rest = [seg[0].iter().zip(&mid).map(|(l, m)| l - m).collect(), seg[1].iter().zip(&mid).map(|(r, m)| r - m).collect()];
        Ok(SegmentStems { vocals: [mid.clone(), mid], rest })
    }
}

/// Fails on a chosen segment (0-based call count), to test error paths.
pub struct FailingModel {
    info: ModelInfo,
    calls: usize,
    fail_on: usize,
}

impl FailingModel {
    pub fn new(segment_len: usize, fail_on: usize) -> FailingModel {
        FailingModel { info: info("failing", segment_len), calls: 0, fail_on }
    }
}

impl StemModel for FailingModel {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    fn separate(&mut self, seg: &Stereo) -> Result<SegmentStems, Error> {
        let n = self.calls;
        self.calls += 1;
        if n == self.fail_on {
            return Err(Error::Model("synthetic model failure".into()));
        }
        Ok(SegmentStems { vocals: seg.clone(), rest: [vec![0.0; seg[0].len()], vec![0.0; seg[1].len()]] })
    }
}
