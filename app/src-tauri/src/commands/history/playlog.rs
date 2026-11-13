//! The real play log: an append-only, app-side record of how the device's play counters changed.
//!
//! The Pacemaker keeps only `times_played` and `last_played` per track. Each time a library is
//! read, [`PlayLog::record`] compares every track's counters with what the log last knew and
//! stores only the tracks that changed. Plays are then *derived* from those changes ([`PlayLog::hits`]).
//! Nothing here touches the device; the log is a JSON file in the app config dir, one per library
//! (see [`store_path`]), written atomically.
//!
//! What the log can and cannot know (also worded in the UI):
//!
//! * A snapshot only exists when the library was opened in this app. Plays between two openings
//!   arrive together: a track whose count rose by 3 was played 3 times in that window, but only
//!   the time of the most recent play is known (the device's `last_played`). The two earlier plays
//!   are placed at that same time and marked approximate. A track played on three nights between
//!   two openings therefore looks like one night with three plays.
//! * A "session" is plays whose times are within a gap of each other (a user setting). It is a
//!   cluster of timestamps, not something the device recorded; a track played twice in one
//!   session shows once with a count, not in its real position in the running order.
//! * Times are the device's own clock. If it was wrong the log is wrong the same way. A time later
//!   than the moment of the snapshot is clamped to it and marked as not known.
//! * A counter that goes backwards (device reset, or the track replaced) restarts from zero: the
//!   new count is taken as plays since the reset. A count that did not move but whose
//!   `last_played` did is taken as one play (approximate). A count that rose with no newer
//!   `last_played` is placed at the snapshot time (time not known).
//! * Plays before logging began exist only as the *baseline* (the counters at the first snapshot),
//!   so a track's history is "N plays before the log" plus the logged ones. Plays in this app's own
//!   player are not counted: only the device writes these counters.
//!
//! Identity: a track is keyed by its `global_id` (random per track, assigned at import), falling
//! back to `#<track_id>` when it has none, so renumbering does not lose history. If most of the
//! tracks the log has seen played are gone from the library, it is probably a different card at
//! the same drive letter and [`Outcome::Mismatch`] is returned instead of recording nonsense.
//! "Most" is a setting (`identity_min_present_pct`, 0 switches the check off), and the user can
//! overrule it with [`PlayLog::adopt`] ("keep this log and continue"): the tracks the library no
//! longer has are set aside (`away`: kept, but not counted by the check, and compared against their
//! old counters if they come back) and tracks the log has never seen start from their present
//! counters, so a clean-out or a new gig set is not mistaken for a pile of plays.
//!
//! Which log belongs to a library: the card holds no identity of its own that the app may rely on
//! (music.db has no library id, and writing one to the card is not done), so the file is named
//! after the drive or path the card is mounted at (see [`store_path`]) and a card that turns up at
//! a new drive letter would start a second log. To avoid that, a library with no log yet is
//! compared with the other logs ([`candidates`]) and, if one matches most of its tracks, the user is
//! asked whether to continue that log ([`adopt_file`]) before a new one starts.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use pm_formats::ops_history::PlayCounter;
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const DEFAULT_RETENTION_DAYS: i64 = 730;
pub const DEFAULT_GAP_MINUTES: i64 = 240;
/// Largest retention the setting accepts (100 years); 0 keeps the log for ever.
pub const MAX_RETENTION_DAYS: i64 = 36_500;
/// Default cap on the changes the file holds, whatever the retention says: past it the oldest are
/// folded into the baseline like retention does (dates lost, counts kept). A setting.
pub const DEFAULT_MAX_EVENTS: i64 = 400_000;
pub const MIN_MAX_EVENTS: i64 = 1_000;
pub const MAX_MAX_EVENTS: i64 = 2_000_000;
/// Default for the "different card" check: fewer than this percentage of the played tracks the
/// log knows still being in the library means another library. A setting; 0 switches it off.
pub const DEFAULT_IDENTITY_PCT: i64 = 50;
/// Below this many played tracks in a log the "different card" check is not made: with so few
/// there is nothing to judge by. Not a policy, a floor on how little evidence counts.
const IDENTITY_MIN_ACTIVE: usize = 5;
const MS_PER_DAY: i64 = 86_400_000;

// ---- settings ---------------------------------------------------------------------------

/// The user's choices for the log; app-wide, saved in the config dir.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Record a snapshot each time a library is read.
    pub enabled: bool,
    /// Days of changes kept; older ones are folded into the baseline. 0 keeps everything.
    pub retention_days: i64,
    /// A pause longer than this many minutes ends a session.
    pub session_gap_minutes: i64,
    /// Cap on the changes kept in the file (the oldest fold into the baseline past it).
    pub max_events: i64,
    /// Percentage of the played tracks the log knows that must still be in the library, or it is
    /// taken for a different card and nothing is recorded until the user decides. 0 never refuses.
    pub identity_min_present_pct: i64,
    /// A library with no log of its own, that matches the log of another drive, is not logged
    /// until the user chooses between continuing that log and starting a new one.
    pub ask_before_new_log: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            retention_days: DEFAULT_RETENTION_DAYS,
            session_gap_minutes: DEFAULT_GAP_MINUTES,
            max_events: DEFAULT_MAX_EVENTS,
            identity_min_present_pct: DEFAULT_IDENTITY_PCT,
            ask_before_new_log: true,
        }
    }
}

impl Settings {
    pub fn sanitised(mut self) -> Settings {
        self.retention_days = self.retention_days.clamp(0, MAX_RETENTION_DAYS);
        self.session_gap_minutes = self.session_gap_minutes.clamp(1, 24 * 60);
        self.max_events = self.max_events.clamp(MIN_MAX_EVENTS, MAX_MAX_EVENTS);
        self.identity_min_present_pct = self.identity_min_present_pct.clamp(0, 100);
        self
    }
}

// ---- counters ---------------------------------------------------------------------------

/// One track's pair of device counters. `last` is Unix ms, -1 for never.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    pub plays: i64,
    pub last: i64,
}

impl Counter {
    pub const ZERO: Counter = Counter { plays: 0, last: -1 };

    pub fn new(plays: i64, last: i64) -> Counter {
        Counter { plays: plays.max(0), last: if last > 0 { last } else { -1 } }
    }
}

/// A track as the log identifies it, with the counters read now.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub track_id: i64,
    pub key: String,
    pub counter: Counter,
}

/// Turns the library's counters into keyed readings. The first track with a given `global_id`
/// keeps it; a duplicate (a copied row) falls back to `#<track_id>` so no two share a key.
pub fn identify(counters: &[PlayCounter]) -> Vec<Reading> {
    let mut seen: HashSet<&str> = HashSet::new();
    counters
        .iter()
        .map(|c| {
            let gid = c.global_id.trim();
            let key = if !gid.is_empty() && seen.insert(gid) { gid.to_string() } else { format!("#{}", c.track_id) };
            Reading { track_id: c.track_id, key, counter: Counter::new(c.times_played, c.last_played) }
        })
        .collect()
}

