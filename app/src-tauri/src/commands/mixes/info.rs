//! Library lookups for the mix editor: track facts and beat grids, fresh clips, and re-linking
//! the tracks of an imported `.pmk` to the open library.

use std::collections::HashMap;

use pm_formats::library::{Library, MixTrackRow, MixTrackSource};
use pm_formats::{mix as m, strfile};

use super::doc::{BeatGrid, Clip, MixDoc, TrackInfo, SAMPLE_RATE};

fn grid_of(a: &strfile::Analysis, sample_rate: i64) -> BeatGrid {
    BeatGrid {
        source_rate: if sample_rate > 0 { sample_rate as u32 } else { SAMPLE_RATE as u32 },
        beat_start_offset: a.beat_start_offset,
        beat_frames: a.beats.iter().map(|b| b.pos).collect(),
        beat_bpm: a.beats.iter().map(|b| b.bpm).collect(),
        first_downbeat: a.first_downbeat,
    }
}

fn info_of(lib: &Library, s: &MixTrackSource) -> TrackInfo {
    let mut info = TrackInfo {
        track_id: s.track_id,
        available: true,
        problem: None,
        title: s.title.clone(),
        artist: s.artist.clone(),
        album: s.album.clone(),
        genre: s.genre.clone(),
        key: s.key.clone(),
        bpm: s.bpm,
        duration_secs: s.play_time_secs as f64,
        source_rate: if s.sample_rate > 0 { s.sample_rate as u32 } else { SAMPLE_RATE as u32 },
        location: s.location.clone(),
        grid: None,
        grid_problem: None,
    };
    match lib.track_paths(&s.location) {
        None => {
            info.available = false;
            info.problem = Some("the track is not stored under /pmdb_tracks/".into());
            info.grid_problem = info.problem.clone();
        }
        Some((audio, sidecar)) => {
            if !audio.is_file() {
                info.available = false;
                info.problem = Some("the audio file is missing from the volume".into());
            }
            match std::fs::read(&sidecar) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => info.grid_problem = Some("no .str file".into()),
                Err(e) => info.grid_problem = Some(format!("cannot read .str: {e}")),
                Ok(b) if b.is_empty() => info.grid_problem = Some("empty .str file".into()),
                Ok(b) => match strfile::decode(&String::from_utf8_lossy(&b)) {
                    Ok(a) if !a.beats.is_empty() => info.grid = Some(grid_of(&a, s.sample_rate)),
                    Ok(_) => info.grid_problem = Some("the .str has no beats".into()),
                    Err(e) => info.grid_problem = Some(format!("unreadable .str: {e:?}")),
                },
            }
        }
    }
    info
}

fn missing(id: i64, row: Option<&MixTrackRow>) -> TrackInfo {
    TrackInfo {
        track_id: id,
        available: false,
        problem: Some(if id < 0 { "the track was removed from the library".into() } else { "the track is not in the library".into() }),
        title: row.and_then(|r| r.dup_title.clone()).unwrap_or_default(),
        artist: row.and_then(|r| r.dup_artist.clone()).unwrap_or_default(),
        album: row.and_then(|r| r.dup_album.clone()).unwrap_or_default(),
        genre: String::new(),
        key: String::new(),
        bpm: 0.0,
        duration_secs: row.and_then(|r| r.dup_play_time_secs).unwrap_or(0) as f64,
        source_rate: SAMPLE_RATE as u32,
        location: String::new(),
        grid: None,
        grid_problem: None,
    }
}

/// Facts and beat grids for the given library ids, in the order asked. Ids that are not tracks
/// (including removed-track placeholders) come back unavailable.
pub fn track_infos(lib: &Library, ids: &[i64]) -> Result<Vec<TrackInfo>, pm_formats::library::Error> {
    let sources: HashMap<i64, MixTrackSource> = lib.mix_track_sources(ids)?.into_iter().map(|s| (s.track_id, s)).collect();
    Ok(ids.iter().map(|id| sources.get(id).map(|s| info_of(lib, s)).unwrap_or_else(|| missing(*id, None))).collect())
}

