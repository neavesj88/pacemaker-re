//! Library-to-library sync: merges the tracks, mixes and cases of a source library into a target
//! library. One engine for both directions ("Send to Pacemaker" and "Get from Pacemaker"): a
//! library on this computer and one on a device hold the exact same layout (`.Pacemaker/music.db`,
//! `.Pacemaker/Music/A/B/<hash>.<ext>` and a `.str` sidecar per track), so the direction is only
//! which library is the source.
//!
//! [`plan`] reads both libraries and decides, per item, what a sync would do (nothing is
//! written). [`apply`] re-plans, refuses when the plan the user saw no longer matches (the
//! digest), drops the items the user left out, re-checks what the mixes need, then runs tracks,
//! mixes, cases and removals in that order. Every item is atomic on its own (audio and `.str`
//! are written to a temp file and renamed, a row is one transaction), so a cancel between items
//! or a failure in one leaves the target consistent. Nothing is deleted unless
//! [`SyncSettings::remove_missing`] is on.
//!
//! Smart cases are kept by the app on one computer, not in `music.db`; they are never synced
//! (the plan says so in [`SyncPlan::notes`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::library::{atomic_copy, atomic_write, CaseMeta, Error, Library, MAX_MIX_NAME_CHARS, MAX_RATING, NO_CUE};
use crate::{appfile, naming};

