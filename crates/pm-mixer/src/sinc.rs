//! Windowed-sinc interpolation: the one resampling primitive of the engine (sample-rate
//! conversion on load, "Pitch (vinyl)" playback, the pitch-shift read-out).
//!
//! The kernel is `sinc(x)` times a 4-term Blackman-Harris window over `|x| < HALF`, tabulated at
//! `RES` points per unit and linearly interpolated. When reading at a rate above 1 (down-sampling)
//! the kernel is stretched by `1 / cut` so that everything above the new Nyquist is removed.

use std::sync::OnceLock;

/// Kernel half width in input samples at `cut == 1`.
pub const HALF: f64 = 8.0;
const RES: usize = 512;

fn table() -> &'static [f32] {
    static T: OnceLock<Vec<f32>> = OnceLock::new();
    T.get_or_init(|| {
        let n = (HALF as usize) * RES + 2;
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let x = i as f64 / RES as f64;
            let s = if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
            let a = x / HALF;
            let w = if a >= 1.0 {
                0.0
            } else {
                // Blackman-Harris over [-HALF, HALF]
                let ph = std::f64::consts::PI * a;
                0.35875 + 0.48829 * ph.cos() + 0.14128 * (2.0 * ph).cos() + 0.01168 * (3.0 * ph).cos()
            };
            v.push((s * w) as f32);
        }
        v
    })
}

#[inline]
fn k(t: &[f32], x: f64) -> f32 {
    let p = x.abs() * RES as f64;
    let i = p as usize;
    if i + 1 >= t.len() {
        return 0.0;
    }
    let f = (p - i as f64) as f32;
    t[i] + (t[i + 1] - t[i]) * f
}

/// Band-limited value of `buf` at fractional index `pos` (outside the buffer the signal is
/// zero). `cut` in (0, 1]: 1 for up-sampling or plain delay, `1/ratio` when reading `ratio` times
/// faster than the buffer's rate.
#[inline]
pub fn interp(buf: &[f32], pos: f64, cut: f64) -> f32 {
    let t = table();
    let w = HALF / cut;
    let lo = ((pos - w).ceil() as i64).max(0);
    let hi = ((pos + w).floor() as i64).min(buf.len() as i64 - 1);
    let c = cut as f32;
    let mut acc = 0.0f32;
    let mut i = lo;
    while i <= hi {
        acc += buf[i as usize] * k(t, (i as f64 - pos) * cut) * c;
        i += 1;
    }
    acc
}

/// Resamples a whole signal from `from` Hz to `to` Hz.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let cut = (1.0 / ratio).min(1.0);
    let n = (input.len() as f64 / ratio).round() as usize;
    (0..n).map(|i| interp(input, i as f64 * ratio, cut)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, rate: f64, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate).sin() as f32 * 0.5).collect()
    }

    #[test]
    fn integer_positions_are_exact_and_fractions_are_accurate() {
        let x = tone(1000.0, 44100.0, 4000);
        for i in [100usize, 1234, 3000] {
            assert!((interp(&x, i as f64, 1.0) - x[i]).abs() < 1e-3);
        }
        for p in [100.25f64, 777.5, 2999.9] {
            let want = (0.5 * (2.0 * std::f64::consts::PI * 1000.0 * p / 44100.0).sin()) as f32;
            assert!((interp(&x, p, 1.0) - want).abs() < 2e-3, "at {p}");
        }
    }

    #[test]
    fn resampling_keeps_level_and_frequency() {
        let x = tone(1000.0, 48000.0, 48000);
        let y = resample(&x, 48000, 44100);
        assert!((y.len() as i64 - 44100).abs() <= 1);
        let rms = (y[2000..40000].iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>() / 38000.0).sqrt();
        assert!((rms - 0.5 / 2f64.sqrt()).abs() < 0.005, "{rms}");
        // above the new Nyquist is removed, not folded back
        let hi = tone(30_000.0, 96000.0, 96000);
        let z = resample(&hi, 96000, 44100);
        let rms = (z[2000..40000].iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>() / 38000.0).sqrt();
        assert!(rms < 0.02, "{rms}");
    }
}
