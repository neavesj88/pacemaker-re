//! One clip as a pull-based stream on its own (clip-local) time axis.
//!
//! Pipeline per block of local samples `[t0, t1)`:
//!
//! 1. **base**: the decoded file, played according to the tempo curve and key curve:
//!    identity (curve is exactly 1, no key shift), "pitch (vinyl)" resampling
//!    (`NonInterpolated`), or WSOLA time-stretch followed, when the key curve is not flat,
//!    by a band-limited resample that moves the pitch while the duration stays on the tempo curve;
//! 2. **edits**: loops (a captured slice repeated), reverse regions, ignore regions (muted);
//! 3. **tools**: EQ, filter, reverb, echo, ... in document order, with reverb and echo tails;
//! 4. **gain** envelope (dB) of the clip's volume tool;
//! 5. the **audible window** `[l_offset, length - r_offset]` with 32-sample edges.
//!
//! The stream is sequential: [`ClipStream::render`] must be called with consecutive blocks.

use std::sync::Arc;

use pm_formats::mix::{LoopProc, Process, TempoMap, Track};

use crate::audio::Source;
use crate::env::Env;
use crate::fx::{self, Fx, TICK};
use crate::sinc;
use crate::stretch::Wsola;

/// Samples generated per `Base::ensure` step.
const CHUNK: i64 = 2048;
/// Fade length at the edges of a tool (dry/wet ramp) in samples.
const TOOL_RAMP: i64 = 128;
/// Fade length at the edges of the audible window, and around ignore regions.
const EDGE: i64 = 32;
const IGNORE_FADE: i64 = 64;
/// Table spacing of the pitch warp.
const WARP_STEP: i64 = 256;

// ---------------------------------------------------------------------------------------------
// pitch warp
// ---------------------------------------------------------------------------------------------

/// Integral of the pitch ratio `r(t) = 2^(cents(t)/1200)` from the stream start: `u(t)` is where
/// the pitch-preserving stretcher output has to be read to get the shifted signal at clip-local
/// time `t`. Piecewise linear, tabulated every [`WARP_STEP`] samples.
struct Warp {
    s0: i64,
    u: Vec<f64>,
    r_first: f64,
    r_last: f64,
}

impl Warp {
    fn new(cents: &Env, s0: i64, t_max: i64) -> Warp {
        let ratio = |t: f64| 2f64.powf(f64::from(cents.at(t, 0.0)) / 1200.0);
        let n = ((t_max - s0).max(0) / WARP_STEP + 3) as usize;
        let mut u = Vec::with_capacity(n + 1);
        u.push(0.0);
        for j in 0..n {
            let mid = (s0 + j as i64 * WARP_STEP) as f64 + WARP_STEP as f64 / 2.0;
            let last = u[j];
            u.push(last + WARP_STEP as f64 * ratio(mid));
        }
        let r_first = ratio(s0 as f64);
        let r_last = ratio((s0 + n as i64 * WARP_STEP) as f64);
        Warp { s0, u, r_first, r_last }
    }

    fn u_at(&self, t: f64) -> f64 {
        let x = (t - self.s0 as f64) / WARP_STEP as f64;
        if x <= 0.0 {
            return (t - self.s0 as f64) * self.r_first;
        }
        let j = x as usize;
        if j + 1 >= self.u.len() {
            let last_t = self.s0 as f64 + (self.u.len() - 1) as f64 * WARP_STEP as f64;
            return self.u[self.u.len() - 1] + (t - last_t) * self.r_last;
        }
        self.u[j] + (self.u[j + 1] - self.u[j]) * (x - j as f64)
    }

    fn inv(&self, u: f64) -> f64 {
        if u <= 0.0 {
            return self.s0 as f64 + u / self.r_first;
        }
        let j = self.u.partition_point(|&v| v <= u);
        if j >= self.u.len() {
            let last_t = self.s0 as f64 + (self.u.len() - 1) as f64 * WARP_STEP as f64;
            return last_t + (u - self.u[self.u.len() - 1]) / self.r_last;
        }
        let j = j - 1;
        let f = (u - self.u[j]) / (self.u[j + 1] - self.u[j]);
        self.s0 as f64 + (j as f64 + f) * WARP_STEP as f64
    }
}

// ---------------------------------------------------------------------------------------------
// base signal
// ---------------------------------------------------------------------------------------------

enum Kind {
    Identity,
    Vinyl { tempo: TempoMap },
    Stretch { w: Box<Wsola>, warp: Option<Arc<Warp>>, s0: i64 },
}

