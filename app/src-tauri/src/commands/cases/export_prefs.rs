//! The user's choices for exporting cases: the folder and format they used last, and every policy
//! of the bulk export (relative paths, copying the audio, what to do with files already there).
//!
//! They are the app's own preferences, so they live in one JSON file in the app's config dir (not
//! on the device, and not per card). Same rules as `smart_settings.rs`: a missing file is the
//! defaults; an unreadable or damaged one is also the defaults, with a warning the UI shows, and
//! the damaged file is kept as `.bad` the first time it is written over; one value this version
//! does not understand costs only that value.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where copied audio files go when "copy the audio" is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// One `Audio` folder beside the playlists; a track in several cases is copied once.
    #[default]
    Shared,
    /// A folder per case holding its playlist and its own copies of the audio (a track in
    /// several cases is copied into each), so every folder can be moved on its own.
    PerCase,
}

/// How copied audio files are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NameBy {
    /// "Artist - Title.mp3", from the track's tags (the file name on the card where a tag is empty).
    #[default]
    Tags,
    /// The file's own name on the card (a short code the device chose).
    Device,
}

/// What to do with a file the export would write where one already exists. An export never
/// replaces anything unless the user chose `Replace` and was shown every file it affects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Conflict {
    /// Leave the existing file as it is and do not write that one.
    #[default]
    Skip,
    /// Write over the existing file.
    Replace,
    /// Keep the existing file and write the new one under a numbered name ("Name (2).m3u8").
    Number,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPrefs {
    /// The folder the last export (or the user, in the dialog) chose; offered again next time.
    pub folder: Option<String>,
    /// "m3u8", "csv" or "rekordbox".
    pub format: String,
    /// M3U8 only: write each track's path relative to the playlist file where that is possible.
    pub relative_paths: bool,
    /// Copy the audio files next to the playlists (and point the lists at the copies).
    pub copy_audio: bool,
    pub audio_layout: Layout,
    pub audio_names: NameBy,
    pub conflict: Conflict,
    /// Save the folder and format after each export. Off: they stay as set here.
    pub remember: bool,
}

