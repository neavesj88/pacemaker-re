//! Duplicate finder and "which cases hold this track" (cases area).
//!
//! Finding is read-only. Copies are found by tags (normalised artist and title), by file size
//! and length, and by sound (a fingerprint from `pm_analysis::fingerprint`, cached per library
//! in the app cache dir by a cancellable background job). The review merges each group into the
//! copy the user keeps: case memberships move, the best rating and plays are kept, and the
//! user can additionally take the copies out of the library and delete their files, as two
//! separate choices. Every write goes through `Library` (one transaction per group, under the
//! write gate with its `music.db` snapshot) and files are deleted only by
//! `ops_device::delete_unreferenced`, which refuses anything a track still uses.
//!
//! The preview (`dupes_merge_plan`) and the job (`dupes_merge_start`) are built by the same
//! function; the job runs only if the plan it rebuilds has the digest the user confirmed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

use pm_analysis::fingerprint::{self, Fingerprint, Store};
use pm_analysis::peaks::{self, Stamp};
use pm_formats::library::{CaseHold, DupRow, GroupPlan, Library, MergeGroup, MergeOptions};
use pm_formats::{naming, ops_device};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::dupes_logic::{self as logic, Link, Mode};
use super::dupes_settings::{self as settings_store, DupSettings};
use super::export::plain_location;
use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

pub const JOB_FINGERPRINT: &str = "dupes-fingerprint";
pub const JOB_MERGE: &str = "dupes-merge";

/// Set by `dupes_cancel`, polled by the fingerprint job between tracks. Cleared when a job starts.
#[derive(Default)]
pub struct Cancel(pub AtomicBool);

pub fn setup(app: &mut tauri::App) {
    app.manage(Cancel::default());
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn fmt_bytes(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GB", b as f64 / (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        format!("{:.1} MB", b as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", b as f64 / 1024.0)
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The label a track goes by in lists and progress lines.
fn label_of(r: &DupRow) -> String {
    match (r.artist.trim().is_empty(), r.title.trim().is_empty()) {
        (false, false) => format!("{} - {}", r.artist.trim(), r.title.trim()),
        (true, false) => r.title.trim().to_string(),
        _ => r.location.rsplit('/').next().unwrap_or("").to_string(),
    }
}

// ---- files, resolved safely ------------------------------------------------------------------

/// The path of a track's audio file relative to `.Pacemaker/Music` ('/'-separated), only for a
/// location that is `/pmdb_tracks/` followed by plain names. Anything else (a `..`, a drive, a
/// rooted part) yields `None`: such a location is never turned into a path.
fn rel_of(location: &str) -> Option<String> {
    if !plain_location(location) {
        return None;
    }
    location.strip_prefix(naming::LOCATION_PREFIX).map(str::to_string)
}

fn music_path(root: &Path, rel: &str) -> PathBuf {
    root.join(naming::MUSIC_DIR).join(rel)
}

fn audio_path(root: &Path, location: &str) -> Option<PathBuf> {
    rel_of(location).map(|rel| music_path(root, &rel))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Present,
    /// The track points at a file that is not on the device.
    Missing,
    /// The location is not a plain path in the music folder; it is never read or deleted.
    Outside,
}

fn file_state(root: &Path, location: &str) -> FileState {
    match audio_path(root, location) {
        None => FileState::Outside,
        Some(p) if p.is_file() => FileState::Present,
        Some(_) => FileState::Missing,
    }
}

// ---- finding -----------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Member {
    #[serde(flatten)]
    pub row: DupRow,
    pub cases: Vec<CaseHold>,
    /// Names of the mixes that use this track.
    pub mixes: Vec<String>,
    pub file: FileState,
}

#[derive(Debug, Serialize)]
pub struct Group {
    /// The lowest track id in the group: stable while the group stays the same.
    pub id: i64,
    pub members: Vec<Member>,
    pub links: Vec<Link>,
    pub modes: Vec<Mode>,
    pub suggested_keeper: i64,
}

