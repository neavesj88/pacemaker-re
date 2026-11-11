//! Export: write playlists, mixes and library listings out of the device (M3U, CSV, other libraries). Reads only; writes to a path the user chose.
//!
//! Mix audio (this file): `export_render_preview` renders part of the timeline of the editor's
//! document to a temporary WAV for the player; `export_mix_audio` renders the whole mix to an
//! Ogg Vorbis (like the original Editor) or WAV file as a background job. The engine is the
//! `pm-mixer` crate (`notes/mix-audio.md`); the library lookup, caching and destination checks
//! are in [`render`].
//!
//! Track list (`export_tracklist`, `export_tracklist_save`): the start times of the tracks of a mix
//! as plain text, CSV, YouTube chapters, a Mixcloud-style list and a CUE sheet, computed from the
//! engine's own clip timing ([`tracklist`]). `export_mix_audio` can write the CUE sheet beside the
//! rendered file.
//!
//! To add a command: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `export_commands!` below, then mirror it in `src/lib/api/export.ts` and
//! `src/lib/devmock/export.ts`.

mod render;
pub mod tracklist;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::mixes::doc::MixDoc;
use tracklist::{Format as ListFormat, TracklistOptions, TracklistOut};
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

/// Where the CUE sheet of an audio export goes: beside the audio file, same name, `.cue`.
fn cue_path_for(audio: &Path) -> std::path::PathBuf {
    audio.with_extension("cue")
}

/// What `export_cue_target` tells the UI before an export.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueTarget {
    pub cue_path: String,
    /// A file is already at `cue_path`.
    pub exists: bool,
}

/// The CUE sheet an export to `dest` would write, and whether one is already there. Lets the UI
/// ask before an existing sheet is replaced. Reads nothing but the file's existence.
#[tauri::command]
pub fn export_cue_target(dest: String) -> CueTarget {
    let p = cue_path_for(Path::new(&dest));
    CueTarget { cue_path: p.to_string_lossy().into_owned(), exists: p.exists() }
}

/// What `export_mix_audio` returns as soon as the job has started.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStarted {
    /// The file that will exist when the job reports `job-done` (`mix-export`).
    pub path: String,
    /// The CUE sheet that will be written beside it, when one was asked for.
    pub cue_path: Option<String>,
    /// A file was already at `cue_path` and will be replaced (only when the caller said it may).
    pub cue_replaces: bool,
}

/// Starts rendering the whole mix to `dest` as `format` ("ogg" or "wav") and returns at once; the
/// job reports `job-progress` (`done` / `total` in seconds of audio) and `job-done` with the
/// result. The file appears under its final name only when the render finished. `dest` must not
/// be inside the device volume. Rendering starts at the first audible frame of the mix.
///
/// `cue`: when given, a CUE sheet of the mix is written next to the audio file (same name, `.cue`,
/// replacing one that is there only when `replace_cue` is true, else the call is refused) once the
/// render has finished. It is the sheet the track list dialog
/// shows for the same options; it is built before the render starts, so a mix that cannot have one
/// (more than 99 tracks) is refused up front instead of after minutes of rendering.
#[tauri::command]
pub fn export_mix_audio(app: AppHandle, lock: State<'_, JobLock>, root: String, mix: MixDoc, dest: String, format: String, cue: Option<TracklistOptions>, replace_cue: Option<bool>) -> AppResult<ExportStarted> {
    let plan = render::plan_export(Path::new(&root), &mix, &dest, &format)?;
    let cue_sheet = match &cue {
        Some(opts) => Some(plan_cue(&plan, &mix, opts, replace_cue.unwrap_or(false))?),
        None => None,
    };
    let total = (plan.span_secs().ceil() as usize).max(1);
    let label = if mix.name.trim().is_empty() { "mix".to_string() } else { mix.name.clone() };
    let out_path = plan.dest.to_string_lossy().into_owned();
    let (cue_path, cue_replaces) = match &cue_sheet {
        Some((p, _)) => (Some(p.to_string_lossy().into_owned()), p.exists()),
        None => (None, false),
    };
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
        let mut skipped = result.warnings.len();
        let mut summary = if result.clips_failed > 0 {
            format!("Exported {label} ({}) to {} with {} clip(s) left out, so it is not the complete mix", clock(result.duration_secs), file_name(&plan.dest), result.clips_failed)
        } else {
            format!("Exported {label} ({}) to {}", clock(result.duration_secs), file_name(&plan.dest))
        };
        if let Some((path, bytes)) = &cue_sheet {
            match tracklist::write_atomic(path, bytes) {
                Ok(()) => summary.push_str(&format!(" and {}", file_name(path))),
                Err(e) => {
                    skipped += 1;
                    summary.push_str(&format!(", but the CUE sheet {} could not be written: {e}", file_name(path)));
                }
            }
        }
        if !result.warnings.is_empty() {
            summary.push_str(&format!("; {} problem(s): {}", result.warnings.len(), result.warnings.join("; ")));
        }
        Ok(JobOutcome { ok: 1, failed: skipped, summary })
    })?;
    Ok(ExportStarted { path: out_path, cue_path, cue_replaces })
}

