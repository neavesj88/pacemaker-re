//! Tracks: operations on tracks already in the library (beat-grid re-analysis now; tags,
//! ratings and removal later).

use std::path::{Component, Path, PathBuf};

use pm_analysis::ingest::{self, Existing};
use pm_formats::library::Library;
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{progress_of, spawn_write_job};
use crate::state::JobLock;

pub fn setup(_app: &mut tauri::App) {}

/// Rewrites the `.str` of the given tracks from the audio already on the volume (no audio is
/// copied). Same events as `add_tracks`.
#[tauri::command]
pub fn reanalyse_tracks(app: AppHandle, lock: State<'_, JobLock>, root: String, track_ids: Vec<i64>) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err("No tracks to analyse.".into());
    }
    spawn_write_job(app, &lock, "reanalyse", root.clone(), move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        let wanted: std::collections::HashSet<i64> = track_ids.into_iter().collect();
        let items: Vec<Existing> = lib
            .tracks()?
            .into_iter()
            .filter(|t| wanted.contains(&t.track_id))
            .map(|t| Existing {
                track_id: t.track_id,
                label: match (t.artist.is_empty(), t.title.is_empty()) {
                    (false, false) => format!("{} - {}", t.artist, t.title),
                    (_, false) => t.title.clone(),
                    _ => t.location.clone(),
                },
                location: t.location,
            })
            .collect();
        let total = items.len();
        let mut done = 0;
        Ok(ingest::reanalyse(&mut lib, &items, |ev| {
            done += 1;
            events::emit(app, JOB_PROGRESS, progress_of("reanalyse", ev, done, total));
        }).into())
    })
}

/// The audio file for a stored `location`, which must lie inside the volume's music folder. A
/// location whose part after `/pmdb_tracks/` is absolute, has a drive prefix or climbs with `..`
/// would make `join` leave the volume, so anything but plain names is refused.
fn audio_in_volume(lib: &Library, location: &str) -> AppResult<PathBuf> {
    let (audio, _) = lib.track_paths(location).ok_or("This track has no file on the device.")?;
    let music = lib.root().join(pm_formats::naming::MUSIC_DIR);
    let inside = audio.strip_prefix(&music).is_ok_and(|rel| rel.components().all(|c| matches!(c, Component::Normal(_))));
    if !inside {
        return Err("The track's location points outside the device.".into());
    }
    Ok(audio)
}

/// The audio file of a track on the volume. The path is rebuilt from the track's stored
/// `location` (never taken from the caller) and must stay inside the volume.
fn audio_file(lib: &Library, track_id: i64) -> AppResult<PathBuf> {
    let track = lib.tracks()?.into_iter().find(|t| t.track_id == track_id).ok_or("That track is not in the library.")?;
    let audio = audio_in_volume(lib, &track.location)?;
    if !audio.is_file() {
        return Err("The audio file is missing from the device.".into());
    }
    Ok(audio)
}

#[cfg(windows)]
fn reveal(path: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    // explorer needs backslashes and the quotes inside the argument; it exits 1 even on success,
    // so the status is not looked at.
    let native = path.to_string_lossy().replace('/', "\\");
    std::process::Command::new("explorer").raw_arg(format!("/select,\"{native}\"")).spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
fn reveal(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("open").arg("-R").arg(path).spawn().map(|_| ())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn reveal(path: &Path) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(path);
    std::process::Command::new("xdg-open").arg(dir).spawn().map(|_| ())
}

/// Names of the mixes that contain any of the tracks, by name. Re-analysing replaces a track's
/// beat grid and tempo, and a mix clip keeps the tempo the track had when the mix was made, so
/// the confirmation lists these mixes first. Read-only.
#[tauri::command]
pub async fn tracks_mixes_using(root: String, track_ids: Vec<i64>) -> AppResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<String>> {
        let lib = Library::open_read_only(Path::new(&root))?;
        let mut names = std::collections::BTreeSet::new();
        for id in track_ids {
            names.extend(lib.mixes_using_track(id)?.into_iter().map(|(_, name)| name));
        }
        Ok(names.into_iter().collect())
    })
    .await?
}

/// Shows the track's audio file in the system file manager (Explorer with the file selected on
/// Windows). Read-only: nothing on the volume changes.
#[tauri::command]
pub async fn tracks_reveal(root: String, track_id: i64) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let lib = Library::open_read_only(Path::new(&root))?;
        reveal(&audio_file(&lib, track_id)?)?;
        Ok(())
    })
    .await?
}

/// This area's command paths, one per line.
#[macro_export]
macro_rules! tracks_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::tracks::reanalyse_tracks,
                $crate::commands::tracks::tracks_reveal,
                $crate::commands::tracks::tracks_mixes_using,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::strfile::{Analysis, Beat};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-tracks-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn library_with_one_track(tag: &str) -> (PathBuf, i64) {
        let vol = temp_dir(tag);
        let src = temp_dir(&format!("{tag}-src")).join("Demo Track.mp3");
        std::fs::write(&src, vec![0u8; 4096]).unwrap();
        let analysis = Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let meta = TrackMeta { title: "Demo Track".into(), format: "MP3".into(), ..Default::default() };
        let mut lib = Library::create(&vol).unwrap();
        let id = lib.add_track(&src, &meta, &analysis).unwrap();
        (vol, id)
    }

    #[test]
    fn audio_file_resolves_inside_the_volume() {
        let (vol, id) = library_with_one_track("ok");
        let lib = Library::open_read_only(&vol).unwrap();
        let audio = audio_file(&lib, id).unwrap();
        assert!(audio.starts_with(&vol));
        assert!(audio.is_file());
    }

    #[test]
    fn audio_in_volume_refuses_locations_that_escape() {
        let (vol, _) = library_with_one_track("escape");
        let lib = Library::open_read_only(&vol).unwrap();
        // an absolute remainder would replace the base path in `join`
        assert!(audio_in_volume(&lib, "/pmdb_tracks//etc/x").is_err());
        assert!(audio_in_volume(&lib, "/pmdb_tracks/../outside.mp3").is_err());
        assert!(audio_in_volume(&lib, "/pmdb_tracks/a/../../outside.mp3").is_err());
        assert!(audio_in_volume(&lib, "/elsewhere/x.mp3").is_err());
        #[cfg(windows)]
        assert!(audio_in_volume(&lib, "/pmdb_tracks/C:\\x\\y.mp3").is_err());
        let ok = audio_in_volume(&lib, "/pmdb_tracks/a/b.mp3").unwrap();
        assert!(ok.starts_with(&vol));
    }

    #[test]
    fn audio_file_rejects_unknown_ids_and_missing_files() {
        let (vol, id) = library_with_one_track("missing");
        let lib = Library::open_read_only(&vol).unwrap();
        assert!(audio_file(&lib, id + 100).unwrap_err().to_string().contains("not in the library"));
        std::fs::remove_file(audio_file(&lib, id).unwrap()).unwrap();
        assert!(audio_file(&lib, id).unwrap_err().to_string().contains("missing"));
    }
}
