//! Playback: audio preview for the player bar.
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `playback_commands!` below, then mirror it in `src/lib/api/playback.ts` and
//! `src/lib/devmock/playback.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::playback::name,`.
#[macro_export]
macro_rules! playback_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::playback::example_command,
            ];
            $($rest),*
        }
    };
}
