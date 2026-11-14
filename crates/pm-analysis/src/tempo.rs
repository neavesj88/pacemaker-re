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
/// The default is [`BpmRange::DNB_FULL_TIME`] (92-184): drum & bass reads at its real ~174 and
/// everything from 92 to 183 keeps its own number. The original Editor folds into 78-156 instead
/// ([`BpmRange::EDITOR_COMPATIBLE`], observed range 79-150: it folds 158 to 79 and drum & bass
/// to ~87), which halves every drum & bass track; mixing one of those with a 130 BPM track then
/// slows the 130 track right down, so that is no longer the default. The device syncs tracks by
/// BPM, so a library mixing tracks the Editor analysed (drum & bass at ~87) with ours (~174)
/// can disagree about those tracks; the app can list and re-analyse them. A tempo of 184 or more
/// does not fit 92-184 (190 reads as 95): [`BpmRange::PRESETS`] has 98-196 for that. Music at 88-91 BPM (hip-hop) reads
/// double under it; 68-136 keeps those.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BpmRange {
    pub lo: f64,
}

impl Default for BpmRange {
    fn default() -> Self {
        Self::DNB_FULL_TIME
    }
}

impl BpmRange {
    /// 78-156, what the original Editor does.
    pub const EDITOR_COMPATIBLE: BpmRange = BpmRange { lo: 78.0 };
    /// 92-184, drum & bass at full time.
    pub const DNB_FULL_TIME: BpmRange = BpmRange { lo: 92.0 };
    /// 98-196, for fast drum & bass and hardcore.
    pub const FAST: BpmRange = BpmRange { lo: 98.0 };
    /// 68-136, slow music (hip-hop, downtempo) at its own tempo, house at half time.
    pub const SLOW: BpmRange = BpmRange { lo: 68.0 };
    /// The windows the app offers, lowest first.
    pub const PRESETS: [BpmRange; 4] = [Self::SLOW, Self::EDITOR_COMPATIBLE, Self::DNB_FULL_TIME, Self::FAST];

    /// The preset whose low end is `lo`, if there is one.
    pub fn from_lo(lo: f64) -> Option<BpmRange> {
        Self::PRESETS.into_iter().find(|r| (r.lo - lo).abs() < 1e-9)
    }

    /// The window's top, exclusive.
    pub fn hi(&self) -> f64 {
        2.0 * self.lo
    }

    /// Whether `bpm` lies inside the window.
    pub fn contains(&self, bpm: f64) -> bool {
        bpm >= self.lo && bpm < self.hi()
    }

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
        let e = BpmRange::EDITOR_COMPATIBLE;
        assert_eq!(e.fold(174.0), 87.0);
        assert_eq!(e.fold(158.0), 79.0);
        assert_eq!(e.fold(150.0), 150.0);
        assert_eq!(BpmRange::default(), BpmRange::DNB_FULL_TIME, "drum & bass at full time is the default");
        let r = BpmRange::DNB_FULL_TIME;
        assert_eq!(r.fold(87.0), 174.0);
        assert_eq!(r.fold(174.0), 174.0);
        assert_eq!(r.fold(256.0), 128.0);
        assert_eq!(r.fold(128.0), 128.0);
    }

    #[test]
    fn presets_round_trip_and_cover_every_tempo_once() {
        for r in BpmRange::PRESETS {
            assert_eq!(BpmRange::from_lo(r.lo), Some(r));
            for bpm in [60.0, 87.0, 100.0, 128.0, 143.0, 174.0, 180.0, 199.0] {
                assert!(r.contains(r.fold(bpm)), "{bpm} in {}-{}", r.lo, r.hi());
            }
        }
        assert_eq!(BpmRange::from_lo(90.0), None);
        assert_eq!(BpmRange::FAST.fold(90.0), 180.0);
        assert_eq!(BpmRange::DNB_FULL_TIME.fold(180.0), 180.0, "fast drum & bass at 180 keeps its number");
        assert_eq!(BpmRange::DNB_FULL_TIME.fold(90.0), 180.0, "the price: 90 BPM music reads double");
        assert_eq!(BpmRange::SLOW.fold(90.0), 90.0);
        assert_eq!(BpmRange::SLOW.fold(136.0), 68.0);
    }
}
