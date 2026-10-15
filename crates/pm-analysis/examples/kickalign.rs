//! Whole-set audio check: for each track, where the kick detector peaks relative to our grid
//! lines (beat-synchronous average over a full beat). |offset| <= 15 ms counts as on the kick.
use pm_analysis::{decode, onset};

fn main() {
    let (mut on, mut total) = (0, 0);
    for path in std::env::args().skip(1) {
        let d = decode::decode(std::path::Path::new(&path)).unwrap();
        let r = pm_analysis::analyze(&d.mono, d.sample_rate, None);
        let env = onset::envelope(&d.mono, d.sample_rate);
        let (fps, sr) = (env.frames_per_sec(), d.sample_rate as f64);
        let period = 60.0 / r.bpm;
        let steps = (period / 0.002) as i64;
        let mut prof = vec![0f64; steps as usize];
        for &b in &r.beats {
            let t = b as f64 / sr - 0.0133;
            for (k, p) in prof.iter_mut().enumerate() {
                let f = ((t + (k as i64 - steps / 2) as f64 * 0.002) * fps).round();
                if f >= 0.0 && (f as usize) < env.kick.len() {
                    *p += env.kick[f as usize] as f64;
                }
            }
        }
        let peak = (0..prof.len()).max_by(|&a, &b| prof[a].partial_cmp(&prof[b]).unwrap()).unwrap();
        let off = (peak as i64 - steps / 2) as f64 * 2.0;
        let mean = prof.iter().sum::<f64>() / prof.len() as f64;
        let contrast = prof[peak] / mean.max(1e-9);
        total += 1;
        if off.abs() <= 15.0 {
            on += 1;
        }
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
        println!("{:40} {:7.2} bpm  kick at {:+5.0} ms  contrast {:4.1}", name.chars().take(40).collect::<String>(), r.bpm, off, contrast);
    }
    println!("kick within 15 ms of grid: {on}/{total}");
}