// ---- the log ----------------------------------------------------------------------------

/// The stored log. `keys` interns the track keys; the baseline and the events refer to them by
/// index, which keeps a change to a handful of numbers (four on disk).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayLog {
    pub version: u32,
    /// Unix ms of the first snapshot; 0 while nothing has been recorded.
    pub started_at: i64,
    /// Unix ms of the newest snapshot that changed something; 0 if none.
    pub last_change_at: i64,
    /// Unix ms the baseline is as of (the first snapshot, or the newest folded change).
    pub baseline_at: i64,
    keys: Vec<String>,
    /// `[key, plays, last]` for every track with a non-zero counter at `baseline_at`.
    baseline: Vec<(u32, i64, i64)>,
    /// `[snapshot time, key, plays, last]`: the track's counters as read at that snapshot,
    /// oldest first. Only tracks that differed from the previous state are stored.
    events: Vec<(i64, u32, i64, i64)>,
    /// Keys of tracks the user told the log to carry on without (see [`PlayLog::adopt`]): still in
    /// the log, but not counted by the "different card" check until they are back in a library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    away: Vec<String>,
    /// The drive or path the library was last logged at, for telling logs apart in the UI. Not an
    /// identity: the file is found by [`store_path`], never by this.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub root: String,
}

/// What a snapshot did to the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The first snapshot: `tracks` played tracks became the baseline.
    Started { tracks: usize },
    Unchanged,
    /// `events` tracks differed from the log and were appended.
    Changed { events: usize },
    /// The user accepted this library as it is: `set_aside` tracks the library lacks were set
    /// aside, `baselined` tracks the log had never seen took their present counters as a
    /// starting point, and `events` tracks were appended as usual.
    Adopted { set_aside: usize, baselined: usize, events: usize },
    /// The library does not look like the one this log was made from; nothing was changed.
    Mismatch { logged: usize, present: usize },
}

/// A play (or several, see `plays`) derived from one change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    pub key: String,
    /// Unix ms the most recent of these plays happened (device clock), or the snapshot time when
    /// the device gave none (`time_known` false).
    pub at: i64,
    /// Plays seen in this change: the rise of the counter.
    pub plays: i64,
    /// More than one play was folded into the one time, or the play was inferred from the
    /// timestamp alone, or the time is not known.
    pub approx: bool,
    pub time_known: bool,
    /// The counter went backwards first (device reset or replaced track).
    pub reset: bool,
    /// When this app saw it.
    pub observed_at: i64,
}

impl Default for PlayLog {
    fn default() -> Self {
        PlayLog {
            version: VERSION,
            started_at: 0,
            last_change_at: 0,
            baseline_at: 0,
            keys: vec![],
            baseline: vec![],
            events: vec![],
            away: vec![],
            root: String::new(),
        }
    }
}

fn apply(state: &mut HashMap<u32, Counter>, key: u32, c: Counter) {
    if c == Counter::ZERO {
        state.remove(&key);
    } else {
        state.insert(key, c);
    }
}

