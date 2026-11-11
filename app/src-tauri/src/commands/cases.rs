//! Cases: list, create, rename, edit, delete, membership and order, favourites, duplicate, merge,
//! copy / move between cases, bulk delete, export of one case or of many to a folder (with the
//! audio copied alongside; `cases/export_bulk.rs`), and smart cases.
//!
//! Every write goes through pm-formats `Library` (so `date_modified`, `play_time_secs` and the
//! rest of a case row stay what the Editor and the device would write) and happens only on an
//! explicit user action. Smart-case rules are kept in the app's config dir, never on the
//! device; see `cases/smart.rs`.

mod export;
mod export_bulk;
mod export_prefs;
mod smart;
mod smart_export;
mod smart_settings;
mod smart_sync;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pm_formats::library::{CaseInfo, CaseMeta, Error, Library, MergeDest, MergeDone, MergePlan, TransferDone, TransferNewDone, TransferPlan};
use serde::Serialize;
use tauri::{Manager, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;
use export_prefs::ExportPrefs;
use smart::{Data, Eval, Query, SmartCase};
use smart_settings::SmartSettings;

/// The `job` name of the bulk case export in `job-progress` / `job-done` events.
pub const JOB_CASES_EXPORT: &str = export_bulk::JOB;

pub fn setup(app: &mut tauri::App) {
    app.manage(export_bulk::Cancel::default());
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Runs `f` on a blocking thread with the library open for writing (refused if the library is
/// not device-compatible), as a quick write: after the session's first-write snapshot, and
/// waited for by Eject (see `state::quick_write`).
async fn write<T: Send + 'static>(
    app: tauri::AppHandle,
    root: String,
    f: impl FnOnce(&mut Library) -> Result<T, Error> + Send + 'static,
) -> AppResult<T> {
    let path = PathBuf::from(&root);
    crate::state::quick_write(app, root, move || {
        let mut lib = Library::open(&path)?;
        Ok(f(&mut lib)?)
    })
    .await
}

/// Runs `f` on a blocking thread with the library open read-only.
async fn read<T: Send + 'static>(root: String, f: impl FnOnce(&Library) -> Result<T, Error> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<T> {
        let lib = Library::open_read_only(&PathBuf::from(root))?;
        Ok(f(&lib)?)
    })
    .await?
}

// ---- cases ---------------------------------------------------------------------------------

/// Track ids in a case, in the order they were added (read-only).
#[tauri::command]
pub async fn case_tracks(root: String, case_id: i64) -> AppResult<Vec<i64>> {
    read(root, move |lib| lib.case_track_ids(case_id)).await
}

/// Every case with its metadata, ordered by name (read-only).
#[tauri::command]
pub async fn cases_list(root: String) -> AppResult<Vec<CaseInfo>> {
    read(root, |lib| lib.case_infos()).await
}

/// Creates a case holding `track_ids` (may be empty); returns its id.
#[tauri::command]
pub async fn cases_create(app: tauri::AppHandle, root: String, name: String, track_ids: Vec<i64>) -> AppResult<i64> {
    write(app, root, move |lib| lib.create_case_with_tracks(&name, &track_ids)).await
}

#[tauri::command]
pub async fn cases_rename(app: tauri::AppHandle, root: String, case_id: i64, name: String) -> AppResult<bool> {
    write(app, root, move |lib| lib.rename_case(case_id, &name)).await
}

/// Saves the case dialog's fields (name, genre, year, rating, producer, remixer, comments).
#[tauri::command]
pub async fn cases_set_meta(app: tauri::AppHandle, root: String, case_id: i64, meta: CaseMeta) -> AppResult<bool> {
    write(app, root, move |lib| lib.set_case_meta(case_id, &meta)).await
}

/// Deletes a case and its membership rows (the tracks themselves stay).
#[tauri::command]
pub async fn cases_delete(app: tauri::AppHandle, root: String, case_id: i64) -> AppResult<bool> {
    write(app, root, move |lib| lib.delete_case(case_id)).await
}

