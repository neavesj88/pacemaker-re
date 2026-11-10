//! Pacemaker Revival: Tauri shell over the pm-formats / pm-analysis crates.
//!
//! This file is registration only. Commands live in `commands/<area>.rs`; see
//! `app/ARCHITECTURE.md` for how an area adds a command, an event or managed state without
//! editing this file.

mod commands;
mod error;
mod events;
mod jobs;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            state::setup(app);
            commands::setup(app);
            Ok(())
        })
        .invoke_handler(register_handlers!())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
