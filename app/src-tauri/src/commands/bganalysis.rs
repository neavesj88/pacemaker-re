//! Background analysis: one low-priority worker that, for the open library on this computer,
//! makes every missing waveform, key, energy estimate and section list, whichever view is shown,
//! and carries on where it left off after a restart (it only ever looks at what is missing).
//!
//! Why this exists: waveforms used to be made only for the table rows on screen (the frontend's
//! `peakCache` asks `analysis_peaks` for a row when it is mounted), so they stopped as soon as
//! another tab was shown; keys, energy and sections were buttons. All four now come from here.
//!
//! The rules (the decisions are in [`decide`], the order and failure handling in
//! [`queue`], both tested without Tauri):
//!
//! * A library on this computer is worked on automatically while the master switch is on. A
//!   Pacemaker (a drive the device reads, possibly an ageing disk) is never touched until the user
//!   presses "Run now" for it. The run reads audio from the device, makes the waveform, energy and
//!   section results in the app's own cache, and writes the keys it finds to the device library
//!   (the only thing written there).
//! * One worker thread, one track at a time. A job the user started (add, re-analyse, sync,
//!   backup) holds the job lock; the worker checks it before every track and waits while it is held.
//!   It also waits while a preview plays (setting) and while the user has paused it.
//! * Results are saved as they come: peaks are cache files, energy and sections are merged into
//!   their cache files in batches, keys are written to `music.db` ten at a time under a
//!   [`WriteTicket`] (so an eject or backup is waited for, and the session's first write is
//!   snapshotted first, like every other write).
//! * A file that fails (or has no result, such as a key that cannot be heard) is written to a
//!   per-library failures file and skipped until its size changes or the user asks to try again.
//!
//! Frontend contract: commands below, status on [`STATUS_EVENT`], finished work on
//! [`CHANGED_EVENT`] (see `src/lib/api/bganalysis.ts`).

pub mod queue;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use pm_analysis::{decode, key, peaks};
use pm_formats::library::Library;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use self::queue::{Backend, Control, Engine, Event, Failure, Failures, Gate, Item, Kind, Outcome, RunEnd, Settings, Stop};
use super::analysis::{self, audio_path, energy, sections, PeaksState};
use super::library::kind::{library_kind, PACEMAKER};
use crate::error::{AppError, AppResult};
use crate::state::{JobLock, WriteTicket};
use crate::store::config_dir;

pub const STATUS_EVENT: &str = "bg-analysis-status";
pub const CHANGED_EVENT: &str = "bg-analysis-changed";

/// Keys are written to the library this many at a time (one write ticket and one sync per batch).
const KEY_BATCH: usize = 10;
/// ... or after this long, so the last keys of a slow pass do not wait for a full batch.
const KEY_MAX_WAIT: Duration = Duration::from_secs(3);
/// How often a held-back worker looks again while a user job or a preview holds it.
const RECHECK: Duration = Duration::from_secs(1);
/// How soon a worker that could not save or could not read the library tries again.
const RETRY_SAVE: Duration = Duration::from_secs(10);
/// The longest wait before keys are tried again after saving them failed.
const KEY_HOLD_MAX: Duration = Duration::from_secs(300);
const RETRY_SCAN: Duration = Duration::from_secs(30);
/// Status events are sent at most this often while tracks fly past (the last of a lane always goes).
const STATUS_EVERY: Duration = Duration::from_millis(120);

// ---- what the frontend sees --------------------------------------------------------------------

/// The one status line. `state` is one of: "no-library", "off", "manual" (a Pacemaker nobody has
/// started it on), "paused", "playing" (a preview is playing), "waiting" (a job of the user's
/// holds the library), "running", "idle" (nothing is missing), "blocked" (a result could not be
/// saved right now), "error" (the library could not be read).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Status {
    pub state: String,
    /// Words for "blocked" / "error".
    pub detail: String,
    pub root: Option<String>,
    /// The open library is a Pacemaker.
    pub pacemaker: bool,
    /// What is being made now ("waveforms", "keys", ...), "" when nothing is.
    pub word: String,
    /// Tracks finished / to do in the lane being worked on.
    pub done: usize,
    pub total: usize,
    /// Tracks still to do over all lanes.
    pub left: usize,
    /// The track being read.
    pub label: String,
    /// Tracks given up on (see [`Failure`]).
    pub failures: usize,
}

