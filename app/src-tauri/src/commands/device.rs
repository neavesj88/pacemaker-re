//! Device: storage, health, firmware file install, backup snapshot, clean-up of unreferenced
//! files, settings.dat and eject. The logic lives in `pm_formats::ops_device`; this file is the
//! command layer. Reads are plain commands; anything that copies or deletes many files is a
//! background job (`device-firmware`, `device-backup`, `device-cleanup`) so it shows progress and
//! cannot overlap another write job.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pm_formats::ops_device::{self, FirmwareInfo, Health, PfwInfo, SettingsView, Storage, VocalMode, VocalState};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_job};
use crate::state::{ExclusiveGuard, JobLock};

/// Set by `device_cancel`, polled by the running device job between files / chunks. Cleared when
/// a job starts, so a cancel never leaks into the next job.
#[derive(Default)]
pub struct DeviceCancel(AtomicBool);

pub fn setup(app: &mut tauri::App) {
    app.manage(DeviceCancel::default());
}

fn start_job_cancel(app: &AppHandle) {
    app.state::<DeviceCancel>().0.store(false, Ordering::SeqCst);
}

fn cancelled(app: &AppHandle) -> bool {
    app.state::<DeviceCancel>().0.load(Ordering::SeqCst)
}

/// Asks the running device job (firmware copy, backup, clean-up) to stop at the next file.
#[tauri::command]
pub fn device_cancel(app: AppHandle) {
    app.state::<DeviceCancel>().0.store(true, Ordering::SeqCst);
}

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
pub async fn device_settings_write(app: AppHandle, root: String, changes: Vec<SettingChange>) -> AppResult<usize> {
    // a quick write: settings.dat is a different file from the one a running import writes, so it
    // does not wait for the job, but Eject waits for it
    let path = root.clone();
    crate::state::quick_write(app, root, move || -> AppResult<usize> {
        let pairs: Vec<(i64, f64)> = changes.iter().map(|c| (c.id, c.value)).collect();
        Ok(ops_device::write_settings(Path::new(&path), &pairs)?)
    })
    .await
}

/// The Vocal filter flag on the card (`.Pacemaker/revival-vocal`) and whether the kill switch
/// (`revival-no-preload`) overrides it. `effective` is what the device will play and what mix
/// previews and exports render. Read-only.
#[tauri::command]
pub async fn device_vocal(root: String) -> AppResult<VocalState> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<VocalState> { Ok(ops_device::read_vocal(Path::new(&root))?) }).await?
}

/// Sets the Vocal filter flag: `off` deletes `revival-vocal`, `vocal` writes it empty, `inverted`
/// writes `i`. Takes effect at the device's next boot. The only file touched is that one flag
/// file (its path is fixed in `pm_formats::ops_device`); `revival-no-preload` is never written.
/// Returns the state read back from the card.
#[tauri::command]
pub async fn device_vocal_set(app: AppHandle, root: String, mode: VocalMode) -> AppResult<VocalState> {
    // a card that already says so needs no write: no gate, no first-write snapshot
    let probe = root.clone();
    let current = tauri::async_runtime::spawn_blocking(move || ops_device::read_vocal(Path::new(&probe))).await??;
    if current.mode == mode {
        return Ok(current);
    }
    // a quick write like settings.dat: refused during an eject / backup / firmware install,
    // snapshotted once per session, waited for by Eject
    let path = root.clone();
    crate::state::quick_write(app, root, move || -> AppResult<VocalState> { Ok(ops_device::write_vocal(Path::new(&path), mode)?) }).await
}

/// Checks a `.pfw` (trailer, checksum, the device's version gate). Nothing is written.
#[tauri::command]
pub async fn device_firmware_inspect(path: String) -> AppResult<PfwInfo> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PfwInfo> { Ok(ops_device::inspect_pfw(Path::new(&path))?) }).await?
}

