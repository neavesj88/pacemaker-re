//! Track analysis for the Pacemaker: tempo, beat grid and first downbeat.
//!
//! Original implementation of published methods (spectral-flux onsets, autocorrelation tempo,
//! Ellis dynamic-programming beat tracking), followed by a locked-grid fit tuned for DJ music.
//! Output converts directly to the device's `.str` sidecar via [`Result::to_str_analysis`].

pub mod align;
pub mod beats;
pub mod decode;
pub mod grid;
pub mod ingest;
pub mod onset;
pub mod tempo;

use std::path::Path;

use pm_formats::strfile;

/// Above this RMS deviation the track is treated as variable-tempo and the tracked beats are
/// kept instead of a straight grid.
const VARIABLE_TEMPO_MS: f64 = 25.0;
const TIGHTNESS: f64 = 100.0;
/// More than this fraction of tracked beats off the grid also marks the tempo as variable.
const MAX_OFF_GRID: f64 = 0.6;
const CANDIDATES: usize = 5;
const DETECT_RANGE: tempo::BpmRange = tempo::BpmRange { lo: 100.0 };
/// The centred 2048-sample spectral-flux frame registers an onset this much before the true
/// attack. Measured as a constant 13.3 ms (+-0.3) across 120-174 BPM and all phases with
/// examples/latency.rs, and added back to every beat.
pub(crate) const ONSET_LEAD_SECS: f64 = 0.0133;

#[derive(Debug, Clone)]
pub struct Result {
    pub bpm: f64,
    /// Beat positions in source sample frames.
    pub beats: Vec<i64>,
    /// Local BPM at each beat (constant on a locked grid).
    pub beat_bpm: Vec<f64>,
    pub first_downbeat: Option<usize>,
    pub sample_rate: u32,
    pub duration_secs: f64,
    pub locked_grid: bool,
    pub snapped: bool,
    pub residual_ms: f64,
    pub tag_bpm: Option<f64>,
}

/// The original Editor places beats this much later than our analysis does on the same audio:
/// median 21.9 ms over 25 tracks where both agree on tempo (20.9 ms on files without a LAME
/// gapless header, 23.1 ms with one, so it is not gapless trimming). Whether it comes from
/// Tonium's MP3 decoder or its detector can't be separated without the device. Exported beat
/// positions follow the Editor so mixed libraries line up when the device syncs. Provisional:
/// refine against a real device.
pub const DEVICE_OFFSET_SECS: f64 = 0.022;

impl Result {
    /// Beat positions in the device's convention (see [`DEVICE_OFFSET_SECS`]).
    pub fn device_beats(&self) -> Vec<i64> {
        let shift = (DEVICE_OFFSET_SECS * self.sample_rate as f64).round() as i64;
        self.beats.iter().map(|p| p + shift).collect()
    }

    pub fn to_str_analysis(&self) -> strfile::Analysis {
        let beats = self.device_beats();
        strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: beats.first().copied().unwrap_or(0),
            beats: beats
                .iter()
                .zip(&self.beat_bpm)
                .map(|(&pos, &bpm)| strfile::Beat { pos, bpm })
                .collect(),
            first_downbeat: self.first_downbeat,
        }
    }
}

pub fn analyze_file(path: &Path) -> std::result::Result<Result, decode::Error> {
    let d = decode::decode(path)?;
    Ok(analyze(&d.mono, d.sample_rate, d.tag_bpm))
}

pub fn analyze(mono: &[f32], sample_rate: u32, tag_bpm: Option<f64>) -> Result {
    analyze_with(mono, sample_rate, tag_bpm, tempo::BpmRange::default())
}

