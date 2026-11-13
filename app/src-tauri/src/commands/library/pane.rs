//! What a pane of the Libraries view shows of a library: its tracks, mixes and cases, read from
//! `music.db` only. Unlike `open_library` it does not look at the `.str` files (no analysis
//! check), so it is quick even on a big library and touches the drive as little as possible. It is
//! called only when the user asks for a library (a Pacemaker is never read by itself).

use std::path::PathBuf;

use pm_formats::library::{Collection, Library, Track};
use serde::Serialize;

use crate::error::AppResult;

#[derive(Serialize)]
pub struct PaneLibrary {
    root: String,
    /// "pacemaker" or "computer", as in `LibraryView`.
    kind: &'static str,
    tracks: Vec<Track>,
    mixes: Vec<Collection>,
    cases: Vec<Collection>,
}

/// Lists a library for a pane, read-only, from `music.db` alone.
#[tauri::command]
pub async fn library_pane(root: String) -> AppResult<PaneLibrary> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PaneLibrary> {
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        Ok(PaneLibrary { tracks: lib.tracks()?, mixes: lib.mixes()?, cases: lib.cases()?, kind: super::kind::library_kind(&root), root })
    })
    .await?
}
