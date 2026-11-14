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
//! The store is generic over `Kind` (track, case, mix). Cases and mixes have no UUID in music.db,
//! so a case or mix is keyed by `<id>:<date_created>`: a rename or an edit keeps its labels, and a
//! case deleted and a new one that reuses the same row id (SQLite may hand the id out again) never
//! share them, because the creation time differs. Delete a case and its labels stay in the file,
//! unattached, like those of a deleted track. Energy is a track label: a case or mix cannot be given one.

pub mod energy_source;
mod store;

pub use energy_source::EnergySource;
pub use store::{build_keys, normalise_tag, tag_key, Labels, COLOURS, MAX_ENERGY};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pm_formats::library::Library;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::store::config_dir;
use store::{Kind, Loaded, Op, Store};

pub fn setup(_app: &mut tauri::App) {}

/// One read-modify-write of the store file at a time.
static STORE_LOCK: Mutex<()> = Mutex::new(());

/// The normalised volume name the store file is keyed by (`E:` and `E:\` are one library).
fn volume(root: &str) -> String {
    crate::state::volume_key(Path::new(root))
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
    forget_kind(Kind::Track, dir, volume_key, idents, expected)
}

fn forget_kind(kind: Kind, dir: &Path, volume_key: &str, idents: &[(i64, String)], expected: usize) -> Result<usize, String> {
    mutate(dir, volume_key, |store| {
        let live = live_keys(idents);
        let now = store.unattached(kind, &live).len();
        if now != expected {
            return Err(format!("{now} labelled {}s are no longer in the library, not the {expected} you were shown. Nothing was removed; reopen the tag manager.", kind.as_str()));
        }
        Ok(store.forget_unattached(kind, &live))
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

// ---- cases and mixes -----------------------------------------------------------------------

/// What can be labelled besides a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThingKind {
    Case,
    Mix,
}

impl ThingKind {
    fn kind(self) -> Kind {
        match self {
            ThingKind::Case => Kind::Case,
            ThingKind::Mix => Kind::Mix,
        }
    }
}

/// The tags and colour of one case or mix as the UI gets them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThingLabels {
    pub id: i64,
    pub tags: Vec<String>,
    pub colour: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThingsView {
    /// Only the cases (or mixes) that have any label.
    pub things: Vec<ThingLabels>,
    /// Labelled cases (or mixes) in the file that no longer exist.
    pub orphans: usize,
    pub notice: Option<String>,
}

/// `(id, store key)` of every case or mix on the card; see the module note for the key.
fn thing_keys(root: &str, which: ThingKind) -> AppResult<Vec<(i64, String)>> {
    let lib = Library::open_read_only(&PathBuf::from(root))?;
    Ok(match which {
        ThingKind::Case => lib.case_infos()?.into_iter().map(|c| (c.id, format!("{}:{}", c.id, c.date_created))).collect(),
        ThingKind::Mix => lib.mix_infos()?.into_iter().map(|m| (m.id, format!("{}:{}", m.id, m.date_created))).collect(),
    })
}

fn thing_row(id: i64, l: Option<&store::Labels>) -> ThingLabels {
    let l = l.cloned().unwrap_or_default();
    ThingLabels { id, tags: l.tags, colour: l.colour }
}

fn things_view(store: &Store, kind: Kind, idents: &[(i64, String)], notice: Option<String>) -> ThingsView {
    let things = idents.iter().filter_map(|(id, key)| store.get(kind, key).map(|l| thing_row(*id, Some(l)))).collect();
    ThingsView { things, orphans: store.unattached(kind, &live_keys(idents)).len(), notice }
}

fn load_things(dir: &Path, volume_key: &str, kind: Kind, idents: &[(i64, String)]) -> ThingsView {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let loaded = store::load(dir, volume_key);
    things_view(&loaded.store, kind, idents, loaded.notice)
}

fn apply_things_core(dir: &Path, volume_key: &str, kind: Kind, idents: &[(i64, String)], ids: &[i64], ops: &[Op]) -> Result<Vec<ThingLabels>, String> {
    if ops.iter().any(|o| matches!(o, Op::SetEnergy { energy: Some(_) })) {
        return Err("Energy is a rating of tracks; a case or mix has none.".into());
    }
    let by_id: HashMap<i64, &str> = idents.iter().map(|(i, k)| (*i, k.as_str())).collect();
    let keys = ids
        .iter()
        .map(|id| by_id.get(id).map(|k| (*k).to_string()).ok_or_else(|| format!("{} {id} does not exist.", kind.as_str())))
        .collect::<Result<Vec<_>, _>>()?;
    mutate(dir, volume_key, |store| {
        store.apply(kind, &keys, ops)?;
        Ok(ids.iter().zip(&keys).map(|(id, k)| thing_row(*id, store.get(kind, k))).collect())
    })
}

/// The labels of every case (or mix); those without any are left out.
#[tauri::command]
pub async fn tags_things_load(app: tauri::AppHandle, root: String, which: ThingKind) -> AppResult<ThingsView> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<ThingsView> {
        let idents = thing_keys(&root, which)?;
        Ok(load_things(&dir, &volume(&root), which.kind(), &idents))
    })
    .await?
}

