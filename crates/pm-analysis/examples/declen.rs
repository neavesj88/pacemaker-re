//! Prints decoded length and sample rate, to compare with an MP3's Xing/LAME header.
fn main() {
    for p in std::env::args().skip(1) {
        let d = pm_analysis::decode::decode(std::path::Path::new(&p)).unwrap();
        let name = std::path::Path::new(&p).file_name().unwrap().to_string_lossy().into_owned();
        println!("{name} samples={} sr={}", d.mono.len(), d.sample_rate);
    }
}
