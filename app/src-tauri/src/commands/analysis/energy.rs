//! Energy analysis (1 to 10), app side: a background job over selected or all tracks, an
//! app-side cache of what was measured, and the commands that turn the cache into numbers.
//!
//! The device cannot store an energy value, so nothing here touches `music.db` or the card's
//! files: results live in `<app config dir>/energy-<hash of the library root>.json`, written
//! temp + fsync + rename, and a corrupt file is treated as empty (and reported) rather than
//! failing the app. The cache holds what the *audio* said (`pm_analysis::energy::Features`), not
//! the final number, so the scale setting, the library's BPM and the fit all apply when the
//! numbers are read, with no new decode. The number in the table, in the gap map and in any
//! preview all come from [`compute_state`].
//!
//! Audio is read through the validated [`super::audio_path`] (never `Library::track_paths`), and
//! the job holds the shared `JobLock`, so Eject, backup and other jobs cannot overlap a read of
//! the card.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use pm_analysis::energy::{self, Anchors, Features, Skip};
use pm_formats::library::Library;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult, IntoApp};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

/// The job kind string (`jobs.ask({ kind: "energy" })` on the frontend).
pub const ENERGY_JOB: &str = "energy";
/// The cache is written to disk after this many finished tracks, so closing the app part way
/// through a long run keeps what was done.
const FLUSH_EVERY: usize = 25;

// ---- settings --------------------------------------------------------------------------------

/// How the 0..1 score becomes 1..10.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    /// Stretch so the library's own spread (between two percentiles) covers 1 to 10.
    Fit,
    /// Fixed anchors: a track gets the same number whatever else is in the library.
    Fixed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub scale: Scale,
    /// Percentile of the library's scores that becomes energy 1 (fit only).
    pub fit_low_pct: f32,
    /// Percentile that becomes energy 10 (fit only).
    pub fit_high_pct: f32,
    /// Tracks shorter than this (seconds) get no energy shown.
    pub min_secs: f32,
    /// Tracks decoded at once by the job. Each holds one decoded track in memory.
    pub threads: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { scale: Scale::Fit, fit_low_pct: 5.0, fit_high_pct: 95.0, min_secs: 20.0, threads: 2 }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.fit_low_pct.is_finite() && (0.0..=40.0).contains(&self.fit_low_pct)) {
            return Err("The low percentile must be between 0 and 40.".into());
        }
        if !(self.fit_high_pct.is_finite() && (60.0..=100.0).contains(&self.fit_high_pct)) {
            return Err("The high percentile must be between 60 and 100.".into());
        }
        if !(self.min_secs.is_finite() && (energy::MIN_SECS as f32..=600.0).contains(&self.min_secs)) {
            return Err(format!("The shortest track must be between {} and 600 seconds.", energy::MIN_SECS));
        }
        if !(1..=8).contains(&self.threads) {
            return Err("Tracks analysed at once must be between 1 and 8.".into());
        }
        Ok(())
    }
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("energy-settings.json")
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

/// The measured part of a track, as stored.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct Measured {
    duration_secs: f32,
    loudness_db: f32,
    onsets_per_sec: f32,
    low_end: f32,
}

impl From<Features> for Measured {
    fn from(f: Features) -> Self {
        Measured { duration_secs: f.duration_secs, loudness_db: f.loudness_db, onsets_per_sec: f.onsets_per_sec, low_end: f.low_end }
    }
}

