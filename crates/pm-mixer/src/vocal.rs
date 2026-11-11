//! The Vocal filter: what the "Wah" tool sounds like on a Pacemaker running Revival firmware
//! 180096073 or later with the vocal flag set. A line-by-line f32 port of the device's
//! `device/vocalfx/vocal_dsp.c` (the DSP) and of the mapping in `vocalfx.c` (Wah value to target,
//! with the engage hysteresis and the invert flag), so a preview and an export sound like the
//! device. The C is the source of truth: when it changes, this file and the golden test
//! (`tests/vocal_equiv.rs`) change with it.
//!
//! Mid/side with a phase-coherent three-band split of the centre. The mid signal goes through
//! two Linkwitz-Riley 4th-order crossovers ([`F1_HZ`], [`F2_HZ`]) into low / mid / high bands
//! that sum back to an all-pass of the input; the side signal goes through the same all-pass so
//! the stereo image stays coherent. Gains are applied per band:
//!
//! ```text
//! focus (k = +kp):  low, high * (1-kp);  mid * (1 + kp*(G-1));  side * (1-kp)
//! cut   (k = -kn):  mid * (1-kn); everything else unchanged
//! ```
//!
//! `k` is smoothed per sample (one pole, [`SMOOTH_MS`]); an engage weight `e` crossfades from the
//! untouched input to the processed signal, and a block is left bit-identical while idle.
//! 11 biquads per stereo frame in f32, transposed direct form II, coefficients designed in f64
//! and cast once, exactly as the C does.
//!
//! The arithmetic order of every expression follows the C source (no fused multiply-add, no
//! reassociation; Rust never contracts, and the C reference is built with
//! `-ffp-contract=off`), which is what makes the output bit-identical to it.

use crate::env::Env;
use crate::fx::Fx;
use crate::RATE;

/// Low / mid crossover (Hz).
pub const F1_HZ: f64 = 150.0;
/// Mid / high crossover (Hz).
pub const F2_HZ: f64 = 4000.0;
/// One-pole time constant of the amount and of the engage weight (ms).
pub const SMOOTH_MS: f64 = 10.0;
/// Make-up gain of the kept centre band at full focus (about +2.9 dB).
pub const FOCUS_GAIN: f32 = 1.4;
/// Alternating offset into every chain: keeps all filter states normal numbers.
pub const DENORM_OFF: f32 = 1e-18;
/// Processed samples above this are compressed towards 1.0 (the all-pass raises crest factors).
pub const KNEE: f32 = 0.97;
/// An input sample at or beyond this (or NaN / Inf) is garbage and replaced by 0 before it reaches a filter.
pub const SANE: f32 = 1e4;
/// `|Wah value|` at which the effect engages ...
pub const ENGAGE_ON: f32 = 1.0;
/// ... and below which it releases again: a value hovering around +-1 must not toggle it.
pub const ENGAGE_OFF: f32 = 0.5;
/// Biquads per stereo frame.
pub const NBQ: usize = 11;
/// Frames per pass inside [`process`] (the C keeps 7 scratch arrays of this size on the stack).
const SUB: usize = 128;

/// Indices into the state arrays (`enum` in the C).
const LP1A: usize = 0;
const LP1B: usize = 1;
const HP1A: usize = 2;
const HP1B: usize = 3;
const LP2A: usize = 4;
const LP2B: usize = 5;
const HP2A: usize = 6;
const HP2B: usize = 7;
const AP2L: usize = 8;
const AP1S: usize = 9;
const AP2S: usize = 10;

/// One biquad section: `b0 b1 b2 a1 a2` (a0 normalised to 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Biquad {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

/// RBJ cookbook biquad kinds, Butterworth Q = 1/sqrt 2.
#[derive(Clone, Copy)]
enum Kind {
    Low,
    High,
    All,
}

