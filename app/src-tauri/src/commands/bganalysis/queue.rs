//! The logic of the background analysis queue, apart from Tauri, threads and files so the rules
//! can be tested with a fake backend: what is worked on in which order, what a failure does,
//! how a stopped run carries on, and that a switched-off setting leaves the queue idle.
//!
//! One pass is a list of lanes in a fixed order (waveforms, keys, energy, sections), each lane
//! the tracks that lack that result when the pass was planned. The cheapest result comes first,
//! so a library full of new tracks gets its waveforms before anything heavier starts. A track
//! that fails (or has no result to give) is written to [`Failures`] and is not planned again
//! until its file changes or the user asks for another try; that is what stops a bad file from
//! being retried for ever, after a restart too, because the failures are kept in a file.

use std::collections::{BTreeMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde::{Deserialize, Serialize};

/// What the queue makes, in the order it makes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Peaks,
    Keys,
    Energy,
    Sections,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Peaks, Kind::Keys, Kind::Energy, Kind::Sections];

    /// The word the status line uses.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Peaks => "waveforms",
            Kind::Keys => "keys",
            Kind::Energy => "energy",
            Kind::Sections => "sections",
        }
    }
}

/// The settings (kept in `bg-analysis-settings.json`, one set for the whole app).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// The master switch. Off: the queue does nothing at all.
    pub enabled: bool,
    pub waveforms: bool,
    pub keys: bool,
    pub energy: bool,
    pub sections: bool,
    /// Wait while a mix preview plays, so the music never competes with the analysis for the CPU.
    pub pause_during_playback: bool,
    /// A detected key below this confidence (per cent) is not written; it is listed instead.
    pub key_min_confidence_pct: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { enabled: true, waveforms: true, keys: true, energy: true, sections: true, pause_during_playback: true, key_min_confidence_pct: 0 }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.key_min_confidence_pct > 100 {
            return Err("The key confidence floor must be between 0 and 100 per cent.".into());
        }
        Ok(())
    }

    /// Whether the queue makes `kind` at all under these settings.
    pub fn wants(&self, kind: Kind) -> bool {
        self.enabled
            && match kind {
                Kind::Peaks => self.waveforms,
                Kind::Keys => self.keys,
                Kind::Energy => self.energy,
                Kind::Sections => self.sections,
            }
    }

    pub fn any(&self) -> bool {
        Kind::ALL.iter().any(|&k| self.wants(k))
    }
}

/// One track that lacks a result.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: i64,
    pub location: String,
    /// `tracks.file_size`: a changed file is tried again even if it failed before.
    pub size: i64,
    pub label: String,
}

/// What came of one track.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The result is stored. `detail` is shown with it (the key that was found, say).
    Done { detail: Option<String> },
    /// The file was read fine but holds no result (no clear key). Listed with its reason and not
    /// asked again, like a failure.
    NoResult(String),
    /// The file could not be analysed.
    Failed(String),
    /// The audio file is not there (a drive unplugged, a file moved). Listed like a failure, but
    /// tried again as soon as the file is back, whatever its size.
    Missing(String),
    /// Could not be stored right now (the library is being backed up, say). Nothing is recorded;
    /// the track stays first in its lane and the run stops so it is tried again shortly.
    Later(String),
}

/// A track the queue gave up on, with why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Failure {
    pub kind: Kind,
    pub track_id: i64,
    pub label: String,
    pub location: String,
    pub size: i64,
    pub reason: String,
    /// The file was read fine but holds no result (no clear key): nothing went wrong, and trying
    /// again cannot change it.
    #[serde(default)]
    pub no_result: bool,
    /// The audio file was not there when this was recorded.
    #[serde(default)]
    pub file_missing: bool,
}

impl Failure {
    /// The record an outcome leaves, or None when it leaves none.
    pub fn of(kind: Kind, item: &Item, outcome: &Outcome) -> Option<Failure> {
        let (reason, no_result, file_missing) = match outcome {
            Outcome::Failed(why) => (why, false, false),
            Outcome::NoResult(why) => (why, true, false),
            Outcome::Missing(why) => (why, false, true),
            Outcome::Done { .. } | Outcome::Later(_) => return None,
        };
        Some(Failure { kind, track_id: item.id, label: item.label.clone(), location: item.location.clone(), size: item.size, reason: reason.clone(), no_result, file_missing })
    }
}

