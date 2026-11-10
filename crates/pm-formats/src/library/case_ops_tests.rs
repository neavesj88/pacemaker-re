//! Tests for the case operations in library.rs (membership edits, metadata, favourites).

use super::*;

/// A library in a temp dir with one synthetic track per entry of `secs`; returns it, the
/// dirs to clean up and the track ids.
fn library_with_tracks(tag: &str, secs: &[i64]) -> (Library, Vec<PathBuf>, Vec<i64>) {
    let base = std::env::temp_dir();
    let vol = base.join(format!("pm-caseops-{tag}-vol-{}", std::process::id()));
    let src_dir = base.join(format!("pm-caseops-{tag}-src-{}", std::process::id()));
    for d in [&vol, &src_dir] {
        let _ = std::fs::remove_dir_all(d);
        std::fs::create_dir_all(d).unwrap();
    }
    let analysis = strfile::Analysis {
        normalize_factor: 1.0,
        beat_start_offset: 0,
        beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
        first_downbeat: Some(0),
    };
    let mut lib = Library::create(&vol).unwrap();
    let mut ids = Vec::new();
    for (i, s) in secs.iter().enumerate() {
        let src = src_dir.join(format!("synthetic {i}.mp3"));
        std::fs::write(&src, vec![0u8; 2000 + i]).unwrap();
        let meta = TrackMeta { title: format!("t{i}"), format: "MP3".into(), play_time_secs: *s, ..Default::default() };
        ids.push(lib.add_track(&src, &meta, &analysis).unwrap());
    }
    (lib, vec![vol, src_dir], ids)
}

