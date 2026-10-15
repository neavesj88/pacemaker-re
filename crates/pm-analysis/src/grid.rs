//! Locked beat grid by direct search: for each candidate tempo, find the (period, phase) whose
//! grid lands on the most onset energy per grid line across the whole track. Candidates are
//! ranked by grid energy times autocorrelation strength. Neither measure alone is enough:
//! energy alone picks the 3:4 alias on psytrance's triplet ("galloping") basslines, because the
//! alias grid lands on the strong triplet bass notes; peak-to-mean contrast prefers 3:4 and 2:3
//! aliases generally. The autocorrelation term includes bar-level repetition, which aliases lack.
//! Coarse-to-fine to 0.0005 BPM and 0.25 ms, then snap to a whole or half BPM when that costs
//! almost nothing, as most dance music is produced at one. Searching the whole track, rather
//! than fitting a line through tracked beats, keeps one bad breakdown from tilting the grid.

use crate::onset::Envelope;
use crate::tempo::Candidate;

#[derive(Debug, Clone)]
pub struct Grid {
    /// Seconds per beat.
    pub period: f64,
    /// Time of a reference beat in seconds (grid lines at phase + k * period).
    pub phase: f64,
    /// Peak-to-mean ratio of grid energy over phases: how clearly the grid stands out.
    pub contrast: f64,
    pub snapped: bool,
}

impl Grid {
    pub fn bpm(&self) -> f64 {
        60.0 / self.period
    }
}

/// Linearly interpolated value of an envelope series at time `t` seconds.
fn series_at(series: &[f32], fps: f64, t: f64) -> f64 {
    let x = t * fps;
    if x < 0.0 {
        return 0.0;
    }
    let i = x.floor() as usize;
    if i + 1 >= series.len() {
        return 0.0;
    }
    let f = x - i as f64;
    series[i] as f64 * (1.0 - f) + series[i + 1] as f64 * f
}

fn odf_at(env: &Envelope, t: f64) -> f64 {
    series_at(&env.odf, env.frames_per_sec(), t)
}

/// Mean bass-band (kick) onset strength on a grid.
fn bass_energy(env: &Envelope, phase: f64, period: f64, duration: f64) -> f64 {
    let (mut e, mut n) = (0.0, 0usize);
    let mut t = phase.rem_euclid(period);
    while t <= duration {
        e += series_at(&env.bass, env.frames_per_sec(), t);
        n += 1;
        t += period;
    }
    if n == 0 { 0.0 } else { e / n as f64 }
}

/// Mean envelope value on the grid lines inside the track.
pub fn mean_energy(env: &Envelope, phase: f64, period: f64, duration: f64) -> f64 {
    let (mut e, mut n) = (0.0, 0usize);
    let mut t = phase.rem_euclid(period);
    while t <= duration {
        e += odf_at(env, t);
        n += 1;
        t += period;
    }
    if n == 0 { 0.0 } else { e / n as f64 }
}

/// Best phase for a period, scanning phases in `[from, to)` with `step` seconds:
/// (best energy, its phase, mean energy over all phases).
pub fn best_phase(env: &Envelope, period: f64, duration: f64, from: f64, to: f64, step: f64) -> (f64, f64, f64) {
    let (mut best_e, mut best_p, mut sum, mut n) = (f64::MIN, from, 0.0, 0usize);
    let mut p = from;
    while p < to {
        let e = mean_energy(env, p, period, duration);
        sum += e;
        n += 1;
        if e > best_e {
            (best_e, best_p) = (e, p);
        }
        p += step;
    }
    (best_e, best_p, sum / n.max(1) as f64)
}

/// Share of the winning grid's energy a faster related tempo (4:3, 3:2, 5:4) needs to take over.
/// Calibrated with examples/ratiodebug.rs: the real triplet alias (psytrance) measured 1.24;
/// house and techno false swaps 0.62-0.78; a correct 140 with a 3:2 sibling 0.86.
const RELATED_TEMPO_SHARE: f64 = 1.0;

pub fn lock(env: &Envelope, candidates: &[Candidate], duration: f64, range_hi: f64, snap: bool) -> Option<Grid> {
    let mut best: Option<(f64, f64, f64, f64)> = None; // (contrast, score, period, phase)
    for c in candidates {
        let p0 = 60.0 / c.bpm;
        for i in -30..=30 {
            let period = p0 * (1.0 + i as f64 * 0.0002);
            let (e, phase, mean) = best_phase(env, period, duration, 0.0, period, 0.002);
            let contrast = if mean > 0.0 { e / mean } else { 0.0 };
            let score = e * c.strength;
            if best.is_none_or(|b| score > b.1) {
                best = Some((contrast, score, period, phase));
            }
        }
    }
    let (contrast, _, mut period, mut phase) = best?;

    // Triplet-alias check: a 3:4 alias of the true tempo can out-score it on triplet basslines
    // (the alias grid lands on the triplet notes). If a faster related tempo still collects
    // at least as much energy as the winner, it is the real beat (the winner only won on
    // autocorrelation strength). Off-beat hats and sixteenths give house a 4:3 grid with
    // 60-80 % of the energy, so anything lower is left alone.
    let (e_win, _, _) = best_phase(env, period, duration, 0.0, period, 0.002);
    let mut swap: Option<(f64, f64, f64)> = None;
    for ratio in [4.0 / 3.0, 3.0 / 2.0, 5.0 / 4.0] {
        let p_fast = period / ratio;
        if 60.0 / p_fast >= range_hi {
            continue;
        }
        for i in -15..=15 {
            let pb = p_fast * (1.0 + i as f64 * 0.0002);
            let (e, ph, _) = best_phase(env, pb, duration, 0.0, pb, 0.002);
            if e >= RELATED_TEMPO_SHARE * e_win && swap.is_none_or(|s| e > s.0) {
                swap = Some((e, pb, ph));
            }
        }
    }
    if let Some((_, pb, ph)) = swap {
        (period, phase) = (pb, ph);
    }

    // fine: period within one coarse step, phase within +-3 ms
    let mut e_best = f64::MIN;
    let p_coarse = period;
    for i in -40..=40 {
        let pb = p_coarse * (1.0 + i as f64 * 0.00001);
        let (e, ph, _) = best_phase(env, pb, duration, phase - 0.003, phase + 0.003, 0.00025);
        if e > e_best {
            (e_best, period, phase) = (e, pb, ph);
        }
    }
    // Beat vs off-beat: off-beat hats (house) and off-beat bass (psytrance) can outweigh the
    // kick in the full-band envelope; the beat is where the kick is, so let the bass band decide.
    if bass_energy(env, phase + period / 2.0, period, duration) > bass_energy(env, phase, period, duration) {
        phase += period / 2.0;
    }
    let mut grid = Grid { period, phase, contrast, snapped: false };

    if snap {
        let bpm = grid.bpm();
        let mut targets = vec![bpm.round(), (bpm * 2.0).round() / 2.0];
        targets.dedup();
        for target in targets {
            if (target - bpm).abs() > 0.3 {
                continue;
            }
            let pb = 60.0 / target;
            let (e, ph, _) = best_phase(env, pb, duration, grid.phase - 0.01, grid.phase + 0.01, 0.00025);
            if e >= 0.985 * e_best {
                grid = Grid { period: pb, phase: ph, snapped: true, ..grid };
                break;
            }
        }
    }
    Some(grid)
}

