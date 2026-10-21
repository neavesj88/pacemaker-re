//! Audio-based grid check: where the kick sits relative to a beat grid. Measurement only; the
//! analyser itself never calls this. Works on any grid (ours or the original Editor's) so both can
//! be judged on the same audio.

use crate::onset::Envelope;

/// A grid whose kick offset is within this many ms counts as being on the kick.
pub const ON_KICK_MS: f64 = 15.0;
/// Resolution of the beat-synchronous profile.
const STEP_SECS: f64 = 0.002;

#[derive(Debug, Clone, Copy)]
pub struct KickOffset {
    /// Where the kick detector peaks relative to the grid lines, in ms (positive = kick after the line).
    pub offset_ms: f64,
    /// Peak of the profile over its mean: how clearly the kick stands out at that offset.
    pub contrast: f64,
}

impl KickOffset {
    pub fn on_kick(&self) -> bool {
        self.offset_ms.abs() <= ON_KICK_MS
    }
}

/// Beat-synchronous average of the kick detector over one full beat around each grid line.
/// `beats` are sample frames on the audio's own timeline (not the device convention) and `bpm`
/// sets the span of the profile. `None` when there is nothing to measure.
pub fn kick_offset(env: &Envelope, beats: &[i64], bpm: f64) -> Option<KickOffset> {
    let (fps, sr) = (env.frames_per_sec(), env.sample_rate as f64);
    let steps = (60.0 / bpm / STEP_SECS) as i64;
    if steps < 2 || beats.is_empty() {
        return None;
    }
    let mut prof = vec![0f64; steps as usize];
    for &b in beats {
        // the detector frame registers a kick ONSET_LEAD_SECS before its true attack
        let t = b as f64 / sr - crate::ONSET_LEAD_SECS;
        for (k, p) in prof.iter_mut().enumerate() {
            let f = ((t + (k as i64 - steps / 2) as f64 * STEP_SECS) * fps).round();
            if f >= 0.0 && (f as usize) < env.kick.len() {
                *p += env.kick[f as usize] as f64;
            }
        }
    }
    let peak = (0..prof.len()).max_by(|&a, &b| prof[a].total_cmp(&prof[b]))?;
    let mean = prof.iter().sum::<f64>() / prof.len() as f64;
    Some(KickOffset {
        offset_ms: (peak as i64 - steps / 2) as f64 * STEP_SECS * 1000.0,
        contrast: prof[peak] / mean.max(1e-9),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kick attacks every 0.5 s (120 BPM), 40 ms after the grid lines; the detector frame sits
    /// ONSET_LEAD_SECS before each attack.
    fn envelope() -> Envelope {
        let (sr, hop) = (44100u32, 512usize);
        let fps = sr as f64 / hop as f64;
        let mut kick = vec![0f32; (fps * 20.0) as usize];
        for k in 0..38 {
            let t = k as f64 * 0.5 + 0.040 - crate::ONSET_LEAD_SECS;
            kick[(t * fps).round() as usize] = 1.0;
        }
        Envelope { odf: vec![], bass: vec![], kick, hop, sample_rate: sr }
    }

    fn grid(shift_secs: f64) -> Vec<i64> {
        (1..30).map(|k| ((k as f64 * 0.5 + shift_secs) * 44100.0) as i64).collect()
    }

    #[test]
    fn finds_the_kick_after_the_line() {
        let k = kick_offset(&envelope(), &grid(0.0), 120.0).unwrap();
        assert!((k.offset_ms - 40.0).abs() <= 12.0, "{k:?}");
        assert!(!k.on_kick());
    }

    #[test]
    fn grid_on_the_kick_is_on_kick() {
        let k = kick_offset(&envelope(), &grid(0.040), 120.0).unwrap();
        assert!(k.on_kick(), "{k:?}");
        assert!(k.contrast > 5.0);
    }

    #[test]
    fn nothing_to_measure() {
        assert!(kick_offset(&envelope(), &[], 120.0).is_none());
    }
}
