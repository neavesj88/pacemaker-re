//! Section detection (intro, build, drop, breakdown, outro) from the beat grid and the audio.
//! Owner: analysis area.
//!
//! The method is deliberately plain and every step can be read here:
//!
//! 1. **Bars from the grid.** [`bar_edges`] turns the stored beat positions and the first
//!    downbeat into bar start times (four beats per bar). Everything after this works in bars,
//!    so a section can only start on a bar line the grid already shows.
//! 2. **Bar features.** [`measure`] decodes nothing itself; it takes mono samples and the bar
//!    edges and gives one [`Row`] per bar: level in three bands (low 20-150 Hz, mid 150-2000 Hz,
//!    high 2-10 kHz), total level, and spectral flux (how busy the bar is). This is the part
//!    worth caching: it needs the audio, but not the detection settings.
//! 3. **Novelty.** [`detect`] normalises every feature to the track's own range, builds a
//!    self-similarity measure between bars and slides a Foote checkerboard kernel along the
//!    diagonal. The novelty is high where the bars before a point look alike, the bars after it
//!    look alike, and the two groups differ.
//! 4. **Phrase snap.** Peaks of the novelty are moved onto the 8 / 16 / 32 bar phrase grid when
//!    one lies within the tolerance; with the phrase length on "auto" the grid is the largest one
//!    that most strong boundaries already sit on (and none when no grid fits).
//! 5. **Labels.** Each section gets a level relative to the loudest and quietest bars of the
//!    same track. First and last sections are intro and outro unless they are loud; loud
//!    sections are drops; quiet ones between others are breakdowns; a rising stretch that leads
//!    into a drop is a build.
//!
//! This is a heuristic, not a trained model, and it was checked on synthetic signals only (see
//! the tests). It will work best on four-on-the-floor dance music with a regular grid and clear
//! level changes; it will be unreliable on music without strong structural level changes, on
//! tracks whose grid or first downbeat is wrong, on tempo changes, and on builds, which are the
//! weakest label. `confidence` is a relative score between 0 and 1 for ranking and for fading
//! markers, not a probability. Everything is deterministic.

use rustfft::{num_complex::Complex, FftPlanner};

/// Bump when the features' definition changes, so cached rows are measured again. Changes to
/// [`detect`] alone do not need it: the cache holds rows, not sections.
pub const VERSION: u32 = 1;
pub const BEATS_PER_BAR: usize = 4;
/// [`measure`] refuses a track with fewer bars than this.
pub const MIN_MEASURE_BARS: usize = 8;
/// Features per bar: low, mid, high band level (dB), total level (dB), spectral flux.
pub const FEATURES: usize = 5;

const FRAME: usize = 2048;
const HOP: usize = 1024;
const LOW_HZ: (f32, f32) = (20.0, 150.0);
const MID_HZ: (f32, f32) = (150.0, 2000.0);
const HIGH_HZ: (f32, f32) = (2000.0, 10000.0);
const FLOOR_DB: f32 = -90.0;
/// A track whose bars are all quieter than this (total level, dBFS) is silent.
const SILENT_DB: f32 = -60.0;

/// One bar's features: `[low_db, mid_db, high_db, total_db, flux]`.
pub type Row = [f32; FEATURES];

/// Why a track gets no rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The grid is missing or unreadable.
    NoGrid,
    /// The grid has no first downbeat, so bars cannot be placed.
    NoDownbeat,
    TooShort,
    Silent,
}

impl Skip {
    pub fn reason(self) -> &'static str {
        match self {
            Skip::NoGrid => "no usable beat grid",
            Skip::NoDownbeat => "the beat grid has no first downbeat set",
            Skip::TooShort => "too short for sections (needs at least 8 bars)",
            Skip::Silent => "silent",
        }
    }
}

fn db(x: f32) -> f32 {
    (10.0 * x.max(1e-12).log10()).max(FLOOR_DB)
}

// ---- bars ------------------------------------------------------------------------------------

/// Bar start times in seconds, from beat times and the index of the first downbeat. Bars run
/// from the first downbeat; a grid that ends before the audio does is continued at its last
/// beat period. The result has one more entry than there are bars (the end of the last bar) and
/// never reaches past `duration` by more than a rounding error. Pre-roll before the first
/// downbeat is not a bar.
pub fn bar_edges(beat_secs: &[f64], first_downbeat: Option<usize>, duration: f64) -> Result<Vec<f64>, Skip> {
    if beat_secs.len() < 2 || beat_secs.iter().any(|t| !t.is_finite()) || beat_secs.windows(2).any(|w| w[1] <= w[0]) {
        return Err(Skip::NoGrid);
    }
    let fd = first_downbeat.ok_or(Skip::NoDownbeat)?;
    if fd >= beat_secs.len() {
        return Err(Skip::NoGrid);
    }
    let mut edges: Vec<f64> = beat_secs.iter().skip(fd).step_by(BEATS_PER_BAR).copied().filter(|&t| t <= duration).collect();
    // a grid is often cut short of the audio: continue it at the period of its last beats
    let tail = &beat_secs[beat_secs.len().saturating_sub(9)..];
    let period = (tail[tail.len() - 1] - tail[0]) / (tail.len() - 1) as f64;
    if let Some(&first) = edges.last() {
        let bar = period * BEATS_PER_BAR as f64;
        let mut last = first;
        while last + bar <= duration + period * 0.5 {
            last += bar;
            edges.push(last);
        }
    }
    if edges.len() < MIN_MEASURE_BARS + 1 {
        return Err(Skip::TooShort);
    }
    Ok(edges)
}

// ---- features --------------------------------------------------------------------------------

