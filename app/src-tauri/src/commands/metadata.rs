//! Metadata: edit track tags and ratings (title, artist, album, genre, rating) in music.db; the editor UI is a modal (views/metadata/Overlays.svelte).
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `metadata_commands!` below, then mirror it in `src/lib/api/metadata.ts` and
//! `src/lib/devmock/metadata.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::metadata::name,`.
#[macro_export]
macro_rules! metadata_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::metadata::example_command,
            ];
            $($rest),*
        }
    };
}
