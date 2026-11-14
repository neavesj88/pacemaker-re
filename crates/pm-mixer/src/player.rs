//! The live player's worker thread: owns the [`Live`] engine, keeps the [`Ring`] filled a quarter
//! of a second ahead of the audio device and answers the transport commands.
//!
//! The device side (WASAPI in the app) only calls [`Ring::fill`]. Everything that costs time
//! (decoding lookups, stretching, tools, the limiter) happens here, on this thread.
//!
//! Edits while playing: the ring already holds the old mix up to a few hundred milliseconds
//! ahead. [`Command::Update`] builds a new engine at the timeline position of a point
//! [`SAFE_FRAMES`] ahead of the device's read position, renders [`XFADE_FRAMES`] of it, blends
//! those into the old audio queued at that point and moves the write position there. The device
//! hears the old mix for about 90 ms more, then a 30 ms crossfade into the new one; nothing
//! stops and nothing clicks.

use std::sync::atomic::{AtomicU64, Ordering::*};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pm_formats::mix::{Mix, Track};

use crate::engine::SourceState;
use crate::live::{Live, LiveOptions};
use crate::ring::Ring;

/// How far ahead of the device the ring is kept filled (250 ms).
pub const TARGET_FRAMES: u64 = 11_025;
/// Frames rendered per refill step.
const CHUNK: usize = 1024;
/// Queued frames needed before the device starts playing (the first block is the expensive one).
const START_FRAMES: u64 = 2048;
/// Distance between the device's read position and the point where an edit takes over; larger
/// than any device buffer we open (about 93 ms).
pub const SAFE_FRAMES: u64 = 4096;
/// Length of the crossfade from the old mix into the edited one (35 ms).
pub const XFADE_FRAMES: usize = 1536;
/// Longest wait for the device to finish a fade-out.
const STOP_WAIT: Duration = Duration::from_millis(60);
/// Position events while playing (about 30 Hz).
const POSITION_EVERY: Duration = Duration::from_millis(33);

pub enum Command {
    /// A (new) mix, replacing any other. Stopped, at frame 0.
    Load(Mix, LiveOptions),
    /// An edited mix, applied now.
    Update(Mix, LiveOptions),
    Play(i64),
    Pause,
    Seek(i64),
    SetMaster(f64),
    Unload,
    Shutdown,
}

/// What the worker reports.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Timeline position in seconds and whether audio is running; about 30 times a second while
    /// playing and once on every change of state.
    Position { sec: f64, playing: bool },
    /// Something to tell the user (an underrun, a clip that cannot be played).
    Note(String),
}

/// Timings the worker keeps, for measurements.
#[derive(Default)]
pub struct Stats {
    /// Microseconds from the last play / seek command to the moment the device was released.
    pub first_audio_us: AtomicU64,
    /// Blocks (refill steps) rendered, their total and their longest time in microseconds.
    pub steps: AtomicU64,
    pub step_total_us: AtomicU64,
    pub step_max_us: AtomicU64,
    /// Steps that took longer than the audio they produced.
    pub slow_steps: AtomicU64,
    /// Passes of the worker loop (a spinning worker shows up here).
    pub loops: AtomicU64,
}

pub type Getter = Box<dyn FnMut(&Track) -> SourceState + Send>;
pub type Notify = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Player {
    tx: Sender<Command>,
    join: Option<JoinHandle<()>>,
    pub ring: Arc<Ring>,
    pub stats: Arc<Stats>,
}