/// One [`Row`] per bar between consecutive `edges` (seconds), from mono samples.
pub fn measure(mono: &[f32], sample_rate: u32, edges: &[f64]) -> Result<Vec<Row>, Skip> {
    let n_bars = edges.len().saturating_sub(1);
    if n_bars < MIN_MEASURE_BARS {
        return Err(Skip::TooShort);
    }
    let sr = sample_rate.max(1) as f64;
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FRAME as f32).cos()).collect();
    let win_power: f32 = window.iter().map(|w| w * w).sum();
    let win_amp = win_power.sqrt();
    let hz = sample_rate.max(1) as f32 / FRAME as f32;
    let nyq = FRAME / 2;
    let bins = |(lo, hi): (f32, f32)| ((lo / hz).ceil().max(1.0) as usize, ((hi / hz).floor() as usize).min(nyq));
    let ranges = [bins(LOW_HZ), bins(MID_HZ), bins(HIGH_HZ)];
    let flux_top = ranges[2].1;

    #[derive(Clone, Copy, Default)]
    struct Acc {
        band: [f64; 3],
        total: f64,
        flux: f64,
        n: u32,
    }
    let mut acc = vec![Acc::default(); n_bars];
    let mut prev = vec![0f32; nyq + 1];
    let mut buf = vec![Complex::new(0f32, 0f32); FRAME];
    let n_frames = if mono.len() < FRAME { 0 } else { (mono.len() - FRAME) / HOP + 1 };
    let mut bar = 0usize;
    for f in 0..n_frames {
        let frame = &mono[f * HOP..f * HOP + FRAME];
        for (c, (s, w)) in buf.iter_mut().zip(frame.iter().zip(&window)) {
            *c = Complex::new(s * w, 0.0);
        }
        fft.process(&mut buf);
        // every frame is transformed, also those outside the bars, so the flux of a bar's first
        // frame is measured against the frame before it
        let mut flux = 0f32;
        for b in 1..=flux_top {
            let mag = (1.0 + 100.0 * buf[b].norm() / win_amp).ln();
            // the first frame has nothing before it: its flux is 0, not a jump up from silence
            if f > 0 {
                flux += (mag - prev[b]).max(0.0);
            }
            prev[b] = mag;
        }
        let centre = (f * HOP + FRAME / 2) as f64 / sr;
        if centre < edges[0] || centre >= edges[n_bars] {
            continue;
        }
        while bar + 1 < n_bars && centre >= edges[bar + 1] {
            bar += 1;
        }
        let a = &mut acc[bar];
        for (k, &(lo, hi)) in ranges.iter().enumerate() {
            a.band[k] += (buf[lo.min(nyq)..=hi.max(lo).min(nyq)].iter().map(|c| c.norm_sqr()).sum::<f32>() / win_power) as f64;
        }
        a.total += (frame.iter().map(|s| s * s).sum::<f32>() / FRAME as f32) as f64;
        a.flux += flux as f64;
        a.n += 1;
    }
    let round2 = |x: f32| (x * 100.0).round() / 100.0;
    let mut rows: Vec<Row> = Vec::with_capacity(n_bars);
    for a in &acc {
        if a.n == 0 {
            // a bar shorter than one frame, or past the audio: silence
            rows.push([FLOOR_DB, FLOOR_DB, FLOOR_DB, FLOOR_DB, 0.0]);
            continue;
        }
        let n = a.n as f64;
        rows.push([
            round2(db((a.band[0] / n) as f32)),
            round2(db((a.band[1] / n) as f32)),
            round2(db((a.band[2] / n) as f32)),
            round2(db((a.total / n) as f32)),
            round2((a.flux / n) as f32),
        ]);
    }
    if rows.iter().filter(|r| r[3] > SILENT_DB).count() * 10 < n_bars {
        return Err(Skip::Silent);
    }
    Ok(rows)
}

// ---- detection -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Intro,
    Build,
    Drop,
    Breakdown,
    Outro,
    /// A section that is neither clearly loud nor clearly quiet.
    Section,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Intro => "intro",
            Kind::Build => "build",
            Kind::Drop => "drop",
            Kind::Breakdown => "breakdown",
            Kind::Outro => "outro",
            Kind::Section => "section",
        }
    }
}

/// The settings of [`detect`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// 1 (only obvious changes) to 5 (small changes count).
    pub sensitivity: u8,
    /// No two section boundaries closer than this many bars.
    pub min_section_bars: usize,
    /// Phrase length boundaries snap to: 4, 8, 16 or 32; 0 chooses from the track.
    pub phrase_bars: usize,
    /// Fit the phrase grid's origin to the track instead of using the first downbeat.
    pub best_fit_phase: bool,
    /// A boundary moves onto the phrase grid only if it is at most this many bars away.
    pub snap_tolerance_bars: usize,
}

impl Default for Params {
    fn default() -> Self {
        Params { sensitivity: 3, min_section_bars: 8, phrase_bars: 0, best_fit_phase: false, snap_tolerance_bars: 2 }
    }
}

/// One section, in bars from the first downbeat (`end_bar` exclusive).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub kind: Kind,
    pub start_bar: usize,
    pub end_bar: usize,
    /// 0 to 1: how sure the boundaries and the label are, relative to this track. Not a probability.
    pub confidence: f32,
    /// 0 to 1: how strong the boundary at `start_bar` is (1 for the start of the track).
    pub start_confidence: f32,
    /// The start sits on the phrase grid.
    pub on_phrase: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub sections: Vec<Section>,
    /// The phrase length boundaries were snapped to; 0 when no phrase grid fits the track.
    pub phrase_bars: usize,
    /// Bar (from the first downbeat) at which the phrase grid starts.
    pub phrase_offset: usize,
    pub bars: usize,
}

impl Detection {
    /// The bars at which a new section starts, without the start of the track.
    pub fn boundary_bars(&self) -> Vec<usize> {
        self.sections.iter().skip(1).map(|s| s.start_bar).collect()
    }
}

/// Why no sections came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoStructure {
    TooShort,
    /// The features never change enough, at this sensitivity, to call anything a boundary.
    Flat,
}

