//! Tags, colour labels and energy ratings (app-side). The device has no place for them, so they
//! live in a JSON store in the app config dir, one file per library volume (`tags/store.rs`
//! holds the store and its rules). Nothing here writes to the device, so none of it goes through
//! the write gate and it works on a read-only card.
//!
//! Track identity. Labels are keyed by the track's `global_id`, the random UUID assigned when the
//! track is imported (FINDINGS section 4b), not by `track_id`: the id is only unique inside one
//! music.db, and a card that is rebuilt or another card mounted at the same drive letter would
//! hand the labels to the wrong tracks. A row with no `global_id` falls back to its `location`
//! (the hash path under /pmdb_tracks, derived from file name and size); a second row that would
//! get the same key as an earlier one falls back to its `track_id` so two tracks never share
//! labels. Consequences, by design:
//! - Delete a track and add the same file again: it is a new track (new UUID), so it starts
//!   with no labels. The old labels stay in the file, unattached, and are counted as orphans.
//! - The same card read on another computer shares the labels only if the file is copied too.
//! - Another card with other tracks sees none of this card's labels.
//!
//! The store is generic over `Kind` (track, case, mix); only tracks have a resolver here.

mod store;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pm_formats::library::Library;
use serde::Serialize;
use tauri::Manager;

use crate::error::AppResult;
use store::{Kind, Loaded, Op, Store};

pub fn setup(_app: &mut tauri::App) {}

/// One read-modify-write of the store file at a time.
static STORE_LOCK: Mutex<()> = Mutex::new(());

/// The normalised volume name the store file is keyed by (`E:` and `E:\` are one library).
fn volume(root: &str) -> String {
    crate::state::volume_key(Path::new(root))
}

fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

/// The labels of one track as the UI gets them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackLabels {
    pub track_id: i64,
    pub tags: Vec<String>,
    pub colour: Option<String>,
    pub energy: Option<u8>,
}

/// The labels of one thing in the store that matches no track on the card. Its track is gone, so
/// the labels are all there is to name it by.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrphanLabels {
    pub tags: Vec<String>,
    pub colour: Option<String>,
    pub energy: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct TagsView {
    /// Only the tracks that have any label.
    pub tracks: Vec<TrackLabels>,
    /// Labelled things in the store that match no track on this card (see the module note); the
    /// same list "forget" removes, so what the UI shows is what it deletes.
    pub orphans: usize,
    pub orphan_labels: Vec<OrphanLabels>,
    /// Set when the store file could not be read (it then starts empty).
    pub notice: Option<String>,
}

// ---- track identity ----------------------------------------------------------------------

/// `(track_id, store key)` for each track row `(track_id, global_id, location)`.
pub fn build_keys(rows: &[(i64, String, String)]) -> Vec<(i64, String)> {
    let mut used: HashSet<String> = HashSet::new();
    rows.iter()
        .map(|(id, global_id, location)| {
            let global_id = global_id.trim();
            let location = location.trim();
            let mut key = if !global_id.is_empty() {
                format!("g:{global_id}")
            } else if !location.is_empty() {
                format!("l:{location}")
            } else {
                format!("t:{id}")
            };
            if !used.insert(key.clone()) {
                key = format!("t:{id}");
                used.insert(key.clone());
            }
            (*id, key)
        })
        .collect()
}

fn read_keys(root: &str) -> AppResult<Vec<(i64, String)>> {
    let lib = Library::open_read_only(&PathBuf::from(root))?;
    Ok(build_keys(&lib.track_identities()?))
}

// ---- pure cores (the commands are thin wrappers) ------------------------------------------

fn labels_row(track_id: i64, l: Option<&store::Labels>) -> TrackLabels {
    let l = l.cloned().unwrap_or_default();
    TrackLabels { track_id, tags: l.tags, colour: l.colour, energy: l.energy }
}

fn live_keys(idents: &[(i64, String)]) -> HashSet<&str> {
    idents.iter().map(|(_, k)| k.as_str()).collect()
}

fn view_of(store: &Store, idents: &[(i64, String)], notice: Option<String>) -> TagsView {
    let tracks = idents.iter().filter_map(|(id, key)| store.get(Kind::Track, key).map(|l| labels_row(*id, Some(l)))).collect();
    let orphan_labels: Vec<OrphanLabels> = store
        .unattached(Kind::Track, &live_keys(idents))
        .into_iter()
        .map(|(_, l)| OrphanLabels { tags: l.tags.clone(), colour: l.colour.clone(), energy: l.energy })
        .collect();
    TagsView { tracks, orphans: orphan_labels.len(), orphan_labels, notice }
}

