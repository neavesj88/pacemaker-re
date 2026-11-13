//! The free-space planner: suggest what to archive or remove from the card to reach a target,
//! export the chosen audio to a PC folder (a verified copy) and/or remove it from the card. The
//! logic is in `pm_formats::ops_space`; this file is the command layer (write gate, job lock,
//! events) and the user-adjustable settings of the planner.
//!
//! Every write goes through the app's write safety. Archiving only reads the card (job lock
//! only). Removing is a device-deleting job like the clean-up: the job holds a `WriteTicket`
//! (refused during an eject or a backup, gets the session snapshot), a fresh snapshot of
//! `music.db` is made just before the rows go, and the run repeats the same safety check the
//! preview shows on the database as it is at that moment.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pm_formats::library::Library;
use pm_formats::ops_device::{self, SNAPSHOTS_KEPT};
use pm_formats::ops_space::{self, Goal, Plan, Rules, RunOptions, RunPlan, GIB};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::store::config_dir;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_job};
use crate::state::{JobLock, snapshots_dir};

/// The job kind of an archive / removal run. It starts with "device-", so the device store shows
/// its progress and the Cancel button works.
pub const JOB_SPACE: &str = "device-space";

// ---- settings -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    /// Have this much free space.
    #[default]
    Free,
    /// Make the library fit a card of this size.
    Card,
}

/// Everything the planner lets the user choose. Stored as JSON in the app's config dir (never on
/// the device); a missing or unreadable file means the defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SpaceSettings {
    pub rules: Rules,
    pub goal_kind: GoalKind,
    /// Free-space target, in GB as the Storage card shows them (1 GB = 2^30 bytes).
    pub free_gb: f64,
    /// The card to fit, in GB as it is sold (1 GB = 10^9 bytes).
    pub card_gb: f64,
    /// Share of that card to keep free, in percent.
    pub reserve_percent: u32,
    /// The PC folder the last archive went to.
    pub archive_dir: String,
    /// Archive each track's beat grid (.str) beside its audio.
    pub include_grids: bool,
    /// Removing from the card needs an archive copy of the same tracks first.
    pub require_archive: bool,
}

impl Default for SpaceSettings {
    fn default() -> Self {
        SpaceSettings {
            rules: Rules::default(),
            goal_kind: GoalKind::Free,
            free_gb: 10.0,
            card_gb: 64.0,
            reserve_percent: 5,
            archive_dir: String::new(),
            include_grids: true,
            require_archive: true,
        }
    }
}

impl SpaceSettings {
    /// Puts every value in its allowed range.
    pub fn clamped(mut self) -> Self {
        self.rules = self.rules.clamped();
        let fin = |v: f64, lo: f64, hi: f64, dflt: f64| if v.is_finite() { v.clamp(lo, hi) } else { dflt };
        self.free_gb = fin(self.free_gb, 0.0, 100_000.0, 10.0);
        self.card_gb = fin(self.card_gb, 0.1, 100_000.0, 64.0);
        self.reserve_percent = self.reserve_percent.min(50);
        self.archive_dir = self.archive_dir.trim().to_string();
        self
    }

    /// The target as bytes.
    pub fn goal(&self) -> Goal {
        match self.goal_kind {
            GoalKind::Free => Goal::Free { bytes: (self.free_gb * GIB as f64) as u64 },
            GoalKind::Card => Goal::Card { bytes: (self.card_gb * 1e9) as u64, reserve_percent: self.reserve_percent },
        }
    }
}

const SETTINGS_FILE: &str = "space-planner.json";