fn design_biquad(kind: Kind, fs: f64, f0: f64) -> Biquad {
    const Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
    // same operation order as the C: ((2 * pi) * f0) / fs
    let w = 2.0 * std::f64::consts::PI * f0 / fs;
    let cs = w.cos();
    let al = w.sin() / (2.0 * Q);
    let a0 = 1.0 + al;
    let (b0, b1, b2);
    match kind {
        Kind::Low => {
            b0 = (1.0 - cs) / 2.0;
            b1 = 1.0 - cs;
            b2 = b0;
        }
        Kind::High => {
            b0 = (1.0 + cs) / 2.0;
            b1 = -(1.0 + cs);
            b2 = b0;
        }
        Kind::All => {
            b0 = 1.0 - al;
            b1 = -2.0 * cs;
            b2 = 1.0 + al;
        }
    }
    Biquad { b0: (b0 / a0) as f32, b1: (b1 / a0) as f32, b2: (b2 / a0) as f32, a1: (-2.0 * cs / a0) as f32, a2: ((1.0 - al) / a0) as f32 }
}

/// Filter coefficients for one sample rate (`vfx_coefs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coefs {
    lp1: Biquad,
    hp1: Biquad,
    ap1: Biquad,
    lp2: Biquad,
    hp2: Biquad,
    ap2: Biquad,
    /// Smoothing coefficient of the amount and the engage weight.
    pub alpha: f32,
}

impl Coefs {
    /// `vfx_design`. The device passes the engine's sample rate (44.1 kHz here).
    pub fn design(fs: f64) -> Coefs {
        Coefs {
            lp1: design_biquad(Kind::Low, fs, F1_HZ),
            hp1: design_biquad(Kind::High, fs, F1_HZ),
            ap1: design_biquad(Kind::All, fs, F1_HZ),
            lp2: design_biquad(Kind::Low, fs, F2_HZ),
            hp2: design_biquad(Kind::High, fs, F2_HZ),
            ap2: design_biquad(Kind::All, fs, F2_HZ),
            alpha: (1.0 - (-1.0 / (SMOOTH_MS * 1e-3 * fs)).exp()) as f32,
        }
    }
}

/// Filter and smoothing state (`vfx_state`).
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    z1: [f32; NBQ],
    z2: [f32; NBQ],
    /// Smoothed amount, -1..+1.
    k: f32,
    /// Smoothed engage weight, 0..1.
    e: f32,
    /// Sign-alternating denormal offset.
    dn: f32,
    active: bool,
}

impl Default for State {
    fn default() -> State {
        State::new()
    }
}

impl State {
    pub fn new() -> State {
        State { z1: [0.0; NBQ], z2: [0.0; NBQ], k: 0.0, e: 0.0, dn: DENORM_OFF, active: false }
    }

    /// `vfx_reset`.
    pub fn reset(&mut self) {
        *self = State::new();
    }

    /// True while the filter is running (not in the idle bypass).
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Soft knee: identity up to [`KNEE`], then asymptotic to 1.0 with continuous slope; never
/// exceeds 1.0 for finite input.
#[inline]
fn soft(x: f32) -> f32 {
    let mut a = if x < 0.0 { -x } else { x };
    if a > KNEE {
        let u = (a - KNEE) * (1.0 / (1.0 - KNEE));
        a = KNEE + (1.0 - KNEE) * (u / (1.0 + u));
        return if x < 0.0 { -a } else { a };
    }
    x
}

/// One biquad over a block, in place or not: `out[i]` from `inp[i]`. Transposed direct form II,
/// the exact expression order of `bq_run3` in the C.
#[inline]
fn run(q: &Biquad, z1: &mut f32, z2: &mut f32, inp: &[f32], out: &mut [f32]) {
    let (mut s1, mut s2) = (*z1, *z2);
    for (o, &x) in out.iter_mut().zip(inp) {
        let y = q.b0 * x + s1;
        s1 = q.b1 * x - q.a1 * y + s2;
        s2 = q.b2 * x - q.a2 * y;
        *o = y;
    }
    *z1 = s1;
    *z2 = s2;
}

/// The same, with input and output the same buffer.
#[inline]
fn run_in_place(q: &Biquad, z1: &mut f32, z2: &mut f32, buf: &mut [f32]) {
    let (mut s1, mut s2) = (*z1, *z2);
    for v in buf.iter_mut() {
        let x = *v;
        let y = q.b0 * x + s1;
        s1 = q.b1 * x - q.a1 * y + s2;
        s2 = q.b2 * x - q.a2 * y;
        *v = y;
    }
    *z1 = s1;
    *z2 = s2;
}

/// `vfx_process`: filters the stereo block in place. `target` is -1..+1 (positive = vocal
/// focus), `engaged` false means "off". Returns true when the block was processed, false when it
/// was left untouched (idle bypass: bit-identical, state forgotten).
// `!(x < y)` is deliberate: a NaN must take the "bad" branch, which `x >= y` would not.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn process(c: &Coefs, s: &mut State, l: &mut [f32], r: &mut [f32], target: f32, engaged: bool) -> bool {
    debug_assert_eq!(l.len(), r.len());
    let n = l.len().min(r.len());
    let mut target = target;
    if target.is_nan() {
        target = 0.0;
    }
    target = target.clamp(-1.0, 1.0);
    let et: f32 = if engaged { 1.0 } else { 0.0 };
    if !engaged {
        target = 0.0;
    }

