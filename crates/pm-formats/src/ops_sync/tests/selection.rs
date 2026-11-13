//! Tests of the selection-scoped plan and its apply (`plan_selection`, `apply_selection`).

use super::*;

impl Fx {
    fn libs(&self) -> (Library, Library) {
        (Library::open(self.src.root()).unwrap(), Library::open(self.dst.root()).unwrap())
    }

    fn set(&mut self, s: Library, d: Library) {
        self.src = s;
        self.dst = d;
    }

    pub(super) fn plan_sel(&self, sel: &Selection, settings: &SyncSettings) -> SyncPlan {
        plan_selection(&self.src, &self.dst, settings, "computer", sel, None).unwrap()
    }

    pub(super) fn run_sel(&mut self, p: &SyncPlan, sel: &Selection) -> Result<SyncReport, Error> {
        apply_selection(&self.src, &mut self.dst, p, sel, &mut |_| {}, &|| false)
    }
}

pub(super) fn sel(tracks: &[String], mixes: &[&str], cases: &[&str]) -> Selection {
    Selection { tracks: tracks.to_vec(), mixes: mixes.iter().map(|s| s.to_string()).collect(), cases: cases.iter().map(|s| s.to_string()).collect(), into_case: None }
}

fn into(mut s: Selection, name: &str, create: bool) -> Selection {
    s.into_case = Some(IntoCase { name: name.into(), create });
    s
}

fn keys(p: &SyncPlan) -> Vec<String> {
    p.items.iter().map(|i| i.key.clone()).collect()
}

fn title_of(lib: &Library, id: i64) -> String {
    lib.con.query_row("SELECT title FROM tracks WHERE track_id = ?1", [id], |r| r.get(0)).unwrap()
}

fn track_count(lib: &Library) -> i64 {
    lib.con.query_row("SELECT COUNT(*) FROM tracks", [], |r| r.get(0)).unwrap()
}

#[test]
fn only_the_picked_tracks_are_planned_and_copied() {
    let mut fx = Fx::new();
    let (mut s, d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    make_mix(&mut s, "Unrelated", &[c]);
    s.create_case_with_tracks("Crate", &[a, b, c]).unwrap();
    fx.set(s, d);
    let pick = sel(&locs(&fx.src, &[a, b]), &[], &[]);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    assert_eq!(keys(&p), locs(&fx.src, &[a, b]).iter().map(|l| format!("track:{l}")).collect::<Vec<_>>(), "only the picked tracks: no mix, no case, no removal");
    assert!(p.items.iter().all(|i| i.action == SyncAction::Copy && i.needed_by.is_empty()));
    assert!(p.bytes_to_copy > 6000 && p.fits);

    let r = fx.run_sel(&p, &pick).unwrap();
    assert_eq!((r.done, r.failed, r.cancelled), (2, 0, false));
    assert_eq!(track_count(&fx.dst), 2);
    assert_eq!(fx.dst.mix_infos().unwrap().len() + fx.dst.case_infos().unwrap().len(), 0);
    assert_eq!(stray_temps(fx.dst.root()), 0);
    // again: what is there shows as the same, and nothing is left to copy
    let again = fx.plan_sel(&pick, &SyncSettings::default());
    assert!(again.items.iter().all(|i| reason(i) == SAME) && again.bytes_to_copy == 0);
}

#[test]
fn a_picked_mix_brings_the_tracks_it_needs() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    // the target already has charlie, identical
    fx.add(&mut d, "charlie", 3200, "Charlie");
    make_mix(&mut s, "Warm up", &[c, a]);
    fx.set(s, d);
    let pick = sel(&[], &["warm up"], &[]); // names match trimmed and case-folded
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    assert!(!keys(&p).iter().any(|k| k.contains(&loc_of(&fx.src, b))), "bravo is not needed");
    let alpha = item(&p, &format!("track:{}", loc_of(&fx.src, a)));
    assert_eq!(alpha.action, SyncAction::Copy);
    assert_eq!(alpha.needed_by, vec!["mix \"Warm up\"".to_string()]);
    let charlie = item(&p, &format!("track:{}", loc_of(&fx.src, c)));
    assert!(!charlie.needed_by.is_empty() && !matches!(charlie.action, SyncAction::Copy), "charlie is already there: {:?}", charlie.action);
    let m = item(&p, "mix:warm up");
    assert_eq!(m.action, SyncAction::Copy);
    assert_eq!(m.needs.len(), 2);

    fx.run_sel(&p, &pick).unwrap();
    let t_mix = fx.dst.mix_infos().unwrap().into_iter().find(|m| m.name == "Warm up").unwrap();
    let doc = fx.dst.read_mix(t_mix.id).unwrap();
    let got: Vec<String> = doc.tracks.iter().map(|t| loc_of(&fx.dst, t.library_id)).collect();
    assert_eq!(got, locs(&fx.src, &[c, a]), "ids point at the same places");
    assert_eq!(track_count(&fx.dst), 2, "alpha came, bravo did not");
    assert!(fx.plan_sel(&pick, &SyncSettings::default()).items.iter().all(|i| matches!(&i.action, SyncAction::Skip { .. })));
}

