//! Engine tests on synthetic audio: generated clicks and sines written as float WAV files into a
//! temp directory, mixed with known positions and tempo changes, and measured. No real music,
//! no device.

use std::f32::consts::PI;
use std::path::PathBuf;

use pm_formats::mix::{Effect, EffectKind, IgnoreProc, LoopProc, Mix, Node, Position, Process, ReverseProc, SpeedNode, Tool, Track, Transition};
use pm_mixer::{engine, Options, RATE};

const SR: usize = RATE as usize;

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pm-mixer-test-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 32-bit float stereo WAV.
fn write_wav(path: &std::path::Path, l: &[f32], r: &[f32]) {
    let n = l.len();
    let mut b = Vec::new();
    let data = (n * 8) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&(SR as u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..n {
        b.extend_from_slice(&l[i].to_le_bytes());
        b.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

fn sine(freq: f32, secs: f32, amp: f32) -> Vec<f32> {
    (0..(secs * SR as f32) as usize).map(|i| amp * (2.0 * PI * freq * i as f32 / SR as f32).sin()).collect()
}

/// Kick-like bursts (80 Hz decaying cosine with a short 3 kHz tick) on every beat.
fn clicks(bpm: f64, secs: f64) -> Vec<f32> {
    let n = (secs * SR as f64) as usize;
    let mut v = vec![0.0f32; n];
    let beat = 60.0 / bpm * SR as f64;
    let mut k = 0;
    loop {
        let at = (k as f64 * beat).round() as usize;
        if at >= n {
            break;
        }
        for i in 0..3000.min(n - at) {
            let t = i as f32 / SR as f32;
            let body = (2.0 * PI * 80.0 * t).cos() * (-(i as f32) / 500.0).exp() * 0.7;
            let tick = if i < 40 { (2.0 * PI * 3000.0 * t).cos() * 0.25 } else { 0.0 };
            v[at + i] += body + tick;
        }
        k += 1;
    }
    v
}

fn pos(start: f64, length: f64) -> Position {
    Position { start, length, limit_size: true, ..Position::default() }
}

fn tpos(start: f64, length: f64) -> Position {
    Position { start, length, ..Position::default() }
}

/// A track playing `frames` source frames of `file` (in `d`) at `speed` times the tempo.
fn track(file: &str, frames: usize, bpm: f64, speed: f32, start: f64) -> Track {
    Track::new(1, file, bpm, speed, pos(start, frames as f64 / f64::from(speed)))
}

fn put(d: &std::path::Path, name: &str, v: &[f32]) -> String {
    write_wav(&d.join(name), v, v);
    name.to_string()
}

fn mix_of(tracks: Vec<Track>) -> Mix {
    let mut m = Mix::new("test");
    m.tracks = tracks;
    m
}

fn render(d: &std::path::Path, m: &Mix, o: &Options) -> (Vec<f32>, Vec<f32>) {
    let (l, r, _) = engine::render_to_vecs(m, &engine::dir_resolver(d), o).unwrap();
    (l, r)
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

/// Onset frames: first sample over `thr` after a quiet stretch of `gap` frames.
fn onsets(v: &[f32], thr: f32, gap: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < v.len() {
        if v[i].abs() > thr {
            out.push(i);
            i += gap;
        } else {
            i += 1;
        }
    }
    out
}


fn crossing_freq(v: &[f32]) -> f64 {
    let mut first = None;
    let mut last = 0.0;
    let mut n = 0usize;
    for i in 1..v.len() {
        if v[i - 1] <= 0.0 && v[i] > 0.0 {
            let at = (i - 1) as f64 + f64::from(-v[i - 1]) / f64::from(v[i] - v[i - 1]);
            if first.is_none() {
                first = Some(at);
            }
            last = at;
            n += 1;
        }
    }
    match first {
        Some(f0) if n > 1 => (n - 1) as f64 * SR as f64 / (last - f0),
        _ => 0.0,
    }
}

fn tool(effect: Effect, start: f64, length: f64, values: &[(&str, f32)]) -> Process {
    let mut t = Tool::flat(effect.clone(), start, length, 0.0, "t".into());
    for (k, v) in values {
        t.automation.insert(format!("{}-{k}", effect.name), pm_formats::mix::Envelope::flat(length, *v));
    }
    Process::Tool(t)
}

// ---------------------------------------------------------------------------------------------
// timing and stretch
// ---------------------------------------------------------------------------------------------

#[test]
fn unit_speed_is_a_bit_exact_copy_with_the_right_length() {
    let d = dir("identity");
    let src = sine(440.0, 3.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0)]);
    let (l, _) = render(&d, &m, &Options::default());
    assert_eq!(l.len(), src.len());
    // away from the 32-sample edges the limiter is transparent: equal to the source
    for i in 100..l.len() - 100 {
        assert!((l[i] - src[i]).abs() < 1e-6, "sample {i}");
    }
}

#[test]
fn clip_start_offsets_the_audio_on_the_timeline() {
    let d = dir("offset");
    let src = sine(440.0, 1.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let start = 2.0 * SR as f64;
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, start)]);
    let (l, _) = render(&d, &m, &Options::default());
    assert_eq!(l.len(), 3 * SR);
    assert!(rms(&l[..2 * SR - 10]) < 1e-9, "silence before the clip");
    assert!(rms(&l[2 * SR + 100..3 * SR - 100]) > 0.3);
    // export mode starts at the first audible frame
    let (l2, _) = render(&d, &m, &Options { skip_leading_silence: true, ..Options::default() });
    assert_eq!(l2.len(), SR);
}

