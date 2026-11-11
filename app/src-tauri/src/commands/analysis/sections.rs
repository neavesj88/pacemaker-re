//! Phrase / section detection (intro, build, drop, breakdown, outro), app side: a background job
//! that measures bar features from the audio, an app-side cache of them, and the commands that
//! turn the cache into sections.
//!
//! The device cannot store sections, so nothing here touches `music.db` or the card's files:
//! results live in `<app config dir>/sections-<hash of the library root>.json`, written temp +
//! fsync + rename, and a corrupt file is treated as empty (and reported) rather than failing the
//! app. The cache holds what the *audio* said per bar (`pm_analysis::sections::Row`) and the bar
//! edges taken from the track's beat grid at that time, not the sections: the sensitivity,
//! minimum section length and phrase settings all apply when the sections are read, with no new
//! decode. The sections in the table, the grid editor and any preview all come from
//! [`compute_state`], so a preview and the action it previews cannot differ.
//!
//! A bar is only as good as the grid it came from: the cache remembers the size and modified
//! time of each track's `.str`, and a run for "new or changed" tracks measures again those whose
//! grid was edited since (the state command does not look at the card, so until then the markers
//! keep the bar lines they were measured against).
//!
//! Audio and `.str` paths come through the validated [`super::audio_path`] (never
//! `Library::track_paths`), and the job holds the shared `JobLock`, so Eject, backup and other
//! jobs cannot overlap a read of the card.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::UNIX_EPOCH;

use pm_analysis::decode;
use pm_analysis::sections::{self as core, Detection, NoStructure, Params, Row, Skip};
use pm_formats::library::Library;
use pm_formats::{naming, strfile};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::energy::{atomic_write, root_hash};
use crate::error::{AppError, AppResult, IntoApp};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

/// The job kind string (`jobs.ask({ kind: "sections" })` on the frontend).
pub const SECTIONS_JOB: &str = "sections";
/// The cache is written to disk after this many finished tracks.
const FLUSH_EVERY: usize = 25;
/// A `.str` larger than this is not read (real ones are a few tens of kilobytes).
const MAX_STR_BYTES: u64 = 16 * 1024 * 1024;

// ---- settings --------------------------------------------------------------------------------

/// Where the phrase grid (8 / 16 / 32 bars) starts.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// At the first downbeat of the beat grid.
    FirstDownbeat,
    /// Wherever most strong boundaries of the track fall (moves only for a clear gain).
    BestFit,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// 1 (only obvious changes) to 5 (small changes count).
    pub sensitivity: u8,
    /// No two section boundaries closer than this many bars (4 to 32).
    pub min_section_bars: usize,
    /// Phrase length boundaries snap to: 4, 8, 16, 32, or 0 to choose from each track.
    pub phrase_bars: usize,
    pub origin: Origin,
    /// A boundary moves onto the phrase grid only if it is at most this many bars away (0 to 8).
    pub snap_tolerance_bars: usize,
    /// Tracks with fewer bars than this get no sections shown.
    pub min_bars: usize,
    /// Tracks decoded at once by the job. Each holds one decoded track in memory.
    pub threads: usize,
    /// Draw section markers on waveforms.
    pub show_markers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { sensitivity: 3, min_section_bars: 8, phrase_bars: 0, origin: Origin::FirstDownbeat, snap_tolerance_bars: 2, min_bars: 32, threads: 2, show_markers: true }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=5).contains(&self.sensitivity) {
            return Err("Sensitivity must be between 1 and 5.".into());
        }
        if !(4..=32).contains(&self.min_section_bars) {
            return Err("The shortest section must be between 4 and 32 bars.".into());
        }
        if ![0, 4, 8, 16, 32].contains(&self.phrase_bars) {
            return Err("The phrase length must be automatic, 4, 8, 16 or 32 bars.".into());
        }
        if self.snap_tolerance_bars > 8 {
            return Err("The snap distance must be between 0 and 8 bars.".into());
        }
        if !(16..=512).contains(&self.min_bars) {
            return Err("The shortest track must be between 16 and 512 bars.".into());
        }
        if !(1..=8).contains(&self.threads) {
            return Err("Tracks analysed at once must be between 1 and 8.".into());
        }
        Ok(())
    }

    fn params(&self) -> Params {
        Params {
            sensitivity: self.sensitivity,
            min_section_bars: self.min_section_bars,
            phrase_bars: self.phrase_bars,
            best_fit_phase: self.origin == Origin::BestFit,
            snap_tolerance_bars: self.snap_tolerance_bars,
        }
    }
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("sections-settings.json")
}

