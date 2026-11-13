//! Loudness and peak measurement: integrated loudness in LUFS (ITU-R BS.1770-4 / EBU R128) and
//! true peak.
//!
//! * **K-weighting**: a high-shelf "pre-filter" (head-related boost, about +4 dB above 2 kHz)
//!   followed by the RLB high-pass (38 Hz), both biquads. The standard gives their coefficients for
//!   48 kHz; for other rates they are derived from the same analogue prototypes by the bilinear
//!   transform (the constants below are the standard's shelf and high-pass parameters), and a test
//!   checks that the derivation at 48 kHz reproduces the published coefficients.
//! * **Integrated loudness**: the K-weighted mean square of the two channels is summed (both
//!   channels have weight 1: this is a stereo programme), measured in 400 ms blocks that overlap
//!   by 75 % (a block every 100 ms), `l = -0.691 + 10 log10(z)`. Blocks below the absolute gate
//!   (-70 LUFS) are dropped, then blocks more than 10 LU below the mean of the rest (the relative
//!   gate); the integrated loudness is the mean of what remains. A programme with no block above
//!   the gates (silence, or shorter than 400 ms) has no integrated loudness (`None`).
//! * **True peak**: the highest absolute value of the signal after 4x oversampling with a
//!   windowed-sinc interpolator (16 taps per phase, Blackman-Harris window). BS.1770-4 Annex 2 gives
//!   a particular 12-tap filter; this one is an independent design with the same purpose, and it
//!   reads a sine at a quarter of the sample rate (the worst case for sample peaks) within
//!   0.1 dB.
//!
//! The [`Meter`] is a streaming object: push blocks of left and right samples as they are produced
//! and read the result at the end. It never allocates per sample.

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::sync::OnceLock;

/// Offset of the loudness scale (BS.1770): `LUFS = -0.691 + 10 log10(mean square sum)`.
const OFFSET: f64 = -0.691;
/// Absolute gate in LUFS.
pub const ABSOLUTE_GATE: f64 = -70.0;
/// Relative gate: this many LU below the mean of the blocks that passed the absolute gate.
pub const RELATIVE_GATE: f64 = 10.0;

/// Loudness of a mean-square sum `z` (the sum over channels).
pub fn lufs_of_power(z: f64) -> f64 {
    OFFSET + 10.0 * z.log10()
}

/// dB of a linear amplitude (`-inf` for 0).
pub fn db(x: f64) -> f64 {
    20.0 * x.log10()
}

/// The result of a measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    /// Integrated loudness, gated; `None` when there is nothing above the gates.
    pub lufs: Option<f64>,
    /// True peak in dBTP (`-inf` for digital silence).
    pub true_peak_db: f64,
    /// Highest sample magnitude in dBFS (`-inf` for digital silence).
    pub sample_peak_db: f64,
}

impl Default for Measure {
    fn default() -> Self {
        Measure { lufs: None, true_peak_db: f64::NEG_INFINITY, sample_peak_db: f64::NEG_INFINITY }
    }
}

// ---------------------------------------------------------------------------------------------
// K-weighting
// ---------------------------------------------------------------------------------------------

/// A biquad `y = b0 x + b1 x1 + b2 x2 - a1 y1 - a2 y2` (direct form I, f64).
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    pub b: [f64; 3],
    pub a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 2]) -> Biquad {
        Biquad { b, a, x: [0.0; 2], y: [0.0; 2] }
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x[1] = self.x[0];
        self.x[0] = x;
        self.y[1] = self.y[0];
        self.y[0] = y;
        y
    }
}

/// The pre-filter (high shelf) of BS.1770 for sample rate `fs`.
pub fn shelf(fs: f64) -> Biquad {
    // parameters of the standard's 48 kHz filter, as fitted by its analogue prototype
    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (PI * f0 / fs).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499666774155);
    let a0 = 1.0 + k / q + k * k;
    Biquad::new(
        [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    )
}