/// The clip's audio after tempo and key, generated in chunks and buffered from `origin`.
struct Base {
    src: Arc<Source>,
    kind: Kind,
    origin: i64,
    produced: i64,
    l: Vec<f32>,
    r: Vec<f32>,
}

impl Base {
    fn ensure(&mut self, upto: i64) {
        while self.produced < upto {
            self.chunk();
        }
    }

    fn chunk(&mut self) {
        let t = self.produced;
        let t_end = t + CHUNK;
        let n = CHUNK as usize;
        let old = self.l.len();
        self.l.resize(old + n, 0.0);
        self.r.resize(old + n, 0.0);
        let (ol, or) = (&mut self.l[old..], &mut self.r[old..]);
        match &mut self.kind {
            Kind::Identity => {
                let len = self.src.len() as i64;
                for i in 0..n {
                    let q = t + i as i64;
                    if q >= 0 && q < len {
                        ol[i] = self.src.l[q as usize];
                        or[i] = self.src.r[q as usize];
                    }
                }
            }
            Kind::Vinyl { tempo } => {
                for i in 0..n {
                    let tt = (t + i as i64) as f64;
                    let pos = tempo.source_at(tt);
                    let cut = (1.0 / tempo.speed_at(tt)).min(1.0);
                    ol[i] = sinc::interp(&self.src.l, pos, cut);
                    or[i] = sinc::interp(&self.src.r, pos, cut);
                }
            }
            Kind::Stretch { w, warp: None, s0 } => {
                let u0 = t - *s0;
                w.ensure(u0 + CHUNK);
                let (xl, xr) = w.slice(u0, u0 + CHUNK);
                ol.copy_from_slice(xl);
                or.copy_from_slice(xr);
                w.trim(u0 - 64);
            }
            Kind::Stretch { w, warp: Some(warp), .. } => {
                let u_end = warp.u_at(t_end as f64);
                w.ensure(u_end.ceil() as i64 + 48);
                let (base, xl, xr) = w.final_region();
                let mut p = warp.u_at(t as f64);
                for i in 0..n {
                    let nxt = warp.u_at((t + i as i64 + 1) as f64);
                    let cut = (1.0 / (nxt - p)).min(1.0);
                    ol[i] = sinc::interp(xl, p - base as f64, cut);
                    or[i] = sinc::interp(xr, p - base as f64, cut);
                    p = nxt;
                }
                w.trim(p.floor() as i64 - 64);
            }
        }
        self.produced = t_end;
    }

    fn slice(&self, from: i64, to: i64) -> (&[f32], &[f32]) {
        let a = (from - self.origin) as usize;
        let b = (to - self.origin) as usize;
        (&self.l[a..b], &self.r[a..b])
    }

