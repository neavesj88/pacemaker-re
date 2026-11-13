//! Energy estimate, 1 to 10, from decoded audio (owner: analysis area).
//!
//! "Energy" is how hard a track pushes a dance floor. There is no ground truth to fit, so this is
//! a transparent heuristic built from four documented features, each measured from the audio and
//! then combined with fixed weights:
//!
//! * **Loudness over time**: the mean, over every frame above a -55 dBFS gate, of the frame's
//!   RMS in dB (unweighted, full band). Averaging in dB lets quiet breakdowns pull a track down,
//!   which a single peak or RMS figure would hide. Silence at either end is gated out.
//! * **Onset density**: attacks per second between the first and last active frame. A frame is an onset when, in one of
//!   three bands (30-150 Hz, 150-2000 Hz, 2-10 kHz), its level rose clearly (a per-band dB
//!   threshold) against two frames earlier, and it is the largest such rise within about 60 ms.
//!   Rises are measured in dB, so the count does not depend on how loud the file is.
//! * **Low-end weight**: the share of spectral power below 150 Hz, over the active frames.
//! * **Tempo**: BPM, folded down once if above 185 (a double-time mis-read), mapped 80 to 160.
//!   It is an input rather than a measurement here: the caller passes the library's BPM, so a
//!   BPM edit changes the score without decoding the file again.
//!
//! [`analyze`] measures the three audio features once ([`Features`], the part worth caching);
//! [`score`] combines them with the tempo into 0..1; [`Anchors`] turn a score into the 1..10
//! scale, either with fixed anchors or fitted to a library's own spread. Everything is
//! deterministic: no randomness, a fixed analysis order, the same result for the same samples.
//!
//! The ranges below come from reasoning about commercial dance music plus synthetic reference
//! signals (see the tests), not from a labelled library: treat the numbers as a consistent
//! relative scale, and use [`fit_anchors`] when a library sits in a narrow band.

use std::path::Path;

use rustfft::{num_complex::Complex, FftPlanner};

use crate::decode;

/// Bump when the features' definition changes, so cached results are recomputed.
pub const VERSION: u32 = 1;
/// Shorter audio than this gives no estimate at all (too little to measure onsets or loudness).
pub const MIN_SECS: f64 = 5.0;

const FRAME: usize = 2048;
const HOP: usize = 512;
/// Frames quieter than this (mean square, dBFS) are silence for every feature.
const GATE_DB: f32 = -55.0;
/// Band levels are floored here so a rise out of digital silence is not an onset.
const BAND_FLOOR_DB: f32 = -70.0;
const LOW_HZ: (f32, f32) = (20.0, 150.0);
const MID_HZ: (f32, f32) = (150.0, 2000.0);
const HIGH_HZ: (f32, f32) = (2000.0, 10000.0);
/// dB a band must rise (against two frames earlier) to count toward an onset, low/mid/high.
/// The low band spans few FFT bins so it fluctuates more by chance and needs the larger margin.
const RISE_DB: [f32; 3] = [9.0, 7.0, 7.0];
/// Seconds within which only the strongest rise counts as the onset.
const ONSET_SPACING_SECS: f32 = 0.06;

// feature -> 0..1 ranges and weights of the score
const LOUD_RANGE: (f32, f32) = (-28.0, -9.0);
const ONSET_RANGE: (f32, f32) = (0.5, 6.0);
const LOW_RANGE: (f32, f32) = (0.15, 0.75);
const TEMPO_RANGE: (f32, f32) = (80.0, 160.0);
const W_LOUD: f32 = 0.35;
const W_ONSET: f32 = 0.30;
const W_LOW: f32 = 0.15;
const W_TEMPO: f32 = 0.20;

/// What the audio itself says; independent of the BPM and of the scale in use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Features {
    pub duration_secs: f32,
    /// Mean frame level in dBFS over the active frames (about -30 quiet, -8 very loud).
    pub loudness_db: f32,
    /// Onsets per second, from the first to the last active frame.
    pub onsets_per_sec: f32,
    /// Share of spectral power under 150 Hz, 0 to 1.
    pub low_end: f32,
}

/// Why no estimate could be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    TooShort,
    Silent,
}