#[test]
fn trims_cut_the_audible_part_but_keep_the_clip_position() {
    let d = dir("trim");
    let src = clicks(120.0, 4.0);
    let f = put(&d, "a.wav", &src);
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    t.position.l_offset = SR as f64; // first second hidden
    t.position.r_offset = SR as f64; // last second hidden
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    assert_eq!(l.len(), 3 * SR);
    // click at 1.0 s of the file lands at 1.0 s on the timeline
    let on = onsets(&l[SR - 100..], 0.3, SR / 4);
    assert!((on[0] as i64 + SR as i64 - 100 - SR as i64).abs() < 40, "{on:?}");
}

fn check_onsets(bpm: f64, curve: &[SpeedNode], tol_ms: f64) {
    let d = dir(&format!("onsets{}", curve.len()));
    let secs = 24.0;
    let src = clicks(bpm, secs);
    let f = put(&d, "a.wav", &src);
    let mut t = track(&f, src.len(), bpm, 1.0, 0.0);
    t.pitch = curve.to_vec();
    let tm = pm_formats::mix::TempoMap::new(curve);
    t.position.length = tm.local_at(src.len() as f64);
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    let got = onsets(&l, 0.25, SR / 4);
    let beat = 60.0 / bpm * SR as f64;
    let n_expected = (secs * bpm / 60.0) as usize - 1;
    assert!(got.len() >= n_expected, "found {} of {n_expected} onsets", got.len());
    let mut worst = 0.0f64;
    for (k, g) in got.iter().take(n_expected).enumerate() {
        let want = tm.local_at((k as f64 * beat).round());
        let err = (*g as f64 - want) / SR as f64 * 1000.0;
        worst = worst.max(err.abs());
    }
    println!("worst onset error {worst:.2} ms over {n_expected} beats");
    assert!(worst < tol_ms, "worst onset error {worst:.2} ms");
}

#[test]
fn beats_stay_on_the_grid_after_a_constant_stretch() {
    check_onsets(120.0, &[SpeedNode { position: 0, speed: 1.06 }], 4.0);
    check_onsets(128.0, &[SpeedNode { position: 0, speed: 0.93 }], 4.0);
}

#[test]
fn beats_follow_a_tempo_ramp() {
    // 1.00 -> 1.10 over the first 8 s, then flat
    check_onsets(120.0, &[SpeedNode { position: 0, speed: 1.0 }, SpeedNode { position: 8 * SR as u32, speed: 1.1 }], 5.0);
}