/// The tracks the queue will not try again, by kind and location.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Failures(BTreeMap<String, Failure>);

fn failure_key(kind: Kind, location: &str) -> String {
    format!("{}|{location}", kind.word())
}

impl Failures {
    pub fn record(&mut self, kind: Kind, item: &Item, reason: String) {
        self.record_outcome(kind, item, &Outcome::Failed(reason));
    }

    /// Records what [`Failure::of`] makes of an outcome; `Done` and `Later` leave nothing.
    pub fn record_outcome(&mut self, kind: Kind, item: &Item, outcome: &Outcome) {
        if let Some(f) = Failure::of(kind, item, outcome) {
            self.0.insert(failure_key(kind, &item.location), f);
        }
    }

    /// True while the record still describes this file: the same size, and, for a file that was
    /// not there, still not there (`present` says whether it is now).
    /// `present` is only called for a record of a missing file, so a library with no such
    /// records never touches the disk.
    pub fn skips(&self, kind: Kind, item: &Item, present: impl FnOnce() -> bool) -> bool {
        self.0.get(&failure_key(kind, &item.location)).is_some_and(|f| f.size == item.size && !(f.file_missing && present()))
    }

    /// Drops the record of a file that has now been analysed (it was given up on, then came back).
    pub fn forget(&mut self, kind: Kind, location: &str) {
        self.0.remove(&failure_key(kind, location));
    }

    pub fn clear(&mut self) -> usize {
        let n = self.0.len();
        self.0.clear();
        n
    }

    /// Drops the records of tracks that are no longer in the library (`locations`), so the file
    /// does not keep what was deleted. True if anything went.
    pub fn retain_present(&mut self, locations: &std::collections::HashSet<&str>) -> bool {
        let before = self.0.len();
        self.0.retain(|_, f| locations.contains(f.location.as_str()));
        self.0.len() != before
    }

