//! Stems: vocals and instrumental versions of tracks, made on this computer.
//!
//! The model runs through ONNX Runtime (`pm-stems`); the model file and the runtime library are
//! downloaded only when the user accepts the licence notice of each (`manifest`, `download`), from
//! the pinned address, SHA-256 checked. The results go to a folder on the PC the user picks and
//! are never written onto the device here: putting them on the card is the existing "add tracks"
//! step, which the UI offers afterwards and which asks first.
//!
//! Nothing here writes to a device, so the jobs use the shared `JobLock` only (ARCHITECTURE.md,
//! "Not a device write"). The audio of a track is found through the validated `plan::audio_in_volume`,
//! never through `Library::track_paths` alone.
//!
//! To add a command: write it here, add its path to `stems_commands!` below, then mirror it in
//! `src/lib/api/stems.ts` and `src/lib/devmock/stems.ts`.

mod download;
mod manifest;
mod plan;
mod run;
mod settings;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pm_formats::library::Library;
use pm_stems::htdemucs::{Device, HtDemucs, LoadOptions};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_job};
use crate::state::JobLock;

use manifest::{Component, Kind};
use plan::{Action, Plan};
use run::{Event, Outcome};
use settings::{Consent, StemSettings};

/// `job` name of a stem run in `job-done`.
pub const JOB_STEMS: &str = "stems";
/// `job` name of a model / runtime download in `job-progress` and `job-done`.
pub const JOB_STEMS_DOWNLOAD: &str = "stems-download";
/// Rich progress of a stem run (`StemsProgress`).
pub const STEMS_PROGRESS: &str = "stems-progress";
/// What a stem run produced (`StemsResult`), emitted just before its `job-done`.
pub const STEMS_RESULT: &str = "stems-result";

/// Set by `stems_cancel`, polled by a running download or stem run. Cleared when one starts.
#[derive(Default)]
pub struct StemsCancel(AtomicBool);

pub fn setup(app: &mut tauri::App) {
    app.manage(StemsCancel::default());
}

// ---- where things live ----------------------------------------------------------------------

fn config_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

fn data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_local_data_dir()?.join("stems"))
}

fn component_dir(data: &Path, c: &Component) -> PathBuf {
    data.join(match c.kind {
        Kind::Model => "models",
        Kind::Runtime => "runtime",
    })
    .join(c.id)
}

/// (is it installed, bytes on disk). A model counts only at its pinned size; a runtime when its library exists.
fn installed(data: &Path, c: &Component) -> (bool, u64) {
    let dir = component_dir(data, c);
    match c.kind {
        Kind::Model => match std::fs::metadata(dir.join(c.file)) {
            Ok(m) if m.is_file() && m.len() == c.size_bytes => (true, m.len()),
            _ => (false, 0),
        },
        Kind::Runtime => match std::fs::metadata(dir.join(manifest::RUNTIME_LIB)) {
            Ok(m) if m.is_file() => (true, m.len()),
            _ => (false, 0),
        },
    }
}

fn model_path(data: &Path) -> Option<(PathBuf, Component)> {
    let c = manifest::all().into_iter().find(|c| c.kind == Kind::Model)?;
    let p = component_dir(data, &c).join(c.file);
    Some((p, c))
}

struct RuntimeChoice {
    path: PathBuf,
    /// "custom" (chosen in Settings) or "downloaded".
    source: &'static str,
}