/// Resolves `ids` to store keys; an id that is not a track on the card is an error.
fn keys_for(idents: &[(i64, String)], ids: &[i64]) -> Result<Vec<String>, String> {
    let by_id: HashMap<i64, &str> = idents.iter().map(|(i, k)| (*i, k.as_str())).collect();
    ids.iter()
        .map(|id| by_id.get(id).map(|k| (*k).to_string()).ok_or_else(|| format!("Track {id} is not in the library.")))
        .collect()
}

/// Loads the store, runs `f` on it and saves it when `f` changed it. The caller holds no lock.
fn mutate<T>(dir: &Path, volume_key: &str, f: impl FnOnce(&mut Store) -> Result<T, String>) -> Result<T, String> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Loaded { mut store, corrupt, read_only, .. } = store::load(dir, volume_key);
    // a file that could not be read, or a newer build wrote, is never saved over
    if let Some(why) = read_only {
        return Err(why);
    }
    let before = store.clone();
    let out = f(&mut store)?;
    if store != before {
        store::save(dir, volume_key, &store, corrupt.as_deref())?;
    }
    Ok(out)
}

fn load_view(dir: &Path, volume_key: &str, idents: &[(i64, String)]) -> TagsView {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let loaded = store::load(dir, volume_key);
    view_of(&loaded.store, idents, loaded.notice)
}

fn forget_core(dir: &Path, volume_key: &str, idents: &[(i64, String)], expected: usize) -> Result<usize, String> {
    mutate(dir, volume_key, |store| {
        let live = live_keys(idents);
        let now = store.unattached(Kind::Track, &live).len();
        if now != expected {
            return Err(format!("{now} labelled tracks are no longer on the card, not the {expected} you were shown. Nothing was removed; reopen the tag manager."));
        }
        Ok(store.forget_unattached(Kind::Track, &live))
    })
}

fn apply_core(dir: &Path, volume_key: &str, idents: &[(i64, String)], ids: &[i64], ops: &[Op]) -> Result<Vec<TrackLabels>, String> {
    let keys = keys_for(idents, ids)?;
    mutate(dir, volume_key, |store| {
        store.apply(Kind::Track, &keys, ops)?;
        Ok(ids.iter().zip(&keys).map(|(id, k)| labels_row(*id, store.get(Kind::Track, k))).collect())
    })
}

// ---- commands -----------------------------------------------------------------------------

/// Every track's labels for the open library (tracks without any are left out).
#[tauri::command]
pub async fn tags_load(app: tauri::AppHandle, root: String) -> AppResult<TagsView> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<TagsView> {
        let idents = read_keys(&root)?;
        Ok(load_view(&dir, &volume(&root), &idents))
    })
    .await?
}

/// Applies the ops (add / remove tags, set / clear colour, set / clear energy) to every listed
/// track, all or nothing; returns the labels of each listed track afterwards.
#[tauri::command]
pub async fn tags_apply(app: tauri::AppHandle, root: String, track_ids: Vec<i64>, ops: Vec<Op>) -> AppResult<Vec<TrackLabels>> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<TrackLabels>> {
        let idents = read_keys(&root)?;
        Ok(apply_core(&dir, &volume(&root), &idents, &track_ids, &ops)?)
    })
    .await?
}

/// Renames a tag everywhere in this library; renaming onto an existing tag merges the two.
/// Returns the fresh view.
#[tauri::command]
pub async fn tags_rename_tag(app: tauri::AppHandle, root: String, from: String, to: String) -> AppResult<TagsView> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<TagsView> {
        let idents = read_keys(&root)?;
        let vk = volume(&root);
        mutate(&dir, &vk, |s| s.rename_tag(Kind::Track, &from, &to).map(|_| ()))?;
        Ok(load_view(&dir, &vk, &idents))
    })
    .await?
}

/// Removes a tag from every track in this library. Returns the fresh view.
#[tauri::command]
pub async fn tags_delete_tag(app: tauri::AppHandle, root: String, tag: String) -> AppResult<TagsView> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<TagsView> {
        let idents = read_keys(&root)?;
        let vk = volume(&root);
        mutate(&dir, &vk, |s| s.delete_tag(Kind::Track, &tag).map(|_| ()))?;
        Ok(load_view(&dir, &vk, &idents))
    })
    .await?
}

