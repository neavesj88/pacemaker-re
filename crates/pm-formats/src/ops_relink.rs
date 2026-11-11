//! Relinking tracks whose audio file is missing (owner: device area).
//!
//! The Health scan lists tracks whose audio is gone. This module finds the file again in a folder
//! the user names (on the PC or on the card), shows each match with the evidence for it, and then
//! either points `tracks.location` at a file that already lies under `.Pacemaker/Music`, or
//! copies a file from elsewhere into the right place first.
//!
//! The card names a file `A/B/C.<ext>`, where A and B come from the file size and C is a hash of
//! the original file name ([`naming::device_paths`]). The original name is not stored, but that
//! hash is, so a candidate whose name and size produce this track's own location is the file the
//! track was added with (kind [`Kind::Exact`]). Weaker evidence is the same name with another
//! size (re-tagged or re-encoded) and the same title, artist and length in the tags. The
//! matching is one function ([`classify`]) used by the preview and by the run, which repeats it
//! on the fresh database before it writes anything.
//!
//! Every card path is built from a `tracks.location` through [`checked_track_paths`] (plain
//! names under Music only). Nothing is deleted: a failed step removes only the files that very
//! step created.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::library::{self, Error, Library};
use crate::naming;
use crate::ops_device::{self, music_dir, sha256_file, Sha256, AUDIO_EXTS};
use crate::ops_fix::{checked_track_paths, membership, track_label};

/// Longest `tracks.location` the column holds (`VARCHAR(255)`).
const MAX_LOCATION: usize = 255;

/// Free space kept back on the card after a copy, in bytes.
const FREE_MARGIN: u64 = 8 << 20;

/// A measured length within this many seconds of the recorded one is rounding, not a different
/// length: the recorded value stays.
const LENGTH_NOISE_SECS: i64 = 1;

/// How many other files to offer per track besides the best one.
const MAX_ALTERNATIVES: usize = 4;

/// Where `Cancelled` errors start, so the UI shows them without "Failed".
pub const CANCELLED: &str = "Cancelled";

// ---- what is asked for ----------------------------------------------------------------------

/// What to do with a match that already lies under `.Pacemaker/Music`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CardFiles {
    /// Point the track at the file where it is (any plain path under Music).
    #[default]
    Reuse,
    /// Copy it to the name the app gives new files (`A/B/C.ext`) and point the track there.
    Standard,
}

/// The user's matching choices.
#[derive(Debug, Clone)]
pub struct MatchOpts {
    /// Also read title, artist and length from files that do not match by name.
    pub use_tags: bool,
    /// How far a file's length may differ from the track's, in seconds.
    pub duration_tolerance_secs: f64,
    /// Stop reading tags after this many files (the name matches still work).
    pub max_tag_reads: usize,
    pub card_files: CardFiles,
}

impl Default for MatchOpts {
    fn default() -> Self {
        MatchOpts { use_tags: true, duration_tolerance_secs: 2.0, max_tag_reads: 20_000, card_files: CardFiles::Reuse }
    }
}

/// How the folder is searched.
#[derive(Debug, Clone)]
pub struct ScanOpts {
    pub subfolders: bool,
    /// Stop listing after this many audio files.
    pub max_files: usize,
}

impl Default for ScanOpts {
    fn default() -> Self {
        ScanOpts { subfolders: true, max_files: 100_000 }
    }
}

// ---- the missing tracks ---------------------------------------------------------------------

/// A track whose audio file is not on the card.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Wanted {
    pub track_id: i64,
    pub label: String,
    pub location: String,
    pub file_size: u64,
    pub play_time_secs: i64,
    /// `tracks.format` as recorded (`MP3`, `WAV`, ...).
    pub format: String,
    pub title: String,
    pub artist: String,
    /// Mixes the track is in (they keep working: a mix refers to the track, not to the path).
    pub in_mixes: Vec<String>,
}

/// What the library lists as missing, and what is left out of that list.
#[derive(Debug, Default)]
pub struct Missing {
    pub wanted: Vec<Wanted>,
    /// Tracks whose location starts like a card path but is not a plain path under Music (`..`,
    /// a drive letter, ...). Health counts them; they are never followed here.
    pub skipped_locations: usize,
    /// The Music folder itself is gone, so every track with a plain location is missing. Copying a
    /// file back creates the folder again.
    pub music_missing: bool,
}

/// The tracks whose stored location is a plain path under Music and whose file is not there, with
/// the number of tracks that were left out because their location is not a plain path. The same
/// classification the Health card's fix uses ([`checked_track_paths`]).
pub fn missing_report(lib: &Library) -> Result<Missing, Error> {
    let root = lib.root();
    let mixes = membership(lib, "SELECT t.track_id, m.name FROM mixtracks t JOIN mixes m ON m.mix_id = t.mix_id WHERE m.mix_id >= 0 ORDER BY m.name")?;
    let mut out = Missing { music_missing: !music_dir(root).is_dir(), ..Missing::default() };
    for t in lib.tracks()? {
        let Some((audio, _)) = checked_track_paths(root, &t.location) else {
            if t.location.starts_with(naming::LOCATION_PREFIX) {
                out.skipped_locations += 1;
            }
            continue;
        };
        if audio.is_file() {
            continue;
        }
        out.wanted.push(Wanted {
            track_id: t.track_id,
            label: track_label(&t.artist, &t.title, &t.location),
            location: t.location.clone(),
            file_size: t.file_size.max(0) as u64,
            play_time_secs: t.play_time_secs,
            format: t.format.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            in_mixes: mixes.get(&t.track_id).cloned().unwrap_or_default(),
        });
    }
    Ok(out)
}

/// [`missing_report`] without the counts.
pub fn missing_tracks(lib: &Library) -> Result<Vec<Wanted>, Error> {
    Ok(missing_report(lib)?.wanted)
}

/// Lower-cased path under Music (`a/b/c.mp3`) -> the track that uses it, for every track.
fn used_files(lib: &Library) -> Result<HashMap<String, i64>, Error> {
    let mut used = HashMap::new();
    for t in lib.tracks()? {
        if let Some(rel) = t.location.strip_prefix(naming::LOCATION_PREFIX) {
            used.insert(rel.to_lowercase(), t.track_id);
        }
    }
    Ok(used)
}

// ---- finding files --------------------------------------------------------------------------

/// An audio file found in the folder.
#[derive(Debug, Clone)]
pub struct Found {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}

impl Found {
    /// Reads the name and size of one file.
    pub fn of(path: &Path) -> Result<Found, Error> {
        let md = std::fs::metadata(path)?;
        if !md.is_file() {
            return Err(Error::Invalid(format!("{} is not a file", path.display())));
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Invalid(format!("{} has no UTF-8 file name", path.display())))?
            .to_string();
        Ok(Found { path: path.to_path_buf(), name, size: md.len() })
    }
}

/// True for a name with an audio extension the app handles.
pub fn is_audio_name(name: &str) -> bool {
    let (_, ext) = split_name(name);
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    AUDIO_EXTS.contains(&ext.as_str())
}

#[derive(Debug, Default)]
pub struct Walked {
    pub files: Vec<Found>,
    /// Stopped at `max_files`: more audio files exist than were listed.
    pub truncated: bool,
    /// Folders that could not be read, with the reason.
    pub unreadable: Vec<String>,
}

fn skipped_folder(name: &str) -> bool {
    name.eq_ignore_ascii_case("$RECYCLE.BIN") || name.eq_ignore_ascii_case("System Volume Information")
}

/// Lists the audio files under `folder`. Symbolic links are not followed (a link back up the tree
/// would never end). `progress(files_so_far, current_folder)` is called as it goes; `cancel()` is
/// polled per folder.
pub fn walk_audio(folder: &Path, opts: &ScanOpts, progress: &mut dyn FnMut(usize, &str), cancel: &dyn Fn() -> bool) -> Result<Walked, Error> {
    if !folder.is_dir() {
        return Err(Error::Invalid(format!("{} is not a folder", folder.display())));
    }
    let mut out = Walked::default();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel() {
            return Err(Error::Invalid(format!("{CANCELLED}: the search was stopped")));
        }
        progress(out.files.len(), &dir.display().to_string());
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                out.unreadable.push(format!("{}: {e}", dir.display()));
                continue;
            }
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                if opts.subfolders && !skipped_folder(&name) {
                    stack.push(e.path());
                }
            } else if ft.is_file() && is_audio_name(&name) {
                if out.files.len() >= opts.max_files {
                    out.truncated = true;
                    continue;
                }
                match e.metadata() {
                    Ok(md) => out.files.push(Found { path: e.path(), name, size: md.len() }),
                    Err(err) => out.unreadable.push(format!("{}: {err}", e.path().display())),
                }
            }
        }
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

// ---- matching -------------------------------------------------------------------------------

/// What reading a file's tags gave.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Its name and size give this track's own location: the file the track was added with.
    Exact,
    /// The same file name with another size: re-tagged or re-encoded since.
    SameName,
    /// The same title and artist, and a length that fits.
    Tags,
    /// Chosen by the user with no evidence either way.
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Sure,
    Likely,
    Check,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Verdict {
    pub kind: Kind,
    pub confidence: Confidence,
    /// Orders candidates for one track; higher is better. Not shown.
    pub score: u32,
    /// The evidence in words, shown beside the match.
    pub notes: Vec<String>,
    pub duration_secs: Option<f64>,
    /// The file's size is not the size the library records.
    pub size_differs: bool,
}

/// `(stem, extension with its dot)`; the extension is empty when there is none. A leading dot is
/// part of the stem, as [`naming::device_paths`] splits it.
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn stem_hex(stem: &str) -> String {
    format!("{:x}", naming::oaat(stem))
}

