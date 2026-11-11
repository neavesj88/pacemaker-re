//! Mix library hygiene: copies of a mix (duplicate, save as), bulk delete, bulk `.pmk` export and
//! the library-row facts of every mix in one query.
//!
//! Everything here is plain functions over a [`Library`] (so tests need no Tauri runtime); the
//! `#[tauri::command]` wrappers are in `mixes.rs`. Writes to the device go through the same
//! `Library` calls the single-mix commands use (`create_mix`, `save_mix`, `set_mix_meta`,
//! `delete_mix`); the bulk export only reads the library and writes files the user chose a folder
//! for.
//!
//! What a copy does and does not carry: the document (clips, tools, curves, transitions) and the
//! library row's genre, year, rating, producer, remixer and comments. The play counters
//! (`times_played`, `last_played`) start at zero, and the sleeve image (`mix_image`) is not copied
//! because `pm-formats` does not model it (notes/mix-format.md section 8).

use std::collections::HashSet;
use std::path::Path;

use pm_formats::library::{Library, MAX_MIX_NAME_CHARS};
use pm_formats::mix::Mix;
use serde::{Deserialize, Serialize};

use super::doc::{MixDoc, MixMeta};
use super::{save_in, MixSaved};
use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------------

/// `base` if no mix has that name, else `base (2)`, `base (3)`, ... The result is never longer
/// than the library allows (the base is shortened to make room for the number).
pub fn unique_name(taken: &HashSet<String>, base: &str) -> String {
    let base = base.trim();
    let cut = |s: &str, room: usize| -> String { s.chars().take(room).collect::<String>().trim_end().to_string() };
    let first = cut(base, MAX_MIX_NAME_CHARS);
    if !taken.contains(&first) {
        return first;
    }
    for n in 2.. {
        let suffix = format!(" ({n})");
        let name = format!("{}{suffix}", cut(base, MAX_MIX_NAME_CHARS - suffix.chars().count()));
        if !taken.contains(&name) {
            return name;
        }
    }
    unreachable!("a free name always exists")
}

/// The default name of a copy: `<name> copy`, numbered when that is taken too.
pub fn copy_name(taken: &HashSet<String>, original: &str) -> String {
    unique_name(taken, &format!("{} copy", original.trim()))
}

fn taken_names(lib: &Library) -> AppResult<HashSet<String>> {
    Ok(lib.mix_infos()?.into_iter().map(|m| m.name).collect())
}

// ---------------------------------------------------------------------------------------------
// Duplicate and save as
// ---------------------------------------------------------------------------------------------

/// What a duplicate reports back.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixDuplicated {
    pub id: i64,
    pub name: String,
}

/// Creates a new mix named `name` (or `<original> copy`) holding the SAVED content of mix `id`
/// and its library-row facts. Nothing is left behind when a step fails.
pub fn duplicate_in(lib: &mut Library, id: i64, name: Option<&str>) -> AppResult<MixDuplicated> {
    let info = lib.mix_info(id)?.ok_or_else(|| AppError::from(format!("there is no mix {id}")))?;
    let mut mix: Mix = lib.read_mix(id)?;
    let name = match name {
        Some(n) => n.trim().to_string(),
        None => copy_name(&taken_names(lib)?, &info.name),
    };
    // create_mix refuses an empty, over-long or taken name with its own message
    let new_id = lib.create_mix(&name)?;
    mix.name = name.clone();
    let finish = |lib: &mut Library| -> AppResult<()> {
        lib.save_mix(new_id, &mix)?;
        lib.set_mix_meta(new_id, &MixMeta::from_info(&info).to_update())?;
        Ok(())
    };
    if let Err(e) = finish(lib) {
        let _ = lib.delete_mix(new_id); // do not leave a half-made copy
        return Err(e);
    }
    Ok(MixDuplicated { id: new_id, name })
}