/// The onnxruntime library to use: the one chosen in Settings, else the downloaded one. The
/// system's own copy is never picked silently (an old one lacks features the model needs).
fn resolve_runtime(s: &StemSettings, data: &Path) -> Result<RuntimeChoice, String> {
    if !s.runtime_path.is_empty() {
        let p = PathBuf::from(&s.runtime_path);
        return if p.is_file() {
            Ok(RuntimeChoice { path: p, source: "custom" })
        } else {
            Err(format!("The onnxruntime library chosen in Settings was not found: {}", s.runtime_path))
        };
    }
    for c in manifest::all().into_iter().filter(|c| c.kind == Kind::Runtime && c.available_here) {
        let p = component_dir(data, &c).join(manifest::RUNTIME_LIB);
        if p.is_file() {
            return Ok(RuntimeChoice { path: p, source: "downloaded" });
        }
    }
    if manifest::all().iter().any(|c| c.kind == Kind::Runtime && c.available_here) {
        Err("ONNX Runtime is not downloaded yet.".into())
    } else {
        Err(format!("There is no downloadable ONNX Runtime for this system. Install ONNX Runtime yourself and choose its {} in Settings.", manifest::RUNTIME_LIB))
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---- status ---------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ComponentStatus {
    #[serde(flatten)]
    pub component: Component,
    pub installed: bool,
    pub installed_bytes: u64,
    pub consent: Option<Consent>,
    /// The user accepted the CURRENT notice of this component.
    pub consent_current: bool,
}

#[derive(Serialize)]
pub struct RuntimeStatus {
    pub source: Option<&'static str>,
    pub path: Option<String>,
    pub problem: Option<String>,
}

#[derive(Serialize)]
pub struct StemsStatus {
    pub components: Vec<ComponentStatus>,
    pub runtime: RuntimeStatus,
    /// Everything needed to separate is in place.
    pub ready: bool,
    /// Why not, in words (the Make stems action shows it instead of running).
    pub blocked: Option<String>,
    pub settings: StemSettings,
    pub settings_warning: Option<String>,
    /// A job (this or another) is running.
    pub busy: bool,
    pub data_dir: String,
}

fn status_of(data: &Path, s: StemSettings, warning: Option<String>, busy: bool) -> StemsStatus {
    let components: Vec<ComponentStatus> = manifest::all()
        .into_iter()
        .map(|c| {
            let (installed, installed_bytes) = installed(data, &c);
            let consent = s.consents.get(c.id).cloned();
            let consent_current = s.consented(c.id, c.notice_version);
            ComponentStatus { component: c, installed, installed_bytes, consent, consent_current }
        })
        .collect();
    let (runtime, runtime_problem) = match resolve_runtime(&s, data) {
        Ok(r) => (RuntimeStatus { source: Some(r.source), path: Some(r.path.to_string_lossy().into_owned()), problem: None }, None),
        Err(e) => (RuntimeStatus { source: None, path: None, problem: Some(e.clone()) }, Some(e)),
    };
    let model_ok = components.iter().any(|c| c.component.kind == Kind::Model && c.installed);
    let blocked = if !model_ok {
        Some("The separation model is not downloaded yet.".to_string())
    } else {
        runtime_problem
    };
    StemsStatus { components, runtime, ready: blocked.is_none(), blocked, settings: s, settings_warning: warning, busy, data_dir: data.to_string_lossy().into_owned() }
}

/// Everything the Stems view shows: the downloadable parts with their licence notices and whether
/// they are installed, the runtime in use, the settings, and whether a run can start.
#[tauri::command]
pub async fn stems_status(app: AppHandle, lock: State<'_, JobLock>) -> AppResult<StemsStatus> {
    let (cfg, data, busy) = (config_dir(&app)?, data_dir(&app)?, lock.is_held());
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let (s, warn) = settings::load(&cfg);
        status_of(&data, s, warn, busy)
    })
    .await?)
}

/// Saves the settings (validated; consents are not taken from the caller).
#[tauri::command]
pub async fn stems_settings_save(app: AppHandle, settings: StemSettings) -> AppResult<StemSettings> {
    let cfg = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || settings::save(&cfg, settings).map_err(AppError::from)).await?
}

// ---- downloads ------------------------------------------------------------------------------

