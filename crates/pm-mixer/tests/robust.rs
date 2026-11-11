//! Hostile or broken input: out-of-range values in a document, clips whose audio is gone.
//! Synthetic float WAVs in a temp directory.

use std::path::{Path, PathBuf};

use pm_formats::mix::{KeyNode, Mix, Position, Track};
use pm_mixer::{engine, Format, Options, RATE};

const SR: usize = RATE as usize;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pm-mixer-robust-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Mono-as-stereo 32-bit float WAV of a sine.
fn put_sine(d: &Path, name: &str, freq: f32, secs: f32) -> usize {
    let n = (secs * SR as f32) as usize;
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
        let v = 0.4 * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin();
        b.extend_from_slice(&v.to_le_bytes());
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(d.join(name), b).unwrap();
    n
}

fn mix_with(file: &str, frames: usize) -> Mix {
    let pos = Position { start: 0.0, length: frames as f64, limit_size: true, ..Position::default() };
    let mut m = Mix::new("robust");
    m.tracks.push(Track::new(1, file, 120.0, 1.0, pos));
    m
}

#[test]
fn out_of_range_key_values_do_not_panic() {
    let d = dir("extreme-key");
    let n = put_sine(&d, "a.wav", 440.0, 3.0);
    for cents in [-1e9f32, 1e9, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut m = mix_with("a.wav", n);
        m.tracks[0].key = vec![KeyNode { position: 0, cents, flags: 1 }];
        let (l, _, _) = engine::render_to_vecs(&m, &engine::dir_resolver(&d), &Options::default()).unwrap();
        assert!(l.iter().all(|x| x.is_finite()), "{cents}");
    }
}

#[test]
fn a_mix_whose_clips_all_fail_writes_no_file_and_keeps_the_old_one() {
    let d = dir("all-missing");
    let dest = d.join("out.wav");
    std::fs::write(&dest, b"previous export").unwrap();
    let m = mix_with("does-not-exist.wav", SR * 2);
    let err = pm_mixer::render_to_file(&m, &engine::dir_resolver(&d), &Options::default(), Format::Wav, &dest, &mut |_| true).unwrap_err();
    assert!(err.to_string().contains("none of the clips"), "{err}");
    assert_eq!(std::fs::read(&dest).unwrap(), b"previous export");
    assert!(!d.join("out.wav.partial").exists());

    // a good mix replaces the old file in place
    let n = put_sine(&d, "ok.wav", 220.0, 1.0);
    let m = mix_with("ok.wav", n);
    let out = pm_mixer::render_to_file(&m, &engine::dir_resolver(&d), &Options::default(), Format::Wav, &dest, &mut |_| true).unwrap();
    assert_eq!((out.clips_rendered, out.clips_failed), (1, 0));
    assert!(std::fs::metadata(&dest).unwrap().len() > 40_000);
}