/// Applies tag and colour changes to every listed case (or mix), all or nothing; returns the
/// labels of each listed one afterwards. Not a device write: the labels are the app's own.
#[tauri::command]
pub async fn tags_things_apply(app: tauri::AppHandle, root: String, which: ThingKind, ids: Vec<i64>, ops: Vec<Op>) -> AppResult<Vec<ThingLabels>> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<ThingLabels>> {
        let idents = thing_keys(&root, which)?;
        Ok(apply_things_core(&dir, &volume(&root), which.kind(), &idents, &ids, &ops)?)
    })
    .await?
}

// ---- the energy choice -----------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EnergySourceView {
    pub source: EnergySource,
    /// Set when the saved file could not be used (the default is then in force).
    pub problem: Option<String>,
}

#[tauri::command]
pub fn energy_source_get(app: tauri::AppHandle) -> AppResult<EnergySourceView> {
    let (source, problem) = energy_source::load(&config_dir(&app)?);
    Ok(EnergySourceView { source, problem })
}

#[tauri::command]
pub fn energy_source_set(app: tauri::AppHandle, source: EnergySource) -> AppResult<EnergySourceView> {
    let dir = config_dir(&app)?;
    energy_source::save(&dir, source)?;
    Ok(EnergySourceView { source, problem: None })
}

// ---- what the smart-case rules read ------------------------------------------------------------

/// The labels, auto energy estimates and energy choice of one library, read once. The smart-case
/// rules (tag, colour and energy rules) run on this, so the live preview, the sidebar count and
/// the rewrite of a smart case's Pacemaker copy all see the same labels the Tracks view shows.
///
/// A source that could not be read is NOT treated as "no labels": a rule like "tags: none of X"
/// would then match every track and rewrite the smart case's device copy wrongly. The reason is
/// kept in `problem` / `energy_problem` instead, and a smart case whose rules read the missing
/// part refuses to evaluate (it is skipped, its device copy left as it was) while the other
/// smart cases carry on.
#[derive(Debug, Clone, Default)]
pub struct LabelSnapshot {
    pub labels: HashMap<i64, Labels>,
    pub auto_energy: HashMap<i64, f32>,
    pub source: EnergySource,
    /// Why the labels file could not be used (tag, colour and energy rules all read it).
    pub problem: Option<String>,
    /// Why the energy choice or the saved energy estimates could not be used (energy rules only).
    pub energy_problem: Option<String>,
}

impl LabelSnapshot {
    /// The track's energy as a whole level under the energy choice (see [`energy_source`]).
    pub fn energy_level(&self, track_id: i64) -> Option<u8> {
        let mine = self.labels.get(&track_id).and_then(|l| l.energy);
        energy_source::resolve_level(mine, self.auto_energy.get(&track_id).copied(), self.source)
    }
}