/// Forgets the labels of tracks that are no longer on the card (the ones `orphan_labels` lists).
/// `expected` is how many the user was shown; if the card changed since, nothing is removed and
/// the manager has to be reopened to see the new list. Returns the fresh view.
#[tauri::command]
pub async fn tags_forget_unattached(app: tauri::AppHandle, root: String, expected: usize) -> AppResult<TagsView> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<TagsView> {
        let idents = read_keys(&root)?;
        let vk = volume(&root);
        forget_core(&dir, &vk, &idents, expected)?;
        Ok(load_view(&dir, &vk, &idents))
    })
    .await?
}

/// This area's command paths, one per line, written `$crate::commands::tags::name,`.
#[macro_export]
macro_rules! tags_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::tags::tags_load,
                $crate::commands::tags::tags_apply,
                $crate::commands::tags::tags_rename_tag,
                $crate::commands::tags::tags_delete_tag,
                $crate::commands::tags::tags_forget_unattached,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(v: &[(i64, &str, &str)]) -> Vec<(i64, String, String)> {
        v.iter().map(|(i, g, l)| (*i, g.to_string(), l.to_string())).collect()
    }
    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-tagcmd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
    fn add(t: &[&str]) -> Op {
        Op::AddTags { tags: t.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn keys_prefer_the_uuid_then_the_location_then_the_id() {
        let k = build_keys(&rows(&[(1, "aaaa", "/pmdb_tracks/1/1/x.mp3"), (2, "", "/pmdb_tracks/2/2/y.mp3"), (3, " ", "")]));
        assert_eq!(k, [(1, "g:aaaa".to_string()), (2, "l:/pmdb_tracks/2/2/y.mp3".to_string()), (3, "t:3".to_string())]);
    }

    #[test]
    fn a_repeated_key_never_shares_labels() {
        let k = build_keys(&rows(&[(1, "same", ""), (2, "same", ""), (3, "", "/p/q.mp3"), (4, "", "/p/q.mp3")]));
        let keys: HashSet<&String> = k.iter().map(|(_, k)| k).collect();
        assert_eq!(keys.len(), 4);
        assert_eq!(k[0].1, "g:same", "the first row keeps the real key");
        assert_eq!(k[1].1, "t:2");
        assert_eq!(k[3].1, "t:4");
    }

    #[test]
    fn apply_load_and_orphans() {
        let dir = tmp_dir("core");
        let idents = build_keys(&rows(&[(1, "u1", ""), (2, "u2", ""), (3, "u3", "")]));
        let ops = [add(&["warm up", "Vocal"]), Op::SetColour { colour: Some("red".into()) }, Op::SetEnergy { energy: Some(6) }];
        let after = apply_core(&dir, "e:", &idents, &[1, 3], &ops).unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[0], TrackLabels { track_id: 1, tags: vec!["Vocal".into(), "warm up".into()], colour: Some("red".into()), energy: Some(6) });

        let v = load_view(&dir, "e:", &idents);
        assert_eq!(v.tracks.iter().map(|t| t.track_id).collect::<Vec<_>>(), [1, 3], "track 2 has no labels");
        assert_eq!((v.orphans, v.notice), (0, None));

        // the card is rebuilt: track 1 comes back under a new uuid and id, so it starts empty
        let rebuilt = build_keys(&rows(&[(7, "new-uuid", ""), (3, "u3", "")]));
        let v = load_view(&dir, "e:", &rebuilt);
        assert_eq!(v.tracks.iter().map(|t| t.track_id).collect::<Vec<_>>(), [3]);
        assert_eq!(v.orphans, 1, "the old labels of track 1 are kept but unattached");

        // an empty result row is returned for a track that has none
        let cleared = apply_core(&dir, "e:", &idents, &[1], &[Op::SetColour { colour: None }, Op::SetEnergy { energy: None }]).unwrap();
        assert_eq!(cleared[0].colour, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_track_or_bad_op_changes_nothing() {
        let dir = tmp_dir("bad");
        let idents = build_keys(&rows(&[(1, "u1", ""), (2, "u2", "")]));
        assert!(apply_core(&dir, "e:", &idents, &[1, 99], &[add(&["x"])]).unwrap_err().contains("99"));
        assert!(apply_core(&dir, "e:", &idents, &[1, 2], &[add(&["x"]), Op::SetEnergy { energy: Some(0) }]).is_err());
        assert!(load_view(&dir, "e:", &idents).tracks.is_empty());
        assert!(!store::store_path(&dir, "e:").exists(), "nothing was written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_and_delete_go_through_the_file() {
        let dir = tmp_dir("manage");
        let idents = build_keys(&rows(&[(1, "u1", ""), (2, "u2", "")]));
        apply_core(&dir, "e:", &idents, &[1], &[add(&["old"])]).unwrap();
        apply_core(&dir, "e:", &idents, &[2], &[add(&["new"]), Op::SetEnergy { energy: Some(2) }]).unwrap();
        mutate(&dir, "e:", |s| s.rename_tag(Kind::Track, "old", "new").map(|_| ())).unwrap();
        let v = load_view(&dir, "e:", &idents);
        assert!(v.tracks.iter().all(|t| t.tags == ["new"]));
        mutate(&dir, "e:", |s| s.delete_tag(Kind::Track, "new").map(|_| ())).unwrap();
        let v = load_view(&dir, "e:", &idents);
        assert_eq!(v.tracks.len(), 1, "track 2 keeps its energy, track 1 has nothing left");
        assert_eq!(v.tracks[0], TrackLabels { track_id: 2, tags: vec![], colour: None, energy: Some(2) });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_store_is_reported_and_then_replaced_with_a_backup() {
        let dir = tmp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let p = store::store_path(&dir, "e:");
        std::fs::write(&p, "garbage").unwrap();
        let idents = build_keys(&rows(&[(1, "u1", "")]));
        let v = load_view(&dir, "e:", &idents);
        assert!(v.notice.is_some() && v.tracks.is_empty());
        apply_core(&dir, "e:", &idents, &[1], &[add(&["x"])]).unwrap();
        assert!(load_view(&dir, "e:", &idents).notice.is_none());
        assert_eq!(std::fs::read_to_string(p.with_extension("json.corrupt")).unwrap(), "garbage");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_never_saved_over() {
        let dir = tmp_dir("ro");
        let idents = build_keys(&rows(&[(1, "u1", "")]));
        let good = apply_core(&dir, "e:", &idents, &[1], &[add(&["keep"])]).unwrap();
        assert_eq!(good[0].tags, ["keep"]);
        let p = store::store_path(&dir, "e:");
        let text = std::fs::read_to_string(&p).unwrap();
        // newer format: shown with a notice, every write refused, file untouched
        let newer = text.replace("\"version\": 1", "\"version\": 2");
        assert_ne!(newer, text);
        std::fs::write(&p, &newer).unwrap();
        let v = load_view(&dir, "e:", &idents);
        assert!(v.notice.is_some());
        assert_eq!(v.tracks[0].tags, ["keep"]);
        assert!(apply_core(&dir, "e:", &idents, &[1], &[add(&["more"])]).is_err());
        assert!(mutate(&dir, "e:", |s| s.delete_tag(Kind::Track, "keep").map(|_| ())).is_err());
        assert!(forget_core(&dir, "e:", &idents, 0).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), newer);
        assert!(!p.with_extension("json.corrupt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_unattached_labels_matches_what_the_view_lists() {
        let dir = tmp_dir("forget");
        let idents = build_keys(&rows(&[(1, "u1", ""), (2, "u2", "")]));
        apply_core(&dir, "e:", &idents, &[1, 2], &[add(&["x"])]).unwrap();
        apply_core(&dir, "e:", &idents, &[2], &[Op::SetEnergy { energy: Some(5) }]).unwrap();
        // track 2 leaves the card
        let now = build_keys(&rows(&[(1, "u1", "")]));
        let v = load_view(&dir, "e:", &now);
        assert_eq!(v.orphans, 1);
        assert_eq!(v.orphan_labels, [OrphanLabels { tags: vec!["x".into()], colour: None, energy: Some(5) }]);
        // the count the user was shown must still be right
        assert!(forget_core(&dir, "e:", &now, 3).unwrap_err().contains("Nothing was removed"));
        assert_eq!(load_view(&dir, "e:", &now).orphans, 1);
        assert_eq!(forget_core(&dir, "e:", &now, 1).unwrap(), 1);
        let v = load_view(&dir, "e:", &now);
        assert_eq!((v.orphans, v.tracks.len()), (0, 1), "track 1 keeps its tag");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ops_parse_the_ui_payload() {
        let ops: Vec<Op> = serde_json::from_str(
            r#"[{"op":"add_tags","tags":["a"]},{"op":"remove_tags","tags":["b"]},{"op":"set_colour","colour":null},{"op":"set_energy","energy":5},{"op":"set_colour"}]"#,
        )
        .unwrap();
        assert_eq!(ops[2], Op::SetColour { colour: None });
        assert_eq!(ops[3], Op::SetEnergy { energy: Some(5) });
        assert_eq!(ops[4], Op::SetColour { colour: None }, "an absent colour clears it");
    }
}
