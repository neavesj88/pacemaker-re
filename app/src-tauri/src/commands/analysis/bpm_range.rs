//! The "BPM range" setting: the window every analysed tempo is doubled or halved into, so drum &
//! bass reads at its real ~174 (92-184, the default) or at the original Editor's ~87 (78-156).
//!
//! One saved value (`bpm-range-settings.json` in the app config folder, never on the device).
//! Every place that analyses audio calls [`current`] when it starts: adding tracks, re-analysing,
//! the grid editor's re-analyse and the grid batch re-analyse.

use std::path::{Path, PathBuf};

use pm_analysis::tempo::BpmRange;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppResult, IntoApp};
use crate::store::config_dir;

/// The saved choice: the low end of the window (the high end is twice that).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lo: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { lo: BpmRange::default().lo }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.lo.is_finite() && BpmRange::from_lo(self.lo).is_some() {
            Ok(())
        } else {
            let all: Vec<String> = BpmRange::PRESETS.iter().map(|r| format!("{}-{}", r.lo, r.hi())).collect();
            Err(format!("The BPM range must be one of {}.", all.join(", ")))
        }
    }

    pub fn range(&self) -> BpmRange {
        BpmRange::from_lo(self.lo).unwrap_or_default()
    }
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("bpm-range-settings.json")
}

/// Saved settings, or the default when there is no file or it cannot be read or is not one of the
/// offered windows.
pub fn load_settings(dir: &Path) -> Settings {
    std::fs::read_to_string(settings_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .filter(|s| s.validate().is_ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    s.validate()?;
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    crate::store::write_bytes(&settings_path(dir), text.as_bytes())
}

/// The range to analyse with now. Falls back to the default when the config folder is unreadable.
pub fn current(app: &AppHandle) -> BpmRange {
    config_dir(app).map(|d| load_settings(&d).range()).unwrap_or_default()
}

/// The saved BPM range.
#[tauri::command]
pub fn analysis_bpm_range_get(app: AppHandle) -> AppResult<Settings> {
    Ok(load_settings(&config_dir(&app)?))
}

/// Saves the BPM range after checking it is one of the offered windows; returns what was saved.
#[tauri::command]
pub fn analysis_bpm_range_set(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    save_settings(&config_dir(&app)?, &settings).app()?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-bpm-range-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn default_is_drum_and_bass_at_full_time() {
        let d = dir("default");
        assert_eq!(load_settings(&d).range(), BpmRange::DNB_FULL_TIME);
        assert_eq!(load_settings(&d).lo, 92.0);
    }

    #[test]
    fn saves_each_offered_window_and_refuses_others() {
        let d = dir("save");
        for r in BpmRange::PRESETS {
            save_settings(&d, &Settings { lo: r.lo }).unwrap();
            assert_eq!(load_settings(&d).range(), r);
        }
        assert!(save_settings(&d, &Settings { lo: 90.0 }).is_err());
        assert!(save_settings(&d, &Settings { lo: f64::NAN }).is_err());
        // a refused save leaves the last good value (PRESETS ends with the widest window)
        assert_eq!(load_settings(&d).range(), BpmRange::FAST);
    }

    #[test]
    fn a_damaged_or_out_of_range_file_reads_as_the_default() {
        let d = dir("bad");
        std::fs::write(settings_path(&d), "{ not json").unwrap();
        assert_eq!(load_settings(&d).lo, 92.0);
        std::fs::write(settings_path(&d), "{\"lo\": 12}").unwrap();
        assert_eq!(load_settings(&d).lo, 92.0);
    }
}
