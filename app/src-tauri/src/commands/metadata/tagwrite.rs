//! Optional write-back of track metadata into the audio files' own tags (ID3v2, Vorbis comments,
//! MP4 atoms), so other software sees what the library shows. Off unless the user asks for it.
//!
//! Safety, in the order the code does it:
//! - the file is found from the track's stored `location` only (never from the caller) and must
//!   stay inside the volume's Music folder (`locate`, same checks as playback's `check_relative`);
//! - `plan_one` reads the file and works out what would change; the preview, the confirmation and
//!   the write all use it, so what is shown is what is done; a file whose tags already match is
//!   never rewritten, and an empty library value never erases a tag;
//! - the write copies the file beside itself, lets lofty write the tag into the copy, checks that
//!   the copy still parses as the same format with the same duration, sample rate and channels and
//!   reads back the new values, fsyncs, and renames it over the original (`rewrite_with`); any
//!   failure removes the copy and leaves the original untouched; audio frames are never edited;
//! - the job runs under the write gate (`spawn_write_job`), and the new file sizes are recorded
//!   in `tracks.file_size` afterwards.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};
use pm_formats::library::{Library, Track};
use pm_formats::naming;
use pm_formats::ops_metadata::{MAX_TEXT_CHARS, TrackEdit};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_write_job};
use crate::state::JobLock;

/// The job kind of the write (own string, not shared with another area).
pub const JOB_KIND: &str = "tag-write";

/// Fields that exist as tags in the audio formats, in the order the UI lists them.
pub const WRITABLE_FIELDS: [&str; 15] = [
    "title",
    "artist",
    "album_artist",
    "album",
    "composer",
    "genre",
    "label",
    "remixer",
    "producer",
    "comments",
    "year",
    "track_number",
    "number_of_tracks",
    "disc_number",
    "number_of_discs",
];

pub fn is_writable_field(f: &str) -> bool {
    WRITABLE_FIELDS.contains(&f)
}

/// Audio file extensions the tool will open, with the container and tag it writes.
pub const FORMATS: [(&str, &str, &str); 11] = [
    ("mp3", "MP3", "ID3v2"),
    ("flac", "FLAC", "Vorbis comments"),
    ("ogg", "Ogg Vorbis", "Vorbis comments"),
    ("oga", "Ogg Vorbis", "Vorbis comments"),
    ("opus", "Opus", "Vorbis comments"),
    ("m4a", "MP4 audio", "MP4 atoms"),
    ("mp4", "MP4 audio", "MP4 atoms"),
    ("aac", "AAC (ADTS)", "ID3v2"),
    ("wav", "WAV", "ID3v2 chunk"),
    ("aif", "AIFF", "ID3v2 chunk"),
    ("aiff", "AIFF", "ID3v2 chunk"),
];

#[derive(Debug, Clone, Serialize)]
pub struct FormatInfo {
    pub ext: String,
    pub container: String,
    pub tag: String,
}

/// One track's values to write: only the fields present are looked at.
#[derive(Debug, Clone, Deserialize)]
pub struct TagItem {
    pub track_id: i64,
    #[serde(flatten)]
    pub values: TrackEdit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagChange {
    pub field: String,
    /// What the file's tag holds now ("" = nothing).
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagPlan {
    pub track_id: i64,
    /// "Artist - Title" as the library has it now.
    pub label: String,
    /// The file's path below the Music folder, e.g. `3/a/1f2e.mp3`.
    pub file: String,
    /// "MP3 (ID3v2)", or "" when the file could not be opened.
    pub format: String,
    /// "write" (something differs), "same" (tags already match), "unsupported" (format),
    /// "missing" (no file) or "error".
    pub status: String,
    pub changes: Vec<TagChange>,
    /// Requested fields this format has no tag for.
    pub unsupported_fields: Vec<String>,
    pub message: String,
}

// ---- finding the file ----------------------------------------------------------------------

/// Every component after `/pmdb_tracks/` must be a plain name: `Path::join` replaces its base for
/// a rooted or drive-prefixed component, and `..` climbs out. Returns the relative path.
fn check_relative(location: &str) -> Result<&str, String> {
    let rel = location
        .strip_prefix(naming::LOCATION_PREFIX)
        .ok_or_else(|| format!("{location} is not a device-side track path"))?;
    let plain = !rel.is_empty()
        && !rel.contains([':', '\\', '\0'])
        && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
        && rel.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    if plain { Ok(rel) } else { Err(format!("{location} points outside the library")) }
}

/// The audio file of a track inside the volume's Music folder, or why it cannot be used.
fn locate(root: &Path, location: &str) -> Result<(PathBuf, String), (&'static str, String)> {
    let rel = check_relative(location).map_err(|m| ("error", m))?;
    let music = root.join(naming::MUSIC_DIR);
    let path = music.join(rel);
    let meta = std::fs::symlink_metadata(&path).map_err(|_| ("missing", "the audio file is not on the device".to_string()))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(("error", "the track's location is not a regular file".into()));
    }
    // belt and braces: whatever the name says, the real path must lie in the Music folder
    let inside = match (std::fs::canonicalize(&path), std::fs::canonicalize(&music)) {
        (Ok(p), Ok(m)) => p.starts_with(&m),
        _ => false,
    };
    if !inside {
        return Err(("error", "the track's location points outside the device".into()));
    }
    Ok((path, rel.to_string()))
}

fn format_of_ext(path: &Path) -> Option<&'static (&'static str, &'static str, &'static str)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    FORMATS.iter().find(|f| f.0 == ext)
}

