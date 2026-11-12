//! Variable-tempo grid editing: tempo sections, single-beat edits, onsets and drift.
//!
//! Everything here except [`grid_onsets`] is pure (a grid in, a grid or a report out); the grid
//! editor keeps the working copy and writes it with the ordinary `grid_save`, which goes through
//! `Library::save_grid` (atomic `.str` rewrite, row tempo re-derived). The maths is in
//! `pm_formats::ops_vtempo`, so the drift shown before an edit and the drift measured after it
//! come from one function.
//!
//! [`grid_onsets`] decodes the track's audio (the file is found with the validated
//! `analysis::audio_path`, never from the raw `tracks.location`) and peak-picks it with
//! `pm_analysis::onsets`; one decode at a time, because a decode holds the whole track.

use std::path::Path;
use std::sync::Mutex;

use pm_formats::library::Library;
use pm_formats::ops_grid::{EditCtx, GridDoc};
use pm_formats::ops_vtempo::{self, DriftOptions, DriftReport, Onset, Section, SectionOptions, TempoEdit};
use serde::Serialize;

use crate::commands::analysis::audio_path;
use crate::error::{AppResult, IntoApp};

/// One onset detection at a time (each holds a whole decoded track in memory).
static DECODING: Mutex<()> = Mutex::new(());

/// The attacks found in a track's audio.
#[derive(Debug, Clone, Serialize)]
pub struct OnsetList {
    pub track_id: i64,
    /// Sample rate of the decoded audio; `onsets` are in frames of this rate.
    pub sample_rate: u32,
    pub onsets: Vec<Onset>,
}

/// Applies one tempo-map edit to a grid; pure, touches no file.
#[tauri::command]
pub fn grid_vtempo_edit(doc: GridDoc, edit: TempoEdit, sample_rate: u32, total_frames: Option<i64>) -> AppResult<GridDoc> {
    Ok(ops_vtempo::apply(&doc, &edit, EditCtx { sample_rate, total_frames })?)
}

/// Cuts a grid into tempo sections; `splits` are the frames the user cut at. Pure.
#[tauri::command]
pub fn grid_sections(doc: GridDoc, sample_rate: u32, options: SectionOptions, splits: Vec<i64>) -> AppResult<Vec<Section>> {
    if !(options.tolerance_pct.is_finite() && options.tolerance_pct >= 0.0) || sample_rate == 0 {
        return Err("The section tolerance must be 0 or more.".into());
    }
    Ok(ops_vtempo::sections(&doc, sample_rate, options, &splits))
}

/// Compares the beat lines of a grid with the onsets of the audio. Pure.
#[tauri::command]
pub fn grid_drift(doc: GridDoc, onsets: Vec<Onset>, sample_rate: u32, options: DriftOptions) -> AppResult<DriftReport> {
    Ok(ops_vtempo::drift(&doc, &onsets, sample_rate, options)?)
}