/// Reads the snapshot for the library `lib` was opened on. A labels file, energy choice or
/// energy cache that cannot be used is reported in the snapshot's problem fields, not hidden as
/// an empty one (see [`LabelSnapshot`]). `uses_energy` says whether any rule reads energy: the
/// energy choice and the estimates are only read, and only held against, when one does.
pub fn snapshot(dir: &Path, lib: &Library, uses_energy: bool) -> Result<LabelSnapshot, String> {
    let root = lib.root().to_string_lossy().to_string();
    let idents = build_keys(&lib.track_identities().map_err(|e| e.to_string())?);
    let loaded = {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        store::load(dir, &volume(&root))
    };
    let labels = idents.iter().filter_map(|(id, key)| loaded.store.get(Kind::Track, key).map(|l| (*id, l.clone()))).collect();
    let mut snap = LabelSnapshot { labels, problem: loaded.notice, ..Default::default() };
    if uses_energy {
        let (source, source_problem) = energy_source::load(dir);
        snap.source = source;
        snap.energy_problem = source_problem;
        // the estimates only matter when the choice reads them
        if source != EnergySource::Mine {
            let (auto, cache_problem) = super::analysis::energy::energy_by_track_checked(dir, &root, lib)?;
            snap.auto_energy = auto;
            snap.energy_problem = snap.energy_problem.or(cache_problem);
        }
    }
    Ok(snap)
}

