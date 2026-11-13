//! Saved filter presets and the remembered view of the Tracks table (app-side).
//!
//! The device has no place for either, so they live in one JSON file per library volume in the
//! app config dir (`views-<hash of the volume>.json`), written atomically (temp, fsync, rename).
//! Nothing here touches the device, so none of it goes through the write gate and it works on a
//! read-only card.
//!
//! The UI owns what a filter state contains (search text, toolbar filters, label filter, sort),
//! so this file stores it as an opaque JSON object and only enforces the things that protect the
//! file: the state is an object of bounded size, preset names are sane and unique, and the
//! number of presets is bounded. The file follows the same safety rules as the tag store
//! (`tags/store.rs`): a missing file is empty; a corrupt one is empty plus a notice and is set
//! aside (never overwritten) before the next save; a file written by a newer build, or one that
//! could not be read at all, is never saved over.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The file format version this build reads and writes.
pub const FORMAT_VERSION: u32 = 1;
/// Longest preset name, in characters.
pub const MAX_NAME_CHARS: usize = 60;
/// Most presets per library.
pub const MAX_PRESETS: usize = 100;
/// Largest serialised filter state, in bytes. A real state is a few hundred bytes; this only
/// stops a runaway caller from filling the config dir.
pub const MAX_STATE_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub state: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Views {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    pub presets: Vec<Preset>,
    /// The view the table was left in, restored when the library is opened again (when the user
    /// has that setting on).
    #[serde(default)]
    pub state: Option<Value>,
}

fn version() -> u32 {
    FORMAT_VERSION
}

impl Default for Views {
    fn default() -> Self {
        Views { version: version(), presets: Vec::new(), state: None }
    }
}

/// A name as it is stored: surrounding space trimmed, inner runs of whitespace collapsed. Fails
/// (worded for the user) on an empty name, a control character or more than [`MAX_NAME_CHARS`]
/// characters.
pub fn normalise_name(raw: &str) -> Result<String, String> {
    if raw.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return Err("A preset name cannot contain control characters.".into());
    }
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("A preset needs a name.".into());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("The preset name is longer than {MAX_NAME_CHARS} characters."));
    }
    Ok(name)
}

/// Two names are the same preset when they are equal ignoring case.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// A filter state must be a JSON object that fits in [`MAX_STATE_BYTES`].
pub fn check_state(state: &Value) -> Result<(), String> {
    if !state.is_object() {
        return Err("The filter state is not an object.".into());
    }
    let size = serde_json::to_vec(state).map_err(|e| e.to_string())?.len();
    if size > MAX_STATE_BYTES {
        return Err(format!("The filter state is too large to save ({size} bytes; the limit is {MAX_STATE_BYTES})."));
    }
    Ok(())
}

impl Views {
    /// Saves `state` under `name`. A preset of the same name (ignoring case) is replaced only
    /// when `replace` is set and keeps its id; otherwise the name being taken is an error. Returns
    /// the preset's id. `fresh_id` supplies a new id (kept out of here so the logic stays pure).
    pub fn save_preset(&mut self, name: &str, state: Value, replace: bool, fresh_id: impl FnOnce(&[Preset]) -> String) -> Result<String, String> {
        let name = normalise_name(name)?;
        check_state(&state)?;
        if let Some(p) = self.presets.iter_mut().find(|p| same_name(&p.name, &name)) {
            if !replace {
                return Err(format!("A preset named \"{}\" already exists.", p.name));
            }
            p.name = name;
            p.state = state;
            return Ok(p.id.clone());
        }
        if self.presets.len() >= MAX_PRESETS {
            return Err(format!("There are already {MAX_PRESETS} presets; delete one first."));
        }
        let id = fresh_id(&self.presets);
        self.presets.push(Preset { id: id.clone(), name, state });
        self.presets.sort_by_cached_key(|p| (p.name.to_lowercase(), p.id.clone()));
        Ok(id)
    }