#[test]
fn a_picked_case_brings_its_tracks_in_order() {
    let mut fx = Fx::new();
    let (mut s, d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    fx.add(&mut s, "delta", 3300, "Delta");
    s.create_case_with_tracks("Crate", &[c, a, b]).unwrap();
    fx.set(s, d);
    let pick = sel(&[], &[], &["Crate"]);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    assert_eq!(p.items.iter().filter(|i| i.kind == ItemKind::Track).count(), 3);
    assert!(p.items.iter().filter(|i| i.kind == ItemKind::Track).all(|i| i.needed_by == vec!["case \"Crate\"".to_string()]));
    assert_eq!(item(&p, "case:crate").action, SyncAction::Copy);
    fx.run_sel(&p, &pick).unwrap();
    let t_case = fx.dst.case_infos().unwrap().into_iter().next().unwrap();
    assert_eq!(locs(&fx.dst, &fx.dst.case_track_ids(t_case.id).unwrap()), locs(&fx.src, &[c, a, b]));
    assert_eq!(track_count(&fx.dst), 3);
}

#[test]
fn picked_tracks_go_into_an_existing_or_a_new_case() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let z = fx.add(&mut d, "zulu", 2900, "Zulu");
    let y = fx.add(&mut d, "yankee", 2800, "Yankee");
    let crate_id = d.create_case_with_tracks("Crate", &[z, y]).unwrap();
    fx.set(s, d);
    let tracks = locs(&fx.src, &[a, b]);

    // existing case, matched by name
    let pick = into(sel(&tracks, &[], &[]), "crate", false);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    let case = item(&p, "into-case:crate");
    assert_eq!(case.label, "Add 2 tracks to case crate");
    assert_eq!(case.action, SyncAction::Merge);
    assert_eq!(case.into_case.as_deref(), Some("crate"));
    assert_eq!(p.items.iter().filter(|i| i.kind == ItemKind::Track && i.action == SyncAction::Copy).count(), 2);
    fx.run_sel(&p, &pick).unwrap();
    let names: Vec<String> = fx.dst.case_track_ids(crate_id).unwrap().iter().map(|i| title_of(&fx.dst, *i)).collect();
    assert_eq!(names, vec!["Zulu", "Yankee", "Alpha", "Bravo"], "appended; nothing taken out");
    assert_eq!(fx.dst.case_infos().unwrap().len(), 1);
    // again: already in the case
    let again = fx.plan_sel(&pick, &SyncSettings::default());
    assert_eq!(reason(item(&again, "into-case:crate")), SAME);

    // a case that is not there: only when asked to create it
    let fresh = into(sel(&tracks, &[], &[]), "Fresh", true);
    let p = fx.plan_sel(&fresh, &SyncSettings::default());
    assert_eq!(item(&p, "into-case:fresh").label, "New case Fresh with 2 tracks");
    assert_eq!(item(&p, "into-case:fresh").action, SyncAction::Copy);
    fx.run_sel(&p, &fresh).unwrap();
    let t_fresh = fx.dst.case_infos().unwrap().into_iter().find(|c| c.name == "Fresh").unwrap();
    assert_eq!(locs(&fx.dst, &fx.dst.case_track_ids(t_fresh.id).unwrap()), tracks);
    let missing = into(sel(&tracks, &[], &[]), "Nowhere", false);
    let p = fx.plan_sel(&missing, &SyncSettings::default());
    assert!(reason(item(&p, "into-case:nowhere")).contains("not on the target"));
    assert!(fx.run_sel(&p, &missing).unwrap().done == 0);
    assert_eq!(fx.dst.case_infos().unwrap().len(), 2, "no case was made");
}

