//! Pitch-preserving time-stretch: WSOLA (waveform-similarity overlap-add) with a variable,
//! caller-supplied time map.
//!
//! The stretcher produces the signal `X(u)` for `u = 0, 1, 2, ...` (samples). For each output
//! frame it asks the map where in the source that moment lies (`map(u)` -> source sample), cuts
//! a Hann-windowed frame there, but first slides the cut by up to +-[`TOL`] samples to the
//! position whose waveform best continues the previous frame (normalised cross-correlation of
//! the overlap region, with a small penalty on the shift so the result stays close to the
//! tempo curve). Frames are `N` long and overlap by 75 % (`HS = N/4`), which keeps the overlap-add
//! gain flat (the periodic Hann window sums to 2 at that hop) and the smear on transients
//! short.
//!
//! The search is two-stage: a coarse pass on the 4x decimated mono source (cheap), then a
//! refinement of +-4 full-rate samples around the winner. With a map of slope exactly 1 the
//! natural continuation is the best candidate, so the stretcher reproduces its input exactly.
//!
//! Speed: about 120 flops per output sample per channel plus the overlap-add, i.e. a few
//! hundred times real time in release builds.

use std::sync::Arc;

use crate::audio::{Source, COARSE};

/// Frame length.
pub const N: usize = 2048;
/// Synthesis hop (75 % overlap).
pub const HS: usize = N / 4;
/// Overlap region compared by the search.
const OVL: usize = N - HS;
/// Search radius in full-rate samples (5.8 ms): small enough to keep beats within a few ms of
/// where the tempo curve puts them, large enough to align periods down to about 80 Hz.
const TOL: i64 = 256;
/// Penalty per full TOL of shift on the normalised correlation (which is within [-1, 1]).
const SHIFT_PENALTY: f32 = 0.06;

/// Map from stretcher time to source position (both in samples at the engine rate).
pub type TimeMap = Box<dyn Fn(f64) -> f64 + Send>;

pub struct Wsola {
    src: Arc<Source>,
    map: TimeMap,
    win: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    /// Stretcher time of `out_l[0]`.
    base: i64,
    next_frame: i64,
    prev_p: Option<i64>,
    /// Samples with index below this are final.
    complete: i64,
    // scratch
    ref_c: Vec<f32>,
    cand_c: Vec<f32>,
    ref_m: Vec<f32>,
}