/// Saved settings, or the defaults when there is no file or it cannot be read or is out of range.
pub fn load_settings(dir: &Path) -> Settings {
    std::fs::read_to_string(settings_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .filter(|s| s.validate().is_ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    s.validate()?;
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    atomic_write(&settings_path(dir), text.as_bytes())
}

// ---- cache -----------------------------------------------------------------------------------

/// Size and modified time of a `.str`, to tell that its grid was edited.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
struct GridStamp {
    len: u64,
    mtime_ms: i64,
}

fn stamp_of(path: &Path) -> Option<GridStamp> {
    let m = std::fs::metadata(path).ok()?;
    let mtime_ms = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64);
    Some(GridStamp { len: m.len(), mtime_ms })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Entry {
    /// `tracks.file_size` when it was measured; a different size later means a different file.
    size: i64,
    /// The track's `.str` when it was measured (`None`: there was none).
    #[serde(default)]
    grid: Option<GridStamp>,
    #[serde(default)]
    duration_secs: f64,
    /// Bar start times in seconds, one more than `rows`.
    #[serde(default)]
    edges: Vec<f64>,
    #[serde(default)]
    rows: Vec<Row>,
    /// Why the track gave nothing to measure (no grid, no downbeat, too short, silent).
    #[serde(default)]
    skip: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct CacheFile {
    #[serde(default)]
    version: u32,
    /// By `tracks.location`, which is stable across database rewrites.
    #[serde(default)]
    tracks: BTreeMap<String, Entry>,
}

/// The file for a library, named by a hash of its root (same normalisation as the energy cache),
/// so nothing about the device is written next to it.
pub fn cache_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("sections-{:016x}.json", root_hash(root)))
}

/// The cache for a library. A missing file is an empty cache; an unreadable or other-version
/// file is also an empty one, with the reason returned so the UI can say results were dropped.
fn load_cache(dir: &Path, root: &str) -> (CacheFile, Option<String>) {
    let p = cache_path(dir, root);
    match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<CacheFile>(&s) {
            Ok(c) if c.version == core::VERSION => (c, None),
            Ok(_) => (CacheFile::default(), Some("The saved section results come from an older method and will be measured again.".into())),
            Err(e) => (CacheFile::default(), Some(format!("The saved section results could not be read ({e}); they will be measured again."))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (CacheFile::default(), None),
        Err(e) => (CacheFile::default(), Some(format!("The saved section results could not be opened ({e})."))),
    }
}

fn save_cache(dir: &Path, root: &str, cache: &CacheFile) -> Result<(), String> {
    let text = serde_json::to_string(&CacheFile { version: core::VERSION, tracks: cache.tracks.clone() }).map_err(|e| e.to_string())?;
    atomic_write(&cache_path(dir, root), text.as_bytes())
}

// ---- reading the cache -----------------------------------------------------------------------

/// Whether the track's `.str` was looked at (only the plan does, see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GridCheck {
    Unchecked,
    Checked(Option<GridStamp>),
}

/// What this module needs to know about a library track.
#[derive(Clone, Debug)]
pub struct TrackRef {
    pub id: i64,
    pub location: String,
    pub size: i64,
    /// Words for the progress strip.
    pub label: String,
    grid: GridCheck,
}

enum Status<'a> {
    /// Measured, and sections were found.
    Ready(Detection, &'a Entry),
    /// Measured, but nothing changes enough to call a section.
    Flat,
    /// Nothing to measure, or too short for the settings: the reason.
    Skipped(String),
    /// Measured for a file of a different size, or for a grid that has been edited.
    Stale,
    /// Never measured.
    None,
}

fn status_of<'a>(t: &TrackRef, cache: &'a CacheFile, settings: &Settings) -> Status<'a> {
    let Some(e) = cache.tracks.get(&t.location) else { return Status::None };
    if e.size != t.size {
        return Status::Stale;
    }
    if let GridCheck::Checked(now) = t.grid {
        if now != e.grid {
            return Status::Stale;
        }
    }
    if let Some(why) = &e.skip {
        return Status::Skipped(why.clone());
    }
    if e.rows.is_empty() || e.edges.len() != e.rows.len() + 1 {
        return Status::Stale;
    }
    if e.rows.len() < settings.min_bars {
        return Status::Skipped(format!("fewer than {} bars", settings.min_bars));
    }
    match core::detect(&e.rows, &settings.params()) {
        Ok(d) => Status::Ready(d, e),
        Err(NoStructure::Flat) => Status::Flat,
        Err(NoStructure::TooShort) => Status::Skipped(NoStructure::TooShort.reason().into()),
    }
}

