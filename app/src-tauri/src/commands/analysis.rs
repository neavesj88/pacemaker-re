//! Analysis: waveform peaks (`analysis_peaks`) and key detection (`analysis_detect_keys`, a
//! background job of kind "keys").
//!
//! Peaks are computed once per track from a full decode at a fixed fine resolution, cached in
//! the app cache dir (see `pm_analysis::peaks`) and resampled to the width asked for. Computing
//! runs on blocking threads, at most [`MAX_DECODES`] at a time and never twice for the same
//! track at once, so a table asking for fifty waveforms neither stalls the UI nor decodes a file
//! fifty times.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use pm_analysis::{decode, key, peaks};
use pm_formats::library::Library;
use pm_formats::naming;
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult, IntoApp};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_job, JobOutcome, Progress};
use crate::state::JobLock;

/// The job kind string of key detection (`jobs.ask({ kind: "keys" })` on the frontend).
pub const KEYS_JOB: &str = "keys";
/// Widest waveform a caller can ask for.
const MAX_WIDTH: usize = 16384;
/// Concurrent decodes for peaks. A decode holds a whole track in memory.
const MAX_DECODES: usize = 2;
/// Keys are written to the database this many tracks at a time (one sync per batch).
const KEY_BATCH: usize = 10;

/// Managed state private to this area.
#[derive(Default)]
pub struct PeaksState {
    /// One lock per track being computed, so concurrent requests for it wait and then hit the cache.
    inflight: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    running: Mutex<usize>,
    freed: Condvar,
}

struct Slot<'a>(&'a PeaksState);

impl<'a> Slot<'a> {
    fn take(state: &'a PeaksState) -> Slot<'a> {
        let mut n = state.running.lock().unwrap_or_else(|e| e.into_inner());
        while *n >= MAX_DECODES {
            n = state.freed.wait(n).unwrap_or_else(|e| e.into_inner());
        }
        *n += 1;
        Slot(state)
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *self.0.running.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        self.0.freed.notify_one();
    }
}

pub fn setup(app: &mut tauri::App) {
    app.manage(Arc::new(PeaksState::default()));
}

/// The audio file for a `tracks.location` on the volume at `root`. Refuses anything that is not
/// a plain relative path below `/pmdb_tracks/`.
fn audio_path(root: &Path, location: &str) -> AppResult<PathBuf> {
    let rel = location.strip_prefix(naming::LOCATION_PREFIX).ok_or("That track is not stored on the device library.")?;
    let rel = Path::new(rel);
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("That track location is not valid.".into());
    }
    Ok(root.join(naming::MUSIC_DIR).join(rel))
}

fn peaks_blocking(state: &PeaksState, cache_dir: &Path, root: &str, location: &str, width: usize) -> AppResult<Vec<peaks::Column>> {
    let audio = audio_path(Path::new(root), location)?;
    // the volume is part of the identity: two devices can hold the same location string
    let id = format!("{root}|{location}");
    let gate = {
        let mut map = state.inflight.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(id.clone()).or_default().clone()
    };
    let result = {
        let _one_at_a_time = gate.lock().unwrap_or_else(|e| e.into_inner());
        let stamp = peaks::stamp_of(&audio)?;
        match peaks::load(cache_dir, &id, stamp) {
            Some(fine) => Ok(fine),
            None => {
                let _slot = Slot::take(state);
                peaks::fine_for_file(cache_dir, &id, &audio)
            }
        }
    };
    // drop the per-track lock entry once nobody else holds it, so the map does not grow forever
    {
        let mut map = state.inflight.lock().unwrap_or_else(|e| e.into_inner());
        drop(gate);
        if map.get(&id).is_some_and(|g| Arc::strong_count(g) == 1) {
            map.remove(&id);
        }
    }
    Ok(peaks::resample(&result.app()?, width))
}

/// Three-band waveform of a track resampled to `width` columns (low, mid, high, 0-255 each).
/// The first call for a track decodes it (seconds for a long file); later calls read the cache.
#[tauri::command]
pub async fn analysis_peaks(
    app: AppHandle,
    state: State<'_, Arc<PeaksState>>,
    root: String,
    location: String,
    width: usize,
) -> AppResult<Vec<peaks::Column>> {
    if width == 0 || width > MAX_WIDTH {
        return Err(format!("Waveform width must be 1 to {MAX_WIDTH}.").into());
    }
    let cache_dir = app.path().app_cache_dir()?.join("peaks");
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || peaks_blocking(&state, &cache_dir, &root, &location, width))
        .await
        .map_err(|e| AppError::from(e.to_string()))?
}

