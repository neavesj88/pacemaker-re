//! Library operations for beat-grid and cue editing (owner: grid area).
//!
//! Two parts: pure grid edit math on [`GridDoc`] (a serialisable copy of
//! [`strfile::Analysis`]), and `impl Library` operations that load a track's grid, store it
//! back through [`Library::store_analysis`] (so the `.str` stays byte-compatible and the row
//! conventions hold) and read / write the device cue point.
//!
//! `tracks.cue_point` is stored in source sample frames, the same unit as the `.str` beat
//! positions. UNVERIFIED against a device: the unset sentinel is verified (FINDINGS 4c), the
//! unit of a set value is inferred from `mixtracks.cue_point` values, which are of the order of
//! sample frames.

use std::path::Path;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::library::{Error, Library, EDITOR_ID, NO_CUE};
use crate::strfile::{self, Analysis, Beat};

/// Tempo range the device and the editor accept for a typed or derived BPM.
pub const MIN_BPM: f64 = 20.0;
pub const MAX_BPM: f64 = 400.0;

/// Bars are four beats; `first_downbeat` is the phase of the first bar line, 0..=3 once edited.
pub(crate) const BAR: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridBeat {
    pub pos: i64,
    pub bpm: f64,
}

/// A `.str` as the editor holds it. Same content as [`strfile::Analysis`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridDoc {
    pub normalize_factor: f32,
    pub beat_start_offset: i64,
    pub beats: Vec<GridBeat>,
    pub first_downbeat: Option<usize>,
}

impl From<&Analysis> for GridDoc {
    fn from(a: &Analysis) -> Self {
        GridDoc {
            normalize_factor: a.normalize_factor,
            beat_start_offset: a.beat_start_offset,
            beats: a.beats.iter().map(|b| GridBeat { pos: b.pos, bpm: b.bpm }).collect(),
            first_downbeat: a.first_downbeat,
        }
    }
}

impl From<&GridDoc> for Analysis {
    fn from(d: &GridDoc) -> Self {
        Analysis {
            normalize_factor: d.normalize_factor,
            beat_start_offset: d.beat_start_offset,
            beats: d.beats.iter().map(|b| Beat { pos: b.pos, bpm: b.bpm }).collect(),
            first_downbeat: d.first_downbeat,
        }
    }
}

/// What the edits need to know about the audio.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EditCtx {
    pub sample_rate: u32,
    /// Length of the audio in sample frames, when known; beats past it are dropped.
    pub total_frames: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GridEdit {
    /// Move every beat by this many frames (negative = earlier).
    Nudge { frames: i64 },
    /// Make the beat nearest `frame` a bar line (the bar phase follows from its index).
    SetDownbeat { frame: i64 },
    /// Half the tempo: keep every second beat, the downbeat parity included.
    Halve,
    /// Double the tempo: add a beat midway between every two.
    Double,
    /// Constant grid at `bpm` from the first beat to the end of the audio.
    SetBpm { bpm: f64 },
    /// Move the beat lines in time by this many beat periods (negative = earlier).
    ShiftBeats { beats: i32 },
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn invalid<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(msg.into()))
}

/// Frames in one beat at `bpm`.
pub(crate) fn period_frames(bpm: f64, sample_rate: u32) -> f64 {
    f64::from(sample_rate) * 60.0 / bpm
}

/// Checks that a grid is something the device can play: at least one beat, increasing
/// non-negative positions, sane tempi and a downbeat index that exists.
pub fn validate(doc: &GridDoc) -> Result<(), Error> {
    if doc.beats.is_empty() {
        return invalid("the grid has no beats");
    }
    let mut prev = -1;
    for (i, b) in doc.beats.iter().enumerate() {
        if b.pos < 0 || b.pos <= prev {
            return invalid(format!("beat {i} is not after the beat before it"));
        }
        if !b.bpm.is_finite() || b.bpm <= 0.0 {
            return invalid(format!("beat {i} has tempo {}", b.bpm));
        }
        prev = b.pos;
    }
    if doc.first_downbeat.is_some_and(|f| f >= doc.beats.len()) {
        return invalid("the first downbeat is past the last beat");
    }
    Ok(())
}

