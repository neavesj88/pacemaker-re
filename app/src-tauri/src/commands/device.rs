//! Device: storage, firmware, backup and settings of the docked device.
//!
//! No commands yet. To add one: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `device_commands!` below, then mirror it in `src/lib/api/device.ts` and
//! `src/lib/devmock/device.ts`.

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// This area's command paths, one per line, written `$crate::commands::device::name,`.
#[macro_export]
macro_rules! device_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                // $crate::commands::device::example_command,
            ];
            $($rest),*
        }
    };
}