fn label_of(t: &pm_formats::library::Track) -> String {
    match (t.artist.is_empty(), t.title.is_empty()) {
        (false, false) => format!("{} - {}", t.artist, t.title),
        (_, false) => t.title.clone(),
        _ => t.location.clone(),
    }
}

/// Detects the key of tracks and writes `tracks.key`. `track_ids` limits it to those tracks
/// (default: the whole library); tracks that already have a key are left alone unless
/// `overwrite` is true. A detection with confidence below `min_confidence` (default 0, i.e.
/// write every result) is skipped and reported. Progress events carry the job kind "keys", the
/// detected key and its confidence in `detail`.
#[tauri::command]
pub fn analysis_detect_keys(
    app: AppHandle,
    lock: State<'_, JobLock>,
    root: String,
    track_ids: Option<Vec<i64>>,
    overwrite: Option<bool>,
    min_confidence: Option<f32>,
) -> AppResult<()> {
    check_key_args(track_ids.as_deref(), overwrite.unwrap_or(false))?;
    spawn_job(app, &lock, KEYS_JOB, move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        Ok(detect_keys(&mut lib, track_ids.as_deref(), overwrite.unwrap_or(false), min_confidence.unwrap_or(0.0), |p| {
            events::emit(app, JOB_PROGRESS, p)
        })?)
    })
}

fn check_key_args(track_ids: Option<&[i64]>, overwrite: bool) -> AppResult<()> {
    if track_ids.is_some_and(|ids| ids.is_empty()) {
        return Err("No tracks to analyse.".into());
    }
    // replacing keys someone set is only allowed for tracks the caller names one by one
    if overwrite && track_ids.is_none() {
        return Err("Overwriting existing keys needs the tracks to be named; refusing to rewrite the whole library.".into());
    }
    Ok(())
}

/// Writes a batch of keys. `set_track_keys` is all-or-nothing, so if it fails (a track removed
/// meanwhile, say) the entries are written one by one and only the bad ones are returned, with
/// the reason. Returns (rows changed, failures).
fn write_keys(lib: &mut Library, pending: &[(i64, String)]) -> (usize, Vec<(i64, String)>) {
    if let Ok(n) = lib.set_track_keys(pending) {
        return (n, Vec::new());
    }
    let mut written = 0;
    let mut bad = Vec::new();
    for (id, key) in pending {
        match lib.set_track_key(*id, key) {
            Ok(changed) => written += usize::from(changed),
            Err(e) => bad.push((*id, e.to_string())),
        }
    }
    (written, bad)
}

/// The body of the key job, apart from Tauri so it can be tested on a temp library.
fn detect_keys(
    lib: &mut Library,
    track_ids: Option<&[i64]>,
    overwrite: bool,
    min_confidence: f32,
    mut progress: impl FnMut(Progress),
) -> Result<JobOutcome, pm_formats::library::Error> {
    let wanted: Option<std::collections::HashSet<i64>> = track_ids.map(|ids| ids.iter().copied().collect());
    let targets: Vec<_> = lib
        .tracks()?
        .into_iter()
        .filter(|t| wanted.as_ref().is_none_or(|w| w.contains(&t.track_id)))
        .filter(|t| overwrite || t.key.trim().is_empty())
        .collect();
    let total = targets.len();
    let (mut ok, mut failed, mut skipped) = (0usize, 0usize, 0usize);
    let mut pending: Vec<(i64, String)> = Vec::new();
    let mut written = 0usize;
    for (i, t) in targets.iter().enumerate() {
        let label = label_of(t);
        let done = i + 1;
        let outcome = match lib.track_paths(&t.location) {
            None => Err("not stored under /pmdb_tracks/".to_string()),
            Some((audio, _)) => decode::decode(&audio).map_err(|e| e.to_string()).map(|d| key::detect(&d.mono, d.sample_rate)),
        };
        let mut p = Progress::new(KEYS_JOB, label, done, total, "");
        match outcome {
            Ok(Some(r)) if r.confidence >= min_confidence => {
                p.status = "detected".into();
                p.detail = Some(format!("{} ({:.0}%)", r.key.name(), r.confidence * 100.0));
                pending.push((t.track_id, r.key.name()));
                ok += 1;
            }
            Ok(Some(r)) => {
                p.status = "skipped".into();
                p.detail = Some(format!("low confidence: {} ({:.0}%)", r.key.name(), r.confidence * 100.0));
                skipped += 1;
            }
            Ok(None) => {
                p.status = "skipped".into();
                p.detail = Some("no clear key (too short, silent or atonal)".into());
                skipped += 1;
            }
            Err(e) => {
                p.status = "failed".into();
                p.error = Some(e);
                failed += 1;
            }
        }
        progress(p);
        if pending.len() >= KEY_BATCH || (done == total && !pending.is_empty()) {
            let (n, bad) = write_keys(lib, &pending);
            written += n;
            for (id, e) in bad {
                ok -= 1;
                failed += 1;
                let label = targets.iter().find(|t| t.track_id == id).map(label_of).unwrap_or_default();
                let mut f = Progress::new(KEYS_JOB, label, done, total, "failed");
                f.error = Some(format!("key not saved: {e}"));
                progress(f);
            }
            pending.clear();
        }
    }
    let mut parts = Vec::new();
    if written > 0 {
        parts.push(format!("{written} key{} written", if written == 1 { "" } else { "s" }));
    }
    if ok > written {
        parts.push(format!("{} already had that key", ok - written));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} skipped"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    let summary = if parts.is_empty() { "nothing to do".to_string() } else { parts.join(", ") };
    Ok(JobOutcome { ok, failed, summary })
}

