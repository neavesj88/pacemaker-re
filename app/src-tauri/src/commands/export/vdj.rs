//! VirtualDJ list export: a case (or the tracks selected in it) becomes a VirtualDJ "List" file,
//! optionally with copies of the audio beside it, in a folder the user chooses (a USB stick, say).
//!
//! The format is VirtualDJ's own XML list (VirtualDJ 2024 and later: a `<VirtualFolder>` root
//! with one `<song path="..." size="...">` per track), as documented on VDJPedia
//! (<https://virtualdj.com/wiki/Lists.html>). Why this format and not OneLibrary / Device Library
//! Plus is in `notes/onelibrary-export.md`.
//!
//! Nothing here writes to the Pacemaker. The library and the audio are only read; everything
//! goes to the destination folder. Each track's audio file is found from its stored location by
//! the same plain-names-only rule the player uses (`playback::check_relative`,
//! `tracks::audio_in_volume`), re-stated in [`audio_parts`] / [`resolve_audio`] and never through
//! `Library::track_paths`. Nothing in the destination is overwritten except the list file
//! itself, and only when the caller says so: an audio copy that already exists under the same
//! name and size is reused, one that differs gets a numbered name.
//!
//! The preview ([`plan_from`]) and the export ([`run_export`]) are one computation: the export
//! re-plans and then does exactly what the plan says.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pm_formats::library::{Library, Track};
use pm_formats::naming::{LOCATION_PREFIX, MUSIC_DIR};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

/// The `job` name of the export in `job-progress` / `job-done` events.
pub const JOB_VDJ_EXPORT: &str = "virtualdj-export";
/// Folder (inside the export folder) that holds the audio copies.
pub const AUDIO_DIR: &str = "Audio";
/// Most tracks in one export (a case this long is not a set).
pub const MAX_TRACKS: usize = 5000;
/// Longest file-name stem for a copy or a list.
const MAX_STEM: usize = 120;
const COPY_CHUNK: usize = 1 << 20;

/// Set by `vdj_export_cancel`, polled between chunks of a copy. Cleared when an export starts.
static CANCEL: AtomicBool = AtomicBool::new(false);

// ---- request, options, plan --------------------------------------------------------------

/// How the audio copies are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NameFiles {
    /// "Artist - Title.ext" (the title alone, or the original name, when tags are missing).
    #[default]
    ArtistTitle,
    /// The file name the track has on the Pacemaker.
    Original,
}

/// The user's choices. Every one is a setting in the dialog; these are the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Copy the audio into the export folder and point the list at the copies. Off: the list
    /// points at the files on the Pacemaker volume as it is mounted now.
    pub copy_audio: bool,
    pub name_files: NameFiles,
    /// List attribute `ordered`: VirtualDJ keeps the list's order between sessions.
    pub ordered: bool,
    /// List attribute `noDuplicates`.
    pub no_duplicates: bool,
    /// Copy mode only: where the export folder will be when VirtualDJ opens it (for a stick that
    /// gets another drive letter on the DJ's laptop). Empty = where it is written now.
    pub path_base: String,
}

impl Default for Options {
    fn default() -> Self {
        Options { copy_audio: true, name_files: NameFiles::ArtistTitle, ordered: true, no_duplicates: true, path_base: String::new() }
    }
}

/// What the UI sends for both the preview and the export.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub root: String,
    pub case_id: i64,
    /// Only these tracks of the case (kept in case order). Empty = the whole case.
    #[serde(default)]
    pub only_track_ids: Vec<i64>,
    /// The list's name; it is also the name of the folder and of the `.xml` file.
    pub list_name: String,
    pub dest_dir: String,
    #[serde(default)]
    pub options: Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The audio file will be copied.
    Copy,
    /// A file with this name and size is already in the Audio folder; it is used as it is.
    Reuse,
    /// No copy: the list points at the file on the Pacemaker volume.
    Reference,
    /// Cannot be exported; `reason` says why. It is not in the list.
    LeftOut,
}

/// The values the list needs for one track.
#[derive(Debug, Clone, PartialEq)]
struct Item {
    path: String,
    size: u64,
    artist: String,
    title: String,
    secs: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanEntry {
    pub track_id: i64,
    pub label: String,
    pub status: Status,
    /// The file name the audio has in the export (empty when left out).
    pub file: String,
    /// The path written into the list (empty when left out).
    pub path: String,
    pub size: u64,
    /// Why the track is left out; empty otherwise.
    pub reason: String,
    #[serde(skip)]
    source: PathBuf,
    #[serde(skip)]
    target: PathBuf,
    #[serde(skip)]
    item: Option<Item>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// The list's name as it will be used (cleaned for use as a file name).
    pub list_name: String,
    /// The export folder: `<destination>/<list name>`.
    pub folder: String,
    /// The `.xml` file in it.
    pub list_file: String,
    /// The list file is already there; the export refuses unless told to replace it.
    pub list_exists: bool,
    pub copy_audio: bool,
    pub entries: Vec<PlanEntry>,
    pub copy_count: usize,
    pub reuse_count: usize,
    pub reference_count: usize,
    pub left_out_count: usize,
    /// Bytes the copies will write.
    pub copy_bytes: u64,
    /// Things worth saying that are not about one track.
    pub notes: Vec<String>,
}

// ---- names and paths ---------------------------------------------------------------------

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
];

/// A string usable as one file or folder name on Windows, macOS and Linux: forbidden characters
/// become `_`, runs of white space one space, no leading/trailing space or dot, not a reserved
/// device name, at most [`MAX_STEM`] characters. Never empty (`fallback`).
fn safe_name(s: &str, fallback: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let mut out = String::new();
    let mut last_space = false;
    for c in mapped.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    let cut: String = out.trim().chars().take(MAX_STEM).collect();
    let cut = cut.trim().trim_end_matches(['.', ' ']).to_string();
    let stem = cut.split('.').next().unwrap_or("");
    let cut = if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) { format!("_{cut}") } else { cut };
    if cut.is_empty() { fallback.to_string() } else { cut }
}