/// `(stem, lower-case extension)` of a stored location's file name. For a location in the shape
/// this app generates the stem is the hash of the original file name; for any other (a track that
/// was pointed at a file under its own name) it is the name itself, see [`stem_matches`].
fn location_name(location: &str) -> Option<(String, String)> {
    let rel = location.strip_prefix(naming::LOCATION_PREFIX)?;
    let file = rel.rsplit('/').next()?;
    let (stem, ext) = split_name(file);
    Some((stem.to_string(), ext.trim_start_matches('.').to_ascii_lowercase()))
}

/// Is the stored location one in the shape this app generates (`h/h/hash.ext`)?
fn standard_location(location: &str) -> bool {
    location.strip_prefix(naming::LOCATION_PREFIX).is_some_and(is_standard_rel)
}

/// Does a candidate file name (`stem`) belong to a stored location whose file name has the stem
/// `lstem`? A standard location holds the hash of the original name; a track that an earlier
/// relink pointed at a file with another path holds that file's own name.
fn stem_matches(location: &str, lstem: &str, stem: &str) -> bool {
    if standard_location(location) {
        stem_hex(stem) == lstem
    } else {
        stem.eq_ignore_ascii_case(lstem)
    }
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn plural_tracks(n: usize) -> String {
    format!("{n} {}", if n == 1 { "track" } else { "tracks" })
}

fn secs_text(s: f64) -> String {
    let t = s.round().max(0.0) as i64;
    format!("{}:{:02}", t / 60, t % 60)
}

/// Does a measured length fit the track's? `None` when either is unknown.
fn length_fits(w: &Wanted, tags: Option<&Tags>, tol: f64) -> Option<bool> {
    let d = tags?.duration_secs?;
    (w.play_time_secs > 0).then(|| (d - w.play_time_secs as f64).abs() <= tol)
}

/// Decides whether `f` could be the missing audio of `w`, and how sure that is. `read_tags` is
/// called at most once and only when the name alone does not settle it. The preview and the run
/// both call this.
pub fn classify(w: &Wanted, f: &Found, read_tags: &mut dyn FnMut() -> Option<Tags>, o: &MatchOpts) -> Option<Verdict> {
    let size_differs = f.size != w.file_size;
    let (_, loc) = naming::device_paths(&f.name, f.size);
    if !size_differs && loc.eq_ignore_ascii_case(&w.location) {
        return Some(Verdict {
            kind: Kind::Exact,
            confidence: Confidence::Sure,
            score: 100,
            notes: vec!["Same file name and size as when the track was added.".into()],
            duration_secs: None,
            size_differs,
        });
    }
    let (stem, ext) = split_name(&f.name);
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    if let Some((lstem, lext)) = location_name(&w.location) {
        if stem_matches(&w.location, &lstem, stem) && ext == lext {
            if !size_differs {
                // the name belongs to this track's location and the size is the recorded one; this
                // is reached when the location is not what the name and size would generate now
                return Some(Verdict {
                    kind: Kind::Exact,
                    confidence: Confidence::Sure,
                    score: 100,
                    notes: vec!["Same file name and size as the library records.".into()],
                    duration_secs: None,
                    size_differs,
                });
            }
            let tags = if o.use_tags { read_tags() } else { None };
            let fit = length_fits(w, tags.as_ref(), o.duration_tolerance_secs);
            let mut notes = vec![format!(
                "Same file name, but {} bytes instead of the {} the library records: it was re-tagged or re-encoded since. The beat grid may not line up with it; re-analyse the track afterwards.",
                f.size, w.file_size
            )];
            let (confidence, score) = match fit {
                Some(true) => {
                    notes.push("Its length fits the track.".into());
                    (Confidence::Likely, 70)
                }
                Some(false) => {
                    let d = tags.as_ref().and_then(|t| t.duration_secs).unwrap_or(0.0);
                    notes.push(format!("Its length is {} but the track is {}: it may be a different version.", secs_text(d), secs_text(w.play_time_secs as f64)));
                    (Confidence::Check, 40)
                }
                None => {
                    notes.push("Its length could not be compared.".into());
                    (Confidence::Likely, 60)
                }
            };
            return Some(Verdict { kind: Kind::SameName, confidence, score, notes, duration_secs: tags.and_then(|t| t.duration_secs), size_differs });
        }
    }
    if !o.use_tags || w.title.trim().is_empty() || w.artist.trim().is_empty() {
        return None;
    }
    let t = read_tags()?;
    if norm(&t.title) != norm(&w.title) || norm(&t.artist) != norm(&w.artist) || t.title.trim().is_empty() || t.artist.trim().is_empty() {
        return None;
    }
    let fit = length_fits(w, Some(&t), o.duration_tolerance_secs);
    let mut notes = vec!["Same title and artist in its tags.".to_string()];
    let (confidence, score) = match fit {
        Some(true) if !size_differs => {
            notes.push("Same length and same size.".into());
            (Confidence::Sure, 80)
        }
        Some(true) => {
            notes.push(format!("Its length fits, but it is {} bytes and the library records {}: another copy or encoding. The beat grid may not line up; re-analyse afterwards.", f.size, w.file_size));
            (Confidence::Likely, 50)
        }
        Some(false) => return None,
        None if !size_differs => {
            notes.push("Same size (its length could not be compared).".into());
            (Confidence::Sure, 75)
        }
        None => return None,
    };
    Some(Verdict { kind: Kind::Tags, confidence, score, notes, duration_secs: t.duration_secs, size_differs })
}

/// Like [`classify`] for a file the user picked by hand: always answers. Without any evidence the
/// verdict is [`Kind::Manual`] with what differs, so the user sees what they are accepting.
pub fn inspect(w: &Wanted, f: &Found, read_tags: &mut dyn FnMut() -> Option<Tags>, o: &MatchOpts) -> Verdict {
    let cached: RefCell<Option<Option<Tags>>> = RefCell::new(None);
    let mut cached_read = || cached.borrow_mut().get_or_insert_with(&mut *read_tags).clone();
    if let Some(mut v) = classify(w, f, &mut cached_read, o) {
        v.notes.insert(0, "You picked this file.".into());
        return v;
    }
    let size_differs = f.size != w.file_size;
    let tags = cached_read();
    let mut notes = vec!["You picked this file; nothing else says it is the track's audio.".to_string()];
    notes.push(if size_differs {
        format!("Size {} bytes, the library records {}.", f.size, w.file_size)
    } else {
        "Same size as the library records.".into()
    });
    notes.push("Its name is not the one the track was added with.".into());
    match &tags {
        Some(t) => {
            if !t.title.is_empty() || !t.artist.is_empty() {
                let same = norm(&t.title) == norm(&w.title) && norm(&t.artist) == norm(&w.artist);
                notes.push(if same { "Its title and artist match.".into() } else { format!("Its tags say \"{} - {}\".", t.artist, t.title) });
            }
            if let Some(d) = t.duration_secs {
                notes.push(format!("Its length is {}, the track's is {}.", secs_text(d), secs_text(w.play_time_secs as f64)));
            }
        }
        None => notes.push("Its tags could not be read.".into()),
    }
    notes.push("The beat grid is kept as it is; if this is a different recording it will not line up, so re-analyse the track afterwards.".into());
    Verdict { kind: Kind::Manual, confidence: Confidence::Check, score: 0, notes, duration_secs: tags.and_then(|t| t.duration_secs), size_differs }
}

// ---- what it would do -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// The file is already under Music: only the track's location changes.
    Point,
    /// The file is copied into Music first.
    Copy,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Planned {
    pub action: ActionKind,
    /// The audio path relative to Music, '/'-separated.
    pub dest_rel: String,
    /// The `tracks.location` the track will have.
    pub new_location: String,
    /// A file already sits at the destination (an unused file with the same size).
    pub dest_exists: bool,
    /// The track will point at a path that is not in the shape the app generates; whether the
    /// device follows it is not verified on hardware.
    pub non_standard: bool,
    /// `tracks.format` becomes this (the picked file's extension differs from the recorded format).
    pub new_format: Option<String>,
    /// `tracks.play_time_secs` becomes this (the picked file's measured length differs).
    pub new_play_time_secs: Option<i64>,
    pub notes: Vec<String>,
}

/// The path of `p` under `music`, '/'-separated, if it lies inside it.
fn rel_in_music(music: &Path, p: &Path) -> Option<String> {
    let m = std::fs::canonicalize(music).ok()?;
    let c = std::fs::canonicalize(p).ok()?;
    let rel = c.strip_prefix(&m).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    let parts = parts?;
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The shape of the names this app (and the Editor) give audio: `h/h/hhhh….ext`.
fn is_standard_rel(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    parts.len() == 3 && parts[0].len() == 1 && hex(parts[0]) && parts[1].len() == 1 && hex(parts[1]) && {
        let (stem, ext) = split_name(parts[2]);
        hex(stem) && ext.len() > 1 && ext[1..].bytes().all(|b| b.is_ascii_alphanumeric())
    }
}

/// What taking `f` for `w` would do, or why it cannot be done. Nothing is written. `used` maps
/// lower-cased paths under Music to the track that uses them.
pub fn plan_action(root: &Path, w: &Wanted, f: &Found, v: &Verdict, used: &HashMap<String, i64>, o: &MatchOpts) -> Result<Planned, String> {
    let music = music_dir(root);
    let in_music = rel_in_music(&music, &f.path);
    let mut notes = Vec::new();
    if let Some(rel) = &in_music {
        if let Some(other) = used.get(&rel.to_lowercase()) {
            if *other != w.track_id {
                return Err(format!("{} is already used by another track, so it cannot also be this one's audio", f.path.display()));
            }
        }
    }
    let standard = naming::device_paths(&f.name, f.size);
    let mut non_standard = false;
    let (action, dest_rel) = match &in_music {
        Some(rel) if o.card_files == CardFiles::Reuse || rel.eq_ignore_ascii_case(&standard.0) => {
            if !is_standard_rel(rel) {
                non_standard = true;
                notes.push("Its path under Music is not one this app generated. The device should follow any plain path there, but that has not been checked on hardware; \"copy to the standard name\" in the options avoids the question.".into());
            }
            (ActionKind::Point, rel.clone())
        }
        _ => (ActionKind::Copy, standard.0.clone()),
    };
    let new_location = format!("{}{}", naming::LOCATION_PREFIX, dest_rel);
    if new_location.len() > MAX_LOCATION {
        return Err(format!("the path {new_location} is longer than the {MAX_LOCATION} characters the library can store"));
    }
    let Some((dest_audio, _)) = checked_track_paths(root, &new_location) else {
        return Err(format!("{new_location} is not a plain path under the card's Music folder"));
    };
    let mut dest_exists = false;
    if action == ActionKind::Copy {
        if let Some(other) = used.get(&dest_rel.to_lowercase()) {
            if *other != w.track_id {
                return Err(format!("another track already uses {new_location}, which is where this file would be copied"));
            }
        }
        if let Ok(md) = std::fs::metadata(&dest_audio) {
            if md.len() != f.size {
                return Err(format!("a different file ({} bytes) already sits at {new_location} on the card", md.len()));
            }
            dest_exists = true;
            notes.push("A file of the same size is already at the destination; it is compared byte for byte before anything is copied.".into());
        }
    }
    // the row describes the file it now points at: format and length follow it, bit rate and
    // sample rate are not read from the file and stay as recorded
    let (_, ext) = split_name(&f.name);
    let ext = ext.trim_start_matches('.');
    let new_format = (!ext.is_empty() && !w.format.eq_ignore_ascii_case(ext)).then(|| ext.to_ascii_uppercase());
    let new_play_time_secs = v.duration_secs.map(|d| d.round() as i64).filter(|&d| d > 0 && (d - w.play_time_secs).abs() > LENGTH_NOISE_SECS);
    if let Some(nf) = &new_format {
        notes.push(format!("The track's format changes from {} to {nf}.", if w.format.is_empty() { "(none)" } else { &w.format }));
    }
    if let Some(n) = new_play_time_secs {
        notes.push(format!("The track's recorded length changes from {} to {}.", secs_text(w.play_time_secs as f64), secs_text(n as f64)));
    }
    if v.size_differs {
        notes.push("Its bit rate and sample rate stay as recorded; they are not read from the file.".into());
    }
    Ok(Planned { action, dest_rel, new_location, dest_exists, non_standard, new_format, new_play_time_secs, notes })
}

// ---- the plan -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    /// The file's full path.
    pub path: String,
    pub name: String,
    pub size: u64,
    pub kind: Kind,
    pub confidence: Confidence,
    pub notes: Vec<String>,
    pub duration_secs: Option<f64>,
    pub action: ActionKind,
    /// Where it will be on the card, relative to Music.
    pub dest: String,
    /// The `tracks.location` after the relink (sent back with the run so a changed outcome is noticed).
    pub new_location: String,
    /// The file is already under Music on the card.
    pub on_card: bool,
    /// The track would point at a path the app did not generate (not verified on hardware).
    pub non_standard: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub track: Wanted,
    /// Best first. Empty: nothing in the folder fits.
    pub candidates: Vec<Candidate>,
    /// Files that fit but cannot be used, and why.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Plan {
    pub folder: String,
    pub total_missing: usize,
    pub files_seen: usize,
    pub tag_reads: usize,
    /// More audio files exist in the folder than were looked at.
    pub truncated: bool,
    /// The tag-read limit was reached: some files were matched by name only.
    pub tag_limit_hit: bool,
    pub rows: Vec<Row>,
    pub warnings: Vec<String>,
    /// Tracks left out because their location is not a plain path under Music.
    pub skipped_locations: usize,
    /// The card has no Music folder: every track is missing, and copying creates the folder.
    pub music_missing: bool,
    /// Files whose tags were read but gave no length (typical for an MP3 without a length header).
    pub tags_without_length: usize,
    /// Some file the plan would use is pointed at a path the app did not generate.
    pub non_standard: bool,
}

struct Pair {
    w: usize,
    f: usize,
    verdict: Verdict,
    planned: Planned,
    on_card: bool,
}

fn make_candidate(f: &Found, v: &Verdict, p: &Planned, on_card: bool) -> Candidate {
    Candidate {
        path: f.path.display().to_string(),
        name: f.name.clone(),
        size: f.size,
        kind: v.kind,
        confidence: v.confidence,
        notes: v.notes.iter().chain(p.notes.iter()).cloned().collect(),
        duration_secs: v.duration_secs,
        action: p.action,
        dest: p.dest_rel.clone(),
        new_location: p.new_location.clone(),
        on_card,
        non_standard: p.non_standard,
    }
}

/// Looks at one file the user picked for one missing track: the evidence for and against it, and
/// what taking it would do. Read-only. Errors when the track's audio is not missing, the file is
/// not audio, or the file cannot be used (see [`plan_action`]).
pub fn inspect_pick(lib: &Library, track_id: i64, path: &Path, o: &MatchOpts, read_tags: &dyn Fn(&Path) -> Option<Tags>) -> Result<Candidate, Error> {
    let w = missing_tracks(lib)?
        .into_iter()
        .find(|w| w.track_id == track_id)
        .ok_or_else(|| Error::Invalid("that track's audio file is not missing (any more)".into()))?;
    let f = Found::of(path)?;
    if !is_audio_name(&f.name) {
        return Err(Error::Invalid(format!("{} is not an audio file", f.name)));
    }
    let cache: RefCell<Option<Option<Tags>>> = RefCell::new(None);
    let mut read = || cache.borrow_mut().get_or_insert_with(|| read_tags(&f.path)).clone();
    let verdict = inspect(&w, &f, &mut read, o);
    let planned = plan_action(lib.root(), &w, &f, &verdict, &used_files(lib)?, o).map_err(Error::Invalid)?;
    let on_card = rel_in_music(&music_dir(lib.root()), &f.path).is_some();
    Ok(make_candidate(&f, &verdict, &planned, on_card))
}

/// Searches `folder` for the audio of every missing track. Read-only. `progress(phase, done,
/// total, label)`: phase 0 is listing folders, phase 1 comparing files. `read_tags` reads one
/// file's title, artist and length (None when it cannot).
pub fn plan(
    lib: &Library,
    folder: &Path,
    scan: &ScanOpts,
    o: &MatchOpts,
    read_tags: &dyn Fn(&Path) -> Option<Tags>,
    progress: &mut dyn FnMut(u8, usize, usize, &str),
    cancel: &dyn Fn() -> bool,
) -> Result<Plan, Error> {
    let missing = missing_report(lib)?;
    let wanted = missing.wanted;
    let mut plan = Plan {
        folder: folder.display().to_string(),
        total_missing: wanted.len(),
        skipped_locations: missing.skipped_locations,
        music_missing: missing.music_missing,
        ..Plan::default()
    };
    if missing.skipped_locations > 0 {
        plan.warnings.push(format!(
            "{} not counted here: {} location is not a plain path under the card's Music folder, so it is not followed. Health lists it.",
            plural_tracks(missing.skipped_locations),
            if missing.skipped_locations == 1 { "its" } else { "their" }
        ));
    }
    if wanted.is_empty() {
        plan.warnings.push("No track is missing its audio file.".into());
        return Ok(plan);
    }
    if missing.music_missing {
        plan.warnings.push("The card has no .Pacemaker/Music folder, so every track is missing its file. Copying a file back creates the folder again.".into());
    }
    let walked = walk_audio(folder, scan, &mut |n, dir| progress(0, n, 0, dir), cancel)?;
    plan.files_seen = walked.files.len();
    plan.truncated = walked.truncated;
    if walked.truncated {
        plan.warnings.push(format!("The folder holds more than {} audio files; only the first {} were looked at. Choose a narrower folder to be sure of finding everything.", scan.max_files, scan.max_files));
    }
    plan.warnings.extend(walked.unreadable.iter().map(|u| format!("Could not read {u}")));

    // look-ups so each file is only compared with the tracks it could belong to
    let mut by_loc: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_stem: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut by_tags: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, w) in wanted.iter().enumerate() {
        by_loc.entry(w.location.to_lowercase()).or_default().push(i);
        if let Some((lstem, lext)) = location_name(&w.location) {
            // a standard location holds a hash (already lower case); another holds the file's own name
            by_stem.entry((lstem.to_lowercase(), lext)).or_default().push(i);
        }
        if !norm(&w.title).is_empty() && !norm(&w.artist).is_empty() {
            by_tags.entry((norm(&w.title), norm(&w.artist))).or_default().push(i);
        }
    }

    let used = used_files(lib)?;
    let reads = Cell::new(0usize);
    let limit_hit = Cell::new(false);
    let no_length = Cell::new(0usize);
    let mut pairs: Vec<Pair> = Vec::new();
    let mut rejected: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let total = walked.files.len();
    for (fi, f) in walked.files.iter().enumerate() {
        if cancel() {
            return Err(Error::Invalid(format!("{CANCELLED}: the search was stopped")));
        }
        if fi % 20 == 0 || fi + 1 == total {
            progress(1, fi, total, &f.name);
        }
        let tags_cache: RefCell<Option<Option<Tags>>> = RefCell::new(None);
        let mut read = || {
            tags_cache
                .borrow_mut()
                .get_or_insert_with(|| {
                    if reads.get() >= o.max_tag_reads {
                        limit_hit.set(true);
                        return None;
                    }
                    reads.set(reads.get() + 1);
                    let t = read_tags(&f.path);
                    if t.as_ref().is_some_and(|t| t.duration_secs.is_none()) {
                        no_length.set(no_length.get() + 1);
                    }
                    t
                })
                .clone()
        };
        let mut hits: Vec<usize> = Vec::new();
        let (_, loc) = naming::device_paths(&f.name, f.size);
        hits.extend(by_loc.get(&loc.to_lowercase()).into_iter().flatten());
        let (stem, ext) = split_name(&f.name);
        let ext_l = ext.trim_start_matches('.').to_ascii_lowercase();
        hits.extend(by_stem.get(&(stem_hex(stem), ext_l.clone())).into_iter().flatten());
        hits.extend(by_stem.get(&(stem.to_lowercase(), ext_l)).into_iter().flatten());
        hits.sort_unstable();
        hits.dedup();
        let mut verdicts: Vec<(usize, Verdict)> = hits.iter().filter_map(|&wi| classify(&wanted[wi], f, &mut read, o).map(|v| (wi, v))).collect();
        if verdicts.is_empty() && o.use_tags && !by_tags.is_empty() {
            if let Some(t) = read() {
                if let Some(idx) = by_tags.get(&(norm(&t.title), norm(&t.artist))) {
                    verdicts.extend(idx.iter().filter_map(|&wi| classify(&wanted[wi], f, &mut read, o).map(|v| (wi, v))));
                }
            }
        }
        for (wi, verdict) in verdicts {
            match plan_action(lib.root(), &wanted[wi], f, &verdict, &used, o) {
                Ok(planned) => {
                    let on_card = rel_in_music(&music_dir(lib.root()), &f.path).is_some();
                    pairs.push(Pair { w: wi, f: fi, verdict, planned, on_card });
                }
                Err(why) => rejected.entry(wi).or_default().push(why),
            }
        }
    }
    plan.tag_reads = reads.get();
    plan.tag_limit_hit = limit_hit.get();
    plan.tags_without_length = no_length.get();
    if plan.tags_without_length > 0 {
        plan.warnings.push(format!(
            "{} of the files whose tags were read gave no length (common for an MP3 without a length header). Such a file can only match by its file name, or be picked by hand; it cannot match by title and artist.",
            plan.tags_without_length
        ));
    }
    if plan.tag_limit_hit {
        plan.warnings.push(format!("Tags were read from {} files and then the limit was reached, so the rest were matched by file name only. Raise the limit in the options, or search a narrower folder.", o.max_tag_reads));
    }

    // best candidate first for each track; ties prefer a file already on the card (nothing to copy)
    let order = |a: &Pair, b: &Pair| {
        b.verdict.score.cmp(&a.verdict.score).then(b.on_card.cmp(&a.on_card)).then_with(|| walked.files[a.f].path.cmp(&walked.files[b.f].path))
    };
    pairs.sort_by(order);
    // one file can only be one track's audio: the strongest match takes it
    let mut file_taken: HashSet<usize> = HashSet::new();
    let mut primary: HashMap<usize, usize> = HashMap::new();
    for (pi, p) in pairs.iter().enumerate() {
        if primary.contains_key(&p.w) || file_taken.contains(&p.f) {
            continue;
        }
        primary.insert(p.w, pi);
        file_taken.insert(p.f);
    }
    plan.non_standard = primary.values().any(|&pi| pairs[pi].planned.non_standard);
    if plan.non_standard {
        plan.warnings.push("Some matches are files already on the card under a path this app did not generate. The track will point at that path, and whether the Pacemaker plays a track from such a path has not been checked on hardware. To avoid the question, set \"A match already on the card\" to copy it to the name this app gives new files.".into());
    }
    let candidate = |p: &Pair| make_candidate(&walked.files[p.f], &p.verdict, &p.planned, p.on_card);
    for (wi, w) in wanted.iter().enumerate() {
        let mut candidates = Vec::new();
        if let Some(&pi) = primary.get(&wi) {
            candidates.push(candidate(&pairs[pi]));
        }
        for p in pairs.iter().filter(|p| p.w == wi) {
            if candidates.len() > MAX_ALTERNATIVES {
                break;
            }
            // a file some track's best match has taken (this one's own included) is not offered again
            if file_taken.contains(&p.f) {
                continue;
            }
            candidates.push(candidate(p));
        }
        let mut notes = rejected.remove(&wi).unwrap_or_default();
        if candidates.is_empty() && pairs.iter().any(|p| p.w == wi) {
            notes.push("The files that fit this track are a better match for other tracks, which have taken them.".into());
        }
        plan.rows.push(Row { track: w.clone(), candidates, notes });
    }
    progress(1, total, total, "");
    Ok(plan)
}

