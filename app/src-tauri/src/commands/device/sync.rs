//! Library sync: merge the tracks, mixes and cases of one library into another, in either
//! direction (a library on this computer to the Pacemaker, or back). The engine is
//! `pm_formats::ops_sync`; this file is the command layer: the settings (app-side file, never on
//! a device), the preview, and the run as a device write job.
//!
//! The preview is read-only (both libraries are opened read-only). The run is a write job on the
//! TARGET like the clean-up: it holds the job lock and a `WriteTicket` (refused during an eject
//! or backup, and the session's first write gets its safety copy), saves a fresh snapshot of the
//! target's `music.db` before the first change, and runs exactly the plan the user saw: the
//! digest of the plan is checked, and the engine plans again and refuses if anything moved. The
//! source is only read. The job kind starts with "device-", so the device store shows the
//! progress and Cancel (`device_cancel`) stops it between items.
//!
//! There is no lock on the source: nothing in the app takes a read lock on a library. Eject takes
//! the job lock, so a device cannot be ejected under a running sync.

use std::path::Path;

use pm_formats::library::Library;
use pm_formats::ops_device::{self, SNAPSHOTS_KEPT};
use pm_formats::ops_sync::{self, ItemKind, Outcome, SyncAction, SyncPlan, SyncReport, SyncSettings};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_write_job, JobOutcome, Progress};
use crate::state::{snapshots_dir, JobLock};
use crate::store::config_dir;

/// The job kind of a sync run ("device-" so the device store shows it and Cancel works).
pub const JOB_SYNC: &str = "device-sync";

// ---- settings -------------------------------------------------------------------------------

/// What the user can tune: the engine's choices plus whether labels follow copied tracks. Stored
/// as JSON in the app's config dir; a missing or unreadable file means the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncPrefs {
    #[serde(flatten)]
    pub sync: SyncSettings,
    /// Tags, colour labels and energy of a copied track follow it to the other library.
    pub labels_follow: bool,
}

impl Default for SyncPrefs {
    fn default() -> Self {
        SyncPrefs { sync: SyncSettings::default(), labels_follow: true }
    }
}

const SETTINGS_FILE: &str = "sync.json";

/// Reads the settings file. A missing file is the defaults; an unreadable one is the defaults
/// plus a warning (the file is left as it is until the user saves new settings).
pub fn load_settings(dir: &Path) -> (SyncPrefs, Option<String>) {
    let p = dir.join(SETTINGS_FILE);
    match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<SyncPrefs>(&s) {
            Ok(v) => (v, None),
            Err(e) => (SyncPrefs::default(), Some(format!("{} could not be read ({e}); the defaults are in use", p.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (SyncPrefs::default(), None),
        Err(e) => (SyncPrefs::default(), Some(format!("{}: {e}; the defaults are in use", p.display()))),
    }
}

pub fn save_settings(dir: &Path, s: &SyncPrefs) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    crate::store::write_atomic(&dir.join(SETTINGS_FILE), text.as_bytes())
}

#[derive(Serialize)]
pub struct SyncSettingsView {
    pub settings: SyncPrefs,
    /// Set when the settings file could not be read and the defaults are showing.
    pub warning: Option<String>,
}

/// The sync settings.
#[tauri::command]
pub fn sync_settings_get(app: AppHandle) -> SyncSettingsView {
    match config_dir(&app) {
        Ok(dir) => {
            let (settings, warning) = load_settings(&dir);
            SyncSettingsView { settings, warning }
        }
        Err(e) => SyncSettingsView { settings: SyncPrefs::default(), warning: Some(format!("the config folder is unavailable ({e}); the defaults are in use")) },
    }
}

/// Saves the settings and returns what is in force.
#[tauri::command]
pub fn sync_settings_set(app: AppHandle, settings: SyncPrefs) -> AppResult<SyncSettingsView> {
    save_settings(&config_dir(&app)?, &settings)?;
    Ok(SyncSettingsView { settings, warning: None })
}

// ---- labels of the two ends -----------------------------------------------------------------

/// "Pacemaker" for a volume root (a drive root such as `G:\`, or a mount point under the usual
/// removable-media folders), otherwise "computer". Names the copies "keep both" makes
/// ("<name> (from computer)") and the words of the preview.
pub fn label_of(root: &Path) -> &'static str {
    // one rule for "Pacemaker or computer" in the whole app (library/kind.rs)
    if crate::commands::library::kind::library_kind(&root.to_string_lossy()) == crate::commands::library::kind::PACEMAKER { "Pacemaker" } else { "computer" }
}

// ---- plan -----------------------------------------------------------------------------------

/// What a sync of `source` into `target` would do. Read-only; the free space is the target
/// volume's. The result's `digest` goes back with [`sync_apply`].
#[tauri::command]
pub async fn sync_plan(app: AppHandle, source: String, target: String) -> AppResult<SyncPlan> {
    let (prefs, _) = load_settings(&config_dir(&app)?);
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SyncPlan> { plan_now(&source, &target, &prefs.sync) }).await?
}

