//! Loudness in the engine: the meter on a real render, master and per-clip gain, the limiter's
//! effect on the readings, per-clip measurement, and the fades at the edges of a partial render.
//! Synthetic sines only, written as float WAV files into a temp directory.

use std::f32::consts::PI;
use std::path::{Path, PathBuf};

use pm_formats::mix::{Mix, Position, Track};
use pm_mixer::loudness::{self, Measure};
use pm_mixer::{engine, Options, RATE};

const SR: usize = RATE as usize;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pm-mixer-loud-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_wav(path: &Path, l: &[f32], r: &[f32]) {
    let n = l.len();
    let data = (n * 8) as u32;
    let mut b = Vec::new();
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

fn sine(freq: f32, secs: f32, dbfs: f32) -> Vec<f32> {
    let amp = 10f32.powf(dbfs / 20.0);
    (0..(secs * SR as f32) as usize).map(|i| amp * (2.0 * PI * freq * i as f32 / SR as f32).sin()).collect()
}

/// A clip of `frames` frames of `file` at unit speed placed at `start`.
fn clip(file: &str, frames: usize, start: f64) -> Track {
    let pos = Position { start, length: frames as f64, limit_size: true, ..Position::default() };
    Track::new(1, file, 120.0, 1.0, pos)
}

fn put(d: &Path, name: &str, v: &[f32]) -> String {
    write_wav(&d.join(name), v, v);
    name.to_string()
}

fn mix_of(tracks: Vec<Track>) -> Mix {
    let mut m = Mix::new("loudness test");
    m.tracks = tracks;
    m
}

fn measured(d: &Path, m: &Mix, o: Options) -> pm_mixer::Rendered {
    let o = Options { measure: true, ..o };
    engine::render_to_vecs(m, &engine::dir_resolver(d), &o).unwrap().2
}

fn lufs(m: Measure) -> f64 {
    m.lufs.expect("a loudness")
}

#[test]
fn a_render_reports_the_loudness_of_its_output() {
    let d = dir("report");
    let n = 20 * SR;
    let f = put(&d, "tone.wav", &sine(1000.0, 20.0, -23.0));
    let m = mix_of(vec![clip(&f, n, 0.0)]);
    let r = measured(&d, &m, Options::default());
    let out = r.loudness.expect("measured").output;
    // the EBU reference signal through the whole engine: -23 dBFS stereo sine = -23 LUFS
    assert!((lufs(out) + 23.0).abs() < 0.1, "{:?}", out);
    assert!((out.sample_peak_db + 23.0).abs() < 0.1, "{:?}", out);
    // nothing touches a signal this quiet, so the limiter input reads the same
    assert!((lufs(r.loudness.unwrap().pre_limiter) - lufs(out)).abs() < 0.001);
    // without `measure` nothing is reported
    let (_, _, plain) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options::default()).unwrap();
    assert!(plain.loudness.is_none());
}

#[test]
fn master_gain_moves_the_reading_by_exactly_that_many_db() {
    let d = dir("master");
    let f = put(&d, "tone.wav", &sine(1000.0, 20.0, -30.0));
    let m = mix_of(vec![clip(&f, 20 * SR, 0.0)]);
    let base = lufs(measured(&d, &m, Options::default()).loudness.unwrap().output);
    for g in [-6.0, 3.0, 9.5] {
        let r = measured(&d, &m, Options { master_gain_db: g, ..Options::default() });
        let got = lufs(r.loudness.unwrap().output);
        assert!((got - (base + g)).abs() < 0.01, "gain {g}: {got} vs {}", base + g);
    }
}

