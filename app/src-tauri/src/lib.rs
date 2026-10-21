//! Pacemaker Revival: Tauri shell over the pm-formats / pm-analysis crates.
//!
//! Browsing (`open_library`, `case_tracks`) opens the database read-only. Only the explicit
//! `add_tracks` and `reanalyse_tracks` commands write to the volume.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pm_analysis::ingest::{self, Event, Existing, Outcome, Summary};
use pm_formats::library::{Collection, Library, Track, find_volumes};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Extensions the analyser's decoder handles and the file picker offers.
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff", "aif"];

#[derive(Serialize)]
struct NeedsAnalysis {
    track_id: i64,
    reason: String,
}

#[derive(Serialize)]
struct LibraryView {
    root: String,
    problems: Vec<String>,
    tracks: Vec<Track>,
    mixes: Vec<Collection>,
    cases: Vec<Collection>,
    /// Tracks whose `.str` is missing, empty or unreadable, or that were never analysed.
    needs_analysis: Vec<NeedsAnalysis>,
}

/// Mounted volumes that hold a Pacemaker library.
#[tauri::command]
async fn list_volumes() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(|| find_volumes().into_iter().map(|p| p.display().to_string()).collect())
        .await
        .unwrap_or_default()
}

/// Reads a library read-only: nothing on the volume is modified by browsing.
#[tauri::command]
async fn open_library(root: String) -> Result<LibraryView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(&PathBuf::from(&root)).map_err(|e| e.to_string())?;
        let tracks = lib.tracks().map_err(|e| e.to_string())?;
        let needs_analysis = tracks
            .iter()
            .filter_map(|t| ingest::analysis_problem(&lib, t).map(|reason| NeedsAnalysis { track_id: t.track_id, reason }))
            .collect();
        Ok(LibraryView {
            problems: lib.compatibility_problems().map_err(|e| e.to_string())?,
            mixes: lib.mixes().map_err(|e| e.to_string())?,
            cases: lib.cases().map_err(|e| e.to_string())?,
            tracks,
            needs_analysis,
            root,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Track ids in a case, in the order they were added (read-only).
#[tauri::command]
async fn case_tracks(root: String, case_id: i64) -> Result<Vec<i64>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(&PathBuf::from(root)).map_err(|e| e.to_string())?;
        lib.case_track_ids(case_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

fn collect_audio(path: &Path, out: &mut Vec<PathBuf>) {
    // DirEntry::file_type doesn't follow symlinks, so a link loop can't recurse forever
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            for e in std::fs::read_dir(path).into_iter().flatten().flatten() {
                collect_audio(&e.path(), out);
            }
        }
        Ok(m) if m.is_file() && is_audio(path) => out.push(path.to_path_buf()),
        _ => {}
    }
}

/// The audio files among dropped/picked paths; folders are searched recursively. Read-only.
#[tauri::command]
async fn expand_audio_paths(paths: Vec<String>) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        for p in &paths {
            collect_audio(Path::new(p), &mut out);
        }
        out.sort();
        out.dedup();
        out.into_iter().map(|p| p.display().to_string()).collect()
    })
    .await
    .unwrap_or_default()
}

/// Only one write job at a time: both write to the same music.db.
#[derive(Default)]
struct JobLock(AtomicBool);

struct JobGuard<'a>(&'a AtomicBool);

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Serialize, Clone)]
struct Progress {
    job: &'static str,
    label: String,
    done: usize,
    total: usize,
    /// "added" | "skipped" | "reanalysed" | "failed"
    status: &'static str,
    bpm: Option<f64>,
    error: Option<String>,
}

#[derive(Serialize, Clone)]
struct JobDone {
    job: &'static str,
    added: usize,
    skipped: usize,
    reanalysed: usize,
    failed: usize,
    /// Set when the job could not run at all (database refused, thread crashed).
    error: Option<String>,
}

