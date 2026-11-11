//! Grid edits on many tracks at once, previewed in full before anything is written.
//!
//! The operations are the grid editor's own: nudge, halve, double (`ops_grid::apply`), the
//! "suggested fix" that the suspect rules pick per track (halve or double,
//! [`super::suspect`]), and re-analyse (the analyser run again on the audio).
//!
//! **One computation serves the preview and the write.** Planning (or, for re-analysis, the
//! analysis job) produces the new grid of every track; those grids are kept in [`BatchState`]
//! under a plan id. The preview rows come from them, the diff view reads them, and
//! `grid_batch_apply` writes exactly those grids, so what was shown is what is saved. Before a
//! track is written its stored grid is read again and compared with the grid the plan started
//! from; if the file changed in between (another tool, another app session) the track is skipped
//! and says so, instead of being overwritten with a result computed from something else.
//!
//! Writes go through `Library::save_grid` (the `.str` atomically, the row's BPM with it) inside a
//! write job, so they get the session snapshot, the write gate and the `JobLock`. A track's audio
//! is only ever read, and its location is checked with `analysis::audio_path` before any file
//! is touched.

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use pm_formats::library::{track_bpm, Library};
use pm_formats::ops_grid::{self, EditCtx, GridDoc, GridEdit, GridLoad};
use pm_formats::strfile::Analysis;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::suspect::{assess, Fix, SuspectSettings};
use crate::commands::analysis::{audio_path, label_of};
use crate::error::{AppResult, IntoApp};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, spawn_write_job, JobOutcome, Progress};
use crate::state::JobLock;

/// Job kind of the write ("grid-batch").
pub const BATCH_JOB: &str = "grid-batch";
/// Job kind of the read-only analysis behind a re-analyse preview.
pub const ANALYSE_JOB: &str = "grid-batch-analyse";
/// The furthest a nudge may move the beat lines, either way.
pub const MAX_NUDGE_MS: f64 = 5000.0;

/// What to do to every track of a batch (re-analysis has its own job: [`grid_batch_analyse`]).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchOp {
    /// Move every beat line by this many milliseconds (negative = earlier).
    Nudge { ms: f64 },
    Halve,
    Double,
    /// Halve or double each track the tempo rule flagged, as that rule suggests; a track it did
    /// not flag, or has no suggestion for, is left alone.
    Suggested { settings: SuspectSettings },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MixRef {
    pub id: i64,
    pub name: String,
}

/// What will happen to one track.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatchRow {
    pub track_id: i64,
    pub label: String,
    /// What is done to this track: "nudge", "halve", "double" or "reanalyse".
    pub op: String,
    pub before_bpm: Option<f64>,
    pub after_bpm: Option<f64>,
    /// Position of the first beat line in source frames, before and after.
    pub before_first: Option<i64>,
    pub after_first: Option<i64>,
    pub before_beats: usize,
    pub after_beats: usize,
    pub sample_rate: u32,
    /// Why the track is left alone; `None` when it will be changed.
    pub skip: Option<String>,
    /// Extra facts worth showing (what the analyser said about the new grid).
    pub note: Option<String>,
    /// Mixes that use the track: their clips keep the tempo the track had when they were made.
    pub mixes: Vec<MixRef>,
}

/// The grid a plan computed for one track.
#[derive(Debug, Clone)]
pub struct Proposal {
    /// The stored grid the plan started from (`None`: the track had no readable grid).
    pub before: Option<GridDoc>,
    pub after: GridDoc,
    pub sample_rate: u32,
    pub total_frames: i64,
}

struct Planned {
    row: BatchRow,
    proposal: Option<Proposal>,
}

/// A computed plan, kept until the next one replaces it.
pub struct StoredPlan {
    id: u64,
    root: String,
    rows: Vec<BatchRow>,
    proposals: HashMap<i64, Proposal>,
}

/// Managed state of the batch tools.
#[derive(Default)]
pub struct BatchState {
    current: Mutex<Option<Arc<StoredPlan>>>,
    next_id: AtomicU64,
    cancel: AtomicBool,
}

