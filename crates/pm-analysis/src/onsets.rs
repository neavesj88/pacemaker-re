//! Onset list for checking a beat grid against the audio (owner: grid area).
//!
//! The same spectral-flux envelope the analyser tracks beats on ([`crate::onset::envelope`]),
//! peak-picked into discrete attacks. Times are returned in the **device convention**, the same
//! convention as the beat positions the analyser writes to a `.str` (the envelope's measured lead
//! [`crate::ONSET_LEAD_SECS`] added back, then [`crate::DEVICE_OFFSET_SECS`]), so a grid that sits
//! on the music has beat lines at (close to) the onset frames and the offset between a line and its
//! onset is the grid's drift. Heuristic, not ground truth: a hi-hat or a bass note is an onset too,
//! and the caller decides which onset belongs to which beat.

use crate::onset::Envelope;

/// One detected attack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Onset {
    /// Position in source sample frames, device convention.
    pub frame: i64,
    /// Envelope height at the peak, in standard deviations of the detrended flux (about 1 to 20).
    pub strength: f32,
}

/// Default for the peak floor (in standard deviations of the detrended flux); the caller passes the
/// user's own setting, this is only its default.
pub const MIN_STRENGTH: f32 = 1.0;
/// Two peaks closer than this many envelope frames (about 46 ms at 44.1 kHz) are one attack.
const MIN_SPACING: usize = 4;

/// Peak-picks an envelope into onsets, earliest first.
pub fn pick(env: &Envelope, min_strength: f32) -> Vec<Onset> {
    let odf = &env.odf;
    let fps = env.frames_per_sec();
    let shift = crate::ONSET_LEAD_SECS + crate::DEVICE_OFFSET_SECS;
    let mut peaks: Vec<(f64, f32)> = Vec::new(); // (envelope-frame position, strength)
    for i in 1..odf.len().saturating_sub(1) {
        let (a, b, c) = (odf[i - 1], odf[i], odf[i + 1]);
        if b < min_strength || b <= a || b < c {
            continue;
        }
        // parabolic interpolation, as the analyser's refine_times does
        let den = a as f64 - 2.0 * b as f64 + c as f64;
        let x = if den.abs() > 1e-9 { i as f64 + (0.5 * (a as f64 - c as f64) / den).clamp(-0.5, 0.5) } else { i as f64 };
        match peaks.last_mut() {
            // a stronger peak within the spacing replaces the weaker one before it
            Some(last) if x - last.0 < MIN_SPACING as f64 => {
                if b > last.1 {
                    *last = (x, b);
                }
            }
            _ => peaks.push((x, b)),
        }
    }
    peaks
        .into_iter()
        .map(|(x, strength)| Onset { frame: ((x / fps + shift) * env.sample_rate as f64).round() as i64, strength })
        .collect()
}

/// Onsets of decoded mono audio.
pub fn detect(mono: &[f32], sample_rate: u32, min_strength: f32) -> Vec<Onset> {
    pick(&crate::onset::envelope(mono, sample_rate), min_strength)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decaying 55 Hz kick with a 2 ms click at each time, as in the analyser's own test.
    pub(crate) fn kicks(sr: u32, secs: f64, times: &[f64]) -> Vec<f32> {
        let n = (sr as f64 * secs) as usize;
        let mut mono = vec![0f32; n];
        for &t in times {
            let start = (t * sr as f64) as usize;
            for i in 0..(0.4 * sr as f64) as usize {
                if start + i >= n {
                    break;
                }
                let tt = i as f32 / sr as f32;
                let click = if tt < 0.002 { 0.3 * (1.0 - tt / 0.002) * if i % 2 == 0 { 1.0 } else { -1.0 } } else { 0.0 };
                mono[start + i] += (2.0 * std::f32::consts::PI * 55.0 * tt).sin() * (-tt * 30.0).exp() + click;
            }
        }
        mono
    }

    #[test]
    fn finds_every_kick_in_the_device_convention() {
        let sr = 44_100;
        let times: Vec<f64> = (0..24).map(|k| 0.3 + k as f64 * 0.5).collect();
        let found = detect(&kicks(sr, 13.0, &times), sr, MIN_STRENGTH);
        for &t in &times {
            let want = ((t + crate::DEVICE_OFFSET_SECS) * sr as f64).round() as i64;
            let near = found.iter().filter(|o| (o.frame - want).abs() < (0.006 * sr as f64) as i64).count();
            assert_eq!(near, 1, "one onset within 6 ms of the kick at {t} s (found {} onsets in all)", found.len());
        }
    }

    #[test]
    fn a_higher_floor_keeps_only_the_stronger_attacks() {
        let sr = 44_100;
        let times: Vec<f64> = (0..10).map(|k| 0.2 + k as f64 * 0.37).collect();
        let audio = kicks(sr, 5.0, &times);
        let all = detect(&audio, sr, MIN_STRENGTH);
        let strongest = all.iter().map(|o| o.strength).fold(0.0, f32::max);
        assert!(detect(&audio, sr, strongest + 1.0).is_empty());
        let mid = detect(&audio, sr, MIN_STRENGTH + 0.5 * (strongest - MIN_STRENGTH));
        assert!(mid.len() <= all.len() && mid.iter().all(|o| o.strength >= MIN_STRENGTH + 0.5 * (strongest - MIN_STRENGTH)));
    }

    #[test]
    fn silence_has_no_onsets_and_order_is_ascending() {
        assert!(detect(&vec![0.0; 44_100 * 3], 44_100, MIN_STRENGTH).is_empty());
        let sr = 44_100;
        let times: Vec<f64> = (0..10).map(|k| 0.2 + k as f64 * 0.37).collect();
        let found = detect(&kicks(sr, 5.0, &times), sr, MIN_STRENGTH);
        assert!(found.windows(2).all(|w| w[0].frame < w[1].frame));
    }
}
