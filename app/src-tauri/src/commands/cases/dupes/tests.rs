//! Tests of the duplicate finder on a synthetic library in a temp dir: finding, the fingerprint
//! job and its cache, the merge plan, and the merge with and without deleting files. Synthetic
//! names and audio only.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use pm_formats::library::{Library, MergeGroup, PlaysRule, RatingRule, TrackMeta};
use pm_formats::strfile::{Analysis, Beat};

use super::*;

struct Fixture {
    vol: PathBuf,
    src: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let base = std::env::temp_dir().join(format!("pm-dupes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let f = Fixture { vol: base.join("vol"), src: base.join("src"), cache: base.join("cache") };
        for d in [&f.vol, &f.src, &f.cache] {
            std::fs::create_dir_all(d).unwrap();
        }
        Library::create(&f.vol).unwrap();
        f
    }

    fn fp_file(&self) -> PathBuf {
        self.cache.join("lib.fp")
    }

    /// Adds a file named `file` with these contents and tags; returns the track id.
    fn add(&self, file: &str, bytes: &[u8], artist: &str, title: &str, secs: i64, kbps: i64) -> i64 {
        let src = self.src.join(file);
        std::fs::write(&src, bytes).unwrap();
        let analysis = Analysis { normalize_factor: 1.0, beat_start_offset: 0, beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(), first_downbeat: Some(0) };
        let meta = TrackMeta { title: title.into(), artist: artist.into(), format: "MP3".into(), play_time_secs: secs, bit_rate_kbps: kbps, ..Default::default() };
        Library::open(&self.vol).unwrap().add_track(&src, &meta, &analysis).unwrap()
    }

    fn lib(&self) -> Library {
        Library::open(&self.vol).unwrap()
    }

    fn rows(&self) -> Vec<DupRow> {
        Library::open_read_only(&self.vol).unwrap().dup_rows().unwrap()
    }

    fn audio_of(&self, id: i64) -> PathBuf {
        let r = self.rows().into_iter().find(|r| r.track_id == id).unwrap();
        audio_path(&self.vol, &r.location).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.vol.parent().unwrap());
    }
}

fn only(s: &DupSettings, tags: bool, size: bool, audio: bool) -> DupSettings {
    DupSettings { by_tags: tags, by_size: size, by_audio: audio, ..s.clone() }
}