/// Reason text of every item that needs nothing; planning again right after an apply gives only these.
pub const SAME: &str = "already the same";
/// Prefix of the error [`apply`] returns when the libraries changed since the plan was made.
pub const STALE: &str = "stale plan";
/// Room kept free on the target beyond the files themselves.
const SPACE_MARGIN: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TracksOnBoth {
    /// Take the source's metadata when its row is newer.
    Newer,
    Leave,
    /// Always take the source's.
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MixesSameName {
    Newer,
    Replace,
    KeepBoth,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CasesSameName {
    /// Keep the target's order, append the source's missing tracks.
    Merge,
    Replace,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayCounts {
    /// On an update keep the higher play count and the later last-played.
    Higher,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncSettings {
    pub tracks_on_both: TracksOnBoth,
    pub mixes_same_name: MixesSameName,
    pub cases_same_name: CasesSameName,
    /// Plan to remove what the target has and the source has not. Off by default.
    pub remove_missing: bool,
    pub play_counts: PlayCounts,
}

impl Default for SyncSettings {
    fn default() -> Self {
        SyncSettings {
            tracks_on_both: TracksOnBoth::Newer,
            mixes_same_name: MixesSameName::Newer,
            cases_same_name: CasesSameName::Merge,
            remove_missing: false,
            play_counts: PlayCounts::Higher,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Track,
    Mix,
    Case,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncAction {
    Copy,
    Update,
    Replace,
    CopyAsNew { name: String },
    Merge,
    Skip { reason: String },
    Remove,
}

impl SyncAction {
    /// Whether the action changes the target.
    pub fn acts(&self) -> bool {
        !matches!(self, SyncAction::Skip { .. })
    }
}

/// A track a mix or case needs on the target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Need {
    pub location: String,
    pub label: String,
}

/// A mix or case on the target that uses a track planned for removal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blocker {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncItem {
    /// Stable across plans of the same libraries: `track:<location>`, `mix:<name>`, `case:<name>`
    /// (names trimmed and lower-cased), `remove-track:..`, `remove-mix:..`, `remove-case:..`.
    pub key: String,
    pub kind: ItemKind,
    pub action: SyncAction,
    /// "Artist - Title", or the mix or case name.
    pub label: String,
    /// Bytes the item copies (audio plus `.str`; an update copies the `.str`).
    pub bytes: u64,
    /// For mixes and cases: the tracks they need on the target.
    pub needs: Vec<Need>,
    /// Extra words for the plan list (for example how many case tracks are left out).
    pub detail: String,
    pub global_id: Option<String>,
    pub location: Option<String>,
    pub source_id: Option<i64>,
    pub target_id: Option<i64>,
    /// For a track removal: the mixes and cases on the target that use it.
    pub blockers: Vec<Blocker>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncPlan {
    pub items: Vec<SyncItem>,
    pub settings: SyncSettings,
    /// "computer" or "Pacemaker": names copies kept with "keep both".
    pub source_label: String,
    pub bytes_to_copy: u64,
    /// Free bytes on the target volume, when they could be read.
    pub free_bytes: Option<u64>,
    pub fits: bool,
    pub digest: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncProgress {
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Skipped { reason: String },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemResult {
    pub key: String,
    pub kind: ItemKind,
    pub label: String,
    pub action: SyncAction,
    pub global_id: Option<String>,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub items: Vec<ItemResult>,
    pub cancelled: bool,
    pub bytes_copied: u64,
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

// ---- reading -------------------------------------------------------------------------------

fn norm(name: &str) -> String {
    name.trim().to_lowercase()
}

/// One `tracks` row with every column, by name.
struct TrackRow {
    id: i64,
    location: String,
    global_id: String,
    file_size: i64,
    cols: BTreeMap<String, Value>,
}

impl TrackRow {
    fn text(&self, col: &str) -> String {
        match self.cols.get(col) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        }
    }

    fn label(&self) -> String {
        let (artist, title) = (self.text("artist"), self.text("title"));
        match (artist.trim().is_empty(), title.trim().is_empty()) {
            (false, false) => format!("{artist} - {title}"),
            (true, false) => title,
            (false, true) => artist,
            _ => self.location.rsplit('/').next().unwrap_or("").to_string(),
        }
    }

    fn int(&self, col: &str) -> Option<i64> {
        match self.cols.get(col) {
            Some(Value::Integer(i)) => Some(*i),
            Some(Value::Real(f)) if f.is_finite() => Some(*f as i64),
            _ => None,
        }
    }
}

fn read_tracks(lib: &Library) -> Result<Vec<TrackRow>, Error> {
    let mut st = lib.con.prepare("SELECT * FROM tracks WHERE track_id >= 0 ORDER BY track_id")?;
    let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
    let rows = st.query_map([], |r| {
        let mut cols = BTreeMap::new();
        for (i, n) in names.iter().enumerate() {
            cols.insert(n.clone(), r.get::<_, Value>(i)?);
        }
        Ok(cols)
    })?;
    let mut out = Vec::new();
    for cols in rows {
        let cols = cols?;
        let id = match cols.get("track_id") {
            Some(Value::Integer(i)) => *i,
            _ => continue,
        };
        let location = match cols.get("location") {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let global_id = match cols.get("global_id") {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let file_size = match cols.get("file_size") {
            Some(Value::Integer(i)) => *i,
            Some(Value::Real(f)) => *f as i64,
            _ => -1,
        };
        out.push(TrackRow { id, location, global_id, file_size, cols });
    }
    Ok(out)
}

/// The columns an update takes from the source row. Not the identity (`track_id`, `location`,
/// `global_id`, `file_size`, `format`, `date_added`) and not the play data, which has its own setting.
const UPDATE_COLS: &[&str] = &[
    "artist", "album_artist", "title", "composer", "album", "track_number", "year", "genre", "is_part_of_c", "comments",
    "label", "rating", "rc_mixes", "bpm", "key", "producer", "remixer", "number_of_tracks", "disc_number",
    "number_of_discs", "cue_point", "loop_in", "loop_out", "track_flags", "structured_ct", "ind_title", "ind_artist",
    "ind_album", "ind_genre", "ind_bpm", "analysis_ver", "analyzed_by_ed", "modified_by_ed", "date_modified",
    "bit_rate", "sample_rate", "play_time_secs",
];

/// A value as the target's conventions want it: an unset cue point is the device's 2^64 (a
/// computer-side Editor library stores -1, FINDINGS 4c), `discid` is empty on a device.
fn conv(col: &str, v: &Value) -> Value {
    match (col, v) {
        ("cue_point", Value::Integer(i)) if *i < 0 => Value::Real(NO_CUE),
        ("cue_point", Value::Real(f)) if *f < 0.0 => Value::Real(NO_CUE),
        ("discid", _) => Value::Text(String::new()),
        _ => v.clone(),
    }
}

fn same_columns(a: &TrackRow, b: &TrackRow) -> bool {
    UPDATE_COLS.iter().all(|c| {
        let x = a.cols.get(*c).map(|v| conv(c, v)).unwrap_or(Value::Null);
        let y = b.cols.get(*c).map(|v| conv(c, v)).unwrap_or(Value::Null);
        x == y
    })
}

/// Rejects a `tracks.location` tail that could leave the Music folder.
fn safe_rel(location: &str) -> Option<&str> {
    let rel = location.strip_prefix(naming::LOCATION_PREFIX)?;
    let ok = !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_))) && !rel.contains('\\');
    ok.then_some(rel)
}

/// `(audio, .str)` of a location on a library, or `None` for a location outside `/pmdb_tracks/`.
fn paths_of(lib: &Library, location: &str) -> Option<(PathBuf, PathBuf)> {
    let rel = safe_rel(location)?;
    let audio = lib.root().join(naming::MUSIC_DIR).join(rel);
    let side = PathBuf::from(naming::str_path(&audio.to_string_lossy()));
    Some((audio, side))
}

fn file_len(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// A cheap fingerprint of the rows a sync reads, so a plan goes stale when either library changes.
fn stamp(lib: &Library) -> Result<String, Error> {
    let q = |sql: &str| -> Result<String, Error> {
        let mut st = lib.con.prepare(sql)?;
        let n = st.column_count();
        Ok(st.query_row([], |r| (0..n).map(|i| Ok(r.get::<_, i64>(i)?.to_string())).collect::<Result<Vec<_>, _>>())?.join(","))
    };
    Ok(format!(
        "t{};m{};c{};mt{};ct{}",
        q("SELECT COUNT(*), IFNULL(MAX(track_id),0), CAST(IFNULL(SUM(date_modified),0) AS INTEGER), CAST(IFNULL(SUM(times_played),0) AS INTEGER), CAST(IFNULL(SUM(rating),0) AS INTEGER) FROM tracks")?,
        q("SELECT COUNT(*), IFNULL(MAX(mix_id),0), CAST(IFNULL(SUM(date_modified),0) AS INTEGER), CAST(IFNULL(SUM(times_played),0) AS INTEGER) FROM mixes")?,
        q("SELECT COUNT(*), IFNULL(MAX(case_id),0), CAST(IFNULL(SUM(date_modified),0) AS INTEGER) FROM cases")?,
        q("SELECT COUNT(*), CAST(IFNULL(SUM(track_id),0) AS INTEGER) FROM mixtracks")?,
        q("SELECT COUNT(*), CAST(IFNULL(SUM(track_id),0) AS INTEGER) FROM casetracks")?,
    ))
}

fn names_list(names: &[String]) -> String {
    let shown: Vec<String> = names.iter().take(3).map(|n| format!("\"{n}\"")).collect();
    if names.len() > 3 {
        format!("{} and {} more", shown.join(", "), names.len() - 3)
    } else {
        shown.join(", ")
    }
}

fn skip(reason: impl Into<String>) -> SyncAction {
    SyncAction::Skip { reason: reason.into() }
}

fn blank_item(key: String, kind: ItemKind, label: String, action: SyncAction) -> SyncItem {
    SyncItem {
        key,
        kind,
        action,
        label,
        bytes: 0,
        needs: Vec::new(),
        detail: String::new(),
        global_id: None,
        location: None,
        source_id: None,
        target_id: None,
        blockers: Vec::new(),
    }
}

// ---- planning ------------------------------------------------------------------------------

/// Plans a sync of `source` into `target`. Reads both; writes nothing. The free space comes from
/// the target's volume.
pub fn plan(source: &Library, target: &Library, settings: &SyncSettings, source_label: &str) -> Result<SyncPlan, Error> {
    let free = crate::ops_device::volume_space(target.root()).ok().map(|(_, free)| free);
    plan_with_free(source, target, settings, source_label, free)
}

/// [`plan`] with the target's free space given (tests give a small one).
pub fn plan_with_free(source: &Library, target: &Library, settings: &SyncSettings, source_label: &str, free: Option<u64>) -> Result<SyncPlan, Error> {
    let same_root = match (std::fs::canonicalize(source.root()), std::fs::canonicalize(target.root())) {
        (Ok(a), Ok(b)) => a == b,
        _ => source.root() == target.root(),
    };
    if same_root {
        return Err(Error::Invalid("the source and the target are the same library".into()));
    }
    let s_tracks = read_tracks(source)?;
    let t_tracks = read_tracks(target)?;
    let t_by_loc: HashMap<&str, &TrackRow> = t_tracks.iter().rev().map(|t| (t.location.as_str(), t)).collect();
    let src_loc_by_id: HashMap<i64, &TrackRow> = s_tracks.iter().map(|t| (t.id, t)).collect();
    let mut items: Vec<SyncItem> = Vec::new();

    // tracks
    let mut seen_locs: HashSet<&str> = HashSet::new();
    for s in &s_tracks {
        if !seen_locs.insert(s.location.as_str()) {
            continue;
        }
        let mut item = blank_item(format!("track:{}", s.location), ItemKind::Track, s.label(), SyncAction::Copy);
        item.global_id = Some(s.global_id.clone()).filter(|g| !g.is_empty());
        item.location = Some(s.location.clone());
        item.source_id = Some(s.id);
        let Some((audio, side)) = paths_of(source, &s.location) else {
            item.action = skip("it is not stored under /pmdb_tracks/");
            items.push(item);
            continue;
        };
        match t_by_loc.get(s.location.as_str()) {
            None => {
                item.action = match (file_len(&audio), file_len(&side)) {
                    (None, _) => skip("its audio file is missing"),
                    (_, None) => skip("its analysis file (.str) is missing"),
                    (Some(a), Some(b)) => {
                        item.bytes = a + b;
                        SyncAction::Copy
                    }
                };
            }
            Some(t) => {
                item.target_id = Some(t.id);
                if s.file_size >= 0 && t.file_size >= 0 && s.file_size != t.file_size {
                    item.action = skip("a different file is stored at the same place");
                } else if same_columns(s, t) {
                    item.action = skip(SAME);
                } else {
                    let want_update = match settings.tracks_on_both {
                        TracksOnBoth::Leave => Err("both have it and the setting leaves it as it is"),
                        TracksOnBoth::Source => Ok(()),
                        TracksOnBoth::Newer => {
                            if s.int("date_modified").unwrap_or(0) > t.int("date_modified").unwrap_or(0) {
                                Ok(())
                            } else {
                                Err("the target's copy is the same age or newer")
                            }
                        }
                    };
                    item.action = match want_update {
                        Err(why) => skip(why),
                        Ok(()) => match file_len(&side) {
                            None => skip("its analysis file (.str) is missing"),
                            Some(b) => {
                                item.bytes = b;
                                SyncAction::Update
                            }
                        },
                    };
                }
            }
        }
        items.push(item);
    }

    // mixes
    let t_mixes = target.mix_infos()?;
    let mut t_mix_by_name: HashMap<String, &crate::library::MixInfo> = HashMap::new();
    for m in t_mixes.iter().rev() {
        t_mix_by_name.insert(norm(&m.name), m);
    }
    let mut taken_names: HashSet<String> = t_mixes.iter().map(|m| norm(&m.name)).collect();
    let s_mixes = source.mix_infos()?;
    let mut seen_mix: HashSet<String> = HashSet::new();
    for m in &s_mixes {
        let n = norm(&m.name);
        if !seen_mix.insert(n.clone()) {
            continue;
        }
        let mut item = blank_item(format!("mix:{n}"), ItemKind::Mix, m.name.clone(), SyncAction::Copy);
        item.source_id = Some(m.id);
        match source.read_mix(m.id) {
            Err(e) => {
                item.action = skip(format!("its document could not be read ({e})"));
                items.push(item);
                continue;
            }
            Ok(doc) => {
                let mut seen = HashSet::new();
                let mut bad = None;
                for t in &doc.tracks {
                    match src_loc_by_id.get(&t.library_id) {
                        Some(row) if t.library_id >= 0 => {
                            if seen.insert(row.location.clone()) {
                                item.needs.push(Need { location: row.location.clone(), label: row.label() });
                            }
                        }
                        _ if t.library_id < 0 => bad = Some("it has a track that was removed from the library"),
                        _ => bad = Some("it refers to a track that is not in the library"),
                    }
                }
                if let Some(b) = bad {
                    item.action = skip(b);
                    items.push(item);
                    continue;
                }
            }
        }
        if let Some(t) = t_mix_by_name.get(&n) {
            item.target_id = Some(t.id);
            let stamps_equal = t.date_created == m.date_created && t.date_modified == m.date_modified;
            item.action = if stamps_equal {
                skip(SAME)
            } else {
                match settings.mixes_same_name {
                    MixesSameName::Skip => skip("a mix with this name is already there and the setting skips it"),
                    MixesSameName::Replace => SyncAction::Replace,
                    MixesSameName::Newer => {
                        if m.date_modified > t.date_modified {
                            SyncAction::Replace
                        } else {
                            skip("the target's mix is newer")
                        }
                    }
                    MixesSameName::KeepBoth => {
                        let base = copy_name(&m.name, source_label);
                        let already = t_mixes.iter().any(|x| {
                            let xn = norm(&x.name);
                            let bn = norm(&base);
                            (xn == bn || xn.strip_prefix(&format!("{bn} ")).is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit())))
                                && x.date_created == m.date_created
                                && x.date_modified == m.date_modified
                        });
                        if already {
                            skip(SAME)
                        } else {
                            let mut name = base.clone();
                            let mut k = 2;
                            while taken_names.contains(&norm(&name)) {
                                name = format!("{base} {k}");
                                k += 1;
                            }
                            taken_names.insert(norm(&name));
                            SyncAction::CopyAsNew { name }
                        }
                    }
                }
            };
        } else {
            taken_names.insert(n);
        }
        items.push(item);
    }

    // cases
    let s_cases = source.case_infos()?;
    let t_cases = target.case_infos()?;
    let mut t_case_by_name: HashMap<String, &crate::library::CaseInfo> = HashMap::new();
    for c in t_cases.iter().rev() {
        t_case_by_name.insert(norm(&c.name), c);
    }
    let t_loc_by_id: HashMap<i64, &str> = t_tracks.iter().map(|t| (t.id, t.location.as_str())).collect();
    let will_exist: HashSet<String> = t_tracks
        .iter()
        .map(|t| t.location.clone())
        .chain(items.iter().filter(|i| i.kind == ItemKind::Track && i.action == SyncAction::Copy).filter_map(|i| i.location.clone()))
        .collect();
    let mut seen_case: HashSet<String> = HashSet::new();
    for c in &s_cases {
        let n = norm(&c.name);
        if !seen_case.insert(n.clone()) {
            continue;
        }
        let mut item = blank_item(format!("case:{n}"), ItemKind::Case, c.name.clone(), SyncAction::Copy);
        item.source_id = Some(c.id);
        let mut locs: Vec<String> = Vec::new();
        for id in source.case_track_ids(c.id)? {
            if let Some(row) = src_loc_by_id.get(&id) {
                if !locs.contains(&row.location) {
                    item.needs.push(Need { location: row.location.clone(), label: row.label() });
                    locs.push(row.location.clone());
                }
            }
        }
        let avail: Vec<&String> = locs.iter().filter(|l| will_exist.contains(*l)).collect();
        if avail.len() < locs.len() {
            item.detail = format!("{} of {} tracks will not be there and are left out", locs.len() - avail.len(), locs.len());
        }
        if let Some(t) = t_case_by_name.get(&n) {
            item.target_id = Some(t.id);
            let t_locs: Vec<String> = target.case_track_ids(t.id)?.iter().filter_map(|id| t_loc_by_id.get(id).map(|s| s.to_string())).collect();
            item.action = match settings.cases_same_name {
                CasesSameName::Skip => skip("a case with this name is already there and the setting skips it"),
                CasesSameName::Merge => {
                    let have: HashSet<&String> = t_locs.iter().collect();
                    if avail.iter().all(|l| have.contains(*l)) {
                        skip(SAME)
                    } else {
                        SyncAction::Merge
                    }
                }
                CasesSameName::Replace => {
                    if avail.len() == t_locs.len() && avail.iter().zip(&t_locs).all(|(a, b)| *a == b) {
                        skip(SAME)
                    } else {
                        SyncAction::Replace
                    }
                }
            };
        }
        items.push(item);
    }

    // removals
    if settings.remove_missing {
        let s_locs: HashSet<&str> = s_tracks.iter().map(|t| t.location.as_str()).collect();
        let s_mix_names: HashSet<String> = s_mixes.iter().map(|m| norm(&m.name)).collect();
        let s_case_names: HashSet<String> = s_cases.iter().map(|c| norm(&c.name)).collect();
        let mut gone: HashSet<String> = HashSet::new();
        let mut removals: Vec<SyncItem> = Vec::new();
        for m in &t_mixes {
            let n = norm(&m.name);
            if !s_mix_names.contains(&n) {
                let key = format!("remove-mix:{n}");
                gone.insert(key.clone());
                let mut it = blank_item(key, ItemKind::Mix, m.name.clone(), SyncAction::Remove);
                it.target_id = Some(m.id);
                removals.push(it);
            }
        }
        for c in &t_cases {
            let n = norm(&c.name);
            if !s_case_names.contains(&n) {
                let key = format!("remove-case:{n}");
                gone.insert(key.clone());
                let mut it = blank_item(key, ItemKind::Case, c.name.clone(), SyncAction::Remove);
                it.target_id = Some(c.id);
                removals.push(it);
            }
        }
        let mut used: HashMap<i64, Vec<Blocker>> = HashMap::new();
        for m in &t_mixes {
            for r in target.mix_track_rows(m.id)? {
                used.entry(r.track_id).or_default().push(Blocker { key: format!("remove-mix:{}", norm(&m.name)), label: format!("mix \"{}\"", m.name) });
            }
        }
        for c in &t_cases {
            for id in target.case_track_ids(c.id)? {
                used.entry(id).or_default().push(Blocker { key: format!("remove-case:{}", norm(&c.name)), label: format!("case \"{}\"", c.name) });
            }
        }
        for t in &t_tracks {
            if s_locs.contains(t.location.as_str()) {
                continue;
            }
            let mut it = blank_item(format!("remove-track:{}", t.location), ItemKind::Track, t.label(), SyncAction::Remove);
            it.location = Some(t.location.clone());
            it.target_id = Some(t.id);
            it.global_id = Some(t.global_id.clone()).filter(|g| !g.is_empty());
            let mut seen = HashSet::new();
            it.blockers = used.remove(&t.id).unwrap_or_default().into_iter().filter(|b| seen.insert(b.key.clone())).collect();
            removals.push(it);
        }
        items.extend(removals);
    }

    let target_locs: HashSet<String> = t_tracks.iter().map(|t| t.location.clone()).collect();
    settle(&mut items, &target_locs, &HashSet::new());
    let bytes: u64 = items.iter().filter(|i| i.kind == ItemKind::Track && i.action.acts()).map(|i| i.bytes).sum();
    let fits = bytes == 0 || free.is_none_or(|f| bytes + SPACE_MARGIN <= f);
    let notes = vec!["Smart cases are kept by the app on this computer and are not synced.".to_string()];
    let digest = digest_of(&items, settings, source_label, &stamp(source)?, &stamp(target)?);
    Ok(SyncPlan { items, settings: settings.clone(), source_label: source_label.to_string(), bytes_to_copy: bytes, free_bytes: free, fits, digest, notes })
}

fn copy_name(name: &str, label: &str) -> String {
    let suffix = format!(" (from {label})");
    let keep = MAX_MIX_NAME_CHARS.saturating_sub(suffix.chars().count() + 4);
    format!("{}{suffix}", name.trim().chars().take(keep).collect::<String>())
}

/// Drops the excluded items, then re-checks what the rest depend on: a mix whose track will not
/// be on the target becomes a skip naming it, a track removal still used by a mix or case that
/// stays becomes a skip naming the user. Idempotent, so [`plan`] and [`apply`] share it.
fn settle(items: &mut [SyncItem], target_locs: &HashSet<String>, excluded: &HashSet<String>) {
    for it in items.iter_mut() {
        if excluded.contains(&it.key) && it.action.acts() {
            it.action = skip("left out by you");
        }
    }
    let copying: HashSet<&str> = items
        .iter()
        .filter(|i| i.kind == ItemKind::Track && i.action == SyncAction::Copy)
        .filter_map(|i| i.location.as_deref())
        .collect();
    let available = |loc: &str| target_locs.contains(loc) || copying.contains(loc);
    let mut missing_for: Vec<(usize, String)> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if it.kind == ItemKind::Mix && matches!(it.action, SyncAction::Copy | SyncAction::Replace | SyncAction::CopyAsNew { .. }) {
            let missing: Vec<String> = it.needs.iter().filter(|n| !available(&n.location)).map(|n| n.label.clone()).collect();
            if !missing.is_empty() {
                missing_for.push((i, format!("it needs {}, which will not be on the target", names_list(&missing))));
            }
        }
    }
    for (i, why) in missing_for {
        items[i].action = skip(why);
    }
    let removing: HashSet<String> = items.iter().filter(|i| i.action == SyncAction::Remove).map(|i| i.key.clone()).collect();
    for it in items.iter_mut() {
        if it.kind == ItemKind::Track && it.action == SyncAction::Remove {
            let still: Vec<String> = it.blockers.iter().filter(|b| !removing.contains(&b.key)).map(|b| b.label.clone()).collect();
            if !still.is_empty() {
                it.action = skip(format!("it is used by {}", still.join(", ")));
            }
        }
    }
}

fn digest_of(items: &[SyncItem], settings: &SyncSettings, label: &str, s_stamp: &str, t_stamp: &str) -> String {
    let mut text = String::new();
    text.push_str(&serde_json::to_string(settings).unwrap_or_default());
    text.push('\n');
    text.push_str(label);
    text.push('\n');
    text.push_str(s_stamp);
    text.push('\n');
    text.push_str(t_stamp);
    for it in items {
        text.push('\n');
        text.push_str(&format!(
            "{}|{}|{}|{}|{:?}|{:?}",
            it.key,
            serde_json::to_string(&it.action).unwrap_or_default(),
            it.bytes,
            it.needs.iter().map(|n| n.location.as_str()).collect::<Vec<_>>().join(","),
            it.source_id,
            it.target_id
        ));
    }
    format!("{:016x}", appfile::fnv1a(&text))
}

// ---- applying ------------------------------------------------------------------------------

/// Runs the plan. `excluded` are item keys the user unticked. `progress` is called before each
/// item that changes something and once at the end; `cancel` is polled before each of them.
pub fn apply(
    source: &Library,
    target: &mut Library,
    plan_seen: &SyncPlan,
    excluded: &[String],
    progress: &mut dyn FnMut(&SyncProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<SyncReport, Error> {
    let fresh = plan(source, target, &plan_seen.settings, &plan_seen.source_label)?;
    if fresh.digest != plan_seen.digest {
        return Err(Error::Invalid(format!("{STALE}: a library changed since the plan was made; plan again")));
    }
    let settings = fresh.settings.clone();
    let excluded: HashSet<String> = excluded.iter().cloned().collect();
    let mut items = fresh.items;
    let target_locs: HashSet<String> = read_tracks(target)?.into_iter().map(|t| t.location).collect();
    settle(&mut items, &target_locs, &excluded);

    let need_bytes: u64 = items.iter().filter(|i| i.kind == ItemKind::Track && i.action.acts()).map(|i| i.bytes).sum();
    if need_bytes > 0 {
        if let Ok((_, free)) = crate::ops_device::volume_space(target.root()) {
            if need_bytes + SPACE_MARGIN > free {
                return Err(Error::Invalid(format!("not enough space on the target: the sync needs {need_bytes} bytes and {free} are free")));
            }
        }
    }

    let total = items.iter().filter(|i| i.action.acts()).count();
    let mut results: Vec<ItemResult> = items
        .iter()
        .map(|i| ItemResult {
            key: i.key.clone(),
            kind: i.kind,
            label: i.label.clone(),
            action: i.action.clone(),
            global_id: i.global_id.clone(),
            outcome: match &i.action {
                SyncAction::Skip { reason } => Outcome::Skipped { reason: reason.clone() },
                _ => Outcome::Skipped { reason: "not run".into() },
            },
        })
        .collect();
    let s_rows: HashMap<String, TrackRow> = read_tracks(source)?.into_iter().map(|t| (t.location.clone(), t)).collect();
    let mut done_n = 0usize;
    let mut bytes_done = 0u64;
    let mut cancelled = false;
    for idx in 0..items.len() {
        if !items[idx].action.acts() {
            continue;
        }
        if cancel() {
            cancelled = true;
            for r in results.iter_mut().skip(idx) {
                if matches!(&r.outcome, Outcome::Skipped { reason } if reason == "not run") {
                    r.outcome = Outcome::Skipped { reason: "cancelled before it ran".into() };
                }
            }
            break;
        }
        progress(&SyncProgress { done: done_n, total, bytes_done, bytes_total: need_bytes, current: items[idx].label.clone() });
        let it = &items[idx];
        let res = match (it.kind, &it.action) {
            (ItemKind::Track, SyncAction::Remove) => remove_track(target, it),
            (ItemKind::Track, _) => match s_rows.get(it.location.as_deref().unwrap_or("")) {
                Some(row) => copy_track(source, target, row, it, &settings),
                None => Err(Error::Invalid("the source track is gone".into())),
            },
            (ItemKind::Mix, SyncAction::Remove) => target.delete_mix(it.target_id.unwrap_or(-1)).map(|_| Outcome::Done),
            (ItemKind::Mix, _) => copy_mix(source, target, it, &settings),
            (ItemKind::Case, SyncAction::Remove) => target.delete_case(it.target_id.unwrap_or(-1)).map(|_| Outcome::Done),
            (ItemKind::Case, _) => copy_case(source, target, it, &settings),
        };
        results[idx].outcome = match res {
            Ok(o) => o,
            Err(e) => Outcome::Failed { error: e.to_string() },
        };
        if results[idx].outcome == Outcome::Done {
            bytes_done += it.bytes;
        }
        done_n += 1;
    }
    progress(&SyncProgress { done: done_n, total, bytes_done, bytes_total: need_bytes, current: String::new() });
    let count = |f: fn(&Outcome) -> bool| results.iter().filter(|r| f(&r.outcome)).count();
    Ok(SyncReport {
        cancelled,
        bytes_copied: bytes_done,
        done: count(|o| matches!(o, Outcome::Done)),
        skipped: count(|o| matches!(o, Outcome::Skipped { .. })),
        failed: count(|o| matches!(o, Outcome::Failed { .. })),
        items: results,
    })
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

fn target_ids(lib: &Library) -> Result<HashMap<String, i64>, Error> {
    Ok(read_tracks(lib)?.into_iter().rev().map(|t| (t.location, t.id)).collect())
}

fn copy_track(source: &Library, target: &mut Library, row: &TrackRow, it: &SyncItem, settings: &SyncSettings) -> Result<Outcome, Error> {
    let (s_audio, s_side) = paths_of(source, &row.location).ok_or_else(|| bad("the track is not stored under /pmdb_tracks/"))?;
    let (t_audio, t_side) = paths_of(target, &row.location).ok_or_else(|| bad("the track is not stored under /pmdb_tracks/"))?;
    if let Some(parent) = t_audio.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if it.action == SyncAction::Update {
        let tid = it.target_id.ok_or_else(|| bad("no target row"))?;
        let previous = std::fs::read(&t_side).ok();
        copy_checked(&s_side, &t_side)?;
        let res = update_row(&target.con, tid, row, settings);
        if let Err(e) = res {
            let _ = match previous {
                Some(old) => atomic_write(&t_side, &old),
                None => std::fs::remove_file(&t_side),
            };
            return Err(e.into());
        }
        return Ok(Outcome::Done);
    }
    let audio_existed = t_audio.exists();
    let side_existed = t_side.exists();
    copy_checked(&s_audio, &t_audio)?;
    if let Err(e) = copy_checked(&s_side, &t_side) {
        if !audio_existed {
            let _ = std::fs::remove_file(&t_audio);
        }
        return Err(e);
    }
    if let Err(e) = insert_row(&target.con, row) {
        if !audio_existed {
            let _ = std::fs::remove_file(&t_audio);
        }
        if !side_existed {
            let _ = std::fs::remove_file(&t_side);
        }
        return Err(e.into());
    }
    Ok(Outcome::Done)
}

/// Atomic copy, then the size on disk is checked against the source.
fn copy_checked(src: &Path, dest: &Path) -> Result<(), Error> {
    let want = std::fs::metadata(src)?.len();
    atomic_copy(src, dest)?;
    let got = std::fs::metadata(dest)?.len();
    if got != want {
        let _ = std::fs::remove_file(dest);
        return Err(bad(format!("{} came out {got} bytes, expected {want}", dest.display())));
    }
    Ok(())
}

fn insert_row(con: &Connection, row: &TrackRow) -> rusqlite::Result<i64> {
    let mut names = Vec::new();
    let mut vals: Vec<Value> = Vec::new();
    for (k, v) in &row.cols {
        if k == "track_id" {
            continue;
        }
        names.push(k.as_str());
        vals.push(conv(k, v));
    }
    let marks: Vec<String> = (1..=names.len()).map(|i| format!("?{i}")).collect();
    con.execute(&format!("INSERT INTO tracks ({}) VALUES ({})", names.join(", "), marks.join(", ")), params_from_iter(vals))?;
    Ok(con.last_insert_rowid())
}

fn update_row(con: &Connection, track_id: i64, row: &TrackRow, settings: &SyncSettings) -> rusqlite::Result<()> {
    let mut sets: Vec<String> = Vec::new();
    let mut vals: Vec<Value> = Vec::new();
    for c in UPDATE_COLS {
        if let Some(v) = row.cols.get(*c) {
            vals.push(conv(c, v));
            sets.push(format!("{c} = ?{}", vals.len()));
        }
    }
    vals.push(Value::Integer(track_id));
    let id_mark = vals.len();
    let tx_sql = format!("UPDATE tracks SET {} WHERE track_id = ?{id_mark}", sets.join(", "));
    con.execute(&tx_sql, params_from_iter(vals))?;
    if settings.play_counts == PlayCounts::Higher {
        let s_times = row.int("times_played").unwrap_or(0);
        let s_last = row.int("last_played").unwrap_or(-1);
        con.execute(
            "UPDATE tracks SET times_played = MAX(IFNULL(times_played,0), ?1), last_played = MAX(IFNULL(last_played,-1), ?2) WHERE track_id = ?3",
            params![s_times, s_last, track_id],
        )?;
    }
    Ok(())
}

fn remove_track(target: &mut Library, it: &SyncItem) -> Result<Outcome, Error> {
    let tid = it.target_id.ok_or_else(|| bad("no target row"))?;
    let used: i64 = target.con.query_row(
        "SELECT (SELECT COUNT(*) FROM mixtracks WHERE track_id = ?1) + (SELECT COUNT(*) FROM casetracks WHERE track_id = ?1)",
        [tid],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Ok(Outcome::Skipped { reason: "it is still used by a mix or case".into() });
    }
    let loc: Option<String> = target.con.query_row("SELECT location FROM tracks WHERE track_id = ?1", [tid], |r| r.get(0)).optional()?;
    let Some(loc) = loc else { return Ok(Outcome::Skipped { reason: "it is already gone".into() }) };
    target.con.execute("DELETE FROM tracks WHERE track_id = ?1 AND track_id >= 0", [tid])?;
    if let Some((audio, side)) = paths_of(target, &loc) {
        let _ = std::fs::remove_file(audio);
        let _ = std::fs::remove_file(side);
    }
    Ok(Outcome::Done)
}

fn copy_mix(source: &Library, target: &mut Library, it: &SyncItem, settings: &SyncSettings) -> Result<Outcome, Error> {
    let sid = it.source_id.ok_or_else(|| bad("no source mix"))?;
    let info = source.mix_info(sid)?.ok_or_else(|| bad("the source mix is gone"))?;
    let mut doc = source.read_mix(sid)?;
    let s_rows = read_tracks(source)?;
    let by_id: HashMap<i64, &TrackRow> = s_rows.iter().map(|t| (t.id, t)).collect();
    let t_ids = target_ids(target)?;
    let mut lost = Vec::new();
    for t in &mut doc.tracks {
        let row = by_id.get(&t.library_id);
        match row.and_then(|r| t_ids.get(&r.location)) {
            Some(id) => t.library_id = *id,
            None => lost.push(row.map(|r| r.label()).unwrap_or_else(|| format!("track {}", t.library_id))),
        }
    }
    if !lost.is_empty() {
        return Ok(Outcome::Skipped { reason: format!("it needs {}, which is not on the target", names_list(&lost)) });
    }
    let image: Option<Vec<u8>> = if info.image_id > 0 {
        source.con.query_row("SELECT data FROM miximages WHERE id = ?1", [info.image_id], |r| r.get(0)).optional()?
    } else {
        None
    };
    let (tid, replacing) = match &it.action {
        SyncAction::Replace => {
            let tid = it.target_id.ok_or_else(|| bad("no target mix"))?;
            let cur = target.mix_info(tid)?.ok_or_else(|| bad("the target mix is gone"))?;
            doc.name = cur.name;
            (tid, true)
        }
        SyncAction::CopyAsNew { name } => {
            doc.name = name.clone();
            (target.create_mix(name)?, false)
        }
        _ => (target.create_mix(&doc.name)?, false),
    };
    if let Err(e) = target.save_mix(tid, &doc) {
        if !replacing {
            let _ = target.delete_mix(tid);
        }
        return Err(e);
    }
    finish_mix_row(&target.con, tid, &info, image, replacing, settings)?;
    Ok(Outcome::Done)
}

fn finish_mix_row(con: &Connection, tid: i64, s: &crate::library::MixInfo, image: Option<Vec<u8>>, replacing: bool, settings: &SyncSettings) -> Result<(), Error> {
    // the source's stamps travel with the mix, so planning again sees the two as the same
    let (times, last) = if replacing && settings.play_counts == PlayCounts::Higher {
        let (t, l): (i64, i64) = con.query_row("SELECT IFNULL(times_played,0), IFNULL(last_played,0) FROM mixes WHERE mix_id = ?1", [tid], |r| Ok((r.get(0)?, r.get(1)?)))?;
        (t.max(s.times_played), l.max(s.last_played))
    } else {
        (s.times_played, s.last_played)
    };
    con.execute(
        "UPDATE mixes SET genre = ?1, year = ?2, rating = ?3, producer = ?4, remixer = ?5, comments = ?6,
                date_created = ?7, date_modified = ?8, creator_id = ?9, times_played = ?10, last_played = ?11
         WHERE mix_id = ?12",
        params![s.genre, s.year, s.rating, s.producer, s.remixer, s.comments, s.date_created, s.date_modified, s.creator_id, times, last, tid],
    )?;
    if replacing {
        con.execute(
            "DELETE FROM miximages WHERE id IN (SELECT mix_image FROM mixes WHERE mix_id = ?1 AND mix_image > 0)",
            [tid],
        )?;
        con.execute("UPDATE mixes SET mix_image = 0 WHERE mix_id = ?1", [tid])?;
    }
    if let Some(blob) = image {
        con.execute("INSERT INTO miximages (data) VALUES (?1)", [blob])?;
        let img = con.last_insert_rowid();
        con.execute("UPDATE mixes SET mix_image = ?1 WHERE mix_id = ?2", params![img, tid])?;
    }
    Ok(())
}

fn copy_case(source: &Library, target: &mut Library, it: &SyncItem, settings: &SyncSettings) -> Result<Outcome, Error> {
    let sid = it.source_id.ok_or_else(|| bad("no source case"))?;
    let info = source.case_info(sid)?.ok_or_else(|| bad("the source case is gone"))?;
    let s_rows = read_tracks(source)?;
    let by_id: HashMap<i64, &str> = s_rows.iter().map(|t| (t.id, t.location.as_str())).collect();
    let t_ids = target_ids(target)?;
    let ids: Vec<i64> = source.case_track_ids(sid)?.iter().filter_map(|id| by_id.get(id).and_then(|l| t_ids.get(*l)).copied()).collect();
    let meta = CaseMeta {
        name: info.name.clone(),
        genre: info.genre.clone(),
        year: info.year.clamp(0, 9999),
        rating: info.rating.clamp(0, i64::from(MAX_RATING)) as u8,
        producer: info.producer.clone(),
        remixer: info.remixer.clone(),
        comments: info.comments.clone(),
    };
    let tid = match &it.action {
        SyncAction::Copy => {
            let id = target.create_case_with_tracks(&info.name, &ids)?;
            if let Err(e) = target.set_case_meta(id, &meta) {
                let _ = target.delete_case(id);
                return Err(e);
            }
            target.con.execute(
                "UPDATE cases SET date_created = ?1, date_modified = ?2, creator_id = ?3, times_played = ?4, last_played = ?5 WHERE case_id = ?6",
                params![info.date_created, info.date_modified, crate::library::CASE_CREATOR_ID, info.times_played, info.last_played, id],
            )?;
            return Ok(Outcome::Done);
        }
        _ => it.target_id.ok_or_else(|| bad("no target case"))?,
    };
    match &it.action {
        SyncAction::Merge => {
            target.add_case_tracks(tid, &ids)?;
        }
        _ => {
            let cur = target.case_info(tid)?.ok_or_else(|| bad("the target case is gone"))?;
            target.set_case_tracks(tid, &ids)?;
            target.set_case_meta(tid, &CaseMeta { name: cur.name, ..meta })?;
            let (times, last) = if settings.play_counts == PlayCounts::Higher {
                (cur.times_played.max(info.times_played), cur.last_played.max(info.last_played))
            } else {
                (cur.times_played, cur.last_played)
            };
            target.con.execute(
                "UPDATE cases SET date_modified = ?1, times_played = ?2, last_played = ?3 WHERE case_id = ?4",
                params![info.date_modified, times, last, tid],
            )?;
        }
    }
    Ok(Outcome::Done)
}

/// Carries the app-side labels (tags, colour, energy) of tracks that were copied onto the target
/// over to the target library's label store. Both stores live in `config_dir`, named by
/// `appfile::volume_key` of each library's root; tracks are keyed by `global_id`, which a copy
/// keeps. Tags are added; a colour or energy the target track already has is kept. Returns how
/// many tracks gained labels. An unreadable target store is left alone and reported.
pub fn copy_labels(config_dir: &Path, source_volume: &str, target_volume: &str, global_ids: &[String]) -> Result<usize, String> {
    use crate::ops_labels::{self as labels, Kind, Op};
    let from = labels::load(config_dir, source_volume);
    let mut to = labels::load(config_dir, target_volume);
    if let Some(why) = to.read_only {
        return Err(why);
    }
    let mut changed = 0;
    for gid in global_ids.iter().filter(|g| !g.trim().is_empty()) {
        let key = format!("g:{gid}");
        let Some(have) = from.store.get(Kind::Track, &key) else { continue };
        let there = to.store.get(Kind::Track, &key).cloned().unwrap_or_default();
        let mut ops = Vec::new();
        if !have.tags.is_empty() {
            ops.push(Op::AddTags { tags: have.tags.clone() });
        }
        if there.colour.is_none() && have.colour.is_some() {
            ops.push(Op::SetColour { colour: have.colour.clone() });
        }
        if there.energy.is_none() && have.energy.is_some() {
            ops.push(Op::SetEnergy { energy: have.energy });
        }
        if ops.is_empty() {
            continue;
        }
        let before = to.store.clone();
        to.store.apply(Kind::Track, &[key], &ops)?;
        if to.store != before {
            changed += 1;
        }
    }
    if changed > 0 {
        labels::save(config_dir, target_volume, &to.store, to.corrupt.as_deref())?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
