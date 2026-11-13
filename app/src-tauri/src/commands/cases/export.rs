//! "Export this case": one case as an M3U8 playlist, a CSV list or a rekordbox XML file, using
//! the converters in `pm_formats::interop` (the same ones the Import view's export uses).
//!
//! Reads the library, writes only the file the user chose, and refuses a destination inside the
//! device volume (a stray file there is not in the device's library). The converters turn each
//! track's `location` into a file path with `Library::track_paths`, so every track in the case
//! is first checked with [`plain_location`]: a location that would climb out of the music
//! folder stops the export instead of being written out or read.

use std::path::{Component, Path, PathBuf};

use pm_formats::interop::{self, Scope};
use pm_formats::library::{Library, Track};
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct ExportDone {
    pub path: String,
    pub tracks: usize,
    /// Things the user should know about this export (smart cases left out and why), in words.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// File extension of each export format; `None` for a format this command does not write.
pub fn extension(format: &str) -> Option<&'static str> {
    match format {
        "m3u8" => Some("m3u8"),
        "csv" => Some("csv"),
        "rekordbox" => Some("xml"),
        _ => None,
    }
}

/// True when `location` is `/pmdb_tracks/` followed by plain names only: no `..`, no root or
/// drive prefix, no colon, backslash or NUL, no empty part. (The same rule as the playback
/// area's `check_relative`, kept here so this area edits no other area's file.)
pub fn plain_location(location: &str) -> bool {
    let rel = location.strip_prefix(pm_formats::naming::LOCATION_PREFIX).unwrap_or(location);
    !rel.is_empty()
        && !rel.contains([':', '\\', '\0'])
        && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
        && rel.split('/').all(|part| !part.is_empty())
}

pub(super) fn check_dest(root: &Path, dest: &Path, ext: &str) -> AppResult<()> {
    if dest.is_dir() {
        return Err("The destination is a folder; choose a file name.".into());
    }
    if !dest.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
        return Err(format!("The file name must end in .{ext} to match the chosen format.").into());
    }
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|_| AppError::from("The destination folder does not exist."))?;
    if let Ok(r) = root.canonicalize()
        && parent.starts_with(&r)
    {
        return Err("Choose a folder outside the device: a file written there is not part of its library.".into());
    }
    Ok(())
}

/// Stops an export whose tracks include a file location outside the device's music folder;
/// `what` names the list in the message ("this case", "the library").
pub(super) fn refuse_escaping(tracks: &[Track], what: &str) -> AppResult<()> {
    let bad: Vec<&Track> = tracks.iter().filter(|t| !plain_location(&t.location)).collect();
    if bad.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = bad.iter().take(5).map(|t| format!("“{}” ({})", t.title, t.location)).collect();
    Err(format!(
        "{} track{} in {what} ha{} a file location outside the device's music folder, so nothing was exported: {}{}",
        bad.len(),
        if bad.len() == 1 { "" } else { "s" },
        if bad.len() == 1 { "s" } else { "ve" },
        names.join(", "),
        if bad.len() > 5 { ", ..." } else { "" }
    )
    .into())
}

/// The tracks whose file location is not a plain path in the music folder, and one sentence
/// naming every one of them (title and location) for an export that is written without them.
/// `None` when there are none. `what` names the list ("the library", "“Name”").
pub(super) fn left_out_escaping(tracks: &[Track], what: &str) -> Option<(Vec<i64>, String)> {
    let bad: Vec<&Track> = tracks.iter().filter(|t| !plain_location(&t.location)).collect();
    if bad.is_empty() {
        return None;
    }
    let names: Vec<String> = bad.iter().map(|t| format!("“{}” ({})", t.title, t.location)).collect();
    let note = format!(
        "{} track{} in {what} ha{} a file location outside the device's music folder and {} left out of the file: {}.",
        bad.len(),
        if bad.len() == 1 { "" } else { "s" },
        if bad.len() == 1 { "s" } else { "ve" },
        if bad.len() == 1 { "was" } else { "were" },
        names.join(", ")
    );
    Some((bad.iter().map(|t| t.track_id).collect(), note))
}

/// Writes case `case_id` in `format` ("m3u8", "csv" or "rekordbox") to `dest`.
pub fn export_case(root: &Path, case_id: i64, format: &str, dest: &str) -> AppResult<ExportDone> {
    let ext = extension(format).ok_or_else(|| AppError::from(format!("unknown export format {format}")))?;
    let dest_path = PathBuf::from(dest);
    check_dest(root, &dest_path, ext)?;
    let lib = Library::open_read_only(root)?;
    let (_, tracks) = interop::scope_tracks(&lib, Scope::Case(case_id))?;
    if tracks.is_empty() {
        return Err("This case has no tracks to export.".into());
    }
    refuse_escaping(&tracks, "this case")?;
    let text = match format {
        "m3u8" => interop::export_m3u8(&lib, &tracks),
        "csv" => interop::export_csv(&lib, &tracks),
        _ => interop::export_rekordbox_case(&lib, case_id)?,
    };
    write_out(dest, &text)?;
    Ok(ExportDone { path: dest.to_string(), tracks: tracks.len(), notes: Vec::new() })
}

