//! Beat-grid timing: beat positions on the mix timeline, beat matching and automatic
//! transitions. Pure functions over [`MixDoc`] (the clips carry their grids in `track.grid`), so
//! they are tested on synthetic grids and the commands only add the library lookups.
//!
//! Model (see `src/lib/api/mixes.ts`): a clip's local axis is the stretched audio counted from
//! the start of the file; the tempo curve maps source time to local time
//! ([`pm_formats::mix::TempoMap`]); a beat at file frame `f` lies at timeline
//! `clip.start + local_at(f * 44100 / sourceRate)`.

use std::collections::BTreeMap;

use pm_formats::mix::{self as m, TempoMap};
use serde::{Deserialize, Serialize};

use super::doc::{
    Between, Clip, CurveNode, EffectDoc, EnvNode, MixDoc, ProcessDoc, Span, TempoNode, ToolDoc, TransitionDoc, SAMPLE_RATE,
    fresh_id, secs,
};

const BEATS_PER_BAR: i64 = 4;
/// Bass-kill value of the EQ "Lo" band (hypothesis: -100 is "kill", 0 is flat; see mix-format.md).
const EQ_LO_KILL: f32 = -100.0;

/// The tempo map of a list of tempo nodes.
fn map_of(nodes: &[TempoNode]) -> TempoMap {
    TempoMap::new(&nodes.iter().map(|n| m::SpeedNode { position: n.pos, speed: n.speed }).collect::<Vec<_>>())
}

fn grid_of(c: &Clip) -> Result<&super::doc::BeatGrid, String> {
    c.track
        .as_ref()
        .and_then(|t| t.grid.as_ref())
        .filter(|g| !g.beat_frames.is_empty())
        .ok_or_else(|| format!("the track of clip {} has no beat grid (analyse it first)", c.id))
}

/// The nearest value of `sorted` to `x` (its index).
fn nearest(sorted: &[f64], x: f64) -> Option<usize> {
    if sorted.is_empty() {
        return None;
    }
    let i = sorted.partition_point(|&v| v < x);
    let cand = [i.checked_sub(1), (i < sorted.len()).then_some(i)];
    cand.into_iter().flatten().min_by(|&a, &b| (sorted[a] - x).abs().total_cmp(&(sorted[b] - x).abs()))
}

/// The clip a beat match of `b` should follow: of the clips that become audible before `b`, the
/// one that stays audible longest.
pub fn previous_clip<'a>(doc: &'a MixDoc, b: &Clip) -> Option<&'a Clip> {
    doc.clips
        .iter()
        .filter(|c| c.id != b.id && c.visible_start < b.visible_start - 0.5)
        .max_by(|x, y| x.visible_end.total_cmp(&y.visible_end))
}

// ---------------------------------------------------------------------------------------------
// Beat matching
// ---------------------------------------------------------------------------------------------

/// The result of [`beatmatch`]: replace the clip's `tempo`, `start`, `length`, `trimStart` and
/// `trimEnd` with these values. Tools, loops and the key / gain nodes of the clip are in the
/// clip's local samples and have to be moved with the audio when the tempo changes (the
/// frontend's `retempo` does it with the same remap, `local' = local_at'(source_at(local))`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatmatchSuggestion {
    pub clip_id: String,
    /// The clip the target was matched to.
    pub reference_clip_id: String,
    /// New tempo curve: one node, the constant speed that brings the clip to `targetBpm`.
    pub tempo: Vec<TempoNode>,
    pub start: f64,
    pub length: f64,
    pub trim_start: f64,
    pub trim_end: f64,
    /// Tempo the clip now plays at: the reference clip's effective tempo where this clip enters.
    pub target_bpm: f64,
    pub speed: f64,
    /// Samples the clip's audible start moved on the timeline (positive = later).
    pub shift: f64,
    /// Beat index (into the clip's grid) pinned to a beat of the reference clip, and that beat.
    pub anchor_beat: usize,
    pub reference_beat: usize,
    /// Whether the bars line up too (needs a first downbeat in both grids; 4/4 assumed).
    pub bar_aligned: bool,
    /// Whether the reference clip is still audible where this clip enters.
    pub overlaps: bool,
    pub notes: Vec<String>,
}

