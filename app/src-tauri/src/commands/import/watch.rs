//! Watch-folder import: folders on this computer that are searched for audio the library does
//! not have yet. The scan only reads; what it finds is handed to the ordinary add-tracks flow by
//! the frontend (which asks first in "ask" mode). Everything here lives in the app config dir,
//! never on the device: the settings are one global file, the last scan result, the ignore list
//! and the log of automatic adds are one file per library (keyed by the volume root, like the
//! smart cases).
//!
//! How a file counts as "already in the library", in this order, and each is reported:
//! 1. Same file name and size. The device names a copy after exactly these two (see
//!    `pm_formats::naming`), so this is the test `add_tracks` itself uses to skip a duplicate;
//!    the extension's letter case is ignored.
//! 2. Same size, and the file name (or "artist - title") equals the track's title, spaces and
//!    case ignored. Catches a file renamed by a downloader; needs no tags.
//! 3. Same title and artist in the tags (the file name stands in for a missing title, as the add
//!    flow does) and a length within 2.5 s of the track's, both known. Tags are read only for
//!    files that fail 1 and 2.
//!
//! Files matched by 2 or 3 are not offered, but they are listed in the result so a wrong guess
//! can be overridden with "Add tracks". No track location is turned into a file path here: the
//! library is only compared with, never read through.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use pm_formats::library::{Library, Track};
use pm_formats::naming;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::is_audio;
use crate::error::AppResult;

/// Serialises the read-modify-write of the settings and state files (a scan and an "ignore"
/// click must not both rewrite a file from the same starting point).
static WATCH_LOCK: Mutex<()> = Mutex::new(());

// ---- limits ------------------------------------------------------------------------------

const MAX_FOLDERS: usize = 20;
/// Directory entries looked at per scan, over all folders; a bigger tree is cut and says so.
const MAX_VISITED: usize = 1_000_000;
/// Candidates and tag matches listed in a result (the totals are always exact).
const MAX_LISTED_CANDIDATES: usize = 5000;
const MAX_LISTED_MATCHES: usize = 300;
const MAX_IGNORED: usize = 50_000;
const MAX_LOG: usize = 100;
const MAX_LOG_NAMES: usize = 200;
/// Length difference still counted as the same recording.
const DURATION_TOLERANCE_SECS: f64 = 2.5;
/// A file dated further than this ahead of the clock is a clock mix-up, not a write in progress.
const FUTURE_SLACK_MS: i64 = 60_000;

// ---- settings ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// New files are listed; adding them goes through the confirmation like any add.
    #[default]
    Ask,
    /// New files are added without the question (still a toast, a job strip and a log entry).
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Vec<String>,
    pub mode: Mode,
    /// Scan each time a library is opened.
    pub scan_on_open: bool,
    /// Scan every this many minutes while the app runs; 0 = never.
    pub interval_minutes: u32,
    pub subfolders: bool,
    /// A file changed less than this many seconds ago is left for the next scan (it may still be
    /// downloading or copying).
    pub min_age_secs: u32,
    /// "auto" adds at most this many files at once; a larger find is left for a confirmation.
    pub auto_max_files: u32,
    /// In "ask" mode: open the confirmation as soon as a scan finds files, instead of a toast.
    pub open_confirmation: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            folders: Vec::new(),
            mode: Mode::Ask,
            scan_on_open: true,
            interval_minutes: 0,
            subfolders: true,
            min_age_secs: 10,
            auto_max_files: 100,
            open_confirmation: false,
        }
    }
}

impl Settings {
    /// The settings clamped to what the scan supports; folders trimmed, de-duplicated and capped.
    pub fn sanitised(mut self) -> Settings {
        let mut seen = HashSet::new();
        self.folders = self
            .folders
            .iter()
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty() && seen.insert(norm_path(f)))
            .take(MAX_FOLDERS)
            .collect();
        self.interval_minutes = self.interval_minutes.min(1440);
        self.min_age_secs = self.min_age_secs.min(3600);
        self.auto_max_files = self.auto_max_files.clamp(1, 5000);
        self
    }

    /// Why these settings cannot be saved, if they cannot.
    fn check(&self) -> Result<(), String> {
        if self.folders.len() > MAX_FOLDERS {
            return Err(format!("At most {MAX_FOLDERS} folders can be watched."));
        }
        for f in &self.folders {
            if !Path::new(f.trim()).is_absolute() {
                return Err(format!("\"{f}\" is not a full folder path."));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub settings: Settings,
    /// Set when the saved file was damaged: it was moved aside and these are the defaults.
    pub problem: Option<String>,
}

// ---- paths and names ---------------------------------------------------------------------

/// A path in one comparable spelling: forward slashes, no trailing slash, folded to lower case
/// where the file system ignores case (Windows).
pub fn norm_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) { s.to_lowercase() } else { s.to_string() }
}