/// The RLB weighting (second-order high-pass) of BS.1770 for sample rate `fs`.
pub fn rlb(fs: f64) -> Biquad {
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    Biquad::new([1.0, -2.0, 1.0], [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0])
}

// ---------------------------------------------------------------------------------------------
// true peak
// ---------------------------------------------------------------------------------------------

const TP_HALF: usize = 8;
const TP_TAPS: usize = 2 * TP_HALF;

/// Interpolation weights of phases 1/4, 2/4 and 3/4: `w[p][i]` multiplies the input sample
/// `i` places before the newest of a window of [`TP_TAPS`] samples. The output position lies
/// between window samples `TP_HALF - 1` and `TP_HALF` counted from the oldest. Each phase sums to
/// 1, so a constant signal keeps its level.
fn tp_weights() -> &'static [[f64; TP_TAPS]; 3] {
    static W: OnceLock<[[f64; TP_TAPS]; 3]> = OnceLock::new();
    W.get_or_init(|| {
        let mut w = [[0.0; TP_TAPS]; 3];
        for (p, row) in w.iter_mut().enumerate() {
            let frac = (p + 1) as f64 / 4.0;
            // window index j (0 = oldest) sits at distance d from the output position
            // (output = sample TP_HALF - 1 + frac)
            for (j, c) in row.iter_mut().enumerate() {
                let d = j as f64 - (TP_HALF as f64 - 1.0 + frac);
                let s = if d == 0.0 { 1.0 } else { (PI * d).sin() / (PI * d) };
                let a = d / TP_HALF as f64;
                let win = if a.abs() >= 1.0 {
                    0.0
                } else {
                    let ph = PI * a;
                    0.35875 + 0.48829 * ph.cos() + 0.14128 * (2.0 * ph).cos() + 0.01168 * (3.0 * ph).cos()
                };
                *c = s * win;
            }
            let sum: f64 = row.iter().sum();
            for c in row.iter_mut() {
                *c /= sum;
            }
        }
        w
    })
}

#[derive(Debug, Clone)]
struct TruePeak {
    hist: [f64; TP_TAPS],
    peak: f64,
}

impl TruePeak {
    fn new() -> TruePeak {
        TruePeak { hist: [0.0; TP_TAPS], peak: 0.0 }
    }