#[derive(Debug, Serialize)]
pub struct FindResult {
    pub groups: Vec<Group>,
    /// Tracks whose fingerprint is not cached yet (sound check on): run the fingerprint job.
    pub pending: Vec<i64>,
    /// Tracks the sound check could compare (fingerprint cached and usable).
    pub fingerprinted: usize,
    pub tracks: usize,
    pub warnings: Vec<String>,
}

/// A track the fingerprint job has to decode.
struct FpItem {
    track_id: i64,
    label: String,
    location: String,
    audio: PathBuf,
    stamp: Stamp,
}

/// Where the sound-check cache of a library lives: one file per volume in the app cache dir.
fn fingerprint_file(cache_dir: &Path, root: &str) -> PathBuf {
    cache_dir.join("fingerprints").join(format!("{:016x}.fp", crate::store::fnv1a(&crate::state::volume_key(Path::new(root)))))
}

/// The cached fingerprints the sound check can use, and the candidate tracks that still need one.
fn audio_state<'a>(root: &Path, store: &'a Store, rows: &[&DupRow], s: &DupSettings) -> (HashMap<i64, &'a Fingerprint>, Vec<FpItem>) {
    let by_id: HashMap<i64, &DupRow> = rows.iter().map(|r| (r.track_id, *r)).collect();
    let mut have = HashMap::new();
    let mut pending = Vec::new();
    for id in logic::audio_candidates(rows, s) {
        let r = by_id[&id];
        let Some(audio) = audio_path(root, &r.location) else { continue };
        let Ok(stamp) = peaks::stamp_of(&audio) else { continue }; // no file: nothing to compare
        match store.get(&r.location, stamp) {
            Some(f) => {
                have.insert(id, f);
            }
            None => pending.push(FpItem { track_id: id, label: label_of(r), location: r.location.clone(), audio, stamp }),
        }
    }
    (have, pending)
}

fn find_in(root: &Path, fp_file: &Path, s: &DupSettings) -> AppResult<FindResult> {
    let s = s.clone().clamped();
    let lib = Library::open_read_only(root)?;
    let all = lib.dup_rows()?;
    let rows: Vec<&DupRow> = all.iter().collect();
    let (mut links, mut warnings) = (Vec::new(), Vec::new());
    if s.by_tags {
        let (l, unusable) = logic::tag_links(&rows, &s);
        links.extend(l);
        if unusable > 0 {
            warnings.push(format!("{} had no artist or no title, so the tag check could not compare {}.", plural(unusable, "track", "tracks"), if unusable == 1 { "it" } else { "them" }));
        }
    }
    if s.by_size {
        links.extend(logic::size_links(&rows, &s));
    }
    let (mut pending, mut fingerprinted) = (Vec::new(), 0);
    if s.by_audio {
        let (store, warn) = Store::load(fp_file);
        warnings.extend(warn);
        let (have, todo) = audio_state(root, &store, &rows, &s);
        fingerprinted = have.values().filter(|f| !f.frames.is_empty()).count();
        links.extend(logic::audio_links(&rows, &have, &s, thread_count(&s)));
        pending = todo.iter().map(|i| i.track_id).collect();
    }

    let raw = logic::group(links);
    let by_id: HashMap<i64, &DupRow> = all.iter().map(|r| (r.track_id, r)).collect();
    let ids: Vec<i64> = raw.iter().flat_map(|g| g.ids.iter().copied()).collect();
    let mut holds = HashMap::new();
    let mut mixes = HashMap::new();
    for chunk in ids.chunks(400) {
        for h in lib.cases_holding(chunk)? {
            holds.insert(h.track_id, h.cases);
        }
        mixes.extend(lib.mix_names_using(chunk)?);
    }
    let states: HashMap<i64, FileState> = ids.iter().map(|id| (*id, file_state(root, &by_id[id].location))).collect();

    let mut groups: Vec<Group> = raw
        .into_iter()
        .map(|g| {
            let members_rows: Vec<&DupRow> = g.ids.iter().map(|id| by_id[id]).collect();
            let keeper = logic::pick_keeper(&members_rows, s.keeper_rule, &|id| states[&id] == FileState::Present);
            let mut modes: Vec<Mode> = g.links.iter().map(|l| l.mode).collect();
            modes.sort();
            modes.dedup();
            let members = members_rows
                .iter()
                .map(|r| Member { row: (*r).clone(), cases: holds.get(&r.track_id).cloned().unwrap_or_default(), mixes: mixes.get(&r.track_id).cloned().unwrap_or_default(), file: states[&r.track_id] })
                .collect();
            Group { id: g.ids[0], members, links: g.links, modes, suggested_keeper: keeper }
        })
        .collect();
    groups.sort_by_cached_key(|g| {
        logic::display_order(g.members.iter().find(|m| m.row.track_id == g.suggested_keeper).map(|m| &m.row), g.id)
    });
    Ok(FindResult { groups, pending, fingerprinted, tracks: all.len(), warnings })
}