/// Decodes the track's audio and returns its attacks (device convention, see
/// `pm_analysis::onsets`) at least `min_strength` strong (a user setting; default the analyser's
/// floor). Read-only; a long track takes a few seconds.
#[tauri::command]
pub async fn grid_onsets(root: String, track_id: i64, min_strength: Option<f32>) -> AppResult<OnsetList> {
    let floor = min_strength.unwrap_or(pm_analysis::onsets::MIN_STRENGTH);
    if !(floor.is_finite() && floor > 0.0) {
        return Err("The onset strength floor must be above 0.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        let track = lib.tracks()?.into_iter().find(|t| t.track_id == track_id).ok_or("That track is not in the library.")?;
        let audio = audio_path(lib.root(), &track.location)?;
        if !audio.is_file() {
            return Err("The audio file is missing from the volume.".into());
        }
        let _one = DECODING.lock().unwrap_or_else(|e| e.into_inner());
        let decoded = pm_analysis::decode::decode(&audio).app()?;
        let onsets = pm_analysis::onsets::detect(&decoded.mono, decoded.sample_rate, floor)
            .into_iter()
            .map(|o| Onset { frame: o.frame, strength: o.strength })
            .collect();
        Ok(OnsetList { track_id, sample_rate: decoded.sample_rate, onsets })
    })
    .await
    .app()?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> GridDoc {
        serde_json::from_str(
            r#"{"normalize_factor":1.0,"beat_start_offset":1000,"beats":[{"pos":1000,"bpm":120.0},{"pos":23050,"bpm":120.0},{"pos":45100,"bpm":120.0},{"pos":67150,"bpm":120.0},{"pos":89200,"bpm":120.0},{"pos":111250,"bpm":120.0}],"first_downbeat":0}"#,
        )
        .unwrap()
    }

    #[test]
    fn tempo_edits_arrive_as_the_frontend_sends_them() {
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"set_section_bpm","from_beat":2,"to_beat":null,"bpm":127.5}"#).unwrap();
        assert_eq!(e, TempoEdit::SetSectionBpm { from_beat: 2, to_beat: None, bpm: 127.5 });
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"set_section_bpm","from_beat":2,"to_beat":5,"bpm":100}"#).unwrap();
        assert_eq!(e, TempoEdit::SetSectionBpm { from_beat: 2, to_beat: Some(5), bpm: 100.0 });
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"move_beat","beat":3,"frame":99}"#).unwrap();
        assert_eq!(e, TempoEdit::MoveBeat { beat: 3, frame: 99 });
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"shift_from","beat":3,"frames":-9}"#).unwrap();
        assert_eq!(e, TempoEdit::ShiftFrom { beat: 3, frames: -9 });
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"even","from_beat":1,"to_beat":4}"#).unwrap();
        assert_eq!(e, TempoEdit::Even { from_beat: 1, to_beat: 4 });
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"insert_beat","frame":5000}"#).unwrap();
        assert_eq!(e, TempoEdit::InsertBeat { frame: 5000 });
        let e: TempoEdit = serde_json::from_str(r#"{"kind":"remove_beat","beat":2}"#).unwrap();
        assert_eq!(e, TempoEdit::RemoveBeat { beat: 2 });
        let e: TempoEdit = serde_json::from_str(
            r#"{"kind":"follow_onsets","from_beat":0,"to_beat":null,"onsets":[{"frame":10,"strength":2.5}],"options":{"window_pct":25,"window_max_ms":120,"threshold_ms":20,"min_run":4},"smooth_beats":5}"#,
        )
        .unwrap();
        match e {
            TempoEdit::FollowOnsets { smooth_beats: 5, options, .. } => assert_eq!(options.max_gap, 2, "an older frontend that sends no max_gap gets the default"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn commands_map_a_grid_and_report_errors_as_text() {
        let out = grid_vtempo_edit(doc(), TempoEdit::MoveBeat { beat: 2, frame: 46_000 }, 44_100, None).unwrap();
        assert_eq!(out.beats[2].pos, 46_000);
        let err = grid_vtempo_edit(doc(), TempoEdit::MoveBeat { beat: 2, frame: 23_100 }, 44_100, None).unwrap_err();
        assert!(err.to_string().contains("closer than"), "{err}");

        let s = grid_sections(doc(), 44_100, SectionOptions::default(), vec![]).unwrap();
        assert_eq!(s.len(), 1);
        assert!(grid_sections(doc(), 44_100, SectionOptions { tolerance_pct: -1.0, min_beats: 4 }, vec![]).is_err());

        let on: Vec<Onset> = doc().beats.iter().map(|b| Onset { frame: b.pos + 441, strength: 3.0 }).collect();
        let r = grid_drift(doc(), on, 44_100, DriftOptions::default()).unwrap();
        assert!((r.bias_ms - 10.0).abs() < 0.01 && r.spans.is_empty());
    }

    #[test]
    fn onset_lists_refuse_locations_outside_the_music_folder() {
        // the validated resolver is what stands between a stored location and the file system
        let root = std::env::temp_dir();
        assert!(audio_path(&root, "/pmdb_tracks/../../secret.mp3").is_err());
        assert!(audio_path(&root, "C:/secret.mp3").is_err());
        assert!(audio_path(&root, "/pmdb_tracks/A/B/c.mp3").is_ok());
    }
}
