//! The Rust Vocal filter against the device's C DSP.
//!
//! `vocal_ref/golden.f32` is the output of `device/vocalfx/vocal_dsp.c` (float build, built by
//! `vocal_ref/gen.sh` from `vocal_ref/vocal_ref.c`) for `vocal_ref/scenario.txt`: engage and
//! release edges, the hysteresis band, both extremes, a sweep, NaN / out-of-range values,
//! garbage samples in the input, a hot input, silence while engaged, odd block sizes and the
//! inverted flag. This test regenerates the same input (it uses only IEEE +, - and *, so it is
//! bit-identical in both languages), runs `pm_mixer::vocal` through the same scenario and
//! compares every output sample.

use pm_formats::mix::Node;
use pm_mixer::env::Env;
use pm_mixer::fx::Fx;
use pm_mixer::vocal::{self, Coefs, State, VocalFx};

const SCENARIO: &str = include_str!("vocal_ref/scenario.txt");
const GOLDEN: &[u8] = include_bytes!("vocal_ref/golden.f32");

/// Digital resonator `y[n+1] = c*y[n] - y[n-1]` (the same as `osc_t` in vocal_ref.c).
struct Osc {
    c: f64,
    y1: f64,
    y2: f64,
}

impl Osc {
    fn new(c: f64, sinw: f64) -> Osc {
        Osc { c, y1: 0.0, y2: -sinw }
    }

    fn next(&mut self) -> f64 {
        let out = self.y1;
        let nx = self.c * self.y1 - self.y2;
        self.y2 = self.y1;
        self.y1 = nx;
        out
    }
}

/// The device's test LCG (`frand()` in dsp_test.c).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.0 >> 8) & 0xffff) as f32 / 32768.0 - 1.0
    }
}

struct Seg {
    value: f32,
    frames: usize,
    block: usize,
    gain: f32,
    silent: bool,
    inject: bool,
}

struct Pass {
    invert: bool,
    segs: Vec<Seg>,
}

fn parse() -> Vec<Pass> {
    let mut passes: Vec<Pass> = Vec::new();
    for line in SCENARIO.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().take_while(|w| !w.starts_with('#')).collect();
        if f[0] == "pass" {
            passes.push(Pass { invert: f[1] == "1", segs: Vec::new() });
        } else {
            passes.last_mut().expect("a segment before the first pass").segs.push(Seg {
                value: f[0].parse().unwrap(),
                frames: f[1].parse().unwrap(),
                block: f[2].parse().unwrap(),
                gain: f[3].parse().unwrap(),
                silent: f[4] == "1",
                inject: f[5] == "1",
            });
        }
    }
    passes
}

/// Runs the scenario through the Rust port; interleaved L, R.
fn run() -> Vec<f32> {
    let mut out = Vec::new();
    for pass in parse() {
        let coefs = Coefs::design(44_100.0);
        let mut st = State::new();
        let mut rng = Noise(12345);
        let mut o220 = Osc::new(1.9990175909339492, 0.03133955622145082);
        let mut o1000 = Osc::new(1.9797349455598832, 0.14199431795762676);
        let mut o60 = Osc::new(1.9999269227133345, 0.008548447320595277);
        let mut o3000 = Osc::new(1.8200702223285332, 0.4145311766902954);
        let mut o440 = Osc::new(1.99607132886337, 0.06264832417874368);
        let mut eng = false;
        for seg in &pass.segs {
            let mut off = 0;
            while off < seg.frames {
                let n = (seg.frames - off).min(seg.block);
                let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
                for i in 0..n {
                    let rel = off + i;
                    let c = 0.2 * o220.next() + 0.15 * o1000.next() + 0.1 * o60.next() + 0.08 * o3000.next();
                    let sd = 0.15 * f64::from(rng.next()) + 0.1 * o440.next();
                    let (mut lv, mut rv) = (((c + sd) as f32) * seg.gain, ((c - sd) as f32) * seg.gain);
                    if seg.silent {
                        lv = 0.0;
                        rv = 0.0;
                    }
                    if seg.inject {
                        match rel {
                            10 => lv = f32::NAN,
                            11 => rv = f32::INFINITY,
                            12 => lv = -1e5,
                            13 => rv = 1e30,
                            _ => {}
                        }
                    }
                    l[i] = lv;
                    r[i] = rv;
                }
                let (target, engaged) = vocal::wah_target(seg.value, eng, pass.invert);
                eng = engaged;
                vocal::process(&coefs, &mut st, &mut l, &mut r, target, engaged);
                for i in 0..n {
                    out.push(l[i]);
                    out.push(r[i]);
                }
                off += n;
            }
        }
    }
    out
}