impl Default for ExportPrefs {
    fn default() -> Self {
        Self { folder: None, format: "m3u8".into(), relative_paths: false, copy_audio: false, audio_layout: Layout::Shared, audio_names: NameBy::Tags, conflict: Conflict::Skip, remember: true }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join("export-prefs.json")
}

fn is_format(f: &str) -> bool {
    super::export::extension(f).is_some()
}

/// Reads one field at a time, so a value this version does not understand costs only itself.
fn from_value(v: &Value) -> (ExportPrefs, Vec<&'static str>) {
    let mut p = ExportPrefs::default();
    let mut ignored = Vec::new();
    match v.get("folder") {
        Some(Value::String(s)) => p.folder = (!s.trim().is_empty()).then(|| s.clone()),
        Some(Value::Null) | None => {}
        Some(_) => ignored.push("folder"),
    }
    match v.get("format") {
        Some(Value::String(s)) if is_format(s) => p.format = s.clone(),
        Some(_) => ignored.push("format"),
        None => {}
    }
    for (key, slot) in [("relative_paths", &mut p.relative_paths), ("copy_audio", &mut p.copy_audio), ("remember", &mut p.remember)] {
        match v.get(key) {
            Some(Value::Bool(b)) => *slot = *b,
            Some(_) => ignored.push(key),
            None => {}
        }
    }
    match v.get("audio_layout").map(|m| serde_json::from_value::<Layout>(m.clone())) {
        Some(Ok(m)) => p.audio_layout = m,
        Some(Err(_)) => ignored.push("audio_layout"),
        None => {}
    }
    match v.get("audio_names").map(|m| serde_json::from_value::<NameBy>(m.clone())) {
        Some(Ok(m)) => p.audio_names = m,
        Some(Err(_)) => ignored.push("audio_names"),
        None => {}
    }
    match v.get("conflict").map(|m| serde_json::from_value::<Conflict>(m.clone())) {
        Some(Ok(m)) => p.conflict = m,
        Some(Err(_)) => ignored.push("conflict"),
        None => {}
    }
    (p, ignored)
}

/// The saved preferences and, when something in the file could not be used, a sentence saying so.
pub fn load(dir: &Path) -> (ExportPrefs, Option<String>) {
    let p = path(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (ExportPrefs::default(), None),
        Err(e) => return (ExportPrefs::default(), Some(format!("{} could not be read ({e}); the default export choices are in use.", p.display()))),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => {
            let (s, ignored) = from_value(&v);
            let warn = (!ignored.is_empty()).then(|| format!("{}: {} could not be understood and {} the default.", p.display(), ignored.join(", "), if ignored.len() == 1 { "uses" } else { "use" }));
            (s, warn)
        }
        _ => (ExportPrefs::default(), Some(format!("{} is not an export-choices file; the default export choices are in use.", p.display()))),
    }
}

/// Writes the preferences (temp file, then rename). A file that could not be read is copied to
/// `export-prefs.json.bad` first, once, so a hand edit is never silently lost.
pub fn save(dir: &Path, s: &ExportPrefs) -> Result<(), String> {
    if !is_format(&s.format) {
        return Err(format!("unknown export format {}", s.format));
    }
    crate::store::save_keeping_bad(&path(dir), s, load(dir).1.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-exportprefs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn missing_file_is_the_defaults_and_a_save_round_trips() {
        let d = temp_dir("rt");
        assert_eq!(load(&d), (ExportPrefs::default(), None));
        let p = ExportPrefs {
            folder: Some("C:\\Exports\\Sets".into()),
            format: "rekordbox".into(),
            relative_paths: true,
            copy_audio: true,
            audio_layout: Layout::PerCase,
            audio_names: NameBy::Device,
            conflict: Conflict::Number,
            remember: false,
        };
        save(&d, &p).unwrap();
        assert_eq!(load(&d), (p, None));
        assert!(!d.join("export-prefs.json.tmp").exists(), "no temp file left");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn defaults_never_overwrite_and_never_copy() {
        let p = ExportPrefs::default();
        assert_eq!((p.conflict, p.copy_audio, p.relative_paths, p.format.as_str()), (Conflict::Skip, false, false, "m3u8"));
    }

    #[test]
    fn one_bad_value_costs_only_that_value() {
        let d = temp_dir("part");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("export-prefs.json"), r#"{"folder":"D:\\Out","format":"pdf","copy_audio":true,"conflict":"explode","relative_paths":"yes"}"#).unwrap();
        let (p, warn) = load(&d);
        assert_eq!((p.folder.as_deref(), p.format.as_str(), p.copy_audio, p.conflict, p.relative_paths), (Some("D:\\Out"), "m3u8", true, Conflict::Skip, false));
        let w = warn.unwrap();
        assert!(w.contains("format") && w.contains("conflict") && w.contains("relative_paths") && !w.contains("folder"), "{w}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_damaged_file_is_the_defaults_with_a_warning_and_is_kept_once_when_written_over() {
        let d = temp_dir("bad");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("export-prefs.json"), "not json").unwrap();
        let (p, warn) = load(&d);
        assert_eq!(p, ExportPrefs::default());
        assert!(warn.unwrap().contains("not an export-choices file"));
        save(&d, &ExportPrefs::default()).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("export-prefs.json.bad")).unwrap(), "not json");
        assert_eq!(load(&d).1, None);
        // a second damaged file does not replace the first copy
        std::fs::write(d.join("export-prefs.json"), "[1]").unwrap();
        save(&d, &ExportPrefs::default()).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("export-prefs.json.bad")).unwrap(), "not json");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_unknown_format_is_refused_on_save() {
        let d = temp_dir("fmt");
        assert!(save(&d, &ExportPrefs { format: "pdf".into(), ..Default::default() }).is_err());
        assert!(!d.join("export-prefs.json").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
