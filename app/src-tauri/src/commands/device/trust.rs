//! The trust pack: restore `music.db` from a snapshot, verify a backup file by file, and the
//! Health card's fixes. The logic is in `pm_formats::{ops_restore, ops_fix}` and
//! `ops_device::verify_manifest_detailed`; this file is the command layer (write gate, job lock,
//! events) and the user-adjustable settings of the pack.
//!
//! Every write here goes through the app's write safety: a restore is an exclusive operation
//! (`take_exclusive_locked`: the job lock plus the write gate, so nothing else writes meanwhile)
//! with a fresh snapshot of the current database first; removing dead rows is a quick write with
//! the session snapshot plus a fresh one.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pm_formats::library::Library;
use pm_formats::ops_device::{self, VerifyReport, SNAPSHOTS_KEPT};
use pm_formats::ops_fix::{self, FixPlan, RemoveReport};
use pm_formats::ops_restore::{self, RestoreReport, SnapshotDiff, SnapshotRow};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_job};
use crate::state::{JobLock, snapshots_dir};

/// The job kind of a backup verification.
pub const JOB_VERIFY: &str = "device-verify";

// ---- settings ------------------------------------------------------------------------------

/// What the user can tune in the trust pack. Stored as JSON in the app's config dir (never on
/// the device); a missing or unreadable file means the defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TrustSettings {
    /// How many edits the undo history keeps per library. 0 turns the history off.
    pub undo_limit: usize,
    /// Share of the library (percent) above which "remove dead rows" asks for an extra
    /// acknowledgement, because that many missing files usually means a card that is not fully
    /// mounted.
    pub dead_row_guard_percent: u32,
    /// How long the Undo pill stays on screen after an edit, in seconds. 0 hides the pill (the
    /// Edit history card still has the list).
    pub undo_pill_seconds: u32,
}

pub const MAX_UNDO_LIMIT: usize = 200;

impl Default for TrustSettings {
    fn default() -> Self {
        TrustSettings { undo_limit: 20, dead_row_guard_percent: 50, undo_pill_seconds: 10 }
    }
}

impl TrustSettings {
    /// Puts every value in its allowed range.
    pub fn clamped(mut self) -> Self {
        self.undo_limit = self.undo_limit.min(MAX_UNDO_LIMIT);
        self.dead_row_guard_percent = self.dead_row_guard_percent.clamp(1, 100);
        self.undo_pill_seconds = self.undo_pill_seconds.min(120);
        self
    }
}

const SETTINGS_FILE: &str = "trust-settings.json";

/// Reads the settings file. A missing file is the defaults; an unreadable one is the defaults
/// plus a warning (the file is left as it is until the user saves new settings).
pub fn load_settings(dir: &Path) -> (TrustSettings, Option<String>) {
    let p = dir.join(SETTINGS_FILE);
    match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<TrustSettings>(&s) {
            Ok(v) => (v.clamped(), None),
            Err(e) => (TrustSettings::default(), Some(format!("{} could not be read ({e}); the defaults are in use", p.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (TrustSettings::default(), None),
        Err(e) => (TrustSettings::default(), Some(format!("{}: {e}; the defaults are in use", p.display()))),
    }
}

/// Writes the settings atomically (temp file, then rename).
pub fn save_settings(dir: &Path, s: &TrustSettings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let p = dir.join(SETTINGS_FILE);
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &p)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[derive(Default)]
pub struct TrustState {
    settings: Mutex<Option<(TrustSettings, Option<String>)>>,
    verify: Mutex<Option<VerifyReport>>,
}

pub fn setup(app: &mut tauri::App) {
    app.manage(TrustState::default());
}

fn config_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

/// The settings in force (read from disk on first use, then kept).
pub fn settings_of(app: &AppHandle) -> TrustSettings {
    settings_and_warning(app).0
}

fn settings_and_warning(app: &AppHandle) -> (TrustSettings, Option<String>) {
    let st = app.state::<TrustState>();
    let mut slot = st.settings.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(match config_dir(app) {
            Ok(dir) => load_settings(&dir),
            Err(e) => (TrustSettings::default(), Some(format!("the config folder is unavailable ({e}); the defaults are in use"))),
        });
    }
    slot.clone().unwrap_or_default()
}

#[derive(Serialize)]
pub struct SettingsView {
    pub settings: TrustSettings,
    pub max_undo_limit: usize,
    /// Set when the settings file could not be read and the defaults are showing.
    pub warning: Option<String>,
}

/// The trust-pack settings.
#[tauri::command]
pub fn device_trust_settings(app: AppHandle) -> SettingsView {
    let (settings, warning) = settings_and_warning(&app);
    SettingsView { settings, max_undo_limit: MAX_UNDO_LIMIT, warning }
}

/// Saves the settings (values are clamped to their range) and returns what is in force.
#[tauri::command]
pub fn device_trust_settings_set(app: AppHandle, settings: TrustSettings) -> AppResult<SettingsView> {
    let settings = settings.clamped();
    save_settings(&config_dir(&app)?, &settings)?;
    *app.state::<TrustState>().settings.lock().unwrap_or_else(|e| e.into_inner()) = Some((settings.clone(), None));
    super::undo::trim_all(&app, settings.undo_limit);
    Ok(SettingsView { settings, max_undo_limit: MAX_UNDO_LIMIT, warning: None })
}

// ---- snapshots: list, preview, restore ----------------------------------------------------

#[derive(Serialize)]
pub struct SnapshotsPanel {
    pub folder: String,
    pub keep: usize,
    /// Newest first, each with its counts and what a restore from it would change.
    pub rows: Vec<SnapshotRow>,
}

/// The automatic snapshots of the library on `root` with a summary of how each differs from the
/// card now. Reads every snapshot's database once; read-only.
#[tauri::command]
pub async fn device_snapshot_rows(app: AppHandle, root: String) -> AppResult<SnapshotsPanel> {
    let dir = snapshots_dir(&app, Path::new(&root))?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SnapshotsPanel> {
        let rows = ops_restore::snapshot_rows(Path::new(&root), &dir);
        Ok(SnapshotsPanel { folder: dir.display().to_string(), keep: SNAPSHOTS_KEPT, rows })
    })
    .await?
}

/// Everything a restore from the named snapshot would change, with names. Read-only. The same
/// comparison `device_restore_snapshot` repeats before it writes.
#[tauri::command]
pub async fn device_snapshot_preview(app: AppHandle, root: String, name: String) -> AppResult<SnapshotDiff> {
    let dir = snapshots_dir(&app, Path::new(&root))?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SnapshotDiff> {
        let entry = ops_restore::find_snapshot(&dir, &name)?;
        Ok(ops_restore::diff_snapshot(Path::new(&root), Path::new(&entry.folder))?)
    })
    .await?
}