    #[inline]
    fn push(&mut self, x: f64) {
        self.hist.copy_within(1.., 0);
        self.hist[TP_TAPS - 1] = x;
        // the sample itself, and the three interpolated points after the middle sample
        self.peak = self.peak.max(self.hist[TP_HALF - 1].abs());
        for row in tp_weights() {
            let v: f64 = row.iter().zip(&self.hist).map(|(w, h)| w * h).sum();
            self.peak = self.peak.max(v.abs());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the meter
// ---------------------------------------------------------------------------------------------

/// Streaming loudness and peak meter for a stereo signal.
#[derive(Debug, Clone)]
pub struct Meter {
    shelf: [Biquad; 2],
    rlb: [Biquad; 2],
    tp: [TruePeak; 2],
    sample_peak: f64,
    /// Frames per 100 ms step.
    step: usize,
    in_step: usize,
    step_power: f64,
    /// Powers of the last four steps.
    last: VecDeque<f64>,
    /// Mean-square sum of every 400 ms block so far.
    blocks: Vec<f64>,
    frames: u64,
}

impl Meter {
    pub fn new(rate: u32) -> Meter {
        let fs = f64::from(rate);
        Meter {
            shelf: [shelf(fs), shelf(fs)],
            rlb: [rlb(fs), rlb(fs)],
            tp: [TruePeak::new(), TruePeak::new()],
            sample_peak: 0.0,
            step: (rate as usize / 10).max(1),
            in_step: 0,
            step_power: 0.0,
            last: VecDeque::with_capacity(5),
            blocks: Vec::new(),
            frames: 0,
        }
    }

    /// Feeds the next frames. Non-finite samples count as silence.
    pub fn push(&mut self, l: &[f32], r: &[f32]) {
        for (&a, &b) in l.iter().zip(r) {
            let xs = [if a.is_finite() { f64::from(a) } else { 0.0 }, if b.is_finite() { f64::from(b) } else { 0.0 }];
            let mut power = 0.0;
            for (c, &x) in xs.iter().enumerate() {
                self.sample_peak = self.sample_peak.max(x.abs());
                self.tp[c].push(x);
                let y = self.rlb[c].run(self.shelf[c].run(x));
                power += y * y;
            }
            self.step_power += power;
            self.in_step += 1;
            if self.in_step == self.step {
                self.close_step();
            }
        }
        self.frames += l.len().min(r.len()) as u64;
    }

    fn close_step(&mut self) {
        self.last.push_back(self.step_power / self.step as f64);
        self.step_power = 0.0;
        self.in_step = 0;
        if self.last.len() > 4 {
            self.last.pop_front();
        }
        if self.last.len() == 4 {
            self.blocks.push(self.last.iter().sum::<f64>() / 4.0);
        }
    }

    /// Frames pushed so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Number of 400 ms blocks measured so far.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// The measurement of everything pushed so far. The interpolator needs a few more samples
    /// to see the last frames' peaks between samples; `finish` flushes them, this does not.
    pub fn measure(&self) -> Measure {
        Measure {
            lufs: gated(&self.blocks),
            true_peak_db: db(self.tp.iter().map(|t| t.peak).fold(self.sample_peak, f64::max)),
            sample_peak_db: db(self.sample_peak),
        }
    }

    /// Flushes the true-peak interpolator with silence and returns the final measurement.
    pub fn finish(mut self) -> Measure {
        for _ in 0..TP_HALF {
            for t in &mut self.tp {
                t.push(0.0);
            }
        }
        self.measure()
    }
}

/// Integrated loudness of block powers by the two gates of BS.1770-4.
pub fn gated(blocks: &[f64]) -> Option<f64> {
    let abs_power = 10f64.powf((ABSOLUTE_GATE - OFFSET) / 10.0);
    let above: Vec<f64> = blocks.iter().copied().filter(|&z| z > abs_power).collect();
    if above.is_empty() {
        return None;
    }
    let mean = above.iter().sum::<f64>() / above.len() as f64;
    let rel_power = 10f64.powf((lufs_of_power(mean) - RELATIVE_GATE - OFFSET) / 10.0);
    let kept: Vec<f64> = above.into_iter().filter(|&z| z > rel_power).collect();
    if kept.is_empty() {
        return None;
    }
    Some(lufs_of_power(kept.iter().sum::<f64>() / kept.len() as f64))
}

/// Measures a whole stereo signal at `rate`.
pub fn measure(l: &[f32], r: &[f32], rate: u32) -> Measure {
    let mut m = Meter::new(rate);
    m.push(l, r);
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, amp: f64, secs: f64, rate: u32) -> Vec<f32> {
        (0..(secs * f64::from(rate)) as usize).map(|i| (amp * (2.0 * PI * freq * i as f64 / f64::from(rate)).sin()) as f32).collect()
    }

    fn amp(dbfs: f64) -> f64 {
        10f64.powf(dbfs / 20.0)
    }

    #[test]
    fn k_weighting_at_48k_matches_the_coefficients_published_in_bs1770() {
        let s = shelf(48_000.0);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        // BS.1770-4 table 1 (pre-filter) and table 2 (RLB high-pass)
        assert!(near(s.b[0], 1.53512485958697) && near(s.b[1], -2.69169618940638) && near(s.b[2], 1.19839281085285), "{:?}", s.b);
        assert!(near(s.a[0], -1.69065929318241) && near(s.a[1], 0.73248077421585), "{:?}", s.a);
        let h = rlb(48_000.0);
        assert!(near(h.a[0], -1.99004745483398) && near(h.a[1], 0.99007225036621), "{:?}", h.a);
    }

    /// The reference signal of EBU Tech 3341: a 1 kHz sine at -23 dBFS in both channels reads
    /// -23.0 LUFS, and the 0 dBFS one reads 0 LUFS (the -0.691 offset is chosen so that the
    /// K-weighted gain near 1 kHz cancels it).
    #[test]
    fn a_1_khz_sine_reads_its_level_in_lufs() {
        for (level, expect) in [(-23.0, -23.0), (-20.0, -20.0), (-40.0, -40.0), (0.0, 0.0)] {
            let s = sine(1000.0, amp(level), 20.0, 44_100);
            let m = measure(&s, &s, 44_100);
            let got = m.lufs.unwrap();
            assert!((got - expect).abs() < 0.05, "1 kHz at {level} dBFS read {got} LUFS, expected {expect}");
        }
        // 997 Hz, the frequency the standard names, is the same within the same tolerance
        let s = sine(997.0, amp(-23.0), 20.0, 44_100);
        assert!((measure(&s, &s, 44_100).lufs.unwrap() + 23.0).abs() < 0.05);
    }

    #[test]
    fn the_same_reading_at_the_standards_own_rate() {
        let s = sine(1000.0, amp(-23.0), 20.0, 48_000);
        assert!((measure(&s, &s, 48_000).lufs.unwrap() + 23.0).abs() < 0.02);
    }

    #[test]
    fn one_silent_channel_is_3_db_quieter() {
        let s = sine(1000.0, amp(-20.0), 20.0, 44_100);
        let z = vec![0.0f32; s.len()];
        let m = measure(&s, &z, 44_100);
        assert!((m.lufs.unwrap() - (-20.0 - 3.0103)).abs() < 0.05, "{:?}", m.lufs);
    }

    #[test]
    fn k_weighting_shape_low_frequencies_count_less_high_frequencies_more() {
        let at = |f: f64| {
            let s = sine(f, amp(-20.0), 20.0, 44_100);
            measure(&s, &s, 44_100).lufs.unwrap() + 20.0
        };
        // BS.1770 K-weighting: about +4 dB at 10 kHz, -0.7 dB at 100 Hz relative to 1 kHz (+0.7), -inf at DC
        let (lo, mid, hi) = (at(50.0), at(1000.0), at(10_000.0));
        assert!(lo < mid - 2.0, "50 Hz {lo} vs 1 kHz {mid}");
        assert!((hi - mid - 3.3).abs() < 0.8, "10 kHz {hi} vs 1 kHz {mid}");
    }

    #[test]
    fn digital_silence_has_no_loudness_and_minus_infinity_peaks() {
        let z = vec![0.0f32; 44_100 * 3];
        let m = measure(&z, &z, 44_100);
        assert_eq!(m.lufs, None);
        assert_eq!(m.true_peak_db, f64::NEG_INFINITY);
        assert_eq!(m.sample_peak_db, f64::NEG_INFINITY);
    }

    #[test]
    fn a_signal_shorter_than_one_block_has_no_integrated_loudness() {
        let s = sine(1000.0, 0.5, 0.3, 44_100);
        assert_eq!(measure(&s, &s, 44_100).lufs, None);
        let s = sine(1000.0, 0.5, 0.5, 44_100);
        assert!(measure(&s, &s, 44_100).lufs.is_some());
    }

    /// Absolute gate: a tone below -70 LUFS is ignored, so silence around a tone does not move it
    /// and a signal only at -80 LUFS has no loudness.
    #[test]
    fn the_absolute_gate_drops_very_quiet_blocks() {
        // (the few blocks that straddle a tone edge are partly tone and pass the gates, as in the
        // standard, so the tone is long enough for them not to matter)
        let tone = sine(1000.0, amp(-23.0), 60.0, 44_100);
        let mut s = vec![0.0f32; 44_100 * 10];
        s.extend_from_slice(&tone);
        s.extend(vec![0.0f32; 44_100 * 10]);
        let m = measure(&s, &s, 44_100);
        assert!((m.lufs.unwrap() + 23.0).abs() < 0.05, "{:?}", m.lufs);
        let faint = sine(1000.0, amp(-80.0), 10.0, 44_100);
        assert_eq!(measure(&faint, &faint, 44_100).lufs, None);
    }

    /// Relative gate: a section more than 10 LU below the rest does not pull the reading down;
    /// a section less than 10 LU below it does.
    #[test]
    fn the_relative_gate_drops_blocks_10_lu_below_the_programme() {
        let loud = sine(1000.0, amp(-20.0), 20.0, 44_100);
        let quiet = sine(1000.0, amp(-45.0), 20.0, 44_100);
        let mut s = loud.clone();
        s.extend_from_slice(&quiet);
        let m = measure(&s, &s, 44_100);
        assert!((m.lufs.unwrap() + 20.0).abs() < 0.1, "-45 is 25 LU below: ignored, read {:?}", m.lufs);
        // 6 LU below: both count, and the reading is the power mean
        let mid = sine(1000.0, amp(-26.0), 20.0, 44_100);
        let mut s = loud;
        s.extend_from_slice(&mid);
        let want = lufs_of_power((10f64.powf((-20.0 - OFFSET) / 10.0) + 10f64.powf((-26.0 - OFFSET) / 10.0)) / 2.0);
        let got = measure(&s, &s, 44_100).lufs.unwrap();
        assert!((got - want).abs() < 0.1, "{got} vs {want}");
    }

    /// Streaming gives the same answer as one call, whatever the block size.
    #[test]
    fn block_size_does_not_change_the_result() {
        let s: Vec<f32> = sine(440.0, 0.3, 7.3, 44_100).iter().zip(sine(3000.0, 0.1, 7.3, 44_100)).map(|(a, b)| a + b).collect();
        let whole = measure(&s, &s, 44_100);
        let mut m = Meter::new(44_100);
        for c in s.chunks(1237) {
            m.push(c, c);
        }
        assert_eq!(m.finish(), whole);
    }

    /// A sine at a quarter of the sample rate with a 45 degree phase has samples at +-0.707 of
    /// its peak: the sample peak reads -3 dB, the true peak must read near 0 dBTP.
    #[test]
    fn true_peak_sees_between_the_samples() {
        // tapered at both ends: an abrupt start or stop is itself a step that rings above the tone
        let taper = |i: usize, n: usize| {
            let e = (i.min(n - 1 - i) as f64 / 4000.0).min(1.0);
            0.5 - 0.5 * (PI * e).cos()
        };
        let n = 44_100;
        let s: Vec<f32> = (0..n).map(|i| (taper(i, n) * (PI / 4.0 + PI / 2.0 * i as f64).sin()) as f32).collect();
        let m = measure(&s, &s, 44_100);
        assert!((m.sample_peak_db + 3.0103).abs() < 0.01, "{}", m.sample_peak_db);
        assert!(m.true_peak_db.abs() < 0.05, "true peak read {} dBTP", m.true_peak_db);
        // the same tone at 0.9 with another phase: samples reach 0.78 and the real peak is 0.9; 4x
        // oversampling lands within 0.3 dB of it (the peak falls between two of its points)
        let s: Vec<f32> = (0..n).map(|i| (0.9 * taper(i, n) * (PI / 3.0 + PI / 2.0 * i as f64).sin()) as f32).collect();
        let m = measure(&s, &s, 44_100);
        assert!(m.true_peak_db > m.sample_peak_db + 0.5 && m.true_peak_db < db(0.9) + 0.05 && m.true_peak_db > db(0.9) - 0.3, "{:?}", m);
    }

    #[test]
    fn true_peak_of_a_low_tone_equals_its_sample_peak_and_is_never_below_it() {
        let s = sine(100.0, 0.5, 2.0, 44_100);
        let m = measure(&s, &s, 44_100);
        assert!(m.true_peak_db >= m.sample_peak_db);
        assert!((m.true_peak_db - db(0.5)).abs() < 0.02, "{:?}", m);
    }

    #[test]
    fn non_finite_samples_count_as_silence() {
        let m = measure(&[f32::NAN; 44_100], &[f32::INFINITY; 44_100], 44_100);
        assert_eq!(m.lufs, None);
        assert_eq!(m.sample_peak_db, f64::NEG_INFINITY);
    }
}
