//! A lock-free single-producer / single-consumer ring of stereo frames between the live engine's
//! worker thread and the audio device's callback.
//!
//! The callback ([`Ring::fill`]) only copies frames out, ramps its gain for pause / resume and
//! counts underruns: no DSP, no allocation, no locks. The worker writes ahead ([`Ring::push`]) and,
//! for an edit made while playing, may move the write position back to a point the callback has
//! not reached yet ([`Ring::rewind_to`]) and overwrite what it had queued there. The samples are
//! atomics (relaxed) so even a callback that reads a frame being overwritten is well defined; the
//! caller keeps the rewind point far enough ahead of the read position that it never happens
//! (see [`crate::player`]).
//!
//! Positions are absolute frame counters that only ever grow, except for a rewind.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::*};

/// Frames over which the callback fades in or out when playback starts or stops (5.8 ms).
pub const FADE_FRAMES: usize = 256;

pub struct Ring {
    buf: Box<[AtomicU32]>,
    cap: u64,
    head: AtomicU64,
    tail: AtomicU64,
    /// The callback should be playing (fade in) rather than silent (fade out).
    run: AtomicBool,
    /// The callback has faded out and is not consuming; set by the callback.
    stopped: AtomicBool,
    underruns: AtomicU64,
}

/// The callback's own state (kept by the device thread, not shared).
#[derive(Default)]
pub struct Consumer {
    gain: f32,
}

impl Ring {
    /// A ring of `frames` frames (rounded up to a power of two).
    pub fn new(frames: usize) -> Ring {
        let cap = frames.max(16).next_power_of_two() as u64;
        Ring {
            buf: (0..cap * 2).map(|_| AtomicU32::new(0)).collect(),
            cap,
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            run: AtomicBool::new(false),
            stopped: AtomicBool::new(true),
            underruns: AtomicU64::new(0),
        }
    }

    pub fn capacity(&self) -> usize {
        self.cap as usize
    }

    // ---- shared reads ----

    /// Absolute index of the next frame the producer writes.
    pub fn head(&self) -> u64 {
        self.head.load(Acquire)
    }

    /// Absolute index of the next frame the callback reads.
    pub fn tail(&self) -> u64 {
        self.tail.load(Acquire)
    }

    /// Frames queued (written, not yet read).
    pub fn len(&self) -> usize {
        self.head().saturating_sub(self.tail()) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn free(&self) -> usize {
        self.cap as usize - self.len().min(self.cap as usize)
    }

    /// Times the callback found the ring short while it was meant to be playing.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Relaxed)
    }

    /// Whether the callback has faded out and is idle.
    pub fn stopped(&self) -> bool {
        self.stopped.load(Acquire)
    }

    /// Asks the callback to play (fade in) or stop (fade out).
    pub fn set_run(&self, run: bool) {
        self.run.store(run, Release);
    }

    // ---- producer ----

    fn put(&self, idx: u64, l: f32, r: f32) {
        let i = ((idx & (self.cap - 1)) * 2) as usize;
        self.buf[i].store(l.to_bits(), Relaxed);
        self.buf[i + 1].store(r.to_bits(), Relaxed);
    }

    fn get(&self, idx: u64) -> (f32, f32) {
        let i = ((idx & (self.cap - 1)) * 2) as usize;
        (f32::from_bits(self.buf[i].load(Relaxed)), f32::from_bits(self.buf[i + 1].load(Relaxed)))
    }

    /// Appends as many of the frames as fit; returns how many.
    pub fn push(&self, l: &[f32], r: &[f32]) -> usize {
        let n = l.len().min(r.len()).min(self.free());
        let head = self.head.load(Relaxed);
        for k in 0..n {
            self.put(head + k as u64, l[k], r[k]);
        }
        self.head.store(head + n as u64, Release);
        n
    }

    /// Reads back frames the producer wrote, starting at absolute index `idx` (for crossfading an
    /// edit into what is already queued). Frames at or beyond the write position read as zero.
    pub fn read_at(&self, idx: u64, l: &mut [f32], r: &mut [f32]) {
        let head = self.head.load(Relaxed);
        for k in 0..l.len().min(r.len()) {
            let i = idx + k as u64;
            let (a, b) = if i < head { self.get(i) } else { (0.0, 0.0) };
            l[k] = a;
            r[k] = b;
        }
    }

