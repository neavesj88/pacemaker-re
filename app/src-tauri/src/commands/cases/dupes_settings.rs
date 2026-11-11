//! Settings of the duplicate finder: which ways of finding copies are on, how strict each is,
//! how the copy to keep is chosen, and what a merge does with ratings, plays and case
//! memberships. App preferences, kept in one JSON file in the app's config dir (not on the
//! device, not per card). A missing file is the defaults; an unreadable or damaged one is also
//! the defaults, with a warning the UI shows, and the damaged file is kept as `.bad` the first
//! time it is written over. One value this version cannot use costs only that setting.

use std::path::{Path, PathBuf};

use pm_formats::library::{MergeOptions, PlaysRule, RatingRule};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which copy is suggested as the one to keep. Ties fall to the next of: bitrate, file size,
/// plays, newest; and a copy whose file is missing from the device is never suggested while
/// another is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeeperRule {
    /// The highest bitrate (the best quality).
    #[default]
    Bitrate,
    /// The most plays.
    Plays,
    /// The most recently added.
    Newest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DupSettings {
    /// Find copies by the same artist and title.
    pub by_tags: bool,
    /// Find copies with the same file size and length.
    pub by_size: bool,
    /// Find copies by the sound itself (needs the fingerprint job to have run).
    pub by_audio: bool,
    /// Tags: ignore "feat. ..." / "ft. ..." in artist and title.
    pub strip_feat: bool,
    /// Tags: ignore text in brackets (off: "Original Mix" and "Extended Mix" stay different).
    pub ignore_brackets: bool,
    /// Tags: ignore a leading "The ".
    pub ignore_the: bool,
    /// Tags: lengths may differ by this many seconds; 0 turns the length check off.
    pub tag_duration_secs: u32,
    /// Size: lengths may differ by this many seconds.
    pub size_duration_secs: u32,
    /// Audio: how alike the sound must be, 0.5 to 1 (1 = identical shape).
    pub audio_threshold: f32,
    /// Audio: only tracks whose lengths differ by at most this many seconds are compared.
    pub audio_duration_secs: u32,
    /// Audio: one copy may start this many seconds later than the other.
    pub audio_offset_secs: u32,
    /// Audio: tracks decoded at once while fingerprinting; 0 picks by the machine.
    pub fingerprint_threads: u32,
    pub keeper_rule: KeeperRule,
    /// Merge: copies' case memberships move to the kept track.
    pub move_cases: bool,
    pub rating_rule: RatingRule,
    pub plays_rule: PlaysRule,
}

impl Default for DupSettings {
    fn default() -> Self {
        DupSettings {
            by_tags: true,
            by_size: true,
            by_audio: false,
            strip_feat: true,
            ignore_brackets: false,
            ignore_the: true,
            tag_duration_secs: 10,
            size_duration_secs: 1,
            audio_threshold: 0.90,
            audio_duration_secs: 3,
            audio_offset_secs: 5,
            fingerprint_threads: 0,
            keeper_rule: KeeperRule::Bitrate,
            move_cases: true,
            rating_rule: RatingRule::Max,
            plays_rule: PlaysRule::Max,
        }
    }
}

impl DupSettings {
    /// Brings every number into the range the finder works in.
    pub fn clamped(mut self) -> Self {
        self.tag_duration_secs = self.tag_duration_secs.min(600);
        self.size_duration_secs = self.size_duration_secs.min(600);
        self.audio_duration_secs = self.audio_duration_secs.min(60);
        self.audio_offset_secs = self.audio_offset_secs.min(30);
        self.fingerprint_threads = self.fingerprint_threads.min(8);
        self.audio_threshold = if self.audio_threshold.is_finite() { self.audio_threshold.clamp(0.5, 1.0) } else { 0.90 };
        self
    }