    if !engaged && s.e < 1e-4 {
        // idle: leave the block alone, forget the state
        if s.active {
            s.reset();
        }
        return false;
    }
    if !s.active {
        // cold start, fade in from the dry signal
        s.reset();
        s.active = true;
    }
    let (mut k, mut e, al) = (s.k, s.e, c.alpha);
    let mut dn = s.dn;
    let mut z1 = s.z1;
    let mut z2 = s.z2;

    let mut off = 0;
    while off < n {
        let cnt = (n - off).min(SUB);
        let lp = &mut l[off..off + cnt];
        let rp = &mut r[off..off + cnt];
        let mut xm = [0.0f32; SUB];
        let mut xs = [0.0f32; SUB];
        let mut lo = [0.0f32; SUB];
        let mut t1 = [0.0f32; SUB];
        let mut mid = [0.0f32; SUB];
        let mut hi = [0.0f32; SUB];
        let mut side = [0.0f32; SUB];

        for i in 0..cnt {
            let mut li = lp[i];
            let mut ri = rp[i];
            // NaN / Inf / absurd: contained to this one sample
            if !(li.abs() < SANE) {
                li = 0.0;
            }
            if !(ri.abs() < SANE) {
                ri = 0.0;
            }
            lp[i] = li;
            rp[i] = ri;
            let m = 0.5 * (li + ri);
            let sd = 0.5 * (li - ri);
            // alternating Nyquist offset, see DENORM_OFF
            xm[i] = m + dn;
            xs[i] = sd - dn;
            dn = -dn;
        }

        // low / high split at F1, then the high part split at F2 (each LR4 = the same BW2 section twice)
        // level 1
        run(&c.lp1, &mut z1[LP1A], &mut z2[LP1A], &xm[..cnt], &mut lo[..cnt]);
        run(&c.hp1, &mut z1[HP1A], &mut z2[HP1A], &xm[..cnt], &mut t1[..cnt]);
        run(&c.ap1, &mut z1[AP1S], &mut z2[AP1S], &xs[..cnt], &mut side[..cnt]);
        // level 2
        run_in_place(&c.lp1, &mut z1[LP1B], &mut z2[LP1B], &mut lo[..cnt]);
        run_in_place(&c.hp1, &mut z1[HP1B], &mut z2[HP1B], &mut t1[..cnt]);
        run_in_place(&c.ap2, &mut z1[AP2S], &mut z2[AP2S], &mut side[..cnt]);
        // level 3: low band onto the same all-pass as mid + high
        run_in_place(&c.ap2, &mut z1[AP2L], &mut z2[AP2L], &mut lo[..cnt]);
        run(&c.hp2, &mut z1[HP2A], &mut z2[HP2A], &t1[..cnt], &mut hi[..cnt]);
        run(&c.lp2, &mut z1[LP2A], &mut z2[LP2A], &t1[..cnt], &mut mid[..cnt]);
        // level 4
        run_in_place(&c.hp2, &mut z1[HP2B], &mut z2[HP2B], &mut hi[..cnt]);
        run_in_place(&c.lp2, &mut z1[LP2B], &mut z2[LP2B], &mut mid[..cnt]);

        for i in 0..cnt {
            let (lv, rv) = (lp[i], rp[i]);
            // amount smoothing: no zipper noise
            k += (target - k) * al;
            e += (et - e) * al;
            let kp = if k > 0.0 { k } else { 0.0 };
            let kn = if k < 0.0 { -k } else { 0.0 };
            // low and high bands, and the side
            let gl = 1.0 - kp;
            let gm = (1.0 + kp * (FOCUS_GAIN - 1.0)) * (1.0 - kn);
            let mo = gl * (lo[i] + hi[i]) + gm * mid[i];
            let so = gl * side[i];
            lp[i] = (1.0 - e) * lv + e * soft(mo + so);
            rp[i] = (1.0 - e) * rv + e * soft(mo - so);
        }
        off += cnt;
    }