/// Result of an edit: the offset follows the first beat (whatever the original relation was,
/// it is kept constant), the downbeat index is clamped to the beats that remain.
pub(crate) fn finish(old: &GridDoc, beats: Vec<GridBeat>, first_downbeat: Option<usize>) -> Result<GridDoc, Error> {
    let Some(first) = beats.first() else {
        return invalid("the edit would leave the grid empty");
    };
    let old_first = old.beats.first().map_or(0, |b| b.pos);
    let doc = GridDoc {
        normalize_factor: old.normalize_factor,
        beat_start_offset: old.beat_start_offset + (first.pos - old_first),
        first_downbeat: first_downbeat.filter(|&f| f < beats.len()),
        beats,
    };
    validate(&doc)?;
    Ok(doc)
}

/// Drops beats before 0 (a prefix) and past the audio end, keeping the bar phase.
pub(crate) fn clip(mut beats: Vec<GridBeat>, fd: Option<usize>, total: Option<i64>) -> (Vec<GridBeat>, Option<usize>) {
    let dropped = beats.iter().take_while(|b| b.pos < 0).count();
    beats.drain(..dropped);
    if let Some(t) = total {
        beats.retain(|b| b.pos <= t);
    }
    let fd = fd.map(|f| {
        if f >= dropped {
            f - dropped
        } else {
            (f as i64 - dropped as i64).rem_euclid(BAR) as usize
        }
    });
    (beats, fd)
}

pub fn apply(doc: &GridDoc, edit: GridEdit, ctx: EditCtx) -> Result<GridDoc, Error> {
    validate(doc)?;
    if ctx.sample_rate == 0 {
        return invalid("sample rate is 0");
    }
    match edit {
        GridEdit::Nudge { frames } => {
            let beats = doc.beats.iter().map(|b| GridBeat { pos: b.pos + frames, bpm: b.bpm }).collect();
            let (beats, fd) = clip(beats, doc.first_downbeat, ctx.total_frames);
            finish(doc, beats, fd)
        }
        GridEdit::SetDownbeat { frame } => {
            let k = doc
                .beats
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| (b.pos - frame).abs())
                .map(|(i, _)| i)
                .expect("validated: not empty");
            finish(doc, doc.beats.clone(), Some(k % BAR as usize))
        }
        GridEdit::Halve => {
            if doc.beats.len() < 2 {
                return invalid("a one-beat grid cannot be halved");
            }
            let fd = doc.first_downbeat;
            let parity = fd.unwrap_or(0) % 2;
            let beats: Vec<GridBeat> = doc.beats[parity..]
                .iter()
                .step_by(2)
                .map(|b| GridBeat { pos: b.pos, bpm: b.bpm / 2.0 })
                .collect();
            if beats[0].bpm < MIN_BPM {
                return invalid(format!("{:.2} BPM is below the minimum of {MIN_BPM}", beats[0].bpm));
            }
            finish(doc, beats, fd.map(|f| (f / 2) % BAR as usize))
        }
        GridEdit::Double => {
            if doc.beats.len() < 2 {
                return invalid("a one-beat grid cannot be doubled");
            }
            let mut beats = Vec::with_capacity(doc.beats.len() * 2);
            for (i, b) in doc.beats.iter().enumerate() {
                let bpm = b.bpm * 2.0;
                if bpm > MAX_BPM {
                    return invalid(format!("{bpm:.2} BPM is above the maximum of {MAX_BPM}"));
                }
                beats.push(GridBeat { pos: b.pos, bpm });
                // the last beat has no successor: reuse the gap before it
                let gap = match doc.beats.get(i + 1) {
                    Some(n) => n.pos - b.pos,
                    None => b.pos - doc.beats[i - 1].pos,
                };
                let mid = b.pos + gap / 2;
                if mid > b.pos && mid > beats.last().map_or(-1, |l| l.pos) && ctx.total_frames.is_none_or(|t| mid <= t) {
                    beats.push(GridBeat { pos: mid, bpm });
                }
            }
            finish(doc, beats, doc.first_downbeat.map(|f| (f * 2) % BAR as usize))
        }
        GridEdit::SetBpm { bpm } => {
            if !bpm.is_finite() || !(MIN_BPM..=MAX_BPM).contains(&bpm) {
                return invalid(format!("tempo must be between {MIN_BPM} and {MAX_BPM} BPM"));
            }
            let first = doc.beats[0].pos;
            let end = ctx.total_frames.unwrap_or(doc.beats[doc.beats.len() - 1].pos).max(first);
            let period = period_frames(bpm, ctx.sample_rate);
            let n = ((end - first) as f64 / period).floor() as usize + 1;
            let beats = (0..n)
                .map(|i| GridBeat { pos: first + (i as f64 * period).round() as i64, bpm })
                .collect();
            finish(doc, beats, doc.first_downbeat)
        }
        GridEdit::ShiftBeats { beats: n } => {
            if n == 0 || n.abs() > 64 {
                return invalid("shift by 1 to 64 beats");
            }
            let first = doc.beats[0];
            let period = period_frames(first.bpm, ctx.sample_rate);
            let delta = (f64::from(n) * period).round() as i64;
            let moved = doc.beats.iter().map(|b| GridBeat { pos: b.pos + delta, bpm: b.bpm }).collect();
            let (mut beats, mut fd) = clip(moved, doc.first_downbeat, ctx.total_frames);
            if n > 0 {
                // keep the grid covering the start of the track: extend backwards
                let mut added = 0usize;
                let mut head = beats.first().copied().unwrap_or(GridBeat { pos: first.pos + delta, bpm: first.bpm });
                let mut front = Vec::new();
                loop {
                    let p = head.pos - period_frames(head.bpm, ctx.sample_rate).round() as i64;
                    if p < 0 {
                        break;
                    }
                    head = GridBeat { pos: p, bpm: head.bpm };
                    front.push(head);
                    added += 1;
                }
                if added > 0 {
                    front.reverse();
                    front.append(&mut beats);
                    beats = front;
                    fd = fd.map(|f| (f + added) % BAR as usize);
                }
            }
            finish(doc, beats, fd)
        }
    }
}