// ---- reading and setting tag values --------------------------------------------------------

fn text_key(field: &str) -> Option<ItemKey> {
    Some(match field {
        "title" => ItemKey::TrackTitle,
        "artist" => ItemKey::TrackArtist,
        "album_artist" => ItemKey::AlbumArtist,
        "album" => ItemKey::AlbumTitle,
        "composer" => ItemKey::Composer,
        "genre" => ItemKey::Genre,
        "label" => ItemKey::Label,
        "remixer" => ItemKey::Remixer,
        "producer" => ItemKey::Producer,
        "comments" => ItemKey::Comment,
        _ => return None,
    })
}

fn number_in(tag: &Tag, field: &str) -> Option<u32> {
    match field {
        "year" => tag.date().map(|d| u32::from(d.year)),
        "track_number" => tag.track(),
        "number_of_tracks" => tag.track_total(),
        "disc_number" => tag.disk(),
        "number_of_discs" => tag.disk_total(),
        _ => None,
    }
}

/// What the tag holds for a field, as text ("" when absent).
fn value_in(tag: &Tag, field: &str) -> String {
    match text_key(field) {
        Some(k) => tag.get_string(k).unwrap_or("").to_string(),
        None => number_in(tag, field).map(|n| n.to_string()).unwrap_or_default(),
    }
}

/// The values the item asks for, in field order; empty text and 0 mean "not set" in the library
/// and are never written (they would erase a tag).
fn wanted(values: &TrackEdit) -> Vec<(&'static str, String)> {
    let text = [
        ("title", &values.title),
        ("artist", &values.artist),
        ("album_artist", &values.album_artist),
        ("album", &values.album),
        ("composer", &values.composer),
        ("genre", &values.genre),
        ("label", &values.label),
        ("remixer", &values.remixer),
        ("producer", &values.producer),
        ("comments", &values.comments),
    ];
    let numbers = [
        ("year", values.year),
        ("track_number", values.track_number),
        ("number_of_tracks", values.number_of_tracks),
        ("disc_number", values.disc_number),
        ("number_of_discs", values.number_of_discs),
    ];
    let mut out = Vec::new();
    for (f, v) in text {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty() && v.chars().count() <= MAX_TEXT_CHARS && !v.contains('\0')) {
            out.push((f, v.to_string()));
        }
    }
    for (f, v) in numbers {
        if let Some(n) = v.filter(|n| (1..=9999).contains(n)) {
            out.push((f, n.to_string()));
        }
    }
    out
}

fn set_value(tag: &mut Tag, field: &str, value: &str) {
    if let Some(k) = text_key(field) {
        tag.insert_text(k, value.to_string());
        return;
    }
    let Ok(n) = value.parse::<u32>() else { return };
    match field {
        "year" => {
            // keep a more exact date when only the year is being "changed" to itself (never here:
            // an equal year is not a change), otherwise a year-only date
            tag.remove_key(ItemKey::Year);
            tag.insert_text(ItemKey::RecordingDate, n.to_string());
        }
        "track_number" => tag.set_track(n),
        "number_of_tracks" => tag.set_track_total(n),
        "disc_number" => tag.set_disk(n),
        "number_of_discs" => tag.set_disk_total(n),
        _ => {}
    }
}

