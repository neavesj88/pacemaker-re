//! Smart cases in exports.
//!
//! A smart case is a saved set of rules, and neither M3U8 nor CSV can say "rules". rekordbox
//! XML cannot either: its playlist tree has only two node types, folder (`Type="0"`) and
//! playlist (`Type="1"`), and no node that holds a query (as far as the format's published
//! description and this app's reader and writer, `pm_formats::interop`, go; a smart list
//! round-tripped through rekordbox itself has not been tried). So every export writes a smart
//! case as what it matches right now, in its own order, cut at its limit: an ordinary
//! playlist.
//!
//! * The library-wide rekordbox XML ([`library_xml`]) adds each smart case as a playlist
//!   (settings: on/off, in a "Smart cases" folder or beside the cases) and leaves out the
//!   star-prefixed Pacemaker copies of the smart cases it wrote, which would be the same list
//!   a second time, possibly out of date.
//! * One smart case can be exported alone as M3U8, CSV or rekordbox XML ([`export_one`]).
//!
//! Nothing here writes to the device; the only file written is the one the user chose.

use std::collections::HashSet;
use std::path::Path;

use pm_formats::interop::{self, ExtraPlaylist, XmlOptions};
use pm_formats::library::{Error, Library, Track};

use crate::error::{AppError, AppResult};

use super::export::{check_dest, extension, left_out_escaping, write_out, ExportDone};
use super::smart::{self, Data, SmartCase};
use super::smart_settings::{self, EXPORT_FOLDER};

/// A smart case and the tracks it matches right now, in its order.
pub struct Matched {
    pub sc: SmartCase,
    pub ids: Vec<i64>,
}

/// The smart cases that could be evaluated, and a sentence for each that could not.
pub struct Evaluation {
    pub matched: Vec<Matched>,
    pub left_out: Vec<String>,
}

/// Evaluates every smart case in `list` against `lib` (one read of the library).
pub fn evaluate_all(lib: &Library, list: &[SmartCase], now_ms: i64) -> Result<Evaluation, Error> {
    let queries: Vec<&smart::Query> = list.iter().map(|s| &s.query).collect();
    let data = if list.is_empty() { None } else { Some(Data::load(lib, &queries, now_ms)?) };
    let mut out = Evaluation { matched: Vec::new(), left_out: Vec::new() };
    for sc in list {
        let result = smart::validate(&sc.name, &sc.query).and_then(|()| smart::evaluate(&sc.query, data.as_ref().expect("list is not empty")).map(|e| e.ids));
        match result {
            Ok(ids) => out.matched.push(Matched { sc: sc.clone(), ids }),
            Err(e) => out.left_out.push(format!("Smart case “{}” was left out: {e}", sc.name)),
        }
    }
    Ok(out)
}

/// The library-wide rekordbox XML with smart cases in it (per the settings), what to tell the
/// user about it, and how many tracks the collection holds.
pub struct LibraryXml {
    pub text: String,
    pub tracks: usize,
    pub notes: Vec<String>,
}

fn quoted(names: &[String]) -> String {
    names.iter().map(|n| format!("“{n}”")).collect::<Vec<_>>().join(", ")
}