#[derive(Serialize)]
pub struct SettingsState {
    settings: DupSettings,
    /// Set when the settings file could not be fully read (the defaults are used for that part).
    warning: Option<String>,
}

/// The duplicate finder's settings.
#[tauri::command]
pub async fn dupes_settings_get(app: AppHandle) -> AppResult<SettingsState> {
    let dir = super::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (settings, warning) = settings_store::load(&dir);
        Ok(SettingsState { settings, warning })
    })
    .await?
}

/// Saves the settings in the app's config dir (not on the device); returns them as stored.
#[tauri::command]
pub async fn dupes_settings_set(app: AppHandle, settings: DupSettings) -> AppResult<SettingsState> {
    let dir = super::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SettingsState> {
        settings_store::save(&dir, &settings)?;
        let (settings, warning) = settings_store::load(&dir);
        Ok(SettingsState { settings, warning })
    })
    .await?
}

/// Finds groups of copies with the ways the settings turn on. Reads only (the fingerprint cache
/// is read, never written, here).
#[tauri::command]
pub async fn dupes_find(app: AppHandle, root: String, settings: DupSettings) -> AppResult<FindResult> {
    let fp = fingerprint_file(&app.path().app_cache_dir()?, &root);
    tauri::async_runtime::spawn_blocking(move || find_in(Path::new(&root), &fp, &settings)).await?
}

// ---- the fingerprint job -----------------------------------------------------------------------

struct FpOutcome {
    done: usize,
    unusable: usize,
    failed: Vec<(String, String)>,
    cancelled: bool,
    total: usize,
    save_error: Option<String>,
}

impl FpOutcome {
    fn summary(&self) -> String {
        let mut parts = vec![format!("Fingerprinted {} of {}", self.done, plural(self.total, "track", "tracks"))];
        if self.unusable > 0 {
            parts.push(format!("{} too short or silent to compare", self.unusable));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} could not be read", self.failed.len()));
        }
        if self.cancelled {
            parts.push("stopped at your request; what was done is kept".into());
        }
        if let Some(e) = &self.save_error {
            parts.push(format!("the cache could not be saved: {e}"));
        }
        parts.join(", ")
    }
}