impl PlayLog {
    pub fn is_started(&self) -> bool {
        self.started_at > 0
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    fn latest_at(&self) -> i64 {
        self.events.last().map(|e| e.0).unwrap_or(0).max(self.baseline_at)
    }

    fn key_index(&self) -> HashMap<&str, u32> {
        self.keys.iter().enumerate().map(|(i, k)| (k.as_str(), i as u32)).collect()
    }

    fn baseline_state(&self) -> HashMap<u32, Counter> {
        let mut m = HashMap::new();
        for &(k, p, l) in &self.baseline {
            apply(&mut m, k, Counter::new(p, l));
        }
        m
    }

    /// The counters of every played track as of the newest snapshot.
    fn state(&self) -> HashMap<u32, Counter> {
        let mut m = self.baseline_state();
        for &(_, k, p, l) in &self.events {
            apply(&mut m, k, Counter::new(p, l));
        }
        m
    }

    /// The counters the first snapshot (or the folded history) left for a track, if it had any.
    pub fn baseline_of(&self, key: &str) -> Option<Counter> {
        let idx = *self.key_index().get(key)?;
        self.baseline_state().get(&idx).copied()
    }

    /// The played tracks the log counts for its identity check (the ones not set aside), and how
    /// many of them are in `readings`.
    fn overlap(&self, readings: &[Reading]) -> (usize, usize) {
        let away: HashSet<&str> = self.away.iter().map(String::as_str).collect();
        let here: HashSet<&str> = readings.iter().map(|r| r.key.as_str()).collect();
        let active: Vec<&str> = self.state().keys().map(|&k| self.keys[k as usize].as_str()).filter(|k| !away.contains(k)).collect();
        (active.len(), active.iter().filter(|k| here.contains(*k)).count())
    }

    /// `Some((played tracks in the log, how many of them are in `readings`))` when fewer than
    /// `min_present_pct` percent are there, so this is not the same library; `None` when it looks
    /// right, when there is too little to judge, or when the check is off (0).
    pub fn identity_problem(&self, readings: &[Reading], min_present_pct: i64) -> Option<(usize, usize)> {
        if min_present_pct <= 0 {
            return None;
        }
        let (active, present) = self.overlap(readings);
        if active < IDENTITY_MIN_ACTIVE {
            return None;
        }
        (present * 100 < active * min_present_pct as usize).then_some((active, present))
    }

    /// Records a snapshot taken at `now` (Unix ms). The first one becomes the baseline; later
    /// ones append only the tracks whose counters differ from the log's picture.
    pub fn record(&mut self, now: i64, readings: &[Reading], settings: &Settings) -> Outcome {
        if !self.is_started() {
            return self.start(now, readings);
        }
        if let Some((logged, present)) = self.identity_problem(readings, settings.identity_min_present_pct) {
            return Outcome::Mismatch { logged, present };
        }
        self.append(now, readings, settings.max_events)
    }

    /// Records a snapshot of a library the identity check refused, because the user said to keep
    /// this log. Tracks of the log that are not in `readings` are set aside, tracks the log has
    /// never seen are baselined at their present counters (not counted as plays), and everything
    /// else is recorded as usual. Never returns `Mismatch`.
    pub fn adopt(&mut self, now: i64, readings: &[Reading], settings: &Settings) -> Outcome {
        if !self.is_started() {
            return self.start(now, readings);
        }
        let here: HashSet<&str> = readings.iter().map(|r| r.key.as_str()).collect();
        let mut gone: Vec<String> = Vec::new();
        for &k in self.state().keys() {
            let key = &self.keys[k as usize];
            if !here.contains(key.as_str()) && !self.away.contains(key) {
                gone.push(key.clone());
            }
        }
        gone.sort_unstable();
        let set_aside = gone.len();
        self.away.extend(gone);
        let mut known: HashSet<String> = self.keys.iter().cloned().collect();
        let mut baselined = 0;
        for r in readings.iter().filter(|r| r.counter != Counter::ZERO) {
            if !known.insert(r.key.clone()) {
                continue;
            }
            self.keys.push(r.key.clone());
            self.baseline.push(((self.keys.len() - 1) as u32, r.counter.plays, r.counter.last));
            baselined += 1;
        }
        let events = match self.append(now, readings, settings.max_events) {
            Outcome::Changed { events } => events,
            _ => 0,
        };
        Outcome::Adopted { set_aside, baselined, events }
    }

    fn start(&mut self, now: i64, readings: &[Reading]) -> Outcome {
        self.started_at = now.max(1);
        self.baseline_at = self.started_at;
        let mut keys: Vec<String> = Vec::new();
        for r in readings.iter().filter(|r| r.counter != Counter::ZERO) {
            keys.push(r.key.clone());
            self.baseline.push(((keys.len() - 1) as u32, r.counter.plays, r.counter.last));
        }
        self.keys = keys;
        Outcome::Started { tracks: self.baseline.len() }
    }

    /// Appends the tracks whose counters differ from the log's picture. A track that was set aside
    /// and is back in `readings` stops being set aside; it is then compared with the counters the
    /// log last saw, so its absence makes no plays.
    fn append(&mut self, now: i64, readings: &[Reading], max_events: i64) -> Outcome {
        // Events must stay in time order for the folds; a clock that stepped back is not trusted.
        let now = now.max(self.latest_at());
        let state = self.state();
        let mut index: HashMap<String, u32> = self.keys.iter().enumerate().map(|(i, k)| (k.clone(), i as u32)).collect();
        let away_before = self.away.len();
        if !self.away.is_empty() {
            let here: HashSet<&str> = readings.iter().map(|r| r.key.as_str()).collect();
            self.away.retain(|k| !here.contains(k.as_str()));
        }
        let returned = self.away.len() != away_before;
        let mut added = 0;
        for r in readings {
            let prev = index.get(&r.key).and_then(|i| state.get(i)).copied().unwrap_or(Counter::ZERO);
            if r.counter == prev {
                continue;
            }
            let idx = *index.entry(r.key.clone()).or_insert_with(|| {
                self.keys.push(r.key.clone());
                (self.keys.len() - 1) as u32
            });
            self.events.push((now, idx, r.counter.plays, r.counter.last));
            added += 1;
        }
        if added == 0 {
            // A track coming back changes the file (the set-aside list shrank), so it must be saved.
            return if returned { Outcome::Changed { events: 0 } } else { Outcome::Unchanged };
        }
        self.last_change_at = now;
        let cap = max_events.clamp(MIN_MAX_EVENTS, MAX_MAX_EVENTS) as usize;
        if self.events.len() > cap {
            self.fold_front(self.events.len() - cap);
        }
        Outcome::Changed { events: added }
    }

    /// Changes older than `retention_days` before `now` (0 keeps everything).
    pub fn prunable(&self, now: i64, retention_days: i64) -> usize {
        if retention_days <= 0 {
            return 0;
        }
        let cutoff = now - retention_days * MS_PER_DAY;
        self.events.iter().take_while(|e| e.0 < cutoff).count()
    }

    /// Folds changes older than the retention into the baseline. Returns how many were folded.
    pub fn prune(&mut self, now: i64, retention_days: i64) -> usize {
        let n = self.prunable(now, retention_days);
        if n > 0 {
            self.fold_front(n);
        }
        n
    }

    /// Moves the oldest `n` events (extended to the end of their snapshot) into the baseline.
    fn fold_front(&mut self, n: usize) {
        let mut n = n.min(self.events.len());
        if n == 0 {
            return;
        }
        let at = self.events[n - 1].0;
        while n < self.events.len() && self.events[n].0 == at {
            n += 1;
        }
        let mut state = self.baseline_state();
        for &(_, k, p, l) in &self.events[..n] {
            apply(&mut state, k, Counter::new(p, l));
        }
        let mut base: Vec<(u32, i64, i64)> = state.into_iter().map(|(k, c)| (k, c.plays, c.last)).collect();
        base.sort_unstable();
        self.baseline = base;
        self.baseline_at = at;
        self.events.drain(..n);
        self.compact_keys();
    }

    /// Drops keys nothing refers to and renumbers the rest.
    fn compact_keys(&mut self) {
        let mut used = vec![false; self.keys.len()];
        for &(k, _, _) in &self.baseline {
            used[k as usize] = true;
        }
        for &(_, k, _, _) in &self.events {
            used[k as usize] = true;
        }
        if used.iter().all(|&u| u) {
            return;
        }
        let mut remap = vec![u32::MAX; self.keys.len()];
        let mut keys = Vec::new();
        for (i, key) in self.keys.drain(..).enumerate() {
            if used[i] {
                remap[i] = keys.len() as u32;
                keys.push(key);
            }
        }
        self.keys = keys;
        for b in &mut self.baseline {
            b.0 = remap[b.0 as usize];
        }
        for e in &mut self.events {
            e.1 = remap[e.1 as usize];
        }
    }

    /// Every play the log can derive, oldest first (ties by key). See the module docs for what
    /// each field means and where it is inferred.
    pub fn hits(&self) -> Vec<Hit> {
        self.derive(None)
    }

    /// [`PlayLog::hits`] of one track only: the same hits, without walking or labelling the
    /// events of every other track (a track's history is asked for one track at a time).
    pub fn hits_of(&self, key: &str) -> Vec<Hit> {
        match self.keys.iter().position(|k| k == key) {
            Some(k) => self.derive(Some(k as u32)),
            None => Vec::new(),
        }
    }

    fn derive(&self, only: Option<u32>) -> Vec<Hit> {
        let mut state = self.baseline_state();
        let mut out = Vec::new();
        for &(observed_at, k, p, l) in &self.events {
            if only.is_some_and(|o| o != k) {
                continue;
            }
            let new = Counter::new(p, l);
            let prev = state.get(&k).copied().unwrap_or(Counter::ZERO);
            apply(&mut state, k, new);
            let reset = new.plays < prev.plays;
            let mut delta = if reset { new.plays } else { new.plays - prev.plays };
            let newer_stamp = new.last > prev.last && new.last > 0;
            let mut approx = delta > 1;
            if delta == 0 && newer_stamp && !reset {
                delta = 1; // the stamp moved without the count: still evidence of a play
                approx = true;
            }
            if delta <= 0 {
                continue;
            }
            let (at, time_known) = if newer_stamp && new.last <= observed_at {
                (new.last, true)
            } else {
                (observed_at, false)
            };
            out.push(Hit { key: self.keys[k as usize].clone(), at, plays: delta, approx: approx || !time_known, time_known, reset, observed_at });
        }
        out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.key.cmp(&b.key)));
        out
    }
}

