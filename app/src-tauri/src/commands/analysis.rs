//! Analysis: key detection and other per-track analysis beyond the beat grid, as background jobs (jobs::spawn_job with its own job kind).
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `analysis_commands!` below, then mirror it in `src/lib/api/analysis.ts` and
//! `src/lib/devmock/analysis.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::analysis::name,`.
#[macro_export]
macro_rules! analysis_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::analysis::example_command,
            ];
            $($rest),*
        }
    };
}