/// Fingerprints the items on `threads` workers, adds them to `store` and appends every 25 new
/// entries to the cache's journal (so cancelling or a crash keeps the work without rewriting the
/// whole cache each time: that grew with the store, 100+ GB over a big first run), then writes the
/// whole store once at the end. A track that decodes but has nothing to fingerprint is stored as
/// empty, so it is not tried again until the file changes.
fn fingerprint_run(items: Vec<FpItem>, store: &mut Store, store_path: &Path, threads: usize, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> FpOutcome {
    let total = items.len();
    let mut out = FpOutcome { done: 0, unusable: 0, failed: Vec::new(), cancelled: false, total, save_error: None };
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<(usize, Result<Option<Fingerprint>, String>)>();
    let mut finished = 0usize;
    let mut unsaved: Vec<String> = Vec::new();
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, total.max(1)) {
            let tx = tx.clone();
            let (items, next) = (&items, &next);
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(i) else { break };
                    // a codec panic on one damaged file must cost that file, not the job
                    let res = match std::panic::catch_unwind(|| fingerprint::fingerprint_file(&item.audio)) {
                        Ok(r) => r.map_err(|e| e.to_string()),
                        Err(_) => Err("the decoder crashed on this file".to_string()),
                    };
                    if tx.send((i, res)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (i, res) in rx {
            let item = &items[i];
            finished += 1;
            let mut p = Progress::new(JOB_FINGERPRINT, item.label.clone(), finished, total, "fingerprinted");
            match res {
                Ok(Some(fp)) => {
                    store.put(item.location.clone(), item.stamp, fp);
                    unsaved.push(item.location.clone());
                    out.done += 1;
                }
                Ok(None) => {
                    store.put(item.location.clone(), item.stamp, Fingerprint { frames: Vec::new() });
                    unsaved.push(item.location.clone());
                    out.done += 1;
                    out.unusable += 1;
                    p.status = "skipped".into();
                }
                Err(e) => {
                    p.status = "failed".into();
                    p.error = Some(e.clone());
                    out.failed.push((item.label.clone(), e));
                }
            }
            progress(p);
            if unsaved.len() >= 25 {
                match store.append(store_path, &unsaved) {
                    Ok(()) => unsaved.clear(),
                    // keep them: the next append (or the full save) retries
                    Err(e) => out.save_error = Some(e.to_string()),
                }
            }
        }
    });
    out.cancelled = cancel.load(Ordering::SeqCst) && finished < total;
    match store.save(store_path) {
        Ok(()) => out.save_error = None,
        Err(e) => out.save_error = Some(e.to_string()),
    }
    out
}

