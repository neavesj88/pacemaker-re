//! Cases: browse (read-only for now) and, later, edit cases and smart cases.

use std::path::PathBuf;

use pm_formats::library::Library;

use crate::error::AppResult;

pub fn setup(_app: &mut tauri::App) {}

/// Track ids in a case, in the order they were added (read-only).
#[tauri::command]
pub async fn case_tracks(root: String, case_id: i64) -> AppResult<Vec<i64>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<i64>> {
        let lib = Library::open_read_only(&PathBuf::from(root))?;
        Ok(lib.case_track_ids(case_id)?)
    })
    .await?
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! cases_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::cases::case_tracks,
            ];
            $($rest),*
        }
    };
}