/// Writes `text` beside `dest` and renames it: a failed write never leaves half a file under
/// the name.
pub(super) fn write_out(dest: &str, text: &str) -> AppResult<()> {
    crate::store::write_file(Path::new(dest), text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::strfile::{Analysis, Beat};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-caseexport-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A volume with three synthetic tracks and a case holding the third then the first.
    fn volume(tag: &str) -> (PathBuf, i64) {
        let vol = temp_dir(tag);
        let src = temp_dir(&format!("{tag}-src"));
        let analysis = Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| Beat { pos: 22050 + i * 22050, bpm: 120.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..3 {
            let p = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&p, vec![0u8; 2048 + i]).unwrap();
            let meta = TrackMeta { title: format!("Synthetic Title {i}"), artist: "Synthetic Artist".into(), format: "MP3".into(), sample_rate: 44100, play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        let case = lib.create_case("Synthetic Case").unwrap();
        lib.set_case_tracks(case, &[ids[2], ids[0]]).unwrap();
        (vol, case)
    }

    #[test]
    fn plain_locations_only() {
        for ok in ["/pmdb_tracks/a.mp3", "/pmdb_tracks/dir/a b.mp3"] {
            assert!(plain_location(ok), "{ok}");
        }
        for bad in ["/pmdb_tracks/../x.mp3", "/pmdb_tracks/a/../../x.mp3", "/pmdb_tracks//x", "/pmdb_tracks/C:/x", "/pmdb_tracks/a\\b.mp3", "/pmdb_tracks/", ""] {
            assert!(!plain_location(bad), "{bad}");
        }
    }

    #[test]
    fn each_format_exports_the_case_in_case_order() {
        let (vol, case) = volume("fmt");
        let out = temp_dir("fmt-out");

        let m3u = out.join("list.m3u8");
        let done = export_case(&vol, case, "m3u8", m3u.to_str().unwrap()).unwrap();
        assert_eq!(done.tracks, 2);
        let text = std::fs::read_to_string(&m3u).unwrap();
        let (a, b) = (text.find("Synthetic Title 2").unwrap(), text.find("Synthetic Title 0").unwrap());
        assert!(a < b && !text.contains("Synthetic Title 1"), "{text}");

        let csv = out.join("list.csv");
        export_case(&vol, case, "csv", csv.to_str().unwrap()).unwrap();
        let text = std::fs::read_to_string(&csv).unwrap();
        assert_eq!(text.lines().count(), 3, "header and two rows");
        assert!(text.contains("Synthetic Title 2") && !text.contains("Synthetic Title 1"));

        let xml = out.join("list.xml");
        export_case(&vol, case, "rekordbox", xml.to_str().unwrap()).unwrap();
        let back = interop::parse_library(&std::fs::read_to_string(&xml).unwrap()).unwrap();
        assert_eq!((back.tracks.len(), back.playlists.len()), (2, 1));
        assert_eq!(back.playlists[0].name, "Synthetic Case");
        assert!(std::fs::read_dir(&out).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")), "no temp file left");
    }

    #[test]
    fn refuses_bad_destinations_formats_and_cases() {
        let (vol, case) = volume("refuse");
        let out = temp_dir("refuse-out");
        let inside = vol.join("list.m3u8");
        assert!(export_case(&vol, case, "m3u8", inside.to_str().unwrap()).is_err(), "inside the device");
        assert!(!inside.exists());
        let wrong_ext = out.join("list.txt");
        assert!(export_case(&vol, case, "m3u8", wrong_ext.to_str().unwrap()).is_err());
        let no_folder = out.join("nowhere").join("list.m3u8");
        assert!(export_case(&vol, case, "m3u8", no_folder.to_str().unwrap()).is_err());
        assert!(export_case(&vol, case, "pdf", out.join("x.pdf").to_str().unwrap()).is_err());
        assert!(export_case(&vol, case + 99, "m3u8", out.join("x.m3u8").to_str().unwrap()).is_err(), "unknown case");
        let empty = {
            let mut lib = Library::open(&vol).unwrap();
            lib.create_case("Empty").unwrap()
        };
        assert!(export_case(&vol, empty, "csv", out.join("e.csv").to_str().unwrap()).unwrap_err().to_string().contains("no tracks"));
    }
}