/// Appends tracks to a case, skipping any already in it; returns how many were added. The
/// tracks view calls this for "Add to case".
#[tauri::command]
pub async fn cases_add_tracks(app: tauri::AppHandle, root: String, case_id: i64, track_ids: Vec<i64>) -> AppResult<usize> {
    write(app, root, move |lib| lib.add_case_tracks(case_id, &track_ids)).await
}

#[tauri::command]
pub async fn cases_remove_tracks(app: tauri::AppHandle, root: String, case_id: i64, track_ids: Vec<i64>) -> AppResult<usize> {
    write(app, root, move |lib| lib.remove_case_tracks(case_id, &track_ids)).await
}

/// Replaces a case's tracks with exactly this list, in this order (drag-to-reorder).
#[tauri::command]
pub async fn cases_set_tracks(app: tauri::AppHandle, root: String, case_id: i64, track_ids: Vec<i64>) -> AppResult<()> {
    write(app, root, move |lib| lib.set_case_tracks(case_id, &track_ids)).await
}

/// Adds the track to the "Favourites" case (created on first use) or removes it; returns
/// whether it is now a favourite. Cross-area contract, see `api/cases.ts`.
#[tauri::command]
pub async fn cases_toggle_favourite(app: tauri::AppHandle, root: String, track_id: i64) -> AppResult<bool> {
    write(app, root, move |lib| lib.toggle_favourite(track_id)).await
}

/// Ids in the "Favourites" case (empty if it does not exist yet).
#[tauri::command]
pub async fn cases_favourite_ids(root: String) -> AppResult<Vec<i64>> {
    read(root, |lib| lib.favourite_track_ids()).await
}


// ---- organising: duplicate, merge, copy / move, delete several, export ---------------------------

/// Copies a case under `name`: same tracks in the same order, same genre / year / rating /
/// producer / remixer / comments, play statistics reset. Returns the new case's id.
#[tauri::command]
pub async fn cases_duplicate(app: tauri::AppHandle, root: String, case_id: i64, name: String) -> AppResult<i64> {
    write(app, root, move |lib| lib.duplicate_case(case_id, &name)).await
}

/// What merging `source_ids` into `target` (an existing case) or into a new case (`None`) would
/// add. The merge itself computes its tracks with the same function. Reads only.
#[tauri::command]
pub async fn cases_merge_preview(root: String, source_ids: Vec<i64>, target: Option<i64>) -> AppResult<MergePlan> {
    read(root, move |lib| lib.merge_plan(&source_ids, target)).await
}

/// Merges `source_ids` without repeats. Exactly one of `target` (an existing case, which keeps
/// its order and gets the new tracks appended) and `new_name` (a new case) must be given.
/// `delete_sources` deletes the source cases afterwards in the same transaction.
#[tauri::command]
pub async fn cases_merge(
    app: tauri::AppHandle,
    root: String,
    source_ids: Vec<i64>,
    target: Option<i64>,
    new_name: Option<String>,
    delete_sources: bool,
) -> AppResult<MergeDone> {
    let dest = match (target, new_name) {
        (Some(id), None) => MergeDest::Existing(id),
        (None, Some(name)) => MergeDest::New(name),
        _ => return Err("Pick either an existing case or a name for a new one.".into()),
    };
    write(app, root, move |lib| lib.merge_cases(&source_ids, &dest, delete_sources)).await
}

/// What copying (or, with `remove_from_source`, moving) `track_ids` from case `from` (`None`:
/// from the library) into case `to` would do. Reads only.
#[tauri::command]
pub async fn cases_transfer_preview(
    root: String,
    from: Option<i64>,
    to: i64,
    track_ids: Vec<i64>,
    remove_from_source: bool,
) -> AppResult<TransferPlan> {
    read(root, move |lib| lib.transfer_plan(from, to, &track_ids, remove_from_source)).await
}

/// Copies or moves tracks between cases; see [`cases_transfer_preview`]. A move removes the
/// tracks from `from` only; the tracks themselves stay in the library.
#[tauri::command]
pub async fn cases_transfer(
    app: tauri::AppHandle,
    root: String,
    from: Option<i64>,
    to: i64,
    track_ids: Vec<i64>,
    remove_from_source: bool,
) -> AppResult<TransferDone> {
    write(app, root, move |lib| lib.transfer_tracks(from, to, &track_ids, remove_from_source)).await
}