#[test]
fn clip_gain_applies_to_its_own_clip_only() {
    let d = dir("clipgain");
    let a = put(&d, "a.wav", &sine(1000.0, 20.0, -30.0));
    let b = put(&d, "b.wav", &sine(1000.0, 20.0, -30.0));
    // two clips one after the other: 20 s each
    let m = mix_of(vec![clip(&a, 20 * SR, 0.0), clip(&b, 20 * SR, 20.0 * SR as f64)]);
    let (l, _, _) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options { clip_gain_db: vec![0.0, 6.0], ..Options::default() }).unwrap();
    let rms = |v: &[f32]| (v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    let (first, second) = (rms(&l[SR..19 * SR]), rms(&l[21 * SR..39 * SR]));
    let diff = 20.0 * (second / first).log10();
    assert!((diff - 6.0).abs() < 0.01, "second clip is {diff} dB above the first");
    // a short list and a non-finite entry mean 0 dB
    let (l0, _, _) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options { clip_gain_db: vec![f64::NAN], ..Options::default() }).unwrap();
    let (l1, _, _) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options::default()).unwrap();
    assert_eq!(l0, l1);
}

#[test]
fn the_limiter_catches_what_the_gain_pushes_over_and_the_pre_limiter_reading_shows_it() {
    let d = dir("limit");
    let f = put(&d, "tone.wav", &sine(220.0, 20.0, -6.0));
    let m = mix_of(vec![clip(&f, 20 * SR, 0.0)]);
    let r = measured(&d, &m, Options { master_gain_db: 12.0, ..Options::default() });
    let l = r.loudness.unwrap();
    // +6 dB over full scale before the limiter, held at the ceiling after it
    assert!((l.pre_limiter.sample_peak_db - 6.0).abs() < 0.05, "{:?}", l.pre_limiter);
    assert!(l.output.sample_peak_db <= 20.0 * f64::from(pm_mixer::master::CEILING).log10() + 1e-4, "{:?}", l.output);
    assert!(lufs(l.output) < lufs(l.pre_limiter) - 3.0, "the limiter took the loudness down: {:?}", l);
}

#[test]
fn every_clip_is_measured_on_its_own_and_a_missing_file_is_reported() {
    let d = dir("clips");
    let a = put(&d, "a.wav", &sine(1000.0, 20.0, -20.0));
    let b = put(&d, "b.wav", &sine(1000.0, 20.0, -32.0));
    let m = mix_of(vec![clip(&a, 20 * SR, 0.0), clip("missing.wav", 20 * SR, 5.0 * SR as f64), clip(&b, 20 * SR, 10.0 * SR as f64)]);
    let got = engine::measure_clips(&m, &engine::dir_resolver(&d), &Options::default(), &mut |_| true).unwrap();
    assert_eq!(got.iter().map(|c| c.index).collect::<Vec<_>>(), vec![0, 1, 2]);
    assert!((lufs(got[0].measure.unwrap()) + 20.0).abs() < 0.1);
    assert!((lufs(got[2].measure.unwrap()) + 32.0).abs() < 0.1);
    assert!(got[1].measure.is_none() && got[1].error.is_some());
    // overlapping neighbours do not leak into a clip's own reading, and the gain options are not applied
    let o = Options { clip_gain_db: vec![10.0, 10.0, 10.0], master_gain_db: 10.0, ..Options::default() };
    let again = engine::measure_clips(&m, &engine::dir_resolver(&d), &o, &mut |_| true).unwrap();
    assert_eq!(got, again);
    // only the clips heard in the range are measured, each over its whole audible part
    let ranged = engine::measure_clips(&m, &engine::dir_resolver(&d), &Options { from: 22 * SR as i64, to: Some(25 * SR as i64), ..Options::default() }, &mut |_| true).unwrap();
    assert_eq!(ranged.iter().map(|c| c.index).collect::<Vec<_>>(), vec![1, 2], "clip 0 ends at 20 s; clips 1 (5-25 s) and 2 (10-30 s) are heard");
    assert_eq!(ranged[1], got[2]);
}