fn is_under(path: &str, dir: &str) -> bool {
    let (p, d) = (norm_path(path), norm_path(dir));
    p == d || p.strip_prefix(&d).is_some_and(|rest| rest.starts_with('/'))
}

/// Spaces collapsed and case folded: "Some  Name" and "some name" are the same title.
fn norm_text(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn stem_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

// ---- what the library has ----------------------------------------------------------------

/// The library's tracks, indexed for the three tests described in the module doc.
pub struct LibIndex {
    /// `tracks.location` (lower case) to track id.
    locations: HashMap<String, i64>,
    /// (title, artist) normalised to `(track id, length in s)`.
    by_tags: HashMap<(String, String), Vec<(i64, i64)>>,
    /// file size to `(track id, normalised title, normalised "artist - title")`.
    by_size: HashMap<i64, Vec<(i64, String, String)>>,
}

impl LibIndex {
    pub fn new(tracks: &[Track]) -> LibIndex {
        let mut ix = LibIndex { locations: HashMap::new(), by_tags: HashMap::new(), by_size: HashMap::new() };
        for t in tracks {
            ix.locations.insert(t.location.to_lowercase(), t.track_id);
            let title = norm_text(&t.title);
            if !title.is_empty() {
                ix.by_tags.entry((title.clone(), norm_text(&t.artist))).or_default().push((t.track_id, t.play_time_secs));
                if t.file_size > 0 {
                    let both = norm_text(&format!("{} - {}", t.artist, t.title));
                    ix.by_size.entry(t.file_size).or_default().push((t.track_id, title, both));
                }
            }
        }
        ix
    }
}

/// What the tags (or, failing that, the file name) say about a file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileTags {
    pub title: String,
    pub artist: String,
    pub duration_secs: Option<f64>,
}

/// Reads title, artist and length. Never fails: an unreadable file gives its name as the title
/// and no length (so it cannot match by tags), the same fallback the add flow stores.
pub fn read_tags(path: &Path) -> FileTags {
    let read = || -> Option<FileTags> {
        let tf = Probe::open(path).ok()?.options(ParseOptions::new().read_cover_art(false)).read().ok()?;
        let tag = tf.primary_tag().or_else(|| tf.first_tag());
        let secs = tf.properties().duration().as_secs_f64();
        Some(FileTags {
            title: tag.and_then(|t| t.title().map(|v| v.into_owned())).unwrap_or_default(),
            artist: tag.and_then(|t| t.artist().map(|v| v.into_owned())).unwrap_or_default(),
            duration_secs: (secs > 0.0).then_some(secs),
        })
    };
    // a codec panic on one odd file must not take a scan (and the command thread) down
    let mut tags = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read)).ok().flatten().unwrap_or_default();
    if tags.title.trim().is_empty() {
        tags.title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    }
    tags
}

/// How a file was recognised as already in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// Same file name and size (what `add_tracks` skips).
    NameAndSize,
    /// Same size, and the file name is the track's title.
    SizeAndTitle(i64),
    /// Same title, artist and length.
    Tags(i64),
}

fn by_name_and_size(ix: &LibIndex, name: &str, size: u64) -> bool {
    let (_, loc) = naming::device_paths(name, size);
    ix.locations.contains_key(&loc.to_lowercase())
}

fn by_size_and_title(ix: &LibIndex, name: &str, size: u64) -> Option<i64> {
    let stem = norm_text(stem_of(name));
    if stem.is_empty() {
        return None;
    }
    ix.by_size.get(&(size as i64))?.iter().find(|(_, title, both)| *title == stem || *both == stem).map(|(id, _, _)| *id)
}

