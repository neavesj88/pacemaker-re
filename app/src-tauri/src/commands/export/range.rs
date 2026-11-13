//! Which part of a mix an audio export covers: the whole mix, a time range of the timeline, or
//! "from clip A to clip B".
//!
//! One function, [`resolve`], turns the request into timeline frames. The export, the loudness
//! analysis, the range preview and the dialog's "this is what will be exported" line all call it,
//! so what is shown is what is rendered. Clip timing is the engine's own
//! ([`pm_mixer::engine::audible_clips`]), the same source the track list uses.
//!
//! Nothing here touches Tauri or the device.

use pm_formats::mix::Mix;
use pm_mixer::RATE;
use pm_mixer::engine::{AudibleClip, audible_clips, export_span};
use serde::{Deserialize, Serialize};

use crate::commands::mixes::doc::{Clip, MixDoc};

/// What the user asked to export.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RangeSpec {
    /// Everything, starting at the first audible frame (the export before this feature).
    #[default]
    Whole,
    /// Timeline seconds, as the editor's ruler shows them. The start is kept exactly (silence
    /// before the first clip is exported if the range includes it); the end is cut to the end of
    /// the last clip.
    Time { from_sec: f64, to_sec: f64 },
    /// From the first audible frame of one clip to the end of another (see [`EndAt`]). The two
    /// are given by clip id in either order: the one that starts first begins the range.
    Clips { from_clip: String, to_clip: String },
}

/// Where a "clip A to clip B" range ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndAt {
    /// At the last audible frame of the last clip. Whatever is already playing under its tail (the
    /// next track fading in) is cut there.
    #[default]
    ClipEnd,
    /// Just before the next clip enters (the start of the crossfade out of the last clip), or at
    /// the clip's end if that comes first: the last clip's own audio, without the next track.
    NextEnters,
}

/// Policy choices of a partial export (the user's settings; every field has a default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RangePrefs {
    pub end_at: EndAt,
    /// Fade at a cut that falls inside audio, in milliseconds. 0 = a hard cut.
    pub edge_fade_ms: f64,
}

impl Default for RangePrefs {
    fn default() -> Self {
        RangePrefs { end_at: EndAt::ClipEnd, edge_fade_ms: 20.0 }
    }
}

/// A resolved partial range, in timeline frames.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub from: i64,
    pub to: i64,
    pub from_secs: f64,
    pub to_secs: f64,
    pub duration_secs: f64,
    /// Fade length at a cut, in frames (see `Options::edge_fade_frames`).
    #[serde(skip)]
    pub edge_fade_frames: i64,
    /// The clips heard somewhere in the range, in timeline order.
    pub clip_ids: Vec<String>,
}

/// What an export of `spec` covers, for the dialog (the same numbers the export uses).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeInfo {
    /// The request is the whole mix.
    pub whole: bool,
    pub from_secs: f64,
    pub to_secs: f64,
    pub duration_secs: f64,
    /// The clips heard in the range, in timeline order.
    pub clip_ids: Vec<String>,
}

/// [`resolve`] with the whole mix described too, so the dialog can show what will be exported
/// whichever mode is chosen.
pub fn describe(doc: &MixDoc, spec: &RangeSpec, prefs: &RangePrefs) -> Result<RangeInfo, String> {
    if let Some(r) = resolve(doc, spec, prefs)? {
        return Ok(RangeInfo { whole: false, from_secs: r.from_secs, to_secs: r.to_secs, duration_secs: r.duration_secs, clip_ids: r.clip_ids });
    }
    let mix = doc.to_mix()?;
    let (from, to) = export_span(&mix).ok_or("The mix has no audible clips.")?;
    let mut heard = audible_clips(&mix);
    heard.sort_by_key(|c| (c.start, c.end, c.index));
    Ok(RangeInfo { whole: true, from_secs: secs(from), to_secs: secs(to), duration_secs: secs(to - from), clip_ids: heard.iter().map(|a| doc.clips[a.index].id.clone()).collect() })
}

fn secs(frames: i64) -> f64 {
    frames as f64 / f64::from(RATE)
}

fn frames(sec: f64) -> i64 {
    (sec * f64::from(RATE)).round() as i64
}