// ---- doing it -------------------------------------------------------------------------------

/// One relink the user ticked.
#[derive(Debug, Clone)]
pub struct Pick {
    pub track_id: i64,
    pub path: PathBuf,
    /// Chosen by hand ([`inspect`]) rather than found by the scan ([`classify`]).
    pub manual: bool,
    /// The `new_location` the preview showed; a different outcome now skips the track.
    pub expect_location: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GridState {
    /// The beat grid is where the track now looks for it.
    InPlace,
    /// The old beat grid was copied next to the new location.
    Copied,
    /// There is no beat grid for it; re-analyse the track.
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct Done {
    pub track_id: i64,
    pub label: String,
    pub from: String,
    pub to: String,
    pub copied: bool,
    pub bytes: u64,
    pub grid: GridState,
    /// `tracks.location` or the size changed (false: the file simply came back to its place).
    pub row_changed: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunReport {
    pub relinked: Vec<Done>,
    /// Left alone, by name, with the reason.
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub cancelled: bool,
    pub copied_bytes: u64,
}

/// Copies `src` to `dest` through a temp file, hashing as it reads, then reads `dest` back. On a
/// mismatch `dest` is removed again.
fn copy_verified(src: &Path, dest: &Path) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let mut sum = String::new();
    let mut total = 0u64;
    library::atomic_write_with(dest, |out| {
        use std::io::{Read, Write};
        let mut input = std::fs::File::open(src)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            total += n as u64;
        }
        sum = h.finish_hex();
        Ok(())
    })
    .map_err(|e| format!("could not copy {} to {}: {e}", src.display(), dest.display()))?;
    match sha256_file(dest) {
        Ok(back) if back == sum => Ok(total),
        Ok(_) => {
            let _ = std::fs::remove_file(dest);
            Err(format!("{} did not copy correctly (the read-back checksum differs)", dest.display()))
        }
        Err(e) => {
            let _ = std::fs::remove_file(dest);
            Err(format!("could not read {} back: {e}", dest.display()))
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

struct Ready {
    w: Wanted,
    f: Found,
    p: Planned,
}

/// Performs one relink. Files this step created are removed again if a later part of it fails.
fn execute(lib: &mut Library, r: &Ready) -> Result<Done, String> {
    let root = lib.root().to_path_buf();
    let (w, f, p) = (&r.w, &r.f, &r.p);
    let (dest_audio, new_sidecar) = checked_track_paths(&root, &p.new_location).ok_or_else(|| format!("{} is not a plain path under Music", p.new_location))?;
    let old_sidecar = checked_track_paths(&root, &w.location).map(|(_, s)| s);
    let mut created: Vec<PathBuf> = Vec::new();
    let undo = |created: &[PathBuf]| {
        for c in created {
            let _ = std::fs::remove_file(c);
        }
    };

    let mut copied = false;
    let mut bytes = 0;
    if p.action == ActionKind::Copy {
        if dest_audio.is_file() {
            // an unused file of the same size: only the identical file may stand in for the copy
            let same = match (sha256_file(&f.path), sha256_file(&dest_audio)) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            };
            if !same {
                return Err(format!("a different file already sits at {}", p.new_location));
            }
        } else {
            let (_, free) = ops_device::volume_space(&root).map_err(|e| format!("could not read the free space of the card: {e}"))?;
            if free < f.size + FREE_MARGIN {
                return Err(format!("the card has {} MB free and this file needs {} MB", free >> 20, (f.size + FREE_MARGIN) >> 20));
            }
            bytes = copy_verified(&f.path, &dest_audio)?;
            copied = true;
            created.push(dest_audio.clone());
        }
    }

    // the beat grid follows the track: look for it beside the new location
    let mut grid = GridState::InPlace;
    let same_sidecar = old_sidecar.as_ref().is_some_and(|o| o.to_string_lossy().eq_ignore_ascii_case(&new_sidecar.to_string_lossy()));
    if !new_sidecar.is_file() {
        match old_sidecar.as_ref().filter(|o| !same_sidecar && o.is_file()) {
            Some(old) => match library::atomic_copy(old, &new_sidecar) {
                Ok(()) => {
                    created.push(new_sidecar.clone());
                    grid = GridState::Copied;
                }
                Err(e) => {
                    undo(&created);
                    return Err(format!("could not copy the beat grid to {}: {e}", new_sidecar.display()));
                }
            },
            None => grid = GridState::Missing,
        }
    }

    let row_changed = p.new_location != w.location || f.size != w.file_size || p.new_format.is_some() || p.new_play_time_secs.is_some();
    if row_changed {
        let res = lib.con.execute(
            "UPDATE tracks SET location = ?1, file_size = ?2, date_modified = ?3, modified_by_ed = ?4,
                 format = COALESCE(?7, format), play_time_secs = COALESCE(?8, play_time_secs)
             WHERE track_id = ?5 AND track_id >= 0 AND location = ?6",
            params![p.new_location, f.size as i64, now_ms(), library::EDITOR_ID, w.track_id, w.location, p.new_format, p.new_play_time_secs],
        );
        match res {
            Ok(1) => {}
            Ok(_) => {
                undo(&created);
                return Err("the track changed while it was being relinked".into());
            }
            Err(e) => {
                undo(&created);
                return Err(format!("the library could not be updated: {e}"));
            }
        }
    }
    Ok(Done { track_id: w.track_id, label: w.label.clone(), from: w.location.clone(), to: p.new_location.clone(), copied, bytes, grid, row_changed })
}

/// Relinks the ticked tracks. Everything is re-checked on the fresh database first, with the same
/// functions the preview used; a track whose audio is back, whose file no longer matches or whose
/// outcome differs from the preview is skipped and named. `snapshot()` runs once, before the first
/// write, and a failure stops the run with nothing changed. `progress(done, total, label)`;
/// `cancel()` is polled before each track.
pub fn run(
    lib: &mut Library,
    picks: &[Pick],
    o: &MatchOpts,
    read_tags: &dyn Fn(&Path) -> Option<Tags>,
    snapshot: &dyn Fn() -> Result<(), Error>,
    progress: &mut dyn FnMut(usize, usize, &str),
    cancel: &dyn Fn() -> bool,
) -> Result<RunReport, Error> {
    let mut rep = RunReport::default();
    let wanted: HashMap<i64, Wanted> = missing_tracks(lib)?.into_iter().map(|w| (w.track_id, w)).collect();
    let mut used = used_files(lib)?;
    let mut ready: Vec<Ready> = Vec::new();
    let mut sources: HashSet<PathBuf> = HashSet::new();
    for pick in picks {
        let Some(w) = wanted.get(&pick.track_id) else {
            rep.skipped.push(format!("track {}: its audio is back, or it is not a track any more", pick.track_id));
            continue;
        };
        let f = match Found::of(&pick.path) {
            Ok(f) if is_audio_name(&f.name) => f,
            Ok(f) => {
                rep.skipped.push(format!("{}: {} is not an audio file", w.label, f.name));
                continue;
            }
            Err(e) => {
                rep.skipped.push(format!("{}: {e}", w.label));
                continue;
            }
        };
        let canon = std::fs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone());
        if !sources.insert(canon) {
            rep.skipped.push(format!("{}: {} is also picked for another track", w.label, f.name));
            continue;
        }
        let cache: RefCell<Option<Option<Tags>>> = RefCell::new(None);
        let mut read = || cache.borrow_mut().get_or_insert_with(|| read_tags(&f.path)).clone();
        let Some(verdict) = (if pick.manual { Some(inspect(w, &f, &mut read, o)) } else { classify(w, &f, &mut read, o) }) else {
            rep.skipped.push(format!("{}: {} no longer matches this track", w.label, f.name));
            continue;
        };
        match plan_action(lib.root(), w, &f, &verdict, &used, o) {
            Ok(p) if p.new_location == pick.expect_location => {
                used.insert(p.dest_rel.to_lowercase(), w.track_id);
                ready.push(Ready { w: w.clone(), f, p });
            }
            Ok(p) => rep.skipped.push(format!("{}: the card would now get {} instead of {} as previewed; preview again", w.label, p.new_location, pick.expect_location)),
            Err(why) => rep.skipped.push(format!("{}: {why}", w.label)),
        }
    }
    if ready.is_empty() {
        return Ok(rep);
    }
    snapshot()?;
    let total = ready.len();
    for (i, r) in ready.iter().enumerate() {
        if cancel() {
            rep.cancelled = true;
            break;
        }
        let verb = if r.p.action == ActionKind::Copy { "Copying" } else { "Relinking" };
        progress(i, total, &format!("{verb} {}", r.w.label));
        match execute(lib, r) {
            Ok(d) => {
                rep.copied_bytes += d.bytes;
                rep.relinked.push(d);
            }
            Err(e) => rep.failed.push(format!("{}: {e}", r.w.label)),
        }
        progress(i + 1, total, &format!("{verb} {}", r.w.label));
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;
    use crate::strfile;
    
    struct Fx {
        base: PathBuf,
        lib: Library,
        ids: Vec<i64>,
        /// The synthetic source files the tracks were added from.
        src: PathBuf,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn analysis() -> strfile::Analysis {
        strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        }
    }

    fn fixture(tag: &str, n: usize) -> Fx {
        let base = std::env::temp_dir().join(format!("pm-relink-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![i as u8 + 1; 1500 + i]).unwrap();
            let meta = TrackMeta { title: format!("Title {i}"), artist: format!("Artist {i}"), format: "MP3".into(), play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis()).unwrap());
        }
        Fx { base, lib, ids, src }
    }

    fn track(fx: &Fx, id: i64) -> crate::library::Track {
        fx.lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap()
    }

    fn audio_of(fx: &Fx, id: i64) -> PathBuf {
        checked_track_paths(fx.lib.root(), &track(fx, id).location).unwrap().0
    }

    fn sidecar_of(fx: &Fx, id: i64) -> PathBuf {
        checked_track_paths(fx.lib.root(), &track(fx, id).location).unwrap().1
    }

    /// Removes a track's audio file and returns its bytes.
    fn kill(fx: &Fx, id: i64) -> Vec<u8> {
        let a = audio_of(fx, id);
        let b = std::fs::read(&a).unwrap();
        std::fs::remove_file(a).unwrap();
        b
    }

    fn no_tags(_: &Path) -> Option<Tags> {
        None
    }

    fn scan(fx: &Fx, folder: &Path, o: &MatchOpts, tags: &dyn Fn(&Path) -> Option<Tags>) -> Plan {
        plan(&fx.lib, folder, &ScanOpts::default(), o, tags, &mut |_, _, _, _| {}, &|| false).unwrap()
    }

    fn ok_snapshot() -> Result<(), Error> {
        Ok(())
    }

    fn pick_of(row: &Row, i: usize) -> Pick {
        let c = &row.candidates[i];
        Pick { track_id: row.track.track_id, path: PathBuf::from(&c.path), manual: false, expect_location: c.new_location.clone() }
    }

    fn run_picks(fx: &mut Fx, picks: &[Pick], o: &MatchOpts, tags: &dyn Fn(&Path) -> Option<Tags>) -> RunReport {
        run(&mut fx.lib, picks, o, tags, &ok_snapshot, &mut |_, _, _| {}, &|| false).unwrap()
    }

    #[test]
    fn the_name_and_size_of_the_original_file_find_it_again() {
        let mut fx = fixture("exact", 3);
        let ids = fx.ids.clone();
        let bytes = kill(&fx, ids[1]);
        let _ = kill(&fx, ids[2]);
        let old_location = track(&fx, ids[1]).location;
        // the original, in a sub folder, plus an unrelated file and a same-sized file under another name
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(pc.join("sets/old")).unwrap();
        std::fs::copy(fx.src.join("synthetic 1.mp3"), pc.join("sets/old/synthetic 1.mp3")).unwrap();
        std::fs::write(pc.join("other.mp3"), vec![9u8; 1501]).unwrap();
        std::fs::write(pc.join("notes.txt"), b"x").unwrap();

        let p = scan(&fx, &pc, &MatchOpts { use_tags: false, ..Default::default() }, &no_tags);
        assert_eq!((p.total_missing, p.files_seen), (2, 2), "only audio files are looked at");
        let r1 = p.rows.iter().find(|r| r.track.track_id == ids[1]).unwrap();
        let r2 = p.rows.iter().find(|r| r.track.track_id == ids[2]).unwrap();
        assert!(r2.candidates.is_empty(), "no file fits the second track");
        let c = &r1.candidates[0];
        assert_eq!((c.kind, c.confidence, c.action), (Kind::Exact, Confidence::Sure, ActionKind::Copy));
        assert_eq!(c.new_location, old_location, "an exact match goes back to the track's own location");
        assert!(!c.on_card);

        let rep = run_picks(&mut fx, &[pick_of(r1, 0)], &MatchOpts::default(), &no_tags);
        assert_eq!((rep.relinked.len(), rep.skipped.len(), rep.failed.len()), (1, 0, 0), "{rep:?}");
        let d = &rep.relinked[0];
        assert!(d.copied && !d.row_changed && d.grid == GridState::InPlace);
        assert_eq!(std::fs::read(audio_of(&fx, ids[1])).unwrap(), bytes, "the identical bytes are back");
        assert!(sidecar_of(&fx, ids[1]).is_file(), "its beat grid never left");
        assert_eq!(track(&fx, ids[1]).location, old_location);
        assert!(pc.join("sets/old/synthetic 1.mp3").is_file(), "the source is not touched");
        let left: Vec<_> = std::fs::read_dir(audio_of(&fx, ids[1]).parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(left.iter().all(|n| !n.ends_with(".tmp")), "no temp file left: {left:?}");
    }

    #[test]
    fn a_file_already_on_the_card_is_pointed_at_and_its_beat_grid_follows() {
        let mut fx = fixture("point", 2);
        let ids = fx.ids.clone();
        let old = track(&fx, ids[0]);
        let (old_audio, old_sidecar) = checked_track_paths(fx.lib.root(), &old.location).unwrap();
        // the audio was moved by hand to another place under Music, without its beat grid
        let moved = music_dir(fx.lib.root()).join("misc").join("moved audio.mp3");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&old_audio, &moved).unwrap();

        let o = MatchOpts { use_tags: false, ..Default::default() };
        // the file kept its name?  No: it was renamed, so only the tags can find it
        let tags = |p: &Path| (p.file_name().unwrap() == "moved audio.mp3").then(|| Tags { title: "Title 0".into(), artist: "ARTIST  0".into(), duration_secs: Some(100.5) });
        let none = scan(&fx, fx.lib.root(), &o, &tags);
        assert!(none.rows[0].candidates.is_empty(), "tags are off");

        let o = MatchOpts::default();
        let p = scan(&fx, &music_dir(fx.lib.root()), &o, &tags);
        let row = &p.rows[0];
        let c = &row.candidates[0];
        assert_eq!((c.kind, c.confidence, c.action, c.on_card), (Kind::Tags, Confidence::Sure, ActionKind::Point, true));
        assert_eq!(c.dest, "misc/moved audio.mp3");
        assert!(c.notes.iter().any(|n| n.contains("not one this app generated")), "{:?}", c.notes);
        assert!(c.non_standard && p.non_standard, "the candidate and the plan say so");
        assert_eq!(p.warnings.iter().filter(|w| w.contains("not been checked on hardware")).count(), 1, "said once for the plan: {:?}", p.warnings);
        assert_eq!(p.tag_reads, 2, "the other, still present file is read too");

        let rep = run_picks(&mut fx, &[pick_of(row, 0)], &o, &tags);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        let d = &rep.relinked[0];
        assert!(!d.copied && d.row_changed && d.grid == GridState::Copied);
        assert_eq!(track(&fx, ids[0]).location, "/pmdb_tracks/misc/moved audio.mp3");
        assert!(moved.is_file(), "the file stays where it was");
        assert!(old_sidecar.is_file(), "the old beat grid is copied, not moved");
        assert!(PathBuf::from(naming::str_path(&moved.to_string_lossy())).is_file());
        assert_eq!(track(&fx, ids[0]).file_size as u64, 1500);
    }

    #[test]
    fn the_standard_setting_copies_a_card_file_to_the_apps_naming() {
        let mut fx = fixture("standard", 1);
        let id = fx.ids[0];
        let old = track(&fx, id);
        let (old_audio, _) = checked_track_paths(fx.lib.root(), &old.location).unwrap();
        let moved = music_dir(fx.lib.root()).join("misc").join("synthetic 0.mp3");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&old_audio, &moved).unwrap();
        let o = MatchOpts { use_tags: false, card_files: CardFiles::Standard, ..Default::default() };
        let p = scan(&fx, &music_dir(fx.lib.root()), &o, &no_tags);
        let c = &p.rows[0].candidates[0];
        assert_eq!((c.kind, c.action, c.on_card), (Kind::Exact, ActionKind::Copy, true));
        assert_eq!(c.new_location, old.location);
        let rep = run_picks(&mut fx, &[pick_of(&p.rows[0], 0)], &o, &no_tags);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        assert!(old_audio.is_file() && moved.is_file(), "both exist: the original stays");
    }

    #[test]
    fn the_same_name_with_another_size_is_a_weaker_match_with_its_warning() {
        let mut fx = fixture("samename", 2);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::write(pc.join("synthetic 0.mp3"), vec![7u8; 1700]).unwrap();
        // its length is wrong: the track is 100 s
        let tags = |_: &Path| Some(Tags { title: "x".into(), artist: "y".into(), duration_secs: Some(240.0) });
        let o = MatchOpts::default();
        let p = scan(&fx, &pc, &o, &tags);
        let c = &p.rows[0].candidates[0];
        assert_eq!((c.kind, c.confidence), (Kind::SameName, Confidence::Check));
        assert!(c.notes.iter().any(|n| n.contains("may be a different version")), "{:?}", c.notes);
        assert_ne!(c.new_location, track(&fx, ids[0]).location, "another size is another place");

        // with a length that fits it is Likely
        let tags_ok = |_: &Path| Some(Tags { title: "x".into(), artist: "y".into(), duration_secs: Some(100.0) });
        let p = scan(&fx, &pc, &o, &tags_ok);
        let c = &p.rows[0].candidates[0];
        assert_eq!((c.kind, c.confidence), (Kind::SameName, Confidence::Likely));
        let rep = run_picks(&mut fx, &[pick_of(&p.rows[0], 0)], &o, &tags_ok);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        let t = track(&fx, ids[0]);
        assert_eq!((t.file_size, t.location.as_str()), (1700, c.new_location.as_str()));
        assert_eq!(rep.relinked[0].grid, GridState::Copied, "the old beat grid is copied next to the new location");
        assert!(audio_of(&fx, ids[0]).is_file());
    }

    #[test]
    fn tags_need_a_fitting_length_and_never_match_blank_tags() {
        let fx = fixture("tags", 3);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        kill(&fx, ids[1]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::write(pc.join("a.flac"), vec![1u8; 4000]).unwrap(); // Title 0, length fits
        std::fs::write(pc.join("b.flac"), vec![2u8; 4001]).unwrap(); // Title 1, length does not fit
        std::fs::write(pc.join("c.flac"), vec![3u8; 4002]).unwrap(); // blank tags
        let tags = |p: &Path| match p.file_name().unwrap().to_str().unwrap() {
            "a.flac" => Some(Tags { title: "title 0".into(), artist: "Artist 0".into(), duration_secs: Some(101.0) }),
            "b.flac" => Some(Tags { title: "Title 1".into(), artist: "Artist 1".into(), duration_secs: Some(300.0) }),
            _ => Some(Tags::default()),
        };
        let p = scan(&fx, &pc, &MatchOpts::default(), &tags);
        let a = &p.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap().candidates;
        let b = &p.rows.iter().find(|r| r.track.track_id == ids[1]).unwrap().candidates;
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].kind, a[0].confidence), (Kind::Tags, Confidence::Likely), "a different size: not Sure");
        assert!(b.is_empty(), "a length 200 s off is a different recording");
        // a wider tolerance is the user's choice
        let wide = MatchOpts { duration_tolerance_secs: 250.0, ..Default::default() };
        let p = scan(&fx, &pc, &wide, &tags);
        assert_eq!(p.rows.iter().find(|r| r.track.track_id == ids[1]).unwrap().candidates.len(), 1);
    }