/// Exports the whole library as rekordbox XML. With the "include smart cases" setting on (the
/// default) every saved smart case becomes a playlist of its current matches and the Pacemaker
/// copies of those smart cases are not written a second time; one that cannot be evaluated is
/// named in `notes`, left out, and its device copy stays in as the ordinary case it is. A track
/// whose file location is outside the music folder is left out of the file (named in full in
/// `notes`); the rest of the library still exports.
pub fn library_xml(lib: &Library, config_dir: &Path, root: &str, now_ms: i64) -> AppResult<LibraryXml> {
    let tracks = lib.tracks()?;
    let (settings, _) = smart_settings::load(config_dir);
    let mut notes = Vec::new();
    let mut opts = XmlOptions::default();
    // a track whose location climbs out of the music folder is never resolved to a file: it is
    // left out of the collection and every playlist, and named in full, rather than stopping
    // the whole export
    if let Some((ids, note)) = left_out_escaping(&tracks, "the library") {
        opts.exclude_tracks = ids.into_iter().collect();
        notes.push(note);
    }
    if settings.export_smart {
        match smart::load(config_dir, root) {
            Err(e) => notes.push(format!("Smart cases were left out of the file: {e}")),
            Ok(list) => {
                let ev = evaluate_all(lib, &list, now_ms)?;
                notes.extend(ev.left_out);
                let case_names: HashSet<String> = lib.cases()?.into_iter().map(|c| c.name.to_lowercase()).collect();
                let mut copies = Vec::new();
                for m in &ev.matched {
                    if let Some(copy) = smart::owned_device_case(lib, &m.sc)? {
                        opts.skip_cases.insert(copy);
                        copies.push(smart::device_name(&m.sc));
                    }
                    // beside the cases a smart case that shares a case's name would be two playlists
                    // of one name; the folder keeps them apart by itself
                    let clash = !settings.export_folder && case_names.contains(&m.sc.name.trim().to_lowercase());
                    let name = if clash { format!("{} (smart)", m.sc.name.trim()) } else { m.sc.name.trim().to_string() };
                    opts.extra.push(ExtraPlaylist { name, track_ids: m.ids.clone() });
                }
                if settings.export_folder {
                    opts.extra_folder = Some(EXPORT_FOLDER.to_string());
                }
                if !ev.matched.is_empty() {
                    let names: Vec<String> = ev.matched.iter().map(|m| m.sc.name.trim().to_string()).collect();
                    let mut line = format!("Smart cases written as playlists of what they match now: {}.", quoted(&names));
                    if !copies.is_empty() {
                        line.push_str(&format!(" Their Pacemaker copies ({}) were not written a second time.", quoted(&copies)));
                    }
                    notes.push(line);
                }
            }
        }
    }
    let text = interop::export_rekordbox_with(lib, &opts)?;
    Ok(LibraryXml { text, tracks: tracks.len() - opts.exclude_tracks.len(), notes })
}

/// Writes one smart case, as it matches now, to `dest` as "m3u8", "csv" or "rekordbox" (XML:
/// one playlist and the tracks in it). The file may not be inside the device volume.
pub fn export_one(root_str: &str, config_dir: &Path, id: u32, format: &str, dest: &str, now_ms: i64) -> AppResult<ExportDone> {
    let root = Path::new(root_str);
    let ext = extension(format).ok_or_else(|| AppError::from(format!("unknown export format {format}")))?;
    check_dest(root, Path::new(dest), ext)?;
    let sc = smart::load(config_dir, root_str)?.into_iter().find(|s| s.id == id).ok_or_else(|| AppError::from(format!("no smart case {id}")))?;
    let lib = Library::open_read_only(root)?;
    let ev = evaluate_all(&lib, std::slice::from_ref(&sc), now_ms)?;
    let Some(m) = ev.matched.into_iter().next() else {
        return Err(ev.left_out.join(" ").into());
    };
    if m.ids.is_empty() {
        return Err(format!("“{}” matches no tracks right now, so there is nothing to export.", sc.name).into());
    }
    let all = lib.tracks()?;
    let by_id: std::collections::HashMap<i64, &Track> = all.iter().map(|t| (t.track_id, t)).collect();
    let mut tracks: Vec<Track> = m.ids.iter().filter_map(|i| by_id.get(i).map(|t| (*t).clone())).collect();
    let mut notes = Vec::new();
    let mut exclude = HashSet::new();
    if let Some((ids, note)) = left_out_escaping(&tracks, &format!("“{}”", sc.name)) {
        exclude = ids.into_iter().collect();
        tracks.retain(|t| !exclude.contains(&t.track_id));
        notes.push(note);
    }
    if tracks.is_empty() {
        return Err(format!("Every track “{}” matches has a file location outside the device's music folder, so there is nothing to export. {}", sc.name, notes.join(" ")).into());
    }
    let text = match format {
        "m3u8" => interop::export_m3u8(&lib, &tracks),
        "csv" => interop::export_csv(&lib, &tracks),
        _ => interop::export_rekordbox_with(
            &lib,
            &XmlOptions { extras_only: true, exclude_tracks: exclude, extra: vec![ExtraPlaylist { name: sc.name.trim().to_string(), track_ids: tracks.iter().map(|t| t.track_id).collect() }], ..Default::default() },
        )?,
    };
    write_out(dest, &text)?;
    Ok(ExportDone { path: dest.to_string(), tracks: tracks.len(), notes })
}

