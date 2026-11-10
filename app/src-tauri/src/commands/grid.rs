//! Beat grid editor: load a track's `.str` grid and cue, apply grid edits, re-analyse, and save.
//!
//! Edits are stateless (`grid_edit` maps a grid to a grid); the editor keeps the working copy
//! and its undo history. Nothing is written until `grid_save`, which goes through
//! `Library::save_grid` (the `.str` via `store_analysis`) and `Library::set_track_cue`.

use std::path::Path;

use pm_formats::library::Library;
use pm_formats::ops_grid::{self, EditCtx, GridDoc, GridEdit, GridLoad};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppResult, IntoApp};
use crate::state::{JobGuard, JobLock};

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// Reads a track's grid, cue and audio facts. Read-only.
#[tauri::command]
pub async fn grid_load(root: String, track_id: i64) -> AppResult<GridLoad> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(lib.load_grid(track_id)?)
    })
    .await
    .app()?
}

/// Applies one edit to a grid; pure, touches no file.
#[tauri::command]
pub fn grid_edit(doc: GridDoc, edit: GridEdit, sample_rate: u32, total_frames: Option<i64>) -> AppResult<GridDoc> {
    Ok(ops_grid::apply(&doc, edit, EditCtx { sample_rate, total_frames })?)
}

/// Analyses the track's audio again and returns the new grid without saving it.
#[tauri::command]
pub async fn grid_analyse(root: String, track_id: i64) -> AppResult<GridDoc> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        let load = lib.load_grid(track_id)?;
        let (audio, _) = lib.track_paths(&load.location).ok_or("This track is not stored on the device.")?;
        if !audio.is_file() {
            return Err("The audio file is missing from the volume.".into());
        }
        let result = pm_analysis::analyze_file(&audio).app()?;
        Ok(GridDoc::from(&result.to_str_analysis()))
    })
    .await
    .app()?
}

/// A cue change: `frames` is the new cue in source frames, `None` clears it.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CueChange {
    pub frames: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GridSaved {
    pub wrote_grid: bool,
    pub wrote_cue: bool,
}

/// Writes the edited grid and/or cue. Each part is optional so only what changed is touched.
/// Takes the shared job lock so it never overlaps an add / re-analyse job on the same music.db.
#[tauri::command]
pub async fn grid_save(
    lock: State<'_, JobLock>,
    root: String,
    track_id: i64,
    doc: Option<GridDoc>,
    cue: Option<CueChange>,
) -> AppResult<GridSaved> {
    if !lock.try_acquire() {
        return Err("Another job is still running.".into());
    }
    // released when this scope ends, panics included; the blocking work below is awaited inside it
    let _guard = JobGuard(&lock.0);
    tauri::async_runtime::spawn_blocking(move || {
        let mut lib = Library::open(Path::new(&root))?;
        let mut saved = GridSaved { wrote_grid: false, wrote_cue: false };
        if let Some(doc) = doc {
            lib.save_grid(track_id, &doc)?;
            saved.wrote_grid = true;
        }
        if let Some(c) = cue {
            saved.wrote_cue = lib.set_track_cue(track_id, c.frames)?;
        }
        let problems = lib.compatibility_problems()?;
        if !problems.is_empty() {
            return Err(format!("music.db is no longer device-compatible: {}", problems.join("; ")).into());
        }
        Ok(saved)
    })
    .await
    .app()?
}

/// This area's command paths, one per line, written `$crate::commands::grid::name,`.
#[macro_export]
macro_rules! grid_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::grid::grid_load,
                $crate::commands::grid::grid_edit,
                $crate::commands::grid::grid_analyse,
                $crate::commands::grid::grid_save,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_arrive_as_the_frontend_sends_them() {
        let e: GridEdit = serde_json::from_str(r#"{"kind":"nudge","frames":-44}"#).unwrap();
        assert_eq!(e, GridEdit::Nudge { frames: -44 });
        let e: GridEdit = serde_json::from_str(r#"{"kind":"shift_beats","beats":1}"#).unwrap();
        assert_eq!(e, GridEdit::ShiftBeats { beats: 1 });
        let e: GridEdit = serde_json::from_str(r#"{"kind":"set_bpm","bpm":127.5}"#).unwrap();
        assert_eq!(e, GridEdit::SetBpm { bpm: 127.5 });
        let e: GridEdit = serde_json::from_str(r#"{"kind":"halve"}"#).unwrap();
        assert_eq!(e, GridEdit::Halve);
        let c: CueChange = serde_json::from_str(r#"{"frames":null}"#).unwrap();
        assert_eq!(c.frames, None);
    }

    #[test]
    fn grid_edit_command_reports_errors_as_text() {
        let doc: GridDoc = serde_json::from_str(
            r#"{"normalize_factor":1.0,"beat_start_offset":0,"beats":[{"pos":0,"bpm":120.0},{"pos":22050,"bpm":120.0}],"first_downbeat":0}"#,
        )
        .unwrap();
        let out = grid_edit(doc.clone(), GridEdit::Nudge { frames: 10 }, 44_100, None).unwrap();
        assert_eq!(out.beats[1].pos, 22_060);
        let err = grid_edit(doc, GridEdit::SetBpm { bpm: 1.0 }, 44_100, None).unwrap_err();
        assert!(err.to_string().contains("tempo must be between"));
    }
}