fn by_tags(ix: &LibIndex, tags: &FileTags) -> Option<i64> {
    let secs = tags.duration_secs?;
    let cands = ix.by_tags.get(&(norm_text(&tags.title), norm_text(&tags.artist)))?;
    let close: Vec<i64> = cands
        .iter()
        .filter(|(_, len)| *len > 0 && (secs - *len as f64).abs() <= DURATION_TOLERANCE_SECS)
        .map(|(id, _)| *id)
        .collect();
    // two library tracks that look alike: a match would be a guess about which one
    (close.len() == 1).then(|| close[0])
}

// ---- the scan ----------------------------------------------------------------------------

/// A file the scan found that the library does not have.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
    /// The watched folder it was found under.
    pub folder: String,
}

/// A file left out because its tags or its name matched a library track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Matched {
    pub path: String,
    pub name: String,
    pub track_id: i64,
    pub how: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderReport {
    pub path: String,
    /// Why the folder was not searched (missing, unreadable, inside the library).
    pub error: Option<String>,
    pub audio_files: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Scan {
    pub at: i64,
    pub root: String,
    pub folders: Vec<FolderReport>,
    /// New files, newest first, at most 5000; see `candidates_total`.
    pub candidates: Vec<Candidate>,
    pub candidates_total: usize,
    /// Audio files found in the folders, ignored and already-in-library ones included.
    pub audio_files: usize,
    /// Same file name and size as a library track.
    pub in_library: usize,
    /// Matched by title / tags (listed in `matched`, not offered).
    pub matched_total: usize,
    pub matched: Vec<Matched>,
    /// Left out because the user dismissed them.
    pub ignored: usize,
    /// Left for the next scan because they changed less than `min_age_secs` ago.
    pub too_new: usize,
    /// A second copy (same name and size) of a file already listed.
    pub duplicates: usize,
    /// Files whose names are not valid text; the add flow cannot take them.
    pub unusable_names: usize,
    /// The folders hold more than the scan looks at.
    pub cut_short: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ignored {
    pub path: String,
    pub size: u64,
    pub at: i64,
}

struct Seen {
    path: PathBuf,
    size: u64,
    modified_ms: i64,
    folder: usize,
}

fn modified_ms(m: &std::fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Audio files under `dir` (symlinks and junctions are not followed, so a loop cannot recurse
/// for ever); `visited` counts directory entries over the whole scan. `skip` is a directory
/// whose files are never listed (the library itself).
fn walk(dir: &Path, folder: usize, subfolders: bool, skip: &str, visited: &mut usize, out: &mut Vec<Seen>) -> Result<(), String> {
    let mut stack = vec![dir.to_path_buf()];
    let mut first = true;
    while let Some(d) = stack.pop() {
        let rd = match std::fs::read_dir(&d) {
            Ok(rd) => rd,
            // only the folder the user named is an error; an unreadable subfolder is skipped
            Err(e) if first => return Err(e.to_string()),
            Err(_) => continue,
        };
        first = false;
        for e in rd.flatten() {
            *visited += 1;
            if *visited > MAX_VISITED {
                return Ok(());
            }
            let p = e.path();
            let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
            if m.is_dir() {
                if subfolders && !is_under(&p.to_string_lossy(), skip) {
                    stack.push(p);
                }
            } else if m.is_file() && is_audio(&p) && !is_under(&p.to_string_lossy(), skip) {
                out.push(Seen { path: p, size: m.len(), modified_ms: modified_ms(&m), folder });
            }
        }
    }
    Ok(())
}

/// True once a file has not changed for `min_age_secs`. A date more than a minute ahead of the
/// clock is taken as a wrong clock, not a write in progress.
pub fn is_settled(now_ms: i64, modified_ms: i64, min_age_secs: u32) -> bool {
    if min_age_secs == 0 {
        return true;
    }
    let age = now_ms - modified_ms;
    age < -FUTURE_SLACK_MS || age >= i64::from(min_age_secs) * 1000
}

/// Searches the watched folders of `settings` for audio that `index` does not know and the
/// ignore list does not cover. Reads the folders; writes nothing. `tags` reads a file's tags
/// (injected so the tests need no real audio).
pub fn scan_folders(
    settings: &Settings,
    library_root: &str,
    index: &LibIndex,
    ignored: &[Ignored],
    now_ms: i64,
    tags: &dyn Fn(&Path) -> FileTags,
) -> Scan {
    let mut scan = Scan { at: now_ms, root: library_root.to_string(), ..Scan::default() };
    let mut files: Vec<Seen> = Vec::new();
    let mut visited = 0usize;
    for (i, f) in settings.folders.iter().enumerate() {
        let mut rep = FolderReport { path: f.clone(), error: None, audio_files: 0 };
        let before = files.len();
        if is_under(f, library_root) {
            rep.error = Some("This folder is inside the open library; the library's own files are not watched.".into());
        } else if !Path::new(f).is_dir() {
            rep.error = Some("This folder was not found (is the drive connected?).".into());
        } else if let Err(e) = walk(Path::new(f), i, settings.subfolders, library_root, &mut visited, &mut files) {
            rep.error = Some(format!("Cannot be read: {e}"));
        }
        rep.audio_files = files.len() - before;
        scan.folders.push(rep);
    }
    scan.cut_short = visited > MAX_VISITED;
    // a file under two watched folders (one inside the other) is one file
    let mut seen_paths = HashSet::new();
    files.retain(|f| seen_paths.insert(norm_path(&f.path.to_string_lossy())));
    scan.audio_files = files.len();

    let ignore: HashSet<(String, u64)> = ignored.iter().map(|i| (norm_path(&i.path), i.size)).collect();
    let mut offered: HashSet<String> = HashSet::new();
    let mut found: Vec<Candidate> = Vec::new();
    for f in files {
        let (Some(path), Some(name)) = (f.path.to_str(), f.path.file_name().and_then(|n| n.to_str())) else {
            scan.unusable_names += 1;
            continue;
        };
        if ignore.contains(&(norm_path(path), f.size)) {
            scan.ignored += 1;
            continue;
        }
        let mut hit = by_name_and_size(index, name, f.size).then_some(Found::NameAndSize);
        if hit.is_none() {
            // a file still being written is judged on the next scan, once it is whole
            if !is_settled(now_ms, f.modified_ms, settings.min_age_secs) {
                scan.too_new += 1;
                continue;
            }
            hit = by_size_and_title(index, name, f.size)
                .map(Found::SizeAndTitle)
                .or_else(|| by_tags(index, &tags(&f.path)).map(Found::Tags));
        }
        match hit {
            Some(Found::NameAndSize) => scan.in_library += 1,
            Some(h @ (Found::SizeAndTitle(id) | Found::Tags(id))) => {
                scan.matched_total += 1;
                if scan.matched.len() < MAX_LISTED_MATCHES {
                    let how = if matches!(h, Found::Tags(_)) { "same title, artist and length" } else { "same size, file name is the title" };
                    scan.matched.push(Matched { path: path.to_string(), name: name.to_string(), track_id: id, how: how.into() });
                }
            }
            None => {
                // the add flow would skip the second of two files with one name and size
                let (_, loc) = naming::device_paths(name, f.size);
                if !offered.insert(loc.to_lowercase()) {
                    scan.duplicates += 1;
                    continue;
                }
                found.push(Candidate {
                    path: path.to_string(),
                    name: name.to_string(),
                    size: f.size,
                    modified_ms: f.modified_ms,
                    folder: settings.folders[f.folder].clone(),
                });
            }
        }
    }
    found.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then_with(|| a.path.cmp(&b.path)));
    scan.candidates_total = found.len();
    found.truncate(MAX_LISTED_CANDIDATES);
    scan.candidates = found;
    scan
}

// ---- storage -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub at: i64,
    /// "auto" or "ask".
    pub mode: String,
    /// Files handed to the add flow.
    pub requested: usize,
    /// Their names (at most 200).
    pub names: Vec<String>,
    pub ok: usize,
    pub failed: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchState {
    pub ignored: Vec<Ignored>,
    pub last_scan: Option<Scan>,
    /// Oldest first, at most 100.
    pub log: Vec<LogEntry>,
}

/// What the frontend gets: the state and a note when the file had to be replaced.
#[derive(Debug, Serialize)]
pub struct StateView {
    #[serde(flatten)]
    pub state: WatchState,
    pub problem: Option<String>,
}

fn key_of(root: &str) -> String {
    let norm = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a, as the smart-case store
    for b in norm.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn settings_path(dir: &Path) -> PathBuf {
    dir.join("watch-settings.json")
}

fn state_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("watch-state-{}.json", key_of(root)))
}

/// Temp file, fsync, rename: a crash leaves the old file or the new one, never half of either.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let res = (|| -> std::io::Result<()> {
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    res.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

/// Reads and parses a JSON file. `Ok(None)` when it does not exist; a file that does not parse
/// is moved aside (never deleted) and reported in the second part.
fn load_json<T: for<'de> Deserialize<'de>>(path: &Path, now_ms: i64) -> Result<(Option<T>, Option<String>), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((None, None)),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match serde_json::from_str::<T>(&text) {
        Ok(v) => Ok((Some(v), None)),
        Err(e) => {
            let aside = path.with_extension(format!("bad-{now_ms}"));
            std::fs::rename(path, &aside).map_err(|r| format!("{} is damaged ({e}) and could not be moved aside: {r}", path.display()))?;
            Ok((None, Some(format!("{} could not be read ({e}). It was kept as {}.", path.display(), aside.display()))))
        }
    }
}

pub fn load_settings(dir: &Path, now_ms: i64) -> Result<SettingsView, String> {
    let (s, problem) = load_json::<Settings>(&settings_path(dir), now_ms)?;
    Ok(SettingsView { settings: s.unwrap_or_default().sanitised(), problem })
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    write_atomic(&settings_path(dir), &text)
}

pub fn load_state(dir: &Path, root: &str, now_ms: i64) -> Result<StateView, String> {
    let (s, problem) = load_json::<WatchState>(&state_path(dir, root), now_ms)?;
    Ok(StateView { state: s.unwrap_or_default(), problem })
}

pub fn save_state(dir: &Path, root: &str, s: &WatchState) -> Result<(), String> {
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    write_atomic(&state_path(dir, root), &text)
}

/// Adds files to the ignore list (a file already on it is not added twice).
pub fn add_ignored(state: &mut WatchState, files: &[(String, u64)], now_ms: i64) {
    for (path, size) in files {
        let k = norm_path(path);
        if !state.ignored.iter().any(|i| norm_path(&i.path) == k && i.size == *size) {
            state.ignored.push(Ignored { path: path.clone(), size: *size, at: now_ms });
        }
    }
    let over = state.ignored.len().saturating_sub(MAX_IGNORED);
    state.ignored.drain(..over);
}

/// Removes these paths from the ignore list (every size they were ignored at).
pub fn remove_ignored(state: &mut WatchState, paths: &[String]) {
    let gone: HashSet<String> = paths.iter().map(|p| norm_path(p)).collect();
    state.ignored.retain(|i| !gone.contains(&norm_path(&i.path)));
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

pub fn push_log(state: &mut WatchState, mut e: LogEntry) {
    e.mode = if e.mode == "auto" { "auto".into() } else { "ask".into() };
    e.names.truncate(MAX_LOG_NAMES);
    e.names = e.names.iter().map(|n| clip(n, 260)).collect();
    e.summary = clip(&e.summary, 500);
    state.log.push(e);
    let over = state.log.len().saturating_sub(MAX_LOG);
    state.log.drain(..over);
}

// ---- commands ----------------------------------------------------------------------------

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

/// The watch-folder settings (defaults until saved: no folders, ask first, scan on open, no timer).
#[tauri::command]
pub async fn watch_settings_get(app: tauri::AppHandle) -> AppResult<SettingsView> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<SettingsView, String> {
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        load_settings(&dir, now_ms())
    })
    .await??)
}