// ---- views over hits ---------------------------------------------------------------------

/// Index ranges of `hits` (which must be sorted by time) forming sessions: a hit more than
/// `gap_ms` after the previous one starts a new session. Oldest first.
pub fn session_ranges(hits: &[Hit], gap_ms: i64) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        match out.last_mut() {
            Some(r) if h.at - hits[i - 1].at <= gap_ms => r.end = i + 1,
            _ => out.push(i..i + 1),
        }
    }
    out
}

/// One row of "most played in a period".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rank {
    pub key: String,
    pub plays: i64,
    /// Changes (observation windows) the plays came from.
    pub windows: usize,
    pub first: i64,
    pub last: i64,
}

/// Plays per track over `hits`: most plays first, then most recent, then key.
pub fn ranking(hits: &[Hit]) -> Vec<Rank> {
    let mut by: HashMap<&str, Rank> = HashMap::new();
    for h in hits {
        let r = by.entry(h.key.as_str()).or_insert_with(|| Rank { key: h.key.clone(), plays: 0, windows: 0, first: h.at, last: h.at });
        r.plays += h.plays;
        r.windows += 1;
        r.first = r.first.min(h.at);
        r.last = r.last.max(h.at);
    }
    let mut v: Vec<Rank> = by.into_values().collect();
    v.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.last.cmp(&a.last)).then_with(|| a.key.cmp(&b.key)));
    v
}

// ---- storage ----------------------------------------------------------------------------

pub fn store_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("playlog-{:016x}.json", crate::store::root_hash(root)))
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("playlog-settings.json")
}

/// Why a saved log could not be used. Only a damaged file is ever moved aside; a file that merely
/// could not be read right now (locked by a scanner, a permissions blip) or that a newer version of
/// the app wrote is left exactly where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The read failed; nothing is known about the content.
    Io(String),
    /// The content is not a log this app can use (not JSON, or indices that point nowhere).
    Corrupt(String),
    /// Written by a newer version of the app than this one.
    Newer(String),
}

impl LoadError {
    pub fn is_corrupt(&self) -> bool {
        matches!(self, LoadError::Corrupt(_))
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(m) | LoadError::Corrupt(m) | LoadError::Newer(m) => f.write_str(m),
        }
    }
}

/// Reads and checks one log file: `Ok(None)` when it does not exist.
pub fn load_file(p: &Path) -> Result<Option<PlayLog>, LoadError> {
    let bytes = match std::fs::read(p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(LoadError::Io(format!("{} could not be read: {e}", p.display()))),
    };
    let log: PlayLog = serde_json::from_slice(&bytes).map_err(|e| LoadError::Corrupt(format!("{} is not readable: {e}", p.display())))?;
    if log.version > VERSION {
        return Err(LoadError::Newer(format!("{} was written by a newer version of this app (log format {})", p.display(), log.version)));
    }
    // Indices must point at keys, or every later fold would panic.
    let n = log.keys.len() as u32;
    if log.baseline.iter().any(|b| b.0 >= n) || log.events.iter().any(|e| e.1 >= n) {
        return Err(LoadError::Corrupt(format!("{} refers to a track that is not in its key list", p.display())));
    }
    Ok(Some(log))
}

/// The saved log of a library: `Ok(None)` when there is none yet, `Err` when the file cannot be
/// used, so the caller decides what to do with it (see [`LoadError`]).
pub fn load(dir: &Path, root: &str) -> Result<Option<PlayLog>, LoadError> {
    load_file(&store_path(dir, root))
}

/// `playlog-` + 16 lowercase hex digits + `.json`: the only names this module writes for a log.
pub fn is_log_file_name(name: &str) -> bool {
    name.strip_prefix("playlog-")
        .and_then(|r| r.strip_suffix(".json"))
        .is_some_and(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

/// A log of another drive or path that matches most of the tracks of a library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    /// File name inside the config folder; what [`adopt_file`] takes.
    pub file: String,
    /// Where that log last saw its library ("" when not known).
    pub root: String,
    pub started_at: i64,
    pub last_change_at: i64,
    /// Played tracks the log knows, and how many of them are in this library.
    pub played_tracks: usize,
    pub present_tracks: usize,
}

/// Logs in `dir`, other than the one for `root`, in which at least `min_present_pct` percent of the
/// played tracks (and at least five) are in `readings`: the same card at another drive letter.
/// The most tracks in common first. A match is never made on less than the default percentage,
/// whatever the setting, because this decides what is offered, not what is refused. Unreadable
/// files are skipped.
pub fn candidates(dir: &Path, root: &str, readings: &[Reading], min_present_pct: i64) -> Vec<Candidate> {
    let own = store_path(dir, root);
    let need = min_present_pct.max(DEFAULT_IDENTITY_PCT) as usize;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<Candidate> = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_log_file_name(&name) || entry.path() == own {
            continue;
        }
        let Ok(Some(log)) = load_file(&entry.path()) else {
            continue;
        };
        if !log.is_started() {
            continue;
        }
        let (active, present) = log.overlap(readings);
        if active >= IDENTITY_MIN_ACTIVE && present * 100 >= active * need {
            out.push(Candidate {
                file: name,
                root: log.root.clone(),
                started_at: log.started_at,
                last_change_at: log.last_change_at,
                played_tracks: active,
                present_tracks: present,
            });
        }
    }
    out.sort_by(|a, b| b.present_tracks.cmp(&a.present_tracks).then_with(|| a.file.cmp(&b.file)));
    out
}

