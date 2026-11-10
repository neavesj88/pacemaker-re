//! Musical key detection (owner: analysis area).
//!
//! Method (all published, nothing copied): a windowed FFT every ~0.2 s; spectral peaks between
//! 80 Hz and 4 kHz are refined by parabolic interpolation and folded into a 12-bin chromagram.
//! Tuning is estimated first from where the peaks sit relative to the semitone grid (a circular
//! mean of their offsets), so a track mastered at 432 Hz or pitched by a turntable still lands on
//! the right pitch classes. The accumulated chromagram is matched against the 24 rotations of two
//! key profiles (Krumhansl-Kessler 1990 and Temperley 2005) by Pearson correlation, and the two
//! scores are averaged.
//!
//! Output is the short notation the device's `tracks.key` (VARCHAR(4)) takes: "C", "F#m".
//! The profiles are the textbook ones; a profile tuned for dance music is a follow-up that needs
//! a supervised benchmark on tagged tracks (see notes).

use std::f32::consts::PI;
use std::path::Path;

use rustfft::{num_complex::Complex, FftPlanner};

use crate::decode;

/// One of the 24 keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Pitch class of the tonic, 0 = C ... 11 = B.
    pub tonic: u8,
    pub minor: bool,
}

const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

impl Key {
    /// "Am", "F#m", "C": sharps only, at most 3 characters.
    pub fn name(&self) -> String {
        format!("{}{}", NAMES[(self.tonic % 12) as usize], if self.minor { "m" } else { "" })
    }

    /// Parses "C", "Am", "F#m", "Bb", "Ebm", "A minor", "C major" (case of the note letter is
    /// ignored; flats accepted). `None` for anything else, including Camelot codes.
    pub fn parse(text: &str) -> Option<Key> {
        let t = text.trim();
        let mut ch = t.chars();
        let letter = ch.next()?.to_ascii_uppercase();
        let base = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest: &str = ch.as_str();
        let (shift, rest) = match rest.chars().next() {
            Some('#') => (1, &rest[1..]),
            Some('b') => (11, &rest[1..]),
            _ => (0, rest),
        };
        let rest = rest.trim().to_ascii_lowercase();
        let minor = match rest.as_str() {
            "" | "maj" | "major" => false,
            "m" | "min" | "minor" => true,
            _ => return None,
        };
        Some(Key { tonic: (base + shift) % 12, minor })
    }
}

/// A detected key with how sure the detector is.
#[derive(Debug, Clone, Copy)]
pub struct KeyResult {
    pub key: Key,
    /// 0..1: the fit of the best key, scaled down when a second key fits nearly as well.
    /// A heuristic for ranking and thresholds, not a probability.
    pub confidence: f32,
    /// Estimated tuning offset from A = 440 Hz in cents (-50..50).
    pub tuning_cents: f32,
}

const KK_MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const KK_MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
const TP_MAJOR: [f32; 12] = [5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0];
const TP_MINOR: [f32; 12] = [5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0];

const LOW_HZ: f32 = 80.0;
const HIGH_HZ: f32 = 4000.0;
/// Frames analysed per track at most; longer audio is sampled at a wider hop.
const MAX_FRAMES: usize = 2500;
/// Shorter audio has no stable key.
const MIN_SECS: f32 = 5.0;
/// Below this length of the mean tuning vector the peaks show no tuning (percussive or noisy
/// material), and 0 is assumed.
const MIN_TUNING_CLARITY: f32 = 0.08;

/// Detects the key of a mono signal. `None` when the audio is shorter than 5 s, silent or has no
/// tonal peaks at all.
pub fn detect(mono: &[f32], sample_rate: u32) -> Option<KeyResult> {
    let chroma_and_tuning = chromagram(mono, sample_rate)?;
    let (chroma, tuning) = chroma_and_tuning;
    let (key, confidence) = match_profiles(&chroma)?;
    Some(KeyResult { key, confidence, tuning_cents: tuning * 100.0 })
}

