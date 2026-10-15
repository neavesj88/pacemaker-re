//! For one file: tempo candidates and the best-phase grid energy at each, plus a sweep of
//! integer tempos, to see why a track locks where it does.
use pm_analysis::{decode, grid, onset, tempo};

fn main() {
    let path = std::env::args().nth(1).expect("usage: tempodebug <audio file>");
    let d = decode::decode(std::path::Path::new(&path)).unwrap();
    let dur = d.mono.len() as f64 / d.sample_rate as f64;
    let env = onset::envelope(&d.mono, d.sample_rate);
    let fps = env.frames_per_sec();
    let cands = tempo::candidates(&env.odf, fps, 8, tempo::BpmRange::default());
    println!("tag {:?}  candidates {:?}", d.tag_bpm, cands.iter().map(|c| format!("{:.2}@{:.2}", c.bpm, c.strength)).collect::<Vec<_>>());
    let mut sweep: Vec<(f64, f64, f64)> = (88..176)
        .map(|b| {
            let p = 60.0 / b as f64;
            let (e, _, mean) = grid::best_phase(&env, p, dur, 0.0, p, 0.002);
            (b as f64, e, e / mean)
        })
        .collect();
    sweep.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for (b, e, c) in sweep.iter().take(8) {
        println!("  {b:>5} bpm  energy {e:.4}  contrast {c:.3}");
    }
    let five: Vec<_> = cands.iter().take(5).copied().collect();
    let g = grid::lock(&env, &five, dur, 156.0, false).unwrap();
    println!("lock(no snap): {:.4} bpm phase {:.4} contrast {:.3}", g.bpm(), g.phase, g.contrast);
    let g = grid::lock(&env, &five, dur, 156.0, true).unwrap();
    println!("lock(snap):    {:.4} bpm phase {:.4} snapped {}", g.bpm(), g.phase, g.snapped);
    let r = pm_analysis::analyze(&d.mono, d.sample_rate, d.tag_bpm);
    println!("analyze: {:.4} locked {} snapped {} residual {:.2}", r.bpm, r.locked_grid, r.snapped, r.residual_ms);
}
