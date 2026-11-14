//! Process-wide managed state that more than one area shares. State private to one area is
//! managed from that area's own `setup` in `commands/<area>.rs` instead.
//!
//! Two pieces guard the device volume:
//!
//! * [`JobLock`]: one long background job (add tracks, re-analyse, key detection, import, backup,
//!   firmware copy, clean-up, mix export) at a time.
//! * [`WriteGate`]: every write to the device goes through it, quick or long. See
//!   `ARCHITECTURE.md` ("Writes to the device") for the rules and why there are two.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use pm_formats::ops_device::{self, SNAPSHOTS_KEPT};
use tauri::{App, AppHandle, Manager};

use crate::error::{AppError, AppResult};

/// Only one background job at a time: add and re-analyse both write to the same music.db, and a
/// backup or firmware copy must not overlap them.
#[derive(Default)]
pub struct JobLock(pub AtomicBool);

/// Releases the lock when the job thread ends, including by panic.
pub struct JobGuard<'a>(pub &'a AtomicBool);

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl JobLock {
    /// True if the lock was free and is now taken.
    pub fn try_acquire(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }

    /// True while a job holds the lock.
    pub fn is_held(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// How long Eject / backup / firmware install wait for quick writes that are already running
/// before they give up and say so.
pub const EXCLUSIVE_WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct GateState {
    /// Writes that hold a [`WriteTicket`] right now (quick writes and the body of write jobs).
    active: usize,
    /// What the exclusive operation (eject, backup, firmware install) is doing, while one is
    /// running or waiting to start ("being ejected"): quoted in the refusal a write gets.
    exclusive: Option<&'static str>,
}

#[derive(Default)]
struct GateInner {
    state: Mutex<GateState>,
    changed: Condvar,
    /// Libraries (volume roots) whose `music.db` was already snapshotted in this app session. The
    /// lock is held across the copy, so two first writes do not both copy, and the second waits
    /// for the first one's snapshot.
    snapshotted: Mutex<HashSet<String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // a panicking writer must not wedge the gate for the rest of the session
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Serialises device writes against the operations that must not overlap them.
///
/// Quick writes (rating, case, mix, grid, settings) and the body of write jobs each hold a
/// [`WriteTicket`]; any number can be active at once, SQLite's own lock (with a busy timeout in
/// `Library::open`) orders the individual statements, so a quick write never fails because a long
/// job is running. Eject, backup and firmware install take [`WriteGate::exclusive`]: new writes
/// are refused at once, writes already in flight finish first (bounded wait), and only then does
/// the operation start.
#[derive(Default, Clone)]
pub struct WriteGate(Arc<GateInner>);

/// Held for the duration of one write. Dropping it ends the write.
pub struct WriteTicket(Arc<GateInner>);

impl Drop for WriteTicket {
    fn drop(&mut self) {
        let mut st = lock(&self.0.state);
        st.active = st.active.saturating_sub(1);
        self.0.changed.notify_all();
    }
}

/// Held while an exclusive operation runs. Dropping it lets writes resume.
pub struct ExclusiveGuard(Arc<GateInner>);

impl Drop for ExclusiveGuard {
    fn drop(&mut self) {
        lock(&self.0.state).exclusive = None;
        self.0.changed.notify_all();
    }
}

impl WriteGate {
    /// Starts a write to the library at `root`. Before the first write of the session to that
    /// library, runs `snapshot` (a copy of `music.db` and `settings.dat` made off the device); if
    /// it fails the write is refused and the next attempt tries the snapshot again. Refused while
    /// an exclusive operation runs.
    pub fn begin_write_with(&self, root: &Path, snapshot: impl FnOnce() -> Result<(), String>) -> Result<WriteTicket, String> {
        {
            let mut st = lock(&self.0.state);
            if let Some(what) = st.exclusive {
                return Err(format!("The device is {what}, so nothing can be written to it right now. Try again when that has finished."));
            }
            st.active += 1;
        }
        // from here on the ticket owns the count, so every early return decrements it
        let ticket = WriteTicket(self.0.clone());
        let key = volume_key(root);
        let mut done = lock(&self.0.snapshotted);
        if !done.contains(&key) {
            snapshot().map_err(|e| format!("Nothing was written: the safety copy of music.db failed ({e})."))?;
            done.insert(key);
        }
        Ok(ticket)
    }

    /// [`begin_write_with`](Self::begin_write_with) with the real snapshot into `snap_dir`.
    pub fn begin_write(&self, root: &Path, snap_dir: &Path) -> Result<WriteTicket, String> {
        self.begin_write_with(root, || ops_device::snapshot_library_pinned(root, snap_dir, SNAPSHOTS_KEPT, &[]).map(|_| ()).map_err(|e| e.to_string()))
    }

    /// Takes the gate for an operation that must not overlap any write. New writes are refused
    /// from this moment; writes in flight get up to `wait` to finish, then the call fails with
    /// the gate released again.
    pub fn exclusive(&self, what: &'static str, wait: Duration) -> Result<ExclusiveGuard, String> {
        let mut st = lock(&self.0.state);
        if st.exclusive.is_some() {
            return Err("Another device operation (eject, backup or firmware) is already running.".into());
        }
        st.exclusive = Some(what);
        let guard = ExclusiveGuard(self.0.clone());
        let deadline = Instant::now() + wait;
        while st.active > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                drop(st);
                drop(guard); // clears the flag
                return Err("A write to the library is still in progress. Try again in a moment.".into());
            }
            st = self.0.changed.wait_timeout(st, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        Ok(guard)
    }

    /// Forget that `root` was snapshotted (after an eject, a different device may come back under
    /// the same drive letter, and its first write deserves its own snapshot).
    pub fn forget(&self, root: &Path) {
        lock(&self.0.snapshotted).remove(&volume_key(root));
    }

    #[cfg(test)]
    fn active(&self) -> usize {
        lock(&self.0.state).active
    }
}

pub use pm_formats::appfile::volume_key;

/// The folder holding the automatic snapshots of the library at `root`:
/// `<app data dir>/snapshots/<root made filename-safe>-<hash>`. Never on the device. Making a
/// path filename-safe is lossy (`/mnt/a-b` and `/mnt/a_b` look alike), so a hash of the
/// normalised path keeps two volumes out of one folder (and out of one 10-snapshot cap).
pub fn snapshots_dir(app: &AppHandle, root: &Path) -> AppResult<PathBuf> {
    Ok(app.path().app_local_data_dir()?.join("snapshots").join(snapshot_folder_name(root)))
}

fn snapshot_folder_name(root: &Path) -> String {
    let norm = volume_key(root);
    let readable: String = norm.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let readable = readable.trim_matches('_');
    let readable = if readable.is_empty() { "library" } else { readable };
    // FNV-1a: stable across Rust versions and runs, unlike the std hasher
    format!("{readable}-{:08x}", crate::store::fnv1a(&norm) as u32)
}

/// Starts a write to the library at `root` (snapshot first, once per session; see
/// [`WriteGate::begin_write_with`]). Blocking: call it from a blocking thread. Keep the ticket
/// until the write is finished.
pub fn begin_write(app: &AppHandle, root: &Path) -> AppResult<WriteTicket> {
    let dir = snapshots_dir(app, root)?;
    app.state::<WriteGate>().begin_write(root, &dir).map_err(AppError::from)
}

/// Runs a quick write on a blocking thread under a [`WriteTicket`]: refused while the device is
/// being ejected, backed up or updated, preceded by the session's first-write snapshot, and
/// waited for by Eject. `f` should open the library, change it and return.
pub async fn quick_write<T: Send + 'static>(
    app: AppHandle,
    root: String,
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ticket = begin_write(&app, Path::new(&root))?;
        f()
    })
    .await?
}