    #[test]
    fn one_file_belongs_to_one_track_and_the_rest_are_alternatives() {
        let fx = fixture("assign", 2);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        // the same original in two folders: both are offered, the first by path is the best
        let pc = fx.base.join("pc");
        for d in ["a", "b"] {
            std::fs::create_dir_all(pc.join(d)).unwrap();
            std::fs::copy(fx.src.join("synthetic 0.mp3"), pc.join(d).join("synthetic 0.mp3")).unwrap();
        }
        let p = scan(&fx, &pc, &MatchOpts { use_tags: false, ..Default::default() }, &no_tags);
        let row = &p.rows[0];
        assert_eq!(row.candidates.len(), 2);
        assert!(row.candidates[0].path < row.candidates[1].path);
        assert!(row.candidates.iter().all(|c| c.kind == Kind::Exact));
    }

    #[test]
    fn a_card_file_that_another_track_uses_is_refused_with_the_reason() {
        let mut fx = fixture("used", 2);
        let ids = fx.ids.clone();
        // track 0's row is made to point at track 1's file: a stand-in for a library with two rows on one file
        kill(&fx, ids[0]);
        let t1 = track(&fx, ids[1]);
        let w = missing_tracks(&fx.lib).unwrap().remove(0);
        let f = Found::of(&audio_of(&fx, ids[1])).unwrap();
        let used = used_files(&fx.lib).unwrap();
        let v = inspect(&w, &f, &mut || None, &MatchOpts::default());
        let e = plan_action(fx.lib.root(), &w, &f, &v, &used, &MatchOpts::default()).unwrap_err();
        assert!(e.contains("already used by another track"), "{e}");
        // and via a pick: skipped, nothing written
        let rep = run_picks(&mut fx, &[Pick { track_id: ids[0], path: f.path.clone(), manual: true, expect_location: t1.location }], &MatchOpts::default(), &no_tags);
        assert!(rep.relinked.is_empty() && rep.skipped.len() == 1, "{rep:?}");
        assert!(rep.skipped[0].contains("already used"), "{:?}", rep.skipped);
    }

