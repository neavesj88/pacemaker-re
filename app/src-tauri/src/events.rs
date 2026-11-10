//! Event names and the one emit helper. Shared events (jobs) live here; an area with an event of
//! its own declares a `pub const` in its own `commands/<area>.rs` and emits through `emit`; the
//! frontend mirror is `src/lib/api/events.ts`.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const JOB_PROGRESS: &str = "job-progress";
pub const JOB_DONE: &str = "job-done";

/// Fire-and-forget: a window that has gone away is not an error worth failing a job over.
pub fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    let _ = app.emit(event, payload);
}