impl Player {
    /// Starts the worker over `ring` (which the device callback reads). `get` supplies clip audio.
    pub fn spawn(ring: Arc<Ring>, get: Getter, notify: Notify) -> Player {
        let (tx, rx) = channel();
        let stats = Arc::new(Stats::default());
        let mut w = Worker {
            ring: ring.clone(),
            get,
            notify,
            stats: stats.clone(),
            live: None,
            mix: None,
            opts: LiveOptions::default(),
            playing: false,
            ending: false,
            paused_at: 0,
            base_idx: 0,
            base_tl: 0,
            last_underruns: 0,
            last_pos: Instant::now(),
            update_cost: 0,
            cl: vec![0.0; CHUNK],
            cr: vec![0.0; CHUNK],
        };
        let join = std::thread::Builder::new()
            .name("mix-live".into())
            .spawn(move || {
                // a panic in the renderer must not leave a player that looks alive and plays nothing
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loop {
                    w.stats.loops.fetch_add(1, Relaxed);
                    // while the last frames are queued there is nothing to render: wait, do not spin
                    let wait = if w.playing && !w.ending && w.ring.len() < TARGET_FRAMES as usize { Duration::ZERO } else { Duration::from_millis(4) };
                    let mut batch = Vec::new();
                    match rx.recv_timeout(wait) {
                        Ok(c) => batch.push(c),
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    while let Ok(c) = rx.try_recv() {
                        batch.push(c);
                    }
                    // a drag sends an Update per mouse move: only the newest of them is built (it
                    // carries the whole mix); Play / Seek / Pause keep their order
                    let last_update = batch.iter().rposition(|c| matches!(c, Command::Update(..)));
                    for (i, c) in batch.into_iter().enumerate() {
                        match c {
                            Command::Shutdown => {
                                w.halt();
                                return;
                            }
                            Command::Update(m, o) if last_update.is_some_and(|l| i < l) => {
                                // superseded: remember the mix so a Play / Seek in between sees it
                                let master = o.master_gain_db;
                                w.mix = Some(m);
                                w.opts = LiveOptions { master_gain_db: master, ..o };
                            }
                            c => w.command(c),
                        }
                    }
                    w.tick();
                }
                w.halt();
                }));
                if run.is_err() {
                    w.playing = false;
                    // fade the device out (no click), then drop what is queued
                    w.stop_device();
                    const DEAD: &str = "The live player stopped unexpectedly. Close and reopen the mix to play it again.";
                    (w.notify)(Event::Note(DEAD.into()));
                    w.emit_position();
                    while let Ok(c) = rx.recv() {
                        if matches!(c, Command::Shutdown) {
                            break;
                        }
                        if matches!(c, Command::Play(_) | Command::Pause | Command::Seek(_) | Command::Load(..)) {
                            (w.notify)(Event::Note(DEAD.into()));
                        }
                    }
                }
            })
            .expect("cannot start the mix-live thread");
        Player { tx, join: Some(join), ring, stats }
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    /// A handle that can send commands from elsewhere (the device's error callback).
    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

struct Worker {
    ring: Arc<Ring>,
    get: Getter,
    notify: Notify,
    stats: Arc<Stats>,
    live: Option<Live>,
    mix: Option<Mix>,
    opts: LiveOptions,
    playing: bool,
    /// The engine has produced the last frame; the ring is running dry.
    ending: bool,
    /// Where playback resumes (timeline frames) while paused.
    paused_at: i64,
    /// Ring index `base_idx` plays timeline frame `base_tl`; later indices follow one to one.
    base_idx: u64,
    base_tl: i64,
    last_underruns: u64,
    last_pos: Instant,
    /// Frames of audio time the last update took to build and render.
    update_cost: u64,
    cl: Vec<f32>,
    cr: Vec<f32>,
}

impl Worker {
    /// Marks that the last frames of the mix are (not) in the ring: the device running dry after them
    /// is the end of the music, not an underrun.
    fn set_ending(&mut self, e: bool) {
        self.ending = e;
        self.ring.set_last(e);
    }

    fn note(&self, s: impl Into<String>) {
        (self.notify)(Event::Note(s.into()));
    }

    fn position_frame(&self) -> i64 {
        let tail = self.ring.tail() as i64;
        let tl = self.base_tl + (tail - self.base_idx as i64);
        match &self.live {
            Some(l) => tl.clamp(0, l.end()),
            None => tl.max(0),
        }
    }

    fn emit_position(&mut self) {
        let frame = if self.playing { self.position_frame() } else { self.paused_at };
        (self.notify)(Event::Position { sec: frame as f64 / f64::from(crate::RATE), playing: self.playing });
        self.last_pos = Instant::now();
    }

