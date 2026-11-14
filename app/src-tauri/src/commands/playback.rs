//! Playback: audio preview for the player bar.
//!
//! Audio itself is streamed by the webview (an `<audio>` element over the asset protocol); this
//! file only (1) works out which file a track is and allows it in the asset scope at run time,
//! and (2) reads the track's `.str` sidecar so the player can draw beat ticks. Nothing here
//! writes to the library.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use pm_formats::library::Library;
use pm_formats::strfile;
use serde::Serialize;
use tauri::{Manager, State};

use crate::error::{AppError, AppResult};

/// Sample rates learned by decoding, for the formats `header_sample_rate` can't read. Keyed by
/// file; a file's rate never changes.
#[derive(Default)]
pub struct SampleRates(Mutex<HashMap<PathBuf, u32>>);

/// Managed state private to this area.
pub fn setup(app: &mut tauri::App) {
    app.manage(SampleRates::default());
}

/// Every extension a library can hold (the set of `ops_device::AUDIO_EXTS`, which includes the importer's),
/// so a track that was added can always be asked for. Whether the webview can decode one is its
/// own answer, and the player shows that as "cannot be played", not as "not an audio file".
const AUDIO_EXTS: [&str; 11] = ["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "aif", "aiff", "wma", "mp4"];

#[derive(Debug, Serialize, PartialEq)]
pub struct Source {
    /// Absolute path of the audio file (already allowed in the asset scope).
    pub path: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Beats {
    /// Beat positions in seconds from the start of the file.
    pub beats: Vec<f64>,
    /// Index into `beats` of the first downbeat.
    pub first_downbeat: Option<usize>,
    /// Gain the device applies (from the `.str`), 1.0 when there is no `.str`.
    pub normalize_factor: f32,
    /// Set when the `.str` is missing, empty or unreadable; the player then shows no ticks.
    pub problem: Option<String>,
}

/// Every component after `/pmdb_tracks/` must be a plain name. `Path::join` replaces its base
/// for a rooted or drive-prefixed component (`/pmdb_tracks/C:/x`, `/pmdb_tracks//x`), so those
/// would leave the Music folder. Colons and backslashes are refused on every platform so the
/// check does not depend on where it runs.
fn check_relative(location: &str) -> AppResult<()> {
    let rel = location.strip_prefix(pm_formats::naming::LOCATION_PREFIX).unwrap_or(location);
    let plain = !rel.is_empty()
        && !rel.contains([':', '\\', '\0'])
        && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
        && rel.split('/').all(|part| !part.is_empty());
    if plain { Ok(()) } else { Err(AppError::from(format!("{location} points outside the library"))) }
}

fn has_audio_ext(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Audio file and `.str` for a `tracks.location`. Rejects locations that would leave the
/// library's Music folder.
fn resolve(root: &Path, location: &str) -> AppResult<(PathBuf, PathBuf)> {
    let lib = Library::open_read_only(root)?;
    let (audio, sidecar) = lib
        .track_paths(location)
        .ok_or_else(|| AppError::from(format!("{location} is not a device-side track path")))?;
    check_relative(location)?;
    Ok((audio, sidecar))
}

/// The audio file of a track, allowed for the `<audio>` element. The frontend turns the path
/// into a URL with `convertFileSrc`.
#[tauri::command]
pub async fn playback_source(app: tauri::AppHandle, root: String, location: String) -> AppResult<Source> {
    let shown = location.clone();
    let (audio, _) = tauri::async_runtime::spawn_blocking(move || resolve(Path::new(&root), &location)).await??;
    if !has_audio_ext(&audio) {
        return Err(AppError::from(format!("{shown} is not an audio file")));
    }
    if !audio.is_file() {
        return Err(AppError::from(format!("audio file not found: {}", audio.display())));
    }
    app.asset_protocol_scope().allow_file(&audio).map_err(|e| AppError::from(e.to_string()))?;
    Ok(Source { path: audio.to_string_lossy().into_owned() })
}

/// Allows a rendered mix preview (or any audio file under the library or the system temp
/// folder) for playback and returns its path. Anything else is refused: the asset scope must
/// not become a way to read arbitrary files.
#[tauri::command]
pub async fn playback_allow_file(app: tauri::AppHandle, root: String, path: String) -> AppResult<Source> {
    let canonical = tauri::async_runtime::spawn_blocking(move || -> AppResult<PathBuf> {
        let p = std::fs::canonicalize(&path)?;
        if !has_audio_ext(&p) {
            return Err(AppError::from(format!("{path} is not an audio file")));
        }
        let roots = [std::fs::canonicalize(std::env::temp_dir()).ok(), std::fs::canonicalize(&root).ok()];
        if !roots.iter().flatten().any(|r| p.starts_with(r)) {
            return Err(AppError::from(format!("{path} is outside the library and the temp folder")));
        }
        Ok(p)
    })
    .await??;
    app.asset_protocol_scope().allow_file(&canonical).map_err(|e| AppError::from(e.to_string()))?;
    Ok(Source { path: canonical.to_string_lossy().into_owned() })
}

/// Beat positions of a track from its `.str`, in seconds.
#[tauri::command]
pub async fn playback_beats(state: State<'_, SampleRates>, root: String, location: String) -> AppResult<Beats> {
    let (audio, sidecar) = {
        let (r, l) = (root.clone(), location.clone());
        tauri::async_runtime::spawn_blocking(move || resolve(Path::new(&r), &l)).await??
    };
    let known = state.0.lock().ok().and_then(|m| m.get(&audio).copied());
    let (beats, learned) = tauri::async_runtime::spawn_blocking(move || read_beats(&audio, &sidecar, known)).await?;
    if let Some((path, rate)) = learned
        && let Ok(mut m) = state.0.lock()
    {
        m.insert(path, rate);
    }
    Ok(beats)
}

fn no_beats(problem: impl Into<String>) -> Beats {
    Beats { beats: vec![], first_downbeat: None, normalize_factor: 1.0, problem: Some(problem.into()) }
}

/// Also returns a sample rate that had to be learned by decoding, for the cache.
fn read_beats(audio: &Path, sidecar: &Path, known: Option<u32>) -> (Beats, Option<(PathBuf, u32)>) {
    let text = match std::fs::read(sidecar) {
        Ok(b) if b.is_empty() => return (no_beats("empty .str file"), None),
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(_) => return (no_beats("no .str file"), None),
    };
    let analysis = match strfile::decode(&text) {
        Ok(a) => a,
        Err(e) => return (no_beats(format!("unreadable .str: {e}")), None),
    };
    let (rate, learned) = match known.or_else(|| header_sample_rate(audio)) {
        Some(r) => (r, None),
        None => match pm_analysis::decode::decode(audio) {
            Ok(d) => (d.sample_rate, Some((audio.to_path_buf(), d.sample_rate))),
            Err(e) => return (no_beats(format!("cannot read the audio: {e:?}")), None),
        },
    };
    (beats_in_seconds(&analysis, rate), learned)
}

fn beats_in_seconds(a: &strfile::Analysis, rate: u32) -> Beats {
    Beats {
        beats: a.beats.iter().map(|b| b.pos as f64 / f64::from(rate)).collect(),
        first_downbeat: a.first_downbeat,
        normalize_factor: a.normalize_factor,
        problem: if a.beats.is_empty() { Some("no beats in the .str".into()) } else { None },
    }
}

/// Sample rate from the first bytes of an MP3, WAV or FLAC file without decoding it. `None`
/// for anything else (the caller decodes then).
fn header_sample_rate(path: &Path) -> Option<u32> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 64];
    let n = f.read(&mut head).ok()?;
    let head = &head[..n];
    if head.starts_with(b"fLaC") {
        // STREAMINFO: 20-bit sample rate at bytes 18..21 of the file.
        let b = head.get(18..21)?;
        let r = (u32::from(b[0]) << 12) | (u32::from(b[1]) << 4) | (u32::from(b[2]) >> 4);
        return (r > 0).then_some(r);
    }
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        // Only the common layout with `fmt ` straight after the RIFF header.
        if head.get(12..16) == Some(b"fmt ") {
            let b = head.get(24..28)?;
            let r = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            return (r > 0).then_some(r);
        }
        return None;
    }
    let mut start = 0u64;
    if head.starts_with(b"ID3") && n >= 10 {
        let size = head[6..10].iter().fold(0u64, |a, &b| (a << 7) | u64::from(b & 0x7f));
        start = 10 + size;
    }
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = vec![0u8; 16 * 1024];
    let got = f.read(&mut buf).ok()?;
    mp3_sample_rate(&buf[..got])
}

