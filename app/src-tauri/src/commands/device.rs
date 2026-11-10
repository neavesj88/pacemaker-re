//! Device: storage, health, firmware file install, backup snapshot, clean-up of unreferenced
//! files, settings.dat and eject. The logic lives in `pm_formats::ops_device`; this file is the
//! command layer. Reads are plain commands; anything that copies or deletes many files is a
//! background job (`device-firmware`, `device-backup`, `device-cleanup`) so it shows progress and
//! cannot overlap another write job.

use std::path::{Path, PathBuf};

use pm_formats::ops_device::{self, FirmwareInfo, Health, PfwInfo, SettingsView, Storage};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_job};
use crate::state::JobLock;

pub fn setup(_app: &mut tauri::App) {}

fn mb(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    }
}

#[derive(Serialize)]
pub struct DeviceOverview {
    storage: Storage,
    firmware: FirmwareInfo,
}

/// Storage use and the versions in `system.xml` (serial masked). Read-only.
#[tauri::command]
pub async fn device_overview(root: String) -> AppResult<DeviceOverview> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<DeviceOverview> {
        let root = PathBuf::from(root);
        Ok(DeviceOverview { storage: ops_device::storage(&root)?, firmware: ops_device::firmware_info(&root) })
    })
    .await?
}

/// Compatibility problems, tracks with missing audio and files nothing uses. Read-only.
#[tauri::command]
pub async fn device_health(root: String) -> AppResult<Health> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Health> { Ok(ops_device::health(Path::new(&root))?) }).await?
}

/// The device settings, or null when the device has not written `settings.dat` yet.
#[tauri::command]
pub async fn device_settings(root: String) -> AppResult<Option<SettingsView>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Option<SettingsView>> {
        Ok(ops_device::read_settings(Path::new(&root))?)
    })
    .await?
}

#[derive(Deserialize)]
pub struct SettingChange {
    id: i64,
    value: f64,
}

/// Writes the given setting changes to `settings.dat`. Returns how many settings changed.
/// The UI asks for confirmation first.
#[tauri::command]
pub async fn device_settings_write(root: String, changes: Vec<SettingChange>) -> AppResult<usize> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<usize> {
        let pairs: Vec<(i64, f64)> = changes.iter().map(|c| (c.id, c.value)).collect();
        Ok(ops_device::write_settings(Path::new(&root), &pairs)?)
    })
    .await?
}

/// Checks a `.pfw` (trailer, checksum, the device's version gate). Nothing is written.
#[tauri::command]
pub async fn device_firmware_inspect(path: String) -> AppResult<PfwInfo> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PfwInfo> { Ok(ops_device::inspect_pfw(Path::new(&path))?) }).await?
}

/// Job `device-firmware`: copies a verified `.pfw` into `.Pacemaker` of the volume.
#[tauri::command]
pub fn device_firmware_install(app: AppHandle, lock: State<'_, JobLock>, root: String, path: String) -> AppResult<()> {
    spawn_job(app, &lock, "device-firmware", move |app| {
        let rep = ops_device::install_pfw(Path::new(&root), Path::new(&path), &mut |done, total| {
            let mut p = Progress::new("device-firmware", "Copying the firmware file", done as usize, total as usize, "copying");
            p.detail = Some(format!("{done} of {total} MB"));
            events::emit(app, JOB_PROGRESS, p);
        })?;
        let same = if rep.same_version_as_installed {
            " Its firmware number equals the installed one, so the device will flash nothing."
        } else {
            ""
        };
        Ok(JobOutcome { ok: 1, failed: 0, summary: format!("Copied {} ({}) to .Pacemaker.{same}", rep.dest_name, mb(rep.size)) })
    })
}

/// Job `device-backup`: a snapshot of the library data (and audio only when `include_audio`)
/// into a new folder inside `dest`, with a SHA-256 manifest. Reads the device only.
#[tauri::command]
pub fn device_backup(app: AppHandle, lock: State<'_, JobLock>, root: String, dest: String, include_audio: bool) -> AppResult<()> {
    spawn_job(app, &lock, "device-backup", move |app| {
        let rep = ops_device::backup_snapshot(Path::new(&root), Path::new(&dest), include_audio, &mut |done, total, label| {
            events::emit(app, JOB_PROGRESS, Progress::new("device-backup", label, done, total, "copied"));
        })?;
        Ok(JobOutcome {
            ok: rep.files,
            failed: 0,
            summary: format!(
                "Backed up {} files ({}) to {}. Every file was read back and checked; checksums are in {}.",
                rep.files,
                mb(rep.bytes),
                rep.folder,
                ops_device::MANIFEST_NAME
            ),
        })
    })
}

/// Job `device-cleanup`: deletes the named files under `.Pacemaker/Music`, each only if a fresh
/// scan still finds it unreferenced. The UI lists every file and asks first.
#[tauri::command]
pub fn device_cleanup(app: AppHandle, lock: State<'_, JobLock>, root: String, files: Vec<String>) -> AppResult<()> {
    if files.is_empty() {
        return Err("No files to delete.".into());
    }
    spawn_job(app, &lock, "device-cleanup", move |app| {
        let rep = ops_device::delete_unreferenced(Path::new(&root), &files, &mut |done, total, label| {
            if !label.is_empty() {
                events::emit(app, JOB_PROGRESS, Progress::new("device-cleanup", label, done, total, "deleting"));
            }
        })?;
        let mut parts = vec![format!("Deleted {} file{} ({})", rep.deleted, if rep.deleted == 1 { "" } else { "s" }, mb(rep.bytes))];
        if !rep.skipped.is_empty() {
            parts.push(format!("{} left alone because they are no longer unused", rep.skipped.len()));
        }
        if !rep.failed.is_empty() {
            parts.push(format!("{} could not be deleted: {}", rep.failed.len(), rep.failed.join("; ")));
        }
        Ok(JobOutcome { ok: rep.deleted, failed: rep.failed.len(), summary: parts.join(", ") })
    })
}

/// Asks the OS to eject the volume. True once the library has disappeared from it.
#[tauri::command]
pub async fn device_eject(root: String) -> AppResult<bool> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<bool> { Ok(ops_device::eject(Path::new(&root))?) }).await?
}

/// This area's command paths, one per line, written `$crate::commands::device::name,`.
#[macro_export]
macro_rules! device_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::device::device_overview,
                $crate::commands::device::device_health,
                $crate::commands::device::device_settings,
                $crate::commands::device::device_settings_write,
                $crate::commands::device::device_firmware_inspect,
                $crate::commands::device::device_firmware_install,
                $crate::commands::device::device_backup,
                $crate::commands::device::device_cleanup,
                $crate::commands::device::device_eject,
            ];
            $($rest),*
        }
    };
}