    // state sanity: a NaN/Inf or runaway state (bad input) must not stick for ever
    let mut bad = !(k.abs() <= 1.0001 && (-0.0001..=1.0001).contains(&e));
    for q in 0..NBQ {
        if !(z1[q].abs() < 1e6 && z2[q].abs() < 1e6) {
            bad = true;
        }
    }
    if bad {
        s.reset();
        return true;
    }
    s.z1 = z1;
    s.z2 = z2;
    s.k = k;
    s.e = e;
    s.dn = dn;
    true
}

/// `vfx_engage`: engage decision with hysteresis. `v` is the Wah value (-100..100, finite), `was`
/// the previous decision.
pub fn engage(v: f32, was: bool) -> bool {
    let a = if v < 0.0 { -v } else { v };
    if was {
        a >= ENGAGE_OFF
    } else {
        a >= ENGAGE_ON
    }
}

/// The mapping of `vocal_process` in `vocalfx.c`: the Wah value to the DSP's `(target, engaged)`.
/// A value outside -100..+100 or NaN counts as 0; `was` is the previous `engaged`; `invert`
/// swaps the two directions (the flag file starts with `i`).
pub fn wah_target(v: f32, was: bool, invert: bool) -> (f32, bool) {
    let v = if (-100.0..=100.0).contains(&v) { v } else { 0.0 };
    let engaged = engage(v, was);
    let mut target = if engaged { v * 0.01 } else { 0.0 };
    if invert {
        target = -target;
    }
    (target, engaged)
}

/// The Vocal filter as a tool effect: reads the `Wah-CutOff` envelope at the centre of each
/// control-rate tick (like every other effect, see [`crate::fx`]), maps it like the device does
/// once per audio block, and runs the DSP over the tick with its per-sample smoothing intact.
pub struct VocalFx {
    cut: Env,
    coefs: Coefs,
    state: State,
    engaged: bool,
    invert: bool,
}

impl VocalFx {
    pub fn new(cut: Env, invert: bool) -> VocalFx {
        VocalFx { cut, coefs: Coefs::design(f64::from(RATE)), state: State::new(), engaged: false, invert }
    }
}

impl Fx for VocalFx {
    fn process(&mut self, l: &mut [f32], r: &mut [f32], pos: f64, _bpm: f64) {
        let v = self.cut.at(pos + l.len() as f64 / 2.0, 0.0);
        let (target, engaged) = wah_target(v, self.engaged, self.invert);
        self.engaged = engaged;
        process(&self.coefs, &mut self.state, l, r, target, engaged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 44_100.0;

    /// The LCG of the device's tests (`dsp_test.c`).
    struct Rng(u32);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((self.0 >> 8) & 0xffff) as f32 / 32768.0 - 1.0
        }
    }

    fn tone(hz: f64, n: usize, off: usize) -> Vec<f32> {
        (0..n).map(|i| (0.5 * (2.0 * std::f64::consts::PI * hz * (off + i) as f64 / FS).sin()) as f32).collect()
    }

    fn rms(a: &[f32], b: &[f32]) -> f64 {
        let s: f64 = a.iter().zip(b).map(|(x, y)| f64::from(*x) * f64::from(*x) + f64::from(*y) * f64::from(*y)).sum();
        (s / (2.0 * a.len() as f64)).sqrt()
    }

