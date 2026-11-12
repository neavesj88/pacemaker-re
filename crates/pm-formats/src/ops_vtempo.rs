//! Variable-tempo beat grids: tempo sections, single-beat edits and drift against the audio
//! (owner: grid area).
//!
//! # What the `.str` can hold
//!
//! The sidecar's `<beats>` document lists one `<beat pos="..." bpm="..."/>` per beat
//! ([`crate::strfile::Beat`]): an integer position in source sample frames and a local tempo,
//! plus one `firstDownBeat` marker. There is no separate tempo-segment structure and no single
//! stored tempo, so a grid with any tempo map is representable: every beat has its own position.
//! The analyser already writes such grids for tracks it finds variable (`pm_analysis`, the
//! non-locked branch) and two of the 240 sidecars on the reference device are variable-tempo
//! (FINDINGS 4c). The functions here only produce grids that [`crate::ops_grid::validate`]
//! accepts (strictly increasing positions, sane tempi), written by the same
//! `Library::save_grid` path as every other grid edit.
//!
//! UNVERIFIED on hardware: that the player places each beat where the file says (the decompiled
//! player feeds the position list to `TimeStretch::generateBeatVector`, FINDINGS 4) rather than
//! smoothing it. The tempo shown for a track (`tracks.bpm`) is the median of the beats' `bpm`.
//!
//! # Conventions
//!
//! * A beat's `bpm` is the local tempo: when a beat is moved the `bpm` of the beats around it is
//!   re-derived from the gaps (`60 * rate / gap`, the last beat takes the gap before it, as the
//!   analyser does); a section set to a tempo, or evened out, labels its beats with that exact
//!   tempo.
//! * Bar lines are `first_downbeat + 4k`. Edits that add or remove beats keep the bar line on
//!   the same beat and reduce the index modulo 4.
//! * Onsets handed to [`drift`] are in the device convention (see `pm_analysis::onsets`), so a
//!   grid that sits on the music has offsets near zero; a constant offset is a nudge, not drift.

use serde::{Deserialize, Serialize};

use crate::library::Error;
use crate::ops_grid::{clip, finish, invalid, period_frames, validate, EditCtx, GridBeat, GridDoc, BAR, MAX_BPM, MIN_BPM};

/// One detected attack in the audio, in source frames (device convention) with its strength.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Onset {
    pub frame: i64,
    pub strength: f32,
}

/// How onsets are matched to beat lines and what counts as drift. Every number is a user setting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DriftOptions {
    /// An onset belongs to a beat when it is within this percentage of the beat period (1 to 50).
    pub window_pct: f64,
    /// ...and within this many milliseconds, whichever is smaller.
    pub window_max_ms: f64,
    /// A beat is off when its offset differs from the grid's median offset by more than this.
    pub threshold_ms: f64,
    /// A drift span needs at least this many consecutive off beats on the same side.
    pub min_run: usize,
    /// A run may contain this many beats in a row with no onset near them and stay one run.
    #[serde(default = "default_max_gap")]
    pub max_gap: usize,
}

fn default_max_gap() -> usize {
    2
}

impl Default for DriftOptions {
    fn default() -> Self {
        DriftOptions { window_pct: 25.0, window_max_ms: 120.0, threshold_ms: 20.0, min_run: 4, max_gap: default_max_gap() }
    }
}

/// How the tempo map is cut into sections. Both numbers are user settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionOptions {
    /// A beat gap that differs from the section's mean gap by more than this percentage starts a
    /// new section.
    pub tolerance_pct: f64,
    /// A run shorter than this many beats is folded into its neighbour (unless the user split
    /// there), so one dragged beat does not become two sections.
    pub min_beats: usize,
}

impl Default for SectionOptions {
    fn default() -> Self {
        SectionOptions { tolerance_pct: 0.5, min_beats: 4 }
    }
}

/// A stretch of the grid at one tempo. It covers beats `first_beat..end_beat` (the beat at
/// `end_beat` is the first of the next section); the last section ends at the last beat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Section {
    pub first_beat: usize,
    /// Exclusive: index of the next section's first beat, or the number of beats for the last one.
    pub end_beat: usize,
    pub start_frame: i64,
    pub end_frame: i64,
    pub beats: usize,
    /// Mean tempo over the section's gaps (the stored label for a one-beat section).
    pub bpm: f64,
    pub min_bpm: f64,
    pub max_bpm: f64,
    /// The user cut the grid here (a split), as opposed to the tempo changing.
    pub forced: bool,
}

/// What to do to the tempo map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TempoEdit {
    /// Rebuild the beats of `from_beat..to_beat` at an exact tempo, starting on `from_beat`
    /// (which does not move). `to_beat` is the next section's first beat (it stays where it is);
    /// `None` runs to the end of the audio.
    SetSectionBpm { from_beat: usize, to_beat: Option<usize>, bpm: f64 },
    /// Move one beat line; its neighbours stay.
    MoveBeat { beat: usize, frame: i64 },
    /// Move this beat and every later one by the same amount (a phase fix from here on).
    ShiftFrom { beat: usize, frames: i64 },
    /// Space the beats between two beats evenly (the two stay).
    Even { from_beat: usize, to_beat: usize },
    /// Add a beat line at `frame`.
    InsertBeat { frame: i64 },
    /// Delete a beat line.
    RemoveBeat { beat: usize },
    /// Move the beats of `from_beat..to_beat` (None: to the end) onto the onsets they drift
    /// from, smoothed over `smooth_beats` beats. The whole grid's constant offset is kept.
    FollowOnsets { from_beat: usize, to_beat: Option<usize>, onsets: Vec<Onset>, options: DriftOptions, smooth_beats: usize },
}

/// Shortest and longest beat gap the device's tempo range allows, in frames.
fn gap_limits(sample_rate: u32) -> (i64, i64) {
    let sr = f64::from(sample_rate);
    ((sr * 60.0 / MAX_BPM).ceil() as i64, (sr * 60.0 / MIN_BPM).floor() as i64)
}

fn gap_bpm(gap: i64, sample_rate: u32) -> f64 {
    60.0 * f64::from(sample_rate) / gap as f64
}

/// Sets the `bpm` label of beats `lo..=hi` from their gaps (the last beat from the gap before it).
fn relabel(beats: &mut [GridBeat], lo: usize, hi: usize, sample_rate: u32) {
    let n = beats.len();
    if n < 2 {
        return;
    }
    for i in lo..=hi.min(n - 1) {
        let gap = if i + 1 < n { beats[i + 1].pos - beats[i].pos } else { beats[i].pos - beats[i - 1].pos };
        if gap > 0 {
            beats[i].bpm = gap_bpm(gap, sample_rate);
        }
    }
}