/// The extension of a stored file name when it is a short plain one (`mp3`, `flac`), else empty.
fn plain_ext(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && (1..=5).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()) => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn stem_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// The path components of a stored `tracks.location` below the music folder, or why it is not
/// acceptable. Every component must be a plain name: `Path::join` replaces its base for a rooted
/// or drive-prefixed component and `..` climbs out, so those (and backslashes, colons, NUL and
/// empty parts) are refused on every platform.
fn audio_parts(location: &str) -> Result<Vec<&str>, String> {
    let rel = location.strip_prefix(LOCATION_PREFIX).ok_or("The track's location is not a Pacemaker music path.")?;
    let parts: Vec<&str> = rel.split('/').collect();
    let plain = !rel.is_empty()
        && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains([':', '\\', '\0']) && !p.chars().any(char::is_control));
    if plain { Ok(parts) } else { Err("The track's location points outside the library.".into()) }
}

/// The audio file for a stored location, which must lie inside the volume's music folder (and,
/// if both exist, still inside it after links are followed). The returned path may not exist.
fn resolve_audio(root: &Path, location: &str) -> Result<PathBuf, String> {
    let parts = audio_parts(location)?;
    let music = root.join(MUSIC_DIR);
    let mut audio = music.clone();
    audio.extend(&parts);
    let inside = audio.strip_prefix(&music).is_ok_and(|rel| rel.components().all(|c| matches!(c, Component::Normal(_))));
    if !inside {
        return Err("The track's location points outside the library.".into());
    }
    if let (Ok(a), Ok(m)) = (audio.canonicalize(), music.canonicalize())
        && !a.starts_with(&m)
    {
        return Err("The track's file is a link that leads outside the library.".into());
    }
    Ok(audio)
}

/// `base` joined with `parts` in the path style `base` is written in: a drive letter or `\\`
/// start means Windows (`\`), a leading `/` means Unix. Anything else is not a full path.
fn join_abs(base: &str, parts: &[&str]) -> Result<String, String> {
    let b = base.trim();
    if b.chars().any(char::is_control) {
        return Err("The path has a control character in it.".into());
    }
    let bytes = b.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
    let unc = b.starts_with("\\\\");
    let sep = if drive || unc {
        '\\'
    } else if b.starts_with('/') {
        '/'
    } else {
        return Err("Write a full path such as D:\\Gig or /Volumes/USB/Gig.".into());
    };
    let norm: String = if sep == '\\' { b.replace('/', "\\") } else { b.to_string() };
    if norm.split(sep).any(|p| p == "..") {
        return Err("The path must not contain \"..\".".into());
    }
    let mut out = if unc { format!("\\\\{}", norm.trim_start_matches('\\').trim_end_matches('\\')) } else { norm.trim_end_matches(sep).to_string() };
    for p in parts {
        out.push(sep);
        out.push_str(p);
    }
    Ok(out)
}