    /// Gain in dB of a centre tone at a fixed target, measured after the smoothing has settled.
    fn gain_db(hz: f64, target: f32) -> f64 {
        let (total, settle, blk) = ((FS * 1.6) as usize, (FS * 0.6) as usize, 512);
        let x = tone(hz, total, 0);
        let (mut l, mut r) = (x.clone(), x.clone());
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut i = 0;
        while i < total {
            let m = blk.min(total - i);
            process(&c, &mut s, &mut l[i..i + m], &mut r[i..i + m], target, target.abs() >= 0.01);
            i += m;
        }
        let gi = rms(&x[settle..], &x[settle..]);
        let go = rms(&l[settle..], &r[settle..]);
        if go < 1e-12 {
            -240.0
        } else {
            20.0 * (go / gi).log10()
        }
    }

    /// Gain of a signal of a given shape at a fixed target.
    fn shape_gain_db(kind: &str, target: f32) -> f64 {
        let (total, settle, blk) = ((FS * 1.6) as usize, (FS * 0.6) as usize, 512);
        let mut rng = Rng(12345);
        let (mut l, mut r) = (vec![0.0f32; total], vec![0.0f32; total]);
        for i in 0..total {
            match kind {
                "side" => {
                    let nz = rng.next() * 0.35;
                    l[i] = nz;
                    r[i] = -nz;
                }
                "bass" => {
                    let b = (0.45 * (2.0 * std::f64::consts::PI * 60.0 * i as f64 / FS).sin()) as f32;
                    l[i] = b;
                    r[i] = b;
                }
                _ => unreachable!(),
            }
        }
        let (il, ir) = (l.clone(), r.clone());
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut i = 0;
        while i < total {
            let m = blk.min(total - i);
            process(&c, &mut s, &mut l[i..i + m], &mut r[i..i + m], target, target.abs() >= 0.01);
            i += m;
        }
        let go = rms(&l[settle..], &r[settle..]);
        20.0 * (go.max(1e-12) / rms(&il[settle..], &ir[settle..])).log10()
    }

    #[test]
    fn idle_block_is_bit_identical_and_reports_unprocessed() {
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut rng = Rng(7);
        let (mut l, mut r): (Vec<f32>, Vec<f32>) = (0..512).map(|_| (rng.next(), rng.next())).unzip();
        let (l0, r0) = (l.clone(), r.clone());
        // every |v| < 1 maps to "not engaged" from rest
        for v in [0.0f32, 0.3, -0.99, 0.999] {
            let (t, eng) = wah_target(v, false, false);
            assert!(!eng, "{v}");
            assert!(!process(&c, &mut s, &mut l, &mut r, t, eng));
            assert_eq!(l.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), l0.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
            assert_eq!(r.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), r0.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        }
        assert!(!s.is_active());
    }

    #[test]
    fn engage_has_hysteresis_at_one_and_a_half() {
        assert!(!engage(0.99, false));
        assert!(engage(1.0, false));
        assert!(engage(-1.0, false));
        assert!(!engage(0.5, false));
        // once engaged it holds down to 0.5 and releases below
        assert!(engage(0.5, true));
        assert!(engage(-0.5, true));
        assert!(!engage(0.49, true));
        assert!(!engage(0.0, true));
        // a pad hovering around +-1 does not toggle
        let mut was = false;
        let mut flips = 0;
        for v in [0.9, 1.0, 0.9, 1.1, 0.7, 0.95, 1.0, 0.6, 0.55] {
            let now = engage(v, was);
            if now != was {
                flips += 1;
            }
            was = now;
        }
        assert_eq!(flips, 1);
    }

    #[test]
    fn wah_target_maps_like_the_device() {
        assert_eq!(wah_target(100.0, false, false), (1.0, true));
        assert_eq!(wah_target(-100.0, false, false), (-1.0, true));
        assert_eq!(wah_target(100.0, false, true), (-1.0, true));
        assert_eq!(wah_target(-50.0, false, true), (0.5, true));
        assert!(!wah_target(0.4, true, false).1);
        assert_eq!(wah_target(0.4, true, false).0, 0.0);
        // out of range or NaN counts as 0 (the device writes 0, it does not clamp)
        for bad in [f32::NAN, 101.0, -250.0, f32::INFINITY] {
            assert_eq!(wah_target(bad, true, false), (0.0, false), "{bad}");
        }
    }