fn thread_count(s: &DupSettings) -> usize {
    if s.fingerprint_threads > 0 {
        return s.fingerprint_threads as usize;
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    (cores / 2).clamp(1, 4)
}

/// Starts the fingerprint job for the tracks the sound check still lacks and returns how many.
/// Progress and the end arrive as `job-progress` / `job-done` of job `dupes-fingerprint`. Writes
/// nothing to the device; the cache is in the app cache dir.
#[tauri::command]
pub async fn dupes_fingerprint_start(app: AppHandle, lock: State<'_, JobLock>, root: String, settings: DupSettings) -> AppResult<usize> {
    let s = settings.clamped();
    let fp = fingerprint_file(&app.path().app_cache_dir()?, &root);
    let (r, f, s2) = (root.clone(), fp.clone(), s.clone());
    let (store, items) = tauri::async_runtime::spawn_blocking(move || -> AppResult<(Store, Vec<FpItem>)> {
        let lib = Library::open_read_only(Path::new(&r))?;
        let all = lib.dup_rows()?;
        let rows: Vec<&DupRow> = all.iter().collect();
        let (mut store, _) = Store::load(&f);
        // tracks that left the library no longer need their fingerprints
        let live: HashSet<&str> = all.iter().map(|r| r.location.as_str()).collect();
        store.retain_keys(|k| live.contains(k));
        let (_, items) = audio_state(Path::new(&r), &store, &rows, &s2);
        Ok((store, items))
    })
    .await??;
    if items.is_empty() {
        return Err("Every track that could be compared already has its fingerprint.".into());
    }
    let n = items.len();
    let threads = thread_count(&s);
    spawn_job(app, &lock, JOB_FINGERPRINT, move |app| {
        // cleared only once this call owns the job lock, so a refused second start cannot un-cancel a running job
        let cancel = &app.state::<Cancel>().0;
        cancel.store(false, Ordering::SeqCst);
        let mut store = store;
        let out = fingerprint_run(items, &mut store, &fp, threads, cancel, &mut |p| events::emit(app, JOB_PROGRESS, p));
        Ok(JobOutcome { ok: out.done, failed: out.failed.len(), summary: out.summary() })
    })?;
    Ok(n)
}

/// Asks the running fingerprint job to stop before its next track.
#[tauri::command]
pub fn dupes_cancel(app: AppHandle) {
    app.state::<Cancel>().0.store(true, Ordering::SeqCst);
}

// ---- merging: the plan ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequest {
    pub groups: Vec<MergeGroup>,
    /// Take the copies' rows out of the library (a copy a mix uses stays).
    pub remove_rows: bool,
    /// Also delete the removed copies' audio and grid files; needs `remove_rows`.
    pub delete_files: bool,
    /// The settings the preview was made with: what a merge does with ratings, plays and cases.
    pub settings: DupSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct Label {
    pub track_id: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileItem {
    pub track_id: i64,
    /// Relative to `.Pacemaker/Music`, '/'-separated.
    pub path: String,
    /// "audio" or "grid" (the `.str` beside it).
    pub kind: &'static str,
    pub size: u64,
    pub exists: bool,
    /// Set when the file will be left alone, and why.
    pub kept_because: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Totals {
    pub groups: usize,
    pub case_entries: usize,
    pub rows_removed: usize,
    pub files_deleted: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub groups: Vec<GroupPlan>,
    pub labels: Vec<Label>,
    pub files: Vec<FileItem>,
    pub notes: Vec<String>,
    /// Things that stop the merge (the kept track has no usable file while a copy being deleted
    /// does): the dialog shows them in red and its button stays off, and `dupes_merge_start`
    /// refuses a plan that has any.
    pub problems: Vec<String>,
    pub totals: Totals,
    /// Identifies this exact plan; the job runs only if the rebuilt plan has the same one.
    pub digest: String,
}

fn options_of(req: &MergeRequest) -> MergeOptions {
    req.settings.merge_options(req.remove_rows)
}

/// The files (relative paths, lowercase: FAT has no case) that rows staying in the library use.
fn files_in_use(all: &[DupRow], removed: &HashSet<i64>) -> HashSet<String> {
    all.iter().filter(|r| !removed.contains(&r.track_id)).filter_map(|r| rel_of(&r.location)).map(|r| r.to_lowercase()).collect()
}

/// Everything a merge would do, without writing: the dialog's preview and the list the user
/// confirms. The job is started from the same function.
fn build_plan(lib: &Library, req: &MergeRequest) -> AppResult<Plan> {
    if req.groups.is_empty() {
        return Err("Pick at least one group to merge.".into());
    }
    if req.delete_files && !req.remove_rows {
        return Err("Deleting the files needs the copies to be removed from the library as well: a track whose file is gone would stay in the library.".into());
    }
    if req.remove_rows && !req.settings.allow_row_removal {
        return Err("Removing copies from the library is switched off in the duplicate finder settings: deleting rows from music.db has not been tried on a real Pacemaker yet.".into());
    }
    let plans = lib.merge_plan_for(&req.groups, &options_of(req))?;
    let all = lib.dup_rows()?;
    let by_id: HashMap<i64, &DupRow> = all.iter().map(|r| (r.track_id, r)).collect();
    let removed: HashSet<i64> = plans.iter().flat_map(|p| p.remove.iter().map(|r| r.track_id)).collect();
    let in_use = files_in_use(&all, &removed);

    let root = lib.root();
    let (mut files, mut notes) = (Vec::new(), Vec::new());
    if req.delete_files {
        for p in &plans {
            for r in &p.remove {
                let label = by_id.get(&r.track_id).map(|r| label_of(r)).unwrap_or_default();
                let Some(rel) = rel_of(&r.location) else {
                    notes.push(format!("“{label}”: its location ({}) is not a plain path in the music folder, so no file is touched; only the library row is removed.", r.location));
                    continue;
                };
                let shared = in_use.contains(&rel.to_lowercase());
                for (kind, path) in [("audio", rel.clone()), ("grid", naming::str_path(&rel))] {
                    let meta = std::fs::metadata(music_path(root, &path)).ok().filter(|m| m.is_file());
                    files.push(FileItem {
                        track_id: r.track_id,
                        path,
                        kind,
                        size: meta.as_ref().map_or(0, |m| m.len()),
                        exists: meta.is_some(),
                        kept_because: shared.then(|| "another track in the library uses this file".to_string()),
                    });
                }
            }
        }
    }
    for p in &plans {
        for b in &p.blocked {
            let label = by_id.get(&b.track_id).map(|r| label_of(r)).unwrap_or_default();
            let mixes = b.mixes.iter().map(|m| format!("“{m}”")).collect::<Vec<_>>().join(", ");
            let tail = if req.settings.move_cases { " Its case places still move." } else { "" };
            notes.push(format!("“{label}” stays in the library: {} {mixes} {} it.{tail}", if b.mixes.len() == 1 { "the mix" } else { "the mixes" }, if b.mixes.len() == 1 { "uses" } else { "use" }));
        }
    }
    let mut problems = Vec::new();
    if req.delete_files {
        for p in &plans {
            let Some(keeper) = by_id.get(&p.keeper) else { continue };
            let state = file_state(root, &keeper.location);
            let loses_a_file = files.iter().any(|f| f.kind == "audio" && f.exists && f.kept_because.is_none() && p.remove.iter().any(|r| r.track_id == f.track_id));
            if state != FileState::Present && loses_a_file {
                let why = if state == FileState::Missing { "its audio file is not in the library" } else { "its location is not a plain path in the music folder" };
                problems.push(format!("“{}”, the track you keep, has no usable audio file ({why}), but a copy you are deleting does: after this merge there would be no playable file. Choose another track to keep, or leave the files alone.", label_of(keeper)));
            }
        }
    }
    let labels = plans
        .iter()
        .flat_map(|p| std::iter::once(p.keeper).chain(p.others.iter().copied()))
        .filter_map(|id| by_id.get(&id).map(|r| Label { track_id: id, label: label_of(r) }))
        .collect();
    let deleted: Vec<&FileItem> = files.iter().filter(|f| f.exists && f.kept_because.is_none()).collect();
    let totals = Totals {
        groups: plans.len(),
        case_entries: plans.iter().map(|p| p.case_edits.iter().map(|e| e.others.len()).sum::<usize>()).sum(),
        rows_removed: removed.len(),
        files_deleted: deleted.len(),
        bytes: deleted.iter().map(|f| f.size).sum(),
    };
    let digest = format!("{:016x}", crate::store::fnv1a(&serde_json::to_string(&(&plans, &files, &problems)).map_err(|e| e.to_string())?));
    Ok(Plan { groups: plans, labels, files, notes, problems, totals, digest })
}

/// The request's settings, held to the stored ones where a setting is a gate rather than a
/// preference: the frontend's copy can only make the rules stricter, never lift a switch the
/// user left off (the same rule as the space planner's `require_archive`).
fn held_to_stored(app: &AppHandle, request: MergeRequest) -> AppResult<MergeRequest> {
    let (stored, _) = settings_store::load(&super::config_dir(app)?);
    Ok(hold_gates(request, &stored))
}

fn hold_gates(mut request: MergeRequest, stored: &DupSettings) -> MergeRequest {
    request.settings.allow_row_removal &= stored.allow_row_removal;
    request
}

/// What merging these groups would do, file by file. Reads only.
#[tauri::command]
pub async fn dupes_merge_plan(app: AppHandle, root: String, request: MergeRequest) -> AppResult<Plan> {
    let request = held_to_stored(&app, request)?;
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        build_plan(&lib, &request)
    })
    .await?
}

// ---- merging: the job ----------------------------------------------------------------------------

struct MergeOutcome {
    merged: usize,
    case_entries: usize,
    removed: usize,
    kept_for_mixes: usize,
    files_deleted: usize,
    bytes: u64,
    failed: Vec<(String, String)>,
    skipped_files: usize,
    delete_failed: Vec<String>,
}

impl MergeOutcome {
    fn summary(&self) -> String {
        let mut parts = vec![format!("Merged {}", plural(self.merged, "group", "groups"))];
        if self.case_entries > 0 {
            parts.push(format!("{} moved to the kept track", plural(self.case_entries, "case place", "case places")));
        }
        if self.removed > 0 {
            parts.push(format!("{} removed from the library", plural(self.removed, "copy", "copies")));
        }
        if self.kept_for_mixes > 0 {
            parts.push(format!("{} kept because mixes use {}", plural(self.kept_for_mixes, "copy", "copies"), if self.kept_for_mixes == 1 { "it" } else { "them" }));
        }
        if self.files_deleted > 0 {
            parts.push(format!("{} deleted ({})", plural(self.files_deleted, "file", "files"), fmt_bytes(self.bytes)));
        }
        if self.skipped_files > 0 {
            parts.push(format!("{} left alone because a track still uses {}", plural(self.skipped_files, "file", "files"), if self.skipped_files == 1 { "it" } else { "them" }));
        }
        if !self.delete_failed.is_empty() {
            parts.push(format!("{} could not be deleted: {}", self.delete_failed.len(), self.delete_failed.join("; ")));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} not merged", plural(self.failed.len(), "group", "groups")));
        }
        parts.join(", ")
    }
}