#[test]
fn into_case_leaves_out_tracks_that_cannot_come() {
    let mut fx = Fx::new();
    let (mut s, d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let (audio, _) = s.track_paths(&loc_of(&s, b)).unwrap();
    std::fs::remove_file(audio).unwrap();
    fx.set(s, d);
    let pick = into(sel(&locs(&fx.src, &[a, b]), &[], &[]), "New", true);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    assert_eq!(item(&p, "into-case:new").label, "New case New with 1 track");
    assert!(item(&p, "into-case:new").detail.contains("1 of 2 tracks will not be there"));
    let b_item = item(&p, &format!("track:{}", loc_of(&fx.src, b)));
    assert!(reason(b_item).contains("audio file is missing"));
    fx.run_sel(&p, &pick).unwrap();
    let c = fx.dst.case_infos().unwrap().into_iter().next().unwrap();
    assert_eq!(c.track_count, 1);
}

#[test]
fn what_is_already_there_is_a_skip_and_a_gone_item_says_so() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    let (m, added): (f64, f64) = s.con.query_row("SELECT date_modified, date_added FROM tracks WHERE track_id = ?1", [a], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    d.con.execute("UPDATE tracks SET date_modified = ?1, date_added = ?2 WHERE track_id = ?3", params![m, added, da]).unwrap();
    fx.set(s, d);
    let pick = sel(&[loc_of(&fx.src, a), "/pmdb_tracks/0/0/nothing.mp3".into()], &["No such mix"], &["No such case"]);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    assert_eq!(reason(item(&p, &format!("track:{}", loc_of(&fx.src, a)))), SAME);
    assert!(reason(item(&p, "track:/pmdb_tracks/0/0/nothing.mp3")).contains("not in the sending library"));
    assert!(reason(item(&p, "mix:no such mix")).contains("not in the sending library"));
    assert!(reason(item(&p, "case:no such case")).contains("not in the sending library"));
    assert!(p.items.iter().all(|i| !i.action.acts()) && p.bytes_to_copy == 0);
    let r = fx.run_sel(&p, &pick).unwrap();
    assert_eq!((r.done, r.failed), (0, 0));
    // nothing is ever removed, even with the removal setting on
    let rm = SyncSettings { remove_missing: true, ..SyncSettings::default() };
    assert!(fx.plan_sel(&pick, &rm).items.iter().all(|i| i.action != SyncAction::Remove));
}

#[test]
fn only_a_selected_item_going_stale_stops_the_apply() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    fx.add(&mut d, "zulu", 2900, "Zulu");
    fx.set(s, d);
    let pick = sel(&[loc_of(&fx.src, a)], &[], &[]);
    let p = fx.plan_sel(&pick, &SyncSettings::default());

    // unrelated changes on both sides: a track edited, a mix and a case added, a track added
    let (mut s, mut d) = fx.libs();
    set_track(&s, b, "title = 'Changed', date_modified = date_modified + 99, times_played = 4");
    make_mix(&mut s, "New mix", &[b]);
    s.create_case("New case").unwrap();
    fx.add(&mut d, "yankee", 2800, "Yankee");
    d.create_case("Another").unwrap();
    fx.set(s, d);
    let r = fx.run_sel(&p, &pick).expect("unrelated changes are not a stale plan");
    assert_eq!(r.done, 1);
    assert_eq!(track_count(&fx.dst), 3);

    // a selected item changing is
    let (mut s, d) = fx.libs();
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    fx.set(s, d);
    let pick = sel(&[loc_of(&fx.src, c)], &[], &[]);
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    set_track(&fx.src, c, "date_modified = date_modified + 5");
    let err = fx.run_sel(&p, &pick).unwrap_err().to_string();
    assert!(err.contains(STALE), "{err}");
    assert_eq!(track_count(&fx.dst), 3, "nothing was written");
    // so is the target row a selected item meets
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    let (_, mut d) = fx.libs();
    fx.add(&mut d, "charlie", 3200, "Charlie");
    assert!(fx.run_sel(&p, &pick).unwrap_err().to_string().contains(STALE));
    // and a different selection never matches an old plan
    let other = sel(&[], &[], &[]);
    let p2 = fx.plan_sel(&pick, &SyncSettings::default());
    assert!(fx.run_sel(&p2, &other).unwrap_err().to_string().contains(STALE));
}

