//! Track list of a mix with start times, in the text formats people post with a mix: a plain
//! list, CSV, YouTube chapters, a Mixcloud-style list and a CUE sheet for the rendered audio.
//!
//! **Timing is the renderer's.** Every start time comes from [`pm_mixer::engine::audible_clips`]
//! and [`pm_mixer::engine::export_span`], the functions the engine builds its clips from, so a
//! time in the list is a time in the exported file: frame 0 of the file is the first audible
//! frame of the mix (an export skips leading silence) and a clip that the engine would not play
//! (bypassed, nothing left after trimming, or audio not on the device) is not in the list. The
//! dialog's preview, the clipboard, a saved file and the `.cue` written beside a rendered mix all
//! come out of [`render_all`], one function, so what is previewed is what is written.
//!
//! Nothing here touches Tauri or the device; the commands are in `export.rs`.

use std::path::Path;

use pm_mixer::RATE;
use pm_mixer::engine::{audible_clips, export_span};
use serde::{Deserialize, Serialize};

use crate::commands::mixes::doc::{Clip, MixDoc};

const FRAMES: i64 = RATE as i64;
/// Audio frames in one CUE frame (a CUE sheet counts 75 frames to the second).
const CUE_FRAME: i64 = FRAMES / 75;
const _: () = assert!(FRAMES % 75 == 0, "a CUE frame must be a whole number of audio frames");
/// A CUE sheet numbers its tracks 01..99.
const CUE_MAX_TRACKS: usize = 99;
/// YouTube chapters: at least this many, each at least this many seconds long.
pub const YT_MIN_CHAPTERS: usize = 3;
pub const YT_MIN_SECS: u64 = 10;

// ---------------------------------------------------------------------------------------------
// options
// ---------------------------------------------------------------------------------------------

/// Which moment of a clip's entrance the list calls its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StampAt {
    /// The first audible frame of the clip: where its crossfade begins.
    #[default]
    Enters,
    /// The middle of the overlap with the clips already playing, i.e. roughly where the new track
    /// takes over. A clip that starts alone is stamped where it enters.
    CrossfadeMid,
}

/// How a time is cut to whole seconds in the text formats (a CUE sheet is exact to 1/75 s and
/// ignores this).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Rounding {
    /// Never later than the real start, so a chapter link never lands after the track began.
    #[default]
    Floor,
    Nearest,
}

/// What the YouTube list does with a track that starts less than 10 s after the previous chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CloseEntries {
    /// Join its name to the previous chapter ("A / B").
    #[default]
    Merge,
    Drop,
}

/// Every policy choice of the list. All of it is a user setting; these are the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TracklistOptions {
    pub stamp_at: StampAt,
    pub rounding: Rounding,
    pub close_entries: CloseEntries,
    /// List a clip whose audio is not on the device (it is silent in the export). Off: left out,
    /// with a warning naming it.
    pub include_unavailable: bool,
    /// Number the lines of the plain list.
    pub numbered: bool,
    /// Start the CSV file with a UTF-8 byte-order mark, which Excel needs to read non-ASCII names.
    pub csv_bom: bool,
}

impl Default for TracklistOptions {
    fn default() -> Self {
        TracklistOptions {
            stamp_at: StampAt::Enters,
            rounding: Rounding::Floor,
            close_entries: CloseEntries::Merge,
            include_unavailable: false,
            numbered: true,
            csv_bom: true,
        }
    }
}

/// The text formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Txt,
    Csv,
    Youtube,
    Mixcloud,
    Cue,
}

impl Format {
    pub fn parse(s: &str) -> Result<Format, String> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "txt" => Format::Txt,
            "csv" => Format::Csv,
            "youtube" => Format::Youtube,
            "mixcloud" => Format::Mixcloud,
            "cue" => Format::Cue,
            other => return Err(format!("Unknown track list format '{other}' (use txt, csv, youtube, mixcloud or cue).")),
        })
    }

    /// The extension a saved file of this format ends in.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Cue => "cue",
            _ => "txt",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the list
// ---------------------------------------------------------------------------------------------

/// One track of the list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// 1-based position in the list.
    pub number: usize,
    pub clip_id: String,
    pub track_id: i64,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub key: String,
    pub bpm: f64,
    /// The clip's audio is on the device; false only when `includeUnavailable` listed it anyway.
    pub available: bool,
    /// Start in frames (44.1 kHz) from the start of the exported audio file.
    pub start_frames: i64,
    pub start_secs: f64,
    /// The start as the plain list prints it (rounded as the options say).
    pub start_label: String,
    /// The clip begins while an earlier listed clip is still playing (a crossfade).
    pub overlaps_previous: bool,
}

/// The entries of a mix plus what was left out.
#[derive(Debug, Clone, PartialEq)]
pub struct Tracklist {
    pub entries: Vec<Entry>,
    /// Length of the exported audio file in frames.
    pub total_frames: i64,
    pub warnings: Vec<String>,
}