    /// Overwrites frames from absolute index `idx` and makes `idx + n` the write position. `idx`
    /// must not be behind the read position.
    pub fn rewrite_from(&self, idx: u64, l: &[f32], r: &[f32]) {
        let n = l.len().min(r.len());
        for k in 0..n {
            self.put(idx + k as u64, l[k], r[k]);
        }
        self.head.store(idx + n as u64, Release);
    }

    /// Drops everything queued. Only when the callback is stopped ([`Ring::stopped`]).
    pub fn flush(&self) {
        self.head.store(self.tail.load(Acquire), Release);
    }

    // ---- consumer (the device callback) ----

    /// Fills `out` (interleaved stereo) with queued frames, or silence. Never blocks.
    pub fn fill(&self, out: &mut [f32], st: &mut Consumer) {
        let frames = out.len() / 2;
        let run = self.run.load(Acquire);
        if !run && st.gain <= 0.0 {
            out.fill(0.0);
            self.stopped.store(true, Release);
            return;
        }
        self.stopped.store(false, Release);
        let head = self.head.load(Acquire);
        let tail = self.tail.load(Relaxed);
        let avail = head.saturating_sub(tail) as usize;
        let n = avail.min(frames);
        let (target, step) = if run { (1.0f32, 1.0 / FADE_FRAMES as f32) } else { (0.0f32, -1.0 / FADE_FRAMES as f32) };
        let mut g = st.gain;
        for k in 0..n {
            g = if run { (g + step).min(target) } else { (g + step).max(target) };
            let (l, r) = self.get(tail + k as u64);
            out[2 * k] = l * g;
            out[2 * k + 1] = r * g;
        }
        st.gain = g;
        self.tail.store(tail + n as u64, Release);
        if n < frames {
            out[2 * n..].fill(0.0);
            if run {
                self.underruns.fetch_add(1, Relaxed);
            } else {
                st.gain = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize, from: f32) -> (Vec<f32>, Vec<f32>) {
        ((0..n).map(|i| from + i as f32).collect(), (0..n).map(|i| -(from + i as f32)).collect())
    }

    #[test]
    fn frames_come_out_in_order_across_the_wrap() {
        let ring = Ring::new(512);
        ring.set_run(true);
        let mut st = Consumer::default();
        let mut next = 0.0f32;
        let mut got = Vec::new();
        // run the fade-in out first so the gain is 1 for what is compared
        let (l, r) = ramp(300, next);
        assert_eq!(ring.push(&l, &r), 300);
        next += 300.0;
        let mut out = vec![0.0; 2 * 300];
        ring.fill(&mut out, &mut st);
        for _ in 0..20 {
            let (l, r) = ramp(37, next);
            assert_eq!(ring.push(&l, &r), 37);
            next += 37.0;
            let mut out = vec![0.0; 2 * 37];
            ring.fill(&mut out, &mut st);
            got.extend(out.chunks(2).map(|c| (c[0], c[1])));
        }
        for (k, (l, r)) in got.iter().enumerate() {
            let want = 300.0 + k as f32;
            assert_eq!((*l, *r), (want, -want), "frame {k}");
        }
        assert_eq!(ring.underruns(), 0);
    }

    #[test]
    fn push_stops_when_full() {
        let ring = Ring::new(32);
        let (l, r) = ramp(100, 0.0);
        assert_eq!(ring.push(&l, &r), 32);
        assert_eq!(ring.free(), 0);
        assert_eq!(ring.push(&l, &r), 0);
    }

    #[test]
    fn underrun_is_silence_and_counted() {
        let ring = Ring::new(64);
        ring.set_run(true);
        let mut st = Consumer::default();
        let (l, r) = ramp(10, 1.0);
        ring.push(&l, &r);
        let mut out = vec![9.0; 2 * 30];
        ring.fill(&mut out, &mut st);
        assert!(out[2 * 10..].iter().all(|&x| x == 0.0));
        assert_eq!(ring.underruns(), 1);
        // not counted while stopped
        ring.set_run(false);
        let mut out = vec![9.0; 2 * 400];
        ring.fill(&mut out, &mut st);
        ring.fill(&mut out, &mut st);
        assert_eq!(ring.underruns(), 1);
        assert!(ring.stopped());
    }

    #[test]
    fn stop_fades_out_and_reports_stopped() {
        let ring = Ring::new(4096);
        ring.set_run(true);
        let mut st = Consumer::default();
        let ones = vec![1.0f32; 2000];
        ring.push(&ones, &ones);
        let mut out = vec![0.0; 2 * 1000];
        ring.fill(&mut out, &mut st);
        assert!((out[2 * 999] - 1.0).abs() < 1e-6);
        ring.set_run(false);
        let mut out = vec![0.0; 2 * 1000];
        ring.fill(&mut out, &mut st);
        // fades to zero over FADE_FRAMES, monotonically
        assert!(out[0] < 1.0 && out[2 * (FADE_FRAMES + 2)] == 0.0);
        assert!(out.chunks(2).map(|c| c[0]).collect::<Vec<_>>().windows(2).all(|w| w[1] <= w[0] + 1e-6));
        ring.fill(&mut out, &mut st);
        assert!(ring.stopped());
        ring.flush();
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn rewind_replaces_queued_audio_ahead_of_the_reader() {
        let ring = Ring::new(1024);
        ring.set_run(true);
        let mut st = Consumer::default();
        let (l, r) = ramp(600, 1000.0);
        ring.push(&l, &r);
        let mut out = vec![0.0; 2 * 300];
        ring.fill(&mut out, &mut st); // past the fade-in
        let tail = ring.tail();
        assert_eq!(tail, 300);
        // the producer replaces everything from tail + 60 on, after reading back 20 old frames
        let at = tail + 60;
        let (mut ol, mut or) = (vec![0.0; 20], vec![0.0; 20]);
        ring.read_at(at, &mut ol, &mut or);
        assert_eq!(ol[0], 1360.0);
        let nl = vec![-5.0f32; 20];
        ring.rewrite_from(at, &nl, &nl);
        assert_eq!(ring.head(), at + 20);
        let mut out = vec![0.0; 2 * 80];
        ring.fill(&mut out, &mut st);
        assert_eq!(out[2 * 59], 1359.0, "old audio up to the rewind point");
        assert_eq!(out[2 * 60], -5.0, "new audio from it");
        assert_eq!(out[2 * 79], -5.0);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn read_past_the_head_is_zero() {
        let ring = Ring::new(64);
        let (l, r) = ramp(10, 1.0);
        ring.push(&l, &r);
        let (mut a, mut b) = (vec![7.0; 6], vec![7.0; 6]);
        ring.read_at(7, &mut a, &mut b);
        assert_eq!(a, vec![8.0, 9.0, 10.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn one_producer_one_consumer_thread_lose_nothing() {
        use std::sync::Arc;
        let ring = Arc::new(Ring::new(1024));
        ring.set_run(true);
        let total = 400_000usize;
        let r2 = ring.clone();
        let cons = std::thread::spawn(move || {
            let mut st = Consumer::default();
            let mut got = 0usize;
            let mut sum = 0.0f64;
            let mut out = vec![0.0f32; 2 * 333];
            while got < total {
                let before = r2.tail();
                r2.fill(&mut out, &mut st);
                let n = (r2.tail() - before) as usize;
                for c in out[..2 * n].chunks(2) {
                    sum += f64::from(c[0]);
                }
                got += n;
                if n == 0 {
                    std::thread::yield_now();
                }
            }
            sum
        });
        let mut sent = 0usize;
        let mut want = 0.0f64;
        while sent < total {
            let n = (total - sent).min(500);
            let l: Vec<f32> = (0..n).map(|i| ((sent + i) % 7) as f32).collect();
            let m = ring.push(&l, &l);
            want += l[..m].iter().map(|&x| f64::from(x)).sum::<f64>();
            sent += m;
            if m == 0 {
                std::thread::yield_now();
            }
        }
        let sum = cons.join().unwrap();
        // the first FADE_FRAMES frames are scaled by the fade-in, so compare after discounting it
        assert!(sum <= want && sum > want * 0.999, "{sum} vs {want}");
    }
}
