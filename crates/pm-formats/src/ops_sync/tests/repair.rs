//! Tests of the repair action: a track whose row is the same on both sides but whose audio or
//! `.str` file is missing on the target gets only the missing file(s) copied.

use super::selection::sel;
use super::*;

/// One track synced to the target (so the rows are the same); returns its location and its
/// `(audio, .str)` paths on each side.
fn both_sides(fx: &mut Fx) -> (String, (PathBuf, PathBuf), (PathBuf, PathBuf)) {
    let mut s = Library::open(fx.src.root()).unwrap();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    fx.run(&p, &[]).unwrap();
    fx.dst = Library::open(fx.dst.root()).unwrap();
    let loc = loc_of(&fx.src, a);
    let sp = paths_of(&fx.src, &loc).unwrap();
    let dp = paths_of(&fx.dst, &loc).unwrap();
    all_same(&fx.plan(&SyncSettings::default()));
    (loc, sp, dp)
}

fn row_dump(lib: &Library) -> Vec<String> {
    let mut st = lib.con.prepare("SELECT * FROM tracks ORDER BY track_id").unwrap();
    let n = st.column_count();
    st.query_map([], |r| Ok((0..n).map(|i| format!("{:?}", r.get::<_, Value>(i).unwrap())).collect::<Vec<_>>().join("|"))).unwrap().map(|r| r.unwrap()).collect()
}

#[test]
fn a_missing_audio_file_is_repaired_and_the_row_is_left_alone() {
    let mut fx = Fx::new();
    let (loc, (s_audio, _), (t_audio, t_side)) = both_sides(&mut fx);
    let t_side_before = std::fs::read(&t_side).unwrap();
    std::fs::remove_file(&t_audio).unwrap();
    let rows_before = row_dump(&fx.dst);
    let p = fx.plan(&SyncSettings::default());
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Repair);
    assert_eq!(it.detail, "the audio file is missing in the receiving library");
    assert_eq!(it.bytes, std::fs::metadata(&s_audio).unwrap().len());
    assert_eq!(p.bytes_to_copy, it.bytes, "the needed bytes count toward the space line");
    let r = fx.run(&p, &[]).unwrap();
    assert_eq!((r.done, r.failed), (1, 0));
    assert_eq!(std::fs::read(&t_audio).unwrap(), std::fs::read(&s_audio).unwrap());
    assert_eq!(std::fs::read(&t_side).unwrap(), t_side_before, "the grid file that was there is not touched");
    assert_eq!(row_dump(&fx.dst), rows_before, "the row is not touched");
    assert_eq!(stray_temps(fx.dst.root()), 0);
    all_same(&fx.plan(&SyncSettings::default()));
}

#[test]
fn a_missing_str_file_is_repaired() {
    let mut fx = Fx::new();
    let (loc, (_, s_side), (t_audio, t_side)) = both_sides(&mut fx);
    let audio_before = std::fs::read(&t_audio).unwrap();
    std::fs::remove_file(&t_side).unwrap();
    let p = fx.plan(&SyncSettings::default());
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Repair);
    assert_eq!(it.detail, "the beat grid file (.str) is missing in the receiving library");
    assert_eq!(it.bytes, std::fs::metadata(&s_side).unwrap().len());
    fx.run(&p, &[]).unwrap();
    assert_eq!(std::fs::read(&t_side).unwrap(), std::fs::read(&s_side).unwrap());
    assert_eq!(std::fs::read(&t_audio).unwrap(), audio_before);
    all_same(&fx.plan(&SyncSettings::default()));
}

