use super::*;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pm-watch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn file(d: &Path, name: &str, size: usize) -> PathBuf {
    let p = d.join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, vec![0u8; size]).unwrap();
    p
}

fn track(id: i64, title: &str, artist: &str, secs: i64, size: i64, location: &str) -> Track {
    Track {
        track_id: id,
        title: title.into(),
        artist: artist.into(),
        album: String::new(),
        genre: String::new(),
        bpm: 0.0,
        key: String::new(),
        play_time_secs: secs,
        rating: 0,
        location: location.into(),
        file_size: size,
        format: "mp3".into(),
        analysed: true,
        times_played: 0,
        last_played: -1,
        year: 0,
        label: String::new(),
        remixer: String::new(),
        composer: String::new(),
        comments: String::new(),
        date_added: -1,
        bit_rate: 0,
    }
}

fn loc(name: &str, size: u64) -> String {
    naming::device_paths(name, size).1
}

fn settings(folders: &[&Path]) -> Settings {
    Settings { folders: folders.iter().map(|f| f.display().to_string()).collect(), min_age_secs: 0, ..Settings::default() }
}

fn no_tags(p: &Path) -> FileTags {
    FileTags { title: p.file_stem().unwrap().to_string_lossy().into_owned(), ..FileTags::default() }
}

const NOW: i64 = 4_000_000_000_000;

fn run(s: &Settings, root: &str, tracks: &[Track], ignored: &[Ignored], tags: &dyn Fn(&Path) -> FileTags) -> Scan {
    scan_folders(s, root, &LibIndex::new(tracks), ignored, NOW, tags)
}

#[test]
fn same_name_and_size_is_in_the_library_whatever_the_extension_case() {
    let d = dir("exact");
    file(&d, "Alpha One.mp3", 10);
    file(&d, "Beta Two.MP3", 12);
    file(&d, "Gamma Three.mp3", 14);
    let tracks = [
        track(1, "x", "y", 0, 0, &loc("Alpha One.mp3", 10)),
        // the library spelled the extension in lower case, the file in upper
        track(2, "x2", "y2", 0, 0, &loc("Beta Two.mp3", 12)),
    ];
    let s = run(&settings(&[&d]), "Q:\\", &tracks, &[], &no_tags);
    assert_eq!(s.audio_files, 3);
    assert_eq!(s.in_library, 2);
    assert_eq!(s.candidates.len(), 1);
    assert_eq!(s.candidates[0].name, "Gamma Three.mp3");
    assert_eq!(s.candidates[0].size, 14);
    assert_eq!(s.candidates[0].folder, d.display().to_string());
}

#[test]
fn same_name_with_another_size_is_a_different_file() {
    let d = dir("size");
    file(&d, "Alpha One.mp3", 11);
    let tracks = [track(1, "x", "y", 0, 0, &loc("Alpha One.mp3", 10))];
    assert_eq!(run(&settings(&[&d]), "Q:\\", &tracks, &[], &no_tags).candidates.len(), 1);
}

#[test]
fn non_audio_files_are_not_listed() {
    let d = dir("nonaudio");
    file(&d, "notes.txt", 5);
    file(&d, "cover.jpg", 5);
    file(&d, "song.mp3.part", 5);
    let s = run(&settings(&[&d]), "Q:\\", &[], &[], &no_tags);
    assert_eq!((s.audio_files, s.candidates.len()), (0, 0));
}

#[test]
fn a_renamed_file_with_the_same_size_matches_the_title() {
    let d = dir("title");
    file(&d, "some   SONG.mp3", 20);
    file(&d, "Artist X - Other Tune.flac", 30);
    file(&d, "unrelated.mp3", 40);
    let tracks = [
        track(1, "Some Song", "Artist X", 0, 20, "/pmdb_tracks/1/1/aa.mp3"),
        track(2, "Other Tune", "Artist X", 0, 30, "/pmdb_tracks/1/2/bb.mp3"),
    ];
    let s = run(&settings(&[&d]), "Q:\\", &tracks, &[], &no_tags);
    assert_eq!(s.matched_total, 2);
    assert_eq!(s.candidates.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["unrelated.mp3"]);
    assert!(s.matched.iter().all(|m| m.how.contains("size")));
}

