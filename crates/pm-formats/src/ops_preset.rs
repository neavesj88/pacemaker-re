//! Settings presets (owner: device area): the editable values of a card's `settings.dat` saved
//! as a named JSON file, shown against another card as a named diff, and applied to it.
//!
//! The device cannot store a preset, so presets are files of this app (the app keeps them in its
//! config folder, one file per name, and can export / import them as plain `.json`). A preset
//! holds only what the settings editor can change: the bool and list settings the card's file
//! holds. The read-only engine levels are left out (their units are unknown), and so is anything
//! the device itself keeps (serial, firmware file).
//!
//! The preview and the action share one function: [`diff`] decides, per setting, whether it
//! changes, is the same, or cannot be applied to this card and why; [`apply`] runs that same
//! function on the card as it is at that moment and writes exactly the rows it says change, via
//! `ops_device::write_settings` (which re-checks every value, rewrites only the changed items
//! and writes atomically). Nothing here opens the card for writing except that one call.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::library::{atomic_write, Error};
use crate::ops_device::{self, SettingView, SettingsView};

/// The `format` field of a preset file.
pub const PRESET_FORMAT: &str = "pacemaker-revival-settings-preset";
/// The only file version this build writes and reads.
pub const PRESET_VERSION: u32 = 1;
/// Longest preset name, in characters.
pub const MAX_NAME_CHARS: usize = 80;
/// Largest preset file that is read (a real one is a few kilobytes).
pub const MAX_FILE_BYTES: u64 = 1 << 20;
/// More settings than the editor knows by a wide margin; a file beyond this is not a preset.
const MAX_SETTINGS: usize = 256;
const MAX_TEXT_CHARS: usize = 120;

fn invalid<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(msg.into()))
}

// ---- the preset ------------------------------------------------------------------------------

/// One setting of a preset. `group`, `title`, `kind` and `label` are for people (and for the
/// safety check in [`diff`]); the card is matched by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetSetting {
    pub id: i64,
    pub group: String,
    pub title: String,
    /// "bool" or "list".
    pub kind: String,
    /// bool: 0 or 1; list: the option index.
    pub value: f64,
    /// The option's name when the preset was saved, e.g. "Bright".
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub format: String,
    pub version: u32,
    pub name: String,
    /// "YYYYMMDDTHHMMSSZ" (UTC) when it was saved from a card; empty when unknown.
    #[serde(default)]
    pub saved_utc: String,
    /// The software version the source card reported, for information only.
    #[serde(default)]
    pub firmware: Option<String>,
    pub settings: Vec<PresetSetting>,
}

fn has_control(s: &str) -> bool {
    s.chars().any(|c| c.is_control())
}

/// A name as the user typed it, with outer spaces removed and runs of spaces (or a stray new
/// line or tab) made one space. Rejects empty, over-long and other control characters.
pub fn clean_name(raw: &str) -> Result<String, Error> {
    if raw.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return invalid("a preset name cannot contain control characters");
    }
    let n = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = n.as_str();
    if n.is_empty() {
        return invalid("a preset needs a name");
    }
    if n.chars().count() > MAX_NAME_CHARS {
        return invalid(format!("a preset name is at most {MAX_NAME_CHARS} characters"));
    }
    if has_control(n) {
        return invalid("a preset name cannot contain control characters");
    }
    Ok(n.to_string())
}

/// Two names are the same preset when they match ignoring case and extra spaces.
pub fn name_key(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The file a preset lives in, inside the presets folder: a readable slug plus a hash of the
/// [`name_key`], so the file name is always plain ASCII, never a reserved device name, and two
/// names never share a file unless they are the same preset. Nothing the user typed becomes a
/// path component.
pub fn file_name_for(name: &str) -> String {
    let key = name_key(name);
    let mut slug = String::new();
    for c in key.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c == '-' && (slug.is_empty() || slug.ends_with('-')) {
            continue;
        }
        slug.push(c);
    }
    let slug = slug.trim_end_matches('-');
    let slug: String = slug.chars().take(40).collect();
    let slug = if slug.is_empty() { "preset".to_string() } else { slug.trim_end_matches('-').to_string() };
    let h = crate::appfile::fnv1a(&key);
    format!("{slug}-{h:016x}.json")
}

fn label_of(s: &SettingView, v: f64) -> String {
    s.options.get(v as usize).cloned().unwrap_or_else(|| format!("#{v}"))
}