    fn command(&mut self, c: Command) {
        match c {
            Command::Load(mix, opts) => {
                self.stop_device();
                self.mix = Some(mix);
                self.opts = opts;
                self.paused_at = 0;
                self.build(0);
                self.emit_position();
            }
            Command::Update(mix, opts) => self.update(mix, opts),
            Command::Play(f) => self.play(f, true),
            Command::Pause => {
                if self.playing {
                    self.stop_device();
                    self.paused_at = self.position_frame();
                    self.playing = false;
                }
                self.emit_position();
            }
            Command::Seek(f) => {
                if self.playing {
                    self.play(f, false);
                } else {
                    self.paused_at = f.max(0);
                    self.emit_position();
                }
            }
            Command::SetMaster(db) => {
                self.opts.master_gain_db = db;
                if let Some(l) = self.live.as_mut() {
                    l.set_master_db(db);
                }
            }
            Command::Unload => {
                self.halt();
                self.live = None;
                self.mix = None;
                self.paused_at = 0;
                self.emit_position();
            }
            Command::Shutdown => self.halt(),
        }
    }

    /// Makes the engine for the current mix at `frame`; `None` when nothing in the mix is audible.
    fn build(&mut self, frame: i64) {
        self.live = None;
        if let Some(m) = &self.mix {
            match Live::new(m, &self.opts, frame) {
                Ok(l) => self.live = Some(l),
                Err(e) => self.note(e.to_string()),
            }
        }
    }

