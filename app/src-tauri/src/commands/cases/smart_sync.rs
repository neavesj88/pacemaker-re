//! Keeping the Pacemaker copies of smart cases (the star-prefixed device cases) in step with
//! their rules: working out what a refresh would change, and doing it.
//!
//! [`plan`] is the one computation. The preview a user is asked about, the check that decides
//! whether anything needs refreshing, and the write itself all take their track lists from it,
//! so what is shown is what is written (the write recomputes inside the write gate, so a
//! library that changed in between is still written correctly and the report says what was
//! actually done).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use pm_formats::library::{Error, Library, Track};
use serde::Serialize;

use super::smart::{self, Data, SmartCase};

/// A track named in a refresh summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackRef {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The device case does not exist (never made, or deleted on the device): it is made again.
    Create,
    /// The device case exists and differs from the rules (tracks, their order, or its name).
    Update,
    Unchanged,
    /// Cannot be refreshed; nothing was written for it. See `error`.
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    /// The smart case.
    pub id: u32,
    pub name: String,
    /// What its device case is (or will be) called.
    pub device_name: String,
    pub action: Action,
    /// Tracks in the device case now, and once refreshed (0 for a case that does not exist yet
    /// / one that cannot run).
    pub before: usize,
    pub after: usize,
    pub added: Vec<TrackRef>,
    pub removed: Vec<TrackRef>,
    /// The same tracks, but in a different order.
    pub reordered: bool,
    /// The name the device case has now, when it is not `device_name`.
    pub renamed_from: Option<String>,
    /// `Create` for a smart case that already had a device case id: the case was deleted (or
    /// replaced) on the device, and is made again.
    pub recreated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub items: Vec<Item>,
    /// Items that are (or were) written: `Create` and `Update`.
    pub changes: usize,
}

impl Report {
    fn new(items: Vec<Item>) -> Report {
        let changes = items.iter().filter(|i| matches!(i.action, Action::Create | Action::Update)).count();
        Report { items, changes }
    }
}

/// An item and the track list its device case should hold.
pub struct Planned {
    pub item: Item,
    pub ids: Vec<i64>,
}

/// The sentence for a smart case whose rules read its own device copy; such a case is skipped.
pub fn self_reference(sc: &SmartCase) -> Option<&'static str> {
    match sc.device_case_id {
        Some(id) if smart::refers_to_case(&sc.query, id) => {
            Some("This smart case has a rule that reads its own Pacemaker copy, which is rebuilt from the rules. Remove that rule first.")
        }
        _ => None,
    }
}

/// For each smart case (by id) in `wanted` whose rules read the Pacemaker copy of a smart case
/// that, through its own rules, reads the first one's copy back (or read its own copy): the
/// sentence that says so. A copy is rebuilt from the rules, so such cases would rewrite each
/// other's copies for ever (in "auto" mode, endlessly); they are skipped instead. A smart case
/// that merely reads another's copy (no way back) is fine: it settles after one more refresh.
pub fn cycle_notes(wanted: &[&SmartCase]) -> HashMap<u32, String> {
    // edge a -> b: a's rules read the device copy of b
    let reads: Vec<Vec<usize>> = wanted
        .iter()
        .map(|a| wanted.iter().enumerate().filter(|(_, b)| b.device_case_id.is_some_and(|id| smart::refers_to_case(&a.query, id))).map(|(j, _)| j).collect())
        .collect();
    let mut out = HashMap::new();
    for (i, sc) in wanted.iter().enumerate() {
        // everything reachable from i; i is in a cycle when it reaches itself
        let mut seen = vec![false; wanted.len()];
        let mut stack = reads[i].clone();
        while let Some(n) = stack.pop() {
            if !seen[n] {
                seen[n] = true;
                stack.extend(reads[n].iter().copied());
            }
        }
        if !seen[i] {
            continue;
        }
        // the others that read each other with i: reachable from i and reaching i
        let mut peers: Vec<String> = (0..wanted.len())
            .filter(|&j| j != i && seen[j] && {
                let mut back = vec![false; wanted.len()];
                let mut st = reads[j].clone();
                while let Some(n) = st.pop() {
                    if !back[n] {
                        back[n] = true;
                        st.extend(reads[n].iter().copied());
                    }
                }
                back[i]
            })
            .map(|j| format!("“{}”", wanted[j].name.trim()))
            .collect();
        peers.sort();
        let note = if peers.is_empty() {
            self_reference(sc).unwrap_or_default().to_string()
        } else {
            format!(
                "This smart case and {} have rules that read each other's Pacemaker copies, which are rebuilt from the rules, so they would keep changing one another. Remove one of those rules first.",
                peers.join(", ")
            )
        };
        out.insert(sc.id, note);
    }
    out
}

