//! Which grid is on the kick? For each track, average the sub-bass (< 100 Hz) energy rise at
//! every beat of our grid and of the original Editor's grid. A grid sitting on the kick shows
//! a large rise (energy in the 30 ms after the line minus the 30 ms before it); one on the
//! off-beat shows little. This is direct evidence for half-beat disputes, independent of
//! either analyser.
//!
//!   kickcheck <tracks dir> <editor str dir>

use std::path::{Path, PathBuf};

use pm_formats::{naming, strfile};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("usage: kickcheck <tracks dir> <editor str dir>"));
    let str_dir = PathBuf::from(args.next().expect("editor str dir"));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mp3"))
        .collect();
    files.sort();
    println!("{:40} {:>9} {:>9}  verdict", "track", "ours", "editor");
    for f in files {
        let Some(ed) = editor(&str_dir, &f) else { continue };
        let d = pm_analysis::decode::decode(&f).unwrap();
        let r = pm_analysis::analyze(&d.mono, d.sample_rate, None);
        let sr = d.sample_rate as f64;
        let sub = lowpass(&d.mono, 100.0, sr);
        // our grid in the audio's own timeline (not the device convention); the Editor's beats
        // shifted back by the same device offset so both are compared on the same audio
        let shift = (pm_analysis::DEVICE_OFFSET_SECS * sr).round() as i64;
        let ours = rise(&sub, &r.beats, sr);
        let eds: Vec<i64> = ed.beats.iter().map(|b| b.pos - shift).collect();
        let theirs = rise(&sub, &eds, sr);
        let verdict = if (ours - theirs).abs() < 0.1 * ours.abs().max(theirs.abs()) {
            "same"
        } else if ours > theirs {
            "OURS on kick"
        } else {
            "EDITOR on kick"
        };
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        println!("{:40} {:9.5} {:9.5}  {verdict}", name.chars().take(40).collect::<String>(), ours, theirs);
    }
}

/// Two cascaded one-pole low-pass filters.
fn lowpass(x: &[f32], hz: f64, sr: f64) -> Vec<f32> {
    let a = (-2.0 * std::f64::consts::PI * hz / sr).exp() as f32;
    let mut y1 = 0f32;
    let mut y2 = 0f32;
    x.iter()
        .map(|&s| {
            y1 = (1.0 - a) * s + a * y1;
            y2 = (1.0 - a) * y1 + a * y2;
            y2
        })
        .collect()
}

/// Mean over beats of (energy in the 30 ms after the beat - energy in the 30 ms before it).
fn rise(sub: &[f32], beats: &[i64], sr: f64) -> f64 {
    let w = (0.030 * sr) as i64;
    let mut total = 0.0;
    let mut n = 0;
    for &b in beats {
        if b - w < 0 || b + w >= sub.len() as i64 {
            continue;
        }
        let e = |from: i64| -> f64 { (from..from + w).map(|i| (sub[i as usize] as f64).powi(2)).sum::<f64>() / w as f64 };
        total += e(b) - e(b - w);
        n += 1;
    }
    if n == 0 { 0.0 } else { total / n as f64 }
}

fn editor(str_dir: &Path, audio: &Path) -> Option<strfile::Analysis> {
    let size = std::fs::metadata(audio).ok()?.len();
    let enc = naming::encoded_name(audio.file_stem()?.to_str()?, size);
    let (ab, c) = enc.rsplit_once('/')?;
    let hit = std::fs::read_dir(str_dir.join(ab))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("{c}."))))?;
    strfile::decode(&std::fs::read_to_string(hit).ok()?).ok()
}