fn progress_of(job: &'static str, ev: &Event, done: usize, total: usize) -> Progress {
    let (status, bpm, error) = match &ev.outcome {
        Outcome::Added { bpm, .. } => ("added", Some(*bpm), None),
        Outcome::Skipped { .. } => ("skipped", None, None),
        Outcome::Reanalysed { bpm, .. } => ("reanalysed", Some(*bpm), None),
        Outcome::Failed(e) => ("failed", None, Some(e.clone())),
    };
    Progress { job, label: ev.label.clone(), done, total, status, bpm, error }
}

fn finish(app: &AppHandle, job: &'static str, res: Result<Summary, String>) {
    let done = match res {
        Ok(s) => JobDone { job, added: s.added, skipped: s.skipped, reanalysed: s.reanalysed, failed: s.failed, error: None },
        Err(e) => JobDone { job, added: 0, skipped: 0, reanalysed: 0, failed: 0, error: Some(e) },
    };
    let _ = app.emit("job-done", done);
}

/// Runs `work` on a background thread holding the job lock, then emits `job-done`. Progress
/// events are emitted from inside `work`. Fails immediately if another job is running.
fn spawn_job<F>(app: AppHandle, lock: &JobLock, job: &'static str, work: F) -> Result<(), String>
where
    F: FnOnce(&AppHandle) -> Result<Summary, String> + Send + 'static,
{
    if lock.0.swap(true, Ordering::SeqCst) {
        return Err("Another add or analysis job is still running.".into());
    }
    // the lock lives in managed state for the whole app run; re-fetch it inside the thread
    std::thread::spawn(move || {
        let state = tauri::Manager::state::<JobLock>(&app);
        let _guard = JobGuard(&state.0);
        let res = catch_unwind(AssertUnwindSafe(|| work(&app))).unwrap_or_else(|_| Err("the job crashed".into()));
        finish(&app, job, res);
    });
    Ok(())
}

/// Analyses `files` and adds them to the library on `root` (copies audio, writes `.str`,
/// inserts the row). Returns as soon as the job has started; per-file `job-progress` events
/// and a final `job-done` follow. A failing file is reported and the batch carries on; tracks
/// already on the volume are skipped without being re-analysed.
#[tauri::command]
fn add_tracks(app: AppHandle, lock: State<'_, JobLock>, root: String, files: Vec<String>) -> Result<(), String> {
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).filter(|p| is_audio(p)).collect();
    if files.is_empty() {
        return Err("No audio files to add.".into());
    }
    spawn_job(app, &lock, "add", move |app| {
        let mut lib = Library::open(Path::new(&root)).map_err(|e| e.to_string())?;
        let total = files.len();
        let mut done = 0;
        Ok(ingest::add_files(&mut lib, &files, |ev| {
            done += 1;
            let _ = app.emit("job-progress", progress_of("add", ev, done, total));
        }))
    })
}

/// Rewrites the `.str` of the given tracks from the audio already on the volume (no audio is
/// copied). Same events as `add_tracks`.
#[tauri::command]
fn reanalyse_tracks(app: AppHandle, lock: State<'_, JobLock>, root: String, track_ids: Vec<i64>) -> Result<(), String> {
    if track_ids.is_empty() {
        return Err("No tracks to analyse.".into());
    }
    spawn_job(app, &lock, "reanalyse", move |app| {
        let mut lib = Library::open(Path::new(&root)).map_err(|e| e.to_string())?;
        let wanted: std::collections::HashSet<i64> = track_ids.into_iter().collect();
        let items: Vec<Existing> = lib
            .tracks()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|t| wanted.contains(&t.track_id))
            .map(|t| Existing {
                track_id: t.track_id,
                label: match (t.artist.is_empty(), t.title.is_empty()) {
                    (false, false) => format!("{} - {}", t.artist, t.title),
                    (_, false) => t.title.clone(),
                    _ => t.location.clone(),
                },
                location: t.location,
            })
            .collect();
        let total = items.len();
        let mut done = 0;
        Ok(ingest::reanalyse(&mut lib, &items, |ev| {
            done += 1;
            let _ = app.emit("job-progress", progress_of("reanalyse", ev, done, total));
        }))
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(JobLock::default())
        .invoke_handler(tauri::generate_handler![
            list_volumes,
            open_library,
            case_tracks,
            expand_audio_paths,
            add_tracks,
            reanalyse_tracks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
