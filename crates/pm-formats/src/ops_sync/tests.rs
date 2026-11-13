//! Tests for the sync engine: two temp libraries built through `Library::create` + `add_track`.

use super::*;
use crate::mix::{Mix, Position, Track as MixTrack};
use crate::strfile;
use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Fx {
    src: Library,
    dst: Library,
    dirs: Vec<PathBuf>,
    files: PathBuf,
}

impl Fx {
    fn new() -> Fx {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!("pm-sync-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (s, d, f) = (base.join("source"), base.join("target"), base.join("files"));
        for p in [&s, &d, &f] {
            std::fs::create_dir_all(p).unwrap();
        }
        Fx { src: Library::create(&s).unwrap(), dst: Library::create(&d).unwrap(), dirs: vec![base], files: f }
    }

    /// Adds the same synthetic file (same name and size, so the same location) to `which`.
    fn add(&self, lib: &mut Library, name: &str, size: usize, title: &str) -> i64 {
        let path = self.files.join(format!("{name}.mp3"));
        std::fs::write(&path, vec![name.len() as u8; size]).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let meta = crate::library::TrackMeta {
            title: title.into(),
            artist: "Synthetic".into(),
            format: "MP3".into(),
            play_time_secs: 100,
            ..Default::default()
        };
        lib.add_track(&path, &meta, &analysis).unwrap()
    }

    fn plan(&self, settings: &SyncSettings) -> SyncPlan {
        plan(&self.src, &self.dst, settings, "computer").unwrap()
    }

    fn run(&mut self, p: &SyncPlan, excluded: &[String]) -> Result<SyncReport, Error> {
        apply(&self.src, &mut self.dst, p, excluded, &mut |_| {}, &|| false)
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        // the libraries hold their database files open; Windows will not delete those
        let _ = self.src.con.execute_batch("PRAGMA optimize");
        for d in &self.dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

fn mix_of(name: &str, ids: &[i64]) -> Mix {
    let mut m = Mix::new(name);
    let mut start = 0.0;
    for (i, id) in ids.iter().enumerate() {
        let pos = Position { start, length: 4_410_000.0, limit_size: true, session_event_id: format!("{:032x}", i + 1), ..Position::default() };
        let mut t = MixTrack::new(*id, &format!("synthetic{i}.mp3"), 128.0, 1.0, pos);
        t.channel = (i % 2) as u32;
        m.tracks.push(t);
        start += 3_000_000.0;
    }
    m
}

fn make_mix(lib: &mut Library, name: &str, ids: &[i64]) -> i64 {
    let id = lib.create_mix(name).unwrap();
    lib.save_mix(id, &mix_of(name, ids)).unwrap();
    id
}

fn loc_of(lib: &Library, id: i64) -> String {
    lib.con.query_row("SELECT location FROM tracks WHERE track_id = ?1", [id], |r| r.get(0)).unwrap()
}

fn locs(lib: &Library, ids: &[i64]) -> Vec<String> {
    ids.iter().map(|i| loc_of(lib, *i)).collect()
}

fn item<'a>(p: &'a SyncPlan, key: &str) -> &'a SyncItem {
    p.items.iter().find(|i| i.key == key).unwrap_or_else(|| panic!("no item {key}: {:?}", p.items.iter().map(|i| &i.key).collect::<Vec<_>>()))
}

fn reason(i: &SyncItem) -> &str {
    match &i.action {
        SyncAction::Skip { reason } => reason,
        other => panic!("{} is {other:?}, not a skip", i.key),
    }
}

fn all_same(p: &SyncPlan) {
    for i in &p.items {
        assert_eq!(reason(i), SAME, "{}", i.key);
    }
}

/// Like [`all_same`] for libraries where a track was added to both sides independently: those
/// rows differ in their add time, and the engine rightly keeps the target's newer one.
fn nothing_left_to_do(p: &SyncPlan) {
    for i in &p.items {
        let r = reason(i);
        assert!(r == SAME || r.contains("already in the plan") || (i.kind == ItemKind::Track && r.contains("same age or newer")), "{}: {r}", i.key);
    }
}

fn set_track(lib: &Library, id: i64, sql_set: &str) {
    lib.con.execute(&format!("UPDATE tracks SET {sql_set} WHERE track_id = {id}"), []).unwrap();
}

fn int(lib: &Library, id: i64, col: &str) -> i64 {
    lib.con.query_row(&format!("SELECT CAST({col} AS INTEGER) FROM tracks WHERE track_id = {id}"), [], |r| r.get(0)).unwrap()
}

fn stray_temps(root: &Path) -> usize {
    fn walk(p: &Path, n: &mut usize) {
        for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                walk(&e.path(), n);
            } else if e.file_name().to_string_lossy().ends_with(".tmp") {
                *n += 1;
            }
        }
    }
    let mut n = 0;
    walk(&root.join(".Pacemaker"), &mut n);
    n
}

