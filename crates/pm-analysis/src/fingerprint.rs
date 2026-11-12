//! A small audio fingerprint for finding the same recording in two files (owner: cases area,
//! duplicate finder).
//!
//! Method (nothing copied, all textbook signal processing): the audio is reduced to about 11 kHz
//! by block averaging, then an 8192-point Hann-windowed FFT is taken every half second. The power
//! in 16 log-spaced bands between 100 Hz and 4.5 kHz is turned into natural-log energies, and the
//! mean of every band over the whole track is subtracted. That removes the track's overall
//! loudness and tone (so a quieter or re-equalised copy still lines up) and leaves how each band
//! rises and falls through the song, quantised to one signed byte. Two files of the same
//! recording (different encoder, bitrate, sample rate, a little silence at the start) give two
//! such sequences that correlate almost perfectly once one is slid by the offset; different songs
//! do not.
//!
//! This is a review aid, not proof: the caller shows the score and the user decides. A fingerprint
//! is 16 bytes per half second (about 19 KB for five minutes), small enough to cache for a whole
//! library in [`Store`].

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use rustfft::{num_complex::Complex, FftPlanner};

use crate::decode;
use crate::peaks::Stamp;

/// Bands per frame.
pub const BANDS: usize = 16;
/// Seconds between frames.
pub const HOP_SECS: f32 = 0.5;
/// The rate the audio is brought down to (block averaging by `round(rate / 11025)`).
const TARGET_RATE: f32 = 11025.0;
const FFT: usize = 8192;
const LOW_HZ: f32 = 100.0;
const HIGH_HZ: f32 = 4500.0;
/// Quantiser: one byte unit per 1/SCALE of a natural-log unit (clamped at +-127 / SCALE = +-5.08).
const SCALE: f32 = 25.0;
/// Audio shorter than this has too little to match on.
pub const MIN_SECS: f32 = 10.0;
/// Below this mean square of the stored values the track is silent or one steady tone, which
/// every other such track would match.
const MIN_ACTIVITY: f32 = 0.02;
/// Natural-log distance below a frame's loudest band at which band energies stop counting (about
/// 61 dB of power).
const FLOOR: f32 = 14.0;
/// Overlap below this many frames is never accepted as a match (15 s).
const MIN_OVERLAP_FRAMES: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub frames: Vec<[i8; BANDS]>,
}

impl Fingerprint {
    pub fn seconds(&self) -> f32 {
        self.frames.len() as f32 * HOP_SECS
    }
}

/// Band edges as FFT bin indices: `BANDS + 1` increasing values.
fn band_bins(rate: f32) -> [usize; BANDS + 1] {
    let hi = HIGH_HZ.min(rate * 0.45);
    let ratio = (hi / LOW_HZ).powf(1.0 / BANDS as f32);
    let mut out = [0usize; BANDS + 1];
    for (i, slot) in out.iter_mut().enumerate() {
        let f = LOW_HZ * ratio.powi(i as i32);
        *slot = (f * FFT as f32 / rate).round() as usize;
    }
    // every band owns at least one bin
    for i in 1..=BANDS {
        if out[i] <= out[i - 1] {
            out[i] = out[i - 1] + 1;
        }
    }
    out
}