impl Status {
    fn new(state: &str) -> Self {
        Status { state: state.into(), detail: String::new(), root: None, pacemaker: false, word: String::new(), done: 0, total: 0, left: 0, label: String::new(), failures: 0 }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct KeyAt {
    pub track_id: i64,
    pub key: String,
}

/// Finished work, so the open views can show it without reloading.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Changed {
    /// The waveform of these tracks is now in the cache.
    Peaks { root: String, locations: Vec<String> },
    /// These keys were written to the library.
    Keys { root: String, keys: Vec<KeyAt> },
    /// The energy cache of the library changed: read the estimates again.
    Energy { root: String },
    Sections { root: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct View {
    pub settings: Settings,
    pub status: Status,
    pub failures: Vec<Failure>,
}

// ---- shared state ------------------------------------------------------------------------------

pub struct Inner {
    settings: Settings,
    root: Option<String>,
    /// The Pacemaker the user pressed "Run now" for. Cleared when another library opens.
    manual_root: Option<String>,
    paused: bool,
    playing: bool,
    /// Bumped by anything that needs a fresh look at what is missing (a library opened or
    /// reloaded, a setting switched, a retry).
    epoch: u64,
    /// Bumped by every change at all; a waiting worker wakes when it moves.
    tick: u64,
    retry: bool,
    status: Status,
    failures: Vec<Failure>,
}

pub struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
}

impl Shared {
    fn new(settings: Settings) -> Self {
        Shared {
            inner: Mutex::new(Inner {
                settings,
                root: None,
                manual_root: None,
                paused: false,
                playing: false,
                epoch: 0,
                tick: 0,
                retry: false,
                status: Status::new("no-library"),
                failures: Vec::new(),
            }),
            wake: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Changes something under the lock, then wakes the worker. `replan` asks for a new look at
    /// what is missing.
    fn change(&self, replan: bool, f: impl FnOnce(&mut Inner)) {
        let mut g = self.lock();
        f(&mut g);
        g.tick += 1;
        if replan {
            g.epoch += 1;
        }
        self.wake.notify_all();
    }
}

// ---- the decision ------------------------------------------------------------------------------

/// What [`decide`] looks at.
#[derive(Clone, Debug)]
pub struct Snap {
    pub settings: Settings,
    pub open: bool,
    pub pacemaker: bool,
    /// The user pressed "Run now" for this library.
    pub started: bool,
    pub paused: bool,
    pub playing: bool,
    pub job_running: bool,
}

/// Why the worker must not work right now, or None when it may. Checked before every track.
pub fn decide(s: &Snap) -> Option<Stop> {
    if !s.open {
        return Some(Stop::NoLibrary);
    }
    if !s.settings.any() {
        return Some(Stop::Off);
    }
    if s.pacemaker && !s.started {
        return Some(Stop::Manual);
    }
    if s.paused {
        return Some(Stop::Paused);
    }
    if s.settings.pause_during_playback && s.playing {
        return Some(Stop::Playing);
    }
    if s.job_running {
        return Some(Stop::Job);
    }
    None
}

fn snap_of(g: &Inner, job_running: bool) -> Snap {
    let pacemaker = g.root.as_deref().is_some_and(|r| library_kind(r) == PACEMAKER);
    Snap {
        settings: g.settings,
        open: g.root.is_some(),
        pacemaker,
        started: g.root.is_some() && g.root == g.manual_root,
        paused: g.paused,
        playing: g.playing,
        job_running,
    }
}

fn state_word(stop: &Stop) -> (&'static str, String) {
    match stop {
        Stop::NoLibrary => ("no-library", String::new()),
        Stop::Off => ("off", String::new()),
        Stop::Manual => ("manual", String::new()),
        Stop::Paused => ("paused", String::new()),
        Stop::Playing => ("playing", String::new()),
        Stop::Job => ("waiting", String::new()),
        Stop::Replan => ("running", String::new()),
        Stop::Blocked(why) => ("blocked", why.clone()),
        Stop::Scan(why) => ("error", why.clone()),
    }
}

// ---- settings and failures files ---------------------------------------------------------------

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("bg-analysis-settings.json")
}

/// Saved settings, or the defaults when there is no file or it cannot be read or is out of range.
pub fn load_settings(dir: &Path) -> Settings {
    std::fs::read_to_string(settings_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .filter(|s| s.validate().is_ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    s.validate()?;
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    crate::store::write_bytes(&settings_path(dir), text.as_bytes())
}

fn failures_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("bg-analysis-failures-{:016x}.json", crate::store::root_hash(root)))
}

fn load_failures(dir: &Path, root: &str) -> Failures {
    std::fs::read_to_string(failures_path(dir, root)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_failures(dir: &Path, root: &str, f: &Failures) -> Result<(), String> {
    if f.len() == 0 {
        return match std::fs::remove_file(failures_path(dir, root)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    crate::store::write_json(&failures_path(dir, root), f)
}

// ---- the outside world -------------------------------------------------------------------------

/// What the worker needs from the app, so the whole worker can be run in a test.
pub trait Env: Send + Sync {
    /// A job the user started holds the library.
    fn job_running(&self) -> bool;
    /// Starts a write to the library at `root` (snapshot first, refused while it is being ejected).
    fn begin_write(&self, root: &Path) -> Result<WriteTicket, String>;
    fn changed(&self, c: Changed);
    fn status(&self, s: &Status);
}

struct AppEnv(AppHandle);

impl Env for AppEnv {
    fn job_running(&self) -> bool {
        self.0.state::<JobLock>().is_held()
    }
    fn begin_write(&self, root: &Path) -> Result<WriteTicket, String> {
        crate::state::begin_write(&self.0, root).map_err(|e| e.to_string())
    }
    fn changed(&self, c: Changed) {
        crate::events::emit(&self.0, CHANGED_EVENT, c);
    }
    fn status(&self, s: &Status) {
        crate::events::emit(&self.0, STATUS_EVENT, s.clone());
    }
}

// ---- making the results ------------------------------------------------------------------------

/// The real work behind the queue's four lanes, for one library.
struct Real {
    env: Arc<dyn Env>,
    root: String,
    root_path: PathBuf,
    cache_dir: PathBuf,
    peaks: Arc<PeaksState>,
    energy: energy::Bg,
    sections: sections::Bg,
    key_min_confidence: f32,
    pending_keys: Vec<(i64, String)>,
    /// Set when saving keys failed (not the other lanes): the worker then leaves keys alone for a
    /// growing while so energy and sections keep going and nothing is decoded again every 10 s.
    keys_failed: bool,
    last_key_save: Instant,
    /// Set when a save failed: tried again before the next track, and the run stops while it fails.
    save_failed: Option<String>,
    /// Every location in the library at the first scan, for forgetting failures of removed tracks.
    present: Option<HashSet<String>>,
    /// Set when a save was skipped because a user job holds the library: the run's results were
    /// dropped with this struct, so the caller replans and they are made again afterwards.
    deferred: bool,
}

impl Real {
    fn new(env: Arc<dyn Env>, cfg_dir: &Path, cache_dir: &Path, peaks: Arc<PeaksState>, root: &str, settings: &Settings) -> Self {
        Real {
            env,
            root: root.to_string(),
            root_path: PathBuf::from(root),
            cache_dir: cache_dir.to_path_buf(),
            peaks,
            energy: energy::Bg::new(cfg_dir, root),
            sections: sections::Bg::new(cfg_dir, root),
            key_min_confidence: f32::from(settings.key_min_confidence_pct) / 100.0,
            pending_keys: Vec::new(),
            keys_failed: false,
            last_key_save: Instant::now(),
            save_failed: None,
            present: None,
            deferred: false,
        }
    }

    /// True while a job the user started holds the library. The worker then writes nothing (the
    /// job may be writing the same music.db or cache file); what it holds is dropped and redone.
    fn held_by_job(&mut self) -> bool {
        if !self.env.job_running() {
            return false;
        }
        if self.has_held() {
            self.deferred = true;
        }
        true
    }

    /// True while results are in memory that no save has written yet.
    fn has_held(&self) -> bool {
        !self.pending_keys.is_empty() || self.energy.has_pending() || self.sections.has_pending()
    }

    fn peak_id(&self, location: &str) -> String {
        format!("{}|{location}", self.root)
    }

    fn save_keys(&mut self) -> Result<(), String> {
        if self.pending_keys.is_empty() {
            return Ok(());
        }
        let ticket = self.env.begin_write(&self.root_path).inspect_err(|_| self.keys_failed = true)?;
        let mut lib = Library::open(&self.root_path).map_err(|e| { self.keys_failed = true; e.to_string() })?;
        // a key is only ever filled in: one the user set (or re-detected) while this result waited
        // is not replaced by the older one
        let blank: HashSet<i64> = lib.track_ids_without_key().map_err(|e| e.to_string())?.into_iter().collect();
        let todo: Vec<(i64, String)> = self.pending_keys.iter().filter(|(id, _)| blank.contains(id)).cloned().collect();
        let (_, bad) = if todo.is_empty() { (0, Vec::new()) } else { analysis::write_keys(&mut lib, &todo) };
        // told apart by looking, not by the error text: a track that is gone is skipped, a write
        // that failed for any other reason (locked, read-only or full music.db) keeps the keys
        let gone: HashSet<i64> = if bad.is_empty() {
            HashSet::new()
        } else {
            let alive: HashSet<i64> = lib.tracks().map_err(|e| e.to_string())?.into_iter().map(|t| t.track_id).collect();
            bad.iter().map(|(id, _)| *id).filter(|id| !alive.contains(id)).collect()
        };
        drop(lib);
        drop(ticket);
        if let Some((_, why)) = bad.iter().find(|(id, _)| !gone.contains(id)) {
            // the keys that did get written are announced; the rest stay held for the retry
            let bad_ids: HashSet<i64> = bad.iter().map(|(id, _)| *id).collect();
            let done: Vec<KeyAt> = todo.iter().filter(|(id, _)| !bad_ids.contains(id)).map(|(track_id, key)| KeyAt { track_id: *track_id, key: key.clone() }).collect();
            self.pending_keys.retain(|(id, _)| bad_ids.contains(id) && !gone.contains(id));
            if !done.is_empty() {
                self.env.changed(Changed::Keys { root: self.root.clone(), keys: done });
            }
            self.keys_failed = true;
            return Err(format!("could not save keys: {why}"));
        }
        // a track removed meanwhile cannot be keyed, one that got a key meanwhile keeps it
        let keys: Vec<KeyAt> = todo.into_iter().filter(|(id, _)| !gone.contains(id)).map(|(track_id, key)| KeyAt { track_id, key }).collect();
        self.pending_keys.clear();
        self.last_key_save = Instant::now();
        if !keys.is_empty() {
            self.env.changed(Changed::Keys { root: self.root.clone(), keys });
        }
        Ok(())
    }

    fn save_energy(&mut self) -> Result<(), String> {
        if self.energy.flush()? {
            self.env.changed(Changed::Energy { root: self.root.clone() });
        }
        Ok(())
    }

    fn save_sections(&mut self) -> Result<(), String> {
        if self.sections.flush()? {
            self.env.changed(Changed::Sections { root: self.root.clone() });
        }
        Ok(())
    }

    /// Saves everything held back. The first error is returned, but every part is tried.
    fn save_all(&mut self) -> Result<(), String> {
        if self.held_by_job() {
            return Ok(());
        }
        let (a, b, c) = (self.save_keys(), self.save_energy(), self.save_sections());
        a.and(b).and(c)
    }

    /// Saves what has gathered enough (or waited long enough). A failure is remembered, not
    /// returned: the track that was just made is done, the run stops before the next one.
    fn save_due(&mut self) {
        if self.held_by_job() {
            return;
        }
        let keys_due = !self.pending_keys.is_empty() && (self.pending_keys.len() >= KEY_BATCH || self.last_key_save.elapsed() >= KEY_MAX_WAIT);
        let mut err = None;
        if keys_due {
            err = self.save_keys().err();
        }
        if self.energy.due() {
            err = self.save_energy().err().or(err);
        }
        if self.sections.due() {
            err = self.save_sections().err().or(err);
        }
        if err.is_some() {
            self.save_failed = err;
        }
    }
}

impl Backend for Real {
    fn missing(&mut self, kind: Kind) -> Result<Vec<Item>, String> {
        match kind {
            Kind::Peaks | Kind::Keys => {
                let lib = Library::open_read_only(&self.root_path).map_err(|e| e.to_string())?;
                let tracks = lib.tracks().map_err(|e| e.to_string())?;
                if kind == Kind::Peaks {
                    self.present = Some(tracks.iter().map(|t| t.location.clone()).collect());
                }
                let want = |t: &pm_formats::library::Track| match kind {
                    Kind::Keys => t.key.trim().is_empty(),
                    _ => {
                        // a file that cannot be found counts as missing: the run then reports why
                        // instead of the track being silently left without a waveform
                        let current = audio_path(&self.root_path, &t.location)
                            .ok()
                            .and_then(|a| peaks::stamp_of(&a).ok())
                            .is_some_and(|st| peaks::is_current(&self.cache_dir, &self.peak_id(&t.location), st));
                        !current
                    }
                };
                Ok(tracks
                    .iter()
                    .filter(|t| want(t))
                    .map(|t| Item { id: t.track_id, location: t.location.clone(), size: t.file_size, label: analysis::label_of(t) })
                    .collect())
            }
            Kind::Energy | Kind::Sections => {
                let lib = Library::open_read_only(&self.root_path).map_err(|e| e.to_string())?;
                if kind == Kind::Energy { self.energy.missing(&lib) } else { self.sections.missing(&lib) }
            }
        }
    }

    fn process(&mut self, kind: Kind, item: &Item, _settings: &Settings) -> Outcome {
        if self.save_failed.is_some() {
            match self.save_all() {
                Ok(()) => self.save_failed = None,
                Err(e) => {
                    self.save_failed = Some(e.clone());
                    return Outcome::Later(e);
                }
            }
        }
        let out = match kind {
            Kind::Peaks => match analysis::peaks_blocking(&self.peaks, &self.cache_dir, &self.root, &item.location, 64) {
                Ok(_) => {
                    self.env.changed(Changed::Peaks { root: self.root.clone(), locations: vec![item.location.clone()] });
                    Outcome::Done { detail: None }
                }
                Err(e) => Outcome::Failed(e.to_string()),
            },
            Kind::Keys => {
                let audio = match audio_path(&self.root_path, &item.location) {
                    Ok(a) => a,
                    Err(e) => return Outcome::Failed(e.to_string()),
                };
                let detected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode::decode(&audio).map(|d| key::detect(&d.mono, d.sample_rate))));
                match detected {
                    Err(_) => Outcome::Failed("the decoder crashed on this file".into()),
                    Ok(Err(e)) => Outcome::Failed(e.to_string()),
                    Ok(Ok(None)) => Outcome::NoResult("no clear key (too short, silent or atonal)".into()),
                    Ok(Ok(Some(r))) if r.confidence < self.key_min_confidence => {
                        Outcome::NoResult(format!("low confidence: {} ({:.0}%)", r.key.name(), r.confidence * 100.0))
                    }
                    Ok(Ok(Some(r))) => {
                        self.pending_keys.push((item.id, r.key.name()));
                        Outcome::Done { detail: Some(format!("{} ({:.0}%)", r.key.name(), r.confidence * 100.0)) }
                    }
                }
            }
            Kind::Energy => match self.energy.measure_one(item) {
                Ok(m) => Outcome::Done { detail: (m != "measured").then_some(m) },
                Err(e) => Outcome::Failed(e),
            },
            Kind::Sections => match self.sections.measure_one(item) {
                Ok(m) => Outcome::Done { detail: (m != "measured").then_some(m) },
                Err(e) => Outcome::Failed(e),
            },
        };
        self.save_due();
        out
    }

    fn flush(&mut self) {
        if let Err(e) = self.save_all() {
            self.save_failed = Some(e);
        }
    }
}

// ---- what the queue reports to ---------------------------------------------------------------

struct Ctl<'a> {
    env: &'a Arc<dyn Env>,
    shared: &'a Arc<Shared>,
    root: String,
    epoch: u64,
    status: Status,
    lanes: Vec<(Kind, usize, usize)>,
    seen: Vec<Failure>,
    last_emit: Instant,
}

impl Ctl<'_> {
    fn left(&self) -> usize {
        self.lanes.iter().map(|(_, done, total)| total.saturating_sub(*done)).sum()
    }

    fn publish(&mut self, force: bool) {
        self.status.left = self.left();
        {
            let mut g = self.shared.lock();
            g.status = self.status.clone();
            g.failures.extend(self.seen.drain(..));
            self.status.failures = g.failures.len();
            g.status.failures = g.failures.len();
        }
        if force || self.last_emit.elapsed() >= STATUS_EVERY {
            self.last_emit = Instant::now();
            self.env.status(&self.status);
        }
    }
}

impl Control for Ctl<'_> {
    fn gate(&mut self) -> Gate {
        let g = self.shared.lock();
        if g.root.as_deref() != Some(self.root.as_str()) || g.epoch != self.epoch || g.retry {
            return Gate::Stop(Stop::Replan);
        }
        match decide(&snap_of(&g, self.env.job_running())) {
            Some(stop) => Gate::Stop(stop),
            None => Gate::Go,
        }
    }

    fn event(&mut self, e: Event<'_>) {
        match e {
            Event::Planned(p) => {
                self.lanes = p.iter().map(|&(k, n)| (k, 0, n)).collect();
                self.status.state = "running".into();
                self.status.detail.clear();
                self.publish(true);
            }
            Event::Starting { kind, done, total, label } => {
                self.status.state = "running".into();
                self.status.word = kind.word().into();
                self.status.done = done;
                self.status.total = total;
                self.status.label = label.into();
                self.publish(false);
            }
            Event::Result { kind, item, outcome, done, total } => {
                if let Some(l) = self.lanes.iter_mut().find(|l| l.0 == kind) {
                    l.1 = done;
                }
                self.status.done = done;
                self.status.total = total;
                if let Outcome::Failed(why) | Outcome::NoResult(why) = outcome {
                    self.seen.push(Failure { kind, track_id: item.id, label: item.label.clone(), location: item.location.clone(), size: item.size, reason: why.clone() });
                }
                self.publish(done == total);
            }
        }
    }
}

// ---- the worker --------------------------------------------------------------------------------

/// How the worker waits after one step.
#[derive(Debug, PartialEq)]
pub enum Wait {
    /// Go round again at once.
    Now,
    /// Until anything changes.
    Change(u64),
    /// Until anything changes or this long has passed.
    Timeout(Duration, u64),
}

pub struct Worker {
    env: Arc<dyn Env>,
    shared: Arc<Shared>,
    cfg_dir: PathBuf,
    cache_dir: PathBuf,
    peaks: Arc<PeaksState>,
    engine: Engine,
    engine_root: Option<String>,
    epoch: u64,
    /// After a key save failed: keys are left out of the plan until this time, and each failure in
    /// a row doubles the wait (up to KEY_HOLD_MAX), so the other lanes run and nothing is decoded in vain.
    keys_hold: Option<Instant>,
    keys_fails: u32,
}

impl Worker {
    pub fn new(env: Arc<dyn Env>, shared: Arc<Shared>, cfg_dir: PathBuf, cache_dir: PathBuf, peaks: Arc<PeaksState>) -> Self {
        Worker { env, shared, cfg_dir, cache_dir, peaks, engine: Engine::default(), engine_root: None, epoch: u64::MAX, keys_hold: None, keys_fails: 0 }
    }

    fn set_state(&self, stop: &Stop) {
        let (state, detail) = state_word(stop);
        let status = {
            let mut g = self.shared.lock();
            g.status.state = state.into();
            g.status.detail = detail;
            g.status.root = g.root.clone();
            g.status.pacemaker = g.root.as_deref().is_some_and(|r| library_kind(r) == PACEMAKER);
            if matches!(stop, Stop::NoLibrary | Stop::Off | Stop::Manual) {
                g.status.word.clear();
                g.status.label.clear();
                g.status.done = 0;
                g.status.total = 0;
                g.status.left = 0;
            }
            g.status.failures = g.failures.len();
            g.status.clone()
        };
        self.env.status(&status);
    }

    /// One look at the state and, if work is allowed, one run until the queue is empty or stopped.
    pub fn step(&mut self) -> Wait {
        let (settings, root, epoch, tick, retry, stop) = {
            let g = self.shared.lock();
            (g.settings, g.root.clone(), g.epoch, g.tick, g.retry, decide(&snap_of(&g, self.env.job_running())))
        };
        let Some(root) = root else {
            self.engine_root = None;
            self.set_state(&Stop::NoLibrary);
            return Wait::Change(tick);
        };
        if let Some(stop) = stop {
            self.set_state(&stop);
            return match stop {
                Stop::Job | Stop::Playing => Wait::Timeout(RECHECK, tick),
                _ => Wait::Change(tick),
            };
        }

        if self.engine_root.as_deref() != Some(root.as_str()) {
            self.engine = Engine::new(load_failures(&self.cfg_dir, &root));
            self.engine_root = Some(root.clone());
            self.epoch = u64::MAX;
            let mut g = self.shared.lock();
            g.failures = self.engine.failures.list();
        }
        if retry {
            self.engine.failures.clear();
            if let Err(e) = save_failures(&self.cfg_dir, &root, &self.engine.failures) {
                eprintln!("bganalysis: could not save the failures file: {e}");
            }
            let mut g = self.shared.lock();
            g.retry = false;
            g.failures.clear();
            drop(g);
            self.epoch = u64::MAX;
        }
        if self.epoch != epoch {
            self.engine.replan();
            self.epoch = epoch;
        }

        let mut real = Real::new(self.env.clone(), &self.cfg_dir, &self.cache_dir, self.peaks.clone(), &root, &settings);
        let mut status = Status::new("running");
        status.root = Some(root.clone());
        status.pacemaker = library_kind(&root) == PACEMAKER;
        status.failures = self.engine.failures.len();
        let mut ctl = Ctl { env: &self.env, shared: &self.shared, root: root.clone(), epoch, status, lanes: Vec::new(), seen: Vec::new(), last_emit: Instant::now() - STATUS_EVERY };
        let mut run_settings = settings;
        if let Some(until) = self.keys_hold {
            if Instant::now() < until {
                run_settings.keys = false;
            } else {
                // the wait is over: the keys left out of the plan are looked for again
                self.keys_hold = None;
                self.engine.replan();
            }
        }
        let end = self.engine.run(&run_settings, &mut real, &mut ctl);
        if real.keys_failed {
            self.keys_fails = self.keys_fails.saturating_add(1);
            let wait = RETRY_SAVE.saturating_mul(1u32 << self.keys_fails.min(5)).min(KEY_HOLD_MAX);
            self.keys_hold = Some(Instant::now() + wait);
        } else if self.keys_hold.is_none() && run_settings.keys == settings.keys && !real.has_held() {
            self.keys_fails = 0;
        }
        if real.deferred || real.save_failed.is_some() || real.has_held() {
            // results held back (a user job, a blocked save, a stop) die with `real` while the
            // engine has already popped their tracks: find those tracks missing again
            self.engine.replan();
        }

        // what the engine now holds is the truth; the list the frontend reads is rebuilt from it
        if let Some(present) = real.present.take() {
            let refs: HashSet<&str> = present.iter().map(String::as_str).collect();
            self.engine.failures.retain_present(&refs);
        }
        if let Err(e) = save_failures(&self.cfg_dir, &root, &self.engine.failures) {
            eprintln!("bganalysis: could not save the failures file: {e}");
        }
        {
            let mut g = self.shared.lock();
            g.failures = self.engine.failures.list();
        }
        let mut end = end;
        if end == RunEnd::Idle {
            if let Some(e) = real.save_failed.take() {
                // the last results could not be saved: look again, they are still missing
                self.engine.replan();
                end = RunEnd::Stopped(Stop::Blocked(e));
            }
        }
        match end {
            RunEnd::Idle => {
                {
                    let mut g = self.shared.lock();
                    g.status.state = "idle".into();
                    g.status.word.clear();
                    g.status.label.clear();
                    g.status.left = 0;
                    g.status.failures = g.failures.len();
                    let s = g.status.clone();
                    drop(g);
                    self.env.status(&s);
                }
                match self.keys_hold {
                    // keys are waiting out a failed save: come back when the wait is over
                    Some(until) => Wait::Timeout(until.saturating_duration_since(Instant::now()).max(Duration::from_secs(1)), tick_now(&self.shared)),
                    None => Wait::Change(tick_now(&self.shared)),
                }
            }
            RunEnd::Stopped(Stop::Replan) => Wait::Now,
            RunEnd::Stopped(stop) => {
                self.set_state(&stop);
                let tick = tick_now(&self.shared);
                match stop {
                    Stop::Job | Stop::Playing => Wait::Timeout(RECHECK, tick),
                    Stop::Blocked(_) => Wait::Timeout(self.keys_hold.map_or(RETRY_SAVE, |u| u.saturating_duration_since(Instant::now()).max(RETRY_SAVE)), tick),
                    Stop::Scan(_) => Wait::Timeout(RETRY_SCAN, tick),
                    _ => Wait::Change(tick),
                }
            }
        }
    }

    fn wait(&self, w: Wait) {
        let mut g = self.shared.lock();
        match w {
            Wait::Now => {}
            Wait::Change(seen) => {
                while g.tick == seen {
                    g = self.shared.wake.wait(g).unwrap_or_else(|e| e.into_inner());
                }
            }
            Wait::Timeout(d, seen) => {
                if g.tick == seen {
                    let _ = self.shared.wake.wait_timeout(g, d).unwrap_or_else(|e| e.into_inner());
                }
            }
        }
    }

    fn run_forever(mut self) {
        loop {
            let w = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.step())).unwrap_or_else(|_| {
                // a bug must not end the queue for the rest of the session; look again shortly
                self.engine.replan();
                Wait::Timeout(RETRY_SAVE, tick_now(&self.shared))
            });
            self.wait(w);
        }
    }
}