impl BatchState {
    fn store(&self, root: &str, planned: Vec<Planned>) -> Arc<StoredPlan> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let mut rows = Vec::with_capacity(planned.len());
        let mut proposals = HashMap::new();
        for p in planned {
            if let Some(prop) = p.proposal {
                proposals.insert(p.row.track_id, prop);
            }
            rows.push(p.row);
        }
        let plan = Arc::new(StoredPlan { id, root: root.to_string(), rows, proposals });
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some(plan.clone());
        plan
    }

    /// The stored plan, if it is `id` (when given) and was made for `root`.
    fn get(&self, root: &str, id: Option<u64>) -> AppResult<Arc<StoredPlan>> {
        let cur = self.current.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match cur {
            Some(p) if p.root == root && id.is_none_or(|i| i == p.id) => Ok(p),
            _ => Err("That preview is no longer current. Preview the change again.".into()),
        }
    }

    fn clear(&self) {
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Managed state private to this area is created from `grid::setup`.
pub fn setup(app: &mut tauri::App) {
    app.manage(BatchState::default());
}

/// A plan as the frontend gets it.
#[derive(Debug, Clone, Serialize)]
pub struct BatchPlan {
    pub plan_id: u64,
    pub rows: Vec<BatchRow>,
}

impl From<&StoredPlan> for BatchPlan {
    fn from(p: &StoredPlan) -> Self {
        BatchPlan { plan_id: p.id, rows: p.rows.clone() }
    }
}

fn skipped(base: BatchRow, why: impl Into<String>) -> Planned {
    Planned { row: BatchRow { skip: Some(why.into()), ..base }, proposal: None }
}

fn blank_row(track_id: i64, label: String, op: &str) -> BatchRow {
    BatchRow {
        track_id,
        label,
        op: op.into(),
        before_bpm: None,
        after_bpm: None,
        before_first: None,
        after_first: None,
        before_beats: 0,
        after_beats: 0,
        sample_rate: 0,
        skip: None,
        note: None,
        mixes: Vec::new(),
    }
}

fn op_name(op: &BatchOp) -> &'static str {
    match op {
        BatchOp::Nudge { .. } => "nudge",
        BatchOp::Halve => "halve",
        BatchOp::Double => "double",
        BatchOp::Suggested { .. } => "suggested",
    }
}

fn check_op(op: &BatchOp) -> AppResult<()> {
    match op {
        BatchOp::Nudge { ms } if !ms.is_finite() || *ms == 0.0 || ms.abs() > MAX_NUDGE_MS => {
            Err(format!("A nudge must be between 0 and {MAX_NUDGE_MS} ms either way.").into())
        }
        BatchOp::Suggested { settings } => Ok(settings.validate()?),
        _ => Ok(()),
    }
}

/// The edit for one track and the name it goes by in the row, or why there is none.
fn edit_for(op: &BatchOp, load: &GridLoad, doc: &GridDoc, genre: &str) -> Result<(GridEdit, &'static str), String> {
    match op {
        BatchOp::Nudge { ms } => {
            let frames = (ms / 1000.0 * f64::from(load.sample_rate)).round() as i64;
            if frames == 0 {
                return Err("the nudge is less than one sample".into());
            }
            Ok((GridEdit::Nudge { frames }, "nudge"))
        }
        BatchOp::Halve => Ok((GridEdit::Halve, "halve")),
        BatchOp::Double => Ok((GridEdit::Double, "double")),
        BatchOp::Suggested { settings } => {
            let bpm = track_bpm(&Analysis::from(doc));
            // only the tempo rule suggests a fix; confidence alone does not say which way to go
            let mut tempo_only = settings.clone();
            tempo_only.quality_rule = false;
            match assess(load.track_id, genre, bpm, None, &tempo_only) {
                None => Err("the tempo rule does not flag this track".into()),
                Some(s) => match s.fix {
                    Some(Fix::Halve) => Ok((GridEdit::Halve, "halve")),
                    Some(Fix::Double) => Ok((GridEdit::Double, "double")),
                    None => Err("flagged, but neither half nor double fits its window".into()),
                },
            }
        }
    }
}

/// Decides, for each id (duplicates dropped, order kept), what the operation does. Reads only.
/// A track is skipped, with the reason in words, when it is not in the library, its location is
/// not a plain path below `/pmdb_tracks/`, it has no readable `.str`, the edit is refused or the
/// grid would not change.
fn plan(lib: &Library, ids: &[i64], op: &BatchOp) -> AppResult<Vec<Planned>> {
    check_op(op)?;
    let tracks = lib.tracks()?;
    let by_id: HashMap<i64, &pm_formats::library::Track> = tracks.iter().map(|t| (t.track_id, t)).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(t) = by_id.get(&id) else {
            out.push(skipped(blank_row(id, format!("track {id}"), op_name(op)), "not in the library"));
            continue;
        };
        let mut row = blank_row(id, label_of(t), op_name(op));
        if let Err(e) = audio_path(lib.root(), &t.location) {
            out.push(skipped(row, e.to_string()));
            continue;
        }
        let load = match lib.load_grid(id) {
            Ok(l) => l,
            Err(e) => {
                out.push(skipped(row, e.to_string()));
                continue;
            }
        };
        row.sample_rate = load.sample_rate;
        row.mixes = mix_refs(lib, id);
        let Some(doc) = load.doc.clone() else {
            let why = load.problem.clone().unwrap_or_else(|| "no readable beat grid".into());
            out.push(skipped(row, format!("{why}; re-analyse it first")));
            continue;
        };
        row.before_bpm = Some(track_bpm(&Analysis::from(&doc)));
        row.before_first = doc.beats.first().map(|b| b.pos);
        row.before_beats = doc.beats.len();
        let (edit, name) = match edit_for(op, &load, &doc, &t.genre) {
            Ok(e) => e,
            Err(why) => {
                out.push(skipped(row, why));
                continue;
            }
        };
        row.op = name.into();
        // the same context the grid editor passes with its edits
        let ctx = EditCtx { sample_rate: load.sample_rate, total_frames: (load.total_frames > 0).then_some(load.total_frames) };
        match ops_grid::apply(&doc, edit, ctx) {
            Err(e) => out.push(skipped(row, e.to_string().trim_start_matches("invalid: ").to_string())),
            Ok(next) if next == doc => out.push(skipped(row, "the grid is already like that")),
            Ok(next) => {
                fill_after(&mut row, &next);
                out.push(Planned { row, proposal: Some(Proposal { before: Some(doc), after: next, sample_rate: load.sample_rate, total_frames: load.total_frames }) });
            }
        }
    }
    Ok(out)
}