/// Reads the settings file. A missing file is the defaults; an unreadable one is the defaults
/// plus a warning (the file is left as it is until the user saves new settings).
pub fn load_settings(dir: &Path) -> (SpaceSettings, Option<String>) {
    let p = dir.join(SETTINGS_FILE);
    match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<SpaceSettings>(&s) {
            Ok(v) => (v.clamped(), None),
            Err(e) => (SpaceSettings::default(), Some(format!("{} could not be read ({e}); the defaults are in use", p.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (SpaceSettings::default(), None),
        Err(e) => (SpaceSettings::default(), Some(format!("{}: {e}; the defaults are in use", p.display()))),
    }
}

/// Writes the settings atomically (see `crate::store::write_atomic`).
pub fn save_settings(dir: &Path, s: &SpaceSettings) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    crate::store::write_atomic(&dir.join(SETTINGS_FILE), text.as_bytes())
}

#[derive(Serialize)]
pub struct SettingsView {
    pub settings: SpaceSettings,
    /// Set when the settings file could not be read and the defaults are showing.
    pub warning: Option<String>,
}

/// The planner's settings.
#[tauri::command]
pub fn device_space_settings(app: AppHandle) -> SettingsView {
    match config_dir(&app) {
        Ok(dir) => {
            let (settings, warning) = load_settings(&dir);
            SettingsView { settings, warning }
        }
        Err(e) => SettingsView { settings: SpaceSettings::default(), warning: Some(format!("the config folder is unavailable ({e}); the defaults are in use")) },
    }
}

/// Saves the settings (values are clamped to their range) and returns what is in force.
#[tauri::command]
pub fn device_space_settings_set(app: AppHandle, settings: SpaceSettings) -> AppResult<SettingsView> {
    let settings = settings.clamped();
    save_settings(&config_dir(&app)?, &settings)?;
    Ok(SettingsView { settings, warning: None })
}

// ---- plan and preview -----------------------------------------------------------------------

/// Counts the plan requests; a plan whose number is no longer the latest is stopped.
static PLAN_GENERATION: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The tracks the rules nominate for the target, with reasons and sizes. Read-only. Looking for
/// duplicates reads the audio files that share a size with another file, so it can take a while
/// on a big card.
#[tauri::command]
pub async fn device_space_plan(root: String, settings: SpaceSettings) -> AppResult<Plan> {
    let settings = settings.clamped();
    // a newer request (a setting was edited) stops this one's file reading instead of letting two
    // scans run side by side; the stopped one returns an error the frontend discards
    let mine = PLAN_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Plan> {
        let root = Path::new(&root);
        let storage = ops_device::storage(root)?;
        let lib = Library::open_read_only(root)?;
        Ok(ops_space::plan_cancellable(&lib, &settings.rules, &settings.goal(), &storage, now_ms(), &|| PLAN_GENERATION.load(Ordering::SeqCst) != mine)?)
    })
    .await?
}

fn options(archive_dir: Option<String>, include_grids: bool, remove: bool) -> RunOptions {
    RunOptions {
        archive_dir: archive_dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()).map(PathBuf::from),
        include_grids,
        remove,
    }
}

/// What a run would do with each ticked track, by name: the archive file, whether it is removed
/// and why not. Read-only. The run computes the same thing again before it acts.
#[tauri::command]
pub async fn device_space_run_preview(
    root: String,
    track_ids: Vec<i64>,
    archive_dir: Option<String>,
    include_grids: bool,
    remove: bool,
) -> AppResult<RunPlan> {
    let opts = options(archive_dir, include_grids, remove);
    tauri::async_runtime::spawn_blocking(move || -> AppResult<RunPlan> {
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(ops_space::plan_run(&lib, &track_ids, &opts)?)
    })
    .await?
}

