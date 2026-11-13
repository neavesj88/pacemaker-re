//! The tool effects. Tonium's own DSP is unknown, so every effect here is a documented,
//! musical stand-in driven by the parameter keys of the mix model (see notes/mix-audio.md for
//! the parameter mappings and the reasoning).
//!
//! All effects work on a stereo block at control rate: the caller cuts the audio into ticks of at
//! most [`TICK`] samples and passes the tick's position inside the tool plus the clip's tempo;
//! the effect reads its envelopes at the tick centre. State (filters, delay lines, gates) lives
//! in the effect, which the clip stream creates when the tool starts, so a run is deterministic.
//!
//! `Wet/Dry` is a percentage (0..100). The mix law of the beat effects is `(1 - w) * dry + w *
//! wet`; the wet path is level-matched to the dry one. Fed with silence, an effect therefore
//! outputs `w * tail`, which is how the clip stream adds reverb and echo tails after a tool ends.

use pm_formats::mix::{Effect, EffectKind, Tool};

use crate::env::Env;
use crate::vocal::VocalFx;
use crate::RATE;

const SR: f64 = RATE as f64;
/// Control-rate tick (samples).
pub const TICK: usize = 32;

pub trait Fx: Send {
    /// Processes one tick in place. `pos` is the sample position of `l[0]` inside the tool,
    /// `bpm` the clip's effective tempo there.
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, bpm: f64);
    /// How long the effect rings on after its input stops, in samples.
    fn tail(&self) -> usize {
        0
    }
}

/// What the `Wah` tool (`EffectKind::Peq`) is rendered as. The Pacemaker's own Wah is replaced by
/// a vocal filter on a device with Revival firmware and the vocal flag set on its card; a render
/// has to use the mode of the library it is for, so a preview and an export of the same mix
/// sound like that device. The default, [`PeqMode::Wah`], is the band-pass stand-in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PeqMode {
    /// Stock Wah (a band-pass sweep).
    #[default]
    Wah,
    /// Vocal filter: positive = keep the vocals (centre band), negative = cut them.
    Vocal,
    /// Vocal filter with the two directions swapped (the flag file starts with `i`).
    VocalInverted,
}

/// Builds the effect for one entry of a tool; `None` for a bypassed or unknown one.
pub fn build(effect: &Effect, tool: &Tool, start_bpm_fallback: f64, peq: PeqMode) -> Option<Box<dyn Fx>> {
    if effect.bypassed {
        return None;
    }
    let key = |suffix: &str| -> Env {
        tool.automation
            .get(&format!("{}-{suffix}", effect.name))
            .map(|e| Env::new(&e.nodes, 0.0))
            .unwrap_or_default()
    };
    Some(match &effect.kind {
        EffectKind::Fx { .. } => Box::new(Reverb::new(key("Time"), key("Wet/Dry"))),
        EffectKind::Filter => Box::new(Filter::new(key("CutOff"))),
        EffectKind::Crush => Box::new(Crush::new(key("Crush"))),
        EffectKind::Delay { delay_type, sync_against_beat, .. } => {
            let time = TimeParam { ms: key("Time"), div: key("Time "), sync: *sync_against_beat };
            if *delay_type == 2 {
                Box::new(Roll::new(time, key("Wet/Dry")))
            } else {
                Box::new(Echo::new(time, key("Wet/Dry")))
            }
        }
        EffectKind::Eq => Box::new(Eq::new(key("Lo"), key("Mid"), key("Hi"))),
        EffectKind::Peq => match peq {
            PeqMode::Wah => Box::new(Wah::new(key("CutOff"))),
            PeqMode::Vocal => Box::new(VocalFx::new(key("CutOff"), false)),
            PeqMode::VocalInverted => Box::new(VocalFx::new(key("CutOff"), true)),
        },
        EffectKind::Volume => Box::new(VolumeFx::new(key("Volume"))),
        EffectKind::Trans { start_bpm, .. } => Box::new(Trans::new(key("Wet/Dry"), f64::from(*start_bpm), start_bpm_fallback)),
    })
}

// ---------------------------------------------------------------------------------------------
// building blocks
// ---------------------------------------------------------------------------------------------

/// Zero-delay-feedback state-variable filter (Zavalishin), stable under fast modulation.
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f64,
    ic2: f64,
    a1: f64,
    a2: f64,
    a3: f64,
    k: f64,
}

impl Svf {
    fn new(fc: f64, q: f64) -> Svf {
        let mut s = Svf::default();
        s.set(fc, q);
        s
    }