fn fill_after(row: &mut BatchRow, next: &GridDoc) {
    row.after_bpm = Some(track_bpm(&Analysis::from(next)));
    row.after_first = next.beats.first().map(|b| b.pos);
    row.after_beats = next.beats.len();
}

fn mix_refs(lib: &Library, id: i64) -> Vec<MixRef> {
    // a failed lookup must not read as "no mix uses it": the row gets an explicit marker instead
    match lib.mixes_using_track(id) {
        Ok(v) => v.into_iter().map(|(id, name)| MixRef { id, name }).collect(),
        Err(e) => vec![MixRef { id: -1, name: format!("(could not check: {e})") }],
    }
}

/// What the operation would do to each track. Computes the new grids, keeps them under a plan id
/// and returns the rows; nothing is written.
#[tauri::command]
pub async fn grid_batch_plan(state: State<'_, BatchState>, root: String, track_ids: Vec<i64>, op: BatchOp) -> AppResult<BatchPlan> {
    let path = root.clone();
    let planned = tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&path))?;
        plan(&lib, &track_ids, &op)
    })
    .await
    .app()??;
    Ok(BatchPlan::from(&*state.store(&root, planned)))
}

/// Starts the read-only "grid-batch-analyse" job: runs the analyser on the audio of each track and
/// keeps the grids it finds as a plan, exactly as the re-analyse job would save them. Nothing is
/// written to the device. Progress is one event per track; when the job is done,
/// `grid_batch_result` returns the plan. `grid_batch_cancel` stops it between tracks and the plan
/// then holds the tracks analysed so far.
#[tauri::command]
pub fn grid_batch_analyse(app: AppHandle, lock: State<'_, JobLock>, state: State<'_, BatchState>, root: String, track_ids: Vec<i64>) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err("No tracks to analyse.".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    state.clear();
    spawn_job(app, &lock, ANALYSE_JOB, move |app| {
        let lib = Library::open_read_only(Path::new(&root))?;
        let state = app.state::<BatchState>();
        let planned = analyse_plan(&lib, &track_ids, &state.cancel, |p| events::emit(app, JOB_PROGRESS, p))?;
        let (changed, other): (usize, usize) = planned.iter().fold((0, 0), |(c, o), p| if p.proposal.is_some() { (c + 1, o) } else { (c, o + 1) });
        let cancelled = state.cancel.load(Ordering::SeqCst);
        state.store(&root, planned);
        let mut summary = format!("{changed} new grid{} ready to review", if changed == 1 { "" } else { "s" });
        if other > 0 {
            summary.push_str(&format!(", {other} left as they are"));
        }
        if cancelled {
            summary.push_str(" (stopped early)");
        }
        Ok(JobOutcome { ok: changed, failed: 0, summary })
    })
}

/// The analysis of every wanted track, ready to review. Tracks that cannot be analysed are
/// returned as skipped rows with the reason. Progress is reported once per track.
fn analyse_plan(lib: &Library, ids: &[i64], cancel: &AtomicBool, mut progress: impl FnMut(Progress)) -> AppResult<Vec<Planned>> {
    let tracks = lib.tracks()?;
    let by_id: HashMap<i64, &pm_formats::library::Track> = tracks.iter().map(|t| (t.track_id, t)).collect();
    let mut seen = HashSet::new();
    let mut out: Vec<Planned> = Vec::new();
    // (index into out, audio path, load) for the tracks that can be analysed
    let mut todo: Vec<(usize, PathBuf, GridLoad)> = Vec::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(t) = by_id.get(&id) else {
            out.push(skipped(blank_row(id, format!("track {id}"), "reanalyse"), "not in the library"));
            continue;
        };
        let mut row = blank_row(id, label_of(t), "reanalyse");
        let audio = match audio_path(lib.root(), &t.location) {
            Ok(a) if a.is_file() => a,
            Ok(_) => {
                out.push(skipped(row, "the audio file is missing from the volume"));
                continue;
            }
            Err(e) => {
                out.push(skipped(row, e.to_string()));
                continue;
            }
        };
        let load = match lib.load_grid(id) {
            Ok(l) => l,
            Err(e) => {
                out.push(skipped(row, e.to_string()));
                continue;
            }
        };
        row.sample_rate = load.sample_rate;
        row.mixes = mix_refs(lib, id);
        if let Some(doc) = &load.doc {
            row.before_bpm = Some(track_bpm(&Analysis::from(doc)));
            row.before_first = doc.beats.first().map(|b| b.pos);
            row.before_beats = doc.beats.len();
        }
        todo.push((out.len(), audio, load));
        out.push(Planned { row, proposal: None });
    }
    let total = out.len();
    let mut finished = 0usize;
    for p in out.iter().filter(|p| p.row.skip.is_some()) {
        finished += 1;
        progress(Progress::new(ANALYSE_JOB, p.row.label.clone(), finished, total, "skipped"));
    }
    let files: Vec<PathBuf> = todo.iter().map(|(_, a, _)| a.clone()).collect();
    analyse_files(&files, cancel, |i, res| {
        finished += 1;
        let (at, _, load) = &todo[i];
        let planned = &mut out[*at];
        let mut ev = Progress::new(ANALYSE_JOB, planned.row.label.clone(), finished, total, "");
        match res {
            Err(e) => {
                planned.row.skip = Some(e.clone());
                ev.status = "failed".into();
                ev.error = Some(e);
            }
            Ok(found) => match finish_analysed(&mut planned.row, load, found) {
                Ok(prop) => {
                    ev.status = "analysed".into();
                    ev.bpm = planned.row.after_bpm;
                    planned.proposal = Some(prop);
                }
                Err(why) => {
                    ev.status = "skipped".into();
                    ev.detail = Some(why.clone());
                    planned.row.skip = Some(why);
                }
            },
        }
        progress(ev);
    });
    // tracks the analyser never reached because the run was stopped
    for p in out.iter_mut() {
        if p.proposal.is_none() && p.row.skip.is_none() {
            p.row.skip = Some("not analysed: the run was stopped".into());
        }
    }
    Ok(out)
}