#[test]
fn tags_and_size_find_groups_with_cases_files_and_the_suggested_keeper() {
    let f = Fixture::new("find");
    let a1 = f.add("a1.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 128);
    let a2 = f.add("a2.mp3", &[2; 3500], "sample artist", "Sample Title (feat. Guest)", 201, 320);
    let b1 = f.add("b1.mp3", &[3; 4000], "Other Artist", "First Title", 100, 192);
    let b2 = f.add("b2.mp3", &[4; 4000], "Third Artist", "Second Title", 100, 192);
    let c = f.add("c.mp3", &[5; 5000], "Solo Artist", "Solo Title", 90, 128);
    let case = {
        let mut lib = f.lib();
        let case = lib.create_case("Sample Case").unwrap();
        lib.set_case_tracks(case, &[a1, c]).unwrap();
        case
    };
    std::fs::remove_file(f.audio_of(b2)).unwrap();

    let s = DupSettings::default();
    let r = find_in(&f.vol, &f.fp_file(), &s).unwrap();
    assert_eq!(r.tracks, 5);
    assert_eq!(r.groups.len(), 2);
    let by_tags = r.groups.iter().find(|g| g.id == a1).unwrap();
    assert_eq!(by_tags.members.iter().map(|m| m.row.track_id).collect::<Vec<_>>(), vec![a1, a2]);
    assert_eq!(by_tags.modes, vec![Mode::Tags]);
    assert_eq!(by_tags.suggested_keeper, a2, "320 kbps beats 128");
    assert_eq!(by_tags.members[0].cases.iter().map(|c| (c.case_id, c.position)).collect::<Vec<_>>(), vec![(case, 1)]);
    assert!(by_tags.members[1].cases.is_empty());
    assert!(by_tags.members.iter().all(|m| m.file == FileState::Present));
    let by_size = r.groups.iter().find(|g| g.id == b1).unwrap();
    assert_eq!(by_size.modes, vec![Mode::Size]);
    assert_eq!(by_size.suggested_keeper, b1, "the copy whose file is missing is not suggested");
    assert_eq!(by_size.members[1].file, FileState::Missing);

    // the keeper rule is a setting
    let plays = find_in(&f.vol, &f.fp_file(), &DupSettings { keeper_rule: super::super::dupes_settings::KeeperRule::Newest, ..s.clone() }).unwrap();
    assert_eq!(plays.groups.iter().find(|g| g.id == a1).unwrap().suggested_keeper, a2, "added later");
    // each way can be turned off
    assert_eq!(find_in(&f.vol, &f.fp_file(), &only(&s, false, true, false)).unwrap().groups.len(), 1);
    assert_eq!(find_in(&f.vol, &f.fp_file(), &only(&s, false, false, false)).unwrap().groups.len(), 0);
}

#[test]
fn a_location_outside_the_music_folder_is_never_turned_into_a_path() {
    assert_eq!(rel_of("/pmdb_tracks/1/2/abc.mp3").as_deref(), Some("1/2/abc.mp3"));
    for bad in ["/pmdb_tracks/../x.mp3", "/pmdb_tracks//x.mp3", "/pmdb_tracks/a/../../x", "/pmdb_tracks/C:/x.mp3", "/pmdb_tracks/a\\b.mp3", "/elsewhere/x.mp3", "x.mp3", "/pmdb_tracks/", "/pmdb_tracks/a//b"] {
        assert!(rel_of(bad).is_none(), "{bad}");
        assert!(audio_path(Path::new("D:/vol"), bad).is_none(), "{bad}");
        assert_eq!(file_state(Path::new("D:/vol"), bad), FileState::Outside);
    }
}

/// A 20 second "song" of 0.25 s notes, as 16-bit mono 22.05 kHz WAV bytes; `gain` scales it.
fn wav_song(seed: u64, gain: f32) -> Vec<u8> {
    let sr = 22_050u32;
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let notes = [220.0f32, 246.9, 277.2, 311.1, 329.6, 370.0, 415.3, 440.0, 493.9, 554.4, 622.3, 659.3, 740.0, 830.6, 880.0, 987.8];
    let n = sr as usize * 20;
    let step = sr as usize / 4;
    let mut x = vec![0f32; n];
    let mut pos = 0;
    while pos < n {
        let f = notes[(next() % 16) as usize];
        let amp = 0.05 + 0.3 * ((next() >> 40) as f32 / 16_777_216.0);
        let h2 = 0.5 * ((next() >> 40) as f32 / 16_777_216.0);
        for i in 0..(step * 2).min(n - pos) {
            let t = i as f32 / sr as f32;
            let w = 2.0 * std::f32::consts::PI * f * t;
            x[pos + i] += amp * (-t * 6.0).exp() * (w.sin() + h2 * (2.0 * w).sin()) * 0.5;
        }
        pos += step;
    }
    let mut b = Vec::new();
    let data = (n * 2) as u32;
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(sr.to_le_bytes());
    b.extend((sr * 2).to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    for v in x {
        b.extend((((v * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

#[test]
fn the_fingerprint_job_fills_the_cache_and_the_sound_check_then_finds_the_copy() {
    let f = Fixture::new("audio");
    // the same song at two levels, a different song, and a file that is not audio
    let x = f.add("x.wav", &wav_song(1, 1.0), "A", "One", 20, 0);
    let y = f.add("y.wav", &wav_song(1, 0.5), "B", "Two", 20, 0);
    let z = f.add("z.wav", &wav_song(2, 1.0), "C", "Three", 20, 0);
    let bad = f.add("bad.wav", b"this is not audio at all", "D", "Four", 20, 0);
    let s = DupSettings { by_audio: true, by_tags: false, by_size: false, ..Default::default() };

    let before = find_in(&f.vol, &f.fp_file(), &s).unwrap();
    assert!(before.groups.is_empty());
    assert_eq!(before.pending, vec![x, y, z, bad], "everything with a same-length neighbour needs a fingerprint");

    let rows = f.rows();
    let refs: Vec<&DupRow> = rows.iter().collect();
    let (mut store, _) = Store::load(&f.fp_file());
    let (_, items) = audio_state(&f.vol, &store, &refs, &s);
    let cancel = AtomicBool::new(false);
    let mut seen = Vec::new();
    let out = fingerprint_run(items, &mut store, &f.fp_file(), 2, &cancel, &mut |p| seen.push((p.done, p.total, p.status.clone())));
    assert_eq!((out.done, out.failed.len(), out.unusable, out.cancelled), (3, 1, 0, false));
    assert!(out.failed[0].0.contains("D - Four"), "{:?}", out.failed);
    assert_eq!(seen.len(), 4);
    assert_eq!(seen.last().unwrap().0, 4);
    assert_eq!(seen.iter().filter(|(_, _, st)| st == "failed").count(), 1);
    assert!(out.summary().contains("Fingerprinted 3 of 4 tracks") && out.summary().contains("1 could not be read"));

    let after = find_in(&f.vol, &f.fp_file(), &s).unwrap();
    assert_eq!(after.pending, vec![bad], "only the unreadable file is still waiting");
    assert_eq!(after.fingerprinted, 3);
    assert_eq!(after.groups.len(), 1);
    let g = &after.groups[0];
    assert_eq!(g.members.iter().map(|m| m.row.track_id).collect::<Vec<_>>(), vec![x, y]);
    assert_eq!(g.modes, vec![Mode::Audio]);
    assert!(g.links[0].score.unwrap() > 0.95, "{:?}", g.links[0]);
    // a stricter threshold than the score hides it; the cache is not rebuilt for that
    assert!(find_in(&f.vol, &f.fp_file(), &DupSettings { audio_threshold: 1.0, ..s.clone() }).unwrap().groups.is_empty());

    // a replaced file is fingerprinted again
    std::fs::write(f.audio_of(y), wav_song(3, 1.0)).unwrap();
    let redo = find_in(&f.vol, &f.fp_file(), &s).unwrap();
    assert!(redo.pending.contains(&y) && redo.groups.is_empty());
}

#[test]
fn cancelling_stops_before_the_next_track_and_keeps_what_was_done() {
    let f = Fixture::new("cancel");
    for i in 0..4 {
        f.add(&format!("s{i}.wav"), &wav_song(i + 10, 1.0), "A", &format!("T{i}"), 20, 0);
    }
    let s = DupSettings { by_audio: true, ..Default::default() };
    let rows = f.rows();
    let refs: Vec<&DupRow> = rows.iter().collect();
    let (mut store, _) = Store::load(&f.fp_file());
    let (_, items) = audio_state(&f.vol, &store, &refs, &s);
    assert_eq!(items.len(), 4);
    let cancel = AtomicBool::new(false);
    // cancelled at the first progress line: the one worker may already have started the next
    // track (it is not interrupted), but never goes on to the third
    let out = fingerprint_run(items, &mut store, &f.fp_file(), 1, &cancel, &mut |_| cancel.store(true, Ordering::SeqCst));
    assert!(out.cancelled);
    assert!((1..=2).contains(&out.done), "{}", out.done);
    assert!(out.summary().contains("stopped at your request"));
    assert_eq!(Store::load(&f.fp_file()).0.len(), out.done, "what was finished is saved");
    let (_, left) = audio_state(&f.vol, &Store::load(&f.fp_file()).0, &refs, &s);
    assert_eq!(left.len(), 4 - out.done);
}

fn request(groups: Vec<MergeGroup>, remove_rows: bool, delete_files: bool) -> MergeRequest {
    MergeRequest { groups, remove_rows, delete_files, settings: DupSettings { allow_row_removal: true, ..Default::default() } }
}

#[test]
fn removing_rows_needs_the_setting_and_a_missing_keeper_file_is_a_warning() {
    let f = Fixture::new("warn");
    let k = f.add("k.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 320);
    let c = f.add("c.mp3", &[2; 3100], "Sample Artist", "Sample Title", 200, 128);
    let lib = Library::open_read_only(&f.vol).unwrap();
    let g = || vec![MergeGroup { keeper: k, others: vec![c] }];

    // the setting is checked in the plan, which both the preview and the job use
    let mut off = request(g(), true, false);
    off.settings.allow_row_removal = false;
    assert!(build_plan(&lib, &off).unwrap_err().to_string().contains("switched off"));
    off.remove_rows = false;
    assert!(build_plan(&lib, &off).is_ok(), "a merge that removes nothing needs no permission");

    // files present on both sides: nothing to warn about
    assert!(build_plan(&lib, &request(g(), true, true)).unwrap().warnings.is_empty());
    // the keeper's file gone while the copy's is deleted: red warning, and it is part of the digest
    let ok_digest = build_plan(&lib, &request(g(), true, true)).unwrap().digest;
    std::fs::remove_file(f.audio_of(k)).unwrap();
    let p = build_plan(&lib, &request(g(), true, true)).unwrap();
    assert_eq!(p.warnings.len(), 1, "{:?}", p.warnings);
    assert!(p.warnings[0].contains("Sample Artist - Sample Title") && p.warnings[0].contains("no playable file"), "{}", p.warnings[0]);
    assert_ne!(p.digest, ok_digest);
    // not deleting files, or keeping the copy that still has its file, is fine
    assert!(build_plan(&lib, &request(g(), true, false)).unwrap().warnings.is_empty());
    let swapped = vec![MergeGroup { keeper: c, others: vec![k] }];
    assert!(build_plan(&lib, &request(swapped, true, true)).unwrap().warnings.is_empty(), "the deleted copy has no file to lose");
}

#[test]
fn the_plan_is_checked_and_the_same_every_time() {
    let f = Fixture::new("plan");
    let k = f.add("k.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 320);
    let c = f.add("c.mp3", &[2; 3100], "Sample Artist", "Sample Title", 200, 128);
    let lib = Library::open_read_only(&f.vol).unwrap();
    let g = || vec![MergeGroup { keeper: k, others: vec![c] }];

    assert!(build_plan(&lib, &request(vec![], true, false)).unwrap_err().to_string().contains("at least one group"));
    assert!(build_plan(&lib, &request(g(), false, true)).unwrap_err().to_string().contains("needs the copies to be removed"));
    assert!(build_plan(&lib, &request(vec![MergeGroup { keeper: k, others: vec![k] }], true, false)).is_err());

    let a = build_plan(&lib, &request(g(), true, true)).unwrap();
    let b = build_plan(&lib, &request(g(), true, true)).unwrap();
    assert_eq!(a.digest, b.digest);
    assert_ne!(a.digest, build_plan(&lib, &request(g(), true, false)).unwrap().digest, "listing files changes the plan");
    assert_eq!(a.files.iter().map(|x| (x.kind, x.exists, x.kept_because.is_none())).collect::<Vec<_>>(), vec![("audio", true, true), ("grid", true, true)]);
    assert_eq!((a.totals.groups, a.totals.rows_removed, a.totals.files_deleted, a.totals.bytes), (1, 1, 2, a.files.iter().map(|x| x.size).sum()));
    assert!(a.labels.iter().any(|l| l.label == "Sample Artist - Sample Title" && l.track_id == c));
    // without deleting files no file is listed
    assert!(build_plan(&lib, &request(g(), true, false)).unwrap().files.is_empty());
}

#[test]
fn a_file_another_track_uses_is_kept_whatever_is_asked() {
    // pure check on rows: the keeper and the copy point at one file, in different spelling of case
    let mk = |id: i64, location: &str| DupRow {
        track_id: id,
        title: String::new(),
        artist: String::new(),
        album_artist: String::new(),
        album: String::new(),
        composer: String::new(),
        genre: String::new(),
        label: String::new(),
        key: String::new(),
        comments: String::new(),
        producer: String::new(),
        remixer: String::new(),
        year: 0,
        track_number: 0,
        disc_number: 0,
        format: String::new(),
        bit_rate_kbps: 0,
        sample_rate: 0,
        file_size: 0,
        play_time_secs: 0,
        bpm: 0.0,
        rating: 0,
        times_played: 0,
        last_played: -1,
        date_added: 0,
        location: location.into(),
        analysed: true,
    };
    let rows = vec![mk(1, "/pmdb_tracks/A/B/FILE.mp3"), mk(2, "/pmdb_tracks/a/b/file.MP3"), mk(3, "/pmdb_tracks/c/d/other.mp3"), mk(4, "/pmdb_tracks/../bad.mp3")];
    let removed: HashSet<i64> = [2, 3, 4].into();
    let in_use = files_in_use(&rows, &removed);
    assert!(in_use.contains("a/b/file.mp3"), "the keeper's file stays in use under FAT's case rules");
    assert!(!in_use.contains("c/d/other.mp3"), "a removed copy's own file is free");
    assert_eq!(in_use.len(), 1, "an unsafe location is never in the set");
}

#[test]
fn merging_moves_cases_keeps_the_best_stats_and_deletes_only_what_was_confirmed() {
    let f = Fixture::new("merge");
    let k = f.add("k.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 320);
    let c1 = f.add("c1.mp3", &[2; 3100], "Sample Artist", "Sample Title", 200, 128);
    let c2 = f.add("c2.mp3", &[3; 3200], "Sample Artist", "Sample Title", 201, 128);
    let other = f.add("o.mp3", &[4; 3300], "Another Artist", "Another Title", 150, 128);
    {
        let mut lib = f.lib();
        let case = lib.create_case("Sample Case").unwrap();
        lib.set_case_tracks(case, &[other, c1]).unwrap();
        lib.set_track_rating(c1, 4).unwrap();
        lib.set_track_rating(k, 1).unwrap();
    }
    let groups = vec![MergeGroup { keeper: k, others: vec![c1, c2] }];
    let mut req = request(groups, true, true);
    req.settings.plays_rule = PlaysRule::Sum;
    req.settings.rating_rule = RatingRule::Max;
    let plan = build_plan(&Library::open_read_only(&f.vol).unwrap(), &req).unwrap();
    assert_eq!(plan.totals.files_deleted, 4);
    let (c1_audio, c2_audio, k_audio, o_audio) = (f.audio_of(c1), f.audio_of(c2), f.audio_of(k), f.audio_of(other));

    let mut lines = Vec::new();
    let out = merge_run(&f.vol, &plan, &req, &mut |p| lines.push((p.label, p.status))).unwrap();
    assert_eq!((out.merged, out.removed, out.files_deleted, out.case_entries), (1, 2, 4, 1));
    assert!(out.failed.is_empty() && out.delete_failed.is_empty() && out.skipped_files == 0);
    assert!(out.summary().starts_with("Merged 1 group, 1 case place moved to the kept track, 2 copies removed from the library, 4 files deleted"), "{}", out.summary());
    assert!(lines.iter().any(|(l, st)| l.starts_with("Merging Sample Artist - Sample Title") && st == "merged"));
    assert!(lines.iter().any(|(_, st)| st == "deleting"));

    let rows = f.rows();
    assert_eq!(rows.iter().map(|r| r.track_id).collect::<Vec<_>>(), vec![k, other]);
    assert_eq!(rows[0].rating, 4, "the best rating");
    let lib = f.lib();
    let case = lib.case_infos().unwrap()[0].id;
    assert_eq!(lib.case_track_ids(case).unwrap(), vec![other, k], "the keeper took the copy's place");
    assert!(lib.compatibility_problems().unwrap().is_empty());
    assert!(!c1_audio.exists() && !c2_audio.exists(), "the copies' audio is gone");
    assert!(!PathBuf::from(naming::str_path(&c1_audio.to_string_lossy())).exists(), "and their grids");
    assert!(k_audio.is_file() && o_audio.is_file(), "nothing else was touched");
}

#[test]
fn removing_rows_and_deleting_files_are_separate_choices() {
    let f = Fixture::new("separate");
    let k = f.add("k.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 320);
    let c = f.add("c.mp3", &[2; 3100], "Sample Artist", "Sample Title", 200, 128);
    let c_audio = f.audio_of(c);

    // merge only: the copy stays in the library with its file
    let req = request(vec![MergeGroup { keeper: k, others: vec![c] }], false, false);
    let plan = build_plan(&Library::open_read_only(&f.vol).unwrap(), &req).unwrap();
    let out = merge_run(&f.vol, &plan, &req, &mut |_| {}).unwrap();
    assert_eq!((out.merged, out.removed, out.files_deleted), (1, 0, 0));
    assert_eq!(f.rows().len(), 2);
    assert!(c_audio.is_file());

    // rows out, files kept: the file is left for the Device clean-up to list
    let req = request(vec![MergeGroup { keeper: k, others: vec![c] }], true, false);
    let plan = build_plan(&Library::open_read_only(&f.vol).unwrap(), &req).unwrap();
    assert!(plan.files.is_empty());
    let out = merge_run(&f.vol, &plan, &req, &mut |_| {}).unwrap();
    assert_eq!((out.removed, out.files_deleted), (1, 0));
    assert_eq!(f.rows().len(), 1);
    assert!(c_audio.is_file(), "no file was deleted because deleting was not asked for");
}

#[test]
fn a_plan_that_went_stale_changes_nothing() {
    let f = Fixture::new("stale");
    let k = f.add("k.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 320);
    let c = f.add("c.mp3", &[2; 3100], "Sample Artist", "Sample Title", 200, 128);
    let req = request(vec![MergeGroup { keeper: k, others: vec![c] }], true, true);
    let plan = build_plan(&Library::open_read_only(&f.vol).unwrap(), &req).unwrap();
    f.lib().set_track_rating(c, 5).unwrap(); // after the preview
    let out = merge_run(&f.vol, &plan, &req, &mut |_| {}).unwrap();
    assert_eq!((out.merged, out.failed.len(), out.removed, out.files_deleted), (0, 1, 0, 0));
    assert!(out.failed[0].1.contains("changed after the preview"), "{:?}", out.failed);
    assert_eq!(f.rows().len(), 2);
    assert!(f.audio_of(c).is_file());
    assert!(out.summary().contains("1 group not merged"));
    // the digest the start command compares is different for the new library state
    let fresh = build_plan(&Library::open_read_only(&f.vol).unwrap(), &req).unwrap();
    assert_ne!(fresh.digest, plan.digest);
}

#[test]
fn cases_holding_names_cases_places_and_the_smart_cases_that_match() {
    let f = Fixture::new("holding");
    let a = f.add("a.mp3", &[1; 3000], "Sample Artist", "Sample Title", 200, 128);
    let b = f.add("b.mp3", &[2; 3100], "Other Artist", "Other Title", 100, 128);
    let (plain, copy) = {
        let mut lib = f.lib();
        let plain = lib.create_case("Sample Case").unwrap();
        let copy = lib.create_case("★ Rated").unwrap();
        lib.set_case_tracks(plain, &[b, a]).unwrap();
        lib.set_case_tracks(copy, &[a]).unwrap();
        lib.set_track_rating(a, 5).unwrap();
        (plain, copy)
    };
    let dir = f.cache.join("config");
    let root = f.vol.to_string_lossy().to_string();
    let query: super::super::smart::Query = serde_json::from_str(r#"{"rules":[{"kind":"rating_min","stars":4}]}"#).unwrap();
    let mut rated = super::super::smart::save(&dir, &root, super::super::smart::SmartCase { id: 0, name: "Rated".into(), query: query.clone(), on_device: false, device_case_id: None }).unwrap();
    // before it is put on the Pacemaker the smart case matches the track but is not a case
    let h = holding_in(&root, &dir, &[a, b, 9999], 0).unwrap();
    assert_eq!(h.iter().map(|t| t.track_id).collect::<Vec<_>>(), vec![a, b]);
    assert_eq!(h[0].cases.iter().map(|c| (c.hold.case_id, c.hold.position, c.hold.of, c.smart_copy_of)).collect::<Vec<_>>(), vec![(plain, 2, 2, None), (copy, 1, 1, None)]);
    assert_eq!(h[0].smart.iter().map(|s| (s.name.as_str(), s.on_device)).collect::<Vec<_>>(), vec![("Rated", false)]);
    assert!(h[1].smart.is_empty() && h[1].cases.len() == 1);

    // once its copy exists, the copy says which smart case it belongs to
    rated.on_device = true;
    rated.device_case_id = Some(copy);
    super::super::smart::update(&dir, &root, &rated).unwrap();
    let h = holding_in(&root, &dir, &[a], 0).unwrap();
    assert_eq!(h[0].cases.iter().find(|c| c.hold.case_id == copy).unwrap().smart_copy_of, Some(rated.id));
    assert_eq!(h[0].smart[0].device_case_id, Some(copy));
    // no smart cases at all is fine
    assert!(holding_in(&root, &f.cache.join("none"), &[a], 0).unwrap()[0].smart.is_empty());
}
