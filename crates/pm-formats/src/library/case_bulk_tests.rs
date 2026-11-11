//! Tests for the bulk case operations in case_bulk.rs. Synthetic tracks only.

use super::case_ops_tests::{cleanup, library_with_tracks};
use super::*;

fn ids(lib: &Library, case: i64) -> Vec<i64> {
    lib.case_track_ids(case).unwrap()
}

#[test]
fn merge_order_is_pure_and_counts_every_skip() {
    // target holds 9; sources: [1,2,9] then [2,3,1]; 9 is already there, 2 and 1 repeat
    let plan = plan_merge(&[vec![1, 2, 9], vec![2, 3, 1]], &[9]);
    assert_eq!(plan.added, vec![1, 2, 3]);
    assert_eq!((plan.already_there, plan.repeated, plan.stale), (1, 2, 0));
    // a new case starts empty
    let plan = plan_merge(&[vec![5, 6], vec![6, 7]], &[]);
    assert_eq!(plan.added, vec![5, 6, 7]);
    assert_eq!((plan.already_there, plan.repeated), (0, 1));
    assert!(plan_merge(&[], &[1]).added.is_empty());
}

#[test]
fn transfer_plan_is_pure() {
    let known = |id: i64| id != 99;
    // select 1,2,3,2(repeat),99(not a track),7(not in the source); dest holds 2
    let p = plan_transfer(&[1, 2, 3, 2, 99, 7], Some(&[1, 2, 3, 4]), &[2], true, known);
    assert_eq!(p.to_add, vec![1, 3]);
    assert_eq!(p.already_there, vec![2]);
    assert_eq!(p.to_remove, vec![1, 2, 3], "a move removes it from the source even when the destination had it");
    assert_eq!(p.not_in_source, vec![7]);
    assert_eq!(p.missing, vec![99]);
    // a copy removes nothing
    let p = plan_transfer(&[1, 2], Some(&[1, 2]), &[], false, known);
    assert_eq!((p.to_add, p.to_remove), (vec![1, 2], vec![]));
    // from the library (no source): nothing is "not in source"
    let p = plan_transfer(&[7], None, &[], false, known);
    assert_eq!(p.to_add, vec![7]);
}

#[test]
fn duplicate_copies_tracks_in_order_and_metadata_but_not_play_stats() {
    let (mut lib, dirs, t) = library_with_tracks("dup", &[100, 250, 40]);
    let c = lib.create_case("Original").unwrap();
    lib.set_case_tracks(c, &[t[2], t[0], t[1]]).unwrap();
    lib.set_case_meta(
        c,
        &CaseMeta { name: "Original".into(), genre: "House".into(), year: 2020, rating: 4, producer: "P".into(), remixer: "R".into(), comments: "note".into() },
    )
    .unwrap();

    let d = lib.duplicate_case(c, "Original copy").unwrap();
    assert_ne!(d, c);
    assert_eq!(ids(&lib, d), vec![t[2], t[0], t[1]]);
    let (a, b) = (lib.case_info(c).unwrap().unwrap(), lib.case_info(d).unwrap().unwrap());
    assert_eq!(b.name, "Original copy");
    assert_eq!((b.genre.as_str(), b.year, b.rating, b.producer.as_str(), b.remixer.as_str(), b.comments.as_str()), ("House", 2020, 4, "P", "R", "note"));
    assert_eq!((b.track_count, b.play_time_secs, b.times_played, b.last_played), (3, 390, 0, 0));
    assert_eq!(a.track_count, 3, "the original is untouched");

    // bad input writes nothing
    assert!(lib.duplicate_case(c, "  ").is_err());
    assert!(lib.duplicate_case(9999, "x").is_err());
    assert_eq!(lib.case_infos().unwrap().len(), 2);
    cleanup(lib, dirs);
}

#[test]
fn duplicate_skips_rows_for_deleted_tracks() {
    let (mut lib, dirs, t) = library_with_tracks("dupstale", &[10, 20]);
    let c = lib.create_case("A").unwrap();
    lib.set_case_tracks(c, &[t[0], t[1]]).unwrap();
    // a membership row whose track is gone (the table has no foreign key)
    lib.con.execute("INSERT INTO casetracks (case_id, track_id) VALUES (?1, 424242)", [c]).unwrap();
    let d = lib.duplicate_case(c, "B").unwrap();
    assert_eq!(ids(&lib, d), vec![t[0], t[1]]);
    assert_eq!(lib.merge_plan(&[c], None).unwrap().stale, 1);
    cleanup(lib, dirs);
}

#[test]
fn merge_into_new_and_existing_without_repeats() {
    let (mut lib, dirs, t) = library_with_tracks("merge", &[10, 20, 30, 40]);
    let a = lib.create_case("A").unwrap();
    let b = lib.create_case("B").unwrap();
    let dst = lib.create_case("Dest").unwrap();
    lib.set_case_tracks(a, &[t[0], t[1]]).unwrap();
    lib.set_case_tracks(b, &[t[1], t[2]]).unwrap();
    lib.set_case_tracks(dst, &[t[3], t[0]]).unwrap();

    // the preview is the plan the write follows
    let plan = lib.merge_plan(&[a, b], Some(dst)).unwrap();
    assert_eq!(plan.added, vec![t[1], t[2]]);
    assert_eq!((plan.already_there, plan.repeated), (1, 1));

    let done = lib.merge_cases(&[a, b], &MergeDest::Existing(dst), false).unwrap();
    assert_eq!((done.case_id, done.added, done.deleted_sources), (dst, 2, 0));
    assert_eq!(ids(&lib, dst), vec![t[3], t[0], t[1], t[2]], "existing order kept, new ones appended");
    assert_eq!(lib.case_info(dst).unwrap().unwrap().play_time_secs, 100);
    assert_eq!(ids(&lib, a), vec![t[0], t[1]], "sources stay by default");

    // into a new case, deleting the sources
    let done = lib.merge_cases(&[b, a], &MergeDest::New("Both".into()), true).unwrap();
    assert_eq!((done.added, done.deleted_sources), (3, 2));
    assert_eq!(ids(&lib, done.case_id), vec![t[1], t[2], t[0]], "first source's order, then the next source's new tracks");
    assert!(lib.case_info(a).unwrap().is_none() && lib.case_info(b).unwrap().is_none());
    assert_eq!(lib.case_info(done.case_id).unwrap().unwrap().play_time_secs, 60);
    assert_eq!(lib.tracks().unwrap().len(), 4, "tracks are never deleted");
    cleanup(lib, dirs);
}