/// Saves the editor's document (unsaved edits included) as a NEW mix called `name`; the mix it
/// came from is not touched. The library-row facts are copied from the source mix's row as it is
/// now (so a rating given in the list after the editor opened it comes along). The same checks as
/// a normal save apply, and nothing is left behind when one refuses.
pub fn save_as_in(lib: &mut Library, doc: &MixDoc, name: &str) -> AppResult<MixSaved> {
    let name = name.trim();
    let source = lib.mix_info(doc.id)?.map(|i| MixMeta::from_info(&i)).or_else(|| doc.meta.clone());
    let new_id = lib.create_mix(name)?;
    let mut copy = doc.clone();
    copy.id = new_id;
    copy.name = name.to_string();
    let result = (|| -> AppResult<MixSaved> {
        let mut saved = save_in(lib, new_id, &copy)?;
        if let Some(meta) = &source {
            lib.set_mix_meta(new_id, &meta.to_update())?;
            saved.date_modified = lib.mix_info(new_id)?.map(|i| i.date_modified).unwrap_or(saved.date_modified);
        }
        Ok(saved)
    })();
    if result.is_err() {
        let _ = lib.delete_mix(new_id);
    }
    result
}

// ---------------------------------------------------------------------------------------------
// Bulk delete
// ---------------------------------------------------------------------------------------------

/// A mix as the user saw it in a confirmation: the id and the name that was on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedMix {
    pub id: i64,
    pub name: String,
}

/// One item a bulk operation could not do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BulkFailure {
    pub id: i64,
    pub name: String,
    pub error: String,
}

/// What a bulk delete did, item by item.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct DeleteReport {
    pub deleted: Vec<NamedMix>,
    pub failed: Vec<BulkFailure>,
}