/// Fills `clip.track` for every clip. `rows` (the mix's `mixtracks`) name removed tracks.
pub fn attach_track_info(lib: &Library, doc: &mut MixDoc, rows: &[MixTrackRow]) -> Result<(), pm_formats::library::Error> {
    let ids: Vec<i64> = doc.clips.iter().map(|c| c.track_id).collect();
    let mut infos = track_infos(lib, &ids)?;
    for (i, (c, info)) in doc.clips.iter_mut().zip(infos.iter_mut()).enumerate() {
        if !info.available && info.title.is_empty() {
            *info = missing(c.track_id, rows.iter().find(|r| r.track_index == i as i64));
        }
        c.track = Some(info.clone());
    }
    Ok(())
}

/// A new clip for library track `track_id`: channel 0 at timeline 0, tempo-matched to
/// `tempo_bpm` when given (else played at its own tempo), no tools, with the track's grid.
/// The length comes from the library's whole-second play time, so it can be up to half a second
/// off; `limitSize` is set, which tells the engine to clip it to the real audio length.
pub fn make_clip(lib: &Library, track_id: i64, tempo_bpm: Option<f64>) -> Result<Clip, String> {
    let s = lib.mix_track_sources(&[track_id]).map_err(|e| e.to_string())?.into_iter().next().ok_or_else(|| format!("track {track_id} is not in the library"))?;
    let info = info_of(lib, &s);
    let grid_bpm = info.grid.as_ref().and_then(|g| g.beat_bpm.first().copied());
    let bpm = if s.bpm > 0.0 { s.bpm } else { grid_bpm.unwrap_or(0.0) };
    if bpm <= 0.0 {
        return Err("the track has no tempo: analyse it before adding it to a mix".into());
    }
    if s.play_time_secs <= 0 {
        return Err("the track has no length in the library".into());
    }
    let speed = tempo_bpm.filter(|t| t.is_finite() && *t > 0.0).map(|t| (t / bpm) as f32).unwrap_or(1.0);
    let length = s.play_time_secs as f64 * SAMPLE_RATE / f64::from(speed);
    let position = m::Position { start: 0.0, length, limit_size: true, session_event_id: super::doc::fresh_id(), ..m::Position::default() };
    let mut t = m::Track::new(s.track_id, &s.location, bpm, speed, position);
    t.fuzzy_id = m::FuzzyId { artist: s.artist.clone(), album: s.album.clone(), title: s.title.clone(), genre: s.genre.clone() };
    let mut mix = m::Mix::new("");
    mix.tracks.push(t);
    let mut clip = MixDoc::from_mix(0, &mix).clips.remove(0);
    clip.track = Some(info);
    Ok(clip)
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Points every track of an imported mix at the library track with the same title and artist
/// (album breaks ties). The ids inside a `.pmk` belong to the library it came from, so they are
/// never trusted. Errors listing the tracks that have no match.
pub fn relink(lib: &Library, mix: &mut m::Mix) -> Result<(), String> {
    let tracks = lib.tracks().map_err(|e| e.to_string())?;
    let mut missing = Vec::new();
    for t in &mut mix.tracks {
        let f = &t.fuzzy_id;
        let (title, artist, album) = (norm(&f.title), norm(&f.artist), norm(&f.album));
        let mut hits: Vec<&pm_formats::library::Track> = tracks
            .iter()
            .filter(|x| !title.is_empty() && norm(&x.title) == title && (artist.is_empty() || norm(&x.artist) == artist))
            .collect();
        hits.sort_by_key(|x| (norm(&x.album) != album, x.track_id));
        match hits.first() {
            Some(x) => {
                t.library_id = x.track_id;
                t.file_path = x.location.clone();
            }
            None => missing.push(match (f.artist.is_empty(), f.title.is_empty()) {
                (false, false) => format!("{} - {}", f.artist, f.title),
                (_, false) => f.title.clone(),
                _ => t.file_path.clone(),
            }),
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        let n = missing.len();
        let shown: Vec<_> = missing.iter().take(8).cloned().collect();
        Err(format!(
            "{n} track(s) of this mix are not in the library, so it cannot be imported: {}{}",
            shown.join("; "),
            if n > shown.len() { "; ..." } else { "" }
        ))
    }
}

#[cfg(test)]
pub mod support {
    use super::*;
    use pm_formats::library::TrackMeta;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pmrev-mixinfo-{tag}-{}-{}", std::process::id(), super::super::doc::fresh_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A temp library with two tracks, one with a real-looking grid.
    pub fn library(tag: &str) -> (Library, Vec<std::path::PathBuf>, Vec<i64>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src = temp_dir(&format!("{tag}-src"));
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, bpm) in [128.0, 124.0].into_iter().enumerate() {
            let a = strfile::Analysis {
                normalize_factor: 1.0,
                beat_start_offset: 2000,
                beats: (0..64).map(|k| strfile::Beat { pos: 2000 + (k as f64 * 60.0 / bpm * 44100.0).round() as i64, bpm }).collect(),
                first_downbeat: Some(1),
            };
            let f = src.join(format!("synthetic track {i}.mp3"));
            std::fs::write(&f, vec![1u8; 2000 + i]).unwrap();
            let meta = TrackMeta {
                title: format!("Title {i}"),
                artist: format!("Artist {i}"),
                album: "Album".into(),
                genre: "House".into(),
                format: "MP3".into(),
                sample_rate: 44100,
                play_time_secs: 240 + i as i64,
                ..Default::default()
            };
            ids.push(lib.add_track(&f, &meta, &a).unwrap());
        }
        (lib, vec![vol, src], ids)
    }

    pub fn cleanup(lib: Library, dirs: Vec<std::path::PathBuf>) {
        drop(lib);
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::support::{cleanup, library};
    use super::*;

    #[test]
    fn infos_clips_and_availability() {
        let (lib, dirs, ids) = library("infos");
        let infos = track_infos(&lib, &[ids[1], -3, 9999, ids[0]]).unwrap();
        assert_eq!(infos.iter().map(|i| i.available).collect::<Vec<_>>(), vec![true, false, false, true]);
        let g = infos[0].grid.as_ref().unwrap();
        assert_eq!((g.source_rate, g.beat_frames.len(), g.first_downbeat), (44100, 64, Some(1)));
        assert!((g.beat_bpm[0] - 124.0).abs() < 1e-9);
        assert!(infos[1].problem.as_deref().unwrap().contains("removed"));

        // a missing audio file or sidecar is reported, not fatal
        let (audio, sidecar) = lib.track_paths(&infos[0].location).unwrap();
        std::fs::remove_file(&audio).unwrap();
        std::fs::write(&sidecar, b"").unwrap();
        let again = track_infos(&lib, &[ids[1]]).unwrap();
        assert!(!again[0].available && again[0].grid.is_none());
        assert_eq!(again[0].grid_problem.as_deref(), Some("empty .str file"));

        // fresh clip: tempo matched to the mix, grid attached, length from the play time
        let c = make_clip(&lib, ids[0], Some(130.0)).unwrap();
        assert!((f64::from(c.tempo[0].speed) - 130.0 / 128.0).abs() < 1e-6);
        assert!((c.span.length - 240.0 * 44100.0 / (130.0 / 128.0)).abs() < 5.0);
        assert!(c.track.as_ref().unwrap().grid.is_some() && c.span.limit_size && c.id.len() == 32);
        assert_eq!(c.fuzzy_id.title, "Title 0");
        assert!(make_clip(&lib, 9999, None).is_err());
        // it joins a document that validates and saves
        let mut mix = m::Mix::new("Clip Test");
        let mut d = MixDoc::from_mix(1, &mix);
        d.clips.push(c);
        d.refresh();
        let known = ids.iter().copied().collect();
        assert_eq!(d.validate(Some(&known)), Vec::<String>::new());
        mix = d.to_mix().unwrap();
        assert!(mix.to_blob().is_ok());
        cleanup(lib, dirs);
    }

    #[test]
    fn import_relinks_by_title_and_artist() {
        let (lib, dirs, ids) = library("relink");
        let mut mix = m::Mix::new("Imported");
        for (i, (title, artist)) in [("title 1", " ARTIST 1"), ("Title 0", "Artist 0")].into_iter().enumerate() {
            let mut t = m::Track::new(9000 + i as i64, "foreign.mp3", 120.0, 1.0, super::super::doc::tests::pos(0.0, 1000.0, &format!("{i:032x}")));
            t.fuzzy_id = m::FuzzyId { artist: artist.into(), album: String::new(), title: title.into(), genre: String::new() };
            mix.tracks.push(t);
        }
        relink(&lib, &mut mix).unwrap();
        assert_eq!(mix.library_ids(), vec![ids[1], ids[0]]);
        assert!(mix.tracks[0].file_path.starts_with("/pmdb_tracks/"));
        mix.tracks[1].fuzzy_id.title = "Unknown".into();
        let e = relink(&lib, &mut mix).unwrap_err();
        assert!(e.contains("1 track(s)") && e.contains("Artist 0 - Unknown"), "{e}");
        cleanup(lib, dirs);
    }
}