#[test]
fn same_title_in_the_name_with_another_size_is_not_a_match() {
    let d = dir("titlesize");
    file(&d, "Some Song.mp3", 21);
    let tracks = [track(1, "Some Song", "A", 0, 20, "/pmdb_tracks/1/1/aa.mp3")];
    assert_eq!(run(&settings(&[&d]), "Q:\\", &tracks, &[], &no_tags).candidates.len(), 1);
}

#[test]
fn tags_match_needs_title_artist_and_a_close_length() {
    let d = dir("tags");
    file(&d, "close.mp3", 50);
    file(&d, "far.mp3", 51);
    file(&d, "nolen.mp3", 52);
    file(&d, "other.mp3", 53);
    let tracks = [track(1, "  The   Tune ", "Some Artist", 200, 999, "/pmdb_tracks/1/1/aa.mp3")];
    let tags = |p: &Path| {
        let n = p.file_stem().unwrap().to_string_lossy().into_owned();
        let (title, artist, secs) = match n.as_str() {
            "close" => ("the tune", "SOME ARTIST", Some(201.9)),
            "far" => ("The Tune", "Some Artist", Some(210.0)),
            "nolen" => ("The Tune", "Some Artist", None),
            _ => ("Different", "Some Artist", Some(200.0)),
        };
        FileTags { title: title.into(), artist: artist.into(), duration_secs: secs }
    };
    let s = run(&settings(&[&d]), "Q:\\", &tracks, &[], &tags);
    assert_eq!(s.matched_total, 1);
    assert_eq!(s.matched[0].name, "close.mp3");
    assert_eq!(s.matched[0].track_id, 1);
    let mut left: Vec<_> = s.candidates.iter().map(|c| c.name.as_str()).collect();
    left.sort();
    assert_eq!(left, ["far.mp3", "nolen.mp3", "other.mp3"]);
}

#[test]
fn the_length_tolerance_is_a_setting() {
    let d = dir("tolerance");
    file(&d, "near.mp3", 70);
    let tracks = [track(1, "Tune", "A", 200, 999, "/pmdb_tracks/1/1/aa.mp3")];
    let tags = |_: &Path| FileTags { title: "Tune".into(), artist: "A".into(), duration_secs: Some(205.0) };
    let mut s = settings(&[&d]);
    assert_eq!(run(&s, "Q:\\", &tracks, &[], &tags).candidates.len(), 1, "5 s apart is not a match by default");
    s.length_tolerance_secs = 5.0;
    let wide = run(&s, "Q:\\", &tracks, &[], &tags);
    assert_eq!((wide.matched_total, wide.candidates.len()), (1, 0));
    s.length_tolerance_secs = 4.9;
    assert_eq!(run(&s, "Q:\\", &tracks, &[], &tags).candidates.len(), 1);
    // a damaged value is brought back to something usable
    let bad = Settings { length_tolerance_secs: f64::NAN, ..Settings::default() }.sanitised();
    assert_eq!(bad.length_tolerance_secs, 2.5);
    assert_eq!(Settings { length_tolerance_secs: -3.0, ..Settings::default() }.sanitised().length_tolerance_secs, 0.0);
    assert_eq!(Settings { length_tolerance_secs: 900.0, ..Settings::default() }.sanitised().length_tolerance_secs, 30.0);
}

