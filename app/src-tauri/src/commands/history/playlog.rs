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
/// A guard against an unbounded file, not a policy: should the retained log ever hold more
/// changes than this, the oldest are folded into the baseline like retention does.
pub const MAX_EVENTS: usize = 400_000;
/// Below this many played tracks in the log the "different card" check is not made.
const IDENTITY_MIN_ACTIVE: usize = 5;
/// Fewer than this percentage of the played tracks still present means a different library.
const IDENTITY_MIN_PRESENT_PCT: usize = 50;
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: true, retention_days: DEFAULT_RETENTION_DAYS, session_gap_minutes: DEFAULT_GAP_MINUTES }
    }
}

impl Settings {
    pub fn sanitised(mut self) -> Settings {
        self.retention_days = self.retention_days.clamp(0, MAX_RETENTION_DAYS);
        self.session_gap_minutes = self.session_gap_minutes.clamp(1, 24 * 60);
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
}

/// What a snapshot did to the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The first snapshot: `tracks` played tracks became the baseline.
    Started { tracks: usize },
    Unchanged,
    /// `events` tracks differed from the log and were appended.
    Changed { events: usize },
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
        PlayLog { version: VERSION, started_at: 0, last_change_at: 0, baseline_at: 0, keys: vec![], baseline: vec![], events: vec![] }
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

    /// `Some((played tracks in the log, how many of them are in `readings`))` when too few are
    /// there for this to be the same library; `None` when it looks right or there is too little to judge.
    pub fn identity_problem(&self, readings: &[Reading]) -> Option<(usize, usize)> {
        let state = self.state();
        if state.len() < IDENTITY_MIN_ACTIVE {
            return None;
        }
        let here: HashSet<&str> = readings.iter().map(|r| r.key.as_str()).collect();
        let present = state.keys().filter(|&&k| here.contains(self.keys[k as usize].as_str())).count();
        (present * 100 < state.len() * IDENTITY_MIN_PRESENT_PCT).then_some((state.len(), present))
    }

