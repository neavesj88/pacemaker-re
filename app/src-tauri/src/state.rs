//! Process-wide managed state that more than one area shares. State private to one area is
//! managed from that area's own `setup` in `commands/<area>.rs` instead.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{App, Manager};

/// Only one write job at a time: add and re-analyse both write to the same music.db.
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
}

pub fn setup(app: &mut App) {
    app.manage(JobLock::default());
}