/// Everything the grid editor needs to open a track.
#[derive(Debug, Clone, Serialize)]
pub struct GridLoad {
    pub track_id: i64,
    pub location: String,
    pub title: String,
    pub artist: String,
    pub sample_rate: u32,
    pub duration_secs: f64,
    /// Upper bound of the audio length in source frames: `(duration_secs + 1) * sample_rate`,
    /// because the stored duration is truncated to whole seconds.
    pub total_frames: i64,
    /// The device cue point in source frames, `None` when unset.
    pub cue_frames: Option<i64>,
    /// `None` when the `.str` is missing, empty or unreadable; `problem` says why.
    pub doc: Option<GridDoc>,
    pub problem: Option<String>,
}

/// A stored cue is "unset" when NULL, negative (the computer-side library uses -1) or the
/// device's 2^64 sentinel.
fn cue_from_stored(v: Option<f64>) -> Option<i64> {
    v.filter(|&c| (0.0..1e18).contains(&c)).map(|c| c as i64)
}

impl Library {
    /// The device cue point of a track in source frames, `None` when unset.
    pub fn track_cue(&self, track_id: i64) -> Result<Option<i64>, Error> {
        let stored: Option<Option<f64>> = self
            .con
            .query_row("SELECT cue_point FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |r| r.get(0))
            .optional()?;
        match stored {
            Some(v) => Ok(cue_from_stored(v)),
            None => invalid(format!("no track {track_id}")),
        }
    }

    /// Sets (`Some(frames)`) or clears (`None`: the device's 2^64 sentinel) a track's cue point,
    /// with `date_modified` / `modified_by_ed` like the other single-column edits, and only when
    /// the value changes. Returns whether the row changed.
    pub fn set_track_cue(&mut self, track_id: i64, cue: Option<i64>) -> Result<bool, Error> {
        if cue.is_some_and(|c| c < 0) {
            return invalid("a cue point cannot be before the start of the track");
        }
        let stored: Option<Option<f64>> = self
            .con
            .query_row("SELECT cue_point FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |r| r.get(0))
            .optional()?;
        let Some(stored) = stored else {
            return invalid(format!("no track {track_id}"));
        };
        let new = cue.map_or(NO_CUE, |c| c as f64);
        // compare as stored: a -1 or NULL "unset" is rewritten to the device sentinel only when
        // the user actually sets a cue, never by clearing an already-unset cue
        if cue_from_stored(stored) == cue && (cue.is_some() || stored.is_none_or(|s| s == NO_CUE || s < 0.0)) {
            return Ok(false);
        }
        self.con.execute(
            "UPDATE tracks SET cue_point = ?1, date_modified = ?2, modified_by_ed = ?3 WHERE track_id = ?4",
            params![new, now_ms(), EDITOR_ID, track_id],
        )?;
        Ok(true)
    }

    /// Loads a track's grid, cue and the facts the editor needs from the row and the `.str`.
    pub fn load_grid(&self, track_id: i64) -> Result<GridLoad, Error> {
        #[allow(clippy::type_complexity)]
        let row: Option<(String, String, String, i64, i64, Option<f64>)> = self
            .con
            .query_row(
                "SELECT IFNULL(location,''), IFNULL(title,''), IFNULL(artist,''), IFNULL(sample_rate,0),
                        IFNULL(play_time_secs,0), cue_point
                 FROM tracks WHERE track_id = ?1 AND track_id >= 0",
                [track_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?;
        let Some((location, title, artist, rate, secs, cue)) = row else {
            return invalid(format!("no track {track_id}"));
        };
        let (doc, problem) = match self.track_paths(&location) {
            None => (None, Some("track is not stored under /pmdb_tracks/".to_string())),
            Some((_, sidecar)) => match read_sidecar(&sidecar) {
                Ok(a) => (Some(GridDoc::from(&a)), None),
                Err(p) => (None, Some(p)),
            },
        };
        // a row without a sample rate (never written by the Editor) is assumed CD-rate
        let sample_rate = if rate > 0 { rate as u32 } else { 44_100 };
        Ok(GridLoad {
            track_id,
            location,
            title,
            artist,
            sample_rate,
            duration_secs: secs as f64,
            // play_time_secs is truncated, so the audio can run up to a second longer: allow one
            // second of slack so no real beat in the last partial second is dropped or missing
            total_frames: (secs + 1) * i64::from(sample_rate),
            cue_frames: cue_from_stored(cue),
            doc,
            problem,
        })
    }

    /// Writes an edited grid for a track through [`Library::store_analysis`] (atomic temp file
    /// then rename, no backup left on the volume; row bpm / flags / `date_modified` updated).
    pub fn save_grid(&mut self, track_id: i64, doc: &GridDoc) -> Result<(), Error> {
        validate(doc)?;
        let location: String = self
            .con
            .query_row("SELECT IFNULL(location,'') FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::Invalid(format!("no track {track_id}")))?;
        self.store_analysis(track_id, &location, &Analysis::from(doc))
    }
}

fn read_sidecar(path: &Path) -> Result<Analysis, String> {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("no .str file".into()),
        Err(e) => Err(format!("cannot read .str: {e}")),
        Ok(b) if b.is_empty() => Err("empty .str file (0 bytes)".into()),
        Ok(b) => strfile::decode(&String::from_utf8_lossy(&b)).map_err(|e| format!("unreadable .str: {e:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SR: u32 = 44_100;

    fn ctx() -> EditCtx {
        EditCtx { sample_rate: SR, total_frames: Some(SR as i64 * 60) }
    }

    /// A constant grid at `bpm` from `first`, `n` beats, downbeat phase `fd`.
    fn grid(bpm: f64, first: i64, n: usize, fd: Option<usize>) -> GridDoc {
        let p = period_frames(bpm, SR);
        GridDoc {
            normalize_factor: 1.0,
            beat_start_offset: first,
            beats: (0..n).map(|i| GridBeat { pos: first + (i as f64 * p).round() as i64, bpm }).collect(),
            first_downbeat: fd,
        }
    }

    #[test]
    fn nudge_moves_everything_and_the_offset() {
        let g = grid(120.0, 5000, 16, Some(1));
        let n = apply(&g, GridEdit::Nudge { frames: 44 }, ctx()).unwrap();
        assert_eq!(n.beats.len(), 16);
        assert!(n.beats.iter().zip(&g.beats).all(|(a, b)| a.pos == b.pos + 44));
        assert_eq!(n.beat_start_offset, 5044);
        assert_eq!(n.first_downbeat, Some(1));
        let back = apply(&n, GridEdit::Nudge { frames: -44 }, ctx()).unwrap();
        assert_eq!(back, g, "nudge and nudge back is the identity");
    }

    #[test]
    fn nudge_before_zero_drops_beats_and_keeps_bar_phase() {
        // beats at 100, 22150, 44200, ...; bar lines at indexes 2, 6, 10 (phase 2)
        let g = grid(120.0, 100, 12, Some(2));
        let n = apply(&g, GridEdit::Nudge { frames: -30_000 }, ctx()).unwrap();
        assert_eq!(n.beats.len(), 10, "beats 0 and 1 fall before the start");
        assert_eq!(n.beats[0].pos, 100 + 2 * 22_050 - 30_000);
        // the old bar line (index 2) is now index 0
        assert_eq!(n.first_downbeat, Some(0));
        // drop the first three: bar lines were at 2, 6: new indexes -1 and 3 -> phase 3
        let n3 = apply(&g, GridEdit::Nudge { frames: -50_000 }, ctx()).unwrap();
        assert_eq!(n3.first_downbeat, Some(3));
        assert_eq!(n3.beat_start_offset, n3.beats[0].pos);
    }

    #[test]
    fn nudge_past_the_end_trims_and_empty_is_an_error() {
        let g = grid(120.0, 0, 4, Some(0));
        let c = EditCtx { sample_rate: SR, total_frames: Some(70_000) };
        let n = apply(&g, GridEdit::Nudge { frames: 10_000 }, c).unwrap();
        assert!(n.beats.iter().all(|b| b.pos <= 70_000));
        assert!(apply(&g, GridEdit::Nudge { frames: 1_000_000 }, c).is_err());
    }

    #[test]
    fn set_downbeat_takes_the_nearest_beat_mod_four() {
        let g = grid(120.0, 0, 40, Some(0));
        let at = g.beats[37].pos + 500;
        let d = apply(&g, GridEdit::SetDownbeat { frame: at }, ctx()).unwrap();
        assert_eq!(d.first_downbeat, Some(1), "37 % 4");
        assert_eq!(d.beats, g.beats, "positions untouched");
        let d0 = apply(&g, GridEdit::SetDownbeat { frame: -10 }, ctx()).unwrap();
        assert_eq!(d0.first_downbeat, Some(0));
    }

    #[test]
    fn halve_keeps_every_second_beat_with_the_downbeat_parity() {
        let g = grid(128.0, 1000, 20, Some(3));
        let h = apply(&g, GridEdit::Halve, ctx()).unwrap();
        // parity 1: keeps old beats 1, 3, 5, ...; old beat 3 is new index 1
        assert_eq!(h.beats.len(), 10);
        assert_eq!(h.beats[0].pos, g.beats[1].pos);
        assert_eq!(h.beats[1].pos, g.beats[3].pos);
        assert!(h.beats.iter().all(|b| b.bpm == 64.0));
        assert_eq!(h.first_downbeat, Some(1));
        assert_eq!(h.beat_start_offset, g.beats[1].pos, "offset follows the first beat");
        assert!(apply(&grid(30.0, 0, 8, None), GridEdit::Halve, ctx()).is_err(), "below the minimum tempo");
    }

    #[test]
    fn double_adds_midpoints_and_doubles_the_phase() {
        let g = grid(64.0, 0, 6, Some(1));
        let d = apply(&g, GridEdit::Double, ctx()).unwrap();
        assert_eq!(d.beats.len(), 12, "the last beat also gets a midpoint");
        assert_eq!(d.beats[2].pos, g.beats[1].pos);
        assert_eq!(d.beats[1].pos, (g.beats[0].pos + g.beats[1].pos) / 2);
        assert!(d.beats.iter().all(|b| b.bpm == 128.0));
        assert_eq!(d.first_downbeat, Some(2));
        assert!(apply(&grid(250.0, 0, 6, None), GridEdit::Double, ctx()).is_err(), "above the maximum tempo");
    }

    #[test]
    fn halve_then_double_returns_to_the_same_beat_positions() {
        let g = grid(120.0, 2000, 40, Some(0));
        let back = apply(&apply(&g, GridEdit::Halve, ctx()).unwrap(), GridEdit::Double, ctx()).unwrap();
        for b in &g.beats {
            assert!(back.beats.iter().any(|x| (x.pos - b.pos).abs() <= 1), "beat at {} survives", b.pos);
        }
    }

    #[test]
    fn set_bpm_rebuilds_a_constant_grid_from_the_first_beat() {
        let mut g = grid(120.0, 3000, 8, Some(2));
        g.beats[3].pos += 900; // a wandering beat disappears in the rebuild
        let n = apply(&g, GridEdit::SetBpm { bpm: 125.0 }, ctx()).unwrap();
        assert_eq!(n.beats[0].pos, 3000);
        assert_eq!(n.first_downbeat, Some(2));
        let p = period_frames(125.0, SR);
        for (i, b) in n.beats.iter().enumerate() {
            assert_eq!(b.pos, 3000 + (i as f64 * p).round() as i64);
            assert_eq!(b.bpm, 125.0);
        }
        // covers the track: next beat would be past the 60 s mark
        let last = n.beats.last().unwrap().pos;
        assert!(last <= SR as i64 * 60 && last + p as i64 + 1 > SR as i64 * 60);
        assert!(apply(&g, GridEdit::SetBpm { bpm: 5.0 }, ctx()).is_err());
        assert!(apply(&g, GridEdit::SetBpm { bpm: f64::NAN }, ctx()).is_err());
    }

    #[test]
    fn shift_beats_moves_in_time_and_keeps_start_coverage() {
        let g = grid(120.0, 30_000, 30, Some(0)); // period 22050
        let later = apply(&g, GridEdit::ShiftBeats { beats: 1 }, ctx()).unwrap();
        // the first beat moved to 52050, then the grid was extended back to cover 0..52050
        assert_eq!(later.beats[0].pos, 52_050 - 2 * 22_050);
        assert!(later.beats.iter().any(|b| b.pos == 52_050));
        assert_eq!(later.beats.iter().filter(|b| b.pos < 30_000).count(), 1, "one beat now precedes the old first");
        // the old downbeat (index 0) is now at index `added`
        let added = later.beats.iter().position(|b| b.pos == 52_050).unwrap();
        assert_eq!(later.first_downbeat, Some(added % 4));
        let earlier = apply(&g, GridEdit::ShiftBeats { beats: -1 }, ctx()).unwrap();
        assert_eq!(earlier.beats[0].pos, 30_000 - 22_050);
        assert!(apply(&g, GridEdit::ShiftBeats { beats: 0 }, ctx()).is_err());
    }

    #[test]
    fn validate_rejects_broken_grids() {
        let mut g = grid(120.0, 0, 4, Some(0));
        assert!(validate(&g).is_ok());
        g.first_downbeat = Some(4);
        assert!(validate(&g).is_err());
        g.first_downbeat = None;
        g.beats[2].pos = g.beats[1].pos;
        assert!(validate(&g).is_err());
        let empty = GridDoc { beats: vec![], ..grid(120.0, 0, 1, None) };
        assert!(validate(&empty).is_err());
        let mut bad = grid(120.0, 0, 3, None);
        bad.beats[1].bpm = 0.0;
        assert!(validate(&bad).is_err());
    }

    // ---- Library operations (synthetic volumes in temp dirs) ----

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-grid-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A library with one synthetic track; returns (library, volume dir, source dir, track id).
    fn one_track(tag: &str) -> (Library, PathBuf, PathBuf, i64) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src_dir = temp_dir(&format!("{tag}-src"));
        let src = src_dir.join("Synthetic Track.mp3");
        std::fs::write(&src, vec![0u8; 4321]).unwrap();
        let analysis = Analysis::from(&grid(128.0, 500, 32, Some(0)));
        let meta = crate::library::TrackMeta {
            title: "Synthetic Track".into(),
            artist: "Synthetic Artist".into(),
            format: "MP3".into(),
            sample_rate: 44_100,
            play_time_secs: 60,
            ..Default::default()
        };
        let mut lib = Library::create(&vol).unwrap();
        let id = lib.add_track(&src, &meta, &analysis).unwrap();
        (lib, vol, src_dir, id)
    }

    fn cleanup(lib: Library, dirs: &[&PathBuf]) {
        drop(lib);
        for d in dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn cue_set_clear_and_row_conventions() {
        let (mut lib, vol, src, id) = one_track("cue");
        assert_eq!(lib.track_cue(id).unwrap(), None, "a fresh track has the 2^64 sentinel");
        assert!(!lib.set_track_cue(id, None).unwrap(), "clearing an unset cue changes nothing");

        assert!(lib.set_track_cue(id, Some(123_456)).unwrap());
        assert_eq!(lib.track_cue(id).unwrap(), Some(123_456));
        assert!(!lib.set_track_cue(id, Some(123_456)).unwrap(), "same value: no write");
        let (by, modified): (String, i64) = lib
            .con
            .query_row("SELECT modified_by_ed, date_modified FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(by, EDITOR_ID);
        assert!(modified > 1_600_000_000_000);

        assert!(lib.set_track_cue(id, None).unwrap());
        let (kind, v): (String, f64) = lib
            .con
            .query_row("SELECT typeof(cue_point), cue_point FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((kind.as_str(), v), ("real", NO_CUE), "cleared back to the device sentinel, as a REAL");
        assert_eq!(lib.track_cue(id).unwrap(), None);

        assert!(lib.set_track_cue(id, Some(-5)).is_err());
        assert!(lib.set_track_cue(9999, Some(1)).is_err());
        assert!(lib.track_cue(9999).is_err());
        assert!(lib.compatibility_problems().unwrap().is_empty());
        cleanup(lib, &[&vol, &src]);
    }

    #[test]
    fn computer_side_minus_one_reads_as_unset_and_is_left_alone() {
        let (mut lib, vol, src, id) = one_track("cue-neg");
        lib.con.execute("UPDATE tracks SET cue_point = -1", []).unwrap();
        assert_eq!(lib.track_cue(id).unwrap(), None);
        assert!(!lib.set_track_cue(id, None).unwrap());
        let v: f64 = lib.con.query_row("SELECT cue_point FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(v, -1.0);
        cleanup(lib, &[&vol, &src]);
    }

    #[test]
    fn load_edit_save_round_trip_leaves_no_backups() {
        let (mut lib, vol, src, id) = one_track("save");
        let loaded = lib.load_grid(id).unwrap();
        assert_eq!((loaded.sample_rate, loaded.total_frames, loaded.cue_frames), (44_100, 61 * 44_100, None));
        assert_eq!(loaded.title, "Synthetic Track");
        let doc = loaded.doc.clone().expect("grid readable");
        assert_eq!(doc.beats.len(), 32);

        let edited = apply(&doc, GridEdit::Nudge { frames: 441 }, EditCtx { sample_rate: 44_100, total_frames: Some(loaded.total_frames) }).unwrap();
        lib.save_grid(id, &edited).unwrap();
        assert_eq!(lib.load_grid(id).unwrap().doc.unwrap(), edited);
        assert!(lib.compatibility_problems().unwrap().is_empty());

        // the saved file is exactly what strfile::encode writes (byte-compatible)
        let (audio, sidecar) = lib.track_paths(&loaded.location).unwrap();
        let on_disk = std::fs::read_to_string(&sidecar).unwrap();
        assert_eq!(on_disk, strfile::encode(&Analysis::from(&edited), strfile::Newline::Crlf));
        // only the audio and its .str live in that folder
        let mut names: Vec<String> = std::fs::read_dir(audio.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.iter().all(|n| !n.ends_with(".tmp") && !n.ends_with(".bak")), "{names:?}");

        let bpm: f64 = lib.con.query_row("SELECT bpm FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(bpm, 128.0);
        assert!(lib.save_grid(id, &GridDoc { beats: vec![], ..edited }).is_err(), "invalid grids are refused");
        cleanup(lib, &[&vol, &src]);
    }

    #[test]
    fn loaded_length_leaves_a_second_of_slack_for_the_truncated_duration() {
        let (lib, vol, src, id) = one_track("slack");
        let l = lib.load_grid(id).unwrap();
        // a beat 0.6 s past the stored 60 s is real audio, not past the end
        let mut doc = l.doc.clone().unwrap();
        let last = 60 * 44_100 + 26_000;
        doc.beats.push(GridBeat { pos: last, bpm: 128.0 });
        let n = apply(&doc, GridEdit::Nudge { frames: 100 }, EditCtx { sample_rate: 44_100, total_frames: Some(l.total_frames) }).unwrap();
        assert_eq!(n.beats.last().unwrap().pos, last + 100);
        cleanup(lib, &[&vol, &src]);
    }

    #[test]
    fn unreadable_sidecars_load_as_a_problem_not_an_error() {
        let (lib, vol, src, id) = one_track("bad");
        let loc = lib.load_grid(id).unwrap().location;
        let (_, sidecar) = lib.track_paths(&loc).unwrap();
        std::fs::write(&sidecar, b"").unwrap();
        let l = lib.load_grid(id).unwrap();
        assert!(l.doc.is_none());
        assert_eq!(l.problem.as_deref(), Some("empty .str file (0 bytes)"));
        std::fs::remove_file(&sidecar).unwrap();
        assert_eq!(lib.load_grid(id).unwrap().problem.as_deref(), Some("no .str file"));
        cleanup(lib, &[&vol, &src]);
    }
}
