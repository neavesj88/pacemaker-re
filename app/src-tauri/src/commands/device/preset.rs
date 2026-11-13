//! Settings presets: save the card's `settings.dat` editor values as a named preset, look at a
//! preset against another card (a named diff) and apply it. The logic is in
//! `pm_formats::ops_preset`; this file is the command layer (app config folder, write gate).
//!
//! Presets are files of this app, one JSON file per name in `<app config>/settings-presets/`;
//! the device cannot hold them and the UI says so. Applying is a quick write like the settings
//! editor's own save: refused during an eject / backup / firmware install, preceded by the
//! session's first-write snapshot (which includes `settings.dat`). The preview and the action
//! run the same `ops_preset::diff` on the card as it is when each runs.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pm_formats::ops_device;
use pm_formats::ops_preset::{self, ApplyReport, Preset, PresetDiff, PresetList, PresetSummary, SaveOutcome};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::AppResult;

const PRESETS_DIR: &str = "settings-presets";

fn presets_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?.join(PRESETS_DIR))
}

/// What a save or import did. `exists` means a preset of that name is already stored and
/// nothing was written (ask, then call again with `overwrite`); `file` is then the file in the
/// presets folder that would be replaced, which matters when that file cannot be read as a
/// preset and so is not in the list.
#[derive(Debug, Serialize, PartialEq)]
pub struct SaveResult {
    pub saved: bool,
    pub exists: bool,
    pub file: Option<String>,
    pub summary: Option<PresetSummary>,
}

fn summary(p: &Preset) -> PresetSummary {
    PresetSummary {
        name: p.name.clone(),
        file: ops_preset::file_name_for(&p.name),
        settings: p.settings.len(),
        saved_utc: p.saved_utc.clone(),
        firmware: p.firmware.clone(),
    }
}

fn store(dir: &Path, p: &Preset, overwrite: bool) -> AppResult<SaveResult> {
    Ok(match ops_preset::save_to_dir(dir, p, overwrite)? {
        SaveOutcome::Saved => SaveResult { saved: true, exists: false, file: None, summary: Some(summary(p)) },
        SaveOutcome::Exists => SaveResult { saved: false, exists: true, file: Some(ops_preset::file_name_for(&p.name)), summary: None },
    })
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Reads the card's settings and stores them as a preset called `name`.
pub fn save_current(dir: &Path, root: &Path, name: &str, overwrite: bool) -> AppResult<SaveResult> {
    let Some(view) = ops_device::read_settings(root)? else {
        return Err("this card has no settings file yet; the Pacemaker creates it the first time it is shut down".into());
    };
    let firmware = ops_device::firmware_info(root).sw_version;
    let preset = ops_preset::from_settings(name, &view, firmware, now_secs())?;
    store(dir, &preset, overwrite)
}

fn get(dir: &Path, name: &str) -> AppResult<Preset> {
    ops_preset::load_from_dir(dir, name)?.ok_or_else(|| format!("there is no preset called {name:?}").into())
}

/// The named diff of a stored preset against the card at `root`.
pub fn diff_named(dir: &Path, root: &Path, name: &str) -> AppResult<PresetDiff> {
    let preset = get(dir, name)?;
    let Some(view) = ops_device::read_settings(root)? else {
        return Err("this card has no settings file yet; the Pacemaker creates it the first time it is shut down".into());
    };
    Ok(ops_preset::diff(&preset, &view))
}

/// Applies a stored preset to the card at `root` (only the ticked ids). Caller holds the write gate.
pub fn apply_named(dir: &Path, root: &Path, name: &str, ids: &[i64]) -> AppResult<ApplyReport> {
    let preset = get(dir, name)?;
    Ok(ops_preset::apply(root, &preset, Some(ids))?)
}

/// Imports a preset file into the presets folder, optionally under another name.
pub fn import_file(dir: &Path, path: &Path, name: Option<&str>, overwrite: bool) -> AppResult<SaveResult> {
    let mut preset = ops_preset::read_file(path)?;
    if let Some(n) = name {
        preset.name = ops_preset::clean_name(n)?;
    }
    store(dir, &preset, overwrite)
}

// ---- commands ---------------------------------------------------------------------------------

/// The stored presets (and any file in the folder that could not be read). Read-only.
#[tauri::command]
pub async fn device_presets_list(app: AppHandle) -> AppResult<PresetList> {
    let dir = presets_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || ops_preset::list_dir(&dir)).await?)
}