fn container_name(ft: &FileType) -> String {
    match ft {
        FileType::Mpeg => "MP3".into(),
        FileType::Flac => "FLAC".into(),
        FileType::Vorbis => "Ogg Vorbis".into(),
        FileType::Opus => "Opus".into(),
        FileType::Mp4 => "MP4 audio".into(),
        FileType::Aac => "AAC (ADTS)".into(),
        FileType::Wav => "WAV".into(),
        FileType::Aiff => "AIFF".into(),
        other => format!("{other:?}"),
    }
}

fn tag_name(t: TagType) -> String {
    match t {
        TagType::Id3v2 => "ID3v2".into(),
        TagType::VorbisComments => "Vorbis comments".into(),
        TagType::Mp4Ilst => "MP4 atoms".into(),
        other => format!("{other:?}"),
    }
}

/// What must stay the same across the rewrite: the audio itself is not touched, so its duration,
/// sample rate and channel count read back identically.
#[derive(Debug, Clone, PartialEq)]
struct AudioFacts {
    file_type: String,
    duration_ms: u128,
    sample_rate: Option<u32>,
    channels: Option<u8>,
}

fn facts_of(tf: &lofty::file::TaggedFile) -> AudioFacts {
    let p = tf.properties();
    AudioFacts {
        file_type: format!("{:?}", tf.file_type()),
        duration_ms: p.duration().as_millis(),
        sample_rate: p.sample_rate(),
        channels: p.channels(),
    }
}

/// A file read and a new tag worked out for it, not yet written.
struct Prepared {
    tag: Tag,
    file_type: FileType,
    facts: AudioFacts,
    format: String,
    changes: Vec<TagChange>,
    unsupported: Vec<String>,
    /// `(field, value)` the tag must read back after the write.
    expect: Vec<(String, String)>,
}

fn prepare(path: &Path, values: &TrackEdit) -> Result<Prepared, String> {
    let tf = Probe::open(path).map_err(|e| e.to_string())?.read().map_err(|e| e.to_string())?;
    let ft = tf.file_type();
    let ptype = ft.primary_tag_type();
    if !ft.tag_support(ptype).is_writable() {
        return Err(format!("{} tags cannot be written", container_name(&ft)));
    }
    let mut tag = tf.tag(ptype).cloned().unwrap_or_else(|| Tag::new(ptype));
    let mut changes = Vec::new();
    let mut unsupported = Vec::new();
    let mut expect = Vec::new();
    for (field, new) in wanted(values) {
        let old = value_in(&tag, field);
        if old == new {
            continue;
        }
        set_value(&mut tag, field, &new);
        if value_in(&tag, field) != new {
            // this tag format has no place for the field (or stores it differently)
            unsupported.push(field.to_string());
            continue;
        }
        expect.push((field.to_string(), new.clone()));
        changes.push(TagChange { field: field.to_string(), old, new });
    }
    Ok(Prepared {
        file_type: ft,
        facts: facts_of(&tf),
        format: format!("{} ({})", container_name(&ft), tag_name(ptype)),
        tag,
        changes,
        unsupported,
        expect,
    })
}

// ---- the atomic rewrite --------------------------------------------------------------------

fn temp_sibling(dest: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    dest.with_file_name(name)
}