impl From<Measured> for Features {
    fn from(m: Measured) -> Self {
        Features { duration_secs: m.duration_secs, loudness_db: m.loudness_db, onsets_per_sec: m.onsets_per_sec, low_end: m.low_end }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Entry {
    /// `tracks.file_size` when it was measured; a different size later means a different file.
    size: i64,
    #[serde(default)]
    measured: Option<Measured>,
    /// Why the audio gave no estimate (too short, silent).
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

/// The file for a library, named by a hash of its root (same normalisation as the smart-case
/// store: case and slash style do not matter), so nothing about the device is written next to it.
pub fn cache_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("energy-{:016x}.json", root_hash(root)))
}

/// FNV-1a of the normalised library root: the per-library part of every app-side file name of
/// the analysis area (energy, sections).
pub(super) fn root_hash(root: &str) -> u64 {
    let norm = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in norm.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    Ok(())
}

/// The cache for a library. A missing file is an empty cache; an unreadable or other-version
/// file is also an empty one, with the reason returned so the UI can say results were dropped.
fn load_cache(dir: &Path, root: &str) -> (CacheFile, Option<String>) {
    let p = cache_path(dir, root);
    match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<CacheFile>(&s) {
            Ok(c) if c.version == energy::VERSION => (c, None),
            Ok(_) => (CacheFile::default(), Some("The saved energy results come from an older method and will be measured again.".into())),
            Err(e) => (CacheFile::default(), Some(format!("The saved energy results could not be read ({e}); they will be measured again."))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (CacheFile::default(), None),
        Err(e) => (CacheFile::default(), Some(format!("The saved energy results could not be opened ({e})."))),
    }
}

fn save_cache(dir: &Path, root: &str, cache: &CacheFile) -> Result<(), String> {
    let text = serde_json::to_string(&CacheFile { version: energy::VERSION, tracks: cache.tracks.clone() }).map_err(|e| e.to_string())?;
    atomic_write(&cache_path(dir, root), text.as_bytes())
}

// ---- reading the cache -----------------------------------------------------------------------

/// What this module needs to know about a library track.
#[derive(Clone, Debug)]
pub struct TrackRef {
    pub id: i64,
    pub location: String,
    pub size: i64,
    pub bpm: f64,
    /// `tracks.analysed`. The stored BPM of a track that was never analysed is not trusted (the
    /// Tracks filter and the gap map ignore it too), so it does not feed the tempo term.
    pub analysed: bool,
    /// Words for the progress strip.
    pub label: String,
}

#[derive(Debug, PartialEq)]
enum Status {
    /// Measured and long enough.
    Ready(Measured),
    /// Measured or tried, but no estimate: the reason.
    Skipped(String),
    /// Measured for a file of a different size.
    Stale,
    /// Never measured.
    None,
}

fn status_of(t: &TrackRef, cache: &CacheFile, min_secs: f32) -> Status {
    let Some(e) = cache.tracks.get(&t.location) else { return Status::None };
    if e.size != t.size {
        return Status::Stale;
    }
    match (&e.measured, &e.skip) {
        (Some(m), _) if m.duration_secs < min_secs => Status::Skipped(format!("shorter than {min_secs:.0} s")),
        (Some(m), _) => Status::Ready(*m),
        (None, Some(why)) => Status::Skipped(why.clone()),
        (None, None) => Status::None,
    }
}

fn bpm_of(t: &TrackRef) -> Option<f64> {
    (t.analysed && t.bpm.is_finite() && t.bpm > 0.0).then_some(t.bpm)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EnergyEntry {
    pub track_id: i64,
    /// 1.0 to 10.0, one decimal.
    pub energy: f32,
    /// The whole level, 1 to 10.
    pub level: u8,
    /// The 0..1 score before the scale.
    pub score: f32,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct EnergyState {
    pub entries: Vec<EnergyEntry>,
    pub total: usize,
    pub ready: usize,
    /// Tracks the audio gave no estimate for (too short, silent), or shorter than the setting.
    pub skipped: usize,
    /// Measured for a file that has since changed size.
    pub stale: usize,
    /// Never analysed.
    pub none: usize,
    /// "fit" or "fixed": the scale actually applied.
    pub scale_used: String,
    pub anchor_lo: f32,
    pub anchor_hi: f32,
    /// Why the fit was not used when it was asked for.
    pub note: Option<String>,
    pub settings: Settings,
    /// The saved results could not be read and were dropped.
    pub problem: Option<String>,
}

fn round1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

/// The numbers for every track that has them, and the counts the UI shows.
fn compute_state(tracks: &[TrackRef], cache: &CacheFile, settings: &Settings, problem: Option<String>) -> EnergyState {
    let mut scored: Vec<(i64, f32)> = Vec::new();
    let (mut skipped, mut stale, mut none) = (0, 0, 0);
    for t in tracks {
        match status_of(t, cache, settings.min_secs) {
            Status::Ready(m) => scored.push((t.id, energy::score(&Features::from(m), bpm_of(t)))),
            Status::Skipped(_) => skipped += 1,
            Status::Stale => stale += 1,
            Status::None => none += 1,
        }
    }
    let mut note = None;
    let (anchors, used) = match settings.scale {
        Scale::Fixed => (Anchors::FIXED, "fixed"),
        Scale::Fit => {
            let scores: Vec<f32> = scored.iter().map(|s| s.1).collect();
            match energy::fit_anchors(&scores, settings.fit_low_pct, settings.fit_high_pct) {
                Some(a) => (a, "fit"),
                None => {
                    note = Some(format!(
                        "The fixed scale is in use: fitting to the library needs at least {} analysed tracks with a real spread between them.",
                        energy::MIN_FIT_TRACKS
                    ));
                    (Anchors::FIXED, "fixed")
                }
            }
        }
    };
    let entries: Vec<EnergyEntry> = scored
        .iter()
        .map(|&(track_id, score)| {
            let e = round1(anchors.energy(score));
            EnergyEntry { track_id, energy: e, level: e.round().clamp(1.0, 10.0) as u8, score: (score * 1000.0).round() / 1000.0 }
        })
        .collect();
    EnergyState {
        ready: entries.len(),
        entries,
        total: tracks.len(),
        skipped,
        stale,
        none,
        scale_used: used.into(),
        anchor_lo: anchors.lo,
        anchor_hi: anchors.hi,
        note,
        settings: *settings,
        problem,
    }
}

fn refs_of(lib: &Library) -> Result<Vec<TrackRef>, pm_formats::library::Error> {
    Ok(lib
        .tracks()?
        .into_iter()
        .map(|t| TrackRef { label: super::label_of(&t), id: t.track_id, location: t.location, size: t.file_size, bpm: t.bpm, analysed: t.analysed })
        .collect())
}

/// The auto energy (1.0 to 10.0) of every track of the library at `root` that has one, by
/// track id. The read API for other features (set builder, smart cases): it is the same number
/// the table shows, under the settings in force. It is not the user's own energy rating.
#[allow(dead_code)] // the read API for the set builder and smart cases; no caller yet
pub fn energy_by_track(dir: &Path, root: &str, lib: &Library) -> Result<HashMap<i64, f32>, String> {
    let (cache, problem) = load_cache(dir, root);
    let state = compute_state(&refs_of(lib).map_err(|e| e.to_string())?, &cache, &load_settings(dir), problem);
    Ok(state.entries.into_iter().map(|e| (e.track_id, e.energy)).collect())
}

// ---- the job ---------------------------------------------------------------------------------

/// Which tracks a run covers. Without `recompute`, only tracks never analysed or whose file
/// changed; with it, every named track again. `ids` limits the run to those tracks.
fn pick_targets(tracks: &[TrackRef], cache: &CacheFile, ids: Option<&[i64]>, recompute: bool, min_secs: f32) -> Vec<TrackRef> {
    let wanted: Option<std::collections::HashSet<i64>> = ids.map(|v| v.iter().copied().collect());
    tracks
        .iter()
        .filter(|t| wanted.as_ref().is_none_or(|w| w.contains(&t.id)))
        .filter(|t| recompute || matches!(status_of(t, cache, min_secs), Status::None | Status::Stale))
        .cloned()
        .collect()
}

enum Measure {
    Done(Measured),
    Skip(Skip),
}

fn measure(root: &Path, t: &TrackRef) -> Result<Measure, String> {
    // the same validated resolution the waveform uses: plain names below /pmdb_tracks/ only
    let audio = super::audio_path(root, &t.location).map_err(|e| e.to_string())?;
    if !audio.is_file() {
        return Err("the audio file is missing from the device".into());
    }
    match catch_unwind(AssertUnwindSafe(|| energy::analyze_file(&audio))) {
        Err(_) => Err("the decoder crashed on this file".into()),
        Ok(Err(e)) => Err(e.to_string()),
        Ok(Ok(Err(skip))) => Ok(Measure::Skip(skip)),
        Ok(Ok(Ok(f))) => Ok(Measure::Done(f.into())),
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
            let mut p = Progress::new(ENERGY_JOB, t.label.clone(), done + 1, total, "");
            match result {
                Ok(Measure::Done(m)) => {
                    // the figure on the strip uses the fixed scale: the fitted one needs the whole library
                    let e = Anchors::FIXED.energy(energy::score(&Features::from(m), bpm_of(t)));
                    p.status = "analysed".into();
                    p.detail = Some(format!("energy about {:.1}", round1(e)));
                    cache.tracks.insert(t.location.clone(), Entry { size: t.size, measured: Some(m), skip: None });
                    ok += 1;
                    unsaved += 1;
                }
                Ok(Measure::Skip(why)) => {
                    p.status = "skipped".into();
                    p.detail = Some(why.reason().into());
                    cache.tracks.insert(t.location.clone(), Entry { size: t.size, measured: None, skip: Some(why.reason().into()) });
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
        parts.push(format!("{skipped} with no estimate (too short or silent)"));
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

/// Every track's auto energy under the saved settings, with the counts for the UI (analysed,
/// no estimate, changed since, never analysed). Read-only: it opens the library read-only and
/// reads the app-side cache; nothing on the device is touched.
#[tauri::command]
pub async fn analysis_energy_state(app: AppHandle, root: String) -> AppResult<EnergyState> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<EnergyState> {
        let lib = Library::open_read_only(Path::new(&root))?;
        let refs = refs_of(&lib)?;
        let (cache, problem) = load_cache(&dir, &root);
        Ok(compute_state(&refs, &cache, &load_settings(&dir), problem))
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))?
}

#[tauri::command]
pub fn analysis_energy_settings(app: AppHandle) -> AppResult<Settings> {
    Ok(load_settings(&config_dir(&app)?))
}

/// Saves the settings after checking every value; returns what was saved.
#[tauri::command]
pub fn analysis_energy_save_settings(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    save_settings(&config_dir(&app)?, &settings).app()?;
    Ok(settings)
}

/// Opens the library read-only and works out which tracks a run would cover. The one place the
/// plan is made: `analysis_energy_plan` shows it, `analysis_energy_analyse` runs it.
fn plan(app: &AppHandle, root: &str, track_ids: Option<&[i64]>, recompute: bool) -> AppResult<(PathBuf, Settings, Vec<TrackRef>)> {
    let dir = config_dir(app)?;
    let settings = load_settings(&dir);
    let lib = Library::open_read_only(Path::new(root))?;
    let refs = refs_of(&lib)?;
    drop(lib);
    let (cache, _) = load_cache(&dir, root);
    let targets = pick_targets(&refs, &cache, track_ids, recompute, settings.min_secs);
    Ok((dir, settings, targets))
}

/// The ids a run with these arguments would analyse, in library order, so the confirmation can
/// name every track. Same selection code as the run itself.
#[tauri::command]
pub async fn analysis_energy_plan(app: AppHandle, root: String, track_ids: Option<Vec<i64>>, recompute: Option<bool>) -> AppResult<Vec<i64>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<i64>> {
        let (_, _, targets) = plan(&app, &root, track_ids.as_deref(), recompute.unwrap_or(false))?;
        Ok(targets.into_iter().map(|t| t.id).collect())
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))?
}

/// Starts the "energy" job. `track_ids` limits it to those tracks (default: the whole
/// library). Without `recompute` only tracks never analysed, or whose file changed, are done.
/// Reads audio from the device and writes only the app-side cache.
#[tauri::command]
pub async fn analysis_energy_analyse(
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
    // reading the library and the cache is off the main thread, which a large library would stall
    let (plan_app, plan_root, plan_ids) = (app.clone(), root.clone(), track_ids.clone());
    let (dir, settings, targets) = tauri::async_runtime::spawn_blocking(move || plan(&plan_app, &plan_root, plan_ids.as_deref(), recompute.unwrap_or(false)))
        .await
        .map_err(|e| AppError::from(e.to_string()))??;
    if targets.is_empty() {
        return Err("Nothing to analyse: those tracks were already analysed. Choose Analyse again to measure them anew.".into());
    }
    spawn_job(app, &lock, ENERGY_JOB, move |app| {
        run_analysis(&dir, &root, &targets, &settings, |p| events::emit(app, JOB_PROGRESS, p)).map_err(AppError::from)
    })
}

/// Forgets every saved energy result of the library. Refused while a job runs. Returns how many
/// results were dropped.
#[tauri::command]
pub fn analysis_energy_clear(app: AppHandle, lock: State<'_, JobLock>, root: String) -> AppResult<usize> {
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
    use pm_formats::strfile;

    const SR: u32 = 22050;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-energy-{tag}-{}", std::process::id()));
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

    /// A quiet sustained tone.
    fn pad(secs: f32) -> Vec<f32> {
        (0..(SR as f32 * secs) as usize).map(|i| 0.04 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR as f32).sin()).collect()
    }

    /// A loud kick-and-hat groove at 150 BPM.
    fn groove(secs: f32) -> Vec<f32> {
        let n = (SR as f32 * secs) as usize;
        let mut out = vec![0f32; n];
        let beat = (SR as f32 * 60.0 / 150.0) as usize;
        let mut k = 0;
        let mut x = 12345u32;
        while k * beat < n {
            for i in 0..(SR as usize / 4) {
                let t = i as f32 / SR as f32;
                let fade = (1.0 - i as f32 / (SR as f32 / 4.0)).powi(2);
                if k * beat + i < n {
                    out[k * beat + i] += 0.95 * (2.0 * std::f32::consts::PI * (55.0 * t + 40.0 * (1.0 - (-t * 30.0).exp()) / 30.0)).sin() * (-t * 12.0).exp() * fade;
                }
                let h = k * beat + beat / 2 + i;
                if i < SR as usize / 20 && h < n {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    out[h] += 0.5 * ((x as f32 / u32::MAX as f32) * 2.0 - 1.0) * (-t * 90.0).exp();
                }
            }
            k += 1;
        }
        out
    }

    fn library(tag: &str, files: &[(&str, Vec<u8>)]) -> (Library, PathBuf) {
        let base = temp_dir(tag);
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
        for (name, bytes) in files {
            let p = src.join(name);
            std::fs::write(&p, bytes).unwrap();
            let meta = TrackMeta { title: name.to_string(), format: "WAV".into(), ..Default::default() };
            lib.add_track(&p, &meta, &analysis).unwrap();
        }
        (lib, base)
    }

    fn tref(id: i64, loc: &str, size: i64, bpm: f64) -> TrackRef {
        TrackRef { id, location: loc.into(), size, bpm, analysed: true, label: format!("Demo {id}") }
    }

    fn measured(loud: f32, onsets: f32, low: f32, dur: f32) -> Measured {
        Measured { duration_secs: dur, loudness_db: loud, onsets_per_sec: onsets, low_end: low }
    }

    fn entry(size: i64, m: Option<Measured>, skip: Option<&str>) -> Entry {
        Entry { size, measured: m, skip: skip.map(String::from) }
    }

    #[test]
    fn status_covers_every_case() {
        let mut c = CacheFile::default();
        c.tracks.insert("/a".into(), entry(10, Some(measured(-14.0, 3.0, 0.4, 200.0)), None));
        c.tracks.insert("/short".into(), entry(10, Some(measured(-14.0, 3.0, 0.4, 12.0)), None));
        c.tracks.insert("/silent".into(), entry(10, None, Some("silent")));
        c.tracks.insert("/changed".into(), entry(10, Some(measured(-14.0, 3.0, 0.4, 200.0)), None));
        assert!(matches!(status_of(&tref(1, "/a", 10, 0.0), &c, 20.0), Status::Ready(_)));
        assert_eq!(status_of(&tref(2, "/short", 10, 0.0), &c, 20.0), Status::Skipped("shorter than 20 s".into()));
        // lowering the setting brings the short one back without any new decode
        assert!(matches!(status_of(&tref(2, "/short", 10, 0.0), &c, 10.0), Status::Ready(_)));
        assert_eq!(status_of(&tref(3, "/silent", 10, 0.0), &c, 20.0), Status::Skipped("silent".into()));
        assert_eq!(status_of(&tref(4, "/changed", 11, 0.0), &c, 20.0), Status::Stale);
        assert_eq!(status_of(&tref(5, "/never", 10, 0.0), &c, 20.0), Status::None);
    }

    #[test]
    fn state_counts_and_numbers_follow_the_settings() {
        let mut c = CacheFile::default();
        let mut tracks = Vec::new();
        // 30 tracks of rising intensity, so a fit is possible
        for i in 0..30 {
            let loc = format!("/t{i}");
            let x = i as f32 / 29.0;
            c.tracks.insert(loc.clone(), entry(100, Some(measured(-26.0 + 14.0 * x, 1.0 + 4.0 * x, 0.2 + 0.4 * x, 240.0)), None));
            tracks.push(tref(i + 1, &loc, 100, 100.0 + 40.0 * x as f64));
        }
        tracks.push(tref(100, "/never", 1, 0.0));
        tracks.push(tref(101, "/t0", 999, 0.0)); // same location, different size: stale
        c.tracks.insert("/tiny".into(), entry(5, None, Some("silent")));
        tracks.push(tref(102, "/tiny", 5, 0.0));

        let fit = compute_state(&tracks, &c, &Settings::default(), None);
        assert_eq!((fit.total, fit.ready, fit.skipped, fit.stale, fit.none), (33, 30, 1, 1, 1));
        assert_eq!(fit.scale_used, "fit");
        assert!(fit.note.is_none());
        let energies: Vec<f32> = fit.entries.iter().map(|e| e.energy).collect();
        assert_eq!(energies.iter().cloned().fold(f32::MAX, f32::min), 1.0);
        assert_eq!(energies.iter().cloned().fold(f32::MIN, f32::max), 10.0);
        assert!(energies.windows(2).all(|w| w[0] <= w[1]), "rising intensity must give rising energy: {energies:?}");
        assert!(fit.entries.iter().all(|e| (1..=10).contains(&e.level) && (e.energy.round() as u8) == e.level));

        let fixed = compute_state(&tracks, &c, &Settings { scale: Scale::Fixed, ..Settings::default() }, None);
        assert_eq!(fixed.scale_used, "fixed");
        assert_eq!((fixed.anchor_lo, fixed.anchor_hi), (Anchors::FIXED.lo, Anchors::FIXED.hi));
        // the same scores, a different scale
        assert_eq!(fixed.entries.len(), fit.entries.len());
        assert!(fixed.entries.iter().zip(&fit.entries).any(|(a, b)| a.energy != b.energy));
        assert!(fixed.entries.iter().zip(&fit.entries).all(|(a, b)| a.score == b.score));
    }

    #[test]
    fn a_fit_with_too_few_tracks_falls_back_and_says_so() {
        let mut c = CacheFile::default();
        let mut tracks = Vec::new();
        for i in 0..5 {
            let loc = format!("/t{i}");
            c.tracks.insert(loc.clone(), entry(1, Some(measured(-20.0 + 2.0 * i as f32, 2.0, 0.3, 200.0)), None));
            tracks.push(tref(i + 1, &loc, 1, 120.0));
        }
        let s = compute_state(&tracks, &c, &Settings::default(), None);
        assert_eq!(s.scale_used, "fixed");
        assert!(s.note.as_deref().is_some_and(|n| n.contains("at least 20")), "{:?}", s.note);
        assert_eq!(compute_state(&[], &CacheFile::default(), &Settings::default(), None).entries.len(), 0);
    }

    #[test]
    fn a_bpm_change_moves_the_score_without_a_new_decode() {
        let mut c = CacheFile::default();
        c.tracks.insert("/a".into(), entry(1, Some(measured(-16.0, 3.0, 0.4, 200.0)), None));
        let fixed = Settings { scale: Scale::Fixed, ..Settings::default() };
        let slow = compute_state(&[tref(1, "/a", 1, 90.0)], &c, &fixed, None).entries[0].score;
        let fast = compute_state(&[tref(1, "/a", 1, 150.0)], &c, &fixed, None).entries[0].score;
        assert!(fast > slow);
    }

    #[test]
    fn the_bpm_of_a_track_never_analysed_is_not_used() {
        let mut c = CacheFile::default();
        c.tracks.insert("/a".into(), entry(1, Some(measured(-16.0, 3.0, 0.4, 200.0)), None));
        let fixed = Settings { scale: Scale::Fixed, ..Settings::default() };
        let score = |bpm: f64, analysed: bool| compute_state(&[TrackRef { analysed, ..tref(1, "/a", 1, bpm) }], &c, &fixed, None).entries[0].score;
        assert_eq!(score(170.0, false), score(0.0, true), "no tempo term without a trusted BPM");
        assert!(score(170.0, true) > score(170.0, false));
    }

    #[test]
    fn settings_validate_and_round_trip_and_survive_junk() {
        let dir = temp_dir("settings");
        assert_eq!(load_settings(&dir), Settings::default());
        let s = Settings { scale: Scale::Fixed, fit_low_pct: 10.0, fit_high_pct: 90.0, min_secs: 30.0, threads: 4 };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), s);
        for bad in [
            Settings { fit_low_pct: 50.0, ..s },
            Settings { fit_low_pct: -1.0, ..s },
            Settings { fit_high_pct: 40.0, ..s },
            Settings { fit_high_pct: 101.0, ..s },
            Settings { min_secs: 1.0, ..s },
            Settings { min_secs: f32::NAN, ..s },
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
        std::fs::write(settings_path(&dir), r#"{"scale": "fixed"}"#).unwrap();
        assert_eq!(load_settings(&dir).scale, Scale::Fixed, "missing fields take their defaults");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_cache_is_per_library_atomic_and_tolerates_corruption() {
        let dir = temp_dir("cache");
        assert_eq!(cache_path(&dir, "P:\\"), cache_path(&dir, "p:/"));
        assert_ne!(cache_path(&dir, "P:\\"), cache_path(&dir, "Q:\\"));
        let (empty, problem) = load_cache(&dir, "P:\\");
        assert!(empty.tracks.is_empty() && problem.is_none());

        let mut c = CacheFile::default();
        c.tracks.insert("/a".into(), entry(1, Some(measured(-14.0, 3.0, 0.4, 200.0)), None));
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

        // clear: counts what it drops, and a second clear has nothing to drop
        save_cache(&dir, "P:\\", &c).unwrap();
        assert_eq!(clear_cache(&dir, "P:\\").unwrap(), 1);
        assert_eq!(clear_cache(&dir, "P:\\").unwrap(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn targets_are_the_missing_by_default_and_everything_named_on_recompute() {
        let mut c = CacheFile::default();
        c.tracks.insert("/done".into(), entry(1, Some(measured(-14.0, 3.0, 0.4, 200.0)), None));
        c.tracks.insert("/changed".into(), entry(1, Some(measured(-14.0, 3.0, 0.4, 200.0)), None));
        c.tracks.insert("/silent".into(), entry(1, None, Some("silent")));
        let tracks = vec![tref(1, "/done", 1, 0.0), tref(2, "/changed", 2, 0.0), tref(3, "/new", 1, 0.0), tref(4, "/silent", 1, 0.0)];
        let ids = |v: Vec<TrackRef>| v.into_iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(ids(pick_targets(&tracks, &c, None, false, 20.0)), vec![2, 3]);
        assert_eq!(ids(pick_targets(&tracks, &c, None, true, 20.0)), vec![1, 2, 3, 4]);
        assert_eq!(ids(pick_targets(&tracks, &c, Some(&[1, 3]), false, 20.0)), vec![3]);
        assert_eq!(ids(pick_targets(&tracks, &c, Some(&[1, 3, 99]), true, 20.0)), vec![1, 3]);
        assert!(pick_targets(&tracks, &c, Some(&[1]), false, 20.0).is_empty());
    }

    #[test]
    fn the_job_measures_caches_and_reports_each_outcome() {
        let (lib, base) = library(
            "job",
            &[
                ("pad.wav", wav_bytes(&pad(25.0))),
                ("groove.wav", wav_bytes(&groove(25.0))),
                ("short.wav", wav_bytes(&pad(2.0))),
                ("silent.wav", wav_bytes(&vec![0.0; SR as usize * 12])),
                ("broken.wav", b"this is not audio".to_vec()),
            ],
        );
        let root = lib.root().to_string_lossy().to_string();
        let cache_dir = base.join("config");
        let tracks = refs_of(&lib).unwrap();
        assert_eq!(tracks.len(), 5);
        let settings = Settings { threads: 3, ..Settings::default() };

        let mut events = Vec::new();
        let out = run_analysis(&cache_dir, &root, &tracks, &settings, |p| events.push((p.label.clone(), p.status.clone(), p.done, p.total))).unwrap();
        assert_eq!((out.ok, out.failed), (4, 1), "{}", out.summary);
        assert_eq!(events.len(), 5);
        assert!(events.iter().enumerate().all(|(i, e)| e.2 == i + 1 && e.3 == 5), "progress counts 1..=total in order: {events:?}");
        assert_eq!(events.iter().filter(|e| e.1 == "analysed").count(), 2);
        assert_eq!(events.iter().filter(|e| e.1 == "skipped").count(), 2);
        assert_eq!(events.iter().filter(|e| e.1 == "failed").count(), 1);

        let state = {
            let (cache, problem) = load_cache(&cache_dir, &root);
            compute_state(&tracks, &cache, &Settings { scale: Scale::Fixed, ..settings }, problem)
        };
        assert_eq!((state.ready, state.skipped, state.none, state.stale), (2, 2, 1, 0));
        let energy_of = |name: &str| {
            let id = tracks.iter().find(|t| t.label.contains(name)).unwrap().id;
            state.entries.iter().find(|e| e.track_id == id).unwrap().energy
        };
        assert!(energy_of("groove") > energy_of("pad") + 3.0, "{} vs {}", energy_of("groove"), energy_of("pad"));
        // the read API gives numbers for the same two tracks
        let read = energy_by_track(&cache_dir, &root, &lib).unwrap();
        assert_eq!(read.len(), 2);

        // running again with nothing new picks only the failed track
        let (cache, _) = load_cache(&cache_dir, &root);
        let again = pick_targets(&tracks, &cache, None, false, settings.min_secs);
        assert_eq!(again.len(), 1);
        assert!(again[0].label.contains("broken"));

        // one worker and three workers measure identically
        let cache_dir1 = base.join("config1");
        run_analysis(&cache_dir1, &root, &tracks, &Settings { threads: 1, ..settings }, |_| {}).unwrap();
        assert_eq!(load_cache(&cache_dir1, &root).0.tracks, load_cache(&cache_dir, &root).0.tracks);

        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_location_outside_the_music_folder_is_refused_not_read() {
        let (lib, base) = library("escape", &[("a.wav", wav_bytes(&pad(10.0)))]);
        let root = lib.root().to_string_lossy().to_string();
        std::fs::write(base.join("outside.wav"), wav_bytes(&groove(10.0))).unwrap();
        let cache_dir = base.join("config");
        let bad = ["/pmdb_tracks/../../outside.wav", "/elsewhere/a.wav", "/pmdb_tracks//abs.wav", ""];
        let tracks: Vec<TrackRef> = bad.iter().enumerate().map(|(i, l)| tref(i as i64 + 1, l, 1, 0.0)).collect();
        let mut statuses = Vec::new();
        let out = run_analysis(&cache_dir, &root, &tracks, &Settings::default(), |p| statuses.push((p.status.clone(), p.error.clone()))).unwrap();
        assert_eq!((out.ok, out.failed), (0, 4), "{}", out.summary);
        assert!(statuses.iter().all(|s| s.0 == "failed"));
        assert!(load_cache(&cache_dir, &root).0.tracks.is_empty(), "nothing is cached for a refused track");
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
        // more tracks than FLUSH_EVERY, all unreadable: failures are never cached, so use skips
        let dir = temp_dir("flush");
        let names: Vec<String> = (0..FLUSH_EVERY + 3).map(|i| format!("s{i}.wav")).collect();
        let files: Vec<(&str, Vec<u8>)> = names.iter().map(|n| (n.as_str(), wav_bytes(&vec![0.0; SR as usize * 6]))).collect();
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
