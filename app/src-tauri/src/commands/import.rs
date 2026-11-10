//! Import: bring audio (and later other libraries) onto the device. Only `add_tracks` writes.

use std::path::{Path, PathBuf};

use pm_analysis::ingest;
use pm_formats::library::Library;
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{progress_of, spawn_job};
use crate::state::JobLock;

/// Extensions the analyser's decoder handles and the file picker offers.
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff", "aif"];

pub fn setup(_app: &mut tauri::App) {}

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
pub async fn expand_audio_paths(paths: Vec<String>) -> Vec<String> {
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

/// Analyses `files` and adds them to the library on `root` (copies audio, writes `.str`,
/// inserts the row). Returns as soon as the job has started; per-file `job-progress` events
/// and a final `job-done` follow. A failing file is reported and the batch carries on; tracks
/// already on the volume are skipped without being re-analysed.
#[tauri::command]
pub fn add_tracks(app: AppHandle, lock: State<'_, JobLock>, root: String, files: Vec<String>) -> AppResult<()> {
    let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).filter(|p| is_audio(p)).collect();
    if files.is_empty() {
        return Err("No audio files to add.".into());
    }
    spawn_job(app, &lock, "add", move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        let total = files.len();
        let mut done = 0;
        Ok(ingest::add_files(&mut lib, &files, |ev| {
            done += 1;
            events::emit(app, JOB_PROGRESS, progress_of("add", ev, done, total));
        }).into())
    })
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! import_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::import::expand_audio_paths,
                $crate::commands::import::add_tracks,
            ];
            $($rest),*
        }
    };
}
