//! One module per feature area. Every area file owns its commands, its payload types and its
//! `setup` (managed state); nothing here needs editing when an area adds a command.
//!
//! How commands get registered without a shared list: each area file defines a small
//! `<area>_commands!` macro holding that area's command paths. `register_handlers!` chains all
//! of them (accumulating the paths) into one `tauri::generate_handler!` call. To add a command,
//! write the `#[tauri::command] pub fn` in your area file and add its path to your area's
//! macro. A brand-new area is the only case that touches this file: add `pub mod`, a `setup`
//! line and the macro name to the chain.

pub mod cases;
pub mod analysis;
pub mod device;
pub mod export;
pub mod grid;
pub mod history;
pub mod import;
pub mod library;
pub mod metadata;
pub mod mixes;
pub mod mixlive;
pub mod playback;
pub mod tags;
pub mod stems;
pub mod tracks;

pub fn setup(app: &mut tauri::App) {
    library::setup(app);
    tracks::setup(app);
    mixes::setup(app);
    mixlive::setup(app);
    cases::setup(app);
    history::setup(app);
    grid::setup(app);
    device::setup(app);
    import::setup(app);
    playback::setup(app);
    metadata::setup(app);
    analysis::setup(app);
    export::setup(app);
    tags::setup(app);
    stems::setup(app);
}

/// The chain terminator: turns the accumulated paths into the Tauri handler.
#[macro_export]
macro_rules! end_commands {
    ([$($acc:tt)*];) => {
        tauri::generate_handler![$($acc)*]
    };
}

/// Expands to the invoke handler holding every area's commands.
#[macro_export]
macro_rules! register_handlers {
    () => {
        $crate::library_commands! {
            [];
            tracks_commands,
            mixes_commands,
            mixlive_commands,
            cases_commands,
            history_commands,
            grid_commands,
            device_commands,
            import_commands,
            playback_commands,
            metadata_commands,
            analysis_commands,
            export_commands,
            tags_commands,
            stems_commands,
            end_commands
        }
    };
}