fn mb(bytes: u64) -> String {
    ops_space::size_text(bytes)
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// What a run is asked to do (the frontend sends this object).
#[derive(Debug, Clone, Deserialize)]
pub struct RunArgs {
    /// The PC folder for the archive, or none for no archive.
    pub archive_dir: Option<String>,
    pub include_grids: bool,
    /// Remove the tracks from the card.
    pub remove: bool,
    /// The user's setting: removing without an archive is refused when it is on.
    pub require_archive: bool,
}

/// Refuses a run that asks for nothing, or that removes without an archive while the user's
/// "archive before removing" setting is on.
fn check_run_request(opts: &RunOptions, require_archive: bool) -> Result<(), String> {
    if opts.archive_dir.is_none() && !opts.remove {
        return Err("Choose an archive folder, remove from the card, or both.".into());
    }
    if opts.remove && opts.archive_dir.is_none() && require_archive {
        return Err("Removing needs an archive copy first. Choose an archive folder, or turn off \"Archive before removing\" in the planner's settings.".into());
    }
    Ok(())
}

/// Job `device-space`: archives the ticked tracks to `archive_dir` (each file read back and
/// checked), then removes those that may go (not in a mix, and archived when an archive was
/// asked for) from the card: rows first, then audio and beat grid. `require_archive` is the
/// user's setting: with it on, removing without an archive is refused. Stopped with
/// `device_cancel`; a stop removes nothing.
#[tauri::command]
pub fn device_space_run(app: AppHandle, lock: State<'_, JobLock>, root: String, track_ids: Vec<i64>, args: RunArgs) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err("No tracks are ticked.".into());
    }
    let opts = options(args.archive_dir, args.include_grids, args.remove);
    // the setting is read from the stored file too, so it holds for any caller: the frontend's
    // flag can only make the rule stricter, never switch a stored "on" off. (A corrupt settings
    // file reads as the defaults, which have it on.)
    let stored = load_settings(&config_dir(&app)?).0.require_archive;
    check_run_request(&opts, stored || args.require_archive)?;
    spawn_job(app, &lock, JOB_SPACE, move |app| {
        super::start_job_cancel(app);
        // removing is a device write: refused during an eject, backup or firmware copy, and the
        // first write of the session gets its safety copy. Archiving alone only reads the card.
        let _ticket = if opts.remove { Some(crate::state::begin_write(app, Path::new(&root))?) } else { None };
        let snap_dir = snapshots_dir(app, Path::new(&root))?;
        let rep = ops_space::run(
            Path::new(&root),
            &track_ids,
            &opts,
            // the session's first-write copy may be hours old: this removal gets its own
            &|| ops_device::snapshot_library(Path::new(&root), &snap_dir, SNAPSHOTS_KEPT).map(|_| ()),
            &mut |done, total, label| {
                if !label.is_empty() {
                    let status = if label.starts_with("Removing") { "removing" } else { "archiving" };
                    events::emit(app, JOB_PROGRESS, Progress::new(JOB_SPACE, label, done, total, status));
                }
            },
            &|| super::cancelled(app),
        )?;
        let mut parts: Vec<String> = Vec::new();
        if let Some(f) = &rep.archive_folder {
            parts.push(format!("Archived {} ({}) to {f}. Every file was read back and checked; checksums are in {}", plural(rep.archived, "track"), mb(rep.archived_bytes), ops_device::MANIFEST_NAME));
        }
        if rep.removed > 0 {
            parts.push(format!("Removed {} from the card, freeing {}", plural(rep.removed, "track"), mb(rep.freed_bytes)));
        }
        if rep.cancelled {
            parts.push("Stopped at your request; nothing was removed".into());
        }
        if !rep.skipped.is_empty() {
            parts.push(format!("{} left alone: {}", plural(rep.skipped.len(), "item"), rep.skipped.join("; ")));
        }
        if !rep.failed.is_empty() {
            parts.push(format!("{} could not be completed: {}", plural(rep.failed.len(), "item"), rep.failed.join("; ")));
        }
        if parts.is_empty() {
            parts.push("Nothing was archived or removed".into());
        }
        Ok(JobOutcome { ok: rep.archived.max(rep.removed), failed: rep.failed.len(), summary: parts.join(". ") })
    })
}