/// The CUE sheet an export writes beside `plan.dest`: its path and bytes. Uses the library audio the
/// render itself will find, so a clip the render leaves out is not in the sheet.
fn plan_cue(plan: &render::ExportPlan, doc: &MixDoc, opts: &TracklistOptions, replace: bool) -> AppResult<(std::path::PathBuf, Vec<u8>)> {
    let path = cue_path_for(&plan.dest);
    if path.is_dir() {
        return Err(format!("Cannot write the CUE sheet: {} is a folder.", file_name(&path)).into());
    }
    if path.exists() && !replace {
        return Err(format!("{} is already there and would be replaced. Confirm the replacement, or turn the CUE sheet off to export the audio only.", file_name(&path)).into());
    }
    let name = file_name(&plan.dest);
    let out = tracklist::render_all(doc, opts, &name, &|id| plan.res.has(id))?;
    if let Some(why) = &out.cue_issue {
        return Err(format!("Cannot write a CUE sheet: {why}. Turn the CUE sheet off to export the audio.").into());
    }
    Ok((path, out.file_bytes(ListFormat::Cue, opts)))
}

/// Which library tracks have audio on the volume, as the render will find it. An unreadable
/// library is not fatal here: the editor's own record of each track decides instead.
fn audio_lookup(root: &Path, doc: &MixDoc) -> Box<dyn Fn(i64) -> bool + Send> {
    let from_doc: std::collections::HashMap<i64, bool> = doc.clips.iter().map(|c| (c.track_id, c.track_id >= 0 && c.track.as_ref().is_none_or(|t| t.available))).collect();
    let resolver = doc.to_mix().ok().and_then(|m| render::LibResolver::open(root, &m).ok());
    match resolver {
        Some(r) => Box::new(move |id| r.has(id)),
        None => Box::new(move |id| from_doc.get(&id).copied().unwrap_or(false)),
    }
}

/// The track list of a mix in every format, from the editor's document (saved or not). `options`
/// are the user's settings (omitted: the defaults); `audio_file_name` is the name of the audio
/// file the CUE sheet should point at (a folder part is ignored). Times are the times in the
/// exported audio file: the first audible frame of the mix is 0:00. Clips the export would leave
/// out because their audio is not on the device are not listed (they are in `warnings`) unless
/// `options.includeUnavailable` says to.
#[tauri::command]
pub async fn export_tracklist(root: String, mix: MixDoc, options: Option<TracklistOptions>, audio_file_name: String) -> AppResult<TracklistOut> {
    let out = tauri::async_runtime::spawn_blocking(move || -> AppResult<TracklistOut> {
        let opts = options.unwrap_or_default();
        let has = audio_lookup(Path::new(&root), &mix);
        Ok(tracklist::render_all(&mix, &opts, &audio_file_name, &*has)?)
    })
    .await??;
    Ok(out)
}

/// Writes one format of the track list to `dest` and returns the path. Same text as
/// `export_tracklist` returns for the same arguments. `format` is txt, csv, youtube, mixcloud or
/// cue; `dest` must end in the format's extension (.txt, .csv, .cue) and be outside the device
/// volume. An existing file is replaced (the save dialog has asked).
#[tauri::command]
pub async fn export_tracklist_save(root: String, mix: MixDoc, options: Option<TracklistOptions>, format: String, dest: String, audio_file_name: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<String> {
        let opts = options.unwrap_or_default();
        let fmt = ListFormat::parse(&format)?;
        let dest = std::path::PathBuf::from(dest);
        render::check_dest_ext(Path::new(&root), &dest, fmt.extension())?;
        let has = audio_lookup(Path::new(&root), &mix);
        let out = tracklist::render_all(&mix, &opts, &audio_file_name, &*has)?;
        if fmt == ListFormat::Cue
            && let Some(why) = &out.cue_issue
        {
            return Err(format!("Cannot write a CUE sheet: {why}.").into());
        }
        tracklist::write_atomic(&dest, &out.file_bytes(fmt, &opts))?;
        Ok(dest.to_string_lossy().into_owned())
    })
    .await?
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
                $crate::commands::export::export_tracklist,
                $crate::commands::export::export_tracklist_save,
                $crate::commands::export::export_cue_target,
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