    /// Fades the device out and waits for it; afterwards nothing is read from the ring.
    fn stop_device(&mut self) {
        self.ring.set_run(false);
        let t = Instant::now();
        while !self.ring.stopped() && t.elapsed() < STOP_WAIT {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.ring.flush();
    }

    fn halt(&mut self) {
        self.stop_device();
        self.playing = false;
    }

    /// `wrap`: pressing Play at (or just before) the end starts the mix again; a seek only clamps.
    fn play(&mut self, frame: i64, wrap: bool) {
        let t0 = Instant::now();
        if self.live.is_none() {
            self.build(frame.max(0));
        }
        if self.live.is_none() {
            self.playing = false;
            self.emit_position();
            return;
        }
        self.stop_device();
        let live = self.live.as_mut().unwrap();
        let frame = if wrap && frame >= live.end() - 2205 { 0 } else { frame.clamp(0, live.end()) };
        live.seek(frame);
        self.base_idx = self.ring.head();
        self.base_tl = frame;
        self.set_ending(false);
        self.playing = true;
        self.paused_at = frame;
        self.last_underruns = self.ring.underruns();
        // queue the first stretch, then release the device
        while self.ring.len() < START_FRAMES as usize && !self.ending {
            self.refill_once();
        }
        self.ring.set_run(true);
        self.stats.first_audio_us.store(t0.elapsed().as_micros() as u64, Relaxed);
        self.emit_position();
    }

    /// Renders one chunk into the ring.
    fn refill_once(&mut self) {
        let Some(live) = self.live.as_mut() else {
            self.set_ending(true);
            return;
        };
        let t = Instant::now();
        let n = live.render(&mut self.get, &mut self.cl, &mut self.cr);
        let us = t.elapsed().as_micros() as u64;
        self.stats.steps.fetch_add(1, Relaxed);
        self.stats.step_total_us.fetch_add(us, Relaxed);
        self.stats.step_max_us.fetch_max(us, Relaxed);
        if us as f64 > n as f64 / f64::from(crate::RATE) * 1e6 {
            self.stats.slow_steps.fetch_add(1, Relaxed);
        }
        let pushed = self.ring.push(&self.cl[..n], &self.cr[..n]);
        debug_assert_eq!(pushed, n, "refill never asks for more than the ring has free");
        if n < CHUNK {
            self.ending = true;
            self.ring.set_last(true);
        }
        for w in live.take_warnings() {
            (self.notify)(Event::Note(w));
        }
    }

    fn tick(&mut self) {
        if !self.playing {
            return;
        }
        if !self.ending {
            // never more than fits, never past the target
            let mut guard = 0;
            while !self.ending && self.ring.len() < TARGET_FRAMES as usize && self.ring.free() >= CHUNK && guard < 8 {
                self.refill_once();
                guard += 1;
            }
        } else if self.ring.is_empty() {
            // the last frame has been played
            self.ring.set_run(false);
            self.playing = false;
            self.paused_at = self.live.as_ref().map_or(0, |l| l.end());
            self.emit_position();
            return;
        }
        let u = self.ring.underruns();
        if u != self.last_underruns {
            self.last_underruns = u;
            self.note("The audio ran short for a moment (the mix could not be rendered fast enough, or a track was still loading).");
        }
        if self.last_pos.elapsed() >= POSITION_EVERY {
            self.emit_position();
        }
    }

    fn update(&mut self, mix: Mix, opts: LiveOptions) {
        let master = opts.master_gain_db;
        self.mix = Some(mix);
        self.opts = LiveOptions { master_gain_db: master, ..opts };
        if !self.playing {
            let at = self.paused_at;
            self.build(at);
            return;
        }
        // Where the edit takes over: far enough ahead that building and rendering finishes before
        // the device gets there. The margin follows what the last update cost (an unoptimised build
        // is 10 to 20 times slower than release), and a try that still lost the race is redone.
        let mut margin = SAFE_FRAMES.max(self.update_cost * 2 + 256).min(TARGET_FRAMES);
        let mut attempt = 0;
        let (resume, tl, fresh, nl, nr, got, had_old) = loop {
            attempt += 1;
            let t0 = Instant::now();
            let (tail, head) = (self.ring.tail(), self.ring.head());
            let resume = if head >= tail + margin { tail + margin } else { head };
            let tl = self.base_tl + (resume as i64 - self.base_idx as i64);
            let mut fresh = match Live::new(self.mix.as_ref().unwrap(), &self.opts, tl) {
                Ok(l) => l,
                Err(_) => {
                    // nothing audible any more: let the queued audio play out
                    self.live = None;
                    self.set_ending(true);
                    return;
                }
            };
            let (mut nl, mut nr) = (vec![0.0f32; XFADE_FRAMES], vec![0.0f32; XFADE_FRAMES]);
            let got = fresh.render(&mut self.get, &mut nl, &mut nr);
            let (mut ol, mut or) = (vec![0.0f32; XFADE_FRAMES], vec![0.0f32; XFADE_FRAMES]);
            self.ring.read_at(resume, &mut ol, &mut or);
            let had_old = head.saturating_sub(resume).min(XFADE_FRAMES as u64) as usize;
            for k in 0..XFADE_FRAMES {
                // old audio fades out only where there is some; where the queue ended the new one is alone
                let w = if k < had_old { (k + 1) as f32 / XFADE_FRAMES as f32 } else { 1.0 };
                nl[k] = ol[k] * (1.0 - w) + nl[k] * w;
                nr[k] = or[k] * (1.0 - w) + nr[k] * w;
            }
            let cost = (t0.elapsed().as_secs_f64() * f64::from(crate::RATE)) as u64;
            self.update_cost = cost;
            // the device passed the take-over point while this was being built: try again further ahead
            if self.ring.tail() + 64 > resume && self.ring.head() > resume && attempt < 3 {
                margin = SAFE_FRAMES.max(cost * 2 + 256).max(margin * 2).min(TARGET_FRAMES);
                continue;
            }
            break (resume, tl, fresh, nl, nr, got, had_old);
        };
        self.ring.rewrite_from(resume, &nl[..got.max(had_old).min(XFADE_FRAMES)], &nr[..got.max(had_old).min(XFADE_FRAMES)]);
        self.base_idx = resume;
        self.base_tl = tl;
        self.set_ending(got < XFADE_FRAMES);
        self.live = Some(fresh);
        if let Some(l) = self.live.as_mut() {
            l.set_master_db(master);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Source;
    use crate::ring::Consumer;
    use pm_formats::mix::{Position, Transition};
    use std::sync::Mutex;

    fn tone(freq: f32, secs: f32, amp: f32) -> Arc<Source> {
        let n = (secs * 44100.0) as usize;
        let l: Vec<f32> = (0..n).map(|i| ((i as f32) * freq * std::f32::consts::TAU / 44100.0).sin() * amp).collect();
        Source::new(l.clone(), l)
    }

    fn track(id: i64, start: f64, len: f64, speed: f32) -> Track {
        Track::new(id, "x.wav", 120.0, speed, Position { start, length: len, limit_size: true, ..Position::default() })
    }

    /// `n` clips of 20 s each, overlapping by 4 s, with a crossfade in every overlap.
    pub(super) fn mix_of(n: usize, speed: f32) -> Mix {
        let s = 44100.0;
        let mut m = Mix::default();
        for k in 0..n {
            m.tracks.push(track(k as i64 + 1, k as f64 * 16.0 * s, 20.0 * s, speed));
            if k > 0 {
                m.transitions.push(Transition::new(Position { start: k as f64 * 16.0 * s, length: 4.0 * s, ..Position::default() }));
            }
        }
        m
    }

    pub(super) fn getter(n: usize) -> Getter {
        let srcs: Vec<Arc<Source>> = (0..n).map(|k| tone(180.0 + 70.0 * k as f32, 22.0, 0.4)).collect();
        Box::new(move |t: &Track| SourceState::Ready(srcs[(t.library_id - 1) as usize].clone()))
    }

    /// A stand-in for the device: pulls `frames` per call every `frames / 44100` seconds and records.
    struct Fake {
        stop: Arc<std::sync::atomic::AtomicBool>,
        out: Arc<Mutex<Vec<f32>>>,
        join: Option<JoinHandle<()>>,
    }

    impl Fake {
        fn start(ring: Arc<Ring>, frames: usize, speed: f64, record: bool) -> Fake {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let out = Arc::new(Mutex::new(Vec::new()));
            let (s2, o2) = (stop.clone(), out.clone());
            let join = std::thread::spawn(move || {
                let mut st = Consumer::default();
                let mut buf = vec![0.0f32; frames * 2];
                let period = Duration::from_secs_f64(frames as f64 / 44100.0 / speed);
                let mut next = Instant::now();
                while !s2.load(Relaxed) {
                    ring.fill(&mut buf, &mut st);
                    if record {
                        o2.lock().unwrap().extend(buf.chunks(2).map(|c| c[0]));
                    }
                    next += period;
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    }
                }
            });
            Fake { stop, out, join: Some(join) }
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.stop.store(true, Relaxed);
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    fn collect_events() -> (Notify, Arc<Mutex<Vec<Event>>>) {
        let ev = Arc::new(Mutex::new(Vec::new()));
        let e2 = ev.clone();
        (Arc::new(move |e| e2.lock().unwrap().push(e)), ev)
    }

    fn wait_for(cond: impl Fn() -> bool, secs: f64) -> bool {
        let t = Instant::now();
        while t.elapsed().as_secs_f64() < secs {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        cond()
    }

    #[test]
    fn plays_a_mix_to_the_end_and_reports_it() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, events) = collect_events();
        let p = Player::spawn(ring.clone(), getter(2), notify);
        let fake = Fake::start(ring, 512, 20.0, true);
        let m = mix_of(2, 1.0);
        let (offline_len, offline_peak) = {
            let (l, _, _) = crate::engine::render_to_vecs(&m, &Res(2), &crate::engine::Options::default()).unwrap();
            (l.len(), l.iter().fold(0.0f32, |a, b| a.max(b.abs())))
        };
        p.send(Command::Load(m, LiveOptions::default()));
        p.send(Command::Play(0));
        assert!(wait_for(|| events.lock().unwrap().iter().any(|e| matches!(e, Event::Position { playing: false, sec } if *sec > 30.0)), 20.0), "never reached the end");
        let rec = fake.out.lock().unwrap().clone();
        let peak = rec.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!((peak - offline_peak).abs() < 0.02, "peak {peak} vs {offline_peak}");
        // the fake device may run dry now and then at 20x real time in a debug build (silence), so
        // count the frames that carry audio rather than the span
        let audible = rec.iter().filter(|&&x| x != 0.0).count();
        assert!((audible as i64 - offline_len as i64).abs() < 1500, "{audible} vs {offline_len}");
        drop(p);
    }

    pub(super) struct Res(pub usize);
    impl crate::engine::Resolver for Res {
        fn source(&self, t: &Track) -> Result<Arc<Source>, crate::Error> {
            Ok(tone(180.0 + 70.0 * (t.library_id - 1) as f32, 22.0, 0.4))
        }
    }

    #[test]
    fn an_edit_while_playing_crossfades_without_a_jump() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, events) = collect_events();
        let p = Player::spawn(ring.clone(), getter(3), notify);
        let fake = Fake::start(ring.clone(), 480, 2.0, true);
        let m = mix_of(3, 1.0);
        p.send(Command::Load(m.clone(), LiveOptions::default()));
        // inside the crossfade of clips 1 and 2 (16 s to 20 s)
        p.send(Command::Play(44100 * 15));
        assert!(wait_for(|| ring.tail() > 44100, 10.0));
        // a hard edit: clip 1 drops by 12 dB
        let lo = LiveOptions { clip_gain_db: vec![-12.0, 0.0, 0.0], ..LiveOptions::default() };
        p.send(Command::Update(m.clone(), lo));
        assert!(wait_for(|| ring.tail() > 44100 * 2 + 20_000, 10.0));
        p.send(Command::Pause);
        std::thread::sleep(Duration::from_millis(100));
        let rec = fake.out.lock().unwrap().clone();
        // the strongest sample-to-sample step of a 180 Hz / 0.4 tone is 0.4 * tau * 180 / 44100 ~ 0.01;
        // a crossfade between two tones may add a little, a cut would show as a jump of 0.4 or more
        let worst = rec.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 0.04, "discontinuity {worst}");
        // and the edit was heard: the last stretch is quieter than the first
        let n = rec.len();
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let first = rec.iter().position(|&x| x != 0.0).unwrap();
        assert!(rms(&rec[n - 4000..]) < rms(&rec[first + 2000..first + 6000]) * 0.7, "the edit was not heard");
        assert!(events.lock().unwrap().iter().all(|e| !matches!(e, Event::Note(n) if n.contains("ran short"))), "underrun on update");
        drop(p);
        drop(fake);
    }

