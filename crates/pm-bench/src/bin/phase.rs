//! Phase relationship between our device-convention grid and the original Editor's beats:
//! the median of each Editor beat's position relative to our grid, as a fraction of a beat
//! (0 = on our beats, +-0.5 = on our off-beats).
//!
//!   phase <tracks dir> <editor str dir>

use std::path::{Path, PathBuf};

use pm_formats::{naming, strfile};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("usage: phase <tracks dir> <editor str dir>"));
    let str_dir = PathBuf::from(args.next().expect("editor str dir"));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mp3"))
        .collect();
    files.sort();
    for f in files {
        let Some(ed) = editor(&str_dir, &f) else { continue };
        let r = pm_analysis::analyze_file(&f).unwrap();
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
        fr.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = fr[fr.len() / 2];
        let mut eb: Vec<f64> = ed.beats.iter().map(|b| b.bpm).collect();
        eb.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        println!(
            "{:40} ours {:7.2} ed {:7.2} phase {:+.3} beat ({:+.0} ms) {}",
            name.chars().take(40).collect::<String>(),
            r.bpm,
            eb[eb.len() / 2],
            med,
            med * period * 1000.0,
            if r.locked_grid { "lock" } else { "var" }
        );
    }
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
