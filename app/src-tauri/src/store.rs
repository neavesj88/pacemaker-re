//! The app's side of the plumbing every app-side file shares. The helpers themselves (file names,
//! atomic writes, what happens to an unreadable file) are `pm_formats::appfile`, the same code
//! `pm` uses, so the app and the command line read and write one file the same way; this module
//! adds only what needs Tauri: where the app keeps its files.

use std::path::PathBuf;

use tauri::Manager;

use crate::error::AppResult;

pub use pm_formats::appfile::*;

/// The folder of every app-side file (labels, settings, caches). Never on a device.
pub fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}