/// One section on the wire. Times are seconds from the start of the audio in the same time base
/// as the beat grid, so they line up with the bar lines of the grid editor.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SectionOut {
    /// "intro", "build", "drop", "breakdown", "outro" or "section".
    pub kind: &'static str,
    pub start_secs: f64,
    pub end_secs: f64,
    /// Bar index from the first downbeat of the grid (0 = the first bar).
    pub start_bar: usize,
    pub end_bar: usize,
    /// 0 to 1, relative to this track: for ranking and fading markers, not a probability.
    pub confidence: f32,
    /// 0 to 1: how strong the boundary at the start is.
    pub start_confidence: f32,
    /// The start sits on the phrase grid.
    pub on_phrase: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TrackSections {
    pub track_id: i64,
    pub bars: usize,
    /// The phrase length boundaries were snapped to; 0 when no phrase grid fits the track.
    pub phrase_bars: usize,
    pub sections: Vec<SectionOut>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SkipReason {
    pub reason: String,
    pub count: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SectionsState {
    /// Tracks that have sections, in library order.
    pub entries: Vec<TrackSections>,
    pub total: usize,
    /// Tracks with at least one boundary found (`entries.len()`).
    pub ready: usize,
    /// Measured, but no clear sections at the current sensitivity.
    pub flat: usize,
    /// Nothing to measure (no grid, no first downbeat, silent) or too short for the settings.
    pub skipped: usize,
    pub skip_reasons: Vec<SkipReason>,
    /// Measured for a file that has since changed size.
    pub stale: usize,
    /// Never analysed.
    pub none: usize,
    pub settings: Settings,
    /// The saved results could not be read and were dropped.
    pub problem: Option<String>,
}

fn ms(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn sections_out(d: &Detection, e: &Entry) -> Vec<SectionOut> {
    d.sections
        .iter()
        .map(|s| SectionOut {
            kind: s.kind.as_str(),
            start_secs: if s.start_bar == 0 { 0.0 } else { ms(e.edges[s.start_bar]) },
            end_secs: if s.end_bar == d.bars { ms(e.duration_secs.max(e.edges[d.bars])) } else { ms(e.edges[s.end_bar]) },
            start_bar: s.start_bar,
            end_bar: s.end_bar,
            confidence: s.confidence,
            start_confidence: s.start_confidence,
            on_phrase: s.on_phrase,
        })
        .collect()
}

/// The sections of every track that has them, and the counts the UI shows.
fn compute_state(tracks: &[TrackRef], cache: &CacheFile, settings: &Settings, problem: Option<String>) -> SectionsState {
    let mut entries = Vec::new();
    let (mut flat, mut skipped, mut stale, mut none) = (0, 0, 0, 0);
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for t in tracks {
        match status_of(t, cache, settings) {
            Status::Ready(d, e) => entries.push(TrackSections { track_id: t.id, bars: d.bars, phrase_bars: d.phrase_bars, sections: sections_out(&d, e) }),
            Status::Flat => flat += 1,
            Status::Skipped(why) => {
                skipped += 1;
                *reasons.entry(why).or_default() += 1;
            }
            Status::Stale => stale += 1,
            Status::None => none += 1,
        }
    }
    let mut skip_reasons: Vec<SkipReason> = reasons.into_iter().map(|(reason, count)| SkipReason { reason, count }).collect();
    skip_reasons.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.reason.cmp(&b.reason)));
    SectionsState { ready: entries.len(), entries, total: tracks.len(), flat, skipped, skip_reasons, stale, none, settings: *settings, problem }
}

fn refs_of(lib: &Library) -> Result<Vec<TrackRef>, pm_formats::library::Error> {
    Ok(lib
        .tracks()?
        .into_iter()
        .map(|t| TrackRef { label: super::label_of(&t), id: t.track_id, location: t.location, size: t.file_size, grid: GridCheck::Unchecked })
        .collect())
}

/// The sections (as the table shows them) of every track of the library at `root` that has
/// some, by track id. The read API for other features (the mix editor's snapping, a set
/// builder): the same sections under the settings in force.
#[allow(dead_code)] // the read API for the mix editor and the set builder; no caller yet
pub fn sections_by_track(dir: &Path, root: &str, lib: &Library) -> Result<HashMap<i64, Vec<SectionOut>>, String> {
    let (cache, problem) = load_cache(dir, root);
    let state = compute_state(&refs_of(lib).map_err(|e| e.to_string())?, &cache, &load_settings(dir), problem);
    Ok(state.entries.into_iter().map(|e| (e.track_id, e.sections)).collect())
}

// ---- the job ---------------------------------------------------------------------------------

/// The `.str` path of a track, from the validated audio path.
fn sidecar_of(root: &Path, location: &str) -> Result<(PathBuf, PathBuf), String> {
    let audio = super::audio_path(root, location).map_err(|e| e.to_string())?;
    let sidecar = PathBuf::from(naming::str_path(&audio.to_string_lossy()));
    Ok((audio, sidecar))
}

/// Looks at the `.str` of every track in `wanted` (all when `None`) that has a result of the
/// right size, so a grid edited since shows up as stale. Tracks whose location is not valid are
/// left unchecked.
fn check_grids(root: &Path, tracks: &mut [TrackRef], cache: &CacheFile, wanted: Option<&HashSet<i64>>) {
    for t in tracks.iter_mut() {
        if wanted.is_some_and(|w| !w.contains(&t.id)) || !cache.tracks.get(&t.location).is_some_and(|e| e.size == t.size) {
            continue;
        }
        if let Ok((_, sidecar)) = sidecar_of(root, &t.location) {
            t.grid = GridCheck::Checked(stamp_of(&sidecar));
        }
    }
}

/// Which tracks a run covers. Without `recompute`, only tracks never analysed or whose file or
/// grid changed; with it, every named track again. `ids` limits the run to those tracks.
fn pick_targets(tracks: &[TrackRef], cache: &CacheFile, ids: Option<&[i64]>, recompute: bool, settings: &Settings) -> Vec<TrackRef> {
    let wanted: Option<HashSet<i64>> = ids.map(|v| v.iter().copied().collect());
    tracks
        .iter()
        .filter(|t| wanted.as_ref().is_none_or(|w| w.contains(&t.id)))
        .filter(|t| recompute || matches!(status_of(t, cache, settings), Status::None | Status::Stale))
        .cloned()
        .collect()
}

struct Measured {
    grid: Option<GridStamp>,
    duration_secs: f64,
    edges: Vec<f64>,
    rows: Vec<Row>,
}

enum Measure {
    Done(Measured),
    Skip(Skip, Option<GridStamp>),
}

fn read_grid(sidecar: &Path) -> Result<strfile::Analysis, Skip> {
    let len = std::fs::metadata(sidecar).map_err(|_| Skip::NoGrid)?.len();
    if len == 0 || len > MAX_STR_BYTES {
        return Err(Skip::NoGrid);
    }
    let bytes = std::fs::read(sidecar).map_err(|_| Skip::NoGrid)?;
    strfile::decode(&String::from_utf8_lossy(&bytes)).map_err(|_| Skip::NoGrid)
}

fn measure(root: &Path, t: &TrackRef) -> Result<Measure, String> {
    // the same validated resolution the waveform uses: plain names below /pmdb_tracks/ only
    let (audio, sidecar) = sidecar_of(root, &t.location)?;
    if !audio.is_file() {
        return Err("the audio file is missing from the device".into());
    }
    // the stamp is taken before the grid is read: an edit that lands in between shows up as stale
    let stamp = stamp_of(&sidecar);
    let grid = match read_grid(&sidecar) {
        Ok(g) => g,
        Err(skip) => return Ok(Measure::Skip(skip, stamp)),
    };
    if grid.first_downbeat.is_none() {
        return Ok(Measure::Skip(Skip::NoDownbeat, stamp));
    }
    let decoded = match catch_unwind(AssertUnwindSafe(|| decode::decode(&audio))) {
        Err(_) => return Err("the decoder crashed on this file".into()),
        Ok(Err(e)) => return Err(e.to_string()),
        Ok(Ok(d)) => d,
    };
    let sr = decoded.sample_rate.max(1) as f64;
    let duration = decoded.mono.len() as f64 / sr;
    let beat_secs: Vec<f64> = grid.beats.iter().map(|b| b.pos as f64 / sr).collect();
    let edges = match core::bar_edges(&beat_secs, grid.first_downbeat, duration) {
        Ok(e) => e,
        Err(skip) => return Ok(Measure::Skip(skip, stamp)),
    };
    match core::measure(&decoded.mono, decoded.sample_rate, &edges) {
        Ok(rows) => Ok(Measure::Done(Measured { grid: stamp, duration_secs: (duration * 1000.0).round() / 1000.0, edges: edges.iter().map(|&x| ms(x)).collect(), rows })),
        Err(skip) => Ok(Measure::Skip(skip, stamp)),
    }
}

/// The body of the job, apart from Tauri so it can be tested on a temp library. Decodes
/// `targets` on `settings.threads` workers, emits one progress per finished track, and saves the
/// cache every [`FLUSH_EVERY`] tracks and at the end.
fn run_analysis(dir: &Path, root: &str, targets: &[TrackRef], settings: &Settings, mut progress: impl FnMut(Progress)) -> Result<JobOutcome, String> {
    let (mut cache, _) = load_cache(dir, root);
    let total = targets.len();
    let (mut ok, mut failed, mut skipped, mut unsaved) = (0usize, 0usize, 0usize, 0usize);
    let mut save_failed = false;
    let workers = settings.threads.clamp(1, 8).min(total.max(1));
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<(usize, Result<Measure, String>)>();
    let root_path = Path::new(root);

    std::thread::scope(|s| {
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            s.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= total {
                    break;
                }
                if tx.send((i, measure(root_path, &targets[i]))).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        for (done, (i, result)) in rx.iter().enumerate() {
            let t = &targets[i];
            let mut p = Progress::new(SECTIONS_JOB, t.label.clone(), done + 1, total, "");
            match result {
                Ok(Measure::Done(m)) => {
                    let entry = Entry { size: t.size, grid: m.grid, duration_secs: m.duration_secs, edges: m.edges, rows: m.rows, skip: None };
                    // the figure on the strip comes from the same detection the table uses
                    let one = TrackRef { grid: GridCheck::Unchecked, ..t.clone() };
                    let mut probe = CacheFile::default();
                    probe.tracks.insert(t.location.clone(), entry.clone());
                    p.status = "analysed".into();
                    p.detail = Some(match status_of(&one, &probe, settings) {
                        Status::Ready(d, _) => format!("{} sections in {} bars", d.sections.len(), d.bars),
                        Status::Flat => "no clear sections at this sensitivity".into(),
                        Status::Skipped(why) => why,
                        Status::Stale | Status::None => String::new(),
                    });
                    cache.tracks.insert(t.location.clone(), entry);
                    ok += 1;
                    unsaved += 1;
                }
                Ok(Measure::Skip(why, grid)) => {
                    p.status = "skipped".into();
                    p.detail = Some(why.reason().into());
                    cache.tracks.insert(t.location.clone(), Entry { size: t.size, grid, duration_secs: 0.0, edges: Vec::new(), rows: Vec::new(), skip: Some(why.reason().into()) });
                    skipped += 1;
                    unsaved += 1;
                }
                Err(e) => {
                    p.status = "failed".into();
                    p.error = Some(e);
                    failed += 1;
                }
            }
            progress(p);
            if unsaved >= FLUSH_EVERY {
                match save_cache(dir, root, &cache) {
                    Ok(()) => {
                        unsaved = 0;
                        save_failed = false;
                    }
                    Err(_) => save_failed = true,
                }
            }
        }
    });
    if unsaved > 0 || save_failed {
        save_cache(dir, root, &cache).map_err(|e| format!("The results could not be saved: {e}"))?;
    }
    let mut parts = Vec::new();
    if ok > 0 {
        parts.push(format!("{ok} analysed"));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} with nothing to measure (no usable grid or first downbeat, too short, or silent)"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    let summary = if parts.is_empty() { "nothing to do".to_string() } else { parts.join(", ") };
    Ok(JobOutcome { ok: ok + skipped, failed, summary })
}

// ---- commands --------------------------------------------------------------------------------

fn config_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

/// Every track's sections under the saved settings, with the counts for the UI. Read-only: it
/// opens the library read-only and reads the app-side cache; nothing on the device is touched.
#[tauri::command]
pub async fn analysis_sections_state(app: AppHandle, root: String) -> AppResult<SectionsState> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SectionsState> {
        let lib = Library::open_read_only(Path::new(&root))?;
        let refs = refs_of(&lib)?;
        let (cache, problem) = load_cache(&dir, &root);
        Ok(compute_state(&refs, &cache, &load_settings(&dir), problem))
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))?
}

