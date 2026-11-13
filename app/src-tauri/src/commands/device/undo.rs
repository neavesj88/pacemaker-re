//! Undo history for ratings, tag edits and favourite toggles (the "trust pack").
//!
//! One bounded in-memory stack per library, keyed by the volume key. The three commands that make
//! these edits (`metadata_set_rating`, `metadata_save`, `cases_toggle_favourite`) call
//! [`tracked_edit`] / [`tracked_favourite`] around their write, which reads the tracks before and
//! after and pushes an entry. Undoing goes through `Library::edit_tracks` / `toggle_favourite`
//! inside a `quick_write`, like the edits themselves, and only puts back fields that still hold
//! the value the edit gave them (see `pm_formats::ops_undo`). The history is deliberately not
//! kept across restarts: an undo days later, after other tools touched the card, is exactly the
//! case the guard exists for, and a stack that can't be trusted is worse than none.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pm_formats::library::{Error, Library};
use pm_formats::ops_undo::{self, EditKind, UndoEntry, UndoPlan, UndoReport, UndoStack};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::AppResult;
use crate::events;
use crate::state::volume_key;

/// "The undo history changed": recorded, undone or cleared. The payload is small; the frontend
/// re-reads the list with `device_undo_list`.
pub const UNDO_CHANGED: &str = "undo-changed";

#[derive(Default)]
pub struct UndoState(Mutex<HashMap<String, UndoStack>>);

/// Runs `f` with the histories locked. Never held across file or database work.
fn with_stacks<R>(app: &AppHandle, f: impl FnOnce(&mut HashMap<String, UndoStack>) -> R) -> R {
    let state = app.state::<UndoState>();
    let mut guard = state.0.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

#[derive(Serialize, Clone)]
pub struct UndoChanged {
    /// The volume key of the library whose history changed.
    pub key: String,
    /// "recorded", "undone", "cleared" or "limit".
    pub why: &'static str,
    /// For "recorded": what the edit was, for the Undo pill.
    pub summary: Option<String>,
    pub id: u64,
}

fn announce(app: &AppHandle, key: &str, why: &'static str, summary: Option<String>, id: u64) {
    events::emit(app, UNDO_CHANGED, UndoChanged { key: key.to_string(), why, summary, id });
}

/// Pushes the entry an edit made (if it changed anything) onto the library's history.
pub fn record(app: &AppHandle, root: &str, entry: Option<UndoEntry>) {
    let Some(e) = entry else { return };
    let limit = super::trust::settings_of(app).undo_limit;
    let key = volume_key(Path::new(root));
    let summary = e.summary.clone();
    let id = with_stacks(app, |m| m.entry(key.clone()).or_default().push(e, limit));
    if id != 0 {
        announce(app, &key, "recorded", Some(summary), id);
    }
}

/// Runs a rating or tag edit of `ids` on the open `lib` and records it for undo. Used by the
/// commands that make those edits; their result and error are the edit's own.
pub fn tracked_edit<T>(
    app: &AppHandle,
    root: &str,
    lib: &mut Library,
    ids: &[i64],
    op: impl FnOnce(&mut Library) -> Result<T, Error>,
) -> Result<T, Error> {
    let (out, entry) = ops_undo::record_edit(lib, ids, op)?;
    record(app, root, entry);
    Ok(out)
}

/// `Library::toggle_favourite` that records itself for undo.
pub fn tracked_favourite(app: &AppHandle, root: &str, lib: &mut Library, track_id: i64) -> Result<bool, Error> {
    let (now, entry) = ops_undo::record_favourite_toggle(lib, track_id)?;
    record(app, root, entry);
    Ok(now)
}

/// Forgets the history of one library (a restore or an eject makes it meaningless).
pub fn clear(app: &AppHandle, root: &str) {
    let key = volume_key(Path::new(root));
    let had = with_stacks(app, |m| m.remove(&key).is_some_and(|s| !s.is_empty()));
    if had {
        announce(app, &key, "cleared", None, 0);
    }
}

/// Shortens every history to `limit` (the setting was lowered).
pub fn trim_all(app: &AppHandle, limit: usize) {
    let keys: Vec<String> = with_stacks(app, |m| {
        m.iter_mut()
            .map(|(k, s)| {
                s.trim(limit);
                k.clone()
            })
            .collect()
    });
    for k in keys {
        announce(app, &k, "limit", None, 0);
    }
}

#[derive(Serialize)]
pub struct EntryView {
    pub id: u64,
    pub kind: EditKind,
    pub at_ms: i64,
    pub summary: String,
    /// Tracks the edit touched.
    pub tracks: usize,
}

#[derive(Serialize)]
pub struct UndoView {
    /// The library's volume key: what `undo-changed` events are matched on.
    pub key: String,
    pub limit: usize,
    /// Newest first.
    pub entries: Vec<EntryView>,
}

/// The history of the library on `root`, newest first. Read-only.
#[tauri::command]
pub fn device_undo_list(app: AppHandle, root: String) -> UndoView {
    let key = volume_key(Path::new(&root));
    let limit = super::trust::settings_of(&app).undo_limit;
    let entries = with_stacks(&app, |m| {
        m.get(&key)
            .map(|s| {
                s.newest_first()
                    .map(|e| EntryView { id: e.id, kind: e.kind, at_ms: e.at_ms, summary: e.summary.clone(), tracks: e.track_count() })
                    .collect()
            })
            .unwrap_or_default()
    });
    UndoView { key, limit, entries }
}

#[derive(Serialize)]
pub struct UndoPreview {
    /// The newest entry's id when the preview was made; the apply must send it back.
    pub top_entry_id: u64,
    pub count: usize,
    pub plan: UndoPlan,
}

/// The top `count` entries, newest first, checked against the id the caller saw on top.
fn take_top(app: &AppHandle, key: &str, count: usize, top_entry_id: Option<u64>) -> AppResult<Vec<UndoEntry>> {
    let entries: Vec<UndoEntry> =
        with_stacks(app, |m| m.get(key).map(|s| s.top(count).into_iter().cloned().collect()).unwrap_or_default());
    if count == 0 || entries.is_empty() {
        return Err("There is nothing to undo.".into());
    }
    if entries.len() < count {
        return Err(format!("Only {} edit{} can be undone.", entries.len(), if entries.len() == 1 { "" } else { "s" }).into());
    }
    if top_entry_id.is_some_and(|id| id != entries[0].id) {
        return Err("The edit history changed since you looked at it. Review it again.".into());
    }
    Ok(entries)
}

/// What undoing the newest `count` edits would do right now, line by line. Read-only; the same
/// plan `device_undo_apply` executes.
#[tauri::command]
pub async fn device_undo_preview(app: AppHandle, root: String, count: usize) -> AppResult<UndoPreview> {
    let key = volume_key(Path::new(&root));
    let entries = take_top(&app, &key, count, None)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<UndoPreview> {
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        let refs: Vec<&UndoEntry> = entries.iter().collect();
        Ok(UndoPreview { top_entry_id: entries[0].id, count, plan: ops_undo::plan_undo(&lib, &refs)? })
    })
    .await?
}