#[test]
fn copies_tracks_mixes_and_cases_and_remaps_ids() {
    let mut fx = Fx::new();
    let mut src = Library::open(fx.src.root()).unwrap();
    let a = fx.add(&mut src, "alpha", 3000, "Alpha");
    let b = fx.add(&mut src, "bravo", 3100, "Bravo");
    let c = fx.add(&mut src, "charlie", 3200, "Charlie");
    // the target already holds a different track first, so its ids cannot match the source's
    let mut dst = Library::open(fx.dst.root()).unwrap();
    fx.add(&mut dst, "zulu", 2900, "Zulu");
    fx.add(&mut dst, "yankee", 2800, "Yankee");
    drop(dst);
    let mix = make_mix(&mut src, "Warm up", &[c, a]);
    src.con.execute("INSERT INTO miximages (data) VALUES (x'89504e47')", []).unwrap();
    let img = src.con.last_insert_rowid();
    src.con.execute("UPDATE mixes SET mix_image = ?1, genre = 'House', rating = 4, comments = 'synthetic' WHERE mix_id = ?2", params![img, mix]).unwrap();
    let case = src.create_case_with_tracks("Opening", &[c, a, b]).unwrap();
    src.set_case_meta(case, &CaseMeta { name: "Opening".into(), genre: "House".into(), year: 2001, rating: 3, producer: "p".into(), remixer: "r".into(), comments: "note".into() }).unwrap();
    fx.src = src;
    fx.dst = Library::open(fx.dst.root()).unwrap();

    let p = fx.plan(&SyncSettings::default());
    assert_eq!(p.items.iter().filter(|i| i.action == SyncAction::Copy).count(), 5, "3 tracks, 1 mix, 1 case");
    assert!(p.bytes_to_copy > 9000 && p.fits);
    assert!(p.notes.iter().any(|n| n.contains("Smart cases")));
    let mix_item = item(&p, "mix:warm up");
    assert_eq!(mix_item.needs.len(), 2);

    let r = fx.run(&p, &[]).unwrap();
    assert_eq!((r.done, r.failed, r.cancelled), (5, 0, false));
    assert_eq!(stray_temps(fx.dst.root()), 0);

    // tracks: rows, files and identity
    for (id, name) in [(a, "alpha"), (b, "bravo"), (c, "charlie")] {
        let loc = loc_of(&fx.src, id);
        let (audio, side) = fx.dst.track_paths(&loc).unwrap();
        assert_eq!(std::fs::read(&audio).unwrap(), std::fs::read(fx.src.track_paths(&loc).unwrap().0).unwrap(), "{name} audio");
        assert!(side.is_file());
        let g_src: String = fx.src.con.query_row("SELECT global_id FROM tracks WHERE track_id = ?1", [id], |r| r.get(0)).unwrap();
        let g_dst: String = fx.dst.con.query_row("SELECT global_id FROM tracks WHERE location = ?1", [&loc], |r| r.get(0)).unwrap();
        assert_eq!(g_src, g_dst, "global_id travels");
    }
    assert!(fx.dst.compatibility_problems().unwrap().is_empty());

    // the mix: every library_id points at the track with the same location
    let t_mix = fx.dst.mix_infos().unwrap().into_iter().find(|m| m.name == "Warm up").unwrap();
    let doc = fx.dst.read_mix(t_mix.id).unwrap();
    let got: Vec<String> = doc.tracks.iter().map(|t| loc_of(&fx.dst, t.library_id)).collect();
    assert_eq!(got, locs(&fx.src, &[c, a]));
    assert_eq!((t_mix.genre.as_str(), t_mix.rating, t_mix.comments.as_str()), ("House", 4, "synthetic"));
    let s_info = fx.src.mix_info(mix).unwrap().unwrap();
    assert_eq!((t_mix.date_created, t_mix.date_modified), (s_info.date_created, s_info.date_modified));
    let t_img: Vec<u8> = fx.dst.con.query_row("SELECT data FROM miximages WHERE id = ?1", [t_mix.image_id], |r| r.get(0)).unwrap();
    assert_eq!(t_img, vec![0x89, 0x50, 0x4e, 0x47]);
    let rows = fx.dst.mix_track_rows(t_mix.id).unwrap();
    assert_eq!(rows.len(), 2);
    for (row, t) in rows.iter().zip(&doc.tracks) {
        assert_eq!(row.track_id, t.library_id);
    }

    // the case keeps its order and metadata
    let t_case = fx.dst.case_infos().unwrap().into_iter().find(|c| c.name == "Opening").unwrap();
    let ids = fx.dst.case_track_ids(t_case.id).unwrap();
    assert_eq!(locs(&fx.dst, &ids), locs(&fx.src, &[c, a, b]));
    assert_eq!((t_case.genre.as_str(), t_case.year, t_case.rating, t_case.comments.as_str()), ("House", 2001, 3, "note"));

    // idempotent
    all_same(&fx.plan(&SyncSettings::default()));
}