/// Sample rate of the first plausible MPEG audio frame header in `buf`.
fn mp3_sample_rate(buf: &[u8]) -> Option<u32> {
    const RATES: [u32; 3] = [44_100, 48_000, 32_000];
    for w in buf.windows(4) {
        if w[0] != 0xff || w[1] & 0xe0 != 0xe0 {
            continue;
        }
        let version = (w[1] >> 3) & 3; // 0 = MPEG 2.5, 1 = reserved, 2 = MPEG 2, 3 = MPEG 1
        let layer = (w[1] >> 1) & 3; // 0 = reserved
        let bitrate_idx = w[2] >> 4;
        let rate_idx = ((w[2] >> 2) & 3) as usize;
        if version == 1 || layer == 0 || bitrate_idx == 0 || bitrate_idx == 15 || rate_idx == 3 {
            continue;
        }
        let base = RATES[rate_idx];
        return Some(match version {
            3 => base,
            2 => base / 2,
            _ => base / 4,
        });
    }
    None
}

/// This area's command paths, one per line, written `$crate::commands::playback::name,`.
#[macro_export]
macro_rules! playback_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::playback::playback_source,
                $crate::commands::playback::playback_allow_file,
                $crate::commands::playback::playback_beats,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use strfile::{Analysis, Beat, Newline};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-playback-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn analysis() -> Analysis {
        Analysis {
            normalize_factor: 0.9,
            beat_start_offset: 0,
            beats: (0..4).map(|i| Beat { pos: 22_050 + i * 22_050, bpm: 120.0 }).collect(),
            first_downbeat: Some(1),
        }
    }

    #[test]
    fn every_imported_extension_can_be_asked_for() {
        for ext in ["mp3", "wav", "flac", "m4a", "aac", "ogg", "opus", "aif", "aiff", "AIFF", "wma", "mp4"] {
            assert!(has_audio_ext(Path::new(&format!("/pmdb_tracks/a.{ext}"))), "{ext}");
        }
        assert!(!has_audio_ext(Path::new("/pmdb_tracks/a.str")));
        assert!(!has_audio_ext(Path::new("/pmdb_tracks/a")));
    }

    #[test]
    fn beats_convert_to_seconds() {
        let b = beats_in_seconds(&analysis(), 44_100);
        assert_eq!(b.beats, vec![0.5, 1.0, 1.5, 2.0]);
        assert_eq!(b.first_downbeat, Some(1));
        assert!(b.problem.is_none());
    }

    #[test]
    fn mp3_header_rates() {
        // MPEG 1 layer 3, 128 kbit, 44.1 / 48 / 32 kHz
        assert_eq!(mp3_sample_rate(&[0, 0, 0xff, 0xfb, 0x90, 0x00]), Some(44_100));
        assert_eq!(mp3_sample_rate(&[0xff, 0xfb, 0x94, 0x00]), Some(48_000));
        assert_eq!(mp3_sample_rate(&[0xff, 0xfb, 0x98, 0x00]), Some(32_000));
        // MPEG 2 layer 3 halves the rate
        assert_eq!(mp3_sample_rate(&[0xff, 0xf3, 0x90, 0x00]), Some(22_050));
        // reserved rate index and no sync are refused
        assert_eq!(mp3_sample_rate(&[0xff, 0xfb, 0x9c, 0x00]), None);
        assert_eq!(mp3_sample_rate(&[1, 2, 3, 4, 5]), None);
    }

    #[test]
    fn header_rate_skips_id3_tag() {
        let d = temp_dir("id3");
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x01\x00".to_vec(); // tag body of 128 bytes
        bytes.extend(vec![0xff; 100]); // 0xff filler that is not a valid frame (bitrate index 15)
        bytes.extend(vec![0u8; 28]);
        bytes.extend([0xff, 0xfb, 0x94, 0x00]);
        let p = d.join("a.mp3");
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(header_sample_rate(&p), Some(48_000));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn header_rate_wav_and_flac() {
        let d = temp_dir("wavflac");
        let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        wav.extend([16, 0, 0, 0, 1, 0, 2, 0]);
        wav.extend(48_000u32.to_le_bytes());
        let p = d.join("a.wav");
        std::fs::write(&p, &wav).unwrap();
        assert_eq!(header_sample_rate(&p), Some(48_000));

        // 44100 = 0x0AC44 in 20 bits: bytes 0x0A 0xC4 0x4_ (low nibble of the third is channels)
        let mut flac = b"fLaC".to_vec();
        flac.extend([0, 0, 0, 34, 0x10, 0, 0x10, 0, 0, 0, 0, 0, 0, 0]); // block header + min/max sizes + frame sizes
        flac.extend([0x0a, 0xc4, 0x42]);
        flac.extend([0u8; 20]);
        let p = d.join("a.flac");
        std::fs::write(&p, &flac).unwrap();
        assert_eq!(header_sample_rate(&p), Some(44_100));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn read_beats_reports_missing_and_empty_sidecars() {
        let d = temp_dir("sidecar");
        let audio = d.join("t.mp3");
        let sidecar = d.join("t.str");
        std::fs::write(&audio, [0xff, 0xfb, 0x90, 0x00]).unwrap();

        let (b, _) = read_beats(&audio, &sidecar, None);
        assert_eq!(b.problem.as_deref(), Some("no .str file"));
        assert!(b.beats.is_empty());

        std::fs::write(&sidecar, b"").unwrap();
        assert_eq!(read_beats(&audio, &sidecar, None).0.problem.as_deref(), Some("empty .str file"));

        std::fs::write(&sidecar, b"not an archive").unwrap();
        assert!(read_beats(&audio, &sidecar, None).0.problem.unwrap().starts_with("unreadable .str"));

        std::fs::write(&sidecar, strfile::encode(&analysis(), Newline::Crlf)).unwrap();
        let (b, learned) = read_beats(&audio, &sidecar, None);
        assert_eq!(b.beats, vec![0.5, 1.0, 1.5, 2.0]); // 44.1 kHz from the MP3 header
        assert!((b.normalize_factor - 0.9).abs() < 1e-6);
        assert!(learned.is_none());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn resolve_refuses_escapes_and_foreign_locations() {
        let d = temp_dir("resolve");
        let _lib = Library::create(&d).unwrap();
        assert!(resolve(&d, "/pmdb_tracks/1/2/abc.mp3").is_ok());
        assert!(resolve(&d, "/pmdb_tracks/../../x.mp3").is_err());
        assert!(resolve(&d, "C:/music/x.mp3").is_err());
        for bad in [
            "/pmdb_tracks/C:/Windows/x.mp3",
            "/pmdb_tracks//Windows/x.mp3",
            "/pmdb_tracks/",
            "/pmdb_tracks/a//b.mp3",
            "/pmdb_tracks/a\\..\\b.mp3",
            "/pmdb_tracks/C:x.mp3",
        ] {
            assert!(resolve(&d, bad).is_err(), "{bad} must be refused");
        }
        let _ = std::fs::remove_dir_all(d);
    }
}
