//! STFT and inverse STFT with the conventions of `torch.stft` / `torch.istft` as Demucs uses
//! them: periodic Hann window, `center = true` with reflect padding, `normalized = true`
//! (forward scaled by `n_fft^-1/2`, inverse by `n_fft^1/2`), one-sided spectrum.
//!
//! Spectra are stored frequency-major: `spec[bin * frames + frame]`, the layout of the model's
//! `[freq, time]` tensors.

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

use crate::Error;

pub struct Stft {
    n_fft: usize,
    hop: usize,
    window: Vec<f32>,
    fwd: Arc<dyn Fft<f32>>,
    inv: Arc<dyn Fft<f32>>,
}

/// Index into a signal of length `len` after reflecting about the first and last sample without
/// repeating them (`pad_mode = "reflect"`); repeats with period `2 * (len - 1)` when the padding
/// is longer than the signal, so a short input cannot index out of range.
fn reflect(i: isize, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let period = 2 * (len as isize - 1);
    let m = i.rem_euclid(period);
    if m < len as isize { m as usize } else { (period - m) as usize }
}

/// `x` extended by `left` and `right` reflected samples.
pub fn reflect_pad(x: &[f32], left: usize, right: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len() + left + right);
    for k in 0..left {
        out.push(x[reflect(k as isize - left as isize, x.len())]);
    }
    out.extend_from_slice(x);
    for k in 0..right {
        out.push(x[reflect((x.len() + k) as isize, x.len())]);
    }
    out
}