impl NoStructure {
    pub fn reason(self) -> &'static str {
        match self {
            NoStructure::TooShort => "too short for sections",
            NoStructure::Flat => "no clear sections at this sensitivity",
        }
    }
}

/// Weight of each feature in the distance between bars: the total level and the low end count
/// most, the highs and the flux least.
const WEIGHTS: [f32; FEATURES] = [1.0, 1.0, 0.8, 1.2, 0.8];
/// A level feature never has a range narrower than this, so a nearly constant track does not
/// have its noise stretched into structure.
const DB_MIN_RANGE: f32 = 6.0;
/// Bandwidth of the similarity, in the normalised units (1.0 is a track's full feature range).
const BANDWIDTH: f32 = 0.5;
/// Novelty (0 to 0.5) at the sensitivities 1 to 5.
const NOVELTY_FLOOR: [f32; 5] = [0.17, 0.13, 0.10, 0.075, 0.05];
/// A peak weaker than this share of the strongest one is not a boundary.
const REL_FLOOR: f32 = 0.2;
/// Novelty that counts as a fully confident boundary.
const NOVELTY_FULL: f32 = 0.35;
/// Level thresholds, relative to the track's quietest (0) and loudest (1) bars.
const HIGH: f32 = 0.62;
const LOW: f32 = 0.38;
/// Rise (share of the track's level range) that makes a stretch a build.
const BUILD_RISE: f32 = 0.25;
const PHRASES: [usize; 3] = [32, 16, 8];
/// Share of strong boundaries that must sit on a phrase grid (within a bar) for "auto" to pick it.
const AUTO_SHARE: f32 = 0.7;

fn percentile(sorted: &[f32], p: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * p).round() as usize]
}

struct Norm {
    /// Weighted, normalised features: the space distances are measured in.
    v: Vec<[f32; FEATURES]>,
    /// One number per bar, 0 (quiet) to 1 (loud): total level, low end and flux.
    level: Vec<f32>,
}

fn normalise(rows: &[Row]) -> Norm {
    let n = rows.len();
    let mut z = vec![[0f32; FEATURES]; n];
    for k in 0..FEATURES {
        let mut col: Vec<f32> = rows.iter().map(|r| r[k]).collect();
        col.sort_by(|a, b| a.total_cmp(b));
        let (lo, hi) = (percentile(&col, 0.05), percentile(&col, 0.95));
        let floor = if k < 4 { DB_MIN_RANGE } else { 0.3 * hi.max(1e-3) };
        let range = (hi - lo).max(floor);
        for i in 0..n {
            z[i][k] = ((rows[i][k] - lo) / range).clamp(-1.0, 2.0);
        }
    }
    let level = z.iter().map(|r| 0.4 * r[3] + 0.35 * r[0] + 0.25 * r[4]).collect();
    let v = z.iter().map(|r| std::array::from_fn(|k| r[k] * WEIGHTS[k])).collect();
    Norm { v, level }
}

fn similarity(a: &[f32; FEATURES], b: &[f32; FEATURES]) -> f32 {
    let d2: f32 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
    (-d2 / (2.0 * BANDWIDTH * BANDWIDTH)).exp()
}

/// Foote novelty at every bar boundary `t` (the first bar of a new section): high when the
/// `half` bars before `t` resemble each other, so do the `half` after, and the two groups do
/// not resemble each other. 0.5 is a perfect step, 0 a flat track. Near the ends the kernel
/// shrinks to what fits (and is 0 where fewer than two bars fit on a side).
fn novelty(v: &[[f32; FEATURES]], half: usize) -> Vec<f32> {
    let n = v.len();
    let mut out = vec![0f32; n];
    for t in 1..n {
        let l = half.min(t).min(n - t);
        if l < 2 {
            continue;
        }
        let sigma = l as f32 / 2.0;
        // weight by distance from the boundary, bar j away (before: t-1-j, after: t+j)
        let w: Vec<f32> = (0..l).map(|j| (-((j as f32 + 0.5).powi(2)) / (2.0 * sigma * sigma)).exp()).collect();
        let (mut num, mut den) = (0f32, 0f32);
        for a in 0..2 * l {
            // a < l: bar t-1-a (before); else bar t+(a-l) (after)
            let (ia, wa, sa) = if a < l { (t - 1 - a, w[a], -1.0f32) } else { (t + a - l, w[a - l], 1.0f32) };
            for b in 0..2 * l {
                let (ib, wb, sb) = if b < l { (t - 1 - b, w[b], -1.0f32) } else { (t + b - l, w[b - l], 1.0f32) };
                let k = wa * wb;
                num += k * sa * sb * similarity(&v[ia], &v[ib]);
                den += k;
            }
        }
        out[t] = num / den;
    }
    out
}

/// Local maxima of the novelty above `floor`: `(bar, novelty)`.
fn peaks(nov: &[f32], floor: f32) -> Vec<(usize, f32)> {
    let n = nov.len();
    let mut out = Vec::new();
    for t in 1..n {
        if nov[t] < floor {
            continue;
        }
        let (a, b) = (t.saturating_sub(2).max(1), (t + 2).min(n - 1));
        // the strongest within two bars; on a tie the earlier bar wins
        if (a..=b).all(|k| nov[k] < nov[t] || (nov[k] == nov[t] && k >= t)) {
            out.push((t, nov[t]));
        }
    }
    out
}

/// Nearest line of the phrase grid (`offset + k * phrase`) to bar `t`.
fn nearest_line(t: usize, phrase: usize, offset: usize) -> isize {
    let k = ((t as f32 - offset as f32) / phrase as f32).round() as isize;
    offset as isize + k * phrase as isize
}