#[test]
fn the_tag_cache_reads_a_file_once_and_again_when_it_changes() {
    use std::cell::{Cell, RefCell};
    let d = dir("cache");
    let f = file(&d, "song.mp3", 10);
    let reads = Cell::new(0);
    let reader = |_: &Path| {
        reads.set(reads.get() + 1);
        FileTags { title: format!("read {}", reads.get()), ..FileTags::default() }
    };
    let mut c = TagCache::default();
    assert_eq!(c.get_or_read(&f, &reader).title, "read 1");
    assert_eq!(c.get_or_read(&f, &reader).title, "read 1");
    assert_eq!(reads.get(), 1);
    // a different size is a different file
    std::fs::write(&f, vec![0u8; 11]).unwrap();
    assert_eq!(c.get_or_read(&f, &reader).title, "read 2");
    assert_eq!(c.get_or_read(&f, &reader).title, "read 2");
    // a file that cannot be examined is read every time and never remembered
    let gone = d.join("absent.mp3");
    c.get_or_read(&gone, &reader);
    c.get_or_read(&gone, &reader);
    assert_eq!(reads.get(), 4);

    // used the way a scan uses it: the second scan of unchanged files reads none
    let d2 = dir("cache-scan");
    file(&d2, "a.mp3", 21);
    file(&d2, "b.mp3", 22);
    let tracks = [track(1, "zzz", "zzz", 100, 1, "/pmdb_tracks/1/1/aa.mp3")];
    let calls = Cell::new(0);
    let counting = |p: &Path| {
        calls.set(calls.get() + 1);
        no_tags(p)
    };
    let cache = RefCell::new(TagCache::default());
    let cached = |p: &Path| cache.borrow_mut().get_or_read(p, &counting);
    let s = settings(&[&d2]);
    assert_eq!(run(&s, "Q:\\", &tracks, &[], &cached).candidates.len(), 2);
    assert_eq!(calls.get(), 2);
    assert_eq!(run(&s, "Q:\\", &tracks, &[], &cached).candidates.len(), 2);
    assert_eq!(calls.get(), 2, "unchanged files are not read again");
}

#[test]
fn two_lookalike_library_tracks_make_a_tag_match_a_guess_so_it_is_not_made() {
    let d = dir("lookalike");
    file(&d, "x.mp3", 60);
    let tracks = [
        track(1, "Tune", "A", 100, 1, "/pmdb_tracks/1/1/aa.mp3"),
        track(2, "Tune", "A", 101, 1, "/pmdb_tracks/1/2/bb.mp3"),
    ];
    let tags = |_: &Path| FileTags { title: "Tune".into(), artist: "A".into(), duration_secs: Some(100.5) };
    let s = run(&settings(&[&d]), "Q:\\", &tracks, &[], &tags);
    assert_eq!((s.matched_total, s.candidates.len()), (0, 1));
}

#[test]
fn tags_are_not_read_for_files_the_name_and_size_already_settle() {
    let d = dir("lazy");
    file(&d, "known.mp3", 10);
    let tracks = [track(1, "x", "y", 0, 0, &loc("known.mp3", 10))];
    let boom = |_: &Path| -> FileTags { panic!("tags read for a file that is already known") };
    assert_eq!(run(&settings(&[&d]), "Q:\\", &tracks, &[], &boom).in_library, 1);
}

#[test]
fn ignored_files_stay_out_until_they_change() {
    let d = dir("ignore");
    let a = file(&d, "a.mp3", 10);
    file(&d, "b.mp3", 10);
    let ig = [Ignored { path: a.display().to_string(), size: 10, at: 1 }];
    let s = run(&settings(&[&d]), "Q:\\", &[], &ig, &no_tags);
    assert_eq!((s.ignored, s.candidates.len()), (1, 1));
    assert_eq!(s.candidates[0].name, "b.mp3");
    // the file changed size (it is a different file now): offered again
    let ig = [Ignored { path: a.display().to_string(), size: 9, at: 1 }];
    assert_eq!(run(&settings(&[&d]), "Q:\\", &[], &ig, &no_tags).candidates.len(), 2);
}

#[test]
fn a_second_copy_with_the_same_name_and_size_is_counted_not_offered() {
    let (d1, d2) = (dir("dup1"), dir("dup2"));
    file(&d1, "same.mp3", 10);
    file(&d2, "same.mp3", 10);
    file(&d2, "same.mp3.bak.mp3", 10);
    let s = run(&settings(&[&d1, &d2]), "Q:\\", &[], &[], &no_tags);
    assert_eq!(s.candidates.len(), 2);
    assert_eq!(s.duplicates, 1);
}