#[test]
fn merge_refuses_bad_input_and_writes_nothing() {
    let (mut lib, dirs, t) = library_with_tracks("mergebad", &[10, 20]);
    let a = lib.create_case("A").unwrap();
    lib.set_case_tracks(a, &[t[0]]).unwrap();
    let before = lib.case_infos().unwrap();
    assert!(lib.merge_cases(&[a], &MergeDest::Existing(a), false).is_err(), "into itself");
    assert!(lib.merge_cases(&[], &MergeDest::New("x".into()), false).is_err());
    assert!(lib.merge_cases(&[a, 9999], &MergeDest::New("x".into()), true).is_err(), "unknown source: nothing created, nothing deleted");
    assert!(lib.merge_cases(&[a], &MergeDest::New("  ".into()), false).is_err());
    assert!(lib.merge_cases(&[a], &MergeDest::Existing(9999), false).is_err());
    assert_eq!(lib.case_infos().unwrap(), before);
    assert_eq!(ids(&lib, a), vec![t[0]]);
    cleanup(lib, dirs);
}

#[test]
fn copy_and_move_tracks_between_cases() {
    let (mut lib, dirs, t) = library_with_tracks("xfer", &[10, 20, 30, 40]);
    let src = lib.create_case("Src").unwrap();
    let dst = lib.create_case("Dst").unwrap();
    lib.set_case_tracks(src, &[t[0], t[1], t[2]]).unwrap();
    lib.set_case_tracks(dst, &[t[1]]).unwrap();

    let sel = [t[1], t[2], t[3]]; // t[3] is not in the source
    let plan = lib.transfer_plan(Some(src), dst, &sel, true).unwrap();
    assert_eq!((plan.to_add.clone(), plan.already_there.clone()), (vec![t[2]], vec![t[1]]));
    assert_eq!(plan.to_remove, vec![t[1], t[2]]);
    assert_eq!(plan.not_in_source, vec![t[3]]);

    // copy first: nothing leaves the source
    let done = lib.transfer_tracks(Some(src), dst, &sel, false).unwrap();
    assert_eq!((done.added, done.removed), (1, 0));
    assert_eq!(ids(&lib, src), vec![t[0], t[1], t[2]]);
    assert_eq!(ids(&lib, dst), vec![t[1], t[2]]);

    // move: leaves the source, and a track the destination already had still leaves
    let (mut lib2, dirs2, u) = library_with_tracks("xfer2", &[10, 20, 30, 40]);
    let s2 = lib2.create_case("Src").unwrap();
    let d2 = lib2.create_case("Dst").unwrap();
    lib2.set_case_tracks(s2, &[u[0], u[1], u[2]]).unwrap();
    lib2.set_case_tracks(d2, &[u[1]]).unwrap();
    let done = lib2.transfer_tracks(Some(s2), d2, &[u[1], u[2]], true).unwrap();
    assert_eq!((done.added, done.removed), (1, 2));
    assert_eq!(ids(&lib2, s2), vec![u[0]]);
    assert_eq!(ids(&lib2, d2), vec![u[1], u[2]]);
    assert_eq!(lib2.case_info(s2).unwrap().unwrap().play_time_secs, 10);
    assert_eq!(lib2.case_info(d2).unwrap().unwrap().play_time_secs, 50);

    // copy from the library (no source case)
    let done = lib2.transfer_tracks(None, d2, &[u[3], u[3], 424242], false).unwrap();
    assert_eq!((done.added, done.removed), (1, 0));
    assert_eq!(ids(&lib2, d2), vec![u[1], u[2], u[3]]);

    // refused combinations write nothing
    assert!(lib2.transfer_tracks(Some(s2), s2, &[u[0]], true).is_err());
    assert!(lib2.transfer_tracks(None, d2, &[u[0]], true).is_err(), "a move needs a source");
    assert!(lib2.transfer_tracks(Some(s2), 9999, &[u[0]], false).is_err());
    assert_eq!(ids(&lib2, s2), vec![u[0]]);
    cleanup(lib2, dirs2);
    cleanup(lib, dirs);
}

#[test]
fn delete_several_cases_in_one_go() {
    let (mut lib, dirs, t) = library_with_tracks("delmany", &[10, 20]);
    let a = lib.create_case("A").unwrap();
    let b = lib.create_case("B").unwrap();
    let c = lib.create_case("C").unwrap();
    lib.set_case_tracks(a, &[t[0]]).unwrap();
    lib.set_case_tracks(b, &[t[0], t[1]]).unwrap();
    assert_eq!(lib.delete_cases(&[a, b, b, 9999]).unwrap(), 2);
    let left = lib.case_infos().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, c);
    let rows: i64 = lib.con.query_row("SELECT COUNT(*) FROM casetracks", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0, "membership rows go with the case");
    assert_eq!(lib.tracks().unwrap().len(), 2);
    assert_eq!(lib.delete_cases(&[]).unwrap(), 0);
    cleanup(lib, dirs);
}