/// Carries the plan out: one transaction per group (each checked against its planned form), then
/// the files of the removed rows. The caller holds the write ticket. A group that fails is
/// reported and the next one goes on.
fn merge_run(root: &Path, plan: &Plan, req: &MergeRequest, progress: &mut dyn FnMut(Progress)) -> AppResult<MergeOutcome> {
    let mut lib = Library::open(root)?;
    let opts = options_of(req);
    let label: HashMap<i64, &str> = plan.labels.iter().map(|l| (l.track_id, l.label.as_str())).collect();
    let mut out = MergeOutcome { merged: 0, case_entries: 0, removed: 0, kept_for_mixes: 0, files_deleted: 0, bytes: 0, failed: Vec::new(), skipped_files: 0, delete_failed: Vec::new() };
    let mut removed_ids = Vec::new();
    let total = req.groups.len() + usize::from(req.delete_files);
    for (i, (group, planned)) in req.groups.iter().zip(&plan.groups).enumerate() {
        let name = label.get(&group.keeper).copied().unwrap_or("track").to_string();
        let mut p = Progress::new(JOB_MERGE, format!("Merging {name}"), i + 1, total, "merged");
        match lib.merge_duplicates(group, &opts, Some(planned)) {
            Ok(done) => {
                out.merged += 1;
                out.case_entries += done.case_edits.iter().map(|e| e.others.len()).sum::<usize>();
                out.kept_for_mixes += done.blocked.len();
                out.removed += done.remove.len();
                removed_ids.extend(done.remove.iter().map(|r| r.track_id));
            }
            Err(e) => {
                p.status = "failed".into();
                p.error = Some(e.to_string());
                out.failed.push((name, e.to_string()));
            }
        }
        progress(p);
    }
    drop(lib);
    if req.delete_files {
        let removed: HashSet<i64> = removed_ids.into_iter().collect();
        let mut seen = HashSet::new();
        // only the files of rows that really left the library, as the plan listed them
        let wanted: Vec<String> = plan.files.iter().filter(|f| removed.contains(&f.track_id) && f.kept_because.is_none() && f.exists).map(|f| f.path.clone()).filter(|p| seen.insert(p.clone())).collect();
        let sizes: HashMap<&str, u64> = plan.files.iter().map(|f| (f.path.as_str(), f.size)).collect();
        if !wanted.is_empty() {
            // the rows are already gone: a refusal here (an incompatible database, say) is reported
            // with the rest, not allowed to hide what the merges did
            match ops_device::delete_unreferenced(
                root,
                &wanted,
                &mut |_, _, path| {
                    if !path.is_empty() {
                        progress(Progress::new(JOB_MERGE, format!("Deleting {path}"), total, total, "deleting"));
                    }
                },
                &|| false,
            ) {
                Ok(rep) => {
                    out.files_deleted = rep.deleted;
                    out.bytes = wanted.iter().filter(|p| !rep.skipped.contains(p) && !rep.failed.iter().any(|f| f.starts_with(&format!("{p}:")))).map(|p| sizes.get(p.as_str()).copied().unwrap_or(0)).sum();
                    out.skipped_files = rep.skipped.len();
                    out.delete_failed = rep.failed;
                }
                Err(e) => out.delete_failed.push(format!("no file was deleted: {e}")),
            }
        }
    }
    Ok(out)
}