#[tauri::command]
pub fn analysis_sections_settings(app: AppHandle) -> AppResult<Settings> {
    Ok(load_settings(&config_dir(&app)?))
}

/// Saves the settings after checking every value; returns what was saved.
#[tauri::command]
pub fn analysis_sections_save_settings(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    save_settings(&config_dir(&app)?, &settings).app()?;
    Ok(settings)
}

/// Opens the library read-only and works out which tracks a run would cover. The one place the
/// plan is made: `analysis_sections_plan` shows it, `analysis_sections_analyse` runs it.
fn plan(app: &AppHandle, root: &str, track_ids: Option<&[i64]>, recompute: bool) -> AppResult<(PathBuf, Settings, Vec<TrackRef>)> {
    let dir = config_dir(app)?;
    let settings = load_settings(&dir);
    let lib = Library::open_read_only(Path::new(root))?;
    let mut refs = refs_of(&lib)?;
    drop(lib);
    let (cache, _) = load_cache(&dir, root);
    let wanted: Option<HashSet<i64>> = track_ids.map(|v| v.iter().copied().collect());
    check_grids(Path::new(root), &mut refs, &cache, wanted.as_ref());
    let targets = pick_targets(&refs, &cache, track_ids, recompute, &settings);
    Ok((dir, settings, targets))
}

