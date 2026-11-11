//! Smart cases: saved rule sets over the library that can optionally be materialised as a real
//! device case. The rules live in the app's config dir (one JSON file per library), never on
//! the device; only an explicit "Put on Pacemaker" writes anything to music.db, and that goes
//! through pm-formats `Library` like every other case write.
//!
//! The rule engine (rules, nested groups, order, limit, validation, evaluation) is in
//! `smart/rules.rs`; this file is the store and the device copy.

mod rules;
#[cfg(test)]
mod rules_tests;

use std::path::{Path, PathBuf};

use pm_formats::library::{Error, Library, MAX_CASE_NAME_CHARS};
use serde::{Deserialize, Serialize};

pub use rules::{Data, Eval, Query, evaluate, migrate, referenced_cases, validate_query};

/// Prefix of the device case a smart case is materialised as, so it is recognisable on the
/// device and in the Cases list. (How the device's font draws the star is not verified.)
pub const DEVICE_PREFIX: &str = "\u{2605} ";

/// The version the store file is written as. 1 (no `version` field) held a flat AND list of the
/// original rules and no order or limit; every one of its fields is still read, and the old
/// genre rule is rewritten as a text rule on load (see [`rules::migrate`]).
const STORE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartCase {
    /// 0 when new; the store assigns the next free id on save.
    #[serde(default)]
    pub id: u32,
    pub name: String,
    /// The rules (a flat list read as before: `mode` defaults to "all"), order and limit.
    #[serde(flatten)]
    pub query: Query,
    /// The user asked for this to exist as a case on the Pacemaker.
    #[serde(default)]
    pub on_device: bool,
    /// `case_id` of that device case once it exists.
    #[serde(default)]
    pub device_case_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SmartFile {
    /// Absent in files written before smart cases had groups, order and limits (= 1).
    #[serde(default)]
    version: u32,
    smart_cases: Vec<SmartCase>,
}

/// Why a smart case cannot be saved or run; worded for the user.
pub fn validate(name: &str, q: &Query) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("The smart case needs a name.".into());
    }
    // The device copy is named with the star prefix, and the device limit applies to that.
    if DEVICE_PREFIX.chars().count() + name.trim().chars().count() > MAX_CASE_NAME_CHARS {
        let max = MAX_CASE_NAME_CHARS - DEVICE_PREFIX.chars().count();
        return Err(format!("The smart case name is too long (at most {max} characters)."));
    }
    validate_query(q)
}

/// Whether the rules refer to this case id (a smart case must not read its own device copy:
/// the copy is rebuilt from the rules, so the answer would depend on the last refresh).
pub fn refers_to_case(q: &Query, case_id: i64) -> bool {
    let mut ids = std::collections::HashSet::new();
    referenced_cases(&q.rules, &mut ids);
    ids.contains(&case_id)
}

// ---- storage -----------------------------------------------------------------------------

/// The file for a library: named by a hash of its root path (a drive letter or mount point),
/// so nothing about the device is written next to the rules.
pub fn store_path(dir: &Path, root: &str) -> PathBuf {
    let norm = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
    for b in norm.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    dir.join(format!("smart-cases-{h:016x}.json"))
}