fn tick_now(shared: &Shared) -> u64 {
    shared.lock().tick
}

pub fn setup(app: &mut tauri::App) {
    let handle = app.handle().clone();
    let (Ok(cfg_dir), Ok(cache)) = (config_dir(&handle), handle.path().app_cache_dir()) else {
        eprintln!("bganalysis: the app folders are not available, so background analysis is off this session");
        return;
    };
    let shared = Arc::new(Shared::new(load_settings(&cfg_dir)));
    app.manage(shared.clone());
    let peaks = app.state::<Arc<PeaksState>>().inner().clone();
    let worker = Worker::new(Arc::new(AppEnv(handle)), shared, cfg_dir, cache.join("peaks"), peaks);
    let spawned = std::thread::Builder::new().name("background-analysis".into()).spawn(move || worker.run_forever());
    if let Err(e) = spawned {
        eprintln!("bganalysis: could not start the worker: {e}");
    }
}

// ---- commands ----------------------------------------------------------------------------------

fn view_of(shared: &Shared) -> View {
    let g = shared.lock();
    View { settings: g.settings, status: g.status.clone(), failures: g.failures.clone() }
}

/// The settings, the status line and the "couldn't analyse" list.
#[tauri::command]
pub fn bganalysis_get(shared: State<'_, Arc<Shared>>) -> View {
    view_of(&shared)
}