fn track_ref(by_id: &HashMap<i64, &Track>, id: i64) -> TrackRef {
    match by_id.get(&id) {
        Some(t) => TrackRef { track_id: id, title: t.title.clone(), artist: t.artist.clone() },
        None => TrackRef { track_id: id, title: "(no longer in the library)".into(), artist: String::new() },
    }
}

fn skipped(sc: &SmartCase, error: String) -> Planned {
    Planned {
        item: Item {
            id: sc.id,
            name: sc.name.clone(),
            device_name: smart::device_name(sc),
            action: Action::Skip,
            before: 0,
            after: 0,
            added: Vec::new(),
            removed: Vec::new(),
            reordered: false,
            renamed_from: None,
            recreated: false,
            error: Some(error),
        },
        ids: Vec::new(),
    }
}

/// For every smart case in `list` that has "Put on Pacemaker" on: what its device case should
/// hold and how that differs from what it holds now. Reads only.
pub fn plan(lib: &Library, list: &[SmartCase], now_ms: i64, config: Option<&Path>) -> Result<Vec<Planned>, Error> {
    let wanted: Vec<&SmartCase> = list.iter().filter(|s| s.on_device).collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let queries: Vec<&smart::Query> = wanted.iter().map(|s| &s.query).collect();
    let data = Data::load(lib, &queries, now_ms, config)?;
    let by_id: HashMap<i64, &Track> = data.tracks.iter().map(|t| (t.track_id, t)).collect();
    let cycles = cycle_notes(&wanted);
    let mut out = Vec::new();
    for sc in wanted {
        // a smart case that cannot run (a deleted case in a rule, rules edited by hand into
        // something invalid) is reported and skipped; its device copy is left as it was
        // rather than emptied
        if let Err(e) = smart::validate(&sc.name, &sc.query) {
            out.push(skipped(sc, e));
            continue;
        }
        if let Some(e) = cycles.get(&sc.id) {
            out.push(skipped(sc, e.clone()));
            continue;
        }
        let ids = match smart::evaluate(&sc.query, &data) {
            Ok(e) => e.ids,
            Err(e) => {
                out.push(skipped(sc, e));
                continue;
            }
        };
        let device_name = smart::device_name(sc);
        let owned = smart::owned_device_case(lib, sc)?;
        let mut item = Item {
            id: sc.id,
            name: sc.name.clone(),
            device_name: device_name.clone(),
            action: Action::Create,
            before: 0,
            after: ids.len(),
            added: Vec::new(),
            removed: Vec::new(),
            reordered: false,
            renamed_from: None,
            recreated: sc.device_case_id.is_some(),
            error: None,
        };
        match owned {
            None => item.added = ids.iter().map(|&i| track_ref(&by_id, i)).collect(),
            Some(case_id) => {
                let now = lib.case_track_ids(case_id)?;
                let in_new: HashSet<i64> = ids.iter().copied().collect();
                let in_now: HashSet<i64> = now.iter().copied().collect();
                item.before = now.len();
                item.recreated = false;
                item.added = ids.iter().filter(|i| !in_now.contains(i)).map(|&i| track_ref(&by_id, i)).collect();
                item.removed = now.iter().filter(|i| !in_new.contains(i)).map(|&i| track_ref(&by_id, i)).collect();
                let kept_then: Vec<i64> = now.iter().copied().filter(|i| in_new.contains(i)).collect();
                let kept_now: Vec<i64> = ids.iter().copied().filter(|i| in_now.contains(i)).collect();
                item.reordered = kept_then != kept_now;
                let current = lib.case_info(case_id)?.map(|c| c.name).unwrap_or_default();
                if current != device_name {
                    item.renamed_from = Some(current);
                }
                let same = now == ids && item.renamed_from.is_none();
                item.action = if same { Action::Unchanged } else { Action::Update };
            }
        }
        out.push(Planned { item, ids });
    }
    Ok(out)
}

/// The plan as the report the UI shows, nothing written.
pub fn preview(lib: &Library, list: &[SmartCase], now_ms: i64, config: Option<&Path>) -> Result<Report, Error> {
    Ok(Report::new(plan(lib, list, now_ms, config)?.into_iter().map(|p| p.item).collect()))
}

