//! Whole-set audio check: for each track, where the kick detector peaks relative to our grid
//! lines (beat-synchronous average over a full beat). |offset| <= 15 ms counts as on the kick.
use pm_analysis::{align, decode, onset};

fn main() {
    let (mut on, mut total) = (0, 0);
    for path in std::env::args().skip(1) {
        let d = decode::decode(std::path::Path::new(&path)).unwrap();
        let r = pm_analysis::analyze(&d.mono, d.sample_rate, None);
        let env = onset::envelope(&d.mono, d.sample_rate);
        let Some(k) = align::kick_offset(&env, &r.beats, r.bpm) else { continue };
        total += 1;
        on += k.on_kick() as usize;
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
        println!(
            "{:40} {:7.2} bpm  kick at {:+5.0} ms  contrast {:4.1}",
            name.chars().take(40).collect::<String>(),
            r.bpm,
            k.offset_ms,
            k.contrast
        );
    }
    println!("kick within 15 ms of grid: {on}/{total}");
}
