//! Three-band waveform peaks for colour waveforms (owner: analysis area).
//!
//! A track is decoded once and reduced to a fixed fine resolution ([`COLUMNS_PER_SEC`] columns
//! per second), each column holding the peak level of three bands: low (below ~250 Hz), mid and
//! high (above ~4 kHz). That fine result is cached on disk, keyed by the track and stamped with
//! the audio file's size and modification time, and any display width is then a cheap max-pool
//! of it ([`resample`]).
//!
//! Levels are absolute (full scale = 255, no per-track normalisation), with a mild gamma so quiet
//! bands stay visible. Bands come from 4th-order Butterworth filters (two cascaded biquads): a
//! sine at 0.4x or 2.5x a band edge leaks about -32 dB into the neighbouring band.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::decode;

/// Fixed resolution of the cached result: 10 ms per column.
pub const COLUMNS_PER_SEC: u32 = 100;
/// Band edges in Hz.
pub const LOW_EDGE_HZ: f64 = 250.0;
pub const HIGH_EDGE_HZ: f64 = 4000.0;
/// Display gamma applied to the 0..1 peak level before scaling to 0..255.
const GAMMA: f64 = 0.7;

/// One column: low, mid, high level, 0..=255.
pub type Column = [u8; 3];

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Decode(decode::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "file: {e}"),
            Error::Decode(e) => write!(f, "decode: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

// ---- band split -----------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ cookbook low-pass (`high == false`) or high-pass.
    fn new(high: bool, freq: f64, q: f64, sample_rate: f64) -> Biquad {
        let w = 2.0 * std::f64::consts::PI * (freq / sample_rate).min(0.49);
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high { ((1.0 + cos) / 2.0, -(1.0 + cos)) } else { ((1.0 - cos) / 2.0, 1.0 - cos) };
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b0 / a0, a1: -2.0 * cos / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// 4th-order Butterworth: two biquads with these Q values.
const BUTTERWORTH4_Q: [f64; 2] = [0.541_196_1, 1.306_562_9];

fn cascade(high: bool, freq: f64, sample_rate: f64) -> [Biquad; 2] {
    [Biquad::new(high, freq, BUTTERWORTH4_Q[0], sample_rate), Biquad::new(high, freq, BUTTERWORTH4_Q[1], sample_rate)]
}

fn run2(f: &mut [Biquad; 2], x: f64) -> f64 {
    let y = f[0].run(x);
    f[1].run(y)
}

fn level_to_u8(peak: f64) -> u8 {
    (255.0 * peak.min(1.0).powf(GAMMA)).round() as u8
}

/// Reduces decoded mono audio to [`COLUMNS_PER_SEC`] three-band columns per second.
pub fn compute(mono: &[f32], sample_rate: u32) -> Vec<Column> {
    if mono.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let sr = sample_rate as f64;
    let mut low = cascade(false, LOW_EDGE_HZ, sr);
    let mut mid_hp = cascade(true, LOW_EDGE_HZ, sr);
    let mut mid_lp = cascade(false, HIGH_EDGE_HZ, sr);
    let mut high = cascade(true, HIGH_EDGE_HZ, sr);

    let per_col = sr / COLUMNS_PER_SEC as f64;
    let cols = (mono.len() as f64 / per_col).ceil() as usize;
    let mut out = Vec::with_capacity(cols);
    let mut peak = [0f64; 3];
    let mut next_edge = per_col;
    for (i, &s) in mono.iter().enumerate() {
        let x = s as f64;
        let l = run2(&mut low, x);
        let m = run2(&mut mid_lp, run2(&mut mid_hp, x));
        let h = run2(&mut high, x);
        peak[0] = peak[0].max(l.abs());
        peak[1] = peak[1].max(m.abs());
        peak[2] = peak[2].max(h.abs());
        if (i + 1) as f64 >= next_edge {
            out.push([level_to_u8(peak[0]), level_to_u8(peak[1]), level_to_u8(peak[2])]);
            peak = [0.0; 3];
            next_edge += per_col;
        }
    }
    if out.len() < cols {
        out.push([level_to_u8(peak[0]), level_to_u8(peak[1]), level_to_u8(peak[2])]);
    }
    out
}

/// Resamples fine columns to `width` columns by taking the maximum of each band over the span a
/// display column covers (so a short transient is never averaged away). When there are fewer
/// fine columns than `width`, columns repeat.
pub fn resample(fine: &[Column], width: usize) -> Vec<Column> {
    if width == 0 {
        return Vec::new();
    }
    if fine.is_empty() {
        return vec![[0; 3]; width];
    }
    let n = fine.len();
    (0..width)
        .map(|j| {
            let a = j * n / width;
            let b = ((j + 1) * n / width).max(a + 1).min(n);
            let mut m = [0u8; 3];
            for c in &fine[a..b] {
                for k in 0..3 {
                    m[k] = m[k].max(c[k]);
                }
            }
            m
        })
        .collect()
}

// ---- cache ----------------------------------------------------------------------------------

/// What identifies one version of an audio file: replacing or editing the file changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub size: u64,
    pub mtime_ns: u128,
}

pub fn stamp_of(path: &Path) -> std::io::Result<Stamp> {
    let md = std::fs::metadata(path)?;
    let mtime_ns = md.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    Ok(Stamp { size: md.len(), mtime_ns })
}

const MAGIC: &[u8; 5] = b"PMPK\x01";

/// FNV-1a: stable across runs and platforms, unlike `DefaultHasher`.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// Cache file for a track identity (`id`: volume plus `tracks.location`). The stamp is stored
/// inside the file, not in its name, so a changed file overwrites its old entry instead of
/// leaving it behind.
pub fn cache_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{:016x}.peaks", fnv1a(id)))
}