/// Saves the settings (every value checked) and makes the worker look again at what is missing.
#[tauri::command]
pub fn bganalysis_save_settings(app: AppHandle, shared: State<'_, Arc<Shared>>, settings: Settings) -> AppResult<Settings> {
    save_settings(&config_dir(&app)?, &settings)?;
    shared.change(true, |g| g.settings = settings);
    Ok(settings)
}

/// Tells the worker which library is open (null: none), from the frontend's session. Called on
/// every open and every in-place reload, so a finished add or sync is looked at again. Opening a
/// different library forgets a Pacemaker the user started a run on.
#[tauri::command]
pub fn bganalysis_watch(shared: State<'_, Arc<Shared>>, root: Option<String>) {
    shared.change(true, |g| {
        if g.root != root {
            g.manual_root = None;
            g.failures.clear();
        }
        g.root = root;
    });
}

/// Pause / resume from the status strip. Not remembered between sessions.
#[tauri::command]
pub fn bganalysis_set_paused(shared: State<'_, Arc<Shared>>, paused: bool) {
    shared.change(false, |g| g.paused = paused);
}

/// The player is playing a track or a mix preview (the worker waits while it does, if the
/// setting says so).
#[tauri::command]
pub fn bganalysis_set_playing(shared: State<'_, Arc<Shared>>, playing: bool) {
    shared.change(false, |g| g.playing = playing);
}