/// A path as the plain string a user would recognise.
fn native(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn label_of(t: &Track) -> String {
    match (t.artist.trim().is_empty(), t.title.trim().is_empty()) {
        (false, false) => format!("{} - {}", t.artist.trim(), t.title.trim()),
        (_, false) => t.title.trim().to_string(),
        _ => t.location.rsplit('/').next().unwrap_or(&t.location).to_string(),
    }
}

// ---- the plan ----------------------------------------------------------------------------

/// The destination must be an existing folder outside the Pacemaker volume.
fn check_dest(root: &Path, dest: &Path) -> Result<(), String> {
    if !dest.is_absolute() {
        return Err("Choose a destination folder.".into());
    }
    if !dest.is_dir() {
        return Err("The destination folder does not exist.".into());
    }
    let d = dest.canonicalize().map_err(|e| format!("The destination folder cannot be used: {e}"))?;
    if let Ok(r) = root.canonicalize()
        && d.starts_with(&r)
    {
        return Err("Choose a folder outside the Pacemaker: this export is not part of its library.".into());
    }
    Ok(())
}

/// What an export of `ids` (case order, repeats allowed) would do. Reads the file system, writes
/// nothing. `tracks` is the library's track list.
pub fn plan_from(root: &Path, tracks: &[Track], ids: &[i64], list_name: &str, dest: &Path, opts: &Options) -> Result<Plan, String> {
    check_dest(root, dest)?;
    let name = safe_name(list_name, "");
    if name.is_empty() {
        return Err("Give the list a name.".into());
    }
    let mut seen = HashSet::new();
    let unique: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if unique.is_empty() {
        return Err("There are no tracks to export.".into());
    }
    if unique.len() > MAX_TRACKS {
        return Err(format!("An export holds at most {MAX_TRACKS} tracks; pick fewer."));
    }
    let folder = dest.join(&name);
    if folder.exists() && !folder.is_dir() {
        return Err(format!("{} is a file, not a folder; pick another list name.", folder.display()));
    }
    let list_file = folder.join(format!("{name}.xml"));
    let audio_dir = folder.join(AUDIO_DIR);
    let base = if opts.path_base.trim().is_empty() { native(&folder) } else { opts.path_base.trim().to_string() };
    if opts.copy_audio {
        // fail early, in words, on a base the list could not use
        join_abs(&base, &[AUDIO_DIR, "x"]).map_err(|e| format!("The folder VirtualDJ will find the audio in: {e}"))?;
    }

    let by_id: HashMap<i64, &Track> = tracks.iter().map(|t| (t.track_id, t)).collect();
    let mut used: HashSet<String> = HashSet::new(); // lower-cased copy names handed out
    let mut by_source: HashMap<PathBuf, String> = HashMap::new();
    let mut entries = Vec::with_capacity(unique.len());
    let mut notes = Vec::new();
    if unique.len() < ids.len() {
        notes.push(format!("{} repeated entr{} in the case {} listed once.", ids.len() - unique.len(), if ids.len() - unique.len() == 1 { "y" } else { "ies" }, if ids.len() - unique.len() == 1 { "is" } else { "are" }));
    }

    for id in unique {
        let left_out = |label: String, reason: String| PlanEntry {
            track_id: id,
            label,
            status: Status::LeftOut,
            file: String::new(),
            path: String::new(),
            size: 0,
            reason,
            source: PathBuf::new(),
            target: PathBuf::new(),
            item: None,
        };
        let Some(t) = by_id.get(&id) else {
            entries.push(left_out(format!("track #{id}"), "No longer in the library.".into()));
            continue;
        };
        let label = label_of(t);
        let src = match resolve_audio(root, &t.location) {
            Ok(p) => p,
            Err(why) => {
                entries.push(left_out(label, why));
                continue;
            }
        };
        let size = match fs::metadata(&src) {
            Ok(m) if m.is_file() => m.len(),
            Ok(_) => {
                entries.push(left_out(label, "The audio is not a file.".into()));
                continue;
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                entries.push(left_out(label, "The audio file is missing from the Pacemaker.".into()));
                continue;
            }
            Err(e) => {
                entries.push(left_out(label, format!("The audio file cannot be read: {e}.")));
                continue;
            }
        };
        let original = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let parts = audio_parts(&t.location)?;
        let item = |path: String| Item { path, size, artist: t.artist.trim().to_string(), title: t.title.trim().to_string(), secs: t.play_time_secs };

        if !opts.copy_audio {
            let mut p: Vec<&str> = MUSIC_DIR.split('/').collect();
            p.extend(&parts);
            let path = match join_abs(&native(root), &p) {
                Ok(p) => p,
                Err(_) => {
                    entries.push(left_out(label, "The Pacemaker's own path is not a full path, so a list cannot point at it.".into()));
                    continue;
                }
            };
            entries.push(PlanEntry { track_id: id, label, status: Status::Reference, file: original, path: path.clone(), size, reason: String::new(), source: src, target: PathBuf::new(), item: Some(item(path)) });
            continue;
        }

        // copy mode: one name per source file, unique in this export and never an overwrite
        let (file, status) = if let Some(f) = by_source.get(&src) {
            (f.clone(), Status::Reuse)
        } else {
            let ext = plain_ext(&original);
            let stem = match (opts.name_files, t.artist.trim().is_empty(), t.title.trim().is_empty()) {
                (NameFiles::ArtistTitle, false, false) => safe_name(&format!("{} - {}", t.artist.trim(), t.title.trim()), "track"),
                (NameFiles::ArtistTitle, true, false) => safe_name(t.title.trim(), "track"),
                _ => safe_name(stem_of(&original), "track"),
            };
            let mut n = 1;
            let picked = loop {
                let cand = if n == 1 { stem.clone() } else { format!("{stem} ({n})") };
                let cand = if ext.is_empty() { cand } else { format!("{cand}.{ext}") };
                n += 1;
                if used.contains(&cand.to_lowercase()) {
                    continue;
                }
                match fs::metadata(audio_dir.join(&cand)) {
                    Ok(m) if m.is_file() && m.len() == size => break (cand, Status::Reuse),
                    Ok(_) => continue,
                    Err(e) if e.kind() == ErrorKind::NotFound => break (cand, Status::Copy),
                    Err(e) => return Err(format!("Cannot look in {}: {e}", audio_dir.display())),
                }
            };
            used.insert(picked.0.to_lowercase());
            by_source.insert(src.clone(), picked.0.clone());
            picked
        };
        let path = join_abs(&base, &[AUDIO_DIR, &file])?;
        let target = audio_dir.join(&file);
        entries.push(PlanEntry { track_id: id, label, status, file, path: path.clone(), size, reason: String::new(), source: src, target, item: Some(item(path)) });
    }

    let count = |s: Status| entries.iter().filter(|e| e.status == s).count();
    let copy_bytes = entries.iter().filter(|e| e.status == Status::Copy).map(|e| e.size).sum();
    Ok(Plan {
        list_name: name,
        folder: native(&folder),
        list_file: native(&list_file),
        list_exists: list_file.exists(),
        copy_audio: opts.copy_audio,
        copy_count: count(Status::Copy),
        reuse_count: count(Status::Reuse),
        reference_count: count(Status::Reference),
        left_out_count: count(Status::LeftOut),
        copy_bytes,
        notes,
        entries,
    })
}

// ---- the list file -----------------------------------------------------------------------

/// Text for an XML attribute value: the five markup characters escaped, control characters (not
/// allowed in XML 1.0) dropped, line breaks and tabs turned into spaces.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// The VirtualDJ list: `<VirtualFolder>` with attributes `noDuplicates`, `singleDrive`, `ordered`
/// and one `<song>` per item with `path` (required), `size`, `artist`, `title`, `songlength`
/// (seconds with one decimal) and, in an ordered list, `idx` (from 0). BPM and key are left out:
/// the list documentation does not say in what unit they are stored.
fn list_xml(items: &[&Item], opts: &Options) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!("<VirtualFolder noDuplicates=\"{}\" singleDrive=\"no\" ordered=\"{}\">\n", yes_no(opts.no_duplicates), yes_no(opts.ordered)));
    for (i, it) in items.iter().enumerate() {
        s.push_str(&format!("  <song path=\"{}\" size=\"{}\"", esc(&it.path), it.size));
        if !it.artist.is_empty() {
            s.push_str(&format!(" artist=\"{}\"", esc(&it.artist)));
        }
        if !it.title.is_empty() {
            s.push_str(&format!(" title=\"{}\"", esc(&it.title)));
        }
        if it.secs > 0 {
            s.push_str(&format!(" songlength=\"{}.0\"", it.secs));
        }
        if opts.ordered {
            s.push_str(&format!(" idx=\"{i}\""));
        }
        s.push_str(" />\n");
    }
    s.push_str("</VirtualFolder>\n");
    s
}

// ---- running it --------------------------------------------------------------------------

/// One step reported while running: the track just handled.
pub struct Step<'a> {
    pub done: usize,
    pub total: usize,
    pub label: &'a str,
    /// "copied", "reused", "listed" or "failed".
    pub status: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct Finished {
    pub copied: usize,
    pub reused: usize,
    pub failed: Vec<(String, String)>,
    pub listed: usize,
    pub cancelled: bool,
    /// The run stopped because the destination is full.
    pub disk_full: bool,
    pub copied_bytes: u64,
}