#[test]
fn both_directions_use_the_same_engine() {
    let mut fx = Fx::new();
    let mut a = Library::open(fx.src.root()).unwrap();
    fx.add(&mut a, "alpha", 3000, "Alpha");
    fx.src = a;
    let p = fx.plan(&SyncSettings::default());
    fx.run(&p, &[]).unwrap();
    // now the other way: the new target holds the track and the old source gets nothing to do
    let back = plan(&fx.dst, &fx.src, &SyncSettings::default(), "Pacemaker").unwrap();
    all_same(&back);
}

#[test]
fn tracks_on_both_follow_the_setting() {
    for (mode, source_newer, expect_update) in [
        (TracksOnBoth::Newer, true, true),
        (TracksOnBoth::Newer, false, false),
        (TracksOnBoth::Leave, true, false),
        (TracksOnBoth::Source, false, true),
    ] {
        let mut fx = Fx::new();
        let mut s = Library::open(fx.src.root()).unwrap();
        let mut d = Library::open(fx.dst.root()).unwrap();
        let sid = fx.add(&mut s, "alpha", 3000, "Alpha");
        let did = fx.add(&mut d, "alpha", 3000, "Alpha");
        let (sd, dd) = if source_newer { (2_000_000_000_000_i64, 1_000_000_000_000_i64) } else { (1_000_000_000_000, 2_000_000_000_000) };
        set_track(&s, sid, &format!("rating = 5, key = '8A', times_played = 7, last_played = 500, date_modified = {sd}"));
        set_track(&d, did, &format!("rating = 1, times_played = 9, last_played = 100, date_modified = {dd}"));
        // the source sidecar differs, so an update has something to copy
        let loc = loc_of(&s, sid);
        let (_, side) = s.track_paths(&loc).unwrap();
        std::fs::write(&side, b"changed sidecar").unwrap();
        fx.src = s;
        fx.dst = d;
        let settings = SyncSettings { tracks_on_both: mode, ..SyncSettings::default() };
        let p = fx.plan(&settings);
        let it = item(&p, &format!("track:{loc}"));
        assert_eq!(it.action == SyncAction::Update, expect_update, "{mode:?} source_newer={source_newer}: {:?}", it.action);
        fx.run(&p, &[]).unwrap();
        if expect_update {
            assert_eq!(int(&fx.dst, did, "rating"), 5);
            assert_eq!(int(&fx.dst, did, "date_modified"), sd);
            assert_eq!(std::fs::read(fx.dst.track_paths(&loc).unwrap().1).unwrap(), b"changed sidecar");
            assert_eq!(int(&fx.dst, did, "times_played"), 9, "the higher play count stays");
            assert_eq!(int(&fx.dst, did, "last_played"), 500, "the later last-played wins");
            all_same(&fx.plan(&settings));
        } else {
            assert_eq!(int(&fx.dst, did, "rating"), 1);
        }
    }
}

#[test]
fn play_counts_can_be_left() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let mut d = Library::open(fx.dst.root()).unwrap();
    let sid = fx.add(&mut s, "alpha", 3000, "Alpha");
    let did = fx.add(&mut d, "alpha", 3000, "Alpha");
    set_track(&s, sid, "rating = 5, times_played = 70, date_modified = 2000000000000");
    set_track(&d, did, "times_played = 9, date_modified = 1000000000000");
    fx.src = s;
    fx.dst = d;
    let settings = SyncSettings { play_counts: PlayCounts::Leave, ..SyncSettings::default() };
    let p = fx.plan(&settings);
    fx.run(&p, &[]).unwrap();
    assert_eq!((int(&fx.dst, did, "rating"), int(&fx.dst, did, "times_played")), (5, 9));
}

#[test]
fn unset_cue_point_takes_the_targets_convention() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let sid = fx.add(&mut s, "alpha", 3000, "Alpha");
    set_track(&s, sid, "cue_point = -1, discid = 'C:'");
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    fx.run(&p, &[]).unwrap();
    let (cue, disc): (f64, String) = fx.dst.con.query_row("SELECT cue_point, discid FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((cue, disc.as_str()), (NO_CUE, ""));
}