#[test]
fn stretch_preserves_pitch_and_vinyl_does_not() {
    let d = dir("pitch");
    let src = sine(440.0, 6.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let t = track(&f, src.len(), 120.0, 1.25, 0.0);
    let (l, _) = render(&d, &mix_of(vec![t.clone()]), &Options::default());
    assert!((l.len() as f64 - src.len() as f64 / 1.25).abs() < 2.0);
    let fr = crossing_freq(&l[SR..l.len() - SR]);
    assert!((fr - 440.0).abs() < 2.0, "stretched sine at {fr} Hz");

    let mut v = t;
    v.non_interpolated = true;
    let (l, _) = render(&d, &mix_of(vec![v]), &Options::default());
    let fr = crossing_freq(&l[SR..l.len() - SR]);
    assert!((fr - 550.0).abs() < 3.0, "vinyl sine at {fr} Hz");
}

#[test]
fn key_envelope_shifts_pitch_without_changing_length() {
    let d = dir("key");
    let src = sine(440.0, 6.0, 0.5);
    let f = put(&d, "a.wav", &src);
    for (cents, want) in [(1200.0f32, 880.0), (-1200.0, 220.0), (100.0, 466.16)] {
        let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
        t.key = vec![pm_formats::mix::KeyNode { position: 0, cents, flags: 1 }];
        let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
        assert!((l.len() as f64 - src.len() as f64).abs() < 2.0, "length {} vs {}", l.len(), src.len());
        let fr = crossing_freq(&l[SR..l.len() - SR]);
        assert!((fr - want).abs() < want * 0.01, "{cents} cents gave {fr} Hz, wanted {want}");
    }
    // key shift combined with a tempo change
    let mut t = track(&f, src.len(), 120.0, 1.1, 0.0);
    t.key = vec![pm_formats::mix::KeyNode { position: 0, cents: 200.0, flags: 1 }];
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    let fr = crossing_freq(&l[SR..l.len() - SR]);
    assert!((fr - 493.88).abs() < 5.0, "{fr}");
}

// ---------------------------------------------------------------------------------------------
// transitions and master
// ---------------------------------------------------------------------------------------------

#[test]
fn crossfade_keeps_the_energy_continuous() {
    let d = dir("xfade");
    let a = sine(300.0, 10.0, 0.5);
    let b = sine(471.0, 10.0, 0.5);
    let fa = put(&d, "a.wav", &a);
    let fb = put(&d, "b.wav", &b);
    let ta = track(&fa, a.len(), 120.0, 1.0, 0.0);
    let tb = track(&fb, b.len(), 120.0, 1.0, 6.0 * SR as f64);
    let mut m = mix_of(vec![ta, tb]);
    m.transitions.push(Transition::new(tpos(6.0 * SR as f64, 4.0 * SR as f64)));
    let (l, _) = render(&d, &m, &Options::default());
    assert_eq!(l.len(), 16 * SR);
    let win = SR / 5;
    let solo = rms(&l[2 * SR..2 * SR + win]);
    for k in 0..20 {
        let at = 6 * SR + k * (4 * SR - win) / 19;
        let e = rms(&l[at..at + win]);
        assert!((db(e) - db(solo)).abs() < 1.0, "energy at {k}/20 of the fade: {:.2} dB vs {:.2} dB", db(e), db(solo));
    }
    // before the fade only A, after only B
    assert!(rms(&l[5 * SR..6 * SR - 100]) > 0.3);
    let after = &l[11 * SR..12 * SR];
    assert!((crossing_freq(after) - 471.0).abs() < 2.0);
}

#[test]
fn master_never_clips_and_output_is_finite() {
    let d = dir("loud");
    let a = sine(100.0, 6.0, 0.95);
    let f = put(&d, "a.wav", &a);
    let tracks: Vec<Track> = (0..3).map(|i| track(&f, a.len(), 120.0, 1.0, i as f64 * 1000.0)).collect();
    let (l, r) = render(&d, &mix_of(tracks), &Options::default());
    assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 0.98 + 1e-6));
    assert!(l.iter().any(|x| x.abs() > 0.9), "limiter should not squash the mix to nothing");
}

