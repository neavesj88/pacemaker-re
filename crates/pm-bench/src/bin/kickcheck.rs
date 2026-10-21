//! Which grid is on the kick? For each track, average the sub-bass (< 100 Hz) energy rise at
//! every beat of our grid and of the original Editor's grid. A grid sitting on the kick shows
//! a large rise (energy in the 30 ms after the line minus the 30 ms before it); one on the
//! off-beat shows little. This is direct evidence for half-beat disputes, independent of
//! either analyser.
//!
//!   kickcheck <tracks dir> <editor str dir | --sidecar>
//!
//! Ends with a tally of which grid sits on the kick, over all tracks and over those whose tempo
//! agrees within 0.5 BPM.

use std::path::PathBuf;

use pm_bench::{audio_files, EditorSource};

fn main() {
    let usage = "usage: kickcheck <tracks dir> <editor str dir | --sidecar>";
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect(usage));
    let src = match args.next().expect(usage).as_str() {
        "--sidecar" => EditorSource::Sidecar,
        d => EditorSource::Dir(PathBuf::from(d)),
    };
    let files = audio_files(&dir);
    // [ours, editor, same] over all tracks and over tempo-agreeing tracks
    let (mut all, mut agreeing) = ([0usize; 3], [0usize; 3]);
    println!("{:40} {:>9} {:>9}  verdict", "track", "ours", "editor");
    for f in files {
        let Some(ed) = src.analysis(&f) else { continue };
        let Ok(d) = pm_analysis::decode::decode(&f) else { continue };
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
        let ed_bpm = pm_bench::median(ed.beats.iter().map(|b| b.bpm).collect());
        let which = match verdict {
            "OURS on kick" => 0,
            "EDITOR on kick" => 1,
            _ => 2,
        };
        all[which] += 1;
        if (r.bpm - ed_bpm).abs() <= 0.5 {
            agreeing[which] += 1;
        }
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        println!("{:40} {:9.5} {:9.5}  {verdict}", name.chars().take(40).collect::<String>(), ours, theirs);
    }
    println!();
    for (label, t) in [("all tracks", all), ("tempo-agreeing tracks", agreeing)] {
        println!("{label}: {} ours on kick, {} Editor on kick, {} same", t[0], t[1], t[2]);
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