/// "Run now": works on the open library even if it is a Pacemaker, which is otherwise never
/// touched. Refused while the master switch is off.
#[tauri::command]
pub fn bganalysis_run_now(shared: State<'_, Arc<Shared>>, root: String) -> AppResult<()> {
    let mut refused = None;
    shared.change(true, |g| {
        if !g.settings.any() {
            refused = Some("Background analysis is switched off in Settings (or every kind is off).");
        } else if g.root.as_deref() != Some(root.as_str()) {
            refused = Some("That library is not the open one.");
        } else {
            g.manual_root = Some(root.clone());
            g.paused = false;
        }
    });
    match refused {
        Some(why) => Err(AppError::from(why)),
        None => Ok(()),
    }
}

/// Forgets what was given up on, so those tracks are tried again.
#[tauri::command]
pub fn bganalysis_retry_failures(shared: State<'_, Arc<Shared>>) {
    shared.change(true, |g| g.retry = true);
}


/// This area's command paths, one per line.
#[macro_export]
macro_rules! bganalysis_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::bganalysis::bganalysis_get,
                $crate::commands::bganalysis::bganalysis_save_settings,
                $crate::commands::bganalysis::bganalysis_watch,
                $crate::commands::bganalysis::bganalysis_set_paused,
                $crate::commands::bganalysis::bganalysis_set_playing,
                $crate::commands::bganalysis::bganalysis_run_now,
                $crate::commands::bganalysis::bganalysis_retry_failures,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WriteGate;
    use pm_formats::library::TrackMeta;
    use pm_formats::strfile;
    use std::sync::atomic::{AtomicBool, Ordering};

    const SR: u32 = 22050;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-bg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn wav_bytes(samples: &[f32]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&SR.to_le_bytes());
        w.extend_from_slice(&(SR * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        w
    }

    /// I-IV-V-I chords in the key whose tonic is `tonic`, `reps` times round (22 s or more).
    fn chords(tonic: i32, minor: bool, reps: usize) -> Vec<f32> {
        let third = if minor { 3 } else { 4 };
        let mut out = Vec::new();
        for _ in 0..reps {
            for root in [0, 5, 7, 0, 0, 5, 7, 0] {
                let notes = [root, root + third, root + 7];
                let n = SR as usize;
                let mut seg = vec![0f32; n];
                for (j, off) in notes.iter().enumerate() {
                    let midi = (60 + tonic + off) as f32;
                    let f0 = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
                    for h in 1..=3 {
                        let w = 2.0 * std::f32::consts::PI * f0 * h as f32 / SR as f32;
                        for (i, s) in seg.iter_mut().enumerate() {
                            *s += (0.25 + 0.05 * j as f32) / h as f32 * (w * i as f32).sin();
                        }
                    }
                }
                out.extend(seg);
            }
        }
        out
    }

    struct TestEnv {
        job: AtomicBool,
        /// While set, begin_write refuses (a blocked save).
        refuse: AtomicBool,
        gate: WriteGate,
        changed: Mutex<Vec<Changed>>,
        statuses: Mutex<Vec<Status>>,
    }

    impl TestEnv {
        fn new() -> Arc<Self> {
            Arc::new(TestEnv { job: AtomicBool::new(false), refuse: AtomicBool::new(false), gate: WriteGate::default(), changed: Mutex::new(Vec::new()), statuses: Mutex::new(Vec::new()) })
        }
        fn peaks_events(&self) -> usize {
            self.changed.lock().unwrap().iter().filter(|c| matches!(c, Changed::Peaks { .. })).count()
        }
        fn key_events(&self) -> Vec<KeyAt> {
            self.changed.lock().unwrap().iter().filter_map(|c| if let Changed::Keys { keys, .. } = c { Some(keys.clone()) } else { None }).flatten().collect()
        }
        fn last_state(&self) -> String {
            self.statuses.lock().unwrap().last().map(|s| s.state.clone()).unwrap_or_default()
        }
    }

    impl Env for TestEnv {
        fn job_running(&self) -> bool {
            self.job.load(Ordering::SeqCst)
        }
        fn begin_write(&self, root: &Path) -> Result<WriteTicket, String> {
            if self.refuse.load(Ordering::SeqCst) {
                return Err("write refused".into());
            }
            self.gate.begin_write_with(root, || Ok(()))
        }
        fn changed(&self, c: Changed) {
            self.changed.lock().unwrap().push(c);
        }
        fn status(&self, s: &Status) {
            self.statuses.lock().unwrap().push(s.clone());
        }
    }

    /// A library on "this computer" (a temp folder) with these files added, three of them real
    /// audio of 24 s and one that is not audio.
    fn library(tag: &str) -> (PathBuf, PathBuf, Vec<i64>) {
        let base = temp_dir(tag);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        let files: Vec<(&str, Vec<u8>)> = vec![
            ("a.wav", wav_bytes(&chords(7, false, 3))),
            ("b.wav", wav_bytes(&chords(2, true, 3))),
            ("broken.wav", b"this is not audio".to_vec()),
            ("c.wav", wav_bytes(&chords(0, false, 3))),
        ];
        for (name, bytes) in files {
            let p = src.join(name);
            std::fs::write(&p, bytes).unwrap();
            let meta = TrackMeta { title: name.to_string(), format: "WAV".into(), ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        drop(lib);
        (base, vol, ids)
    }

    fn worker(env: &Arc<TestEnv>, base: &Path, vol: &Path, settings: Settings) -> (Worker, Arc<Shared>) {
        let shared = Arc::new(Shared::new(settings));
        shared.change(true, |g| g.root = Some(vol.display().to_string()));
        let w = Worker::new(env.clone(), shared.clone(), base.join("cfg"), base.join("cache"), Arc::new(PeaksState::default()));
        std::fs::create_dir_all(base.join("cfg")).unwrap();
        (w, shared)
    }

    fn keys_of(vol: &Path) -> Vec<String> {
        Library::open_read_only(vol).unwrap().tracks().unwrap().into_iter().map(|t| t.key).collect()
    }

    fn peak_files(base: &Path) -> usize {
        std::fs::read_dir(base.join("cache")).map(|d| d.count()).unwrap_or(0)
    }

    #[test]
    fn decide_applies_the_rules_in_order() {
        let ok = Snap { settings: Settings::default(), open: true, pacemaker: false, started: false, paused: false, playing: false, job_running: false };
        assert_eq!(decide(&ok), None, "a library on this computer is worked on by itself");
        assert_eq!(decide(&Snap { open: false, ..ok.clone() }), Some(Stop::NoLibrary));
        assert_eq!(decide(&Snap { settings: Settings { enabled: false, ..ok.settings }, ..ok.clone() }), Some(Stop::Off));
        let none = Settings { waveforms: false, keys: false, energy: false, sections: false, ..ok.settings };
        assert_eq!(decide(&Snap { settings: none, ..ok.clone() }), Some(Stop::Off), "every kind off is off");
        assert_eq!(decide(&Snap { pacemaker: true, ..ok.clone() }), Some(Stop::Manual), "a Pacemaker waits for the user");
        assert_eq!(decide(&Snap { pacemaker: true, started: true, ..ok.clone() }), None);
        assert_eq!(decide(&Snap { paused: true, ..ok.clone() }), Some(Stop::Paused));
        assert_eq!(decide(&Snap { playing: true, ..ok.clone() }), Some(Stop::Playing));
        let ignore = Settings { pause_during_playback: false, ..ok.settings };
        assert_eq!(decide(&Snap { settings: ignore, playing: true, ..ok.clone() }), None, "the playback wait is a setting");
        assert_eq!(decide(&Snap { job_running: true, ..ok.clone() }), Some(Stop::Job));
        // the switch beats everything else, so a Pacemaker with the switch off says "off"
        assert_eq!(decide(&Snap { settings: Settings { enabled: false, ..ok.settings }, pacemaker: true, paused: true, ..ok }), Some(Stop::Off));
    }

    #[test]
    fn a_pacemaker_root_is_never_started_by_a_plain_library_open() {
        let s = Shared::new(Settings::default());
        s.change(true, |g| g.root = Some("G:\\".into()));
        let g = s.lock();
        assert_eq!(decide(&snap_of(&g, false)), Some(Stop::Manual));
        drop(g);
        s.change(true, |g| g.manual_root = Some("G:\\".into()));
        assert_eq!(decide(&snap_of(&s.lock(), false)), None);
        s.change(true, |g| g.root = Some("H:\\".into()));
        // the run was for G:, not for H:
        let started = {
            let g = s.lock();
            snap_of(&g, false).started
        };
        assert!(!started);
    }

    #[test]
    fn a_pass_makes_every_missing_result_once_and_a_second_pass_does_nothing() {
        let (base, vol, ids) = library("pass");
        let env = TestEnv::new();
        let (mut w, shared) = worker(&env, &base, &vol, Settings::default());

        let first = w.step();
        assert!(matches!(first, Wait::Change(_)), "{first:?}");
        assert_eq!(env.last_state(), "idle");
        // three waveforms (the broken file has none), three keys, none for the bad one
        assert_eq!(peak_files(&base), 3);
        assert_eq!(env.peaks_events(), 3);
        let keys = keys_of(&vol);
        assert_eq!((keys[0].as_str(), keys[1].as_str(), keys[2].as_str(), keys[3].as_str()), ("G", "Dm", "", "C"), "{keys:?}");
        let announced: Vec<(i64, String)> = env.key_events().into_iter().map(|k| (k.track_id, k.key)).collect();
        assert_eq!(announced, vec![(ids[0], "G".to_string()), (ids[1], "Dm".to_string()), (ids[3], "C".to_string())]);
        assert!(env.changed.lock().unwrap().iter().any(|c| matches!(c, Changed::Energy { .. })), "the energy cache was saved and announced");
        assert!(env.changed.lock().unwrap().iter().any(|c| matches!(c, Changed::Sections { .. })));

        // the broken file is listed once per kind that could not read it, with a reason
        let view = view_of(&shared);
        assert!(!view.failures.is_empty());
        assert!(view.failures.iter().all(|f| f.track_id == ids[2] && !f.reason.is_empty()), "{:?}", view.failures);
        assert_eq!(view.status.failures, view.failures.len());
        assert!(base.join("cfg").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("bg-analysis-failures-")), "kept in a file");

        // a second look (a reload of the library) finds nothing missing and does not retry the failure
        let before = env.changed.lock().unwrap().len();
        shared.change(true, |_| {});
        w.step();
        assert_eq!(env.changed.lock().unwrap().len(), before, "no new work");
        assert_eq!(env.last_state(), "idle");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn after_a_restart_only_what_is_still_missing_is_made() {
        let (base, vol, _) = library("restart");
        let env = TestEnv::new();
        // the first session only had waveforms switched on
        let only_peaks = Settings { keys: false, energy: false, sections: false, ..Settings::default() };
        let (mut w, _) = worker(&env, &base, &vol, only_peaks);
        w.step();
        assert_eq!(env.peaks_events(), 3);
        assert_eq!(keys_of(&vol), vec!["", "", "", ""]);

        // a new process: a new worker, same files, everything on
        let env2 = TestEnv::new();
        let (mut w2, shared2) = worker(&env2, &base, &vol, Settings::default());
        w2.step();
        assert_eq!(env2.peaks_events(), 0, "the waveforms on disk were not made again");
        assert_eq!(keys_of(&vol)[0], "G");
        // and the failure of the broken file from the first session was remembered
        assert!(view_of(&shared2).failures.iter().any(|f| f.kind == Kind::Peaks));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn switched_off_it_touches_nothing_and_switching_on_starts_it() {
        let (base, vol, _) = library("off");
        let env = TestEnv::new();
        let (mut w, shared) = worker(&env, &base, &vol, Settings { enabled: false, ..Settings::default() });
        assert!(matches!(w.step(), Wait::Change(_)));
        assert_eq!(env.last_state(), "off");
        assert_eq!(peak_files(&base), 0);
        assert_eq!(keys_of(&vol), vec!["", "", "", ""]);
        assert!(env.changed.lock().unwrap().is_empty());

        shared.change(true, |g| g.settings.enabled = true);
        w.step();
        assert_eq!(env.last_state(), "idle");
        assert_eq!(peak_files(&base), 3);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_user_job_holds_the_worker_back_and_a_kind_switched_off_is_left_alone() {
        let (base, vol, _) = library("job");
        let env = TestEnv::new();
        let s = Settings { keys: false, energy: false, sections: false, ..Settings::default() };
        let (mut w, _) = worker(&env, &base, &vol, s);
        env.job.store(true, Ordering::SeqCst);
        assert!(matches!(w.step(), Wait::Timeout(..)), "it looks again soon");
        assert_eq!(env.last_state(), "waiting");
        assert_eq!(peak_files(&base), 0, "nothing was read while the user's job held the library");

        env.job.store(false, Ordering::SeqCst);
        w.step();
        assert_eq!(peak_files(&base), 3);
        assert_eq!(keys_of(&vol), vec!["", "", "", ""], "keys were switched off");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn pausing_and_playback_hold_it_and_resuming_continues() {
        let (base, vol, _) = library("pause");
        let env = TestEnv::new();
        let s = Settings { keys: false, energy: false, sections: false, ..Settings::default() };
        let (mut w, shared) = worker(&env, &base, &vol, s);
        shared.change(false, |g| g.paused = true);
        w.step();
        assert_eq!(env.last_state(), "paused");
        assert_eq!(peak_files(&base), 0);
        shared.change(false, |g| {
            g.paused = false;
            g.playing = true;
        });
        w.step();
        assert_eq!(env.last_state(), "playing");
        assert_eq!(peak_files(&base), 0);
        shared.change(false, |g| g.playing = false);
        w.step();
        assert_eq!(peak_files(&base), 3);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn retrying_gives_the_failed_tracks_another_go() {
        let (base, vol, ids) = library("retry");
        let env = TestEnv::new();
        let s = Settings { keys: false, energy: false, sections: false, ..Settings::default() };
        let (mut w, shared) = worker(&env, &base, &vol, s);
        w.step();
        assert_eq!(view_of(&shared).failures.len(), 1);
        // the file is repaired, but the same size is still skipped ...
        assert_eq!(view_of(&shared).failures[0].track_id, ids[2], "the broken file");
        shared.change(true, |_| {});
        w.step();
        assert_eq!(view_of(&shared).failures.len(), 1);
        // ... until the user asks for another try, which makes it fail again with the same reason
        shared.change(true, |g| g.retry = true);
        w.step();
        let v = view_of(&shared);
        assert_eq!((v.failures.len(), v.failures[0].track_id), (1, ids[2]));
        assert!(!shared.lock().retry);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn keys_below_the_confidence_floor_are_listed_not_written() {
        let (base, vol, _) = library("floor");
        let env = TestEnv::new();
        let s = Settings { waveforms: false, energy: false, sections: false, key_min_confidence_pct: 100, ..Settings::default() };
        let (mut w, shared) = worker(&env, &base, &vol, s);
        w.step();
        assert_eq!(keys_of(&vol), vec!["", "", "", ""]);
        let v = view_of(&shared);
        assert!(v.failures.iter().any(|f| f.reason.starts_with("low confidence")), "{:?}", v.failures);
        std::fs::remove_dir_all(&base).unwrap();
    }

    fn real_for(env: &Arc<TestEnv>, base: &Path, vol: &Path) -> Real {
        std::fs::create_dir_all(base.join("cfg")).unwrap();
        let mut real = Real::new(env.clone(), &base.join("cfg"), &base.join("cache"), Arc::new(PeaksState::default()), &vol.display().to_string(), &Settings::default());
        // no timed auto-save during a test, however slow the machine is
        real.last_key_save = Instant::now() + Duration::from_secs(3600);
        real
    }

    #[test]
    fn a_key_set_while_a_result_waited_is_not_replaced() {
        let (base, vol, ids) = library("keep-key");
        let env = TestEnv::new();
        let mut real = real_for(&env, &base, &vol);
        let items = real.missing(Kind::Keys).unwrap();
        assert!(items.iter().any(|i| i.id == ids[0]));
        let a = items.iter().find(|i| i.id == ids[0]).unwrap().clone();
        let b = items.iter().find(|i| i.id == ids[1]).unwrap().clone();
        // process() holds the detected keys back, the user sets one of them meanwhile
        assert!(matches!(real.process(Kind::Keys, &a, &Settings::default()), Outcome::Done { .. }));
        assert!(matches!(real.process(Kind::Keys, &b, &Settings::default()), Outcome::Done { .. }));
        assert_eq!(real.pending_keys.len(), 2, "both are still waiting");
        Library::open(&vol).unwrap().set_track_key(ids[0], "F#m").unwrap();
        real.flush();
        let keys = keys_of(&vol);
        assert_eq!(keys[0], "F#m", "the user's key survives the worker's older result");
        assert!(!keys[1].is_empty(), "the other track still got its key");
        let events: Vec<i64> = env.key_events().iter().map(|k| k.track_id).collect();
        assert_eq!(events, vec![ids[1]], "only what was written is announced");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn nothing_is_written_while_a_user_job_holds_the_library_and_the_tracks_are_redone() {
        let (base, vol, _) = library("job-flush");
        let env = TestEnv::new();
        let s = Settings { waveforms: false, energy: false, sections: false, ..Settings::default() };
        let (mut w, _) = worker(&env, &base, &vol, s);
        let mut real = real_for(&env, &base, &vol);
        let item = real.missing(Kind::Keys).unwrap().remove(0);
        // a job starts while the track is being made: its result must not be saved
        assert!(matches!(real.process(Kind::Keys, &item, &s), Outcome::Done { .. }));
        env.job.store(true, Ordering::SeqCst);
        real.flush();
        assert!(real.deferred);
        assert_eq!(keys_of(&vol), vec!["", "", "", ""], "the job's library was not touched");
        assert!(env.key_events().is_empty());
        // once the job is over the worker finds the track missing again and keys it
        env.job.store(false, Ordering::SeqCst);
        w.step();
        assert!(keys_of(&vol).iter().filter(|k| !k.is_empty()).count() >= 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn results_lost_to_a_blocked_save_are_made_again() {
        let (base, vol, _) = library("blocked");
        let env = TestEnv::new();
        let s = Settings { waveforms: false, energy: false, sections: false, ..Settings::default() };
        let (mut w, _) = worker(&env, &base, &vol, s);
        env.refuse.store(true, Ordering::SeqCst);
        w.step();
        assert_eq!(env.last_state(), "blocked");
        assert_eq!(keys_of(&vol), vec!["", "", "", ""], "nothing could be written");
        assert!(w.keys_hold.is_some(), "keys wait before the next try");
        // the save works again, but the wait is still running: keys are not decoded in vain
        env.refuse.store(false, Ordering::SeqCst);
        w.step();
        assert_eq!(keys_of(&vol), vec!["", "", "", ""], "keys are left alone during the wait");
        w.keys_hold = Some(Instant::now() - Duration::from_secs(1));
        w.step();
        assert_eq!(keys_of(&vol).iter().filter(|k| !k.is_empty()).count(), 3, "the dropped keys were redone");
        assert!(w.keys_hold.is_none() && w.keys_fails == 0, "a good save ends the back-off");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_failing_key_save_does_not_stop_the_energy_lane() {
        let (base, vol, _) = library("keyhold");
        let env = TestEnv::new();
        let s = Settings { waveforms: false, sections: false, ..Settings::default() };
        let (mut w, _) = worker(&env, &base, &vol, s);
        env.refuse.store(true, Ordering::SeqCst);
        w.step();
        assert_eq!(env.last_state(), "blocked");
        assert!(w.keys_hold.is_some());
        let energy_events = || env.changed.lock().unwrap().iter().filter(|c| matches!(c, Changed::Energy { .. })).count();
        let before = energy_events();
        // still refused, still inside the wait: keys are skipped, energy gets its turn
        w.step();
        assert!(energy_events() > before, "energy ran while keys waited");
        assert_eq!(keys_of(&vol), vec!["", "", "", ""]);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_blocked_save_keeps_the_results_visible_to_step_so_it_replans() {
        let (base, vol, _) = library("held");
        let env = TestEnv::new();
        let mut real = real_for(&env, &base, &vol);
        let items = real.missing(Kind::Keys).unwrap();
        env.refuse.store(true, Ordering::SeqCst);
        assert!(matches!(real.process(Kind::Keys, &items[0], &Settings::default()), Outcome::Done { .. }));
        // the batch timer runs out and the save is refused
        real.last_key_save = Instant::now() - KEY_MAX_WAIT - Duration::from_secs(1);
        real.save_due();
        assert!(real.save_failed.is_some() && real.has_held(), "step() replans on exactly this state");
        assert!(matches!(real.process(Kind::Keys, &items[1], &Settings::default()), Outcome::Later(_)));
        assert!(real.has_held(), "still held after the run stopped Blocked");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_failing_database_write_keeps_the_keys_and_reports_it() {
        let (base, vol, _) = library("dbfail");
        let env = TestEnv::new();
        let mut real = real_for(&env, &base, &vol);
        let items = real.missing(Kind::Keys).unwrap();
        assert!(matches!(real.process(Kind::Keys, &items[0], &Settings::default()), Outcome::Done { .. }));
        assert_eq!(real.pending_keys.len(), 1);
        // music.db turns read-only (locked, full or protected drives fail the same way)
        let db = std::fs::read_dir(&vol).unwrap().flatten().flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten()).map(|d| d.path()).find(|p| p.file_name().is_some_and(|n| n == "music.db")).expect("music.db");
        let mut perm = std::fs::metadata(&db).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&db, perm.clone()).unwrap();
        let r = real.save_keys();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&db, perm).unwrap();
        assert!(r.is_err(), "a failed write must not look like a removed track");
        assert_eq!(real.pending_keys.len(), 1, "the key is kept for the retry");
        assert!(real.keys_failed);
        assert!(env.key_events().is_empty());
        // and once the database is writable again the held key is saved
        real.save_keys().unwrap();
        assert!(real.pending_keys.is_empty());
        assert_eq!(keys_of(&vol).iter().filter(|k| !k.is_empty()).count(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_removed_track_is_skipped_not_an_error() {
        let (base, vol, ids) = library("gone");
        let env = TestEnv::new();
        let mut real = real_for(&env, &base, &vol);
        let items = real.missing(Kind::Keys).unwrap();
        let item = items.iter().find(|i| i.id == ids[0]).unwrap();
        assert!(matches!(real.process(Kind::Keys, item, &Settings::default()), Outcome::Done { .. }));
        real.pending_keys.push((9999, "Am".into()));
        // an id the library does not have: dropped quietly, the real key still saved
        real.save_keys().unwrap();
        assert!(real.pending_keys.is_empty() && !real.keys_failed);
        assert_eq!(env.key_events().len(), 1);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_library_that_cannot_be_read_is_an_error_state_not_a_crash() {
        let base = temp_dir("noread");
        let env = TestEnv::new();
        let (mut w, _) = worker(&env, &base, &base.join("nothing-here"), Settings::default());
        assert!(matches!(w.step(), Wait::Timeout(..)));
        assert_eq!(env.last_state(), "error");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn settings_file_round_trips_and_a_bad_file_means_defaults() {
        let dir = temp_dir("settings");
        assert_eq!(load_settings(&dir), Settings::default());
        let s = Settings { keys: false, key_min_confidence_pct: 40, ..Settings::default() };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), s);
        assert!(save_settings(&dir, &Settings { key_min_confidence_pct: 101, ..s }).is_err());
        assert_eq!(load_settings(&dir), s, "a refused save leaves the old file");
        std::fs::write(settings_path(&dir), "{ not json").unwrap();
        assert_eq!(load_settings(&dir), Settings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