/// The ids a run with these arguments would analyse, in library order, so the confirmation can
/// name every track. Same selection code as the run itself.
#[tauri::command]
pub async fn analysis_sections_plan(app: AppHandle, root: String, track_ids: Option<Vec<i64>>, recompute: Option<bool>) -> AppResult<Vec<i64>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<i64>> {
        let (_, _, targets) = plan(&app, &root, track_ids.as_deref(), recompute.unwrap_or(false))?;
        Ok(targets.into_iter().map(|t| t.id).collect())
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))?
}

/// Starts the "sections" job. `track_ids` limits it to those tracks (default: the whole
/// library). Without `recompute` only tracks never analysed, or whose file or grid changed, are
/// done. Reads audio and `.str` files from the device and writes only the app-side cache.
#[tauri::command]
pub async fn analysis_sections_analyse(
    app: AppHandle,
    lock: State<'_, JobLock>,
    root: String,
    track_ids: Option<Vec<i64>>,
    recompute: Option<bool>,
) -> AppResult<()> {
    if track_ids.as_ref().is_some_and(|ids| ids.is_empty()) {
        return Err("No tracks to analyse.".into());
    }
    // choose the tracks before taking the lock, so an empty run reports itself without a job;
    // reading the library, the cache and the grids' stamps is off the main thread
    let (plan_app, plan_root, plan_ids) = (app.clone(), root.clone(), track_ids.clone());
    let (dir, settings, targets) = tauri::async_runtime::spawn_blocking(move || plan(&plan_app, &plan_root, plan_ids.as_deref(), recompute.unwrap_or(false)))
        .await
        .map_err(|e| AppError::from(e.to_string()))??;
    if targets.is_empty() {
        return Err("Nothing to analyse: those tracks were already analysed. Choose Analyse again to measure them anew.".into());
    }
    spawn_job(app, &lock, SECTIONS_JOB, move |app| {
        run_analysis(&dir, &root, &targets, &settings, |p| events::emit(app, JOB_PROGRESS, p)).map_err(AppError::from)
    })
}

/// Forgets every saved section result of the library. Refused while a job runs. Returns how many
/// results were dropped.
#[tauri::command]
pub fn analysis_sections_clear(app: AppHandle, lock: State<'_, JobLock>, root: String) -> AppResult<usize> {
    if lock.is_held() {
        return Err("A job is still running.".into());
    }
    clear_cache(&config_dir(&app)?, &root).app()
}