impl Wsola {
    pub fn new(src: Arc<Source>, map: TimeMap) -> Wsola {
        let win = (0..N).map(|n| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * n as f64 / N as f64).cos()) as f32 * 0.5).collect();
        Wsola {
            src,
            map,
            win,
            out_l: Vec::new(),
            out_r: Vec::new(),
            base: 0,
            next_frame: 0,
            prev_p: None,
            complete: 0,
            ref_c: Vec::new(),
            cand_c: Vec::new(),
            ref_m: Vec::new(),
        }
    }

    /// First index not yet final.
    pub fn complete(&self) -> i64 {
        self.complete
    }

    /// Produces frames until every sample below `upto` is final.
    pub fn ensure(&mut self, upto: i64) {
        while self.complete < upto {
            self.step();
        }
    }

    /// Final samples `[from, to)`; requires `base <= from` and `to <= complete`.
    pub fn slice(&self, from: i64, to: i64) -> (&[f32], &[f32]) {
        debug_assert!(from >= self.base && to <= self.complete);
        let a = (from - self.base) as usize;
        let b = (to - self.base) as usize;
        (&self.out_l[a..b], &self.out_r[a..b])
    }

    /// All final samples held: (stretcher time of the first, left, right).
    pub fn final_region(&self) -> (i64, &[f32], &[f32]) {
        let n = (self.complete - self.base).max(0) as usize;
        (self.base, &self.out_l[..n], &self.out_r[..n])
    }

    /// Drops everything before `before` (never past the final region).
    pub fn trim(&mut self, before: i64) {
        let cut = (before.min(self.complete) - self.base).max(0) as usize;
        if cut > 16384 {
            self.out_l.drain(..cut);
            self.out_r.drain(..cut);
            self.base += cut as i64;
        }
    }

    fn mono(&self, i: i64) -> f32 {
        if i < 0 || i as usize >= self.src.l.len() {
            0.0
        } else {
            0.5 * (self.src.l[i as usize] + self.src.r[i as usize])
        }
    }

    fn coarse_into(&self, from: i64, out: &mut [f32]) {
        let c = &self.src.coarse;
        for (k, o) in out.iter_mut().enumerate() {
            let i = from + k as i64;
            *o = if i < 0 || i as usize >= c.len() { 0.0 } else { c[i as usize] };
        }
    }

    fn step(&mut self) {
        let j = self.next_frame;
        // frame j starts at (j - 3) * HS so that sample 0 is covered by four windows
        let s = (j - 3) * HS as i64;
        let nominal = ((self.map)((s + (N / 2) as i64) as f64) - (N / 2) as f64).round() as i64;
        let p = match self.prev_p {
            None => nominal,
            Some(pp) => self.search(nominal, pp + HS as i64),
        };
        self.add_frame(s, p);
        self.prev_p = Some(p);
        self.next_frame += 1;
        self.complete = s + HS as i64;
    }

    fn add_frame(&mut self, s: i64, p: i64) {
        let end = s + N as i64;
        let need = (end - self.base).max(0) as usize;
        if self.out_l.len() < need {
            self.out_l.resize(need, 0.0);
            self.out_r.resize(need, 0.0);
        }
        let len = self.src.l.len() as i64;
        for n in 0..N {
            let u = s + n as i64;
            if u < self.base {
                continue;
            }
            let q = p + n as i64;
            if q < 0 || q >= len {
                continue;
            }
            let w = self.win[n];
            let o = (u - self.base) as usize;
            self.out_l[o] += w * self.src.l[q as usize];
            self.out_r[o] += w * self.src.r[q as usize];
        }
    }

    /// Best start in `nominal +- TOL` for a frame whose overlap should continue the signal that
    /// starts at `ref_start`.
    fn search(&mut self, nominal: i64, ref_start: i64) -> i64 {
        let c = COARSE as i64;
        let mc = OVL / COARSE;
        let rad = (TOL / c) as usize;
        // coarse reference
        let mut ref_c = std::mem::take(&mut self.ref_c);
        ref_c.resize(mc, 0.0);
        self.coarse_into(ref_start.div_euclid(c), &mut ref_c);
        let e_ref: f32 = ref_c.iter().map(|x| x * x).sum();
        let mut best_m = 0i64;
        let c0 = nominal.div_euclid(c);
        if e_ref > 1e-9 {
            let mut cand = std::mem::take(&mut self.cand_c);
            cand.resize(mc + 2 * rad, 0.0);
            self.coarse_into(c0 - rad as i64, &mut cand);
            // energy of the window at offset 0, then slide
            let mut e: f32 = cand[..mc].iter().map(|x| x * x).sum();
            let mut best = f32::MIN;
            let mut scores = vec![0.0f32; 2 * rad + 1];
            for m in 0..=2 * rad {
                let dot: f32 = cand[m..m + mc].iter().zip(&ref_c).map(|(a, b)| a * b).sum();
                scores[m] = if e > 1e-9 { dot / (e * e_ref).sqrt() } else { 0.0 };
                if m < 2 * rad {
                    e += cand[m + mc] * cand[m + mc] - cand[m] * cand[m];
                    if e < 0.0 {
                        e = 0.0;
                    }
                }
            }
            // centre first so ties resolve to the smallest shift
            let order = (0..=rad).flat_map(|d| if d == 0 { vec![rad] } else { vec![rad - d, rad + d] });
            for m in order {
                let shift = (m as i64 - rad as i64).abs() as f32 / rad as f32;
                let sc = scores[m] - SHIFT_PENALTY * shift;
                if sc > best {
                    best = sc;
                    best_m = m as i64 - rad as i64;
                }
            }
            self.cand_c = cand;
        }
        self.ref_c = ref_c;

        // refine at full rate (every second sample) around the coarse winner
        let centre = (c0 + best_m) * c;
        let mut ref_m = std::mem::take(&mut self.ref_m);
        ref_m.clear();
        ref_m.extend((0..OVL).step_by(2).map(|i| self.mono(ref_start + i as i64)));
        let e_rm: f32 = ref_m.iter().map(|x| x * x).sum();
        let mut qs: Vec<i64> = (centre - c..=centre + c).collect();
        qs.sort_by_key(|q| ((q - nominal).abs(), *q));
        let mut best_q = nominal;
        if e_rm > 1e-9 {
            let mut best = f32::MIN;
            for q in qs {
                let mut dot = 0.0f32;
                let mut e = 0.0f32;
                for (k, r) in ref_m.iter().enumerate() {
                    let v = self.mono(q + 2 * k as i64);
                    dot += v * r;
                    e += v * v;
                }
                let sc = if e > 1e-9 { dot / (e * e_rm).sqrt() } else { 0.0 };
                let sc = sc - SHIFT_PENALTY * ((q - nominal).abs() as f32 / TOL as f32).min(2.0);
                if sc > best {
                    best = sc;
                    best_q = q;
                }
            }
        }
        self.ref_m = ref_m;
        best_q.clamp(nominal - TOL - c, nominal + TOL + c)
    }
}