    pub fn list(&self) -> Vec<Failure> {
        let mut v: Vec<Failure> = self.0.values().cloned().collect();
        v.sort_by(|a, b| (a.kind, &a.label).cmp(&(b.kind, &b.label)));
        v
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// What the queue needs from the outside world.
pub trait Backend {
    /// The tracks that lack a `kind` result now, in library order. An error stops the pass.
    fn missing(&mut self, kind: Kind) -> Result<Vec<Item>, String>;
    /// Makes the result for one track.
    fn process(&mut self, kind: Kind, item: &Item, settings: &Settings) -> Outcome;
    /// Saves anything held back (called whenever a run ends, for any reason).
    fn flush(&mut self);
    /// Whether the audio file of `item` is there now. Only asked about a track that was given up
    /// on because its file was missing, so a file that came back is tried again.
    fn file_present(&self, _item: &Item) -> bool {
        false
    }
}

/// Why a run stopped before the work was done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// No library is open.
    NoLibrary,
    /// The user paused it.
    Paused,
    /// The master switch is off, or no kind is switched on.
    Off,
    /// A Pacemaker library: nothing runs until the user starts it.
    Manual,
    /// A mix preview is playing.
    Playing,
    /// A job the user started holds the library.
    Job,
    /// The owner is looking at a Sync plan: keys written now would make it stale.
    Held,
    /// The open library changed, or something asks for a fresh look at what is missing.
    Replan,
    /// Could not store a result right now.
    Blocked(String),
    /// Looking for what is missing failed.
    Scan(String),
}

/// What the caller decides before each track.
pub enum Gate {
    Go,
    Stop(Stop),
}

/// What the queue reports while it runs.
pub enum Event<'a> {
    /// A pass was planned: the number of tracks per lane.
    Planned(&'a [(Kind, usize)]),
    /// Work on a track is about to start (so the status shows what is being read).
    Starting { kind: Kind, done: usize, total: usize, label: &'a str },
    /// A track is finished.
    Result { kind: Kind, item: &'a Item, outcome: &'a Outcome, done: usize, total: usize },
}

pub trait Control {
    fn gate(&mut self) -> Gate;
    fn event(&mut self, e: Event<'_>);
}

/// How a call to [`Engine::run`] ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// Every lane is empty: nothing is missing (apart from what was given up on).
    Idle,
    Stopped(Stop),
}

struct Lane {
    kind: Kind,
    items: VecDeque<Item>,
    total: usize,
    done: usize,
}

#[derive(Default)]
pub struct Engine {
    pub failures: Failures,
    lanes: Vec<Lane>,
    planned: bool,
}

impl Engine {
    pub fn new(failures: Failures) -> Self {
        Engine { failures, lanes: Vec::new(), planned: false }
    }

    /// Forget the plan: the next run looks at what is missing again (a library was opened or
    /// changed, a setting was switched, the user asked for another try).
    pub fn replan(&mut self) {
        self.lanes.clear();
        self.planned = false;
    }

    /// Tracks still waiting, per kind, in lane order (the tests read it).
    #[cfg(test)]
    pub fn remaining(&self) -> Vec<(Kind, usize)> {
        self.lanes.iter().map(|l| (l.kind, l.items.len())).collect()
    }

    fn plan(&mut self, settings: &Settings, b: &mut dyn Backend, c: &mut dyn Control) -> Result<(), String> {
        self.lanes.clear();
        for kind in Kind::ALL {
            if !settings.wants(kind) {
                continue;
            }
            let found = b.missing(kind)?;
            let items: VecDeque<Item> = found.into_iter().filter(|i| !self.failures.skips(kind, i, || b.file_present(i))).collect();
            self.lanes.push(Lane { kind, total: items.len(), items, done: 0 });
        }
        self.planned = true;
        let counts: Vec<(Kind, usize)> = self.lanes.iter().map(|l| (l.kind, l.total)).collect();
        c.event(Event::Planned(&counts));
        Ok(())
    }

    /// Works through the lanes until nothing is left or `c.gate()` says stop. Checked before every
    /// track, so a user job or a pause never waits for more than the track in hand.
    pub fn run(&mut self, settings: &Settings, b: &mut dyn Backend, c: &mut dyn Control) -> RunEnd {
        let end = self.run_inner(settings, b, c);
        b.flush();
        end
    }

    fn run_inner(&mut self, settings: &Settings, b: &mut dyn Backend, c: &mut dyn Control) -> RunEnd {
        if !settings.any() {
            return RunEnd::Stopped(Stop::Off);
        }
        loop {
            if let Gate::Stop(why) = c.gate() {
                return RunEnd::Stopped(why);
            }
            if !self.planned {
                if let Err(e) = self.plan(settings, b, c) {
                    return RunEnd::Stopped(Stop::Scan(e));
                }
            }
            // a lane whose kind has since been switched off is passed over; the caller replans
            // on a settings change anyway, this keeps a stale plan from doing unwanted work
            let Some(lane) = self.lanes.iter_mut().find(|l| !l.items.is_empty() && settings.wants(l.kind)) else {
                return RunEnd::Idle;
            };
            let kind = lane.kind;
            let item = lane.items.front().cloned().expect("lane is not empty");
            c.event(Event::Starting { kind, done: lane.done, total: lane.total, label: &item.label });
            let outcome = catch_unwind(AssertUnwindSafe(|| b.process(kind, &item, settings)))
                .unwrap_or_else(|_| Outcome::Failed("the decoder crashed on this file".into()));
            if let Outcome::Later(why) = &outcome {
                return RunEnd::Stopped(Stop::Blocked(why.clone()));
            }
            lane.items.pop_front();
            lane.done += 1;
            let (done, total) = (lane.done, lane.total);
            match &outcome {
                Outcome::NoResult(_) | Outcome::Failed(_) | Outcome::Missing(_) => self.failures.record_outcome(kind, &item, &outcome),
                Outcome::Done { .. } => self.failures.forget(kind, &item.location),
                Outcome::Later(_) => {}
            }
            c.event(Event::Result { kind, item: &item, outcome: &outcome, done, total });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn item(id: i64) -> Item {
        Item { id, location: format!("/pmdb_tracks/t{id}.mp3"), size: 1000 + id, label: format!("Track {id}") }
    }

    /// A library of `n` tracks where every result is missing; `process` records what it was
    /// asked and applies the scripted outcome, and a finished track stops being missing.
    struct Fake {
        n: i64,
        have: HashSet<(Kind, i64)>,
        script: HashMap<(Kind, i64), Outcome>,
        calls: Vec<(Kind, i64)>,
        scans: Vec<Kind>,
        flushes: usize,
        sizes: HashMap<i64, i64>,
        /// Whether every audio file is there "now" (asked only about tracks given up on as missing).
        present: bool,
    }

    impl Fake {
        fn new(n: i64) -> Self {
            Fake { n, have: HashSet::new(), script: HashMap::new(), calls: Vec::new(), scans: Vec::new(), flushes: 0, sizes: HashMap::new(), present: false }
        }
    }

    impl Backend for Fake {
        fn missing(&mut self, kind: Kind) -> Result<Vec<Item>, String> {
            self.scans.push(kind);
            Ok((1..=self.n)
                .filter(|id| !self.have.contains(&(kind, *id)))
                .map(|id| {
                    let mut i = item(id);
                    if let Some(s) = self.sizes.get(&id) {
                        i.size = *s;
                    }
                    i
                })
                .collect())
        }
        fn process(&mut self, kind: Kind, it: &Item, _: &Settings) -> Outcome {
            self.calls.push((kind, it.id));
            let out = self.script.get(&(kind, it.id)).cloned().unwrap_or(Outcome::Done { detail: None });
            if matches!(out, Outcome::Done { .. }) {
                self.have.insert((kind, it.id));
            }
            out
        }
        fn flush(&mut self) {
            self.flushes += 1;
        }
        fn file_present(&self, _: &Item) -> bool {
            self.present
        }
    }

    /// Lets `go` tracks through, then asks for `then`; records every event.
    struct Script {
        go: usize,
        then: Stop,
        seen: usize,
        planned: Vec<Vec<(Kind, usize)>>,
        results: Vec<(Kind, i64, usize, usize)>,
    }

    impl Script {
        fn unlimited() -> Self {
            Script { go: usize::MAX, then: Stop::Paused, seen: 0, planned: vec![], results: vec![] }
        }
        fn stop_after(go: usize, then: Stop) -> Self {
            Script { go, then, seen: 0, planned: vec![], results: vec![] }
        }
    }

    impl Control for Script {
        fn gate(&mut self) -> Gate {
            if self.seen >= self.go { Gate::Stop(self.then.clone()) } else { Gate::Go }
        }
        fn event(&mut self, e: Event<'_>) {
            match e {
                Event::Planned(p) => self.planned.push(p.to_vec()),
                Event::Starting { .. } => {}
                Event::Result { kind, item, done, total, .. } => {
                    self.seen += 1;
                    self.results.push((kind, item.id, done, total));
                }
            }
        }
    }

    fn all_on() -> Settings {
        Settings::default()
    }

    #[test]
    fn waveforms_come_first_then_keys_energy_sections_whatever_the_library_order() {
        let mut f = Fake::new(3);
        let mut c = Script::unlimited();
        let mut e = Engine::default();
        assert_eq!(e.run(&all_on(), &mut f, &mut c), RunEnd::Idle);
        let kinds: Vec<Kind> = f.calls.iter().map(|c| c.0).collect();
        let mut sorted = kinds.clone();
        sorted.sort();
        assert_eq!(kinds, sorted, "never a later kind before an earlier one");
        assert_eq!(f.calls.len(), 12);
        assert_eq!(f.calls[0], (Kind::Peaks, 1));
        assert_eq!(f.calls[3], (Kind::Keys, 1));
        assert_eq!(c.planned, vec![vec![(Kind::Peaks, 3), (Kind::Keys, 3), (Kind::Energy, 3), (Kind::Sections, 3)]]);
        assert_eq!(c.results[0], (Kind::Peaks, 1, 1, 3), "the strip shows 1 of 3 - waveforms");
        assert_eq!(f.flushes, 1);
    }

    #[test]
    fn a_kind_that_is_switched_off_is_not_even_looked_for() {
        let mut f = Fake::new(2);
        let s = Settings { keys: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        assert_eq!(e.run(&s, &mut f, &mut Script::unlimited()), RunEnd::Idle);
        assert_eq!(f.scans, vec![Kind::Peaks, Kind::Energy]);
        assert!(f.calls.iter().all(|c| c.0 == Kind::Peaks || c.0 == Kind::Energy));
    }

    #[test]
    fn the_master_switch_off_leaves_the_queue_idle_and_touches_nothing() {
        let mut f = Fake::new(5);
        let s = Settings { enabled: false, ..all_on() };
        let mut e = Engine::default();
        let mut c = Script::unlimited();
        assert_eq!(e.run(&s, &mut f, &mut c), RunEnd::Stopped(Stop::Off));
        assert!(f.calls.is_empty());
        assert!(f.scans.is_empty(), "the library is not even scanned");
        assert!(c.planned.is_empty());
        // every kind off with the master on is the same
        let none = Settings { waveforms: false, keys: false, energy: false, sections: false, ..all_on() };
        assert_eq!(e.run(&none, &mut f, &mut Script::unlimited()), RunEnd::Stopped(Stop::Off));
        assert!(f.calls.is_empty());
    }

    #[test]
    fn switching_a_kind_off_part_way_stops_its_lane_without_a_new_plan() {
        let mut f = Fake::new(2);
        let mut e = Engine::default();
        assert_eq!(e.run(&all_on(), &mut f, &mut Script::stop_after(1, Stop::Replan)), RunEnd::Stopped(Stop::Replan));
        let s = Settings { keys: false, ..all_on() };
        assert_eq!(e.run(&s, &mut f, &mut Script::unlimited()), RunEnd::Idle);
        assert!(!f.calls.iter().any(|c| c.0 == Kind::Keys));
    }

    #[test]
    fn a_failing_file_is_recorded_and_skipped_not_retried() {
        let mut f = Fake::new(3);
        f.script.insert((Kind::Peaks, 2), Outcome::Failed("cannot decode".into()));
        let mut e = Engine::default();
        assert_eq!(e.run(&all_on(), &mut f, &mut Script::unlimited()), RunEnd::Idle);
        assert_eq!(f.calls.iter().filter(|c| **c == (Kind::Peaks, 2)).count(), 1, "tried once");
        assert_eq!(e.failures.len(), 1);
        let list = e.failures.list();
        assert_eq!((list[0].kind, list[0].track_id, list[0].reason.as_str()), (Kind::Peaks, 2, "cannot decode"));
        // the other kinds still ran on that track: one bad waveform does not condemn the track
        assert!(f.calls.contains(&(Kind::Keys, 2)));

        // a new pass (a poke, a restart that kept the failures file) does not try it again
        let before = f.calls.len();
        e.replan();
        assert_eq!(e.run(&all_on(), &mut f, &mut Script::unlimited()), RunEnd::Idle);
        assert_eq!(f.calls.len(), before, "nothing is missing but the failure, and it is skipped");
        assert!(!f.calls[before..].contains(&(Kind::Peaks, 2)));
    }

    #[test]
    fn a_track_whose_file_was_missing_is_tried_again_when_the_file_is_back() {
        let mut f = Fake::new(2);
        f.script.insert((Kind::Keys, 1), Outcome::Missing("its audio file is not there".into()));
        let s = Settings { waveforms: false, energy: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        e.run(&s, &mut f, &mut Script::unlimited());
        let list = e.failures.list();
        assert_eq!((list.len(), list[0].track_id, list[0].file_missing, list[0].no_result), (1, 1, true, false), "listed, with why");

        // still gone: left alone, as any failure is
        e.replan();
        let before = f.calls.len();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), before, "the file is still not there");

        // the drive is back, same size: the track is looked at again and now gets its result
        f.present = true;
        f.script.remove(&(Kind::Keys, 1));
        e.replan();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), before + 1);
        assert!(f.have.contains(&(Kind::Keys, 1)));
        assert!(e.failures.list().is_empty(), "the old note goes once the track has its result");
    }

    #[test]
    fn a_file_that_failed_to_decode_is_not_tried_again_just_because_it_exists() {
        let mut f = Fake::new(1);
        f.script.insert((Kind::Keys, 1), Outcome::Failed("cannot decode".into()));
        let s = Settings { waveforms: false, energy: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        e.run(&s, &mut f, &mut Script::unlimited());
        f.present = true;
        e.replan();
        let before = f.calls.len();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), before, "only a missing file is tried again when it is present");
    }

    #[test]
    fn no_result_is_flagged_so_it_is_not_shown_as_a_failure_and_old_files_still_load() {
        let mut f = Fake::new(1);
        f.script.insert((Kind::Keys, 1), Outcome::NoResult("no clear key".into()));
        let s = Settings { waveforms: false, energy: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        e.run(&s, &mut f, &mut Script::unlimited());
        let list = e.failures.list();
        assert_eq!((list[0].no_result, list[0].file_missing), (true, false));
        // a failures file written before these flags existed has neither: both read as false
        let old = r#"{"keys|/pmdb_tracks/a.mp3":{"kind":"keys","track_id":1,"label":"A","location":"/pmdb_tracks/a.mp3","size":5,"reason":"x"}}"#;
        let back: Failures = serde_json::from_str(old).unwrap();
        assert_eq!(back.list().len(), 1);
        assert!(!back.list()[0].no_result && !back.list()[0].file_missing);
    }

    #[test]
    fn a_changed_file_or_the_retry_button_gives_a_failure_another_try() {
        let mut f = Fake::new(1);
        f.script.insert((Kind::Energy, 1), Outcome::Failed("damaged".into()));
        let s = Settings { waveforms: false, keys: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), 1);

        // same size: still skipped
        e.replan();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), 1);