    /// Deletes a preset by id; false when there is none with that id.
    pub fn delete_preset(&mut self, id: &str) -> bool {
        let before = self.presets.len();
        self.presets.retain(|p| p.id != id);
        self.presets.len() != before
    }

    /// Drops everything invalid, so a hand-edited file cannot put junk on screen: presets with a
    /// bad name or state, repeated names or ids, anything past the cap; an oversized or
    /// non-object remembered state.
    fn sanitise(&mut self) {
        let mut seen_names: Vec<String> = Vec::new();
        let mut seen_ids: Vec<String> = Vec::new();
        let mut kept = Vec::new();
        for p in std::mem::take(&mut self.presets) {
            let Ok(name) = normalise_name(&p.name) else { continue };
            if p.id.is_empty() || check_state(&p.state).is_err() {
                continue;
            }
            let lower = name.to_lowercase();
            if seen_names.contains(&lower) || seen_ids.contains(&p.id) {
                continue;
            }
            seen_names.push(lower);
            seen_ids.push(p.id.clone());
            kept.push(Preset { name, ..p });
        }
        kept.truncate(MAX_PRESETS);
        kept.sort_by_cached_key(|p| (p.name.to_lowercase(), p.id.clone()));
        self.presets = kept;
        if self.state.as_ref().is_some_and(|s| check_state(s).is_err()) {
            self.state = None;
        }
        self.version = version();
    }
}

/// A preset id that no existing preset has: time in milliseconds in hex plus a counter.
pub fn fresh_id(existing: &[Preset]) -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    (0u32..)
        .map(|n| format!("p{ms:x}{n:x}"))
        .find(|id| !existing.iter().any(|p| &p.id == id))
        .unwrap_or_else(|| format!("p{ms:x}"))
}

// ---- storage -----------------------------------------------------------------------------

/// The file for a library, named by a hash of its normalised root path so nothing about the
/// device is written next to the presets.
pub fn store_path(dir: &Path, volume_key: &str) -> PathBuf {
    dir.join(format!("views-{:016x}.json", crate::store::fnv1a(volume_key)))
}

/// What reading the file found.
pub struct Loaded {
    pub views: Views,
    /// Set when the file existed but could not be used.
    pub notice: Option<String>,
    /// The file itself when it is not valid JSON for this format, to be set aside before a save.
    pub corrupt: Option<PathBuf>,
    /// Why a save must be refused (unreadable file, or written by a newer build).
    pub read_only: Option<String>,
}

pub fn load(dir: &Path, volume_key: &str) -> Loaded {
    let p = store_path(dir, volume_key);
    let empty = |notice: Option<String>, corrupt: Option<PathBuf>, read_only: Option<String>| Loaded { views: Views::default(), notice, corrupt, read_only };
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return empty(None, None, None),
        Err(e) => {
            let why = format!("The saved-filters file {} could not be read ({e}); presets are not available and nothing was changed. Try again.", p.display());
            return empty(Some(why.clone()), None, Some(why));
        }
    };
    let bad = |why: String| {
        empty(
            Some(format!(
                "The saved-filters file {} could not be read ({why}); presets start empty. The file is kept as it is until you save something, then set aside next to it.",
                p.display()
            )),
            Some(p.clone()),
            None,
        )
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let file_version = value.get("version").and_then(Value::as_u64).unwrap_or(u64::from(FORMAT_VERSION));
    if file_version > u64::from(FORMAT_VERSION) {
        let why = format!("The saved-filters file {} was written by a newer version of this app (format {file_version}); it is shown read-only and not changed.", p.display());
        let mut views = serde_json::from_value::<Views>(value).unwrap_or_default();
        views.sanitise();
        return Loaded { views, notice: Some(why.clone()), corrupt: None, read_only: Some(why) };
    }
    match serde_json::from_value::<Views>(value) {
        Ok(mut views) => {
            views.sanitise();
            Loaded { views, notice: None, corrupt: None, read_only: None }
        }
        Err(e) => bad(e.to_string()),
    }
}