/// The phrase grid origin (0 to `phrase`-1) with the most strong boundaries on or next to its
/// lines. Ties and near ties go to the first downbeat: the origin moves only for a clear gain.
fn best_offset(strong: &[(usize, f32)], phrase: usize) -> usize {
    let score = |o: usize| -> f32 {
        strong.iter().map(|&(t, n)| (1.0 - (t as isize - nearest_line(t, phrase, o)).abs() as f32 / 1.5).max(0.0) * n).sum()
    };
    let base = score(0);
    let (mut best, mut best_score) = (0, base);
    for o in 1..phrase {
        let s = score(o);
        if s > best_score {
            (best, best_score) = (o, s);
        }
    }
    if best_score > base * 1.3 + 0.05 {
        best
    } else {
        0
    }
}

fn on_grid_share(strong: &[(usize, f32)], phrase: usize, offset: usize) -> f32 {
    if strong.is_empty() {
        return 0.0;
    }
    let hit = strong.iter().filter(|&&(t, _)| (t as isize - nearest_line(t, phrase, offset)).abs() <= 1).count();
    hit as f32 / strong.len() as f32
}

#[derive(Clone, Copy)]
struct Draft {
    kind: Kind,
    start: usize,
    end: usize,
    start_conf: f32,
    on_phrase: bool,
    /// How decisive the label is, 0.4 to 1.
    label_conf: f32,
    /// Mean level relative to the track's quiet (0) and loud (1) bars.
    r: f32,
}

fn mean(x: &[f32]) -> f32 {
    if x.is_empty() {
        0.0
    } else {
        x.iter().sum::<f32>() / x.len() as f32
    }
}

/// Last third minus first third of a stretch of level values, as a share of the level range.
fn rise(level: &[f32], span: f32) -> f32 {
    let third = (level.len() / 3).max(1);
    (mean(&level[level.len() - third..]) - mean(&level[..third])) / span
}

/// Finds the sections of a track from its bar features.
pub fn detect(rows: &[Row], p: &Params) -> Result<Detection, NoStructure> {
    let n = rows.len();
    let min_len = p.min_section_bars.max(2);
    if n < 2 * min_len {
        return Err(NoStructure::TooShort);
    }
    let norm = normalise(rows);
    let half = (min_len / 2).clamp(2, 8);
    let nov = novelty(&norm.v, half);
    let nov_max = nov.iter().copied().fold(0.0f32, f32::max);
    let floor = NOVELTY_FLOOR[(p.sensitivity.clamp(1, 5) - 1) as usize].max(REL_FLOOR * nov_max);
    let raw = peaks(&nov, floor);
    if raw.is_empty() {
        return Err(NoStructure::Flat);
    }

    // the phrase grid
    let strong: Vec<(usize, f32)> = raw.iter().copied().filter(|&(_, v)| v >= 0.5 * nov_max).collect();
    let (phrase, offset) = if p.phrase_bars > 0 {
        let off = if p.best_fit_phase { best_offset(&strong, p.phrase_bars) } else { 0 };
        (p.phrase_bars, off)
    } else if strong.len() < 2 {
        // too little evidence to tell: the usual dance-music phrase
        (8, 0)
    } else {
        PHRASES
            .iter()
            .map(|&ph| (ph, if p.best_fit_phase { best_offset(&strong, ph) } else { 0 }))
            .find(|&(ph, off)| on_grid_share(&strong, ph, off) >= AUTO_SHARE)
            .unwrap_or((0, 0))
    };

    // snap, then keep the strongest boundaries that are far enough apart
    let edge_min = (min_len / 2).max(2);
    let mut cand: Vec<(usize, f32, bool)> = raw
        .iter()
        .map(|&(t, v)| {
            if phrase > 0 {
                let line = nearest_line(t, phrase, offset);
                // a line at or past the end of the track is no boundary (a wide tolerance can reach one)
                if line >= 1 && (line as usize) < n && (t as isize - line).unsigned_abs() <= p.snap_tolerance_bars {
                    return (line as usize, v, true);
                }
            }
            (t, v, false)
        })
        .filter(|&(t, _, _)| t >= edge_min && t + edge_min <= n)
        .collect();
    cand.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut picked: Vec<(usize, f32, bool)> = Vec::new();
    for c in cand {
        if picked.iter().all(|q| q.0.abs_diff(c.0) >= min_len) {
            picked.push(c);
        }
    }
    if picked.is_empty() {
        return Err(NoStructure::Flat);
    }
    picked.sort_by_key(|q| q.0);

    // sections between the boundaries
    let mut cuts = vec![(0usize, 1.0f32, true)];
    cuts.extend(picked.iter().map(|&(t, v, on)| (t, (v / NOVELTY_FULL).clamp(0.05, 1.0) * if phrase > 0 && !on { 0.8 } else { 1.0 }, on)));
    let mut level_sorted = norm.level.clone();
    level_sorted.sort_by(|a, b| a.total_cmp(b));
    let (ref_lo, ref_hi) = (percentile(&level_sorted, 0.1), percentile(&level_sorted, 0.9));
    let span = (ref_hi - ref_lo).max(0.12);
    let contrast = ((ref_hi - ref_lo) / 0.5).clamp(0.35, 1.0);
    let rel = |a: usize, b: usize| (mean(&norm.level[a..b]) - ref_lo) / span;

    let mut drafts: Vec<Draft> = Vec::new();
    let last = cuts.len() - 1;
    for (i, &(start, start_conf, on)) in cuts.iter().enumerate() {
        let end = if i == last { n } else { cuts[i + 1].0 };
        let r = rel(start, end);
        let (kind, label_conf) = if r >= HIGH {
            (Kind::Drop, 0.5 + 0.5 * ((r - HIGH) / 0.38).clamp(0.0, 1.0))
        } else if i == 0 {
            (Kind::Intro, 0.5 + 0.5 * ((HIGH - r) / 0.5).clamp(0.0, 1.0))
        } else if i == last {
            (Kind::Outro, 0.5 + 0.5 * ((HIGH - r) / 0.5).clamp(0.0, 1.0))
        } else if r <= LOW {
            (Kind::Breakdown, 0.5 + 0.5 * ((LOW - r) / 0.38).clamp(0.0, 1.0))
        } else {
            (Kind::Section, 0.4)
        };
        drafts.push(Draft { kind, start, end, start_conf, on_phrase: on, label_conf, r });
    }

    // builds: a rising stretch that leads into a stronger drop
    let pm = if phrase > 0 { phrase } else { 8 };
    let mut out: Vec<Draft> = Vec::new();
    for i in 0..drafts.len() {
        let d = &drafts[i];
        let len = d.end - d.start;
        // a build leads into a drop that is clearly stronger than it (a drop that is itself a
        // loud stretch can still be the run-up to a bigger one)
        let leads_up = drafts.get(i + 1).is_some_and(|nx| nx.r >= HIGH && nx.r - d.r >= 0.12);
        if !leads_up || d.kind == Kind::Outro || len < 4 {
            out.push(*d);
            continue;
        }
        let lv = &norm.level[d.start..d.end];
        let whole = rise(lv, span);
        if i > 0 && len <= 2 * pm && whole >= BUILD_RISE {
            out.push(Draft { kind: Kind::Build, label_conf: (whole / 0.6).clamp(0.2, 1.0), ..*d });
            continue;
        }
        // the longest tail (two phrases, then one) that rises throughout while the part before is flat
        let tail = [2 * pm, pm].into_iter().find(|&tl| {
            if len < tl + (min_len / 2).max(2) {
                return false;
            }
            let (head, tl_lv) = lv.split_at(len - tl);
            rise(tl_lv, span) >= BUILD_RISE && rise(&tl_lv[..tl / 2], span) >= 0.08 && rise(head, span) < 0.2
        });
        match tail {
            Some(tl) => {
                let split = d.end - tl;
                let r = rise(&lv[len - tl..], span);
                out.push(Draft { end: split, ..*d });
                out.push(Draft {
                    kind: Kind::Build,
                    start: split,
                    end: d.end,
                    start_conf: (r / 0.6).clamp(0.1, 1.0),
                    on_phrase: phrase > 0 && split >= offset && (split - offset) % phrase == 0,
                    label_conf: (r / 0.6).clamp(0.2, 1.0),
                    r: d.r,
                });
            }
            None => out.push(*d),
        }
    }

    let sections = out
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let end_conf = out.get(i + 1).map_or(1.0, |nx| nx.start_conf);
            let edges = (d.start_conf + end_conf) / 2.0;
            let confidence = ((0.6 * edges + 0.4 * d.label_conf) * contrast).clamp(0.0, 1.0);
            Section {
                kind: d.kind,
                start_bar: d.start,
                end_bar: d.end,
                confidence: (confidence * 100.0).round() / 100.0,
                start_confidence: (d.start_conf * 100.0).round() / 100.0,
                on_phrase: d.on_phrase,
            }
        })
        .collect();
    Ok(Detection { sections, phrase_bars: phrase, phrase_offset: offset, bars: n })
}

