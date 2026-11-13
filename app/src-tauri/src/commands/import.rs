//! Import: bring audio (and later other libraries) onto the device. Only `add_tracks` and
//! `interop_apply` write; the watch-folder scan (`watch.rs`) only reads and keeps its settings
//! in the app config dir.

pub mod watch;

pub mod request;

use std::path::{Path, PathBuf};

use pm_analysis::ingest;
use pm_formats::interop::{self, ImportOptions, ImportPlan};
use pm_formats::library::Library;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{progress_of, spawn_write_job, JobOutcome, Progress};
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
    spawn_write_job(app, &lock, "add", root.clone(), move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        let total = files.len();
        let mut done = 0;
        Ok(ingest::add_files(&mut lib, &files, |ev| {
            done += 1;
            events::emit(app, JOB_PROGRESS, progress_of("add", ev, done, total));
        }).into())
    })
}

// ---- other libraries: rekordbox XML, iTunes, Traktor NML, Serato crates, M3U / M3U8; M3U8 / CSV out ----

const JOB_INTEROP: &str = "interop";

/// Reads whatever the user picked (a library file, a playlist, a `.crate` or a `_Serato_` folder).
/// The reader picks the format; paths inside the file are never probed unless they are plain
/// local ones (see `pm_formats::interop::paths`).
fn read_foreign(path: &str) -> AppResult<interop::ForeignLibrary> {
    Ok(interop::read_path(Path::new(path))?)
}

/// Reads a rekordbox / iTunes / Traktor library, a Serato crate (file or `_Serato_` folder) or an
/// M3U / M3U8 playlist and lists exactly what importing it would change in the library on
/// `root`. Writes nothing.
#[tauri::command]
pub async fn interop_preview(root: String, path: String, options: Option<ImportOptions>) -> AppResult<ImportPlan> {
    tauri::async_runtime::spawn_blocking(move || {
        let foreign = read_foreign(&path)?;
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(interop::plan_import(&lib, &foreign, &options.unwrap_or_default())?)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Applies the plan [`interop_preview`] shows (it is computed again from the file and the
/// current library, so the two always agree). Ratings go in one transaction, each case in its
/// own. Returns as soon as the job has started.
#[tauri::command]
pub async fn interop_apply(
    app: AppHandle,
    lock: State<'_, JobLock>,
    root: String,
    path: String,
    options: Option<ImportOptions>,
) -> AppResult<()> {
    let opts = options.unwrap_or_default();
    // fail before starting the job when the file is unreadable; read and parsed off the UI thread
    let foreign = tauri::async_runtime::spawn_blocking(move || read_foreign(&path)).await.map_err(|e| e.to_string())??;
    spawn_write_job(app, &lock, JOB_INTEROP, root.clone(), move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        let plan = interop::plan_import(&lib, &foreign, &opts)?;
        let rep = interop::apply_plan(&mut lib, &plan, &opts, |s| {
            let mut p = Progress::new(JOB_INTEROP, s.label, s.done, s.total, if s.error.is_some() { "failed" } else { "done" });
            p.error = s.error.map(str::to_string);
            events::emit(app, JOB_PROGRESS, p);
        })?;
        Ok(JobOutcome {
            ok: rep.ratings_set + rep.cues_set + rep.cases_created + rep.cases_extended,
            failed: rep.failures.len(),
            summary: rep.summary(),
        })
    })
}

#[derive(Serialize)]
pub struct ExportDone {
    pub path: String,
    pub tracks: usize,
    /// What the user should know about this export (smart cases written or left out), in words.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Writes a track list for other software. `format` is "m3u8", "csv" or "rekordbox" (the whole
/// library with playlists, and the smart cases as playlists of their current matches when the
/// smart-case setting is on; `scope` is ignored); `scope` is "library", "case" or "mix" with the
/// id of the case or mix. The file goes to `dest`, a path the user chose; nothing on the
/// volume is changed.
#[tauri::command]
pub async fn interop_export(app: AppHandle, root: String, format: String, scope: String, id: Option<i64>, dest: String) -> AppResult<ExportDone> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        let mut notes = Vec::new();
        let (text, tracks) = if format == "rekordbox" {
            // the cases area owns smart cases, so the library XML is built there
            let x = crate::commands::cases::library_xml(&app, &root)?;
            notes = x.notes;
            (x.text, x.tracks)
        } else {
            let scope = match (scope.as_str(), id) {
                ("library", _) => interop::Scope::Library,
                ("case", Some(id)) => interop::Scope::Case(id),
                ("mix", Some(id)) => interop::Scope::Mix(id),
                _ => return Err(AppError::from(format!("unknown export scope {scope}"))),
            };
            let (_, tracks) = interop::scope_tracks(&lib, scope)?;
            let text = match format.as_str() {
                "m3u8" => interop::export_m3u8(&lib, &tracks),
                "csv" => interop::export_csv(&lib, &tracks),
                other => return Err(AppError::from(format!("unknown export format {other}"))),
            };
            (text, tracks.len())
        };
        // write beside the destination and rename, so a failed write never leaves half a file
        crate::store::write_file(Path::new(&dest), text.as_bytes())?;
        Ok(ExportDone { path: dest, tracks, notes })
    })
    .await
    .map_err(|e| e.to_string())?
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
                $crate::commands::import::interop_preview,
                $crate::commands::import::interop_apply,
                $crate::commands::import::interop_export,
                $crate::commands::import::watch::watch_settings_get,
                $crate::commands::import::watch::watch_settings_set,
                $crate::commands::import::watch::watch_scan,
                $crate::commands::import::watch::watch_state,
                $crate::commands::import::watch::watch_ignore,
                $crate::commands::import::watch::watch_unignore,
                $crate::commands::import::watch::watch_log_add,
                $crate::commands::import::request::matcher_read_list,
                $crate::commands::import::request::matcher_save_text,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::interop::{CueMode, Source};

    // the UI sends these exact names; an options object from before `cues` existed still works
    #[test]
    fn options_json_contract() {
        let o: ImportOptions =
            serde_json::from_str(r#"{"ratings":true,"playlists":false,"playlist_indexes":[1,2],"cues":"overwrite"}"#).unwrap();
        assert_eq!((o.cues, o.playlists, o.playlist_indexes), (CueMode::Overwrite, false, Some(vec![1, 2])));
        let o: ImportOptions = serde_json::from_str(r#"{"ratings":true,"playlists":true,"playlist_indexes":null}"#).unwrap();
        assert_eq!(o.cues, CueMode::Off);
        assert!(serde_json::from_str::<ImportOptions>(r#"{"cues":"sometimes"}"#).is_err());
        assert_eq!(serde_json::to_string(&Source::Serato).unwrap(), "\"serato\"");
        assert_eq!(serde_json::to_string(&Source::M3u).unwrap(), "\"m3u\"");
        assert_eq!(serde_json::to_string(&Source::Traktor).unwrap(), "\"traktor\"");
    }

    #[test]
    fn read_foreign_reports_unreadable_input_as_an_error_string() {
        let e = read_foreign("Z:/this/does/not/exist.nml").unwrap_err();
        assert!(!e.to_string().is_empty());
    }
}