/// Undoes the newest `count` edits. `top_entry_id` is the id from the preview: if the history
/// moved on, nothing is done. Fields changed since their edit are left alone and reported.
/// The entries leave the history once reversed.
#[tauri::command]
pub async fn device_undo_apply(app: AppHandle, root: String, count: usize, top_entry_id: u64) -> AppResult<UndoReport> {
    let key = volume_key(Path::new(&root));
    let entries = take_top(&app, &key, count, Some(top_entry_id))?;
    let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
    let path = root.clone();
    let report = crate::state::quick_write(app.clone(), root, move || -> AppResult<UndoReport> {
        let mut lib = Library::open(&PathBuf::from(path))?;
        let refs: Vec<&UndoEntry> = entries.iter().collect();
        Ok(ops_undo::apply_undo(&mut lib, &refs)?)
    })
    .await?;
    with_stacks(&app, |m| {
        if let Some(s) = m.get_mut(&key) {
            s.remove(&ids);
        }
    });
    announce(&app, &key, "undone", None, 0);
    Ok(report)
}

/// Empties the history of the library on `root`.
#[tauri::command]
pub fn device_undo_clear(app: AppHandle, root: String) {
    clear(&app, &root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_payload_names_the_volume_and_why() {
        let v = serde_json::to_value(UndoChanged { key: "e:".into(), why: "recorded", summary: Some("Rated x".into()), id: 3 }).unwrap();
        assert_eq!(v["key"], "e:");
        assert_eq!(v["why"], "recorded");
        assert_eq!(v["id"], 3);
    }

    #[test]
    fn the_entry_view_serialises_the_kind_in_snake_case() {
        let v = serde_json::to_value(EntryView { id: 1, kind: EditKind::Favourite, at_ms: 5, summary: "s".into(), tracks: 2 }).unwrap();
        assert_eq!(v["kind"], "favourite");
        assert_eq!(v["tracks"], 2);
    }
}