/// Writes the file (`crate::store::write_atomic`). A corrupt file
/// found by [`load`] is moved aside first; if that fails nothing is saved.
pub fn save(dir: &Path, volume_key: &str, views: &Views, corrupt: Option<&Path>) -> Result<(), String> {
    if let Some(bad) = corrupt {
        crate::store::set_aside(bad).map_err(|e| format!("Could not set the unreadable saved-filters file aside ({e}); nothing was saved."))?;
    }
    crate::store::write_json(&store_path(dir, volume_key), views).map_err(|e| format!("Could not save the filters: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-views-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn st(q: &str) -> Value {
        json!({ "query": q, "genre": "" })
    }

    fn add(v: &mut Views, name: &str, q: &str, replace: bool) -> Result<String, String> {
        v.save_preset(name, st(q), replace, fresh_id)
    }

    #[test]
    fn names_are_trimmed_collapsed_and_bounded() {
        assert_eq!(normalise_name("  Peak   time ").unwrap(), "Peak time");
        assert!(normalise_name("   ").is_err());
        assert!(normalise_name("a\u{0}b").is_err());
        assert_eq!(normalise_name("a\tb").unwrap(), "a b");
        assert!(normalise_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(normalise_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
    }

    #[test]
    fn state_must_be_a_small_object() {
        assert!(check_state(&st("a")).is_ok());
        assert!(check_state(&json!("text")).is_err());
        assert!(check_state(&json!([1, 2])).is_err());
        assert!(check_state(&json!({ "q": "x".repeat(MAX_STATE_BYTES) })).is_err());
    }

    #[test]
    fn presets_save_replace_and_delete() {
        let mut v = Views::default();
        let a = add(&mut v, "Warm up", "bpm:100-120", false).unwrap();
        let b = add(&mut v, "Peak", "bpm:128-140", false).unwrap();
        assert_ne!(a, b);
        assert_eq!(v.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Peak", "Warm up"], "sorted by name");

        // same name ignoring case needs an explicit replace, and keeps the id
        let err = add(&mut v, "  peak ", "x", false).unwrap_err();
        assert!(err.contains("already exists") && err.contains("Peak"), "{err}");
        assert_eq!(v.presets.len(), 2);
        assert_eq!(add(&mut v, "PEAK", "bpm:130-145", true).unwrap(), b);
        let peak = v.presets.iter().find(|p| p.id == b).unwrap();
        assert_eq!((peak.name.as_str(), peak.state["query"].as_str()), ("PEAK", Some("bpm:130-145")));

        assert!(v.delete_preset(&a));
        assert!(!v.delete_preset(&a), "deleting twice reports nothing was deleted");
        assert_eq!(v.presets.len(), 1);
    }

    #[test]
    fn the_preset_count_is_capped_but_replacing_is_not_blocked() {
        let mut v = Views::default();
        for i in 0..MAX_PRESETS {
            add(&mut v, &format!("p{i}"), "x", false).unwrap();
        }
        assert!(add(&mut v, "one too many", "x", false).unwrap_err().contains("delete one first"));
        assert!(add(&mut v, "P5", "y", true).is_ok());
        assert_eq!(v.presets.len(), MAX_PRESETS);
    }

    #[test]
    fn fresh_ids_never_repeat() {
        let mut v = Views::default();
        let ids: Vec<String> = (0..50).map(|i| add(&mut v, &format!("n{i}"), "x", false).unwrap()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
    }

    #[test]
    fn round_trip_through_the_file() {
        let dir = tmp_dir("rt");
        let mut v = Views::default();
        add(&mut v, "Warm up", "tag:warm", false).unwrap();
        v.state = Some(st("remembered"));
        save(&dir, "e:", &v, None).unwrap();
        let back = load(&dir, "e:");
        assert!(back.notice.is_none() && back.read_only.is_none() && back.corrupt.is_none());
        assert_eq!(back.views, v);
        // another library has its own file
        assert!(load(&dir, "f:").views.presets.is_empty());
        assert!(std::fs::read_dir(&dir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")), "no temp file is left behind");
    }

    #[test]
    fn a_missing_file_is_empty_without_a_notice() {
        let l = load(&tmp_dir("missing"), "e:");
        assert!(l.views.presets.is_empty() && l.views.state.is_none() && l.notice.is_none());
    }

    #[test]
    fn a_corrupt_file_is_empty_with_a_notice_and_set_aside_not_overwritten_on_save() {
        let dir = tmp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let p = store_path(&dir, "e:");
        std::fs::write(&p, "{ not json").unwrap();
        let l = load(&dir, "e:");
        assert!(l.notice.as_deref().unwrap().contains("could not be read"));
        assert_eq!(l.corrupt.as_deref(), Some(p.as_path()));
        assert!(l.read_only.is_none());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json", "reading does not touch the file");

        let mut v = Views::default();
        add(&mut v, "Fresh", "x", false).unwrap();
        save(&dir, "e:", &v, l.corrupt.as_deref()).unwrap();
        let aside = PathBuf::from(format!("{}.corrupt", p.display()));
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "{ not json", "the bad copy is kept");
        assert_eq!(load(&dir, "e:").views.presets.len(), 1);

        // a second bad file does not replace the first copy
        std::fs::write(&p, "[1").unwrap();
        let l = load(&dir, "e:");
        save(&dir, "e:", &v, l.corrupt.as_deref()).unwrap();
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "{ not json");
        assert_eq!(std::fs::read_to_string(PathBuf::from(format!("{}.corrupt.1", p.display()))).unwrap(), "[1");
    }

    #[test]
    fn a_newer_file_is_read_only() {
        let dir = tmp_dir("newer");
        std::fs::create_dir_all(&dir).unwrap();
        let p = store_path(&dir, "e:");
        std::fs::write(&p, json!({ "version": 99, "presets": [{ "id": "a", "name": "Future", "state": { "query": "q" } }], "extra": 1 }).to_string()).unwrap();
        let l = load(&dir, "e:");
        assert!(l.read_only.as_deref().unwrap().contains("newer version"));
        assert_eq!(l.views.presets.len(), 1, "what this build understands is still shown");
        assert!(l.corrupt.is_none());
    }

    #[test]
    fn hand_edited_junk_is_dropped_on_load() {
        let dir = tmp_dir("junk");
        std::fs::create_dir_all(&dir).unwrap();
        let big = "x".repeat(MAX_STATE_BYTES);
        std::fs::write(
            store_path(&dir, "e:"),
            json!({
                "version": 1,
                "presets": [
                    { "id": "1", "name": "  Good   one ", "state": { "query": "a" } },
                    { "id": "2", "name": "good one", "state": { "query": "dup name" } },
                    { "id": "1", "name": "Dup id", "state": { "query": "b" } },
                    { "id": "3", "name": "", "state": { "query": "c" } },
                    { "id": "4", "name": "Not an object", "state": "oops" },
                    { "id": "5", "name": "Too big", "state": { "q": big } },
                    { "id": "", "name": "No id", "state": {} },
                ],
                "state": "not an object",
            })
            .to_string(),
        )
        .unwrap();
        let l = load(&dir, "e:");
        assert!(l.notice.is_none());
        assert_eq!(l.views.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Good one"]);
        assert!(l.views.state.is_none());
    }

    #[test]
    fn an_unreadable_path_refuses_to_save() {
        // a directory where the file should be: reading it fails with something other than NotFound
        let dir = tmp_dir("unreadable");
        std::fs::create_dir_all(store_path(&dir, "e:")).unwrap();
        let l = load(&dir, "e:");
        assert!(l.read_only.is_some() && l.notice.is_some());
    }
}