#[test]
fn render_is_deterministic() {
    let d = dir("det");
    let src = clicks(124.0, 8.0);
    let f = put(&d, "a.wav", &src);
    let mut t = track(&f, src.len(), 124.0, 1.04, 0.0);
    t.key = vec![pm_formats::mix::KeyNode { position: 0, cents: 150.0, flags: 1 }];
    t.processes.push(tool(Effect::reverb(), 2.0 * SR as f64, SR as f64, &[("Time", 40.0), ("Wet/Dry", 50.0)]));
    t.processes.push(tool(Effect::echo(), 4.0 * SR as f64, SR as f64, &[("Time ", -1.0), ("Wet/Dry", 50.0)]));
    let m = mix_of(vec![t]);
    let a = render(&d, &m, &Options::default());
    let b = render(&d, &m, &Options::default());
    assert!(a.0.iter().zip(&b.0).all(|(x, y)| x.to_bits() == y.to_bits()));
    assert!(a.1.iter().zip(&b.1).all(|(x, y)| x.to_bits() == y.to_bits()));
}

#[test]
fn a_preview_window_matches_the_full_render() {
    let d = dir("window");
    let src = sine(330.0, 10.0, 0.4);
    let f = put(&d, "a.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0)]);
    let (full, _) = render(&d, &m, &Options::default());
    let (win, _) = render(&d, &m, &Options::window_secs(3.0, 5.0));
    assert_eq!(win.len(), 2 * SR);
    for i in 0..win.len() {
        assert!((win[i] - full[3 * SR + i]).abs() < 1e-6);
    }
    // a window in the middle of a stretched clip is still pitch-correct and on the right beat
    let src = clicks(120.0, 20.0);
    let f = put(&d, "b.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.05, 0.0)]);
    let (win, _) = render(&d, &m, &Options::window_secs(8.0, 12.0));
    let on = onsets(&win, 0.25, SR / 4);
    assert!(on.len() >= 7);
    for w in on.windows(2) {
        let beat = (w[1] - w[0]) as f64 / SR as f64;
        assert!((beat - 0.5 / 1.05).abs() < 0.006, "beat {beat}");
    }
}

// ---------------------------------------------------------------------------------------------
// tools
// ---------------------------------------------------------------------------------------------

/// RMS of the same tone with and without a tool across `[1 s, 3 s)` of a 4 s clip.
fn tool_gain_db(freq_hz: f32, effect: Effect, values: &[(&str, f32)]) -> f64 {
    tool_gain_db_with(&Options::default(), freq_hz, effect, values)
}

fn tool_gain_db_with(opts: &Options, freq_hz: f32, effect: Effect, values: &[(&str, f32)]) -> f64 {
    let d = dir("tool");
    let src = sine(freq_hz, 4.0, 0.3);
    let f = put(&d, &format!("t{freq_hz}.wav"), &src);
    let plain = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0)]);
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    t.processes.push(tool(effect, SR as f64, 2.0 * SR as f64, values));
    let (a, _) = render(&d, &plain, opts);
    let (b, _) = render(&d, &mix_of(vec![t]), opts);
    let w = |v: &[f32]| rms(&v[SR + SR / 2..2 * SR + SR / 2]);
    db(w(&b)) - db(w(&a))
}

#[test]
fn eq_kills_a_band_and_leaves_the_others() {
    assert!(tool_gain_db(50.0, Effect::eq(), &[("Lo", -100.0), ("Mid", 0.0), ("Hi", 0.0)]) < -35.0);
    assert!(tool_gain_db(1000.0, Effect::eq(), &[("Lo", -100.0), ("Mid", 0.0), ("Hi", 0.0)]).abs() < 1.0);
    assert!(tool_gain_db(9000.0, Effect::eq(), &[("Lo", -100.0), ("Mid", 0.0), ("Hi", 0.0)]).abs() < 1.0);
    assert!(tool_gain_db(1000.0, Effect::eq(), &[("Lo", 0.0), ("Mid", -100.0), ("Hi", 0.0)]) < -25.0);
    assert!(tool_gain_db(9000.0, Effect::eq(), &[("Lo", 0.0), ("Mid", 0.0), ("Hi", -100.0)]) < -35.0);
    assert!(tool_gain_db(80.0, Effect::eq(), &[("Lo", 0.0), ("Mid", 0.0), ("Hi", -100.0)]).abs() < 1.0);
    // flat EQ is transparent
    assert!(tool_gain_db(500.0, Effect::eq(), &[("Lo", 0.0), ("Mid", 0.0), ("Hi", 0.0)]).abs() < 0.05);
    // half cut = -12 dB
    let half = tool_gain_db(80.0, Effect::eq(), &[("Lo", -50.0), ("Mid", 0.0), ("Hi", 0.0)]);
    assert!((half + 12.0).abs() < 1.5, "{half}");
}