fn plan_now(source: &str, target: &str, settings: &SyncSettings) -> AppResult<SyncPlan> {
    let src = Library::open_read_only(Path::new(source))?;
    let dst = Library::open_read_only(Path::new(target))?;
    Ok(ops_sync::plan(&src, &dst, settings, label_of(Path::new(source)))?)
}

// ---- run ------------------------------------------------------------------------------------

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// The sentence a finished run is reported with.
pub fn summarise(rep: &SyncReport) -> String {
    let count = |kind: ItemKind, f: fn(&SyncAction) -> bool| rep.items.iter().filter(|i| i.kind == kind && i.outcome == Outcome::Done && f(&i.action)).count();
    let copied = |a: &SyncAction| matches!(a, SyncAction::Copy | SyncAction::CopyAsNew { .. });
    let mut parts: Vec<String> = Vec::new();
    let (tc, mc, cc) = (count(ItemKind::Track, copied), count(ItemKind::Mix, copied), count(ItemKind::Case, copied));
    if tc + mc + cc > 0 {
        let mut what = Vec::new();
        if tc > 0 {
            what.push(format!("{} ({})", plural(tc, "track"), super::mb(rep.bytes_copied)));
        }
        if mc > 0 {
            what.push(plural(mc, "mix").replace("mixs", "mixes"));
        }
        if cc > 0 {
            what.push(plural(cc, "case"));
        }
        parts.push(format!("Copied {}", what.join(", ")));
    }
    let n = count(ItemKind::Track, |a| *a == SyncAction::Update);
    if n > 0 {
        parts.push(format!("Updated {}", plural(n, "track")));
    }
    let n = rep.items.iter().filter(|i| i.outcome == Outcome::Done && i.action == SyncAction::UpdatePlays).count();
    if n > 0 {
        parts.push(format!("Updated the play counts of {}", plural(n, "item")));
    }
    let n = count(ItemKind::Mix, |a| *a == SyncAction::Replace);
    if n > 0 {
        parts.push(format!("Replaced {}", plural(n, "mix").replace("mixs", "mixes")));
    }
    let n = count(ItemKind::Case, |a| *a == SyncAction::Replace);
    if n > 0 {
        parts.push(format!("Replaced {}", plural(n, "case")));
    }
    let n = count(ItemKind::Case, |a| *a == SyncAction::Merge);
    if n > 0 {
        parts.push(format!("Merged {}", plural(n, "case")));
    }
    let removed = rep.items.iter().filter(|i| i.outcome == Outcome::Done && i.action == SyncAction::Remove).count();
    if removed > 0 {
        parts.push(format!("Removed {removed} items that the other side does not have"));
    }
    if rep.cancelled {
        parts.push("Stopped at your request; the rest were not touched".into());
    }
    let left: Vec<String> = rep
        .items
        .iter()
        .filter_map(|i| match &i.outcome {
            // what the user is owed an explanation for: not the many "already the same"
            Outcome::Skipped { reason } if (i.action.acts() && !reason.starts_with("cancelled")) || reason.starts_with("it needs") || reason == "left out by you" => {
                Some(format!("{}: {reason}", i.label))
            }
            _ => None,
        })
        .collect();
    if !left.is_empty() {
        parts.push(format!("{} left out: {}", plural(left.len(), "item"), left.join("; ")));
    }
    let failed: Vec<String> = rep
        .items
        .iter()
        .filter_map(|i| match &i.outcome {
            Outcome::Failed { error } => Some(format!("{}: {error}", i.label)),
            _ => None,
        })
        .collect();
    if !failed.is_empty() {
        parts.push(format!("{} could not be synced: {}", plural(failed.len(), "item"), failed.join("; ")));
    }
    if parts.is_empty() {
        parts.push("Nothing to sync, the two libraries already match".into());
    }
    parts.join(". ")
}