    /// Two tracks, the first missing; the only copy of it on the PC has another size, so it lands at a
    /// different place on the card. Returns the fixture, the previewed pick and that destination.
    fn same_name_scene(tag: &str) -> (Fx, Pick, PathBuf, MatchOpts) {
        let fx = fixture(tag, 2);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::write(pc.join("synthetic 0.mp3"), vec![7u8; 1700]).unwrap();
        let o = MatchOpts { use_tags: false, ..Default::default() };
        let p = scan(&fx, &pc, &o, &no_tags);
        let c = &p.rows[0].candidates[0];
        assert_eq!((c.kind, c.action), (Kind::SameName, ActionKind::Copy));
        assert_ne!(c.new_location, track(&fx, ids[0]).location, "this scene needs a different destination");
        let dest = checked_track_paths(fx.lib.root(), &c.new_location).unwrap().0;
        let pick = pick_of(&p.rows[0], 0);
        (fx, pick, dest, o)
    }

    #[test]
    fn a_different_file_at_the_destination_is_never_overwritten() {
        let (mut fx, pick, dest, o) = same_name_scene("clash");
        let ids = fx.ids.clone();
        let before = track(&fx, ids[0]);
        // after the preview a different file of the same size appears where the copy would go
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, vec![0xEEu8; 1700]).unwrap();
        let rep = run_picks(&mut fx, &[pick], &o, &no_tags);
        assert!(rep.relinked.is_empty() && rep.failed.len() == 1, "{rep:?}");
        assert!(rep.failed[0].contains("different file"), "{:?}", rep.failed);
        assert_eq!(std::fs::read(&dest).unwrap(), vec![0xEEu8; 1700], "untouched");
        let after = track(&fx, ids[0]);
        assert_eq!((after.location, after.file_size), (before.location, before.file_size), "the row did not change");
        // and a file of another size there is refused already at the preview
        std::fs::write(&dest, vec![0xEEu8; 1699]).unwrap();
        let w = missing_tracks(&fx.lib).unwrap().remove(0);
        let f = Found::of(&fx.base.join("pc/synthetic 0.mp3")).unwrap();
        let v = inspect(&w, &f, &mut || None, &o);
        let e = plan_action(fx.lib.root(), &w, &f, &v, &used_files(&fx.lib).unwrap(), &o).unwrap_err();
        assert!(e.contains("1699 bytes"), "{e}");
    }

    #[test]
    fn an_identical_file_already_at_the_destination_is_reused_not_copied() {
        let (mut fx, pick, dest, o) = same_name_scene("identical");
        let ids = fx.ids.clone();
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, vec![7u8; 1700]).unwrap();
        let rep = run_picks(&mut fx, &[pick], &o, &no_tags);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        assert!(!rep.relinked[0].copied && rep.relinked[0].row_changed);
        assert_eq!(track(&fx, ids[0]).file_size, 1700);
        assert_eq!(rep.copied_bytes, 0);
    }

    #[test]
    fn stale_picks_are_skipped_by_name_and_a_changed_outcome_is_noticed() {
        let mut fx = fixture("stale", 3);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        kill(&fx, ids[1]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        for i in 0..2 {
            std::fs::copy(fx.src.join(format!("synthetic {i}.mp3")), pc.join(format!("synthetic {i}.mp3"))).unwrap();
        }
        let o = MatchOpts { use_tags: false, ..Default::default() };
        let p = scan(&fx, &pc, &o, &no_tags);
        let mut picks: Vec<Pick> = p.rows.iter().map(|r| pick_of(r, 0)).collect();
        // track 0: the file is deleted after the preview; track 1: expects another outcome; plus a track that is not missing
        std::fs::remove_file(&picks[0].path).unwrap();
        picks[1].expect_location = "/pmdb_tracks/0/0/other.mp3".into();
        picks.push(Pick { track_id: ids[2], path: pc.join("synthetic 1.mp3"), manual: false, expect_location: String::new() });
        let rep = run_picks(&mut fx, &picks, &o, &no_tags);
        assert!(rep.relinked.is_empty() && rep.failed.is_empty(), "{rep:?}");
        assert_eq!(rep.skipped.len(), 3, "{:?}", rep.skipped);
        assert!(rep.skipped.iter().any(|s| s.contains("preview again")));
        assert!(rep.skipped.iter().any(|s| s.contains("audio is back")));
    }

    #[test]
    fn a_manual_pick_is_always_inspected_and_labelled_by_its_evidence() {
        let mut fx = fixture("manual", 2);
        let ids = fx.ids.clone();
        kill(&fx, ids[1]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        let odd = pc.join("totally different name.wav");
        std::fs::write(&odd, vec![5u8; 2222]).unwrap();
        let w = missing_tracks(&fx.lib).unwrap().remove(0);
        let f = Found::of(&odd).unwrap();
        let o = MatchOpts::default();
        let v = inspect(&w, &f, &mut || Some(Tags { title: "Something else".into(), artist: "Someone".into(), duration_secs: Some(555.0) }), &o);
        assert_eq!((v.kind, v.confidence), (Kind::Manual, Confidence::Check));
        let text = v.notes.join(" ");
        assert!(text.contains("Size 2222") && text.contains("Something else") && text.contains("9:15"), "{text}");
        let used = used_files(&fx.lib).unwrap();
        let p = plan_action(fx.lib.root(), &w, &f, &v, &used, &o).unwrap();
        assert_eq!(p.action, ActionKind::Copy);
        assert_eq!(p.new_format.as_deref(), Some("WAV"), "the recorded format follows the picked file");
        assert_eq!(p.new_play_time_secs, Some(555), "and so does the measured length");
        assert!(p.notes.iter().any(|n| n.contains("format changes from MP3 to WAV")) && p.notes.iter().any(|n| n.contains("1:41 to 9:15")), "{:?}", p.notes);
        // run it: classify would reject this file, but the user picked it
        let rep = run_picks(&mut fx, &[Pick { track_id: ids[1], path: odd.clone(), manual: true, expect_location: p.new_location.clone() }], &o, &no_tags);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        assert_eq!(track(&fx, ids[1]).location, p.new_location);
        assert_eq!(track(&fx, ids[1]).file_size, 2222);
        assert_eq!(track(&fx, ids[1]).format, "WAV");
        // and the same pick not marked manual is refused
        let mut fx = fixture("manual2", 2);
        let ids = fx.ids.clone();
        kill(&fx, ids[1]);
        let rep = run_picks(&mut fx, &[Pick { track_id: ids[1], path: odd, manual: false, expect_location: p.new_location }], &o, &no_tags);
        assert!(rep.relinked.is_empty() && rep.skipped[0].contains("no longer matches"), "{rep:?}");
    }

    #[test]
    fn a_failed_snapshot_stops_the_run_with_nothing_written() {
        let mut fx = fixture("snap", 1);
        let id = fx.ids[0];
        kill(&fx, id);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::copy(fx.src.join("synthetic 0.mp3"), pc.join("synthetic 0.mp3")).unwrap();
        let o = MatchOpts { use_tags: false, ..Default::default() };
        let p = scan(&fx, &pc, &o, &no_tags);
        let pick = pick_of(&p.rows[0], 0);
        let e = run(&mut fx.lib, std::slice::from_ref(&pick), &o, &no_tags, &|| Err(Error::Invalid("disk full".into())), &mut |_, _, _| {}, &|| false).unwrap_err();
        assert!(e.to_string().contains("disk full"));
        assert!(!audio_of(&fx, id).exists(), "nothing was copied");
        // the snapshot runs exactly once however many tracks are relinked
        let calls = RefCell::new(0);
        let rep = run(&mut fx.lib, &[pick], &o, &no_tags, &|| { *calls.borrow_mut() += 1; Ok(()) }, &mut |_, _, _| {}, &|| false).unwrap();
        assert_eq!((rep.relinked.len(), *calls.borrow()), (1, 1));
    }

    #[test]
    fn cancelling_stops_before_the_next_track_and_keeps_the_finished_ones() {
        let mut fx = fixture("cancel", 3);
        let ids = fx.ids.clone();
        for id in &ids {
            kill(&fx, *id);
        }
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        for i in 0..3 {
            std::fs::copy(fx.src.join(format!("synthetic {i}.mp3")), pc.join(format!("synthetic {i}.mp3"))).unwrap();
        }
        let o = MatchOpts { use_tags: false, ..Default::default() };
        let p = scan(&fx, &pc, &o, &no_tags);
        let picks: Vec<Pick> = p.rows.iter().map(|r| pick_of(r, 0)).collect();
        let done = RefCell::new(0usize);
        let rep = run(&mut fx.lib, &picks, &o, &no_tags, &ok_snapshot, &mut |d, _, _| *done.borrow_mut() = d, &|| *done.borrow() >= 1).unwrap();
        assert!(rep.cancelled);
        assert_eq!(rep.relinked.len(), 1);
        assert_eq!(missing_tracks(&fx.lib).unwrap().len(), 2);
    }

    #[test]
    fn nothing_is_missing_means_nothing_is_searched() {
        let fx = fixture("none", 2);
        let p = scan(&fx, &fx.base.join("does-not-exist"), &MatchOpts::default(), &no_tags);
        assert!(p.rows.is_empty() && p.warnings[0].contains("No track is missing"));
    }

    #[test]
    fn a_missing_folder_or_music_folder_is_an_error_not_a_dead_library() {
        let fx = fixture("nofolder", 2);
        kill(&fx, fx.ids[0]);
        let e = plan(&fx.lib, &fx.base.join("nope"), &ScanOpts::default(), &MatchOpts::default(), &no_tags, &mut |_, _, _, _| {}, &|| false).unwrap_err();
        assert!(e.to_string().contains("not a folder"));
    }

    #[test]
    fn a_card_without_its_music_folder_lists_every_track_and_a_copy_makes_the_folder_again() {
        let mut fx = fixture("nomusic", 2);
        let ids = fx.ids.clone();
        let old = [track(&fx, ids[0]).location, track(&fx, ids[1]).location];
        std::fs::remove_dir_all(music_dir(fx.lib.root())).unwrap();
        let m = missing_report(&fx.lib).unwrap();
        assert!(m.music_missing && m.wanted.len() == 2, "{m:?}");
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::copy(fx.src.join("synthetic 0.mp3"), pc.join("synthetic 0.mp3")).unwrap();
        let o = MatchOpts { use_tags: false, ..Default::default() };
        let p = scan(&fx, &pc, &o, &no_tags);
        assert!(p.music_missing && p.total_missing == 2);
        assert!(p.warnings.iter().any(|w| w.contains("no .Pacemaker/Music folder")), "{:?}", p.warnings);
        let r0 = p.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap();
        assert_eq!((r0.candidates[0].kind, r0.candidates[0].action), (Kind::Exact, ActionKind::Copy));
        let rep = run_picks(&mut fx, &[pick_of(r0, 0)], &o, &no_tags);
        assert_eq!((rep.relinked.len(), rep.failed.len()), (1, 0), "{rep:?}");
        assert_eq!(track(&fx, ids[0]).location, old[0]);
        assert!(audio_of(&fx, ids[0]).is_file(), "the folder and the file are back");
        assert!(!audio_of(&fx, ids[1]).is_file(), "the other track is still missing");
    }

    #[test]
    fn locations_that_are_not_plain_paths_are_counted_not_silently_dropped() {
        let fx = fixture("skipped", 3);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        fx.lib.con.execute("UPDATE tracks SET location = '/pmdb_tracks/../x.mp3' WHERE track_id = ?1", [ids[1]]).unwrap();
        let m = missing_report(&fx.lib).unwrap();
        assert_eq!((m.wanted.len(), m.skipped_locations), (1, 1));
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        let p = scan(&fx, &pc, &MatchOpts::default(), &no_tags);
        assert_eq!(p.skipped_locations, 1);
        assert!(p.warnings.iter().any(|w| w.contains("1 track not counted here") && w.contains("its location")), "{:?}", p.warnings);
        // nothing else missing: the skipped one is still said
        let fx = fixture("skipped2", 1);
        fx.lib.con.execute("UPDATE tracks SET location = '/pmdb_tracks/a/../x.mp3'", []).unwrap();
        let p = scan(&fx, &fx.base.join("whatever"), &MatchOpts::default(), &no_tags);
        assert!(p.rows.is_empty() && p.warnings.len() == 2 && p.warnings[1].contains("No track is missing"), "{:?}", p.warnings);
    }

    #[test]
    fn the_row_follows_the_file_it_now_points_at_in_format_and_length_only_where_they_differ() {
        let (mut fx, pick, _dest, o) = same_name_scene("rowdata");
        let id = fx.ids[0];
        // the file is read again at the run; its length is 30 s longer than recorded (100 s)
        let tags = |_: &Path| Some(Tags { title: "x".into(), artist: "y".into(), duration_secs: Some(130.2) });
        let o = MatchOpts { use_tags: true, ..o };
        let rep = run_picks(&mut fx, &[pick], &o, &tags);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        let t = track(&fx, id);
        assert_eq!((t.play_time_secs, t.format.as_str()), (130, "MP3"), "the length follows, the format is the same extension");

        // a length within a second of the recorded one is rounding: kept
        let (mut fx, pick, _d, o) = same_name_scene("rowdata2");
        let id = fx.ids[0];
        let o = MatchOpts { use_tags: true, ..o };
        let near = |_: &Path| Some(Tags { title: "x".into(), artist: "y".into(), duration_secs: Some(100.6) });
        let rep = run_picks(&mut fx, &[pick], &o, &near);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        assert_eq!(track(&fx, id).play_time_secs, 100);

        // another container: the format follows, and the preview says so in words
        let fx = fixture("rowdata3", 1);
        let id = fx.ids[0];
        kill(&fx, id);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::write(pc.join("synthetic 0.flac"), vec![3u8; 2000]).unwrap();
        let c = inspect_pick(&fx.lib, id, &pc.join("synthetic 0.flac"), &MatchOpts::default(), &no_tags).unwrap();
        assert!(c.notes.iter().any(|n| n.contains("format changes from MP3 to FLAC")), "{:?}", c.notes);
        assert!(c.notes.iter().any(|n| n.contains("bit rate and sample rate stay as recorded")), "{:?}", c.notes);
    }

    #[test]
    fn a_track_pointed_at_another_name_is_found_by_that_name_when_it_goes_missing_again() {
        let mut fx = fixture("renamed", 2);
        let ids = fx.ids.clone();
        let music = music_dir(fx.lib.root());
        std::fs::create_dir_all(music.join("misc")).unwrap();
        // an earlier relink left this row on a file with its own name; the file is gone again
        kill(&fx, ids[0]);
        fx.lib
            .con
            .execute("UPDATE tracks SET location = '/pmdb_tracks/misc/moved audio.mp3', file_size = 1800 WHERE track_id = ?1", [ids[0]])
            .unwrap();
        let pc = fx.base.join("pc");
        let pc2 = fx.base.join("pc2");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::create_dir_all(&pc2).unwrap();
        std::fs::write(pc.join("moved audio.mp3"), vec![4u8; 1800]).unwrap();
        std::fs::write(pc2.join("Moved Audio.MP3"), vec![4u8; 1900]).unwrap();
        let o = MatchOpts { use_tags: false, ..Default::default() };
        let p = scan(&fx, &pc, &o, &no_tags);
        let c = &p.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap().candidates;
        assert_eq!((c[0].kind, c[0].confidence), (Kind::Exact, Confidence::Sure), "same name and same size");
        let p2 = scan(&fx, &pc2, &o, &no_tags);
        let c2 = &p2.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap().candidates;
        assert_eq!((c2[0].kind, c2[0].confidence), (Kind::SameName, Confidence::Likely), "the name is compared without case");
        // and a file with a different name is not taken for it by the name rule
        std::fs::write(pc.join("another.mp3"), vec![4u8; 1800]).unwrap();
        let p3 = scan(&fx, &pc, &o, &no_tags);
        assert_eq!(p3.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap().candidates.len(), 1);
        // the run copies it to the app's naming and the row follows
        let rep = run_picks(&mut fx, &[pick_of(p.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap(), 0)], &o, &no_tags);
        assert_eq!(rep.relinked.len(), 1, "{rep:?}");
        assert!(is_standard_rel(track(&fx, ids[0]).location.strip_prefix(naming::LOCATION_PREFIX).unwrap()));
    }

    #[test]
    fn tags_without_a_length_are_counted_and_said() {
        let fx = fixture("nolength", 1);
        kill(&fx, fx.ids[0]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::write(pc.join("renamed.mp3"), vec![8u8; 1777]).unwrap();
        let tags = |_: &Path| Some(Tags { title: "Title 0".into(), artist: "Artist 0".into(), duration_secs: None });
        let p = scan(&fx, &pc, &MatchOpts::default(), &tags);
        assert_eq!(p.tags_without_length, 1);
        assert!(p.rows[0].candidates.is_empty(), "a re-tagged file whose length is unknown cannot match by tags");
        assert!(p.warnings.iter().any(|w| w.contains("no length") && w.contains("picked by hand")), "{:?}", p.warnings);
    }

    #[test]
    fn the_tag_read_limit_is_reported_and_name_matches_still_work() {
        let fx = fixture("limit", 1);
        kill(&fx, fx.ids[0]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::copy(fx.src.join("synthetic 0.mp3"), pc.join("synthetic 0.mp3")).unwrap();
        for i in 0..5 {
            std::fs::write(pc.join(format!("noise {i}.mp3")), vec![i as u8; 3000 + i]).unwrap();
        }
        let o = MatchOpts { max_tag_reads: 2, ..Default::default() };
        let p = scan(&fx, &pc, &o, &|_: &Path| Some(Tags { title: "t".into(), artist: "a".into(), duration_secs: None }));
        assert!(p.tag_limit_hit && p.tag_reads == 2, "{p:?}");
        assert_eq!(p.rows[0].candidates[0].kind, Kind::Exact);
        assert!(p.warnings.iter().any(|w| w.contains("limit was reached")));
    }

    #[test]
    fn the_walk_skips_links_system_folders_and_stops_at_the_limit() {
        let base = std::env::temp_dir().join(format!("pm-relink-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["a/b", "$RECYCLE.BIN", "System Volume Information"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        for f in ["a/1.mp3", "a/b/2.FLAC", "a/b/readme.txt", "$RECYCLE.BIN/3.mp3", "System Volume Information/4.mp3", "5.wav"] {
            std::fs::write(base.join(f), b"x").unwrap();
        }
        let w = walk_audio(&base, &ScanOpts::default(), &mut |_, _| {}, &|| false).unwrap();
        let names: Vec<_> = w.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["5.wav", "1.mp3", "2.FLAC"], "sorted by path");
        let w = walk_audio(&base, &ScanOpts { subfolders: false, max_files: 100 }, &mut |_, _| {}, &|| false).unwrap();
        assert_eq!(w.files.len(), 1);
        let w = walk_audio(&base, &ScanOpts { subfolders: true, max_files: 2 }, &mut |_, _| {}, &|| false).unwrap();
        assert!(w.truncated && w.files.len() == 2);
        let e = walk_audio(&base, &ScanOpts::default(), &mut |_, _| {}, &|| true).unwrap_err();
        assert!(e.to_string().contains(CANCELLED));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_file_two_tracks_fit_goes_to_the_better_match_and_the_other_row_says_so() {
        let fx = fixture("taken", 2);
        let ids = fx.ids.clone();
        // two rows for the same recording (a duplicate): same tags, sizes 1500 and 1501
        fx.lib.con.execute("UPDATE tracks SET title = 'Title 0', artist = 'Artist 0' WHERE track_id = ?1", [ids[1]]).unwrap();
        kill(&fx, ids[0]);
        kill(&fx, ids[1]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        std::fs::write(pc.join("renamed.flac"), vec![3u8; 1500]).unwrap();
        let tags = |_: &Path| Some(Tags { title: "Title 0".into(), artist: "Artist 0".into(), duration_secs: Some(100.5) });
        let p = scan(&fx, &pc, &MatchOpts::default(), &tags);
        let r0 = p.rows.iter().find(|r| r.track.track_id == ids[0]).unwrap();
        let r1 = p.rows.iter().find(|r| r.track.track_id == ids[1]).unwrap();
        assert_eq!((r0.candidates[0].kind, r0.candidates[0].confidence), (Kind::Tags, Confidence::Sure), "the same size wins");
        assert!(r1.candidates.is_empty() && r1.notes.iter().any(|n| n.contains("taken them")), "{:?}", r1.notes);
    }

    #[test]
    fn inspecting_a_picked_file_reports_the_evidence_and_the_outcome() {
        let fx = fixture("inspect", 2);
        let ids = fx.ids.clone();
        kill(&fx, ids[0]);
        let pc = fx.base.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        let odd = pc.join("whatever.wav");
        std::fs::write(&odd, vec![5u8; 2000]).unwrap();
        let o = MatchOpts::default();
        let c = inspect_pick(&fx.lib, ids[0], &odd, &o, &no_tags).unwrap();
        assert_eq!((c.kind, c.confidence, c.action), (Kind::Manual, Confidence::Check, ActionKind::Copy));
        assert!(c.notes.iter().any(|n| n.contains("Its tags could not be read")), "{:?}", c.notes);
        // the track whose audio is present, a non-audio file and a missing file are refused
        assert!(inspect_pick(&fx.lib, ids[1], &odd, &o, &no_tags).unwrap_err().to_string().contains("not missing"));
        std::fs::write(pc.join("a.txt"), b"x").unwrap();
        assert!(inspect_pick(&fx.lib, ids[0], &pc.join("a.txt"), &o, &no_tags).unwrap_err().to_string().contains("not an audio file"));
        assert!(inspect_pick(&fx.lib, ids[0], &pc.join("nope.mp3"), &o, &no_tags).is_err());
        // the exact original is recognised even when picked by hand
        let orig = fx.src.join("synthetic 0.mp3");
        let c = inspect_pick(&fx.lib, ids[0], &orig, &o, &no_tags).unwrap();
        assert_eq!((c.kind, c.confidence), (Kind::Exact, Confidence::Sure));
        assert!(c.notes[0].contains("You picked"));
    }

    #[test]
    fn standard_names_and_name_splitting() {
        for ok in ["6/8/722ef9a6.mp3", "a/b/c.FLAC", "0/0/0.m4a"] {
            assert!(is_standard_rel(ok), "{ok}");
        }
        for bad in ["misc/x.mp3", "6/8/722EF9A6.mp3", "66/8/x.mp3", "6/8/722ef9a6", "6/8/g.mp3", "6/8/a/b.mp3", "6/8/a."] {
            assert!(!is_standard_rel(bad), "{bad}");
        }
        assert_eq!(split_name("a.b.mp3"), ("a.b", ".mp3"));
        assert_eq!(split_name(".hidden"), (".hidden", ""));
        assert_eq!(split_name("noext"), ("noext", ""));
        assert!(is_audio_name("X.MP3") && !is_audio_name("x.txt") && !is_audio_name("mp3"));
    }
}