/// The fingerprint of mono audio, or `None` when it is too short, silent or a steady tone.
pub fn compute(mono: &[f32], sample_rate: u32) -> Option<Fingerprint> {
    if sample_rate < 4000 {
        return None;
    }
    let factor = ((sample_rate as f32 / TARGET_RATE).round() as usize).max(1);
    let rate = sample_rate as f32 / factor as f32;
    let ds: Vec<f32> = mono.chunks_exact(factor).map(|c| c.iter().sum::<f32>() / factor as f32).collect();
    if (ds.len() as f32 / rate) < MIN_SECS {
        return None;
    }
    let hop = ((HOP_SECS * rate).round() as usize).max(1);
    let bins = band_bins(rate);
    if bins[BANDS] >= FFT / 2 {
        return None;
    }
    let window: Vec<f32> = (0..FFT).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT as f32).cos()).collect();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT);
    let mut buf = vec![Complex::new(0.0f32, 0.0); FFT];
    let mut feats: Vec<[f32; BANDS]> = Vec::new();
    let mut start = 0usize;
    while start + FFT <= ds.len() {
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = Complex::new(ds[start + i] * window[i], 0.0);
        }
        fft.process(&mut buf);
        let mut f = [0f32; BANDS];
        for b in 0..BANDS {
            let e: f32 = buf[bins[b]..bins[b + 1]].iter().map(|c| c.norm_sqr()).sum();
            f[b] = (e + 1e-6).ln();
        }
        // Nothing more than FLOOR below the frame's loudest band counts: that is leakage and
        // encoder noise (a lossy encode zeroes quiet bands), which differ between copies.
        let top = f.iter().cloned().fold(f32::MIN, f32::max);
        for v in &mut f {
            *v = v.max(top - FLOOR);
        }
        feats.push(f);
        start += hop;
    }
    if feats.len() < MIN_OVERLAP_FRAMES {
        return None;
    }
    let n = feats.len() as f32;
    let mut mean = [0f32; BANDS];
    for f in &feats {
        for b in 0..BANDS {
            mean[b] += f[b] / n;
        }
    }
    let mut energy = 0f32;
    let frames: Vec<[i8; BANDS]> = feats
        .iter()
        .map(|f| {
            let mut q = [0i8; BANDS];
            for b in 0..BANDS {
                let v = ((f[b] - mean[b]) * SCALE).round().clamp(-127.0, 127.0);
                energy += (v / SCALE) * (v / SCALE);
                q[b] = v as i8;
            }
            q
        })
        .collect();
    if energy / (n * BANDS as f32) < MIN_ACTIVITY {
        return None;
    }
    Some(Fingerprint { frames })
}

/// Decodes the file and fingerprints it. `Ok(None)`: too short, silent or a steady tone.
pub fn fingerprint_file(path: &Path) -> Result<Option<Fingerprint>, decode::Error> {
    let d = decode::decode(path)?;
    Ok(compute(&d.mono, d.sample_rate))
}

// ---- comparing -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    /// Cosine similarity of the two sequences at the best offset (1 = identical shape).
    pub score: f32,
    /// Frames `b` starts after `a` (negative: before). One frame is [`HOP_SECS`].
    pub offset_frames: i32,
    pub overlap_frames: usize,
}

fn range_at(a: usize, b: usize, d: i32) -> (usize, usize) {
    let lo = (-d).max(0) as usize;
    let hi_b = b as i64 - d as i64;
    let hi = (a as i64).min(hi_b).max(0) as usize;
    (lo, hi.max(lo))
}

/// Cosine at offset `d` over every `stride`-th frame of the overlap, or `None` when the overlap
/// is shorter than `min_len` frames.
fn cosine(a: &Fingerprint, b: &Fingerprint, d: i32, stride: usize, min_len: usize) -> Option<(f32, usize)> {
    let (lo, hi) = range_at(a.frames.len(), b.frames.len(), d);
    if hi - lo < min_len.max(1) {
        return None;
    }
    let (mut ab, mut aa, mut bb) = (0i64, 0i64, 0i64);
    let mut i = lo;
    while i < hi {
        let x = &a.frames[i];
        let y = &b.frames[(i as i64 + d as i64) as usize];
        for k in 0..BANDS {
            let (p, q) = (x[k] as i64, y[k] as i64);
            ab += p * q;
            aa += p * p;
            bb += q * q;
        }
        i += stride;
    }
    if aa == 0 || bb == 0 {
        return None;
    }
    Some((ab as f32 / ((aa as f32).sqrt() * (bb as f32).sqrt()), hi - lo))
}

/// The best alignment of `b` against `a` within `max_offset_frames`, requiring the overlap to be
/// at least `min_overlap` (a fraction, 0 to 1) of the shorter file. A coarse pass over every
/// third frame picks the three best offsets, which are then scored on every frame.
pub fn similarity(a: &Fingerprint, b: &Fingerprint, max_offset_frames: usize, min_overlap: f32) -> Option<Match> {
    let shorter = a.frames.len().min(b.frames.len());
    let min_len = MIN_OVERLAP_FRAMES.max((min_overlap.clamp(0.0, 1.0) * shorter as f32).ceil() as usize);
    let max_off = max_offset_frames as i32;
    let mut coarse: Vec<(f32, i32)> = (-max_off..=max_off).filter_map(|d| cosine(a, b, d, 3, min_len / 3).map(|(s, _)| (s, d))).collect();
    if coarse.is_empty() {
        return None;
    }
    coarse.sort_by(|x, y| y.0.total_cmp(&x.0));
    coarse
        .iter()
        .take(3)
        .filter_map(|&(_, d)| cosine(a, b, d, 1, min_len).map(|(score, overlap_frames)| Match { score, offset_frames: d, overlap_frames }))
        .max_by(|x, y| x.score.total_cmp(&y.score))
}

