//! Decodes the clips of the open mix in the background and hands them to the live engine.
//!
//! One thread decodes the clips nearest the play position first. Decoded audio is kept (a decoded
//! 6-minute track is about 70 MB) until the budget is used up; then the clips farthest from the
//! play position are dropped, and decoded again when the position comes back. A clip the engine
//! asks for before it is decoded is [`SourceState::Pending`]: it plays silent for a moment and is
//! picked up at the next block.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use pm_mixer::audio::Source;
use pm_mixer::engine::SourceState;

/// Bytes per decoded frame: two f32 channels plus the quarter-rate search signal.
const BYTES_PER_FRAME: u64 = 9;

/// One audio file the mix needs and where on the timeline it is used (frames).
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub id: i64,
    /// `None` when the file is not on the volume.
    pub path: Option<PathBuf>,
    pub start: i64,
    pub end: i64,
}

enum Entry {
    Ready(Arc<Source>, u64),
    Failed(String),
}

struct Inner {
    wanted: Vec<Wanted>,
    cache: HashMap<i64, Entry>,
    /// Files decoded and dropped again for the budget, with the bytes they take: not decoded again
    /// until they fit or are nearer the play position than something held.
    parked: HashMap<i64, u64>,
    pos: i64,
    bytes: u64,
    budget: u64,
    stop: bool,
}

impl Inner {
    /// Frames between the play position and the nearest use of the file (0 while it is in use).
    fn distance(&self, w: &Wanted) -> i64 {
        if self.pos < w.start {
            w.start - self.pos
        } else if self.pos > w.end {
            self.pos - w.end
        } else {
            0
        }
    }

    fn dist_of(&self, id: i64) -> i64 {
        self.wanted.iter().filter(|w| w.id == id).map(|w| self.distance(w)).min().unwrap_or(i64::MAX)
    }
}

/// Progress for the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub loaded: usize,
    pub total: usize,
}