/// Makes another drive's log (`file`, a name from [`candidates`]) the log of `root`, by moving it:
/// nothing is copied or deleted, and `root` must not have a log yet. The log then remembers `root`.
pub fn adopt_file(dir: &Path, file: &str, root: &str) -> Result<(), String> {
    if !is_log_file_name(file) {
        return Err(format!("{file} is not a play log file name"));
    }
    let from = dir.join(file);
    let to = store_path(dir, root);
    if from == to {
        return Err("That is already this library's log".into());
    }
    if to.exists() {
        return Err("This library already has a play log; delete it first if you want to continue the other one".into());
    }
    if !from.is_file() {
        return Err(format!("{file} is no longer there"));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    // The remembered drive is only a label; failing to update it must not undo the move.
    if let Ok(Some(mut log)) = load_file(&to) {
        log.root = root.to_string();
        let _ = save(dir, root, &log);
    }
    Ok(())
}

pub fn save(dir: &Path, root: &str, log: &PlayLog) -> Result<(), String> {
    let bytes = serde_json::to_vec(log).map_err(|e| e.to_string())?;
    crate::store::write_bytes(&store_path(dir, root), &bytes)
}

/// Deletes the saved log; true if there was one.
pub fn remove(dir: &Path, root: &str) -> Result<bool, String> {
    match std::fs::remove_file(store_path(dir, root)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// Moves an unreadable log aside (kept for inspection, never deleted) so a fresh one can start.
/// Returns the new path.
pub fn quarantine(dir: &Path, root: &str, now: i64) -> Result<PathBuf, String> {
    let from = store_path(dir, root);
    let to = from.with_extension(format!("corrupt-{now}.json"));
    std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
    Ok(to)
}

/// Size of the saved log in bytes, 0 if none.
pub fn file_size(dir: &Path, root: &str) -> u64 {
    std::fs::metadata(store_path(dir, root)).map(|m| m.len()).unwrap_or(0)
}

/// The saved settings; a missing or unreadable file gives the defaults (the settings hold no
/// data worth refusing to start over, unlike the log).
pub fn load_settings(dir: &Path) -> Settings {
    std::fs::read(settings_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .map(Settings::sanitised)
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, s: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    crate::store::write_bytes(&settings_path(dir), &bytes)
}

// ---- tests ------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    const T0: i64 = 1_700_000_000_000;

    /// `record` with the default settings, which is what most tests want.
    trait Rec {
        fn rec(&mut self, now: i64, readings: &[Reading]) -> Outcome;
    }
    impl Rec for PlayLog {
        fn rec(&mut self, now: i64, readings: &[Reading]) -> Outcome {
            self.record(now, readings, &Settings::default())
        }
    }

    fn r(id: i64, key: &str, plays: i64, last: i64) -> Reading {
        Reading { track_id: id, key: key.into(), counter: Counter::new(plays, last) }
    }
    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-playlog-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn started(readings: &[Reading]) -> PlayLog {
        let mut log = PlayLog::default();
        log.rec(T0, readings);
        log
    }

    #[test]
    fn first_snapshot_is_the_baseline_and_derives_no_plays() {
        let mut log = PlayLog::default();
        let out = log.rec(T0, &[r(1, "a", 4, T0 - H), r(2, "b", 0, -1), r(3, "c", 0, T0 - 5 * H)]);
        assert_eq!(out, Outcome::Started { tracks: 2 }, "zero-counter tracks are not stored");
        assert!(log.hits().is_empty());
        assert_eq!(log.baseline_of("a"), Some(Counter { plays: 4, last: T0 - H }));
        assert_eq!(log.baseline_of("b"), None);
        assert_eq!((log.started_at, log.baseline_at, log.event_count()), (T0, T0, 0));
    }

    #[test]
    fn only_changes_are_stored() {
        let mut log = started(&[r(1, "a", 4, T0 - H), r(2, "b", 1, T0 - 2 * H), r(3, "c", 0, -1)]);
        assert_eq!(log.rec(T0 + H, &[r(1, "a", 4, T0 - H), r(2, "b", 1, T0 - 2 * H), r(3, "c", 0, -1)]), Outcome::Unchanged);
        assert_eq!(log.event_count(), 0);
        let same_but_b = [r(1, "a", 4, T0 - H), r(2, "b", 2, T0 + 2 * H), r(3, "c", 0, -1)];
        assert_eq!(log.rec(T0 + 3 * H, &same_but_b), Outcome::Changed { events: 1 });
        assert_eq!(log.event_count(), 1);
        assert_eq!(log.rec(T0 + 4 * H, &same_but_b), Outcome::Unchanged, "re-reading the same counters adds nothing");
        assert_eq!(log.last_change_at, T0 + 3 * H);
    }

    #[test]
    fn a_rise_in_the_count_is_a_play_at_the_devices_last_played_time() {
        let mut log = started(&[r(1, "a", 4, T0 - H)]);
        log.rec(T0 + 5 * H, &[r(1, "a", 5, T0 + 2 * H)]);
        let h = log.hits();
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].key.as_str(), h[0].at, h[0].plays, h[0].approx, h[0].time_known, h[0].reset), ("a", T0 + 2 * H, 1, false, true, false));
        assert_eq!(h[0].observed_at, T0 + 5 * H);
    }

    #[test]
    fn one_tracks_hits_are_the_same_as_its_share_of_all_hits() {
        let mut log = started(&[r(1, "a", 4, T0 - H), r(2, "b", 1, T0 - H)]);
        log.rec(T0 + 5 * H, &[r(1, "a", 5, T0 + 2 * H), r(2, "b", 3, T0 + 3 * H)]);
        log.rec(T0 + 9 * H, &[r(1, "a", 6, T0 + 8 * H), r(2, "b", 3, T0 + 3 * H)]);
        for key in ["a", "b"] {
            let want: Vec<Hit> = log.hits().into_iter().filter(|h| h.key == key).collect();
            assert!(!want.is_empty());
            assert_eq!(log.hits_of(key), want, "{key}");
        }
        assert!(log.hits_of("never seen").is_empty());
    }

    #[test]
    fn several_plays_in_one_window_are_one_approximate_hit() {
        let mut log = started(&[r(1, "a", 4, T0 - H)]);
        log.rec(T0 + 5 * H, &[r(1, "a", 7, T0 + 4 * H)]);
        let h = log.hits();
        assert_eq!((h[0].plays, h[0].approx, h[0].at), (3, true, T0 + 4 * H));
    }

    #[test]
    fn a_new_track_played_before_it_was_seen_counts_from_zero() {
        let mut log = started(&[r(1, "a", 4, T0 - H)]);
        log.rec(T0 + 5 * H, &[r(1, "a", 4, T0 - H), r(2, "new", 1, T0 + 3 * H)]);
        let h = log.hits();
        assert_eq!((h[0].key.as_str(), h[0].plays, h[0].approx), ("new", 1, false));
    }

    #[test]
    fn counter_going_backwards_is_a_reset_and_counts_from_zero() {
        let mut log = started(&[r(1, "a", 40, T0 - H), r(2, "b", 3, T0 - H)]);
        log.rec(T0 + 5 * H, &[r(1, "a", 2, T0 + 2 * H), r(2, "b", 0, -1)]);
        let h = log.hits();
        assert_eq!(h.len(), 1, "a reset to zero is not a play");
        assert_eq!((h[0].key.as_str(), h[0].plays, h[0].reset, h[0].time_known), ("a", 2, true, true));
        // and it keeps working from the new level afterwards
        log.rec(T0 + 9 * H, &[r(1, "a", 3, T0 + 8 * H), r(2, "b", 0, -1)]);
        assert_eq!(log.hits().last().unwrap().plays, 1);
    }

    #[test]
    fn stamp_without_count_is_one_approximate_play_and_count_without_stamp_has_no_time() {
        let mut log = started(&[r(1, "a", 4, T0 - 3 * H), r(2, "b", 4, T0 - 3 * H), r(3, "c", 2, T0 - 3 * H)]);
        let now = T0 + 5 * H;
        // a: stamp moved, count did not. b: count rose, stamp did not. c: stamp went back, count did not.
        log.rec(now, &[r(1, "a", 4, T0 + H), r(2, "b", 5, T0 - 3 * H), r(3, "c", 2, T0 - 9 * H)]);
        let h = log.hits();
        let by = |k: &str| h.iter().find(|x| x.key == k);
        let a = by("a").unwrap();
        assert_eq!((a.plays, a.approx, a.at, a.time_known), (1, true, T0 + H, true));
        let b = by("b").unwrap();
        assert_eq!((b.plays, b.approx, b.at, b.time_known), (1, true, now, false), "time falls back to the snapshot");
        assert!(by("c").is_none(), "a clock step back is not a play");
    }

    #[test]
    fn a_last_played_in_the_future_is_clamped_to_the_snapshot() {
        let mut log = started(&[r(1, "a", 0, -1)]);
        log.rec(T0 + H, &[r(1, "a", 1, T0 + 50 * H)]);
        let h = log.hits();
        assert_eq!((h[0].at, h[0].time_known, h[0].approx), (T0 + H, false, true));
    }

    #[test]
    fn clock_stepping_back_does_not_reorder_events() {
        let mut log = started(&[r(1, "a", 0, -1)]);
        log.rec(T0 + 10 * H, &[r(1, "a", 1, T0 + 9 * H)]);
        log.rec(T0 + 2 * H, &[r(1, "a", 2, T0 + 9 * H + 1)]); // computer clock went back
        assert!(log.events.windows(2).all(|w| w[0].0 <= w[1].0));
        assert_eq!(log.last_change_at, T0 + 10 * H);
    }

    #[test]
    fn hits_are_sorted_by_time_and_sessions_split_on_the_gap() {
        let mut log = started(&[r(1, "a", 0, -1), r(2, "b", 0, -1), r(3, "c", 0, -1)]);
        log.rec(T0 + 100 * H, &[r(1, "a", 1, T0 + 50 * H), r(2, "b", 1, T0 + 1 * H), r(3, "c", 1, T0 + 2 * H)]);
        let hits = log.hits();
        assert_eq!(hits.iter().map(|h| h.key.as_str()).collect::<Vec<_>>(), vec!["b", "c", "a"]);
        assert_eq!(session_ranges(&hits, 4 * H), vec![0..2, 2..3]);
        assert_eq!(session_ranges(&hits, 1), vec![0..1, 1..2, 2..3]);
        assert_eq!(session_ranges(&hits, 100 * H), vec![0..3]);
        assert!(session_ranges(&[], H).is_empty());
        // the boundary is inclusive, like the counter-only sessions
        assert_eq!(session_ranges(&hits, 48 * H).len(), 1);
        assert_eq!(session_ranges(&hits, 48 * H - 1).len(), 2);
    }

    #[test]
    fn ranking_sums_plays_and_orders_by_count_then_recency() {
        let mut log = started(&[r(1, "a", 0, -1), r(2, "b", 0, -1), r(3, "c", 0, -1)]);
        log.rec(T0 + 10 * H, &[r(1, "a", 2, T0 + 5 * H), r(2, "b", 3, T0 + 6 * H), r(3, "c", 3, T0 + 7 * H)]);
        log.rec(T0 + 20 * H, &[r(1, "a", 3, T0 + 15 * H), r(2, "b", 3, T0 + 6 * H), r(3, "c", 3, T0 + 7 * H)]);
        let rk = ranking(&log.hits());
        // all three have 3 plays; the tie goes to the newest last play: a (15h), c (7h), b (6h)
        let got: Vec<(&str, i64, usize)> = rk.iter().map(|x| (x.key.as_str(), x.plays, x.windows)).collect();
        assert_eq!(got, vec![("a", 3, 2), ("c", 3, 1), ("b", 3, 1)]);
        assert_eq!((rk[0].first, rk[0].last), (T0 + 5 * H, T0 + 15 * H));
    }

    #[test]
    fn retention_folds_old_changes_into_the_baseline_without_losing_totals() {
        let mut log = started(&[r(1, "a", 1, T0 - H), r(2, "b", 0, -1)]);
        log.rec(T0 + 1 * MS_PER_DAY, &[r(1, "a", 2, T0 + 1 * MS_PER_DAY - H), r(2, "b", 0, -1)]);
        log.rec(T0 + 40 * MS_PER_DAY, &[r(1, "a", 2, T0 + 1 * MS_PER_DAY - H), r(2, "b", 1, T0 + 40 * MS_PER_DAY - H)]);
        let before = log.state();
        assert_eq!(log.prunable(T0 + 40 * MS_PER_DAY, 30), 1);
        assert_eq!(log.prunable(T0 + 40 * MS_PER_DAY, 0), 0, "0 keeps everything");
        assert_eq!(log.prune(T0 + 40 * MS_PER_DAY, 30), 1);
        assert_eq!(log.event_count(), 1);
        assert_eq!(log.baseline_at, T0 + 1 * MS_PER_DAY);
        assert_eq!(log.baseline_of("a").unwrap().plays, 2, "the pruned play is part of the baseline now");
        assert_eq!(log.state().len(), before.len());
        let hits = log.hits();
        assert_eq!((hits.len(), hits[0].key.as_str()), (1, "b"));
        assert_eq!(log.prune(T0 + 40 * MS_PER_DAY, 30), 0, "idempotent");
    }

    #[test]
    fn folding_never_splits_one_snapshot_and_compacts_the_key_list() {
        let mut log = started(&[]);
        log.rec(T0 + H, &[r(1, "a", 1, T0 + H - 1), r(2, "b", 1, T0 + H - 1)]);
        log.rec(T0 + 2 * H, &[r(1, "a", 1, T0 + H - 1), r(2, "b", 1, T0 + H - 1), r(3, "c", 1, T0 + 2 * H - 1)]);
        assert_eq!(log.keys.len(), 3);
        log.fold_front(1); // asks for one of the two events of the first snapshot
        assert_eq!(log.event_count(), 1, "both events of that snapshot were folded");
        assert_eq!(log.baseline_at, T0 + H);
        assert_eq!(log.keys.len(), 3, "a and b are baseline, c is an event");
        // a key nothing refers to any more is dropped
        let mut log2 = started(&[r(1, "gone", 1, T0 - H)]);
        log2.rec(T0 + H, &[r(1, "gone", 0, -1)]);
        log2.fold_front(1);
        assert!(log2.keys.is_empty() && log2.baseline.is_empty());
    }

    #[test]
    fn the_event_cap_is_a_setting_and_folds_the_oldest_into_the_baseline() {
        let mut log = started(&[r(1, "a", 0, -1)]);
        let small = Settings { max_events: MIN_MAX_EVENTS, ..Settings::default() };
        for i in 1..=(MIN_MAX_EVENTS + 5) {
            log.record(T0 + i * 1000, &[r(1, "a", i, T0 + i * 1000 - 1)], &small);
        }
        assert_eq!(log.event_count(), MIN_MAX_EVENTS as usize, "a cap of 1000 keeps 1000 changes, whatever the retention");
        assert_eq!(log.baseline_of("a").unwrap().plays, 5, "the folded ones are part of the baseline");
        assert_eq!(log.hits().iter().map(|h| h.plays).sum::<i64>(), MIN_MAX_EVENTS);
        assert_eq!(Settings { max_events: 3, ..Settings::default() }.sanitised().max_events, MIN_MAX_EVENTS, "a too small cap is raised");
    }

    #[test]
    fn max_events_bounds_growth() {
        let mut log = started(&[r(1, "a", 0, -1)]);
        for i in 1..=5i64 {
            log.rec(T0 + i * H, &[r(1, "a", i, T0 + i * H - 1)]);
        }
        log.fold_front(log.event_count() - 2);
        assert_eq!(log.event_count(), 2);
        assert_eq!(log.baseline_of("a").unwrap().plays, 3);
        assert_eq!(log.hits().iter().map(|h| h.plays).sum::<i64>(), 2);
    }

    #[test]
    fn a_different_library_is_refused_not_logged() {
        let mut log = started(&(1..=10).map(|i| r(i, &format!("k{i}"), 2, T0 - H)).collect::<Vec<_>>());
        let other: Vec<Reading> = (1..=50).map(|i| r(i, &format!("x{i}"), 9, T0)).collect();
        let before = log.clone();
        assert_eq!(log.rec(T0 + H, &other), Outcome::Mismatch { logged: 10, present: 0 });
        assert_eq!(log, before, "a mismatch changes nothing");
        // losing a few played tracks is fine, losing most is not
        let mut mostly: Vec<Reading> = (1..=6).map(|i| r(i, &format!("k{i}"), 2, T0 - H)).collect();
        assert!(matches!(log.rec(T0 + H, &mostly), Outcome::Unchanged));
        mostly.truncate(4);
        assert!(matches!(log.rec(T0 + 2 * H, &mostly), Outcome::Mismatch { logged: 10, present: 4 }));
    }

    #[test]
    fn the_identity_threshold_is_a_setting_and_zero_switches_the_check_off() {
        let base: Vec<Reading> = (1..=10).map(|i| r(i, &format!("k{i}"), 2, T0 - H)).collect();
        let five: Vec<Reading> = base[..5].to_vec();
        let mut log = started(&base);
        // 5 of 10 is exactly 50 percent: allowed; 6 of 10 needed at 60.
        let at = |pct| Settings { identity_min_present_pct: pct, ..Settings::default() };
        assert_eq!(log.identity_problem(&five, 50), None);
        assert_eq!(log.identity_problem(&five, 60), Some((10, 5)));
        assert_eq!(log.record(T0 + H, &five, &at(60)), Outcome::Mismatch { logged: 10, present: 5 });
        assert_eq!(log.record(T0 + H, &[], &at(0)), Outcome::Unchanged, "0 never refuses, not even an empty library");
        assert_eq!(Settings { identity_min_present_pct: 500, ..Settings::default() }.sanitised().identity_min_present_pct, 100);
    }

    #[test]
    fn keeping_the_log_after_a_clean_out_resumes_logging_without_false_plays() {
        // 10 tracks ever played; the user clears 6 off the card and puts a new gig set on it.
        let played: Vec<Reading> = (1..=10).map(|i| r(i, &format!("k{i}"), 2, T0 - H)).collect();
        let mut log = started(&played);
        let mut now_lib: Vec<Reading> = played[..4].to_vec();
        now_lib.extend((1..=20).map(|i| r(100 + i, &format!("new{i}"), 7, T0 - 5 * H)));
        now_lib.push(r(200, "never", 0, -1));
        // The guard refuses it, and the refusal is permanent without the user's decision.
        assert!(matches!(log.rec(T0 + H, &now_lib), Outcome::Mismatch { logged: 10, present: 4 }));
        assert!(matches!(log.rec(T0 + 2 * H, &now_lib), Outcome::Mismatch { .. }));
        // One of the kept tracks was played on the device meanwhile; the new ones are only taken as they are.
        now_lib[0] = r(1, "k1", 3, T0 + 3 * H);
        let out = log.adopt(T0 + 4 * H, &now_lib, &Settings::default());
        assert_eq!(out, Outcome::Adopted { set_aside: 6, baselined: 20, events: 1 });
        let hits = log.hits();
        assert_eq!(hits.len(), 1, "only the real play: {hits:?}");
        assert_eq!((hits[0].key.as_str(), hits[0].plays), ("k1", 1));
        assert_eq!(log.baseline_of("new3"), Some(Counter { plays: 7, last: T0 - 5 * H }), "new tracks start from their counters");
        // And from now on the same library is accepted without asking again.
        assert!(matches!(log.rec(T0 + 5 * H, &now_lib), Outcome::Unchanged));
        now_lib[5] = r(106, "new6", 8, T0 + 5 * H - 1);
        assert!(matches!(log.rec(T0 + 6 * H, &now_lib), Outcome::Changed { events: 1 }));
        assert_eq!(log.hits().last().unwrap().key, "new6");
    }

    #[test]
    fn a_set_aside_track_that_comes_back_makes_no_plays_and_counts_again() {
        let played: Vec<Reading> = (1..=10).map(|i| r(i, &format!("k{i}"), 2, T0 - H)).collect();
        let mut log = started(&played);
        let few: Vec<Reading> = played[..2].to_vec();
        log.adopt(T0 + H, &few, &Settings::default());
        assert_eq!(log.away.len(), 8);
        assert_eq!(log.identity_problem(&few, 50), None, "set aside tracks are not held against the library");
        // all ten come back with the counters they had: nothing happened
        assert!(matches!(log.rec(T0 + 2 * H, &played), Outcome::Changed { events: 0 }), "the file changes (the list shrank) but no event");
        assert!(log.away.is_empty() && log.hits().is_empty());
        assert!(matches!(log.rec(T0 + 3 * H, &played), Outcome::Unchanged));
        // and they count for the identity check again
        assert_eq!(log.identity_problem(&few, 50), Some((10, 2)));
    }

    #[test]
    fn adopting_a_log_that_has_not_started_just_starts_it() {
        let mut log = PlayLog::default();
        assert!(matches!(log.adopt(T0, &[r(1, "a", 1, T0 - H)], &Settings::default()), Outcome::Started { tracks: 1 }));
    }

    #[test]
    fn the_away_list_and_remembered_root_survive_the_file_and_old_files_still_load() {
        let dir = temp_dir("away");
        let played: Vec<Reading> = (1..=6).map(|i| r(i, &format!("k{i}"), 1, T0 - H)).collect();
        let mut log = started(&played);
        log.adopt(T0 + H, &played[..1], &Settings::default());
        log.root = "F:\\".into();
        save(&dir, "F:", &log).unwrap();
        assert_eq!(load(&dir, "F:").unwrap().unwrap(), log);
        // a file written before these fields existed
        std::fs::write(store_path(&dir, "G:"), r#"{"version":1,"started_at":1,"last_change_at":0,"baseline_at":1,"keys":["a"],"baseline":[[0,1,1]],"events":[]}"#).unwrap();
        let old = load(&dir, "G:").unwrap().unwrap();
        assert!(old.away.is_empty() && old.root.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_card_at_a_new_drive_letter_is_offered_the_old_log_and_adopting_moves_it() {
        let dir = temp_dir("candidates");
        let played: Vec<Reading> = (1..=8).map(|i| r(i, &format!("k{i}"), 3, T0 - H)).collect();
        let mut old = started(&played);
        old.root = "E:\\".into();
        save(&dir, "E:", &old).unwrap();
        let other = started(&(1..=8).map(|i| r(i, &format!("z{i}"), 3, T0 - H)).collect::<Vec<_>>());
        save(&dir, "H:", &other).unwrap();
        std::fs::write(dir.join("playlog-0123456789abcdef.json"), "garbage").unwrap();
        std::fs::write(dir.join("playlog-settings.json"), "{}").unwrap();

        // the same card, now at F:
        let found = candidates(&dir, "F:", &played, 50);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].root.as_str(), found[0].played_tracks, found[0].present_tracks), ("E:\\", 8, 8));
        assert!(is_log_file_name(&found[0].file));
        assert!(candidates(&dir, "E:", &played, 50).is_empty(), "a library is not offered its own log");
        // a card with only 3 of the 8 does not match, even with the guard switched off
        assert!(candidates(&dir, "F:", &played[..3], 0).is_empty());

        adopt_file(&dir, &found[0].file, "F:").unwrap();
        assert!(!store_path(&dir, "E:").exists() && store_path(&dir, "F:").exists(), "moved, not copied");
        assert_eq!(load(&dir, "F:").unwrap().unwrap().root, "F:");
        assert!(adopt_file(&dir, &found[0].file, "F:").is_err(), "gone now");
        // never over an existing log, never a name that is not a log file, never a path
        let again = store_path(&dir, "H:").file_name().unwrap().to_string_lossy().into_owned();
        assert!(adopt_file(&dir, &again, "F:").unwrap_err().contains("already has a play log"));
        for bad in ["playlog-settings.json", "../playlog-0123456789abcdef.json", "playlog-0123456789ABCDEF.json", "playlog-0123456789abcde.json", ""] {
            assert!(adopt_file(&dir, bad, "K:").is_err(), "{bad}");
        }
        assert!(store_path(&dir, "H:").exists(), "refusals touch nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_log_is_not_judged() {
        let mut log = started(&[r(1, "a", 1, T0 - H), r(2, "b", 1, T0 - H)]);
        assert!(matches!(log.rec(T0 + H, &[r(7, "z", 1, T0)]), Outcome::Changed { .. }));
    }

    #[test]
    fn identify_prefers_global_id_and_falls_back_for_blank_and_duplicate() {
        let c = |track_id, g: &str| PlayCounter { track_id, global_id: g.into(), times_played: 1, last_played: T0 };
        let ids = identify(&[c(1, "abc"), c(2, ""), c(3, "abc"), c(4, " def ")]);
        assert_eq!(ids.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), vec!["abc", "#2", "#3", "def"]);
    }

    #[test]
    fn the_file_round_trips_compactly_and_a_new_snapshot_continues_it() {
        let dir = temp_dir("roundtrip");
        let mut log = started(&[r(1, "a1b2c3", 1, T0 - H)]);
        log.rec(T0 + H, &[r(1, "a1b2c3", 2, T0 + 30_000)]);
        save(&dir, "E:\\", &log).unwrap();
        let text = std::fs::read_to_string(store_path(&dir, "e:/")).unwrap();
        assert!(text.contains("\"events\":[["), "events are plain arrays: {text}");
        assert!(!text.contains('\n'), "compact, one line");
        assert_eq!(load(&dir, "E:").unwrap().unwrap(), log, "E:, E:\\ and e:/ are one library");
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        assert!(remove(&dir, "E:").unwrap());
        assert!(!remove(&dir, "E:").unwrap());
        assert_eq!(load(&dir, "E:").unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_files_are_reported_then_moved_aside_never_deleted() {
        let dir = temp_dir("corrupt");
        std::fs::write(store_path(&dir, "P:"), "{ not json").unwrap();
        let err = load(&dir, "P:").unwrap_err();
        assert!(err.is_corrupt() && err.to_string().contains("not readable"));
        let moved = quarantine(&dir, "P:", 123).unwrap();
        assert!(moved.exists() && !store_path(&dir, "P:").exists());
        assert_eq!(load(&dir, "P:").unwrap(), None);
        // an index pointing past the key list would panic later: reject it on load
        std::fs::write(store_path(&dir, "P:"), r#"{"version":1,"started_at":1,"last_change_at":0,"baseline_at":1,"keys":[],"baseline":[[3,1,1]],"events":[]}"#).unwrap();
        assert!(load(&dir, "P:").unwrap_err().is_corrupt());
        std::fs::write(store_path(&dir, "P:"), r#"{"version":99,"started_at":1,"last_change_at":0,"baseline_at":1,"keys":[],"baseline":[],"events":[]}"#).unwrap();
        let newer = load(&dir, "P:").unwrap_err();
        assert!(matches!(newer, LoadError::Newer(_)) && !newer.is_corrupt(), "a newer app's file is not damage");
        assert!(newer.to_string().contains("newer version"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_error_that_is_not_damage_is_not_corruption() {
        let dir = temp_dir("io");
        // A folder where the file should be: reading it fails with something other than NotFound.
        std::fs::create_dir_all(store_path(&dir, "Q:")).unwrap();
        let err = load(&dir, "Q:").unwrap_err();
        assert!(matches!(err, LoadError::Io(_)) && !err.is_corrupt(), "{err:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_default_on_clamp_and_survive_garbage() {
        let dir = temp_dir("settings");
        assert_eq!(
            load_settings(&dir),
            Settings { enabled: true, retention_days: 730, session_gap_minutes: 240, max_events: 400_000, identity_min_present_pct: 50, ask_before_new_log: true }
        );
        save_settings(&dir, &Settings { enabled: false, retention_days: -5, session_gap_minutes: 99_999, ..Settings::default() }.sanitised()).unwrap();
        let got = load_settings(&dir);
        assert_eq!((got.enabled, got.retention_days, got.session_gap_minutes), (false, 0, 1440));
        std::fs::write(settings_path(&dir), "garbage").unwrap();
        assert_eq!(load_settings(&dir), Settings::default());
        std::fs::write(settings_path(&dir), r#"{"enabled":false}"#).unwrap();
        assert_eq!(load_settings(&dir).retention_days, 730, "missing fields take their defaults");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
