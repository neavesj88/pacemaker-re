//! Mixes: the mix editor (read, create, edit and write mixes), built on the mix helpers in pm-formats.
//!
//! `mix_list` is the mixes area's own listing, so the mix editor does not depend on the
//! `mixes` field of `LibraryView`. To add a command: write a `#[tauri::command] pub async fn`
//! here, add its full path to the list in `mixes_commands!` below, then mirror it in
//! `src/lib/api/mixes.ts` and `src/lib/devmock/mixes.ts`.

use std::path::PathBuf;

use pm_formats::library::{Collection, Library};

use crate::error::AppResult;

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// The mixes on the volume (read-only), summarised.
#[tauri::command]
pub async fn mix_list(root: String) -> AppResult<Vec<Collection>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<Collection>> {
        Ok(Library::open_read_only(&PathBuf::from(root))?.mixes()?)
    })
    .await?
}

/// This area's command paths, one per line, written `$crate::commands::mixes::name,`.
#[macro_export]
macro_rules! mixes_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::mixes::mix_list,
            ];
            $($rest),*
        }
    };
}