#[test]
fn clip_measurement_follows_the_audible_window_and_can_be_cancelled() {
    let d = dir("window");
    // 20 s at -20 dBFS then 20 s at -40: a clip trimmed to the first half reads -20
    let mut v = sine(1000.0, 20.0, -20.0);
    v.extend(sine(1000.0, 20.0, -40.0));
    let f = put(&d, "two.wav", &v);
    let mut t = clip(&f, 40 * SR, 0.0);
    t.position.r_offset = (20 * SR) as f64;
    let m = mix_of(vec![t]);
    let got = engine::measure_clips(&m, &engine::dir_resolver(&d), &Options::default(), &mut |_| true).unwrap();
    assert!((lufs(got[0].measure.unwrap()) + 20.0).abs() < 0.1, "{:?}", got[0]);
    let err = engine::measure_clips(&m, &engine::dir_resolver(&d), &Options::default(), &mut |_| false).unwrap_err();
    assert!(matches!(err, pm_mixer::Error::Cancelled));
}

#[test]
fn a_cut_range_is_the_same_audio_as_the_full_render_with_short_fades_at_the_cuts() {
    let d = dir("edges");
    let f = put(&d, "tone.wav", &sine(300.0, 10.0, -12.0));
    let m = mix_of(vec![clip(&f, 10 * SR, 0.0)]);
    let res = engine::dir_resolver(&d);
    let (full, _, _) = engine::render_to_vecs(&m, &res, &Options::default()).unwrap();
    let (from, to) = (2 * SR as i64, 6 * SR as i64);
    let hard = Options { from, to: Some(to), ..Options::default() };
    let (cut, _, _) = engine::render_to_vecs(&m, &res, &hard).unwrap();
    assert_eq!(cut.len(), 4 * SR);
    assert_eq!(&cut[..], &full[from as usize..to as usize], "the default is a hard cut of the same samples");

    let edge = 441; // 10 ms
    let soft = Options { edge_fade_frames: edge, ..hard.clone() };
    let (faded, _, _) = engine::render_to_vecs(&m, &res, &soft).unwrap();
    assert_eq!(faded.len(), cut.len());
    assert_eq!(faded[0], 0.0);
    for k in [0usize, 100, 440] {
        assert!(faded[k].abs() <= cut[k].abs() + 1e-6, "fade-in only lowers samples");
    }
    let mid = 1000..faded.len() - 1000;
    assert_eq!(&faded[mid.clone()], &cut[mid], "away from the cuts nothing changes");
    let peak = |v: &[f32]| v.iter().map(|x| x.abs()).fold(0.0, f32::max);
    assert!(peak(&faded[faded.len() - 441..]) < peak(&cut[cut.len() - 441..]), "the last 10 ms fade out");
    assert!(faded[faded.len() - 1].abs() < 0.01, "and end near silence");

    // a range that starts at the first sound and ends with the last clip is not cut: no fade
    let whole = Options { edge_fade_frames: edge, from: 0, to: Some(10 * SR as i64), ..Options::default() };
    let (w, _, _) = engine::render_to_vecs(&m, &res, &whole).unwrap();
    assert_eq!(w, full);
}

#[test]
fn the_measurement_of_a_range_is_the_measurement_of_those_samples() {
    let d = dir("rangemeasure");
    let mut v = sine(1000.0, 20.0, -20.0);
    v.extend(sine(1000.0, 20.0, -40.0));
    let f = put(&d, "two.wav", &v);
    let m = mix_of(vec![clip(&f, 40 * SR, 0.0)]);
    let r = measured(&d, &m, Options { from: 2 * SR as i64, to: Some(18 * SR as i64), ..Options::default() });
    assert!((lufs(r.loudness.unwrap().output) + 20.0).abs() < 0.1);
    let (l, rr, _) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options { from: 22 * SR as i64, to: Some(38 * SR as i64), ..Options::default() }).unwrap();
    assert!((lufs(loudness::measure(&l, &rr, RATE)) + 40.0).abs() < 0.1);
}
