//! The size of the main window, remembered between runs. A setting (`remember`, on by default)
//! decides whether it is; the size itself is saved when the window closes and put back at start.
//! A saved size that no longer fits the screen (another monitor) is shrunk to fit, never larger.
//! The file lives in the app config folder like every other app-side file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalSize, Size, WindowEvent};

use crate::error::AppResult;
use crate::store::config_dir;

pub const FILE: &str = "window-size.json";
/// The window never opens smaller than this (the same as `minWidth` / `minHeight` in tauri.conf.json).
const MIN: (u32, u32) = (900, 560);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WindowPrefs {
    /// Open at the size the window had when it was last closed.
    pub remember: bool,
    /// Physical pixels, the size of the window's inside (what `set_size` sets); 0 = none saved yet.
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for WindowPrefs {
    fn default() -> Self {
        WindowPrefs { remember: true, width: 0, height: 0, maximized: false }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// What is saved; defaults when there is no file or it cannot be read (the window then just opens
/// at the size tauri.conf.json gives it).
pub fn load(dir: &Path) -> WindowPrefs {
    std::fs::read_to_string(path(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(dir: &Path, prefs: &WindowPrefs) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    crate::store::write_json(&path(dir), prefs)
}

/// The size to open at: the saved one, shrunk to the monitor and never below the minimum. `None`
/// when nothing usable is saved.
pub fn fit(saved: (u32, u32), monitor: Option<(u32, u32)>) -> Option<(u32, u32)> {
    if saved.0 == 0 || saved.1 == 0 {
        return None;
    }
    let (mw, mh) = monitor.unwrap_or((u32::MAX, u32::MAX));
    Some((saved.0.min(mw).max(MIN.0.min(mw)), saved.1.min(mh).max(MIN.1.min(mh))))
}

/// Called once the window exists: puts back the saved size.
pub fn restore(app: &AppHandle) {
    let (Some(win), Ok(dir)) = (app.get_webview_window("main"), config_dir(app)) else { return };
    let prefs = load(&dir);
    if !prefs.remember {
        return;
    }
    let monitor = win.current_monitor().ok().flatten().map(|m| (m.size().width, m.size().height));
    if let Some((w, h)) = fit((prefs.width, prefs.height), monitor) {
        let _ = win.set_size(Size::Physical(PhysicalSize::new(w, h)));
        let _ = win.center();
    }
    if prefs.maximized {
        let _ = win.maximize();
    }
}

/// Window events: remembers the size when the window closes (a minimised window has no useful
/// size, and a maximised one keeps the size it had before, so un-maximising goes back to it).
pub fn on_event(win: &tauri::Window, event: &WindowEvent) {
    if !matches!(event, WindowEvent::CloseRequested { .. }) || win.label() != "main" {
        return;
    }
    let app = win.app_handle();
    let Ok(dir) = config_dir(app) else { return };
    let mut prefs = load(&dir);
    if !prefs.remember || win.is_minimized().unwrap_or(false) {
        return;
    }
    prefs.maximized = win.is_maximized().unwrap_or(false);
    if !prefs.maximized {
        if let Ok(size) = win.inner_size() {
            prefs.width = size.width;
            prefs.height = size.height;
        }
    }
    let _ = save(&dir, &prefs);
}

fn current(app: &AppHandle) -> AppResult<WindowPrefs> {
    Ok(load(&config_dir(app)?))
}

/// Whether the window size is remembered.
#[tauri::command]
pub fn window_remember_get(app: AppHandle) -> AppResult<bool> {
    Ok(current(&app)?.remember)
}

/// Turns remembering on or off. Off forgets the saved size, so turning it on again starts afresh.
#[tauri::command]
pub fn window_remember_set(app: AppHandle, remember: bool) -> AppResult<bool> {
    let dir = config_dir(&app)?;
    let mut prefs = load(&dir);
    prefs.remember = remember;
    if !remember {
        prefs.width = 0;
        prefs.height = 0;
        prefs.maximized = false;
    }
    save(&dir, &prefs)?;
    Ok(remember)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_saved_gives_no_size() {
        assert_eq!(fit((0, 0), Some((3840, 2160))), None);
        assert_eq!(fit((1280, 0), None), None);
    }

    #[test]
    fn a_saved_size_is_kept_when_it_fits() {
        assert_eq!(fit((2560, 1440), Some((5120, 2160))), Some((2560, 1440)));
        assert_eq!(fit((1500, 900), None), Some((1500, 900)));
    }

    #[test]
    fn a_size_bigger_than_this_monitor_shrinks_to_it_and_a_tiny_one_grows_to_the_minimum() {
        assert_eq!(fit((5120, 2160), Some((1920, 1080))), Some((1920, 1080)));
        assert_eq!(fit((300, 200), Some((1920, 1080))), Some((900, 560)));
        assert_eq!(fit((300, 200), Some((800, 500))), Some((800, 500)), "a monitor under the minimum wins");
    }

    #[test]
    fn prefs_round_trip_and_a_bad_file_means_defaults() {
        let dir = std::env::temp_dir().join(format!("pmr-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), WindowPrefs::default());
        let p = WindowPrefs { remember: true, width: 1700, height: 950, maximized: false };
        save(&dir, &p).unwrap();
        assert_eq!(load(&dir), p);
        std::fs::write(path(&dir), "not json").unwrap();
        assert_eq!(load(&dir), WindowPrefs::default());
        assert!(WindowPrefs::default().remember, "remembering is the default");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