/// Aligns clip `clip_id` to the clip before it: sets its tempo to the reference clip's tempo at
/// the entry point and moves it so that its beat nearest the cue lands on a beat of the
/// reference clip (on a matching bar position when both grids know their downbeat).
pub fn beatmatch(doc: &MixDoc, clip_id: &str) -> Result<BeatmatchSuggestion, String> {
    let b = doc.clip(clip_id).ok_or_else(|| format!("no clip {clip_id}"))?;
    let a = previous_clip(doc, b).ok_or("there is no earlier clip to match to")?;
    let (gb, ga) = (grid_of(b)?, grid_of(a)?);
    let mut notes = Vec::new();

    // tempo to reach: the reference clip's effective tempo where this clip enters
    let entry = (b.visible_start - a.span.start).clamp(a.span.trim_start, a.span.length - a.span.trim_end);
    let target = a.effective_bpm(entry);
    if !(target.is_finite() && target > 0.0) {
        return Err("the reference clip has no usable tempo".into());
    }
    if let Some(mix_bpm) = doc.tempo_bpm
        && (mix_bpm - target).abs() > 0.05
    {
        notes.push(format!("the reference clip plays at {target:.2} BPM, not the mix tempo {mix_bpm:.2}; matched to the clip"));
    }
    let b_bpm = if b.bpm > 0.0 {
        b.bpm
    } else {
        let mut v = gb.beat_bpm.clone();
        v.sort_by(f64::total_cmp);
        *v.get(v.len() / 2).ok_or("the clip has no tempo")?
    };
    let speed32 = (target / b_bpm) as f32;
    if !speed32.is_finite() || speed32 <= 0.0 {
        return Err("the tempo ratio is not usable".into());
    }
    let speed = f64::from(speed32);
    if !(0.5..=2.0).contains(&speed) {
        notes.push(format!("the clip needs a speed of {speed:.3}, a large stretch (check the beat grid for a half/double tempo)"));
    }

    // re-time the clip: the audio keeps its place relative to its own beats
    let old = b.tempo_map();
    let tempo = vec![TempoNode { pos: 0, speed: speed32 }];
    let new = map_of(&tempo);
    let remap = |p: f64| new.local_at(old.source_at(p));
    let length = remap(b.span.length);
    let trim_start = remap(b.span.trim_start);
    let trim_end = (length - remap(b.span.length - b.span.trim_end)).max(0.0);
    let start0 = b.span.start + b.span.trim_start - trim_start; // the cue stays where the user put it

    // anchor: the beat of this clip nearest its cue, in the new local axis
    let k_src = SAMPLE_RATE / f64::from(gb.source_rate.max(1));
    let b_local: Vec<f64> = gb.beat_frames.iter().map(|&f| new.local_at(f as f64 * k_src)).collect();
    let anchor = nearest(&b_local, trim_start).ok_or("the clip has no beats")?;
    let anchor_time = start0 + b_local[anchor];

    // reference beats on the timeline
    let a_times = a.beat_times();
    let bar_aware = ga.first_downbeat.is_some() && gb.first_downbeat.is_some();
    let phase = |i: usize, fd: Option<usize>| (i as i64 - fd.unwrap_or(0) as i64).rem_euclid(BEATS_PER_BAR);
    let want = phase(anchor, gb.first_downbeat);
    let pick = if bar_aware {
        let ok: Vec<usize> = (0..a_times.len()).filter(|&j| phase(j, ga.first_downbeat) == want).collect();
        let times: Vec<f64> = ok.iter().map(|&j| a_times[j]).collect();
        nearest(&times, anchor_time).map(|i| ok[i])
    } else {
        notes.push("no first downbeat in one of the grids: beats are aligned, bars are not guaranteed".into());
        nearest(&a_times, anchor_time)
    }
    .ok_or("the reference clip has no beats")?;
    let shift_align = a_times[pick] - anchor_time;
    let beat_len = 60.0 / target * SAMPLE_RATE;
    if shift_align.abs() > 2.0 * beat_len {
        notes.push(format!("the clip moves {:.2} s to find a matching beat", secs(shift_align)));
    }
    let start = start0 + shift_align;
    let new_cue = start + trim_start;
    Ok(BeatmatchSuggestion {
        clip_id: b.id.clone(),
        reference_clip_id: a.id.clone(),
        tempo,
        start,
        length,
        trim_start,
        trim_end,
        target_bpm: target,
        speed,
        shift: new_cue - b.visible_start,
        anchor_beat: anchor,
        reference_beat: pick,
        bar_aligned: bar_aware,
        overlaps: a.visible_end > new_cue,
        notes,
    })
}