/// Deletes the listed mixes. An item is deleted only if the mix still has the name the user
/// confirmed (a mix renamed since, or replaced by another under a reused id, is reported and kept).
/// A mix listed twice is deleted once.
pub fn delete_many_in(lib: &mut Library, items: &[NamedMix]) -> AppResult<DeleteReport> {
    let mut report = DeleteReport::default();
    let mut seen = HashSet::new();
    for it in items {
        if !seen.insert(it.id) {
            continue;
        }
        let fail = |error: String| BulkFailure { id: it.id, name: it.name.clone(), error };
        match lib.mix_info(it.id) {
            Ok(None) => report.failed.push(fail("the mix is no longer in the library".into())),
            Ok(Some(info)) if info.name != it.name => {
                report.failed.push(fail(format!("kept: it is now called {:?}, not the name you confirmed", info.name)));
            }
            Ok(Some(_)) => match lib.delete_mix(it.id) {
                Ok(true) => report.deleted.push(it.clone()),
                Ok(false) => report.failed.push(fail("the mix is no longer in the library".into())),
                Err(e) => report.failed.push(fail(e.to_string())),
            },
            Err(e) => report.failed.push(fail(e.to_string())),
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// Bulk .pmk export
// ---------------------------------------------------------------------------------------------

/// Longest file name stem written (characters), so a long mix name cannot make the path too long.
const MAX_STEM: usize = 120;
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A file name stem made from a mix name that every desktop file system accepts: the characters
/// Windows forbids (and control characters) become `_`, trailing dots and spaces go, device names
/// like `CON` get a `_` in front, the length is capped, and an empty result is `mix`.
pub fn safe_file_stem(name: &str) -> String {
    let mapped: String = name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let trim = |s: &str| s.trim().trim_end_matches(['.', ' ']).to_string();
    let mut s = trim(&mapped);
    if s.chars().count() > MAX_STEM {
        s = trim(&s.chars().take(MAX_STEM).collect::<String>());
    }
    let head = s.split('.').next().unwrap_or("").trim_end().to_uppercase();
    if RESERVED.contains(&head.as_str()) {
        s = format!("_{s}");
    }
    if s.is_empty() {
        s = "mix".into();
    }
    s
}

/// True when `file` is a name this module could have produced: `<stem>.pmk` with a stem that
/// `safe_file_stem` leaves unchanged. The export refuses anything else, so a plan sent back from
/// the UI can never reach outside the chosen folder.
pub fn is_safe_pmk_name(file: &str) -> bool {
    match file.strip_suffix(".pmk") {
        Some(stem) => !stem.is_empty() && safe_file_stem(stem) == stem,
        None => false,
    }
}

/// What to do when the file a mix would be written to already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OnExisting {
    /// Write under the next free name (`name (2).pmk`); nothing is ever overwritten.
    Number,
    /// Overwrite the existing file (the plan names it, the user confirmed).
    Replace,
    /// Leave the existing file alone and do not export that mix.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportAction {
    /// A new file.
    Write,
    /// Overwrites a file that exists now.
    Replace,
    /// Not exported (its file exists and the setting says skip).
    Skip,
}

/// One line of the export plan: shown to the user in the confirmation and sent back unchanged to
/// run it, so the preview and the action are the same list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlanItem {
    pub id: i64,
    pub name: String,
    pub file_name: String,
    pub action: ExportAction,
}

/// File names for `names` in a folder. `exists` says whether a file name is already there. Two
/// mixes whose names reduce to the same file name (or differ only in case) never share a file.
pub fn plan_names(names: &[NamedMix], exists: impl Fn(&str) -> bool, on_existing: OnExisting) -> Vec<ExportPlanItem> {
    let mut used: HashSet<String> = HashSet::new(); // lower-cased: Windows and macOS ignore case
    let mut out = Vec::new();
    for m in names {
        let stem = safe_file_stem(&m.name);
        let file = format!("{stem}.pmk");
        let clash_in_batch = used.contains(&file.to_lowercase());
        let on_disk = exists(&file);
        let (file_name, action) = if !clash_in_batch && !on_disk {
            (file, ExportAction::Write)
        } else if !clash_in_batch && on_disk && on_existing == OnExisting::Replace {
            (file, ExportAction::Replace)
        } else if !clash_in_batch && on_disk && on_existing == OnExisting::Skip {
            (file, ExportAction::Skip)
        } else {
            let mut n = 2;
            loop {
                let suffix = format!(" ({n})");
                let base: String = stem.chars().take(MAX_STEM - suffix.chars().count()).collect();
                let cand = format!("{}{suffix}.pmk", base.trim_end());
                if !used.contains(&cand.to_lowercase()) && !exists(&cand) {
                    break (cand, ExportAction::Write);
                }
                n += 1;
            }
        };
        used.insert(file_name.to_lowercase());
        out.push(ExportPlanItem { id: m.id, name: m.name.clone(), file_name, action });
    }
    out
}

/// The plan for exporting `items` into `dir`, using what is in the folder now.
pub fn export_plan_in(items: &[NamedMix], dir: &Path, on_existing: OnExisting) -> AppResult<Vec<ExportPlanItem>> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()).into());
    }
    let mut seen = HashSet::new();
    let unique: Vec<NamedMix> = items.iter().filter(|m| seen.insert(m.id)).cloned().collect();
    Ok(plan_names(&unique, |f| dir.join(f).exists(), on_existing))
}

/// One exported file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Exported {
    pub id: i64,
    pub name: String,
    pub path: String,
}

/// What a bulk export did, item by item.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ExportReport {
    pub written: Vec<Exported>,
    pub skipped: Vec<NamedMix>,
    pub failed: Vec<BulkFailure>,
}

