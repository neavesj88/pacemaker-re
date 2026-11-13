//! Export: write playlists, mixes and library listings out of the device (M3U, CSV, other libraries). Reads only; writes to a path the user chose.
//!
//! Mix audio (this file): `export_render_preview` renders part of the timeline of the editor's
//! document to a temporary WAV for the player; `export_mix_audio` renders the whole mix to an
//! Ogg Vorbis (like the original Editor) or WAV file as a background job. The engine is the
//! `pm-mixer` crate (`notes/mix-audio.md`); the library lookup, caching and destination checks
//! are in [`render`]. FLAC and MP3 are written too, with tags (title, artist, album, year, comment,
//! cover, the track list as chapters); the format settings and the tags are in [`encoding`].
//!
//! Track list (`export_tracklist`, `export_tracklist_save`): the start times of the tracks of a mix
//! as plain text, CSV, YouTube chapters, a Mixcloud-style list and a CUE sheet, computed from the
//! engine's own clip timing ([`tracklist`]). `export_mix_audio` can write the CUE sheet beside the
//! rendered file.
//!
//! Printable setlist (case or mix to a print-ready HTML page): the notes store and the save-to-file
//! command are in [`setlist`]; the page itself is built by the UI.
//!
//! VirtualDJ list (a case to a VirtualDJ List file, optionally with copies of the audio, in a
//! folder outside the device): [`vdj`]. Why that format: `notes/onelibrary-export.md`.
//!
//! To add a command: write a `#[tauri::command] pub async fn` here, add its full
//! path to the list in `export_commands!` below, then mirror it in `src/lib/api/export.ts` and
//! `src/lib/devmock/export.ts`.

pub mod encoding;
pub mod loudness;
pub mod range;
mod render;
pub mod tracklist;
pub mod setlist;
pub mod vdj;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::mixes::doc::MixDoc;
use encoding::{EncodingSpec, TagSpec};
use loudness::{LevelSettings, PreviewGains, Reading};
use range::{RangeInfo, RangePrefs, RangeSpec};
use tracklist::{Format as ListFormat, TracklistOptions, TracklistOut};
use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

pub use render::RenderedAudio;