#[cfg(test)]
mod tests {
    use super::super::smart::{Query, Rule};
    use super::*;
    use crate::commands::cases::smart_settings::{RefreshMode, SmartSettings};
    use pm_formats::{library::TrackMeta, strfile};
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-smartexp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn library(tag: &str, n: usize) -> (Library, PathBuf, Vec<i64>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src = temp_dir(&format!("{tag}-src"));
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: 22050 + i * 22050, bpm: 120.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let p = src.join(format!("exp {i}.mp3"));
            std::fs::write(&p, vec![0u8; 2048 + i]).unwrap();
            let meta = TrackMeta { title: format!("Export Title {i}"), artist: "Export Artist".into(), format: "MP3".into(), sample_rate: 44100, play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        let _ = std::fs::remove_dir_all(&src); // best effort: Windows may still hold the copied file
        (lib, vol, ids)
    }

    fn rated(name: &str, stars: u8, on_device: bool) -> SmartCase {
        SmartCase { id: 0, name: name.into(), query: Query { rules: vec![Rule::RatingMin { stars }], ..Default::default() }, on_device, device_case_id: None }
    }

    /// A smart case nothing in the test libraries matches.
    fn never(name: &str) -> SmartCase {
        SmartCase { id: 0, name: name.into(), query: Query { rules: vec![Rule::PlaysMin { n: 99 }], ..Default::default() }, on_device: false, device_case_id: None }
    }

    fn titles(x: &interop::ForeignLibrary) -> Vec<String> {
        x.tracks.iter().map(|t| t.title.clone()).collect()
    }

    #[test]
    fn library_xml_writes_smart_cases_as_playlists_and_not_their_device_copies_twice() {
        let (mut lib, vol, ids) = library("libxml", 4);
        let cfg = temp_dir("libxml-cfg");
        lib.set_track_ratings(&[(ids[0], 5), (ids[2], 5), (ids[3], 2)]).unwrap();
        let a = smart::save(&cfg, "P:\\", rated("Top rated", 5, true)).unwrap();
        smart::save(&cfg, "P:\\", never("Never")).unwrap();
        let (copy, _) = smart::put_on_device(&mut lib, &a, &[ids[0]]).unwrap(); // out of date on purpose
        smart::update(&cfg, "P:\\", &SmartCase { on_device: true, device_case_id: Some(copy), ..a.clone() }).unwrap();
        let plain = lib.create_case_with_tracks("Plain", &[ids[1]]).unwrap();
        let _ = plain;

        // default settings: a "Smart cases" folder, the device copy is not written again
        let x = library_xml(&lib, &cfg, "P:\\", 0).unwrap();
        assert_eq!(x.tracks, 4);
        let back = interop::parse_library(&x.text).unwrap();
        let names: Vec<&str> = back.playlists.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Plain", "Smart cases / Top rated", "Smart cases / Never"], "{names:?}");
        assert_eq!(titles(&back).len(), 4);
        assert_eq!(back.playlists[1].tracks.len(), 2, "its current matches, not the stale device copy of one track");
        assert!(back.playlists[2].tracks.is_empty(), "a smart case that matches nothing is an empty playlist");
        assert!(x.notes.iter().any(|n| n.contains("“Top rated”") && n.contains("“★ Top rated”")), "{:?}", x.notes);
        assert!(!x.text.contains("★ Top rated"));

        // beside the cases, with a name that clashes with an ordinary case
        smart::save(&cfg, "P:\\", rated("Plain", 5, false)).unwrap();
        smart_settings::save(&cfg, &SmartSettings { export_folder: false, ..Default::default() }).unwrap();
        let back = interop::parse_library(&library_xml(&lib, &cfg, "P:\\", 0).unwrap().text).unwrap();
        let names: Vec<&str> = back.playlists.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Plain", "Top rated", "Never", "Plain (smart)"], "{names:?}");

        // switched off: the file is exactly the plain library export, device copy included
        smart_settings::save(&cfg, &SmartSettings { export_smart: false, refresh_mode: RefreshMode::Off, ..Default::default() }).unwrap();
        let x = library_xml(&lib, &cfg, "P:\\", 0).unwrap();
        assert_eq!(x.text, interop::export_rekordbox(&lib).unwrap());
        assert!(x.notes.is_empty());
        assert!(x.text.contains("★ Top rated"));

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&cfg).unwrap();
    }