/// One stored preset with every setting in it.
#[tauri::command]
pub async fn device_preset_get(app: AppHandle, name: String) -> AppResult<Preset> {
    let dir = presets_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || get(&dir, &name)).await?
}

/// Saves the card's current editable settings as a preset. Writes only to the app's config
/// folder, never to the card.
#[tauri::command]
pub async fn device_preset_save_current(app: AppHandle, root: String, name: String, overwrite: bool) -> AppResult<SaveResult> {
    let dir = presets_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || save_current(&dir, Path::new(&root), &name, overwrite)).await?
}

/// Deletes the stored preset of that name (its own file only). Returns whether there was one.
#[tauri::command]
pub async fn device_preset_delete(app: AppHandle, name: String) -> AppResult<bool> {
    let dir = presets_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<bool> { Ok(ops_preset::delete_from_dir(&dir, &name)?) }).await?
}

/// Writes a stored preset to a `.json` file the user chose (a plain copy, for another PC).
#[tauri::command]
pub async fn device_preset_export(app: AppHandle, name: String, dest: String) -> AppResult<()> {
    let dir = presets_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        if dest.trim().is_empty() {
            return Err("choose where to save the preset".into());
        }
        let preset = get(&dir, &name)?;
        Ok(ops_preset::write_file(Path::new(&dest), &preset)?)
    })
    .await?
}

/// Reads and checks a preset file without storing it, so the dialog can show what it holds.
#[tauri::command]
pub async fn device_preset_read_file(path: String) -> AppResult<Preset> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Preset> { Ok(ops_preset::read_file(Path::new(&path))?) }).await?
}

/// Stores a preset file in the presets folder, under `name` if given, else its own name.
#[tauri::command]
pub async fn device_preset_import(app: AppHandle, path: String, name: Option<String>, overwrite: bool) -> AppResult<SaveResult> {
    let dir = presets_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || import_file(&dir, Path::new(&path), name.as_deref(), overwrite)).await?
}

/// What applying the preset would do to this card, setting by setting. Read-only.
#[tauri::command]
pub async fn device_preset_diff(app: AppHandle, root: String, name: String) -> AppResult<PresetDiff> {
    let dir = presets_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || diff_named(&dir, Path::new(&root), &name)).await?
}