pub fn store(dir: &Path, id: &str, stamp: Stamp, fine: &[Column]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut bytes = Vec::with_capacity(MAGIC.len() + 8 + 16 + 4 + 4 + 4 + id.len() + fine.len() * 3);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&COLUMNS_PER_SEC.to_le_bytes());
    bytes.extend_from_slice(&stamp.size.to_le_bytes());
    bytes.extend_from_slice(&stamp.mtime_ns.to_le_bytes());
    bytes.extend_from_slice(&(id.len() as u32).to_le_bytes());
    bytes.extend_from_slice(id.as_bytes());
    bytes.extend_from_slice(&(fine.len() as u32).to_le_bytes());
    for c in fine {
        bytes.extend_from_slice(c);
    }
    let path = cache_path(dir, id);
    let tmp = path.with_extension("peaks.tmp");
    let written = std::fs::File::create(&tmp).and_then(|mut f| f.write_all(&bytes)).and_then(|_| std::fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// The cached fine columns, or `None` when there is no entry, it belongs to another track
/// (hash collision), the audio file's stamp or the resolution has changed, or it is damaged.
pub fn load(dir: &Path, id: &str, stamp: Stamp) -> Option<Vec<Column>> {
    let b = std::fs::read(cache_path(dir, id)).ok()?;
    let mut at = 0usize;
    let mut take = |n: usize| -> Option<&[u8]> {
        let s = b.get(at..at + n)?;
        at += n;
        Some(s)
    };
    if take(MAGIC.len())? != MAGIC {
        return None;
    }
    if u32::from_le_bytes(take(4)?.try_into().ok()?) != COLUMNS_PER_SEC {
        return None;
    }
    let size = u64::from_le_bytes(take(8)?.try_into().ok()?);
    let mtime_ns = u128::from_le_bytes(take(16)?.try_into().ok()?);
    if (Stamp { size, mtime_ns }) != stamp {
        return None;
    }
    let id_len = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    if take(id_len)? != id.as_bytes() {
        return None;
    }
    let n = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
    let data = take(n.checked_mul(3)?)?;
    if at != b.len() {
        return None;
    }
    Some(data.as_chunks::<3>().0.to_vec())
}

/// Fine columns for an audio file: from the cache when current, otherwise decoded, computed and
/// stored. A failure to write the cache does not fail the call.
pub fn fine_for_file(cache_dir: &Path, id: &str, audio: &Path) -> Result<Vec<Column>, Error> {
    let stamp = stamp_of(audio)?;
    if let Some(hit) = load(cache_dir, id, stamp) {
        return Ok(hit);
    }
    let d = decode::decode(audio).map_err(Error::Decode)?;
    let fine = compute(&d.mono, d.sample_rate);
    let _ = store(cache_dir, id, stamp, &fine);
    Ok(fine)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44100;

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        (0..(secs * SR as f32) as usize).map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin()).collect()
    }

    /// Mean level of each band over the middle of the result (skips the filter start-up).
    fn middle(cols: &[Column]) -> [f64; 3] {
        let part = &cols[cols.len() / 4..cols.len() * 3 / 4];
        let mut m = [0f64; 3];
        for c in part {
            for k in 0..3 {
                m[k] += c[k] as f64 / part.len() as f64;
            }
        }
        m
    }

    #[test]
    fn column_count_follows_duration() {
        let cols = compute(&sine(100.0, 0.5, 3.0), SR);
        assert_eq!(cols.len(), 300);
        assert_eq!(compute(&sine(100.0, 0.5, 2.505), SR).len(), 251);
        assert!(compute(&[], SR).is_empty());
    }

    #[test]
    fn sines_land_in_their_own_band() {
        for (freq, band) in [(60.0, 0), (100.0, 0), (700.0, 1), (1000.0, 1), (8000.0, 2), (12000.0, 2)] {
            let m = middle(&compute(&sine(freq, 0.8, 2.0), SR));
            for (k, level) in m.iter().enumerate() {
                if k == band {
                    assert!(*level > 190.0, "{freq} Hz: band {k} = {level}");
                } else {
                    assert!(*level < 25.0, "{freq} Hz leaks {level} into band {k}");
                }
            }
        }
    }

    #[test]
    fn level_follows_amplitude_and_clips_at_full_scale() {
        let loud = middle(&compute(&sine(100.0, 0.9, 2.0), SR))[0];
        let quiet = middle(&compute(&sine(100.0, 0.1, 2.0), SR))[0];
        let silent = middle(&compute(&sine(100.0, 0.0, 2.0), SR))[0];
        assert!(loud > quiet && quiet > 20.0 && silent == 0.0, "{loud} {quiet} {silent}");
        let clipped = middle(&compute(&sine(100.0, 3.0, 2.0), SR))[0];
        assert!(clipped > 254.9, "{clipped}");
    }

    #[test]
    fn resample_keeps_transients_and_pads_short_input() {
        let mut fine = vec![[0u8; 3]; 1000];
        fine[503] = [200, 100, 50];
        let r = resample(&fine, 10);
        assert_eq!(r.len(), 10);
        assert_eq!(r[5], [200, 100, 50]);
        assert!(r.iter().enumerate().all(|(i, c)| i == 5 || *c == [0; 3]));
        let up = resample(&[[1, 2, 3], [4, 5, 6]], 6);
        assert_eq!(up, vec![[1, 2, 3], [1, 2, 3], [1, 2, 3], [4, 5, 6], [4, 5, 6], [4, 5, 6]]);
        assert_eq!(resample(&[], 4), vec![[0; 3]; 4]);
        assert!(resample(&fine, 0).is_empty());
        assert_eq!(resample(&fine, 1000), fine);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-peaks-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Writes a 16-bit mono WAV so the whole decode -> compute -> cache path runs on a real file.
    fn write_wav(path: &Path, samples: &[f32]) {
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&SR.to_le_bytes());
        w.extend_from_slice(&(SR * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        std::fs::write(path, w).unwrap();
    }

    #[test]
    fn cache_hit_and_invalidation() {
        let dir = temp_dir("cache");
        let cache = dir.join("cache");
        let audio = dir.join("a.wav");
        write_wav(&audio, &sine(100.0, 0.8, 2.0));

        let first = fine_for_file(&cache, "vol|/t/a", &audio).unwrap();
        assert_eq!(first.len(), 200);
        assert!(middle(&first)[0] > 190.0);
        assert!(cache_path(&cache, "vol|/t/a").is_file());

        // a hit is served from the cache: poison the entry and see the poison come back
        let stamp = stamp_of(&audio).unwrap();
        let marker = vec![[1u8, 2, 3]; 7];
        store(&cache, "vol|/t/a", stamp, &marker).unwrap();
        assert_eq!(fine_for_file(&cache, "vol|/t/a", &audio).unwrap(), marker);
        assert_eq!(load(&cache, "vol|/t/a", stamp), Some(marker.clone()));

        // another track id with the same file is a different entry
        assert_eq!(load(&cache, "vol|/t/b", stamp), None);

        // size or mtime change invalidates
        assert_eq!(load(&cache, "vol|/t/a", Stamp { size: stamp.size + 1, ..stamp }), None);
        assert_eq!(load(&cache, "vol|/t/a", Stamp { mtime_ns: stamp.mtime_ns + 1, ..stamp }), None);

        // replacing the audio (different length) recomputes and overwrites the entry
        write_wav(&audio, &sine(8000.0, 0.8, 3.0));
        let again = fine_for_file(&cache, "vol|/t/a", &audio).unwrap();
        assert_eq!(again.len(), 300);
        assert!(middle(&again)[2] > 190.0);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1, "old entry replaced, not accumulated");

        // damaged entry is ignored, not trusted
        let p = cache_path(&cache, "vol|/t/a");
        let mut bytes = std::fs::read(&p).unwrap();
        bytes.truncate(bytes.len() - 5);
        std::fs::write(&p, bytes).unwrap();
        assert_eq!(load(&cache, "vol|/t/a", stamp_of(&audio).unwrap()), None);
        assert_eq!(fine_for_file(&cache, "vol|/t/a", &audio).unwrap().len(), 300);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_audio_is_an_error_not_a_panic() {
        let dir = temp_dir("missing");
        assert!(fine_for_file(&dir, "x", &dir.join("nope.wav")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