/// This area's command paths, one per line, written `$crate::commands::analysis::name,`.
#[macro_export]
macro_rules! analysis_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::analysis::analysis_peaks,
                $crate::commands::analysis::analysis_detect_keys,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::strfile;

    const SR: u32 = 22050;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-analysis-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn wav_bytes(samples: &[f32]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&SR.to_le_bytes());
        w.extend_from_slice(&(SR * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        w
    }

    /// I-IV-V-I in the key whose tonic is `tonic` (0 = C), as additive-synth chords with a bass.
    fn chords(tonic: i32, minor: bool) -> Vec<f32> {
        let third = if minor { 3 } else { 4 };
        let mut out = Vec::new();
        for root in [0, 5, 7, 0, 0, 5, 7, 0] {
            let notes = [root, root + third, root + 7];
            let n = (SR as f32 * 1.0) as usize;
            let mut seg = vec![0f32; n];
            for (j, off) in notes.iter().enumerate() {
                let midi = (60 + tonic + off) as f32;
                let f0 = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
                for h in 1..=3 {
                    let w = 2.0 * std::f32::consts::PI * f0 * h as f32 / SR as f32;
                    for (i, s) in seg.iter_mut().enumerate() {
                        *s += (0.25 + 0.05 * j as f32) / h as f32 * (w * i as f32).sin();
                    }
                }
            }
            out.extend(seg);
        }
        out
    }

    fn library(tag: &str, files: &[(&str, Vec<u8>)]) -> (Library, PathBuf, Vec<i64>) {
        let base = temp_dir(tag);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (name, bytes) in files {
            let p = src.join(name);
            std::fs::write(&p, bytes).unwrap();
            let meta = TrackMeta { title: name.to_string(), format: "WAV".into(), ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        (lib, base, ids)
    }

    #[test]
    fn location_must_stay_inside_the_music_dir() {
        let root = Path::new("/vol");
        assert!(audio_path(root, "/pmdb_tracks/A/B/C.mp3").unwrap().ends_with("A/B/C.mp3"));
        for bad in ["", "/etc/passwd", "/pmdb_tracks/", "/pmdb_tracks/../x.mp3", "/pmdb_tracks/a/../../x", "/pmdb_tracks//abs.mp3", "relative/x.mp3"] {
            assert!(audio_path(root, bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn key_job_writes_keys_and_reports_failures() {
        let (mut lib, base, ids) = library(
            "keys",
            &[
                ("a.wav", wav_bytes(&chords(7, false))),   // G major
                ("b.wav", wav_bytes(&chords(2, true))),    // D minor
                ("c.wav", b"this is not audio".to_vec()), // undecodable
                ("d.wav", wav_bytes(&chords(0, false))),   // C major, but already keyed
            ],
        );
        lib.set_track_key(ids[3], "F#m").unwrap();

        let mut events = Vec::new();
        let out = detect_keys(&mut lib, None, false, 0.0, |p| events.push((p.label.clone(), p.status.clone(), p.done, p.total))).unwrap();
        assert_eq!((out.ok, out.failed), (2, 1), "{}", out.summary);
        assert_eq!(events.len(), 3, "the already-keyed track is not touched");
        assert_eq!(events.last().unwrap().2, events.last().unwrap().3);
        assert_eq!(events.iter().filter(|e| e.1 == "failed").count(), 1);

        let keys: Vec<String> = lib.tracks().unwrap().into_iter().map(|t| t.key).collect();
        assert_eq!(keys, vec!["G", "Dm", "", "F#m"]);
        assert!(lib.compatibility_problems().unwrap().is_empty());

        // second run: only the undecodable track is still keyless, and it fails again
        let again = detect_keys(&mut lib, None, false, 0.0, |_| {}).unwrap();
        assert_eq!((again.ok, again.failed), (0, 1));

        // overwrite + explicit ids re-detects just those
        let one = detect_keys(&mut lib, Some(&[ids[3]]), true, 0.0, |_| {}).unwrap();
        assert_eq!(one.ok, 1);
        assert_eq!(lib.tracks().unwrap()[3].key, "C");

        // a confidence floor nothing can reach skips and writes nothing
        let (mut lib2, base2, ids2) = library("keys-floor", &[("a.wav", wav_bytes(&chords(7, false)))]);
        let skipped = detect_keys(&mut lib2, Some(&ids2), false, 2.0, |_| {}).unwrap();
        assert_eq!((skipped.ok, skipped.failed), (0, 0));
        assert!(skipped.summary.contains("1 skipped"), "{}", skipped.summary);
        assert_eq!(lib2.tracks().unwrap()[0].key, "");

        drop((lib, lib2));
        std::fs::remove_dir_all(base).unwrap();
        std::fs::remove_dir_all(base2).unwrap();
    }

    #[test]
    fn overwrite_needs_named_tracks() {
        assert!(check_key_args(None, true).is_err());
        assert!(check_key_args(Some(&[]), false).is_err());
        assert!(check_key_args(None, false).is_ok());
        assert!(check_key_args(Some(&[1]), true).is_ok());
    }

    #[test]
    fn a_failing_batch_write_keeps_the_other_keys() {
        let (mut lib, base, ids) = library("keys-batch", &[("a.wav", wav_bytes(&chords(7, false))), ("b.wav", wav_bytes(&chords(2, true)))]);
        let batch = vec![(ids[0], "G".to_string()), (987_654, "Am".to_string()), (ids[1], "Dm".to_string())];
        let (written, bad) = write_keys(&mut lib, &batch);
        assert_eq!(written, 2);
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].0, 987_654);
        let keys: Vec<String> = lib.tracks().unwrap().into_iter().map(|t| t.key).collect();
        assert_eq!(keys, vec!["G", "Dm"]);
        assert!(lib.compatibility_problems().unwrap().is_empty());
        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn peaks_are_computed_once_cached_and_resampled() {
        let (lib, base, ids) = library("peaks", &[("a.wav", wav_bytes(&chords(0, false)))]);
        let t = lib.tracks().unwrap().into_iter().find(|t| t.track_id == ids[0]).unwrap();
        let root = lib.root().to_string_lossy().to_string();
        let cache = base.join("cache");
        let state = PeaksState::default();

        let w = peaks_blocking(&state, &cache, &root, &t.location, 64).unwrap();
        assert_eq!(w.len(), 64);
        assert!(w.iter().any(|c| c.iter().any(|&v| v > 50)));
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        assert!(state.inflight.lock().unwrap().is_empty(), "per-track locks are released");

        // a second width comes from the same single cache entry
        let w2 = peaks_blocking(&state, &cache, &root, &t.location, 500).unwrap();
        assert_eq!(w2.len(), 500);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);

        assert!(peaks_blocking(&state, &cache, &root, "/pmdb_tracks/Z/Z/missing.wav", 10).is_err());
        assert!(peaks_blocking(&state, &cache, &root, "/pmdb_tracks/../x", 10).is_err());

        // concurrent requests for one track all succeed and still leave one entry
        let state = Arc::new(state);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let (s, c, r, l) = (state.clone(), cache.clone(), root.clone(), t.location.clone());
                std::thread::spawn(move || peaks_blocking(&s, &c, &r, &l, 32).unwrap().len())
            })
            .collect();
        assert!(handles.into_iter().all(|h| h.join().unwrap() == 32));

        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }
}
