//! Track analysis for the Pacemaker: tempo, beat grid and first downbeat.
//!
//! Original implementation of published methods (spectral-flux onsets, autocorrelation tempo,
//! Ellis dynamic-programming beat tracking), followed by a locked-grid fit tuned for DJ music.
//! Output converts directly to the device's `.str` sidecar via [`Result::to_str_analysis`].

pub mod align;
pub mod beats;
pub mod decode;
pub mod fingerprint;
pub mod energy;
pub mod grid;
pub mod ingest;
pub mod key;
pub mod onset;
pub mod onsets;
pub mod peaks;
pub mod sections;
pub mod probe;
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

/// Analyses a file with the default BPM range ([`tempo::BpmRange::default`]).
pub fn analyze_file(path: &Path) -> std::result::Result<Result, decode::Error> {
    analyze_file_with(path, tempo::BpmRange::default())
}

pub fn analyze_file_with(path: &Path, range: tempo::BpmRange) -> std::result::Result<Result, decode::Error> {
    let d = decode::decode(path)?;
    Ok(analyze_with(&d.mono, d.sample_rate, d.tag_bpm, range))
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
            while median < range.lo && times.len() >= 2 {
                times = grid::double_times(&times);
                bpms = times.windows(2).map(|w| 60.0 / (w[1] - w[0])).collect();
                if let Some(&last) = bpms.last() {
                    bpms.push(last);
                }
                median *= 2.0;
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

    const SR: u32 = 22050;

    /// Deterministic noise in -1..1 (a tiny LCG) so the snare and hats are the same every run.
    fn noise(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (*state >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    fn add_kick(mono: &mut [f32], at: f64) {
        let start = (at * SR as f64) as usize;
        for i in 0..(0.3 * SR as f64) as usize {
            if let Some(v) = mono.get_mut(start + i) {
                let t = i as f32 / SR as f32;
                *v += 0.9 * (2.0 * std::f32::consts::PI * (50.0 + 60.0 * (-t * 40.0).exp()) * t).sin() * (-t * 18.0).exp();
            }
        }
    }

    fn add_noise_hit(mono: &mut [f32], at: f64, len: f32, decay: f32, level: f32, seed: &mut u32) {
        let start = (at * SR as f64) as usize;
        for i in 0..(len * SR as f32) as usize {
            if let Some(v) = mono.get_mut(start + i) {
                *v += level * noise(seed) * (-(i as f32 / SR as f32) * decay).exp();
            }
        }
    }

    /// A drum & bass break at `bpm` (beats per minute, e.g. 174): kick on beat 1 and the "and" of
    /// beat 2, snare on beats 2 and 4, eighth-note hats. 40 s, first beat at `offset`.
    fn dnb_break(bpm: f64, offset: f64) -> Vec<f32> {
        let beat = 60.0 / bpm;
        let mut mono = vec![0f32; (SR as f64 * 40.0) as usize];
        let mut seed = 7u32;
        let mut bar = 0;
        while offset + bar as f64 * 4.0 * beat < 40.0 {
            let t0 = offset + bar as f64 * 4.0 * beat;
            add_kick(&mut mono, t0);
            add_kick(&mut mono, t0 + 2.5 * beat);
            add_noise_hit(&mut mono, t0 + beat, 0.15, 25.0, 0.7, &mut seed);
            add_noise_hit(&mut mono, t0 + 3.0 * beat, 0.15, 25.0, 0.7, &mut seed);
            for h in 0..8 {
                add_noise_hit(&mut mono, t0 + h as f64 * 0.5 * beat, 0.03, 120.0, 0.15, &mut seed);
            }
            bar += 1;
        }
        mono
    }

    /// Four-on-the-floor house at `bpm`: kick every beat, open hat on the off-beats.
    fn house(bpm: f64, offset: f64) -> Vec<f32> {
        let beat = 60.0 / bpm;
        let mut mono = vec![0f32; (SR as f64 * 40.0) as usize];
        let mut seed = 11u32;
        let mut k = 0;
        while offset + k as f64 * beat < 40.0 {
            let t = offset + k as f64 * beat;
            add_kick(&mut mono, t);
            add_noise_hit(&mut mono, t + 0.5 * beat, 0.05, 80.0, 0.25, &mut seed);
            k += 1;
        }
        mono
    }

    /// The median spacing of the result's beats, as a tempo: the grid must agree with the number.
    fn grid_bpm(r: &Result) -> f64 {
        let mut gaps: Vec<f64> = r.beats.windows(2).map(|w| (w[1] - w[0]) as f64 / r.sample_rate as f64).collect();
        gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        60.0 / gaps[gaps.len() / 2]
    }

    #[test]
    fn drum_and_bass_follows_the_range_and_the_grid_agrees() {
        let mono = dnb_break(174.0, 0.21);
        for (range, want) in [
            (tempo::BpmRange::EDITOR_COMPATIBLE, 87.0),
            (tempo::BpmRange::DNB_FULL_TIME, 174.0),
            (tempo::BpmRange::FAST, 174.0),
            (tempo::BpmRange::SLOW, 87.0),
        ] {
            let r = analyze_with(&mono, SR, None, range);
            assert!((r.bpm - want).abs() < 0.6, "{}-{}: bpm {}", range.lo, range.hi(), r.bpm);
            assert!((grid_bpm(&r) - r.bpm).abs() < 0.6, "{}-{}: grid {} vs bpm {}", range.lo, range.hi(), grid_bpm(&r), r.bpm);
            // the beats lie on the kick of beat 1: every (174 / bpm * 4)th beat starts a bar
            let per_bar = (4.0 * 174.0 / want).round() as usize;
            assert_eq!(per_bar % 4, 0);
            let beat = 60.0 / 174.0;
            let on_kick = r.beats.iter().filter(|&&b| {
                let t = b as f64 / SR as f64 - 0.21;
                let x = (t / beat).rem_euclid(1.0);
                x.min(1.0 - x) * beat < 0.02
            });
            let n_on = on_kick.count();
            assert_eq!(n_on, r.beats.len(), "{}-{}: every beat sits on a 174 BPM beat; first beats {:?}", range.lo, range.hi(), &r.beats[..4.min(r.beats.len())]);
        }
    }

    #[test]
    fn house_stays_put_unless_the_range_cannot_hold_it() {
        let mono = house(128.0, 0.1);
        for (range, want) in [
            (tempo::BpmRange::EDITOR_COMPATIBLE, 128.0),
            (tempo::BpmRange::DNB_FULL_TIME, 128.0),
            (tempo::BpmRange::FAST, 128.0),
            (tempo::BpmRange::SLOW, 128.0),
        ] {
            let r = analyze_with(&mono, SR, None, range);
            assert!((r.bpm - want).abs() < 0.3, "{}-{}: bpm {}", range.lo, range.hi(), r.bpm);
            assert!((grid_bpm(&r) - r.bpm).abs() < 0.3, "{}-{}: grid {}", range.lo, range.hi(), grid_bpm(&r));
        }
    }

    #[test]
    fn the_default_range_reads_drum_and_bass_at_full_time() {
        let r = analyze(&dnb_break(172.0, 0.0), SR, None);
        assert!((r.bpm - 172.0).abs() < 0.6, "bpm {}", r.bpm);
        // and 130 next to it is untouched, which is what mixing the two needs
        let r = analyze(&house(130.0, 0.0), SR, None);
        assert!((r.bpm - 130.0).abs() < 0.3, "bpm {}", r.bpm);
    }

    #[test]
    fn the_unlocked_branch_doubles_up_into_a_faster_range() {
        let slow = [0.0, 0.7, 1.4, 2.1];
        let doubled = grid::double_times(&slow);
        let want = [0.0, 0.35, 0.7, 1.05, 1.4, 1.75, 2.1, 2.45];
        assert_eq!(doubled.len(), want.len());
        assert!(doubled.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9), "{doubled:?}");
        assert_eq!(grid::double_times(&[1.0]), vec![1.0]);
        assert!(grid::double_times(&[]).is_empty());
    }
}