    #[test]
    fn a_panic_in_the_worker_is_reported_and_later_plays_say_so() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, events) = collect_events();
        let p = Player::spawn(ring.clone(), Box::new(|_t| panic!("codec panic")), notify);
        p.send(Command::Load(mix_of(2, 1.0), LiveOptions::default()));
        p.send(Command::Play(0));
        let dead = |n: &String| n.contains("stopped unexpectedly");
        assert!(wait_for(|| events.lock().unwrap().iter().any(|e| matches!(e, Event::Note(n) if dead(n))), 10.0), "no message after the panic");
        let before = events.lock().unwrap().iter().filter(|e| matches!(e, Event::Note(n) if dead(n))).count();
        p.send(Command::Play(0));
        assert!(wait_for(|| events.lock().unwrap().iter().filter(|e| matches!(e, Event::Note(n) if dead(n))).count() > before, 5.0));
        drop(p);
    }

    #[test]
    fn seek_while_playing_lands_on_the_new_position() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, events) = collect_events();
        let p = Player::spawn(ring.clone(), getter(2), notify);
        let _fake = Fake::start(ring.clone(), 480, 4.0, false);
        p.send(Command::Load(mix_of(2, 1.0), LiveOptions::default()));
        p.send(Command::Play(0));
        assert!(wait_for(|| ring.tail() > 20_000, 10.0));
        p.send(Command::Seek(44100 * 20));
        assert!(wait_for(|| events.lock().unwrap().iter().rev().take(5).any(|e| matches!(e, Event::Position { sec, playing: true } if *sec >= 20.0 && *sec < 21.0)), 10.0));
        p.send(Command::Pause);
        assert!(wait_for(|| matches!(events.lock().unwrap().last(), Some(Event::Position { playing: false, .. })), 5.0));
        drop(p);
    }

    /// A drag sends an Update every 30 ms for seconds: every one must be absorbed without the
    /// device running dry or a click, and a backlog must not build up.
    #[test]
    fn a_stream_of_updates_while_playing_causes_no_underruns() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, _events) = collect_events();
        let p = Player::spawn(ring.clone(), getter(3), notify);
        let fake = Fake::start(ring.clone(), 480, 1.0, true);
        let m = mix_of(3, 1.07);
        p.send(Command::Load(m.clone(), LiveOptions::default()));
        p.send(Command::Play(44100 * 14));
        assert!(wait_for(|| ring.tail() > 4410, 10.0));
        let u0 = ring.underruns();
        let t = Instant::now();
        let mut k = 0;
        while t.elapsed().as_secs_f64() < 3.0 {
            let g = -((k % 20) as f64) * 0.3;
            p.send(Command::Update(m.clone(), LiveOptions { clip_gain_db: vec![g, 0.0, 0.0], ..LiveOptions::default() }));
            k += 1;
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(k > 80, "only {k} updates sent");
        p.send(Command::Pause);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(ring.underruns() - u0, 0, "the device ran dry during the updates");
        let rec = fake.out.lock().unwrap().clone();
        let worst = rec.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 0.1, "click of {worst}");
        drop(p);
        drop(fake);
    }

    /// While the last frames wait to be played the worker sleeps instead of spinning.
    #[test]
    fn the_worker_does_not_spin_at_the_end_of_the_mix() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, _ev) = collect_events();
        let p = Player::spawn(ring.clone(), getter(1), notify);
        // no consumer: the ring holds the whole short mix and nothing drains it
        let m = {
            let mut m = Mix::default();
            m.tracks.push(track(1, 0.0, 0.15 * 44100.0, 1.0));
            m
        };
        p.send(Command::Load(m, LiveOptions::default()));
        p.send(Command::Play(0));
        std::thread::sleep(Duration::from_millis(200));
        let (steps, loops) = (p.stats.steps.load(Relaxed), p.stats.loops.load(Relaxed));
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(p.stats.steps.load(Relaxed), steps, "still rendering after the end");
        // a 4 ms wait gives about 125 passes in 500 ms; a spinning loop gives hundreds of thousands
        let passes = p.stats.loops.load(Relaxed) - loops;
        assert!(passes < 400, "{passes} loop passes in 500 ms: the worker is spinning");
        drop(p);
    }

    #[test]
    fn seek_near_the_end_while_playing_does_not_restart() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, events) = collect_events();
        let p = Player::spawn(ring.clone(), getter(1), notify);
        let _fake = Fake::start(ring.clone(), 480, 1.0, false);
        let m = {
            let mut m = Mix::default();
            m.tracks.push(track(1, 0.0, 10.0 * 44100.0, 1.0));
            m
        };
        p.send(Command::Load(m, LiveOptions::default()));
        p.send(Command::Play(0));
        assert!(wait_for(|| ring.tail() > 4410, 10.0));
        p.send(Command::Seek(10 * 44100 - 1000));
        assert!(wait_for(|| events.lock().unwrap().iter().any(|e| matches!(e, Event::Position { sec, playing: false } if *sec > 9.9)), 5.0), "did not end near 10 s");
        drop(p);
    }

    #[test]
    fn unload_and_an_empty_mix_do_not_hang() {
        let ring = Arc::new(Ring::new(1 << 12));
        let (notify, _ev) = collect_events();
        let p = Player::spawn(ring.clone(), getter(1), notify);
        p.send(Command::Load(Mix::default(), LiveOptions::default()));
        p.send(Command::Play(0));
        p.send(Command::Unload);
        drop(p);
    }

    /// Realtime run for the measurements in the report: `cargo test --release -p pm-mixer -- --ignored three_minutes --nocapture`.
    #[test]
    #[ignore]
    fn three_minutes_six_clips() {
        let ring = Arc::new(Ring::new(1 << 15));
        let (notify, _ev) = collect_events();
        // 6 clips with tempo changes (WSOLA on every clip), 16 s apart, 20 s long: 116 s of mix; loop it
        // by seeking back so the run lasts the full 3 minutes
        let m = mix_of(6, 1.07);
        let p = Player::spawn(ring.clone(), getter(6), notify);
        let _fake = Fake::start(ring.clone(), 480, 1.0, false);
        p.send(Command::Load(m, LiveOptions::default()));
        let t = Instant::now();
        p.send(Command::Play(0));
        let mut seeks = 0;
        let mut seen = 0;
        while t.elapsed().as_secs() < 180 {
            std::thread::sleep(Duration::from_millis(20));
            // say when an underrun happens, so it can be matched with what the worker was doing
            if ring.underruns() != seen {
                seen = ring.underruns();
                println!("underrun #{seen} at {:.2} s of the run, ring holds {} frames, {seeks} seek(s) so far", t.elapsed().as_secs_f64(), ring.len());
            }
            if ring.tail() as f64 / 44100.0 > 100.0 * (seeks + 1) as f64 {
                println!("seek at {:.2} s of the run", t.elapsed().as_secs_f64());
                p.send(Command::Seek(0));
                seeks += 1;
            }
        }
        let st = &p.stats;
        let steps = st.steps.load(Relaxed).max(1);
        println!(
            "first audio {} us, {} steps, mean {} us, max {} us per {}-frame step ({} us of audio), slow steps {}, underruns {}",
            st.first_audio_us.load(Relaxed),
            steps,
            st.step_total_us.load(Relaxed) / steps,
            st.step_max_us.load(Relaxed),
            CHUNK,
            CHUNK * 1_000_000 / 44100,
            st.slow_steps.load(Relaxed),
            ring.underruns()
        );
        assert_eq!(ring.underruns(), 0);
    }
}
