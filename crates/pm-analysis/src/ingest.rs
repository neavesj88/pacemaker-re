//! Batch ingest for a Pacemaker volume: analyse audio files and add them to a library, or
//! re-analyse tracks already on it. Shared by the desktop app (and usable from the CLI).
//!
//! One bad file never stops a batch: decode errors, I/O errors and even a panic inside a codec
//! become a [`Outcome::Failed`] for that file. Analysis (the slow part) runs on a few worker
//! threads; the library is only ever written from the calling thread.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use pm_formats::library::{Library, Track, TrackMeta};
use pm_formats::strfile;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Copied to the volume, `.str` written, row inserted.
    Added { track_id: i64, bpm: f64 },
    /// Already on the volume: nothing was analysed or copied.
    Skipped { track_id: i64 },
    /// An existing track's `.str` and row were rewritten from its audio file.
    Reanalysed { track_id: i64, bpm: f64 },
    Failed(String),
}

/// One finished file. `label` is the file name when adding, the track's own label when
/// re-analysing.
#[derive(Debug, Clone)]
pub struct Event {
    pub label: String,
    pub outcome: Outcome,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Summary {
    pub added: usize,
    pub skipped: usize,
    pub reanalysed: usize,
    pub failed: usize,
}

impl Summary {
    fn count(&mut self, o: &Outcome) {
        match o {
            Outcome::Added { .. } => self.added += 1,
            Outcome::Skipped { .. } => self.skipped += 1,
            Outcome::Reanalysed { .. } => self.reanalysed += 1,
            Outcome::Failed(_) => self.failed += 1,
        }
    }
}

/// A track already in the library whose analysis is to be redone.
#[derive(Debug, Clone)]
pub struct Existing {
    pub track_id: i64,
    /// `tracks.location`
    pub location: String,
    pub label: String,
}

struct Analysed {
    meta: TrackMeta,
    analysis: strfile::Analysis,
    bpm: f64,
}

fn analyse_file(path: &Path) -> Result<Analysed, String> {
    catch_unwind(AssertUnwindSafe(|| {
        let d = crate::decode::decode(path).map_err(|e| e.to_string())?;
        let r = crate::analyze(&d.mono, d.sample_rate, d.tag_bpm);
        Ok(Analysed { meta: d.meta, analysis: r.to_str_analysis(), bpm: r.bpm })
    }))
    .unwrap_or_else(|_| Err("the audio decoder crashed on this file".to_string()))
}

/// Analyses `files` on worker threads, handing each result to `sink` (on the calling thread)
/// as it finishes. `sink` gets the index into `files`.
fn analyse_many(files: &[PathBuf], mut sink: impl FnMut(usize, Result<Analysed, String>)) {
    if files.is_empty() {
        return;
    }
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .saturating_sub(1)
        .clamp(1, 4)
        .min(files.len());
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|s| {
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            s.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= files.len() {
                        break;
                    }
                    if tx.send((i, analyse_file(&files[i]))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (i, r) in rx {
            sink(i, r);
        }
    });
}

/// Why a track needs (re-)analysis, or `None` if its `.str` reads fine: the sidecar is
/// missing, empty (a real device had a 0-byte one), unreadable, or the track was never analysed.
/// Read-only. Tracks not stored under `/pmdb_tracks/` can't be checked and report `None`.
pub fn analysis_problem(lib: &Library, t: &Track) -> Option<String> {
    let (_, sidecar) = lib.track_paths(&t.location)?;
    match std::fs::read(&sidecar) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some("no .str file".into()),
        Err(e) => Some(format!("cannot read .str: {e}")),
        Ok(b) if b.is_empty() => Some("empty .str file (0 bytes)".into()),
        Ok(b) => match strfile::decode(&String::from_utf8_lossy(&b)) {
            Err(e) => Some(format!("unreadable .str: {e:?}")),
            Ok(_) if !t.analysed => Some("never analysed".into()),
            Ok(_) => None,
        },
    }
}