#[test]
fn nested_watch_folders_do_not_list_a_file_twice() {
    let d = dir("nested");
    file(&d, "sub/one.mp3", 10);
    let sub = d.join("sub");
    let s = run(&settings(&[&d, &sub]), "Q:\\", &[], &[], &no_tags);
    assert_eq!((s.audio_files, s.candidates.len()), (1, 1));
}

#[test]
fn files_still_being_written_wait_for_the_next_scan() {
    let d = dir("young");
    let p = file(&d, "fresh.mp3", 10);
    let m = modified_ms(&std::fs::metadata(&p).unwrap());
    let mut set = settings(&[&d]);
    set.min_age_secs = 10;
    let at = |now| scan_folders(&set, "Q:\\", &LibIndex::new(&[]), &[], now, &no_tags);
    let young = at(m + 2_000);
    assert_eq!((young.too_new, young.candidates.len()), (1, 0));
    let old = at(m + 10_000);
    assert_eq!((old.too_new, old.candidates.len()), (0, 1));
    set.min_age_secs = 0;
    assert_eq!(scan_folders(&set, "Q:\\", &LibIndex::new(&[]), &[], m, &no_tags).candidates.len(), 1);
}

#[test]
fn settled_handles_a_clock_that_is_wrong() {
    assert!(is_settled(1_000, 1_000, 0));
    assert!(!is_settled(10_000, 5_000, 10));
    assert!(is_settled(15_000, 5_000, 10));
    // dated 30 s in the future: probably being written right now
    assert!(!is_settled(0, 30_000, 10));
    // dated hours ahead: the clock is off, the file is not "new"
    assert!(is_settled(0, 3_600_000, 10));
}

