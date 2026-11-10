//! Export: write playlists, mixes and library listings out of the device (M3U, CSV, other libraries). Reads only; writes to a path the user chose.
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `export_commands!` below, then mirror it in `src/lib/api/export.ts` and
//! `src/lib/devmock/export.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::export::name,`.
#[macro_export]
macro_rules! export_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::export::example_command,
            ];
            $($rest),*
        }
    };
}