/// Where the labels file of the library at `root` lives (tests set one up, or damage it).
#[cfg(test)]
pub(crate) fn labels_file(dir: &Path, root: &str) -> PathBuf {
    store::store_path(dir, &volume(root))
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
                $crate::commands::tags::tags_things_load,
                $crate::commands::tags::tags_things_apply,
                $crate::commands::tags::energy_source_get,
                $crate::commands::tags::energy_source_set,
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

    // ---- cases and mixes, the energy choice and the smart-rule snapshot ----

    fn synthetic_library(tag: &str, n: usize) -> (pm_formats::library::Library, PathBuf, Vec<i64>) {
        use pm_formats::{library::TrackMeta, strfile};
        let base = std::env::temp_dir();
        let vol = base.join(format!("pmr-tags-{tag}-vol-{}", std::process::id()));
        let src = base.join(format!("pmr-tags-{tag}-src-{}", std::process::id()));
        for d in [&vol, &src] {
            let _ = std::fs::remove_dir_all(d);
            std::fs::create_dir_all(d).unwrap();
        }
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![0u8; 3000 + i]).unwrap();
            let meta = TrackMeta { title: format!("t{i}"), format: "MP3".into(), play_time_secs: 60 * (i as i64 + 1), ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        std::fs::remove_dir_all(&src).unwrap();
        (lib, vol, ids)
    }

    #[test]
    fn cases_and_mixes_keep_their_labels_through_a_rename_and_never_share_a_reused_id() {
        let (mut lib, vol, ids) = synthetic_library("things", 2);
        let cfg = tmp_dir("things-cfg");
        let root = vol.to_string_lossy().to_string();
        let a = lib.create_case_with_tracks("Synthetic case A", &ids).unwrap();
        let m = lib.create_mix("Synthetic mix").unwrap();
        let ops = [add(&["Opener", "Late"]), Op::SetColour { colour: Some("teal".into()) }];

        let ck = thing_keys(&root, ThingKind::Case).unwrap();
        let after = apply_things_core(&cfg, "e:", Kind::Case, &ck, &[a], &ops).unwrap();
        assert_eq!(after, [ThingLabels { id: a, tags: vec!["Late".into(), "Opener".into()], colour: Some("teal".into()) }]);
        let mk = thing_keys(&root, ThingKind::Mix).unwrap();
        apply_things_core(&cfg, "e:", Kind::Mix, &mk, &[m], &[add(&["Sunday"])]).unwrap();

        // kinds are separate namespaces even when ids collide
        assert_eq!(load_things(&cfg, "e:", Kind::Mix, &mk).things.len(), 1);
        assert_eq!(load_things(&cfg, "e:", Kind::Mix, &mk).things[0].tags, ["Sunday"]);

        // a rename does not change the key
        lib.rename_case(a, "Synthetic case renamed").unwrap();
        let ck2 = thing_keys(&root, ThingKind::Case).unwrap();
        assert_eq!(ck2, ck);
        assert_eq!(load_things(&cfg, "e:", Kind::Case, &ck2).things[0].colour.as_deref(), Some("teal"));

        // delete the case: its labels are kept but unattached
        lib.delete_case(a).unwrap();
        let ck3 = thing_keys(&root, ThingKind::Case).unwrap();
        let v = load_things(&cfg, "e:", Kind::Case, &ck3);
        assert_eq!((v.things.len(), v.orphans), (0, 1));
        // a new case, even one that gets the old row id, starts empty
        let b = lib.create_case_with_tracks("Synthetic case B", &ids).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let ck4 = thing_keys(&root, ThingKind::Case).unwrap();
        let v = load_things(&cfg, "e:", Kind::Case, &ck4);
        assert!(v.things.iter().all(|t| t.id != b), "the new case has no labels");
        assert_eq!(v.orphans, 1);

        // energy is a track rating only; an unknown id changes nothing
        assert!(apply_things_core(&cfg, "e:", Kind::Case, &ck4, &[b], &[Op::SetEnergy { energy: Some(3) }]).unwrap_err().contains("track"));
        assert!(apply_things_core(&cfg, "e:", Kind::Case, &ck4, &[b, 9999], &[add(&["x"])]).is_err());
        assert!(load_things(&cfg, "e:", Kind::Case, &ck4).things.iter().all(|t| t.id != b));
        // clearing the energy (what a generic clear op sends) is allowed
        apply_things_core(&cfg, "e:", Kind::Case, &ck4, &[b], &[Op::SetEnergy { energy: None }, add(&["y"])]).unwrap();
        let _ = std::fs::remove_dir_all(&vol);
        let _ = std::fs::remove_dir_all(&cfg);
    }

    #[test]
    fn the_snapshot_gives_smart_rules_the_labels_and_the_energy_choice() {
        use crate::commands::cases::smart::{evaluate, Data, Query, Rule};
        let (lib, vol, ids) = synthetic_library("snap", 3);
        let cfg = tmp_dir("snap-cfg");
        let root = vol.to_string_lossy().to_string();
        let idents = build_keys(&lib.track_identities().unwrap());
        apply_core(&cfg, &volume(&root), &idents, &[ids[0]], &[add(&["Peak"]), Op::SetColour { colour: Some("red".into()) }, Op::SetEnergy { energy: Some(8) }]).unwrap();
        apply_core(&cfg, &volume(&root), &idents, &[ids[1]], &[add(&["peak"])]).unwrap();

        let snap = snapshot(&cfg, &lib, true).unwrap();
        assert_eq!(snap.source, EnergySource::MineElseAuto, "no file = the default choice");
        assert_eq!(snap.labels.len(), 2);
        assert_eq!(snap.energy_level(ids[0]), Some(8));
        assert_eq!(snap.energy_level(ids[1]), None, "no rating and no estimate");

        let q = |r| Query { rules: vec![r], ..Default::default() };
        let tagged = q(Rule::Tags { how: crate::commands::cases::smart::TagMatch::Any, tags: vec!["PEAK".into()] });
        let none_of = q(Rule::Tags { how: crate::commands::cases::smart::TagMatch::None, tags: vec!["peak".into()] });
        let d = Data::load(&lib, &[&tagged, &none_of], 0, Some(&cfg)).unwrap();
        let mut got = evaluate(&tagged, &d).unwrap().ids;
        got.sort_unstable();
        assert_eq!(got, [ids[0], ids[1]]);
        assert_eq!(evaluate(&none_of, &d).unwrap().ids, [ids[2]]);

        // the choice is read from the config dir: "auto only" ignores the rating of track 0
        energy_source::save(&cfg, EnergySource::Auto).unwrap();
        let energy = q(Rule::EnergyRange { min: Some(1), max: None });
        let d = Data::load(&lib, &[&energy], 0, Some(&cfg)).unwrap();
        assert!(evaluate(&energy, &d).unwrap().ids.is_empty());
        energy_source::save(&cfg, EnergySource::Mine).unwrap();
        let d = Data::load(&lib, &[&energy], 0, Some(&cfg)).unwrap();
        assert_eq!(evaluate(&energy, &d).unwrap().ids, [ids[0]]);
        // with no config dir (tests only) there are no labels at all
        let d = Data::load(&lib, &[&energy], 0, None).unwrap();
        assert!(evaluate(&energy, &d).unwrap().ids.is_empty());
        let _ = std::fs::remove_dir_all(&vol);
        let _ = std::fs::remove_dir_all(&cfg);
    }

    #[test]
    fn unreadable_label_sources_refuse_the_rules_that_read_them_instead_of_matching_everything() {
        use crate::commands::cases::smart::{evaluate, Data, Mode, Query, Rule, TagMatch};
        let (lib, vol, ids) = synthetic_library("snapbad", 3);
        let cfg = tmp_dir("snapbad-cfg");
        let root = vol.to_string_lossy().to_string();
        let idents = build_keys(&lib.track_identities().unwrap());
        apply_core(&cfg, &volume(&root), &idents, &[ids[0]], &[add(&["Peak"]), Op::SetEnergy { energy: Some(8) }]).unwrap();
        let q = |r| Query { rules: vec![r], ..Default::default() };
        let none_of = q(Rule::Tags { how: TagMatch::None, tags: vec!["peak".into()] });
        let not_red = q(Rule::Colour { colours: vec!["red".into()], value: false });
        let group = q(Rule::Group { mode: Mode::None, items: vec![Rule::EnergyRange { min: Some(1), max: None }] });
        let plain = q(Rule::RatingMin { stars: 1 });
        let all = [&none_of, &not_red, &group, &plain];

        // a damaged labels file: every label rule refuses, the other smart cases are untouched
        let file = labels_file(&cfg, &root);
        std::fs::write(&file, "{ this is not json").unwrap();
        let snap = snapshot(&cfg, &lib, true).unwrap();
        assert!(snap.problem.is_some(), "the notice is kept, not dropped");
        let d = Data::load(&lib, &all, 0, Some(&cfg)).unwrap();
        for bad in [&none_of, &not_red, &group] {
            assert!(evaluate(bad, &d).unwrap_err().contains("could not be read"), "{bad:?}");
        }
        assert!(evaluate(&plain, &d).unwrap().ids.is_empty(), "a rule that reads no labels still runs");
        // the file is left exactly as it was
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ this is not json");

        // an unreadable file (a directory where the file should be) is the same
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        let d = Data::load(&lib, &[&none_of], 0, Some(&cfg)).unwrap();
        assert!(evaluate(&none_of, &d).is_err());
        std::fs::remove_dir(&file).unwrap();

        // a good file again: back to normal
        let d = Data::load(&lib, &[&none_of], 0, Some(&cfg)).unwrap();
        assert_eq!(evaluate(&none_of, &d).unwrap().ids.len(), 3, "no labels file at all is a real, empty set of labels");

        // a damaged energy choice blocks energy rules only, and only when something reads energy
        apply_core(&cfg, &volume(&root), &idents, &[ids[0]], &[add(&["Peak"]), Op::SetEnergy { energy: Some(8) }]).unwrap();
        std::fs::write(cfg.join("energy-source.json"), "{\"source\": \"sideways\"}").unwrap();
        let d = Data::load(&lib, &[&none_of, &group], 0, Some(&cfg)).unwrap();
        assert_eq!(evaluate(&none_of, &d).unwrap().ids.len(), 2);
        assert!(evaluate(&group, &d).unwrap_err().contains("Energy cannot be read"));
        let _ = std::fs::remove_dir_all(&vol);
        let _ = std::fs::remove_dir_all(&cfg);
    }
}