/// Sub-frame time of the onset peak nearest each tracked beat.
pub fn refine_times(env: &Envelope, frames: &[usize]) -> Vec<f64> {
    let fps = env.frames_per_sec();
    frames
        .iter()
        .map(|&f| {
            let lo = f.saturating_sub(2);
            let hi = (f + 2).min(env.odf.len() - 1);
            let m = (lo..=hi).max_by(|&a, &b| env.odf[a].partial_cmp(&env.odf[b]).unwrap()).unwrap_or(f);
            let mut x = m as f64;
            if m > 0 && m + 1 < env.odf.len() {
                let (y0, y1, y2) = (env.odf[m - 1] as f64, env.odf[m] as f64, env.odf[m + 1] as f64);
                let den = y0 - 2.0 * y1 + y2;
                if den.abs() > 1e-9 {
                    x += (0.5 * (y0 - y2) / den).clamp(-0.5, 0.5);
                }
            }
            x / fps
        })
        .collect()
}

/// RMS distance (ms) of tracked beats from their nearest grid line, over beats within 40 ms,
/// and the fraction of tracked beats that fall outside that window.
pub fn residual(g: &Grid, times: &[f64]) -> (f64, f64) {
    let mut sq = 0.0;
    let mut n_in = 0usize;
    for &t in times {
        let d = (t - g.phase).rem_euclid(g.period);
        let d = d.min(g.period - d);
        if d < 0.040 {
            sq += d * d;
            n_in += 1;
        }
    }
    let rms = if n_in > 0 { (sq / n_in as f64).sqrt() * 1000.0 } else { f64::NAN };
    let outside = 1.0 - n_in as f64 / times.len().max(1) as f64;
    (rms, outside)
}

/// Grid beat times (seconds) from the first beat at or after 0 to `duration`.
pub fn grid_times(g: &Grid, duration: f64) -> Vec<f64> {
    let mut t = g.phase.rem_euclid(g.period);
    let mut out = Vec::new();
    while t <= duration {
        out.push(t);
        t += g.period;
    }
    out
}

/// Of the four beat phases, the one whose beats carry the most onset energy, read as the
/// downbeat (in dance music new parts, crashes and fills land on the one). Heuristic: v1.
pub fn downbeat_phase(env: &Envelope, beat_times: &[f64]) -> usize {
    let mut score = [0f64; 4];
    for (i, &t) in beat_times.iter().enumerate() {
        let f = (t * env.frames_per_sec()).round() as usize;
        if f < env.odf.len() {
            score[i % 4] += env.odf[f] as f64 + env.bass[f] as f64;
        }
    }
    (0..4).max_by(|&a, &b| score[a].partial_cmp(&score[b]).unwrap()).unwrap()
}

/// Re-express a grid inside the user's BPM range. Halving the tempo keeps every other beat,
/// choosing the parity with more kick (bass-band) energy; doubling adds lines between beats.
pub fn fold_to_range(env: &Envelope, g: &Grid, duration: f64, range: crate::tempo::BpmRange) -> Grid {
    let mut g = g.clone();
    while g.bpm() >= 2.0 * range.lo {
        // keep the kick: in drum & bass the snare on the other parity is as loud in full band
        let a = bass_energy(env, g.phase, 2.0 * g.period, duration);
        let b = bass_energy(env, g.phase + g.period, 2.0 * g.period, duration);
        g.phase = if b > a { g.phase + g.period } else { g.phase };
        g.period *= 2.0;
    }
    while g.bpm() < range.lo {
        g.period /= 2.0;
    }
    g
}

/// Every other beat, starting with whichever of the two parities carries more kick energy.
pub fn stronger_parity(env: &Envelope, times: &[f64]) -> Vec<f64> {
    let fps = env.frames_per_sec();
    let energy = |start: usize| -> f64 {
        times.iter().skip(start).step_by(2).map(|&t| series_at(&env.bass, fps, t)).sum()
    };
    let start = if energy(1) > energy(0) { 1 } else { 0 };
    times.iter().skip(start).step_by(2).copied().collect()
}
