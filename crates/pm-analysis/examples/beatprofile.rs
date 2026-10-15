//! Beat-synchronous profile: average the kick detector, sub-bass flux and full-band onset
//! envelope in a +-210 ms window around every line of our grid (audio timeline, not device
//! convention). Peaks at 0 ms mean the grid sits on that event; elsewhere shows the offset.
use pm_analysis::{decode, onset};

fn main() {
    for path in std::env::args().skip(1) {
        let d = decode::decode(std::path::Path::new(&path)).unwrap();
        let r = pm_analysis::analyze(&d.mono, d.sample_rate, None);
        let env = onset::envelope(&d.mono, d.sample_rate);
        let fps = env.frames_per_sec();
        let sr = d.sample_rate as f64;
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
        println!("== {} ({:.2} bpm, {})", name, r.bpm, if r.locked_grid { "lock" } else { "var" });
        for (label, series) in [("kick", &env.kick), ("odf ", &env.odf)] {
            let mut prof = vec![0f64; 61];
            for &b in &r.beats {
                // analyze() adds the onset-lead compensation; envelope frames are uncompensated
                let t = b as f64 / sr - 0.0133;
                for (k, p) in prof.iter_mut().enumerate() {
                    let tt = t + (k as f64 - 30.0) * 0.007;
                    let f = (tt * fps).round();
                    if f >= 0.0 && (f as usize) < series.len() {
                        *p += series[f as usize] as f64;
                    }
                }
            }
            let peak = (0..61).max_by(|&a, &b| prof[a].partial_cmp(&prof[b]).unwrap()).unwrap();
            let bars: String = prof
                .iter()
                .map(|v| {
                    let m = prof.iter().cloned().fold(f64::MIN, f64::max);
                    [' ', '.', ':', '-', '=', '+', '*', '#'][((v / m).max(0.0) * 7.0) as usize]
                })
                .collect();
            println!("  {label} [{bars}] peak at {:+} ms", (peak as i32 - 30) * 7);
        }
    }
}