fn collapse(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The name, artist, album and so on of a clip: from the library when the editor has it, else
/// from the document's own copy (`fuzzyId`), else the clip's name.
fn identity(c: &Clip) -> (String, String, String, String, f64) {
    if let Some(t) = &c.track
        && !collapse(&t.title).is_empty()
    {
        return (collapse(&t.title), collapse(&t.artist), collapse(&t.album), collapse(&t.key), t.bpm);
    }
    let f = &c.fuzzy_id;
    if !collapse(&f.title).is_empty() {
        return (collapse(&f.title), collapse(&f.artist), collapse(&f.album), String::new(), c.bpm);
    }
    let name = collapse(&c.clip_name);
    let title = if name.is_empty() || name == "AudioClip" { "Unknown track".to_string() } else { name };
    (title, collapse(&f.artist), String::new(), String::new(), c.bpm)
}

fn whole_secs(frames: i64, r: Rounding) -> u64 {
    let f = frames.max(0);
    (match r {
        Rounding::Floor => f / FRAMES,
        Rounding::Nearest => (f + FRAMES / 2) / FRAMES,
    }) as u64
}

/// `m:ss`, or `h:mm:ss` with `hours`; `pad` zero-pads the first field (`00:00`).
fn clock(secs: u64, hours: bool, pad: bool) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    match (hours, pad) {
        (true, true) => format!("{h:02}:{m:02}:{s:02}"),
        (true, false) => format!("{h}:{m:02}:{s:02}"),
        (false, true) => format!("{:02}:{s:02}", secs / 60),
        (false, false) => format!("{}:{s:02}", secs / 60),
    }
}

/// Builds the list for the clips of `doc` that an export of it would play. `has_audio` says
/// whether a library track id has audio on the device (the engine leaves a clip without out).
#[cfg(test)]
pub fn build(doc: &MixDoc, opts: &TracklistOptions, has_audio: &dyn Fn(i64) -> bool) -> Result<Tracklist, String> {
    build_in(doc, opts, has_audio, None)
}

/// [`build`] for an export of only the timeline frames `span` (`None` = the whole export). Times
/// are relative to the start of that part; a clip already playing at the start is listed at 0:00 and
/// a clip that is not heard inside the span is not listed.
pub fn build_in(doc: &MixDoc, opts: &TracklistOptions, has_audio: &dyn Fn(i64) -> bool, span: Option<(i64, i64)>) -> Result<Tracklist, String> {
    let mix = doc.to_mix()?;
    let whole = export_span(&mix).ok_or("the mix has no audible clips")?;
    let (from, to) = span.map_or(whole, |(f, t)| (f, t.min(whole.1)));
    let mut all = audible_clips(&mix);
    let mut warnings = Vec::new();
    if span.is_some() {
        let before = all.len();
        all.retain(|a| a.end > from && a.start < to);
        if before > all.len() {
            let out = before - all.len();
            warnings.push(format!("{out} clip{} outside the exported range, so not in the list", if out == 1 { " is" } else { "s are" }));
        }
    }
    let bypassed = mix.tracks.iter().filter(|t| t.clip_bypassed).count();
    if bypassed > 0 {
        warnings.push(format!("{bypassed} bypassed clip{} not in the audio, so not in the list", if bypassed == 1 { " is" } else { "s are" }));
    }

    // audible clips in play order; the engine's order is document order
    let mut order: Vec<_> = all.iter().collect();
    order.sort_by_key(|a| (a.start, a.end, a.index));
    let mut kept = Vec::new();
    for a in order {
        let clip = &doc.clips[a.index];
        let available = has_audio(clip.track_id);
        if !available && !opts.include_unavailable {
            let (title, artist, ..) = identity(clip);
            let what = if artist.is_empty() { title } else { format!("{artist} - {title}") };
            warnings.push(format!("{what} is left out of the list: its audio is not on the device, so the export leaves it out too"));
            continue;
        }
        kept.push((a, clip, available));
    }

    // the stamp of each entry, never earlier than the one before it
    let mut raw = Vec::with_capacity(kept.len());
    let mut latest_end = i64::MIN;
    let mut overlaps = Vec::with_capacity(kept.len());
    for (a, _, _) in &kept {
        let overlap = latest_end > a.start;
        let stamp = match opts.stamp_at {
            StampAt::CrossfadeMid if overlap => a.start + (latest_end.min(a.end) - a.start) / 2,
            _ => a.start,
        };
        raw.push(stamp);
        overlaps.push(overlap);
        latest_end = latest_end.max(a.end);
    }
    let mut floor = 0i64;
    let stamps: Vec<i64> = raw
        .into_iter()
        .map(|s| {
            floor = floor.max(s - from);
            floor
        })
        .collect();

    let total_frames = to - from;
    let hours = whole_secs(total_frames.max(stamps.last().copied().unwrap_or(0)), opts.rounding) >= 3600;
    let entries = kept
        .iter()
        .enumerate()
        .map(|(i, (_, clip, available))| {
            let (title, artist, album, key, bpm) = identity(clip);
            Entry {
                number: i + 1,
                clip_id: clip.id.clone(),
                track_id: clip.track_id,
                artist,
                title,
                album,
                key,
                bpm,
                available: *available,
                start_frames: stamps[i],
                start_secs: stamps[i] as f64 / f64::from(RATE),
                start_label: clock(whole_secs(stamps[i], opts.rounding), hours, true),
                overlaps_previous: overlaps[i],
            }
        })
        .collect();
    Ok(Tracklist { entries, total_frames, warnings })
}