    #[test]
    fn a_smart_case_that_cannot_run_is_named_and_left_out_but_its_device_copy_stays() {
        let (mut lib, vol, ids) = library("broken", 2);
        let cfg = temp_dir("broken-cfg");
        let gone = lib.create_case("Soon gone").unwrap();
        let rules = Query { rules: vec![Rule::InCase { case_id: gone, value: true, case_name: None }], ..Default::default() };
        let sc = smart::save(&cfg, "P:\\", SmartCase { id: 0, name: "Depends".into(), query: rules, on_device: true, device_case_id: None }).unwrap();
        let (copy, _) = smart::put_on_device(&mut lib, &sc, &[ids[0]]).unwrap();
        smart::update(&cfg, "P:\\", &SmartCase { on_device: true, device_case_id: Some(copy), ..sc }).unwrap();
        lib.delete_case(gone).unwrap();

        let x = library_xml(&lib, &cfg, "P:\\", 0).unwrap();
        assert!(x.notes.iter().any(|n| n.contains("“Depends” was left out") && n.contains("no longer exists")), "{:?}", x.notes);
        let back = interop::parse_library(&x.text).unwrap();
        assert_eq!(back.playlists.len(), 1);
        assert_eq!(back.playlists[0].name, "★ Depends", "its device copy is still written, as the ordinary case it is");

        // a damaged smart-case store is said so; the library still exports
        std::fs::write(smart::store_path(&cfg, "P:\\"), "not json").unwrap();
        let x = library_xml(&lib, &cfg, "P:\\", 0).unwrap();
        assert!(x.notes[0].starts_with("Smart cases were left out of the file"), "{:?}", x.notes);
        assert_eq!(interop::parse_library(&x.text).unwrap().playlists.len(), 1);

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&cfg).unwrap();
    }

    /// Rewrites the stored location of track `id` in `music.db` to a path that climbs out of
    /// the music folder, keeping the byte length so the file layout stays valid. (The library
    /// API never writes such a location; a hand-edited or foreign database could hold one.)
    fn corrupt_location(vol: &Path, lib: Library, id: i64) -> String {
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap().location;
        drop(lib);
        assert!(loc.len() > 16, "{loc}");
        let bad = format!("/pmdb_tracks/../{}", "x".repeat(loc.len() - 16));
        let db = pm_formats::library::db_path(vol);
        let bytes = std::fs::read(&db).unwrap();
        let (from, to) = (loc.as_bytes(), bad.as_bytes());
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        let mut hits = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(from) {
                out.extend_from_slice(to);
                i += from.len();
                hits += 1;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        assert!(hits > 0, "the location is stored in music.db");
        std::fs::write(&db, out).unwrap();
        bad
    }

    #[test]
    fn a_track_outside_the_music_folder_is_left_out_and_named_and_the_rest_still_exports() {
        let (lib, vol, ids) = library("escape", 3);
        let cfg = temp_dir("escape-cfg");
        let root = vol.to_string_lossy().into_owned();
        let mut lib = lib;
        lib.set_track_ratings(&[(ids[0], 5), (ids[1], 5), (ids[2], 5)]).unwrap();
        lib.create_case_with_tracks("Plain", &[ids[1], ids[2]]).unwrap();
        smart::save(&cfg, &root, rated("Top rated", 5, false)).unwrap();
        let bad = corrupt_location(&vol, lib, ids[1]);
        let lib = Library::open_read_only(&vol).unwrap();

        let x = library_xml(&lib, &cfg, &root, 0).unwrap();
        assert_eq!(x.tracks, 2, "the collection holds the other two");
        let back = interop::parse_library(&x.text).unwrap();
        assert_eq!(titles(&back), ["Export Title 0", "Export Title 2"]);
        assert!(!x.text.contains("Export Title 1") && !x.text.contains("xxxx"), "the bad row appears nowhere");
        let by_name = |n: &str| back.playlists.iter().find(|p| p.name.ends_with(n)).unwrap().tracks.len();
        assert_eq!((by_name("Plain"), by_name("Top rated")), (1, 2), "playlists lose only that track");
        let note = x.notes.iter().find(|n| n.contains("left out of the file")).unwrap();
        assert!(note.contains("“Export Title 1”") && note.contains(&bad) && note.starts_with("1 track in the library has"), "{note}");

        // one smart case alone: the same, with the note on the result
        let out = temp_dir("escape-out");
        let sc = smart::load(&cfg, &root).unwrap().remove(0);
        let csv = out.join("a.csv");
        let done = export_one(&root, &cfg, sc.id, "csv", csv.to_str().unwrap(), 0).unwrap();
        assert_eq!(done.tracks, 2);
        assert!(done.notes[0].contains("“Export Title 1”") && done.notes[0].contains("“Top rated”"), "{:?}", done.notes);
        let text = std::fs::read_to_string(&csv).unwrap();
        assert!(!text.contains("Export Title 1") && text.contains("Export Title 0"));
        let xml = out.join("a.xml");
        export_one(&root, &cfg, sc.id, "rekordbox", xml.to_str().unwrap(), 0).unwrap();
        let back = interop::parse_library(&std::fs::read_to_string(&xml).unwrap()).unwrap();
        assert_eq!((titles(&back), back.playlists[0].tracks.len()), (vec!["Export Title 0".to_string(), "Export Title 2".to_string()], 2));

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&cfg).unwrap();
        std::fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn left_out_note_names_every_track_not_just_five() {
        let (lib, vol, _) = library("note", 8);
        let mut tracks = lib.tracks().unwrap();
        assert!(left_out_escaping(&tracks, "the library").is_none());
        for t in tracks.iter_mut().take(7) {
            t.location = "/pmdb_tracks/../secret.mp3".into();
        }
        let (ids, note) = left_out_escaping(&tracks, "the library").unwrap();
        assert_eq!(ids.len(), 7);
        for i in 0..7 {
            assert!(note.contains(&format!("“Export Title {i}”")), "{note}");
        }
        assert!(note.starts_with("7 tracks in the library have") && note.contains("were left out"), "{note}");
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
    }

    #[test]
    fn one_smart_case_exports_in_its_own_order_in_each_format() {
        let (mut lib, vol, ids) = library("one", 4);
        let cfg = temp_dir("one-cfg");
        let out = temp_dir("one-out");
        lib.set_track_ratings(&[(ids[0], 5), (ids[1], 4), (ids[3], 5)]).unwrap();
        let root = vol.to_string_lossy().into_owned();
        // newest-listed first (longest first): track 3 (103 s), then 1 (101 s), then 0 (100 s)
        let query = Query {
            rules: vec![Rule::RatingMin { stars: 4 }],
            sort: Some(smart::Sort { field: smart::SortField::Duration, descending: true }),
            ..Default::default()
        };
        let sc = smart::save(&cfg, &root, SmartCase { id: 0, name: "Long first".into(), query, on_device: false, device_case_id: None }).unwrap();
        let empty = smart::save(&cfg, &root, never("Nothing")).unwrap();

        let m3u = out.join("a.m3u8");
        let done = export_one(&root, &cfg, sc.id, "m3u8", m3u.to_str().unwrap(), 0).unwrap();
        assert_eq!(done.tracks, 3);
        let text = std::fs::read_to_string(&m3u).unwrap();
        let pos = |n: usize| text.find(&format!("Export Title {n}")).unwrap();
        assert!(pos(3) < pos(1) && pos(1) < pos(0) && !text.contains("Export Title 2"), "{text}");

        let csv = out.join("a.csv");
        export_one(&root, &cfg, sc.id, "csv", csv.to_str().unwrap(), 0).unwrap();
        assert_eq!(std::fs::read_to_string(&csv).unwrap().lines().count(), 4);

        let xml = out.join("a.xml");
        export_one(&root, &cfg, sc.id, "rekordbox", xml.to_str().unwrap(), 0).unwrap();
        let back = interop::parse_library(&std::fs::read_to_string(&xml).unwrap()).unwrap();
        assert_eq!(titles(&back), ["Export Title 3", "Export Title 1", "Export Title 0"]);
        assert_eq!((back.playlists.len(), back.playlists[0].name.as_str(), back.playlists[0].tracks.clone()), (1, "Long first", vec![0, 1, 2]));
        assert!(std::fs::read_dir(&out).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));

        // refusals: inside the device, wrong extension, unknown id / format, nothing matched
        assert!(export_one(&root, &cfg, sc.id, "m3u8", vol.join("x.m3u8").to_str().unwrap(), 0).is_err());
        assert!(export_one(&root, &cfg, sc.id, "m3u8", out.join("x.txt").to_str().unwrap(), 0).is_err());
        assert!(export_one(&root, &cfg, 99, "m3u8", out.join("y.m3u8").to_str().unwrap(), 0).unwrap_err().to_string().contains("no smart case"));
        assert!(export_one(&root, &cfg, sc.id, "pdf", out.join("y.pdf").to_str().unwrap(), 0).is_err());
        assert!(export_one(&root, &cfg, empty.id, "csv", out.join("z.csv").to_str().unwrap(), 0).unwrap_err().to_string().contains("matches no tracks"));
        assert!(!out.join("y.m3u8").exists() && !out.join("z.csv").exists());

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&cfg).unwrap();
        std::fs::remove_dir_all(&out).unwrap();
    }
}