/// Errors when a gap in `lo..=hi` (gap `i` runs from beat `i` to `i + 1`) is outside 20..400 BPM.
fn check_gaps(beats: &[GridBeat], lo: usize, hi: usize, sample_rate: u32) -> Result<(), Error> {
    let (min, max) = gap_limits(sample_rate);
    for i in lo..=hi {
        if i + 1 >= beats.len() {
            break;
        }
        let gap = beats[i + 1].pos - beats[i].pos;
        if gap < min {
            return invalid(format!("beats {} and {} would be {gap} frames apart, closer than {MAX_BPM} BPM allows", i + 1, i + 2));
        }
        if gap > max {
            return invalid(format!("beats {} and {} would be further apart than {MIN_BPM} BPM allows", i + 1, i + 2));
        }
    }
    Ok(())
}

fn nearest_index(beats: &[GridBeat], frame: i64) -> usize {
    let i = beats.partition_point(|b| b.pos < frame);
    match (i.checked_sub(1), beats.get(i)) {
        (Some(p), Some(n)) => {
            if frame - beats[p].pos <= n.pos - frame {
                p
            } else {
                i
            }
        }
        (Some(p), None) => p,
        (None, _) => 0,
    }
}

/// Cuts the grid into tempo sections (see [`SectionOptions`]). `splits` are frames where the user
/// cut the grid; each snaps to the beat nearest it (only when that beat is within half a gap).
pub fn sections(doc: &GridDoc, sample_rate: u32, options: SectionOptions, splits: &[i64]) -> Vec<Section> {
    let beats = &doc.beats;
    let n = beats.len();
    if n == 0 {
        return Vec::new();
    }
    let tol = options.tolerance_pct.max(0.0) / 100.0;
    let mut starts: Vec<usize> = vec![0];
    let (mut sum, mut cnt) = (0.0f64, 0usize);
    for i in 0..n - 1 {
        let gap = (beats[i + 1].pos - beats[i].pos) as f64;
        if cnt > 0 {
            let mean = sum / cnt as f64;
            if (gap - mean).abs() / mean > tol {
                starts.push(i);
                sum = 0.0;
                cnt = 0;
            }
        }
        sum += gap;
        cnt += 1;
    }
    let mut forced: Vec<usize> = Vec::new();
    for &s in splits {
        let k = nearest_index(beats, s);
        let gap = if k + 1 < n { beats[k + 1].pos - beats[k].pos } else if k > 0 { beats[k].pos - beats[k - 1].pos } else { 0 };
        // beat 0 starts the first section anyway; a split well past the last beat is not on the grid
        if k > 0 && (beats[k].pos - s).abs() * 2 <= gap.max(1) {
            forced.push(k);
            if !starts.contains(&k) {
                starts.push(k);
            }
        }
    }
    starts.sort_unstable();
    starts.dedup();
    let mean_gap = |first: usize, end: usize| -> f64 {
        let last = end.min(n - 1);
        if last <= first {
            return f64::NAN;
        }
        (beats[last].pos - beats[first].pos) as f64 / (last - first) as f64
    };
    loop {
        let mut changed = false;
        // fold short unforced runs into a neighbour
        for k in 0..starts.len() {
            let end = starts.get(k + 1).copied().unwrap_or(n);
            if end - starts[k] >= options.min_beats || starts.len() < 2 {
                continue;
            }
            if k > 0 && !forced.contains(&starts[k]) {
                starts.remove(k);
                changed = true;
                break;
            }
            if k + 1 < starts.len() && !forced.contains(&starts[k + 1]) {
                starts.remove(k + 1);
                changed = true;
                break;
            }
        }
        // neighbours that ended up at the same tempo (a folded blip) are one section again
        if !changed {
            for k in 1..starts.len() {
                if forced.contains(&starts[k]) {
                    continue;
                }
                let end = starts.get(k + 1).copied().unwrap_or(n);
                let (a, b) = (mean_gap(starts[k - 1], starts[k]), mean_gap(starts[k], end));
                if a.is_finite() && b.is_finite() && (a - b).abs() / a <= tol {
                    starts.remove(k);
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    (0..starts.len())
        .map(|k| {
            let first = starts[k];
            let end = starts.get(k + 1).copied().unwrap_or(n);
            let gaps: Vec<i64> = (first..end.min(n - 1)).map(|i| beats[i + 1].pos - beats[i].pos).collect();
            let (bpm, lo, hi) = if gaps.is_empty() {
                (beats[first].bpm, beats[first].bpm, beats[first].bpm)
            } else {
                let total: i64 = gaps.iter().sum();
                let mean = gap_bpm(total, sample_rate) * gaps.len() as f64;
                let fastest = gaps.iter().min().copied().unwrap_or(1);
                let slowest = gaps.iter().max().copied().unwrap_or(1);
                (mean, gap_bpm(slowest, sample_rate), gap_bpm(fastest, sample_rate))
            };
            Section {
                first_beat: first,
                end_beat: end,
                start_frame: beats[first].pos,
                end_frame: beats[end.min(n - 1)].pos,
                beats: end - first,
                bpm,
                min_bpm: lo,
                max_bpm: hi,
                forced: forced.contains(&first),
            }
        })
        .collect()
}

/// How far each beat line is from the onset that belongs to it, and where that adds up to drift.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DriftReport {
    /// Per beat: onset minus beat line in milliseconds (positive: the line is early), `None`
    /// when no onset lies in the beat's window.
    pub offsets_ms: Vec<Option<f64>>,
    /// The median offset: the grid sitting early or late as a whole. Not counted as drift.
    pub bias_ms: f64,
    pub matched: usize,
    pub beats: usize,
    /// Largest `|offset - bias|` over the matched beats.
    pub worst_ms: f64,
    pub spans: Vec<DriftSpan>,
}

/// A run of beats that all sit off the music on the same side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DriftSpan {
    pub first_beat: usize,
    pub last_beat: usize,
    pub start_frame: i64,
    pub end_frame: i64,
    /// Beats in the run that are off (unmatched beats inside it are not counted).
    pub off_beats: usize,
    /// Mean of `offset - bias` over the off beats (positive: the lines are early).
    pub mean_ms: f64,
    pub max_abs_ms: f64,
}

fn check_drift_options(o: &DriftOptions) -> Result<(), Error> {
    if !(1.0..=50.0).contains(&o.window_pct) || !o.window_pct.is_finite() {
        return invalid("the match window must be between 1 and 50 % of a beat");
    }
    if !(o.window_max_ms.is_finite() && o.window_max_ms >= 2.0) {
        return invalid("the match window must be at least 2 ms");
    }
    if !(o.threshold_ms.is_finite() && o.threshold_ms > 0.0) {
        return invalid("the drift threshold must be above 0 ms");
    }
    if o.min_run == 0 {
        return invalid("a drift span needs at least 1 beat");
    }
    Ok(())
}

/// Matches onsets (ascending by frame) to the beat lines of `doc` and measures the offsets.
pub fn drift(doc: &GridDoc, onsets: &[Onset], sample_rate: u32, options: DriftOptions) -> Result<DriftReport, Error> {
    validate(doc)?;
    check_drift_options(&options)?;
    let beats = &doc.beats;
    let n = beats.len();
    let ms = 1000.0 / f64::from(sample_rate);
    let mut offsets: Vec<Option<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let gap = if i + 1 < n { beats[i + 1].pos - beats[i].pos } else if i > 0 { beats[i].pos - beats[i - 1].pos } else { period_frames(beats[i].bpm, sample_rate) as i64 };
        let window = ((gap as f64 * options.window_pct / 100.0).min(options.window_max_ms / ms)).max(1.0);
        let (lo, hi) = (beats[i].pos as f64 - window, beats[i].pos as f64 + window);
        let from = onsets.partition_point(|o| (o.frame as f64) < lo);
        let mut best: Option<(f64, i64)> = None;
        for o in onsets[from..].iter().take_while(|o| o.frame as f64 <= hi) {
            let d = (o.frame - beats[i].pos).abs() as f64;
            // strength, discounted towards the edge of the window so a loud hat far out does not
            // beat a kick on the line
            let score = f64::from(o.strength) * (1.0 - 0.5 * d / window);
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, o.frame));
            }
        }
        offsets.push(best.map(|(_, f)| (f - beats[i].pos) as f64 * ms));
    }
    let mut matched_vals: Vec<f64> = offsets.iter().flatten().copied().collect();
    matched_vals.sort_by(|a, b| a.total_cmp(b));
    let bias = if matched_vals.is_empty() { 0.0 } else { matched_vals[matched_vals.len() / 2] };
    let worst = matched_vals.iter().map(|v| (v - bias).abs()).fold(0.0, f64::max);

    // spans: runs of off beats on the same side, up to `max_gap` unmatched beats in a row allowed inside a run
    let mut spans = Vec::new();
    struct Run {
        first: usize,
        last: usize,
        sign: f64,
        count: usize,
        sum: f64,
        max: f64,
    }
    let mut run: Option<Run> = None;
    let close = |run: &mut Option<Run>, spans: &mut Vec<DriftSpan>| {
        if let Some(r) = run.take() {
            if r.count >= options.min_run {
                spans.push(DriftSpan {
                    first_beat: r.first,
                    last_beat: r.last,
                    start_frame: beats[r.first].pos,
                    end_frame: beats[r.last].pos,
                    off_beats: r.count,
                    mean_ms: r.sum / r.count as f64,
                    max_abs_ms: r.max,
                });
            }
        }
    };
    for (i, off) in offsets.iter().enumerate() {
        match off {
            Some(v) => {
                let d = v - bias;
                if d.abs() > options.threshold_ms {
                    let sign = d.signum();
                    if run.as_ref().is_some_and(|r| r.sign != sign) {
                        close(&mut run, &mut spans);
                    }
                    let r = run.get_or_insert(Run { first: i, last: i, sign, count: 0, sum: 0.0, max: 0.0 });
                    r.last = i;
                    r.count += 1;
                    r.sum += d;
                    r.max = r.max.max(d.abs());
                } else {
                    close(&mut run, &mut spans);
                }
            }
            None => {
                if run.as_ref().is_some_and(|r| i - r.last > options.max_gap) {
                    close(&mut run, &mut spans);
                }
            }
        }
    }
    close(&mut run, &mut spans);
    Ok(DriftReport { matched: matched_vals.len(), beats: n, offsets_ms: offsets, bias_ms: bias, worst_ms: worst, spans })
}