/// Downloads a component after the user accepted its licence notice. `accept_notice_version`
/// must equal the notice version the UI showed: a stale or missing acceptance is refused and
/// nothing is fetched. Returns once the job has started; progress is `job-progress` with the job
/// name `stems-download` (bytes), the end is `job-done`.
#[tauri::command]
pub fn stems_download(app: AppHandle, lock: State<'_, JobLock>, id: String, accept_notice_version: u32) -> AppResult<()> {
    let c = manifest::find(&id).ok_or_else(|| AppError::from(format!("Unknown download {id}.")))?;
    if !c.available_here {
        return Err("This download is not available for this system.".into());
    }
    if accept_notice_version != c.notice_version {
        return Err("The licence notice must be accepted before anything is downloaded.".into());
    }
    let (cfg, data) = (config_dir(&app)?, data_dir(&app)?);
    if installed(&data, &c).0 {
        return Err(format!("{} is already downloaded.", c.name).into());
    }
    // the job lock is taken first so a refused start leaves no consent record behind
    spawn_job(app, &lock, JOB_STEMS_DOWNLOAD, move |app| {
        settings::record_consent(&cfg, c.id, c.notice_version, now_secs())?;
        let cancel = &app.state::<StemsCancel>().0;
        cancel.store(false, Ordering::SeqCst);
        let dir = component_dir(&data, &c);
        let file = dir.join(c.file);
        let mut last = Instant::now() - Duration::from_secs(1);
        let fetched = download::fetch(c.url, c.sha256, c.size_bytes, &file, cancel, &mut |done, total| {
            if last.elapsed() >= Duration::from_millis(250) || done == total {
                last = Instant::now();
                let mut p = Progress::new(JOB_STEMS_DOWNLOAD, c.name, done as usize, total as usize, "downloading");
                p.detail = Some(format!("{:.0} of {:.0} MB", done as f64 / 1e6, total as f64 / 1e6));
                events::emit(app, JOB_PROGRESS, p);
            }
        });
        match fetched {
            Err(e) if e == "cancelled" => return Ok(JobOutcome { ok: 0, failed: 0, summary: format!("Download of {} cancelled. Nothing was kept.", c.name) }),
            Err(e) => return Err(AppError::from(e)),
            Ok(()) => {}
        }
        if let Some(prefix) = c.unzip_dir {
            let unpacked = download::unzip_dlls(&file, prefix, &dir);
            let _ = std::fs::remove_file(&file);
            let names = unpacked?;
            return Ok(JobOutcome { ok: 1, failed: 0, summary: format!("Downloaded and verified {}; unpacked {}.", c.name, names.join(", ")) });
        }
        Ok(JobOutcome { ok: 1, failed: 0, summary: format!("Downloaded {} and checked its SHA-256.", c.name) })
    })
}

/// Deletes a downloaded component from the app's own data folder (never anything else). Refused
/// while a job runs.
#[tauri::command]
pub async fn stems_remove(app: AppHandle, lock: State<'_, JobLock>, id: String) -> AppResult<()> {
    let c = manifest::find(&id).ok_or_else(|| AppError::from(format!("Unknown download {id}.")))?;
    if lock.is_held() {
        return Err("A job is still running.".into());
    }
    let dir = component_dir(&data_dir(&app)?, &c);
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Could not remove {}: {e}", dir.display()).into()),
        }
    })
    .await?
}

/// Asks the running download or stem run to stop. Files that were not finished are removed.
#[tauri::command]
pub fn stems_cancel(app: AppHandle) {
    app.state::<StemsCancel>().0.store(true, Ordering::SeqCst);
}

// ---- plan and run ---------------------------------------------------------------------------

/// What a run would do with these tracks into `dest`, item by item, in words: which files are
/// written, which already exist (kept or replaced, per the setting), which tracks cannot be done
/// and why. Reads only. The run computes the same plan again from the same function.
#[tauri::command]
pub async fn stems_plan(app: AppHandle, root: String, track_ids: Vec<i64>, dest: String) -> AppResult<Plan> {
    let cfg = config_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Plan> {
        let (s, _) = settings::load(&cfg);
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(plan::build_plan(&lib, &track_ids, Path::new(&dest), &s)?)
    })
    .await?
}

#[derive(Serialize, Clone)]
pub struct StemsProgress {
    /// "checking-model", "loading-model" or "separating".
    pub phase: String,
    pub index: usize,
    pub count: usize,
    pub label: String,
    pub segment: usize,
    pub segments: usize,
    pub eta_secs: Option<u64>,
    pub device: String,
}

#[derive(Serialize, Clone)]
pub struct WrittenItem {
    pub track_id: i64,
    pub label: String,
    pub vocals: Option<String>,
    pub instrumental: Option<String>,
    pub seconds: f64,
}

#[derive(Serialize, Clone)]
pub struct FailedItem {
    pub label: String,
    pub error: String,
}

#[derive(Serialize, Clone)]
pub struct StemsResult {
    pub dest: String,
    pub device: String,
    pub written: Vec<WrittenItem>,
    /// Labels of tracks the plan left alone (files existed, or it could not be done), with the reason.
    pub skipped: Vec<FailedItem>,
    pub failed: Vec<FailedItem>,
    pub cancelled: bool,
    /// Samples that had to be limited to full scale, over all files; 0 = none.
    pub limited_samples: u64,
}