/// Takes the gate for backup / firmware, which then run as a job under the job lock (so the
/// lock is not taken here): refused at once while a background job runs, otherwise waits briefly
/// for quick writes to finish (see [`WriteGate::exclusive`]). `what` starts the refusal ("Back
/// up when it has finished"), `busy` is what the device is doing meanwhile ("being backed up").
pub fn take_exclusive(app: &AppHandle, what: &str, busy: &'static str) -> AppResult<ExclusiveGuard> {
    if app.state::<JobLock>().is_held() {
        return Err(format!("A job is still running. {what} when it has finished.").into());
    }
    app.state::<WriteGate>().exclusive(busy, EXCLUSIVE_WAIT).map_err(AppError::from)
}

/// Like [`take_exclusive`] for an operation that is not a job (eject): it takes the job lock
/// itself and keeps it until the returned guard drops, so no job (clean-up, export, add) can
/// start under it, and waits for quick writes. Blocking.
pub fn take_exclusive_locked(app: &AppHandle, what: &str, busy: &'static str) -> AppResult<LockedExclusive> {
    if !app.state::<JobLock>().try_acquire() {
        return Err(format!("A job is still running. {what} when it has finished.").into());
    }
    // from here the job lock is ours; release it on every path
    let job = JobRelease(app.clone());
    let gate = app.state::<WriteGate>().exclusive(busy, EXCLUSIVE_WAIT).map_err(AppError::from)?;
    Ok(LockedExclusive { _gate: gate, _job: job })
}