/// Opens an existing folder in the system file manager (the archive folder after a run).
#[tauri::command]
pub async fn device_space_open_folder(path: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let dir = PathBuf::from(&path);
        if !dir.is_dir() {
            return Err(format!("{path} is not a folder.").into());
        }
        super::open_folder(&dir)?;
        Ok(())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-space-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn defaults_are_cautious_and_the_goal_is_in_bytes() {
        let d = SpaceSettings::default();
        assert!(d.require_archive && d.include_grids && d.rules.protect_favourites);
        assert_eq!(d.goal(), Goal::Free { bytes: 10 * GIB });
        let card = SpaceSettings { goal_kind: GoalKind::Card, card_gb: 32.0, reserve_percent: 10, ..SpaceSettings::default() };
        assert_eq!(card.goal(), Goal::Card { bytes: 32_000_000_000, reserve_percent: 10 });
    }

    #[test]
    fn values_are_clamped_and_junk_numbers_fall_back() {
        let c = SpaceSettings { free_gb: f64::NAN, card_gb: -3.0, reserve_percent: 900, archive_dir: "  C:\\Archive  ".into(), ..SpaceSettings::default() }.clamped();
        assert_eq!((c.free_gb, c.card_gb, c.reserve_percent), (10.0, 0.1, 50));
        assert_eq!(c.archive_dir, "C:\\Archive");
        let c = SpaceSettings { free_gb: 1e12, rules: Rules { stale_months: 9999, ..Rules::default() }, ..SpaceSettings::default() }.clamped();
        assert_eq!((c.free_gb, c.rules.stale_months), (100_000.0, 240));
    }

    #[test]
    fn settings_round_trip_and_tolerate_a_missing_or_corrupt_file() {
        let dir = temp("settings");
        assert_eq!(load_settings(&dir), (SpaceSettings::default(), None), "no file: defaults, no warning");
        let s = SpaceSettings { free_gb: 25.5, require_archive: false, rules: Rules { stale_months: 3, never_played: false, ..Rules::default() }, ..SpaceSettings::default() };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), (s.clone(), None));
        assert!(!dir.join("space-planner.json.tmp").exists(), "no temp file left behind");

        // a file written by an older version with fewer keys: the missing ones default, including
        // the ones nested inside the rules
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"free_gb": 4, "rules": {"stale_months": 2}}"#).unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!((v.free_gb, v.rules.stale_months, v.rules.never_played, v.require_archive), (4.0, 2, true, true));
        assert!(w.is_none());

        // out-of-range values on disk are clamped on load
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"reserve_percent": 99, "rules": {"min_reasons": 0}}"#).unwrap();
        let (v, _) = load_settings(&dir);
        assert_eq!((v.reserve_percent, v.rules.min_reasons), (50, 1));

        // a corrupt file: defaults and a warning, and the file is left for the user to see
        std::fs::write(dir.join(SETTINGS_FILE), "not json").unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!(v, SpaceSettings::default());
        assert!(w.unwrap().contains("could not be read"));
        assert_eq!(std::fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap(), "not json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_wire_format_is_what_the_frontend_sends() {
        let json = serde_json::to_value(SpaceSettings::default()).unwrap();
        assert_eq!(json["goal_kind"], "free");
        assert_eq!(json["rules"]["order"], "certain");
        let back: SpaceSettings = serde_json::from_value(json).unwrap();
        assert_eq!(back, SpaceSettings::default());
        let s: SpaceSettings = serde_json::from_str(r#"{"goal_kind":"card","rules":{"order":"largest"}}"#).unwrap();
        assert_eq!((s.goal_kind, s.rules.order), (GoalKind::Card, ops_space::Order::Largest));
    }

    #[test]
    fn run_arguments_arrive_as_the_frontend_sends_them() {
        let a: RunArgs = serde_json::from_str(r#"{"archive_dir":null,"include_grids":true,"remove":true,"require_archive":false}"#).unwrap();
        assert!(a.archive_dir.is_none() && a.include_grids && a.remove && !a.require_archive);
        let a: RunArgs = serde_json::from_str(r#"{"archive_dir":"/x/archive","include_grids":false,"remove":false,"require_archive":true}"#).unwrap();
        assert_eq!(a.archive_dir.as_deref(), Some("/x/archive"));
        assert!(serde_json::from_str::<RunArgs>(r#"{"remove":true}"#).is_err(), "a missing field is an error, not a silent default");
    }

    #[test]
    fn removing_without_an_archive_is_refused_while_the_setting_is_on() {
        let remove_only = options(None, true, true);
        assert!(check_run_request(&remove_only, true).unwrap_err().contains("archive copy first"));
        assert!(check_run_request(&remove_only, false).is_ok(), "the user turned the setting off");
        assert!(check_run_request(&options(Some("/x/a".into()), true, true), true).is_ok(), "with an archive the setting is met");
        assert!(check_run_request(&options(None, true, false), false).unwrap_err().contains("Choose an archive folder"));
        // the stored setting wins over a caller that sends false
        let dir = temp("enforce");
        save_settings(&dir, &SpaceSettings { require_archive: true, ..SpaceSettings::default() }).unwrap();
        let stored = load_settings(&dir).0.require_archive;
        assert!(check_run_request(&remove_only, stored || false).is_err());
        save_settings(&dir, &SpaceSettings { require_archive: false, ..SpaceSettings::default() }).unwrap();
        let stored = load_settings(&dir).0.require_archive;
        assert!(check_run_request(&remove_only, stored || false).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn options_ignore_a_blank_archive_folder() {
        assert!(options(Some("   ".into()), true, true).archive_dir.is_none());
        assert_eq!(options(Some(" /x/y ".into()), false, false).archive_dir, Some(PathBuf::from("/x/y")));
        assert!(options(None, true, false).archive_dir.is_none());
    }
}