fn golden() -> Vec<f32> {
    let (words, rest) = GOLDEN.as_chunks::<4>();
    assert!(rest.is_empty());
    words.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

#[test]
fn rust_port_matches_the_device_c_dsp() {
    let want = golden();
    let got = run();
    assert_eq!(got.len(), want.len(), "frame count differs from the golden file");
    assert!(want.iter().all(|v| v.is_finite()), "the C output holds a non-finite sample");
    let mut max_err = 0.0f64;
    let mut worst = 0;
    let mut bit_diff = 0usize;
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        if g.to_bits() != w.to_bits() {
            bit_diff += 1;
        }
        let e = (f64::from(*g) - f64::from(*w)).abs();
        if e > max_err {
            max_err = e;
            worst = i;
        }
    }
    println!("vocal C vs Rust: {} samples, max abs error {max_err:e} (at sample {worst}), {bit_diff} samples differ in bits", got.len());
    assert!(max_err <= 1e-6, "max abs error {max_err:e} at sample {worst}: rust {} vs C {}", got[worst], want[worst]);
    // the port is meant to be the same arithmetic, not just close
    assert_eq!(bit_diff, 0, "not bit-identical: {bit_diff} samples differ (max error {max_err:e}); a libm difference in a coefficient would show here");
}

#[test]
fn the_scenario_exercises_what_it_claims_to() {
    let passes = parse();
    assert_eq!(passes.len(), 2);
    assert!(passes[0].segs.iter().any(|s| s.value.is_nan()) && passes[0].segs.iter().any(|s| s.value == 250.0));
    assert!(passes[0].segs.iter().any(|s| s.inject) && passes[0].segs.iter().any(|s| s.silent));
    assert!(passes[0].segs.iter().any(|s| s.block == 129 || s.block == 1));
    assert!(passes[1].invert && !passes[0].invert);
    assert!(GOLDEN.len() < 300 * 1024);
}

/// The tool effect reads the envelope at each tick centre and maps it like the device maps one
/// value per block: driving `VocalFx` over an envelope must equal driving `process` by hand.
#[test]
fn the_tool_effect_is_the_dsp_plus_the_mapping_at_tick_centres() {
    let nodes = vec![
        Node { pos: 0, value: 0.0 },
        Node { pos: 4_000, value: 0.9 },
        Node { pos: 8_000, value: 100.0 },
        Node { pos: 14_000, value: 0.6 },
        Node { pos: 18_000, value: -100.0 },
        Node { pos: 24_000, value: 0.0 },
    ];
    let env = Env::new(&nodes, 0.0);
    for invert in [false, true] {
        let mut fx = VocalFx::new(env.clone(), invert);
        let coefs = Coefs::design(44_100.0);
        let mut st = State::new();
        let mut eng = false;
        let mut rng = Noise(77);
        let total = 26_000;
        let (mut a_l, mut a_r) = (Vec::new(), Vec::new());
        let (mut b_l, mut b_r) = (Vec::new(), Vec::new());
        let mut pos = 0;
        while pos < total {
            let n = (total - pos).min(pm_mixer::fx::TICK);
            let src_l: Vec<f32> = (0..n).map(|_| rng.next() * 0.5).collect();
            let src_r: Vec<f32> = (0..n).map(|_| rng.next() * 0.5).collect();
            let (mut l, mut r) = (src_l.clone(), src_r.clone());
            fx.process(&mut l, &mut r, pos as f64, 120.0);
            a_l.extend_from_slice(&l);
            a_r.extend_from_slice(&r);
            let (mut l2, mut r2) = (src_l, src_r);
            let v = env.at(pos as f64 + n as f64 / 2.0, 0.0);
            let (target, engaged) = vocal::wah_target(v, eng, invert);
            eng = engaged;
            vocal::process(&coefs, &mut st, &mut l2, &mut r2, target, engaged);
            b_l.extend_from_slice(&l2);
            b_r.extend_from_slice(&r2);
            pos += n;
        }
        assert_eq!(a_l.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), b_l.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
        assert_eq!(a_r.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), b_r.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
    }
}