/// A preset of the editable settings in a card's settings view. `firmware` is the card's
/// software version (information only). Fails if the card holds nothing that can be saved.
pub fn from_settings(name: &str, view: &SettingsView, firmware: Option<String>, unix_secs: u64) -> Result<Preset, Error> {
    let settings: Vec<PresetSetting> = view
        .settings
        .iter()
        .filter(|s| s.present && s.editable && (s.kind == "bool" || s.kind == "list"))
        .map(|s| PresetSetting {
            id: s.id,
            group: s.group.clone(),
            title: s.title.clone(),
            kind: s.kind.clone(),
            value: s.value,
            label: label_of(s, s.value),
        })
        .collect();
    if settings.is_empty() {
        return invalid("this card's settings file holds no setting the editor can save");
    }
    let preset = Preset {
        format: PRESET_FORMAT.into(),
        version: PRESET_VERSION,
        name: clean_name(name)?,
        saved_utc: ops_device::utc_stamp(unix_secs),
        firmware: firmware.filter(|f| !f.trim().is_empty()),
        settings,
    };
    validate(&preset)?;
    Ok(preset)
}

/// Checks a preset read from outside (a file the user picked, or one in the presets folder).
pub fn validate(p: &Preset) -> Result<(), Error> {
    if p.format != PRESET_FORMAT {
        return invalid("this is not a Pacemaker Revival settings preset");
    }
    if p.version != PRESET_VERSION {
        return invalid(format!("preset file version {} (this build reads version {PRESET_VERSION}); update the app", p.version));
    }
    if clean_name(&p.name)? != p.name {
        return invalid("the preset name has extra spaces or line breaks");
    }
    if p.settings.is_empty() {
        return invalid("the preset holds no settings");
    }
    if p.settings.len() > MAX_SETTINGS {
        return invalid(format!("the preset holds {} settings; that is not a settings preset", p.settings.len()));
    }
    let mut seen = HashSet::new();
    for s in &p.settings {
        if !seen.insert(s.id) {
            return invalid(format!("setting {} appears twice", s.id));
        }
        if s.kind != "bool" && s.kind != "list" {
            return invalid(format!("setting {} ({}) has kind {:?}; only bool and list can be saved", s.id, s.title, s.kind));
        }
        if !s.value.is_finite() || s.value.fract() != 0.0 || s.value < 0.0 || (s.kind == "bool" && s.value > 1.0) {
            return invalid(format!("setting {} ({}) has the value {}, which is not a valid {}", s.id, s.title, s.value, s.kind));
        }
        for t in [&s.group, &s.title, &s.label] {
            if t.chars().count() > MAX_TEXT_CHARS || has_control(t) {
                return invalid(format!("setting {} has a text that is too long or holds control characters", s.id));
            }
        }
    }
    Ok(())
}

