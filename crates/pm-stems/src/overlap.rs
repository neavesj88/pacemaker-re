//! Cutting a track into overlapping segments and joining the model's answers: a weighted
//! overlap-add that hands out each frame as soon as no later segment can touch it, so the
//! output can be streamed to disk while the model is still running.
//!
//! The weights are the triangle Demucs uses for `apply_model` (rising 1..=w/2, falling to 1,
//! divided by the peak): never zero, so the very first and last frames of a track, which only one
//! segment covers, still divide cleanly.

/// Where the segments of a track of `total` frames start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub window: usize,
    pub hop: usize,
    pub total: usize,
}

impl Plan {
    /// `overlap` is the fraction of a segment shared with the next, clamped to 0.1..=0.9 so the
    /// hop is neither tiny nor the whole window.
    pub fn new(window: usize, overlap: f32, total: usize) -> Plan {
        assert!(window >= 2, "a segment needs at least two frames");
        let overlap = if overlap.is_finite() { overlap.clamp(0.1, 0.9) } else { 0.5 };
        let hop = (((window as f64) * (1.0 - overlap as f64)).round() as usize).clamp(1, window - 1);
        Plan { window, hop, total }
    }

    /// Number of model calls (at least one, even for an empty track, which the caller rejects).
    pub fn segments(&self) -> usize {
        if self.total <= self.window { 1 } else { 1 + (self.total - self.window).div_ceil(self.hop) }
    }

    pub fn start(&self, k: usize) -> usize {
        k * self.hop
    }
}

/// The segment weights (length `window`, peak 1).
pub fn weights(window: usize) -> Vec<f32> {
    let half = window / 2;
    let mut w: Vec<f32> = (1..=half).map(|i| i as f32).collect();
    w.extend((1..=window - half).rev().map(|i| i as f32));
    let peak = w.iter().copied().fold(0.0f32, f32::max);
    w.iter_mut().for_each(|v| *v /= peak);
    w
}

/// Receives each run of finished frames: the global start index and one slice per channel.
pub type Emit<'a> = &'a mut dyn FnMut(usize, &[&[f32]]) -> Result<(), crate::Error>;

/// Accumulates `channels` equally long signals from successive segments.
pub struct OverlapAdd {
    plan: Plan,
    weights: Vec<f32>,
    acc: Vec<Vec<f32>>,
    wsum: Vec<f32>,
    /// Segments added so far.
    added: usize,
    /// Global frame index of `acc[..][0]`.
    base: usize,
}

impl OverlapAdd {
    pub fn new(plan: Plan, channels: usize) -> OverlapAdd {
        OverlapAdd { weights: weights(plan.window), acc: vec![vec![0.0; plan.window]; channels], wsum: vec![0.0; plan.window], plan, added: 0, base: 0 }
    }

    /// Adds segment number `k` (which must be the next one in order): one slice of `window`
    /// frames per channel. `emit(start, channels)` is called with every run of frames that is now
    /// final; `start` is the global frame index and the slices are already divided by the weight sum.
    pub fn push(&mut self, k: usize, seg: &[&[f32]], emit: Emit) -> Result<(), crate::Error> {
        if k != self.added || seg.len() != self.acc.len() || seg.iter().any(|s| s.len() != self.plan.window) {
            return Err(crate::Error::Invalid("segments must be added in order, full length".into()));
        }
        for (a, s) in self.acc.iter_mut().zip(seg) {
            for ((o, v), w) in a.iter_mut().zip(*s).zip(&self.weights) {
                *o += v * w;
            }
        }
        for (o, w) in self.wsum.iter_mut().zip(&self.weights) {
            *o += w;
        }
        self.added += 1;
        let last = self.added == self.plan.segments();
        let finished = if last { self.plan.total.saturating_sub(self.base).min(self.plan.window) } else { self.plan.hop };
        self.flush(finished, emit)?;
        if !last {
            let hop = self.plan.hop;
            for a in &mut self.acc {
                a.copy_within(hop.., 0);
                let n = a.len();
                a[n - hop..].fill(0.0);
            }
            self.wsum.copy_within(hop.., 0);
            let n = self.wsum.len();
            self.wsum[n - hop..].fill(0.0);
            self.base += hop;
        }
        Ok(())
    }

