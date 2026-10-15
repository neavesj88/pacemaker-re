//! Pacemaker Editor: Tauri shell over the pm-formats / pm-analysis crates.

use std::path::PathBuf;

use pm_formats::library::{find_volumes, Collection, Library, Track};
use serde::Serialize;

#[derive(Serialize)]
struct LibraryView {
    root: String,
    problems: Vec<String>,
    tracks: Vec<Track>,
    mixes: Vec<Collection>,
    cases: Vec<Collection>,
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
        Ok(LibraryView {
            problems: lib.compatibility_problems().map_err(|e| e.to_string())?,
            tracks: lib.tracks().map_err(|e| e.to_string())?,
            mixes: lib.mixes().map_err(|e| e.to_string())?,
            cases: lib.cases().map_err(|e| e.to_string())?,
            root,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![list_volumes, open_library])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
