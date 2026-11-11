//! Cases: list, create, rename, edit, delete, membership and order, favourites, and smart cases.
//!
//! Every write goes through pm-formats `Library` (so `date_modified`, `play_time_secs` and the
//! rest of a case row stay what the Editor and the device would write) and happens only on an
//! explicit user action. Smart-case rules are kept in the app's config dir, never on the
//! device; see `cases/smart.rs`.

mod smart;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pm_formats::library::{CaseInfo, CaseMeta, Error, Library};
use serde::Serialize;
use tauri::Manager;

use crate::error::AppResult;
use smart::{Context, Rule, SmartCase};

pub fn setup(_app: &mut tauri::App) {}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Runs `f` on a blocking thread with the library open for writing (refused if the library is
/// not device-compatible).
async fn write<T: Send + 'static>(root: String, f: impl FnOnce(&mut Library) -> Result<T, Error> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<T> {
        let mut lib = Library::open(&PathBuf::from(root))?;
        Ok(f(&mut lib)?)
    })
    .await?
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
pub async fn cases_create(root: String, name: String, track_ids: Vec<i64>) -> AppResult<i64> {
    write(root, move |lib| lib.create_case_with_tracks(&name, &track_ids)).await
}

#[tauri::command]
pub async fn cases_rename(root: String, case_id: i64, name: String) -> AppResult<bool> {
    write(root, move |lib| lib.rename_case(case_id, &name)).await
}

/// Saves the case dialog's fields (name, genre, year, rating, producer, remixer, comments).
#[tauri::command]
pub async fn cases_set_meta(root: String, case_id: i64, meta: CaseMeta) -> AppResult<bool> {
    write(root, move |lib| lib.set_case_meta(case_id, &meta)).await
}

/// Deletes a case and its membership rows (the tracks themselves stay).
#[tauri::command]
pub async fn cases_delete(root: String, case_id: i64) -> AppResult<bool> {
    write(root, move |lib| lib.delete_case(case_id)).await
}

/// Appends tracks to a case, skipping any already in it; returns how many were added. The
/// tracks view calls this for "Add to case".
#[tauri::command]
pub async fn cases_add_tracks(root: String, case_id: i64, track_ids: Vec<i64>) -> AppResult<usize> {
    write(root, move |lib| lib.add_case_tracks(case_id, &track_ids)).await
}

#[tauri::command]
pub async fn cases_remove_tracks(root: String, case_id: i64, track_ids: Vec<i64>) -> AppResult<usize> {
    write(root, move |lib| lib.remove_case_tracks(case_id, &track_ids)).await
}

/// Replaces a case's tracks with exactly this list, in this order (drag-to-reorder).
#[tauri::command]
pub async fn cases_set_tracks(root: String, case_id: i64, track_ids: Vec<i64>) -> AppResult<()> {
    write(root, move |lib| lib.set_case_tracks(case_id, &track_ids)).await
}

/// Adds the track to the "Favourites" case (created on first use) or removes it; returns
/// whether it is now a favourite. Cross-area contract, see `api/cases.ts`.
#[tauri::command]
pub async fn cases_toggle_favourite(root: String, track_id: i64) -> AppResult<bool> {
    write(root, move |lib| lib.toggle_favourite(track_id)).await
}

/// Ids in the "Favourites" case (empty if it does not exist yet).
#[tauri::command]
pub async fn cases_favourite_ids(root: String) -> AppResult<Vec<i64>> {
    read(root, |lib| lib.favourite_track_ids()).await
}

// ---- smart cases -----------------------------------------------------------------------------

fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

/// What the rules match right now in `lib`.
fn matches_in(lib: &Library, rules: &[Rule]) -> Result<Vec<i64>, Error> {
    let tracks = lib.tracks()?;
    let favourites: HashSet<i64> = lib.favourite_track_ids()?.into_iter().collect();
    Ok(smart::evaluate(rules, &tracks, &Context { favourites: &favourites, now_ms: now_ms() }))
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
    count: usize,
}

/// How many tracks each saved smart case matches right now (one read of the library).
#[tauri::command]
pub async fn smart_cases_counts(app: tauri::AppHandle, root: String) -> AppResult<Vec<SmartCount>> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<SmartCount>> {
        let list = smart::load(&dir, &root)?;
        let lib = Library::open_read_only(&PathBuf::from(&root))?;
        let tracks = lib.tracks()?;
        let favourites: HashSet<i64> = lib.favourite_track_ids()?.into_iter().collect();
        let cx = Context { favourites: &favourites, now_ms: now_ms() };
        Ok(list.iter().map(|s| SmartCount { id: s.id, count: smart::evaluate(&s.rules, &tracks, &cx).len() }).collect())
    })
    .await?
}

/// The track ids a rule set matches, in display order: the rule builder's live count and the
/// smart case's track list. Nothing is saved.
#[tauri::command]
pub async fn smart_cases_evaluate(root: String, rules: Vec<Rule>) -> AppResult<Vec<i64>> {
    smart::validate_rules(&rules)?;
    read(root, move |lib| matches_in(lib, &rules)).await
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
    tauri::async_runtime::spawn_blocking(move || -> AppResult<bool> {
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
    .await?
}

#[derive(Serialize)]
pub struct DeviceCopy {
    smart_case: SmartCase,
    /// Tracks in the device case after the write (0 after taking it off).
    count: usize,
}

/// The "Put on Pacemaker" toggle. `on = true` creates the device case (named with a star
/// prefix) or, if it exists, refreshes it to the current matches; `on = false` deletes it.
#[tauri::command]
pub async fn smart_cases_set_on_device(app: tauri::AppHandle, root: String, id: u32, on: bool) -> AppResult<DeviceCopy> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<DeviceCopy> {
        let mut sc = smart::load(&dir, &root)?.into_iter().find(|s| s.id == id).ok_or_else(|| format!("no smart case {id}"))?;
        smart::validate(&sc.name, &sc.rules)?;
        let mut lib = Library::open(&PathBuf::from(&root))?;
        let count = if on {
            let ids = matches_in(&lib, &sc.rules)?;
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
        Ok(DeviceCopy { smart_case: sc, count })
    })
    .await?
}

/// Refreshes the device case of every smart case that has "Put on Pacemaker" on.
#[tauri::command]
pub async fn smart_cases_refresh_all(app: tauri::AppHandle, root: String) -> AppResult<Vec<DeviceCopy>> {
    let dir = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<DeviceCopy>> {
        let list: Vec<SmartCase> = smart::load(&dir, &root)?.into_iter().filter(|s| s.on_device).collect();
        if list.is_empty() {
            return Ok(Vec::new());
        }
        let mut lib = Library::open(&PathBuf::from(&root))?;
        let mut out = Vec::new();
        for mut sc in list {
            let ids = matches_in(&lib, &sc.rules)?;
            let (case_id, count) = smart::put_on_device(&mut lib, &sc, &ids)?;
            sc.device_case_id = Some(case_id);
            smart::update(&dir, &root, &sc)?;
            out.push(DeviceCopy { smart_case: sc, count });
        }
        Ok(out)
    })
    .await?
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
                $crate::commands::cases::smart_cases_list,
                $crate::commands::cases::smart_cases_counts,
                $crate::commands::cases::smart_cases_evaluate,
                $crate::commands::cases::smart_cases_save,
                $crate::commands::cases::smart_cases_delete,
                $crate::commands::cases::smart_cases_set_on_device,
                $crate::commands::cases::smart_cases_refresh_all,
            ];
            $($rest),*
        }
    };
}