/// Copies `dest` beside itself, lets `fill` change the copy, then renames the copy over `dest`.
/// On any failure the copy is removed and `dest` is untouched. Returns the new size.
fn rewrite_with(dest: &Path, fill: impl FnOnce(&Path, &mut File) -> Result<(), String>) -> Result<u64, String> {
    let tmp = temp_sibling(dest);
    let res = (|| -> Result<u64, String> {
        std::fs::copy(dest, &tmp).map_err(|e| format!("could not copy the file: {e}"))?;
        let mut f = OpenOptions::new().read(true).write(true).open(&tmp).map_err(|e| e.to_string())?;
        fill(&tmp, &mut f)?;
        f.flush().map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        let size = f.metadata().map_err(|e| e.to_string())?.len();
        drop(f);
        std::fs::rename(&tmp, dest).map_err(|e| format!("could not replace the file: {e}"))?;
        Ok(size)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Writes the prepared tag into the file; returns the file's new size in bytes.
fn write_prepared(path: &Path, p: &Prepared) -> Result<u64, String> {
    rewrite_with(path, |tmp, f| {
        p.tag.save_to(f, WriteOptions::default()).map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
        // the copy must still be the same audio with the new tags
        let again = Probe::open(tmp)
            .map_err(|e| e.to_string())?
            .set_file_type(p.file_type)
            .read().map_err(|e| format!("the rewritten file does not parse: {e}"))?;
        if facts_of(&again) != p.facts {
            return Err("the rewritten file reads differently (format, length, rate or channels); left unchanged".into());
        }
        let ptype = again.file_type().primary_tag_type();
        let back = again.tag(ptype).ok_or("the rewritten file has no tag")?;
        for (field, want) in &p.expect {
            if value_in(back, field) != *want {
                return Err(format!("{field} does not read back after the write; left unchanged"));
            }
        }
        Ok(())
    })
}

// ---- planning ------------------------------------------------------------------------------

fn label_of(t: &Track) -> String {
    match (t.artist.is_empty(), t.title.is_empty()) {
        (false, false) => format!("{} - {}", t.artist, t.title),
        (_, false) => t.title.clone(),
        _ => t.location.clone(),
    }
}

fn blank_plan(track_id: i64, label: String, status: &str, message: String) -> TagPlan {
    TagPlan {
        track_id,
        label,
        file: String::new(),
        format: String::new(),
        status: status.into(),
        changes: Vec::new(),
        unsupported_fields: Vec::new(),
        message,
    }
}

/// The plan for one item and, when something must be written, what to write. Never panics: a
/// parser panic on a damaged file becomes an "error" plan.
fn plan_one(root: &Path, tracks: &HashMap<i64, Track>, item: &TagItem) -> (TagPlan, Option<(PathBuf, Prepared)>) {
    let Some(track) = tracks.get(&item.track_id) else {
        return (blank_plan(item.track_id, format!("track {}", item.track_id), "error", "that track is not in the library".into()), None);
    };
    let label = label_of(track);
    let (path, rel) = match locate(root, &track.location) {
        Ok(x) => x,
        Err((status, msg)) => return (blank_plan(item.track_id, label, status, msg), None),
    };
    let mut plan = blank_plan(item.track_id, label, "error", String::new());
    plan.file = rel;
    let Some(fmt) = format_of_ext(&path) else {
        plan.status = "unsupported".into();
        plan.message = "tags of this file type are not written".into();
        return (plan, None);
    };
    plan.format = format!("{} ({})", fmt.1, fmt.2);
    match catch_unwind(AssertUnwindSafe(|| prepare(&path, &item.values))) {
        Ok(Ok(p)) => {
            plan.format = p.format.clone();
            plan.changes = p.changes.clone();
            plan.unsupported_fields = p.unsupported.clone();
            if p.changes.is_empty() {
                plan.status = "same".into();
                plan.message = if p.unsupported.is_empty() { "the tags already match".into() } else { "nothing this format can hold differs".into() };
                (plan, None)
            } else {
                plan.status = "write".into();
                (plan, Some((path, p)))
            }
        }
        Ok(Err(e)) => {
            plan.message = e;
            (plan, None)
        }
        Err(_) => {
            plan.message = "the tag reader crashed on this file".into();
            (plan, None)
        }
    }
}

fn tracks_by_id(lib: &Library) -> AppResult<HashMap<i64, Track>> {
    Ok(lib.tracks()?.into_iter().map(|t| (t.track_id, t)).collect())
}

/// What writing `items` would do to each file (read-only; opens every file to read its tags).
#[tauri::command]
pub async fn metadata_tags_plan(root: String, items: Vec<TagItem>) -> AppResult<Vec<TagPlan>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<TagPlan>> {
        let root = PathBuf::from(root);
        let lib = Library::open_read_only(&root)?;
        let tracks = tracks_by_id(&lib)?;
        Ok(items.iter().map(|i| plan_one(&root, &tracks, i).0).collect())
    })
    .await?
}

/// The file types whose tags can be written.
#[tauri::command]
pub async fn metadata_tags_formats() -> Vec<FormatInfo> {
    FORMATS.iter().map(|f| FormatInfo { ext: f.0.into(), container: f.1.into(), tag: f.2.into() }).collect()
}

/// The write itself, without the job plumbing: plans every item, rewrites the files that differ,
/// records their new sizes in the library and reports each file through `progress`. A file that
/// fails stays as it was and is counted as failed.
fn run_write(root: &Path, lib: &mut Library, items: &[TagItem], mut progress: impl FnMut(Progress)) -> Result<JobOutcome, String> {
    let tracks = tracks_by_id(lib).map_err(|e| e.to_string())?;
    let total = items.len();
    let (mut written, mut same, mut failed) = (0usize, 0usize, 0usize);
    let mut sizes: Vec<(i64, i64)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let (plan, work) = plan_one(root, &tracks, item);
        let mut ev = Progress::new(JOB_KIND, format!("{} ({})", plan.label, plan.file), i + 1, total, "written");
        match (plan.status.as_str(), work) {
            ("write", Some((path, prepared))) => match catch_unwind(AssertUnwindSafe(|| write_prepared(&path, &prepared))) {
                Ok(Ok(size)) => {
                    written += 1;
                    sizes.push((item.track_id, size as i64));
                }
                Ok(Err(e)) => {
                    failed += 1;
                    ev.status = "failed".into();
                    ev.error = Some(e);
                }
                Err(_) => {
                    failed += 1;
                    ev.status = "failed".into();
                    ev.error = Some("the tag writer crashed on this file; left unchanged".into());
                }
            },
            ("same", _) => {
                same += 1;
                ev.status = "skipped".into();
            }
            (_, _) => {
                failed += 1;
                ev.status = "failed".into();
                ev.error = Some(if plan.message.is_empty() { plan.status.clone() } else { plan.message.clone() });
            }
        }
        progress(ev);
    }
    if !sizes.is_empty() {
        lib.set_file_sizes(&sizes).map_err(|e| {
            format!("Tags were written to {written} files, but the library could not record their new sizes: {e}")
        })?;
    }
    let mut parts = vec![format!("{written} file{} tagged", if written == 1 { "" } else { "s" })];
    if same > 0 {
        parts.push(format!("{same} already matched"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed (left as they were)"));
    }
    Ok(JobOutcome { ok: written + same, failed, summary: parts.join(", ") })
}

/// Writes the values into the audio files' tags as a background job (kind `tag-write`), under the
/// write gate and the job lock. Files whose tags already match are skipped; a file that fails
/// stays as it was and is listed as failed.
#[tauri::command]
pub fn metadata_tags_write(app: tauri::AppHandle, lock: State<'_, JobLock>, root: String, items: Vec<TagItem>) -> AppResult<()> {
    if items.is_empty() {
        return Err("No files to write.".into());
    }
    spawn_write_job(app, &lock, JOB_KIND, root.clone(), move |app| {
        let root = PathBuf::from(root);
        let mut lib = Library::open(&root)?;
        Ok(run_write(&root, &mut lib, &items, |ev| events::emit(app, JOB_PROGRESS, ev))?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn scratch(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("pm-tagwrite-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 30 silent MPEG-1 Layer III frames (128 kbit/s, 44.1 kHz, 417 bytes each), no tag.
    fn mp3_bytes() -> Vec<u8> {
        let mut v = Vec::new();
        for _ in 0..30 {
            let mut frame = vec![0u8; 417];
            frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            v.extend(frame);
        }
        v
    }

    /// 1 s of 16-bit mono silence at 8 kHz.
    fn wav_bytes() -> Vec<u8> {
        let data = vec![0u8; 16000];
        let mut v = Vec::new();
        v.extend(b"RIFF");
        v.extend((36 + data.len() as u32).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes()); // PCM
        v.extend(1u16.to_le_bytes()); // mono
        v.extend(8000u32.to_le_bytes());
        v.extend(16000u32.to_le_bytes());
        v.extend(2u16.to_le_bytes());
        v.extend(16u16.to_le_bytes());
        v.extend(b"data");
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }

    /// A FLAC stream that is only a STREAMINFO block (44.1 kHz stereo 16-bit, 44100 samples).
    fn flac_bytes() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(b"fLaC");
        v.extend([0x80, 0, 0, 34]); // last block, STREAMINFO, 34 bytes
        v.extend(4096u16.to_be_bytes());
        v.extend(4096u16.to_be_bytes());
        v.extend([0, 0, 0]);
        v.extend([0, 0, 0]);
        let packed: u64 = (44_100u64 << 44) | (1u64 << 41) | (15u64 << 36) | 44_100u64;
        v.extend(packed.to_be_bytes());
        v.extend([0u8; 16]);
        v
    }

    fn edit(title: &str, artist: &str) -> TrackEdit {
        TrackEdit { title: Some(title.into()), artist: Some(artist.into()), ..Default::default() }
    }

    fn read_tag(path: &Path) -> Tag {
        let tf = Probe::open(path).unwrap().read().unwrap();
        tf.tag(tf.file_type().primary_tag_type()).cloned().unwrap()
    }

    #[test]
    fn relative_locations_must_stay_inside() {
        assert_eq!(check_relative("/pmdb_tracks/3/a/1f.mp3"), Ok("3/a/1f.mp3"));
        for bad in [
            "/pmdb_tracks/../x.mp3",
            "/pmdb_tracks/C:/x.mp3",
            "/pmdb_tracks//x.mp3",
            "/pmdb_tracks/a\\b.mp3",
            "/pmdb_tracks/",
            "/other/x.mp3",
            "x.mp3",
            "/pmdb_tracks/a/./b.mp3",
        ] {
            assert!(check_relative(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn locate_refuses_what_is_not_a_plain_file_in_the_music_folder() {
        let root = scratch("locate");
        let music = root.join(naming::MUSIC_DIR);
        std::fs::create_dir_all(music.join("3/a")).unwrap();
        std::fs::write(music.join("3/a/x.mp3"), mp3_bytes()).unwrap();
        std::fs::write(root.join("outside.mp3"), mp3_bytes()).unwrap();
        std::fs::create_dir_all(music.join("dir.mp3")).unwrap();
        assert!(locate(&root, "/pmdb_tracks/3/a/x.mp3").is_ok());
        assert_eq!(locate(&root, "/pmdb_tracks/3/a/none.mp3").unwrap_err().0, "missing");
        assert_eq!(locate(&root, "/pmdb_tracks/../../outside.mp3").unwrap_err().0, "error");
        assert_eq!(locate(&root, "/pmdb_tracks/dir.mp3").unwrap_err().0, "error", "a folder is not a file");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mp3_gets_its_tags_and_keeps_every_audio_byte() {
        let dir = scratch("mp3");
        let path = dir.join("a.mp3");
        let audio = mp3_bytes();
        std::fs::write(&path, &audio).unwrap();
        let values = TrackEdit {
            title: Some("Example Title".into()),
            artist: Some("Example Artist".into()),
            album: Some("Example Album".into()),
            genre: Some("Tech House".into()),
            remixer: Some("Example Remixer".into()),
            comments: Some("A comment".into()),
            year: Some(2019),
            track_number: Some(3),
            ..Default::default()
        };
        let p = prepare(&path, &values).unwrap();
        assert_eq!(p.format, "MP3 (ID3v2)");
        assert_eq!(p.changes.iter().map(|c| c.field.as_str()).collect::<Vec<_>>(), vec!["title", "artist", "album", "genre", "remixer", "comments", "year", "track_number"]);
        assert!(p.changes.iter().all(|c| c.old.is_empty()));
        let size = write_prepared(&path, &p).unwrap();
        let after = std::fs::read(&path).unwrap();
        assert_eq!(size as usize, after.len());
        assert!(after.len() > audio.len());
        assert!(after.ends_with(&audio), "the audio frames are byte-identical, only a tag was put in front");
        let tag = read_tag(&path);
        assert_eq!(tag.title().as_deref(), Some("Example Title"));
        assert_eq!(tag.artist().as_deref(), Some("Example Artist"));
        assert_eq!(tag.get_string(ItemKey::Remixer), Some("Example Remixer"));
        assert_eq!(tag.get_string(ItemKey::Comment), Some("A comment"));
        assert_eq!(tag.track(), Some(3));
        assert_eq!(number_in(&tag, "year"), Some(2019));
        // a second plan with the same values has nothing to do, so nothing is rewritten
        let again = prepare(&path, &values).unwrap();
        assert!(again.changes.is_empty(), "{:?}", again.changes);
        // one value changes: only that field is in the plan and the other tags survive
        let p2 = prepare(&path, &TrackEdit { genre: Some("Deep House".into()), title: Some("Example Title".into()), ..Default::default() }).unwrap();
        assert_eq!(p2.changes, vec![TagChange { field: "genre".into(), old: "Tech House".into(), new: "Deep House".into() }]);
        write_prepared(&path, &p2).unwrap();
        let tag = read_tag(&path);
        assert_eq!(tag.get_string(ItemKey::Genre), Some("Deep House"));
        assert_eq!(tag.album().as_deref(), Some("Example Album"));
        assert!(std::fs::read(&path).unwrap().ends_with(&audio));
        // no temp file is left behind
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["a.mp3".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn empty_and_zero_values_never_erase_a_tag() {
        let dir = scratch("empty");
        let path = dir.join("a.mp3");
        std::fs::write(&path, mp3_bytes()).unwrap();
        let p = prepare(&path, &edit("Kept Title", "Kept Artist")).unwrap();
        write_prepared(&path, &p).unwrap();
        let blank = TrackEdit {
            title: Some(String::new()),
            artist: Some(String::new()),
            year: Some(0),
            track_number: Some(0),
            ..Default::default()
        };
        assert!(prepare(&path, &blank).unwrap().changes.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wav_audio_chunk_is_untouched() {
        let dir = scratch("wav");
        let path = dir.join("a.wav");
        let wav = wav_bytes();
        std::fs::write(&path, &wav).unwrap();
        let p = prepare(&path, &edit("Example Title", "Example Artist")).unwrap();
        assert!(p.format.starts_with("WAV"));
        write_prepared(&path, &p).unwrap();
        let after = std::fs::read(&path).unwrap();
        let data_at = |b: &[u8]| b.windows(4).position(|w| w == b"data").unwrap();
        let (a, b) = (data_at(&wav), data_at(&after));
        assert_eq!(&wav[a + 8..a + 8 + 16000], &after[b + 8..b + 8 + 16000], "the sample data is identical");
        assert_eq!(read_tag(&path).title().as_deref(), Some("Example Title"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flac_gets_vorbis_comments() {
        let dir = scratch("flac");
        let path = dir.join("a.flac");
        let flac = flac_bytes();
        std::fs::write(&path, &flac).unwrap();
        let values = TrackEdit {
            title: Some("Example Title".into()),
            label: Some("Example Label".into()),
            year: Some(2001),
            disc_number: Some(2),
            ..Default::default()
        };
        let p = prepare(&path, &values).unwrap();
        assert_eq!(p.format, "FLAC (Vorbis comments)");
        write_prepared(&path, &p).unwrap();
        let after = std::fs::read(&path).unwrap();
        assert!(after.starts_with(b"fLaC"));
        let tag = read_tag(&path);
        assert_eq!(tag.title().as_deref(), Some("Example Title"));
        assert_eq!(tag.get_string(ItemKey::Label), Some("Example Label"));
        assert_eq!(number_in(&tag, "year"), Some(2001));
        assert_eq!(tag.disk(), Some(2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_write_leaves_the_original_and_no_temp() {
        let dir = scratch("fail");
        let path = dir.join("a.mp3");
        let audio = mp3_bytes();
        std::fs::write(&path, &audio).unwrap();
        // the fill step fails after the copy exists
        let err = rewrite_with(&path, |tmp, _f| {
            assert!(tmp.exists());
            Err("boom".to_string())
        });
        assert_eq!(err, Err("boom".to_string()));
        assert_eq!(std::fs::read(&path).unwrap(), audio);
        // a file that is not audio at all fails the verification and is left alone
        let junk = dir.join("junk.mp3");
        std::fs::write(&junk, b"this is not audio").unwrap();
        assert!(prepare(&junk, &edit("T", "A")).is_err());
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.iter().all(|n| !n.ends_with(".tmp")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extensions_outside_the_supported_list_are_not_opened() {
        assert!(format_of_ext(Path::new("a.MP3")).is_some());
        assert!(format_of_ext(Path::new("a.wma")).is_none());
        assert!(format_of_ext(Path::new("a")).is_none());
    }

    #[test]
    fn wanted_skips_blanks_and_over_long_text() {
        let long = "x".repeat(300);
        let v = TrackEdit { title: Some("T".into()), artist: Some(String::new()), album: Some(long), year: Some(0), disc_number: Some(2), ..Default::default() };
        assert_eq!(wanted(&v), vec![("title", "T".to_string()), ("disc_number", "2".to_string())]);
    }

    /// A card with the given files added as tracks (title = file name, artist "Example Artist").
    fn card(tag: &str, files: &[(&str, Vec<u8>)]) -> (PathBuf, Library, Vec<i64>) {
        use pm_formats::library::TrackMeta;
        use pm_formats::strfile;
        let base = scratch(tag);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (name, bytes) in files {
            let p = src.join(name);
            std::fs::write(&p, bytes).unwrap();
            let meta = TrackMeta { title: (*name).into(), artist: "Example Artist".into(), format: "MP3".into(), play_time_secs: 1, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        (base, lib, ids)
    }

    #[test]
    fn plan_reports_missing_unsupported_and_same() {
        let (base, lib, ids) = card("plan", &[("one.mp3", mp3_bytes()), ("two.wma", vec![1u8; 500]), ("three.mp3", mp3_bytes())]);
        let vol = base.join("vol");
        let tracks = tracks_by_id(&lib).unwrap();
        // remove the third file from the card
        let third = tracks[&ids[2]].location.clone();
        std::fs::remove_file(vol.join(naming::MUSIC_DIR).join(third.strip_prefix(naming::LOCATION_PREFIX).unwrap())).unwrap();
        let mk = |id: i64| TagItem { track_id: id, values: edit("Example Title", "Example Artist") };
        let (p1, work) = plan_one(&vol, &tracks, &mk(ids[0]));
        assert_eq!((p1.status.as_str(), p1.changes.len()), ("write", 2), "{p1:?}"); // the file has no tag yet
        assert_eq!(p1.label, "Example Artist - one.mp3");
        assert!(p1.file.ends_with(".mp3") && !p1.file.starts_with('/'));
        let (path, prepared) = work.unwrap();
        write_prepared(&path, &prepared).unwrap();
        let (again, work) = plan_one(&vol, &tracks, &mk(ids[0]));
        assert_eq!(again.status, "same");
        assert!(work.is_none());
        assert_eq!(plan_one(&vol, &tracks, &mk(ids[1])).0.status, "unsupported");
        assert_eq!(plan_one(&vol, &tracks, &mk(ids[2])).0.status, "missing");
        assert_eq!(plan_one(&vol, &tracks, &mk(999)).0.status, "error");
        drop(lib); // Windows cannot delete the folder while music.db is open
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_write_records_new_sizes_counts_outcomes_and_reports_each_file() {
        let (base, mut lib, ids) = card(
            "run",
            &[("one.mp3", mp3_bytes()), ("two.mp3", mp3_bytes()), ("three.wma", vec![1u8; 500]), ("four.mp3", vec![9u8; 100])],
        );
        let vol = base.join("vol");
        let tracks = tracks_by_id(&lib).unwrap();
        let path_of = |id: i64| vol.join(naming::MUSIC_DIR).join(tracks[&id].location.strip_prefix(naming::LOCATION_PREFIX).unwrap());
        let before = std::fs::metadata(path_of(ids[0])).unwrap().len();
        // the second file already has these tags
        let (p2, w2) = plan_one(&vol, &tracks, &TagItem { track_id: ids[1], values: edit("Same Title", "Example Artist") });
        assert_eq!(p2.status, "write");
        let (path2, prepared2) = w2.unwrap();
        write_prepared(&path2, &prepared2).unwrap();
        let items: Vec<TagItem> = vec![
            TagItem { track_id: ids[0], values: edit("New Title", "New Artist") },
            TagItem { track_id: ids[1], values: edit("Same Title", "Example Artist") },
            TagItem { track_id: ids[2], values: edit("T", "A") },  // unsupported type
            TagItem { track_id: ids[3], values: edit("T", "A") },  // not audio
        ];
        let mut events = Vec::new();
        let out = run_write(&vol, &mut lib, &items, |ev| events.push((ev.status.clone(), ev.done, ev.total))).unwrap();
        assert_eq!((out.ok, out.failed), (2, 2), "{}", out.summary);
        assert_eq!(out.summary, "1 file tagged, 1 already matched, 2 failed (left as they were)");
        assert_eq!(events, vec![("written".into(), 1, 4), ("skipped".into(), 2, 4), ("failed".into(), 3, 4), ("failed".into(), 4, 4)]);
        let after = std::fs::metadata(path_of(ids[0])).unwrap().len();
        assert!(after > before);
        let recorded = lib.tracks().unwrap().into_iter().find(|t| t.track_id == ids[0]).unwrap().file_size;
        assert_eq!(recorded as u64, after, "tracks.file_size follows the rewritten file");
        // the failed files are exactly as they were
        assert_eq!(std::fs::read(path_of(ids[3])).unwrap(), vec![9u8; 100]);
        assert_eq!(std::fs::read(path_of(ids[2])).unwrap(), vec![1u8; 500]);
        drop(lib);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
