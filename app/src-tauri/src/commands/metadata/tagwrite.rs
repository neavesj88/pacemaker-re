//! Optional write-back of track metadata into the audio files' own tags (ID3v2, Vorbis comments,
//! MP4 atoms), so other software sees what the library shows. Off unless the user asks for it.
//!
//! Safety, in the order the code does it:
//! - the file is found from the track's stored `location` only (never from the caller) and must
//!   stay inside the volume's Music folder (`locate`, same checks as playback's `check_relative`);
//! - `plan_one` reads the file and works out what would change; the preview, the confirmation and
//!   the write all use it, so what is shown is what is done; a file whose tags already match is
//!   never rewritten, and an empty library value never erases a tag;
//! - only the requested fields are touched. For the ID3-based formats (MP3, AAC, WAV, AIFF) the
//!   ID3v2 tag is edited frame by frame (`Id3v2Tag`, not lofty's generic `Tag`, which drops what
//!   it cannot express): a comment is written to the COMM frame without a description only, so
//!   iTunes' iTunNORM / iTunSMPB frames stay; POPM ratings, PRIV, TXXX, pictures and every other
//!   frame are kept as they are; an ID3v2.3 tag stays 2.3 (an ID3v2.2 tag is upgraded to 2.4, which
//!   the plan says). The other formats go through the generic tag, with a check that no item or
//!   picture outside the requested fields changed;
//! - the write copies the file beside itself, lets lofty write the tag into the copy, then checks
//!   the copy: same format, duration, sample rate and channels, the new values read back, every
//!   frame / item that was not requested unchanged, and the audio payload byte for byte the same
//!   (`audiohash`: a fingerprint of everything but the tags, taken before and after). It is fsynced
//!   and renamed over the original only if all of that holds; any failure removes the copy and
//!   leaves the original untouched; audio frames are never edited;
//! - the job runs under the write gate (`spawn_write_job`), the folders of the files are swept of
//!   temp copies that an earlier interrupted write left behind, and the new file sizes are recorded
//!   in `tracks.file_size` afterwards.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use lofty::TextEncoding;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::id3::v2::{CommentFrame, Frame, FrameId, Id3v2Tag, Id3v2Version, KeyValueFrame, TextInformationFrame, TimestampFrame};
use lofty::probe::Probe;
use lofty::tag::items::{Timestamp, UNKNOWN_LANGUAGE};
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};
use pm_formats::library::{Library, Track};
use pm_formats::naming;
use pm_formats::ops_metadata::{MAX_TEXT_CHARS, TrackEdit};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tauri::State;

use super::audiohash;
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

/// Audio file extensions the tool will open, with the container and tag it writes. Every one of
/// them has a fixture test that writes a tag and proves the audio payload identical
/// (`every_format_keeps_its_audio` below); an extension without one does not belong in this list.
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
    /// Things about this file the user should know before agreeing (a tag version upgrade).
    pub notes: Vec<String>,
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

// ---- reading and setting tag values: the generic tag (FLAC, Ogg, Opus, MP4) -----------------

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

/// The generic items a field's write can touch: everything else must come back unchanged.
fn item_keys(field: &str) -> Vec<ItemKey> {
    if let Some(k) = text_key(field) {
        return vec![k];
    }
    match field {
        "year" => vec![ItemKey::Year, ItemKey::RecordingDate],
        "track_number" => vec![ItemKey::TrackNumber],
        "number_of_tracks" => vec![ItemKey::TrackTotal],
        "disc_number" => vec![ItemKey::DiscNumber],
        "number_of_discs" => vec![ItemKey::DiscTotal],
        _ => Vec::new(),
    }
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

/// What the generic items outside `touched` look like, as comparable text; with the pictures'
/// hashed in full, since a rewrite must not lose or alter the cover art.
fn untouched_items(tag: &Tag, touched: &[ItemKey]) -> Vec<String> {
    let mut v: Vec<String> = tag.items().filter(|i| !touched.contains(&i.key())).map(|i| format!("{i:?}")).collect();
    v.extend(tag.pictures().iter().map(|p| format!("picture {:?} {:?} {} {:x}", p.pic_type(), p.mime_type(), p.data().len(), sha2::Sha256::digest(p.data()))));
    v.sort();
    v
}

// ---- reading and setting tag values: ID3v2 frames (MP3, AAC, WAV, AIFF) --------------------

fn is_id3_based(ft: FileType) -> bool {
    matches!(ft, FileType::Mpeg | FileType::Aac | FileType::Wav | FileType::Aiff)
}

fn read_id3(path: &Path, ft: FileType) -> Result<Option<Id3v2Tag>, String> {
    use lofty::aac::AacFile;
    use lofty::iff::aiff::AiffFile;
    use lofty::iff::wav::WavFile;
    use lofty::mpeg::MpegFile;
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let o = ParseOptions::new();
    let tag = match ft {
        FileType::Mpeg => MpegFile::read_from(&mut f, o).map_err(|e| e.to_string())?.id3v2().cloned(),
        FileType::Aac => AacFile::read_from(&mut f, o).map_err(|e| e.to_string())?.id3v2().cloned(),
        FileType::Wav => WavFile::read_from(&mut f, o).map_err(|e| e.to_string())?.id3v2().cloned(),
        FileType::Aiff => AiffFile::read_from(&mut f, o).map_err(|e| e.to_string())?.id3v2().cloned(),
        other => return Err(format!("{other:?} has no ID3v2 tag")),
    };
    Ok(tag)
}

/// The text frame a field is stored in (the ones that are not plain Accessor fields).
fn id3_text_frame(field: &str) -> Option<&'static str> {
    Some(match field {
        "album_artist" => "TPE2",
        "composer" => "TCOM",
        "label" => "TPUB",
        "remixer" => "TPE4",
        _ => return None,
    })
}

