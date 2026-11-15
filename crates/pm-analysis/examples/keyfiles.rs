//! Key detection over a folder of audio files (or single files): name, detected key or none.
//! Used to calibrate the key detector against the real test files, not only synthetic audio.
use pm_analysis::key;
use std::path::PathBuf;

fn main() {
    let mut files: Vec<PathBuf> = Vec::new();
    for a in std::env::args().skip(1) {
        let p = PathBuf::from(a);
        if p.is_dir() {
            let mut v: Vec<PathBuf> = std::fs::read_dir(&p).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
            v.sort();
            files.extend(v);
        } else {
            files.push(p);
        }
    }
    for f in files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        match key::detect_file(&f) {
            Ok(Some(r)) => println!("{name:<20} {:<4} {}  conf {:.2}  tuning {:+.0}c", r.key.name(), r.key.camelot(), r.confidence, r.tuning_cents),
            Ok(None) => println!("{name:<20} none"),
            Err(e) => println!("{name:<20} error {e}"),
        }
    }
}