/// Starts the merge as a background write job and returns at once; progress and the end arrive
/// as `job-progress` / `job-done` of job `dupes-merge`. `digest` is the digest of the plan the
/// user confirmed: the plan is rebuilt here and nothing is written if it differs. The job holds
/// a write ticket (so the first write of the session gets its `music.db` safety copy and an
/// eject or backup refuses it).
#[tauri::command]
pub async fn dupes_merge_start(app: AppHandle, lock: State<'_, JobLock>, root: String, request: MergeRequest, digest: String) -> AppResult<()> {
    let request = held_to_stored(&app, request)?;
    let (r, q) = (root.clone(), request.clone());
    let plan = tauri::async_runtime::spawn_blocking(move || -> AppResult<Plan> {
        let lib = Library::open_read_only(Path::new(&r))?;
        let plan = build_plan(&lib, &q)?;
        if !plan.problems.is_empty() {
            return Err(format!("Nothing was written: {}", plan.problems.join(" ")).into());
        }
        if plan.digest != digest {
            return Err("The library changed after the preview was made, so nothing was written. The preview has been refreshed: check it and press Merge again.".into());
        }
        Ok(plan)
    })
    .await??;
    spawn_job(app, &lock, JOB_MERGE, move |app| {
        let _ticket = crate::state::begin_write(app, Path::new(&root))?;
        let out = merge_run(Path::new(&root), &plan, &request, &mut |p| events::emit(app, JOB_PROGRESS, p))?;
        Ok(JobOutcome { ok: out.merged, failed: out.failed.len() + out.delete_failed.len(), summary: out.summary() })
    })?;
    Ok(())
}