struct JobRelease(AppHandle);

impl Drop for JobRelease {
    fn drop(&mut self) {
        self.0.state::<JobLock>().0.store(false, Ordering::SeqCst);
    }
}

/// The write gate and the job lock, held together; both are released on drop.
pub struct LockedExclusive {
    _gate: ExclusiveGuard,
    _job: JobRelease,
}

pub fn setup(app: &mut App) {
    app.manage(JobLock::default());
    app.manage(WriteGate::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::AtomicUsize;

    fn root(n: &str) -> PathBuf {
        PathBuf::from(format!("/virtual/{n}"))
    }

    #[test]
    fn snapshot_runs_before_the_first_write_of_each_library_only() {
        let gate = WriteGate::default();
        let calls = Cell::new(0);
        let snap = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let a = gate.begin_write_with(&root("a"), snap).unwrap();
        assert_eq!(calls.get(), 1);
        let b = gate.begin_write_with(&root("a"), snap).unwrap();
        assert_eq!(calls.get(), 1, "second write to the same library: no new snapshot");
        gate.begin_write_with(&root("other"), snap).unwrap();
        assert_eq!(calls.get(), 2, "another library gets its own");
        drop((a, b));
        gate.forget(&root("a"));
        gate.begin_write_with(&root("a"), snap).unwrap();
        assert_eq!(calls.get(), 3, "after forget (eject) the library is snapshotted again");
    }

    #[test]
    fn a_failed_snapshot_blocks_the_write_and_is_retried() {
        let gate = WriteGate::default();
        let err = gate.begin_write_with(&root("a"), || Err("disk full".into())).err().unwrap();
        assert!(err.contains("Nothing was written") && err.contains("disk full"), "{err}");
        assert_eq!(gate.active(), 0, "the refused write holds no ticket");
        let ok = gate.begin_write_with(&root("a"), || Ok(()));
        assert!(ok.is_ok(), "the next attempt snapshots again and goes through");
    }

    #[test]
    fn real_snapshot_holds_the_pre_write_database() {
        let base = std::env::temp_dir().join(format!("pm-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let snaps = base.join("snaps");
        std::fs::create_dir_all(vol.join(".Pacemaker")).unwrap();
        std::fs::write(vol.join(".Pacemaker/music.db"), b"before").unwrap();
        std::fs::write(vol.join(".Pacemaker/settings.dat"), b"settings").unwrap();
        let gate = WriteGate::default();
        let t = gate.begin_write(&vol, &snaps).unwrap();
        // the write the ticket stands for
        std::fs::write(vol.join(".Pacemaker/music.db"), b"after").unwrap();
        drop(t);
        gate.begin_write(&vol, &snaps).unwrap();
        let list = ops_device::list_snapshots(&snaps);
        assert_eq!(list.len(), 1, "one snapshot for the session, not one per write");
        assert_eq!(std::fs::read(Path::new(&list[0].folder).join("music.db")).unwrap(), b"before");
        assert_eq!(std::fs::read(Path::new(&list[0].folder).join("settings.dat")).unwrap(), b"settings");
        // a volume with no library: the snapshot fails and so does the write
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(gate.begin_write(&empty, &snaps).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn many_writes_run_at_once_so_quick_ones_never_wait_on_a_long_job() {
        let gate = WriteGate::default();
        let long_job = gate.begin_write_with(&root("a"), || Ok(())).unwrap();
        for _ in 0..5 {
            gate.begin_write_with(&root("a"), || panic!("no second snapshot")).unwrap();
        }
        assert_eq!(gate.active(), 1, "the quick tickets were dropped; the long job still holds its own");
        drop(long_job);
        assert_eq!(gate.active(), 0);
    }

    #[test]
    fn one_volume_has_one_key_and_one_snapshot_folder_however_it_is_spelled() {
        assert_eq!(volume_key(Path::new("/mnt/usb")), volume_key(Path::new("/mnt/usb/")));
        assert_eq!(volume_key(Path::new("E:")), volume_key(Path::new("E:\\")));
        assert_eq!(snapshot_folder_name(Path::new("E:")), snapshot_folder_name(Path::new("E:\\")));
        assert_ne!(snapshot_folder_name(Path::new("/mnt/a-b")), snapshot_folder_name(Path::new("/mnt/a_b")));
        let gate = WriteGate::default();
        let calls = Cell::new(0);
        let snap = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        drop(gate.begin_write_with(Path::new("E:"), snap).unwrap());
        drop(gate.begin_write_with(Path::new("E:\\"), snap).unwrap());
        assert_eq!(calls.get(), 1, "E: and E:\\ are one library: one snapshot");
        gate.forget(Path::new("E:\\"));
        drop(gate.begin_write_with(Path::new("E:"), snap).unwrap());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn exclusive_refuses_new_writes_and_lets_them_resume() {
        let gate = WriteGate::default();
        let x = gate.exclusive("being ejected", Duration::from_millis(50)).unwrap();
        let err = gate.begin_write_with(&root("a"), || Ok(())).err().unwrap();
        assert!(err.contains("being ejected"), "{err}");
        assert!(gate.exclusive("being ejected", Duration::from_millis(50)).is_err(), "only one exclusive operation");
        drop(x);
        assert!(gate.begin_write_with(&root("a"), || Ok(())).is_ok());
    }

    #[test]
    fn exclusive_waits_for_a_write_in_flight() {
        let gate = WriteGate::default();
        let ticket = gate.begin_write_with(&root("a"), || Ok(())).unwrap();
        let finished = Arc::new(AtomicUsize::new(0));
        let (g2, f2) = (gate.clone(), finished.clone());
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            f2.store(1, Ordering::SeqCst);
            drop(ticket);
            drop(g2);
        });
        let started = Instant::now();
        let x = gate.exclusive("being ejected", Duration::from_secs(5)).unwrap();
        assert_eq!(finished.load(Ordering::SeqCst), 1, "the operation started only after the write ended");
        assert!(started.elapsed() >= Duration::from_millis(100));
        drop(x);
        writer.join().unwrap();
    }

    #[test]
    fn exclusive_gives_up_on_a_stuck_write_and_releases_the_gate() {
        let gate = WriteGate::default();
        let _long = gate.begin_write_with(&root("a"), || Ok(())).unwrap();
        let err = gate.exclusive("being ejected", Duration::from_millis(80)).err().unwrap();
        assert!(err.contains("still in progress"), "{err}");
        // the failed attempt must not leave the gate closed
        assert!(gate.begin_write_with(&root("a"), || Ok(())).is_ok());
    }
}