#[test]
fn missing_files_skip_the_track_and_the_mix_that_needs_it() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    make_mix(&mut s, "Needs bravo", &[a, b]);
    make_mix(&mut s, "Fine", &[a, c]);
    let case = s.create_case_with_tracks("Some", &[a, b, c]).unwrap();
    let _ = case;
    let (audio, _) = s.track_paths(&loc_of(&s, b)).unwrap();
    std::fs::remove_file(audio).unwrap();
    let (_, side) = s.track_paths(&loc_of(&s, c)).unwrap();
    std::fs::remove_file(side).unwrap();
    fx.src = s;

    let p = fx.plan(&SyncSettings::default());
    assert!(reason(item(&p, &format!("track:{}", loc_of(&fx.src, b)))).contains("audio"));
    assert!(reason(item(&p, &format!("track:{}", loc_of(&fx.src, c)))).contains(".str"));
    let m = reason(item(&p, "mix:needs bravo"));
    assert!(m.contains("Synthetic - Bravo"), "{m}");
    assert!(reason(item(&p, "mix:fine")).contains("Synthetic - Charlie"));
    assert_eq!(item(&p, "case:some").action, SyncAction::Copy);
    assert!(item(&p, "case:some").detail.contains("2 of 3"));
    let r = fx.run(&p, &[]).unwrap();
    assert_eq!(r.failed, 0);
    assert_eq!(fx.dst.tracks().unwrap().len(), 1);
    let cs = fx.dst.case_infos().unwrap();
    assert_eq!((cs.len(), cs[0].track_count), (1, 1), "the case keeps what could be copied");
    assert!(fx.dst.mix_infos().unwrap().is_empty());
}

#[test]
fn excluding_a_track_turns_the_mix_into_a_skip_with_a_reason() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    make_mix(&mut s, "Both", &[a, b]);
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    assert_eq!(item(&p, "mix:both").action, SyncAction::Copy);
    let left_out = format!("track:{}", loc_of(&fx.src, b));
    let r = fx.run(&p, std::slice::from_ref(&left_out)).unwrap();
    let mix = r.items.iter().find(|i| i.key == "mix:both").unwrap();
    match &mix.outcome {
        Outcome::Skipped { reason } => assert!(reason.contains("Synthetic - Bravo"), "{reason}"),
        o => panic!("{o:?}"),
    }
    let t = r.items.iter().find(|i| i.key == left_out).unwrap();
    assert_eq!(t.outcome, Outcome::Skipped { reason: "left out by you".into() });
    assert_eq!(fx.dst.tracks().unwrap().len(), 1);
    assert!(fx.dst.mix_infos().unwrap().is_empty());
}

#[test]
fn mixes_with_the_same_name_follow_the_setting() {
    for (mode, expect) in [
        (MixesSameName::Newer, "replace"),
        (MixesSameName::Replace, "replace"),
        (MixesSameName::Skip, "skip"),
        (MixesSameName::KeepBoth, "new"),
    ] {
        let mut fx = Fx::new();
        let mut s = Library::open(fx.src.root()).unwrap();
        let mut d = Library::open(fx.dst.root()).unwrap();
        let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
        let sb = fx.add(&mut s, "bravo", 3100, "Bravo");
        let da = fx.add(&mut d, "alpha", 3000, "Alpha");
        make_mix(&mut s, "Set", &[sa, sb]);
        make_mix(&mut d, "Set", &[da]);
        s.con.execute("UPDATE mixes SET date_modified = 2000000000000", []).unwrap();
        d.con.execute("UPDATE mixes SET date_modified = 1000000000000", []).unwrap();
        fx.src = s;
        fx.dst = d;
        let settings = SyncSettings { mixes_same_name: mode, ..SyncSettings::default() };
        let p = fx.plan(&settings);
        let it = item(&p, "mix:set");
        match expect {
            "replace" => assert_eq!(it.action, SyncAction::Replace, "{mode:?}"),
            "skip" => assert!(matches!(it.action, SyncAction::Skip { .. }), "{mode:?}"),
            _ => assert_eq!(it.action, SyncAction::CopyAsNew { name: "Set (from computer)".into() }),
        }
        fx.run(&p, &[]).unwrap();
        let names: Vec<String> = fx.dst.mix_infos().unwrap().into_iter().map(|m| m.name).collect();
        match expect {
            "replace" => {
                assert_eq!(names, vec!["Set"]);
                let m = fx.dst.mix_infos().unwrap().remove(0);
                assert_eq!(fx.dst.read_mix(m.id).unwrap().tracks.len(), 2);
                assert_eq!(m.date_modified, 2_000_000_000_000);
            }
            "skip" => assert_eq!(names, vec!["Set"]),
            _ => assert_eq!(names, vec!["Set", "Set (from computer)"]),
        }
        if mode != MixesSameName::Skip {
            nothing_left_to_do(&fx.plan(&settings));
        }
    }
}