/// Copies or moves tracks into a case created by the same write (one transaction, so a failure
/// leaves no empty case). Returns the new case's id with what was transferred.
#[tauri::command]
pub async fn cases_transfer_new(
    app: tauri::AppHandle,
    root: String,
    from: Option<i64>,
    new_name: String,
    track_ids: Vec<i64>,
    remove_from_source: bool,
) -> AppResult<TransferNewDone> {
    write(app, root, move |lib| lib.transfer_to_new_case(from, &new_name, &track_ids, remove_from_source)).await
}

/// Deletes several cases (and their membership rows) in one transaction; the tracks stay.
/// Returns how many cases were deleted.
#[tauri::command]
pub async fn cases_delete_many(app: tauri::AppHandle, root: String, case_ids: Vec<i64>) -> AppResult<usize> {
    write(app, root, move |lib| lib.delete_cases(&case_ids)).await
}

/// Writes one case to `dest` as "m3u8", "csv" or "rekordbox" (XML). Reads the library, writes
/// only the chosen file, which may not be inside the device volume.
#[tauri::command]
pub async fn cases_export(root: String, case_id: i64, format: String, dest: String) -> AppResult<export::ExportDone> {
    tauri::async_runtime::spawn_blocking(move || export::export_case(Path::new(&root), case_id, &format, &dest)).await?
}

// ---- smart cases -----------------------------------------------------------------------------

fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

/// What the query matches right now in `lib`: the one evaluation behind the live preview, the
/// sidebar counts and the device copy.
fn matches_in(lib: &Library, q: &Query) -> AppResult<Eval> {
    let data = Data::load(lib, &[q], now_ms())?;
    Ok(smart::evaluate(q, &data)?)
}

/// Blocking: loads the store, runs `f`, maps the string errors.
fn with_store<T>(dir: &Path, root: &str, f: impl FnOnce(Vec<SmartCase>) -> Result<T, String>) -> AppResult<T> {
    Ok(f(smart::load(dir, root)?)?)
}

/// The saved smart cases of this library.
#[tauri::command]
pub async fn smart_cases_list(app: tauri::AppHandle, root: String) -> AppResult<Vec<SmartCase>> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || with_store(&dir, &root, Ok)).await?
}

#[derive(Serialize)]
pub struct SmartCount {
    id: u32,
    /// Tracks matched after the limit; 0 when `error` is set.
    count: usize,
    /// Why this smart case cannot be evaluated (a rule names a case that was deleted).
    error: Option<String>,
}

/// How many tracks each saved smart case matches right now (one read of the library).
#[tauri::command]
pub async fn smart_cases_counts(app: tauri::AppHandle, root: String) -> AppResult<Vec<SmartCount>> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<SmartCount>> {
        let list = smart::load(&dir, &root)?;
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        let queries: Vec<&Query> = list.iter().map(|s| &s.query).collect();
        let data = Data::load(&lib, &queries, now_ms())?;
        Ok(list
            .iter()
            .map(|s| match smart::evaluate(&s.query, &data) {
                Ok(e) => SmartCount { id: s.id, count: e.ids.len(), error: None },
                Err(error) => SmartCount { id: s.id, count: 0, error: Some(error) },
            })
            .collect())
    })
    .await?
}

/// The track ids a query (rules, order, limit) matches, in display order, and how many matched
/// before the limit: the rule builder's live count and the smart case's track list. Nothing is
/// saved.
#[tauri::command]
pub async fn smart_cases_evaluate(root: String, query: Query) -> AppResult<Eval> {
    smart::validate_query(&query)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Eval> {
        let lib = Library::open_read_only(&PathBuf::from(root))?;
        matches_in(&lib, &query)
    })
    .await?
}

/// Saves a new (id 0) or edited smart case in the config dir; returns it as stored. Does not
/// touch the device copy: use `smart_cases_put_on_device` to refresh that.
#[tauri::command]
pub async fn smart_cases_save(app: tauri::AppHandle, root: String, smart_case: SmartCase) -> AppResult<SmartCase> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SmartCase> { Ok(smart::save(&dir, &root, smart_case)?) }).await?
}

