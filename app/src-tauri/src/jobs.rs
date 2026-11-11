//! Background jobs for any area: the `job-progress` / `job-done` payloads and `spawn_job`.
//! Generic on purpose: a new kind of job (key detection, sync, export) needs no change here,
//! it passes its own `job` string and returns a `JobOutcome`. The wire shape is mirrored in
//! `src/lib/api/types.ts`; changing a field is an integration step.

use std::panic::{AssertUnwindSafe, catch_unwind};

use pm_analysis::ingest::{Event, Outcome, Summary};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::events::{self, JOB_DONE};
use crate::state::{JobGuard, JobLock};

/// One `job-progress` event. `job` is the kind of job ("add", "reanalyse", or any string a new
/// area picks); `status` is free-form per kind, with "failed" meaning the item failed (the UI
/// lists those). `detail` carries optional extra text for the strip.
#[derive(Serialize, Clone)]
pub struct Progress {
    pub job: String,
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub status: String,
    pub bpm: Option<f64>,
    pub detail: Option<String>,
    pub error: Option<String>,
}

impl Progress {
    pub fn new(job: &str, label: impl Into<String>, done: usize, total: usize, status: &str) -> Self {
        Progress { job: job.into(), label: label.into(), done, total, status: status.into(), bpm: None, detail: None, error: None }
    }
}

/// What a job reports when it ends: items that went through, items that failed, and the line
/// shown to the user (already worded; the UI does not interpret it).
pub struct JobOutcome {
    pub ok: usize,
    pub failed: usize,
    pub summary: String,
}

impl From<Summary> for JobOutcome {
    fn from(s: Summary) -> Self {
        let mut parts = Vec::new();
        if s.added > 0 {
            parts.push(format!("{} added", s.added));
        }
        if s.reanalysed > 0 {
            parts.push(format!("{} analysed", s.reanalysed));
        }
        if s.skipped > 0 {
            parts.push(format!("{} already on the device", s.skipped));
        }
        if s.failed > 0 {
            parts.push(format!("{} failed", s.failed));
        }
        let summary = if parts.is_empty() { "nothing to do".to_string() } else { parts.join(", ") };
        JobOutcome { ok: s.added + s.reanalysed + s.skipped, failed: s.failed, summary }
    }
}

/// The final `job-done` event.
#[derive(Serialize, Clone)]
pub struct JobDone {
    pub job: String,
    pub ok: usize,
    pub failed: usize,
    pub summary: String,
    /// Set when the job could not run at all (database refused, thread crashed).
    pub error: Option<String>,
}

/// Maps an ingest event to a progress payload.
pub fn progress_of(job: &str, ev: &Event, done: usize, total: usize) -> Progress {
    let mut p = Progress::new(job, ev.label.clone(), done, total, "");
    match &ev.outcome {
        Outcome::Added { bpm, .. } => {
            p.status = "added".into();
            p.bpm = Some(*bpm);
        }
        Outcome::Skipped { .. } => p.status = "skipped".into(),
        Outcome::Reanalysed { bpm, .. } => {
            p.status = "reanalysed".into();
            p.bpm = Some(*bpm);
        }
        Outcome::Failed(e) => {
            p.status = "failed".into();
            p.error = Some(e.clone());
        }
    }
    p
}

fn finish(app: &AppHandle, job: &str, res: Result<JobOutcome, AppError>) {
    let done = match res {
        Ok(o) => JobDone { job: job.into(), ok: o.ok, failed: o.failed, summary: o.summary, error: None },
        Err(e) => JobDone { job: job.into(), ok: 0, failed: 0, summary: String::new(), error: Some(e.to_string()) },
    };
    events::emit(app, JOB_DONE, done);
}

/// Runs `work` on a background thread holding the job lock, then emits `job-done`. Progress
/// events (`Progress`, on `events::JOB_PROGRESS`) are emitted from inside `work`. Fails
/// immediately if another job is running.
pub fn spawn_job<F>(app: AppHandle, lock: &JobLock, job: &str, work: F) -> AppResult<()>
where
    F: FnOnce(&AppHandle) -> AppResult<JobOutcome> + Send + 'static,
{
    if !lock.try_acquire() {
        return Err("Another job is still running.".into());
    }
    let job = job.to_string();
    // the lock lives in managed state for the whole app run; re-fetch it inside the thread
    std::thread::spawn(move || {
        let state = app.state::<JobLock>();
        let _guard = JobGuard(&state.0);
        let res = catch_unwind(AssertUnwindSafe(|| work(&app))).unwrap_or_else(|_| Err("the job crashed".into()));
        finish(&app, &job, res);
    });
    Ok(())
}

/// [`spawn_job`] for a job that writes to the library at `root`: the job body runs under a
/// `state::WriteTicket`, after the session's first-write snapshot of `music.db`. If the snapshot
/// fails or the device is being ejected / backed up, the job ends with that error and writes
/// nothing. Use this for every job that changes the device; `spawn_job` stays for jobs that only
/// read it or write elsewhere (mix export).
pub fn spawn_write_job<F>(app: AppHandle, lock: &JobLock, job: &str, root: String, work: F) -> AppResult<()>
where
    F: FnOnce(&AppHandle) -> AppResult<JobOutcome> + Send + 'static,
{
    spawn_job(app, lock, job, move |app| {
        let _ticket = crate::state::begin_write(app, std::path::Path::new(&root))?;
        work(app)
    })
}