#[test]
fn newer_mix_on_the_target_is_kept() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let mut d = Library::open(fx.dst.root()).unwrap();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    make_mix(&mut s, "Set", &[sa]);
    make_mix(&mut d, "Set", &[da]);
    s.con.execute("UPDATE mixes SET date_modified = 1000000000000", []).unwrap();
    d.con.execute("UPDATE mixes SET date_modified = 2000000000000", []).unwrap();
    fx.src = s;
    fx.dst = d;
    let p = fx.plan(&SyncSettings::default());
    assert!(reason(item(&p, "mix:set")).contains("newer"));
}

#[test]
fn keep_both_names_stay_unique() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let mut d = Library::open(fx.dst.root()).unwrap();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    make_mix(&mut s, "Set", &[sa]);
    make_mix(&mut s, "set ", &[sa]); // same name once trimmed and folded: one item, and a skip that says so
    make_mix(&mut d, "Set", &[da]);
    make_mix(&mut d, "Set (from computer)", &[da]);
    make_mix(&mut d, "set (FROM computer) 2", &[da]);
    s.con.execute("UPDATE mixes SET date_modified = 2000000000000", []).unwrap();
    fx.src = s;
    fx.dst = d;
    let settings = SyncSettings { mixes_same_name: MixesSameName::KeepBoth, ..SyncSettings::default() };
    let p = fx.plan(&settings);
    assert_eq!(p.items.iter().filter(|i| i.kind == ItemKind::Mix && i.action.acts()).count(), 1);
    assert_eq!(item(&p, "mix:set").action, SyncAction::CopyAsNew { name: "Set (from computer) 3".into() });
    assert!(reason(item(&p, "mix:set#2")).contains("already in the plan"));
    fx.run(&p, &[]).unwrap();
    nothing_left_to_do(&fx.plan(&settings));
    // the other direction names its copies after the Pacemaker
    let back = plan(&fx.dst, &fx.src, &settings, "Pacemaker").unwrap();
    assert!(back.items.iter().filter(|i| i.kind == ItemKind::Mix).all(|i| match &i.action {
        SyncAction::CopyAsNew { name } => name.contains("(from Pacemaker)"),
        _ => true,
    }));
}

#[test]
fn cases_merge_replace_and_skip() {
    for mode in [CasesSameName::Merge, CasesSameName::Replace, CasesSameName::Skip] {
        let mut fx = Fx::new();
        let mut s = Library::open(fx.src.root()).unwrap();
        let mut d = Library::open(fx.dst.root()).unwrap();
        let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
        let sb = fx.add(&mut s, "bravo", 3100, "Bravo");
        let sc = fx.add(&mut s, "charlie", 3200, "Charlie");
        let db = fx.add(&mut d, "bravo", 3100, "Bravo");
        let dz = fx.add(&mut d, "zulu", 2900, "Zulu");
        s.create_case_with_tracks("Crate", &[sa, sb, sc]).unwrap();
        let tc = d.create_case_with_tracks("crate", &[dz, db]).unwrap();
        fx.src = s;
        fx.dst = d;
        let settings = SyncSettings { cases_same_name: mode, ..SyncSettings::default() };
        let p = fx.plan(&settings);
        fx.run(&p, &[]).unwrap();
        let names: Vec<String> = fx.dst.case_track_ids(tc).unwrap().iter().map(|i| fx.dst.con.query_row("SELECT title FROM tracks WHERE track_id = ?1", [i], |r| r.get(0)).unwrap()).collect();
        match mode {
            CasesSameName::Merge => assert_eq!(names, vec!["Zulu", "Bravo", "Alpha", "Charlie"], "the target's order first, then the missing ones in the source's order"),
            CasesSameName::Replace => assert_eq!(names, vec!["Alpha", "Bravo", "Charlie"]),
            CasesSameName::Skip => assert_eq!(names, vec!["Zulu", "Bravo"]),
        }
        assert_eq!(fx.dst.case_infos().unwrap().len(), 1, "no second case");
        if mode != CasesSameName::Skip {
            nothing_left_to_do(&fx.plan(&settings));
        }
    }
}