#[derive(Serialize)]
pub struct StemsStarted {
    pub tracks: usize,
}

/// Estimated time left from the segments done so far. The per-track estimates are replaced by the
/// real segment count as each track starts.
struct Eta {
    est: Vec<usize>,
    finished: usize,
}

impl Eta {
    fn new(est: Vec<usize>) -> Eta {
        Eta { est, finished: 0 }
    }

    /// `index` is at `segment` of `segments`; `elapsed` seconds since the first segment began.
    fn secs(&mut self, index: usize, segment: usize, segments: usize, elapsed: f64) -> Option<u64> {
        if let Some(e) = self.est.get_mut(index) {
            *e = segments;
        }
        let total: usize = self.est.iter().sum();
        let done = self.finished + segment;
        if segment == segments && segment > 0 {
            self.finished += segments;
        }
        if done == 0 || elapsed <= 0.0 || total <= done {
            return None;
        }
        Some((elapsed / done as f64 * (total - done) as f64).round() as u64)
    }
}

fn device_of(s: &StemSettings) -> Device {
    match s.device.as_str() {
        "cpu" => Device::Cpu,
        "gpu" => Device::Gpu,
        _ => Device::Auto,
    }
}

fn clock(secs: f64) -> String {
    let s = secs.round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn summarise(plan: &Plan, outcomes: &[Outcome], device: &str, dest: &str, elapsed: f64) -> (StemsResult, JobOutcome) {
    let mut res = StemsResult { dest: dest.into(), device: device.into(), written: vec![], skipped: vec![], failed: vec![], cancelled: false, limited_samples: 0 };
    for (item, o) in plan.items.iter().zip(outcomes) {
        match o {
            Outcome::Written { vocals, instrumental, seconds, report } => {
                res.limited_samples += report.clipped_vocals + report.clipped_instrumental;
                res.written.push(WrittenItem { track_id: item.track_id, label: item.label.clone(), vocals: vocals.clone(), instrumental: instrumental.clone(), seconds: *seconds });
            }
            Outcome::Failed(e) => res.failed.push(FailedItem { label: item.label.clone(), error: e.clone() }),
            Outcome::Cancelled => res.cancelled = true,
            Outcome::NotRun => res.skipped.push(FailedItem { label: item.label.clone(), error: item.what.clone() }),
        }
    }
    let mut parts = vec![format!("{} track{} done in {} ({device})", res.written.len(), if res.written.len() == 1 { "" } else { "s" }, clock(elapsed))];
    if !res.failed.is_empty() {
        parts.push(format!("{} failed", res.failed.len()));
    }
    if !res.skipped.is_empty() {
        parts.push(format!("{} left alone", res.skipped.len()));
    }
    if res.cancelled {
        parts.push("cancelled; the files of finished tracks were kept, nothing half-written was left".into());
    }
    if res.limited_samples > 0 {
        parts.push(format!("{} samples were limited to avoid clipping", res.limited_samples));
    }
    let out = JobOutcome { ok: res.written.len(), failed: res.failed.len(), summary: parts.join("; ") };
    (res, out)
}

/// Makes the vocals and instrumental files for these tracks into `dest` (a folder on this
/// computer). Returns as soon as the job has started: `stems-progress` events follow, then
/// `stems-result`, then `job-done` (job `stems`). The plan is computed again here from the same
/// function the preview used; if it would replace existing files, `confirm_replace` must be true.
#[tauri::command]
pub async fn stems_run(app: AppHandle, lock: State<'_, JobLock>, root: String, track_ids: Vec<i64>, dest: String, confirm_replace: bool) -> AppResult<StemsStarted> {
    let (cfg, data) = (config_dir(&app)?, data_dir(&app)?);
    let (s, _) = settings::load(&cfg);
    let status = status_of(&data, s.clone(), None, false);
    if let Some(why) = status.blocked {
        return Err(why.into());
    }
    let runtime = resolve_runtime(&s, &data)?;
    let (model_file, model_component) = model_path(&data).ok_or("No model is defined.")?;

    let (plan, tracks) = {
        let (root, dest, ids, s) = (root.clone(), dest.clone(), track_ids.clone(), s.clone());
        tauri::async_runtime::spawn_blocking(move || -> AppResult<(Plan, Vec<pm_formats::library::Track>)> {
            let lib = Library::open_read_only(Path::new(&root))?;
            let plan = plan::build_plan(&lib, &ids, Path::new(&dest), &s)?;
            Ok((plan, lib.tracks()?))
        })
        .await??
    };
    let runnable = plan.runs().count();
    if runnable == 0 {
        return Err(format!("Nothing to do: {} left alone, {} cannot be done.", plan.skip, plan.refuse).into());
    }
    if plan.replace > 0 && !confirm_replace {
        return Err(format!("This would replace {} existing file(s); confirm that first.", plan.replaced_files.len()).into());
    }

    let est: Vec<usize> = plan.items.iter().map(|i| if matches!(i.action, Action::Make | Action::Replace | Action::Partial) { i.segments } else { 0 }).collect();
    let dest_for_result = dest.clone();
    spawn_job(app, &lock, JOB_STEMS, move |app| {
        let cancel = &app.state::<StemsCancel>().0;
        cancel.store(false, Ordering::SeqCst);
        let count = plan.items.len();
        let progress = |phase: &str, index: usize, label: &str, segment: usize, segments: usize, eta: Option<u64>, device: &str| {
            events::emit(app, STEMS_PROGRESS, StemsProgress { phase: phase.into(), index, count, label: label.into(), segment, segments, eta_secs: eta, device: device.into() });
        };

        // the file on disk must be the file that was pinned (a damaged or swapped download is not loaded)
        progress("checking-model", 0, "", 0, 0, None, "");
        let got = download::sha256_file(&model_file).map_err(|e| AppError::from(format!("Cannot read the model: {e}")))?;
        if got != model_component.sha256 {
            return Err(AppError::from("The downloaded model does not match its pinned SHA-256. Remove it in Settings and download it again."));
        }
        progress("loading-model", 0, "", 0, 0, None, "");
        let mut model = HtDemucs::load(&LoadOptions { runtime: &runtime.path, model: &model_file, device: device_of(&s), threads: s.threads as usize })
            .map_err(|e| AppError::from(e.to_string()))?;
        let device = pm_stems::provider::StemModel::info(&model).device.clone();

        let started = Instant::now();
        let mut eta = Eta::new(est);
        let mut first_segment_at: Option<Instant> = None;
        let mut label = String::new();
        let outcomes = run::run_batch(&plan, &tracks, &s, &mut model, cancel, &mut |ev| match ev {
            Event::Track { index, label: l, .. } => {
                label = l.to_string();
                progress("separating", index, &label, 0, 0, None, &device);
            }
            Event::Segment { index, segment, segments } => {
                let t0 = *first_segment_at.get_or_insert_with(Instant::now);
                let e = eta.secs(index, segment, segments, t0.elapsed().as_secs_f64());
                progress("separating", index, &label, segment, segments, e, &device);
            }
        });
        let (result, outcome) = summarise(&plan, &outcomes, &device, &dest_for_result, started.elapsed().as_secs_f64());
        events::emit(app, STEMS_RESULT, result);
        // progress per track for generic job consumers
        events::emit(app, JOB_PROGRESS, Progress::new(JOB_STEMS, "", count, count, "done"));
        Ok(outcome)
    })?;
    Ok(StemsStarted { tracks: runnable })
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! stems_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::stems::stems_status,
                $crate::commands::stems::stems_settings_save,
                $crate::commands::stems::stems_download,
                $crate::commands::stems::stems_remove,
                $crate::commands::stems::stems_cancel,
                $crate::commands::stems::stems_plan,
                $crate::commands::stems::stems_run,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-stems-mod-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn status_says_what_is_missing_in_order() {
        let data = temp_dir("status");
        let s = StemSettings::default();
        let st = status_of(&data, s.clone(), None, false);
        assert!(!st.ready);
        assert_eq!(st.blocked.as_deref(), Some("The separation model is not downloaded yet."));
        assert_eq!(st.components.len(), 2);
        assert!(st.components.iter().all(|c| !c.installed && !c.consent_current));

        // a model file of the wrong size is not an installed model
        let (mp, mc) = model_path(&data).unwrap();
        std::fs::create_dir_all(mp.parent().unwrap()).unwrap();
        std::fs::write(&mp, b"short").unwrap();
        assert!(!installed(&data, &mc).0);

        // with a custom runtime that does not exist the reason names it
        let custom = StemSettings { runtime_path: data.join("nope.dll").to_string_lossy().into_owned(), ..s.clone() };
        assert!(resolve_runtime(&custom, &data).err().unwrap().contains("chosen in Settings was not found"));
        // a custom runtime that exists wins over the downloaded one
        let lib = data.join("custom-onnxruntime.dll");
        std::fs::write(&lib, b"x").unwrap();
        let custom = StemSettings { runtime_path: lib.to_string_lossy().into_owned(), ..s };
        let r = resolve_runtime(&custom, &data).unwrap();
        assert_eq!((r.source, r.path), ("custom", lib));
        std::fs::remove_dir_all(&data).unwrap();
    }

    #[test]
    fn a_downloaded_runtime_is_found_where_the_download_puts_it() {
        let data = temp_dir("runtime");
        let c = manifest::all().into_iter().find(|c| c.kind == Kind::Runtime).unwrap();
        if !c.available_here {
            return; // no downloadable runtime on this platform
        }
        assert_eq!(resolve_runtime(&StemSettings::default(), &data).err().unwrap(), "ONNX Runtime is not downloaded yet.");
        let dir = component_dir(&data, &c);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(manifest::RUNTIME_LIB), b"x").unwrap();
        assert!(installed(&data, &c).0);
        let r = resolve_runtime(&StemSettings::default(), &data).unwrap();
        assert_eq!(r.source, "downloaded");
        std::fs::remove_dir_all(&data).unwrap();
    }

    #[test]
    fn eta_extrapolates_from_segments_done_and_corrects_the_estimate() {
        let mut e = Eta::new(vec![10, 10]);
        assert_eq!(e.secs(0, 0, 8, 0.0), None, "nothing done yet");
        // track 0 really has 8 segments: 2 done in 4 s -> 18 - 2 = 16 left at 2 s each
        assert_eq!(e.secs(0, 2, 8, 4.0), Some(32));
        assert_eq!(e.secs(0, 8, 8, 16.0), Some(20), "10 left (the second track's estimate) at 2 s each");
        // second track starts: its real count replaces the estimate
        assert_eq!(e.secs(1, 1, 6, 18.0), Some(((18.0 / 9.0) * 5.0_f64).round() as u64));
        assert_eq!(e.secs(1, 6, 6, 28.0), None, "nothing left");
    }

    #[test]
    fn summaries_count_written_failed_skipped_and_cancelled() {
        use run::Outcome;
        let mk = |id: i64, action: Action| plan::PlanItem {
            track_id: id,
            label: format!("T{id}"),
            minutes: 1.0,
            source: String::new(),
            vocals_path: String::new(),
            instrumental_path: String::new(),
            vocals_exists: false,
            instrumental_exists: false,
            write_vocals: true,
            write_instrumental: true,
            action,
            what: "because".into(),
            problem: None,
            segments: 1,
        };
        let plan = Plan {
            dest: "D".into(),
            format_label: "x".into(),
            items: vec![mk(1, Action::Make), mk(2, Action::Make), mk(3, Action::Skip), mk(4, Action::Make)],
            make: 3,
            replace: 0,
            partial: 0,
            skip: 1,
            refuse: 0,
            minutes: 3.0,
            replaced_files: vec![],
        };
        let rep = pm_stems::pipeline::Report { clipped_vocals: 2, ..Default::default() };
        let outcomes = vec![Outcome::Written { vocals: Some("v".into()), instrumental: Some("i".into()), seconds: 1.0, report: rep }, Outcome::Failed("boom".into()), Outcome::NotRun, Outcome::Cancelled];
        let (res, out) = summarise(&plan, &outcomes, "CPU", "D", 125.0);
        assert_eq!((res.written.len(), res.failed.len(), res.skipped.len(), res.cancelled, res.limited_samples), (1, 1, 1, true, 2));
        assert_eq!((out.ok, out.failed), (1, 1));
        assert!(out.summary.contains("1 track done in 2:05 (CPU)") && out.summary.contains("1 failed") && out.summary.contains("1 left alone") && out.summary.contains("cancelled") && out.summary.contains("2 samples"), "{}", out.summary);
        assert_eq!(res.skipped[0].error, "because");
    }
}