fn name_of(e: &Entry) -> String {
    if e.artist.is_empty() { e.title.clone() } else { format!("{} - {}", e.artist, e.title) }
}

// ---------------------------------------------------------------------------------------------
// formats
// ---------------------------------------------------------------------------------------------

fn uses_hours(tl: &Tracklist, opts: &TracklistOptions) -> bool {
    let last = tl.entries.last().map_or(0, |e| e.start_frames);
    whole_secs(tl.total_frames.max(last), opts.rounding) >= 3600
}

/// `01. 00:00 Artist - Title`, one line per track (numbers optional).
pub fn plain_text(tl: &Tracklist, opts: &TracklistOptions) -> String {
    let hours = uses_hours(tl, opts);
    let width = tl.entries.len().to_string().len().max(2);
    tl.entries
        .iter()
        .map(|e| {
            let t = clock(whole_secs(e.start_frames, opts.rounding), hours, true);
            if opts.numbered { format!("{:0width$}. {t} {}\n", e.number, name_of(e)) } else { format!("{t} {}\n", name_of(e)) }
        })
        .collect()
}

/// `1. Artist - Title (00:00)`: the shape Mixcloud's tracklist is usually typed in. Mixcloud has
/// its own per-track form (artist, title, start time); this is a list to copy from.
pub fn mixcloud_text(tl: &Tracklist, opts: &TracklistOptions) -> String {
    let hours = uses_hours(tl, opts);
    tl.entries.iter().map(|e| format!("{}. {} ({})\n", e.number, name_of(e), clock(whole_secs(e.start_frames, opts.rounding), hours, true))).collect()
}

/// A CSV cell: quoted when it needs to be, and a leading `= + - @` (or a tab / return) gets an
/// apostrophe so a spreadsheet shows the text instead of running it as a formula.
fn csv_cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
}

pub fn csv_text(tl: &Tracklist, opts: &TracklistOptions) -> String {
    let hours = uses_hours(tl, opts);
    let mut out = String::from("Number,Start,Seconds,Artist,Title,Album,Key,BPM\r\n");
    for e in &tl.entries {
        let bpm = if e.bpm > 0.0 { format!("{:.1}", e.bpm) } else { String::new() };
        out.push_str(&format!(
            "{},{},{:.3},{},{},{},{},{}\r\n",
            e.number,
            clock(whole_secs(e.start_frames, opts.rounding), hours, true),
            e.start_secs,
            csv_cell(&e.artist),
            csv_cell(&e.title),
            csv_cell(&e.album),
            csv_cell(&e.key),
            bpm
        ));
    }
    out
}

/// The YouTube chapter text and whether YouTube will take it as chapters.
#[derive(Debug, Clone, PartialEq)]
pub struct Youtube {
    pub text: String,
    pub valid: bool,
    /// What was changed to meet the rules, and what could not be met.
    pub issues: Vec<String>,
}

/// YouTube makes chapters of the timestamps in a description when the first is 0:00, there are at
/// least three, and each chapter is at least ten seconds long (the last one too, up to the end of
/// the video). A track list that breaks the rules is changed to meet them as the options say; a
/// mix with fewer than three tracks cannot, and is reported as not valid. Angle brackets are
/// removed from names, which a YouTube description does not allow.
pub fn youtube(tl: &Tracklist, opts: &TracklistOptions) -> Youtube {
    struct Chapter {
        secs: u64,
        name: String,
        /// Number of the track the chapter starts at.
        number: usize,
    }
    let mut issues = Vec::new();
    let mut chapters: Vec<Chapter> = Vec::new();
    let clean = |s: String| collapse(&s.replace(['<', '>'], ""));
    // joins or drops `e` as the policy says; `chapters` is never empty here
    let fold = |chapters: &mut Vec<Chapter>, issues: &mut Vec<String>, number: usize, name: String, why: &str| {
        let last = chapters.last_mut().expect("a first chapter exists");
        match opts.close_entries {
            CloseEntries::Merge => {
                if !last.name.split(" / ").any(|n| n == name) {
                    last.name = format!("{} / {}", last.name, name);
                }
                issues.push(format!("Track {number} {why}; its name was added to the chapter before it."));
            }
            CloseEntries::Drop => issues.push(format!("Track {number} {why}; it is not a chapter.")),
        }
    };
    for (i, e) in tl.entries.iter().enumerate() {
        let name = clean(name_of(e));
        let real = whole_secs(e.start_frames, opts.rounding);
        if i == 0 {
            if real > 0 {
                issues.push(format!("The first track enters at {}, but a YouTube chapter list has to begin at 0:00, so it starts there.", clock(real, real >= 3600, false)));
            }
            chapters.push(Chapter { secs: 0, name, number: e.number });
        } else if real < chapters.last().map_or(0, |c| c.secs) + YT_MIN_SECS {
            fold(&mut chapters, &mut issues, e.number, name, "starts less than 10 seconds after the chapter before it");
        } else {
            chapters.push(Chapter { secs: real, name, number: e.number });
        }
    }
    // the last chapter has to run to the end of the video, at least ten seconds
    let end = (tl.total_frames.max(0) / FRAMES) as u64;
    if chapters.len() > 1 && end < chapters.last().map_or(0, |c| c.secs) + YT_MIN_SECS {
        let last = chapters.pop().expect("more than one chapter");
        fold(&mut chapters, &mut issues, last.number, last.name, "would be a chapter shorter than 10 seconds at the end of the video");
    }
    let valid = chapters.len() >= YT_MIN_CHAPTERS;
    if !valid {
        issues.push(format!(
            "YouTube needs at least {YT_MIN_CHAPTERS} chapters to show them, and this list gives {}. Add tracks (each at least {YT_MIN_SECS} seconds long) or post the list as plain text.",
            chapters.len()
        ));
    }
    let text = chapters.iter().map(|c| format!("{} {}\n", clock(c.secs, c.secs >= 3600, false), c.name)).collect();
    Youtube { text, valid, issues }
}