/// Deletes a smart case. With `remove_device_case` the case it made on the Pacemaker goes too;
/// without it that case stays as an ordinary case. Returns whether a device case was deleted.
#[tauri::command]
pub async fn smart_cases_delete(app: tauri::AppHandle, root: String, id: u32, remove_device_case: bool) -> AppResult<bool> {
    let dir = config_dir(&app)?;
    crate::state::quick_write(app, root.clone(), move || -> AppResult<bool> {
        let list = smart::load(&dir, &root)?;
        let Some(sc) = list.into_iter().find(|s| s.id == id) else {
            return Ok(false);
        };
        let mut deleted = false;
        if remove_device_case && sc.device_case_id.is_some() {
            let mut lib = Library::open(&PathBuf::from(&root))?;
            deleted = smart::take_off_device(&mut lib, &sc)?;
        }
        smart::remove(&dir, &root, id)?;
        Ok(deleted)
    })
    .await
}

#[derive(Serialize)]
pub struct DeviceCopy {
    smart_case: SmartCase,
    /// Tracks in the device case after the write (0 after taking it off).
    count: usize,
    /// Set when this smart case was skipped (refresh-all only): why, and nothing was written.
    error: Option<String>,
}

/// A smart case must not read its own device copy: that copy is rebuilt from the rules.
fn check_not_self_referencing(sc: &SmartCase) -> AppResult<()> {
    match smart_sync::self_reference(sc) {
        Some(why) => Err(why.into()),
        None => Ok(()),
    }
}

/// The "Put on Pacemaker" toggle. `on = true` creates the device case (named with a star
/// prefix) or, if it exists, refreshes it to the current matches; `on = false` deletes it.
#[tauri::command]
pub async fn smart_cases_set_on_device(app: tauri::AppHandle, root: String, id: u32, on: bool) -> AppResult<DeviceCopy> {
    let dir = config_dir(&app)?;
    crate::state::quick_write(app, root.clone(), move || -> AppResult<DeviceCopy> {
        let mut sc = smart::load(&dir, &root)?.into_iter().find(|s| s.id == id).ok_or_else(|| format!("no smart case {id}"))?;
        smart::validate(&sc.name, &sc.query)?;
        let mut lib = Library::open(&PathBuf::from(&root))?;
        let count = if on {
            check_not_self_referencing(&sc)?;
            let ids = matches_in(&lib, &sc.query)?.ids;
            let (case_id, n) = smart::put_on_device(&mut lib, &sc, &ids)?;
            sc.on_device = true;
            sc.device_case_id = Some(case_id);
            n
        } else {
            smart::take_off_device(&mut lib, &sc)?;
            sc.on_device = false;
            sc.device_case_id = None;
            0
        };
        smart::update(&dir, &root, &sc)?;
        Ok(DeviceCopy { smart_case: sc, count, error: None })
    })
    .await
}

/// What refreshing the Pacemaker copies of the smart cases that are switched on would change,
/// by smart case, with the tracks that would come and go named. Reads only; the same
/// computation `smart_cases_refresh_apply` writes from.
#[tauri::command]
pub async fn smart_cases_refresh_plan(app: tauri::AppHandle, root: String) -> AppResult<smart_sync::Report> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || refresh_preview(&dir, &root)).await?
}

fn refresh_preview(dir: &Path, root: &str) -> AppResult<smart_sync::Report> {
    let list: Vec<SmartCase> = smart::load(dir, root)?.into_iter().filter(|s| s.on_device).collect();
    if list.is_empty() {
        return Ok(smart_sync::Report { items: Vec::new(), changes: 0 });
    }
    let lib = Library::open_read_only(&PathBuf::from(root))?;
    Ok(smart_sync::preview(&lib, &list, now_ms())?)
}