/// What the analyser found for a file.
struct Found {
    doc: GridDoc,
    note: String,
}

/// Turns a finished analysis into the row's "after" and a proposal, or says why the track stays.
fn finish_analysed(row: &mut BatchRow, load: &GridLoad, found: Found) -> Result<Proposal, String> {
    ops_grid::validate(&found.doc).map_err(|e| format!("the analyser found no usable beats ({})", e.to_string().trim_start_matches("invalid: ")))?;
    row.note = Some(found.note);
    if load.doc.as_ref() == Some(&found.doc) {
        return Err("the new analysis is the same as the stored grid".into());
    }
    fill_after(row, &found.doc);
    Ok(Proposal { before: load.doc.clone(), after: found.doc, sample_rate: load.sample_rate, total_frames: load.total_frames })
}

/// Analyses `files` on a few worker threads, handing each result to `sink` (on the calling thread)
/// as it finishes. Stops taking new files once `cancel` is set. A decoder that panics becomes a
/// failed file, never a crashed job.
fn analyse_files(files: &[PathBuf], cancel: &AtomicBool, mut sink: impl FnMut(usize, Result<Found, String>)) {
    if files.is_empty() {
        return;
    }
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).saturating_sub(1).clamp(1, 4).min(files.len());
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|s| {
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            s.spawn(move || loop {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= files.len() {
                    break;
                }
                if tx.send((i, analyse_one(&files[i]))).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        for (i, r) in rx {
            sink(i, r);
        }
    });
}

fn analyse_one(path: &Path) -> Result<Found, String> {
    catch_unwind(AssertUnwindSafe(|| {
        let r = pm_analysis::analyze_file(path).map_err(|e| e.to_string())?;
        let note = if r.locked_grid {
            format!("even grid at {:.2} BPM", r.bpm)
        } else if r.residual_ms.is_finite() {
            format!("the tempo varies: the tracked beats were kept (they stray {:.0} ms from an even grid)", r.residual_ms)
        } else {
            "no even grid fitted: the tracked beats were kept".to_string()
        };
        Ok(Found { doc: GridDoc::from(&r.to_str_analysis()), note })
    }))
    .unwrap_or_else(|_| Err("the audio decoder crashed on this file".to_string()))
}

/// The plan the analysis job produced, once the job is done (`None` when there is none for this
/// library, for example because the job failed).
#[tauri::command]
pub fn grid_batch_result(state: State<'_, BatchState>, root: String) -> Option<BatchPlan> {
    state.get(&root, None).ok().map(|p| BatchPlan::from(&*p))
}

/// Stops a running analysis between tracks.
#[tauri::command]
pub fn grid_batch_cancel(state: State<'_, BatchState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Both grids of one planned track, for the old-vs-new diff view.
#[derive(Debug, Clone, Serialize)]
pub struct BatchDiff {
    pub before: Option<GridDoc>,
    pub after: GridDoc,
    pub sample_rate: u32,
    pub total_frames: i64,
}

#[tauri::command]
pub fn grid_batch_diff(state: State<'_, BatchState>, root: String, plan_id: u64, track_id: i64) -> AppResult<BatchDiff> {
    let plan = state.get(&root, Some(plan_id))?;
    let p = plan.proposals.get(&track_id).ok_or("That track has no new grid in this preview.")?;
    Ok(BatchDiff { before: p.before.clone(), after: p.after.clone(), sample_rate: p.sample_rate, total_frames: p.total_frames })
}

/// Starts the "grid-batch" write job: saves the grids of the plan for `track_ids` (the ones the
/// user left ticked). Each track's stored grid is read first and must still equal the grid the
/// plan started from. One progress event per track; `bpm` carries the new tempo.
#[tauri::command]
pub fn grid_batch_apply(app: AppHandle, lock: State<'_, JobLock>, state: State<'_, BatchState>, root: String, plan_id: u64, track_ids: Vec<i64>) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err("No tracks to change.".into());
    }
    let plan = state.get(&root, Some(plan_id))?;
    spawn_write_job(app, &lock, BATCH_JOB, root.clone(), move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        run(&mut lib, &plan, &track_ids, |p| events::emit(app, JOB_PROGRESS, p))
    })
}