#[test]
fn wah_tool_follows_the_peq_mode() {
    use pm_mixer::PeqMode;
    let with = |peq| Options { peq, ..Options::default() };
    let wah = |peq, v: f32| tool_gain_db_with(&with(peq), 1000.0, Effect::wah(), &[("CutOff", v)]);
    // vocal: negative cuts a centred 1 kHz tone, positive keeps it (about +2.9 dB)
    assert!(wah(PeqMode::Vocal, -100.0) < -30.0, "{}", wah(PeqMode::Vocal, -100.0));
    let keep = wah(PeqMode::Vocal, 100.0);
    assert!(keep > 1.5 && keep < 4.0, "{keep}");
    // inverted swaps the two directions
    assert!(wah(PeqMode::VocalInverted, 100.0) < -30.0);
    assert!(wah(PeqMode::VocalInverted, -100.0) > 1.5);
    // the neutral position of the vocal filter is transparent (the stock Wah stand-in is not)
    for m in [PeqMode::Vocal, PeqMode::VocalInverted] {
        assert!(wah(m, 0.0).abs() < 1e-6, "{m:?}");
    }
    // the default stays the stock Wah stand-in, which is not the vocal filter
    assert_eq!(Options::default().peq, PeqMode::Wah);
    assert_eq!(tool_gain_db(1000.0, Effect::wah(), &[("CutOff", -100.0)]), wah(PeqMode::Wah, -100.0));
    assert!(wah(PeqMode::Wah, -100.0) > -30.0);
}

/// A window that starts inside a tool sounds like the same stretch of the full render: the lead-in
/// the engine renders and drops has settled the Vocal filter (it cold-starts with a ~10 ms fade-in
/// from the dry signal) before the first kept frame.
#[test]
fn a_window_starting_inside_a_vocal_tool_matches_the_full_render() {
    use pm_mixer::PeqMode;
    let d = dir("vocalwindow");
    let src = sine(1000.0, 5.0, 0.3);
    let f = put(&d, "v.wav", &src);
    for peq in [PeqMode::Vocal, PeqMode::VocalInverted] {
        let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
        t.processes.push(tool(Effect::wah(), SR as f64, 3.0 * SR as f64, &[("CutOff", -100.0)]));
        let m = mix_of(vec![t]);
        let full_opts = Options { peq, ..Options::default() };
        let (full, _) = render(&d, &m, &full_opts);
        // start well inside the tool, a few frames past a block boundary, and just after its start
        for from in [1.5f64, 2.0, 2.0 + 37.0 / SR as f64, 1.2] {
            let (win, _) = render(&d, &m, &Options { peq, ..Options::window_secs(from, from + 1.0) });
            let at = (from * SR as f64).round() as usize;
            assert_eq!(win.len(), SR);
            let worst = win.iter().zip(&full[at..]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 1e-3, "{peq:?} from {from}: window differs from the full render by {worst}");
        }
    }
}

#[test]
fn filter_sweeps_low_pass_and_high_pass() {
    assert!(tool_gain_db(8000.0, Effect::filter(), &[("CutOff", -70.0)]) < -30.0);
    assert!(tool_gain_db(60.0, Effect::filter(), &[("CutOff", -70.0)]).abs() < 1.5);
    assert!(tool_gain_db(60.0, Effect::filter(), &[("CutOff", 70.0)]) < -30.0);
    assert!(tool_gain_db(8000.0, Effect::filter(), &[("CutOff", 70.0)]).abs() < 1.5);
    assert!(tool_gain_db(500.0, Effect::filter(), &[("CutOff", 0.0)]).abs() < 0.01);
}