fn file_label(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// Adds `files` to `lib`. Files already on the volume (same device location) are reported as
/// [`Outcome::Skipped`] without being decoded. `progress` is called once per file.
pub fn add_files(lib: &mut Library, files: &[PathBuf], mut progress: impl FnMut(&Event)) -> Summary {
    let mut summary = Summary::default();
    let mut emit = |label: String, outcome: Outcome, summary: &mut Summary| {
        summary.count(&outcome);
        progress(&Event { label, outcome });
    };
    let mut todo: Vec<PathBuf> = Vec::new();
    for f in files {
        match lib.existing_track(f) {
            Ok(Some(track_id)) => emit(file_label(f), Outcome::Skipped { track_id }, &mut summary),
            Ok(None) => todo.push(f.clone()),
            Err(e) => emit(file_label(f), Outcome::Failed(e.to_string()), &mut summary),
        }
    }
    analyse_many(&todo, |i, res| {
        let path = &todo[i];
        let outcome = match res {
            Err(e) => Outcome::Failed(e),
            Ok(a) => match lib.add_track(path, &a.meta, &a.analysis) {
                Ok(track_id) => Outcome::Added { track_id, bpm: a.bpm },
                Err(e) => Outcome::Failed(e.to_string()),
            },
        };
        emit(file_label(path), outcome, &mut summary);
    });
    summary
}

/// Rewrites the `.str` (and bpm / analysed flag) of existing tracks from their audio files on
/// the volume. The audio is never copied or modified; a track whose audio can't be decoded keeps
/// whatever `.str` it had.
pub fn reanalyse(lib: &mut Library, tracks: &[Existing], mut progress: impl FnMut(&Event)) -> Summary {
    let mut summary = Summary::default();
    let mut emit = |label: String, outcome: Outcome, summary: &mut Summary| {
        summary.count(&outcome);
        progress(&Event { label, outcome });
    };
    let mut todo: Vec<(&Existing, PathBuf)> = Vec::new();
    for t in tracks {
        match lib.track_paths(&t.location) {
            Some((audio, _)) if audio.is_file() => todo.push((t, audio)),
            Some(_) => emit(t.label.clone(), Outcome::Failed("audio file is missing from the volume".into()), &mut summary),
            None => emit(t.label.clone(), Outcome::Failed("track is not stored on the device".into()), &mut summary),
        }
    }
    let audio: Vec<PathBuf> = todo.iter().map(|(_, p)| p.clone()).collect();
    analyse_many(&audio, |i, res| {
        let t = todo[i].0;
        let outcome = match res {
            Err(e) => Outcome::Failed(e),
            Ok(a) => match lib.store_analysis(t.track_id, &t.location, &a.analysis) {
                Ok(()) => Outcome::Reanalysed { track_id: t.track_id, bpm: a.bpm },
                Err(e) => Outcome::Failed(e.to_string()),
            },
        };
        emit(t.label.clone(), outcome, &mut summary);
    });
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-ingest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 24 s of 16-bit mono WAV with a decaying 60 Hz thump on every beat at `bpm`.
    fn click_track(path: &Path, bpm: f64) {
        let sr = 22050u32;
        let n = sr as usize * 24;
        let period = (sr as f64 * 60.0 / bpm) as usize;
        let mut pcm = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = (i % period) as f64 / sr as f64;
            let v = (-t * 40.0).exp() * (t * 2.0 * std::f64::consts::PI * 60.0).sin();
            pcm.extend_from_slice(&((v * 24000.0) as i16).to_le_bytes());
        }
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&sr.to_le_bytes());
        w.extend_from_slice(&(sr * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        w.extend_from_slice(&pcm);
        std::fs::write(path, w).unwrap();
    }

    #[test]
    fn add_skip_fail_then_reanalyse() {
        let vol = temp_dir("vol");
        let src = temp_dir("src");
        let good = src.join("Click 120.wav");
        click_track(&good, 120.0);
        let bad = src.join("Not Audio.mp3");
        std::fs::write(&bad, b"this is not an mp3").unwrap();
        let missing = src.join("Gone.flac");

        let mut lib = Library::create(&vol).unwrap();
        let mut events = Vec::new();
        let files = vec![good.clone(), bad.clone(), missing.clone()];
        let s = add_files(&mut lib, &files, |e| events.push(e.clone()));
        assert_eq!(s, Summary { added: 1, skipped: 0, reanalysed: 0, failed: 2 });
        assert_eq!(events.len(), 3, "one event per file");
        let Outcome::Added { track_id, bpm } = events.iter().find(|e| e.label == "Click 120.wav").unwrap().outcome else {
            panic!("good file not added: {events:?}")
        };
        assert!((bpm - 120.0).abs() < 1.0, "bpm {bpm}");
        // the batch survived the bad files, and they left nothing behind on the volume
        assert_eq!(lib.tracks().unwrap().len(), 1);

        // adding again skips without re-analysing
        let s = add_files(&mut lib, &[good.clone()], |_| {});
        assert_eq!(s, Summary { skipped: 1, ..Default::default() });

        // a 0-byte .str (as seen on a real device) is detected and repaired from the audio
        let t = lib.tracks().unwrap().remove(0);
        assert_eq!(analysis_problem(&lib, &t), None);
        let (audio, sidecar) = lib.track_paths(&t.location).unwrap();
        let audio_len = std::fs::metadata(&audio).unwrap().len();
        std::fs::write(&sidecar, b"").unwrap();
        assert_eq!(analysis_problem(&lib, &t).as_deref(), Some("empty .str file (0 bytes)"));
        std::fs::write(&sidecar, b"<TrackAnalysedData>garbage").unwrap();
        assert!(analysis_problem(&lib, &t).unwrap().starts_with("unreadable .str"));
        std::fs::remove_file(&sidecar).unwrap();
        assert_eq!(analysis_problem(&lib, &t).as_deref(), Some("no .str file"));
        std::fs::write(&sidecar, b"").unwrap();
        let item = Existing { track_id, location: t.location.clone(), label: "Click".into() };
        let s = reanalyse(&mut lib, &[item.clone()], |_| {});
        assert_eq!(s, Summary { reanalysed: 1, ..Default::default() });
        let text = std::fs::read_to_string(&sidecar).unwrap();
        let a = strfile::decode(&text).unwrap();
        assert!((a.beats[0].bpm - 120.0).abs() < 1.0);
        assert_eq!(std::fs::metadata(&audio).unwrap().len(), audio_len, "audio untouched");
        assert!(!sidecar.with_extension("str.tmp").exists());
        let t = lib.tracks().unwrap().remove(0);
        assert_eq!(analysis_problem(&lib, &t), None, "repaired track no longer flagged");

        // audio gone: reported, old .str kept
        std::fs::remove_file(&audio).unwrap();
        let s = reanalyse(&mut lib, &[item], |_| {});
        assert_eq!(s.failed, 1);
        assert!(sidecar.is_file());

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src).unwrap();
    }
}
