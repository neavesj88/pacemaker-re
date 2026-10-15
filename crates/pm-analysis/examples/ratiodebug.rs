//! For each file: the winning grid's tempo (before the related-tempo check) and the energy
//! share of each faster related tempo, to calibrate the triplet-alias rule.
use pm_analysis::{decode, grid, onset, tempo};

fn main() {
    for path in std::env::args().skip(1) {
        let d = decode::decode(std::path::Path::new(&path)).unwrap();
        let dur = d.mono.len() as f64 / d.sample_rate as f64;
        let env = onset::envelope(&d.mono, d.sample_rate);
        let cands = tempo::candidates(&env.odf, env.frames_per_sec(), 5, tempo::BpmRange { lo: 100.0 });
        // winner without the related check: disable it with an impossible share via range_hi = 0
        let g = grid::lock(&env, &cands, dur, 0.0, false).unwrap();
        let (e_win, _, _) = grid::best_phase(&env, g.period, dur, 0.0, g.period, 0.002);
        let mut shares = Vec::new();
        for (name, r) in [("4:3", 4.0 / 3.0), ("3:2", 1.5), ("5:4", 1.25)] {
            let p = g.period / r;
            let mut best = 0f64;
            for i in -15..=15 {
                let pb = p * (1.0 + i as f64 * 0.0002);
                best = best.max(grid::best_phase(&env, pb, dur, 0.0, pb, 0.002).0);
            }
            shares.push(format!("{name}={:.2}", best / e_win));
        }
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
        println!("{:40} tag {:>5} win {:7.2}  {}", &name[..name.len().min(40)], d.tag_bpm.map_or("-".into(), |b| b.to_string()), g.bpm(), shares.join(" "));
    }
}