/// The `job` name of mix exports in `job-progress` / `job-done` events.
pub const JOB_MIX_EXPORT: &str = "mix-export";
/// The `job` name of the loudness analysis.
pub const JOB_MIX_LOUDNESS: &str = "mix-loudness";
/// Event with a [`LoudnessReport`]: sent when an analysis finishes and again when an export has
/// measured the finished audio.
pub const MIX_LOUDNESS_REPORT: &str = "mix-loudness-report";

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
///
/// `gains`: the clip gains and master gain an analysis (`export_loudness_analyse`) found; the
/// preview then plays at the level the export will have. Omitted = no loudness processing.
/// `edge_fade_ms`: fade both ends of the window, as a partial export does at its cuts (previewing a
/// range); omitted = no fade, so plain playback windows join seamlessly.
#[tauri::command]
pub async fn export_render_preview(app: AppHandle, root: String, mix: MixDoc, from_sec: f64, to_sec: f64, gains: Option<PreviewGains>, edge_fade_ms: Option<f64>) -> AppResult<RenderedAudio> {
    let out = tauri::async_runtime::spawn_blocking(move || -> AppResult<RenderedAudio> {
        let dest = render::next_preview_path()?;
        render::render_preview_with(Path::new(&root), &mix, from_sec, to_sec, &dest, gains.as_ref(), edge_fade_ms)
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
    /// A file is already at `dest` itself (the audio the export would replace).
    pub audio_exists: bool,
}

/// The CUE sheet an export to `dest` would write, and whether one is already there; also whether
/// `dest` itself exists. Lets the UI ask before an existing file is replaced. Reads nothing but
/// existence.
#[tauri::command]
pub fn export_cue_target(dest: String) -> CueTarget {
    let p = cue_path_for(Path::new(&dest));
    CueTarget { cue_path: p.to_string_lossy().into_owned(), exists: p.exists(), audio_exists: Path::new(&dest).exists() }
}

/// Whether the cover image the export dialog remembers can still be used: `None` when it can, else
/// the sentence the export would refuse with (the same `encoding::read_cover` runs). Reads the
/// file (8 MB at most) and changes nothing.
#[tauri::command]
pub async fn export_cover_check(path: String) -> AppResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || encoding::read_cover(&path).err().map(|e| e.to_string())).await.map_err(|e| e.to_string().into())
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

/// Starts rendering the mix (the whole mix, or the `range`) to `dest` as `format` ("ogg", "wav",
/// "flac" or "mp3") and returns at once; the job reports `job-progress` (`done` / `total` in seconds of audio) and `job-done` with the
/// result. The file appears under its final name only when the render finished. `dest` must not
/// be inside the device volume. A whole-mix render starts at the first audible frame of the mix.
///
/// `range` (default: the whole mix) picks a time range or "clip A to clip B" (`range_prefs`: where
/// a clip range ends, how long the fade at a cut is); `levels` (default: nothing) is the loudness
/// setting: a target loudness and/or per-clip levelling. With levels active the job first measures
/// (the first half of the progress), then renders with the gains found, exactly the gains
/// `export_loudness_analyse` reports and `export_render_preview` can play. When the render has
/// finished the loudness of the file is sent as a `mix-loudness-report` event and put in the
/// summary.
///
/// `encoding` (default: 192 kbit/s MP3, 16-bit FLAC): `{ mp3: { kind: "cbr", kbps } | { kind: "vbr",
/// quality }, flacBits: 16 | 24, flacLevel }`, used by the format it belongs to. `tags` (default:
/// none; Ogg then gets the mix name as its title): the texts, an optional cover image file (JPEG or
/// PNG, up to 8 MB) and, when `chapters` carries track list options, the tracks of the mix as
/// chapters of the file, from the same list the CUE sheet uses. All of it is checked before the job
/// starts. A format that cannot hold part of it (a cover in a WAV) says so in the summary.
///
/// `cue`: when given, a CUE sheet of the mix is written next to the audio file (same name, `.cue`,
/// replacing one that is there only when `replace_cue` is true, else the call is refused) once the
/// render has finished. It is the sheet the track list dialog
/// shows for the same options; it is built before the render starts, so a mix that cannot have one
/// (more than 99 tracks) is refused up front instead of after minutes of rendering.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn export_mix_audio(
    app: AppHandle,
    lock: State<'_, JobLock>,
    root: String,
    mix: MixDoc,
    dest: String,
    format: String,
    cue: Option<TracklistOptions>,
    replace_cue: Option<bool>,
    range: Option<RangeSpec>,
    range_prefs: Option<RangePrefs>,
    levels: Option<LevelSettings>,
    encoding: Option<EncodingSpec>,
    tags: Option<TagSpec>,
) -> AppResult<ExportStarted> {
    let request = render::ExportRequest {
        range: range.unwrap_or_default(),
        range_prefs: range_prefs.unwrap_or_default(),
        levels: levels.unwrap_or_default(),
        encoding: encoding.unwrap_or_default(),
        tags,
    };
    let plan = render::plan_export_with(Path::new(&root), &mix, &dest, &format, &request)?;
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
        let measuring_first = plan.levels.active();
        let rendered = render::run_export_full(&plan, &mut |p| {
            if last.elapsed() >= Duration::from_millis(250) {
                last = Instant::now();
                let done = (p * total as f64) as usize;
                let status = if measuring_first && p < 0.5 { "measuring" } else { "rendering" };
                events::emit(app, JOB_PROGRESS, Progress::new(JOB_MIX_EXPORT, label.clone(), done.min(total), total, status));
            }
            keep_going(cancel)
        });
        // the engine stops at the next block and removes its `.partial` file: no file is left
        // only the engine's own cancel error counts: any other failure after a cancel request is a failure
        let was_cancelled = matches!(&rendered, Err(e) if e.to_string() == pm_mixer::Error::Cancelled.to_string());
        if was_cancelled {
            return Ok(JobOutcome { ok: 0, failed: 0, summary: format!("Export of {label} was cancelled. No file was written.") });
        }
        let exported = rendered?;
        let result = exported.audio;
        let loudness_report = exported.report;
        // the throttle can swallow the last update, so the bar always ends full
        events::emit(app, JOB_PROGRESS, Progress::new(JOB_MIX_EXPORT, label.clone(), total, total, "rendering"));
        let mut skipped = result.warnings.len();
        let mut summary = if result.clips_failed > 0 {
            format!("Exported {label} ({}) to {} with {} clip(s) left out, so it is not the complete mix", clock(result.duration_secs), file_name(&plan.dest), result.clips_failed)
        } else {
            format!("Exported {label} ({}) to {}", clock(result.duration_secs), file_name(&plan.dest))
        };
        if let Some(r) = &loudness_report.result {
            summary.push_str(&format!(", {}", reading_text(r)));
        }
        if let Some(t) = &plan.encoding.tags {
            if !t.chapters.is_empty() {
                summary.push_str(&format!(", {} chapter(s)", t.chapters.len()));
            }
            for n in encoding::format_notes(plan.format, t) {
                summary.push_str(&format!(". {n}"));
            }
        }
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
        for n in &loudness_report.notes {
            summary.push_str(&format!(" {n}"));
        }
        events::emit(app, MIX_LOUDNESS_REPORT, loudness_report);
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
    let out = tracklist::render_all_in(doc, opts, &name, &|id| plan.res.has(id), plan.span())?;
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

/// What an export of `range` covers (the numbers the export uses), for the dialog: the whole mix
/// when `range` is whole. Cheap and read-only; no audio is touched.
#[tauri::command]
pub fn export_range_resolve(mix: MixDoc, range: RangeSpec, range_prefs: Option<RangePrefs>) -> AppResult<RangeInfo> {
    Ok(range::describe(&mix, &range, &range_prefs.unwrap_or_default())?)
}

/// Starts a loudness analysis of the mix (or the `range`) and returns at once. It renders without
/// writing any file: if `levels` has per-clip levelling on, every clip heard in the range is measured
/// alone, then the mix is rendered with the clip gains and measured before and after the limiter.
/// The result is one `mix-loudness-report` event (a [`LoudnessReport`] with `source` "analysis":
/// the gains an export with the same arguments applies, and `gains`, which `export_render_preview`
/// takes to play at that level) followed by `job-done` for the job `mix-loudness`. Progress is
/// `job-progress` (`done` / `total` in seconds of audio). Cancel with `export_cancel`.
#[tauri::command]
pub fn export_loudness_analyse(app: AppHandle, lock: State<'_, JobLock>, root: String, mix: MixDoc, range: Option<RangeSpec>, range_prefs: Option<RangePrefs>, levels: Option<LevelSettings>) -> AppResult<()> {
    let levels = levels.unwrap_or_default();
    levels.validate()?;
    let prefs = range_prefs.unwrap_or_default();
    let spec = range.unwrap_or_default();
    let resolved = range::resolve(&mix, &spec, &prefs)?;
    let parsed = mix.to_mix()?;
    let res = render::LibResolver::open(Path::new(&root), &parsed)?;
    let peq = render::peq_mode(Path::new(&root));
    let (from, to) = pm_mixer::engine::export_span(&parsed).ok_or("The mix has no audible clips.")?;
    let rate = f64::from(pm_mixer::RATE);
    let total = (resolved.as_ref().map_or((to - from) as f64 / rate, |r| (r.to.min(to) - r.from).max(0) as f64 / rate).ceil() as usize).max(1);
    let label = if mix.name.trim().is_empty() { "mix".to_string() } else { mix.name.clone() };
    spawn_job(app, &lock, JOB_MIX_LOUDNESS, move |app| {
        let mut last = Instant::now() - Duration::from_secs(1);
        let cancel = &app.state::<ExportCancel>().0;
        cancel.store(false, Ordering::SeqCst);
        let subject = loudness::Subject { mix: &parsed, doc: &mix, res: &res, peq, range: resolved.as_ref() };
        let report = loudness::analyse(&subject, &levels, "analysis", &mut |p| {
            if last.elapsed() >= Duration::from_millis(250) {
                last = Instant::now();
                events::emit(app, JOB_PROGRESS, Progress::new(JOB_MIX_LOUDNESS, label.clone(), ((p * total as f64) as usize).min(total), total, "measuring"));
            }
            keep_going(cancel)
        });
        let report = match report {
            Ok(r) => r,
            Err(pm_mixer::Error::Cancelled) => return Ok(JobOutcome { ok: 0, failed: 0, summary: format!("Measuring {label} was cancelled.") }),
            Err(e) => return Err(e.to_string().into()),
        };
        events::emit(app, JOB_PROGRESS, Progress::new(JOB_MIX_LOUDNESS, label.clone(), total, total, "measuring"));
        let summary = match &report.before_limiter {
            Some(r) => format!("Measured {label}: {} before the limiter", reading_text(r)),
            None => format!("Measured {label}"),
        };
        events::emit(app, MIX_LOUDNESS_REPORT, report);
        Ok(JobOutcome { ok: 1, failed: 0, summary })
    })
}

/// "-14.2 LUFS, -1.0 dBTP", or what is known of it.
fn reading_text(r: &Reading) -> String {
    match (r.lufs, r.true_peak_db) {
        (Some(l), Some(p)) => format!("{l:.1} LUFS, {p:.1} dBTP"),
        (Some(l), None) => format!("{l:.1} LUFS"),
        (None, Some(p)) => format!("no measurable loudness, {p:.1} dBTP"),
        (None, None) => "silent".to_string(),
    }
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
                $crate::commands::export::export_cover_check,
                $crate::commands::export::export_range_resolve,
                $crate::commands::export::export_loudness_analyse,
                $crate::commands::export::setlist::setlist_sheet_load,
                $crate::commands::export::setlist::setlist_sheet_save,
                $crate::commands::export::setlist::setlist_save_html,
                $crate::commands::export::vdj::vdj_export_plan,
                $crate::commands::export::vdj::vdj_export_start,
                $crate::commands::export::vdj::vdj_export_cancel,
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
    fn the_cue_target_says_whether_the_audio_file_and_the_sheet_exist() {
        let d = std::env::temp_dir().join(format!("pmrev-cuetarget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let audio = d.join("set.flac");
        let t = export_cue_target(audio.to_string_lossy().into_owned());
        assert!(!t.audio_exists && !t.exists);
        assert!(t.cue_path.ends_with("set.cue"));
        std::fs::write(&audio, b"x").unwrap();
        let t = export_cue_target(audio.to_string_lossy().into_owned());
        assert!(t.audio_exists && !t.exists, "only the audio file is there");
        std::fs::write(d.join("set.cue"), b"x").unwrap();
        assert!(export_cue_target(audio.to_string_lossy().into_owned()).exists);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn clock_formats_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(75.4), "1:15");
        assert_eq!(clock(3600.0), "60:00");
    }
}
