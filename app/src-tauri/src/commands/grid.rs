//! Beat grid editor: load a track's `.str` grid and cue, apply grid edits, re-analyse, and save.
//!
//! Edits are stateless (`grid_edit` maps a grid to a grid); the editor keeps the working copy
//! and its undo history. Nothing is written until `grid_save`, which goes through
//! `Library::save_grid` (the `.str` via `store_analysis`) and `Library::set_track_cue`.

use std::path::Path;

pub mod batch;
pub mod suspect;
pub mod tempo;
pub mod vtempo;

use pm_formats::library::Library;
use pm_formats::ops_grid::{self, EditCtx, GridDoc, GridEdit, GridLoad};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::analysis::audio_path;
use crate::error::{AppResult, IntoApp};

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(app: &mut tauri::App) {
    batch::setup(app);
}

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

/// Names of the mixes that contain the track. A mix clip keeps the tempo the track had when the
/// mix was made, so saving a new beat grid leaves those mixes out of step until they are
/// beatmatched again; the grid editor lists them and asks before saving. Read-only.
#[tauri::command]
pub async fn grid_mixes_using(root: String, track_id: i64) -> AppResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(lib.mixes_using_track(track_id)?.into_iter().map(|(_, name)| name).collect())
    })
    .await
    .app()?
}

/// Refuses a track whose stored location is not a plain path below `/pmdb_tracks/`. `save_grid`
/// rebuilds the `.str` path with `Library::track_paths`, which does not refuse `..`, so the stored
/// location is checked with the validated resolver before anything is written (as the batch
/// writes do).
fn checked_location(lib: &Library, track_id: i64) -> AppResult<()> {
    let track = lib.tracks()?.into_iter().find(|t| t.track_id == track_id).ok_or("That track is not in the library.")?;
    audio_path(lib.root(), &track.location)?;
    Ok(())
}

/// Writes the edited grid and/or cue. Each part is optional so only what changed is touched.
/// A quick write (`state::quick_write`): it neither waits for nor fails because of a long job
/// such as adding tracks, and Eject waits for it. The editor calls [`grid_mixes_using`] and gets
/// the user's confirmation before it saves a grid.
#[tauri::command]
pub async fn grid_save(
    app: AppHandle,
    root: String,
    track_id: i64,
    doc: Option<GridDoc>,
    cue: Option<CueChange>,
) -> AppResult<GridSaved> {
    let path = root.clone();
    crate::state::quick_write(app, root, move || {
        let mut lib = Library::open(Path::new(&path))?;
        // refuse before touching anything: a library the device cannot read must not get a new .str
        let problems = lib.compatibility_problems()?;
        if !problems.is_empty() {
            return Err(format!("music.db is not device-compatible, nothing was saved: {}", problems.join("; ")).into());
        }
        let mut saved = GridSaved { wrote_grid: false, wrote_cue: false };
        if let Some(doc) = doc {
            checked_location(&lib, track_id)?;
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
                $crate::commands::grid::grid_mixes_using,
                $crate::commands::grid::grid_save,
                $crate::commands::grid::tempo::grid_tempo_plan,
                $crate::commands::grid::tempo::grid_tempo_apply,
                $crate::commands::grid::suspect::grid_suspects,
                $crate::commands::grid::vtempo::grid_vtempo_edit,
                $crate::commands::grid::vtempo::grid_sections,
                $crate::commands::grid::vtempo::grid_drift,
                $crate::commands::grid::vtempo::grid_onsets,
                $crate::commands::grid::batch::grid_batch_plan,
                $crate::commands::grid::batch::grid_batch_analyse,
                $crate::commands::grid::batch::grid_batch_result,
                $crate::commands::grid::batch::grid_batch_cancel,
                $crate::commands::grid::batch::grid_batch_diff,
                $crate::commands::grid::batch::grid_batch_apply,
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
    fn saving_needs_a_track_of_the_library_with_a_valid_location() {
        let dir = std::env::temp_dir().join(format!("pm-app-gridsave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lib = Library::create(&dir).unwrap();
        let err = checked_location(&lib, 7).unwrap_err();
        assert!(err.to_string().contains("not in the library"), "{err}");
        drop(lib);
        std::fs::remove_dir_all(&dir).unwrap();
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