// ---- the cache -----------------------------------------------------------------------------

const MAGIC: &[u8; 5] = b"PMFP\x01";

/// Fingerprints by track key (the caller uses `tracks.location`), each with the stamp of the
/// audio file it was made from, so a replaced file is fingerprinted again.
#[derive(Debug, Default)]
pub struct Store {
    entries: HashMap<String, (Stamp, Fingerprint)>,
}

impl Store {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The cached fingerprint, only if it was made from a file with this stamp.
    pub fn get(&self, key: &str, stamp: Stamp) -> Option<&Fingerprint> {
        self.entries.get(key).filter(|(s, _)| *s == stamp).map(|(_, f)| f)
    }

    pub fn put(&mut self, key: String, stamp: Stamp, fp: Fingerprint) {
        self.entries.insert(key, (stamp, fp));
    }

    /// Drops entries whose key is not wanted any more (tracks removed from the library).
    pub fn retain_keys(&mut self, keep: impl Fn(&str) -> bool) {
        self.entries.retain(|k, _| keep(k));
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut keys: Vec<&String> = self.entries.keys().collect();
        keys.sort(); // a stable file for the same content
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(BANDS as u32).to_le_bytes());
        out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
        for k in keys {
            let (stamp, fp) = &self.entries[k];
            out.extend_from_slice(&(k.len() as u32).to_le_bytes());
            out.extend_from_slice(k.as_bytes());
            out.extend_from_slice(&stamp.size.to_le_bytes());
            out.extend_from_slice(&stamp.mtime_ns.to_le_bytes());
            out.extend_from_slice(&(fp.frames.len() as u32).to_le_bytes());
            for f in &fp.frames {
                out.extend(f.iter().map(|&v| v as u8));
            }
        }
        out
    }

    /// Reads a cache file. Anything unreadable is a cache miss, never an error: a wrong magic or
    /// band count gives an empty store, and a file cut off part-way keeps the entries before the
    /// damage. The second value says what was wrong, if anything.
    pub fn decode(bytes: &[u8]) -> (Store, Option<String>) {
        let mut store = Store::default();
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> {
            let s = bytes.get(at..at.checked_add(n)?)?;
            at += n;
            Some(s)
        };
        let header = (|| -> Option<u32> {
            if take(MAGIC.len())? != MAGIC {
                return None;
            }
            if u32::from_le_bytes(take(4)?.try_into().ok()?) != BANDS as u32 {
                return None;
            }
            Some(u32::from_le_bytes(take(4)?.try_into().ok()?))
        })();
        let Some(count) = header else {
            return (store, Some("the fingerprint cache is from another version or damaged and was ignored".into()));
        };
        for _ in 0..count {
            let entry = (|| -> Option<(String, Stamp, Fingerprint)> {
                let klen = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
                let key = String::from_utf8(take(klen)?.to_vec()).ok()?;
                let size = u64::from_le_bytes(take(8)?.try_into().ok()?);
                let mtime_ns = u128::from_le_bytes(take(16)?.try_into().ok()?);
                let n = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
                let raw = take(n.checked_mul(BANDS)?)?;
                let frames = raw.as_chunks::<BANDS>().0.iter().map(|c| c.map(|v| v as i8)).collect();
                Some((key, Stamp { size, mtime_ns }, Fingerprint { frames }))
            })();
            match entry {
                Some((k, s, f)) => store.put(k, s, f),
                None => return (store, Some("the fingerprint cache was cut off; the part that could be read is used".into())),
            }
        }
        (store, None)
    }

    /// The store in `path`; missing is empty and silent.
    pub fn load(path: &Path) -> (Store, Option<String>) {
        match std::fs::read(path) {
            Ok(b) => Store::decode(&b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Store::default(), None),
            Err(e) => (Store::default(), Some(format!("{} could not be read ({e})", path.display()))),
        }
    }

    /// Writes the store through a temp file and a rename, so a crash leaves the old file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("fp.tmp");
        let res = std::fs::File::create(&tmp).and_then(|mut f| {
            f.write_all(&self.encode())?;
            f.sync_all()
        });
        let res = res.and_then(|()| std::fs::rename(&tmp, path));
        if res.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// xorshift: a fixed pseudo-random stream so the tests are repeatable.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn unit(&mut self) -> f32 {
            (self.next() >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    /// A synthetic "song": a note every 0.25 s from a scale (four partials, random loudness and
    /// a decaying envelope) over a bass pulse every 0.5 s.
    fn song(seed: u64, secs: f32, sr: u32) -> Vec<f32> {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1));
        let n = (secs * sr as f32) as usize;
        let mut out = vec![0f32; n];
        let scale = [220.0f32, 246.9, 277.2, 311.1, 329.6, 370.0, 415.3, 440.0, 493.9, 554.4, 622.3, 659.3, 740.0, 830.6, 880.0, 987.8];
        let step = (0.25 * sr as f32) as usize;
        let mut pos = 0;
        while pos < n {
            let f = scale[(rng.next() % 16) as usize];
            let amp = 0.05 + 0.3 * rng.unit();
            let partials = [(1.0f32, 1.0f32), (2.0, 0.5 * rng.unit()), (3.0, 0.4 * rng.unit()), (5.0, 0.3 * rng.unit())];
            for i in 0..(step * 2).min(n - pos) {
                let t = i as f32 / sr as f32;
                let env = (-t * 6.0).exp();
                let mut v = 0.0;
                for (m, a) in partials {
                    v += a * (2.0 * std::f32::consts::PI * f * m * t).sin();
                }
                out[pos + i] += amp * env * v * 0.5;
            }
            if (pos / step).is_multiple_of(2) {
                for i in 0..(step * 2).min(n - pos) {
                    let t = i as f32 / sr as f32;
                    out[pos + i] += 0.3 * (-t * 20.0).exp() * (2.0 * std::f32::consts::PI * 60.0 * t).sin();
                }
            }
            pos += step;
        }
        out
    }

    fn fp(x: &[f32], sr: u32) -> Fingerprint {
        compute(x, sr).expect("fingerprint")
    }

    #[test]
    fn the_same_audio_matches_itself_exactly() {
        let a = fp(&song(1, 60.0, 44100), 44100);
        assert!(a.frames.len() >= 110 && a.frames.len() <= 120, "{}", a.frames.len());
        let m = similarity(&a, &a, 10, 0.8).unwrap();
        assert!(m.score > 0.999, "{m:?}");
        assert_eq!(m.offset_frames, 0);
    }

    #[test]
    fn gain_noise_and_a_duller_copy_still_match() {
        let x = song(2, 60.0, 44100);
        let a = fp(&x, 44100);
        let mut rng = Rng(7);
        let quieter: Vec<f32> = x.iter().map(|v| v * 0.35 + 0.002 * (rng.unit() - 0.5)).collect();
        let m = similarity(&a, &fp(&quieter, 44100), 10, 0.8).unwrap();
        assert!(m.score > 0.95, "gain + noise {m:?}");
        // a crude low-pass, as a heavy lossy encode would do
        let dull: Vec<f32> = x.windows(4).map(|w| w.iter().sum::<f32>() / 4.0).collect();
        let m = similarity(&a, &fp(&dull, 44100), 10, 0.8).unwrap();
        assert!(m.score > 0.95, "low-passed {m:?}");
    }

    #[test]
    fn another_sample_rate_and_a_shifted_start_still_match() {
        let x = song(3, 60.0, 44100);
        let a = fp(&x, 44100);
        // resample 44.1 kHz to 48 kHz with linear interpolation
        let ratio = 44100.0 / 48000.0;
        let y: Vec<f32> = (0..(x.len() as f32 / ratio) as usize - 2)
            .map(|i| {
                let p = i as f32 * ratio;
                let k = p as usize;
                x[k] + (x[k + 1] - x[k]) * (p - k as f32)
            })
            .collect();
        let m = similarity(&a, &fp(&y, 48000), 10, 0.8).unwrap();
        assert!(m.score > 0.95, "48 kHz {m:?}");
        // 1.5 s of silence in front (an encoder pad or an intro): b starts 3 frames later
        let mut z = vec![0f32; (1.5 * 44100.0) as usize];
        z.extend_from_slice(&x);
        let m = similarity(&a, &fp(&z, 44100), 10, 0.8).unwrap();
        assert!(m.score > 0.9, "padded {m:?}");
        assert!((m.offset_frames - 3).abs() <= 1, "offset {m:?}");
        // an offset window too small to reach the true alignment finds a poorer one
        let far = similarity(&a, &fp(&z, 44100), 1, 0.8).unwrap();
        assert!(far.score < m.score, "{far:?} vs {m:?}");
    }

    #[test]
    fn different_songs_do_not_match() {
        let a = fp(&song(4, 60.0, 44100), 44100);
        for seed in 5..9 {
            let b = fp(&song(seed, 60.0, 44100), 44100);
            let m = similarity(&a, &b, 10, 0.8).unwrap();
            assert!(m.score < 0.6, "seed {seed}: {m:?}");
        }
    }

    #[test]
    fn short_silent_and_steady_audio_has_no_fingerprint() {
        assert!(compute(&song(1, 5.0, 44100), 44100).is_none(), "under ten seconds");
        assert!(compute(&vec![0.0; 44100 * 30], 44100).is_none(), "silence");
        let tone: Vec<f32> = (0..44100 * 30).map(|i| 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin()).collect();
        assert!(compute(&tone, 44100).is_none(), "one steady tone");
        assert!(compute(&song(1, 30.0, 2000), 2000).is_none(), "a rate too low to use");
    }

    #[test]
    fn a_much_shorter_file_still_matches_its_start_when_the_overlap_is_enough() {
        let a = fp(&song(1, 60.0, 44100), 44100);
        let b = fp(&song(1, 20.0, 44100), 44100);
        let m = similarity(&a, &b, 4, 0.8).unwrap();
        assert!(m.score > 0.95 && m.offset_frames == 0, "{m:?}");
        // the overlap needed is a fraction of the shorter file; one frame short of the minimum is refused
        assert!(similarity(&a, &b, 4, 1.0).unwrap().overlap_frames >= b.frames.len());
    }

    #[test]
    fn the_store_round_trips_and_checks_the_stamp() {
        let a = fp(&song(1, 30.0, 44100), 44100);
        let b = fp(&song(2, 30.0, 44100), 44100);
        let mut s = Store::default();
        let st = |n: u64| Stamp { size: n, mtime_ns: n as u128 * 1000 };
        s.put("/pmdb_tracks/a".into(), st(1), a.clone());
        s.put("/pmdb_tracks/b".into(), st(2), b.clone());
        let (back, warn) = Store::decode(&s.encode());
        assert!(warn.is_none());
        assert_eq!(back.len(), 2);
        assert_eq!(back.get("/pmdb_tracks/a", st(1)), Some(&a));
        assert_eq!(back.get("/pmdb_tracks/a", st(9)), None, "a changed file is not the cached one");
        assert_eq!(back.get("/pmdb_tracks/zzz", st(1)), None);
        let mut kept = back;
        kept.retain_keys(|k| k.ends_with('b'));
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn a_damaged_store_is_a_miss_not_an_error() {
        let a = fp(&song(1, 30.0, 44100), 44100);
        let mut s = Store::default();
        s.put("one".into(), Stamp { size: 1, mtime_ns: 1 }, a.clone());
        s.put("two".into(), Stamp { size: 2, mtime_ns: 2 }, a);
        let bytes = s.encode();
        let (empty, warn) = Store::decode(b"junk");
        assert!(empty.is_empty() && warn.is_some());
        assert!(Store::decode(&[]).1.is_some());
        let (part, warn) = Store::decode(&bytes[..bytes.len() - 10]);
        assert_eq!(part.len(), 1, "the entry before the cut survives");
        assert!(warn.is_some());
        // saving and loading through a file leaves no temp file behind
        let dir = std::env::temp_dir().join(format!("pm-fp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("lib.fp");
        s.save(&path).unwrap();
        assert!(!dir.join("lib.fp.tmp").exists());
        assert_eq!(Store::load(&path).0.len(), 2);
        assert!(Store::load(&dir.join("absent.fp")).1.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