/// Writes the SAVED content of each planned mix to `dir`/`file_name` (temp file then rename, so a
/// failure never leaves half a `.pmk` under a real name). `Replace` overwrites; `Write` refuses
/// to overwrite a file that appeared since the plan was made.
pub fn export_many_in(lib: &Library, plan: &[ExportPlanItem], dir: &Path) -> AppResult<ExportReport> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()).into());
    }
    let mut report = ExportReport::default();
    let mut files = HashSet::new();
    for it in plan {
        let named = NamedMix { id: it.id, name: it.name.clone() };
        let fail = |error: String| BulkFailure { id: it.id, name: it.name.clone(), error };
        if !is_safe_pmk_name(&it.file_name) {
            report.failed.push(fail(format!("{:?} is not a file name this export writes", it.file_name)));
            continue;
        }
        if !files.insert(it.file_name.to_lowercase()) {
            report.failed.push(fail(format!("another mix in this export already goes to {:?}", it.file_name)));
            continue;
        }
        if it.action == ExportAction::Skip {
            report.skipped.push(named);
            continue;
        }
        let dest = dir.join(&it.file_name);
        if it.action == ExportAction::Write && dest.exists() {
            report.failed.push(fail(format!("{:?} appeared in the folder after the list was made; not overwritten", it.file_name)));
            continue;
        }
        let result = (|| -> AppResult<()> {
            let mix = lib.read_mix(it.id)?;
            let bytes = mix.to_pmk().map_err(|e| AppError::from(e.to_string()))?;
            let tmp = dir.join(format!("{}.part", it.file_name));
            let written = std::fs::write(&tmp, &bytes).and_then(|()| std::fs::rename(&tmp, &dest));
            if let Err(e) = written {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
            Ok(())
        })();
        match result {
            Ok(()) => report.written.push(Exported { id: it.id, name: it.name.clone(), path: dest.display().to_string() }),
            Err(e) => report.failed.push(fail(e.to_string())),
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// The list's library-row facts
// ---------------------------------------------------------------------------------------------

/// A mix's id with its library row (rating, genre, dates, ...), for the list's columns, sort and
/// search. One query for the whole library; `mix_open` would parse every document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MixRow {
    pub id: i64,
    pub meta: MixMeta,
}

pub fn list_meta_in(lib: &Library) -> AppResult<Vec<MixRow>> {
    Ok(lib.mix_infos()?.iter().map(|i| MixRow { id: i.id, meta: MixMeta::from_info(i) }).collect())
}

#[cfg(test)]
mod tests {
    use super::super::doc::tests::synthetic_mix;
    use super::super::info::support::{cleanup, library};
    use super::super::open_in;
    use super::*;
    use pm_formats::mix as m;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn names_are_numbered_and_stay_within_the_limit() {
        assert_eq!(unique_name(&set(&[]), "Set"), "Set");
        assert_eq!(unique_name(&set(&["Set"]), "Set"), "Set (2)");
        assert_eq!(unique_name(&set(&["Set", "Set (2)", "Set (3)"]), " Set "), "Set (4)");
        assert_eq!(copy_name(&set(&["Set"]), "Set"), "Set copy");
        assert_eq!(copy_name(&set(&["Set", "Set copy"]), "Set"), "Set copy (2)");
        let long = "x".repeat(MAX_MIX_NAME_CHARS);
        let n = unique_name(&set(&[long.as_str()]), &long);
        assert_eq!(n.chars().count(), MAX_MIX_NAME_CHARS);
        assert!(n.ends_with(" (2)"));
        assert_eq!(copy_name(&set(&[]), &long).chars().count(), MAX_MIX_NAME_CHARS);
    }

    #[test]
    fn file_stems_are_safe_on_every_desktop_file_system() {
        assert_eq!(safe_file_stem("Warm-up: Set 1/2?"), "Warm-up_ Set 1_2_");
        assert_eq!(safe_file_stem("  Friday night...  "), "Friday night");
        assert_eq!(safe_file_stem("CON"), "_CON");
        assert_eq!(safe_file_stem("nul.txt"), "_nul.txt");
        assert_eq!(safe_file_stem("com1"), "_com1");
        assert_eq!(safe_file_stem("Console"), "Console");
        assert_eq!(safe_file_stem(""), "mix");
        assert_eq!(safe_file_stem("..."), "mix");
        assert_eq!(safe_file_stem("a\u{7}b\\c"), "a_b_c");
        assert_eq!(safe_file_stem("ünï cödé ♪"), "ünï cödé ♪");
        let long = safe_file_stem(&"y".repeat(400));
        assert_eq!(long.chars().count(), MAX_STEM);
        // every stem is stable under the same function, which is what validation relies on
        for n in ["a/b", "CON", "x. ", "", "..", "Set (2)"] {
            let s = safe_file_stem(n);
            assert_eq!(safe_file_stem(&s), s, "{n:?}");
        }
        assert!(is_safe_pmk_name("Set (2).pmk"));
        for bad in ["../x.pmk", "a/b.pmk", "a\\b.pmk", ".pmk", "x.txt", "CON.pmk", "x .pmk", "C:evil.pmk", "x"] {
            assert!(!is_safe_pmk_name(bad), "{bad:?}");
        }
    }

    fn nm(id: i64, name: &str) -> NamedMix {
        NamedMix { id, name: name.into() }
    }

    #[test]
    fn plan_never_overwrites_by_default_and_never_shares_a_file() {
        let items = [nm(1, "Set"), nm(2, "SET"), nm(3, "a/b"), nm(4, "a?b"), nm(5, "Other")];
        let disk: HashSet<&str> = ["Other.pmk", "Set (2).pmk"].into();
        // the file systems we write to ignore case, so the stand-in does too
        let exists = |f: &str| disk.iter().any(|d| d.eq_ignore_ascii_case(f));
        let p = plan_names(&items, exists, OnExisting::Number);
        let files: Vec<&str> = p.iter().map(|x| x.file_name.as_str()).collect();
        // SET and Set differ only in case: two files, and "Set (2).pmk" is taken on disk
        assert_eq!(files, ["Set.pmk", "SET (3).pmk", "a_b.pmk", "a_b (2).pmk", "Other (2).pmk"]);
        assert!(p.iter().all(|x| x.action == ExportAction::Write));

        // Replace names the file it will overwrite; a second mix to the same file is still numbered
        let p = plan_names(&[nm(5, "Other"), nm(6, "Other")], exists, OnExisting::Replace);
        assert_eq!((p[0].file_name.as_str(), p[0].action), ("Other.pmk", ExportAction::Replace));
        assert_eq!((p[1].file_name.as_str(), p[1].action), ("Other (2).pmk", ExportAction::Write));
        // (2) was free in this folder for "Other"
        let p = plan_names(&[nm(5, "Other")], exists, OnExisting::Skip);
        assert_eq!((p[0].file_name.as_str(), p[0].action), ("Other.pmk", ExportAction::Skip));
        // every name the plan makes passes the check the export applies
        assert!(plan_names(&items, exists, OnExisting::Number).iter().all(|x| is_safe_pmk_name(&x.file_name)));
        let long = plan_names(&[nm(1, &"z".repeat(300)), nm(2, &"z".repeat(300))], |_| false, OnExisting::Number);
        assert!(long.iter().all(|x| is_safe_pmk_name(&x.file_name)), "{long:?}");
    }

    fn mix_over(ids: &[i64], name: &str) -> Mix {
        let mut mix = synthetic_mix();
        mix.name = name.into();
        mix.tracks.truncate(2);
        mix.transitions.truncate(1);
        mix.tracks[0].library_id = ids[0];
        mix.tracks[1].library_id = ids[1];
        mix.tracks[0].processes.retain(|p| matches!(p, m::Process::Tool(_)));
        mix
    }

    fn meta(genre: &str, rating: i64) -> pm_formats::library::MixMetaUpdate {
        pm_formats::library::MixMetaUpdate { genre: genre.into(), year: 2020, rating, producer: "P".into(), remixer: "R".into(), comments: "note".into() }
    }

    #[test]
    fn duplicate_copies_the_document_and_the_row_and_leaves_the_original_alone() {
        let (mut lib, dirs, ids) = library("dup");
        let id = lib.create_mix("Original").unwrap();
        lib.save_mix(id, &mix_over(&ids, "Original")).unwrap();
        lib.set_mix_meta(id, &meta("House", 4)).unwrap();
        let before = lib.read_mix(id).unwrap();

        let d = duplicate_in(&mut lib, id, None).unwrap();
        assert_eq!(d.name, "Original copy");
        let copy = lib.read_mix(d.id).unwrap();
        assert_eq!(copy.name, "Original copy");
        assert_eq!(copy.tracks, before.tracks);
        assert_eq!(copy.transitions, before.transitions);
        let row = lib.mix_info(d.id).unwrap().unwrap();
        assert_eq!((row.genre.as_str(), row.rating, row.year, row.comments.as_str()), ("House", 4, 2020, "note"));
        assert_eq!(row.times_played, 0);
        assert_eq!(row.track_count, 2);
        assert_eq!(lib.read_mix(id).unwrap(), before, "the original is untouched");
        assert_eq!(lib.mix_info(id).unwrap().unwrap().rating, 4);

        // a second copy is numbered, a chosen name is used as is, a taken chosen name is refused
        assert_eq!(duplicate_in(&mut lib, id, None).unwrap().name, "Original copy (2)");
        assert_eq!(duplicate_in(&mut lib, id, Some("  My name ")).unwrap().name, "My name");
        let n = lib.mix_infos().unwrap().len();
        assert!(duplicate_in(&mut lib, id, Some("My name")).unwrap_err().to_string().contains("already exists"));
        assert!(duplicate_in(&mut lib, id, Some("  ")).is_err());
        assert!(duplicate_in(&mut lib, 4242, None).is_err());
        assert_eq!(lib.mix_infos().unwrap().len(), n, "a refused copy leaves no mix behind");
        assert!(lib.compatibility_problems().unwrap().is_empty());
        cleanup(lib, dirs);
    }

    #[test]
    fn save_as_stores_the_edits_in_a_new_mix_only() {
        let (mut lib, dirs, ids) = library("saveas");
        let id = lib.create_mix("Source").unwrap();
        lib.save_mix(id, &mix_over(&ids, "Source")).unwrap();
        lib.set_mix_meta(id, &meta("Techno", 2)).unwrap();
        let saved_before = lib.read_mix(id).unwrap();

        let mut doc = open_in(&lib, id).unwrap();
        // the list gave the mix a new rating after the editor opened it: the copy takes the row's
        lib.set_mix_meta(id, &meta("Techno", 5)).unwrap();
        doc.clips[1].span.start += 44_100.0; // an unsaved edit
        let s = save_as_in(&mut lib, &doc, "  Source v2 ").unwrap();
        assert_ne!(s.id, id);
        assert_eq!(s.name, "Source v2");
        assert_eq!(s.track_count, 2);
        let new = lib.read_mix(s.id).unwrap();
        assert_eq!(new.name, "Source v2");
        assert_eq!(new.tracks[1].position.start, saved_before.tracks[1].position.start + 44_100.0, "the edit is in the copy");
        assert_eq!(lib.read_mix(id).unwrap(), saved_before, "the source keeps its saved content");
        let row = lib.mix_info(s.id).unwrap().unwrap();
        assert_eq!((row.genre.as_str(), row.rating), ("Techno", 5));
        assert_eq!(s.date_modified, row.date_modified);

        // refusals leave nothing behind: a taken name, a document the save would refuse
        let n = lib.mix_infos().unwrap().len();
        assert!(save_as_in(&mut lib, &doc, "Source").unwrap_err().to_string().contains("already exists"));
        let mut bad = doc.clone();
        bad.clips[0].track_id = 4242;
        assert!(save_as_in(&mut lib, &bad, "Never saved").unwrap_err().to_string().contains("4242"));
        let mut overlap = doc.clone();
        overlap.clips[1].channel = 0;
        overlap.transitions.clear();
        assert!(save_as_in(&mut lib, &overlap, "Never saved 2").is_err());
        assert_eq!(lib.mix_infos().unwrap().len(), n);
        assert!(lib.mix_infos().unwrap().iter().all(|i| !i.name.starts_with("Never")));
        assert!(lib.compatibility_problems().unwrap().is_empty());
        cleanup(lib, dirs);
    }

    /// The Trans tool exactly as the editor's `newTool("Trans", ...)` builds it: class trans, no
    /// automation params (a Trans has none to key), flat span.
    fn trans_tool() -> super::super::doc::ProcessDoc {
        serde_json::from_value(serde_json::json!({
            "kind": "tool", "id": "trans-1", "toolType": "Trans", "name": "ToolProcess", "bypassed": false,
            "effects": [{ "name": "Trans", "bypassed": false, "class": "trans", "startBpm": 128.0, "active": true }],
            "params": {}, "start": 44100.0, "length": 88200.0, "trimStart": 0.0, "trimEnd": 0.0,
            "limitSize": false, "startSec": 1.0, "lengthSec": 2.0
        }))
        .unwrap()
    }

    fn has_trans(mix: &Mix) -> bool {
        mix.tracks.iter().flat_map(|t| &t.processes).any(|p| match p {
            m::Process::Tool(t) => t.effects.iter().any(|e| matches!(e.kind, m::EffectKind::Trans { .. })),
            _ => false,
        })
    }

    #[test]
    fn a_trans_tool_as_the_editor_makes_it_validates_saves_and_reads_back() {
        let (mut lib, dirs, ids) = library("trans");
        let id = lib.create_mix("Gate").unwrap();
        lib.save_mix(id, &mix_over(&ids, "Gate")).unwrap();

        let mut doc = open_in(&lib, id).unwrap();
        // the shared fixture already carries a Trans of its own (with keyed params): drop it, so the
        // only Trans left is the one in the editor's shape
        for c in &mut doc.clips {
            c.processes.retain(|p| !matches!(p, super::super::doc::ProcessDoc::Tool(t) if t.tool_type == "Trans"));
        }
        doc.refresh();
        assert!(!has_trans(&doc.to_mix().unwrap()), "no Trans left before the editor's one is added");
        doc.clips[0].processes.push(trans_tool());
        doc.refresh();
        assert!(doc.validate(None).is_empty(), "{:?}", doc.validate(None));
        assert!(super::super::problems_in(&lib, &doc).unwrap().is_empty());
        assert!(has_trans(&doc.to_mix().unwrap()));

        // a normal save keeps it ...
        save_in(&mut lib, id, &doc).unwrap();
        assert!(has_trans(&lib.read_mix(id).unwrap()));
        let reopened = open_in(&lib, id).unwrap();
        assert!(reopened.clips[0].processes.iter().any(|p| matches!(p, super::super::doc::ProcessDoc::Tool(t) if t.tool_type == "Trans")));
        assert!(reopened.validate(None).is_empty());

        // ... and so does Save as, and a duplicate of the saved mix
        let s = save_as_in(&mut lib, &doc, "Gate v2").unwrap();
        assert!(has_trans(&lib.read_mix(s.id).unwrap()));
        let d = duplicate_in(&mut lib, id, None).unwrap();
        assert!(has_trans(&lib.read_mix(d.id).unwrap()));
        assert!(lib.compatibility_problems().unwrap().is_empty());
        cleanup(lib, dirs);
    }

    #[test]
    fn bulk_delete_names_what_it_deletes_and_keeps_what_changed() {
        let (mut lib, dirs, ids) = library("del");
        let mut made = Vec::new();
        for n in ["One", "Two", "Three", "Four"] {
            let id = lib.create_mix(n).unwrap();
            lib.save_mix(id, &mix_over(&ids, n)).unwrap();
            made.push(nm(id, n));
        }
        lib.rename_mix(made[1].id, "Two renamed").unwrap(); // renamed after the user confirmed
        let asked = vec![made[0].clone(), made[0].clone(), made[1].clone(), made[2].clone(), nm(9999, "Ghost")];
        let r = delete_many_in(&mut lib, &asked).unwrap();
        assert_eq!(r.deleted, vec![made[0].clone(), made[2].clone()], "listed twice = deleted once");
        assert_eq!(r.failed.len(), 2);
        assert!(r.failed[0].error.contains("Two renamed"), "{:?}", r.failed[0]);
        assert_eq!(r.failed[1].id, 9999);
        let left: Vec<String> = lib.mix_infos().unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(left, ["Four", "Two renamed"]);
        assert_eq!(lib.mix_track_rows(made[0].id).unwrap().len(), 0, "mixtracks go with the mix");
        assert!(delete_many_in(&mut lib, &[]).unwrap().deleted.is_empty());
        cleanup(lib, dirs);
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pmrev-hyg-{tag}-{}-{}", std::process::id(), super::super::doc::fresh_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn bulk_export_writes_the_plan_and_only_the_plan() {
        let (mut lib, dirs, ids) = library("exp");
        let mut made = Vec::new();
        for n in ["Set A", "Set/B", "Set?B"] {
            let id = lib.create_mix(n).unwrap();
            lib.save_mix(id, &mix_over(&ids, n)).unwrap();
            made.push(nm(id, n));
        }
        let out = temp_dir("out");
        std::fs::write(out.join("Set A.pmk"), b"precious").unwrap();

        // default: nothing is overwritten
        let plan = export_plan_in(&made, &out, OnExisting::Number).unwrap();
        assert_eq!(plan.iter().map(|p| p.file_name.as_str()).collect::<Vec<_>>(), ["Set A (2).pmk", "Set_B.pmk", "Set_B (2).pmk"]);
        let r = export_many_in(&lib, &plan, &out).unwrap();
        assert!(r.failed.is_empty() && r.skipped.is_empty(), "{r:?}");
        assert_eq!(r.written.len(), 3);
        assert_eq!(std::fs::read(out.join("Set A.pmk")).unwrap(), b"precious");
        // the files are the mixes' .pmk and parse back
        for (it, mix) in plan.iter().zip(&made) {
            let bytes = std::fs::read(out.join(&it.file_name)).unwrap();
            assert_eq!(Mix::from_pmk(&bytes).unwrap(), lib.read_mix(mix.id).unwrap(), "{}", it.file_name);
        }
        assert!(std::fs::read_dir(&out).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));

        // Replace overwrites exactly the file it named
        let plan = export_plan_in(&made[..1], &out, OnExisting::Replace).unwrap();
        assert_eq!((plan[0].file_name.as_str(), plan[0].action), ("Set A.pmk", ExportAction::Replace));
        assert!(export_many_in(&lib, &plan, &out).unwrap().failed.is_empty());
        assert_ne!(std::fs::read(out.join("Set A.pmk")).unwrap(), b"precious");

        // Skip leaves it, and a file that appears after the plan is not overwritten by a Write
        std::fs::write(out.join("Set A.pmk"), b"mine").unwrap();
        let plan = export_plan_in(&made[..1], &out, OnExisting::Skip).unwrap();
        let r = export_many_in(&lib, &plan, &out).unwrap();
        assert_eq!((r.skipped.len(), r.written.len()), (1, 0));
        assert_eq!(std::fs::read(out.join("Set A.pmk")).unwrap(), b"mine");
        let stale = vec![ExportPlanItem { id: made[0].id, name: "Set A".into(), file_name: "Set A.pmk".into(), action: ExportAction::Write }];
        let r = export_many_in(&lib, &stale, &out).unwrap();
        assert_eq!(r.failed.len(), 1);
        assert_eq!(std::fs::read(out.join("Set A.pmk")).unwrap(), b"mine");

        // a forged plan cannot leave the folder, name a file twice, or export a missing mix
        let evil = vec![
            ExportPlanItem { id: made[0].id, name: "x".into(), file_name: "../escape.pmk".into(), action: ExportAction::Replace },
            ExportPlanItem { id: made[0].id, name: "x".into(), file_name: "ok.pmk".into(), action: ExportAction::Write },
            ExportPlanItem { id: made[1].id, name: "x".into(), file_name: "OK.pmk".into(), action: ExportAction::Write },
            ExportPlanItem { id: 9999, name: "ghost".into(), file_name: "ghost.pmk".into(), action: ExportAction::Write },
        ];
        let r = export_many_in(&lib, &evil, &out).unwrap();
        assert_eq!((r.written.len(), r.failed.len()), (1, 3), "{r:?}");
        assert!(!out.parent().unwrap().join("escape.pmk").exists());
        assert!(!out.join("ghost.pmk").exists());
        assert!(export_plan_in(&made, &out.join("nope"), OnExisting::Number).is_err());
        std::fs::remove_dir_all(out).unwrap();
        cleanup(lib, dirs);
    }

    #[test]
    fn list_meta_covers_every_mix_in_one_pass() {
        let (mut lib, dirs, ids) = library("rows");
        let a = lib.create_mix("A").unwrap();
        lib.save_mix(a, &mix_over(&ids, "A")).unwrap();
        lib.set_mix_meta(a, &meta("Disco", 3)).unwrap();
        let b = lib.create_mix("B").unwrap();
        let rows = list_meta_in(&lib).unwrap();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [a, b]);
        assert_eq!((rows[0].meta.genre.as_str(), rows[0].meta.rating), ("Disco", 3));
        assert_eq!(rows[1].meta.rating, 0);
        assert!(rows[0].meta.date_modified > 0);
        cleanup(lib, dirs);
    }
}