enum CopyError {
    Cancelled,
    Io(std::io::Error),
    Other(String),
}

/// Copies `src` to `dst` through `<dst>.part` (fsync, then rename), so a stopped copy leaves no
/// half file under the real name. Refuses to replace a file that appeared since the plan.
fn copy_atomic(src: &Path, dst: &Path, expect: u64, keep_going: &dyn Fn() -> bool) -> Result<u64, CopyError> {
    let mut tmp = dst.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let run = || -> Result<u64, CopyError> {
        let mut input = File::open(src).map_err(CopyError::Io)?;
        let _ = fs::remove_file(&tmp);
        let mut out = fs::OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(CopyError::Io)?;
        let mut buf = vec![0u8; COPY_CHUNK];
        let mut total = 0u64;
        loop {
            if !keep_going() {
                return Err(CopyError::Cancelled);
            }
            let n = input.read(&mut buf).map_err(CopyError::Io)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(CopyError::Io)?;
            total += n as u64;
        }
        out.sync_all().map_err(CopyError::Io)?;
        drop(out);
        if total != expect {
            return Err(CopyError::Other(format!("the file changed while it was copied ({total} bytes read, {expect} expected)")));
        }
        if dst.exists() {
            return Err(CopyError::Other("a file with that name appeared in the Audio folder; nothing was replaced".into()));
        }
        fs::rename(&tmp, dst).map_err(CopyError::Io)?;
        Ok(total)
    };
    let res = run();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

fn write_atomic(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let res = File::create(&tmp).and_then(|mut f| f.write_all(bytes).and_then(|_| f.sync_all())).and_then(|_| fs::rename(&tmp, dest));
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// Does what `plan` says: copies the audio, then writes the list (last, so a stopped export
/// leaves no list that points at files that are not there). `progress` returns false to stop.
/// A track that cannot be copied is left out of the list and reported; a full disk stops the run.
pub fn run_export(plan: &Plan, opts: &Options, replace_list: bool, progress: &mut dyn FnMut(&Step) -> bool) -> Result<Finished, String> {
    if plan.list_exists && !replace_list {
        return Err(format!("{} already exists. Choose to replace it, or pick another list name.", plan.list_file));
    }
    let todo: Vec<&PlanEntry> = plan.entries.iter().filter(|e| e.status != Status::LeftOut).collect();
    if todo.is_empty() {
        return Err("None of these tracks can be exported.".into());
    }
    let folder = Path::new(&plan.folder);
    if plan.copy_audio {
        fs::create_dir_all(folder.join(AUDIO_DIR)).map_err(|e| format!("Cannot create {}: {e}", folder.join(AUDIO_DIR).display()))?;
    } else {
        fs::create_dir_all(folder).map_err(|e| format!("Cannot create {}: {e}", folder.display()))?;
    }
    let total = todo.len();
    let mut fin = Finished::default();
    let mut listed: Vec<&Item> = Vec::new();
    let mut failed_targets: HashSet<&Path> = HashSet::new();
    for (i, e) in todo.iter().enumerate() {
        let item = e.item.as_ref().expect("an exportable entry has its list values");
        let mut error: Option<String> = None;
        let status = match e.status {
            Status::Copy => match copy_atomic(&e.source, &e.target, e.size, &|| !CANCEL.load(Ordering::SeqCst)) {
                Ok(n) => {
                    fin.copied += 1;
                    fin.copied_bytes += n;
                    listed.push(item);
                    "copied"
                }
                Err(CopyError::Cancelled) => {
                    fin.cancelled = true;
                    break;
                }
                Err(CopyError::Io(io)) => {
                    fin.disk_full = io.kind() == ErrorKind::StorageFull;
                    error = Some(io.to_string());
                    "failed"
                }
                Err(CopyError::Other(why)) => {
                    error = Some(why);
                    "failed"
                }
            },
            // the same audio file as an earlier track of this export: it is there unless that copy failed
            Status::Reuse if failed_targets.contains(e.target.as_path()) => {
                error = Some("it is the same audio file as an earlier track that could not be copied".into());
                "failed"
            }
            Status::Reuse => {
                fin.reused += 1;
                listed.push(item);
                "reused"
            }
            _ => {
                listed.push(item);
                "listed"
            }
        };
        if let Some(why) = &error {
            fin.failed.push((e.label.clone(), why.clone()));
            failed_targets.insert(e.target.as_path());
        }
        let keep = progress(&Step { done: i + 1, total, label: &e.label, status, error });
        if !keep {
            fin.cancelled = true;
            break;
        }
        if fin.disk_full {
            break;
        }
    }
    if fin.cancelled || fin.disk_full {
        return Ok(fin);
    }
    if listed.is_empty() {
        return Err("No track could be copied, so no list was written.".into());
    }
    fin.listed = listed.len();
    write_atomic(Path::new(&plan.list_file), list_xml(&listed, opts).as_bytes()).map_err(|e| format!("Cannot write {}: {e}", plan.list_file))?;
    Ok(fin)
}

// ---- commands ----------------------------------------------------------------------------

/// Reads what the request needs from the library: all tracks, the case's track ids in case order
/// (limited to `only_track_ids` when given) and the case's name.
fn plan_request(req: &Request) -> Result<Plan, String> {
    let root = PathBuf::from(&req.root);
    let lib = Library::open_read_only(&root).map_err(|e| e.to_string())?;
    let tracks = lib.tracks().map_err(|e| e.to_string())?;
    let mut ids = lib.case_track_ids(req.case_id).map_err(|e| e.to_string())?;
    if !req.only_track_ids.is_empty() {
        let keep: HashSet<i64> = req.only_track_ids.iter().copied().collect();
        ids.retain(|id| keep.contains(id));
    }
    plan_from(&root, &tracks, &ids, &req.list_name, Path::new(&req.dest_dir), &req.options)
}

/// What exporting would do, track by track (read-only). The export itself re-plans with the same
/// function, so the preview is what happens.
#[tauri::command]
pub async fn vdj_export_plan(request: Request) -> AppResult<Plan> {
    tauri::async_runtime::spawn_blocking(move || plan_request(&request).map_err(AppError::from))
        .await
        .map_err(|e| AppError::from(e.to_string()))?
}

/// What `vdj_export_start` returns once the job has started.
#[derive(Debug, Clone, Serialize)]
pub struct Started {
    pub folder: String,
    pub list_file: String,
}

/// Starts the export as a background job and returns at once. Progress arrives as `job-progress`
/// (job `virtualdj-export`, one step per track) and the end as `job-done`. An existing list file
/// is replaced only when `replace_list` is true. Writes nothing to the Pacemaker.
#[tauri::command]
pub fn vdj_export_start(app: AppHandle, lock: State<'_, JobLock>, request: Request, replace_list: bool) -> AppResult<Started> {
    let plan = plan_request(&request)?;
    if plan.list_exists && !replace_list {
        return Err(AppError::from(format!("{} already exists. Choose to replace it, or pick another list name.", plan.list_file)));
    }
    let started = Started { folder: plan.folder.clone(), list_file: plan.list_file.clone() };
    let opts = request.options.clone();
    spawn_job(app, &lock, JOB_VDJ_EXPORT, move |app| {
        // cleared only once this call owns the job lock, so a refused second call cannot un-cancel
        CANCEL.store(false, Ordering::SeqCst);
        let name = plan.list_name.clone();
        let fin = run_export(&plan, &opts, replace_list, &mut |s| {
            let mut p = Progress::new(JOB_VDJ_EXPORT, s.label, s.done, s.total, s.status);
            p.error = s.error.clone();
            events::emit(app, JOB_PROGRESS, p);
            !CANCEL.load(Ordering::SeqCst)
        })?;
        Ok(outcome(&name, &plan, &fin))
    })?;
    Ok(started)
}

/// Asks the running export to stop. Files already copied stay; no list is written.
#[tauri::command]
pub fn vdj_export_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn outcome(name: &str, plan: &Plan, fin: &Finished) -> JobOutcome {
    let left_out = plan.left_out_count;
    let failed = fin.failed.len();
    if fin.cancelled || fin.disk_full {
        let why = if fin.cancelled { "was stopped" } else { "stopped because the destination is full" };
        return JobOutcome {
            ok: fin.copied + fin.reused,
            failed,
            summary: format!(
                "The export of {name} {why}. No list was written; {} already copied stay in {}.",
                plural(fin.copied, "file", "files"),
                plan.folder
            ),
        };
    }
    let mut parts = vec![format!("Wrote {} with {}", plan.list_file, plural(fin.listed, "track", "tracks"))];
    if plan.copy_audio {
        parts.push(format!("{} copied, {} reused", plural(fin.copied, "file", "files"), fin.reused));
    }
    if left_out > 0 {
        parts.push(format!("{} left out", plural(left_out, "track", "tracks")));
    }
    if failed > 0 {
        parts.push(format!("{} could not be copied: {}", plural(failed, "track", "tracks"), fin.failed.iter().map(|(l, w)| format!("{l} ({w})")).collect::<Vec<_>>().join("; ")));
    }
    JobOutcome { ok: fin.listed, failed: failed + left_out, summary: parts.join("; ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-vdj-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn track(id: i64, artist: &str, title: &str, location: &str) -> Track {
        Track {
            track_id: id,
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            genre: String::new(),
            bpm: 120.0,
            key: String::new(),
            play_time_secs: 200,
            rating: 0,
            location: location.into(),
            file_size: 0,
            format: "mp3".into(),
            analysed: true,
            times_played: 0,
            last_played: -1,
        }
    }

    /// A fake Pacemaker volume with these audio files (location tail, content).
    fn volume(name: &str, files: &[(&str, &[u8])]) -> (PathBuf, PathBuf) {
        let base = scratch(name);
        let root = base.join("card");
        for (rel, bytes) in files {
            let p = root.join(MUSIC_DIR).join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes).unwrap();
        }
        let dest = base.join("usb");
        fs::create_dir_all(&dest).unwrap();
        (root, dest)
    }

    fn go(root: &Path, dest: &Path, tracks: &[Track], opts: &Options) -> Plan {
        let ids: Vec<i64> = tracks.iter().map(|t| t.track_id).collect();
        plan_from(root, tracks, &ids, "Set A", dest, opts).unwrap()
    }

    // ---- names ----

    #[test]
    fn names_are_safe_on_every_platform() {
        assert_eq!(safe_name("AC/DC: Live?", "x"), "AC_DC_ Live_");
        assert_eq!(safe_name("  a   b  ", "x"), "a b");
        assert_eq!(safe_name("name. . ", "x"), "name");
        assert_eq!(safe_name("con", "x"), "_con");
        assert_eq!(safe_name("NUL.txt", "x"), "_NUL.txt");
        assert_eq!(safe_name("...", "x"), "x");
        assert_eq!(safe_name("", "x"), "x");
        assert_eq!(safe_name("a\u{0}b", "x"), "a_b");
        assert_eq!(safe_name(&"é".repeat(300), "x").chars().count(), MAX_STEM);
    }

    #[test]
    fn extensions_are_short_and_plain() {
        assert_eq!(plain_ext("a.MP3"), "mp3");
        assert_eq!(plain_ext("a.flac"), "flac");
        assert_eq!(plain_ext("noext"), "");
        assert_eq!(plain_ext(".hidden"), "");
        assert_eq!(plain_ext("a.toolongext"), "");
        assert_eq!(plain_ext("a.m p"), "");
    }

    #[test]
    fn locations_must_be_plain_names_below_the_music_folder() {
        assert_eq!(audio_parts("/pmdb_tracks/ab/cd.mp3").unwrap(), vec!["ab", "cd.mp3"]);
        for bad in [
            "/pmdb_tracks/", "/pmdb_tracks//x.mp3", "/pmdb_tracks/../x.mp3", "/pmdb_tracks/a/../../x.mp3", "/pmdb_tracks/C:/x.mp3", "/pmdb_tracks/a\\b.mp3",
            "/pmdb_tracks/./x.mp3", "/pmdb_tracks/a/", "/etc/passwd", "pmdb_tracks/x.mp3", "", "/pmdb_tracks/a\0b.mp3", "/pmdb_tracks/x\n.mp3",
        ] {
            assert!(audio_parts(bad).is_err(), "{bad:?} must be refused");
        }
        let root = Path::new("/card");
        assert!(resolve_audio(root, "/pmdb_tracks/../../x").is_err());
        assert!(resolve_audio(root, "/pmdb_tracks/a/b.mp3").unwrap().ends_with("b.mp3"));
    }

    #[test]
    fn full_paths_keep_the_style_they_are_written_in() {
        assert_eq!(join_abs("D:\\Gig\\Set A", &["Audio", "a.mp3"]).unwrap(), "D:\\Gig\\Set A\\Audio\\a.mp3");
        assert_eq!(join_abs("D:/Gig/Set A/", &["Audio", "a.mp3"]).unwrap(), "D:\\Gig\\Set A\\Audio\\a.mp3");
        assert_eq!(join_abs("D:\\", &["a.mp3"]).unwrap(), "D:\\a.mp3");
        assert_eq!(join_abs("/Volumes/USB/Set A", &["Audio", "a.mp3"]).unwrap(), "/Volumes/USB/Set A/Audio/a.mp3");
        assert_eq!(join_abs("/", &["a.mp3"]).unwrap(), "/a.mp3");
        assert_eq!(join_abs("\\\\nas\\share\\Gig", &["a.mp3"]).unwrap(), "\\\\nas\\share\\Gig\\a.mp3");
        for bad in ["Gig", "Audio\\x", "", "  ", "D:", "D:Gig", "./x", "D:\\a\\..\\b", "/a/../b", "D:\\a\u{1}"] {
            assert!(join_abs(bad, &["x"]).is_err(), "{bad:?}");
        }
    }

    // ---- the list file ----

    fn item(path: &str, artist: &str, title: &str, secs: i64) -> Item {
        Item { path: path.into(), size: 1234, artist: artist.into(), title: title.into(), secs }
    }

    #[test]
    fn the_list_follows_the_documented_shape() {
        let a = item("D:\\Gig\\Audio\\A - B.mp3", "Example Artist", "Demo Track", 200);
        let b = item("D:\\Gig\\Audio\\c.mp3", "", "", 0);
        let xml = list_xml(&[&a, &b], &Options::default());
        let want = [
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<VirtualFolder noDuplicates=\"yes\" singleDrive=\"no\" ordered=\"yes\">",
            r#"  <song path="D:\Gig\Audio\A - B.mp3" size="1234" artist="Example Artist" title="Demo Track" songlength="200.0" idx="0" />"#,
            r#"  <song path="D:\Gig\Audio\c.mp3" size="1234" idx="1" />"#,
            "</VirtualFolder>",
            "",
        ]
        .join("\n");
        assert_eq!(xml, want);
    }

    #[test]
    fn list_attributes_follow_the_options_and_unordered_lists_have_no_idx() {
        let a = item("/m/a.mp3", "", "T", 10);
        let o = Options { ordered: false, no_duplicates: false, ..Options::default() };
        let xml = list_xml(&[&a], &o);
        assert!(xml.contains("noDuplicates=\"no\" singleDrive=\"no\" ordered=\"no\""));
        assert!(!xml.contains("idx="));
    }

    #[test]
    fn markup_and_control_characters_cannot_break_the_list() {
        let a = item("D:\\a&b\\\"q\".mp3", "A <b> & 'c'", "line\nbreak\u{1}x\ttab", 5);
        let xml = list_xml(&[&a], &Options::default());
        assert!(xml.contains("path=\"D:\\a&amp;b\\&quot;q&quot;.mp3\""));
        assert!(xml.contains("artist=\"A &lt;b&gt; &amp; &apos;c&apos;\""));
        assert!(xml.contains("title=\"line breakx tab\""));
        assert_eq!(xml.matches("<song").count(), 1);
        assert_eq!(esc("é日本 ✓"), "é日本 ✓");
    }

    // ---- planning ----

    #[test]
    fn a_plan_copies_names_and_orders_like_the_case() {
        let (root, dest) = volume("plan", &[("a/1.mp3", b"one"), ("b/2.flac", b"twotwo")]);
        let t = [track(1, "Example Artist", "Demo One", "/pmdb_tracks/a/1.mp3"), track(2, "", "Demo Two", "/pmdb_tracks/b/2.flac")];
        let p = go(&root, &dest, &t, &Options::default());
        assert_eq!((p.copy_count, p.reuse_count, p.left_out_count), (2, 0, 0));
        assert_eq!(p.copy_bytes, 9);
        assert_eq!(p.entries[0].file, "Example Artist - Demo One.mp3");
        assert_eq!(p.entries[1].file, "Demo Two.flac");
        assert!(p.entries[0].path.replace('\\', "/").ends_with("/Set A/Audio/Example Artist - Demo One.mp3"), "{}", p.entries[0].path);
        assert!(!p.list_exists);
        assert!(p.list_file.ends_with("Set A.xml"));
        let orig = go(&root, &dest, &t, &Options { name_files: NameFiles::Original, ..Options::default() });
        assert_eq!(orig.entries[0].file, "1.mp3");
        assert_eq!(orig.entries[1].file, "2.flac");
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn same_names_get_numbers_and_a_repeated_source_is_one_file() {
        let (root, dest) = volume("dups", &[("a/1.mp3", b"one"), ("b/1.mp3", b"two")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3"), track(2, "A", "T", "/pmdb_tracks/b/1.mp3"), track(3, "A", "T", "/pmdb_tracks/a/1.mp3")];
        let p = go(&root, &dest, &t, &Options::default());
        assert_eq!(p.entries[0].file, "A - T.mp3");
        assert_eq!(p.entries[1].file, "A - T (2).mp3");
        assert_eq!(p.entries[2].file, "A - T.mp3");
        assert_eq!(p.entries[2].status, Status::Reuse, "the same audio file is not copied twice");
        assert_eq!(p.copy_count, 2);
        // the same name in another case is the same file name
        let case_only = go(&root, &dest, &[track(1, "a", "t", "/pmdb_tracks/a/1.mp3"), track(2, "A", "T", "/pmdb_tracks/b/1.mp3")], &Options::default());
        assert_eq!(case_only.entries[1].file, "A - T (2).mp3", "names are compared without regard to case");
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn files_already_in_the_audio_folder_are_reused_or_never_overwritten() {
        let (root, dest) = volume("reuse", &[("a/1.mp3", b"one"), ("a/2.mp3", b"twotwo")]);
        let audio = dest.join("Set A").join(AUDIO_DIR);
        fs::create_dir_all(&audio).unwrap();
        fs::write(audio.join("A - One.mp3"), b"one").unwrap(); // same size: reused
        fs::write(audio.join("A - Two.mp3"), b"different size").unwrap(); // other file with that name: kept
        let t = [track(1, "A", "One", "/pmdb_tracks/a/1.mp3"), track(2, "A", "Two", "/pmdb_tracks/a/2.mp3")];
        let p = go(&root, &dest, &t, &Options::default());
        assert_eq!(p.entries[0].status, Status::Reuse);
        assert_eq!(p.entries[0].file, "A - One.mp3");
        assert_eq!(p.entries[1].status, Status::Copy);
        assert_eq!(p.entries[1].file, "A - Two (2).mp3");
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn tracks_that_cannot_be_exported_are_left_out_with_a_reason() {
        let (root, dest) = volume("leftout", &[("a/1.mp3", b"one")]);
        let t = [
            track(1, "A", "Ok", "/pmdb_tracks/a/1.mp3"),
            track(2, "A", "Gone", "/pmdb_tracks/a/none.mp3"),
            track(3, "A", "Escape", "/pmdb_tracks/../../outside.mp3"),
            track(4, "A", "Drive", "/pmdb_tracks/C:/x.mp3"),
            track(5, "A", "Elsewhere", "/somewhere/else.mp3"),
        ];
        let mut ids: Vec<i64> = t.iter().map(|x| x.track_id).collect();
        ids.push(99);
        let p = plan_from(&root, &t, &ids, "Set A", &dest, &Options::default()).unwrap();
        assert_eq!((p.copy_count, p.left_out_count), (1, 5));
        for e in p.entries.iter().skip(1) {
            assert_eq!(e.status, Status::LeftOut);
            assert!(e.path.is_empty() && e.file.is_empty() && !e.reason.is_empty(), "{e:?}");
        }
        assert!(p.entries[5].reason.contains("library"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn reference_mode_points_at_the_card_and_copies_nothing() {
        let (root, dest) = volume("refmode", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3")];
        let o = Options { copy_audio: false, ..Options::default() };
        let p = go(&root, &dest, &t, &o);
        assert_eq!(p.reference_count, 1);
        assert_eq!(p.copy_count, 0);
        assert!(p.entries[0].path.replace('\\', "/").ends_with("/card/.Pacemaker/Music/a/1.mp3"), "{}", p.entries[0].path);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn the_path_base_replaces_where_the_folder_is_written_into_the_list() {
        let (root, dest) = volume("base", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3")];
        let o = Options { path_base: "E:\\Gigs\\Set A".into(), ..Options::default() };
        assert_eq!(go(&root, &dest, &t, &o).entries[0].path, "E:\\Gigs\\Set A\\Audio\\A - T.mp3");
        let o = Options { path_base: "/Volumes/USB/Set A".into(), ..Options::default() };
        assert_eq!(go(&root, &dest, &t, &o).entries[0].path, "/Volumes/USB/Set A/Audio/A - T.mp3");
        let bad = Options { path_base: "Gigs".into(), ..Options::default() };
        assert!(plan_from(&root, &t, &[1], "Set A", &dest, &bad).unwrap_err().contains("full path"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn bad_requests_are_refused_in_words() {
        let (root, dest) = volume("refuse", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3")];
        let o = Options::default();
        assert!(plan_from(&root, &t, &[1], "  ", &dest, &o).unwrap_err().contains("name"));
        assert!(plan_from(&root, &t, &[], "Set", &dest, &o).unwrap_err().contains("no tracks"));
        assert!(plan_from(&root, &t, &[1], "Set", &dest.join("nope"), &o).unwrap_err().contains("not exist"));
        assert!(plan_from(&root, &t, &[1], "Set", Path::new("relative"), &o).unwrap_err().contains("destination"));
        // inside the Pacemaker volume
        assert!(plan_from(&root, &t, &[1], "Set", &root, &o).unwrap_err().contains("outside"));
        assert!(plan_from(&root, &t, &[1], "Set", &root.join(MUSIC_DIR), &o).unwrap_err().contains("outside"));
        // a file where the folder would go
        fs::write(dest.join("Taken"), b"x").unwrap();
        assert!(plan_from(&root, &t, &[1], "Taken", &dest, &o).unwrap_err().contains("is a file"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn repeats_in_the_case_are_listed_once_and_said_so() {
        let (root, dest) = volume("repeats", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3")];
        let p = plan_from(&root, &t, &[1, 1, 1], "Set", &dest, &Options::default()).unwrap();
        assert_eq!(p.entries.len(), 1);
        assert!(p.notes[0].contains("2 repeated entries"), "{:?}", p.notes);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    // ---- running ----

    fn run(plan: &Plan, opts: &Options, replace: bool) -> Result<Finished, String> {
        CANCEL.store(false, Ordering::SeqCst);
        run_export(plan, opts, replace, &mut |_| true)
    }

    #[test]
    fn an_export_copies_the_audio_and_writes_the_list_last() {
        let (root, dest) = volume("run", &[("a/1.mp3", b"one"), ("b/2.mp3", b"twotwo")]);
        let t = [track(1, "Example Artist", "One", "/pmdb_tracks/a/1.mp3"), track(2, "Example Artist", "Two", "/pmdb_tracks/b/2.mp3")];
        let o = Options::default();
        let p = go(&root, &dest, &t, &o);
        let mut steps = Vec::new();
        let fin = run_export(&p, &o, false, &mut |s| {
            steps.push((s.done, s.total, s.status));
            true
        })
        .unwrap();
        assert_eq!((fin.copied, fin.reused, fin.listed, fin.failed.len()), (2, 0, 2, 0));
        assert_eq!(steps, vec![(1, 2, "copied"), (2, 2, "copied")]);
        let audio = dest.join("Set A").join(AUDIO_DIR);
        assert_eq!(fs::read(audio.join("Example Artist - One.mp3")).unwrap(), b"one");
        assert_eq!(fs::read(audio.join("Example Artist - Two.mp3")).unwrap(), b"twotwo");
        let xml = fs::read_to_string(&p.list_file).unwrap();
        assert!(xml.contains("<VirtualFolder noDuplicates=\"yes\""));
        assert_eq!(xml.matches("<song ").count(), 2);
        assert!(xml.contains("size=\"3\"") && xml.contains("size=\"6\""));
        // the source is untouched and no temp file is left behind
        assert_eq!(fs::read(root.join(MUSIC_DIR).join("a/1.mp3")).unwrap(), b"one");
        assert!(fs::read_dir(&audio).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));

        // exporting again: everything is reused and the list needs the go-ahead to be replaced
        let again = go(&root, &dest, &t, &o);
        assert!(again.list_exists);
        assert_eq!((again.copy_count, again.reuse_count), (0, 2));
        assert!(run(&again, &o, false).unwrap_err().contains("already exists"));
        let fin = run(&again, &o, true).unwrap();
        assert_eq!((fin.copied, fin.reused, fin.listed), (0, 2, 2));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_stopped_export_writes_no_list_and_leaves_no_half_file() {
        let (root, dest) = volume("cancel", &[("a/1.mp3", b"one"), ("a/2.mp3", b"two")]);
        let t = [track(1, "A", "One", "/pmdb_tracks/a/1.mp3"), track(2, "A", "Two", "/pmdb_tracks/a/2.mp3")];
        let o = Options::default();
        let p = go(&root, &dest, &t, &o);
        let fin = run_export(&p, &o, false, &mut |_| false).unwrap();
        assert!(fin.cancelled);
        assert_eq!(fin.copied, 1);
        assert!(!Path::new(&p.list_file).exists());
        let audio = dest.join("Set A").join(AUDIO_DIR);
        let names: Vec<String> = fs::read_dir(&audio).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["A - One.mp3".to_string()]);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_copy_never_replaces_a_file_that_appeared_after_the_plan() {
        let (root, dest) = volume("race", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "One", "/pmdb_tracks/a/1.mp3")];
        let o = Options::default();
        let p = go(&root, &dest, &t, &o);
        let audio = dest.join("Set A").join(AUDIO_DIR);
        fs::create_dir_all(&audio).unwrap();
        fs::write(audio.join("A - One.mp3"), b"somebody else's").unwrap();
        let err = run(&p, &o, false).unwrap_err();
        assert!(err.contains("No track could be copied"), "{err}");
        assert_eq!(fs::read(audio.join("A - One.mp3")).unwrap(), b"somebody else's");
        assert!(!Path::new(&p.list_file).exists());
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn a_track_that_fails_is_left_out_of_the_list_and_reported() {
        let (root, dest) = volume("fail", &[("a/1.mp3", b"one"), ("a/2.mp3", b"two")]);
        let t = [track(1, "A", "One", "/pmdb_tracks/a/1.mp3"), track(2, "A", "Two", "/pmdb_tracks/a/2.mp3")];
        let o = Options::default();
        let p = go(&root, &dest, &t, &o);
        fs::remove_file(root.join(MUSIC_DIR).join("a/1.mp3")).unwrap(); // vanishes between plan and run
        let fin = run(&p, &o, false).unwrap();
        assert_eq!((fin.copied, fin.listed, fin.failed.len()), (1, 1, 1));
        assert_eq!(fin.failed[0].0, "A - One");
        let xml = fs::read_to_string(&p.list_file).unwrap();
        assert_eq!(xml.matches("<song ").count(), 1);
        assert!(xml.contains("A - Two.mp3") && !xml.contains("A - One.mp3"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn reference_mode_writes_only_the_list() {
        let (root, dest) = volume("refrun", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3")];
        let o = Options { copy_audio: false, ..Options::default() };
        let p = go(&root, &dest, &t, &o);
        let fin = run(&p, &o, false).unwrap();
        assert_eq!((fin.copied, fin.listed), (0, 1));
        assert!(!dest.join("Set A").join(AUDIO_DIR).exists());
        let xml = fs::read_to_string(&p.list_file).unwrap();
        assert!(xml.replace('\\', "/").contains("/card/.Pacemaker/Music/a/1.mp3"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn the_job_summary_names_what_happened() {
        let (root, dest) = volume("summary", &[("a/1.mp3", b"one")]);
        let t = [track(1, "A", "T", "/pmdb_tracks/a/1.mp3"), track(2, "A", "Gone", "/pmdb_tracks/a/x.mp3")];
        let o = Options::default();
        let p = go(&root, &dest, &t, &o);
        let fin = run(&p, &o, false).unwrap();
        let out = outcome("Set A", &p, &fin);
        assert!(out.summary.contains("with 1 track"), "{}", out.summary);
        assert!(out.summary.contains("1 file copied, 0 reused"), "{}", out.summary);
        assert!(out.summary.contains("1 track left out"), "{}", out.summary);
        assert_eq!((out.ok, out.failed), (1, 1));
        let stopped = Finished { cancelled: true, copied: 2, ..Finished::default() };
        assert!(outcome("Set A", &p, &stopped).summary.contains("was stopped. No list was written; 2 files already copied"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }
}
