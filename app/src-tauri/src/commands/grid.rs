//! Beat grid editor: read and write .str grids and per-track cues.
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `grid_commands!` below, then mirror it in `src/lib/api/grid.ts` and
//! `src/lib/devmock/grid.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::grid::name,`.
#[macro_export]
macro_rules! grid_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::grid::example_command,
            ];
            $($rest),*
        }
    };
}
