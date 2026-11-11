//! The spectrogram front end of HTDemucs: the 4096-point STFT (hop 1024) with Demucs' own
//! padding, the Nyquist bin dropped and the frames trimmed, and the complex-as-channels ("CaC")
//! packing the model takes as its second input. Also the inverse, used when a model graph returns
//! the spectrogram branch separately from the waveform branch.
//!
//! This follows the `_spec` / `_ispec` / `_magnitude` / `_mask` methods of the Demucs
//! reference (`demucs/htdemucs.py`) as the model-export notes describe them (sevagh/demucs.onnx).
//! It has been checked against its own inverse and a plain DFT, NOT against the reference
//! implementation's output on real audio (no PyTorch available here).

use rustfft::num_complex::Complex32;

use crate::Error;
use crate::stft::{Stft, reflect_pad};

pub const N_FFT: usize = 4096;
pub const HOP: usize = 1024;
/// Frequency bins the model sees (the Nyquist bin is dropped).
pub const BINS: usize = N_FFT / 2;

pub struct Frontend {
    stft: Stft,
}

impl Default for Frontend {
    fn default() -> Self {
        Self::new()
    }
}

/// Frames of the model's spectrogram for a segment of `len` samples.
pub fn frames_for(len: usize) -> usize {
    len.div_ceil(HOP)
}

impl Frontend {
    pub fn new() -> Frontend {
        Frontend { stft: Stft::new(N_FFT, HOP) }
    }

    /// `_spec`: `[BINS][frames_for(len)]`, frequency-major.
    pub fn spec(&self, x: &[f32]) -> Result<Vec<Complex32>, Error> {
        let le = frames_for(x.len());
        let pad = HOP / 2 * 3;
        let padded = reflect_pad(x, pad, pad + le * HOP - x.len());
        let z = self.stft.forward(&padded)?;
        let frames = self.stft.frames_for(padded.len());
        debug_assert_eq!(frames, le + 4);
        let mut out = Vec::with_capacity(BINS * le);
        for k in 0..BINS {
            out.extend_from_slice(&z[k * frames + 2..k * frames + 2 + le]);
        }
        Ok(out)
    }

    /// `_ispec`: back to `length` samples from `[BINS][frames_for(length)]`.
    pub fn ispec(&self, z: &[Complex32], length: usize) -> Result<Vec<f32>, Error> {
        let le = frames_for(length);
        if z.len() != BINS * le {
            return Err(Error::Invalid(format!("spectrogram has {} values, expected {BINS} x {le}", z.len())));
        }
        let pad = HOP / 2 * 3;
        let frames = le + 4;
        let mut full = vec![Complex32::new(0.0, 0.0); (BINS + 1) * frames];
        for k in 0..BINS {
            full[k * frames + 2..k * frames + 2 + le].copy_from_slice(&z[k * le..(k + 1) * le]);
        }
        let samples = HOP * le + 2 * pad;
        let y = self.stft.inverse(&full, frames, samples)?;
        Ok(y[pad..pad + length].to_vec())
    }
}

/// `_magnitude` with `cac = True`: channels `[L.re, L.im, R.re, R.im]`, each `[BINS][frames]`,
/// concatenated (the model's `x` input without the batch axis).
pub fn pack_cac(left: &[Complex32], right: &[Complex32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(4 * left.len());
    for ch in [left, right] {
        out.extend(ch.iter().map(|c| c.re));
        out.extend(ch.iter().map(|c| c.im));
    }
    out
}

/// `_mask` with `cac = True` for one stem: the inverse of [`pack_cac`]. `packed` holds four
/// `[BINS][frames]` planes.
pub fn unpack_cac(packed: &[f32]) -> Result<(Vec<Complex32>, Vec<Complex32>), Error> {
    if packed.is_empty() || !packed.len().is_multiple_of(4) {
        return Err(Error::Invalid("a complex-as-channels block needs four equal planes".into()));
    }
    let n = packed.len() / 4;
    let plane = |i: usize| &packed[i * n..(i + 1) * n];
    let make = |re: &[f32], im: &[f32]| re.iter().zip(im).map(|(a, b)| Complex32::new(*a, *b)).collect::<Vec<_>>();
    Ok((make(plane(0), plane(1)), make(plane(2), plane(3))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::tests::noise;

    /// A signal with no energy at Nyquist (the model never sees that bin).
    fn band_limited(n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.4 * (i as f32 * 0.07).sin() + 0.3 * (i as f32 * 0.9).cos() + 0.1 * (i as f32 * 1.6).sin()).collect()
    }

    #[test]
    fn spectrogram_has_the_shape_the_model_expects() {
        // 343,980 samples (7.8 s at 44.1 kHz) -> [2048, 336], the shape in the model manifest
        let fe = Frontend::new();
        let x = band_limited(343_980);
        let z = fe.spec(&x).unwrap();
        assert_eq!(frames_for(343_980), 336);
        assert_eq!(z.len(), 2048 * 336);
    }

    /// Demucs drops two frames at each end of the spectrogram, so the first and last ~3000 samples
    /// of a segment are only partly covered by the spectral branch (the model's time branch and
    /// the overlap-add cover them); compare the interior.
    const EDGE: usize = 3072;

    #[test]
    fn spec_then_ispec_restores_the_interior_of_a_band_limited_signal() {
        let fe = Frontend::new();
        for len in [12_345usize, 20_000, 30_001] {
            let x = band_limited(len);
            let y = fe.ispec(&fe.spec(&x).unwrap(), len).unwrap();
            assert_eq!(y.len(), len);
            let err = (EDGE..len - EDGE).map(|i| (x[i] - y[i]).abs()).fold(0.0f32, f32::max);
            assert!(err < 1e-3, "len {len}: max error {err}");
        }
    }

    #[test]
    fn white_noise_only_loses_the_dropped_nyquist_bin() {
        // dropping one of 2049 bins removes about 1/2049 of the energy
        let fe = Frontend::new();
        let x = noise(20_000, 3);
        let y = fe.ispec(&fe.spec(&x).unwrap(), x.len()).unwrap();
        let inner = EDGE..x.len() - EDGE;
        let e: f64 = x[inner.clone()].iter().map(|v| (*v as f64).powi(2)).sum();
        let d: f64 = x[inner.clone()].iter().zip(&y[inner]).map(|(a, b)| ((a - b) as f64).powi(2)).sum();
        assert!(d / e < 2e-3, "relative error energy {}", d / e);
    }

    #[test]
    fn cac_pack_unpack_round_trips() {
        let l: Vec<Complex32> = (0..6).map(|i| Complex32::new(i as f32, -(i as f32))).collect();
        let r: Vec<Complex32> = (0..6).map(|i| Complex32::new(10.0 + i as f32, 0.5 * i as f32)).collect();
        let packed = pack_cac(&l, &r);
        assert_eq!(packed.len(), 24);
        assert_eq!(&packed[0..6], &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0], "left real plane first");
        assert_eq!(&packed[6..12], &[0.0, -1.0, -2.0, -3.0, -4.0, -5.0], "then left imaginary");
        let (l2, r2) = unpack_cac(&packed).unwrap();
        assert_eq!((l2, r2), (l, r));
        assert!(unpack_cac(&[1.0, 2.0, 3.0]).is_err());
    }

    #[test]
    fn ispec_rejects_a_wrong_sized_spectrogram() {
        let fe = Frontend::new();
        assert!(fe.ispec(&[Complex32::new(0.0, 0.0); 10], 5000).is_err());
    }
}