    fn set(&mut self, fc: f64, q: f64) {
        let fc = fc.clamp(10.0, 0.45 * SR);
        let g = (std::f64::consts::PI * fc / SR).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Returns (low-pass, band-pass, high-pass).
    #[inline]
    fn tick(&mut self, x: f64) -> (f64, f64, f64) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - self.k * v1 - v2)
    }

    fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
}

const BUTTER_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// One-pole smoother toward a target.
#[inline]
fn glide(cur: &mut f32, target: f32, coef: f32) -> f32 {
    *cur += (target - *cur) * coef;
    if (*cur - target).abs() < 1e-6 {
        *cur = target;
    }
    *cur
}

/// Beat period in samples at `bpm` (falls back to 120 BPM for a non-positive tempo).
fn beat_samples(bpm: f64) -> f64 {
    let bpm = if bpm.is_finite() && bpm > 1.0 { bpm } else { 120.0 };
    SR * 60.0 / bpm
}

// ---------------------------------------------------------------------------------------------
// EQ
// ---------------------------------------------------------------------------------------------
/// Three-band isolator-style EQ with kill. Linkwitz-Riley (4th-order, two cascaded Butterworth
/// sections) crossovers at 250 Hz and 2.5 kHz split the signal into low, mid and high:
/// `low = LP250(x)`, `rest = HP250(x)`, `mid = LP2500(rest)`, `high = HP2500(rest)`. The low band
/// also passes an all-pass at 2.5 kHz (`LP2500 + HP2500`), which restores the phase relation the
/// other bands have, so with every gain at 1 the sum is flat in magnitude (a pure all-pass).
/// Knob value `v` (Lo/Mid/Hi): `v <= 0` cuts by `(1 + v/100)^2` (-100 = kill, -50 = -12 dB),
/// `v > 0` boosts up to +6 dB at +100; gains glide (about 2 ms).
pub struct Eq {
    lo: Env,
    mid: Env,
    hi: Env,
    /// Per channel, 12 sections: low LP, rest HP, mid LP, high HP, low all-pass LP, all-pass HP
    /// (two cascaded sections each).
    s: [[Svf; 12]; 2],
    g: [f32; 3],
}

fn eq_gain(v: f32) -> f32 {
    if v <= 0.0 {
        let a = (1.0 + v / 100.0).clamp(0.0, 1.0);
        a * a
    } else {
        10f32.powf(v.min(100.0) / 100.0 * 6.0 / 20.0)
    }
}

impl Eq {
    fn new(lo: Env, mid: Env, hi: Env) -> Eq {
        let f1 = Svf::new(250.0, BUTTER_Q);
        let f2 = Svf::new(2500.0, BUTTER_Q);
        let ch = [f1, f1, f1, f1, f2, f2, f2, f2, f2, f2, f2, f2];
        Eq { lo, mid, hi, s: [ch, ch], g: [1.0; 3] }
    }
}