/// Writes every `Create` / `Update` of [`plan`] (nothing for `Unchanged` and `Skip`), records
/// each device case id in the store, and returns what was done. A write that fails turns that
/// one item into a `Skip` with the reason and the others still run.
pub fn apply(lib: &mut Library, dir: &Path, root: &str, list: &[SmartCase], now_ms: i64) -> Result<Report, Error> {
    let planned = plan(lib, list, now_ms, Some(dir))?;
    let mut items = Vec::new();
    for Planned { mut item, ids } in planned {
        if matches!(item.action, Action::Create | Action::Update) {
            let sc = list.iter().find(|s| s.id == item.id).expect("planned from this list");
            let written = smart::put_on_device(lib, sc, &ids).map_err(|e| e.to_string()).and_then(|(case_id, _)| {
                let mut stored = sc.clone();
                stored.device_case_id = Some(case_id);
                smart::update(dir, root, &stored).map_err(|e| format!("the copy was written but the app could not remember it: {e}"))
            });
            if let Err(e) = written {
                item.action = Action::Skip;
                item.error = Some(e);
            }
        }
        items.push(item);
    }
    Ok(Report::new(items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cases::smart::{Query, Rule};
    use pm_formats::{library::TrackMeta, strfile};
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-smartsync-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A library of `n` synthetic tracks "Sync Title i" (rating 0), returning the track ids.
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
            let p = src.join(format!("sync {i}.mp3"));
            std::fs::write(&p, vec![0u8; 2048 + i]).unwrap();
            let meta = TrackMeta { title: format!("Sync Title {i}"), artist: "Sync Artist".into(), format: "MP3".into(), sample_rate: 44100, play_time_secs: 100, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        let _ = std::fs::remove_dir_all(&src); // best effort: Windows may still hold the copied file
        (lib, vol, ids)
    }

    fn starred(id: u32, name: &str, min_stars: u8) -> SmartCase {
        SmartCase { id, name: name.into(), query: Query { rules: vec![Rule::RatingMin { stars: min_stars }], ..Default::default() }, on_device: true, device_case_id: None }
    }

    /// Stores the smart cases the way the app does, so `apply` can record device ids.
    fn stored(dir: &Path, list: Vec<SmartCase>) -> Vec<SmartCase> {
        for s in list {
            smart::save(dir, "P:\\", s.clone()).unwrap();
            // `save` forgets the on-device fields of a new smart case (they only come from the
            // device-copy commands), so put them back the way those commands do
            let mut with = smart::load(dir, "P:\\").unwrap().into_iter().last().unwrap();
            with.on_device = s.on_device;
            smart::update(dir, "P:\\", &with).unwrap();
        }
        smart::load(dir, "P:\\").unwrap()
    }

    #[test]
    fn plan_names_what_a_refresh_would_change_and_apply_does_exactly_that() {
        let (mut lib, vol, ids) = library("plan", 4);
        let dir = temp_dir("plan-cfg");
        lib.set_track_ratings(&[(ids[0], 5), (ids[1], 5)]).unwrap();
        let list = stored(&dir, vec![starred(0, "Top", 5), SmartCase { on_device: false, ..starred(0, "Off device", 5) }]);

        // never put on the device: created from the current matches; the one switched off is not in the plan
        let pre = preview(&lib, &list, 0, None).unwrap();
        assert_eq!((pre.items.len(), pre.changes), (1, 1));
        let it = &pre.items[0];
        assert_eq!((it.name.as_str(), it.device_name.as_str(), it.action, it.before, it.after), ("Top", "\u{2605} Top", Action::Create, 0, 2));
        assert_eq!(it.added.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Sync Title 0", "Sync Title 1"]);
        assert!(!it.recreated);
        assert!(lib.cases().unwrap().is_empty(), "a preview writes nothing");

        let done = apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        assert_eq!(done.items, pre.items, "the report of the write is the preview");
        let case = smart::load(&dir, "P:\\").unwrap()[0].device_case_id.expect("recorded in the store");
        assert_eq!(lib.case_track_ids(case).unwrap(), vec![ids[0], ids[1]]);

        // nothing changed: unchanged, and apply writes nothing (the case row keeps its date)
        let list = smart::load(&dir, "P:\\").unwrap();
        let before = lib.case_info(case).unwrap().unwrap();
        let again = apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        assert_eq!((again.changes, again.items[0].action), (0, Action::Unchanged));
        assert_eq!(lib.case_info(case).unwrap().unwrap().date_modified, before.date_modified);

        // a rating changes: one track in, one out, both named
        lib.set_track_ratings(&[(ids[1], 2), (ids[3], 5)]).unwrap();
        let pre = preview(&lib, &list, 0, None).unwrap();
        let it = &pre.items[0];
        assert_eq!((it.action, it.before, it.after, it.recreated), (Action::Update, 2, 2, false));
        assert_eq!(it.added.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Sync Title 3"]);
        assert_eq!(it.removed.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Sync Title 1"]);
        assert_eq!(it.added[0].artist, "Sync Artist");
        assert_eq!(apply(&mut lib, &dir, "P:\\", &list, 0).unwrap().items, pre.items);
        assert_eq!(lib.case_track_ids(case).unwrap(), vec![ids[0], ids[3]]);

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_labels_file_skips_label_smart_cases_and_never_rewrites_their_device_copy() {
        use crate::commands::cases::smart::TagMatch;
        let (mut lib, vol, ids) = library("badlabels", 3);
        let dir = temp_dir("badlabels-cfg");
        lib.set_track_ratings(&[(ids[0], 5)]).unwrap();
        let root = vol.to_string_lossy().to_string();
        let by_tag = SmartCase { query: Query { rules: vec![Rule::Tags { how: TagMatch::None, tags: vec!["x".into()] }], ..Default::default() }, ..starred(0, "No x", 1) };
        let list = stored(&dir, vec![by_tag, starred(0, "Top", 5)]);

        // labels file fine (absent): "none of x" holds for every track, so its copy has all three
        apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        let list = smart::load(&dir, "P:\\").unwrap();
        let copy = list.iter().find(|s| s.name == "No x").unwrap().device_case_id.unwrap();
        assert_eq!(lib.case_track_ids(copy).unwrap().len(), 3);
        // then the labels file is damaged
        let file = crate::commands::tags::labels_file(&dir, &root);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{ not json").unwrap();

        let pre = preview(&lib, &list, 0, Some(&dir)).unwrap();
        let tagged = pre.items.iter().find(|i| i.name == "No x").unwrap();
        assert_eq!(tagged.action, Action::Skip, "{tagged:?}");
        assert!(tagged.error.as_deref().unwrap().contains("could not be read"));
        let plain = pre.items.iter().find(|i| i.name == "Top").unwrap();
        assert_ne!(plain.action, Action::Skip, "a case without label rules is unaffected");
        let done = apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        assert_eq!(done.items.iter().find(|i| i.name == "No x").unwrap().action, Action::Skip);
        assert_eq!(lib.case_track_ids(copy).unwrap().len(), 3, "the device copy was left as it was");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_renamed_reordered_or_deleted_device_copy_is_noticed_and_a_foreign_one_is_left_alone() {
        let (mut lib, vol, ids) = library("states", 3);
        let dir = temp_dir("states-cfg");
        lib.set_track_ratings(&[(ids[0], 4), (ids[1], 4), (ids[2], 4)]).unwrap();
        let list = stored(&dir, vec![starred(0, "Four", 4)]);
        apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        let list = smart::load(&dir, "P:\\").unwrap();
        let case = list[0].device_case_id.unwrap();
        let now = lib.case_track_ids(case).unwrap();
        assert_eq!(now.len(), 3);

        // renamed on the device (still ours: the star prefix): renamed back
        lib.rename_case(case, "\u{2605} Other name").unwrap();
        let it = &preview(&lib, &list, 0, None).unwrap().items[0];
        assert_eq!((it.action, it.renamed_from.as_deref(), it.added.len(), it.removed.len()), (Action::Update, Some("\u{2605} Other name"), 0, 0));
        apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        assert_eq!(lib.case_info(case).unwrap().unwrap().name, "\u{2605} Four");

        // reordered on the device: same tracks, different order, restored
        let mut shuffled = now.clone();
        shuffled.reverse();
        lib.set_case_tracks(case, &shuffled).unwrap();
        let it = &preview(&lib, &list, 0, None).unwrap().items[0];
        assert_eq!((it.action, it.reordered, it.added.len(), it.removed.len()), (Action::Update, true, 0, 0));
        apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        assert_eq!(lib.case_track_ids(case).unwrap(), now);

        // deleted on the device: made again, and the summary says it was a re-creation
        lib.delete_case(case).unwrap();
        let it = &preview(&lib, &list, 0, None).unwrap().items[0];
        assert_eq!((it.action, it.recreated, it.before, it.after), (Action::Create, true, 0, 3));
        apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        let fresh = smart::load(&dir, "P:\\").unwrap()[0].device_case_id.unwrap();
        assert_ne!(fresh, case);

        // the remembered id now belongs to a plain case that is not ours: it is never touched
        let foreign = lib.create_case_with_tracks("Plain", &[ids[0]]).unwrap();
        let stale = SmartCase { device_case_id: Some(foreign), ..list[0].clone() };
        let it = &preview(&lib, std::slice::from_ref(&stale), 0, None).unwrap().items[0];
        assert_eq!((it.action, it.recreated), (Action::Create, true));
        smart::update(&dir, "P:\\", &stale).unwrap();
        apply(&mut lib, &dir, "P:\\", &[stale], 0).unwrap();
        assert_eq!(lib.case_info(foreign).unwrap().unwrap().name, "Plain");
        assert_eq!(lib.case_track_ids(foreign).unwrap(), vec![ids[0]]);

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_smart_case_that_cannot_run_is_skipped_by_name_and_its_copy_kept_while_the_others_refresh() {
        let (mut lib, vol, ids) = library("skip", 3);
        let dir = temp_dir("skip-cfg");
        lib.set_track_ratings(&[(ids[0], 5)]).unwrap();
        let gone = lib.create_case("Will be deleted").unwrap();
        let broken = SmartCase { id: 0, name: "Broken".into(), query: Query { rules: vec![Rule::InCase { case_id: gone, value: true, case_name: None }], ..Default::default() }, on_device: true, device_case_id: None };
        let list = stored(&dir, vec![starred(0, "Fine", 5), broken]);
        // give "Broken" a device copy while its case still exists
        apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        let list = smart::load(&dir, "P:\\").unwrap();
        let copy = list[1].device_case_id.unwrap();
        lib.delete_case(gone).unwrap();
        lib.set_track_ratings(&[(ids[1], 5)]).unwrap();

        let rep = preview(&lib, &list, 0, None).unwrap();
        assert_eq!(rep.changes, 1);
        assert_eq!((rep.items[1].name.as_str(), rep.items[1].action), ("Broken", Action::Skip));
        assert!(rep.items[1].error.as_ref().unwrap().contains("no longer exists"), "{:?}", rep.items[1].error);
        let done = apply(&mut lib, &dir, "P:\\", &list, 0).unwrap();
        assert_eq!((done.items[0].action, done.items[1].action), (Action::Update, Action::Skip));
        assert!(lib.case_info(copy).unwrap().is_some(), "the broken one's copy is left, not emptied");
        assert_eq!(lib.case_track_ids(list[0].device_case_id.unwrap()).unwrap().len(), 2);

        // a rule that reads its own copy is skipped with the explanation
        let own = SmartCase { id: 9, name: "Own".into(), query: Query { rules: vec![Rule::InCase { case_id: copy, value: false, case_name: None }], ..Default::default() }, on_device: true, device_case_id: Some(copy) };
        let rep = preview(&lib, &[own], 0, None).unwrap();
        assert!(rep.items[0].error.as_ref().unwrap().contains("reads its own Pacemaker copy"));

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn smart_cases_that_read_each_others_copies_are_skipped_so_they_cannot_rewrite_one_another_for_ever() {
        let (mut lib, vol, ids) = library("cycle", 3);
        let dir = temp_dir("cycle-cfg");
        let (ca, cb) = (lib.create_case_with_tracks("\u{2605} A", &[ids[0]]).unwrap(), lib.create_case_with_tracks("\u{2605} B", &[ids[1]]).unwrap());
        let reads = |case_id: i64, value: bool| Query { rules: vec![Rule::InCase { case_id, value, case_name: None }], ..Default::default() };
        let mk = |id: u32, name: &str, query: Query, copy: Option<i64>| SmartCase { id, name: name.into(), query, on_device: true, device_case_id: copy };
        // A is in the star copy of B, B is not in the star copy of A: every write to one changes the other
        let a = mk(1, "A", reads(cb, true), Some(ca));
        let b = mk(2, "B", reads(ca, false), Some(cb));
        // C only reads B's copy (no way back): an ordinary dependent, refreshed normally
        let c = mk(3, "C", reads(cb, true), None);
        let list = [a.clone(), b.clone(), c];

        let rep = preview(&lib, &list, 0, None).unwrap();
        let by = |n: &str| rep.items.iter().find(|i| i.name == n).unwrap();
        assert_eq!((by("A").action, by("B").action, by("C").action), (Action::Skip, Action::Skip, Action::Create));
        assert!(by("A").error.as_ref().unwrap().contains("This smart case and \u{201c}B\u{201d} have rules that read each other"), "{:?}", by("A").error);
        assert!(by("B").error.as_ref().unwrap().contains("and \u{201c}A\u{201d} have rules"), "{:?}", by("B").error);

        // the write leaves the looping pair's copies as they were
        let before = (lib.case_track_ids(ca).unwrap(), lib.case_track_ids(cb).unwrap());
        let done = apply(&mut lib, &dir, "P:\\", &[a, b], 0).unwrap();
        assert_eq!(done.changes, 0);
        assert_eq!(before, (lib.case_track_ids(ca).unwrap(), lib.case_track_ids(cb).unwrap()));

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
