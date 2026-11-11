//! Export: write playlists, mixes and library listings out of the device (M3U, CSV, other libraries). Reads only; writes to a path the user chose.
//!
//! Mix audio (this file): `export_render_preview` renders part of the timeline of the editor's
//! document to a temporary WAV for the player; `export_mix_audio` renders the whole mix to an
//! Ogg Vorbis (like the original Editor) or WAV file as a background job. The engine is the
//! `pm-mixer` crate (`notes/mix-audio.md`); the library lookup, caching and destination checks
//! are in [`render`].
//!
//! To add a command: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `export_commands!` below, then mirror it in `src/lib/api/export.ts` and
//! `src/lib/devmock/export.ts`.

mod render;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::mixes::doc::MixDoc;
use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

pub use render::RenderedAudio;

/// The `job` name of mix exports in `job-progress` / `job-done` events.
pub const JOB_MIX_EXPORT: &str = "mix-export";

/// Set by `export_cancel`, polled by the running mix export between render blocks. Cleared when
/// an export starts, so a cancel never leaks into the next one.
#[derive(Default)]
pub struct ExportCancel(AtomicBool);

pub fn setup(app: &mut tauri::App) {
    app.manage(ExportCancel::default());
}

/// Renders `[from_sec, to_sec)` of the mix timeline (seconds) to a temporary WAV and returns its
/// path, which the player can stream (the file is added to the asset scope here). `mix` is the
/// editor's document; it does not need to be saved. Clips whose audio is missing are left out and
/// listed in `warnings`. At most 15 minutes per preview.
#[tauri::command]
pub async fn export_render_preview(app: AppHandle, root: String, mix: MixDoc, from_sec: f64, to_sec: f64) -> AppResult<RenderedAudio> {
    let out = tauri::async_runtime::spawn_blocking(move || -> AppResult<RenderedAudio> {
        let dest = render::next_preview_path()?;
        render::render_preview(Path::new(&root), &mix, from_sec, to_sec, &dest)
    })
    .await??;
    // the player streams the file over the asset protocol, whose scope starts empty
    let _ = app.asset_protocol_scope().allow_file(&out.path);
    Ok(out)
}

/// What `export_mix_audio` returns as soon as the job has started.
#[derive(Debug, Clone, Serialize)]
pub struct ExportStarted {
    /// The file that will exist when the job reports `job-done` (`mix-export`).
    pub path: String,
}

/// Starts rendering the whole mix to `dest` as `format` ("ogg" or "wav") and returns at once; the
/// job reports `job-progress` (`done` / `total` in seconds of audio) and `job-done` with the
/// result. The file appears under its final name only when the render finished. `dest` must not
/// be inside the device volume. Rendering starts at the first audible frame of the mix.
#[tauri::command]
pub fn export_mix_audio(app: AppHandle, lock: State<'_, JobLock>, root: String, mix: MixDoc, dest: String, format: String) -> AppResult<ExportStarted> {
    let plan = render::plan_export(Path::new(&root), &mix, &dest, &format)?;
    let total = (plan.span_secs().ceil() as usize).max(1);
    let label = if mix.name.trim().is_empty() { "mix".to_string() } else { mix.name.clone() };
    let out_path = plan.dest.to_string_lossy().into_owned();
    spawn_job(app, &lock, JOB_MIX_EXPORT, move |app| {
        let mut last = Instant::now() - Duration::from_secs(1);
        let cancel = &app.state::<ExportCancel>().0;
        // cleared only once this call owns the job lock: a rejected second call must not
        // un-cancel the export that is already running
        cancel.store(false, Ordering::SeqCst);
        let rendered = render::run_export(&plan, &mut |p| {
            if last.elapsed() >= Duration::from_millis(250) {
                last = Instant::now();
                let done = (p * total as f64) as usize;
                events::emit(app, JOB_PROGRESS, Progress::new(JOB_MIX_EXPORT, label.clone(), done.min(total), total, "rendering"));
            }
            keep_going(cancel)
        });
        // the engine stops at the next block and removes its `.partial` file: no file is left
        // only the engine's own cancel error counts: any other failure after a cancel request is a failure
        let was_cancelled = matches!(&rendered, Err(e) if e.to_string() == pm_mixer::Error::Cancelled.to_string());
        if was_cancelled {
            return Ok(JobOutcome { ok: 0, failed: 0, summary: format!("Export of {label} was cancelled. No file was written.") });
        }
        let result = rendered?;
        // the throttle can swallow the last update, so the bar always ends full
        events::emit(app, JOB_PROGRESS, Progress::new(JOB_MIX_EXPORT, label.clone(), total, total, "rendering"));
        let skipped = result.warnings.len();
        let mut summary = if result.clips_failed > 0 {
            format!("Exported {label} ({}) to {} with {} clip(s) left out, so it is not the complete mix", clock(result.duration_secs), file_name(&plan.dest), result.clips_failed)
        } else {
            format!("Exported {label} ({}) to {}", clock(result.duration_secs), file_name(&plan.dest))
        };
        if skipped > 0 {
            summary.push_str(&format!("; {skipped} problem(s): {}", result.warnings.join("; ")));
        }
        Ok(JobOutcome { ok: 1, failed: skipped, summary })
    })?;
    Ok(ExportStarted { path: out_path })
}

/// The value the render progress callback returns: false (stop) once a cancel was asked for.
fn keep_going(cancel: &AtomicBool) -> bool {
    !cancel.load(Ordering::SeqCst)
}

/// Asks the running mix export (`export_mix_audio`) to stop. The render ends at its next block
/// and no file is left behind; the job then reports `job-done` with a "cancelled" summary. Does
/// nothing when no export is running.
#[tauri::command]
pub fn export_cancel(app: AppHandle) {
    app.state::<ExportCancel>().0.store(true, Ordering::SeqCst);
}

fn clock(secs: f64) -> String {
    let s = secs.round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// This area's command paths, one per line, written `$crate::commands::export::name,`.
#[macro_export]
macro_rules! export_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::export::export_render_preview,
                $crate::commands::export::export_mix_audio,
                $crate::commands::export::export_cancel,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_flag_turns_into_a_stop_for_the_engine() {
        let flag = AtomicBool::new(false);
        assert!(keep_going(&flag));
        flag.store(true, Ordering::SeqCst);
        assert!(!keep_going(&flag), "the progress callback returns false, which the engine reads as cancel");
    }

    #[test]
    fn clock_formats_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(75.4), "1:15");
        assert_eq!(clock(3600.0), "60:00");
    }
}