pub fn load(dir: &Path, root: &str) -> Result<Vec<SmartCase>, String> {
    let p = store_path(dir, root);
    match std::fs::read_to_string(&p) {
        Ok(s) => {
            let mut f = serde_json::from_str::<SmartFile>(&s).map_err(|e| format!("{} is not readable: {e}", p.display()))?;
            for sc in &mut f.smart_cases {
                migrate(&mut sc.query.rules);
            }
            Ok(f.smart_cases)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

fn save_all(dir: &Path, root: &str, list: Vec<SmartCase>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = store_path(dir, root);
    keep_old_version(&p)?;
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&SmartFile { version: STORE_VERSION, smart_cases: list }).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// Before the first write over a file an older version of the app wrote, keeps a copy of it
/// next to it (`...json.v1.bak`), so a downgrade or a bad migration loses nothing. An existing
/// backup is never overwritten.
fn keep_old_version(p: &Path) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(p) else {
        return Ok(()); // no file yet (an unreadable one makes `load` refuse before we get here)
    };
    let old = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v.get("version").and_then(|n| n.as_u64())).unwrap_or(1);
    let bak = p.with_extension("json.v1.bak");
    if old < u64::from(STORE_VERSION) && !bak.exists() {
        std::fs::write(&bak, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Saves a new (id 0) or existing smart case. The on-device fields of an existing one are kept
/// from the store, not taken from the caller, so an editor holding a stale copy cannot detach
/// or re-attach a device case. Returns the stored smart case.
pub fn save(dir: &Path, root: &str, incoming: SmartCase) -> Result<SmartCase, String> {
    let mut incoming = incoming;
    migrate(&mut incoming.query.rules);
    validate(&incoming.name, &incoming.query)?;
    let mut list = load(dir, root)?;
    let name = incoming.name.trim().to_string();
    let stored = if let Some(cur) = list.iter_mut().find(|s| s.id == incoming.id && incoming.id != 0) {
        cur.name = name;
        cur.query = incoming.query;
        cur.clone()
    } else {
        let id = list.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let fresh = SmartCase { id, name, query: incoming.query, on_device: false, device_case_id: None };
        list.push(fresh.clone());
        fresh
    };
    save_all(dir, root, list)?;
    Ok(stored)
}

/// Replaces the stored copy of a smart case (used after a device write updates its fields).
pub fn update(dir: &Path, root: &str, sc: &SmartCase) -> Result<(), String> {
    let mut list = load(dir, root)?;
    match list.iter_mut().find(|s| s.id == sc.id) {
        Some(slot) => *slot = sc.clone(),
        None => return Err(format!("no smart case {}", sc.id)),
    }
    save_all(dir, root, list)
}

/// Removes a smart case from the store; returns it, or `None` if it was not there.
pub fn remove(dir: &Path, root: &str, id: u32) -> Result<Option<SmartCase>, String> {
    let mut list = load(dir, root)?;
    let Some(pos) = list.iter().position(|s| s.id == id) else {
        return Ok(None);
    };
    let gone = list.remove(pos);
    save_all(dir, root, list)?;
    Ok(Some(gone))
}

// ---- the device copy ----------------------------------------------------------------------

/// The name of the device case for a smart case.
pub fn device_name(sc: &SmartCase) -> String {
    format!("{DEVICE_PREFIX}{}", sc.name.trim())
}

/// The device case a smart case remembers, but only while it is still ours: the id must exist
/// and the case must still carry [`DEVICE_PREFIX`]. A different card mounted at the same path,
/// or a case the user renamed and repurposed, fails this and is left alone.
fn owned_device_case(lib: &Library, sc: &SmartCase) -> Result<Option<i64>, Error> {
    let Some(id) = sc.device_case_id else {
        return Ok(None);
    };
    Ok(lib.case_info(id)?.filter(|c| c.name.starts_with(DEVICE_PREFIX)).map(|_| id))
}

/// Creates or refreshes the device case for `sc` so it holds exactly the current matches, in
/// match order; returns the case id and how many tracks it holds. The case is ours only while
/// it still carries the star prefix: it is recreated if it was deleted on the device or no
/// longer looks like ours, and a prefixed case that was merely renamed is renamed back.
pub fn put_on_device(lib: &mut Library, sc: &SmartCase, ids: &[i64]) -> Result<(i64, usize), Error> {
    let name = device_name(sc);
    let case_id = match owned_device_case(lib, sc)? {
        Some(id) => {
            // Members first: if that fails nothing has been renamed yet.
            lib.set_case_tracks(id, ids)?;
            lib.rename_case(id, &name)?;
            id
        }
        None => lib.create_case_with_tracks(&name, ids)?,
    };
    Ok((case_id, ids.len()))
}

/// Deletes the device case of `sc` if it still exists and is still ours (see
/// [`owned_device_case`]); returns whether one was deleted.
pub fn take_off_device(lib: &mut Library, sc: &SmartCase) -> Result<bool, Error> {
    match owned_device_case(lib, sc)? {
        Some(id) => lib.delete_case(id),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::rules::Rule;
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-smart-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn sc(name: &str) -> SmartCase {
        SmartCase { id: 0, name: name.into(), query: Query { rules: vec![Rule::PlaysZero], ..Default::default() }, on_device: false, device_case_id: None }
    }

    #[test]
    fn store_is_per_library_and_keeps_device_fields_out_of_the_callers_hands() {
        let dir = temp_dir("store");
        assert!(load(&dir, "P:\\").unwrap().is_empty(), "no file yet is an empty list");
        assert_eq!(store_path(&dir, "P:\\"), store_path(&dir, "p:/"), "case and slash style do not matter");
        assert_ne!(store_path(&dir, "P:\\"), store_path(&dir, "Q:\\"));

        let a = save(&dir, "P:\\", sc("One")).unwrap();
        let b = save(&dir, "P:\\", sc("Two")).unwrap();
        assert_eq!((a.id, b.id), (1, 2));
        assert!(save(&dir, "Q:\\", sc("Elsewhere")).is_ok());
        assert_eq!(load(&dir, "P:\\").unwrap().len(), 2);
        assert_eq!(load(&dir, "Q:\\").unwrap().len(), 1);

        // device fields set by the store survive a save from a stale editor copy
        let mut attached = a.clone();
        attached.on_device = true;
        attached.device_case_id = Some(9);
        update(&dir, "P:\\", &attached).unwrap();
        let edited = save(&dir, "P:\\", SmartCase { id: 1, name: " Renamed ".into(), query: Query { rules: vec![Rule::PlaysMin { n: 3 }], ..Default::default() }, on_device: false, device_case_id: None }).unwrap();
        assert_eq!((edited.name.as_str(), edited.on_device, edited.device_case_id), ("Renamed", true, Some(9)));
        assert_eq!(edited.query.rules, vec![Rule::PlaysMin { n: 3 }]);

        // an unknown id is stored as a new one rather than overwriting; invalid input never writes
        let c = save(&dir, "P:\\", SmartCase { id: 77, ..sc("Three") }).unwrap();
        assert_eq!(c.id, 3);
        assert!(save(&dir, "P:\\", sc("")).is_err());
        assert!(save(&dir, "P:\\", SmartCase { query: Query::default(), ..sc("Empty") }).is_err());
        assert_eq!(load(&dir, "P:\\").unwrap().len(), 3);
        assert!(update(&dir, "P:\\", &SmartCase { id: 99, ..sc("x") }).is_err());

        assert_eq!(remove(&dir, "P:\\", 2).unwrap().unwrap().name, "Two");
        assert!(remove(&dir, "P:\\", 2).unwrap().is_none());
        assert_eq!(load(&dir, "P:\\").unwrap().iter().map(|s| s.id).collect::<Vec<_>>(), vec![1, 3]);
        let d = save(&dir, "P:\\", sc("Four")).unwrap();
        assert_eq!(d.id, 4, "ids come from the highest in use");

        std::fs::write(store_path(&dir, "P:\\"), "not json").unwrap();
        assert!(load(&dir, "P:\\").is_err(), "a corrupt file is reported, not silently emptied");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_version_1_file_loads_migrates_and_is_backed_up_before_its_first_rewrite() {
        use rules::{Mode, Sort, SortField, TextField, TextOp};
        let dir = temp_dir("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let path = store_path(&dir, "P:\\");
        // exactly the shape the first version wrote: no version, a flat rules list, no mode/sort/limit
        let old = r#"{"smart_cases":[
            {"id":1,"name":"Old","rules":[{"kind":"rating_min","stars":4},{"kind":"genre","text":"house"},{"kind":"bpm_range","min":120,"max":null}],"on_device":true,"device_case_id":5},
            {"id":2,"name":"Plain","rules":[{"kind":"plays_zero"}]}]}"#;
        std::fs::write(&path, old).unwrap();

        let list = load(&dir, "P:\\").unwrap();
        assert_eq!(list.len(), 2);
        let first = &list[0];
        assert_eq!((first.query.mode, first.query.sort, first.query.limit), (Mode::All, None, None));
        assert_eq!((first.on_device, first.device_case_id), (true, Some(5)));
        assert_eq!(first.query.rules[0], Rule::RatingMin { stars: 4 });
        assert_eq!(first.query.rules[1], Rule::Text { field: TextField::Genre, op: TextOp::Contains, text: "house".into() }, "genre rule migrated");
        assert_eq!(first.query.rules[2], Rule::BpmRange { min: Some(120.0), max: None });
        assert!(list.iter().all(|s| validate(&s.name, &s.query).is_ok()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old, "reading never rewrites the file");
        assert!(!path.with_extension("json.v1.bak").exists());

        // the first write keeps the old file once, then upgrades the store
        let grouped = Query {
            mode: Mode::Any,
            rules: vec![Rule::Group { mode: Mode::None, items: vec![Rule::Favourite { value: true }] }, Rule::AddedWithin { days: 7 }],
            sort: Some(Sort { field: SortField::DateAdded, descending: true }),
            limit: Some(25),
        };
        let saved = save(&dir, "P:\\", SmartCase { id: 2, query: grouped.clone(), ..sc("Plain") }).unwrap();
        assert_eq!(saved.query, grouped);
        assert_eq!(std::fs::read_to_string(path.with_extension("json.v1.bak")).unwrap(), old);
        let now: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(now["version"], 2);
        assert_eq!(now["smart_cases"][1]["limit"], 25);
        assert_eq!(now["smart_cases"][1]["rules"][0]["kind"], "group");

        // everything survives a reload, the untouched case included; a second write keeps the first backup
        let again = load(&dir, "P:\\").unwrap();
        assert_eq!(again[1].query, grouped);
        assert_eq!(again[0], list[0]);
        save(&dir, "P:\\", SmartCase { id: 2, name: "Again".into(), query: grouped, ..sc("x") }).unwrap();
        assert_eq!(std::fs::read_to_string(path.with_extension("json.v1.bak")).unwrap(), old);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rules_that_would_read_their_own_device_copy_are_noticed() {
        let q = Query { rules: vec![Rule::Group { mode: rules::Mode::Any, items: vec![Rule::InCase { case_id: 12, value: false }] }], ..Default::default() };
        assert!(refers_to_case(&q, 12));
        assert!(!refers_to_case(&q, 13));
        assert!(!refers_to_case(&sc("x").query, 12));
    }

    fn library(tag: &str, n: usize) -> (Library, PathBuf, Vec<i64>) {
        use pm_formats::{library::TrackMeta, strfile};
        let base = std::env::temp_dir();
        let vol = base.join(format!("pmr-smart-{tag}-vol-{}", std::process::id()));
        let src = base.join(format!("pmr-smart-{tag}-src-{}", std::process::id()));
        for d in [&vol, &src] {
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
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![0u8; 3000 + i]).unwrap();
            let meta = TrackMeta { title: format!("t{i}"), format: "MP3".into(), play_time_secs: 60 * (i as i64 + 1), ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        std::fs::remove_dir_all(&src).unwrap();
        (lib, vol, ids)
    }

    #[test]
    fn rules_run_against_a_real_library_including_extra_columns_and_case_membership() {
        use pm_formats::{library::TrackMeta, strfile};
        use rules::{Mode, Sort, SortField, TextField, TextOp};
        let base = std::env::temp_dir();
        let vol = base.join(format!("pmr-smart-real-vol-{}", std::process::id()));
        let src = base.join(format!("pmr-smart-real-src-{}", std::process::id()));
        for d in [&vol, &src] {
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
        for (i, (label, year, secs)) in [("Synthetic Alpha", 1998, 120), ("Synthetic Beta", 2008, 300), ("", 0, 0)].into_iter().enumerate() {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![0u8; 3000 + i]).unwrap();
            let meta = TrackMeta { title: format!("t{i}"), format: "MP3".into(), play_time_secs: secs, label: label.into(), year, remixer: format!("Remixer {i}"), ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        let case = lib.create_case_with_tracks("Synthetic case", &[ids[1], ids[2]]).unwrap();

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let run = |lib: &Library, q: &Query| -> Result<Vec<i64>, String> {
            let data = Data::load(lib, &[q], now).map_err(|e| e.to_string())?;
            evaluate(q, &data).map(|e| e.ids)
        };
        let rules = |rules| Query { rules, ..Default::default() };

        assert_eq!(run(&lib, &rules(vec![Rule::Text { field: TextField::Label, op: TextOp::StartsWith, text: "synthetic a".into() }])).unwrap(), vec![ids[0]]);
        assert_eq!(run(&lib, &rules(vec![Rule::YearRange { min: Some(2000), max: None }])).unwrap(), vec![ids[1]], "year 0 is unknown and never matches");
        assert_eq!(run(&lib, &rules(vec![Rule::Text { field: TextField::Remixer, op: TextOp::Is, text: "remixer 2".into() }])).unwrap(), vec![ids[2]]);
        assert_eq!(run(&lib, &rules(vec![Rule::AddedWithin { days: 1 }])).unwrap().len(), 3, "add_track stamps date_added with now");
        assert!(run(&lib, &rules(vec![Rule::AddedBefore { days: 1 }])).unwrap().is_empty());
        assert_eq!(run(&lib, &rules(vec![Rule::DurationRange { min_secs: Some(1), max_secs: Some(200) }])).unwrap(), vec![ids[0]]);
        assert_eq!(run(&lib, &rules(vec![Rule::InCase { case_id: case, value: true }])).unwrap(), vec![ids[1], ids[2]]);
        assert_eq!(run(&lib, &rules(vec![Rule::InCase { case_id: case, value: false }])).unwrap(), vec![ids[0]]);
        let ordered = Query {
            mode: Mode::Any,
            rules: vec![Rule::InCase { case_id: case, value: true }, Rule::Text { field: TextField::Title, op: TextOp::Is, text: "t0".into() }],
            sort: Some(Sort { field: SortField::Duration, descending: true }),
            limit: Some(2),
        };
        assert_eq!(run(&lib, &ordered).unwrap(), vec![ids[1], ids[0]], "300 s, 120 s; the track with no length is last and cut by the limit");

        // deleting the case turns the rule into an explanation, not into "everything"
        let gone = rules(vec![Rule::InCase { case_id: case, value: false }]);
        lib.delete_case(case).unwrap();
        assert!(run(&lib, &gone).unwrap_err().contains("no longer exists"));
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src).unwrap();
    }

    #[test]
    fn device_copy_is_created_refreshed_renamed_back_and_removed() {
        let (mut lib, vol, ids) = library("device", 3);
        let mut s = SmartCase { id: 1, name: "Warm".into(), ..sc("x") };
        assert_eq!(device_name(&s), "\u{2605} Warm");

        let (case_id, n) = put_on_device(&mut lib, &s, &[ids[2], ids[0]]).unwrap();
        assert_eq!(n, 2);
        s.on_device = true;
        s.device_case_id = Some(case_id);
        let info = lib.case_info(case_id).unwrap().unwrap();
        assert_eq!((info.name.as_str(), info.track_count, info.play_time_secs), ("\u{2605} Warm", 2, 240));
        assert_eq!(lib.case_track_ids(case_id).unwrap(), vec![ids[2], ids[0]]);

        // refresh replaces the membership in place (same case) and restores a changed name
        lib.rename_case(case_id, "\u{2605} Someone renamed it").unwrap();
        let (again, _) = put_on_device(&mut lib, &s, &[ids[1]]).unwrap();
        assert_eq!(again, case_id);
        assert_eq!(lib.case_track_ids(case_id).unwrap(), vec![ids[1]]);
        assert_eq!(lib.case_info(case_id).unwrap().unwrap().name, "\u{2605} Warm");
        assert_eq!(lib.cases().unwrap().len(), 1);

        // an empty result is a valid, empty case
        put_on_device(&mut lib, &s, &[]).unwrap();
        assert_eq!(lib.case_info(case_id).unwrap().unwrap().track_count, 0);

        // renaming the smart case renames the device case on the next refresh
        s.name = "Warm 2".into();
        put_on_device(&mut lib, &s, &[ids[0]]).unwrap();
        assert_eq!(lib.case_info(case_id).unwrap().unwrap().name, "\u{2605} Warm 2");

        // removal deletes it; a refresh after the user deleted it on the device recreates it
        assert!(take_off_device(&mut lib, &s).unwrap());
        assert!(!take_off_device(&mut lib, &s).unwrap());
        assert!(lib.case_info(case_id).unwrap().is_none());
        let (fresh, _) = put_on_device(&mut lib, &s, &[ids[0]]).unwrap();
        assert_ne!(fresh, case_id, "ids are never reused");
        assert!(!take_off_device(&mut lib, &sc("never on device")).unwrap());

        // a case at the remembered id that is not ours (other card, renamed away) is never
        // overwritten or deleted
        let foreign = lib.create_case_with_tracks("Plain case", &[ids[1]]).unwrap();
        let stale = SmartCase { device_case_id: Some(foreign), ..s.clone() };
        assert!(!take_off_device(&mut lib, &stale).unwrap());
        assert_eq!(lib.case_info(foreign).unwrap().unwrap().name, "Plain case");
        let (made, _) = put_on_device(&mut lib, &stale, &[ids[0], ids[2]]).unwrap();
        assert_ne!(made, foreign);
        assert_eq!(lib.case_info(foreign).unwrap().unwrap().name, "Plain case");
        assert_eq!(lib.case_track_ids(foreign).unwrap(), vec![ids[1]]);

        // the name limit counts the star prefix
        assert!(validate(&"x".repeat(MAX_CASE_NAME_CHARS - 2), &sc("x").query).is_ok());
        assert!(validate(&"x".repeat(MAX_CASE_NAME_CHARS - 1), &sc("x").query).is_err());

        // an over-long name is refused before any write
        let long = SmartCase { name: "x".repeat(300), device_case_id: None, ..s.clone() };
        let before = lib.cases().unwrap().len();
        assert!(put_on_device(&mut lib, &long, &[ids[0]]).is_err());
        assert_eq!(lib.cases().unwrap().len(), before);
        assert!(lib.compatibility_problems().unwrap().is_empty());
        drop(lib);
        std::fs::remove_dir_all(vol).unwrap();
    }
}
