//! History: what the device played, read back from the library.
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `history_commands!` below, then mirror it in `src/lib/api/history.ts` and
//! `src/lib/devmock/history.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::history::name,`.
#[macro_export]
macro_rules! history_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::history::example_command,
            ];
            $($rest),*
        }
    };
}