#[test]
fn a_folder_inside_the_library_is_refused_and_the_librarys_own_files_are_never_listed() {
    let root = dir("lib");
    file(&root, ".Pacemaker/Music/1/2/aa.mp3", 10);
    file(&root, "loose.mp3", 10);
    let inside = root.join(".Pacemaker");
    let s = run(&settings(&[&inside]), &root.display().to_string(), &[], &[], &no_tags);
    assert!(s.folders[0].error.as_deref().unwrap().contains("inside the open library"));
    assert!(s.candidates.is_empty());
    // watching the folder that holds the library: its own files are skipped, others are not
    let parent = std::env::temp_dir().join(format!("pm-watch-libparent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&parent);
    let lib2 = parent.join("card");
    file(&lib2, "x.mp3", 10);
    file(&parent, "mine.mp3", 10);
    let s = run(&settings(&[&parent]), &lib2.display().to_string(), &[], &[], &no_tags);
    assert_eq!(s.candidates.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["mine.mp3"]);
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn a_sibling_with_the_same_prefix_is_not_inside_the_library() {
    assert!(is_under("P:\\Music\\a.mp3", "P:\\"));
    assert!(is_under("/mnt/card/x", "/mnt/card"));
    assert!(!is_under("/mnt/card2/x", "/mnt/card"));
    assert!(is_under("/mnt/card", "/mnt/card/"));
    assert!(!is_under("/mnt", "/mnt/card"));
}

#[test]
fn subfolders_can_be_switched_off_and_a_missing_folder_is_reported() {
    let d = dir("sub");
    file(&d, "top.mp3", 10);
    file(&d, "deep/down/low.mp3", 11);
    let mut s = settings(&[&d]);
    assert_eq!(run(&s, "Q:\\", &[], &[], &no_tags).candidates.len(), 2);
    s.subfolders = false;
    assert_eq!(run(&s, "Q:\\", &[], &[], &no_tags).candidates.len(), 1);
    let gone = d.join("nope");
    let r = run(&settings(&[&gone, &d]), "Q:\\", &[], &[], &no_tags);
    assert!(r.folders[0].error.as_deref().unwrap().contains("not found"));
    assert!(r.folders[1].error.is_none());
    assert_eq!(r.candidates.len(), 2, "one bad folder does not stop the others");
}

#[test]
fn newest_files_come_first() {
    let d = dir("order");
    file(&d, "a.mp3", 1);
    file(&d, "b.mp3", 2);
    let s = run(&settings(&[&d]), "Q:\\", &[], &[], &no_tags);
    assert!(s.candidates[0].modified_ms >= s.candidates[1].modified_ms);
    assert_eq!(s.candidates_total, 2);
}

#[test]
fn settings_are_clamped_and_folders_deduplicated() {
    let s = Settings {
        folders: vec![" C:\\Music ".into(), "c:\\music\\".into(), "".into(), "   ".into(), "D:\\More".into()],
        interval_minutes: 99_999,
        min_age_secs: 99_999,
        auto_max_files: 0,
        ..Settings::default()
    }
    .sanitised();
    assert_eq!(s.folders.len(), if cfg!(windows) { 2 } else { 3 });
    assert_eq!(s.folders[0], "C:\\Music");
    assert_eq!((s.interval_minutes, s.min_age_secs, s.auto_max_files), (1440, 3600, 1));
    let many = Settings { folders: (0..40).map(|i| format!("/x/{i}")).collect(), ..Settings::default() }.sanitised();
    assert_eq!(many.folders.len(), MAX_FOLDERS);
    assert!(Settings { folders: vec!["relative/dir".into()], ..Settings::default() }.check().is_err());
    assert!(Settings::default().check().is_ok());
}

#[test]
fn defaults_are_ask_first_scan_on_open_no_timer() {
    let s = Settings::default();
    assert_eq!(s.mode, Mode::Ask);
    assert!(s.scan_on_open && s.subfolders && !s.open_confirmation);
    assert_eq!(s.interval_minutes, 0);
    assert!(s.folders.is_empty());
}

#[test]
fn settings_round_trip_and_a_damaged_file_is_moved_aside_not_deleted() {
    let d = dir("settings");
    // nothing saved: the defaults, no complaint
    let v = load_settings(&d, 5).unwrap();
    assert_eq!(v.settings, Settings::default());
    assert!(v.problem.is_none());
    let s = Settings { folders: vec!["/music".into()], mode: Mode::Auto, interval_minutes: 15, ..Settings::default() };
    save_settings(&d, &s).unwrap();
    assert_eq!(load_settings(&d, 5).unwrap().settings, s);
    assert!(!d.join("watch-settings.json.tmp").exists());
    // a file with fields missing or from the future still loads
    std::fs::write(settings_path(&d), r#"{"mode":"auto","folders":["/a"],"from_the_future":1}"#).unwrap();
    let v = load_settings(&d, 5).unwrap();
    assert_eq!((v.settings.mode, v.settings.folders.clone(), v.settings.min_age_secs), (Mode::Auto, vec!["/a".to_string()], 10));
    // damaged
    std::fs::write(settings_path(&d), "not json").unwrap();
    let v = load_settings(&d, 77).unwrap();
    assert_eq!(v.settings, Settings::default());
    assert!(v.problem.unwrap().contains("watch-settings.bad-77"));
    assert_eq!(std::fs::read_to_string(d.join("watch-settings.bad-77")).unwrap(), "not json");
    assert!(!settings_path(&d).exists());
}

#[test]
fn state_files_are_per_library_and_the_drive_spelling_does_not_matter() {
    let d = Path::new("cfg");
    assert_eq!(state_path(d, "P:\\"), state_path(d, "p:/"));
    assert_ne!(state_path(d, "P:\\"), state_path(d, "Q:\\"));
}

#[test]
fn ignore_list_adds_once_and_removes_by_path() {
    let mut st = WatchState::default();
    add_ignored(&mut st, &[("/m/a.mp3".into(), 5), ("/m/a.mp3".into(), 5), ("/m/a.mp3".into(), 6), ("/m/b.mp3".into(), 1)], 9);
    assert_eq!(st.ignored.len(), 3);
    remove_ignored(&mut st, &["/m/a.mp3".into()]);
    assert_eq!(st.ignored.len(), 1);
    assert_eq!(st.ignored[0].path, "/m/b.mp3");
    remove_ignored(&mut st, &["/m/never.mp3".into()]);
    assert_eq!(st.ignored.len(), 1);
}

#[test]
fn state_round_trips_with_scan_ignore_list_and_a_capped_log() {
    let d = dir("state");
    let mut st = load_state(&d, "P:\\", 1).unwrap().state;
    assert_eq!(st, WatchState::default());
    st.last_scan = Some(Scan { at: 5, root: "P:\\".into(), candidates_total: 1, ..Scan::default() });
    add_ignored(&mut st, &[("/m/a.mp3".into(), 5)], 9);
    for i in 0..(MAX_LOG + 5) {
        push_log(
            &mut st,
            LogEntry { at: i as i64, mode: "weird".into(), requested: 1, names: vec!["n".repeat(400)], ok: 1, failed: 0, summary: "s".repeat(900) },
        );
    }
    save_state(&d, "P:\\", &st).unwrap();
    let back = load_state(&d, "p:/", 1).unwrap();
    assert!(back.problem.is_none());
    assert_eq!(back.state, st);
    assert_eq!(back.state.log.len(), MAX_LOG);
    assert_eq!(back.state.log[0].at, 5, "oldest entries are dropped");
    assert_eq!(back.state.log[0].mode, "ask");
    assert_eq!(back.state.log[0].names[0].len(), 260);
    assert_eq!(back.state.log[0].summary.len(), 500);
    // another library has its own, empty state
    assert_eq!(load_state(&d, "Q:\\", 1).unwrap().state, WatchState::default());
    // damaged: set aside, empty state, a note
    std::fs::write(state_path(&d, "P:\\"), "{").unwrap();
    let v = load_state(&d, "P:\\", 3).unwrap();
    assert_eq!(v.state, WatchState::default());
    assert!(v.problem.is_some());
}

#[test]
fn dropping_ignored_candidates_keeps_the_totals_straight() {
    let c = |p: &str| Candidate { path: p.into(), name: p.into(), size: 1, modified_ms: 0, folder: "/".into() };
    let mut scan = Scan { candidates: vec![c("/a.mp3"), c("/b.mp3"), c("/c.mp3")], candidates_total: 7, ignored: 2, ..Scan::default() };
    drop_candidates(&mut scan, &[("/b.mp3".into(), 1), ("/zzz.mp3".into(), 1), ("/c.mp3".into(), 2)]);
    assert_eq!(scan.candidates.len(), 2);
    assert_eq!((scan.candidates_total, scan.ignored), (6, 3));
}

/// A mono 16-bit WAV of `secs` seconds of silence.
fn wav(path: &Path, secs: u32) {
    let sr = 8000u32;
    let data = (sr * secs * 2) as usize;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&sr.to_le_bytes());
    b.extend_from_slice(&(sr * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data as u32).to_le_bytes());
    b.resize(b.len() + data, 0);
    std::fs::write(path, b).unwrap();
}

#[test]
fn read_tags_gives_the_length_and_the_file_name_for_a_missing_title() {
    let d = dir("readtags");
    let w = d.join("Plain Name.wav");
    wav(&w, 3);
    let t = read_tags(&w);
    assert_eq!(t.title, "Plain Name");
    assert_eq!(t.artist, "");
    assert!((t.duration_secs.unwrap() - 3.0).abs() < 0.05, "{t:?}");
    // garbage: no length, so it can never match by tags
    let junk = file(&d, "junk.mp3", 100);
    let t = read_tags(&junk);
    assert_eq!((t.title.as_str(), t.duration_secs), ("junk", None));
    // a missing file does not panic
    assert_eq!(read_tags(&d.join("absent.flac")).title, "absent");
}

#[test]
fn a_real_wav_goes_through_the_real_tag_reader_and_matches_by_title_and_length() {
    let d = dir("realwav");
    wav(&d.join("Some Title.wav"), 4);
    let size = std::fs::metadata(d.join("Some Title.wav")).unwrap().len() as i64;
    // a library track made from this recording, but added under another file name
    let tracks = [track(7, "Some Title", "", 4, size + 1, "/pmdb_tracks/1/1/zz.wav")];
    let s = run(&settings(&[&d]), "Q:\\", &tracks, &[], &read_tags);
    assert_eq!(s.matched_total, 1, "{s:?}");
    assert_eq!(s.matched[0].track_id, 7);
    assert!(s.matched[0].how.contains("length"));
    assert!(s.candidates.is_empty());
}