/// The frame id a field's write replaces (COMM means the one without a description).
fn id3_touched(field: &str) -> &'static str {
    match field {
        "title" => "TIT2",
        "artist" => "TPE1",
        "album" => "TALB",
        "genre" => "TCON",
        "comments" => "COMM",
        "year" => "TDRC",
        "track_number" | "number_of_tracks" => "TRCK",
        "disc_number" | "number_of_discs" => "TPOS",
        "producer" => "TIPL",
        other => id3_text_frame(other).unwrap_or(""),
    }
}

const TIPL: FrameId<'static> = FrameId::Valid(Cow::Borrowed("TIPL"));

fn producer_in(tag: &Id3v2Tag) -> String {
    match tag.get(&TIPL) {
        Some(Frame::KeyValue(kv)) => kv.key_value_pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case("producer")).map(|(_, v)| v.to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// The "involved people" entries other than the producer (they must survive a producer write).
fn other_people(tag: &Id3v2Tag) -> Vec<(String, String)> {
    match tag.get(&TIPL) {
        Some(Frame::KeyValue(kv)) => kv.key_value_pairs.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("producer")).map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        _ => Vec::new(),
    }
}

fn id3_value(tag: &Id3v2Tag, field: &str) -> String {
    let n = |v: Option<u32>| v.map(|n| n.to_string()).unwrap_or_default();
    match field {
        "title" => tag.title().map(|v| v.into_owned()).unwrap_or_default(),
        "artist" => tag.artist().map(|v| v.into_owned()).unwrap_or_default(),
        "album" => tag.album().map(|v| v.into_owned()).unwrap_or_default(),
        "genre" => tag.genre().map(|v| v.into_owned()).unwrap_or_default(),
        // the COMM frame without a description only: iTunNORM and friends are not "the comment"
        "comments" => tag.comments().next().map(|c| c.content.to_string()).unwrap_or_default(),
        "producer" => producer_in(tag),
        "year" => n(tag.date().map(|d| u32::from(d.year))),
        "track_number" => n(tag.track()),
        "number_of_tracks" => n(tag.track_total()),
        "disc_number" => n(tag.disk()),
        "number_of_discs" => n(tag.disk_total()),
        other => id3_text_frame(other)
            .and_then(|id| tag.get_text(&FrameId::Valid(Cow::Borrowed(id))))
            .map(|v| v.replace('\0', "/"))
            .unwrap_or_default(),
    }
}

fn text_frame(id: &'static str, value: &str) -> Frame<'static> {
    Frame::Text(TextInformationFrame::new(FrameId::Valid(Cow::Borrowed(id)), TextEncoding::UTF8, value.to_string()))
}

/// Replaces the content of the first COMM frame without a description; every other COMM frame is
/// left alone. The frame is re-made as UTF-8 so a Latin-1 frame can hold the new text.
fn set_comment(tag: &mut Id3v2Tag, value: &str) {
    let mut done = false;
    tag.retain_mut(|f| {
        if let Frame::Comment(c) = f {
            if !done && c.description.is_empty() {
                *c = CommentFrame::new(TextEncoding::UTF8, c.language, Cow::Borrowed(""), value.to_string());
                done = true;
            }
        }
        true
    });
    if !done {
        tag.insert(Frame::Comment(CommentFrame::new(TextEncoding::UTF8, UNKNOWN_LANGUAGE, Cow::Borrowed(""), value.to_string())));
    }
}

fn set_producer(tag: &mut Id3v2Tag, value: &str) {
    let mut pairs: Vec<(Cow<'static, str>, Cow<'static, str>)> = match tag.get(&TIPL) {
        Some(Frame::KeyValue(kv)) => kv.key_value_pairs.iter().map(|(k, v)| (Cow::Owned(k.to_string()), Cow::Owned(v.to_string()))).collect(),
        _ => Vec::new(),
    };
    pairs.retain(|(k, _)| !k.eq_ignore_ascii_case("producer"));
    pairs.push((Cow::Borrowed("producer"), Cow::Owned(value.to_string())));
    // a KeyValueFrame compares equal only to an identical one, so the old frame is removed first
    tag.remove(&TIPL).for_each(drop);
    tag.insert(Frame::KeyValue(KeyValueFrame::new(TIPL, TextEncoding::UTF8, pairs)));
}

fn id3_set(tag: &mut Id3v2Tag, field: &str, value: &str) {
    match field {
        "title" => tag.set_title(value.to_string()),
        "artist" => tag.set_artist(value.to_string()),
        "album" => tag.set_album(value.to_string()),
        "genre" => tag.set_genre(value.to_string()),
        "comments" => set_comment(tag, value),
        "producer" => set_producer(tag, value),
        other => {
            if let Some(id) = id3_text_frame(other) {
                tag.insert(text_frame(id, value));
                return;
            }
            let Ok(n) = value.parse::<u32>() else { return };
            match other {
                "year" => {
                    // a Timestamp frame (what a parsed TDRC is), not the text frame `set_date` makes
                    let stamp = Timestamp { year: n.min(9999) as u16, ..Timestamp::default() };
                    tag.insert(Frame::Timestamp(TimestampFrame::new(FrameId::Valid(Cow::Borrowed("TDRC")), TextEncoding::UTF8, stamp)));
                }
                "track_number" => tag.set_track(n),
                "number_of_tracks" => tag.set_track_total(n),
                "disc_number" => tag.set_disk(n),
                "number_of_discs" => tag.set_disk_total(n),
                _ => {}
            }
        }
    }
}

/// Is this frame one the write replaces on purpose?
fn is_touched(frame: &Frame<'_>, touched: &[&'static str]) -> bool {
    match frame {
        Frame::Comment(c) => touched.contains(&"COMM") && c.description.is_empty(),
        f => touched.contains(&f.id_str()),
    }
}

/// Checks a tag read back from the rewritten file against the tag that was written: every frame
/// that was not requested is there, identical, and the version is the one the file had. Returns why not.
fn check_id3_untouched(written: &Id3v2Tag, back: &Id3v2Tag, touched: &[&'static str]) -> Result<(), String> {
    // an ID3v2.2 tag is written as 2.4 (lofty cannot write 2.2); 2.3 and 2.4 stay as they were
    let want = match written.original_version() {
        Id3v2Version::V2 => Id3v2Version::V4,
        v => v,
    };
    if back.original_version() != want {
        return Err(format!("the tag version changed from {want} to {}", back.original_version()));
    }
    let mut pool: Vec<String> = back.iter().map(|f| format!("{f:?}")).collect();
    for f in written.iter().filter(|f| !is_touched(f, touched)) {
        let want = format!("{f:?}");
        match pool.iter().position(|g| *g == want) {
            Some(i) => {
                pool.swap_remove(i);
            }
            None => return Err(format!("the {} frame did not survive the rewrite unchanged", f.id_str())),
        }
    }
    if touched.contains(&"TIPL") && other_people(written) != other_people(back) {
        return Err("the other credits of the involved-people frame did not survive".into());
    }
    Ok(())
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
/// sample rate and channel count read back identically (and `audiohash` proves the payload).
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
        // lofty estimates the length of an ADTS stream from the file size, tag included, so it
        // moves with the tag; for AAC the payload fingerprint is what proves the audio
        duration_ms: if tf.file_type() == FileType::Aac { 0 } else { p.duration().as_millis() },
        sample_rate: p.sample_rate(),
        channels: p.channels(),
    }
}

/// The tag as it will be written, and what to check about it afterwards.
enum Edit {
    /// MP3, AAC, WAV, AIFF: the ID3v2 tag, edited frame by frame. `touched` are the frame ids the
    /// requested fields replace; every other frame must come back identical.
    Id3 { tag: Id3v2Tag, touched: Vec<&'static str> },
    /// FLAC, Ogg, Opus, MP4: the generic tag. `touched` are the items the requested fields replace.
    Generic { tag: Tag, before: Tag, touched: Vec<ItemKey> },
}

/// A file read and a new tag worked out for it, not yet written.
struct Prepared {
    edit: Edit,
    file_type: FileType,
    facts: AudioFacts,
    format: String,
    changes: Vec<TagChange>,
    unsupported: Vec<String>,
    notes: Vec<String>,
    /// `(field, value)` the tag must read back after the write.
    expect: Vec<(String, String)>,
}

fn prepare(path: &Path, values: &TrackEdit) -> Result<Prepared, String> {
    let tf = Probe::open(path).map_err(|e| e.to_string())?.read().map_err(|e| e.to_string())?;
    let ft = tf.file_type();
    // the writer only goes where the fingerprint can follow
    if !matches!(ft, FileType::Mpeg | FileType::Aac | FileType::Wav | FileType::Aiff | FileType::Flac | FileType::Vorbis | FileType::Opus | FileType::Mp4) {
        return Err(format!("{} tags are not written", container_name(&ft)));
    }
    let ptype = ft.primary_tag_type();
    if !ft.tag_support(ptype).is_writable() {
        return Err(format!("{} tags cannot be written", container_name(&ft)));
    }
    let facts = facts_of(&tf);
    let format = format!("{} ({})", container_name(&ft), tag_name(ptype));
    let mut changes = Vec::new();
    let mut unsupported = Vec::new();
    let mut notes = Vec::new();
    let mut expect = Vec::new();
    let edit = if is_id3_based(ft) {
        let mut tag = read_id3(path, ft)?.unwrap_or_default();
        if tag.original_version() == Id3v2Version::V2 {
            notes.push("its ID3v2.2 tag is upgraded to ID3v2.4 (all frames kept)".to_string());
        }
        let mut touched = Vec::new();
        for (field, new) in wanted(values) {
            let old = id3_value(&tag, field);
            if old == new {
                continue;
            }
            let mut trial = tag.clone();
            id3_set(&mut trial, field, &new);
            if id3_value(&trial, field) != new {
                // this tag has no place for the field (or stores it differently)
                unsupported.push(field.to_string());
                continue;
            }
            tag = trial;
            touched.push(id3_touched(field));
            expect.push((field.to_string(), new.clone()));
            changes.push(TagChange { field: field.to_string(), old, new });
        }
        Edit::Id3 { tag, touched }
    } else {
        let before = tf.tag(ptype).cloned().unwrap_or_else(|| Tag::new(ptype));
        let mut tag = before.clone();
        let mut touched = Vec::new();
        for (field, new) in wanted(values) {
            let old = value_in(&tag, field);
            if old == new {
                continue;
            }
            let mut trial = tag.clone();
            set_value(&mut trial, field, &new);
            if value_in(&trial, field) != new {
                unsupported.push(field.to_string());
                continue;
            }
            tag = trial;
            touched.extend(item_keys(field));
            expect.push((field.to_string(), new.clone()));
            changes.push(TagChange { field: field.to_string(), old, new });
        }
        Edit::Generic { tag, before, touched }
    };
    Ok(Prepared { edit, file_type: ft, facts, format, changes, unsupported, notes, expect })
}

// ---- the atomic rewrite --------------------------------------------------------------------

fn temp_sibling(dest: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    dest.with_file_name(name)
}

/// A temp copy this old belongs to a write that never finished (a crash or an unplug between the
/// copy and the rename); a live write renames its copy within seconds. Same rule as the library's
/// own temp files (`remove_stale_temps` in pm-formats `library.rs`).
const STALE_TEMP_AGE: Duration = Duration::from_secs(600);

/// Is this file name `<audio file name>.<pid>-<n>.tmp`, the copy `temp_sibling` makes?
fn is_tag_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_suffix(".tmp") else { return false };
    let Some((audio, mid)) = rest.rsplit_once('.') else { return false };
    let Some((pid, n)) = mid.split_once('-') else { return false };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(pid) && digits(n) && format_of_ext(Path::new(audio)).is_some()
}

/// Removes the temp copies in `dir` that an interrupted write left (older than `min_age`).
/// Returns how many it removed.
fn sweep_stale_temps(dir: &Path, min_age: Duration) -> usize {
    let mut removed = 0;
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !is_tag_temp_name(&name) || !e.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= min_age);
        if old && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Makes the rename itself durable. Unix only: std cannot open a directory for syncing on
/// Windows, where NTFS journals the rename.
#[cfg(unix)]
fn sync_parent(path: &Path) {
    if let Some(dir) = path.parent() {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) {}

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
        sync_parent(dest);
        Ok(size)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Writes the prepared tag into the file; returns the file's new size in bytes. The copy is
/// swapped in only after it passed every check; see the module comment.
fn write_prepared(path: &Path, p: &Prepared) -> Result<u64, String> {
    let before = audiohash::fingerprint(path, p.file_type)
        .map_err(|e| format!("the audio of this file cannot be checked ({e}); left unchanged"))?;
    rewrite_with(path, |tmp, f| {
        match &p.edit {
            Edit::Id3 { tag, .. } => {
                let mut o = WriteOptions::default();
                // an ID3v2.3 file stays 2.3; UTF-8 text is converted to what 2.3 allows by lofty.
                // Text that the frame's encoding cannot hold is an error, never a silent "?".
                o.use_id3v23(tag.original_version() == Id3v2Version::V3);
                o.lossy_text_encoding(false);
                tag.save_to(f, o).map_err(|e| e.to_string())?;
            }
            Edit::Generic { tag, .. } => tag.save_to(f, WriteOptions::default()).map_err(|e| e.to_string())?,
        }
        f.flush().map_err(|e| e.to_string())?;
        // the copy must still be the same audio with the new tags
        let again = Probe::open(tmp)
            .map_err(|e| e.to_string())?
            .set_file_type(p.file_type)
            .read().map_err(|e| format!("the rewritten file does not parse: {e}"))?;
        let now = facts_of(&again);
        if now != p.facts {
            return Err(format!("the rewritten file reads differently (was {:?}, now {:?}); left unchanged", p.facts, now));
        }
        match &p.edit {
            Edit::Id3 { tag, touched } => {
                let back = read_id3(tmp, p.file_type)?.ok_or("the rewritten file has no tag")?;
                for (field, want) in &p.expect {
                    if id3_value(&back, field) != *want {
                        return Err(format!("{field} does not read back after the write; left unchanged"));
                    }
                }
                check_id3_untouched(tag, &back, touched).map_err(|e| format!("{e}; left unchanged"))?;
            }
            Edit::Generic { before, touched, .. } => {
                let back = again.tag(p.file_type.primary_tag_type()).ok_or("the rewritten file has no tag")?;
                for (field, want) in &p.expect {
                    if value_in(back, field) != *want {
                        return Err(format!("{field} does not read back after the write; left unchanged"));
                    }
                }
                if untouched_items(before, touched) != untouched_items(back, touched) {
                    return Err("a tag item that was not requested changed in the rewrite; left unchanged".into());
                }
            }
        }
        let after = audiohash::fingerprint(tmp, p.file_type).map_err(|e| format!("the audio of the rewritten file cannot be checked ({e}); left unchanged"))?;
        if after != before {
            return Err("the audio payload differs after the rewrite; left unchanged".into());
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
        notes: Vec::new(),
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
            plan.notes = p.notes.clone();
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

/// The write itself, without the job plumbing: sweeps the folders of the files of temp copies an
/// interrupted write left, plans every item, rewrites the files that differ, records their new
/// sizes in the library and reports each file through `progress`. A file that fails stays as it
/// was and is counted as failed.
fn run_write(root: &Path, lib: &mut Library, items: &[TagItem], mut progress: impl FnMut(Progress)) -> Result<JobOutcome, String> {
    let tracks = tracks_by_id(lib).map_err(|e| e.to_string())?;
    let dirs: BTreeSet<PathBuf> = items
        .iter()
        .filter_map(|i| tracks.get(&i.track_id))
        .filter_map(|t| locate(root, &t.location).ok())
        .filter_map(|(p, _)| p.parent().map(Path::to_path_buf))
        .collect();
    let swept: usize = dirs.iter().map(|d| sweep_stale_temps(d, STALE_TEMP_AGE)).sum();
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
    if swept > 0 {
        parts.push(format!("{swept} leftover temp cop{} of an interrupted write removed", if swept == 1 { "y" } else { "ies" }));
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

    #[test]
    fn cover_art_change_deep_in_the_image_is_noticed() {
        use lofty::picture::{MimeType, Picture, PictureType};
        let make = |last: u8| {
            let mut data = vec![7u8; 4096];
            data[4000] = last;
            let mut tag = Tag::new(TagType::Id3v2);
            tag.push_picture(Picture::unchecked(data).pic_type(PictureType::CoverFront).mime_type(MimeType::Png).build());
            tag
        };
        assert_eq!(untouched_items(&make(1), &[]), untouched_items(&make(1), &[]));
        assert_ne!(untouched_items(&make(1), &[]), untouched_items(&make(2), &[]));
    }

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

    // ---- the ID3 frames the write must leave alone ----

    use lofty::id3::v2::{ExtendedTextFrame, PopularimeterFrame, PrivateFrame};

    /// An MP3 made of `audio` behind an ID3v2 tag with these frames (2.3 when `v23`).
    fn id3_file(audio: &[u8], frames: Vec<Frame<'static>>, v23: bool) -> Vec<u8> {
        let dir = scratch("build");
        let p = dir.join("b.mp3");
        std::fs::write(&p, audio).unwrap();
        let mut tag = Id3v2Tag::new();
        for f in frames {
            tag.insert(f);
        }
        let mut f = OpenOptions::new().read(true).write(true).open(&p).unwrap();
        let mut o = WriteOptions::default();
        o.use_id3v23(v23);
        tag.save_to(&mut f, o).unwrap();
        drop(f);
        let bytes = std::fs::read(&p).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        bytes
    }

    fn comm(desc: &str, text: &str) -> Frame<'static> {
        Frame::Comment(CommentFrame::new(TextEncoding::UTF8, *b"eng", desc.to_string(), text.to_string()))
    }

    fn rich_frames() -> Vec<Frame<'static>> {
        vec![
            text_frame("TIT2", "Old Title"),
            text_frame("TPE1", "Old Artist"),
            comm("iTunNORM", " 00000000 00000000 00000000 00000000"),
            comm("iTunSMPB", " 00000000 00000210 00000A2C"),
            comm("", "old comment"),
            Frame::Private(PrivateFrame::new("owner@example.test", vec![1u8, 2, 3, 250])),
            Frame::UserText(ExtendedTextFrame::new(TextEncoding::UTF8, "MY_TAG", "kept value")),
            Frame::Popularimeter(PopularimeterFrame::new("user@example.test", 200, 7)),
            Frame::KeyValue(KeyValueFrame::new(
                TIPL,
                TextEncoding::UTF8,
                vec![(Cow::Borrowed("arranger"), Cow::Borrowed("Example Arranger")), (Cow::Borrowed("producer"), Cow::Borrowed("Old Producer"))],
            )),
        ]
    }

    #[test]
    fn id3_write_changes_only_the_requested_frames() {
        for v23 in [false, true] {
            let dir = scratch("id3keep");
            let path = dir.join("a.mp3");
            let audio = mp3_bytes();
            std::fs::write(&path, id3_file(&audio, rich_frames(), v23)).unwrap();
            let want_version = if v23 { Id3v2Version::V3 } else { Id3v2Version::V4 };
            assert_eq!(read_id3(&path, FileType::Mpeg).unwrap().unwrap().original_version(), want_version);
            let values = TrackEdit {
                title: Some("New Title".into()),
                comments: Some("new comment".into()),
                producer: Some("New Producer".into()),
                label: Some("Example Label".into()),
                year: Some(2020),
                ..Default::default()
            };
            let p = prepare(&path, &values).unwrap();
            let change = |f: &str| p.changes.iter().find(|c| c.field == f).unwrap().clone();
            // "the comment" is the frame without a description, not Sound Check
            assert_eq!(change("comments").old, "old comment", "v23={v23}");
            assert_eq!(change("producer").old, "Old Producer");
            assert!(p.unsupported.is_empty(), "{:?}", p.unsupported);
            write_prepared(&path, &p).unwrap();
            let after = std::fs::read(&path).unwrap();
            assert!(after.ends_with(&audio));
            let back = read_id3(&path, FileType::Mpeg).unwrap().unwrap();
            assert_eq!(back.original_version(), want_version, "the tag version is kept");
            assert_eq!(id3_value(&back, "title"), "New Title");
            assert_eq!(id3_value(&back, "artist"), "Old Artist");
            assert_eq!(id3_value(&back, "comments"), "new comment");
            assert_eq!(id3_value(&back, "label"), "Example Label");
            assert_eq!(id3_value(&back, "year"), "2020");
            assert_eq!(id3_value(&back, "producer"), "New Producer");
            assert_eq!(other_people(&back), vec![("arranger".to_string(), "Example Arranger".to_string())]);
            let comments: Vec<(String, String)> = back
                .iter()
                .filter_map(|f| if let Frame::Comment(c) = f { Some((c.description.to_string(), c.content.to_string())) } else { None })
                .collect();
            assert!(comments.contains(&("iTunNORM".into(), " 00000000 00000000 00000000 00000000".into())), "{comments:?}");
            assert!(comments.contains(&("iTunSMPB".into(), " 00000000 00000210 00000A2C".into())), "{comments:?}");
            assert_eq!(comments.len(), 3);
            let popm = back.iter().find_map(|f| if let Frame::Popularimeter(p) = f { Some((p.email.to_string(), p.rating, p.counter)) } else { None });
            assert_eq!(popm, Some(("user@example.test".to_string(), 200, 7)), "the rating is not rounded to a star step");
            assert!(back.iter().any(|f| matches!(f, Frame::Private(p) if p.owner == "owner@example.test" && p.private_data[..] == [1u8, 2, 3, 250][..])));
            assert_eq!(back.get_user_text("MY_TAG"), Some("kept value"));
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn a_dropped_or_altered_frame_fails_the_check() {
        let mut written = Id3v2Tag::new();
        for f in rich_frames() {
            written.insert(f);
        }
        let touched = vec!["TIT2"];
        assert!(check_id3_untouched(&written, &written.clone(), &touched).is_ok());
        // a frame missing from the rewritten file
        let mut lost = written.clone();
        lost.remove(&FrameId::Valid(Cow::Borrowed("POPM"))).for_each(drop);
        assert!(check_id3_untouched(&written, &lost, &touched).unwrap_err().contains("POPM"));
        // a rating changed on the way (what the generic tag did)
        let mut moved = written.clone();
        moved.insert(Frame::Popularimeter(PopularimeterFrame::new("user@example.test", 196, 7)));
        assert!(check_id3_untouched(&written, &moved, &touched).unwrap_err().contains("POPM"));
        // the description-less comment is the only COMM a comment write may replace
        let mut norm = written.clone();
        norm.retain(|f| !matches!(f, Frame::Comment(c) if c.description == "iTunNORM"));
        assert!(check_id3_untouched(&written, &norm, &["COMM"]).unwrap_err().contains("COMM"));
    }

    #[test]
    fn an_id3v22_tag_is_upgraded_and_the_plan_says_so() {
        let frame = |id: &[u8; 3], text: &str| {
            let mut v = id.to_vec();
            v.extend(((text.len() + 1) as u32).to_be_bytes()[1..].iter());
            v.push(0); // Latin-1
            v.extend(text.as_bytes());
            v
        };
        let body = [frame(b"TT2", "Old Title"), frame(b"TP1", "Old Artist")].concat();
        let mut file = b"ID3".to_vec();
        file.extend([2, 0, 0]);
        file.extend([0, 0, (body.len() >> 7) as u8, (body.len() & 0x7f) as u8]);
        file.extend(&body);
        let audio = mp3_bytes();
        file.extend(&audio);
        let dir = scratch("v22");
        let path = dir.join("a.mp3");
        std::fs::write(&path, &file).unwrap();
        assert_eq!(read_id3(&path, FileType::Mpeg).unwrap().unwrap().original_version(), Id3v2Version::V2);
        let p = prepare(&path, &edit("New Title", "Old Artist")).unwrap();
        assert_eq!(p.changes.len(), 1);
        assert!(p.notes.iter().any(|n| n.contains("ID3v2.2")), "{:?}", p.notes);
        write_prepared(&path, &p).unwrap();
        let back = read_id3(&path, FileType::Mpeg).unwrap().unwrap();
        assert_eq!(back.original_version(), Id3v2Version::V4);
        assert_eq!((id3_value(&back, "title").as_str(), id3_value(&back, "artist").as_str()), ("New Title", "Old Artist"));
        assert!(std::fs::read(&path).unwrap().ends_with(&audio));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn non_latin_text_is_not_turned_into_question_marks() {
        let dir = scratch("utf");
        let path = dir.join("a.mp3");
        // an old Latin-1 comment frame: the new text has to move it to a wider encoding
        let latin1 = Frame::Comment(CommentFrame::new(TextEncoding::Latin1, *b"eng", String::new(), "old".to_string()));
        std::fs::write(&path, id3_file(&mp3_bytes(), vec![latin1], true)).unwrap();
        let values = TrackEdit { comments: Some("Привет 世界".into()), title: Some("Ünïcode ✓".into()), ..Default::default() };
        let p = prepare(&path, &values).unwrap();
        write_prepared(&path, &p).unwrap();
        let back = read_id3(&path, FileType::Mpeg).unwrap().unwrap();
        assert_eq!(id3_value(&back, "comments"), "Привет 世界");
        assert_eq!(id3_value(&back, "title"), "Ünïcode ✓");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---- every supported format keeps its audio ----

    const FIXTURES: [(&str, &[u8]); 9] = [
        ("flac", include_bytes!("fixtures/silence.flac")),
        ("ogg", include_bytes!("fixtures/silence.ogg")),
        ("opus", include_bytes!("fixtures/silence.opus")),
        ("m4a", include_bytes!("fixtures/silence.m4a")),
        ("m4a", include_bytes!("fixtures/silence_moovlast.m4a")),
        ("aac", include_bytes!("fixtures/silence.aac")),
        ("aiff", include_bytes!("fixtures/silence.aiff")),
        ("mp3", &[]),
        ("wav", &[]),
    ];

    fn fixture(i: usize) -> Vec<u8> {
        match FIXTURES[i].0 {
            "mp3" => mp3_bytes(),
            "wav" => wav_bytes(),
            _ => FIXTURES[i].1.to_vec(),
        }
    }

    /// Flips one byte of the audio payload (not a tag) of `bytes`.
    fn damage_audio(i: usize, bytes: &mut [u8]) {
        // WAV and AIFF keep their ID3 chunk at the end, so the last bytes are not audio there
        let after = |marker: &[u8]| bytes.windows(4).position(|w| w == marker).unwrap() + 4 + 20;
        let at = match FIXTURES[i].0 {
            "m4a" => after(b"mdat"),
            "aiff" => after(b"SSND"),
            "wav" => after(b"data"),
            _ => bytes.len() - 5,
        };
        bytes[at] ^= 0x55;
    }

    #[test]
    fn every_format_keeps_its_audio() {
        let rich = |n: i64| TrackEdit {
            title: Some(format!("Example Title {n}")),
            artist: Some(format!("Example Artist {n}")),
            album: Some(format!("Example Album {n}")),
            album_artist: Some("Example Album Artist".into()),
            genre: Some("Tech House".into()),
            label: Some("Example Label".into()),
            remixer: Some("Example Remixer".into()),
            comments: Some("c".repeat(200 + n as usize * 50)),
            year: Some(2000 + n),
            track_number: Some(n),
            ..Default::default()
        };
        for i in 0..FIXTURES.len() {
            let ext = FIXTURES[i].0;
            let dir = scratch(&format!("fmt{i}"));
            let path = dir.join(format!("a.{ext}"));
            let original = fixture(i);
            std::fs::write(&path, &original).unwrap();
            let ft = Probe::open(&path).unwrap().read().unwrap().file_type();
            let fp = audiohash::fingerprint(&path, ft).unwrap_or_else(|e| panic!("{ext} #{i}: {e}"));
            for round in [1i64, 2] {
                let p = prepare(&path, &rich(round)).unwrap_or_else(|e| panic!("{ext} #{i}: {e}"));
                assert!(!p.changes.is_empty(), "{ext} #{i}");
                write_prepared(&path, &p).unwrap_or_else(|e| panic!("{ext} #{i} round {round}: {e}"));
                assert_eq!(audiohash::fingerprint(&path, ft).unwrap(), fp, "{ext} #{i}: the audio changed");
                // what was written reads back, and a second plan has nothing left to do
                let again = prepare(&path, &rich(round)).unwrap();
                assert!(again.changes.is_empty(), "{ext} #{i} round {round}: {:?}", again.changes);
                let tf = Probe::open(&path).unwrap().read().unwrap();
                let tag = tf.tag(tf.file_type().primary_tag_type()).unwrap();
                assert_eq!(tag.title().as_deref(), Some(format!("Example Title {round}").as_str()), "{ext} #{i}");
            }
            // the file differs from the original (a tag was written) but no temp copy is left
            assert_ne!(std::fs::read(&path).unwrap(), original, "{ext} #{i}");
            let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            assert_eq!(names.len(), 1, "{names:?}");
            // and damage to the audio is what the fingerprint notices
            let mut bad = std::fs::read(&path).unwrap();
            damage_audio(i, &mut bad);
            let badp = dir.join(format!("bad.{ext}"));
            std::fs::write(&badp, &bad).unwrap();
            assert_ne!(audiohash::fingerprint(&badp, ft).unwrap(), fp, "{ext} #{i}: damage went unnoticed");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn generic_tag_items_outside_the_request_must_survive() {
        let dir = scratch("generic");
        let path = dir.join("a.flac");
        std::fs::write(&path, include_bytes!("fixtures/silence.flac")).unwrap();
        let first = TrackEdit { title: Some("Example Title".into()), album: Some("Example Album".into()), ..Default::default() };
        let p = prepare(&path, &first).unwrap();
        write_prepared(&path, &p).unwrap();
        let p2 = prepare(&path, &TrackEdit { title: Some("Other Title".into()), ..Default::default() }).unwrap();
        write_prepared(&path, &p2).unwrap();
        let tag = read_tag(&path);
        assert_eq!(tag.album().as_deref(), Some("Example Album"));
        assert_eq!(tag.title().as_deref(), Some("Other Title"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---- leftover temp copies ----

    #[test]
    fn temp_copies_of_interrupted_writes_are_recognised_and_swept() {
        assert!(is_tag_temp_name("1f2e.mp3.1234-0.tmp"));
        assert!(is_tag_temp_name("1f2e.FLAC.77-12.tmp"));
        for not in ["1f2e.mp3", "1f2e.mp3.tmp", "1f2e.mp3.12.tmp", "notes.txt.12-3.tmp", "1f2e.mp3.a-1.tmp", "1f2e.mp3.1-b.tmp", "x.12-3.tmp"] {
            assert!(!is_tag_temp_name(not), "{not}");
        }
        let dir = scratch("sweep");
        let audio = dir.join("1f2e.mp3");
        std::fs::write(&audio, mp3_bytes()).unwrap();
        let orphan = dir.join("1f2e.mp3.4242-3.tmp");
        std::fs::write(&orphan, mp3_bytes()).unwrap();
        let other = dir.join("notes.txt.4242-3.tmp");
        std::fs::write(&other, b"x").unwrap();
        // a copy that is just being written is not touched ...
        assert_eq!(sweep_stale_temps(&dir, STALE_TEMP_AGE), 0);
        assert!(orphan.exists());
        // ... one older than the limit is, and only that kind of file
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        File::options().write(true).open(&orphan).unwrap().set_modified(old).unwrap();
        File::options().write(true).open(&other).unwrap().set_modified(old).unwrap();
        assert_eq!(sweep_stale_temps(&dir, STALE_TEMP_AGE), 1);
        assert!(!orphan.exists() && audio.exists() && other.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_job_sweeps_the_folders_of_its_files() {
        let (base, mut lib, ids) = card("jobsweep", &[("one.mp3", mp3_bytes())]);
        let vol = base.join("vol");
        let tracks = tracks_by_id(&lib).unwrap();
        let rel = tracks[&ids[0]].location.strip_prefix(naming::LOCATION_PREFIX).unwrap().to_string();
        let file = vol.join(naming::MUSIC_DIR).join(&rel);
        let orphan = file.with_file_name(format!("{}.99-1.tmp", file.file_name().unwrap().to_string_lossy()));
        std::fs::write(&orphan, b"half a file").unwrap();
        File::options().write(true).open(&orphan).unwrap().set_modified(std::time::SystemTime::now() - Duration::from_secs(7200)).unwrap();
        let items = vec![TagItem { track_id: ids[0], values: edit("New Title", "New Artist") }];
        let out = run_write(&vol, &mut lib, &items, |_| {}).unwrap();
        assert!(!orphan.exists());
        assert!(out.summary.contains("1 leftover temp copy"), "{}", out.summary);
        drop(lib);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
