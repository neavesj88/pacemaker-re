//! Tempo candidates from the onset envelope's autocorrelation.
//!
//! Each candidate period is scored by its autocorrelation plus half the autocorrelation at
//! twice the period (a true beat period also repeats at the bar level). The strongest few
//! peaks are returned. The final choice, and the half/double resolution, is made by the grid
//! search (grid::lock) and the preferred BPM range.

pub const MIN_BPM: f64 = 60.0;
pub const MAX_BPM: f64 = 200.0;

/// Preferred tempo window, the analogue of DJ software's "BPM range" setting: candidate
/// tempos are doubled or halved into [lo, 2 * lo).
///
/// The default matches the original Editor (observed range 79-150: it folds 158 to 79 and
/// drum & bass to ~87), because the device syncs tracks by BPM and a library mixing our
/// analysis with the Editor's must agree. [`BpmRange::DNB_FULL_TIME`] gives the rekordbox-style
/// reading (drum & bass at ~174).
#[derive(Debug, Clone, Copy)]
pub struct BpmRange {
    pub lo: f64,
}

impl Default for BpmRange {
    fn default() -> Self {
        Self::EDITOR_COMPATIBLE
    }
}

impl BpmRange {
    pub const EDITOR_COMPATIBLE: BpmRange = BpmRange { lo: 78.0 };
    pub const DNB_FULL_TIME: BpmRange = BpmRange { lo: 88.0 };

    pub fn fold(&self, mut bpm: f64) -> f64 {
        while bpm < self.lo {
            bpm *= 2.0;
        }
        while bpm >= 2.0 * self.lo {
            bpm /= 2.0;
        }
        bpm
    }
}

/// A tempo candidate: BPM folded into the preferred range, and its autocorrelation strength
/// relative to the strongest candidate (1.0 = strongest).
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub bpm: f64,
    pub strength: f64,
}

/// Up to `k` candidate tempos (folded into `range`, deduplicated), strongest first.
pub fn candidates(odf: &[f32], frames_per_sec: f64, k: usize, range: BpmRange) -> Vec<Candidate> {
    let min_lag = (frames_per_sec * 60.0 / MAX_BPM).floor().max(1.0) as usize;
    let max_lag = (frames_per_sec * 60.0 / MIN_BPM).ceil() as usize;
    let ac = autocorr(odf, 2 * max_lag + 2);
    let score: Vec<f64> = (0..=max_lag).map(|l| if l < min_lag { 0.0 } else { ac[l] + 0.5 * ac[2 * l] }).collect();
    let mut peaks: Vec<(f64, f64)> = (min_lag + 1..max_lag)
        .filter(|&l| score[l] > score[l - 1] && score[l] >= score[l + 1])
        .map(|l| {
            let (y0, y1, y2) = (score[l - 1], score[l], score[l + 1]);
            let den = y0 - 2.0 * y1 + y2;
            let lag = if den.abs() > 1e-12 { l as f64 + 0.5 * (y0 - y2) / den } else { l as f64 };
            (y1, 60.0 * frames_per_sec / lag)
        })
        .collect();
    peaks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let top = peaks.first().map(|p| p.0).unwrap_or(1.0).max(1e-12);
    let mut out: Vec<Candidate> = Vec::new();
    for (s, bpm) in peaks {
        let b = range.fold(bpm);
        if out.iter().all(|o| (o.bpm - b).abs() / b > 0.01) {
            out.push(Candidate { bpm: b, strength: (s / top).max(0.0) });
        }
        if out.len() == k {
            break;
        }
    }
    out
}

fn autocorr(x: &[f32], max_lag: usize) -> Vec<f64> {
    (0..=max_lag)
        .map(|lag| {
            if lag >= x.len() {
                return 0.0;
            }
            let n = x.len() - lag;
            (0..n).map(|i| x[i] as f64 * x[i + lag] as f64).sum::<f64>() / n as f64
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_into_range() {
        assert_eq!(BpmRange::default().fold(174.0), 87.0);
        assert_eq!(BpmRange::default().fold(158.0), 79.0);
        assert_eq!(BpmRange::default().fold(150.0), 150.0);
        let r = BpmRange::DNB_FULL_TIME;
        assert_eq!(r.fold(87.0), 174.0);
        assert_eq!(r.fold(174.0), 174.0);
        assert_eq!(r.fold(256.0), 128.0);
        assert_eq!(r.fold(128.0), 128.0);
    }
}