#[test]
fn both_files_missing_are_both_repaired() {
    let mut fx = Fx::new();
    let (loc, (s_audio, s_side), (t_audio, t_side)) = both_sides(&mut fx);
    std::fs::remove_file(&t_audio).unwrap();
    std::fs::remove_file(&t_side).unwrap();
    let p = fx.plan(&SyncSettings::default());
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Repair);
    assert!(it.detail.contains("audio file is missing") && it.detail.contains("(.str) is missing"), "{}", it.detail);
    assert_eq!(it.bytes, std::fs::metadata(&s_audio).unwrap().len() + std::fs::metadata(&s_side).unwrap().len());
    fx.run(&p, &[]).unwrap();
    assert!(t_audio.exists() && t_side.exists());
    all_same(&fx.plan(&SyncSettings::default()));
}

#[test]
fn a_file_missing_on_both_sides_stays_a_skip_that_says_so() {
    let mut fx = Fx::new();
    let (loc, (s_audio, s_side), (t_audio, t_side)) = both_sides(&mut fx);
    std::fs::remove_file(&t_audio).unwrap();
    std::fs::remove_file(&s_audio).unwrap();
    let p = fx.plan(&SyncSettings::default());
    assert_eq!(reason(item(&p, &format!("track:{loc}"))), "the audio file is missing in both libraries");
    assert_eq!(p.bytes_to_copy, 0);
    // the audio is lacking everywhere but the target's grid is gone and the source's is there: that part is repaired
    std::fs::remove_file(&t_side).unwrap();
    let p = fx.plan(&SyncSettings::default());
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Repair);
    assert_eq!(it.bytes, std::fs::metadata(&s_side).unwrap().len());
    assert!(it.detail.contains("(.str) is missing in the receiving library") && it.detail.contains("audio file is missing in both libraries"), "{}", it.detail);
    fx.run(&p, &[]).unwrap();
    assert!(t_side.exists() && !t_audio.exists());
}

#[test]
fn a_file_that_appears_after_the_plan_makes_it_stale() {
    let mut fx = Fx::new();
    let (_, (s_audio, _), (t_audio, _)) = both_sides(&mut fx);
    std::fs::remove_file(&t_audio).unwrap();
    let p = fx.plan(&SyncSettings::default());
    std::fs::copy(&s_audio, &t_audio).unwrap();
    let err = fx.run(&p, &[]).unwrap_err().to_string();
    assert!(err.contains(STALE), "{err}");
}

#[test]
fn the_drag_plan_repairs_too_and_only_adds_files() {
    let mut fx = Fx::new();
    let (loc, (s_audio, _), (t_audio, _)) = both_sides(&mut fx);
    std::fs::remove_file(&t_audio).unwrap();
    let rows_before = row_dump(&fx.dst);
    let pick = sel(&[loc.clone()], &[], &[]);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Repair);
    assert_eq!(it.detail, "the audio file is missing in the receiving library");
    assert_eq!(p.bytes_to_copy, std::fs::metadata(&s_audio).unwrap().len());
    let r = fx.run_sel(&p, &pick).unwrap();
    assert_eq!((r.done, r.failed), (1, 0));
    assert_eq!(std::fs::read(&t_audio).unwrap(), std::fs::read(&s_audio).unwrap());
    assert_eq!(row_dump(&fx.dst), rows_before);
    assert!(fx.plan_sel(&pick, &SyncSettings::default()).items.iter().all(|i| reason(i) == SAME));
    // a changed picked file after planning is stale
    std::fs::remove_file(&t_audio).unwrap();
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    std::fs::copy(&s_audio, &t_audio).unwrap();
    assert!(fx.run_sel(&p, &pick).unwrap_err().to_string().contains(STALE));
}

#[test]
fn a_repair_says_the_play_counts_come_on_the_next_sync() {
    let mut fx = Fx::new();
    let (loc, _, (t_audio, _)) = both_sides(&mut fx);
    std::fs::remove_file(&t_audio).unwrap();
    fx.src.con.execute("UPDATE tracks SET times_played = 50, last_played = 9999999999999", []).unwrap();
    let p = fx.plan(&SyncSettings::default());
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Repair, "one action per track: the files first");
    assert_eq!(it.detail, "the audio file is missing in the receiving library; the play counts follow on the next sync");
    fx.run(&p, &[]).unwrap();
    fx.dst = Library::open(fx.dst.root()).unwrap();
    assert_eq!(item(&fx.plan(&SyncSettings::default()), &format!("track:{loc}")).action, SyncAction::UpdatePlays, "the next sync brings them");
}