fn cleanup(lib: Library, dirs: Vec<PathBuf>) {
    drop(lib); // Windows won't delete a database file that is still open
    for d in dirs {
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn add_and_remove_tracks_keep_order_and_derived_fields() {
    let (mut lib, dirs, ids) = library_with_tracks("addrem", &[100, 250, 40, 7]);
    let c = lib.create_case("Synthetic").unwrap();
    let other = lib.create_case("Other").unwrap();
    lib.set_case_tracks(other, &[ids[3]]).unwrap();
    let other_before = lib.case_info(other).unwrap().unwrap();

    // appended in order, repeats and already-present ids skipped
    assert_eq!(lib.add_case_tracks(c, &[ids[2], ids[0], ids[2]]).unwrap(), 2);
    assert_eq!(lib.add_case_tracks(c, &[ids[0], ids[1]]).unwrap(), 1);
    assert_eq!(lib.case_track_ids(c).unwrap(), vec![ids[2], ids[0], ids[1]]);
    let info = lib.case_info(c).unwrap().unwrap();
    assert_eq!((info.track_count, info.play_time_secs), (3, 390));
    assert!(info.date_modified >= info.date_created);

    // nothing to add: a no-op that leaves date_modified alone
    let stamp = info.date_modified;
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert_eq!(lib.add_case_tracks(c, &[ids[1]]).unwrap(), 0);
    assert_eq!(lib.case_info(c).unwrap().unwrap().date_modified, stamp);

    // removal keeps the survivors' order and refreshes the sum; absent ids are ignored
    assert_eq!(lib.remove_case_tracks(c, &[ids[0], ids[3]]).unwrap(), 1);
    assert_eq!(lib.case_track_ids(c).unwrap(), vec![ids[2], ids[1]]);
    assert_eq!(lib.case_info(c).unwrap().unwrap().play_time_secs, 290);
    assert_eq!(lib.remove_case_tracks(c, &[ids[2], ids[1]]).unwrap(), 2);
    assert_eq!(lib.case_info(c).unwrap().unwrap().play_time_secs, 0, "empty case sums to 0, not NULL");

    // a bad id aborts the whole add; the other case was never touched
    lib.add_case_tracks(c, &[ids[0]]).unwrap();
    assert!(matches!(lib.add_case_tracks(c, &[ids[1], 999_999]), Err(Error::Invalid(_))));
    assert_eq!(lib.case_track_ids(c).unwrap(), vec![ids[0]], "failed add rolled back");
    assert!(matches!(lib.add_case_tracks(999_999, &[ids[0]]), Err(Error::Invalid(_))));
    assert!(matches!(lib.remove_case_tracks(999_999, &[ids[0]]), Err(Error::Invalid(_))));
    assert_eq!(lib.case_info(other).unwrap().unwrap(), other_before);
    assert!(lib.compatibility_problems().unwrap().is_empty());
    cleanup(lib, dirs);
}

#[test]
fn reorder_with_set_case_tracks_rewrites_rowid_order() {
    let (mut lib, dirs, ids) = library_with_tracks("reorder", &[1, 2, 3]);
    let c = lib.create_case("Synthetic").unwrap();
    lib.add_case_tracks(c, &ids).unwrap();
    let reversed: Vec<i64> = ids.iter().rev().copied().collect();
    lib.set_case_tracks(c, &reversed).unwrap();
    assert_eq!(lib.case_track_ids(c).unwrap(), reversed);
    // a track appended after a reorder lands at the end
    lib.remove_case_tracks(c, &[ids[1]]).unwrap();
    lib.add_case_tracks(c, &[ids[1]]).unwrap();
    assert_eq!(lib.case_track_ids(c).unwrap(), vec![ids[2], ids[0], ids[1]]);
    cleanup(lib, dirs);
}

#[test]
fn toggle_and_favourites() {
    let (mut lib, dirs, ids) = library_with_tracks("fav", &[60, 90]);
    assert!(lib.favourite_track_ids().unwrap().is_empty());
    assert!(lib.cases().unwrap().is_empty(), "reading favourites does not create the case");

    assert!(lib.toggle_favourite(ids[1]).unwrap());
    assert!(lib.toggle_favourite(ids[0]).unwrap());
    assert_eq!(lib.favourite_track_ids().unwrap(), vec![ids[1], ids[0]]);
    let cases = lib.cases().unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!((cases[0].name.as_str(), cases[0].track_count, cases[0].play_time_secs), (FAVOURITES_CASE_NAME, 2, 150));

    assert!(!lib.toggle_favourite(ids[1]).unwrap());
    assert_eq!(lib.favourite_track_ids().unwrap(), vec![ids[0]]);
    assert_eq!(lib.cases().unwrap().len(), 1, "the case is reused, not recreated");

    // an unknown track errors before creating the case
    let (mut empty, edirs, _) = library_with_tracks("fav-empty", &[]);
    assert!(matches!(empty.toggle_favourite(1), Err(Error::Invalid(_))));
    assert!(empty.cases().unwrap().is_empty());
    cleanup(empty, edirs);

    // an existing device case of that name is used (lowest id when duplicated)
    let (mut lib2, dirs2, ids2) = library_with_tracks("fav-existing", &[10]);
    let first = lib2.create_case(FAVOURITES_CASE_NAME).unwrap();
    lib2.create_case(FAVOURITES_CASE_NAME).unwrap();
    lib2.toggle_favourite(ids2[0]).unwrap();
    assert_eq!(lib2.case_track_ids(first).unwrap(), vec![ids2[0]]);
    assert_eq!(lib2.case_id_by_name("Nope").unwrap(), None);
    cleanup(lib2, dirs2);
    cleanup(lib, dirs);
}

#[test]
fn metadata_and_rename() {
    let (mut lib, dirs, ids) = library_with_tracks("meta", &[30]);
    let c = lib.create_case("Synthetic").unwrap();
    lib.add_case_tracks(c, &ids).unwrap();
    let before = lib.case_info(c).unwrap().unwrap();
    assert_eq!((before.genre.as_str(), before.rating, before.track_count), ("", 0, 1));
    assert_eq!(before.year, year_of_ms(before.date_created));

    let meta = CaseMeta {
        name: "Synthetic renamed".into(),
        genre: "House".into(),
        year: 2021,
        rating: 4,
        producer: "P".into(),
        remixer: "R".into(),
        comments: "line one\nline two".into(),
    };
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(lib.set_case_meta(c, &meta).unwrap());
    let after = lib.case_info(c).unwrap().unwrap();
    assert_eq!(after.meta(), meta);
    assert!(after.date_modified > before.date_modified);
    assert_eq!((after.date_created, after.track_count, after.play_time_secs), (before.date_created, 1, 30));
    assert_eq!(after.id, c);

    // identical values: no write, date_modified unchanged
    assert!(!lib.set_case_meta(c, &meta).unwrap());
    assert_eq!(lib.case_info(c).unwrap().unwrap().date_modified, after.date_modified);

    // rename keeps the rest
    assert!(lib.rename_case(c, "Again").unwrap());
    assert!(!lib.rename_case(c, "Again").unwrap());
    let r = lib.case_info(c).unwrap().unwrap();
    assert_eq!((r.name.as_str(), r.genre.as_str(), r.rating, r.year), ("Again", "House", 4, 2021));

    // validation: nothing written on failure
    for bad in [
        CaseMeta { name: "  ".into(), ..meta.clone() },
        CaseMeta { name: "x".repeat(MAX_CASE_NAME_CHARS + 1), ..meta.clone() },
        CaseMeta { rating: 6, ..meta.clone() },
        CaseMeta { year: -1, ..meta.clone() },
        CaseMeta { year: 10_000, ..meta.clone() },
    ] {
        assert!(matches!(lib.set_case_meta(c, &bad), Err(Error::Invalid(_))));
    }
    assert!(matches!(lib.set_case_meta(999_999, &meta), Err(Error::Invalid(_))));
    assert!(matches!(lib.rename_case(999_999, "x"), Err(Error::Invalid(_))));
    assert_eq!(lib.case_info(c).unwrap().unwrap().name, "Again");
    assert!(lib.case_info(999_999).unwrap().is_none());
    assert!(lib.compatibility_problems().unwrap().is_empty());
    cleanup(lib, dirs);
}

#[test]
fn create_with_tracks_and_infos() {
    let (mut lib, dirs, ids) = library_with_tracks("withtracks", &[11, 22, 33]);
    let a = lib.create_case_with_tracks("Beta", &[ids[2], ids[0], ids[2]]).unwrap();
    assert_eq!(lib.case_track_ids(a).unwrap(), vec![ids[2], ids[0]]);
    assert_eq!(lib.case_info(a).unwrap().unwrap().play_time_secs, 44);

    // a bad id leaves no stray empty case behind
    let n = lib.cases().unwrap().len();
    assert!(matches!(lib.create_case_with_tracks("Gamma", &[ids[0], 999_999]), Err(Error::Invalid(_))));
    assert!(matches!(lib.create_case_with_tracks("", &[ids[0]]), Err(Error::Invalid(_))));
    assert_eq!(lib.cases().unwrap().len(), n);

    lib.create_case("Alpha").unwrap();
    let infos = lib.case_infos().unwrap();
    assert_eq!(infos.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["Alpha", "Beta"], "ordered by name");
    let listed = lib.cases().unwrap();
    for (i, c) in infos.iter().zip(&listed) {
        assert_eq!((i.id, i.track_count, i.play_time_secs), (c.id, c.track_count, c.play_time_secs));
    }
    assert!(lib.compatibility_problems().unwrap().is_empty());
    cleanup(lib, dirs);
}