    fn trim(&mut self, before: i64) {
        let cut = (before.min(self.produced) - self.origin).max(0) as usize;
        if cut > 32768 {
            self.l.drain(..cut);
            self.r.drain(..cut);
            self.origin += cut as i64;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// edits and tools
// ---------------------------------------------------------------------------------------------

struct LoopRt {
    a: i64,
    end: i64,
    /// Index of the root loop (whose slice is repeated).
    root: usize,
}

struct RootSlice {
    start: i64,
    len: i64,
    data: Option<(Vec<f32>, Vec<f32>)>,
}

struct ReverseRt {
    a: i64,
    b: i64,
    data: Option<(Vec<f32>, Vec<f32>)>,
}

struct ToolRt {
    a: i64,
    b: i64,
    tool: pm_formats::mix::Tool,
    fxs: Option<Vec<Box<dyn Fx>>>,
    tail_end: i64,
    done: bool,
}

pub struct ClipStream {
    base: Base,
    cursor: i64,
    aud_start: i64,
    aud_end: i64,
    loops: Vec<LoopRt>,
    roots: Vec<RootSlice>,
    reverses: Vec<ReverseRt>,
    ignores: Vec<(i64, i64)>,
    tools: Vec<ToolRt>,
    gain: Env,
    track_bpm: f64,
    tempo: TempoMap,
    scratch: Scratch,
}

#[derive(Default)]
struct Scratch {
    dl: Vec<f32>,
    dr: Vec<f32>,
    wl: Vec<f32>,
    wr: Vec<f32>,
}

fn round(x: f64) -> i64 {
    x.round() as i64
}

impl ClipStream {
    /// Builds the stream of `track` over decoded `src`; the first sample it will be asked for is
    /// `start_local` or earlier (it moves back to the start of any loop or reverse region that
    /// is still running there). `track` must outlive nothing: tools are cloned.
    pub fn new(track: &Track, src: Arc<Source>, start_local: i64) -> ClipStream {
        let aud_start = round(track.position.l_offset);
        let aud_end = round(track.position.length - track.position.r_offset);
        let tempo = TempoMap::new(&track.pitch);

        // loops
        let mut loops = Vec::new();
        let mut roots: Vec<RootSlice> = Vec::new();
        let mut root_of: Vec<Option<usize>> = vec![None; track.processes.len()];
        for (i, p) in track.processes.iter().enumerate() {
            let Process::Loop(lp) = p else { continue };
            // follow the re-loop chain to its first loop
            let mut ri = i;
            for _ in 0..64 {
                match &track.processes[ri] {
                    Process::Loop(LoopProc { parent: Some(pi), .. }) if *pi < track.processes.len() && matches!(track.processes[*pi], Process::Loop(_)) => ri = *pi,
                    _ => break,
                }
            }
            let Process::Loop(root) = &track.processes[ri] else { continue };
            let len = round(root.length);
            if lp.bypassed || len < 1 {
                continue;
            }
            let ridx = *root_of[ri].get_or_insert_with(|| {
                roots.push(RootSlice { start: round(root.position.start), len, data: None });
                roots.len() - 1
            });
            let a = round(lp.position.start);
            let by_iter = round(f64::from(lp.iterations) * lp.length);
            let by_pos = round(lp.position.length);
            let span = match (by_iter > 0, by_pos > 0) {
                (true, true) => by_iter.min(by_pos),
                (true, false) => by_iter,
                _ => by_pos,
            };
            if span > 0 {
                loops.push(LoopRt { a, end: a + span, root: ridx });
            }
        }
        let reverses: Vec<ReverseRt> = track
            .processes
            .iter()
            .filter_map(|p| match p {
                Process::Reverse(r) if !r.bypassed && r.position.length >= 2.0 => {
                    Some(ReverseRt { a: round(r.position.start), b: round(r.position.start + r.position.length), data: None })
                }
                _ => None,
            })
            .collect();
        let ignores: Vec<(i64, i64)> = track
            .processes
            .iter()
            .filter_map(|p| match p {
                Process::Ignore(g) if !g.bypassed && g.position.length >= 1.0 => Some((round(g.position.start), round(g.position.start + g.position.length))),
                _ => None,
            })
            .collect();

        // the stream has to start early enough to see the source of regions still running
        let mut s0 = start_local.max(0);
        for l in &loops {
            if l.end > start_local {
                s0 = s0.min(roots[l.root].start.max(0));
            }
        }
        for r in &reverses {
            if r.b > start_local {
                s0 = s0.min(r.a.max(0));
            }
        }

        // tools (cloned so the stream owns them)
        let tools: Vec<ToolRt> = track
            .processes
            .iter()
            .filter_map(|p| match p {
                Process::Tool(t) if !t.bypassed && t.position.length >= 1.0 => {
                    let a = round(t.position.start);
                    let b = round(t.position.start + t.position.length);
                    Some(ToolRt { a, b, tool: t.clone(), fxs: None, tail_end: b, done: b <= start_local })
                }
                _ => None,
            })
            .collect();

        let gain = if track.volume.bypassed {
            Env::default()
        } else {
            track.volume.automation.get("Volume-Volume").map(|e| Env::new(&e.nodes, track.volume.position.start)).unwrap_or_default()
        };

        // base signal
        let any_shift = track.key.iter().any(|k| k.cents.is_finite() && k.cents.abs() > 0.5);
        let unit_speed = track.pitch.iter().all(|n| (n.speed - 1.0).abs() < 1e-6);
        let t_max = aud_end.max(s0) + 8 * CHUNK;
        let kind = if track.non_interpolated {
            Kind::Vinyl { tempo: tempo.clone() }
        } else if unit_speed && !any_shift {
            Kind::Identity
        } else {
            let warp = if any_shift {
                let cents = Env::from_points(&track.key.iter().map(|k| (f64::from(k.position), k.cents)).collect::<Vec<_>>());
                Some(Arc::new(Warp::new(&cents, s0, t_max)))
            } else {
                None
            };
            let tm = tempo.clone();
            let wp = warp.clone();
            let map: crate::stretch::TimeMap = match wp {
                Some(w) => Box::new(move |u| tm.source_at(w.inv(u))),
                None => Box::new(move |u| tm.source_at(s0 as f64 + u)),
            };
            Kind::Stretch { w: Box::new(Wsola::new(src.clone(), map)), warp, s0 }
        };

        ClipStream {
            base: Base { src, kind, origin: s0, produced: s0, l: Vec::new(), r: Vec::new() },
            cursor: s0,
            aud_start,
            aud_end,
            loops,
            roots,
            reverses,
            ignores,
            tools,
            gain,
            track_bpm: track.bpm,
            tempo,
            scratch: Scratch::default(),
        }
    }

    /// First local sample the stream can produce.
    pub fn start(&self) -> i64 {
        self.base.origin.min(self.cursor)
    }

    /// Renders local samples `[t0, t0 + l.len())` into `l` / `r` (overwritten). Calls must be
    /// consecutive; a jump forward skips the audio in between.
    pub fn render(&mut self, t0: i64, l: &mut [f32], r: &mut [f32]) {
        let n = l.len() as i64;
        let t1 = t0 + n;
        if t0 > self.cursor {
            self.cursor = t0;
        }
        debug_assert!(t0 >= self.base.origin, "stream asked for audio before its start");
        let t0c = t0.max(self.base.origin);
        let skip = (t0c - t0) as usize;
        l[..skip].fill(0.0);
        r[..skip].fill(0.0);
        if t1 <= self.base.origin {
            return;
        }

        // 1. base
        self.base.ensure(t1);
        // loop slices and reverse regions need their source before the block is edited
        self.capture(t1);
        {
            let (bl, br) = self.base.slice(t0c, t1);
            l[skip..].copy_from_slice(bl);
            r[skip..].copy_from_slice(br);
        }
        // 2. edits
        self.apply_edits(t0, l, r);
        // 3. tools
        self.apply_tools(t0, l, r);
        // 4. gain
        self.apply_gain(t0, l, r);
        // 5. audible window
        self.apply_window(t0, l, r);

        self.cursor = t1;
        self.base.trim(t1 - 1);
    }

    fn capture(&mut self, t1: i64) {
        for s in &mut self.roots {
            if s.data.is_none() && t1 > s.start && s.start >= self.base.origin {
                self.base.ensure(s.start + s.len);
                let (a, b) = self.base.slice(s.start, s.start + s.len);
                s.data = Some((a.to_vec(), b.to_vec()));
            }
        }
        for rv in &mut self.reverses {
            if rv.data.is_none() && t1 > rv.a && rv.a >= self.base.origin {
                self.base.ensure(rv.b);
                let (a, b) = self.base.slice(rv.a, rv.b);
                rv.data = Some((a.to_vec(), b.to_vec()));
            }
        }
    }

    fn apply_edits(&mut self, t0: i64, l: &mut [f32], r: &mut [f32]) {
        let t1 = t0 + l.len() as i64;
        for lp in &self.loops {
            let Some((sl, sr)) = &self.roots[lp.root].data else { continue };
            let slen = self.roots[lp.root].len;
            let root_start = self.roots[lp.root].start;
            for t in lp.a.max(t0)..lp.end.min(t1) {
                let k = (t - root_start).rem_euclid(slen) as usize;
                l[(t - t0) as usize] = sl[k];
                r[(t - t0) as usize] = sr[k];
            }
        }
        for rv in &self.reverses {
            let Some((sl, sr)) = &rv.data else { continue };
            for t in rv.a.max(t0)..rv.b.min(t1) {
                // the data starts at rv.a: source sample a + b - 1 - t sits at index b - 1 - t
                let k = (rv.b - 1 - t) as usize;
                l[(t - t0) as usize] = sl[k];
                r[(t - t0) as usize] = sr[k];
            }
        }
        for &(a, b) in &self.ignores {
            for t in a.max(t0)..b.min(t1) {
                let env = (((t - a) as f32) / IGNORE_FADE as f32).min(((b - t) as f32) / IGNORE_FADE as f32).clamp(0.0, 1.0);
                let m = 1.0 - env;
                l[(t - t0) as usize] *= m;
                r[(t - t0) as usize] *= m;
            }
        }
    }

    fn bpm_at(&self, local: i64) -> f64 {
        self.track_bpm * self.tempo.speed_at(local as f64)
    }

    fn apply_tools(&mut self, t0: i64, l: &mut [f32], r: &mut [f32]) {
        let t1 = t0 + l.len() as i64;
        let aud_end = self.aud_end;
        for ti in 0..self.tools.len() {
            if self.tools[ti].done || self.tools[ti].a >= t1 {
                continue;
            }
            let (a, b) = (self.tools[ti].a, self.tools[ti].b);
            if self.tools[ti].fxs.is_none() {
                let fallback = self.bpm_at(a);
                let tool = &self.tools[ti].tool;
                let fxs: Vec<Box<dyn Fx>> = tool.effects.iter().filter_map(|e| fx::build(e, tool, fallback)).collect();
                let tail = fxs.iter().map(|f| f.tail()).max().unwrap_or(0) as i64;
                self.tools[ti].tail_end = (b + tail).min(aud_end.max(b));
                self.tools[ti].fxs = Some(fxs);
            }
            let mut fxs = self.tools[ti].fxs.take().unwrap();
            let tail_end = self.tools[ti].tail_end;
            // active part
            let mut t = a.max(t0);
            let active_end = b.min(t1);
            while t < active_end {
                let m = ((active_end - t).min(TICK as i64)) as usize;
                let o = (t - t0) as usize;
                self.process_tick(&mut fxs, a, b, t, &mut l[o..o + m], &mut r[o..o + m]);
                t += m as i64;
            }
            // tail part: silence in, wet out, added to the dry signal
            let mut t = b.max(t0);
            let tail_to = tail_end.min(t1);
            while t < tail_to && tail_end > b {
                let m = ((tail_to - t).min(TICK as i64)) as usize;
                let o = (t - t0) as usize;
                let bpm = self.bpm_at(t);
                let sc = &mut self.scratch;
                sc.wl.clear();
                sc.wl.resize(m, 0.0);
                sc.wr.clear();
                sc.wr.resize(m, 0.0);
                for f in fxs.iter_mut() {
                    f.process(&mut sc.wl, &mut sc.wr, (t - a) as f64, bpm);
                }
                // fade the last 2048 samples of the tail
                for k in 0..m {
                    let g = (((tail_end - (t + k as i64)) as f32) / 2048.0).clamp(0.0, 1.0);
                    l[o + k] += sc.wl[k] * g;
                    r[o + k] += sc.wr[k] * g;
                }
                t += m as i64;
            }
            if t1 >= tail_end.max(b) {
                self.tools[ti].done = true;
            } else {
                self.tools[ti].fxs = Some(fxs);
            }
        }
    }

    fn process_tick(&mut self, fxs: &mut [Box<dyn Fx>], a: i64, b: i64, t: i64, l: &mut [f32], r: &mut [f32]) {
        let m = l.len();
        let bpm = self.bpm_at(t);
        let sc = &mut self.scratch;
        sc.dl.clear();
        sc.dl.extend_from_slice(l);
        sc.dr.clear();
        sc.dr.extend_from_slice(r);
        for f in fxs.iter_mut() {
            f.process(l, r, (t - a) as f64, bpm);
        }
        // dry/wet ramp at the tool edges: out = dry + ramp * (fx - dry)
        for k in 0..m {
            let tt = t + k as i64;
            let ramp = (((tt - a + 1) as f32) / TOOL_RAMP as f32).min(((b - tt) as f32) / TOOL_RAMP as f32).clamp(0.0, 1.0);
            l[k] = sc.dl[k] + ramp * (l[k] - sc.dl[k]);
            r[k] = sc.dr[k] + ramp * (r[k] - sc.dr[k]);
        }
    }

    fn apply_gain(&self, t0: i64, l: &mut [f32], r: &mut [f32]) {
        if self.gain.is_empty() {
            return;
        }
        let n = l.len();
        let db = |t: f64| 10f32.powf(self.gain.at(t, 0.0).clamp(-96.0, 24.0) / 20.0);
        let mut k = 0;
        while k < n {
            let m = (n - k).min(TICK);
            let g0 = db((t0 + k as i64) as f64);
            let g1 = db((t0 + (k + m) as i64) as f64);
            for j in 0..m {
                let g = g0 + (g1 - g0) * (j as f32 / m as f32);
                l[k + j] *= g;
                r[k + j] *= g;
            }
            k += m;
        }
    }

    fn apply_window(&self, t0: i64, l: &mut [f32], r: &mut [f32]) {
        for i in 0..l.len() {
            let t = t0 + i as i64;
            if t < self.aud_start || t >= self.aud_end {
                l[i] = 0.0;
                r[i] = 0.0;
                continue;
            }
            let e = (t - self.aud_start).min(self.aud_end - 1 - t);
            if e < EDGE {
                let g = (e + 1) as f32 / (EDGE + 1) as f32;
                l[i] *= g;
                r[i] *= g;
            }
        }
    }
}