impl Skip {
    pub fn reason(self) -> &'static str {
        match self {
            Skip::TooShort => "too short for an energy estimate",
            Skip::Silent => "silent",
        }
    }
}

fn db(x: f32) -> f32 {
    10.0 * x.max(1e-12).log10()
}

fn unit(x: f32, (lo, hi): (f32, f32)) -> f32 {
    ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Measures [`Features`] from mono samples.
pub fn analyze(mono: &[f32], sample_rate: u32) -> Result<Features, Skip> {
    let sr = sample_rate.max(1) as f32;
    let duration = mono.len() as f32 / sr;
    if (duration as f64) < MIN_SECS || mono.len() < FRAME {
        return Err(Skip::TooShort);
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos()).collect();
    let win_power: f32 = window.iter().map(|w| w * w).sum();
    let hz = sr / FRAME as f32;
    let nyquist_bin = FRAME / 2;
    let bins = |(lo, hi): (f32, f32)| ((lo / hz).ceil().max(1.0) as usize, ((hi / hz).floor() as usize).min(nyquist_bin));
    let ranges = [bins(LOW_HZ), bins(MID_HZ), bins(HIGH_HZ)];

    let n_frames = (mono.len() - FRAME) / HOP + 1;
    let mut buf = vec![Complex::new(0f32, 0f32); FRAME];
    // per frame: whether it is active, and the three band levels in dB
    let mut active = Vec::with_capacity(n_frames);
    let mut band_db = Vec::with_capacity(n_frames);
    let (mut level_sum, mut active_n) = (0f64, 0usize);
    let (mut low_power, mut total_power) = (0f64, 0f64);
    for f in 0..n_frames {
        let start = f * HOP;
        let frame = &mono[start..start + FRAME];
        let ms = frame.iter().map(|s| s * s).sum::<f32>() / FRAME as f32;
        let level = db(ms);
        let is_active = level >= GATE_DB;
        for (c, (s, w)) in buf.iter_mut().zip(frame.iter().zip(&window)) {
            *c = Complex::new(s * w, 0.0);
        }
        fft.process(&mut buf);
        let mut p = [0f32; 3];
        for (band, &(lo, hi)) in ranges.iter().enumerate() {
            p[band] = buf[lo.min(nyquist_bin)..=hi.max(lo).min(nyquist_bin)].iter().map(|c| c.norm_sqr()).sum::<f32>() / win_power;
        }
        if is_active {
            level_sum += level as f64;
            active_n += 1;
            low_power += p[0] as f64;
            total_power += (p[0] + p[1] + p[2]) as f64;
        }
        active.push(is_active);
        band_db.push([db(p[0]).max(BAND_FLOOR_DB), db(p[1]).max(BAND_FLOOR_DB), db(p[2]).max(BAND_FLOOR_DB)]);
    }
    // under two seconds of sound is not a track to rate
    if (active_n as f32) * HOP as f32 / sr < 2.0 {
        return Err(Skip::Silent);
    }

    // onset strength per frame: summed excess rise over the per-band threshold
    let mut strength = vec![0f32; n_frames];
    for f in 2..n_frames {
        if !active[f] {
            continue;
        }
        strength[f] = (0..3).map(|b| ((band_db[f][b] - band_db[f - 2][b]) - RISE_DB[b]).max(0.0)).sum();
    }
    let reach = ((ONSET_SPACING_SECS * sr / HOP as f32).round() as usize).max(1);
    let mut onsets = 0usize;
    for f in 0..n_frames {
        let s = strength[f];
        if s <= 0.0 {
            continue;
        }
        let (a, b) = (f.saturating_sub(reach), (f + reach + 1).min(n_frames));
        // the strongest in its neighbourhood; on a tie the earlier frame wins
        let top = (a..b).all(|k| strength[k] < s || (strength[k] == s && k >= f));
        if top {
            onsets += 1;
        }
    }
    // density is per second between the first and last active frame, so silence at either end
    // does not dilute it while gaps inside the track still do
    let first = active.iter().position(|&a| a).unwrap_or(0);
    let last = active.iter().rposition(|&a| a).unwrap_or(0);
    let span_secs = ((last - first + 1) * HOP) as f32 / sr;
    Ok(Features {
        duration_secs: duration,
        loudness_db: (level_sum / active_n as f64) as f32,
        onsets_per_sec: onsets as f32 / span_secs,
        low_end: if total_power > 0.0 { (low_power / total_power) as f32 } else { 0.0 },
    })
}

/// Decodes `path` and measures it. The outer error is a file that cannot be decoded; the inner
/// one is audio that decoded but gives no estimate.
pub fn analyze_file(path: &Path) -> Result<Result<Features, Skip>, decode::Error> {
    let d = decode::decode_limited(path, decode::ANALYSIS_MAX_MINUTES)?;
    Ok(analyze(&d.mono, d.sample_rate))
}

/// The 0..1 score: a fixed weighted mean of the four features, each first mapped to 0..1. A
/// missing or non-positive `bpm` leaves the tempo out and shares its weight among the rest.
pub fn score(f: &Features, bpm: Option<f64>) -> f32 {
    let mut parts = vec![
        (W_LOUD, unit(f.loudness_db, LOUD_RANGE)),
        (W_ONSET, unit(f.onsets_per_sec, ONSET_RANGE)),
        (W_LOW, unit(f.low_end, LOW_RANGE)),
    ];
    if let Some(b) = bpm.filter(|b| b.is_finite() && *b > 0.0) {
        let mut b = b as f32;
        if b > 185.0 {
            b /= 2.0;
        }
        parts.push((W_TEMPO, unit(b, TEMPO_RANGE)));
    }
    let weight: f32 = parts.iter().map(|p| p.0).sum();
    parts.iter().map(|(w, x)| w * x).sum::<f32>() / weight
}

/// Where scores 0..1 land on the 1..10 scale: `lo` and below is 1, `hi` and above is 10, linear
/// between.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchors {
    pub lo: f32,
    pub hi: f32,
}

