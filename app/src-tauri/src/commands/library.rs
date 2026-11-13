//! Library / session: find volumes and read a library read-only. Nothing here writes, except that
//! `computer::computer_library_create` makes the empty library on this computer.

pub mod computer;
pub(crate) mod kind;

use std::path::PathBuf;

use pm_analysis::ingest;
use pm_formats::library::{Collection, Library, Track, find_volumes};
use serde::Serialize;

use crate::error::AppResult;

#[derive(Serialize)]
pub struct NeedsAnalysis {
    track_id: i64,
    reason: String,
}

#[derive(Serialize)]
pub struct LibraryView {
    root: String,
    /// "pacemaker" (a drive or card the device reads) or "computer" (a folder on this computer).
    kind: &'static str,
    problems: Vec<String>,
    tracks: Vec<Track>,
    mixes: Vec<Collection>,
    cases: Vec<Collection>,
    /// Tracks whose `.str` is missing, empty or unreadable, or that were never analysed.
    needs_analysis: Vec<NeedsAnalysis>,
}

pub fn setup(_app: &mut tauri::App) {}

/// The third-party notices (`THIRD-PARTY-NOTICES.md`, bundled with the app as a resource), for the
/// About entry of the Settings view.
#[tauri::command]
pub fn app_notices(app: tauri::AppHandle) -> AppResult<String> {
    use tauri::Manager;
    let path = app.path().resolve("THIRD-PARTY-NOTICES.md", tauri::path::BaseDirectory::Resource)?;
    std::fs::read_to_string(&path).map_err(|e| format!("The notices file is missing from this build ({}): {e}", path.display()).into())
}

/// Mounted volumes that hold a Pacemaker library.
#[tauri::command]
pub async fn list_volumes() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(|| find_volumes().into_iter().map(|p| p.display().to_string()).collect())
        .await
        .unwrap_or_default()
}

/// Reads a library read-only: nothing on the volume is modified by browsing.
#[tauri::command]
pub async fn open_library(root: String) -> AppResult<LibraryView> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<LibraryView> {
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        let tracks = lib.tracks()?;
        let needs_analysis = tracks
            .iter()
            .filter_map(|t| ingest::analysis_problem(&lib, t).map(|reason| NeedsAnalysis { track_id: t.track_id, reason }))
            .collect();
        Ok(LibraryView {
            problems: lib.compatibility_problems()?,
            mixes: lib.mixes()?,
            cases: lib.cases()?,
            tracks,
            needs_analysis,
            kind: kind::library_kind(&root),
            root,
        })
    })
    .await?
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! library_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::library::list_volumes,
                $crate::commands::library::open_library,
                $crate::commands::library::app_notices,
                $crate::commands::library::computer::computer_library_get,
                $crate::commands::library::computer::computer_library_set,
                $crate::commands::library::computer::computer_library_create,
            ];
            $($rest),*
        }
    };
}