    #[test]
    fn full_cut_matches_the_documented_response() {
        // the table in device/vocalfx/README.md (cut -100, centre tone, dB): the deep part is
        // only about 300 Hz to 2 kHz, the crossovers are -6 dB, the extremes barely move
        let table = [(150.0, -6.0), (220.0, -15.0), (300.0, -24.6), (500.0, -41.7), (1000.0, -47.9), (2000.0, -25.3), (3000.0, -12.7), (4000.0, -6.0), (6000.0, -1.4)];
        for (hz, want) in table {
            let g = gain_db(hz, -1.0);
            assert!((g - want).abs() < 0.6, "{hz} Hz is {g:.1} dB, documented {want}");
        }
        assert!(gain_db(1000.0, -1.0) < -40.0, "about -48 dB at 1 kHz");
        // more than 25 dB only between about 300 Hz and 2 kHz
        for hz in [40.0, 60.0, 100.0, 220.0, 3000.0, 6000.0, 10000.0, 16000.0] {
            assert!(gain_db(hz, -1.0) > -25.0, "{hz} Hz cut {:.1} dB should be outside the deep band", gain_db(hz, -1.0));
        }
        for hz in [500.0, 1000.0, 2000.0] {
            assert!(gain_db(hz, -1.0) < -25.0, "{hz} Hz only {:.1} dB", gain_db(hz, -1.0));
        }
        // -6 dB at both crossover points
        for hz in [F1_HZ, F2_HZ] {
            let g = gain_db(hz, -1.0);
            assert!((g + 6.0).abs() < 0.7, "{hz} Hz is {g:.2} dB, expected about -6");
        }
    }

    #[test]
    fn partial_cut_has_no_notch() {
        for hz in [40.0, 100.0, 150.0, 220.0, 500.0, 1000.0, 3000.0, 4000.0, 10000.0] {
            assert!(gain_db(hz, -0.5) > -7.0, "{hz} Hz at -50: {:.1} dB", gain_db(hz, -0.5));
        }
    }

    #[test]
    fn cut_leaves_side_and_bass_alone() {
        assert!(shape_gain_db("side", -1.0).abs() < 0.3, "side {:.2}", shape_gain_db("side", -1.0));
        assert!(shape_gain_db("bass", -1.0) > -1.0, "60 Hz bass {:.2}", shape_gain_db("bass", -1.0));
    }

    #[test]
    fn full_focus_keeps_only_the_centre_band() {
        let side = shape_gain_db("side", 1.0);
        assert!(side < -80.0, "side {side:.1} dB");
        let bass = shape_gain_db("bass", 1.0);
        assert!(bass < -25.0, "60 Hz centre bass {bass:.1} dB");
        let voice = gain_db(1000.0, 1.0);
        assert!(voice > 1.0 && voice < 4.0, "kept band gain {voice:.2} dB (about +2.9)");
        // focus at +50: the side fades by about -6 dB
        let half = shape_gain_db("side", 0.5);
        assert!(half < -5.5 && half > -7.0, "side at +50 {half:.2}");
    }

    #[test]
    fn crossover_points_are_minus_six_db_in_focus_too() {
        // in focus the make-up gain applies to the mid band only; at the crossovers the low/high
        // band is at -6 dB of the mid signal and the mid band meets it there
        for hz in [F1_HZ, F2_HZ] {
            let g = gain_db(hz, 1.0);
            assert!(g < 0.0 && g > -9.0, "{hz} Hz focus {g:.2} dB");
        }
    }