pub fn analyze_with(mono: &[f32], sample_rate: u32, tag_bpm: Option<f64>, range: tempo::BpmRange) -> Result {
    let duration = mono.len() as f64 / sample_rate as f64;
    let env = onset::envelope(mono, sample_rate);
    let fps = env.frames_per_sec();
    // Detect in a full-time internal range so drum & bass is ~174 while the triplet-alias
    // check runs (at 87 its 4:3 sibling, 116, collects real breakbeat energy and wins); the
    // user's range is applied only to the final result.
    let candidates = tempo::candidates(&env.odf, fps, CANDIDATES, DETECT_RANGE);
    let locked = grid::lock(&env, &candidates, duration, 2.0 * DETECT_RANGE.lo, true);
    // the beat tracker runs at the chosen tempo; how far its beats stray from the locked grid
    // decides whether the track really has a constant tempo
    let period_frames = locked.as_ref().map(|g| g.period * fps).unwrap_or(fps * 60.0 / 120.0);
    let tracked = beats::track(&env.odf, period_frames, TIGHTNESS);
    let times = grid::refine_times(&env, &tracked);
    let fit = locked.map(|g| {
        let (rms, outside) = grid::residual(&g, &times);
        (g, rms, outside)
    });

    let (beat_times, beat_bpm, bpm, locked, snapped, resid) = match fit {
        Some((g, rms, outside)) if rms <= VARIABLE_TEMPO_MS && outside <= MAX_OFF_GRID => {
            let g = grid::fold_to_range(&env, &g, duration, range);
            let t = grid::grid_times(&g, duration);
            let n = t.len();
            (t, vec![g.bpm(); n], g.bpm(), true, g.snapped, rms)
        }
        other => {
            let mut bpms: Vec<f64> = times.windows(2).map(|w| 60.0 / (w[1] - w[0])).collect();
            if let Some(&last) = bpms.last() {
                bpms.push(last);
            }
            let median = {
                let mut s = bpms.clone();
                s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                s.get(s.len() / 2).copied().unwrap_or(0.0)
            };
            let resid = other.map(|(_, rms, _)| rms).unwrap_or(f64::NAN);
            let (mut times, mut bpms, mut median) = (times.clone(), bpms, median);
            while median >= 2.0 * range.lo && times.len() >= 4 {
                times = grid::stronger_parity(&env, &times);
                bpms = times.windows(2).map(|w| 60.0 / (w[1] - w[0])).collect();
                if let Some(&last) = bpms.last() {
                    bpms.push(last);
                }
                median /= 2.0;
            }
            (times, bpms, median, false, false, resid)
        }
    };
    let first_downbeat = (beat_times.len() >= 4).then(|| grid::downbeat_phase(&env, &beat_times));
    Result {
        bpm,
        beats: beat_times
            .iter()
            .map(|t| ((t + ONSET_LEAD_SECS) * sample_rate as f64).round() as i64)
            .collect(),
        beat_bpm,
        first_downbeat,
        sample_rate,
        duration_secs: duration,
        locked_grid: locked,
        snapped,
        residual_ms: resid,
        tag_bpm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic four-on-the-floor click track at a known tempo must come back exact.
    #[test]
    fn click_track_tempo_and_grid() {
        let sr = 44100u32;
        let bpm = 126.0;
        let secs = 60.0;
        let n = (sr as f64 * secs) as usize;
        let period = 60.0 / bpm;
        let offset = 0.137;
        let mut mono = vec![0f32; n];
        let mut k = 0;
        loop {
            let t = offset + k as f64 * period;
            let start = (t * sr as f64) as usize;
            if start >= n {
                break;
            }
            // smoothly decaying 55 Hz kick with a 2 ms attack transient (no cut-off click)
            for i in 0..(0.4 * sr as f64) as usize {
                if start + i < n {
                    let tt = i as f32 / sr as f32;
                    let click = if tt < 0.002 { 0.3 * (1.0 - tt / 0.002) * if i % 2 == 0 { 1.0 } else { -1.0 } } else { 0.0 };
                    mono[start + i] += (2.0 * std::f32::consts::PI * 55.0 * tt).sin() * (-tt * 30.0).exp() + click;
                }
            }
            k += 1;
        }
        let r = analyze(&mono, sr, None);
        assert!((r.bpm - bpm).abs() < 0.01, "bpm {}", r.bpm);
        assert!(r.locked_grid);
        let first = r.beats[0] as f64 / sr as f64;
        let err = ((first - offset) / period).fract().abs() * period;
        assert!(err.min(period - err) < 0.003, "phase error {err}");
    }
}