// ---------------------------------------------------------------------------------------------
// Automatic transition
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSuggestion {
    /// The clip to add `tool` to (a new process at the end of its list).
    pub clip_id: String,
    /// Always a `ProcessDoc::Tool`.
    pub tool: ProcessDoc,
    /// Ids of existing EQ tools of that clip that overlap it (the Editor allows one per range).
    pub conflicts: Vec<String>,
}

#[cfg(test)]
impl ToolSuggestion {
    pub fn tool_doc(&self) -> &ToolDoc {
        match &self.tool {
            ProcessDoc::Tool(t) => t,
            _ => unreachable!("a tool suggestion always holds a tool"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionSuggestion {
    pub transition: TransitionDoc,
    /// Bass swap: cut the incoming clip's low band, hand it over at the middle of the overlap.
    pub tools: Vec<ToolSuggestion>,
    /// Length of the overlap in bars (4 beats) at the incoming clip's tempo.
    pub bars: f64,
    /// Samples in one beat of the incoming clip at the transition start (the unit the recipes
    /// in `state/transitionRecipes.ts` snap their envelope nodes to).
    pub beat: f64,
    /// How many bars both clips are audible together from the transition start (what the
    /// length could be at most), for the length choice in the UI.
    pub bars_available: f64,
    pub notes: Vec<String>,
}

/// Longest first: the lengths an automatic transition shrinks through when the requested one
/// does not fit.
const BAR_LADDER: [f64; 7] = [64.0, 32.0, 16.0, 8.0, 4.0, 2.0, 1.0];
/// The length asked for when none is given (the original behaviour).
const DEFAULT_BARS: f64 = 16.0;
/// Longest transition that may be asked for, in bars.
const MAX_BARS: f64 = 128.0;

/// The transition length for `avail` samples of shared audio: the requested number of bars
/// (rounded to whole beats) when it fits, else the largest ladder step not above it that does,
/// else whole beats. Returns `(length in samples, bars)`.
fn pick_length(avail: f64, beat: f64, requested: f64) -> (f64, f64) {
    let bar = beat * BEATS_PER_BAR as f64;
    let beats = (requested * BEATS_PER_BAR as f64).round().max(1.0);
    if beats * beat <= avail + 1.0 {
        return (beats * beat, beats / BEATS_PER_BAR as f64);
    }
    match BAR_LADDER.into_iter().find(|n| *n <= requested && n * bar <= avail + 1.0) {
        Some(n) => (n * bar, n),
        None => {
            let b = (avail / beat).floor().max(1.0);
            (b * beat, b / BEATS_PER_BAR as f64)
        }
    }
}

/// An EQ tool over timeline `[t0, t0 + len)` of `clip`, with Lo held flat then killed (or the
/// reverse) around `swap` (samples from `t0`), switching over one beat.
fn eq_swap_tool(clip: &Clip, t0: f64, len: f64, swap: f64, beat: f64, cut_after: bool) -> ToolDoc {
    let eq = m::Effect::eq();
    let last = (len - 1.0).max(1.0) as u32;
    let (open, shut) = (0.0_f32, EQ_LO_KILL);
    let s = swap.round() as u32;
    let b = beat.round().max(1.0) as u32;
    // outgoing: flat, drops over the beat before the swap; incoming: killed, opens over the beat after
    let lo = if cut_after {
        vec![(0, open), (s.saturating_sub(b), open), (s, shut), (last, shut)]
    } else {
        vec![(0, shut), (s, shut), (s.saturating_add(b).min(last), open), (last, open)]
    };
    let lo: Vec<EnvNode> = dedupe(lo).into_iter().map(|(pos, value)| EnvNode { pos, value }).collect();
    let flat = |v: f32| vec![EnvNode { pos: 0, value: v }, EnvNode { pos: last, value: v }];
    let mut params = BTreeMap::new();
    for key in eq.automation_keys() {
        let nodes = if key.ends_with("-Lo") { lo.clone() } else { flat(0.0) };
        params.insert(key, nodes);
    }
    let class = super::doc::EffectClass::Eq;
    ToolDoc {
        id: fresh_id(),
        legacy_session_id: None,
        span: Span { start: t0 - clip.span.start, length: len, start_sec: secs(t0 - clip.span.start), length_sec: secs(len), ..Span::default() },
        tool_type: eq.name.clone(),
        name: "ToolProcess".into(),
        bypassed: false,
        effects: vec![EffectDoc { name: eq.name, bypassed: false, class }],
        params,
    }
}

/// Keeps strictly increasing positions (a later node at the same position wins).
fn dedupe(v: Vec<(u32, f32)>) -> Vec<(u32, f32)> {
    let mut out: Vec<(u32, f32)> = Vec::new();
    for (p, x) in v {
        match out.last_mut() {
            Some(l) if l.0 >= p => *l = (l.0, x),
            _ => out.push((p, x)),
        }
    }
    out
}

/// Suggests the transition between `from` and `to`: the overlap starts at the incoming clip's
/// beat nearest its cue and lasts `bars` bars (default 16; rounded to whole beats; the largest
/// of 64, 32, ... 1 bars below it, else a number of beats, when the audio both clips share does
/// not allow it), a linear crossfade, and a bass swap with EQ tools on both clips. The other
/// recipes (state/transitionRecipes.ts) reshape the crossfade and tools on this geometry.
pub fn auto_transition(doc: &MixDoc, from_id: &str, to_id: &str, bars: Option<f64>) -> Result<TransitionSuggestion, String> {
    let requested = bars.unwrap_or(DEFAULT_BARS);
    if !requested.is_finite() || requested < 1.0 / BEATS_PER_BAR as f64 || requested > MAX_BARS {
        return Err(format!("a transition length of {requested} bars is not usable (a beat to {MAX_BARS} bars)"));
    }
    let a = doc.clip(from_id).ok_or_else(|| format!("no clip {from_id}"))?;
    let b = doc.clip(to_id).ok_or_else(|| format!("no clip {to_id}"))?;
    if a.id == b.id {
        return Err("a transition needs two different clips".into());
    }
    let mut notes = Vec::new();
    let bpm = b.effective_bpm(b.span.trim_start);
    if !(bpm.is_finite() && bpm > 0.0) {
        return Err("the incoming clip has no usable tempo".into());
    }
    let beat = 60.0 / bpm * SAMPLE_RATE;
    let bar = beat * BEATS_PER_BAR as f64;

    // the first beat of the incoming clip at or after its cue (within a beat of it)
    let b_times = b.beat_times();
    let t0 = match b_times.iter().copied().find(|&t| t >= b.visible_start - 1.0) {
        Some(t) if t - b.visible_start <= beat => t,
        _ => {
            notes.push("no beat grid for the incoming clip: the transition starts at its cue".into());
            b.visible_start
        }
    };
    // both clips have to be audible for the whole overlap: the outgoing one up to its end, the
    // incoming one from the transition start on
    let avail = a.visible_end.min(b.visible_end) - t0;
    if avail < beat * 0.5 {
        return Err(format!(
            "the clips do not overlap: the outgoing clip ends {:.1} s before the incoming one starts (move the later clip earlier)",
            secs(-avail)
        ));
    }
    let bars_avail = avail / bar;
    let (len, bars) = pick_length(avail, beat, requested);
    if bars < 1.0 {
        notes.push(format!("{} beat(s) is under a bar: no bass swap tools", (len / beat).round()));
    }
    if bars < requested {
        notes.push(format!("{bars_avail:.1} bars overlap: using {bars}"));
    }
    if (a.effective_bpm(t0 - a.span.start) - bpm).abs() > 0.1 {
        notes.push("the clips play at different tempos here: beat-match the incoming clip first".into());
    }

    let both_grids = grid_of(a).is_ok() && grid_of(b).is_ok();
    let transition = TransitionDoc {
        id: fresh_id(),
        legacy_session_id: None,
        span: Span { start: t0, length: len, start_sec: secs(t0), length_sec: secs(len), ..Span::default() },
        name: "Transition".into(),
        bypassed: false,
        beat_match: both_grids,
        crossfade_type: 1,
        curve: vec![CurveNode { x: 0.0, value: 0.0 }, CurveNode { x: 1.0, value: 1.0 }],
        between: Some(Between { from: a.id.clone(), to: b.id.clone() }),
    };

    let mut tools = Vec::new();
    if bars >= 1.0 {
        let swap = ((len / 2.0) / beat).round() * beat;
        for (clip, cut_after) in [(a, true), (b, false)] {
            let tool = eq_swap_tool(clip, t0, len, swap, beat, cut_after);
            let (lo, hi) = (tool.span.start, tool.span.end());
            let conflicts = clip
                .processes
                .iter()
                .filter_map(|p| match p {
                    ProcessDoc::Tool(t)
                        if t.effects.first().is_some_and(|x| x.name == "EQ") && t.span.start < hi && lo < t.span.end() =>
                    {
                        Some(t.id.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !conflicts.is_empty() {
                notes.push("an EQ tool already covers part of the overlap on one clip".into());
            }
            tools.push(ToolSuggestion { clip_id: clip.id.clone(), tool: ProcessDoc::Tool(tool), conflicts });
        }
    }
    Ok(TransitionSuggestion { transition, tools, bars, beat, bars_available: bars_avail, notes })
}

#[cfg(test)]
mod tests {
    use super::super::doc::{tests::pos, BeatGrid, FuzzyIdDoc, TrackInfo};
    use super::*;

    /// A clip on a locked grid: `n` beats at `bpm` from frame `first`, at 44.1 kHz, downbeat at
    /// beat `fd`.
    fn grid(bpm: f64, first: i64, n: usize, fd: Option<usize>) -> BeatGrid {
        let step = 60.0 / bpm * 44100.0;
        BeatGrid {
            source_rate: 44100,
            beat_start_offset: first,
            beat_frames: (0..n).map(|i| first + (step * i as f64).round() as i64).collect(),
            beat_bpm: vec![bpm; n],
            first_downbeat: fd,
        }
    }

    fn clip(id: &str, start: f64, length: f64, cue: f64, bpm: f64, speed: f32, g: BeatGrid) -> Clip {
        let t = m::Track::new(1, "x.mp3", bpm, speed, pos(start, length, id));
        let mut doc = MixDoc::from_mix(1, &{
            let mut mix = m::Mix::new("t");
            let mut t = t;
            t.position.l_offset = cue;
            mix.tracks.push(t);
            mix
        });
        let mut c = doc.clips.remove(0);
        c.track = Some(TrackInfo {
            track_id: 1,
            available: true,
            problem: None,
            title: id.into(),
            artist: String::new(),
            album: String::new(),
            genre: String::new(),
            key: String::new(),
            bpm,
            duration_secs: 300.0,
            source_rate: 44100,
            location: String::new(),
            grid: Some(g),
            grid_problem: None,
        });
        c.fuzzy_id = FuzzyIdDoc::default();
        c.id = id.into();
        c
    }

    fn doc_of(clips: Vec<Clip>, tempo: Option<f64>) -> MixDoc {
        let mut d = MixDoc::from_mix(1, &m::Mix::new("t"));
        d.clips = clips;
        d.tempo_bpm = tempo;
        d.refresh();
        d
    }

    #[test]
    fn beat_times_follow_the_tempo_curve() {
        let c = clip("a", 1000.0, 5_000_000.0, 0.0, 120.0, 1.0, grid(120.0, 0, 8, Some(0)));
        let t = c.beat_times();
        assert_eq!(t[0], 1000.0);
        assert!((t[1] - (1000.0 + 22050.0)).abs() < 1e-6);
        // 4 % faster playback: beats come 1/1.04 as far apart
        let c = clip("a", 0.0, 5_000_000.0, 0.0, 120.0, 1.04, grid(120.0, 0, 8, Some(0)));
        assert!((c.beat_times()[2] - 2.0 * 22050.0 / f64::from(1.04f32)).abs() < 1e-3);
    }

    #[test]
    fn beatmatch_sets_tempo_and_lands_on_a_matching_bar_beat() {
        // A: 128 BPM material played at 128 (speed 1), beats every 20671.875 samples, bar starts every 4th
        let a = clip("a", 0.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 5000, 400, Some(1)));
        // B: 124.5 BPM material, its first downbeat is beat 2, cue near 3 000 000 of local, placed arbitrarily
        let b = clip("b", 4_001_234.0, 9_000_000.0, 0.0, 124.5, 1.0, grid(124.5, 7000, 400, Some(2)));
        let d = doc_of(vec![a, b], Some(128.0));
        let s = beatmatch(&d, "b").unwrap();
        assert_eq!(s.reference_clip_id, "a");
        assert!((s.target_bpm - 128.0).abs() < 1e-9);
        assert!((s.speed - 128.0 / 124.5).abs() < 1e-6);
        assert!(s.bar_aligned && s.overlaps);
        // verify on the timeline: apply the suggestion and compare beat positions
        let mut d2 = d.clone();
        let c = d2.clips.iter_mut().find(|c| c.id == "b").unwrap();
        c.tempo = s.tempo.clone();
        c.span.start = s.start;
        c.span.length = s.length;
        c.span.trim_start = s.trim_start;
        c.span.trim_end = s.trim_end;
        let bt = c.beat_times();
        let at = d2.clips[0].beat_times();
        let bt_anchor = bt[s.anchor_beat];
        assert!((bt_anchor - at[s.reference_beat]).abs() < 1.0, "anchor beat must sit on the reference beat");
        // every later beat stays within a sample or so of a reference beat (both are locked at 128)
        for k in 0..32 {
            let t = bt[s.anchor_beat + k];
            let j = nearest(&at, t).unwrap();
            assert!((at[j] - t).abs() < 40.0, "beat {k}: {} off", at[j] - t);
        }
        // bar phase matches: (anchor - fdB) mod 4 == (ref - fdA) mod 4
        assert_eq!((s.anchor_beat as i64 - 2).rem_euclid(4), (s.reference_beat as i64 - 1).rem_euclid(4));
        // the audible start moved by less than half a bar... at most 2 beats from where the user put it unless bars force it
        assert!(s.shift.abs() < 2.5 * (60.0 / 128.0 * 44100.0), "shift {}", s.shift);
    }

    #[test]
    fn beatmatch_without_downbeats_aligns_beats_and_says_so() {
        let a = clip("a", 0.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 5000, 400, None));
        let b = clip("b", 3_000_000.0, 9_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 100, 400, Some(0)));
        let d = doc_of(vec![a, b], None);
        let s = beatmatch(&d, "b").unwrap();
        assert!(!s.bar_aligned);
        assert!(s.notes.iter().any(|n| n.contains("downbeat")));
        assert!((s.speed - 1.0).abs() < 1e-6);
    }

    #[test]
    fn beatmatch_keeps_the_cue_when_the_tempo_changes() {
        // B has a cue at local 2 000 000 and a 3 % tempo change: its cue (source position) must not move
        let a = clip("a", 0.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 5000, 400, None));
        let b = clip("b", 1_000_000.0, 9_000_000.0, 2_000_000.0, 124.0, 1.0, grid(124.0, 100, 400, None));
        let d = doc_of(vec![a, b], None);
        let s = beatmatch(&d, "b").unwrap();
        let old = TempoMap::new(&[m::SpeedNode { position: 0, speed: 1.0 }]);
        let new = map_of(&s.tempo);
        assert!((new.source_at(s.trim_start) - old.source_at(2_000_000.0)).abs() < 1.0);
        assert!((new.source_at(s.length) - 9_000_000.0).abs() < 1.0, "same audio, new length {}", s.length);
    }

    #[test]
    fn beatmatch_errors_are_clear() {
        let a = clip("a", 0.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 10, None));
        let d = doc_of(vec![a.clone()], None);
        assert!(beatmatch(&d, "a").unwrap_err().contains("earlier clip"));
        assert!(beatmatch(&d, "zz").is_err());
        let mut b = clip("b", 3_000_000.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 10, None));
        b.track.as_mut().unwrap().grid = None;
        let d = doc_of(vec![a, b], None);
        assert!(beatmatch(&d, "b").unwrap_err().contains("no beat grid"));
    }

    #[test]
    fn auto_transition_is_16_bars_with_a_bass_swap() {
        let a = clip("a", 0.0, 20_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 800, Some(0)));
        let mut b = clip("b", 8_000_000.0, 20_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 3000, 800, Some(0)));
        b.channel = 1;
        let d = doc_of(vec![a, b], Some(128.0));
        let s = auto_transition(&d, "a", "b", None).unwrap();
        let bar = 4.0 * 60.0 / 128.0 * 44100.0;
        assert_eq!(s.bars, 16.0);
        assert!((s.transition.span.length - 16.0 * bar).abs() < 1e-6);
        // starts on a beat of b near its cue
        let bt = d.clip("b").unwrap().beat_times();
        assert!(bt.iter().any(|t| (t - s.transition.span.start).abs() < 1e-6));
        assert_eq!(s.transition.between, Some(Between { from: "a".into(), to: "b".into() }));
        assert!(s.transition.beat_match);
        assert_eq!(s.tools.len(), 2);
        let lo_a = &s.tools[0].tool_doc().params["EQ-Lo"];
        let lo_b = &s.tools[1].tool_doc().params["EQ-Lo"];
        assert_eq!((lo_a.first().unwrap().value, lo_a.last().unwrap().value), (0.0, EQ_LO_KILL));
        assert_eq!((lo_b.first().unwrap().value, lo_b.last().unwrap().value), (EQ_LO_KILL, 0.0));
        assert!(lo_a.windows(2).all(|w| w[1].pos > w[0].pos) && lo_b.windows(2).all(|w| w[1].pos > w[0].pos));
        // tools sit in each clip's local axis over exactly the overlap
        let t = s.transition.span.start;
        assert!((s.tools[0].tool_doc().span.start - (t - 0.0)).abs() < 1e-6);
        assert!((s.tools[1].tool_doc().span.start - (t - 8_000_000.0)).abs() < 1e-6);
        // applying the suggestion yields a document that validates
        let mut d2 = d.clone();
        d2.transitions.push(s.transition.clone());
        for ts in &s.tools {
            let c = d2.clips.iter_mut().find(|c| c.id == ts.clip_id).unwrap();
            c.processes.push(ts.tool.clone());
        }
        d2.refresh();
        assert_eq!(d2.validate(None), Vec::<String>::new());
    }