    #[test]
    fn hostile_input_is_contained() {
        let c = Coefs::design(FS);
        let mut s = State::new();
        let n = 4096;
        let mut rng = Rng(99);
        let mut l: Vec<f32> = (0..n).map(|_| rng.next() * 0.5).collect();
        let mut r = l.clone();
        l[100] = f32::NAN;
        r[200] = f32::INFINITY;
        l[300] = f32::NEG_INFINITY;
        r[400] = 1e30;
        l[500] = -1e4;
        r[600] = f32::MAX;
        process(&c, &mut s, &mut l, &mut r, 1.0, true);
        for (i, v) in l.iter().chain(r.iter()).enumerate() {
            assert!(v.is_finite() && v.abs() <= 1.0001 + 1e-3, "sample {i} = {v}");
        }
        // the bad samples were replaced by zero before they entered the filters
        // (a later clean block still gives a finite, bounded result)
        let mut l2: Vec<f32> = (0..n).map(|_| rng.next() * 0.5).collect();
        let mut r2 = l2.clone();
        process(&c, &mut s, &mut l2, &mut r2, 1.0, true);
        assert!(l2.iter().chain(r2.iter()).all(|v| v.is_finite() && v.abs() <= 1.0001));
        // NaN / out of range targets
        let mut l3 = vec![0.25f32; 256];
        let mut r3 = vec![0.25f32; 256];
        process(&c, &mut s, &mut l3, &mut r3, f32::NAN, true);
        process(&c, &mut s, &mut l3, &mut r3, 55.0, true);
        process(&c, &mut s, &mut l3, &mut r3, -55.0, true);
        assert!(l3.iter().chain(r3.iter()).all(|v| v.is_finite() && v.abs() <= 1.0001));
        assert!(s.k.abs() <= 1.0001);
    }

    #[test]
    fn output_never_leaves_the_soft_knee_ceiling() {
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut rng = Rng(5);
        let mut l: Vec<f32> = (0..20_000).map(|_| rng.next()).collect();
        let mut r: Vec<f32> = (0..20_000).map(|_| rng.next()).collect();
        for (a, b) in l.chunks_mut(500).zip(r.chunks_mut(500)) {
            process(&c, &mut s, a, b, 1.0, true);
        }
        assert!(l.iter().chain(r.iter()).all(|v| v.abs() <= 1.0));
    }

    #[test]
    fn re_running_is_deterministic() {
        let run_once = || {
            let c = Coefs::design(FS);
            let mut s = State::new();
            let mut rng = Rng(42);
            let mut l: Vec<f32> = (0..3000).map(|_| rng.next() * 0.7).collect();
            let mut r: Vec<f32> = (0..3000).map(|_| rng.next() * 0.7).collect();
            for (i, (a, b)) in l.chunks_mut(97).zip(r.chunks_mut(97)).enumerate() {
                let v = [-100.0f32, 40.0, 0.0, 100.0][i % 4];
                let (t, eng) = wah_target(v, s.is_active(), false);
                process(&c, &mut s, a, b, t, eng);
            }
            l.iter().chain(r.iter()).map(|x| x.to_bits()).collect::<Vec<_>>()
        };
        assert_eq!(run_once(), run_once());
    }

    #[test]
    fn call_size_does_not_change_the_output() {
        let c = Coefs::design(FS);
        let mut rng = Rng(3);
        let x: Vec<f32> = (0..6000).map(|_| rng.next() * 0.6).collect();
        let y: Vec<f32> = (0..6000).map(|_| rng.next() * 0.6).collect();
        let render = |blk: usize| {
            let (mut l, mut r) = (x.clone(), y.clone());
            let mut s = State::new();
            let mut i = 0;
            while i < l.len() {
                let m = blk.min(l.len() - i);
                process(&c, &mut s, &mut l[i..i + m], &mut r[i..i + m], 0.7, true);
                i += m;
            }
            l.iter().chain(r.iter()).map(|v| v.to_bits()).collect::<Vec<_>>()
        };
        let whole = render(6000);
        assert_eq!(render(1), whole);
        assert_eq!(render(32), whole);
        assert_eq!(render(500), whole);
        assert_eq!(render(129), whole);
    }