/// Refreshes the Pacemaker copy of every smart case that has "Put on Pacemaker" on and no
/// longer matches its rules, and returns what was done. Nothing is written (and no safety copy
/// of `music.db` is made) when every copy is already current. A smart case that cannot run is
/// reported as skipped with the reason and its copy is left as it was.
#[tauri::command]
pub async fn smart_cases_refresh_apply(app: tauri::AppHandle, root: String) -> AppResult<smart_sync::Report> {
    let dir = config_dir(&app)?;
    let (d, r) = (dir.clone(), root.clone());
    let pre = tauri::async_runtime::spawn_blocking(move || refresh_preview(&d, &r)).await??;
    if pre.changes == 0 {
        return Ok(pre);
    }
    crate::state::quick_write(app, root.clone(), move || -> AppResult<smart_sync::Report> {
        let list: Vec<SmartCase> = smart::load(&dir, &root)?.into_iter().filter(|s| s.on_device).collect();
        let mut lib = Library::open(&PathBuf::from(&root))?;
        Ok(smart_sync::apply(&mut lib, &dir, &root, &list, now_ms())?)
    })
    .await
}

// ---- smart-case settings and exports -----------------------------------------------------------

#[derive(Serialize)]
pub struct SettingsState {
    settings: SmartSettings,
    /// Set when the settings file could not be fully read (the defaults are used for that part).
    warning: Option<String>,
}

/// The smart-case settings (refresh mode, export choices).
#[tauri::command]
pub async fn smart_settings_get(app: tauri::AppHandle) -> AppResult<SettingsState> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (settings, warning) = smart_settings::load(&dir);
        Ok(SettingsState { settings, warning })
    })
    .await?
}

/// Saves the smart-case settings in the app's config dir (not on the device).
#[tauri::command]
pub async fn smart_settings_set(app: tauri::AppHandle, settings: SmartSettings) -> AppResult<SettingsState> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SettingsState> {
        smart_settings::save(&dir, &settings)?;
        let (settings, warning) = smart_settings::load(&dir);
        Ok(SettingsState { settings, warning })
    })
    .await?
}

/// Writes one smart case, as it matches now, to `dest` as "m3u8", "csv" or "rekordbox" (XML).
/// Reads only; the file may not be inside the device volume.
#[tauri::command]
pub async fn smart_cases_export(app: tauri::AppHandle, root: String, id: u32, format: String, dest: String) -> AppResult<export::ExportDone> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || smart_export::export_one(&root, &dir, id, &format, &dest, now_ms())).await?
}

// ---- exporting several cases to a folder ---------------------------------------------------------

#[derive(Serialize)]
pub struct ExportPrefsState {
    prefs: ExportPrefs,
    /// Set when the choices file could not be fully read (the defaults are used for that part).
    warning: Option<String>,
}

/// The export choices: last folder and format, relative paths, copying the audio, what to do
/// with files already there. App preferences, not on the device.
#[tauri::command]
pub async fn cases_export_prefs_get(app: tauri::AppHandle) -> AppResult<ExportPrefsState> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (prefs, warning) = export_prefs::load(&dir);
        Ok(ExportPrefsState { prefs, warning })
    })
    .await?
}

/// Saves the export choices in the app's config dir.
#[tauri::command]
pub async fn cases_export_prefs_set(app: tauri::AppHandle, prefs: ExportPrefs) -> AppResult<ExportPrefsState> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<ExportPrefsState> {
        export_prefs::save(&dir, &prefs)?;
        let (prefs, warning) = export_prefs::load(&dir);
        Ok(ExportPrefsState { prefs, warning })
    })
    .await?
}

/// Everything an export would do, file by file, without writing anything: the dialog's preview
/// and the list the user confirms. The export runs from the same function.
#[tauri::command]
pub async fn cases_export_plan(app: tauri::AppHandle, root: String, request: export_bulk::Request) -> AppResult<export_bulk::Plan> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<export_bulk::Plan> {
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        Ok(export_bulk::build(&lib, &dir, &root, &request, now_ms())?.plan())
    })
    .await?
}

#[derive(Serialize)]
pub struct ExportStarted {
    folder: String,
    /// Files the job will write (audio copies plus playlists): the progress total.
    writes: usize,
}