/// Saves the settings (clamped) and returns them as stored. Changes nothing on the device.
#[tauri::command]
pub async fn watch_settings_set(app: tauri::AppHandle, settings: Settings) -> AppResult<Settings> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<Settings, String> {
        let s = settings.sanitised();
        s.check()?;
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        save_settings(&dir, &s)?;
        Ok(s)
    })
    .await??)
}

/// Searches the watched folders (from the saved settings) for audio the library on `root` does
/// not have, remembers the result as the library's last scan and returns it. Reads the folders
/// and the library read-only; writes only the app config dir.
#[tauri::command]
pub async fn watch_scan(app: tauri::AppHandle, root: String) -> AppResult<Scan> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Scan> {
        let now = now_ms();
        let settings = {
            let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            load_settings(&dir, now)?.settings
        };
        let tracks = Library::open_read_only(Path::new(&root))?.tracks()?;
        let index = LibIndex::new(&tracks);
        let ignored = {
            let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            load_state(&dir, &root, now)?.state.ignored
        };
        // the long part (walking folders, reading tags) runs without the lock held
        let mut scan = scan_folders(&settings, &root, &index, &ignored, now, &read_tags);
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // read again: an "ignore" may have been saved while the scan ran
        let mut view = load_state(&dir, &root, now)?;
        if view.state.ignored.len() != ignored.len() {
            let now_ignored: Vec<(String, u64)> = view.state.ignored.iter().map(|i| (i.path.clone(), i.size)).collect();
            drop_candidates(&mut scan, &now_ignored);
        }
        view.state.last_scan = Some(scan.clone());
        save_state(&dir, &root, &view.state)?;
        Ok(scan)
    })
    .await?
}