fn clock(sec: f64) -> String {
    let s = sec.max(0.0);
    format!("{}:{:04.1}", (s / 60.0).floor() as u64, s % 60.0)
}

/// A clip's name for messages: the library title, the document's copy, or its clip name.
fn name_of(c: &Clip) -> String {
    let t = c.track.as_ref().map(|t| t.title.trim()).filter(|t| !t.is_empty()).or_else(|| Some(c.fuzzy_id.title.trim()).filter(|t| !t.is_empty()));
    t.map(str::to_string).unwrap_or_else(|| "a clip".to_string())
}

/// The frames to render for `spec`, or `None` for the whole mix (which keeps the export's
/// original behaviour: from the first audible frame to the end). Errors are written for the user.
pub fn resolve(doc: &MixDoc, spec: &RangeSpec, prefs: &RangePrefs) -> Result<Option<Resolved>, String> {
    if matches!(spec, RangeSpec::Whole) {
        return Ok(None);
    }
    if !prefs.edge_fade_ms.is_finite() || prefs.edge_fade_ms < 0.0 {
        return Err("The fade at the cuts must be 0 or more milliseconds.".into());
    }
    let mix = doc.to_mix()?;
    resolve_mix(doc, &mix, spec, prefs)
}

/// [`resolve`] when the caller already has the converted mix.
pub fn resolve_mix(doc: &MixDoc, mix: &Mix, spec: &RangeSpec, prefs: &RangePrefs) -> Result<Option<Resolved>, String> {
    let audible = audible_clips(mix);
    let (_, last) = export_span(mix).ok_or("The mix has no audible clips.")?;
    let (from, to) = match spec {
        RangeSpec::Whole => return Ok(None),
        RangeSpec::Time { from_sec, to_sec } => {
            if !from_sec.is_finite() || !to_sec.is_finite() {
                return Err("The range needs a start and an end time.".into());
            }
            let (from, to) = (frames(from_sec.max(0.0)), frames(*to_sec));
            if to <= from {
                return Err("The range is empty: it must end after it starts.".into());
            }
            if from >= last {
                return Err(format!("The range starts at {}, after the mix has ended ({}).", clock(secs(from)), clock(secs(last))));
            }
            (from, to.min(last))
        }
        RangeSpec::Clips { from_clip, to_clip } => {
            let pick = |id: &str| -> Result<AudibleClip, String> {
                let idx = doc.clips.iter().position(|c| c.id == id).ok_or("A clip of the range is not in the mix any more (it may have been deleted).")?;
                audible.iter().copied().find(|a| a.index == idx).ok_or_else(|| format!("{} is not heard in the export (it is bypassed or trimmed away), so it cannot start or end a range.", name_of(&doc.clips[idx])))
            };
            let (a, b) = (pick(from_clip)?, pick(to_clip)?);
            let (a, b) = if (a.start, a.end, a.index) <= (b.start, b.end, b.index) { (a, b) } else { (b, a) };
            // of the two, the one that starts later ends the range; if the earlier one outlasts it, the range ends with the earlier
            let last_end = if a.end > b.end { a.end } else { b.end };
            let end = match prefs.end_at {
                EndAt::ClipEnd => last_end,
                EndAt::NextEnters => {
                    let ender = if a.end > b.end { a } else { b };
                    let next = audible.iter().filter(|c| c.index != ender.index && c.start > ender.start && c.start > a.start).map(|c| c.start).min();
                    next.map_or(ender.end, |n| n.min(ender.end)).max(a.start + 1)
                }
            };
            (a.start, end)
        }
    };
    if to <= from {
        return Err("The range is empty.".into());
    }
    let mut heard: Vec<&AudibleClip> = audible.iter().filter(|c| c.end > from && c.start < to).collect();
    heard.sort_by_key(|c| (c.start, c.end, c.index));
    if heard.is_empty() {
        return Err(format!("No track is heard between {} and {}.", clock(secs(from)), clock(secs(to))));
    }
    let fade = frames(prefs.edge_fade_ms / 1000.0).clamp(0, 2 * i64::from(RATE));
    Ok(Some(Resolved {
        from,
        to,
        from_secs: secs(from),
        to_secs: secs(to),
        duration_secs: secs(to - from),
        edge_fade_frames: fade,
        clip_ids: heard.iter().map(|a| doc.clips[a.index].id.clone()).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::mix::{Position, Track};

    const SR: f64 = 44100.0;

    /// Clips as (start s, length s): ids are "c0", "c1", ...
    fn doc(clips: &[(f64, f64)]) -> MixDoc {
        let mut mix = Mix::new("Range Test");
        for (i, (start, len)) in clips.iter().enumerate() {
            let pos = Position { start: start * SR, length: len * SR, limit_size: true, session_event_id: format!("{:032}", i + 1), ..Position::default() };
            mix.tracks.push(Track::new(i as i64 + 1, "/pmdb_tracks/unused", 120.0, 1.0, pos));
        }
        let mut d = MixDoc::from_mix(1, &mix);
        for (i, c) in d.clips.iter_mut().enumerate() {
            c.id = format!("c{i}");
        }
        d
    }

    fn time(a: f64, b: f64) -> RangeSpec {
        RangeSpec::Time { from_sec: a, to_sec: b }
    }

    fn clips(a: &str, b: &str) -> RangeSpec {
        RangeSpec::Clips { from_clip: a.into(), to_clip: b.into() }
    }

    #[test]
    fn the_whole_mix_is_not_a_range() {
        assert_eq!(resolve(&doc(&[(0.0, 10.0)]), &RangeSpec::Whole, &RangePrefs::default()).unwrap(), None);
    }

    #[test]
    fn a_time_range_is_kept_exactly_and_cut_to_the_end_of_the_mix() {
        let d = doc(&[(0.0, 60.0)]);
        let r = resolve(&d, &time(10.0, 25.5), &RangePrefs::default()).unwrap().unwrap();
        assert_eq!((r.from, r.to), (441_000, (25.5 * SR) as i64));
        assert!((r.duration_secs - 15.5).abs() < 1e-9);
        assert_eq!(r.clip_ids, ["c0"]);
        assert_eq!(r.edge_fade_frames, 882, "20 ms by default");
        let r = resolve(&d, &time(50.0, 500.0), &RangePrefs::default()).unwrap().unwrap();
        assert_eq!(r.to, 60 * 44100, "the end is cut to the last clip");
        // a start before the first sound is honoured: the user asked for it
        let d2 = doc(&[(8.0, 20.0)]);
        assert_eq!(resolve(&d2, &time(2.0, 10.0), &RangePrefs::default()).unwrap().unwrap().from, 2 * 44100);
    }

    #[test]
    fn bad_time_ranges_are_refused_with_a_reason() {
        let d = doc(&[(0.0, 60.0)]);
        let p = RangePrefs::default();
        assert!(resolve(&d, &time(20.0, 10.0), &p).unwrap_err().contains("empty"));
        assert!(resolve(&d, &time(10.0, 10.0), &p).unwrap_err().contains("empty"));
        assert!(resolve(&d, &time(f64::NAN, 10.0), &p).is_err());
        assert!(resolve(&d, &time(0.0, f64::INFINITY), &p).is_err());
        assert!(resolve(&d, &time(70.0, 80.0), &p).unwrap_err().contains("after the mix has ended"));
        // the part of a mix between two clips is silent
        let gap = doc(&[(0.0, 10.0), (30.0, 10.0)]);
        assert!(resolve(&gap, &time(12.0, 20.0), &p).unwrap_err().contains("No track is heard"));
        assert!(resolve(&d, &time(0.0, 10.0), &RangePrefs { edge_fade_ms: -1.0, ..p }).is_err());
    }

    #[test]
    fn clip_a_to_clip_b_runs_from_a_start_to_b_end() {
        // three overlapping 30 s clips, each entering 20 s after the one before
        let d = doc(&[(0.0, 30.0), (20.0, 30.0), (40.0, 30.0)]);
        let p = RangePrefs::default();
        let r = resolve(&d, &clips("c1", "c2"), &p).unwrap().unwrap();
        assert_eq!((r.from_secs, r.to_secs), (20.0, 70.0));
        assert_eq!(r.clip_ids, ["c0", "c1", "c2"], "c0 ends at 30 s, after the range starts at 20: it is heard too");
        let r = resolve(&d, &clips("c0", "c1"), &p).unwrap().unwrap();
        assert_eq!((r.from_secs, r.to_secs), (0.0, 50.0));
        assert_eq!(r.clip_ids.len(), 3, "c2 enters at 40 s, inside the range");
        // either order gives the same range
        assert_eq!(resolve(&d, &clips("c1", "c0"), &p).unwrap(), resolve(&d, &clips("c0", "c1"), &p).unwrap());
        // one clip alone
        let r = resolve(&d, &clips("c1", "c1"), &p).unwrap().unwrap();
        assert_eq!((r.from_secs, r.to_secs), (20.0, 50.0));
    }

    #[test]
    fn ending_before_the_next_track_enters_is_a_choice() {
        let d = doc(&[(0.0, 30.0), (20.0, 30.0), (40.0, 30.0)]);
        let early = RangePrefs { end_at: EndAt::NextEnters, ..RangePrefs::default() };
        let r = resolve(&d, &clips("c0", "c1"), &early).unwrap().unwrap();
        assert_eq!((r.from_secs, r.to_secs), (0.0, 40.0), "c2 enters at 40 s");
        // the last clip has no successor: its own end
        let r = resolve(&d, &clips("c0", "c2"), &early).unwrap().unwrap();
        assert_eq!(r.to_secs, 70.0);
        // a successor that comes after the clip has ended does not extend the range
        let gap = doc(&[(0.0, 10.0), (30.0, 10.0)]);
        assert_eq!(resolve(&gap, &clips("c0", "c0"), &early).unwrap().unwrap().to_secs, 10.0);
    }

    #[test]
    fn clips_that_are_not_heard_or_not_there_cannot_make_a_range() {
        let mut d = doc(&[(0.0, 30.0), (20.0, 30.0)]);
        d.clips[1].clip_bypassed = true;
        let p = RangePrefs::default();
        assert!(resolve(&d, &clips("c0", "c1"), &p).unwrap_err().contains("not heard"));
        assert!(resolve(&d, &clips("c0", "zzz"), &p).unwrap_err().contains("not in the mix"));
    }

    #[test]
    fn the_fade_setting_is_in_milliseconds_and_zero_is_a_hard_cut() {
        let d = doc(&[(0.0, 60.0)]);
        let hard = RangePrefs { edge_fade_ms: 0.0, ..RangePrefs::default() };
        assert_eq!(resolve(&d, &time(1.0, 5.0), &hard).unwrap().unwrap().edge_fade_frames, 0);
        let long = RangePrefs { edge_fade_ms: 100.0, ..RangePrefs::default() };
        assert_eq!(resolve(&d, &time(1.0, 5.0), &long).unwrap().unwrap().edge_fade_frames, 4410);
    }

    #[test]
    fn describe_covers_the_whole_mix_too() {
        let d = doc(&[(5.0, 30.0), (20.0, 30.0)]);
        let all = describe(&d, &RangeSpec::Whole, &RangePrefs::default()).unwrap();
        assert!(all.whole);
        assert_eq!((all.from_secs, all.to_secs, all.duration_secs), (5.0, 50.0, 45.0));
        assert_eq!(all.clip_ids, ["c0", "c1"]);
        let part = describe(&d, &time(6.0, 10.0), &RangePrefs::default()).unwrap();
        assert!(!part.whole);
        assert_eq!((part.from_secs, part.to_secs, part.clip_ids.len()), (6.0, 10.0, 1));
        assert!(describe(&doc(&[]), &RangeSpec::Whole, &RangePrefs::default()).is_err());
    }

    #[test]
    fn the_wire_format_is_what_the_frontend_sends() {
        let r: RangeSpec = serde_json::from_str(r#"{"kind":"time","fromSec":1.5,"toSec":9}"#).unwrap();
        assert_eq!(r, time(1.5, 9.0));
        let r: RangeSpec = serde_json::from_str(r#"{"kind":"clips","fromClip":"a","toClip":"b"}"#).unwrap();
        assert_eq!(r, clips("a", "b"));
        assert_eq!(serde_json::from_str::<RangeSpec>(r#"{"kind":"whole"}"#).unwrap(), RangeSpec::Whole);
        let p: RangePrefs = serde_json::from_str(r#"{"endAt":"nextEnters"}"#).unwrap();
        assert_eq!((p.end_at, p.edge_fade_ms), (EndAt::NextEnters, 20.0), "missing fields take their defaults");
    }
}