/// Job `device-sync`: runs the plan the user saw. `digest` is the plan's digest, `excluded` the
/// keys of the items the user unticked. If either library changed since the preview, nothing is
/// written and the job ends asking for a new preview. The target's `music.db` is snapshotted
/// first (Restore from a snapshot undoes the row changes; copied files stay and show up under
/// unused files). Stopped with `device_cancel`: finished items stay finished.
#[tauri::command]
pub fn sync_apply(app: AppHandle, lock: State<'_, JobLock>, source: String, target: String, digest: String, excluded: Vec<String>) -> AppResult<()> {
    let (prefs, _) = load_settings(&config_dir(&app)?);
    let target_root = target.clone();
    spawn_write_job(app, &lock, JOB_SYNC, target_root, move |app| {
        super::start_job_cancel(app);
        let src = Library::open_read_only(Path::new(&source))?;
        let mut dst = Library::open(Path::new(&target))?;
        let label = label_of(Path::new(&source));
        let seen = ops_sync::plan(&src, &dst, &prefs.sync, label)?;
        if seen.digest != digest {
            return Err(format!("{}: the libraries changed since the preview, so nothing was synced. Look at the preview again.", ops_sync::STALE).into());
        }
        // the session's first-write copy may be hours old: this run gets its own
        ops_device::snapshot_library(Path::new(&target), &snapshots_dir(app, Path::new(&target))?, SNAPSHOTS_KEPT)?;
        let rep = ops_sync::apply(
            &src,
            &mut dst,
            &seen,
            &excluded,
            &mut |p| {
                let mut ev = Progress::new(JOB_SYNC, p.current.clone(), p.done, p.total, "syncing");
                ev.detail = Some(format!("{} of {}", super::mb(p.bytes_done), super::mb(p.bytes_total)));
                events::emit(app, JOB_PROGRESS, ev);
            },
            &|| super::cancelled(app),
        )?;
        let mut summary = summarise(&rep);
        if prefs.labels_follow {
            let ids: Vec<String> = rep
                .items
                .iter()
                .filter(|i| i.kind == ItemKind::Track && i.action == SyncAction::Copy && i.outcome == Outcome::Done)
                .filter_map(|i| i.global_id.clone())
                .collect();
            if !ids.is_empty() {
                let dir = config_dir(app)?;
                let vk = |p: &str| crate::state::volume_key(Path::new(p));
                match ops_sync::copy_labels(&dir, &vk(&source), &vk(&target), &ids) {
                    Ok(0) => {}
                    Ok(n) => summary.push_str(&format!(". Tags and labels came with {}", plural(n, "track"))),
                    Err(e) => summary.push_str(&format!(". The tags and labels did not come along ({e})")),
                }
            }
        }
        Ok(JobOutcome { ok: rep.done, failed: rep.failed, summary })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::ops_sync::{ItemResult, SyncReport};

    fn done(kind: ItemKind, action: SyncAction, label: &str) -> ItemResult {
        ItemResult { key: label.into(), kind, label: label.into(), action, global_id: None, outcome: Outcome::Done }
    }

    #[test]
    fn defaults_follow_the_documented_choices() {
        let d = SyncPrefs::default();
        assert!(d.labels_follow && !d.sync.remove_missing);
        assert_eq!(d.sync, SyncSettings::default());
    }

    #[test]
    fn settings_round_trip_and_tolerate_a_missing_or_corrupt_file() {
        let dir = std::env::temp_dir().join(format!("pm-app-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load_settings(&dir), (SyncPrefs::default(), None));
        let mut s = SyncPrefs { labels_follow: false, ..SyncPrefs::default() };
        s.sync.remove_missing = true;
        s.sync.mixes_same_name = pm_formats::ops_sync::MixesSameName::KeepBoth;
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), (s.clone(), None));
        assert!(!dir.join("sync.json.tmp").exists());
        // the flat wire format the frontend sends
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["mixes_same_name"], "keep_both");
        assert_eq!(json["labels_follow"], false);
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"labels_follow": false}"#).unwrap();
        let (v, w) = load_settings(&dir);
        assert!(!v.labels_follow && v.sync == SyncSettings::default() && w.is_none(), "missing keys default");
        std::fs::write(dir.join(SETTINGS_FILE), "not json").unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!(v, SyncPrefs::default());
        assert!(w.unwrap().contains("could not be read"));
        assert_eq!(std::fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap(), "not json", "a bad file is left as it is");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn labels_say_which_end_is_which() {
        if cfg!(windows) {
            assert_eq!(label_of(Path::new("G:\\")), "Pacemaker");
            assert_eq!(label_of(Path::new("G:")), "Pacemaker");
            assert_eq!(label_of(Path::new("C:\\Users\\Example\\Music\\Pacemaker Revival")), "computer");
        } else {
            assert_eq!(label_of(Path::new("/media/example/PACEMAKER")), "Pacemaker");
            assert_eq!(label_of(Path::new("/Volumes/PACEMAKER")), "Pacemaker");
            assert_eq!(label_of(Path::new("/home/example/Music/Pacemaker Revival")), "computer");
            assert_eq!(label_of(Path::new("/media/example/PACEMAKER/Sub")), "computer");
        }
    }

    #[test]
    fn the_summary_names_what_happened() {
        let mut rep = SyncReport { items: vec![], cancelled: false, bytes_copied: 5 << 20, done: 0, skipped: 0, failed: 0 };
        assert_eq!(summarise(&rep), "Nothing to sync, the two libraries already match");
        rep.items = vec![
            done(ItemKind::Track, SyncAction::Copy, "A - One"),
            done(ItemKind::Track, SyncAction::Copy, "B - Two"),
            done(ItemKind::Track, SyncAction::Update, "C - Three"),
            done(ItemKind::Mix, SyncAction::Copy, "Set"),
            done(ItemKind::Case, SyncAction::UpdatePlays, "Crate"),
            done(ItemKind::Mix, SyncAction::Replace, "Old set"),
            done(ItemKind::Case, SyncAction::Merge, "Crate"),
            ItemResult { outcome: Outcome::Failed { error: "disk full".into() }, ..done(ItemKind::Track, SyncAction::Copy, "D - Four") },
            ItemResult { outcome: Outcome::Skipped { reason: "it needs \"X\"".into() }, ..done(ItemKind::Mix, SyncAction::Copy, "Other set") },
        ];
        rep.cancelled = true;
        let s = summarise(&rep);
        assert!(s.starts_with("Copied 2 tracks (5.0 MB), 1 mix"), "{s}");
        assert!(s.contains("Updated the play counts of 1 item"), "{s}");
        assert!(s.contains("Updated 1 track") && s.contains("Replaced 1 mix") && s.contains("Merged 1 case"), "{s}");
        assert!(s.contains("Stopped at your request"), "{s}");
        assert!(s.contains("1 item left out: Other set: it needs \"X\""), "{s}");
        assert!(s.contains("1 item could not be synced: D - Four: disk full"), "{s}");
    }
}
