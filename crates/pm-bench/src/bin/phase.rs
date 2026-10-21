//! Phase relationship between our device-convention grid and the original Editor's beats:
//! the median of each Editor beat's position relative to our grid, as a fraction of a beat
//! (0 = on our beats, +-0.5 = on our off-beats).
//!
//!   phase <tracks dir> <editor str dir>
//!   phase <tracks dir> --sidecar        (the Editor's .str sits next to each audio file)
//!
//! Ends with a summary: tracks within 0.05 beat of each other, and half-beat flips (more than
//! 0.4 beat apart), over all tracks and over those whose tempo agrees within 0.5 BPM.

use std::path::PathBuf;

use pm_bench::{audio_files, median, EditorSource};

/// Phases within this many beats count as agreeing.
const AGREE: f64 = 0.05;
/// Phases beyond this many beats count as a half-beat flip (the grid is on the other beat).
const FLIP: f64 = 0.4;

#[derive(Default)]
struct Tally {
    n: usize,
    agree: usize,
    flips: usize,
    offsets_ms: Vec<f64>,
}

impl Tally {
    fn add(&mut self, phase: f64, ms: f64) {
        self.n += 1;
        self.agree += (phase.abs() <= AGREE) as usize;
        self.flips += (phase.abs() > FLIP) as usize;
        self.offsets_ms.push(ms);
    }

    fn print(&self, label: &str) {
        println!(
            "{label}: {} tracks, {} within {AGREE} beat, {} half-beat flips, median phase offset {:+.1} ms",
            self.n,
            self.agree,
            self.flips,
            median(self.offsets_ms.clone())
        );
    }
}

fn main() {
    let usage = "usage: phase <tracks dir> <editor str dir | --sidecar>";
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect(usage));
    let src = match args.next().expect(usage).as_str() {
        "--sidecar" => EditorSource::Sidecar,
        d => EditorSource::Dir(PathBuf::from(d)),
    };
    let (mut all, mut agreeing) = (Tally::default(), Tally::default());
    for f in audio_files(&dir) {
        let Some(ed) = src.analysis(&f) else { continue };
        let Ok(r) = pm_analysis::analyze_file(&f) else { continue };
        if r.beats.len() < 2 || ed.beats.len() < 2 {
            continue;
        }
        let sr = r.sample_rate as f64;
        let ours: Vec<f64> = r.device_beats().iter().map(|&p| p as f64 / sr).collect();
        let period = 60.0 / r.bpm;
        let mut fr: Vec<f64> = ed
            .beats
            .iter()
            .map(|b| {
                let t = b.pos as f64 / sr;
                let i = ours.partition_point(|&o| o <= t).saturating_sub(1);
                (((t - ours[i]) / period).rem_euclid(1.0) + 0.5).rem_euclid(1.0) - 0.5
            })
            .collect();
        fr.sort_by(f64::total_cmp);
        let med = fr[fr.len() / 2];
        let ed_bpm = median(ed.beats.iter().map(|b| b.bpm).collect());
        let tempo_agrees = (r.bpm - ed_bpm).abs() <= 0.5;
        all.add(med, med * period * 1000.0);
        if tempo_agrees {
            agreeing.add(med, med * period * 1000.0);
        }
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        println!(
            "{:40} ours {:7.2} ed {:7.2} phase {:+.3} beat ({:+.0} ms) {}{}",
            name.chars().take(40).collect::<String>(),
            r.bpm,
            ed_bpm,
            med,
            med * period * 1000.0,
            if r.locked_grid { "lock" } else { "var" },
            if tempo_agrees { "" } else { " tempo-differs" }
        );
    }
    println!();
    all.print("all tracks");
    agreeing.print("tempo-agreeing tracks");
}