#[test]
fn remove_missing_respects_what_still_uses_a_track() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let mut d = Library::open(fx.dst.root()).unwrap();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    let dz = fx.add(&mut d, "zulu", 2900, "Zulu"); // used by a case that stays
    let dy = fx.add(&mut d, "yankee", 2800, "Yankee"); // used by a mix that goes
    let dx = fx.add(&mut d, "xray", 2700, "Xray"); // used by nothing
    let _ = sa;
    s.create_case("Keep").unwrap();
    d.create_case_with_tracks("Keep", &[da, dz]).unwrap();
    d.create_case_with_tracks("Only here", &[dx]).unwrap();
    d.delete_case(d.case_id_by_name("Only here").unwrap().unwrap()).unwrap();
    make_mix(&mut d, "Only on target", &[dy]);
    let (audio_x, side_x) = d.track_paths(&loc_of(&d, dx)).unwrap();
    let key_x = format!("remove-track:{}", loc_of(&d, dx));
    let key_y = format!("remove-track:{}", loc_of(&d, dy));
    let key_z = format!("remove-track:{}", loc_of(&d, dz));
    fx.src = s;
    fx.dst = d;

    // off by default: nothing is planned to go
    assert!(!fx.plan(&SyncSettings::default()).items.iter().any(|i| i.action == SyncAction::Remove));

    let settings = SyncSettings { remove_missing: true, ..SyncSettings::default() };
    let p = fx.plan(&settings);
    assert_eq!(item(&p, "remove-mix:only on target").action, SyncAction::Remove);
    assert_eq!(item(&p, &key_x).action, SyncAction::Remove);
    assert_eq!(item(&p, &key_y).action, SyncAction::Remove, "its only user is also being removed");
    let z = reason(item(&p, &key_z));
    assert!(z.contains("case \"Keep\""), "{z}");

    // leaving the mix removal out keeps the track that mix uses
    let r = fx.run(&p, &["remove-mix:only on target".to_string()]).unwrap();
    assert_eq!(r.failed, 0);
    let y = r.items.iter().find(|i| i.key == key_y).unwrap();
    match &y.outcome {
        Outcome::Skipped { reason } => assert!(reason.contains("mix \"Only on target\""), "{reason}"),
        o => panic!("{o:?}"),
    }
    assert!(!audio_x.exists() && !side_x.exists(), "the unused track's files are gone");
    let left: Vec<String> = fx.dst.tracks().unwrap().into_iter().map(|t| t.title).collect();
    assert_eq!(left, vec!["Alpha", "Zulu", "Yankee"]);

    // now with everything included
    let p2 = fx.plan(&settings);
    let r2 = fx.run(&p2, &[]).unwrap();
    assert_eq!(r2.failed, 0);
    let left: Vec<String> = fx.dst.tracks().unwrap().into_iter().map(|t| t.title).collect();
    assert_eq!(left, vec!["Alpha", "Zulu"]);
    assert!(fx.dst.mix_infos().unwrap().is_empty());
}

#[test]
fn a_changed_library_makes_the_plan_stale() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    fx.add(&mut s, "alpha", 3000, "Alpha");
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    assert_eq!(p.digest, fx.plan(&SyncSettings::default()).digest, "planning is deterministic");
    let mut d = Library::open(fx.dst.root()).unwrap();
    fx.add(&mut d, "zulu", 2900, "Zulu");
    fx.dst = d;
    let err = fx.run(&p, &[]).unwrap_err().to_string();
    assert!(err.contains(STALE), "{err}");
    assert_eq!(fx.dst.tracks().unwrap().len(), 1, "nothing was written");
    // and a different setting is a different plan
    assert_ne!(fx.plan(&SyncSettings::default()).digest, fx.plan(&SyncSettings { remove_missing: true, ..SyncSettings::default() }).digest);
}

#[test]
fn cancel_between_items_leaves_a_consistent_target() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    make_mix(&mut s, "All", &[a, b, c]);
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    let polls = Cell::new(0);
    let seen = Cell::new(0);
    let r = apply(&fx.src, &mut fx.dst, &p, &[], &mut |_| seen.set(seen.get() + 1), &|| {
        polls.set(polls.get() + 1);
        polls.get() > 1
    })
    .unwrap();
    assert!(r.cancelled);
    assert_eq!((r.done, r.failed), (1, 0));
    assert!(r.items.iter().any(|i| matches!(&i.outcome, Outcome::Skipped { reason } if reason.contains("cancelled"))));
    assert!(seen.get() >= 2);
    let t = fx.dst.tracks().unwrap();
    assert_eq!(t.len(), 1);
    let (audio, side) = fx.dst.track_paths(&t[0].location).unwrap();
    assert!(audio.is_file() && side.is_file());
    assert_eq!(stray_temps(fx.dst.root()), 0);
    assert!(fx.dst.compatibility_problems().unwrap().is_empty());
    assert!(fx.dst.mix_infos().unwrap().is_empty());
    // a second run finishes the job
    let p2 = fx.plan(&SyncSettings::default());
    fx.run(&p2, &[]).unwrap();
    all_same(&fx.plan(&SyncSettings::default()));
}

#[test]
fn not_enough_space_shows_in_the_plan() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    fx.add(&mut s, "alpha", 3000, "Alpha");
    fx.src = s;
    let tight = plan_with_free(&fx.src, &fx.dst, &SyncSettings::default(), "computer", Some(10)).unwrap();
    assert!(!tight.fits && tight.bytes_to_copy > 0 && tight.free_bytes == Some(10));
    let roomy = plan_with_free(&fx.src, &fx.dst, &SyncSettings::default(), "computer", Some(1 << 40)).unwrap();
    assert!(roomy.fits);
    let unknown = plan_with_free(&fx.src, &fx.dst, &SyncSettings::default(), "computer", None).unwrap();
    assert!(unknown.fits, "free space that cannot be read does not block");
    assert_eq!(tight.digest, roomy.digest, "free space is not part of what the user approved");
}