/// Replaces the card's `music.db` with the named snapshot's, after a fresh safety copy of the
/// current one. `fingerprint` is the value from `device_snapshot_preview`: if the card or the
/// snapshot changed since, nothing is done. The undo history of this library is dropped (it
/// described a database that is gone). Refused while a job runs.
#[tauri::command]
pub async fn device_restore_snapshot(app: AppHandle, root: String, name: String, fingerprint: String) -> AppResult<RestoreReport> {
    let dir = snapshots_dir(&app, Path::new(&root))?;
    let a = app.clone();
    let r = root.clone();
    let report = tauri::async_runtime::spawn_blocking(move || -> AppResult<RestoreReport> {
        // the job lock and the write gate together: no job, no quick write, no eject while the
        // file is swapped
        let _exclusive = crate::state::take_exclusive_locked(&a, "Restore", "being restored")?;
        Ok(ops_restore::restore_snapshot(Path::new(&r), &dir, &name, &fingerprint, SNAPSHOTS_KEPT)?)
    })
    .await??;
    super::undo::clear(&app, &root);
    Ok(report)
}

// ---- verify a backup ----------------------------------------------------------------------

fn mb(bytes: u64) -> String {
    super::mb(bytes)
}

/// Job `device-verify`: re-reads every file of a backup folder and checks it against its
/// `MANIFEST.sha256`. Reads the backup only (never the device). The per-file result is fetched
/// with `device_verify_report` once the job is done. Stopped with `device_cancel`.
#[tauri::command]
pub fn device_verify_backup(app: AppHandle, lock: State<'_, JobLock>, folder: String) -> AppResult<()> {
    let dir = PathBuf::from(&folder);
    if !dir.is_dir() {
        return Err(format!("{folder} is not a folder.").into());
    }
    app.state::<TrustState>().verify.lock().unwrap_or_else(|e| e.into_inner()).take();
    spawn_job(app, &lock, JOB_VERIFY, move |app| {
        super::start_job_cancel(app);
        let rep = ops_device::verify_manifest_detailed(
            &dir,
            &mut |done, total, label| {
                if !label.is_empty() {
                    events::emit(app, JOB_PROGRESS, Progress::new(JOB_VERIFY, label, done, total, "checking"));
                }
            },
            &|| super::cancelled(app),
        )?;
        if rep.cancelled {
            return Err(format!("Cancelled: the check was stopped after {} of {} files, so it is not a result", rep.checked, rep.listed).into());
        }
        let problems = rep.problems();
        let mut summary = if problems == 0 {
            format!("All {} files in {} match their checksums ({} read).", rep.listed, rep.folder, mb(rep.bytes))
        } else {
            format!("{problems} of {} files in {} do not match the manifest; see the list.", rep.listed, rep.folder)
        };
        if !rep.extra.is_empty() {
            summary.push_str(&format!(" {} other file{} in the folder {} not in the manifest.", rep.extra.len(), if rep.extra.len() == 1 { "" } else { "s" }, if rep.extra.len() == 1 { "is" } else { "are" }));
        }
        let outcome = JobOutcome { ok: rep.ok, failed: problems, summary };
        *app.state::<TrustState>().verify.lock().unwrap_or_else(|e| e.into_inner()) = Some(rep);
        Ok(outcome)
    })
}