    #[test]
    fn auto_transition_shrinks_to_what_fits() {
        let a = clip("a", 0.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 800, Some(0)));
        // overlap of ~ 3 bars
        let bar = 4.0 * 60.0 / 128.0 * 44100.0;
        let b = clip("b", 8_000_000.0 - 3.3 * bar, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 800, Some(0)));
        let d = doc_of(vec![a.clone(), b], Some(128.0));
        let s = auto_transition(&d, "a", "b", None).unwrap();
        assert_eq!(s.bars, 2.0);
        // no overlap at all
        let far = clip("c", 9_000_000.0, 1_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 100, None));
        let d = doc_of(vec![a, far], None);
        assert!(auto_transition(&d, "a", "c", None).unwrap_err().contains("do not overlap"));
        assert!(auto_transition(&d, "a", "a", None).is_err());
    }

    #[test]
    fn auto_transition_honours_the_requested_length() {
        let a = clip("a", 0.0, 40_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 1600, Some(0)));
        let mut b = clip("b", 8_000_000.0, 40_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 3000, 1600, Some(0)));
        b.channel = 1;
        let d = doc_of(vec![a, b], Some(128.0));
        let beat = 60.0 / 128.0 * 44100.0;
        // lengths the old ladder never offered: 32 bars, 4 bars, and a fraction of a bar
        for want in [32.0, 4.0, 1.0, 0.5, 0.25] {
            let s = auto_transition(&d, "a", "b", Some(want)).unwrap();
            assert_eq!(s.bars, want);
            assert!((s.transition.span.length - want * 4.0 * beat).abs() < 1e-6, "{want} bars");
            assert!((s.beat - beat).abs() < 1e-9);
            assert!(s.bars_available > 32.0);
        }
        // under a bar: no bass swap tools and a note saying so
        let s = auto_transition(&d, "a", "b", Some(0.25)).unwrap();
        assert!(s.tools.is_empty());
        assert!(s.notes.iter().any(|n| n.contains("under a bar")));
        // None is the old default
        assert_eq!(auto_transition(&d, "a", "b", None).unwrap().bars, 16.0);
        // not usable
        for bad in [0.0, -1.0, 0.1, 200.0, f64::NAN, f64::INFINITY] {
            assert!(auto_transition(&d, "a", "b", Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn auto_transition_shrinks_a_long_request_and_says_so() {
        let a = clip("a", 0.0, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 800, Some(0)));
        let bar = 4.0 * 60.0 / 128.0 * 44100.0;
        let b = clip("b", 8_000_000.0 - 11.3 * bar, 8_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 800, Some(0)));
        let d = doc_of(vec![a, b], Some(128.0));
        // 11 bars shared: 32 bars shrinks to 8, 4 bars still fits
        let s = auto_transition(&d, "a", "b", Some(32.0)).unwrap();
        assert_eq!(s.bars, 8.0);
        assert!(s.bars_available > 10.0 && s.bars_available < 12.0);
        assert!(s.notes.iter().any(|n| n.contains("using 8")));
        assert_eq!(auto_transition(&d, "a", "b", Some(4.0)).unwrap().bars, 4.0);
        // a length that is not on the ladder is kept when it fits
        assert_eq!(auto_transition(&d, "a", "b", Some(6.0)).unwrap().bars, 6.0);
    }

    #[test]
    fn auto_transition_never_outlasts_the_incoming_clip() {
        // b has only ~5 bars of audio left after its first beat although a keeps playing
        let bar = 4.0 * 60.0 / 128.0 * 44100.0;
        let a = clip("a", 0.0, 40_000_000.0, 0.0, 128.0, 1.0, grid(128.0, 0, 1600, Some(0)));
        let mut b = clip("b", 8_000_000.0, 5.2 * bar, 0.0, 128.0, 1.0, grid(128.0, 3000, 1600, Some(0)));
        b.channel = 1;
        let d = doc_of(vec![a, b], Some(128.0));
        let s = auto_transition(&d, "a", "b", Some(16.0)).unwrap();
        assert_eq!(s.bars, 4.0);
        let b_end = d.clip("b").unwrap().visible_end;
        assert!(s.transition.span.start + s.transition.span.length <= b_end + 1.0);
    }
}