/// The library's last scan, ignore list and log of automatic adds.
#[tauri::command]
pub async fn watch_state(app: tauri::AppHandle, root: String) -> AppResult<StateView> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<StateView, String> {
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        load_state(&dir, &root, now_ms())
    })
    .await??)
}

#[derive(Debug, Deserialize)]
pub struct IgnoreArg {
    pub path: String,
    pub size: u64,
}

/// Puts files on the library's ignore list (they are not offered again while unchanged). The
/// files themselves are not touched.
#[tauri::command]
pub async fn watch_ignore(app: tauri::AppHandle, root: String, files: Vec<IgnoreArg>) -> AppResult<StateView> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<StateView, String> {
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let now = now_ms();
        let mut view = load_state(&dir, &root, now)?;
        let args: Vec<(String, u64)> = files.into_iter().map(|f| (f.path, f.size)).collect();
        add_ignored(&mut view.state, &args, now);
        // the files just ignored leave the remembered result too, so the list agrees at once
        if let Some(scan) = view.state.last_scan.as_mut() {
            drop_candidates(scan, &args);
        }
        save_state(&dir, &root, &view.state)?;
        Ok(view)
    })
    .await??)
}

/// Removes ignored files from a scan's candidate list and counts them as ignored.
fn drop_candidates(scan: &mut Scan, files: &[(String, u64)]) {
    let gone: HashSet<(String, u64)> = files.iter().map(|(p, s)| (norm_path(p), *s)).collect();
    let before = scan.candidates.len();
    scan.candidates.retain(|c| !gone.contains(&(norm_path(&c.path), c.size)));
    let removed = before - scan.candidates.len();
    scan.candidates_total = scan.candidates_total.saturating_sub(removed);
    scan.ignored += removed;
}