#[test]
fn a_library_cannot_sync_into_itself() {
    let fx = Fx::new();
    let again = Library::open(fx.src.root()).unwrap();
    assert!(plan(&fx.src, &again, &SyncSettings::default(), "computer").is_err());
}

#[test]
fn locations_outside_the_music_folder_are_not_followed() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    set_track(&s, a, "location = '/pmdb_tracks/../../escape.mp3'");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    set_track(&s, b, "location = 'C:\\\\Music\\\\bravo.mp3'");
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    assert_eq!(p.items.len(), 2);
    assert!(p.items.iter().all(|i| matches!(i.action, SyncAction::Skip { .. })));
}

#[test]
fn settings_have_the_documented_defaults_and_read_partial_json() {
    let d = SyncSettings::default();
    assert_eq!(
        (d.tracks_on_both, d.mixes_same_name, d.cases_same_name, d.remove_missing, d.play_counts),
        (TracksOnBoth::Newer, MixesSameName::Newer, CasesSameName::Merge, false, PlayCounts::Higher)
    );
    let s: SyncSettings = serde_json::from_str(r#"{"mixes_same_name":"keep_both"}"#).unwrap();
    assert_eq!(s.mixes_same_name, MixesSameName::KeepBoth);
    assert_eq!(s.tracks_on_both, TracksOnBoth::Newer);
    assert!(serde_json::from_str::<SyncSettings>(r#"{"mixes_same_name":"nope"}"#).is_err());
}

#[test]
fn labels_follow_copied_tracks_without_overwriting_the_targets() {
    use crate::ops_labels::{self as labels, Kind, Op};
    let dir = std::env::temp_dir().join(format!("pm-sync-labels-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = labels::load(&dir, "src").store;
    s.apply(Kind::Track, &["g:aaa".to_string()], &[Op::AddTags { tags: vec!["warm".into(), "Peak".into()] }, Op::SetColour { colour: Some("red".into()) }, Op::SetEnergy { energy: Some(7) }]).unwrap();
    s.apply(Kind::Track, &["g:bbb".to_string()], &[Op::SetColour { colour: Some("blue".into()) }]).unwrap();
    labels::save(&dir, "src", &s, None).unwrap();
    let mut t = labels::load(&dir, "dst").store;
    t.apply(Kind::Track, &["g:aaa".to_string()], &[Op::AddTags { tags: vec!["mine".into()] }, Op::SetColour { colour: Some("green".into()) }]).unwrap();
    labels::save(&dir, "dst", &t, None).unwrap();

    let ids = vec!["aaa".to_string(), "bbb".to_string(), "zzz".to_string(), String::new()];
    assert_eq!(copy_labels(&dir, "src", "dst", &ids).unwrap(), 2);
    let after = labels::load(&dir, "dst").store;
    let a = after.get(Kind::Track, "g:aaa").unwrap();
    assert_eq!(a.tags, vec!["mine", "Peak", "warm"]);
    assert_eq!((a.colour.as_deref(), a.energy), (Some("green"), Some(7)), "the target's colour stays, the missing energy arrives");
    assert_eq!(after.get(Kind::Track, "g:bbb").unwrap().colour.as_deref(), Some("blue"));
    assert_eq!(copy_labels(&dir, "src", "dst", &ids).unwrap(), 0, "again: nothing more to carry");
    let _ = std::fs::remove_dir_all(&dir);
}

fn set_plays(lib: &Library, table: &str, id_col: &str, id: i64, times: i64, last: i64) {
    lib.con.execute(&format!("UPDATE {table} SET times_played = {times}, last_played = {last} WHERE {id_col} = {id}"), []).unwrap();
}

fn plays(lib: &Library, table: &str, id_col: &str, id: i64) -> (i64, i64) {
    lib.con.query_row(&format!("SELECT CAST(times_played AS INTEGER), CAST(last_played AS INTEGER) FROM {table} WHERE {id_col} = {id}"), [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

#[test]
fn plays_alone_come_over_for_tracks_mixes_and_cases() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let mut d = Library::open(fx.dst.root()).unwrap();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    // identical rows, then the device counts plays (no date_modified move)
    let row = |lib: &Library, id: i64| -> (f64, f64) { lib.con.query_row("SELECT date_modified, date_added FROM tracks WHERE track_id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap() };
    let (m, a) = row(&s, sa);
    d.con.execute("UPDATE tracks SET date_modified = ?1, date_added = ?2 WHERE track_id = ?3", params![m, a, da]).unwrap();
    let sm = make_mix(&mut s, "Set", &[sa]);
    let dm = make_mix(&mut d, "Set", &[da]);
    let (cr, md): (i64, i64) = s.con.query_row("SELECT date_created, date_modified FROM mixes WHERE mix_id = ?1", [sm], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    d.con.execute("UPDATE mixes SET date_created = ?1, date_modified = ?2 WHERE mix_id = ?3", params![cr, md, dm]).unwrap();
    let sc = s.create_case_with_tracks("Crate", &[sa]).unwrap();
    let dc = d.create_case_with_tracks("Crate", &[da]).unwrap();
    set_plays(&s, "tracks", "track_id", sa, 7, 500);
    set_plays(&s, "mixes", "mix_id", sm, 3, 600);
    set_plays(&s, "cases", "case_id", sc, 2, 700);
    set_plays(&d, "tracks", "track_id", da, 9, 100);
    fx.src = s;
    fx.dst = d;
    let settings = SyncSettings::default();
    let p = fx.plan(&settings);
    let loc = loc_of(&fx.src, sa);
    for key in [format!("track:{loc}"), "mix:set".to_string(), "case:crate".to_string()] {
        assert_eq!(item(&p, &key).action, SyncAction::UpdatePlays, "{key}");
        assert_eq!(item(&p, &key).detail, PLAYED_MORE);
        assert_eq!(item(&p, &key).bytes, 0);
    }
    let r = fx.run(&p, &[]).unwrap();
    assert_eq!((r.done, r.failed), (3, 0));
    assert_eq!(plays(&fx.dst, "tracks", "track_id", da), (9, 500), "the higher count stays, the later last-played wins");
    assert_eq!(plays(&fx.dst, "mixes", "mix_id", dm), (3, 600));
    assert_eq!(plays(&fx.dst, "cases", "case_id", dc), (2, 700));
    all_same(&fx.plan(&settings));
    // the Leave setting brings nothing
    set_plays(&fx.src, "tracks", "track_id", sa, 50, 900);
    assert_eq!(reason(item(&fx.plan(&SyncSettings { play_counts: PlayCounts::Leave, ..settings.clone() }), &format!("track:{loc}"))), SAME);
    // a later run does not lower anything
    let p = fx.plan(&settings);
    fx.run(&p, &[]).unwrap();
    assert_eq!(plays(&fx.dst, "tracks", "track_id", da), (50, 900));
}

#[test]
fn a_second_source_item_with_the_same_name_or_place_is_reported() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    s.create_case_with_tracks("Crate", &[sa]).unwrap();
    s.create_case_with_tracks("crate ", &[sa]).unwrap();
    fx.src = s;
    let p = fx.plan(&SyncSettings::default());
    assert!(reason(item(&p, "case:crate#2")).contains("already in the plan"));
    assert_eq!(item(&p, "case:crate").action, SyncAction::Copy);
}

#[test]
fn keep_both_copies_are_not_removals_and_trackmap_rows_go_with_a_track() {
    let mut fx = Fx::new();
    let mut s = Library::open(fx.src.root()).unwrap();
    let mut d = Library::open(fx.dst.root()).unwrap();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    let lone = fx.add(&mut d, "lone", 3100, "Lone");
    make_mix(&mut s, "Set", &[sa]);
    make_mix(&mut d, "Set", &[da]);
    make_mix(&mut d, "Set (from computer)", &[da]);
    s.con.execute("UPDATE mixes SET date_modified = 2000000000000", []).unwrap();
    d.con.execute("INSERT INTO trackmap (ext_db, ext_id, int_id) VALUES ('x', 1, ?1)", [lone]).unwrap();
    fx.src = s;
    fx.dst = d;
    let settings = SyncSettings { mixes_same_name: MixesSameName::KeepBoth, remove_missing: true, ..SyncSettings::default() };
    let p = fx.plan(&settings);
    assert!(p.items.iter().all(|i| i.key != "remove-mix:set (from computer)"), "{:?}", p.items.iter().map(|i| &i.key).collect::<Vec<_>>());
    assert_eq!(item(&p, &format!("remove-track:{}", loc_of(&fx.dst, lone))).action, SyncAction::Remove);
    let before: i64 = fx.dst.con.query_row("SELECT COUNT(*) FROM trackmap WHERE int_id = ?1", [lone], |r| r.get(0)).unwrap();
    assert_eq!(before, 1);
    fx.run(&p, &[]).unwrap();
    let left: i64 = fx.dst.con.query_row("SELECT COUNT(*) FROM trackmap WHERE int_id = ?1", [lone], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "no dangling trackmap row");
}

mod selection;