fn clear_cache(dir: &Path, root: &str) -> Result<usize, String> {
    let (cache, _) = load_cache(dir, root);
    match std::fs::remove_file(cache_path(dir, root)) {
        Ok(()) => Ok(cache.tracks.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;

    const SR: u32 = 22050;
    const BPM: f64 = 128.0;
    const BAR_SECS: f64 = 4.0 * 60.0 / BPM;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-sections-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn wav_bytes(samples: &[f32]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&SR.to_le_bytes());
        w.extend_from_slice(&(SR * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        w
    }

    /// 64 bars at 128 BPM: a quiet pad for 16 bars, a loud kick groove for 32, a quiet pad for 16.
    fn structured() -> Vec<f32> {
        let n = (64.0 * BAR_SECS * SR as f64) as usize + SR as usize;
        let beat = 60.0 / BPM;
        (0..n)
            .map(|i| {
                let t = i as f64 / SR as f64;
                let bar = t / BAR_SECS;
                let pad = 0.03 * (2.0 * std::f64::consts::PI * 220.0 * t).sin();
                if (16.0..48.0).contains(&bar) {
                    let b = (t / beat).fract() * beat;
                    let kick = 0.9 * (2.0 * std::f64::consts::PI * 55.0 * b).sin() * (-b * 14.0).exp();
                    (kick + 0.3 * (2.0 * std::f64::consts::PI * 45.0 * t).sin()) as f32
                } else {
                    pad as f32
                }
            })
            .collect()
    }

    fn grid_analysis(bars: usize, first_downbeat: Option<usize>) -> strfile::Analysis {
        let beat = SR as f64 * 60.0 / BPM;
        strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..bars * 4).map(|i| strfile::Beat { pos: (i as f64 * beat).round() as i64, bpm: BPM }).collect(),
            first_downbeat,
        }
    }

    fn library(tag: &str, files: &[(&str, Vec<u8>, strfile::Analysis)]) -> (Library, PathBuf) {
        let base = temp_dir(tag);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let mut lib = Library::create(&vol).unwrap();
        for (name, bytes, analysis) in files {
            let p = src.join(name);
            std::fs::write(&p, bytes).unwrap();
            let meta = TrackMeta { title: name.to_string(), format: "WAV".into(), ..Default::default() };
            lib.add_track(&p, &meta, analysis).unwrap();
        }
        (lib, base)
    }

    fn tref(id: i64, loc: &str, size: i64) -> TrackRef {
        TrackRef { id, location: loc.into(), size, label: format!("Demo {id}"), grid: GridCheck::Unchecked }
    }

    /// Bar rows with a quiet bar-level intro, a loud middle and a quiet end.
    fn step_rows(bars: usize) -> Vec<Row> {
        (0..bars)
            .map(|i| if i < bars / 4 || i >= bars * 3 / 4 { [-40.0, -30.0, -30.0, -35.0, 20.0] } else { [10.0, 0.0, -5.0, -10.0, 150.0] })
            .collect()
    }

    fn entry(size: i64, rows: Vec<Row>, skip: Option<&str>) -> Entry {
        let n = rows.len();
        let edges = if n > 0 { (0..=n).map(|k| 1.0 + k as f64 * 2.0).collect() } else { Vec::new() };
        Entry { size, grid: None, duration_secs: 1.0 + n as f64 * 2.0 + 0.5, edges, rows, skip: skip.map(String::from) }
    }

    #[test]
    fn status_covers_every_case() {
        let mut c = CacheFile::default();
        let s = Settings::default();
        c.tracks.insert("/ok".into(), entry(10, step_rows(64), None));
        c.tracks.insert("/flat".into(), entry(10, vec![[-20.0, -20.0, -20.0, -20.0, 50.0]; 64], None));
        c.tracks.insert("/short".into(), entry(10, step_rows(24), None));
        c.tracks.insert("/nogrid".into(), entry(10, vec![], Some("no usable beat grid")));
        c.tracks.insert("/torn".into(), Entry { edges: vec![1.0], ..entry(10, step_rows(64), None) });
        assert!(matches!(status_of(&tref(1, "/ok", 10), &c, &s), Status::Ready(..)));
        assert!(matches!(status_of(&tref(2, "/flat", 10), &c, &s), Status::Flat));
        assert!(matches!(status_of(&tref(3, "/short", 10), &c, &s), Status::Skipped(w) if w == "fewer than 32 bars"));
        // lowering the setting brings the short one back without any new decode
        assert!(matches!(status_of(&tref(3, "/short", 10), &c, &Settings { min_bars: 16, ..s }), Status::Ready(..)));
        assert!(matches!(status_of(&tref(4, "/nogrid", 10), &c, &s), Status::Skipped(w) if w == "no usable beat grid"));
        assert!(matches!(status_of(&tref(5, "/ok", 11), &c, &s), Status::Stale), "a different file size is a different file");
        assert!(matches!(status_of(&tref(6, "/torn", 10), &c, &s), Status::Stale), "rows and edges that do not match are measured again");
        assert!(matches!(status_of(&tref(7, "/never", 10), &c, &s), Status::None));
        // an edited grid is noticed only where the .str was looked at
        let looked = GridCheck::Checked(Some(GridStamp { len: 5, mtime_ms: 5 }));
        assert!(matches!(status_of(&TrackRef { grid: looked, ..tref(1, "/ok", 10) }, &c, &s), Status::Stale));
        assert!(matches!(status_of(&TrackRef { grid: GridCheck::Checked(None), ..tref(1, "/ok", 10) }, &c, &s), Status::Ready(..)));
    }

    #[test]
    fn state_gives_times_counts_and_follows_the_settings() {
        let mut c = CacheFile::default();
        c.tracks.insert("/ok".into(), entry(10, step_rows(64), None));
        c.tracks.insert("/flat".into(), entry(10, vec![[-20.0, -20.0, -20.0, -20.0, 50.0]; 64], None));
        c.tracks.insert("/a".into(), entry(1, vec![], Some("silent")));
        c.tracks.insert("/b".into(), entry(1, vec![], Some("silent")));
        c.tracks.insert("/c".into(), entry(1, vec![], Some("no usable beat grid")));
        let tracks = vec![tref(1, "/ok", 10), tref(2, "/flat", 10), tref(3, "/a", 1), tref(4, "/b", 1), tref(5, "/c", 1), tref(6, "/never", 1), tref(7, "/ok", 99)];
        let s = compute_state(&tracks, &c, &Settings::default(), None);
        assert_eq!((s.total, s.ready, s.flat, s.skipped, s.stale, s.none), (7, 1, 1, 3, 1, 1));
        assert_eq!(s.skip_reasons, vec![SkipReason { reason: "silent".into(), count: 2 }, SkipReason { reason: "no usable beat grid".into(), count: 1 }]);
        let t = &s.entries[0];
        assert_eq!((t.track_id, t.bars), (1, 64));
        let kinds: Vec<_> = t.sections.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec!["intro", "drop", "outro"]);
        // edges are 1.0 + 2 s per bar; the first section starts at the top of the file and the
        // last ends at the stored duration, not at the last bar line
        assert_eq!(t.sections[0].start_secs, 0.0);
        assert_eq!(t.sections[1].start_secs, 1.0 + 16.0 * 2.0);
        assert_eq!(t.sections[1].start_bar, 16);
        assert_eq!(t.sections[1].end_secs, t.sections[2].start_secs);
        assert_eq!(t.sections[2].end_secs, 1.0 + 64.0 * 2.0 + 0.5);
        assert!(t.sections.windows(2).all(|w| w[0].end_secs == w[1].start_secs));
        // a setting changes the sections with no new decode: a 64-bar minimum track length hides it
        let strict = compute_state(&tracks, &c, &Settings { min_bars: 100, ..Settings::default() }, None);
        assert_eq!((strict.ready, strict.skipped), (0, 5));
        // and the same tracks give the same answer twice
        assert_eq!(compute_state(&tracks, &c, &Settings::default(), None), s);
    }

    #[test]
    fn settings_validate_and_round_trip_and_survive_junk() {
        let dir = temp_dir("settings");
        assert_eq!(load_settings(&dir), Settings::default());
        let s = Settings { sensitivity: 4, min_section_bars: 16, phrase_bars: 16, origin: Origin::BestFit, snap_tolerance_bars: 1, min_bars: 48, threads: 4, show_markers: false };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), s);
        for bad in [
            Settings { sensitivity: 0, ..s },
            Settings { sensitivity: 6, ..s },
            Settings { min_section_bars: 3, ..s },
            Settings { min_section_bars: 33, ..s },
            Settings { phrase_bars: 12, ..s },
            Settings { snap_tolerance_bars: 9, ..s },
            Settings { min_bars: 15, ..s },
            Settings { min_bars: 513, ..s },
            Settings { threads: 0, ..s },
            Settings { threads: 9, ..s },
        ] {
            assert!(save_settings(&dir, &bad).is_err(), "{bad:?}");
        }
        assert_eq!(load_settings(&dir), s, "a refused save leaves the old file");
        std::fs::write(settings_path(&dir), "{ not json").unwrap();
        assert_eq!(load_settings(&dir), Settings::default());
        std::fs::write(settings_path(&dir), r#"{"threads": 99}"#).unwrap();
        assert_eq!(load_settings(&dir), Settings::default(), "out-of-range values are not trusted");
        std::fs::write(settings_path(&dir), r#"{"origin": "best_fit"}"#).unwrap();
        assert_eq!(load_settings(&dir).origin, Origin::BestFit, "missing fields take their defaults");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_cache_is_per_library_atomic_and_tolerates_corruption() {
        let dir = temp_dir("cache");
        assert_eq!(cache_path(&dir, "P:\\"), cache_path(&dir, "p:/"));
        assert_ne!(cache_path(&dir, "P:\\"), cache_path(&dir, "Q:\\"));
        assert_ne!(cache_path(&dir, "P:\\"), super::super::energy::cache_path(&dir, "P:\\"), "energy and sections never share a file");
        let (empty, problem) = load_cache(&dir, "P:\\");
        assert!(empty.tracks.is_empty() && problem.is_none());

        let mut c = CacheFile::default();
        c.tracks.insert("/a".into(), entry(1, step_rows(40), None));
        c.tracks.insert("/b".into(), Entry { grid: Some(GridStamp { len: 9, mtime_ms: 123 }), ..entry(2, vec![], Some("silent")) });
        save_cache(&dir, "P:\\", &c).unwrap();
        let (back, problem) = load_cache(&dir, "p:/");
        assert!(problem.is_none());
        assert_eq!(back.tracks, c.tracks);
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")), "no temp file left");

        std::fs::write(cache_path(&dir, "P:\\"), "garbage").unwrap();
        let (dropped, problem) = load_cache(&dir, "P:\\");
        assert!(dropped.tracks.is_empty());
        assert!(problem.unwrap().contains("could not be read"));
        std::fs::write(cache_path(&dir, "P:\\"), r#"{"version":0,"tracks":{}}"#).unwrap();
        assert!(load_cache(&dir, "P:\\").1.unwrap().contains("older method"));

        save_cache(&dir, "P:\\", &c).unwrap();
        assert_eq!(clear_cache(&dir, "P:\\").unwrap(), 2);
        assert_eq!(clear_cache(&dir, "P:\\").unwrap(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn targets_are_the_missing_by_default_and_everything_named_on_recompute() {
        let s = Settings::default();
        let mut c = CacheFile::default();
        c.tracks.insert("/done".into(), entry(1, step_rows(64), None));
        c.tracks.insert("/changed".into(), entry(1, step_rows(64), None));
        c.tracks.insert("/silent".into(), entry(1, vec![], Some("silent")));
        c.tracks.insert("/regrid".into(), Entry { grid: Some(GridStamp { len: 1, mtime_ms: 1 }), ..entry(1, step_rows(64), None) });
        let regrid = TrackRef { grid: GridCheck::Checked(Some(GridStamp { len: 2, mtime_ms: 2 })), ..tref(5, "/regrid", 1) };
        let tracks = vec![tref(1, "/done", 1), tref(2, "/changed", 2), tref(3, "/new", 1), tref(4, "/silent", 1), regrid];
        let ids = |v: Vec<TrackRef>| v.into_iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(ids(pick_targets(&tracks, &c, None, false, &s)), vec![2, 3, 5], "new, resized and re-gridded; a skip stays skipped");
        assert_eq!(ids(pick_targets(&tracks, &c, None, true, &s)), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids(pick_targets(&tracks, &c, Some(&[1, 3]), false, &s)), vec![3]);
        assert_eq!(ids(pick_targets(&tracks, &c, Some(&[1, 3, 99]), true, &s)), vec![1, 3]);
        assert!(pick_targets(&tracks, &c, Some(&[1]), false, &s).is_empty());
    }

    #[test]
    fn the_job_measures_caches_and_reports_each_outcome() {
        let wav = wav_bytes(&structured());
        let (mut lib, base) = library(
            "job",
            &[
                ("structured.wav", wav.clone(), grid_analysis(64, Some(0))),
                ("nodownbeat.wav", wav, grid_analysis(64, None)),
                ("broken.wav", b"this is not audio".to_vec(), grid_analysis(64, Some(0))),
            ],
        );
        let root = lib.root().to_string_lossy().to_string();
        let cache_dir = base.join("config");
        let tracks = refs_of(&lib).unwrap();
        assert_eq!(tracks.len(), 3);
        let settings = Settings { threads: 3, ..Settings::default() };

        let mut events = Vec::new();
        let out = run_analysis(&cache_dir, &root, &tracks, &settings, |p| events.push((p.label.clone(), p.status.clone(), p.done, p.total, p.detail.clone()))).unwrap();
        assert_eq!((out.ok, out.failed), (2, 1), "{}", out.summary);
        assert!(events.iter().enumerate().all(|(i, e)| e.2 == i + 1 && e.3 == 3), "progress counts 1..=total in order: {events:?}");
        let analysed = events.iter().find(|e| e.1 == "analysed").unwrap();
        assert!(analysed.0.contains("structured"));
        assert_eq!(analysed.4.as_deref(), Some("3 sections in 64 bars"));
        assert!(events.iter().any(|e| e.1 == "skipped" && e.0.contains("nodownbeat") && e.4.as_deref().is_some_and(|d| d.contains("first downbeat"))));
        assert!(events.iter().any(|e| e.1 == "failed" && e.0.contains("broken")));

        let (cache, problem) = load_cache(&cache_dir, &root);
        let state = compute_state(&tracks, &cache, &settings, problem);
        assert_eq!((state.ready, state.skipped, state.none, state.stale), (1, 1, 1, 0));
        let t = &state.entries[0];
        let kinds: Vec<_> = t.sections.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec!["intro", "drop", "outro"], "{:?}", t.sections);
        // the true changes are at bars 16 and 48: 30 s and 90 s into the file
        assert!((t.sections[1].start_secs - 16.0 * BAR_SECS).abs() < 0.05, "{}", t.sections[1].start_secs);
        assert!((t.sections[2].start_secs - 48.0 * BAR_SECS).abs() < 0.05, "{}", t.sections[2].start_secs);
        assert_eq!((t.sections[1].start_bar, t.sections[2].start_bar), (16, 48));
        assert!(t.sections[1].on_phrase && t.sections[2].on_phrase);
        // the read API gives the same sections
        let read = sections_by_track(&cache_dir, &root, &lib).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[&t.track_id], t.sections);

        // one worker and three workers measure identically
        let cache_dir1 = base.join("config1");
        let first_two: Vec<TrackRef> = tracks.iter().filter(|t| !t.label.contains("broken")).cloned().collect();
        run_analysis(&cache_dir1, &root, &first_two, &Settings { threads: 1, ..settings }, |_| {}).unwrap();
        let a = load_cache(&cache_dir1, &root).0.tracks;
        let b = load_cache(&cache_dir, &root).0.tracks;
        for (loc, e) in &a {
            let e2 = &b[loc];
            assert_eq!((&e.rows, &e.edges, &e.skip), (&e2.rows, &e2.edges, &e2.skip));
        }

        // running again with nothing new picks only the failed track
        let mut again_refs = refs_of(&lib).unwrap();
        check_grids(Path::new(&root), &mut again_refs, &cache, None);
        let again = pick_targets(&again_refs, &cache, None, false, &settings);
        assert_eq!(again.len(), 1);
        assert!(again[0].label.contains("broken"));

        // editing the grid of the structured track makes it due again, and only it
        let id = tracks.iter().find(|t| t.label.contains("structured")).unwrap().id;
        let loc = tracks.iter().find(|t| t.id == id).unwrap().location.clone();
        lib.store_analysis(id, &loc, &grid_analysis(60, Some(1))).unwrap();
        let mut regridded = refs_of(&lib).unwrap();
        check_grids(Path::new(&root), &mut regridded, &cache, None);
        let due: Vec<_> = pick_targets(&regridded, &cache, None, false, &settings).into_iter().map(|t| t.id).collect();
        assert!(due.contains(&id), "{due:?}");
        assert_eq!(due.len(), 2, "the re-gridded track and the failed one");

        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_location_outside_the_music_folder_is_refused_not_read() {
        let (lib, base) = library("escape", &[("a.wav", wav_bytes(&structured()), grid_analysis(64, Some(0)))]);
        let root = lib.root().to_string_lossy().to_string();
        std::fs::write(base.join("outside.wav"), wav_bytes(&structured())).unwrap();
        let cache_dir = base.join("config");
        let bad = ["/pmdb_tracks/../../outside.wav", "/elsewhere/a.wav", "/pmdb_tracks//abs.wav", ""];
        let mut tracks: Vec<TrackRef> = bad.iter().enumerate().map(|(i, l)| tref(i as i64 + 1, l, 1)).collect();
        let mut statuses = Vec::new();
        let out = run_analysis(&cache_dir, &root, &tracks, &Settings::default(), |p| statuses.push((p.status.clone(), p.error.clone()))).unwrap();
        assert_eq!((out.ok, out.failed), (0, 4), "{}", out.summary);
        assert!(statuses.iter().all(|s| s.0 == "failed"));
        assert!(load_cache(&cache_dir, &root).0.tracks.is_empty(), "nothing is cached for a refused track");
        // and the grid check never stats them either
        let mut cache = CacheFile::default();
        for t in &tracks {
            cache.tracks.insert(t.location.clone(), entry(1, step_rows(64), None));
        }
        check_grids(Path::new(&root), &mut tracks, &cache, None);
        assert!(tracks.iter().all(|t| t.grid == GridCheck::Unchecked));
        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn an_empty_run_writes_nothing() {
        let dir = temp_dir("empty");
        let out = run_analysis(&dir, "P:\\", &[], &Settings::default(), |_| {}).unwrap();
        assert_eq!((out.ok, out.failed, out.summary.as_str()), (0, 0, "nothing to do"));
        assert!(!cache_path(&dir, "P:\\").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_long_run_flushes_part_way() {
        // more tracks than FLUSH_EVERY, all with no first downbeat: they are cached as skips with no decode
        let dir = temp_dir("flush");
        let names: Vec<String> = (0..FLUSH_EVERY + 3).map(|i| format!("s{i}.wav")).collect();
        let files: Vec<(&str, Vec<u8>, strfile::Analysis)> = names.iter().map(|n| (n.as_str(), wav_bytes(&vec![0.0; SR as usize]), grid_analysis(8, None))).collect();
        let (lib, base) = library("flush-lib", &files);
        let root = lib.root().to_string_lossy().to_string();
        let tracks = refs_of(&lib).unwrap();
        let mut seen_file_mid_run = false;
        let mut n = 0;
        run_analysis(&dir, &root, &tracks, &Settings::default(), |_| {
            n += 1;
            if n == FLUSH_EVERY + 1 {
                seen_file_mid_run = cache_path(&dir, &root).exists();
            }
        })
        .unwrap();
        assert!(seen_file_mid_run, "the cache is on disk before the run ends");
        assert_eq!(load_cache(&dir, &root).0.tracks.len(), FLUSH_EVERY + 3);
        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
