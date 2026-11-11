//! Metadata: edit track tags and ratings (title, artist, album, genre, rating) in music.db; the
//! editor UI is a modal (views/metadata/Overlays.svelte). Writes go through the pm-formats
//! `Library` (ops_metadata.rs), which holds the Editor's row conventions.

use std::path::PathBuf;

use pm_formats::library::Library;
use pm_formats::ops_metadata::{TrackDetail, TrackEdit};

use crate::error::AppResult;

pub fn setup(_app: &mut tauri::App) {}

/// One track's edit as the UI sends it: the id plus only the fields that changed. Keys are
/// snake_case (Tauri's camelCase conversion applies to top-level arguments only).
#[derive(serde::Deserialize)]
pub struct TrackEditEntry {
    pub track_id: i64,
    #[serde(flatten)]
    pub edit: TrackEdit,
}

/// Sets a track's star rating (0 = unrated to 5).
#[tauri::command]
pub async fn metadata_set_rating(app: tauri::AppHandle, root: String, track_id: i64, stars: u8) -> AppResult<()> {
    crate::state::quick_write(app, root.clone(), move || -> AppResult<()> {
        let mut lib = Library::open(&PathBuf::from(root))?;
        lib.set_track_rating(track_id, stars)?;
        Ok(())
    })
    .await
}

/// The editable fields of the given tracks, in the order asked (read-only).
#[tauri::command]
pub async fn metadata_track_details(root: String, track_ids: Vec<i64>) -> AppResult<Vec<TrackDetail>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<TrackDetail>> {
        let lib = Library::open_read_only(&PathBuf::from(root))?;
        Ok(lib.track_details(&track_ids)?)
    })
    .await?
}

/// Saves the edits in one transaction; returns how many tracks actually changed.
#[tauri::command]
pub async fn metadata_save(app: tauri::AppHandle, root: String, edits: Vec<TrackEditEntry>) -> AppResult<usize> {
    crate::state::quick_write(app, root.clone(), move || -> AppResult<usize> {
        let mut lib = Library::open(&PathBuf::from(root))?;
        let edits: Vec<(i64, TrackEdit)> = edits.into_iter().map(|e| (e.track_id, e.edit)).collect();
        Ok(lib.edit_tracks(&edits)?)
    })
    .await
}

/// This area's command paths, one per line, written `$crate::commands::metadata::name,`.
#[macro_export]
macro_rules! metadata_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::metadata::metadata_set_rating,
                $crate::commands::metadata::metadata_track_details,
                $crate::commands::metadata::metadata_save,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_parses_the_ui_payload() {
        let e: TrackEditEntry = serde_json::from_str(r#"{"track_id":7,"genre":"House","year":1999,"rating":3}"#).unwrap();
        assert_eq!(e.track_id, 7);
        assert_eq!(e.edit.genre.as_deref(), Some("House"));
        assert_eq!((e.edit.year, e.edit.rating), (Some(1999), Some(3)));
        assert_eq!(e.edit.title, None, "absent keys leave the field alone");
    }
}