/// Decodes a file and detects its key.
pub fn detect_file(path: &Path) -> Result<Option<KeyResult>, decode::Error> {
    let d = decode::decode(path)?;
    Ok(detect(&d.mono, d.sample_rate))
}

/// A spectral peak: fractional MIDI note number and compressed magnitude.
struct Peak {
    midi: f32,
    weight: f32,
}

fn frame_len(sample_rate: u32) -> usize {
    // about 0.37 s: 2.7 Hz bins at 44.1 kHz, narrower than a semitone down to ~80 Hz
    ((0.37 * sample_rate as f32) as usize).next_power_of_two().clamp(4096, 32768)
}

fn chromagram(mono: &[f32], sample_rate: u32) -> Option<([f32; 12], f32)> {
    let n = frame_len(sample_rate);
    if (mono.len() as f32) < MIN_SECS * sample_rate as f32 || mono.len() < n {
        return None;
    }
    let hop = (n / 2).max((mono.len() - n) / MAX_FRAMES);
    let fft = FftPlanner::<f32>::new().plan_fft_forward(n);
    let window: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos()).collect();
    let sr = sample_rate as f32;
    let kmin = ((LOW_HZ * n as f32 / sr).ceil() as usize).max(2);
    let kmax = ((HIGH_HZ.min(0.45 * sr) * n as f32 / sr).floor() as usize).min(n / 2 - 3);

    let mut buf = vec![Complex::new(0.0f32, 0.0); n];
    let mut mag = vec![0f32; n / 2];
    let mut prefix = vec![0f32; n / 2 + 1];
    let mut frames: Vec<Vec<Peak>> = Vec::new();
    let mut start = 0;
    while start + n <= mono.len() {
        let seg = &mono[start..start + n];
        start += hop;
        let energy: f32 = seg.iter().map(|s| s * s).sum::<f32>() / n as f32;
        if energy < 1e-8 {
            continue; // digital silence or near it: its noise must not vote
        }
        for i in 0..n {
            buf[i] = Complex::new(seg[i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        for k in 0..n / 2 {
            mag[k] = buf[k].norm();
            prefix[k + 1] = prefix[k] + mag[k];
        }
        let mut peaks = Vec::new();
        for k in kmin..=kmax {
            let m = mag[k];
            if m <= mag[k - 1] || m < mag[k + 1] {
                continue;
            }
            // a tonal peak stands well above its neighbourhood; broadband noise and drum hits do not
            let lo = k.saturating_sub(20);
            let hi = (k + 21).min(n / 2);
            let mean = (prefix[hi] - prefix[lo]) / (hi - lo) as f32;
            if m < 3.0 * mean {
                continue;
            }
            let (a, b, c) = (mag[k - 1].max(1e-12).ln(), m.max(1e-12).ln(), mag[k + 1].max(1e-12).ln());
            let denom = a - 2.0 * b + c;
            let delta = if denom < -1e-9 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
            let freq = (k as f32 + delta) * sr / n as f32;
            peaks.push(Peak { midi: 69.0 + 12.0 * (freq / 440.0).log2(), weight: m.sqrt() });
        }
        if !peaks.is_empty() {
            frames.push(peaks);
        }
    }
    if frames.is_empty() {
        return None;
    }

    // tuning: circular mean of every peak's offset from the nearest semitone
    let (mut sx, mut sy, mut sw) = (0f64, 0f64, 0f64);
    for p in frames.iter().flatten() {
        let a = 2.0 * std::f64::consts::PI * (p.midi - p.midi.round()) as f64;
        sx += p.weight as f64 * a.cos();
        sy += p.weight as f64 * a.sin();
        sw += p.weight as f64;
    }
    let clarity = (sx.hypot(sy) / sw.max(1e-12)) as f32;
    let tuning = if clarity >= MIN_TUNING_CLARITY {
        (sy.atan2(sx) / (2.0 * std::f64::consts::PI)) as f32
    } else {
        0.0
    };

    let mut chroma = [0f32; 12];
    for p in frames.iter().flatten() {
        let m = p.midi - tuning;
        let r = m.round();
        // a peak midway between two semitones is ambiguous and counts for little
        let closeness = 0.5 + 0.5 * (2.0 * PI * (m - r)).cos();
        chroma[(r as i32).rem_euclid(12) as usize] += p.weight * closeness * closeness;
    }
    (chroma.iter().sum::<f32>() > 0.0).then_some((chroma, tuning))
}

fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let (ma, mb) = (a.iter().sum::<f32>() / 12.0, b.iter().sum::<f32>() / 12.0);
    let (mut num, mut da, mut db) = (0f32, 0f32, 0f32);
    for i in 0..12 {
        let (x, y) = (a[i] - ma, b[i] - mb);
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 0.0 || db <= 0.0 { 0.0 } else { num / (da * db).sqrt() }
}

fn rotated(profile: &[f32; 12], tonic: usize) -> [f32; 12] {
    let mut out = [0f32; 12];
    for (i, v) in profile.iter().enumerate() {
        out[(tonic + i) % 12] = *v;
    }
    out
}

/// Scores every key against the chromagram: `(key, score)` for all 24, best first.
pub fn rank(chroma: &[f32; 12]) -> Vec<(Key, f32)> {
    let mut scores = Vec::with_capacity(24);
    for minor in [false, true] {
        let (kk, tp) = if minor { (&KK_MINOR, &TP_MINOR) } else { (&KK_MAJOR, &TP_MAJOR) };
        for tonic in 0..12 {
            let s = 0.5 * (pearson(chroma, &rotated(kk, tonic)) + pearson(chroma, &rotated(tp, tonic)));
            scores.push((Key { tonic: tonic as u8, minor }, s));
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    scores
}

fn match_profiles(chroma: &[f32; 12]) -> Option<(Key, f32)> {
    let scores = rank(chroma);
    let (best, r1) = scores[0];
    let r2 = scores[1].1;
    if !r1.is_finite() || r1 <= 0.0 {
        return None;
    }
    // a relative or parallel key can legitimately sit close behind the winner, so the margin
    // only discounts the fit; it never zeroes it
    let margin = ((r1 - r2) / 0.08).clamp(0.0, 1.0);
    Some((best, r1.clamp(0.0, 1.0) * (0.5 + 0.5 * margin)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 22050;

    #[test]
    fn names_round_trip_and_fit_the_column() {
        for tonic in 0..12 {
            for minor in [false, true] {
                let k = Key { tonic, minor };
                assert!(k.name().chars().count() <= 4);
                assert_eq!(Key::parse(&k.name()), Some(k));
            }
        }
        assert_eq!(Key { tonic: 6, minor: true }.name(), "F#m");
        assert_eq!(Key::parse("Bb"), Some(Key { tonic: 10, minor: false }));
        assert_eq!(Key::parse("ebm"), Some(Key { tonic: 3, minor: true }));
        assert_eq!(Key::parse("A minor"), Some(Key { tonic: 9, minor: true }));
        assert_eq!(Key::parse("8A"), None);
        assert_eq!(Key::parse(""), None);
    }

    /// Deterministic noise so tests do not depend on a rand crate.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    /// Chord progression in `key`: semitone offsets above the tonic per chord.
    fn progression(minor: bool) -> Vec<[i32; 3]> {
        if minor {
            // i, iv, V (harmonic minor), i, then i, VI, III, VII
            vec![[0, 3, 7], [5, 8, 12], [7, 11, 14], [0, 3, 7], [0, 3, 7], [8, 12, 15], [3, 7, 10], [10, 14, 17]]
        } else {
            // I, IV, V, I, then I, vi, IV, V
            vec![[0, 4, 7], [5, 9, 12], [7, 11, 14], [0, 4, 7], [0, 4, 7], [9, 12, 16], [5, 9, 12], [7, 11, 14]]
        }
    }

    /// Additive-synth chords (four decaying harmonics, plus a bass root) with light noise.
    fn render(key: Key, cents: f32, secs_per_chord: f32, noise: f32) -> Vec<f32> {
        let mut rng = Lcg(42 + key.tonic as u64);
        let mut out = Vec::new();
        for chord in progression(key.minor) {
            let n = (secs_per_chord * SR as f32) as usize;
            let mut seg = vec![0f32; n];
            let notes = chord.iter().map(|o| (60 + key.tonic as i32 + o, 0.5)).chain([(36 + key.tonic as i32, 0.7)]);
            for (midi, amp) in notes {
                let f0 = 440.0 * 2f32.powf((midi as f32 - 69.0 + cents / 100.0) / 12.0);
                for h in 1..=4 {
                    let f = f0 * h as f32;
                    let a = amp / h as f32;
                    let w = 2.0 * PI * f / SR as f32;
                    for (i, s) in seg.iter_mut().enumerate() {
                        *s += a * (w * i as f32).sin();
                    }
                }
            }
            let peak = seg.iter().fold(0f32, |m, s| m.max(s.abs())).max(1e-6);
            for (i, s) in seg.iter_mut().enumerate() {
                // short fade at each chord edge so the chord change has no click
                let edge = (i.min(n - 1 - i) as f32 / 400.0).min(1.0);
                *s = 0.6 * edge * *s / peak + noise * rng.next();
            }
            out.extend(seg);
        }
        out
    }

    fn accuracy(cents: f32, noise: f32, step: usize) -> (usize, Vec<String>) {
        let mut correct = 0;
        let mut misses = Vec::new();
        for minor in [false, true] {
            for tonic in (0..12).step_by(step) {
                let key = Key { tonic, minor };
                let audio = render(key, cents, 1.5, noise);
                let got = detect(&audio, SR).expect("tonal audio must give a key");
                if got.key == key {
                    correct += 1;
                } else {
                    misses.push(format!("{} detected as {}", key.name(), got.key.name()));
                }
            }
        }
        (correct, misses)
    }

    #[test]
    fn synthetic_progressions_in_all_24_keys() {
        let (correct, misses) = accuracy(0.0, 0.02, 1);
        eprintln!("key accuracy, A440, light noise: {correct} {misses:?}");
        assert!(correct >= 23, "{correct}, misses: {misses:?}");
    }

    #[test]
    fn detuned_audio_keeps_its_key() {
        // a quarter-tone-ish drift either way (+-35 cents) must not move pitch classes
        for cents in [-35.0, 35.0] {
            let (correct, misses) = accuracy(cents, 0.02, 3);
            eprintln!("key accuracy at {cents:+} cents: {correct} {misses:?}");
            assert!(correct >= 7, "{cents} cents: {correct}, misses: {misses:?}");
        }
    }

    #[test]
    fn tuning_offset_is_reported() {
        let audio = render(Key { tonic: 9, minor: true }, 30.0, 1.5, 0.0);
        let r = detect(&audio, SR).unwrap();
        assert!((r.tuning_cents - 30.0).abs() < 8.0, "tuning {}", r.tuning_cents);
        assert!(r.confidence > 0.3, "confidence {}", r.confidence);
    }

    #[test]
    fn silence_noise_and_short_audio_give_no_confident_key() {
        assert!(detect(&vec![0.0; SR as usize * 10], SR).is_none());
        assert!(detect(&render(Key { tonic: 0, minor: false }, 0.0, 0.4, 0.0)[..SR as usize * 2], SR).is_none());
        // pure white noise has no tonal peaks worth a key; whatever comes back must be low confidence
        let mut rng = Lcg(7);
        let noise: Vec<f32> = (0..SR as usize * 12).map(|_| 0.3 * rng.next()).collect();
        if let Some(r) = detect(&noise, SR) {
            assert!(r.confidence < 0.35, "noise gave confidence {}", r.confidence);
        }
    }
}