impl Stft {
    pub fn new(n_fft: usize, hop: usize) -> Stft {
        assert!(n_fft >= 4 && n_fft.is_multiple_of(2) && hop > 0 && hop <= n_fft, "bad STFT size");
        let window = (0..n_fft).map(|n| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / n_fft as f64).cos()) as f32).collect();
        let mut planner = FftPlanner::new();
        Stft { n_fft, hop, window, fwd: planner.plan_fft_forward(n_fft), inv: planner.plan_fft_inverse(n_fft) }
    }

    pub fn n_fft(&self) -> usize {
        self.n_fft
    }

    pub fn hop(&self) -> usize {
        self.hop
    }

    /// Frequency bins of the one-sided spectrum.
    pub fn bins(&self) -> usize {
        self.n_fft / 2 + 1
    }

    /// Frames a signal of `len` samples gives with `center = true`.
    pub fn frames_for(&self, len: usize) -> usize {
        len / self.hop + 1
    }

    /// `torch.stft(x, n_fft, hop, window=hann, center=True, pad_mode="reflect", normalized=True)`.
    pub fn forward(&self, x: &[f32]) -> Result<Vec<Complex32>, Error> {
        if x.is_empty() {
            return Err(Error::Invalid("cannot take the STFT of an empty signal".into()));
        }
        let pad = self.n_fft / 2;
        let padded = reflect_pad(x, pad, pad);
        let frames = self.frames_for(x.len());
        let bins = self.bins();
        let scale = 1.0 / (self.n_fft as f32).sqrt();
        let mut spec = vec![Complex32::new(0.0, 0.0); bins * frames];
        let mut buf = vec![Complex32::new(0.0, 0.0); self.n_fft];
        let mut scratch = vec![Complex32::new(0.0, 0.0); self.fwd.get_inplace_scratch_len()];
        for t in 0..frames {
            let start = t * self.hop;
            for n in 0..self.n_fft {
                buf[n] = Complex32::new(padded[start + n] * self.window[n], 0.0);
            }
            self.fwd.process_with_scratch(&mut buf, &mut scratch);
            for (k, v) in buf.iter().take(bins).enumerate() {
                spec[k * frames + t] = *v * scale;
            }
        }
        Ok(spec)
    }

    /// `torch.istft(spec, n_fft, hop, window=hann, center=True, normalized=True, length=length)`.
    /// `spec` is `[bins][frames]`. Positions the window envelope does not cover come out as zero.
    pub fn inverse(&self, spec: &[Complex32], frames: usize, length: usize) -> Result<Vec<f32>, Error> {
        let bins = self.bins();
        if frames == 0 || spec.len() != bins * frames {
            return Err(Error::Invalid(format!("spectrum has {} values, expected {bins} x {frames}", spec.len())));
        }
        let n = self.n_fft;
        let total = (frames - 1) * self.hop + n;
        let mut y = vec![0.0f32; total];
        let mut env = vec![0.0f32; total];
        let mut buf = vec![Complex32::new(0.0, 0.0); n];
        let mut scratch = vec![Complex32::new(0.0, 0.0); self.inv.get_inplace_scratch_len()];
        let scale = (n as f32).sqrt() / n as f32;
        for t in 0..frames {
            for k in 0..bins {
                buf[k] = spec[k * frames + t];
            }
            // conjugate symmetry fills the upper half; the imaginary part of DC and Nyquist falls
            // out in the real part below, as in irfft
            for k in 1..n / 2 {
                buf[n - k] = buf[k].conj();
            }
            self.inv.process_with_scratch(&mut buf, &mut scratch);
            let start = t * self.hop;
            for i in 0..n {
                y[start + i] += buf[i].re * scale * self.window[i];
                env[start + i] += self.window[i] * self.window[i];
            }
        }
        let skip = n / 2;
        let mut out = vec![0.0f32; length];
        for (i, o) in out.iter_mut().enumerate() {
            let p = skip + i;
            if p < total && env[p] > 1e-11 {
                *o = y[p] / env[p];
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Deterministic pseudo-random signal in [-0.5, 0.5).
    pub(crate) fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed.max(1);
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect()
    }

    #[test]
    fn reflect_pad_matches_the_torch_reflect_mode() {
        // torch: F.pad([1,2,3,4], (2,3), "reflect") == [3,2,1,2,3,4,3,2,1]
        assert_eq!(reflect_pad(&[1.0, 2.0, 3.0, 4.0], 2, 3), vec![3.0, 2.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0]);
        // longer than the signal: wraps instead of panicking
        assert_eq!(reflect_pad(&[1.0, 2.0], 3, 0), vec![2.0, 1.0, 2.0, 1.0, 2.0]);
        assert_eq!(reflect_pad(&[5.0], 2, 2), vec![5.0; 5]);
    }

    #[test]
    fn forward_matches_a_plain_dft_of_one_frame() {
        let st = Stft::new(16, 4);
        let x = noise(40, 7);
        let spec = st.forward(&x).unwrap();
        let frames = st.frames_for(x.len());
        assert_eq!(frames, 11);
        let padded = reflect_pad(&x, 8, 8);
        for &t in &[0usize, 3, 10] {
            for k in 0..=8usize {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for n in 0..16usize {
                    let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / 16.0).cos();
                    let v = padded[t * 4 + n] as f64 * w;
                    let a = -2.0 * std::f64::consts::PI * (k * n) as f64 / 16.0;
                    re += v * a.cos();
                    im += v * a.sin();
                }
                let got = spec[k * frames + t];
                assert!((got.re as f64 - re / 4.0).abs() < 1e-5 && (got.im as f64 - im / 4.0).abs() < 1e-5, "bin {k} frame {t}: {got:?} vs {re} {im}");
            }
        }
    }

    #[test]
    fn round_trip_restores_the_signal() {
        for (n_fft, hop, len) in [(16usize, 4usize, 40usize), (64, 16, 1000), (4096, 1024, 20_000), (32, 16, 33), (32, 8, 5)] {
            let st = Stft::new(n_fft, hop);
            let x = noise(len, 99);
            let spec = st.forward(&x).unwrap();
            let y = st.inverse(&spec, st.frames_for(len), len).unwrap();
            let err = x.iter().zip(&y).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(err < 2e-5, "n_fft {n_fft} hop {hop} len {len}: max error {err}");
        }
    }

    #[test]
    fn rejects_bad_input() {
        let st = Stft::new(16, 4);
        assert!(st.forward(&[]).is_err());
        assert!(st.inverse(&[Complex32::new(0.0, 0.0); 5], 2, 10).is_err());
    }
}
