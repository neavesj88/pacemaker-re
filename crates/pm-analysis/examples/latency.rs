//! Measures the analyser's systematic beat-time offset on synthetic kick tracks.
fn kick_track(sr: u32, bpm: f64, offset: f64, secs: f64) -> Vec<f32> {
    let n = (sr as f64 * secs) as usize;
    let mut mono = vec![0f32; n];
    let period = 60.0 / bpm;
    let mut k = 0;
    loop {
        let start = ((offset + k as f64 * period) * sr as f64) as usize;
        if start >= n {
            break;
        }
        // smooth 400 ms decay (no cut-off click) plus a 2 ms attack transient, like a real kick
        for i in 0..(0.4 * sr as f64) as usize {
            if start + i < n {
                let t = i as f32 / sr as f32;
                let click = if t < 0.002 { 0.3 * (1.0 - t / 0.002) * if i % 2 == 0 { 1.0 } else { -1.0 } } else { 0.0 };
                mono[start + i] += (2.0 * std::f32::consts::PI * 55.0 * t).sin() * (-t * 30.0).exp() + click;
            }
        }
        k += 1;
    }
    mono
}

fn main() {
    let sr = 44100;
    for bpm in [120.0, 126.0, 128.0, 140.0, 174.0] {
        for offset in [0.05, 0.137, 0.3] {
            let r = pm_analysis::analyze(&kick_track(sr, bpm, offset, 90.0), sr, None);
            let period = 60.0 / bpm;
            let first = r.beats[0] as f64 / sr as f64;
            let mut d = (first - offset) % period;
            if d > period / 2.0 { d -= period; }
            if d < -period / 2.0 { d += period; }
            println!("bpm {bpm:>5} got {:>8.3} offset {offset:.3} signed error {:+.1} ms", r.bpm, d * 1000.0);
        }
    }
}