fn cue_str(s: &str) -> String {
    collapse(s).replace('"', "'")
}

/// A CUE sheet for the rendered file `audio_file_name` (a file name, no folder). Times are exact
/// to 1/75 s. Track 1 gets a pre-gap index when it does not start at 0:00 (a clip before it was
/// left out). `WAVE` is the file type for both WAV and Ogg files: the CUE format has no Ogg
/// type, and players go by the file extension.
pub fn cue(doc: &MixDoc, tl: &Tracklist, audio_file_name: &str) -> Result<String, String> {
    if tl.entries.is_empty() {
        return Err("there is no track to put in a CUE sheet".into());
    }
    if tl.entries.len() > CUE_MAX_TRACKS {
        return Err(format!("a CUE sheet holds at most {CUE_MAX_TRACKS} tracks and this mix has {}", tl.entries.len()));
    }
    let stamp = |frames: i64| {
        let f = frames.max(0) / CUE_FRAME;
        format!("{:02}:{:02}:{:02}", f / (75 * 60), f / 75 % 60, f % 75)
    };
    let file = file_name_only(audio_file_name);
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push_str("\r\n");
    };
    line("REM GENERATOR \"Pacemaker Revival\"".into());
    if !cue_str(&doc.artist).is_empty() {
        line(format!("PERFORMER \"{}\"", cue_str(&doc.artist)));
    }
    line(format!("TITLE \"{}\"", cue_str(&doc.name)));
    line(format!("FILE \"{}\" WAVE", cue_str(&file)));
    for e in &tl.entries {
        line(format!("  TRACK {:02} AUDIO", e.number));
        line(format!("    TITLE \"{}\"", cue_str(&e.title)));
        if !cue_str(&e.artist).is_empty() {
            line(format!("    PERFORMER \"{}\"", cue_str(&e.artist)));
        }
        if e.number == 1 && e.start_frames >= CUE_FRAME {
            line("    INDEX 00 00:00:00".into());
        }
        line(format!("    INDEX 01 {}", stamp(e.start_frames)));
    }
    Ok(out)
}

/// The last component of a path typed in either style.
pub fn file_name_only(s: &str) -> String {
    s.rsplit(['/', '\\']).next().unwrap_or(s).trim().to_string()
}

// ---------------------------------------------------------------------------------------------
// everything at once
// ---------------------------------------------------------------------------------------------

/// What the dialog shows and the commands write: every format of one list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TracklistOut {
    pub entries: Vec<Entry>,
    /// Length of the exported audio file.
    pub duration_secs: f64,
    pub txt: String,
    pub csv: String,
    pub youtube: String,
    pub youtube_valid: bool,
    pub youtube_issues: Vec<String>,
    pub mixcloud: String,
    pub cue: String,
    /// Why there is no usable CUE sheet (empty `cue`), if so.
    pub cue_issue: Option<String>,
    /// Clips left out of the list and why.
    pub warnings: Vec<String>,
}

impl TracklistOut {
    pub fn text(&self, f: Format) -> &str {
        match f {
            Format::Txt => &self.txt,
            Format::Csv => &self.csv,
            Format::Youtube => &self.youtube,
            Format::Mixcloud => &self.mixcloud,
            Format::Cue => &self.cue,
        }
    }

    /// The bytes of a saved file: the text of the format plus a byte-order mark where the format
    /// needs one (CSV for Excel, if the options say so; a CUE sheet only when it has non-ASCII
    /// characters, so plain ones stay plain).
    pub fn file_bytes(&self, f: Format, opts: &TracklistOptions) -> Vec<u8> {
        let text = self.text(f);
        let bom = match f {
            Format::Csv => opts.csv_bom,
            Format::Cue => !text.is_ascii(),
            _ => false,
        };
        let mut out = if bom { vec![0xEF, 0xBB, 0xBF] } else { Vec::new() };
        out.extend_from_slice(text.as_bytes());
        out
    }
}

/// Builds every format for `doc`. `audio_file_name` goes into the CUE sheet.
pub fn render_all(doc: &MixDoc, opts: &TracklistOptions, audio_file_name: &str, has_audio: &dyn Fn(i64) -> bool) -> Result<TracklistOut, String> {
    render_all_in(doc, opts, audio_file_name, has_audio, None)
}

