//! Master bus: a look-ahead peak limiter, so a mix never clips.
//!
//! Per frame the needed gain is `g = min(1, ceiling / peak)` over both channels. The limiter
//! delays the audio by `LOOKAHEAD` frames and applies
//!
//! ```text
//! f[n] = min( mean(min(g[k-L..=k]) for k in n-L..=n),  release follower )
//! ```
//!
//! to the frame `L` back. Each inner minimum covers that frame, so the mean (a linear attack ramp
//! of `L` samples) is never above its own `g`, and the output is at or below the ceiling. The
//! release follower recovers at a 100 ms time constant, so gain changes stay slow compared with
//! the audio (no distortion from fast gain modulation of bass). Below the ceiling the audio
//! passes bit-exact apart from the delay.

use std::collections::VecDeque;

/// Output ceiling (-0.18 dBFS).
pub const CEILING: f32 = 0.98;
/// Look-ahead in frames (3 ms).
pub const LOOKAHEAD: usize = 132;
const RELEASE: f32 = 1.0 / (0.1 * 44_100.0);

pub struct Limiter {
    delay: VecDeque<(f32, f32)>,
    min_q: VecDeque<(u64, f32)>,
    avg_q: VecDeque<f32>,
    avg_sum: f64,
    idx: u64,
    follow: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    pub fn new() -> Limiter {
        Limiter { delay: VecDeque::new(), min_q: VecDeque::new(), avg_q: VecDeque::new(), avg_sum: 0.0, idx: 0, follow: 1.0 }
    }

    /// Pushes one frame; returns the frame `LOOKAHEAD` frames earlier once the delay is full.
    pub fn push(&mut self, l: f32, r: f32) -> Option<(f32, f32)> {
        let (l, r) = (if l.is_finite() { l } else { 0.0 }, if r.is_finite() { r } else { 0.0 });
        let peak = l.abs().max(r.abs());
        let g = if peak > CEILING { CEILING / peak } else { 1.0 };
        // sliding minimum over the last LOOKAHEAD + 1 inputs
        while self.min_q.back().is_some_and(|&(_, v)| v >= g) {
            self.min_q.pop_back();
        }
        self.min_q.push_back((self.idx, g));
        while self.min_q.front().is_some_and(|&(i, _)| i + (LOOKAHEAD as u64) < self.idx) {
            self.min_q.pop_front();
        }
        let gmin = self.min_q.front().map_or(1.0, |&(_, v)| v);
        // moving average of that minimum over the same span
        self.avg_q.push_back(gmin);
        self.avg_sum += f64::from(gmin);
        if self.avg_q.len() > LOOKAHEAD + 1 {
            self.avg_sum -= f64::from(self.avg_q.pop_front().unwrap());
        }
        // frames before the start count as g = 1
        let missing = (LOOKAHEAD + 1).saturating_sub(self.avg_q.len());
        let smooth = ((self.avg_sum + missing as f64) / (LOOKAHEAD + 1) as f64) as f32;
        self.follow = (self.follow + (1.0 - self.follow) * RELEASE).min(smooth);
        let f = self.follow.min(smooth);

        self.delay.push_back((l, r));
        self.idx += 1;
        if self.delay.len() > LOOKAHEAD {
            let (dl, dr) = self.delay.pop_front().unwrap();
            // the final clamp can only matter through float rounding
            Some(((dl * f).clamp(-CEILING, CEILING), (dr * f).clamp(-CEILING, CEILING)))
        } else {
            None
        }
    }

    /// Frames still inside the delay after the last input.
    pub fn pending(&self) -> usize {
        self.delay.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[(f32, f32)]) -> Vec<(f32, f32)> {
        let mut lim = Limiter::new();
        let mut out = Vec::new();
        for &(l, r) in input {
            out.extend(lim.push(l, r));
        }
        while lim.pending() > 0 {
            out.extend(lim.push(0.0, 0.0));
            if out.len() >= input.len() {
                break;
            }
        }
        out
    }

    #[test]
    fn quiet_audio_passes_unchanged() {
        let input: Vec<(f32, f32)> = (0..2000).map(|i| ((i as f32 * 0.01).sin() * 0.5, 0.25)).collect();
        let out = run(&input);
        assert_eq!(out.len(), input.len());
        for (a, b) in input.iter().zip(&out) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn loud_audio_stays_under_the_ceiling() {
        let input: Vec<(f32, f32)> = (0..20000).map(|i| (((i as f32) * 0.05).sin() * 3.0, ((i as f32) * 0.031).cos() * 2.0)).collect();
        let out = run(&input);
        assert_eq!(out.len(), input.len());
        for &(l, r) in &out {
            assert!(l.abs() <= CEILING && r.abs() <= CEILING);
        }
        // it should not have flattened everything: the output still carries signal
        assert!(out.iter().any(|&(l, _)| l.abs() > 0.9));
    }

    #[test]
    fn non_finite_input_is_silenced() {
        let out = run(&[(f32::NAN, f32::INFINITY); 500]);
        assert!(out.iter().all(|&(l, r)| l == 0.0 && r == 0.0));
    }
}