    /// Records a snapshot taken at `now` (Unix ms). The first one becomes the baseline; later
    /// ones append only the tracks whose counters differ from the log's picture.
    pub fn record(&mut self, now: i64, readings: &[Reading]) -> Outcome {
        if !self.is_started() {
            self.started_at = now.max(1);
            self.baseline_at = self.started_at;
            let mut keys: Vec<String> = Vec::new();
            for r in readings.iter().filter(|r| r.counter != Counter::ZERO) {
                keys.push(r.key.clone());
                self.baseline.push(((keys.len() - 1) as u32, r.counter.plays, r.counter.last));
            }
            self.keys = keys;
            return Outcome::Started { tracks: self.baseline.len() };
        }
        if let Some((logged, present)) = self.identity_problem(readings) {
            return Outcome::Mismatch { logged, present };
        }
        // Events must stay in time order for the folds; a clock that stepped back is not trusted.
        let now = now.max(self.latest_at());
        let state = self.state();
        let mut index: HashMap<String, u32> = self.keys.iter().enumerate().map(|(i, k)| (k.clone(), i as u32)).collect();
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
            return Outcome::Unchanged;
        }
        self.last_change_at = now;
        if self.events.len() > MAX_EVENTS {
            self.fold_front(self.events.len() - MAX_EVENTS);
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
        let mut state = self.baseline_state();
        let mut out = Vec::new();
        for &(observed_at, k, p, l) in &self.events {
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

/// FNV-1a of the volume root with separators unified, trailing ones dropped and case folded, so
/// `E:` and `e:/` are one library (the same spelling rule as the smart-case store).
fn root_hash(root: &str) -> u64 {
    let norm = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in norm.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn store_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("playlog-{:016x}.json", root_hash(root)))
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join("playlog-settings.json")
}

/// Writes `bytes` to `path` through a temp file in the same folder, so a crash leaves the old
/// file or the new one, never half of either.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let res = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    Ok(())
}

/// The saved log of a library: `Ok(None)` when there is none yet, `Err` (worded for the user)
/// when the file cannot be read, so the caller decides what to do with it.
pub fn load(dir: &Path, root: &str) -> Result<Option<PlayLog>, String> {
    let p = store_path(dir, root);
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    let log: PlayLog = serde_json::from_slice(&bytes).map_err(|e| format!("{} is not readable: {e}", p.display()))?;
    if log.version > VERSION {
        return Err(format!("{} was written by a newer version of this app (log format {})", p.display(), log.version));
    }
    // Indices must point at keys, or every later fold would panic.
    let n = log.keys.len() as u32;
    if log.baseline.iter().any(|b| b.0 >= n) || log.events.iter().any(|e| e.1 >= n) {
        return Err(format!("{} refers to a track that is not in its key list", p.display()));
    }
    Ok(Some(log))
}

pub fn save(dir: &Path, root: &str, log: &PlayLog) -> Result<(), String> {
    let bytes = serde_json::to_vec(log).map_err(|e| e.to_string())?;
    atomic_write(&store_path(dir, root), &bytes)
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
    atomic_write(&settings_path(dir), &bytes)
}

// ---- tests ------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;
    const T0: i64 = 1_700_000_000_000;

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
        log.record(T0, readings);
        log
    }

    #[test]
    fn first_snapshot_is_the_baseline_and_derives_no_plays() {
        let mut log = PlayLog::default();
        let out = log.record(T0, &[r(1, "a", 4, T0 - H), r(2, "b", 0, -1), r(3, "c", 0, T0 - 5 * H)]);
        assert_eq!(out, Outcome::Started { tracks: 2 }, "zero-counter tracks are not stored");
        assert!(log.hits().is_empty());
        assert_eq!(log.baseline_of("a"), Some(Counter { plays: 4, last: T0 - H }));
        assert_eq!(log.baseline_of("b"), None);
        assert_eq!((log.started_at, log.baseline_at, log.event_count()), (T0, T0, 0));
    }

    #[test]
    fn only_changes_are_stored() {
        let mut log = started(&[r(1, "a", 4, T0 - H), r(2, "b", 1, T0 - 2 * H), r(3, "c", 0, -1)]);
        assert_eq!(log.record(T0 + H, &[r(1, "a", 4, T0 - H), r(2, "b", 1, T0 - 2 * H), r(3, "c", 0, -1)]), Outcome::Unchanged);
        assert_eq!(log.event_count(), 0);
        let same_but_b = [r(1, "a", 4, T0 - H), r(2, "b", 2, T0 + 2 * H), r(3, "c", 0, -1)];
        assert_eq!(log.record(T0 + 3 * H, &same_but_b), Outcome::Changed { events: 1 });
        assert_eq!(log.event_count(), 1);
        assert_eq!(log.record(T0 + 4 * H, &same_but_b), Outcome::Unchanged, "re-reading the same counters adds nothing");
        assert_eq!(log.last_change_at, T0 + 3 * H);
    }

    #[test]
    fn a_rise_in_the_count_is_a_play_at_the_devices_last_played_time() {
        let mut log = started(&[r(1, "a", 4, T0 - H)]);
        log.record(T0 + 5 * H, &[r(1, "a", 5, T0 + 2 * H)]);
        let h = log.hits();
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].key.as_str(), h[0].at, h[0].plays, h[0].approx, h[0].time_known, h[0].reset), ("a", T0 + 2 * H, 1, false, true, false));
        assert_eq!(h[0].observed_at, T0 + 5 * H);
    }

    #[test]
    fn several_plays_in_one_window_are_one_approximate_hit() {
        let mut log = started(&[r(1, "a", 4, T0 - H)]);
        log.record(T0 + 5 * H, &[r(1, "a", 7, T0 + 4 * H)]);
        let h = log.hits();
        assert_eq!((h[0].plays, h[0].approx, h[0].at), (3, true, T0 + 4 * H));
    }

    #[test]
    fn a_new_track_played_before_it_was_seen_counts_from_zero() {
        let mut log = started(&[r(1, "a", 4, T0 - H)]);
        log.record(T0 + 5 * H, &[r(1, "a", 4, T0 - H), r(2, "new", 1, T0 + 3 * H)]);
        let h = log.hits();
        assert_eq!((h[0].key.as_str(), h[0].plays, h[0].approx), ("new", 1, false));
    }

    #[test]
    fn counter_going_backwards_is_a_reset_and_counts_from_zero() {
        let mut log = started(&[r(1, "a", 40, T0 - H), r(2, "b", 3, T0 - H)]);
        log.record(T0 + 5 * H, &[r(1, "a", 2, T0 + 2 * H), r(2, "b", 0, -1)]);
        let h = log.hits();
        assert_eq!(h.len(), 1, "a reset to zero is not a play");
        assert_eq!((h[0].key.as_str(), h[0].plays, h[0].reset, h[0].time_known), ("a", 2, true, true));
        // and it keeps working from the new level afterwards
        log.record(T0 + 9 * H, &[r(1, "a", 3, T0 + 8 * H), r(2, "b", 0, -1)]);
        assert_eq!(log.hits().last().unwrap().plays, 1);
    }

    #[test]
    fn stamp_without_count_is_one_approximate_play_and_count_without_stamp_has_no_time() {
        let mut log = started(&[r(1, "a", 4, T0 - 3 * H), r(2, "b", 4, T0 - 3 * H), r(3, "c", 2, T0 - 3 * H)]);
        let now = T0 + 5 * H;
        // a: stamp moved, count did not. b: count rose, stamp did not. c: stamp went back, count did not.
        log.record(now, &[r(1, "a", 4, T0 + H), r(2, "b", 5, T0 - 3 * H), r(3, "c", 2, T0 - 9 * H)]);
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
        log.record(T0 + H, &[r(1, "a", 1, T0 + 50 * H)]);
        let h = log.hits();
        assert_eq!((h[0].at, h[0].time_known, h[0].approx), (T0 + H, false, true));
    }

    #[test]
    fn clock_stepping_back_does_not_reorder_events() {
        let mut log = started(&[r(1, "a", 0, -1)]);
        log.record(T0 + 10 * H, &[r(1, "a", 1, T0 + 9 * H)]);
        log.record(T0 + 2 * H, &[r(1, "a", 2, T0 + 9 * H + 1)]); // computer clock went back
        assert!(log.events.windows(2).all(|w| w[0].0 <= w[1].0));
        assert_eq!(log.last_change_at, T0 + 10 * H);
    }

    #[test]
    fn hits_are_sorted_by_time_and_sessions_split_on_the_gap() {
        let mut log = started(&[r(1, "a", 0, -1), r(2, "b", 0, -1), r(3, "c", 0, -1)]);
        log.record(T0 + 100 * H, &[r(1, "a", 1, T0 + 50 * H), r(2, "b", 1, T0 + 1 * H), r(3, "c", 1, T0 + 2 * H)]);
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
        log.record(T0 + 10 * H, &[r(1, "a", 2, T0 + 5 * H), r(2, "b", 3, T0 + 6 * H), r(3, "c", 3, T0 + 7 * H)]);
        log.record(T0 + 20 * H, &[r(1, "a", 3, T0 + 15 * H), r(2, "b", 3, T0 + 6 * H), r(3, "c", 3, T0 + 7 * H)]);
        let rk = ranking(&log.hits());
        // all three have 3 plays; the tie goes to the newest last play: a (15h), c (7h), b (6h)
        let got: Vec<(&str, i64, usize)> = rk.iter().map(|x| (x.key.as_str(), x.plays, x.windows)).collect();
        assert_eq!(got, vec![("a", 3, 2), ("c", 3, 1), ("b", 3, 1)]);
        assert_eq!((rk[0].first, rk[0].last), (T0 + 5 * H, T0 + 15 * H));
    }

    #[test]
    fn retention_folds_old_changes_into_the_baseline_without_losing_totals() {
        let mut log = started(&[r(1, "a", 1, T0 - H), r(2, "b", 0, -1)]);
        log.record(T0 + 1 * MS_PER_DAY, &[r(1, "a", 2, T0 + 1 * MS_PER_DAY - H), r(2, "b", 0, -1)]);
        log.record(T0 + 40 * MS_PER_DAY, &[r(1, "a", 2, T0 + 1 * MS_PER_DAY - H), r(2, "b", 1, T0 + 40 * MS_PER_DAY - H)]);
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
        log.record(T0 + H, &[r(1, "a", 1, T0 + H - 1), r(2, "b", 1, T0 + H - 1)]);
        log.record(T0 + 2 * H, &[r(1, "a", 1, T0 + H - 1), r(2, "b", 1, T0 + H - 1), r(3, "c", 1, T0 + 2 * H - 1)]);
        assert_eq!(log.keys.len(), 3);
        log.fold_front(1); // asks for one of the two events of the first snapshot
        assert_eq!(log.event_count(), 1, "both events of that snapshot were folded");
        assert_eq!(log.baseline_at, T0 + H);
        assert_eq!(log.keys.len(), 3, "a and b are baseline, c is an event");
        // a key nothing refers to any more is dropped
        let mut log2 = started(&[r(1, "gone", 1, T0 - H)]);
        log2.record(T0 + H, &[r(1, "gone", 0, -1)]);
        log2.fold_front(1);
        assert!(log2.keys.is_empty() && log2.baseline.is_empty());
    }

    #[test]
    fn max_events_bounds_growth() {
        let mut log = started(&[r(1, "a", 0, -1)]);
        for i in 1..=5i64 {
            log.record(T0 + i * H, &[r(1, "a", i, T0 + i * H - 1)]);
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
        assert_eq!(log.record(T0 + H, &other), Outcome::Mismatch { logged: 10, present: 0 });
        assert_eq!(log, before, "a mismatch changes nothing");
        // losing a few played tracks is fine, losing most is not
        let mut mostly: Vec<Reading> = (1..=6).map(|i| r(i, &format!("k{i}"), 2, T0 - H)).collect();
        assert!(matches!(log.record(T0 + H, &mostly), Outcome::Unchanged));
        mostly.truncate(4);
        assert!(matches!(log.record(T0 + 2 * H, &mostly), Outcome::Mismatch { logged: 10, present: 4 }));
    }

    #[test]
    fn a_small_log_is_not_judged() {
        let mut log = started(&[r(1, "a", 1, T0 - H), r(2, "b", 1, T0 - H)]);
        assert!(matches!(log.record(T0 + H, &[r(7, "z", 1, T0)]), Outcome::Changed { .. }));
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
        log.record(T0 + H, &[r(1, "a1b2c3", 2, T0 + 30_000)]);
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
        assert!(load(&dir, "P:").unwrap_err().contains("not readable"));
        let moved = quarantine(&dir, "P:", 123).unwrap();
        assert!(moved.exists() && !store_path(&dir, "P:").exists());
        assert_eq!(load(&dir, "P:").unwrap(), None);
        // an index pointing past the key list would panic later: reject it on load
        std::fs::write(store_path(&dir, "P:"), r#"{"version":1,"started_at":1,"last_change_at":0,"baseline_at":1,"keys":[],"baseline":[[3,1,1]],"events":[]}"#).unwrap();
        assert!(load(&dir, "P:").is_err());
        std::fs::write(store_path(&dir, "P:"), r#"{"version":99,"started_at":1,"last_change_at":0,"baseline_at":1,"keys":[],"baseline":[],"events":[]}"#).unwrap();
        assert!(load(&dir, "P:").unwrap_err().contains("newer version"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_default_on_clamp_and_survive_garbage() {
        let dir = temp_dir("settings");
        assert_eq!(load_settings(&dir), Settings { enabled: true, retention_days: 730, session_gap_minutes: 240 });
        save_settings(&dir, &Settings { enabled: false, retention_days: -5, session_gap_minutes: 99_999 }.sanitised()).unwrap();
        assert_eq!(load_settings(&dir), Settings { enabled: false, retention_days: 0, session_gap_minutes: 1440 });
        std::fs::write(settings_path(&dir), "garbage").unwrap();
        assert_eq!(load_settings(&dir), Settings::default());
        std::fs::write(settings_path(&dir), r#"{"enabled":false}"#).unwrap();
        assert_eq!(load_settings(&dir).retention_days, 730, "missing fields take their defaults");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