#[test]
fn every_tool_renders_finite_audio_and_changes_the_signal() {
    let d = dir("alltools");
    let src = clicks(124.0, 10.0);
    let f = put(&d, "a.wav", &src);
    let effects: Vec<(Effect, Vec<(&str, f32)>)> = vec![
        (Effect::reverb(), vec![("Time", 50.0), ("Wet/Dry", 60.0)]),
        (Effect::echo(), vec![("Time ", -1.0), ("Wet/Dry", 60.0)]),
        (Effect::roll(), vec![("Time ", -1.0), ("Wet/Dry", 70.0)]),
        (Effect::wah(), vec![("CutOff", 20.0)]),
        (Effect::crush(), vec![("Crush", 70.0)]),
        (Effect { name: "Trans".into(), bypassed: false, kind: EffectKind::Trans { start_bpm: 124.0, active: true } }, vec![]),
        (Effect::volume(), vec![("Volume", -12.0)]),
    ];
    let (plain, _) = render(&d, &mix_of(vec![track(&f, src.len(), 124.0, 1.0, 0.0)]), &Options::default());
    for (e, v) in effects {
        let name = e.name.clone();
        let mut t = track(&f, src.len(), 124.0, 1.0, 0.0);
        t.processes.push(tool(e, 2.0 * SR as f64, 3.0 * SR as f64, &v));
        let (l, r) = render(&d, &mix_of(vec![t]), &Options::default());
        assert_eq!(l.len(), plain.len(), "{name}");
        assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() <= 0.98 + 1e-6), "{name} not finite or clipping");
        // untouched before the tool, different inside it
        assert!(l[..2 * SR - 200].iter().zip(&plain).all(|(a, b)| (a - b).abs() < 1e-5), "{name} leaks before its start");
        let diff: f64 = l[2 * SR..5 * SR].iter().zip(&plain[2 * SR..5 * SR]).map(|(a, b)| f64::from((a - b).abs())).sum();
        assert!(diff > 1.0, "{name} did nothing ({diff})");
    }
}

#[test]
fn reverb_and_echo_ring_on_after_the_tool_ends() {
    let d = dir("tails");
    let mut src = vec![0.0f32; 6 * SR];
    for i in 0..400 {
        src[SR + i] = 0.8 * (1.0 - i as f32 / 400.0); // a burst inside the tool
    }
    let f = put(&d, "a.wav", &src);
    for (e, v) in [
        (Effect::reverb(), vec![("Time", 60.0), ("Wet/Dry", 100.0)]),
        (Effect::echo(), vec![("Time ", 0.0), ("Wet/Dry", 100.0)]),
    ] {
        let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
        t.processes.push(tool(e.clone(), SR as f64 / 2.0, 1.2 * SR as f64, &v));
        let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
        // the tool ends at 1.7 s; the burst reverberates / echoes past it
        let tail = rms(&l[(1.75 * SR as f32) as usize..3 * SR]);
        assert!(tail > 1e-4, "{}: no tail after the tool ({tail})", e.name);
        assert!(l.iter().all(|x| x.is_finite()));
        // and dies out
        assert!(rms(&l[5 * SR..]) < tail, "{}: tail does not decay", e.name);
    }
}