// ---- snapping --------------------------------------------------------------------------------

/// The boundary (seconds, ascending) nearest to `t` within `max_dist` seconds, or `None`.
/// Ties go to the earlier boundary. The mix editor's snap uses the same rule on the section
/// starts of a track (`sectionLogic.ts`).
pub fn snap_secs(boundaries: &[f64], t: f64, max_dist: f64) -> Option<f64> {
    let i = boundaries.partition_point(|&b| b < t);
    [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter_map(|k| boundaries.get(k).copied())
        .map(|b| (b, (b - t).abs()))
        .filter(|&(_, d)| d <= max_dist)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(b, _)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 22050;
    const BPM: f64 = 128.0;
    const BAR: f64 = 4.0 * 60.0 / BPM;

    struct Rng(u32);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
        }
    }

    /// What plays in a stretch of bars.
    #[derive(Clone, Copy)]
    struct Layer {
        kick: f32,
        bass: f32,
        hats: f32,
        pad: f32,
        /// Noise riser from 0 to this level across the stretch.
        riser: f32,
    }
    const HATS: Layer = Layer { kick: 0.0, bass: 0.0, hats: 0.12, pad: 0.0, riser: 0.0 };
    const DROP: Layer = Layer { kick: 0.9, bass: 0.35, hats: 0.3, pad: 0.1, riser: 0.0 };
    const BREAK: Layer = Layer { kick: 0.0, bass: 0.0, hats: 0.0, pad: 0.12, riser: 0.0 };
    const BUILD: Layer = Layer { kick: 0.0, bass: 0.0, hats: 0.12, pad: 0.12, riser: 0.2 };

    /// A track made of `(bars, layers)` stretches at 128 BPM, a bar line every `BAR` seconds from `pre` seconds.
    fn render(parts: &[(usize, Layer)], pre: f64) -> Vec<f32> {
        let total_bars: usize = parts.iter().map(|p| p.0).sum();
        let n = ((pre + total_bars as f64 * BAR) * SR as f64) as usize + SR as usize;
        let mut out = vec![0f32; n];
        let mut rng = Rng(2463534242);
        let beat = 60.0 / BPM;
        let mut bar0 = 0usize;
        for &(bars, l) in parts {
            let (t0, t1) = (pre + bar0 as f64 * BAR, pre + (bar0 + bars) as f64 * BAR);
            let (s0, s1) = ((t0 * SR as f64) as usize, (t1 * SR as f64) as usize);
            for i in s0..s1.min(n) {
                let t = i as f64 / SR as f64;
                let within = ((t - t0) / (t1 - t0)) as f32;
                let mut x = 0.0f32;
                // continuous layers
                x += l.bass * (2.0 * std::f32::consts::PI * 45.0 * t as f32).sin();
                x += l.pad * ((2.0 * std::f32::consts::PI * 220.0 * t as f32).sin() + (2.0 * std::f32::consts::PI * 330.0 * t as f32).sin()) * 0.5;
                x += l.riser * within * rng.next();
                // beat-locked layers
                let in_beat = ((t - pre) / beat).fract() as f32 * beat as f32;
                if l.kick > 0.0 && in_beat < 0.3 {
                    x += l.kick * (2.0 * std::f32::consts::PI * 55.0 * in_beat).sin() * (-in_beat * 14.0).exp();
                }
                // off-beat hat
                let off = (((t - pre) / beat) + 0.5).fract() as f32 * beat as f32;
                if l.hats > 0.0 && off < 0.04 {
                    x += l.hats * rng.next() * (-off * 90.0).exp();
                }
                out[i] = x;
            }
            bar0 += bars;
        }
        out
    }

    fn edges_for(total_bars: usize, pre: f64) -> Vec<f64> {
        (0..=total_bars).map(|k| pre + k as f64 * BAR).collect()
    }

    fn rows_of(parts: &[(usize, Layer)]) -> Vec<Row> {
        let bars: usize = parts.iter().map(|p| p.0).sum();
        measure(&render(parts, 0.0), SR, &edges_for(bars, 0.0)).unwrap()
    }

    fn kinds(d: &Detection) -> Vec<&'static str> {
        d.sections.iter().map(|s| s.kind.as_str()).collect()
    }

    fn starts(d: &Detection) -> Vec<usize> {
        d.sections.iter().map(|s| s.start_bar).collect()
    }

    /// intro 16 | build 8 | drop 32 | breakdown 16 | build 8 | drop 16 | outro 8
    fn full_track() -> Vec<(usize, Layer)> {
        vec![(16, HATS), (8, BUILD), (32, DROP), (16, BREAK), (8, BUILD), (16, DROP), (8, HATS)]
    }

    #[test]
    fn finds_the_boundaries_and_labels_of_a_full_track() {
        let d = detect(&rows_of(&full_track()), &Params::default()).unwrap();
        // true boundaries: 16 24 56 72 80 96
        let truth = [16usize, 24, 56, 72, 80, 96];
        let found = d.boundary_bars();
        for t in truth {
            assert!(found.iter().any(|&f| f.abs_diff(t) <= 1), "boundary {t} missing in {found:?}");
        }
        assert!(found.len() <= truth.len() + 1, "extra boundaries: {found:?}");
        let ks = kinds(&d);
        assert_eq!(ks.first(), Some(&"intro"), "{ks:?}");
        assert_eq!(ks.last(), Some(&"outro"), "{ks:?}");
        assert_eq!(ks.iter().filter(|k| **k == "drop").count(), 2, "{ks:?}");
        assert!(ks.contains(&"breakdown"), "{ks:?}");
        let at = |bar: usize| d.sections.iter().find(|s| s.start_bar <= bar && bar < s.end_bar).unwrap().kind.as_str();
        assert_eq!(at(40), "drop");
        assert_eq!(at(20), "build");
        assert_eq!(at(64), "breakdown");
        assert_eq!(at(76), "build");
        assert_eq!(at(88), "drop");
        assert_eq!(at(100), "outro");
        assert_eq!(at(5), "intro");
        // sections tile the track with no gaps
        assert_eq!(d.sections[0].start_bar, 0);
        assert_eq!(d.sections.last().unwrap().end_bar, 104);
        assert!(d.sections.windows(2).all(|w| w[0].end_bar == w[1].start_bar));
        assert!(d.sections.iter().all(|s| (0.0..=1.0).contains(&s.confidence) && s.end_bar > s.start_bar));
    }

    #[test]
    fn boundaries_land_on_the_phrase_grid() {
        let d = detect(&rows_of(&full_track()), &Params::default()).unwrap();
        assert!(d.phrase_bars == 8 || d.phrase_bars == 16 || d.phrase_bars == 32, "phrase {}", d.phrase_bars);
        assert!(d.boundary_bars().iter().all(|b| b % 8 == 0), "{:?}", d.boundary_bars());
        assert!(d.sections.iter().skip(1).all(|s| s.on_phrase));
    }

    #[test]
    fn builds_are_found_before_drops() {
        let d = detect(&rows_of(&full_track()), &Params::default()).unwrap();
        let ks = kinds(&d);
        let builds: Vec<_> = d.sections.iter().filter(|s| s.kind == Kind::Build).collect();
        assert!(!builds.is_empty(), "no build in {ks:?}");
        for b in builds {
            // every build ends where a drop begins
            assert!(d.sections.iter().any(|s| s.kind == Kind::Drop && s.start_bar == b.end_bar), "{ks:?} {:?}", starts(&d));
        }
    }

    #[test]
    fn a_ramp_inside_a_section_is_split_off_as_a_build() {
        // features given directly: quiet to bar 32, a ramp to bar 40 that has no step of its own
        // at its start, a loud stretch to bar 72, then quiet again
        let quiet: Row = [-40.0, -30.0, -30.0, -35.0, 20.0];
        let loud: Row = [10.0, 0.0, -5.0, -10.0, 150.0];
        let rows: Vec<Row> = (0..96)
            .map(|i| match i {
                0..=31 | 72..=95 => quiet,
                32..=39 => {
                    let x = (i - 32) as f32 / 8.0;
                    std::array::from_fn(|k| quiet[k] + (loud[k] - quiet[k]) * x * 0.85)
                }
                _ => loud,
            })
            .collect();
        let d = detect(&rows, &Params::default()).unwrap();
        assert_eq!(kinds(&d), vec!["intro", "build", "drop", "outro"], "{:?}", starts(&d));
        assert_eq!(starts(&d), vec![0, 32, 40, 72]);
        assert!(d.sections[1].on_phrase);
    }

    #[test]
    fn a_constant_groove_has_no_sections() {
        let r = detect(&rows_of(&[(64, DROP)]), &Params::default());
        assert_eq!(r, Err(NoStructure::Flat));
    }

    #[test]
    fn a_track_with_one_change_gets_two_sections() {
        let d = detect(&rows_of(&[(16, HATS), (32, DROP)]), &Params::default()).unwrap();
        assert_eq!(kinds(&d), vec!["intro", "drop"]);
        assert!(d.boundary_bars()[0].abs_diff(16) <= 1);
    }

    #[test]
    fn a_boundary_one_bar_early_snaps_to_the_phrase_line() {
        // the true change is at bar 15; with an 8-bar phrase and a 2-bar tolerance it is moved to 16
        let parts = [(15, HATS), (33, DROP)];
        let p = Params { phrase_bars: 8, ..Params::default() };
        let d = detect(&rows_of(&parts), &p).unwrap();
        assert_eq!(d.boundary_bars(), vec![16]);
        assert!(d.sections[1].on_phrase);
        // without a tolerance it stays where the music changes, flagged as off the grid, and weaker
        let strict = detect(&rows_of(&parts), &Params { snap_tolerance_bars: 0, ..p }).unwrap();
        assert_eq!(strict.boundary_bars().len(), 1);
        assert!(strict.boundary_bars()[0].abs_diff(15) <= 1);
        if strict.boundary_bars()[0] != 16 {
            assert!(!strict.sections[1].on_phrase);
            assert!(strict.sections[1].start_confidence < d.sections[1].start_confidence);
        }
    }

    #[test]
    fn auto_phrase_prefers_the_longest_grid_the_boundaries_sit_on() {
        // boundaries at 32 and 64: a 32-bar grid fits
        let d = detect(&rows_of(&[(32, HATS), (32, DROP), (32, BREAK), (32, DROP)]), &Params::default()).unwrap();
        assert_eq!(d.phrase_bars, 32, "{:?}", d.boundary_bars());
        // boundaries at 16 and 48: 16 fits, 32 does not
        let d = detect(&rows_of(&[(16, HATS), (32, DROP), (16, BREAK), (32, DROP)]), &Params::default()).unwrap();
        assert_eq!(d.phrase_bars, 16, "{:?}", d.boundary_bars());
    }

    #[test]
    fn no_phrase_grid_when_boundaries_are_scattered() {
        // boundaries at 11, 29, 45 sit on no common grid
        let d = detect(&rows_of(&[(11, HATS), (18, DROP), (16, BREAK), (19, DROP)]), &Params::default()).unwrap();
        assert_eq!(d.phrase_bars, 0, "{:?}", d.boundary_bars());
        assert!(d.sections.iter().skip(1).all(|s| !s.on_phrase));
        // the boundaries stay where the music changes
        for t in [11usize, 29, 45] {
            assert!(d.boundary_bars().iter().any(|&b| b.abs_diff(t) <= 1), "{t} missing in {:?}", d.boundary_bars());
        }
    }

    #[test]
    fn best_fit_phase_follows_a_track_whose_phrases_start_late() {
        // a 4-bar pre-roll before the phrases begin: changes at 4+16 and 4+48 (bars 20, 52)
        let parts = [(20, HATS), (32, DROP), (16, BREAK), (32, DROP)];
        let rows = rows_of(&parts);
        let first_downbeat = Params { phrase_bars: 16, snap_tolerance_bars: 0, ..Params::default() };
        let fitted = Params { best_fit_phase: true, ..first_downbeat };
        let a = detect(&rows, &first_downbeat).unwrap();
        let b = detect(&rows, &fitted).unwrap();
        assert_eq!(a.phrase_offset, 0);
        assert_eq!(b.phrase_offset, 4, "{:?}", b.boundary_bars());
        assert!(b.sections.iter().skip(1).all(|s| s.on_phrase), "{:?}", b.boundary_bars());
        assert!(a.sections.iter().skip(1).any(|s| !s.on_phrase));
    }

    #[test]
    fn sensitivity_changes_how_much_is_found() {
        // a small change (the highs of the groove drop out) is found only at high sensitivity
        let quiet_hats = Layer { hats: 0.12, ..DROP };
        let parts = [(24, DROP), (24, quiet_hats), (24, DROP)];
        let rows = rows_of(&parts);
        let low = detect(&rows, &Params { sensitivity: 1, ..Params::default() });
        let high = detect(&rows, &Params { sensitivity: 5, ..Params::default() }).unwrap();
        assert!(low.map(|d| d.sections.len()).unwrap_or(0) <= high.sections.len());
        assert!(high.sections.len() >= 2, "{:?}", starts(&high));
    }

    #[test]
    fn a_larger_minimum_section_drops_the_short_ones() {
        let parts = [(16, HATS), (8, DROP), (16, BREAK), (24, DROP)];
        let rows = rows_of(&parts);
        let short = detect(&rows, &Params { min_section_bars: 4, ..Params::default() }).unwrap();
        let long = detect(&rows, &Params { min_section_bars: 16, ..Params::default() }).unwrap();
        assert!(long.sections.len() < short.sections.len(), "{} vs {}", long.sections.len(), short.sections.len());
        assert!(long.sections.iter().all(|s| s.end_bar - s.start_bar >= 8));
    }

    #[test]
    fn too_short_tracks_are_refused() {
        let rows: Vec<Row> = vec![[-30.0, -30.0, -40.0, -25.0, 1.0]; 10];
        assert_eq!(detect(&rows, &Params::default()), Err(NoStructure::TooShort));
        assert!(matches!(measure(&render(&[(4, DROP)], 0.0), SR, &edges_for(4, 0.0)), Err(Skip::TooShort)));
    }

    #[test]
    fn silence_is_refused() {
        let mono = vec![0f32; (BAR * 20.0 * SR as f64) as usize];
        assert_eq!(measure(&mono, SR, &edges_for(16, 0.0)), Err(Skip::Silent));
    }

    #[test]
    fn detection_is_deterministic_and_ignores_pre_roll_in_the_audio() {
        let parts = full_track();
        let a = detect(&rows_of(&parts), &Params::default()).unwrap();
        let b = detect(&rows_of(&parts), &Params::default()).unwrap();
        assert_eq!(a, b);
        // the same music starting 1.3 s into the file, with the bar lines given from there
        let bars: usize = parts.iter().map(|p| p.0).sum();
        let shifted = measure(&render(&parts, 1.3), SR, &edges_for(bars, 1.3)).unwrap();
        let c = detect(&shifted, &Params::default()).unwrap();
        assert_eq!(starts(&a), starts(&c));
        assert_eq!(kinds(&a), kinds(&c));
    }

    #[test]
    fn bar_rows_follow_the_music() {
        let rows = rows_of(&[(8, HATS), (8, DROP)]);
        assert_eq!(rows.len(), 16);
        // the groove is louder, bassier and busier than the hats
        for k in [0usize, 3] {
            assert!(rows[12][k] > rows[3][k] + 10.0, "feature {k}: {} vs {}", rows[12][k], rows[3][k]);
        }
        assert!(rows[12][4] > rows[3][4]);
    }

    #[test]
    fn bar_edges_start_at_the_first_downbeat_and_continue_the_grid() {
        let beat = 60.0 / BPM;
        // 2 pre-roll beats, then 40 beats (10 bars); the audio is 60 s long
        let beats: Vec<f64> = (0..42).map(|i| 0.5 + i as f64 * beat).collect();
        let e = bar_edges(&beats, Some(2), 60.0).unwrap();
        assert!((e[0] - (0.5 + 2.0 * beat)).abs() < 1e-9);
        assert!(e.windows(2).all(|w| ((w[1] - w[0]) - BAR).abs() < 1e-6));
        // continued past the stored beats up to the end of the audio
        assert!(e.len() > 11, "{}", e.len());
        assert!(*e.last().unwrap() <= 60.0 + beat);
        assert!(*e.last().unwrap() > 60.0 - BAR - beat);
        // errors
        assert_eq!(bar_edges(&beats, None, 60.0), Err(Skip::NoDownbeat));
        assert_eq!(bar_edges(&beats, Some(99), 60.0), Err(Skip::NoGrid));
        assert_eq!(bar_edges(&[1.0], Some(0), 60.0), Err(Skip::NoGrid));
        assert_eq!(bar_edges(&[2.0, 1.0, 3.0], Some(0), 60.0), Err(Skip::NoGrid));
        assert_eq!(bar_edges(&beats[..10], Some(2), 8.0), Err(Skip::TooShort));
    }

    #[test]
    fn snap_secs_picks_the_nearest_boundary_in_reach() {
        let b = [10.0, 20.0, 30.0];
        assert_eq!(snap_secs(&b, 11.0, 2.0), Some(10.0));
        assert_eq!(snap_secs(&b, 19.0, 2.0), Some(20.0));
        assert_eq!(snap_secs(&b, 25.0, 6.0), Some(20.0), "a tie goes to the earlier boundary");
        assert_eq!(snap_secs(&b, 25.0, 4.0), None);
        assert_eq!(snap_secs(&b, 0.0, 2.0), None);
        assert_eq!(snap_secs(&b, 31.5, 2.0), Some(30.0));
        assert_eq!(snap_secs(&[], 5.0, 100.0), None);
    }

    /// Blocky random bar features: stretches of one level with a few jumps, the shape that makes
    /// the detector find boundaries.
    fn blocky_rows(seed: &mut u64, n: usize) -> Vec<Row> {
        let mut next = move |seed: &mut u64| {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (*seed >> 33) as f32 / (1u64 << 31) as f32
        };
        let mut rows = Vec::with_capacity(n);
        let mut cur: Row = [0.0; FEATURES];
        let mut left = 0usize;
        while rows.len() < n {
            if left == 0 {
                cur = std::array::from_fn(|_| next(seed) * 120.0 - 60.0);
                left = 1 + (next(seed) * 20.0) as usize;
            }
            left -= 1;
            rows.push(std::array::from_fn(|k| cur[k] + next(seed) * 2.0));
        }
        rows
    }

    #[test]
    fn a_wide_snap_tolerance_never_puts_a_boundary_past_the_end() {
        // a step a few bars from the end of the track: the nearest phrase line can be past the
        // end, and must not become a boundary (it used to slice out of range)
        let quiet: Row = [-40.0, -30.0, -30.0, -35.0, 20.0];
        let loud: Row = [10.0, 0.0, -5.0, -10.0, 150.0];
        for n in 16..=48usize {
            for step in n - 8..n - 3 {
                let rows: Vec<Row> = (0..n).map(|i| if i < step { quiet } else { loud }).collect();
                for phrase_bars in [4usize, 8, 16, 32] {
                    for tol in 0..=8 {
                        let p = Params { phrase_bars, snap_tolerance_bars: tol, min_section_bars: 4, ..Params::default() };
                        if let Ok(d) = detect(&rows, &p) {
                            let ctx = format!("n {n} step {step} phrase {phrase_bars} tol {tol}");
                            assert!(d.sections.iter().all(|s| s.end_bar <= n && s.start_bar < s.end_bar), "{ctx}");
                            assert!(d.boundary_bars().iter().all(|&b| b < n), "{ctx}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn detect_never_panics_over_the_whole_settings_range() {
        let mut seed = 0x5eed_u64;
        let mut ok = 0;
        for round in 0..6000 {
            let n = 8 + (round * 7) % 190;
            let rows = blocky_rows(&mut seed, n);
            let p = Params {
                sensitivity: 1 + (round % 5) as u8,
                min_section_bars: 4 + (round * 3) % 29,
                phrase_bars: [0usize, 4, 8, 16, 32][round % 5],
                best_fit_phase: round % 3 == 0,
                snap_tolerance_bars: (round / 5) % 9,
            };
            if let Ok(d) = detect(&rows, &p) {
                ok += 1;
                assert!(d.sections.windows(2).all(|w| w[0].end_bar == w[1].start_bar), "round {round}");
                assert_eq!(d.sections.last().unwrap().end_bar, n, "round {round}");
            }
        }
        assert!(ok > 500, "the fuzz should reach the snapping code, only {ok} tracks had structure");
    }
}