/// Parses and validates the text of a preset file.
pub fn parse(text: &str) -> Result<Preset, Error> {
    if text.len() as u64 > MAX_FILE_BYTES {
        return invalid("the file is too large to be a settings preset");
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p: Preset = serde_json::from_str(text).map_err(|e| Error::Invalid(format!("not a settings preset: {e}")))?;
    // a hand-edited file may have stray spaces; the stored name is the cleaned one
    if p.format == PRESET_FORMAT {
        p.name = clean_name(&p.name)?;
    }
    validate(&p)?;
    Ok(p)
}

/// The file text of a preset: indented, ends with a new line.
pub fn to_json(p: &Preset) -> Result<String, Error> {
    validate(p)?;
    let mut s = serde_json::to_string_pretty(p).map_err(|e| Error::Invalid(e.to_string()))?;
    s.push('\n');
    Ok(s)
}

/// Reads a preset file the user picked. Refuses files over [`MAX_FILE_BYTES`] before reading them.
pub fn read_file(path: &Path) -> Result<Preset, Error> {
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return invalid(format!("{} is not a file", path.display()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return invalid(format!("{} is too large to be a settings preset", path.display()));
    }
    let bytes = fs::read(path)?;
    let text = String::from_utf8(bytes).or_else(|_| invalid(format!("{} is not text", path.display())))?;
    parse(&text)
}

/// Writes a preset to a file the user chose (temp file, fsync, rename). Overwriting is the
/// caller's decision (the save dialog already asked).
pub fn write_file(path: &Path, p: &Preset) -> Result<(), Error> {
    let text = to_json(p)?;
    atomic_write(path, text.as_bytes())?;
    Ok(())
}

// ---- the presets folder ----------------------------------------------------------------------

/// A preset in the list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PresetSummary {
    pub name: String,
    /// The file name inside the presets folder.
    pub file: String,
    pub settings: usize,
    pub saved_utc: String,
    pub firmware: Option<String>,
}

fn summary_of(p: &Preset, file: &str) -> PresetSummary {
    PresetSummary { name: p.name.clone(), file: file.to_string(), settings: p.settings.len(), saved_utc: p.saved_utc.clone(), firmware: p.firmware.clone() }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PresetList {
    /// By name, ignoring case.
    pub presets: Vec<PresetSummary>,
    /// Files in the folder that could not be read as presets, each with the reason. They are left
    /// untouched.
    pub unreadable: Vec<String>,
}

/// What [`save_to_dir`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    /// A preset of that name exists and `overwrite` was false; nothing was written.
    Exists,
}

/// Saves a preset in the presets folder (created if needed).
pub fn save_to_dir(dir: &Path, p: &Preset, overwrite: bool) -> Result<SaveOutcome, Error> {
    let text = to_json(p)?;
    let path = dir.join(file_name_for(&p.name));
    if path.exists() && !overwrite {
        return Ok(SaveOutcome::Exists);
    }
    fs::create_dir_all(dir)?;
    atomic_write(&path, text.as_bytes())?;
    Ok(SaveOutcome::Saved)
}

/// The preset of that name, or `None` if there is none.
pub fn load_from_dir(dir: &Path, name: &str) -> Result<Option<Preset>, Error> {
    let file = file_name_for(name);
    let text = match fs::read_to_string(dir.join(&file)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let p = parse(&text).map_err(|e| Error::Invalid(format!("{file}: {e}")))?;
    if name_key(&p.name) != name_key(name) {
        return invalid(format!("{file} holds a preset named {:?}, not {:?}", p.name, name));
    }
    Ok(Some(p))
}

/// Deletes the preset of that name. Returns whether there was one. Only the preset's own file is
/// removed.
pub fn delete_from_dir(dir: &Path, name: &str) -> Result<bool, Error> {
    match fs::remove_file(dir.join(file_name_for(name))) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Every preset in the folder. A missing folder is an empty list; a file that does not parse is
/// reported in `unreadable` and does not hide the others.
pub fn list_dir(dir: &Path) -> PresetList {
    let mut out = PresetList::default();
    let Ok(rd) = fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        let parsed = fs::metadata(e.path())
            .map_err(Error::from)
            .and_then(|m| if m.len() > MAX_FILE_BYTES { invalid("too large") } else { Ok(()) })
            .and_then(|()| fs::read_to_string(e.path()).map_err(Error::from))
            .and_then(|t| parse(&t));
        match parsed {
            Ok(p) => out.presets.push(summary_of(&p, &name)),
            Err(err) => out.unreadable.push(format!("{name}: {err}")),
        }
    }
    out.presets.sort_by_key(|p| (name_key(&p.name), p.file.clone()));
    out.unreadable.sort();
    out
}

// ---- the diff --------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    /// The card has another value; applying changes it.
    Change,
    /// The card already has this value.
    Same,
    /// This card's settings file does not hold the setting yet (the device adds it when it saves).
    Absent,
    /// The editor does not know a setting with this id.
    Unknown,
    /// The preset's entry cannot be applied to this setting: another kind, a value out of range,
    /// an option that means something else here, or a setting that is read-only.
    Invalid,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiffRow {
    pub id: i64,
    pub group: String,
    pub title: String,
    pub status: RowStatus,
    /// What the card has now, in words (none when the card lacks the setting).
    pub current: Option<String>,
    /// What the preset says, in words.
    pub wanted: String,
    /// Why, for every status but `Change` and `Same`.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PresetDiff {
    pub name: String,
    /// The card's settings in the editor's order, then the preset's entries the editor does not know.
    pub rows: Vec<DiffRow>,
    pub change: usize,
    pub same: usize,
    /// Rows that cannot be applied (`Absent`, `Unknown`, `Invalid`).
    pub blocked: usize,
    /// Editable settings on the card that the preset does not mention; applying leaves them alone.
    pub untouched: usize,
}

/// What applying `preset` to a card with these settings would do, setting by setting.
pub fn diff(preset: &Preset, view: &SettingsView) -> PresetDiff {
    let mut rows = Vec::new();
    for card in &view.settings {
        let Some(want) = preset.settings.iter().find(|p| p.id == card.id) else { continue };
        let cur_label = card.present.then(|| label_of(card, card.value));
        let (status, detail) = if !card.present {
            (RowStatus::Absent, Some("this card's settings file does not hold it yet; the device adds it when it saves".to_string()))
        } else if card.kind != want.kind {
            (RowStatus::Invalid, Some(format!("the preset says {}, this setting is a {}", want.kind, card.kind)))
        } else if !card.editable {
            (RowStatus::Invalid, Some("the editor cannot change this setting".to_string()))
        } else if want.value < 0.0 || want.value.fract() != 0.0 || (want.value as usize) >= card.options.len() {
            (RowStatus::Invalid, Some(format!("the value {} is not one of this setting's {} options", want.value, card.options.len())))
        } else if label_of(card, want.value) != want.label {
            (
                RowStatus::Invalid,
                Some(format!("the preset calls option {} \"{}\"; this editor calls it \"{}\", so the meaning may differ", want.value, want.label, label_of(card, want.value))),
            )
        } else if card.value == want.value {
            (RowStatus::Same, None)
        } else {
            (RowStatus::Change, None)
        };
        rows.push(DiffRow {
            id: card.id,
            group: card.group.clone(),
            title: card.title.clone(),
            status,
            current: cur_label,
            wanted: want.label.clone(),
            detail,
        });
    }
    for want in &preset.settings {
        if !view.settings.iter().any(|c| c.id == want.id) {
            rows.push(DiffRow {
                id: want.id,
                group: want.group.clone(),
                title: want.title.clone(),
                status: RowStatus::Unknown,
                current: None,
                wanted: want.label.clone(),
                detail: Some("this editor does not know a setting with this number".into()),
            });
        }
    }
    let count = |s: RowStatus| rows.iter().filter(|r| r.status == s).count();
    let untouched = view.settings.iter().filter(|s| s.present && s.editable && !preset.settings.iter().any(|p| p.id == s.id)).count();
    PresetDiff {
        name: preset.name.clone(),
        change: count(RowStatus::Change),
        same: count(RowStatus::Same),
        blocked: count(RowStatus::Absent) + count(RowStatus::Unknown) + count(RowStatus::Invalid),
        untouched,
        rows,
    }
}

/// The `(id, value)` pairs [`ops_device::write_settings`] needs for the rows that change. With
/// `only`, just those ids: an id that is not in the diff, or whose row cannot be applied, is an
/// error (a stale confirmation must not write something else); an id that is already the same is
/// skipped.
pub fn changes_of(d: &PresetDiff, preset: &Preset, only: Option<&[i64]>) -> Result<Vec<(i64, f64)>, Error> {
    let value_of = |id: i64| preset.settings.iter().find(|s| s.id == id).map(|s| s.value);
    let mut out = Vec::new();
    match only {
        None => {
            for r in d.rows.iter().filter(|r| r.status == RowStatus::Change) {
                out.push((r.id, value_of(r.id).unwrap_or(0.0)));
            }
        }
        Some(ids) => {
            let mut done = HashSet::new();
            for &id in ids {
                if !done.insert(id) {
                    continue;
                }
                let Some(r) = d.rows.iter().find(|r| r.id == id) else {
                    return invalid(format!("setting {id} is not in the preset"));
                };
                match r.status {
                    RowStatus::Change => out.push((id, value_of(id).unwrap_or(0.0))),
                    RowStatus::Same => {}
                    _ => return invalid(format!("{} cannot be applied: {}", r.title, r.detail.clone().unwrap_or_default())),
                }
            }
        }
    }
    Ok(out)
}

/// What [`apply`] did.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApplyReport {
    /// Settings whose value actually changed on the card.
    pub changed: usize,
    /// The ids written, in the order of the diff.
    pub written: Vec<i64>,
}

/// Applies a preset to the card at `root`: reads its `settings.dat`, runs [`diff`], and writes
/// the rows that change (only the ids in `only`, when given) with `ops_device::write_settings`.
/// The caller holds the app's write gate. Nothing is written when no row changes.
pub fn apply(root: &Path, preset: &Preset, only: Option<&[i64]>) -> Result<ApplyReport, Error> {
    validate(preset)?;
    let Some(view) = ops_device::read_settings(root)? else {
        return invalid("this card has no settings file yet; the Pacemaker creates it the first time it is shut down");
    };
    let d = diff(preset, &view);
    let pairs = changes_of(&d, preset, only)?;
    if pairs.is_empty() {
        return Ok(ApplyReport { changed: 0, written: Vec::new() });
    }
    let changed = ops_device::write_settings(root, &pairs)?;
    Ok(ApplyReport { changed, written: pairs.iter().map(|p| p.0).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops_device::{emit_settings, parse_settings, Item, Value};
    use std::path::PathBuf;

    // a card's settings.dat with the header of a real dump (see the ops_device tests)
    const BASE: &str = "22 serialization::archive 4 0 16 0 0 6 0 1 1 0\n0 0 0 2004 604 0 1 1\n1 2003 604 9 0 3 1 0\n2 2000 604 0 2 4 1 0\n3 604 5 0 2 1 0\n4 1400 200 0 0.021000000000000001 5 1 0\n5 1100 4 0 4983";
    const BOOL: u8 = 1;
    const FLOAT: u8 = 2;
    const LIST: u8 = 3;

    fn card_text(items: &[(u8, i64, Value)]) -> String {
        let mut a = parse_settings(BASE).unwrap();
        a.items = items.iter().map(|(cls, id, value)| Item { cls: *cls, id: *id, parent: 0, mru: 0, value: *value }).collect();
        a.mru_counter = 100;
        emit_settings(&a)
    }

    /// Standby (list 0..3), Backlight intensity (list 0..3), Buttons blink + Nightlight (bool),
    /// FX North (13 options), and a read-only level. Backlight timeout (2002) is not in the file.
    fn card_a() -> String {
        card_text(&[
            (BOOL, 2004, Value::Bool(false)),
            (BOOL, 2003, Value::Bool(true)),
            (LIST, 2000, Value::Index(1)),
            (LIST, 2001, Value::Index(2)),
            (LIST, 1900, Value::Index(5)),
            (FLOAT, 1400, Value::Double(0.5)),
        ])
    }

    fn card_b() -> String {
        card_text(&[
            (BOOL, 2004, Value::Bool(true)),
            (BOOL, 2003, Value::Bool(false)),
            (LIST, 2000, Value::Index(2)),
            (LIST, 2001, Value::Index(1)),
            (LIST, 1900, Value::Index(2)),
            (FLOAT, 1400, Value::Double(0.25)),
        ])
    }

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pm-preset-test-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn card(tag: &str, text: &str) -> PathBuf {
        let root = scratch(tag);
        fs::create_dir_all(root.join(".Pacemaker")).unwrap();
        fs::write(root.join(".Pacemaker").join("settings.dat"), text).unwrap();
        root
    }

    fn view_of(root: &Path) -> SettingsView {
        ops_device::read_settings(root).unwrap().unwrap()
    }

    fn view_of_text(text: &str) -> SettingsView {
        ops_device::describe_settings(&parse_settings(text).unwrap())
    }

    fn values(v: &SettingsView) -> Vec<(i64, f64)> {
        v.settings.iter().filter(|s| s.present).map(|s| (s.id, s.value)).collect()
    }

    fn preset_a() -> Preset {
        from_settings("Club night", &view_of_text(&card_a()), Some("180096069".into()), 1_781_000_000).unwrap()
    }

    #[test]
    fn a_preset_holds_the_editable_settings_only() {
        let p = preset_a();
        let ids: Vec<i64> = p.settings.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), 5, "the read-only level (1400) is not saved: {ids:?}");
        assert!(!ids.contains(&1400));
        assert!(!ids.contains(&2002), "a setting the card file lacks is not saved");
        let backlight = p.settings.iter().find(|s| s.id == 2001).unwrap();
        assert_eq!((backlight.kind.as_str(), backlight.value, backlight.label.as_str()), ("list", 2.0, "Bright"));
        let nightlight = p.settings.iter().find(|s| s.id == 2003).unwrap();
        assert_eq!((nightlight.kind.as_str(), nightlight.value, nightlight.label.as_str()), ("bool", 1.0, "On"));
        assert_eq!(p.saved_utc, "20260609T101320Z");
        assert_eq!(p.firmware.as_deref(), Some("180096069"));
    }

    #[test]
    fn json_round_trip_is_exact_and_stable() {
        let p = preset_a();
        let text = to_json(&p).unwrap();
        assert!(text.ends_with("}\n"));
        let back = parse(&text).unwrap();
        assert_eq!(back, p);
        assert_eq!(to_json(&back).unwrap(), text, "write, read, write gives the same bytes");
        // a byte-order mark from a Windows editor does not matter
        assert_eq!(parse(&format!("\u{feff}{text}")).unwrap(), p);
    }

    #[test]
    fn round_trip_across_cards_makes_the_editable_values_equal() {
        let a = card("rt-a", &card_a());
        let b = card("rt-b", &card_b());
        let preset = from_settings("x", &view_of(&a), None, 0).unwrap();
        // via a file, as an export / import would
        let file = a.join("export.json");
        write_file(&file, &preset).unwrap();
        let imported = read_file(&file).unwrap();
        assert_eq!(imported, preset);

        let rep = apply(&b, &imported, None).unwrap();
        assert_eq!(rep.changed, 5);
        let editable = |v: &SettingsView| -> Vec<(i64, f64)> { v.settings.iter().filter(|s| s.present && s.editable).map(|s| (s.id, s.value)).collect() };
        assert_eq!(editable(&view_of(&b)), editable(&view_of(&a)));
        // the read-only level of card B is untouched, and the file still parses and re-emits byte-identical
        assert_eq!(view_of(&b).settings.iter().find(|s| s.id == 1400).unwrap().value, 0.25);
        let text = fs::read_to_string(b.join(".Pacemaker").join("settings.dat")).unwrap();
        assert_eq!(emit_settings(&parse_settings(&text).unwrap()), text);
        // applying again changes nothing and writes nothing
        let before = fs::read(b.join(".Pacemaker").join("settings.dat")).unwrap();
        assert_eq!(apply(&b, &imported, None).unwrap(), ApplyReport { changed: 0, written: Vec::new() });
        assert_eq!(fs::read(b.join(".Pacemaker").join("settings.dat")).unwrap(), before);
        assert!(fs::read_dir(b.join(".Pacemaker")).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));
        for d in [a, b] {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn applying_a_preset_leaves_unchanged_items_byte_for_byte() {
        let b_text = card_b();
        let b = card("bytes", &b_text);
        let preset = preset_a();
        apply(&b, &preset, Some(&[2001])).unwrap();
        let after = fs::read_to_string(b.join(".Pacemaker").join("settings.dat")).unwrap();
        let (x, y) = (parse_settings(&b_text).unwrap(), parse_settings(&after).unwrap());
        for (i, (p, q)) in x.items.iter().zip(&y.items).enumerate() {
            if p.id == 2001 {
                assert_eq!(q.value, Value::Index(2));
                assert_eq!(q.mru, 101, "the changed item gets the next recently-used stamp");
            } else {
                assert_eq!(p, q, "item {i} ({}) must not change", p.id);
            }
        }
        assert_eq!(y.mru_counter, 101);
        let _ = fs::remove_dir_all(b);
    }

    #[test]
    fn diff_names_every_row_and_the_reason() {
        let mut preset = preset_a();
        let v = view_of_text(&card_b());
        // (1) plain differences / same
        let d = diff(&preset, &v);
        assert_eq!((d.change, d.same, d.blocked), (5, 0, 0));
        assert_eq!(d.untouched, 0);
        let standby = d.rows.iter().find(|r| r.id == 2000).unwrap();
        assert_eq!((standby.status, standby.current.as_deref(), standby.wanted.as_str()), (RowStatus::Change, Some("5 min"), "1 min"));

        // (2) every way a row can be blocked
        preset.settings.push(PresetSetting { id: 2002, group: "Power".into(), title: "Backlight timeout".into(), kind: "list".into(), value: 1.0, label: "10 sec".into() });
        preset.settings.push(PresetSetting { id: 9999, group: "Other".into(), title: "Mystery".into(), kind: "bool".into(), value: 1.0, label: "On".into() });
        preset.settings.push(PresetSetting { id: 801, group: "Playback".into(), title: "Shuffle".into(), kind: "list".into(), value: 1.0, label: "On".into() });
        let d = diff(&preset, &v);
        let st = |id| d.rows.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!(st(2002), RowStatus::Absent, "not in this card's file");
        assert_eq!(st(9999), RowStatus::Unknown);
        assert_eq!(st(801), RowStatus::Absent, "Shuffle is not in card B's file");
        assert_eq!(d.blocked, 3);
        assert!(d.rows.iter().find(|r| r.id == 9999).unwrap().detail.as_deref().unwrap().contains("does not know"));

        // value out of range, wrong kind, option that means something else, read-only
        let mut p2 = preset_a();
        p2.settings.iter_mut().find(|s| s.id == 2000).unwrap().value = 3.0; // Standby has 3 options
        p2.settings.iter_mut().find(|s| s.id == 2003).unwrap().kind = "list".into();
        p2.settings.iter_mut().find(|s| s.id == 2001).unwrap().label = "Blinding".into();
        p2.settings.push(PresetSetting { id: 1400, group: "Levels".into(), title: "Phones EQ treble".into(), kind: "float".into(), value: 1.0, label: "1".into() });
        let d = diff(&p2, &v);
        let st = |id| d.rows.iter().find(|r| r.id == id).unwrap().status;
        assert_eq!((st(2000), st(2003), st(2001), st(1400)), (RowStatus::Invalid, RowStatus::Invalid, RowStatus::Invalid, RowStatus::Invalid));
        assert!(d.rows.iter().find(|r| r.id == 2001).unwrap().detail.as_deref().unwrap().contains("Blinding"));
    }

    #[test]
    fn untouched_counts_what_the_preset_does_not_mention() {
        let mut preset = preset_a();
        preset.settings.retain(|s| s.id == 2001 || s.id == 2004);
        let d = diff(&preset, &view_of_text(&card_b()));
        assert_eq!(d.rows.len(), 2);
        assert_eq!(d.untouched, 3, "2000, 2003 and 1900 stay as they are (the read-only level is not counted)");
    }

    #[test]
    fn apply_uses_the_same_rows_the_preview_shows() {
        let b = card("same", &card_b());
        let preset = preset_a();
        let d = diff(&preset, &view_of(&b));
        let expect: Vec<i64> = d.rows.iter().filter(|r| r.status == RowStatus::Change).map(|r| r.id).collect();
        let rep = apply(&b, &preset, None).unwrap();
        assert_eq!(rep.written, expect);
        assert_eq!(rep.changed, expect.len());
        let _ = fs::remove_dir_all(b);
    }

    #[test]
    fn apply_with_a_subset_writes_only_those_and_refuses_a_row_that_cannot_apply() {
        let b = card("subset", &card_b());
        let mut preset = preset_a();
        let rep = apply(&b, &preset, Some(&[2001, 2004])).unwrap();
        assert_eq!(rep.changed, 2);
        let v = view_of(&b);
        let val = |id| v.settings.iter().find(|s| s.id == id).unwrap().value;
        assert_eq!((val(2001), val(2004)), (2.0, 0.0));
        assert_eq!((val(2000), val(2003), val(1900)), (2.0, 0.0, 2.0), "the others keep card B's values");

        // an id the preset does not hold, or a blocked row, writes nothing at all
        preset.settings.push(PresetSetting { id: 2002, group: "Power".into(), title: "Backlight timeout".into(), kind: "list".into(), value: 1.0, label: "10 sec".into() });
        let before = fs::read(b.join(".Pacemaker").join("settings.dat")).unwrap();
        assert!(apply(&b, &preset, Some(&[2000, 2002])).is_err());
        assert!(apply(&b, &preset, Some(&[2000, 12345])).is_err());
        assert_eq!(fs::read(b.join(".Pacemaker").join("settings.dat")).unwrap(), before);
        // asking again for one that is already the same is not an error and not a write
        assert_eq!(apply(&b, &preset, Some(&[2001])).unwrap().changed, 0);
        let _ = fs::remove_dir_all(b);
    }

    #[test]
    fn apply_to_a_card_without_settings_says_so() {
        let root = scratch("none");
        let err = apply(&root, &preset_a(), None).unwrap_err().to_string();
        assert!(err.contains("no settings file"), "{err}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validation_rejects_bad_files() {
        let good = to_json(&preset_a()).unwrap();
        assert!(parse(&good).is_ok());
        let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
        let mutate = |f: &dyn Fn(&mut serde_json::Value)| -> Result<Preset, Error> {
            let mut c = v.clone();
            f(&mut c);
            parse(&c.to_string())
        };
        assert!(mutate(&|c| c["format"] = "something-else".into()).is_err());
        assert!(mutate(&|c| c["version"] = 2.into()).is_err());
        assert!(mutate(&|c| c["name"] = "  ".into()).is_err());
        assert!(mutate(&|c| c["name"] = "x".repeat(81).into()).is_err());
        assert!(mutate(&|c| c["name"] = "bad\u{7}name".into()).is_err());
        assert_eq!(mutate(&|c| c["name"] = " two   words\n".into()).unwrap().name, "two words", "a file's name is cleaned on reading");
        assert!(mutate(&|c| c["settings"] = serde_json::json!([])).is_err());
        assert!(mutate(&|c| c["settings"][0]["kind"] = "float".into()).is_err());
        assert!(mutate(&|c| c["settings"][0]["value"] = 0.5.into()).is_err());
        assert!(mutate(&|c| c["settings"][0]["value"] = (-1).into()).is_err());
        assert!(
            mutate(&|c| {
                c["settings"][0]["kind"] = "bool".into();
                c["settings"][0]["value"] = 2.into();
            })
            .is_err(),
            "a bool is 0 or 1"
        );
        assert!(mutate(&|c| c["settings"][1]["id"] = c["settings"][0]["id"].clone()).is_err(), "duplicate id");
        assert!(mutate(&|c| c["settings"][0]["title"] = "x".repeat(121).into()).is_err());
        // unknown extra fields are ignored; missing optional fields are fine
        assert!(mutate(&|c| c["extra"] = 1.into()).is_ok());
        v.as_object_mut().unwrap().remove("saved_utc");
        v.as_object_mut().unwrap().remove("firmware");
        assert!(parse(&v.to_string()).is_ok());
        assert!(parse("not json").is_err());
        assert!(parse(&"x".repeat(MAX_FILE_BYTES as usize + 1)).is_err());
    }

    #[test]
    fn read_file_refuses_big_and_non_text_files() {
        let dir = scratch("file");
        let big = dir.join("big.json");
        fs::write(&big, vec![b' '; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert!(read_file(&big).is_err());
        let bin = dir.join("bin.json");
        fs::write(&bin, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert!(read_file(&bin).is_err());
        assert!(read_file(&dir.join("missing.json")).is_err());
        assert!(read_file(&dir).is_err(), "a folder is not a preset");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn file_names_are_plain_and_one_per_name() {
        let n = file_name_for("Club night");
        assert!(n.starts_with("club-night-") && n.ends_with(".json"), "{n}");
        assert_eq!(file_name_for("  CLUB NIGHT "), n, "case and outer spaces do not make a second preset");
        assert_ne!(file_name_for("a/b"), file_name_for("a-b"), "different names never share a file");
        for hostile in ["../../x", "con", "nul", "C:\\x", "a\u{0}b", "名前", "...", "", "x".repeat(80).as_str()] {
            let f = file_name_for(hostile);
            assert!(f.ends_with(".json") && f.len() < 70, "{f}");
            assert!(f.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'), "{f}");
            assert!(!f.contains(".."), "{f}");
        }
    }

    #[test]
    fn the_presets_folder_saves_lists_loads_and_deletes() {
        let dir = scratch("dir").join("presets");
        assert!(list_dir(&dir).presets.is_empty(), "a missing folder is an empty list");
        let a = preset_a();
        assert_eq!(save_to_dir(&dir, &a, false).unwrap(), SaveOutcome::Saved);
        // same name in other case: refused unless overwrite
        let mut again = a.clone();
        again.name = "CLUB NIGHT".into();
        again.settings.truncate(2);
        assert_eq!(save_to_dir(&dir, &again, false).unwrap(), SaveOutcome::Exists);
        assert_eq!(load_from_dir(&dir, "club night").unwrap().unwrap(), a, "unchanged by the refused save");
        assert_eq!(save_to_dir(&dir, &again, true).unwrap(), SaveOutcome::Saved);
        assert_eq!(load_from_dir(&dir, "Club Night").unwrap().unwrap().settings.len(), 2);

        let mut other = a.clone();
        other.name = "Another".into();
        save_to_dir(&dir, &other, false).unwrap();
        // a damaged file and a stray temp file do not hide the good ones
        fs::write(dir.join("broken.json"), "{ nope").unwrap();
        fs::write(dir.join("half.json.tmp"), "x").unwrap();
        let l = list_dir(&dir);
        assert_eq!(l.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Another", "CLUB NIGHT"]);
        assert_eq!(l.presets[1].settings, 2);
        assert_eq!(l.unreadable.len(), 1);
        assert!(l.unreadable[0].starts_with("broken.json"));

        assert!(load_from_dir(&dir, "missing").unwrap().is_none());
        assert!(delete_from_dir(&dir, "club night").unwrap());
        assert!(!delete_from_dir(&dir, "club night").unwrap());
        assert!(load_from_dir(&dir, "club night").unwrap().is_none());
        assert!(dir.join("broken.json").exists(), "only the preset's own file is removed");
        assert!(fs::read_dir(&dir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".json.tmp") || e.file_name() == "half.json.tmp"));
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn a_file_whose_name_is_not_its_own_is_not_loaded_as_that_preset() {
        let dir = scratch("renamed");
        let a = preset_a();
        // someone copied another preset's content over the file of the name "Mine"
        fs::write(dir.join(file_name_for("Mine")), to_json(&a).unwrap()).unwrap();
        assert!(load_from_dir(&dir, "Mine").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_from_a_card_with_nothing_editable_fails() {
        let only_level = view_of_text(&card_text(&[(FLOAT, 1400, Value::Double(0.5))]));
        assert!(from_settings("x", &only_level, None, 0).is_err());
        assert!(from_settings(" ", &view_of_text(&card_a()), None, 0).is_err());
    }

    #[test]
    fn values_listing_helper_matches_cards() {
        // guards the fixtures themselves: card A and B differ in every editable setting
        let (a, b) = (values(&view_of_text(&card_a())), values(&view_of_text(&card_b())));
        for ((ia, va), (ib, vb)) in a.iter().zip(&b) {
            assert_eq!(ia, ib);
            if *ia != 1400 {
                assert_ne!(va, vb, "setting {ia}");
            }
        }
    }
}