/// The body of the write job, apart from Tauri so it can be tested on a temp library.
fn run(lib: &mut Library, plan: &StoredPlan, ids: &[i64], mut progress: impl FnMut(Progress)) -> AppResult<JobOutcome> {
    // refuse before touching anything: a library the device cannot read must not get new .str files
    let problems = lib.compatibility_problems()?;
    if !problems.is_empty() {
        return Err(format!("music.db is not device-compatible, nothing was changed: {}", problems.join("; ")).into());
    }
    let labels: HashMap<i64, &str> = plan.rows.iter().map(|r| (r.track_id, r.label.as_str())).collect();
    let mut seen = HashSet::new();
    let wanted: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    let total = wanted.len();
    let (mut ok, mut failed, mut skipped_n) = (0usize, 0usize, 0usize);
    for (i, id) in wanted.into_iter().enumerate() {
        let label = labels.get(&id).map_or_else(|| format!("track {id}"), |l| (*l).to_string());
        let mut ev = Progress::new(BATCH_JOB, label, i + 1, total, "");
        match save_one(lib, plan, id) {
            Ok(Saved::Wrote(bpm)) => {
                ev.status = "changed".into();
                ev.bpm = Some(bpm);
                ok += 1;
            }
            Ok(Saved::Skipped(why)) => {
                ev.status = "skipped".into();
                ev.detail = Some(why);
                skipped_n += 1;
            }
            Err(e) => {
                ev.status = "failed".into();
                ev.error = Some(e.to_string());
                failed += 1;
            }
        }
        progress(ev);
    }
    let problems = lib.compatibility_problems()?;
    if !problems.is_empty() {
        return Err(format!("music.db is no longer device-compatible: {}", problems.join("; ")).into());
    }
    let mut parts = Vec::new();
    if ok > 0 {
        parts.push(format!("{ok} beat grid{} changed", if ok == 1 { "" } else { "s" }));
    }
    if skipped_n > 0 {
        parts.push(format!("{skipped_n} skipped"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    let summary = if parts.is_empty() { "nothing to do".to_string() } else { parts.join(", ") };
    Ok(JobOutcome { ok, failed, summary })
}

enum Saved {
    Wrote(f64),
    Skipped(String),
}

fn save_one(lib: &mut Library, plan: &StoredPlan, id: i64) -> AppResult<Saved> {
    let Some(prop) = plan.proposals.get(&id) else {
        let why = plan.rows.iter().find(|r| r.track_id == id).and_then(|r| r.skip.clone()).unwrap_or_else(|| "it is not part of this preview".into());
        return Ok(Saved::Skipped(why));
    };
    let load = lib.load_grid(id)?;
    // `load_grid` and `save_grid` rebuild the `.str` path with `Library::track_paths`, which does not
    // refuse `..`: check the stored location with the validated resolver first
    audio_path(lib.root(), &load.location)?;
    if load.doc != prop.before {
        return Ok(Saved::Skipped("its beat grid changed since the preview; preview it again".into()));
    }
    lib.save_grid(id, &prop.after)?;
    Ok(Saved::Wrote(track_bpm(&Analysis::from(&prop.after))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::mix::{Mix, Position, Track as MixTrack};
    use pm_formats::strfile::Beat;

    const SR: u32 = 44_100;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-batch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn constant(bpm: f64, first: i64, secs: i64) -> Analysis {
        let period = f64::from(SR) * 60.0 / bpm;
        let n = ((secs * i64::from(SR) - first) as f64 / period).floor() as usize + 1;
        Analysis {
            normalize_factor: 1.0,
            beat_start_offset: first,
            beats: (0..n).map(|i| Beat { pos: first + (i as f64 * period).round() as i64, bpm }).collect(),
            first_downbeat: Some(0),
        }
    }

    /// A library with one track per `(genre, bpm, secs)`; the audio is not decodable and does not
    /// need to be for grid edits.
    fn library(tag: &str, specs: &[(&str, f64, i64)]) -> (Library, PathBuf, Vec<i64>) {
        let base = temp_dir(tag);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, (genre, bpm, secs)) in specs.iter().enumerate() {
            let p = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&p, vec![0u8; 2048 + i]).unwrap();
            let meta = TrackMeta { title: format!("Synthetic {i}"), genre: (*genre).into(), format: "MP3".into(), sample_rate: i64::from(SR), play_time_secs: *secs, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &constant(*bpm, 3000, *secs)).unwrap());
        }
        (lib, base, ids)
    }

    fn bpm_of(lib: &Library, id: i64) -> f64 {
        lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap().bpm
    }

    fn grid_of(lib: &Library, id: i64) -> GridDoc {
        lib.load_grid(id).unwrap().doc.expect("a readable grid")
    }

    fn cleanup(lib: Library, base: PathBuf) {
        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }

    /// Plans and stores, like the command does.
    fn stored(lib: &Library, state: &BatchState, ids: &[i64], op: &BatchOp) -> Arc<StoredPlan> {
        state.store(lib.root().to_str().unwrap(), plan(lib, ids, op).unwrap())
    }

    #[test]
    fn the_op_arrives_as_the_frontend_sends_it() {
        assert_eq!(serde_json::from_str::<BatchOp>(r#"{"kind":"nudge","ms":-12.5}"#).unwrap(), BatchOp::Nudge { ms: -12.5 });
        assert_eq!(serde_json::from_str::<BatchOp>(r#"{"kind":"halve"}"#).unwrap(), BatchOp::Halve);
        let s: BatchOp = serde_json::from_str(r#"{"kind":"suggested","settings":{"fallback_max":200}}"#).unwrap();
        match s {
            BatchOp::Suggested { settings } => assert_eq!(settings.fallback_max, 200.0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_nudge_moves_every_beat_by_the_same_frames_and_the_preview_says_how_far_the_first_beat_goes() {
        let (mut lib, base, ids) = library("nudge", &[("House", 126.0, 30), ("House", 120.0, 30)]);
        let state = BatchState::default();
        let p = stored(&lib, &state, &ids, &BatchOp::Nudge { ms: 20.0 });
        // 20 ms at 44.1 kHz is 882 frames
        for r in &p.rows {
            assert_eq!((r.before_first, r.after_first), (Some(3000), Some(3882)), "{r:?}");
            assert_eq!(r.before_bpm, r.after_bpm, "a nudge keeps the tempo");
            assert!(r.skip.is_none());
        }
        let before: Vec<GridDoc> = ids.iter().map(|&i| grid_of(&lib, i)).collect();
        let out = run(&mut lib, &p, &ids, |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (2, 0), "{}", out.summary);
        for (id, old) in ids.iter().zip(&before) {
            let now = grid_of(&lib, *id);
            assert!(now.beats.iter().zip(&old.beats).all(|(n, o)| n.pos == o.pos + 882));
        }
        // and back
        let back = stored(&lib, &state, &ids, &BatchOp::Nudge { ms: -20.0 });
        run(&mut lib, &back, &ids, |_| {}).unwrap();
        assert_eq!(grid_of(&lib, ids[0]).beats[0].pos, 3000);
        cleanup(lib, base);
    }

    #[test]
    fn a_nudge_outside_the_limit_or_of_nothing_is_refused_before_any_track_is_read() {
        let (lib, base, ids) = library("nudge-range", &[("House", 126.0, 20)]);
        for bad in [0.0, f64::NAN, f64::INFINITY, 5000.1, -6000.0] {
            assert!(plan(&lib, &ids, &BatchOp::Nudge { ms: bad }).is_err(), "{bad}");
        }
        assert!(plan(&lib, &ids, &BatchOp::Nudge { ms: 5000.0 }).is_ok());
        // less than a sample is not a nudge
        let rows = plan(&lib, &ids, &BatchOp::Nudge { ms: 0.001 }).unwrap();
        assert!(rows[0].row.skip.as_deref().unwrap().contains("less than one sample"));
        cleanup(lib, base);
    }

    #[test]
    fn halve_and_double_keep_row_and_grid_in_step_and_preview_old_and_new() {
        let (mut lib, base, ids) = library("hd", &[("Techno", 250.0, 30), ("Drum & Bass", 87.0, 30)]);
        let state = BatchState::default();
        let p = stored(&lib, &state, &ids[..1], &BatchOp::Halve);
        assert_eq!((p.rows[0].before_bpm, p.rows[0].after_bpm), (Some(250.0), Some(125.0)));
        assert!(p.rows[0].after_beats < p.rows[0].before_beats);
        let out = run(&mut lib, &p, &ids[..1], |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (1, 0), "{}", out.summary);
        assert_eq!(bpm_of(&lib, ids[0]), 125.0);
        assert_eq!(track_bpm(&Analysis::from(&grid_of(&lib, ids[0]))), 125.0);
        let p = stored(&lib, &state, &ids[1..], &BatchOp::Double);
        assert_eq!(p.rows[0].after_bpm, Some(174.0));
        run(&mut lib, &p, &ids[1..], |_| {}).unwrap();
        assert_eq!(bpm_of(&lib, ids[1]), 174.0);
        assert!(lib.compatibility_problems().unwrap().is_empty());
        cleanup(lib, base);
    }

    #[test]
    fn suggested_fixes_follow_the_rules_per_track_and_leave_the_rest() {
        let (mut lib, base, ids) = library(
            "sugg",
            &[("Techno", 250.0, 30), ("Drum & Bass", 87.0, 30), ("House", 126.0, 30), ("House", 172.0, 30)],
        );
        let state = BatchState::default();
        let op = BatchOp::Suggested { settings: SuspectSettings::default() };
        let p = stored(&lib, &state, &ids, &op);
        assert_eq!((p.rows[0].op.as_str(), p.rows[0].after_bpm), ("halve", Some(125.0)));
        assert_eq!((p.rows[1].op.as_str(), p.rows[1].after_bpm), ("double", Some(174.0)));
        assert!(p.rows[2].skip.as_deref().unwrap().contains("does not flag"), "{:?}", p.rows[2]);
        assert!(p.rows[3].skip.as_deref().unwrap().contains("neither half nor double"), "{:?}", p.rows[3]);
        let out = run(&mut lib, &p, &ids, |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (2, 0), "{}", out.summary);
        assert!(out.summary.contains("2 skipped"), "{}", out.summary);
        assert_eq!(bpm_of(&lib, ids[2]), 126.0);
        assert_eq!(bpm_of(&lib, ids[3]), 172.0);
        // the fixed tracks are no longer suspect under the same rules
        let report = super::super::suspect::scan(&lib, &SuspectSettings::default()).unwrap();
        let flagged: Vec<i64> = report.suspects.iter().map(|s| s.track_id).collect();
        assert_eq!(flagged, vec![ids[3]], "{report:?}");
        assert_eq!(report.checked, 4);
        cleanup(lib, base);
    }

    #[test]
    fn the_job_writes_exactly_the_previewed_grids_for_the_ticked_tracks_only() {
        let (mut lib, base, ids) = library("subset", &[("House", 100.0, 20), ("House", 110.0, 20), ("House", 120.0, 20)]);
        let state = BatchState::default();
        let p = stored(&lib, &state, &ids, &BatchOp::Double);
        let want = p.proposals[&ids[0]].after.clone();
        let mut events = Vec::new();
        // only the first and a track that is not in the plan at all
        let out = run(&mut lib, &p, &[ids[0], 987_654, ids[0]], |e| events.push((e.status, e.label))).unwrap();
        assert_eq!((out.ok, out.failed), (1, 0), "{}", out.summary);
        assert_eq!(events.len(), 2, "a repeated id is written once");
        assert_eq!(grid_of(&lib, ids[0]), want, "the saved grid is the previewed one");
        assert_eq!(bpm_of(&lib, ids[1]), 110.0, "unticked tracks are untouched");
        assert_eq!(bpm_of(&lib, ids[2]), 120.0);
        assert!(out.summary.contains("1 skipped"), "{}", out.summary);
        cleanup(lib, base);
    }

    #[test]
    fn a_track_whose_grid_changed_after_the_preview_is_not_overwritten() {
        let (mut lib, base, ids) = library("stale", &[("House", 100.0, 20), ("House", 110.0, 20)]);
        let state = BatchState::default();
        let p = stored(&lib, &state, &ids, &BatchOp::Double);
        // someone else retimes the first track after the preview
        let mut other = grid_of(&lib, ids[0]);
        other.beats.iter_mut().for_each(|b| b.pos += 5);
        lib.save_grid(ids[0], &other).unwrap();
        let out = run(&mut lib, &p, &ids, |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (1, 0), "{}", out.summary);
        assert_eq!(grid_of(&lib, ids[0]), other, "left as the other change made it");
        assert_eq!(bpm_of(&lib, ids[1]), 220.0);
        cleanup(lib, base);
    }

    #[test]
    fn mixes_that_use_a_track_are_listed_on_its_row() {
        let (mut lib, base, ids) = library("mixes", &[("House", 100.0, 20), ("House", 110.0, 20)]);
        let mix = lib.create_mix("Synthetic mix").unwrap();
        let mut doc = Mix::new("Synthetic mix");
        let pos = Position { start: 0.0, length: 4_410_000.0, limit_size: true, session_event_id: format!("{:032x}", 1), ..Position::default() };
        doc.tracks.push(MixTrack::new(ids[0], "synthetic0.mp3", 128.0, 1.0, pos));
        lib.save_mix(mix, &doc).unwrap();
        let rows = plan(&lib, &ids, &BatchOp::Double).unwrap();
        assert_eq!(rows[0].row.mixes, vec![MixRef { id: mix, name: "Synthetic mix".into() }]);
        assert!(rows[1].row.mixes.is_empty());
        cleanup(lib, base);
    }

    #[test]
    fn unknown_unreadable_and_refused_tracks_are_skipped_one_by_one_with_a_reason() {
        let (lib, base, ids) = library("odd", &[("House", 250.0, 20), ("House", 110.0, 20), ("House", 30.0, 20)]);
        let location = lib.tracks().unwrap().into_iter().find(|t| t.track_id == ids[1]).unwrap().location;
        let (_, sidecar) = lib.track_paths(&location).unwrap();
        std::fs::write(&sidecar, b"").unwrap();
        let rows = plan(&lib, &[ids[0], ids[0], 987_654, ids[1], ids[2]], &BatchOp::Double).unwrap();
        assert_eq!(rows.len(), 4, "a repeated id is planned once");
        assert!(rows[0].row.skip.as_deref().unwrap().contains("above the maximum"), "{:?}", rows[0].row);
        assert_eq!(rows[1].row.skip.as_deref(), Some("not in the library"));
        assert!(rows[2].row.skip.as_deref().unwrap().contains("empty .str"), "{:?}", rows[2].row);
        assert_eq!(rows[3].row.after_bpm, Some(60.0));
        cleanup(lib, base);
    }

    #[test]
    fn a_stored_plan_is_only_served_for_its_own_id_and_library() {
        let (lib, base, ids) = library("ids", &[("House", 100.0, 20)]);
        let state = BatchState::default();
        let a = stored(&lib, &state, &ids, &BatchOp::Double);
        let root = lib.root().to_str().unwrap().to_string();
        assert!(state.get(&root, Some(a.id)).is_ok());
        assert!(state.get(&root, None).is_ok());
        assert!(state.get("elsewhere", None).is_err());
        let b = stored(&lib, &state, &ids, &BatchOp::Halve);
        assert_ne!(a.id, b.id);
        assert!(state.get(&root, Some(a.id)).is_err(), "an older preview is replaced");
        state.clear();
        assert!(state.get(&root, None).is_err());
        cleanup(lib, base);
    }

    /// 24 s of 16-bit mono WAV with a decaying 60 Hz thump on every beat at `bpm`.
    fn click_track(path: &Path, bpm: f64) {
        let sr = 22050u32;
        let n = sr as usize * 24;
        let period = (f64::from(sr) * 60.0 / bpm) as usize;
        let mut pcm = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = (i % period) as f64 / f64::from(sr);
            let v = (-t * 40.0).exp() * (t * 2.0 * std::f64::consts::PI * 60.0).sin();
            pcm.extend_from_slice(&((v * 24000.0) as i16).to_le_bytes());
        }
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&sr.to_le_bytes());
        w.extend_from_slice(&(sr * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(&pcm);
        std::fs::write(path, w).unwrap();
    }

    #[test]
    fn a_reanalysis_preview_is_what_the_write_saves() {
        let base = temp_dir("reanalyse");
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let mut lib = Library::create(&vol).unwrap();
        let wav = src.join("synthetic click.wav");
        click_track(&wav, 126.0);
        let meta = TrackMeta { title: "Synthetic click".into(), genre: "House".into(), format: "WAV".into(), sample_rate: 22_050, play_time_secs: 24, ..Default::default() };
        // stored as if the analyser had read it at double tempo
        let id = lib.add_track(&wav, &meta, &constant(252.0, 3000, 24)).unwrap();
        let gone = lib.add_track(&{
            let p = src.join("synthetic gone.wav");
            click_track(&p, 120.0);
            p
        }, &TrackMeta { title: "Synthetic gone".into(), format: "WAV".into(), sample_rate: 22_050, play_time_secs: 24, ..Default::default() }, &constant(120.0, 3000, 24)).unwrap();
        // its audio disappears from the volume
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == gone).unwrap().location;
        std::fs::remove_file(lib.track_paths(&loc).unwrap().0).unwrap();

        let state = BatchState::default();
        let mut events = Vec::new();
        let cancel = AtomicBool::new(false);
        let planned = analyse_plan(&lib, &[id, gone, id, 987_654], &cancel, |e| events.push((e.status, e.done, e.total))).unwrap();
        assert_eq!(planned.len(), 3, "a repeated id is analysed once");
        let stored = state.store(lib.root().to_str().unwrap(), planned);
        let r = &stored.rows[0];
        assert_eq!(r.op, "reanalyse");
        assert_eq!(r.before_bpm, Some(252.0));
        let after = r.after_bpm.expect("the analyser found a tempo");
        assert!((after - 126.0).abs() < 1.0, "{after}");
        assert!(r.note.as_deref().unwrap().contains("even grid"), "{:?}", r.note);
        assert!(stored.rows[1].skip.as_deref().unwrap().contains("audio file is missing"), "{:?}", stored.rows[1]);
        assert_eq!(stored.rows[2].skip.as_deref(), Some("not in the library"));
        assert!(events.iter().all(|(_, done, total)| done <= total && *total == 3), "{events:?}");
        assert_eq!(events.iter().filter(|e| e.0 == "analysed").count(), 1);

        // nothing was written by the preview
        assert_eq!(bpm_of(&lib, id), 252.0);
        let want = stored.proposals[&id].after.clone();
        let out = run(&mut lib, &stored, &[id], |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (1, 0), "{}", out.summary);
        assert_eq!(grid_of(&lib, id), want, "the saved grid is the previewed one");
        assert_eq!(bpm_of(&lib, id), r.after_bpm.unwrap());
        cleanup(lib, base);
    }

    #[test]
    fn stopping_the_analysis_keeps_the_tracks_not_reached_as_skipped_rows() {
        let (lib, base, ids) = library("stop", &[("House", 100.0, 20)]);
        let cancel = AtomicBool::new(true);
        // the synthetic audio is not decodable; with the stop already set nothing is attempted
        let planned = analyse_plan(&lib, &ids, &cancel, |_| {}).unwrap();
        assert_eq!(planned[0].row.skip.as_deref(), Some("not analysed: the run was stopped"));
        assert!(planned[0].proposal.is_none());
        cleanup(lib, base);
    }
}