/// Job `device-firmware`: copies a verified `.pfw` into `.Pacemaker` of the volume.
#[tauri::command]
pub async fn device_firmware_install(app: AppHandle, lock: State<'_, JobLock>, root: String, path: String) -> AppResult<()> {
    // waits for (or refuses on) writes in progress, and keeps new ones out while it copies
    let exclusive = exclusive_gate(&app, "Install the firmware", "being updated").await?;
    spawn_job(app, &lock, "device-firmware", move |app| {
        let _exclusive = exclusive;
        start_job_cancel(app);
        let rep = ops_device::install_pfw(
            Path::new(&root),
            Path::new(&path),
            &mut |done, total| {
                let mut p = Progress::new("device-firmware", "Copying the firmware file", done as usize, total as usize, "copying");
                p.detail = Some(format!("{done} of {total} MB"));
                events::emit(app, JOB_PROGRESS, p);
            },
            &|| cancelled(app),
        )?;
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
pub async fn device_backup(app: AppHandle, lock: State<'_, JobLock>, root: String, dest: String, include_audio: bool) -> AppResult<()> {
    // music.db must not change under the copy: new writes are held off until the backup ends
    let exclusive = exclusive_gate(&app, "Back up", "being backed up").await?;
    spawn_job(app, &lock, "device-backup", move |app| {
        let _exclusive = exclusive;
        start_job_cancel(app);
        let rep = ops_device::backup_snapshot(
            Path::new(&root),
            Path::new(&dest),
            include_audio,
            &mut |done, total, label| {
                events::emit(app, JOB_PROGRESS, Progress::new("device-backup", label, done, total, "copied"));
            },
            &|| cancelled(app),
        )?;
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
        // deleting audio is a device write: refused during an eject, backup or firmware copy, and
        // the first write of the session gets its safety copy
        let _ticket = crate::state::begin_write(app, Path::new(&root))?;
        start_job_cancel(app);
        let rep = ops_device::delete_unreferenced(
            Path::new(&root),
            &files,
            &mut |done, total, label| {
                if !label.is_empty() {
                    events::emit(app, JOB_PROGRESS, Progress::new("device-cleanup", label, done, total, "deleting"));
                }
            },
            &|| cancelled(app),
        )?;
        let mut parts = vec![format!("Deleted {} file{} ({})", rep.deleted, if rep.deleted == 1 { "" } else { "s" }, mb(rep.bytes))];
        if !rep.skipped.is_empty() {
            parts.push(format!("{} left alone because they are no longer unused", rep.skipped.len()));
        }
        if rep.cancelled {
            parts.push("stopped early at your request; the rest were not touched".to_string());
        }
        if !rep.failed.is_empty() {
            parts.push(format!("{} could not be deleted: {}", rep.failed.len(), rep.failed.join("; ")));
        }
        Ok(JobOutcome { ok: rep.deleted, failed: rep.failed.len(), summary: parts.join(", ") })
    })
}

/// Asks the OS to eject the volume. True once the library has disappeared from it.
#[tauri::command]
pub async fn device_eject(app: AppHandle, root: String) -> AppResult<bool> {
    // ejecting under a running copy or a quick write would cut it off half-written: refuse while
    // a job runs, wait briefly for quick writes, and hold new writes off until the eject returns
    tauri::async_runtime::spawn_blocking(move || -> AppResult<bool> {
        // the job lock too: a clean-up or export that starts under the eject would read or delete
        // files while the volume goes away
        let _exclusive = crate::state::take_exclusive_locked(&app, "Eject", "being ejected")?;
        let ejected = ops_device::eject(Path::new(&root))?;
        if ejected {
            // a different device may come back under this drive letter: it gets its own snapshot
            app.state::<crate::state::WriteGate>().forget(Path::new(&root));
        }
        Ok(ejected)
    })
    .await?
}

/// Takes the write gate for an operation that must not overlap any device write (the wait for
/// quick writes in flight blocks, so it runs off the async threads).
async fn exclusive_gate(app: &AppHandle, what: &'static str, busy: &'static str) -> AppResult<ExclusiveGuard> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || crate::state::take_exclusive(&app, what, busy)).await?
}

/// Where the automatic `music.db` snapshots of this library go and what is in there now.
#[derive(Serialize)]
pub struct SnapshotsView {
    /// The folder (never on the device); it may not exist yet before the first write.
    folder: String,
    /// How many snapshots are kept per library.
    keep: usize,
    /// Newest first.
    snapshots: Vec<ops_device::SnapshotEntry>,
}

/// The automatic snapshot folder of the library on `root`. Read-only.
#[tauri::command]
pub async fn device_snapshots(app: AppHandle, root: String) -> AppResult<SnapshotsView> {
    let dir = crate::state::snapshots_dir(&app, Path::new(&root))?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<SnapshotsView> {
        Ok(SnapshotsView { folder: dir.display().to_string(), keep: ops_device::SNAPSHOTS_KEPT, snapshots: ops_device::list_snapshots(&dir) })
    })
    .await?
}

/// Opens the snapshot folder in the system file manager.
#[tauri::command]
pub async fn device_snapshots_open(app: AppHandle, root: String) -> AppResult<()> {
    let dir = crate::state::snapshots_dir(&app, Path::new(&root))?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        std::fs::create_dir_all(&dir)?;
        open_folder(&dir)?;
        Ok(())
    })
    .await?
}

#[cfg(windows)]
fn open_folder(dir: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer").arg(dir.to_string_lossy().replace('/', "\\")).spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_folder(dir: &Path) -> std::io::Result<()> {
    std::process::Command::new("open").arg(dir).spawn().map(|_| ())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn open_folder(dir: &Path) -> std::io::Result<()> {
    std::process::Command::new("xdg-open").arg(dir).spawn().map(|_| ())
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
                $crate::commands::device::device_vocal,
                $crate::commands::device::device_vocal_set,
                $crate::commands::device::device_firmware_inspect,
                $crate::commands::device::device_firmware_install,
                $crate::commands::device::device_backup,
                $crate::commands::device::device_cleanup,
                $crate::commands::device::device_eject,
                $crate::commands::device::device_cancel,
                $crate::commands::device::device_snapshots,
                $crate::commands::device::device_snapshots_open,
            ];
            $($rest),*
        }
    };
}