    fn flush(&self, n: usize, emit: Emit) -> Result<(), crate::Error> {
        if n == 0 {
            return Ok(());
        }
        let out: Vec<Vec<f32>> = self.acc.iter().map(|a| a[..n].iter().zip(&self.wsum[..n]).map(|(v, w)| v / w).collect()).collect();
        let refs: Vec<&[f32]> = out.iter().map(|v| v.as_slice()).collect();
        emit(self.base, &refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_covers_the_track() {
        let p = Plan::new(100, 0.5, 100);
        assert_eq!((p.hop, p.segments()), (50, 1));
        let p = Plan::new(100, 0.5, 101);
        assert_eq!(p.segments(), 2, "one frame past the window needs a second segment");
        let p = Plan::new(100, 0.5, 400);
        assert_eq!(p.segments(), 7);
        assert!(p.start(p.segments() - 1) + p.window >= p.total);
        assert!(p.start(p.segments() - 2) + p.window < p.total, "no segment is wasted");
        // overlap is clamped; nonsense falls back to the default
        assert_eq!(Plan::new(100, 5.0, 10).hop, 10);
        assert_eq!(Plan::new(100, f32::NAN, 10).hop, 50);
    }

    #[test]
    fn weights_are_a_positive_triangle() {
        let w = weights(8);
        assert_eq!(w, vec![0.25, 0.5, 0.75, 1.0, 1.0, 0.75, 0.5, 0.25]);
        assert!(weights(7).iter().all(|v| *v > 0.0));
    }

    /// Feeds every segment of `x` through unchanged and collects what is emitted.
    fn run(x: &[f32], window: usize, overlap: f32) -> Vec<f32> {
        let plan = Plan::new(window, overlap, x.len());
        let mut oa = OverlapAdd::new(plan, 1);
        let mut out = Vec::new();
        for k in 0..plan.segments() {
            let s = plan.start(k);
            let mut seg = vec![0.0f32; window];
            let end = (s + window).min(x.len());
            seg[..end - s].copy_from_slice(&x[s..end]);
            oa.push(k, &[&seg], &mut |start, ch| {
                assert_eq!(start, out.len(), "runs arrive in order with no gap");
                out.extend_from_slice(ch[0]);
                Ok(())
            })
            .unwrap();
        }
        out
    }

    #[test]
    fn overlap_add_of_identical_segments_reproduces_the_input() {
        let x: Vec<f32> = (0..1003).map(|i| ((i * 37 % 101) as f32 - 50.0) / 50.0).collect();
        for (window, overlap) in [(100usize, 0.5f32), (128, 0.25), (64, 0.75), (1003, 0.5), (2000, 0.5), (7, 0.5)] {
            let y = run(&x, window, overlap);
            assert_eq!(y.len(), x.len(), "window {window} overlap {overlap}");
            let err = x.iter().zip(&y).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(err < 1e-5, "window {window} overlap {overlap}: max error {err}");
        }
    }

    #[test]
    fn crossfade_blends_segments_by_weight() {
        // two segments with different constant answers: the overlap blends from one to the other
        let plan = Plan::new(8, 0.5, 12);
        assert_eq!(plan.segments(), 2);
        let mut oa = OverlapAdd::new(plan, 1);
        let mut out = Vec::new();
        let mut take = |start: usize, ch: &[&[f32]]| {
            assert_eq!(start, out.len());
            out.extend_from_slice(ch[0]);
            Ok(())
        };
        oa.push(0, &[&[1.0; 8]], &mut take).unwrap();
        oa.push(1, &[&[3.0; 8]], &mut take).unwrap();
        assert_eq!(out.len(), 12);
        assert_eq!(&out[..4], &[1.0; 4], "only the first segment reaches the start");
        // frames 4..8: first segment weights .75 .5 .25 .. wait: they fall .75,1,1,.75 -> falling half
        assert!(out[4] < out[5] && out[5] < out[6] && out[6] < out[7], "blend rises monotonically from 1 to 3: {out:?}");
        assert!(out[4] > 1.0 && out[7] < 3.0);
        assert_eq!(&out[8..], &[3.0; 4]);
    }

    #[test]
    fn out_of_order_or_short_segments_are_refused() {
        let plan = Plan::new(8, 0.5, 20);
        let mut oa = OverlapAdd::new(plan, 2);
        let seg = [0.0f32; 8];
        let mut sink = |_: usize, _: &[&[f32]]| Ok(());
        assert!(oa.push(1, &[&seg, &seg], &mut sink).is_err());
        assert!(oa.push(0, &[&seg], &mut sink).is_err());
        assert!(oa.push(0, &[&seg, &seg[..4]], &mut sink).is_err());
        assert!(oa.push(0, &[&seg, &seg], &mut sink).is_ok());
    }
}
