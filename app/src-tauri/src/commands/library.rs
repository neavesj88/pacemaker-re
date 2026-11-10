//! Library / session: find volumes and read a library read-only. Nothing here writes.

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
    problems: Vec<String>,
    tracks: Vec<Track>,
    mixes: Vec<Collection>,
    cases: Vec<Collection>,
    /// Tracks whose `.str` is missing, empty or unreadable, or that were never analysed.
    needs_analysis: Vec<NeedsAnalysis>,
}

pub fn setup(_app: &mut tauri::App) {}

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
            ];
            $($rest),*
        }
    };
}