// ---- which cases hold a track ----------------------------------------------------------------------

/// A case that holds a track, as the Cases view lists it.
#[derive(Debug, Serialize)]
pub struct HoldingCase {
    #[serde(flatten)]
    pub hold: CaseHold,
    /// Set when this case is the Pacemaker copy of a smart case.
    pub smart_copy_of: Option<u32>,
}

/// A smart case whose rules match the track right now.
#[derive(Debug, Serialize)]
pub struct HoldingSmart {
    pub id: u32,
    pub name: String,
    pub on_device: bool,
    pub device_case_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TrackHolding {
    pub track_id: i64,
    pub cases: Vec<HoldingCase>,
    pub smart: Vec<HoldingSmart>,
}

/// For each track: the cases that hold it (with its place in each) and the smart cases that
/// match it now. Reads only.
#[tauri::command]
pub async fn cases_holding(app: AppHandle, root: String, track_ids: Vec<i64>) -> AppResult<Vec<TrackHolding>> {
    let dir = super::config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || holding_in(&root, &dir, &track_ids, now_ms())).await?
}

fn holding_in(root: &str, dir: &Path, track_ids: &[i64], now: i64) -> AppResult<Vec<TrackHolding>> {
    let lib = Library::open_read_only(Path::new(root))?;
    let holds = lib.cases_holding(track_ids)?;
    let smart_list = super::smart::load(dir, root)?;
    // a smart case that cannot be evaluated (a rule names a deleted case) simply matches nothing here
    let matched: Vec<(usize, HashSet<i64>)> = if smart_list.is_empty() || track_ids.is_empty() {
        Vec::new()
    } else {
        let queries: Vec<&super::smart::Query> = smart_list.iter().map(|s| &s.query).collect();
        let data = super::smart::Data::load(&lib, &queries, now, Some(dir))?;
        smart_list.iter().enumerate().filter_map(|(i, s)| super::smart::holds_among(&s.query, &data, track_ids).ok().map(|ids| (i, ids))).collect()
    };
    Ok(holds
        .into_iter()
        .map(|h| TrackHolding {
            track_id: h.track_id,
            cases: h
                .cases
                .into_iter()
                .map(|c| {
                    let smart_copy_of = smart_list.iter().find(|s| s.device_case_id == Some(c.case_id)).map(|s| s.id);
                    HoldingCase { hold: c, smart_copy_of }
                })
                .collect(),
            smart: matched
                .iter()
                .filter(|(_, ids)| ids.contains(&h.track_id))
                .map(|(i, _)| HoldingSmart { id: smart_list[*i].id, name: smart_list[*i].name.clone(), on_device: smart_list[*i].on_device, device_case_id: smart_list[*i].device_case_id })
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests;