#[test]
fn crush_quantises() {
    let d = dir("crush");
    let src = sine(300.0, 3.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    t.processes.push(tool(Effect::crush(), 0.0, 3.0 * SR as f64, &[("Crush", 100.0)]));
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    let mid = &l[SR..2 * SR];
    let mut levels: Vec<i64> = mid.iter().map(|x| (x * 1e5).round() as i64).collect();
    levels.sort_unstable();
    levels.dedup();
    assert!(levels.len() < 120, "{} distinct levels", levels.len());
}

// ---------------------------------------------------------------------------------------------
// loops, reverse, ignore, gain
// ---------------------------------------------------------------------------------------------

fn ramp(secs: f32) -> Vec<f32> {
    let n = (secs * SR as f32) as usize;
    (0..n).map(|i| 0.8 * (i as f32 / n as f32)).collect()
}

#[test]
fn reverse_ignore_and_loop_edit_the_clip_audio() {
    let d = dir("edits");
    let src = ramp(8.0);
    let f = put(&d, "a.wav", &src);
    let at = |s: f32| (s * SR as f32) as usize;

    // reverse [2 s, 3 s)
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    t.processes.push(Process::Reverse(ReverseProc { name: "REVERSE".into(), bypassed: false, position: tpos(at(2.0) as f64, SR as f64) }));
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    for k in [200usize, 20_000, 40_000] {
        let t_ = at(2.0) + k;
        let want = src[at(2.0) + at(1.0) - 1 - k];
        assert!((l[t_] - want).abs() < 1e-5, "reverse at {k}");
    }
    assert!((l[at(3.0) + 500] - src[at(3.0) + 500]).abs() < 1e-6, "audio after the region is untouched");

    // ignore [4 s, 5 s)
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    t.processes.push(Process::Ignore(IgnoreProc { name: "IGNORE".into(), bypassed: false, position: tpos(at(4.0) as f64, SR as f64), start_pos: 0 }));
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    assert!(rms(&l[at(4.0) + 200..at(5.0) - 200]) < 1e-6);
    assert!((l[at(5.0) + 500] - src[at(5.0) + 500]).abs() < 1e-6);

    // loop: the slice [3 s, 3.5 s) repeated 3 times from 3 s
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    let one = 0.5 * SR as f64;
    t.processes.push(Process::Loop(LoopProc {
        name: "LOOP".into(),
        bypassed: false,
        position: tpos(at(3.0) as f64, 3.0 * one),
        length: one,
        iterations: 3,
        re_loop: false,
        mode: false,
        parent: None,
        offsets: vec![],
    }));
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    for k in [100usize, 5000, 20000] {
        for rep in 1..3 {
            let a = l[at(3.0) + rep * at(0.5) + k];
            assert!((a - src[at(3.0) + k]).abs() < 1e-6, "loop repeat {rep} at {k}");
        }
    }
    assert!((l[at(4.5) + 1000] - src[at(4.5) + 1000]).abs() < 1e-6, "audio resumes after the loop");
}

#[test]
fn gain_envelope_is_applied_in_db() {
    let d = dir("gain");
    let src = sine(500.0, 4.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let mut t = track(&f, src.len(), 120.0, 1.0, 0.0);
    // -6 dB from 1 s on, ramping from 0 dB over the first second
    t.volume.automation.insert(
        "Volume-Volume".into(),
        pm_formats::mix::Envelope { nodes: vec![Node { pos: 0, value: 0.0 }, Node { pos: SR as u32, value: -6.0 }] },
    );
    let (l, _) = render(&d, &mix_of(vec![t]), &Options::default());
    let g = db(rms(&l[2 * SR..3 * SR])) - db(rms(&src[2 * SR..3 * SR]));
    assert!((g + 6.0).abs() < 0.05, "{g}");
    let g0 = db(rms(&l[100..2000])) - db(rms(&src[100..2000]));
    assert!(g0.abs() < 0.2, "{g0}");
}

// ---------------------------------------------------------------------------------------------
// files
// ---------------------------------------------------------------------------------------------

#[test]
fn wav_and_ogg_export_roundtrip() {
    let d = dir("files");
    let src = sine(440.0, 5.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0)]);
    let res = engine::dir_resolver(&d);
    for (fmt, name) in [(pm_mixer::Format::Wav, "out.wav"), (pm_mixer::Format::Ogg, "out.ogg")] {
        let dest = d.join(name);
        let mut last = 0.0;
        let out = pm_mixer::render_to_file(&m, &res, &Options::default(), fmt, &dest, &mut |p| {
            last = p;
            true
        })
        .unwrap();
        assert_eq!(out.frames as usize, src.len());
        assert!((last - 1.0).abs() < 1e-9);
        assert!(!d.join(format!("{name}.partial")).exists());
        let back = pm_mixer::audio::load(&dest).unwrap();
        // the Ogg decoder may add or drop a few priming frames
        assert!((back.len() as i64 - src.len() as i64).abs() < 4096, "{name}: {} frames", back.len());
        let mid = &back.l[SR..back.len() - SR];
        assert!((crossing_freq(mid) - 440.0).abs() < 1.0, "{name}");
        assert!((rms(mid) - rms(&src[SR..src.len() - SR])).abs() < 0.02, "{name}");
    }
}