/// The per-file result of the last finished `device-verify` job, or null if none finished.
#[tauri::command]
pub fn device_verify_report(app: AppHandle) -> Option<VerifyReport> {
    app.state::<TrustState>().verify.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// ---- Health card fixes --------------------------------------------------------------------

/// What the Health card could fix, with names. Read-only. The same classification
/// `device_fix_remove_rows` uses.
#[tauri::command]
pub async fn device_fix_plan(app: AppHandle, root: String) -> AppResult<FixPlan> {
    let guard = settings_of(&app).dead_row_guard_percent;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<FixPlan> {
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(ops_fix::plan_fixes(&lib, guard)?)
    })
    .await?
}

/// Removes the library rows of the named tracks whose audio is gone (only those a fresh scan
/// still finds dead and in no mix; the report names what was skipped). Files are not touched.
/// A fresh snapshot of `music.db` is made first; a restore brings the rows back. Refused while
/// a job runs, and for a library that looks mostly dead unless `acknowledge_mass` is true.
#[tauri::command]
pub async fn device_fix_remove_rows(app: AppHandle, root: String, track_ids: Vec<i64>, acknowledge_mass: bool) -> AppResult<RemoveReport> {
    if track_ids.is_empty() {
        return Err("No tracks to remove.".into());
    }
    if app.state::<JobLock>().is_held() {
        return Err("A job is still running. Remove the rows when it has finished.".into());
    }
    let guard = settings_of(&app).dead_row_guard_percent;
    let a = app.clone();
    let r = root.clone();
    crate::state::quick_write(app, root, move || -> AppResult<RemoveReport> {
        let dir = snapshots_dir(&a, Path::new(&r))?;
        // the session's first-write copy may be hours old: this removal gets its own
        ops_device::snapshot_library(Path::new(&r), &dir, SNAPSHOTS_KEPT)?;
        let mut lib = Library::open(Path::new(&r))?;
        Ok(ops_fix::remove_dead_rows(&mut lib, &track_ids, guard, acknowledge_mass)?)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-trust-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn defaults_and_clamping() {
        let d = TrustSettings::default();
        assert_eq!((d.undo_limit, d.dead_row_guard_percent, d.undo_pill_seconds), (20, 50, 10));
        let c = TrustSettings { undo_limit: 5000, dead_row_guard_percent: 0, undo_pill_seconds: 9999 }.clamped();
        assert_eq!((c.undo_limit, c.dead_row_guard_percent, c.undo_pill_seconds), (MAX_UNDO_LIMIT, 1, 120));
        let c = TrustSettings { undo_limit: 0, dead_row_guard_percent: 400, undo_pill_seconds: 0 }.clamped();
        assert_eq!((c.undo_limit, c.dead_row_guard_percent, c.undo_pill_seconds), (0, 100, 0), "0 is a valid choice: history off, no pill");
    }

    #[test]
    fn settings_round_trip_and_tolerate_a_missing_or_corrupt_file() {
        let dir = temp("settings");
        assert_eq!(load_settings(&dir), (TrustSettings::default(), None), "no file: defaults, no warning");
        let s = TrustSettings { undo_limit: 7, dead_row_guard_percent: 30, undo_pill_seconds: 0 };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), (s.clone(), None));
        assert!(!dir.join("trust-settings.json.tmp").exists(), "no temp file left behind");

        // a file written by an older version with fewer keys: the missing ones default
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"undo_limit": 3}"#).unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!((v.undo_limit, v.dead_row_guard_percent), (3, 50));
        assert!(w.is_none());

        // out-of-range values on disk are clamped on load
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"undo_limit": 99999, "dead_row_guard_percent": 0}"#).unwrap();
        let (v, _) = load_settings(&dir);
        assert_eq!((v.undo_limit, v.dead_row_guard_percent), (MAX_UNDO_LIMIT, 1));

        // a corrupt file: defaults and a warning, and the file is left for the user to see
        std::fs::write(dir.join(SETTINGS_FILE), "not json").unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!(v, TrustSettings::default());
        assert!(w.unwrap().contains("could not be read"));
        assert_eq!(std::fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap(), "not json");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