        // the file was replaced (different size): tried again
        f.sizes.insert(1, 5);
        f.script.remove(&(Kind::Energy, 1));
        e.replan();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), 2);

        // the user clears the list: tried again even at the same size
        f.have.clear();
        f.script.insert((Kind::Energy, 1), Outcome::Failed("damaged".into()));
        e.replan();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(e.failures.len(), 1);
        let calls = f.calls.len();
        assert_eq!(e.failures.clear(), 1);
        e.replan();
        e.run(&s, &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), calls + 1);
    }

    #[test]
    fn no_result_is_listed_with_its_reason_and_not_asked_again() {
        let mut f = Fake::new(1);
        f.script.insert((Kind::Keys, 1), Outcome::NoResult("no clear key".into()));
        let mut e = Engine::default();
        e.run(&all_on(), &mut f, &mut Script::unlimited());
        assert_eq!(e.failures.list()[0].reason, "no clear key");
        let n = f.calls.len();
        e.replan();
        e.run(&all_on(), &mut f, &mut Script::unlimited());
        assert_eq!(f.calls.len(), n);
    }

    #[test]
    fn a_panic_in_the_decoder_is_a_failure_not_the_end_of_the_queue() {
        struct Boom(Fake);
        impl Backend for Boom {
            fn missing(&mut self, k: Kind) -> Result<Vec<Item>, String> {
                self.0.missing(k)
            }
            fn process(&mut self, k: Kind, it: &Item, s: &Settings) -> Outcome {
                if k == Kind::Peaks && it.id == 1 {
                    panic!("codec bug");
                }
                self.0.process(k, it, s)
            }
            fn flush(&mut self) {}
        }
        let mut b = Boom(Fake::new(2));
        let s = Settings { keys: false, energy: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        assert_eq!(e.run(&s, &mut b, &mut Script::unlimited()), RunEnd::Idle);
        assert_eq!(e.failures.len(), 1);
        assert!(b.0.calls.contains(&(Kind::Peaks, 2)), "the next track still ran");
    }

    #[test]
    fn a_stopped_run_carries_on_where_it_left_off() {
        let mut f = Fake::new(3);
        let mut e = Engine::default();
        let mut c = Script::stop_after(4, Stop::Job);
        assert_eq!(e.run(&all_on(), &mut f, &mut c), RunEnd::Stopped(Stop::Job));
        assert_eq!(f.calls, vec![(Kind::Peaks, 1), (Kind::Peaks, 2), (Kind::Peaks, 3), (Kind::Keys, 1)]);
        assert_eq!(f.flushes, 1, "what was held back is saved when a run stops");
        assert_eq!(e.remaining()[1], (Kind::Keys, 2));

        // the same engine, no new plan: no repeats, no new scan
        let scans = f.scans.len();
        assert_eq!(e.run(&all_on(), &mut f, &mut Script::unlimited()), RunEnd::Idle);
        assert_eq!(f.scans.len(), scans);
        let mut seen = f.calls.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), f.calls.len(), "no track was done twice");
        assert_eq!(f.calls.len(), 12);
    }

    #[test]
    fn after_a_restart_the_queue_just_finds_what_is_still_missing() {
        let mut f = Fake::new(3);
        let mut first = Engine::default();
        first.run(&all_on(), &mut f, &mut Script::stop_after(5, Stop::Paused));
        let done = f.calls.len();
        assert_eq!(done, 5);

        // a new process: a new engine knows nothing, the backend's stores know what exists
        let mut second = Engine::default();
        let mut c = Script::unlimited();
        assert_eq!(second.run(&all_on(), &mut f, &mut c), RunEnd::Idle);
        assert_eq!(f.calls.len(), 12, "every result exists once, none was made twice");
        assert_eq!(c.planned[0], vec![(Kind::Peaks, 0), (Kind::Keys, 1), (Kind::Energy, 3), (Kind::Sections, 3)]);
    }

    #[test]
    fn a_result_that_cannot_be_stored_yet_is_kept_first_and_the_run_stops() {
        let mut f = Fake::new(2);
        f.script.insert((Kind::Keys, 1), Outcome::Later("the library is being backed up".into()));
        let s = Settings { waveforms: false, energy: false, sections: false, ..all_on() };
        let mut e = Engine::default();
        assert_eq!(e.run(&s, &mut f, &mut Script::unlimited()), RunEnd::Stopped(Stop::Blocked("the library is being backed up".into())));
        assert_eq!(e.failures.len(), 0, "not a failure");
        assert_eq!(e.remaining(), vec![(Kind::Keys, 2)]);
        f.script.remove(&(Kind::Keys, 1));
        assert_eq!(e.run(&s, &mut f, &mut Script::unlimited()), RunEnd::Idle);
        assert_eq!(f.calls, vec![(Kind::Keys, 1), (Kind::Keys, 1), (Kind::Keys, 2)]);
    }

    #[test]
    fn a_library_that_cannot_be_scanned_stops_the_run_with_the_reason() {
        struct Bad;
        impl Backend for Bad {
            fn missing(&mut self, _: Kind) -> Result<Vec<Item>, String> {
                Err("music.db is locked".into())
            }
            fn process(&mut self, _: Kind, _: &Item, _: &Settings) -> Outcome {
                unreachable!()
            }
            fn flush(&mut self) {}
        }
        let mut e = Engine::default();
        assert_eq!(e.run(&all_on(), &mut Bad, &mut Script::unlimited()), RunEnd::Stopped(Stop::Scan("music.db is locked".into())));
    }

    #[test]
    fn failures_survive_a_round_trip_through_json_and_forget_removed_tracks() {
        let mut fl = Failures::default();
        fl.record(Kind::Keys, &item(1), "no clear key".into());
        fl.record(Kind::Peaks, &item(2), "cannot decode".into());
        let back: Failures = serde_json::from_str(&serde_json::to_string(&fl).unwrap()).unwrap();
        assert_eq!(back, fl);
        assert!(back.skips(Kind::Keys, &item(1), || false));
        assert!(!back.skips(Kind::Energy, &item(1), || false), "a failure belongs to one kind");
        let mut gone = back.clone();
        let kept = item(2).location;
        let here: HashSet<&str> = [kept.as_str()].into_iter().collect();
        assert!(gone.retain_present(&here));
        assert_eq!(gone.len(), 1);
        assert_eq!(gone.list()[0].track_id, 2);
    }

    #[test]
    fn settings_default_to_everything_on_and_read_partial_files() {
        let d = Settings::default();
        assert!(d.enabled && d.waveforms && d.keys && d.energy && d.sections && d.pause_during_playback);
        let partial: Settings = serde_json::from_str(r#"{"keys":false}"#).unwrap();
        assert!(!partial.keys && partial.enabled && partial.waveforms);
        assert!(Settings { key_min_confidence_pct: 101, ..d }.validate().is_err());
        assert!(!Settings { enabled: false, ..d }.wants(Kind::Peaks));
    }
}