#[test]
fn cancelling_leaves_no_file() {
    let d = dir("cancel");
    let src = sine(440.0, 5.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0)]);
    let dest = d.join("c.wav");
    let e = pm_mixer::render_to_file(&m, &engine::dir_resolver(&d), &Options::default(), pm_mixer::Format::Wav, &dest, &mut |p| p < 0.3);
    assert!(matches!(e, Err(pm_mixer::Error::Cancelled)));
    assert!(!dest.exists() && !d.join("c.wav.partial").exists());
}

#[test]
fn a_missing_file_is_a_warning_not_a_failure() {
    let d = dir("missing");
    let src = sine(440.0, 2.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0), track("nope.wav", src.len(), 120.0, 1.0, 0.0)]);
    let (l, _, out) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options::default()).unwrap();
    assert_eq!(l.len(), src.len());
    assert_eq!(out.warnings.len(), 1);
    assert!(out.warnings[0].contains("clip 2"));
}

#[test]
fn empty_ranges_are_errors() {
    let d = dir("empty");
    let src = sine(440.0, 2.0, 0.5);
    let f = put(&d, "a.wav", &src);
    let m = mix_of(vec![track(&f, src.len(), 120.0, 1.0, 0.0)]);
    let res = engine::dir_resolver(&d);
    assert!(engine::render_to_vecs(&m, &res, &Options::window_secs(5.0, 6.0)).is_err());
    assert!(engine::render_to_vecs(&Mix::new("x"), &res, &Options::default()).is_err());
}

// ---------------------------------------------------------------------------------------------
// speed
// ---------------------------------------------------------------------------------------------

/// Run with `cargo test -p pm-mixer --release -- --ignored --nocapture`.
#[test]
#[ignore]
fn benchmark_stretched_mix_with_tools() {
    let d = dir("bench");
    let secs = 180.0;
    let src = clicks(126.0, secs);
    let f = put(&d, "a.wav", &src);
    let mut tracks = Vec::new();
    for i in 0..3 {
        let mut t = track(&f, src.len(), 126.0, 1.03, i as f64 * 120.0 * SR as f64 / 1.03);
        t.key = vec![pm_formats::mix::KeyNode { position: 0, cents: if i == 1 { 100.0 } else { 0.0 }, flags: 1 }];
        t.processes.push(tool(Effect::reverb(), 10.0 * SR as f64, 8.0 * SR as f64, &[("Time", 50.0), ("Wet/Dry", 40.0)]));
        t.processes.push(tool(Effect::eq(), 30.0 * SR as f64, 20.0 * SR as f64, &[("Lo", -100.0)]));
        t.processes.push(tool(Effect::echo(), 60.0 * SR as f64, 8.0 * SR as f64, &[("Time ", -1.0), ("Wet/Dry", 40.0)]));
        tracks.push(t);
    }
    let mut m = mix_of(tracks);
    for i in 0..2 {
        m.transitions.push(Transition::new(tpos((i as f64 * 120.0 + 100.0) * SR as f64 / 1.03, 20.0 * SR as f64 / 1.03)));
    }
    let res = engine::dir_resolver(&d);
    let t0 = std::time::Instant::now();
    let out = engine::render(&m, &res, &Options::default(), &mut |_| true, &mut |_, _| Ok(())).unwrap();
    let el = t0.elapsed().as_secs_f64();
    let audio = out.frames as f64 / SR as f64;
    println!("rendered {audio:.0} s of audio in {el:.2} s = {:.1}x real time", audio / el);
    assert!(audio / el > 10.0, "slower than 10x real time");
}