/// Applies the preset's ticked settings to the card's `settings.dat`. The diff is computed again
/// on the card as it is now; an id that no longer differs is skipped, an id that cannot be
/// applied refuses the whole call. Returns what was written.
#[tauri::command]
pub async fn device_preset_apply(app: AppHandle, root: String, name: String, ids: Vec<i64>) -> AppResult<ApplyReport> {
    let dir = presets_dir(&app)?;
    let path = root.clone();
    crate::state::quick_write(app, root, move || apply_named(&dir, Path::new(&path), &name, &ids)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::ops_device::{emit_settings, parse_settings, Item, Value};
    use pm_formats::ops_preset::RowStatus;

    const BASE: &str = "22 serialization::archive 4 0 16 0 0 6 0 1 1 0\n0 0 0 2004 604 0 1 1\n1 2003 604 9 0 3 1 0\n2 2000 604 0 2 4 1 0\n3 604 5 0 2 1 0\n4 1400 200 0 0.021000000000000001 5 1 0\n5 1100 4 0 4983";

    fn scratch(tag: &str) -> PathBuf {
        // a counter, not the clock: tests run in parallel and several use the same tag within one
        // second, which made two of them share (and wipe) one folder
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("pm-app-preset-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A card with Buttons blink (bool) and Standby (list).
    fn card(tag: &str, blink: bool, standby: i64) -> PathBuf {
        let root = scratch(tag);
        let mut a = parse_settings(BASE).unwrap();
        a.items = vec![
            Item { cls: 1, id: 2004, parent: 0, mru: 0, value: Value::Bool(blink) },
            Item { cls: 3, id: 2000, parent: 0, mru: 0, value: Value::Index(standby) },
        ];
        std::fs::create_dir_all(root.join(".Pacemaker")).unwrap();
        std::fs::write(root.join(".Pacemaker").join("settings.dat"), emit_settings(&a)).unwrap();
        root
    }

    #[test]
    fn a_damaged_file_in_the_way_is_named_not_silently_replaced() {
        let dir = scratch("damaged");
        let a = card("a", false, 0);
        let file = ops_preset::file_name_for("Quiet set");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&file), b"{ not a preset").unwrap();
        assert!(ops_preset::list_dir(&dir).presets.is_empty(), "the damaged file is not a listed preset");
        let r = save_current(&dir, &a, "Quiet set", false).unwrap();
        assert!(!r.saved && r.exists);
        assert_eq!(r.file.as_deref(), Some(file.as_str()));
        assert_eq!(std::fs::read(dir.join(&file)).unwrap(), b"{ not a preset", "untouched until overwrite is asked for");
        assert!(save_current(&dir, &a, "Quiet set", true).unwrap().saved);
    }

    #[test]
    fn save_on_one_card_diff_and_apply_on_another() {
        let dir = scratch("store");
        let (a, b) = (card("a", false, 0), card("b", true, 2));
        let saved = save_current(&dir, &a, "Quiet  set", false).unwrap();
        assert!(saved.saved && !saved.exists);
        assert_eq!(saved.summary.as_ref().unwrap().settings, 2);

        // the same name again asks first
        let again = save_current(&dir, &a, "quiet set", false).unwrap();
        assert!(!again.saved && again.exists && again.summary.is_none());
        assert_eq!(again.file.as_deref(), Some(saved.summary.as_ref().unwrap().file.as_str()), "names the file that would be replaced");

        let d = diff_named(&dir, &b, "QUIET SET").unwrap();
        assert_eq!((d.change, d.same, d.blocked), (2, 0, 0));
        assert!(d.rows.iter().all(|r| r.status == RowStatus::Change));

        // apply only the standby row; the preview's other row stays as it was
        let rep = apply_named(&dir, &b, "Quiet set", &[2000]).unwrap();
        assert_eq!(rep.written, vec![2000]);
        let d = diff_named(&dir, &b, "Quiet set").unwrap();
        assert_eq!((d.change, d.same), (1, 1));
        assert_eq!(d.rows.iter().find(|r| r.status == RowStatus::Change).unwrap().id, 2004);

        // an id the preview never showed refuses, writes nothing
        assert!(apply_named(&dir, &b, "Quiet set", &[2004, 31337]).is_err());
        assert_eq!(diff_named(&dir, &b, "Quiet set").unwrap().change, 1);

        // an unknown preset and a card without settings say so
        assert!(diff_named(&dir, &b, "nothing").unwrap_err().to_string().contains("no preset"));
        let empty = scratch("empty");
        assert!(diff_named(&dir, &empty, "Quiet set").unwrap_err().to_string().contains("no settings file"));
        assert!(save_current(&dir, &empty, "x", false).is_err());
        for d in [a, b, empty, dir] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn import_stores_a_file_under_its_own_or_another_name() {
        let dir = scratch("imp-store");
        let a = card("imp-a", false, 1);
        save_current(&dir, &a, "Original", false).unwrap();
        let file = a.join("copy.json");
        ops_preset::write_file(&file, &get(&dir, "Original").unwrap()).unwrap();

        let r = import_file(&dir, &file, None, false).unwrap();
        assert!(r.exists, "same name as the stored one: asks first");
        let r = import_file(&dir, &file, Some("Copy of original"), false).unwrap();
        assert!(r.saved);
        assert_eq!(ops_preset::list_dir(&dir).presets.len(), 2);
        assert!(import_file(&dir, &file, Some("  "), false).is_err(), "a blank new name is refused");
        std::fs::write(&file, "{}").unwrap();
        assert!(import_file(&dir, &file, None, true).is_err(), "not a preset");
        for d in [a, dir] {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}