pub struct Loader {
    inner: Mutex<Inner>,
    cv: Condvar,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl Loader {
    /// `changed` is called (from the decode thread) after every decode, success or failure.
    pub fn start(budget_bytes: u64, changed: Arc<dyn Fn() + Send + Sync>) -> Arc<Loader> {
        let l = Arc::new(Loader {
            inner: Mutex::new(Inner { wanted: Vec::new(), cache: HashMap::new(), parked: HashMap::new(), pos: 0, bytes: 0, budget: budget_bytes, stop: false }),
            cv: Condvar::new(),
            join: Mutex::new(None),
        });
        let me = l.clone();
        let j = std::thread::Builder::new().name("mix-live-decode".into()).spawn(move || me.run(changed)).expect("cannot start the decode thread");
        *l.join.lock().unwrap() = Some(j);
        l
    }

    /// The files the mix needs now (replacing the earlier list). Decoded files that are no longer
    /// needed are dropped; files that failed are tried again only if their path changed.
    pub fn set_wanted(&self, wanted: Vec<Wanted>) {
        let mut g = self.inner.lock().unwrap();
        let keep: Vec<i64> = wanted.iter().map(|w| w.id).collect();
        let dropped: Vec<i64> = g.cache.keys().copied().filter(|k| !keep.contains(k)).collect();
        for k in dropped {
            if let Some(Entry::Ready(_, b)) = g.cache.remove(&k) {
                g.bytes -= b;
            }
        }
        // a file that failed because it was missing gets another chance once the mix names a path
        let retry: Vec<i64> = wanted.iter().filter(|w| w.path.is_some()).map(|w| w.id).filter(|id| matches!(g.cache.get(id), Some(Entry::Failed(_)))).collect();
        for k in retry {
            if g.wanted.iter().find(|w| w.id == k).map(|w| &w.path) != wanted.iter().find(|w| w.id == k).map(|w| &w.path) {
                g.cache.remove(&k);
            }
        }
        g.parked.retain(|k, _| keep.contains(k));
        g.wanted = wanted;
        self.cv.notify_all();
    }

    /// Where the music is now (timeline frames), so the nearest clips decode first.
    pub fn set_position(&self, frame: i64) {
        let mut g = self.inner.lock().unwrap();
        if (g.pos - frame).abs() > 44_100 {
            self.cv.notify_all();
        }
        g.pos = frame;
    }

    pub fn progress(&self) -> Progress {
        let g = self.inner.lock().unwrap();
        let ids: std::collections::BTreeSet<i64> = g.wanted.iter().map(|w| w.id).collect();
        Progress { total: ids.len(), loaded: ids.iter().filter(|id| g.cache.contains_key(id)).count() }
    }

    /// Decoded bytes held right now.
    #[allow(dead_code)] // read by the tests and the measurements
    pub fn bytes(&self) -> u64 {
        self.inner.lock().unwrap().bytes
    }

    /// The audio of a clip for the engine.
    pub fn get(&self, library_id: i64) -> SourceState {
        let g = self.inner.lock().unwrap();
        match g.cache.get(&library_id) {
            Some(Entry::Ready(s, _)) => SourceState::Ready(s.clone()),
            Some(Entry::Failed(e)) => SourceState::Failed(e.clone()),
            None => match g.wanted.iter().find(|w| w.id == library_id) {
                Some(w) if w.path.is_none() => SourceState::Failed(if library_id < 0 { "the track was removed from the library".into() } else { "the audio file is not on the volume".into() }),
                _ => SourceState::Pending,
            },
        }
    }

    pub fn shutdown(&self) {
        {
            let mut g = self.inner.lock().unwrap();
            g.stop = true;
            self.cv.notify_all();
        }
        if let Some(j) = self.join.lock().unwrap().take() {
            let _ = j.join();
        }
    }

    fn run(&self, changed: Arc<dyn Fn() + Send + Sync>) {
        loop {
            let job = {
                let mut g = self.inner.lock().unwrap();
                loop {
                    if g.stop {
                        return;
                    }
                    if let Some(j) = self.next(&g) {
                        break j;
                    }
                    g = self.cv.wait(g).unwrap();
                }
            };
            let (id, path) = job;
            let res = pm_mixer::audio::load(&path);
            {
                let mut g = self.inner.lock().unwrap();
                if g.stop {
                    return;
                }
                // the mix may have changed while the file decoded
                let still = g.wanted.iter().any(|w| w.id == id && w.path.as_ref() == Some(&path));
                if still {
                    match res {
                        Ok(src) => {
                            let b = src.len() as u64 * BYTES_PER_FRAME;
                            g.bytes += b;
                            g.cache.insert(id, Entry::Ready(src, b));
                            g.parked.remove(&id);
                            self.evict(&mut g);
                        }
                        Err(e) => {
                            g.cache.insert(id, Entry::Failed(e.to_string()));
                        }
                    }
                }
            }
            changed();
        }
    }

    /// The nearest file that is wanted but not held, when there is room for it or it is nearer than
    /// something held (which would then make room).
    fn next(&self, g: &Inner) -> Option<(i64, PathBuf)> {
        let farthest_held = g.cache.iter().filter(|(_, e)| matches!(e, Entry::Ready(..))).map(|(id, _)| g.dist_of(*id)).max();
        g.wanted
            .iter()
            .filter(|w| w.path.is_some() && !g.cache.contains_key(&w.id))
            .filter(|w| {
                let fits = match g.parked.get(&w.id) {
                    Some(size) => g.bytes + size <= g.budget,
                    None => g.bytes < g.budget,
                };
                fits || farthest_held.is_some_and(|f| g.dist_of(w.id) < f)
            })
            .min_by_key(|w| g.distance(w))
            .map(|w| (w.id, w.path.clone().unwrap()))
    }

    /// Drops the farthest held clips while over budget, the one just loaded included when it is the
    /// farthest (it is parked then, and comes back when it fits or is nearer). A single clip stays
    /// even when it alone is over budget.
    fn evict(&self, g: &mut Inner) {
        while g.bytes > g.budget {
            let ready: Vec<(i64, i64)> = g.cache.iter().filter(|(_, e)| matches!(e, Entry::Ready(..))).map(|(id, _)| (*id, g.dist_of(*id))).collect();
            if ready.len() < 2 {
                break;
            }
            let Some((id, _)) = ready.into_iter().max_by_key(|(_, d)| *d) else { break };
            if let Some(Entry::Ready(_, b)) = g.cache.remove(&id) {
                g.bytes -= b;
                g.parked.insert(id, b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::*};
    use std::time::{Duration, Instant};

    fn wav(path: &std::path::Path, secs: f32) {
        let n = (secs * 44100.0) as usize;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n as u32 * 4).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&44100u32.to_le_bytes());
        b.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n as u32 * 4).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 * 0.05).sin() * 8000.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    fn wait(what: &str, mut f: impl FnMut() -> bool) {
        let t = Instant::now();
        while !f() {
            assert!(t.elapsed() < Duration::from_secs(10), "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmrev-live-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn decodes_pending_then_ready_and_reports_a_missing_file() {
        let d = dir("ready");
        wav(&d.join("a.wav"), 1.0);
        let count = Arc::new(AtomicUsize::new(0));
        let c2 = count.clone();
        let l = Loader::start(u64::MAX, Arc::new(move || {
            c2.fetch_add(1, Relaxed);
        }));
        l.set_wanted(vec![Wanted { id: 1, path: Some(d.join("a.wav")), start: 0, end: 44100 }, Wanted { id: 2, path: None, start: 0, end: 10 }, Wanted { id: -3, path: None, start: 0, end: 10 }]);
        assert!(matches!(l.get(2), SourceState::Failed(_)));
        assert!(matches!(l.get(-3), SourceState::Failed(e) if e.contains("removed")));
        wait("the decode", || matches!(l.get(1), SourceState::Ready(_)));
        assert_eq!(l.progress(), Progress { loaded: 1, total: 3 });
        assert!(count.load(Relaxed) >= 1);
        l.shutdown();
    }

    #[test]
    fn over_budget_keeps_the_clips_nearest_the_position() {
        let d = dir("budget");
        for k in 0..3 {
            wav(&d.join(format!("{k}.wav")), 1.0);
        }
        // room for about two of the three one-second files
        let l = Loader::start(2 * 44100 * BYTES_PER_FRAME + 1000, Arc::new(|| {}));
        let w = |k: i64| Wanted { id: k, path: Some(d.join(format!("{k}.wav"))), start: k * 10 * 44100, end: k * 10 * 44100 + 44100 };
        l.set_position(0);
        l.set_wanted(vec![w(0), w(1), w(2)]);
        wait("two decoded", || l.progress().loaded == 2);
        std::thread::sleep(Duration::from_millis(200));
        assert!(matches!(l.get(0), SourceState::Ready(_)) && matches!(l.get(1), SourceState::Ready(_)));
        assert!(matches!(l.get(2), SourceState::Pending), "the far clip waits");
        // moving to the end swaps the far clip in and the first one out
        l.set_position(20 * 44100 + 100);
        wait("the far clip", || matches!(l.get(2), SourceState::Ready(_)));
        // the far clip becomes visible before the farthest one is evicted, so wait for the eviction
        wait("the eviction", || l.bytes() <= 2 * 44100 * BYTES_PER_FRAME + 1000);
        l.shutdown();
    }

    #[test]
    fn a_file_that_cannot_be_decoded_fails_once_and_shutdown_joins() {
        let d = dir("bad");
        std::fs::write(d.join("bad.wav"), b"not audio").unwrap();
        let l = Loader::start(u64::MAX, Arc::new(|| {}));
        l.set_wanted(vec![Wanted { id: 5, path: Some(d.join("bad.wav")), start: 0, end: 10 }]);
        wait("the failure", || matches!(l.get(5), SourceState::Failed(_)));
        l.shutdown();
    }
}