    #[test]
    fn release_fades_out_and_returns_to_exact_bypass() {
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut rng = Rng(11);
        let mut gen = |n: usize| -> (Vec<f32>, Vec<f32>) { (0..n).map(|_| (rng.next() * 0.5, rng.next() * 0.5)).unzip() };
        for _ in 0..40 {
            let (mut l, mut r) = gen(512);
            process(&c, &mut s, &mut l, &mut r, 1.0, true);
        }
        assert!(s.is_active());
        let mut blocks = 0;
        let processed_last;
        loop {
            let (mut l, mut r) = gen(512);
            let (l0, r0) = (l.clone(), r.clone());
            let p = process(&c, &mut s, &mut l, &mut r, 0.0, false);
            blocks += 1;
            if !p {
                assert_eq!(l, l0);
                assert_eq!(r, r0);
                processed_last = p;
                break;
            }
            assert!(blocks < 400, "never returned to bypass");
        }
        assert!(!processed_last && !s.is_active());
        // e falls from 1 by exp(-t/10 ms): below 1e-4 after ln(1e4) * 10 ms = 92 ms = 8 blocks
        assert!((7..=12).contains(&blocks), "{blocks} blocks");
    }

    #[test]
    fn silence_in_silence_out_and_state_stays_small() {
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut rng = Rng(8);
        let (mut l, mut r): (Vec<f32>, Vec<f32>) = (0..4096).map(|_| (rng.next(), rng.next())).unzip();
        process(&c, &mut s, &mut l, &mut r, -1.0, true);
        // ten seconds of silence while engaged: the output decays to (denormal-offset) silence
        let mut peak = 0.0f32;
        for _ in 0..(10 * 44100 / 512) {
            let (mut a, mut b) = (vec![0.0f32; 512], vec![0.0f32; 512]);
            process(&c, &mut s, &mut a, &mut b, -1.0, true);
            peak = a.iter().chain(b.iter()).fold(peak, |p, v| p.max(v.abs()));
        }
        let (mut a, mut b) = (vec![0.0f32; 512], vec![0.0f32; 512]);
        process(&c, &mut s, &mut a, &mut b, -1.0, true);
        assert!(a.iter().chain(b.iter()).all(|v| v.abs() < 1e-9), "tail did not die out");
        assert!(peak > 0.0);
        assert!(s.z1.iter().chain(s.z2.iter()).all(|v| v.is_finite() && (*v == 0.0 || v.abs() >= f32::MIN_POSITIVE)));
    }

    #[test]
    fn a_runaway_state_resets_instead_of_sticking() {
        let c = Coefs::design(FS);
        let mut s = State::new();
        let mut l = vec![0.1f32; 64];
        let mut r = vec![0.1f32; 64];
        process(&c, &mut s, &mut l, &mut r, 1.0, true);
        s.z1[3] = 1e7;
        process(&c, &mut s, &mut l, &mut r, 1.0, true);
        assert!(!s.is_active(), "bad state must be dropped");
        assert_eq!(s, State::new());
    }

    #[test]
    fn fx_reads_the_envelope_with_hysteresis_and_invert() {
        use pm_formats::mix::Node;
        let nodes = |v: f32| vec![Node { pos: 0, value: v }, Node { pos: 100_000, value: v }];
        let mk = |v: f32, inv: bool| VocalFx::new(Env::new(&nodes(v), 0.0), inv);
        let mut rng = Rng(21);
        let src: Vec<f32> = (0..32).map(|_| rng.next() * 0.5).collect();
        // dead zone: untouched
        let (mut l, mut r) = (src.clone(), src.clone());
        mk(0.5, false).process(&mut l, &mut r, 0.0, 120.0);
        assert_eq!(l, src);
        // a centre signal at full cut ends up quieter than at full focus (after settling), and
        // inverting swaps them
        let level = |v: f32, inv: bool| {
            let mut fx = mk(v, inv);
            let mut acc = 0.0f64;
            let x = tone(1000.0, 44_100, 0);
            for (i, ch) in x.chunks(32).enumerate() {
                let (mut l, mut r) = (ch.to_vec(), ch.to_vec());
                fx.process(&mut l, &mut r, (i * 32) as f64, 120.0);
                if i * 32 > 20_000 {
                    acc += l.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>();
                }
            }
            acc
        };
        assert!(level(-100.0, false) < level(100.0, false) * 1e-3);
        assert!(level(100.0, true) < level(-100.0, true) * 1e-3);
        assert!((level(-100.0, false) - level(100.0, true)).abs() <= level(100.0, true).max(1e-9));
    }
}