#[test]
fn a_selection_plan_decides_like_the_full_plan_under_the_adding_settings() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha renamed");
    let db = fx.add(&mut d, "bravo", 3100, "Bravo");
    let z = fx.add(&mut d, "zulu", 2900, "Zulu");
    set_track(&s, a, "date_modified = 2000000000000");
    set_track(&d, db, "date_modified = 2000000000001");
    set_plays(&s, "tracks", "track_id", b, 9, 900);
    let sm = make_mix(&mut s, "Set", &[a, b]);
    make_mix(&mut s, "Other", &[c]);
    make_mix(&mut d, "Set", &[da]);
    make_mix(&mut d, "Set (from computer)", &[da]);
    set_plays(&s, "mixes", "mix_id", sm, 4, 400);
    s.create_case_with_tracks("Crate", &[a, b, c]).unwrap();
    d.create_case_with_tracks("crate", &[z, db]).unwrap();
    fx.set(s, d);
    let pick = Selection {
        tracks: locs(&fx.src, &[a, b, c]),
        mixes: vec!["Set".into(), "Other".into()],
        cases: vec!["Crate".into()],
        into_case: None,
    };
    let mut n = 0;
    for tob in [TracksOnBoth::Newer, TracksOnBoth::Leave, TracksOnBoth::Source] {
        for mix in [MixesSameName::Newer, MixesSameName::Replace, MixesSameName::KeepBoth, MixesSameName::Skip] {
            for case in [CasesSameName::Merge, CasesSameName::Replace, CasesSameName::Skip] {
                for plays in [PlayCounts::Higher, PlayCounts::Leave] {
                    let settings = SyncSettings { tracks_on_both: tob, mixes_same_name: mix, cases_same_name: case, remove_missing: false, play_counts: plays };
                    // a drag decides by the full plan's rules under the settings that only add
                    let full = fx.plan(&drag_settings(&settings));
                    let part = fx.plan_sel(&pick, &settings);
                    assert!(part.items.iter().all(|i| !matches!(i.action, SyncAction::Replace | SyncAction::Update | SyncAction::Remove)), "{settings:?}");
                    assert_eq!(part.items.len(), full.items.len(), "{settings:?}");
                    for it in &part.items {
                        let f = item(&full, &it.key);
                        // only the words differ where a drag explains a track it leaves alone
                        let worded = matches!(&it.action, SyncAction::Skip { reason } if reason.contains("a drag never changes"));
                        if !worded {
                            assert_eq!(it.action, f.action, "{} under {settings:?}", it.key);
                        }
                        assert_eq!((it.bytes, it.target_id), (f.bytes, f.target_id), "{} under {settings:?}", it.key);
                        assert_eq!(it.needs, f.needs, "{}", it.key);
                    }
                    assert_eq!(part.bytes_to_copy, full.bytes_to_copy);
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 72);
}

#[test]
fn keep_both_plays_go_to_the_kept_copy_not_the_original() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let sa = fx.add(&mut s, "alpha", 3000, "Alpha");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha");
    let sm = make_mix(&mut s, "Set", &[sa]);
    let original = make_mix(&mut d, "Set", &[da]);
    let kept = make_mix(&mut d, "Set (from computer)", &[da]);
    // the kept copy carries the source's stamps (it was copied earlier); the original differs
    let (cr, md): (i64, i64) = s.con.query_row("SELECT date_created, date_modified FROM mixes WHERE mix_id = ?1", [sm], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    d.con.execute("UPDATE mixes SET date_created = ?1, date_modified = ?2 WHERE mix_id = ?3", params![cr, md, kept]).unwrap();
    d.con.execute("UPDATE mixes SET date_created = 1, date_modified = 2 WHERE mix_id = ?1", [original]).unwrap();
    set_plays(&s, "mixes", "mix_id", sm, 6, 600);
    set_plays(&d, "mixes", "mix_id", original, 1, 100);
    set_plays(&d, "mixes", "mix_id", kept, 2, 200);
    fx.set(s, d);
    let settings = SyncSettings { mixes_same_name: MixesSameName::KeepBoth, ..SyncSettings::default() };

    for scoped in [false, true] {
        let p = if scoped { fx.plan_sel(&sel(&[], &["Set"], &[]), &settings) } else { fx.plan(&settings) };
        let m = item(&p, "mix:set");
        assert_eq!(m.action, SyncAction::UpdatePlays, "scoped: {scoped}");
        assert_eq!(m.target_id, Some(kept), "the kept copy, not the same-named original (scoped: {scoped})");
    }
    let pick = sel(&[], &["Set"], &[]);
    let p = fx.plan_sel(&pick, &settings);
    fx.run_sel(&p, &pick).unwrap();
    assert_eq!(plays(&fx.dst, "mixes", "mix_id", kept), (6, 600));
    assert_eq!(plays(&fx.dst, "mixes", "mix_id", original), (1, 100), "the original is untouched");
    assert_eq!(fx.dst.mix_infos().unwrap().len(), 2, "no third copy");
    // the kept copy is ahead of the source: nothing to bring, however few plays the original has
    set_plays(&fx.dst, "mixes", "mix_id", kept, 9, 900);
    for scoped in [false, true] {
        let p = if scoped { fx.plan_sel(&pick, &settings) } else { fx.plan(&settings) };
        assert_eq!(reason(item(&p, "mix:set")), SAME, "scoped: {scoped}");
    }
}

#[test]
fn a_big_target_is_not_walked_for_a_few_picked_tracks() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let c = fx.add(&mut s, "charlie", 3200, "Charlie");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha renamed");
    set_track(&s, a, "date_modified = 2000000000000");
    // 30 000 other tracks on the target, none of them the picked ones
    {
        let tx = d.con.unchecked_transaction().unwrap();
        {
            let mut ins = tx.prepare("INSERT INTO tracks (location, title, artist, file_size, global_id) VALUES (?1, ?2, 'Bulk', 1000, ?3)").unwrap();
            for i in 0..30_000 {
                ins.execute(params![format!("/pmdb_tracks/{:x}/{:x}/bulk{i}.mp3", i % 16, (i / 16) % 16), format!("Bulk {i}"), format!("bulk-{i}")]).unwrap();
            }
        }
        tx.commit().unwrap();
    }
    fx.set(s, d);
    let pick = sel(&locs(&fx.src, &[a, b, c]), &[], &[]);
    let t0 = std::time::Instant::now();
    let p = fx.plan_sel(&pick, &SyncSettings::default());
    let scoped = t0.elapsed();
    let t0 = std::time::Instant::now();
    let full = fx.plan(&SyncSettings::default());
    let whole = t0.elapsed();
    assert_eq!(p.items.len(), 3);
    let alpha = item(&p, &format!("track:{}", loc_of(&fx.src, a)));
    assert!(matches!(alpha.action, SyncAction::Skip { .. }), "alpha is there with other details: a drag leaves the target's row alone");
    assert_eq!(alpha.target_id, Some(da));
    assert_eq!(item(&p, &format!("track:{}", loc_of(&fx.src, b))).action, SyncAction::Copy);
    for it in p.items.iter().filter(|i| i.key != alpha.key) {
        assert_eq!(it.action, item(&full, &it.key).action, "{}", it.key);
    }
    assert!(scoped * 3 < whole, "the selection plan should not walk the target: {scoped:?} against {whole:?} for the whole plan");
    let r = fx.run_sel(&p, &pick).unwrap();
    assert_eq!((r.done, r.failed), (2, 0));
    assert_eq!(track_count(&fx.dst), 30_000 + 3);
}

#[test]
fn a_drag_never_overwrites_or_takes_out_anything_on_the_target() {
    let mut fx = Fx::new();
    let (mut s, mut d) = fx.libs();
    let a = fx.add(&mut s, "alpha", 3000, "Alpha new");
    let b = fx.add(&mut s, "bravo", 3100, "Bravo");
    let da = fx.add(&mut d, "alpha", 3000, "Alpha old");
    let z = fx.add(&mut d, "zulu", 2900, "Zulu");
    set_track(&s, a, "date_modified = 2000000000000");
    let sm = make_mix(&mut s, "Set", &[a, b]);
    let tm = make_mix(&mut d, "Set", &[da]);
    s.con.execute("UPDATE mixes SET date_modified = 2000000000000 WHERE mix_id = ?1", [sm]).unwrap();
    s.create_case_with_tracks("Crate", &[b]).unwrap();
    let tc = d.create_case_with_tracks("Crate", &[z, da]).unwrap();
    fx.set(s, d);
    let pick = Selection { tracks: locs(&fx.src, &[a]), mixes: vec!["Set".into()], cases: vec!["Crate".into()], into_case: None };
    let mix_state = |fx: &Fx| (fx.dst.read_mix(tm).unwrap().tracks.iter().map(|t| t.library_id).collect::<Vec<_>>(), fx.dst.mix_info(tm).unwrap().unwrap().date_modified);
    let mix_before = mix_state(&fx);
    for mix in [MixesSameName::Newer, MixesSameName::Replace, MixesSameName::KeepBoth] {
        for case in [CasesSameName::Merge, CasesSameName::Replace] {
            let settings = SyncSettings { tracks_on_both: TracksOnBoth::Source, mixes_same_name: mix, cases_same_name: case, ..SyncSettings::default() };
            let p = fx.plan_sel(&pick, &settings);
            let r = fx.run_sel(&p, &pick).unwrap();
            assert_eq!(r.failed, 0, "{settings:?}");
            let d = &fx.dst;
            assert_eq!(title_of(d, da), "Alpha old", "the target's track row is untouched under {settings:?}");
            assert_eq!(d.case_track_ids(tc).unwrap().iter().filter(|i| **i == z || **i == da).count(), 2, "the target case kept its tracks under {settings:?}");
            assert_eq!(mix_state(&fx), mix_before, "the target mix is untouched under {settings:?}");
        }
    }
}