/// The source's row is edited (a rating, a newer date) and its grid changes, so the rows differ.
fn edit_source(fx: &mut Fx, loc: &str, date_modified: i64) {
    let mut s = Library::open(fx.src.root()).unwrap();
    s.con.execute("UPDATE tracks SET rating = 5, date_modified = ?1 WHERE location = ?2", params![date_modified, loc]).unwrap();
    let (_, side) = s.track_paths(loc).unwrap();
    std::fs::write(&side, b"changed sidecar").unwrap();
    fx.src = s;
}

#[test]
fn a_row_that_differs_does_not_leave_the_missing_audio_missing() {
    let mut fx = Fx::new();
    let (loc, (s_audio, _), (t_audio, t_side)) = both_sides(&mut fx);
    std::fs::remove_file(&t_audio).unwrap();
    edit_source(&mut fx, &loc, 4_000_000_000_000);
    let settings = SyncSettings { tracks_on_both: TracksOnBoth::Source, ..SyncSettings::default() };
    let p = fx.plan(&settings);
    let it = item(&p, &format!("track:{loc}"));
    assert_eq!(it.action, SyncAction::Update);
    assert_eq!(it.bytes, std::fs::metadata(&s_audio).unwrap().len() + b"changed sidecar".len() as u64, "the audio counts toward the space line");
    assert!(it.detail.contains("audio file is missing"), "{}", it.detail);
    let r = fx.run(&p, &[]).unwrap();
    assert_eq!((r.done, r.failed), (1, 0));
    assert_eq!(std::fs::read(&t_audio).unwrap(), std::fs::read(&s_audio).unwrap(), "the audio came with the update");
    assert_eq!(std::fs::read(&t_side).unwrap(), b"changed sidecar");
    assert_eq!(stray_temps(fx.dst.root()), 0);
    all_same(&fx.plan(&settings));
}

#[test]
fn a_row_left_alone_still_gets_its_missing_files() {
    for (mode, source_date) in [(TracksOnBoth::Leave, 4_000_000_000_000_i64), (TracksOnBoth::Newer, 0)] {
        let mut fx = Fx::new();
        let (loc, (s_audio, _), (t_audio, t_side)) = both_sides(&mut fx);
        let t_side_before = std::fs::read(&t_side).unwrap();
        std::fs::remove_file(&t_audio).unwrap();
        edit_source(&mut fx, &loc, source_date);
        let rows_before = row_dump(&fx.dst);
        let settings = SyncSettings { tracks_on_both: mode, ..SyncSettings::default() };
        let p = fx.plan(&settings);
        let it = item(&p, &format!("track:{loc}"));
        assert_eq!(it.action, SyncAction::Repair, "{mode:?}");
        assert_eq!(it.detail, "the audio file is missing in the receiving library");
        fx.run(&p, &[]).unwrap();
        assert_eq!(std::fs::read(&t_audio).unwrap(), std::fs::read(&s_audio).unwrap(), "{mode:?}");
        assert_eq!(row_dump(&fx.dst), rows_before, "{mode:?}: the row stays as the setting says");
        assert_eq!(std::fs::read(&t_side).unwrap(), t_side_before, "{mode:?}: and so does the grid");
    }
}

#[test]
fn a_row_left_alone_with_nothing_missing_keeps_its_reason() {
    let mut fx = Fx::new();
    let (loc, _, _) = both_sides(&mut fx);
    edit_source(&mut fx, &loc, 4_000_000_000_000);
    let p = fx.plan(&SyncSettings { tracks_on_both: TracksOnBoth::Leave, ..SyncSettings::default() });
    assert_eq!(reason(item(&p, &format!("track:{loc}"))), LEAVE_TRACK);
}
