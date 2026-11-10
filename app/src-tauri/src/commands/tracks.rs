//! Tracks: operations on tracks already in the library (beat-grid re-analysis now; tags,
//! ratings and removal later).

use std::path::Path;

use pm_analysis::ingest::{self, Existing};
use pm_formats::library::Library;
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{progress_of, spawn_job};
use crate::state::JobLock;

pub fn setup(_app: &mut tauri::App) {}

/// Rewrites the `.str` of the given tracks from the audio already on the volume (no audio is
/// copied). Same events as `add_tracks`.
#[tauri::command]
pub fn reanalyse_tracks(app: AppHandle, lock: State<'_, JobLock>, root: String, track_ids: Vec<i64>) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err("No tracks to analyse.".into());
    }
    spawn_job(app, &lock, "reanalyse", move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        let wanted: std::collections::HashSet<i64> = track_ids.into_iter().collect();
        let items: Vec<Existing> = lib
            .tracks()?
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
            events::emit(app, JOB_PROGRESS, progress_of("reanalyse", ev, done, total));
        }).into())
    })
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! tracks_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::tracks::reanalyse_tracks,
            ];
            $($rest),*
        }
    };
}