impl Fx for Eq {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let c = pos + l.len() as f64 / 2.0;
        let target = [eq_gain(self.lo.at(c, 0.0)), eq_gain(self.mid.at(c, 0.0)), eq_gain(self.hi.at(c, 0.0))];
        for i in 0..l.len() {
            for (ch, buf) in [&mut *l, &mut *r].into_iter().enumerate() {
                let x = f64::from(buf[i]);
                let s = &mut self.s[ch];
                let a = s[0].tick(x).0;
                let low = s[1].tick(a).0;
                let b = s[2].tick(x).2;
                let rest = s[3].tick(b).2;
                let m = s[4].tick(rest).0;
                let mid = s[5].tick(m).0;
                let h = s[6].tick(rest).2;
                let high = s[7].tick(h).2;
                let p = s[8].tick(low).0;
                let ap_lp = s[9].tick(p).0;
                let q = s[10].tick(low).2;
                let ap_hp = s[11].tick(q).2;
                let (gl, gm, gh) = (f64::from(self.g[0]), f64::from(self.g[1]), f64::from(self.g[2]));
                buf[i] = (gl * (ap_lp + ap_hp) + gm * mid + gh * high) as f32;
            }
            for (k, t) in target.iter().enumerate() {
                glide(&mut self.g[k], *t, 0.01);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// FILTER
// ---------------------------------------------------------------------------------------------

/// DJ filter sweep. `CutOff` runs -100..+100: negative is a low-pass closing from 18 kHz (just
/// below 0) to 80 Hz (-100), positive is a high-pass opening from 25 Hz to 6 kHz (+100); |v| < 0.5
/// is off. Two cascaded 12 dB/oct state-variable filters per channel (24 dB/oct), Q 0.8.
pub struct Filter {
    cut: Env,
    f: [[Svf; 2]; 2],
    mode: i8,
}

impl Filter {
    fn new(cut: Env) -> Filter {
        Filter { cut, f: [[Svf::default(); 2]; 2], mode: 0 }
    }
}

impl Fx for Filter {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let v = self.cut.at(pos + l.len() as f64 / 2.0, 0.0).clamp(-100.0, 100.0);
        let mode: i8 = if v < -0.5 { -1 } else if v > 0.5 { 1 } else { 0 };
        if mode != self.mode {
            self.f.iter_mut().flatten().for_each(Svf::reset);
            self.mode = mode;
        }
        if mode == 0 {
            return;
        }
        let a = f64::from((v.abs() - 0.5) / 99.5).clamp(0.0, 1.0);
        let fc = if mode < 0 { 18000.0 * (80.0f64 / 18000.0).powf(a) } else { 25.0 * (6000.0f64 / 25.0).powf(a) };
        for ch in 0..2 {
            for s in &mut self.f[ch] {
                s.set(fc, 0.8);
            }
        }
        for (ch, buf) in [l, r].into_iter().enumerate() {
            for x in buf.iter_mut() {
                let pick = |t: (f64, f64, f64)| if mode < 0 { t.0 } else { t.2 };
                let s1 = pick(self.f[ch][0].tick(f64::from(*x)));
                *x = pick(self.f[ch][1].tick(s1)) as f32;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Wah
// ---------------------------------------------------------------------------------------------

/// Wah: a resonant band-pass (Q 3) whose centre sweeps 400 Hz..3 kHz (log) with `CutOff`
/// -100..+100, added to the dry signal: `0.4 * dry + 1.8 * bandpass`.
pub struct Wah {
    cut: Env,
    f: [Svf; 2],
}

impl Wah {
    fn new(cut: Env) -> Wah {
        Wah { cut, f: [Svf::default(); 2] }
    }
}

impl Fx for Wah {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let v = f64::from(self.cut.at(pos + l.len() as f64 / 2.0, 0.0).clamp(-100.0, 100.0));
        let fc = 400.0 * (3000.0f64 / 400.0).powf((v + 100.0) / 200.0);
        for s in &mut self.f {
            s.set(fc, 3.0);
        }
        for (ch, buf) in [l, r].into_iter().enumerate() {
            for x in buf.iter_mut() {
                let d = f64::from(*x);
                let (_, bp, _) = self.f[ch].tick(d);
                *x = (0.4 * d + 1.8 * bp / 3.0) as f32;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Crush
// ---------------------------------------------------------------------------------------------

/// Bit and sample-rate reduction. `Crush` 0..100 (a = v/100): word length `16 - 10a` bits
/// (fractional values interpolate the level count), sample-and-hold factor `1 + 11a^2`. No
/// dither, so the result is deterministic. Off below 0.5.
pub struct Crush {
    amount: Env,
    phase: f64,
    held: [f32; 2],
}

impl Crush {
    fn new(amount: Env) -> Crush {
        Crush { amount, phase: 1.0, held: [0.0; 2] }
    }
}

impl Fx for Crush {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let v = self.amount.at(pos + l.len() as f64 / 2.0, 0.0).clamp(0.0, 100.0);
        if v < 0.5 {
            return;
        }
        let a = f64::from(v) / 100.0;
        let levels = 2f64.powf(15.0 - 10.0 * a) as f32;
        let hold = 1.0 + 11.0 * a * a;
        for i in 0..l.len() {
            self.phase += 1.0;
            if self.phase >= hold {
                self.phase -= hold;
                self.held = [(l[i] * levels).round() / levels, (r[i] * levels).round() / levels];
            }
            l[i] = self.held[0];
            r[i] = self.held[1];
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Trans
// ---------------------------------------------------------------------------------------------

/// Tempo-synced gate: 8th-note period, open for the first half, closed for the second, 2 ms
/// edges, phase locked to the tool's start. The period follows `StartBPM` (the tempo stored with
/// the tool) or, when that is 0, the clip's tempo at the tool start. Strength is the optional
/// `Trans-Wet/Dry` envelope (default 100 %).
pub struct Trans {
    wet: Env,
    period: f64,
    gate: f32,
}

impl Trans {
    fn new(wet: Env, start_bpm: f64, fallback: f64) -> Trans {
        let bpm = if start_bpm > 1.0 { start_bpm } else { fallback };
        Trans { wet, period: beat_samples(bpm) / 2.0, gate: 1.0 }
    }
}

impl Fx for Trans {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let w = self.wet.at(pos + l.len() as f64 / 2.0, 100.0).clamp(0.0, 100.0) / 100.0;
        for i in 0..l.len() {
            let ph = (pos + i as f64).rem_euclid(self.period) / self.period;
            let g = glide(&mut self.gate, if ph < 0.5 { 1.0 } else { 0.0 }, 1.0 / (0.002 * SR as f32));
            let m = (1.0 - w) + w * g;
            l[i] *= m;
            r[i] *= m;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Volume
// ---------------------------------------------------------------------------------------------

/// Volume tool: `Volume` in dB (0 = unity), smoothed.
pub struct VolumeFx {
    db: Env,
    g: f32,
    init: bool,
}

impl VolumeFx {
    fn new(db: Env) -> VolumeFx {
        VolumeFx { db, g: 1.0, init: false }
    }
}

impl Fx for VolumeFx {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let t = 10f32.powf(self.db.at(pos + l.len() as f64 / 2.0, 0.0).clamp(-96.0, 24.0) / 20.0);
        if !self.init {
            self.g = t;
            self.init = true;
        }
        for i in 0..l.len() {
            let g = glide(&mut self.g, t, 0.01);
            l[i] *= g;
            r[i] *= g;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reverb
// ---------------------------------------------------------------------------------------------

/// Reverb: two series Schroeder all-passes per channel diffuse the input, which feeds an
/// 8-line feedback delay network (delays 23..66 ms, mixed by an orthonormal Hadamard matrix,
/// one-pole damping in the loop). `Time` 0..100 sets RT60 = 0.3 + 4.7 * v/100 seconds (each
/// line's gain is `10^(-3 d / (RT60 sr))`); `Wet/Dry` 0..100 is the percentage of wet. Left
/// feeds and reads the even lines, right the odd ones.
pub struct Reverb {
    time: Env,
    wet: Env,
    ap: [[Allpass; 2]; 2],
    lines: Vec<Line>,
    damp: [f32; 8],
    rt60: f32,
    gains: [f32; 8],
}

const REVERB_DELAYS: [usize; 8] = [1031, 1327, 1523, 1801, 2063, 2371, 2647, 2927];

struct Line {
    buf: Vec<f32>,
    pos: usize,
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(n: usize) -> Allpass {
        Allpass { buf: vec![0.0; n], pos: 0 }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        const G: f32 = 0.6;
        let d = self.buf[self.pos];
        let v = x - G * d;
        self.buf[self.pos] = v;
        self.pos = (self.pos + 1) % self.buf.len();
        d + G * v
    }
}

impl Reverb {
    fn new(time: Env, wet: Env) -> Reverb {
        let mut r = Reverb {
            time,
            wet,
            ap: [[Allpass::new(223), Allpass::new(337)], [Allpass::new(241), Allpass::new(353)]],
            lines: REVERB_DELAYS.iter().map(|&n| Line { buf: vec![0.0; n], pos: 0 }).collect(),
            damp: [0.0; 8],
            rt60: -1.0,
            gains: [0.0; 8],
        };
        r.set_rt60(0.3 + 4.7 * 0.3);
        r
    }

    fn set_rt60(&mut self, rt60: f32) {
        if (rt60 - self.rt60).abs() < 1e-3 {
            return;
        }
        self.rt60 = rt60;
        for (g, d) in self.gains.iter_mut().zip(REVERB_DELAYS) {
            *g = 10f32.powf(-3.0 * d as f32 / (rt60 * SR as f32));
        }
    }
}

impl Fx for Reverb {
    fn tail(&self) -> usize {
        (self.rt60.clamp(0.3, 5.0) * SR as f32) as usize
    }

    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let c = pos + l.len() as f64 / 2.0;
        self.set_rt60(0.3 + 4.7 * self.time.at(c, 30.0).clamp(0.0, 100.0) / 100.0);
        let w = self.wet.at(c, 30.0).clamp(0.0, 100.0) / 100.0;
        const DAMP: f32 = 0.3;
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let il0 = self.ap[0][0].tick(dl);
            let il = self.ap[0][1].tick(il0);
            let ir0 = self.ap[1][0].tick(dr);
            let ir = self.ap[1][1].tick(ir0);
            let mut v = [0.0f32; 8];
            for (k, line) in self.lines.iter().enumerate() {
                v[k] = line.buf[line.pos];
            }
            let (mut outl, mut outr) = (0.0, 0.0);
            for (k, x) in v.iter().enumerate() {
                if k % 2 == 0 {
                    outl += x;
                } else {
                    outr += x;
                }
            }
            // damping and loop gain, then the Hadamard mix
            let mut m = [0.0f32; 8];
            for k in 0..8 {
                self.damp[k] = (1.0 - DAMP) * v[k] + DAMP * self.damp[k];
                m[k] = self.damp[k] * self.gains[k];
            }
            hadamard8(&mut m);
            for (k, line) in self.lines.iter_mut().enumerate() {
                let inj = if k % 2 == 0 { il } else { ir } * if (k / 2) % 2 == 0 { 0.25 } else { -0.25 };
                line.buf[line.pos] = m[k] * 0.353_553_4 + inj;
                line.pos = (line.pos + 1) % line.buf.len();
            }
            l[i] = (1.0 - w) * dl + w * outl * 0.5;
            r[i] = (1.0 - w) * dr + w * outr * 0.5;
        }
    }
}

/// In-place fast Walsh-Hadamard transform of 8 values (unnormalised).
fn hadamard8(v: &mut [f32; 8]) {
    let mut h = 1;
    while h < 8 {
        let mut i = 0;
        while i < 8 {
            for j in i..i + h {
                let (a, b) = (v[j], v[j + h]);
                v[j] = a + b;
                v[j + h] = a - b;
            }
            i += h * 2;
        }
        h *= 2;
    }
}

// ---------------------------------------------------------------------------------------------
// Echo and Roll: shared time parameter
// ---------------------------------------------------------------------------------------------

/// The two time parameters of Echo and Roll: `Time` in milliseconds and `Time ` (trailing space
/// in the key, as in the Editor) as a beat division `d`, meaning `2^d` beats (-2 = a 16th,
/// 0 = a beat, 0.585 = 1.5 beats). The effect's `SyncAgainstBeat` flag picks the division.
struct TimeParam {
    ms: Env,
    div: Env,
    sync: bool,
}

impl TimeParam {
    /// Delay time in samples at tool position `c`; `default_div` is used when no envelope exists.
    fn samples(&self, c: f64, bpm: f64, default_div: f32) -> f64 {
        let by_div = |d: f32| beat_samples(bpm) * 2f64.powf(f64::from(d.clamp(-6.0, 3.0)));
        let by_ms = |ms: f32| SR * f64::from(ms.clamp(1.0, 4000.0)) / 1000.0;
        match (self.sync, self.div.eval(c), self.ms.eval(c)) {
            (true, Some(d), _) => by_div(d),
            (true, None, Some(ms)) => by_ms(ms),
            (false, _, Some(ms)) => by_ms(ms),
            (false, Some(d), None) => by_div(d),
            (_, None, None) => by_div(default_div),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Echo
// ---------------------------------------------------------------------------------------------

/// Stereo echo with cross-fed feedback (0.45, one-pole low-pass at about 4 kHz in the loop, so
/// repeats get darker and bounce between the channels). The delay time glides (about 100 ms) when
/// its envelope moves, which gives the tape-style pitch sweep. Default time one beat, wet 35 %.
pub struct Echo {
    time: TimeParam,
    wet: Env,
    buf: [Vec<f32>; 2],
    w: usize,
    cur: f64,
    init: bool,
    lp: [f32; 2],
    max_delay: f64,
}

const ECHO_LEN: usize = 1 << 18;

impl Echo {
    fn new(time: TimeParam, wet: Env) -> Echo {
        Echo {
            time,
            wet,
            buf: [vec![0.0; ECHO_LEN], vec![0.0; ECHO_LEN]],
            w: 0,
            cur: 0.0,
            init: false,
            lp: [0.0; 2],
            max_delay: 3.0 * SR,
        }
    }
}

impl Fx for Echo {
    /// Repeats until they fall under -60 dB: `ln(0.001)/ln(0.45)` = 8.65 delays, capped at 10 s.
    fn tail(&self) -> usize {
        ((8.65 * self.cur) as usize).clamp(SR as usize / 2, 10 * SR as usize)
    }

    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, bpm: f64) {
        let c = pos + l.len() as f64 / 2.0;
        let target = self.time.samples(c, bpm, 0.0).clamp(32.0, self.max_delay);
        if !self.init {
            self.cur = target;
            self.init = true;
        }
        let w = self.wet.at(c, 35.0).clamp(0.0, 100.0) / 100.0;
        const FB: f32 = 0.45;
        const DAMP: f32 = 0.45;
        let mask = ECHO_LEN - 1;
        for i in 0..l.len() {
            self.cur += (target - self.cur) * 0.0002;
            let rp = self.w as f64 + ECHO_LEN as f64 - self.cur;
            let i0 = rp.floor() as usize;
            let f = (rp - rp.floor()) as f32;
            let tap = |b: &Vec<f32>| b[i0 & mask] * (1.0 - f) + b[(i0 + 1) & mask] * f;
            let (da, db) = (tap(&self.buf[0]), tap(&self.buf[1]));
            let (dl, dr) = (l[i], r[i]);
            // cross-fed feedback through a damping low-pass
            self.lp[0] += DAMP * (db - self.lp[0]);
            self.lp[1] += DAMP * (da - self.lp[1]);
            self.buf[0][self.w] = dl + FB * self.lp[0];
            self.buf[1][self.w] = dr + FB * self.lp[1];
            self.w = (self.w + 1) & mask;
            l[i] = (1.0 - w) * dl + w * da;
            r[i] = (1.0 - w) * dr + w * db;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Roll
// ---------------------------------------------------------------------------------------------

/// Roll (beat repeat): records one slice as it passes (the slice plays through live while it is
/// captured), then loops it. Slice length from the time parameters (default 1/2 beat); when the
/// requested length changes by more than 5 % a new slice is captured. Loop edges have 32-sample
/// fades. Wet 25..75 % in the known documents; default 50 %.
pub struct Roll {
    time: TimeParam,
    wet: Env,
    buf: [Vec<f32>; 2],
    cap: usize,
    filled: usize,
    playing: bool,
    ppos: usize,
}

const ROLL_MAX: usize = 2 * RATE as usize;

impl Roll {
    fn new(time: TimeParam, wet: Env) -> Roll {
        Roll { time, wet, buf: [vec![0.0; ROLL_MAX], vec![0.0; ROLL_MAX]], cap: 0, filled: 0, playing: false, ppos: 0 }
    }
}

impl Fx for Roll {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, bpm: f64) {
        let c = pos + l.len() as f64 / 2.0;
        let want = (self.time.samples(c, bpm, -1.0).round() as usize).clamp(128, ROLL_MAX);
        if self.cap == 0 || (want as f64 - self.cap as f64).abs() > 0.05 * self.cap as f64 {
            self.cap = want;
            self.filled = 0;
            self.playing = false;
        }
        let w = self.wet.at(c, 50.0).clamp(0.0, 100.0) / 100.0;
        const FADE: usize = 32;
        for i in 0..l.len() {
            let (dl, dr) = (l[i], r[i]);
            let (wl, wr) = if self.playing {
                let p = self.ppos;
                let g = if p < FADE {
                    p as f32 / FADE as f32
                } else if p + FADE > self.cap {
                    (self.cap - p) as f32 / FADE as f32
                } else {
                    1.0
                };
                self.ppos += 1;
                if self.ppos >= self.cap {
                    self.ppos = 0;
                }
                (self.buf[0][p] * g, self.buf[1][p] * g)
            } else {
                self.buf[0][self.filled] = dl;
                self.buf[1][self.filled] = dr;
                self.filled += 1;
                if self.filled >= self.cap {
                    self.playing = true;
                    self.ppos = 0;
                }
                (dl, dr)
            };
            l[i] = (1.0 - w) * dl + w * wl;
            r[i] = (1.0 - w) * dr + w * wr;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hadamard_is_orthogonal_up_to_scale() {
        let mut v = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        hadamard8(&mut v);
        assert!(v.iter().all(|x| (*x - 1.0).abs() < 1e-6));
        hadamard8(&mut v);
        assert!((v[0] - 8.0).abs() < 1e-5 && v[1..].iter().all(|x| x.abs() < 1e-5));
    }

    #[test]
    fn eq_gain_curve() {
        assert_eq!(eq_gain(0.0), 1.0);
        assert_eq!(eq_gain(-100.0), 0.0);
        assert!((eq_gain(-50.0) - 0.25).abs() < 1e-6);
        assert!(eq_gain(100.0) > 1.9);
    }
}
