//! Beat placement: Ellis's dynamic-programming beat tracker (D. P. W. Ellis, "Beat Tracking by
//! Dynamic Programming", JNMR 2007). Each frame's score is its onset strength plus the best
//! predecessor score, penalised by how far the gap strays from the target period on a log scale.

/// Frames of the envelope chosen as beats, in order.
pub fn track(odf: &[f32], period: f64, tightness: f64) -> Vec<usize> {
    let n = odf.len();
    if n == 0 || period < 2.0 {
        return Vec::new();
    }
    let mut score = vec![0f64; n];
    let mut back: Vec<Option<usize>> = vec![None; n];
    let lo = (period / 2.0).round() as usize;
    let hi = (period * 2.0).round() as usize;
    for t in 0..n {
        let mut best: Option<(f64, usize)> = None;
        if t >= lo {
            #[allow(clippy::needless_range_loop)] // p is both an index and the gap arithmetic
            for p in t.saturating_sub(hi)..=t - lo {
                let gap = (t - p) as f64 / period;
                let s = score[p] - tightness * gap.ln().powi(2);
                if best.is_none_or(|(b, _)| s > b) {
                    best = Some((s, p));
                }
            }
        }
        match best {
            Some((s, p)) if s > 0.0 => {
                score[t] = odf[t] as f64 + s;
                back[t] = Some(p);
            }
            _ => score[t] = odf[t] as f64,
        }
    }
    // end on the best-scoring frame within the last period
    let tail = n.saturating_sub(period.ceil() as usize);
    let mut t = (tail..n).max_by(|&a, &b| score[a].partial_cmp(&score[b]).unwrap()).unwrap_or(n - 1);
    let mut beats = vec![t];
    while let Some(p) = back[t] {
        beats.push(p);
        t = p;
    }
    beats.reverse();
    beats
}