impl Anchors {
    /// Chosen so the synthetic references in the tests (a soft pad, a mid-tempo groove, a hard
    /// fast kick-and-hat track) span roughly 1 to 10. Not validated on a real library.
    pub const FIXED: Anchors = Anchors { lo: 0.18, hi: 0.78 };

    /// The energy, 1.0 to 10.0.
    pub fn energy(self, score: f32) -> f32 {
        1.0 + 9.0 * ((score - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0)
    }
}

/// Fewest scored tracks a library fit is attempted on; below it the spread says nothing.
pub const MIN_FIT_TRACKS: usize = 20;
/// Narrowest score spread worth stretching to 1..10; a library this uniform keeps fixed anchors.
const MIN_FIT_SPAN: f32 = 0.08;

/// Anchors that put the `low_pct`th and `high_pct`th percentile of `scores` at 1 and 10, so a
/// library spreads across the whole scale whatever its style. `None` when there are too few
/// scores or they are nearly all the same.
pub fn fit_anchors(scores: &[f32], low_pct: f32, high_pct: f32) -> Option<Anchors> {
    if scores.len() < MIN_FIT_TRACKS || !(0.0..high_pct).contains(&low_pct) || high_pct > 100.0 {
        return None;
    }
    let mut s: Vec<f32> = scores.iter().copied().filter(|x| x.is_finite()).collect();
    if s.len() < MIN_FIT_TRACKS {
        return None;
    }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |pct: f32| {
        let pos = pct / 100.0 * (s.len() - 1) as f32;
        let (i, frac) = (pos.floor() as usize, pos.fract());
        let next = (i + 1).min(s.len() - 1);
        s[i] + (s[next] - s[i]) * frac
    };
    let (lo, hi) = (at(low_pct), at(high_pct));
    (hi - lo >= MIN_FIT_SPAN).then_some(Anchors { lo, hi })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44100;

    /// Small deterministic noise source (xorshift), so tests never depend on a clock or seed.
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        }
    }

    fn secs(n: f32) -> usize {
        (n * SR as f32) as usize
    }

    /// A sustained chord pad: no attacks at all.
    fn pad(len: f32, amp: f32) -> Vec<f32> {
        (0..secs(len))
            .map(|i| {
                let t = i as f32 / SR as f32;
                let s: f32 = [220.0, 277.2, 329.6].iter().map(|f| (2.0 * std::f32::consts::PI * f * t).sin()).sum();
                amp * s / 3.0
            })
            .collect()
    }

    fn add_hit(out: &mut [f32], at: usize, len: usize, f: impl Fn(usize) -> f32) {
        for i in 0..len {
            if at + i < out.len() {
                out[at + i] += f(i);
            }
        }
    }

    /// A kick train at `bpm` (one per beat), optionally with off-beat hats and a bass line.
    fn groove(len: f32, bpm: f32, kick_amp: f32, hats: f32, bass: f32) -> Vec<f32> {
        let mut out = vec![0f32; secs(len)];
        let beat = SR as f32 * 60.0 / bpm;
        let mut noise = Noise(0x9e37_79b9_7f4a_7c15);
        let mut k = 0usize;
        loop {
            let at = (k as f32 * beat) as usize;
            if at >= out.len() {
                break;
            }
            // pitch-dropping kick, 60 Hz body, faded out so its end is not a click
            let kick_len = secs(0.3);
            add_hit(&mut out, at, kick_len, |i| {
                let t = i as f32 / SR as f32;
                let fade = (1.0 - i as f32 / kick_len as f32).powi(2);
                kick_amp * (2.0 * std::f32::consts::PI * (55.0 * t + 40.0 * (1.0 - (-t * 30.0).exp()) / 30.0)).sin() * (-t * 12.0).exp() * fade
            });
            if hats > 0.0 {
                let off = at + (beat / 2.0) as usize;
                let hat_noise: Vec<f32> = (0..secs(0.05)).map(|_| noise.next()).collect();
                add_hit(&mut out, off, hat_noise.len(), |i| hats * hat_noise[i] * (-(i as f32) / SR as f32 * 90.0).exp());
            }
            k += 1;
        }
        if bass > 0.0 {
            for (i, s) in out.iter_mut().enumerate() {
                *s += bass * (2.0 * std::f32::consts::PI * 49.0 * i as f32 / SR as f32).sin();
            }
        }
        out
    }

    fn feats(mono: &[f32]) -> Features {
        analyze(mono, SR).expect("a measurable signal")
    }

    #[test]
    fn short_and_silent_audio_give_no_estimate() {
        assert_eq!(analyze(&vec![0.3; secs(2.0)], SR), Err(Skip::TooShort));
        assert_eq!(analyze(&[], SR), Err(Skip::TooShort));
        assert_eq!(analyze(&vec![0.0; secs(12.0)], SR), Err(Skip::Silent));
        // faint hiss far below the gate counts as silence too
        let mut n = Noise(7);
        assert_eq!(analyze(&(0..secs(12.0)).map(|_| n.next() * 0.0005).collect::<Vec<_>>(), SR), Err(Skip::Silent));
    }

    #[test]
    fn a_sustained_pad_has_no_onsets() {
        let f = feats(&pad(20.0, 0.3));
        assert!(f.onsets_per_sec < 0.2, "{f:?}");
        assert!((f.duration_secs - 20.0).abs() < 0.01);
    }

    #[test]
    fn steady_noise_is_not_mistaken_for_attacks() {
        let mut n = Noise(42);
        let noise: Vec<f32> = (0..secs(20.0)).map(|_| n.next() * 0.2).collect();
        let f = feats(&noise);
        assert!(f.onsets_per_sec < 0.5, "{f:?}");
    }

    #[test]
    fn onset_density_follows_the_pattern_not_the_level() {
        let slow = feats(&groove(30.0, 120.0, 0.8, 0.0, 0.0));
        let busy = feats(&groove(30.0, 120.0, 0.8, 0.5, 0.0));
        // one kick per beat at 120 BPM is 2 per second
        assert!((1.6..=2.4).contains(&slow.onsets_per_sec), "{slow:?}");
        // hats on the off-beats double it
        assert!(busy.onsets_per_sec > slow.onsets_per_sec * 1.6, "{busy:?} vs {slow:?}");
        // the same pattern 20 dB quieter has the same number of attacks
        let quiet = feats(&groove(30.0, 120.0, 0.08, 0.0, 0.0));
        assert!((quiet.onsets_per_sec - slow.onsets_per_sec).abs() < 0.3, "{quiet:?} vs {slow:?}");
        // the gate drops the quiet tail of each hit from the mean, so the gap is under the full 20 dB
        assert!(quiet.loudness_db < slow.loudness_db - 8.0, "{quiet:?} vs {slow:?}");
    }

    #[test]
    fn low_end_weight_separates_bass_from_treble() {
        let mut bassy = pad(15.0, 0.0);
        let mut bright = bassy.clone();
        for (i, (a, b)) in bassy.iter_mut().zip(bright.iter_mut()).enumerate() {
            let t = i as f32 / SR as f32;
            *a = 0.3 * (2.0 * std::f32::consts::PI * 60.0 * t).sin() + 0.02 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
            *b = 0.3 * (2.0 * std::f32::consts::PI * 3000.0 * t).sin() + 0.02 * (2.0 * std::f32::consts::PI * 60.0 * t).sin();
        }
        assert!(feats(&bassy).low_end > 0.9);
        assert!(feats(&bright).low_end < 0.05);
    }

    #[test]
    fn loudness_tracks_level_and_breakdowns_pull_it_down() {
        let loud = feats(&pad(20.0, 0.5));
        let soft = feats(&pad(20.0, 0.05));
        assert!(loud.loudness_db > soft.loudness_db + 15.0);
        // half the time at full level, half 20 dB down: the mean sits between
        let mut mixed = pad(20.0, 0.5);
        let half = mixed.len() / 2;
        mixed[half..].iter_mut().for_each(|s| *s *= 0.1);
        let m = feats(&mixed);
        assert!(m.loudness_db < loud.loudness_db - 5.0 && m.loudness_db > soft.loudness_db, "{m:?}");
        // leading silence does not count against a track
        let mut padded = vec![0f32; secs(10.0)];
        padded.extend(pad(20.0, 0.5));
        assert!((feats(&padded).loudness_db - loud.loudness_db).abs() < 0.5);
    }

    #[test]
    fn analysis_is_deterministic() {
        let g = groove(20.0, 128.0, 0.7, 0.4, 0.1);
        assert_eq!(feats(&g), feats(&g));
        assert_eq!(score(&feats(&g), Some(128.0)), score(&feats(&g), Some(128.0)));
    }

    /// The reference ladder the fixed anchors are set against: ordering must hold and the
    /// ends must reach the ends of the scale.
    #[test]
    fn reference_signals_climb_the_scale() {
        let soft_pad = (feats(&pad(30.0, 0.04)), Some(70.0));
        let mellow = (feats(&groove(30.0, 100.0, 0.25, 0.0, 0.0)), Some(100.0));
        let groove_mid = (feats(&groove(30.0, 124.0, 0.6, 0.25, 0.1)), Some(124.0));
        let hard_fast = (feats(&groove(30.0, 150.0, 0.95, 0.7, 0.25)), Some(150.0));
        let levels: Vec<f32> = [soft_pad, mellow, groove_mid, hard_fast].iter().map(|(f, bpm)| Anchors::FIXED.energy(score(f, *bpm))).collect();
        eprintln!("reference ladder: {levels:?}");
        assert!(levels.windows(2).all(|w| w[0] < w[1]), "{levels:?}");
        assert!(levels[0] <= 2.0, "{levels:?}");
        assert!(levels[3] >= 8.5, "{levels:?}");
    }

    #[test]
    fn score_is_monotonic_in_each_feature() {
        let base = Features { duration_secs: 200.0, loudness_db: -18.0, onsets_per_sec: 3.0, low_end: 0.4 };
        let s0 = score(&base, Some(120.0));
        assert!(score(&Features { loudness_db: -12.0, ..base }, Some(120.0)) > s0);
        assert!(score(&Features { onsets_per_sec: 5.0, ..base }, Some(120.0)) > s0);
        assert!(score(&Features { low_end: 0.6, ..base }, Some(120.0)) > s0);
        assert!(score(&base, Some(140.0)) > s0);
        assert!((0.0..=1.0).contains(&s0));
        // extremes stay inside 0..1
        let top = Features { duration_secs: 1.0, loudness_db: 0.0, onsets_per_sec: 99.0, low_end: 1.0 };
        let bottom = Features { duration_secs: 1.0, loudness_db: -80.0, onsets_per_sec: 0.0, low_end: 0.0 };
        assert_eq!(score(&top, Some(160.0)), 1.0);
        assert_eq!(score(&bottom, Some(80.0)), 0.0);
    }

    #[test]
    fn tempo_is_optional_and_a_double_time_reading_is_folded() {
        let f = Features { duration_secs: 200.0, loudness_db: -14.0, onsets_per_sec: 3.0, low_end: 0.4 };
        // no usable tempo: the other three features carry the whole weight
        assert_eq!(score(&f, None), score(&f, Some(0.0)));
        assert_eq!(score(&f, None), score(&f, Some(f64::NAN)));
        assert!(score(&f, None) > 0.0);
        // 240 BPM is read as 120
        assert_eq!(score(&f, Some(240.0)), score(&f, Some(120.0)));
    }

    #[test]
    fn anchors_map_the_ends_and_the_middle() {
        let a = Anchors { lo: 0.2, hi: 0.8 };
        assert_eq!(a.energy(0.0), 1.0);
        assert_eq!(a.energy(0.2), 1.0);
        assert_eq!(a.energy(1.0), 10.0);
        assert!((a.energy(0.5) - 5.5).abs() < 1e-5);
    }

    #[test]
    fn a_fit_spreads_a_narrow_library_over_the_whole_scale() {
        // a library that sits in 0.40..0.60
        let scores: Vec<f32> = (0..200).map(|i| 0.40 + 0.20 * i as f32 / 199.0).collect();
        // the fixed scale puts all of them in the middle...
        let fixed: Vec<f32> = scores.iter().map(|&s| Anchors::FIXED.energy(s)).collect();
        assert!(fixed.iter().all(|e| (3.0..=7.5).contains(e)));
        // ...the fit uses the whole range, with about a tenth of tracks in each whole level
        let a = fit_anchors(&scores, 5.0, 95.0).expect("enough tracks");
        let levels: Vec<u32> = scores.iter().map(|&s| a.energy(s).round() as u32).collect();
        for lvl in 1..=10u32 {
            let n = levels.iter().filter(|&&l| l == lvl).count();
            assert!(n >= 8, "level {lvl} has {n} of 200: {levels:?}");
        }
        assert_eq!(*levels.iter().min().unwrap(), 1);
        assert_eq!(*levels.iter().max().unwrap(), 10);
    }

    #[test]
    fn a_fit_needs_enough_varied_tracks() {
        let few: Vec<f32> = (0..MIN_FIT_TRACKS - 1).map(|i| i as f32 / 40.0).collect();
        assert!(fit_anchors(&few, 5.0, 95.0).is_none());
        let flat = vec![0.5f32; 100];
        assert!(fit_anchors(&flat, 5.0, 95.0).is_none());
        let ok: Vec<f32> = (0..MIN_FIT_TRACKS).map(|i| i as f32 / MIN_FIT_TRACKS as f32).collect();
        assert!(fit_anchors(&ok, 5.0, 95.0).is_some());
        // nonsense percentiles are refused, not clamped
        assert!(fit_anchors(&ok, 60.0, 40.0).is_none());
        assert!(fit_anchors(&ok, -1.0, 95.0).is_none());
        assert!(fit_anchors(&ok, 5.0, 101.0).is_none());
        // NaNs are ignored
        let mut with_nan = ok.clone();
        with_nan.push(f32::NAN);
        assert_eq!(fit_anchors(&with_nan, 5.0, 95.0), fit_anchors(&ok, 5.0, 95.0));
    }

    #[test]
    fn works_at_other_sample_rates() {
        let n = 22050usize;
        let mut out = vec![0f32; n * 20];
        let beat = n / 2; // 120 BPM
        let mut k = 0;
        while k * beat < out.len() {
            for i in 0..(n / 4) {
                let t = i as f32 / n as f32;
                if k * beat + i < out.len() {
                    let fade = (1.0 - i as f32 / (n / 4) as f32).powi(2);
                    out[k * beat + i] += 0.8 * (2.0 * std::f32::consts::PI * 55.0 * t).sin() * (-t * 12.0).exp() * fade;
                }
            }
            k += 1;
        }
        let f = analyze(&out, 22050).unwrap();
        assert!((1.6..=2.4).contains(&f.onsets_per_sec), "{f:?}");
    }
}