/// Takes files off the ignore list; the next scan offers them again.
#[tauri::command]
pub async fn watch_unignore(app: tauri::AppHandle, root: String, paths: Vec<String>) -> AppResult<StateView> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<StateView, String> {
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut view = load_state(&dir, &root, now_ms())?;
        remove_ignored(&mut view.state, &paths);
        save_state(&dir, &root, &view.state)?;
        Ok(view)
    })
    .await??)
}

/// What the frontend reports about an add it started from a scan.
#[derive(Debug, Deserialize)]
pub struct LogArg {
    pub mode: String,
    pub requested: usize,
    pub names: Vec<String>,
    pub ok: usize,
    pub failed: usize,
    pub summary: String,
}

/// Appends an entry to the library's log of watch-folder adds.
#[tauri::command]
pub async fn watch_log_add(app: tauri::AppHandle, root: String, entry: LogArg) -> AppResult<StateView> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<StateView, String> {
        let _held = WATCH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let now = now_ms();
        let mut view = load_state(&dir, &root, now)?;
        let e = LogEntry { at: now, mode: entry.mode, requested: entry.requested, names: entry.names, ok: entry.ok, failed: entry.failed, summary: entry.summary };
        push_log(&mut view.state, e);
        save_state(&dir, &root, &view.state)?;
        Ok(view)
    })
    .await??)
}

#[cfg(test)]
mod tests;
