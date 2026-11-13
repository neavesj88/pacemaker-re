//! Settings of the smart-case features that are choices, not rules: whether the device copies
//! of smart cases are refreshed by themselves, and how smart cases appear in exports.
//!
//! They are the app's own preferences, so they live in one JSON file in the app's config dir
//! (not on the device, and not per card: the same habits apply to every library). A missing
//! file is the defaults; an unreadable or damaged one is also the defaults, with a warning the
//! UI shows, and the damaged file is kept as `.bad` the first time it is written over.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What happens when the library is opened or edited and a smart case's Pacemaker copy no
/// longer matches its rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RefreshMode {
    /// Never by itself; only the "Refresh smart cases" button.
    Off,
    /// Say what would change and wait for a yes.
    #[default]
    Ask,
    /// Write it, then say what changed.
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartSettings {
    pub refresh_mode: RefreshMode,
    /// Library-wide rekordbox XML export: write smart cases as playlists of their current
    /// matches (and leave out their star-prefixed device copies, which say the same).
    pub export_smart: bool,
    /// ...inside a folder called "Smart cases" instead of beside the ordinary cases.
    pub export_folder: bool,
}

impl Default for SmartSettings {
    fn default() -> Self {
        Self { refresh_mode: RefreshMode::Ask, export_smart: true, export_folder: true }
    }
}

/// The name of the folder smart cases go in when [`SmartSettings::export_folder`] is on.
pub const EXPORT_FOLDER: &str = "Smart cases";

fn path(dir: &Path) -> PathBuf {
    dir.join("smart-settings.json")
}

/// Reads one field at a time, so one value from a newer or hand-edited file that this version
/// does not understand costs only that setting.
fn from_value(v: &Value) -> (SmartSettings, Vec<&'static str>) {
    let mut s = SmartSettings::default();
    let mut ignored = Vec::new();
    match v.get("refresh_mode").map(|m| serde_json::from_value::<RefreshMode>(m.clone())) {
        Some(Ok(m)) => s.refresh_mode = m,
        Some(Err(_)) => ignored.push("refresh_mode"),
        None => {}
    }
    for (key, slot) in [("export_smart", &mut s.export_smart), ("export_folder", &mut s.export_folder)] {
        match v.get(key) {
            Some(Value::Bool(b)) => *slot = *b,
            Some(_) => ignored.push(key),
            None => {}
        }
    }
    (s, ignored)
}

/// The saved settings and, when something in the file could not be used, a sentence saying so
/// (the defaults are used for what could not be read).
pub fn load(dir: &Path) -> (SmartSettings, Option<String>) {
    let p = path(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (SmartSettings::default(), None),
        Err(e) => return (SmartSettings::default(), Some(format!("{} could not be read ({e}); the default smart-case settings are in use.", p.display()))),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => {
            let (s, ignored) = from_value(&v);
            let warn = (!ignored.is_empty()).then(|| format!("{}: {} could not be understood and {} the default.", p.display(), ignored.join(", "), if ignored.len() == 1 { "uses" } else { "use" }));
            (s, warn)
        }
        _ => (SmartSettings::default(), Some(format!("{} is not a settings file; the default smart-case settings are in use.", p.display()))),
    }
}

/// Writes the settings (temp file, then rename). A file that could not be read is copied to
/// `smart-settings.json.bad` first, once, so a hand edit is never silently lost.
pub fn save(dir: &Path, s: &SmartSettings) -> Result<(), String> {
    crate::store::save_keeping_bad(&path(dir), s, load(dir).1.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-smartset-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_are_ask_and_include_smart_cases_in_a_folder() {
        let d = temp_dir("defaults");
        let (s, warn) = load(&d);
        assert_eq!((s.refresh_mode, s.export_smart, s.export_folder), (RefreshMode::Ask, true, true));
        assert!(warn.is_none(), "no file is not a problem");
    }

    #[test]
    fn saved_settings_come_back_and_the_file_is_snake_case_json() {
        let d = temp_dir("roundtrip");
        let s = SmartSettings { refresh_mode: RefreshMode::Auto, export_smart: false, export_folder: false };
        save(&d, &s).unwrap();
        assert_eq!(load(&d), (s.clone(), None));
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path(&d)).unwrap()).unwrap();
        assert_eq!(raw["refresh_mode"], "auto");
        assert!(!path(&d).with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn one_bad_value_costs_only_that_setting() {
        let d = temp_dir("partial");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(path(&d), r#"{"refresh_mode":"sometimes","export_smart":false,"export_folder":"yes","future_key":1}"#).unwrap();
        let (s, warn) = load(&d);
        assert_eq!((s.refresh_mode, s.export_smart, s.export_folder), (RefreshMode::Ask, false, true));
        let warn = warn.unwrap();
        assert!(warn.contains("refresh_mode") && warn.contains("export_folder") && !warn.contains("future_key"), "{warn}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_damaged_file_gives_defaults_a_warning_and_is_kept_once_before_being_overwritten() {
        let d = temp_dir("corrupt");
        std::fs::create_dir_all(&d).unwrap();
        for junk in ["not json", "[1,2]", ""] {
            std::fs::write(path(&d), junk).unwrap();
            let (s, warn) = load(&d);
            assert_eq!(s, SmartSettings::default());
            assert!(warn.is_some(), "{junk:?}");
        }
        std::fs::write(path(&d), "hand edited {").unwrap();
        save(&d, &SmartSettings { refresh_mode: RefreshMode::Off, ..Default::default() }).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d).with_extension("json.bad")).unwrap(), "hand edited {");
        assert_eq!(load(&d).0.refresh_mode, RefreshMode::Off);
        // a second write does not replace the kept copy (the file is healthy now)
        save(&d, &SmartSettings::default()).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d).with_extension("json.bad")).unwrap(), "hand edited {");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
