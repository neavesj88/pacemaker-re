//! Onset strength envelope: log-magnitude spectral flux, half-wave rectified, with the bass
//! band weighted up so kick drums dominate (DJ music puts the beat in the kick).

use rustfft::{num_complex::Complex, FftPlanner};

pub const FRAME: usize = 2048;
pub const HOP: usize = 512;

pub struct Envelope {
    /// One value per hop; index i covers samples centred on i * HOP.
    pub odf: Vec<f32>,
    /// Bass-only flux, used to choose the downbeat phase.
    pub bass: Vec<f32>,
    pub hop: usize,
    pub sample_rate: u32,
}

impl Envelope {
    pub fn frames_per_sec(&self) -> f64 {
        self.sample_rate as f64 / self.hop as f64
    }
}

pub fn envelope(mono: &[f32], sample_rate: u32) -> Envelope {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos())
        .collect();
    let bins = FRAME / 2 + 1;
    let hz_per_bin = sample_rate as f32 / FRAME as f32;
    let bass_top = (200.0 / hz_per_bin).ceil() as usize;
    let weight: Vec<f32> = (0..bins).map(|b| if b <= bass_top { 2.0 } else { 1.0 }).collect();

    let n_frames = mono.len() / HOP + 1;
    let mut prev = vec![0f32; bins];
    let mut odf = Vec::with_capacity(n_frames);
    let mut bass = Vec::with_capacity(n_frames);
    let mut buf = vec![Complex::new(0f32, 0f32); FRAME];
    for f in 0..n_frames {
        let centre = f * HOP;
        for (i, c) in buf.iter_mut().enumerate() {
            let s = (centre + i).checked_sub(FRAME / 2).and_then(|k| mono.get(k)).copied().unwrap_or(0.0);
            *c = Complex::new(s * window[i], 0.0);
        }
        fft.process(&mut buf);
        let (mut total, mut low) = (0f32, 0f32);
        for b in 1..bins {
            let mag = (1.0 + 1000.0 * buf[b].norm()).ln();
            let d = (mag - prev[b]).max(0.0);
            prev[b] = mag;
            total += d * weight[b];
            if b <= bass_top {
                low += d;
            }
        }
        odf.push(total);
        bass.push(low);
    }
    let odf = detrend(&odf, 16);
    let bass = detrend(&bass, 16);
    Envelope { odf, bass, hop: HOP, sample_rate }
}

/// Subtract a centred moving average (removes loudness swells), clamp at zero, scale to unit std.
fn detrend(x: &[f32], half: usize) -> Vec<f32> {
    let n = x.len();
    let mut prefix = vec![0f64; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + x[i] as f64;
    }
    let mut out: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(n));
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (x[i] - mean as f32).max(0.0)
        })
        .collect();
    let m = out.iter().map(|&v| v as f64).sum::<f64>() / n.max(1) as f64;
    let var = out.iter().map(|&v| (v as f64 - m).powi(2)).sum::<f64>() / n.max(1) as f64;
    let sd = var.sqrt().max(1e-9) as f32;
    out.iter_mut().for_each(|v| *v /= sd);
    out
}