/// Starts the export as a background job (it writes only into the chosen folder, never to the
/// device) and returns at once; progress and the end arrive as `job-progress` / `job-done` events
/// of job `cases-export`. A folder or selection that cannot work is an error here, before any file.
#[tauri::command]
pub async fn cases_export_start(app: tauri::AppHandle, lock: State<'_, JobLock>, root: String, request: export_bulk::Request) -> AppResult<ExportStarted> {
    let dir = config_dir(&app)?;
    let (d, r, q) = (dir.clone(), root.clone(), request.clone());
    let writes = tauri::async_runtime::spawn_blocking(move || -> AppResult<usize> {
        let lib = Library::open_read_only(&PathBuf::from(&r))?;
        Ok(export_bulk::build(&lib, &d, &r, &q, now_ms())?.writes())
    })
    .await??;
    let folder = request.folder.clone();
    let shown = folder.clone();
    spawn_job(app, &lock, JOB_CASES_EXPORT, move |app| {
        // cleared only once this call owns the job lock, so a refused second start cannot un-cancel a running export
        let cancel = &app.state::<export_bulk::Cancel>().0;
        cancel.store(false, std::sync::atomic::Ordering::SeqCst);
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        let built = export_bulk::build(&lib, &dir, &root, &request, now_ms())?;
        let out = export_bulk::execute(&lib, &built, cancel, &mut |s| {
            let mut p = Progress::new(JOB_CASES_EXPORT, s.label, s.done, s.total, s.status);
            p.error = s.error;
            events::emit(app, JOB_PROGRESS, p);
        });
        Ok(JobOutcome { ok: out.lists_written + out.audio_copied, failed: out.failed.len(), summary: out.summary(&shown) })
    })?;
    Ok(ExportStarted { folder, writes })
}

/// Asks the running case export to stop. It ends before the next file (a copy stops within a
/// block and removes its partial file); files already written stay. Does nothing when no export runs.
#[tauri::command]
pub fn cases_export_cancel(app: tauri::AppHandle) {
    app.state::<export_bulk::Cancel>().0.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The library-wide rekordbox XML, smart cases included per the settings. For the Import view's
/// "Export XML" (`import::interop_export`), so there is one library export. Blocking.
pub fn library_xml(app: &tauri::AppHandle, root: &str) -> AppResult<smart_export::LibraryXml> {
    let dir = config_dir(app)?;
    let lib = Library::open_read_only(&PathBuf::from(root))?;
    smart_export::library_xml(&lib, &dir, root, now_ms())
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! cases_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::cases::case_tracks,
                $crate::commands::cases::cases_list,
                $crate::commands::cases::cases_create,
                $crate::commands::cases::cases_rename,
                $crate::commands::cases::cases_set_meta,
                $crate::commands::cases::cases_delete,
                $crate::commands::cases::cases_add_tracks,
                $crate::commands::cases::cases_remove_tracks,
                $crate::commands::cases::cases_set_tracks,
                $crate::commands::cases::cases_toggle_favourite,
                $crate::commands::cases::cases_favourite_ids,
                $crate::commands::cases::cases_duplicate,
                $crate::commands::cases::cases_merge_preview,
                $crate::commands::cases::cases_merge,
                $crate::commands::cases::cases_transfer_preview,
                $crate::commands::cases::cases_transfer,
                $crate::commands::cases::cases_transfer_new,
                $crate::commands::cases::cases_delete_many,
                $crate::commands::cases::cases_export,
                $crate::commands::cases::smart_cases_list,
                $crate::commands::cases::smart_cases_counts,
                $crate::commands::cases::smart_cases_evaluate,
                $crate::commands::cases::smart_cases_save,
                $crate::commands::cases::smart_cases_delete,
                $crate::commands::cases::smart_cases_set_on_device,
                $crate::commands::cases::smart_cases_refresh_plan,
                $crate::commands::cases::smart_cases_refresh_apply,
                $crate::commands::cases::smart_cases_export,
                $crate::commands::cases::smart_settings_get,
                $crate::commands::cases::smart_settings_set,
                $crate::commands::cases::cases_export_prefs_get,
                $crate::commands::cases::cases_export_prefs_set,
                $crate::commands::cases::cases_export_plan,
                $crate::commands::cases::cases_export_start,
                $crate::commands::cases::cases_export_cancel,
            ];
            $($rest),*
        }
    };
}
