//! Prints the detected BPM of audio files under each preferred BPM range.
//!
//! `cargo run --release --offline -p pm-analysis --example bpmrange -- <root> <file>...`
//! Each file is decoded once. Files are relative to `<root>`; only a short name (the file's
//! stem) is printed, never tags.
use pm_analysis::tempo::BpmRange;

fn main() {
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(args.next().expect("root folder"));
    println!("{:<12} {:>8} {:>8} {:>8} {:>8} {:>7}", "file", "78-156", "92-184", "98-196", "68-136", "grid");
    for rel in args {
        let path = root.join(&rel);
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let d = match pm_analysis::decode::decode(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("{name:<12} decode failed: {e}");
                continue;
            }
        };
        let mut cols = Vec::new();
        let mut note = String::new();
        for lo in [78.0, 92.0, 98.0, 68.0] {
            let r = pm_analysis::analyze_with(&d.mono, d.sample_rate, d.tag_bpm, BpmRange { lo });
            if lo == 92.0 {
                // grid consistency: median spacing between beats must match the BPM
                let b = &r.beats;
                if b.len() > 2 {
                    let mut gaps: Vec<f64> = b.windows(2).map(|w| (w[1] - w[0]) as f64 / r.sample_rate as f64).collect();
                    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    note = format!("{:.1}", 60.0 / gaps[gaps.len() / 2]);
                }
            }
            cols.push(format!("{:.2}{}", r.bpm, if r.locked_grid { "" } else { "v" }));
        }
        println!("{name:<12} {:>8} {:>8} {:>8} {:>8} {:>7}", cols[0], cols[1], cols[2], cols[3], note);
    }
}