    /// What a merge does with the group, as pm-formats takes it. `remove_rows` is the per-merge
    /// choice, never a stored setting.
    pub fn merge_options(&self, remove_rows: bool) -> MergeOptions {
        MergeOptions { move_cases: self.move_cases, rating: self.rating_rule, plays: self.plays_rule, remove_rows }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join("duplicate-settings.json")
}

/// Reads one field at a time: a value of the wrong type is dropped (the default stays) and named.
fn from_value(v: &Value) -> (DupSettings, Vec<String>) {
    let mut base = serde_json::to_value(DupSettings::default()).unwrap_or(Value::Null);
    let mut ignored = Vec::new();
    if let (Some(obj), Some(file)) = (base.as_object_mut(), v.as_object()) {
        for (key, value) in file {
            let Some(old) = obj.get(key).cloned() else { continue }; // a key from a newer version
            obj.insert(key.clone(), value.clone());
            if serde_json::from_value::<DupSettings>(Value::Object(obj.clone())).is_err() {
                obj.insert(key.clone(), old);
                ignored.push(key.clone());
            }
        }
    }
    let settings = serde_json::from_value(base).unwrap_or_default();
    (DupSettings::clamped(settings), ignored)
}

/// The saved settings and, when something in the file could not be used, a sentence saying so.
pub fn load(dir: &Path) -> (DupSettings, Option<String>) {
    let p = path(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (DupSettings::default(), None),
        Err(e) => return (DupSettings::default(), Some(format!("{} could not be read ({e}); the default duplicate settings are in use.", p.display()))),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => {
            let (s, ignored) = from_value(&v);
            let warn = (!ignored.is_empty()).then(|| format!("{}: {} could not be understood and {} the default.", p.display(), ignored.join(", "), if ignored.len() == 1 { "uses" } else { "use" }));
            (s, warn)
        }
        _ => (DupSettings::default(), Some(format!("{} is not a settings file; the default duplicate settings are in use.", p.display()))),
    }
}

/// Writes the settings (temp file, then rename). A file that could not be read is copied to
/// `duplicate-settings.json.bad` first, once, so a hand edit is never silently lost.
pub fn save(dir: &Path, s: &DupSettings) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = path(dir);
    if load(dir).1.is_some() && p.exists() {
        let bad = p.with_extension("json.bad");
        if !bad.exists() {
            std::fs::copy(&p, &bad).map_err(|e| e.to_string())?;
        }
    }
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&s.clone().clamped()).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p)
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-dupset-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let (s, warn) = load(&temp_dir("defaults"));
        assert!(warn.is_none());
        assert!(s.by_tags && s.by_size && !s.by_audio, "the sound check is opt-in: it needs the fingerprint job");
        assert_eq!((s.keeper_rule, s.move_cases, s.rating_rule, s.plays_rule), (KeeperRule::Bitrate, true, RatingRule::Max, PlaysRule::Max));
        assert_eq!((s.tag_duration_secs, s.size_duration_secs, s.audio_duration_secs, s.audio_offset_secs), (10, 1, 3, 5));
        assert!(!s.merge_options(false).remove_rows && s.merge_options(true).remove_rows);
    }

    #[test]
    fn saved_settings_come_back_snake_case_and_clamped() {
        let d = temp_dir("roundtrip");
        let s = DupSettings { by_audio: true, keeper_rule: KeeperRule::Newest, plays_rule: PlaysRule::Sum, audio_threshold: 7.0, audio_offset_secs: 999, fingerprint_threads: 99, ..Default::default() };
        save(&d, &s).unwrap();
        let (back, warn) = load(&d);
        assert!(warn.is_none());
        assert_eq!((back.keeper_rule, back.plays_rule, back.by_audio), (KeeperRule::Newest, PlaysRule::Sum, true));
        assert_eq!((back.audio_threshold, back.audio_offset_secs, back.fingerprint_threads), (1.0, 30, 8));
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path(&d)).unwrap()).unwrap();
        assert_eq!(raw["keeper_rule"], "newest");
        assert_eq!(raw["plays_rule"], "sum");
        assert!(!path(&d).with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn one_bad_value_costs_only_that_setting() {
        let d = temp_dir("partial");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(path(&d), r#"{"keeper_rule":"loudest","by_size":false,"tag_duration_secs":"ten","future_key":1,"audio_threshold":0.8}"#).unwrap();
        let (s, warn) = load(&d);
        assert_eq!((s.keeper_rule, s.by_size, s.tag_duration_secs, s.audio_threshold), (KeeperRule::Bitrate, false, 10, 0.8));
        let warn = warn.unwrap();
        assert!(warn.contains("keeper_rule") && warn.contains("tag_duration_secs") && !warn.contains("future_key") && !warn.contains("by_size"), "{warn}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_damaged_file_gives_defaults_a_warning_and_is_kept_once() {
        let d = temp_dir("corrupt");
        std::fs::create_dir_all(&d).unwrap();
        for junk in ["not json", "[1,2]", ""] {
            std::fs::write(path(&d), junk).unwrap();
            let (s, warn) = load(&d);
            assert_eq!(s, DupSettings::default());
            assert!(warn.is_some(), "{junk:?}");
        }
        std::fs::write(path(&d), "hand edited {").unwrap();
        save(&d, &DupSettings { by_tags: false, ..Default::default() }).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d).with_extension("json.bad")).unwrap(), "hand edited {");
        assert!(!load(&d).0.by_tags);
        save(&d, &DupSettings::default()).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d).with_extension("json.bad")).unwrap(), "hand edited {", "a healthy file does not replace the kept copy");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