/// [`render_all`] for an export of only the timeline frames `span` (see [`build_in`]).
pub fn render_all_in(doc: &MixDoc, opts: &TracklistOptions, audio_file_name: &str, has_audio: &dyn Fn(i64) -> bool, span: Option<(i64, i64)>) -> Result<TracklistOut, String> {
    let tl = build_in(doc, opts, has_audio, span)?;
    let yt = youtube(&tl, opts);
    let (cue, cue_issue) = match cue(doc, &tl, audio_file_name) {
        Ok(c) => (c, None),
        Err(e) => (String::new(), Some(e)),
    };
    Ok(TracklistOut {
        duration_secs: tl.total_frames as f64 / f64::from(RATE),
        txt: plain_text(&tl, opts),
        csv: csv_text(&tl, opts),
        youtube: yt.text,
        youtube_valid: yt.valid,
        youtube_issues: yt.issues,
        mixcloud: mixcloud_text(&tl, opts),
        cue,
        cue_issue,
        warnings: tl.warnings,
        entries: tl.entries,
    })
}

/// Writes `bytes` to `dest` through a temporary file in the same folder, so an interrupted write
/// never leaves a truncated file under the real name.
pub fn write_atomic(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dest.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::mixes::doc::TrackInfo;
    use pm_formats::mix::{Mix, Position, Track};

    const SR: f64 = 44100.0;

    /// (library id, start s, audible length s, title, artist)
    type Spec<'a> = (i64, f64, f64, &'a str, &'a str);

    fn doc(specs: &[Spec]) -> MixDoc {
        let mut mix = Mix::new("Fixture Mix");
        mix.artist = "Fixture DJ".into();
        for (i, (id, start, len, _, _)) in specs.iter().enumerate() {
            let pos = Position { start: start * SR, length: len * SR, limit_size: true, session_event_id: format!("{:032x}", i + 1), ..Position::default() };
            mix.tracks.push(Track::new(*id, "/pmdb_tracks/unused", 120.0, 1.0, pos));
        }
        let mut d = MixDoc::from_mix(5, &mix);
        for (c, (id, _, _, title, artist)) in d.clips.iter_mut().zip(specs) {
            c.track = Some(TrackInfo {
                track_id: *id,
                available: true,
                problem: None,
                title: (*title).into(),
                artist: (*artist).into(),
                album: "Fixture Album".into(),
                genre: String::new(),
                key: "8A".into(),
                bpm: 124.0,
                duration_secs: len_of(specs, *id),
                source_rate: 44100,
                location: String::new(),
                grid: None,
                grid_problem: None,
            });
        }
        d
    }

    fn len_of(specs: &[Spec], id: i64) -> f64 {
        specs.iter().find(|s| s.0 == id).map_or(0.0, |s| s.2)
    }

    fn all(_: i64) -> bool {
        true
    }

    fn opts() -> TracklistOptions {
        TracklistOptions::default()
    }

    /// Four tracks with crossfades of 8 s, 4 s and 20 s.
    fn four() -> MixDoc {
        doc(&[(1, 5.0, 120.0, "Title One", "Artist A"), (2, 117.0, 130.0, "Title Two", "Artist B"), (3, 243.0, 100.0, "Title Three", ""), (4, 323.0, 60.0, "Title Four", "Artist D")])
    }

    #[test]
    fn times_are_relative_to_the_first_audible_frame_and_sorted() {
        let mut d = four();
        d.clips.reverse(); // document order is not play order
        let tl = build(&d, &opts(), &all).unwrap();
        let secs: Vec<f64> = tl.entries.iter().map(|e| e.start_secs).collect();
        assert_eq!(secs, vec![0.0, 112.0, 238.0, 318.0]);
        assert_eq!(tl.entries.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Title One", "Title Two", "Title Three", "Title Four"]);
        assert_eq!(tl.entries.iter().map(|e| e.number).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(tl.total_frames, (383.0 - 5.0) as i64 * 44100);
        assert!(tl.entries[1].overlaps_previous && !tl.entries[0].overlaps_previous);
    }

    #[test]
    fn times_equal_the_engines_for_trims_and_fractional_positions() {
        let mut d = doc(&[(1, 2.5, 100.0, "Title One", "Artist A"), (2, 90.3, 100.0, "Title Two", "Artist B")]);
        d.clips[0].span.trim_start = 3.2 * SR; // cue: audible from 5.7 s
        d.clips[1].span.trim_end = 10.0 * SR;
        d.refresh();
        let mix = d.to_mix().unwrap();
        let (from, to) = export_span(&mix).unwrap();
        let tl = build(&d, &opts(), &all).unwrap();
        let a = audible_clips(&mix);
        assert_eq!(tl.entries[0].start_frames, 0);
        assert_eq!(tl.entries[1].start_frames, a[1].start - from, "same frames as the renderer's clip start");
        assert_eq!(tl.total_frames, to - from);
        assert_eq!(from, (5.7 * SR).round() as i64);
    }

    #[test]
    fn bypassed_trimmed_away_and_missing_clips_are_not_listed_but_origin_matches_the_engine() {
        let mut d = four();
        d.clips[0].clip_bypassed = true; // the earliest clip is not in the audio
        d.clips[3].span.trim_start = d.clips[3].span.length; // nothing left
        let av = |id: i64| id != 3;
        let tl = build(&d, &opts(), &av).unwrap();
        // the bypassed clip does not decide where the file starts, so clip 2 (117 s) is at 0; clip
        // 3 has no audio and is left out of the list
        assert_eq!(tl.entries.len(), 1, "{:?}", tl.entries);
        assert_eq!(tl.entries[0].title, "Title Two");
        assert_eq!(tl.entries[0].start_frames, 0);
        assert!(tl.warnings.iter().any(|w| w.contains("bypassed")));
        assert!(tl.warnings.iter().any(|w| w.contains("Title Three") && w.contains("left out")));
        // listing it anyway when the user says so
        let o = TracklistOptions { include_unavailable: true, ..opts() };
        let tl = build(&d, &o, &av).unwrap();
        assert_eq!(tl.entries.len(), 2);
        assert!(!tl.entries[1].available && tl.entries[0].available);
    }

    #[test]
    fn crossfade_middle_stamps_never_run_backwards() {
        let d = four();
        let tl = build(&d, &TracklistOptions { stamp_at: StampAt::CrossfadeMid, ..opts() }, &all).unwrap();
        // overlaps: clip 2 enters at 112 s while clip 1 plays to 120 s -> mid 116; clip 3 at 238
        // while clip 2 plays to 242 -> mid 240; clip 4 at 318 while clip 3 plays to 338 -> mid 328
        let secs: Vec<f64> = tl.entries.iter().map(|e| e.start_secs).collect();
        assert_eq!(secs, vec![0.0, 116.0, 240.0, 328.0]);
        // a clip nested inside a long one is stamped at its middle, then the next one is held at
        // least that late
        let d = doc(&[(1, 0.0, 200.0, "Title One", "A"), (2, 100.0, 50.0, "Title Two", "A"), (3, 110.0, 30.0, "Title Three", "A")]);
        let tl = build(&d, &TracklistOptions { stamp_at: StampAt::CrossfadeMid, ..opts() }, &all).unwrap();
        let s: Vec<f64> = tl.entries.iter().map(|e| e.start_secs).collect();
        assert_eq!(s, vec![0.0, 125.0, 125.0], "{s:?}");
    }

    #[test]
    fn plain_text_numbering_and_rounding() {
        let mut d = four();
        d.clips[1].span.start += 0.9 * SR; // 112.9 s into the file
        d.refresh();
        let t = plain_text(&build(&d, &opts(), &all).unwrap(), &opts());
        assert_eq!(t, "01. 00:00 Artist A - Title One\n02. 01:52 Artist B - Title Two\n03. 03:58 Title Three\n04. 05:18 Artist D - Title Four\n");
        let near = TracklistOptions { rounding: Rounding::Nearest, numbered: false, ..opts() };
        let t = plain_text(&build(&d, &near, &all).unwrap(), &near);
        assert!(t.contains("01:53 Artist B - Title Two") && !t.contains("01."), "{t}");
        // an hour or more switches every line to h:mm:ss
        let long = doc(&[(1, 0.0, 3000.0, "Title One", "A"), (2, 2990.0, 900.0, "Title Two", "A")]);
        let t = plain_text(&build(&long, &opts(), &all).unwrap(), &opts());
        assert_eq!(t, "01. 00:00:00 A - Title One\n02. 00:49:50 A - Title Two\n");
    }

    #[test]
    fn names_fall_back_and_are_cleaned() {
        let mut d = doc(&[(1, 0.0, 60.0, "  Title\n One ", " Artist\tA "), (2, 50.0, 60.0, "", ""), (3, 100.0, 60.0, "", "")]);
        d.clips[1].track = None;
        d.clips[1].fuzzy_id.title = "Doc Title".into();
        d.clips[1].fuzzy_id.artist = "Doc Artist".into();
        d.clips[2].track = None;
        let tl = build(&d, &opts(), &all).unwrap();
        assert_eq!(name_of(&tl.entries[0]), "Artist A - Title One");
        assert_eq!(name_of(&tl.entries[1]), "Doc Artist - Doc Title");
        assert_eq!(name_of(&tl.entries[2]), "Unknown track");
    }

    #[test]
    fn csv_quotes_and_neutralises_formulas() {
        let mut d = doc(&[(1, 0.0, 60.0, "Title, \"One\"", "=Artist"), (2, 50.0, 60.0, "-Dash", "A")]);
        d.clips[1].track.as_mut().unwrap().bpm = 0.0;
        let tl = build(&d, &opts(), &all).unwrap();
        let c = csv_text(&tl, &opts());
        let lines: Vec<&str> = c.split("\r\n").collect();
        assert_eq!(lines[0], "Number,Start,Seconds,Artist,Title,Album,Key,BPM");
        assert_eq!(lines[1], "1,00:00,0.000,'=Artist,\"Title, \"\"One\"\"\",Fixture Album,8A,124.0");
        assert_eq!(lines[2], "2,00:50,50.000,A,'-Dash,Fixture Album,8A,");
        assert_eq!(*lines.last().unwrap(), "");
    }

    #[test]
    fn youtube_meets_its_rules() {
        let tl = build(&four(), &opts(), &all).unwrap();
        let y = youtube(&tl, &opts());
        assert!(y.valid, "{:?}", y.issues);
        assert_eq!(y.text, "0:00 Artist A - Title One\n1:52 Artist B - Title Two\n3:58 Title Three\n5:18 Artist D - Title Four\n");
        assert!(y.issues.is_empty());
    }

    #[test]
    fn youtube_merges_or_drops_chapters_closer_than_ten_seconds() {
        // starts 0, 6 (too close), 40, and a last one 5 s before the end of the video
        let d = doc(&[(1, 0.0, 30.0, "Title One", "A"), (2, 6.0, 40.0, "Title Two", "A"), (3, 40.0, 40.0, "Title Three", "A"), (4, 76.0, 9.0, "Title Four", "A")]);
        let tl = build(&d, &opts(), &all).unwrap();
        assert_eq!(tl.total_frames, 85 * 44100);
        let y = youtube(&tl, &opts());
        assert_eq!(y.text, "0:00 A - Title One / A - Title Two\n0:40 A - Title Three / A - Title Four\n");
        assert!(!y.valid, "two chapters are not enough");
        assert!(y.issues.iter().any(|i| i.contains("Track 2") && i.contains("less than 10 seconds")), "{:?}", y.issues);
        assert!(y.issues.iter().any(|i| i.contains("Track 4") && i.contains("end of the video")), "{:?}", y.issues);
        assert!(y.issues.iter().any(|i| i.contains("at least 3 chapters")));
        let drop = TracklistOptions { close_entries: CloseEntries::Drop, ..opts() };
        let y = youtube(&build(&d, &drop, &all).unwrap(), &drop);
        assert_eq!(y.text, "0:00 A - Title One\n0:40 A - Title Three\n");
        // every chapter that is kept is at least ten seconds from the next
        let d = doc(&[(1, 0.0, 30.0, "Title One", "A"), (2, 25.0, 40.0, "Title Two", "A"), (3, 60.0, 50.0, "Title Three", "A"), (4, 100.0, 40.0, "Title Four", "A")]);
        let y = youtube(&build(&d, &opts(), &all).unwrap(), &opts());
        assert!(y.valid, "{:?}", y.issues);
        let ts: Vec<u64> = y.text.lines().map(|l| l.split(' ').next().unwrap()).map(|t| t.split(':').fold(0, |a, p| a * 60 + p.parse::<u64>().unwrap())).collect();
        assert_eq!(ts[0], 0);
        assert!(ts.windows(2).all(|w| w[1] >= w[0] + 10) && 140 >= ts.last().unwrap() + 10, "{ts:?}");
    }

    #[test]
    fn youtube_needs_three_and_starts_at_zero_even_when_the_first_clip_is_missing() {
        let d = doc(&[(1, 0.0, 60.0, "Title One", "A"), (2, 50.0, 60.0, "Title Two", "A")]);
        let y = youtube(&build(&d, &opts(), &all).unwrap(), &opts());
        assert!(!y.valid);
        // the first clip has no audio: the file starts at the (silent) clip, so the first listed
        // track enters 50 s in, and the chapter list still has to begin at 0:00
        let d = doc(&[(1, 0.0, 60.0, "Title One", "A"), (2, 50.0, 60.0, "Title Two", "A"), (3, 100.0, 60.0, "Title Three", "A"), (4, 150.0, 60.0, "Title Four", "A")]);
        let y = youtube(&build(&d, &opts(), &|id| id != 1).unwrap(), &opts());
        assert!(y.valid, "{:?}", y.issues);
        assert!(y.text.starts_with("0:00 A - Title Two\n"), "{}", y.text);
        assert!(y.issues.iter().any(|i| i.contains("first track enters at 0:50")), "{:?}", y.issues);
        // angle brackets are not allowed in a description
        let d = doc(&[(1, 0.0, 60.0, "A <b>", "X"), (2, 50.0, 60.0, "Two", "X"), (3, 100.0, 60.0, "Three", "X")]);
        assert!(youtube(&build(&d, &opts(), &all).unwrap(), &opts()).text.starts_with("0:00 X - A b\n"));
    }

    #[test]
    fn youtube_uses_hours_per_line_after_an_hour() {
        let d = doc(&[(1, 0.0, 2000.0, "Title One", "A"), (2, 1990.0, 2000.0, "Title Two", "A"), (3, 3980.0, 700.0, "Title Three", "A")]);
        let y = youtube(&build(&d, &opts(), &all).unwrap(), &opts());
        assert_eq!(y.text, "0:00 A - Title One\n33:10 A - Title Two\n1:06:20 A - Title Three\n");
        assert!(y.valid);
    }

    #[test]
    fn cue_sheet_is_exact_to_a_frame() {
        let mut d = four();
        d.clips[1].span.start += 588.0 * 40.0 + 587.0; // 40 CUE frames and 587 audio frames: the rest is cut off, not rounded up
        d.refresh();
        let tl = build(&d, &opts(), &all).unwrap();
        let c = cue(&d, &tl, "C:\\out\\My \"Mix\".ogg").unwrap();
        let expect = [
            "REM GENERATOR \"Pacemaker Revival\"",
            "PERFORMER \"Fixture DJ\"",
            "TITLE \"Fixture Mix\"",
            "FILE \"My 'Mix'.ogg\" WAVE",
            "  TRACK 01 AUDIO",
            "    TITLE \"Title One\"",
            "    PERFORMER \"Artist A\"",
            "    INDEX 01 00:00:00",
            "  TRACK 02 AUDIO",
            "    TITLE \"Title Two\"",
            "    PERFORMER \"Artist B\"",
            "    INDEX 01 01:52:40",
            "  TRACK 03 AUDIO",
            "    TITLE \"Title Three\"",
            "    INDEX 01 03:58:00",
            "  TRACK 04 AUDIO",
            "    TITLE \"Title Four\"",
            "    PERFORMER \"Artist D\"",
            "    INDEX 01 05:18:00",
            "",
        ];
        assert_eq!(c, expect.join("\r\n"));
    }

    #[test]
    fn cue_pregap_limit_and_empty() {
        let d = doc(&[(1, 0.0, 60.0, "Title One", "A"), (2, 50.0, 60.0, "Title Two", "A")]);
        let tl = build(&d, &opts(), &|id| id != 1).unwrap();
        let c = cue(&d, &tl, "m.wav").unwrap();
        // the file starts with the silent clip, so the first listed track enters 50 s in and gets a pre-gap
        assert!(c.contains("    INDEX 00 00:00:00\r\n    INDEX 01 00:50:00\r\n"), "{c}");
        // many tracks
        let specs: Vec<Spec> = (0..100).map(|i| (1 + i as i64, i as f64 * 20.0, 30.0, "T", "")).collect();
        let d = doc(&specs);
        let tl = build(&d, &opts(), &all).unwrap();
        assert!(cue(&d, &tl, "m.wav").unwrap_err().contains("at most 99"));
        let out = render_all(&d, &opts(), "m.wav", &all).unwrap();
        assert!(out.cue.is_empty() && out.cue_issue.is_some());
        let first_99 = Tracklist { entries: tl.entries[..99].to_vec(), ..tl };
        assert!(cue(&d, &first_99, "m.wav").unwrap().contains("TRACK 99 AUDIO"));
    }

    #[test]
    fn long_cue_times_go_past_99_minutes_without_breaking() {
        let d = doc(&[(1, 0.0, 6200.0, "Title One", "A"), (2, 6100.0, 100.0, "Title Two", "A")]);
        let c = cue(&d, &build(&d, &opts(), &all).unwrap(), "m.wav").unwrap();
        assert!(c.contains("INDEX 01 101:40:00"), "{c}");
    }

    #[test]
    fn file_bytes_add_a_bom_only_where_asked() {
        let d = doc(&[(1, 0.0, 60.0, "Tïtle", "A"), (2, 50.0, 60.0, "Two", "A")]);
        let out = render_all(&d, &opts(), "m.wav", &all).unwrap();
        assert!(out.file_bytes(Format::Csv, &opts()).starts_with(&[0xEF, 0xBB, 0xBF]));
        assert!(!out.file_bytes(Format::Csv, &TracklistOptions { csv_bom: false, ..opts() }).starts_with(&[0xEF]));
        assert!(out.file_bytes(Format::Cue, &opts()).starts_with(&[0xEF, 0xBB, 0xBF]), "non-ASCII CUE gets a BOM");
        assert!(!out.file_bytes(Format::Txt, &opts()).starts_with(&[0xEF]));
        let plain = render_all(&doc(&[(1, 0.0, 60.0, "Title", "A")]), &opts(), "m.wav", &all).unwrap();
        assert!(plain.file_bytes(Format::Cue, &opts()).starts_with(b"REM"));
        assert_eq!(out.file_bytes(Format::Txt, &opts()), out.txt.as_bytes());
    }

    #[test]
    fn mixcloud_list_and_format_names() {
        let t = mixcloud_text(&build(&four(), &opts(), &all).unwrap(), &opts());
        assert_eq!(t.lines().nth(1), Some("2. Artist B - Title Two (01:52)"));
        assert_eq!(Format::parse("YouTube").unwrap(), Format::Youtube);
        assert!(Format::parse("m3u").is_err());
        assert_eq!((Format::Cue.extension(), Format::Csv.extension(), Format::Mixcloud.extension()), ("cue", "csv", "txt"));
        assert_eq!(file_name_only("C:\\a\\b\\mix.ogg"), "mix.ogg");
        assert_eq!(file_name_only("/a/b/mix.ogg"), "mix.ogg");
    }

    #[test]
    fn an_empty_mix_is_an_error_and_options_have_defaults_from_json() {
        let d = doc(&[]);
        assert!(build(&d, &opts(), &all).is_err());
        let o: TracklistOptions = serde_json::from_str(r#"{"stampAt":"crossfadeMid","rounding":"nearest"}"#).unwrap();
        assert_eq!(o, TracklistOptions { stamp_at: StampAt::CrossfadeMid, rounding: Rounding::Nearest, ..TracklistOptions::default() });
        let o: TracklistOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(o, TracklistOptions::default());
    }

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp() {
        let d = std::env::temp_dir().join(format!("pmrev-tracklist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("list.txt");
        write_atomic(&f, b"one").unwrap();
        write_atomic(&f, b"two").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        assert!(write_atomic(&d.join("missing").join("x.txt"), b"x").is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
}