fn norm_fd(fd: Option<usize>) -> Option<usize> {
    fd.map(|f| f % BAR as usize)
}

/// Applies one tempo-map edit. Pure: nothing is read or written.
pub fn apply(doc: &GridDoc, edit: &TempoEdit, ctx: EditCtx) -> Result<GridDoc, Error> {
    validate(doc)?;
    if ctx.sample_rate == 0 {
        return invalid("sample rate is 0");
    }
    let sr = ctx.sample_rate;
    let n = doc.beats.len();
    let (min_gap, _) = gap_limits(sr);
    match edit {
        TempoEdit::SetSectionBpm { from_beat, to_beat, bpm } => {
            let (from, bpm) = (*from_beat, *bpm);
            if !bpm.is_finite() || !(MIN_BPM..=MAX_BPM).contains(&bpm) {
                return invalid(format!("tempo must be between {MIN_BPM} and {MAX_BPM} BPM"));
            }
            if from >= n {
                return invalid(format!("there is no beat {}", from + 1));
            }
            if let Some(t) = *to_beat {
                if t <= from || t >= n {
                    return invalid("the section must end at a later beat");
                }
            }
            let start = doc.beats[from].pos;
            let p = period_frames(bpm, sr);
            let count = match *to_beat {
                // whole beats that fit before the next section's first beat, at least this one
                Some(t) => (((doc.beats[t].pos - start) as f64 / p).round() as usize).max(1),
                None => {
                    let end = ctx.total_frames.unwrap_or(doc.beats[n - 1].pos).max(start);
                    ((end - start) as f64 / p).floor() as usize + 1
                }
            };
            let mut mid: Vec<GridBeat> = (0..count).map(|j| GridBeat { pos: start + (j as f64 * p).round() as i64, bpm }).collect();
            let tail_from = to_beat.unwrap_or(n);
            if let Some(t) = *to_beat {
                // The last generated beat sits between 0.5 and 1.5 periods before the next section's
                // first beat, which stays put. Above ~200 BPM that can be closer than the device's
                // fastest gap, so the beat is dropped (a longer, playable gap) rather than written.
                while mid.len() > 1 && doc.beats[t].pos - mid[mid.len() - 1].pos < min_gap {
                    mid.pop();
                }
            }
            let new_count = mid.len();
            let mut beats: Vec<GridBeat> = doc.beats[..from].to_vec();
            beats.extend(mid);
            beats.extend_from_slice(&doc.beats[tail_from..]);
            if to_beat.is_some() {
                // the gap across the join is whatever is left, so check it and label its beat by it
                let last_mid = from + new_count - 1;
                check_gaps(&beats, last_mid, last_mid, sr)?;
                relabel(&mut beats, last_mid, last_mid, sr);
            }
            let old_count = tail_from - from;
            // a bar line before or inside the section keeps its index (inside, it stays on the same
            // count of beats from the section start); one after it follows its beat
            let fd = doc.first_downbeat.map(|f| if f >= tail_from { (f + new_count - old_count) % BAR as usize } else { f });
            finish(doc, beats, fd)
        }
        TempoEdit::MoveBeat { beat, frame } => {
            let (b, frame) = (*beat, *frame);
            if b >= n {
                return invalid(format!("there is no beat {}", b + 1));
            }
            if frame < 0 {
                return invalid("a beat cannot be before the start of the track");
            }
            if b + 1 == n && ctx.total_frames.is_some_and(|t| frame > t) {
                return invalid("a beat cannot be after the end of the audio");
            }
            let mut beats = doc.beats.clone();
            beats[b].pos = frame;
            check_gaps(&beats, b.saturating_sub(1), b, sr)?;
            relabel(&mut beats, b.saturating_sub(1), b + 1, sr);
            finish(doc, beats, doc.first_downbeat)
        }
        TempoEdit::ShiftFrom { beat, frames } => {
            let (b, frames) = (*beat, *frames);
            if b >= n {
                return invalid(format!("there is no beat {}", b + 1));
            }
            if frames == 0 {
                return invalid("nothing to move");
            }
            let mut beats = doc.beats.clone();
            for x in &mut beats[b..] {
                x.pos += frames;
            }
            if b == 0 {
                let (beats, fd) = clip(beats, doc.first_downbeat, ctx.total_frames);
                return finish(doc, beats, fd);
            }
            if let Some(t) = ctx.total_frames {
                beats.retain(|x| x.pos <= t);
            }
            if beats.len() <= b {
                return invalid("the move would push every later beat past the end of the audio");
            }
            check_gaps(&beats, b - 1, b - 1, sr)?;
            relabel(&mut beats, b - 1, b, sr);
            let len = beats.len();
            let fd = doc.first_downbeat.map(|f| if f >= len { f % BAR as usize } else { f });
            finish(doc, beats, fd)
        }
        TempoEdit::Even { from_beat, to_beat } => {
            let (from, to) = (*from_beat, *to_beat);
            if to >= n || from >= to || to - from < 2 {
                return invalid("choose two beats with at least one beat between them");
            }
            let span = doc.beats[to].pos - doc.beats[from].pos;
            let steps = (to - from) as i64;
            let bpm = 60.0 * f64::from(sr) * steps as f64 / span as f64;
            let mut beats = doc.beats.clone();
            for (k, x) in beats[from..to].iter_mut().enumerate() {
                x.pos = doc.beats[from].pos + (k as i64 * span + steps / 2) / steps;
                x.bpm = bpm;
            }
            check_gaps(&beats, from, to - 1, sr)?;
            finish(doc, beats, doc.first_downbeat)
        }
        TempoEdit::InsertBeat { frame } => {
            let frame = *frame;
            if frame < 0 {
                return invalid("a beat cannot be before the start of the track");
            }
            if ctx.total_frames.is_some_and(|t| frame > t) {
                return invalid("a beat cannot be after the end of the audio");
            }
            let i = doc.beats.partition_point(|b| b.pos < frame);
            if doc.beats.get(i).is_some_and(|b| b.pos == frame) {
                return invalid("there is already a beat there");
            }
            let mut beats = doc.beats.clone();
            beats.insert(i, GridBeat { pos: frame, bpm: doc.beats[i.min(n - 1)].bpm });
            // the new gaps on both sides must be playable, and the one at an end not too long
            check_gaps(&beats, i.saturating_sub(1), i, sr)?;
            relabel(&mut beats, i.saturating_sub(1), i + 1, sr);
            let fd = doc.first_downbeat.map(|f| if i <= f { f + 1 } else { f });
            finish(doc, beats, norm_fd(fd))
        }
        TempoEdit::RemoveBeat { beat } => {
            let b = *beat;
            if b >= n {
                return invalid(format!("there is no beat {}", b + 1));
            }
            if n < 2 {
                return invalid("the last beat cannot be removed");
            }
            let mut beats = doc.beats.clone();
            beats.remove(b);
            if b > 0 && b < n - 1 {
                check_gaps(&beats, b - 1, b - 1, sr)?;
            }
            let last = beats.len() - 1;
            relabel(&mut beats, b.saturating_sub(1), b.max(1).min(last), sr);
            let fd = doc.first_downbeat.map(|f| if b < f { f - 1 } else { f });
            finish(doc, beats, norm_fd(fd))
        }
        TempoEdit::FollowOnsets { from_beat, to_beat, onsets, options, smooth_beats } => {
            let from = *from_beat;
            let to = to_beat.unwrap_or(n);
            if from >= n || to > n || to <= from {
                return invalid("choose a range of beats");
            }
            let report = drift(doc, onsets, sr, *options)?;
            let win = (*smooth_beats).max(1) | 1; // odd
            let raw: Vec<Option<f64>> = report.offsets_ms.iter().map(|o| o.map(|v| v - report.bias_ms)).collect();
            // median of the matched corrections around each beat of the range
            let mut corr: Vec<Option<f64>> = (from..to)
                .map(|i| {
                    let lo = i.saturating_sub(win / 2);
                    let hi = (i + win / 2 + 1).min(n);
                    let mut v: Vec<f64> = raw[lo..hi].iter().flatten().copied().collect();
                    if v.is_empty() {
                        return None;
                    }
                    v.sort_by(|a, b| a.total_cmp(b));
                    Some(v[v.len() / 2])
                })
                .collect();
            if corr.iter().all(Option::is_none) {
                return invalid("no onsets lie near the beat lines of this range, so there is nothing to follow");
            }
            // beats with no onset nearby take the corrections of the nearest ones on either side
            let known: Vec<(usize, f64)> = corr.iter().enumerate().filter_map(|(k, c)| c.map(|c| (k, c))).collect();
            for (k, slot) in corr.iter_mut().enumerate() {
                if slot.is_some() {
                    continue;
                }
                let before = known.iter().rev().find(|(j, _)| *j < k);
                let after = known.iter().find(|(j, _)| *j > k);
                *slot = Some(match (before, after) {
                    (Some(&(a, va)), Some(&(b, vb))) => va + (vb - va) * (k - a) as f64 / (b - a) as f64,
                    (Some(&(_, v)), None) | (None, Some(&(_, v))) => v,
                    (None, None) => 0.0,
                });
            }
            let mut beats = doc.beats.clone();
            for (k, i) in (from..to).enumerate() {
                let frames = (corr[k].unwrap_or(0.0) / 1000.0 * f64::from(sr)).round() as i64;
                beats[i].pos = (beats[i].pos + frames).max(0);
            }
            // never let two lines swap or touch
            for i in 1..n {
                if beats[i].pos < beats[i - 1].pos + min_gap {
                    beats[i].pos = beats[i - 1].pos + min_gap;
                }
            }
            if let Some(t) = ctx.total_frames {
                beats.retain(|b| b.pos <= t);
            }
            if beats.len() < 2 {
                return invalid("following the onsets would leave the grid empty");
            }
            relabel(&mut beats, from.saturating_sub(1), to, sr);
            let len = beats.len();
            let fd = doc.first_downbeat.map(|f| if f >= len { f % BAR as usize } else { f });
            finish(doc, beats, fd)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strfile::{self, Analysis, Newline};

    const SR: u32 = 44_100;

    fn ctx(total_secs: i64) -> EditCtx {
        EditCtx { sample_rate: SR, total_frames: Some(SR as i64 * total_secs) }
    }

    /// Beats from `start`, one tempo per `(bpm, beats)` run; each run starts where the last ended.
    fn map(start: i64, runs: &[(f64, usize)], fd: Option<usize>) -> GridDoc {
        let mut beats: Vec<GridBeat> = Vec::new();
        let mut pos = start as f64;
        for &(bpm, count) in runs {
            let p = period_frames(bpm, SR);
            for _ in 0..count {
                beats.push(GridBeat { pos: pos.round() as i64, bpm });
                pos += p;
            }
        }
        GridDoc { normalize_factor: 1.0, beat_start_offset: start, beats, first_downbeat: fd }
    }

    fn onsets_of(doc: &GridDoc, shift_of: impl Fn(usize) -> i64) -> Vec<Onset> {
        doc.beats.iter().enumerate().map(|(i, b)| Onset { frame: b.pos + shift_of(i), strength: 5.0 }).collect()
    }

    #[test]
    fn the_str_format_stores_a_tempo_map_beat_by_beat() {
        // evidence for the report: a grid with three tempi and an irregular beat survives the
        // real encoder/decoder, per-beat positions and labels intact, and is valid for the device
        let mut g = map(1710, &[(120.0, 16), (126.5, 16), (98.25, 12)], Some(2));
        g.beats[7].pos += 333;
        validate(&g).unwrap();
        let a = Analysis::from(&g);
        for nl in [Newline::Crlf, Newline::Lf] {
            let back = strfile::decode(&strfile::encode(&a, nl)).unwrap();
            assert_eq!(back, a);
            assert_eq!(back.beats[7].pos, g.beats[7].pos);
            assert_eq!(back.beats[20].bpm, 126.5);
        }
        assert_eq!(sections(&g, SR, SectionOptions::default(), &[]).len(), 3);
    }

    #[test]
    fn a_constant_grid_is_one_section_and_a_tempo_change_makes_two() {
        let s = sections(&map(0, &[(128.0, 64)], Some(0)), SR, SectionOptions::default(), &[]);
        assert_eq!(s.len(), 1);
        assert!((s[0].bpm - 128.0).abs() < 0.01 && s[0].beats == 64 && !s[0].forced);

        let g = map(1000, &[(120.0, 32), (130.0, 32)], Some(0));
        let s = sections(&g, SR, SectionOptions::default(), &[]);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].first_beat, s[0].end_beat, s[1].first_beat, s[1].end_beat), (0, 32, 32, 64));
        assert!((s[0].bpm - 120.0).abs() < 0.01 && (s[1].bpm - 130.0).abs() < 0.01);
        assert_eq!(s[1].start_frame, g.beats[32].pos);
    }

    #[test]
    fn one_dragged_beat_does_not_become_sections_unless_asked() {
        let mut g = map(0, &[(120.0, 40)], Some(0));
        g.beats[20].pos += 1500; // 34 ms late
        assert_eq!(sections(&g, SR, SectionOptions::default(), &[]).len(), 1, "short blips fold into the section");
        let strict = SectionOptions { tolerance_pct: 0.5, min_beats: 1 };
        assert_eq!(sections(&g, SR, strict, &[]).len(), 4, "with min_beats 1 the blip is its own run");
    }

    #[test]
    fn a_split_forces_a_section_boundary_at_the_nearest_beat() {
        let g = map(0, &[(120.0, 40)], Some(0));
        let s = sections(&g, SR, SectionOptions::default(), &[g.beats[16].pos + 100]);
        assert_eq!(s.len(), 2);
        assert!(s[1].forced && s[1].first_beat == 16 && !s[0].forced);
        // a split at beat 0 or well past the last beat is ignored
        let s = sections(&g, SR, SectionOptions::default(), &[0, g.beats[39].pos + 1_000_000]);
        assert_eq!(s.len(), 1);
        // a forced section may be shorter than min_beats
        let s = sections(&g, SR, SectionOptions::default(), &[g.beats[16].pos, g.beats[18].pos]);
        assert_eq!(s.iter().map(|x| x.beats).collect::<Vec<_>>(), vec![16, 2, 22]);
    }

    #[test]
    fn set_section_bpm_retimes_only_that_section() {
        let g = map(2000, &[(120.0, 32), (120.0, 32)], Some(1));
        let e = TempoEdit::SetSectionBpm { from_beat: 32, to_beat: None, bpm: 140.0 };
        let n = apply(&g, &e, ctx(60)).unwrap();
        assert_eq!(&n.beats[..32], &g.beats[..32], "the section before is untouched");
        assert_eq!(n.beats[32].pos, g.beats[32].pos, "the start of the section does not move");
        let p = period_frames(140.0, SR);
        assert_eq!(n.beats[33].pos, g.beats[32].pos + p.round() as i64);
        assert!(n.beats[32..].iter().all(|b| b.bpm == 140.0));
        assert!(n.beats.last().unwrap().pos <= SR as i64 * 60);
        assert_eq!(n.first_downbeat, Some(1));
        let s = sections(&n, SR, SectionOptions::default(), &[]);
        assert_eq!(s.len(), 2);
        assert!((s[1].bpm - 140.0).abs() < 0.01);
    }

    #[test]
    fn set_section_bpm_between_two_beats_keeps_the_later_beats_in_place() {
        let g = map(0, &[(120.0, 48)], Some(0));
        // beats 16..32 (16 beats) re-timed to 160 BPM: 24 beats fit in the same 8 s
        let e = TempoEdit::SetSectionBpm { from_beat: 16, to_beat: Some(32), bpm: 160.0 };
        let n = apply(&g, &e, ctx(60)).unwrap();
        assert_eq!(n.beats.len(), 48 - 16 + 21, "round(8 s / 0.375 s) beats now fill the span");
        assert_eq!(&n.beats[..16], &g.beats[..16]);
        let k = n.beats.iter().position(|b| b.pos == g.beats[32].pos).expect("beat 33 keeps its frame");
        assert_eq!(&n.beats[k..], &g.beats[32..]);
        // bar line keeps its beat when it is before the edit; one after the edit follows its beat
        let late = GridDoc { first_downbeat: Some(3), ..g.clone() };
        let m = apply(&late, &TempoEdit::SetSectionBpm { from_beat: 0, to_beat: Some(2), bpm: 60.0 }, ctx(60)).unwrap();
        assert_eq!(m.first_downbeat, Some(3 - 2 + 1), "two beats became one");
    }

    #[test]
    fn set_section_bpm_between_beats_never_writes_a_gap_the_device_cannot_play() {
        // at high tempi the last generated beat can land closer to the next section than the
        // device's fastest gap; it is dropped, and the beat before the join is labelled by its real gap
        let g = map(0, &[(120.0, 16)], Some(0));
        let (min, max) = gap_limits(SR);
        for bpm in [201.0, 233.3, 250.0, 299.9, 333.0, 400.0] {
            for to in 5..12 {
                let e = TempoEdit::SetSectionBpm { from_beat: 3, to_beat: Some(to), bpm };
                let n = apply(&g, &e, ctx(60)).unwrap();
                for w in n.beats.windows(2) {
                    assert!((min..=max).contains(&(w[1].pos - w[0].pos)), "{bpm} BPM to beat {to}");
                }
                let k = n.beats.iter().position(|x| x.pos == g.beats[to].pos).expect("the next section keeps its frame");
                let gap = n.beats[k].pos - n.beats[k - 1].pos;
                assert!((n.beats[k - 1].bpm - gap_bpm(gap, SR)).abs() < 1e-6, "{bpm} BPM to beat {to}");
            }
        }
        // 1.0 s at 350 BPM (p = 0.1714 s) rounds to 6 beats, the 6th only 0.14 s (0.8 periods) before the join
        let n = apply(&g, &TempoEdit::SetSectionBpm { from_beat: 4, to_beat: Some(6), bpm: 350.0 }, ctx(60)).unwrap();
        assert!(n.beats.windows(2).all(|w| w[1].pos - w[0].pos >= min));
    }

    #[test]
    fn set_section_bpm_rejects_what_the_device_cannot_play() {
        let g = map(0, &[(120.0, 16)], Some(0));
        for bpm in [5.0, 500.0, f64::NAN] {
            assert!(apply(&g, &TempoEdit::SetSectionBpm { from_beat: 0, to_beat: None, bpm }, ctx(60)).is_err());
        }
        assert!(apply(&g, &TempoEdit::SetSectionBpm { from_beat: 99, to_beat: None, bpm: 120.0 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::SetSectionBpm { from_beat: 4, to_beat: Some(4), bpm: 120.0 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::SetSectionBpm { from_beat: 4, to_beat: Some(16), bpm: 120.0 }, ctx(60)).is_err());
    }

    #[test]
    fn setting_the_same_tempo_again_changes_nothing_audible() {
        let g = map(500, &[(124.0, 60)], Some(0));
        let n = apply(&g, &TempoEdit::SetSectionBpm { from_beat: 10, to_beat: None, bpm: 124.0 }, ctx(60)).unwrap();
        for (a, b) in g.beats.iter().zip(&n.beats) {
            assert!((a.pos - b.pos).abs() <= 1);
        }
    }

    #[test]
    fn move_beat_moves_one_line_and_relabels_its_neighbours() {
        let g = map(0, &[(120.0, 16)], Some(0));
        let n = apply(&g, &TempoEdit::MoveBeat { beat: 5, frame: g.beats[5].pos + 800 }, ctx(60)).unwrap();
        for (i, (a, b)) in g.beats.iter().zip(&n.beats).enumerate() {
            if i == 5 {
                assert_eq!(b.pos, a.pos + 800);
            } else {
                assert_eq!(b.pos, a.pos);
            }
        }
        assert!(n.beats[4].bpm < 120.0 && n.beats[5].bpm > 120.0, "the long gap before is slower, the short one after faster");
        assert_eq!(n.beats[2].bpm, 120.0, "far beats keep their label");
        // beat 0 moves the offset with it
        let z = apply(&g, &TempoEdit::MoveBeat { beat: 0, frame: 300 }, ctx(60)).unwrap();
        assert_eq!(z.beat_start_offset, 300);
    }

    #[test]
    fn move_beat_cannot_cross_a_neighbour_or_leave_the_audio() {
        let g = map(0, &[(120.0, 16)], Some(0));
        assert!(apply(&g, &TempoEdit::MoveBeat { beat: 5, frame: g.beats[4].pos }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::MoveBeat { beat: 5, frame: g.beats[6].pos + 5 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::MoveBeat { beat: 5, frame: g.beats[4].pos + 1000 }, ctx(60)).is_err(), "closer than 400 BPM allows");
        assert!(apply(&g, &TempoEdit::MoveBeat { beat: 5, frame: -1 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::MoveBeat { beat: 15, frame: SR as i64 * 61 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::MoveBeat { beat: 16, frame: 5 }, ctx(60)).is_err());
    }

    #[test]
    fn shift_from_moves_the_tail_and_a_shift_from_zero_is_a_nudge() {
        let g = map(1000, &[(120.0, 24)], Some(0));
        let n = apply(&g, &TempoEdit::ShiftFrom { beat: 10, frames: 441 }, ctx(60)).unwrap();
        assert!(n.beats[..10].iter().zip(&g.beats[..10]).all(|(a, b)| a.pos == b.pos));
        assert!(n.beats[10..].iter().zip(&g.beats[10..]).all(|(a, b)| a.pos == b.pos + 441));
        assert!(n.beats[9].bpm < 120.0, "the gap before the shift is longer now");
        let all = apply(&g, &TempoEdit::ShiftFrom { beat: 0, frames: 44 }, ctx(60)).unwrap();
        assert_eq!(all.beats[0].pos, 1044);
        assert_eq!(all.beat_start_offset, 1044);
        assert!(apply(&g, &TempoEdit::ShiftFrom { beat: 10, frames: -(g.beats[10].pos - g.beats[9].pos) }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::ShiftFrom { beat: 10, frames: 0 }, ctx(60)).is_err());
        // the tail running past the end is trimmed, never kept beyond the audio
        let end = apply(&g, &TempoEdit::ShiftFrom { beat: 20, frames: SR as i64 * 2 }, ctx(13)).unwrap();
        assert_eq!(end.beats.len(), 22, "the two beats that would run past 13 s are dropped");
        assert!(end.beats.iter().all(|b| b.pos <= SR as i64 * 13));
    }

    #[test]
    fn even_spaces_the_beats_between_two_anchors() {
        let mut g = map(0, &[(120.0, 20)], Some(0));
        for i in 5..10 {
            g.beats[i].pos += (i as i64 - 7) * 400; // wobble
        }
        let n = apply(&g, &TempoEdit::Even { from_beat: 4, to_beat: 11 }, ctx(60)).unwrap();
        assert_eq!(n.beats[4], GridBeat { pos: g.beats[4].pos, bpm: n.beats[4].bpm });
        assert_eq!(n.beats[11].pos, g.beats[11].pos);
        let gaps: Vec<i64> = (4..11).map(|i| n.beats[i + 1].pos - n.beats[i].pos).collect();
        assert!(gaps.iter().max().unwrap() - gaps.iter().min().unwrap() <= 1, "{gaps:?}");
        assert!(apply(&g, &TempoEdit::Even { from_beat: 4, to_beat: 5 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::Even { from_beat: 6, to_beat: 4 }, ctx(60)).is_err());
    }

    #[test]
    fn insert_and_remove_beat_keep_the_bar_line_on_its_beat() {
        let g = map(25_000, &[(120.0, 16)], Some(2));
        let mid = (g.beats[3].pos + g.beats[4].pos) / 2;
        let n = apply(&g, &TempoEdit::InsertBeat { frame: mid }, ctx(60)).unwrap();
        assert_eq!(n.beats.len(), 17);
        assert_eq!(n.beats[4].pos, mid);
        assert_eq!(n.first_downbeat, Some(2), "inserted after the bar line: unchanged");
        assert!(apply(&g, &TempoEdit::InsertBeat { frame: g.beats[0].pos }, ctx(60)).is_err(), "a beat is already there");
        let pre = apply(&g, &TempoEdit::InsertBeat { frame: g.beats[0].pos - 10_000 }, ctx(60)).unwrap();
        assert_eq!(pre.beats.len(), 17);
        assert_eq!(pre.first_downbeat, Some(3), "the bar line is still on the same beat");
        assert_eq!(pre.beat_start_offset, g.beats[0].pos - 10_000);
        // remove the inserted beat again: the original grid, label for label near the join
        let back = apply(&n, &TempoEdit::RemoveBeat { beat: 4 }, ctx(60)).unwrap();
        assert_eq!(back.beats.iter().map(|b| b.pos).collect::<Vec<_>>(), g.beats.iter().map(|b| b.pos).collect::<Vec<_>>());
        assert_eq!(back.first_downbeat, Some(2));
        // removing a beat before the bar line moves the index down with it
        let r = apply(&g, &TempoEdit::RemoveBeat { beat: 1 }, ctx(60)).unwrap();
        assert_eq!(r.first_downbeat, Some(1));
        assert!(apply(&g, &TempoEdit::RemoveBeat { beat: 16 }, ctx(60)).is_err());
        assert!(apply(&map(0, &[(120.0, 1)], None), &TempoEdit::RemoveBeat { beat: 0 }, ctx(60)).is_err());
    }

    #[test]
    fn insert_beat_refuses_gaps_the_device_cannot_play() {
        let g = map(0, &[(120.0, 8)], Some(0));
        assert!(apply(&g, &TempoEdit::InsertBeat { frame: g.beats[2].pos + 1000 }, ctx(60)).is_err(), "closer than 400 BPM");
        assert!(apply(&g, &TempoEdit::InsertBeat { frame: -5 }, ctx(60)).is_err());
        assert!(apply(&g, &TempoEdit::InsertBeat { frame: SR as i64 * 61 }, ctx(60)).is_err());
        // far past the last beat would leave a gap slower than 20 BPM
        assert!(apply(&g, &TempoEdit::InsertBeat { frame: g.beats[7].pos + SR as i64 * 4 }, ctx(60)).is_err());
    }

    #[test]
    fn drift_is_clean_on_a_grid_that_sits_on_the_music() {
        let g = map(3000, &[(124.0, 64)], Some(0));
        let r = drift(&g, &onsets_of(&g, |_| 0), SR, DriftOptions::default()).unwrap();
        assert_eq!((r.matched, r.beats), (64, 64));
        assert!(r.bias_ms.abs() < 0.1 && r.worst_ms < 0.1 && r.spans.is_empty());
    }

    #[test]
    fn a_constant_offset_is_bias_not_drift() {
        let g = map(3000, &[(124.0, 64)], Some(0));
        let late = (0.030 * SR as f64) as i64;
        let r = drift(&g, &onsets_of(&g, |_| late), SR, DriftOptions::default()).unwrap();
        assert!((r.bias_ms - 30.0).abs() < 0.1, "{}", r.bias_ms);
        assert!(r.spans.is_empty() && r.worst_ms < 0.1);
    }

    /// Offsets of the music against a 120 BPM grid that stays put: beats before `from` are on
    /// the grid, every later beat arrives `ms_per_beat` later than the one before it.
    fn slowing(from: usize, ms_per_beat: f64) -> impl Fn(usize) -> i64 {
        move |i| if i < from { 0 } else { ((i - from + 1) as f64 * ms_per_beat / 1000.0 * f64::from(SR)).round() as i64 }
    }

    #[test]
    fn a_tempo_that_wanders_off_the_grid_is_found_as_a_span() {
        // the music slows after beat 64 (1.5 ms later each beat); the grid stays at 120 BPM
        let g = map(2000, &[(120.0, 96)], Some(0));
        let real = slowing(64, 1.5);
        let r = drift(&g, &onsets_of(&g, &real), SR, DriftOptions::default()).unwrap();
        assert_eq!(r.spans.len(), 1, "{:?}", r.spans);
        let s = r.spans[0];
        // 20 ms is passed at the 14th late beat, so the span starts near beat 78
        assert!((76..=80).contains(&s.first_beat), "starts at {}", s.first_beat);
        assert_eq!(s.last_beat, 95);
        assert!(s.mean_ms > 25.0 && s.max_abs_ms > 45.0 && s.max_abs_ms < 50.0, "{s:?}");
        assert!(r.bias_ms.abs() < 0.1, "most of the track is on the grid, so the median offset is 0");
        // a looser threshold sees less of it
        let loose = DriftOptions { threshold_ms: 30.0, ..DriftOptions::default() };
        assert!(drift(&g, &onsets_of(&g, &real), SR, loose).unwrap().spans[0].first_beat > s.first_beat);
        // a threshold above the worst drift finds none
        let blind = DriftOptions { threshold_ms: 60.0, ..DriftOptions::default() };
        assert!(drift(&g, &onsets_of(&g, &real), SR, blind).unwrap().spans.is_empty());
    }

    #[test]
    fn how_many_unmatched_beats_a_drift_span_may_bridge_is_a_setting() {
        let g = map(2000, &[(120.0, 32)], Some(0));
        let late = (0.040 * SR as f64) as i64;
        // beats 10..14 and 17..22 are 40 ms late; the three beats between have no onset at all
        let on: Vec<Onset> = g
            .beats
            .iter()
            .enumerate()
            .filter(|(i, _)| !(14..17).contains(i))
            .map(|(i, b)| Onset { frame: b.pos + if (10..22).contains(&i) { late } else { 0 }, strength: 5.0 })
            .collect();
        let tight = drift(&g, &on, SR, DriftOptions { max_gap: 2, ..DriftOptions::default() }).unwrap();
        assert_eq!(tight.spans.len(), 2, "{:?}", tight.spans);
        let loose = drift(&g, &on, SR, DriftOptions { max_gap: 3, ..DriftOptions::default() }).unwrap();
        assert_eq!(loose.spans.len(), 1, "{:?}", loose.spans);
        assert_eq!((loose.spans[0].first_beat, loose.spans[0].last_beat, loose.spans[0].off_beats), (10, 21, 9));
    }

    #[test]
    fn unmatched_beats_and_bad_options_are_handled() {
        let g = map(0, &[(120.0, 16)], Some(0));
        let none = drift(&g, &[], SR, DriftOptions::default()).unwrap();
        assert_eq!(none.matched, 0);
        assert!(none.offsets_ms.iter().all(Option::is_none) && none.spans.is_empty() && none.bias_ms == 0.0);
        // an onset half a beat away belongs to no beat
        let off = [Onset { frame: g.beats[3].pos + 11_000, strength: 9.0 }];
        assert_eq!(drift(&g, &off, SR, DriftOptions::default()).unwrap().matched, 0);
        for bad in [
            DriftOptions { window_pct: 0.0, ..DriftOptions::default() },
            DriftOptions { window_pct: 80.0, ..DriftOptions::default() },
            DriftOptions { window_max_ms: 0.5, ..DriftOptions::default() },
            DriftOptions { threshold_ms: 0.0, ..DriftOptions::default() },
            DriftOptions { min_run: 0, ..DriftOptions::default() },
        ] {
            assert!(drift(&g, &[], SR, bad).is_err());
        }
    }

    #[test]
    fn the_stronger_onset_wins_but_a_far_one_does_not_beat_a_near_one_of_similar_strength() {
        let g = map(10_000, &[(120.0, 8)], Some(0));
        let b = g.beats[2].pos;
        let on = [Onset { frame: b - 4000, strength: 5.0 }, Onset { frame: b + 100, strength: 4.5 }];
        let r = drift(&g, &on, SR, DriftOptions::default()).unwrap();
        assert!((r.offsets_ms[2].unwrap() - 100.0 * 1000.0 / 44_100.0).abs() < 0.01);
    }

    #[test]
    fn following_the_onsets_repairs_a_drifting_section_and_keeps_the_rest() {
        let g = map(2000, &[(120.0, 96)], Some(1));
        let on = onsets_of(&g, slowing(64, 1.5));
        let opts = DriftOptions::default();
        assert_eq!(drift(&g, &on, SR, opts).unwrap().spans.len(), 1);
        let edit = TempoEdit::FollowOnsets { from_beat: 0, to_beat: None, onsets: on.clone(), options: opts, smooth_beats: 5 };
        let n = apply(&g, &edit, ctx(60)).unwrap();
        let after = drift(&n, &on, SR, opts).unwrap();
        assert!(after.spans.is_empty(), "{:?}", after.spans);
        assert!(after.worst_ms < 5.0, "worst {} ms after following", after.worst_ms);
        assert_eq!(&n.beats[..62], &g.beats[..62], "beats on the music do not move");
        assert_eq!(n.first_downbeat, Some(1));
        validate(&n).unwrap();
        // the repaired tail is slower than the grid it replaced: 1.5 ms later each beat
        let tail = |d: &GridDoc| d.beats[95].pos - d.beats[80].pos;
        let extra = tail(&n) - tail(&g);
        assert!((extra - (15.0 * 1.5 / 1000.0 * 44_100.0) as i64).abs() < 100, "{extra}");
    }

    #[test]
    fn following_only_touches_the_range_and_needs_onsets() {
        let g = map(2000, &[(120.0, 40)], Some(0));
        // beats 5..15 sit 20 ms late in the audio; only beats 10..20 are asked to follow
        let on = onsets_of(&g, |i| if (5..15).contains(&i) { 882 } else { 0 });
        let opts = DriftOptions::default();
        let edit = TempoEdit::FollowOnsets { from_beat: 10, to_beat: Some(20), onsets: on, options: opts, smooth_beats: 1 };
        let n = apply(&g, &edit, ctx(60)).unwrap();
        for i in 0..40 {
            let want = if (10..15).contains(&i) { g.beats[i].pos + 882 } else { g.beats[i].pos };
            assert_eq!(n.beats[i].pos, want, "beat {i}");
        }
        let none = TempoEdit::FollowOnsets { from_beat: 0, to_beat: None, onsets: vec![], options: opts, smooth_beats: 5 };
        assert!(apply(&g, &none, ctx(60)).unwrap_err().to_string().contains("nothing to follow"));
        let bad_range = TempoEdit::FollowOnsets { from_beat: 5, to_beat: Some(5), onsets: vec![], options: opts, smooth_beats: 5 };
        assert!(apply(&g, &bad_range, ctx(60)).is_err());
    }

    #[test]
    fn every_edit_result_is_a_valid_grid() {
        let g = map(1500, &[(120.0, 20), (140.0, 20)], Some(3));
        let edits = [
            TempoEdit::SetSectionBpm { from_beat: 20, to_beat: None, bpm: 100.0 },
            TempoEdit::MoveBeat { beat: 7, frame: g.beats[7].pos + 300 },
            TempoEdit::ShiftFrom { beat: 25, frames: -200 },
            TempoEdit::Even { from_beat: 2, to_beat: 9 },
            TempoEdit::InsertBeat { frame: g.beats[10].pos + 11_000 },
            TempoEdit::RemoveBeat { beat: 12 },
        ];
        for e in &edits {
            validate(&apply(&g, e, ctx(60)).unwrap()).unwrap_or_else(|er| panic!("{e:?}: {er}"));
        }
    }
}
