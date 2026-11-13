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
//!
//! [`plan_selection`] / [`apply_selection`] (in `selection`) do the same for only what the user
//! picked (some tracks, mixes and cases, optionally tracks into a named case on the target): the
//! picked items plus the tracks they need, found by indexed lookups instead of reading every row,
//! decided by the same helpers under settings that only add (see [`drag_settings`]): it never
//! removes or overwrites anything on the target.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::library::{atomic_copy, atomic_write, CaseMeta, Error, Library, MAX_MIX_NAME_CHARS, MAX_RATING, NO_CUE};
use crate::{appfile, naming};

/// Reason text of every item that needs nothing; planning again right after an apply gives only these.
pub const SAME: &str = "already the same";
/// Why a track or mix is not updated because the target's is the same age or newer.
const LEAVE_TRACK: &str = "both have it and the setting leaves it as it is";
const TARGET_NEWER_TRACK: &str = "the target's copy is the same age or newer";
const TARGET_NEWER_MIX: &str = "the target's mix is newer";
/// Detail of an item that only brings play counts over.
pub const PLAYED_MORE: &str = "played more on the sending side";
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
    /// Bring a higher play count and a later last-played over, even when nothing else differs
    /// (plays counted on a Pacemaker never change a track's details).
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
    /// Only the play counts and last-played time are brought over (the row is otherwise the same,
    /// or the target's is newer); no file is copied.
    UpdatePlays,
    /// The target's row is the same as the source's but its audio file and/or its `.str` file is
    /// missing on the target: only the missing file(s) are copied from the source. The row, its
    /// grid and its plays are not touched. The item's `detail` names what is missing.
    Repair,
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
    /// For a track a selection plan pulled in because a selected mix or case needs it (and that
    /// was not itself selected): those mixes and cases, as `mix "Name"` / `case "Name"`.
    pub needed_by: Vec<String>,
    /// For the item that puts the selected tracks into a case on the target: that case's name.
    pub into_case: Option<String>,
    /// What the item was decided from (the rows' ids and stamps). Only a selection plan's digest
    /// reads it, so a plan goes stale when a selected item moved and not when something else did.
    #[serde(skip)]
    fp: String,
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
    query_tracks(lib, "SELECT * FROM tracks WHERE track_id >= 0 ORDER BY track_id", Vec::new())
}

/// Rows per `IN (...)` query (SQLite's variable limit is far above this).
const IN_CHUNK: usize = 400;

/// The rows with these ids (indexed lookups); an id nobody has is not in the result.
fn rows_by_ids(lib: &Library, ids: &[i64]) -> Result<HashMap<i64, TrackRow>, Error> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(IN_CHUNK) {
        let marks = vec!["?"; chunk.len()].join(",");
        let rows = query_tracks(lib, &format!("SELECT * FROM tracks WHERE track_id >= 0 AND track_id IN ({marks}) ORDER BY track_id"), chunk.iter().map(|i| Value::Integer(*i)).collect())?;
        out.extend(rows.into_iter().map(|r| (r.id, r)));
    }
    Ok(out)
}

/// The rows at these locations (the `tracks_location_idx` index); where a location repeats the
/// lowest track id is kept, as the full plan does.
fn rows_by_locations(lib: &Library, locs: &[String]) -> Result<HashMap<String, TrackRow>, Error> {
    let mut out: HashMap<String, TrackRow> = HashMap::new();
    for chunk in locs.chunks(IN_CHUNK) {
        let marks = vec!["?"; chunk.len()].join(",");
        let rows = query_tracks(lib, &format!("SELECT * FROM tracks WHERE track_id >= 0 AND location IN ({marks}) ORDER BY track_id"), chunk.iter().map(|l| Value::Text(l.clone())).collect())?;
        for r in rows {
            out.entry(r.location.clone()).or_insert(r);
        }
    }
    Ok(out)
}

/// `(location -> lowest track id)` of these locations on `lib`.
fn ids_by_locations(lib: &Library, locs: &[String]) -> Result<HashMap<String, i64>, Error> {
    Ok(rows_by_locations(lib, locs)?.into_iter().map(|(l, r)| (l, r.id)).collect())
}

fn query_tracks(lib: &Library, sql: &str, args: Vec<Value>) -> Result<Vec<TrackRow>, Error> {
    let mut st = lib.con.prepare(sql)?;
    let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
    let rows = st.query_map(params_from_iter(args), |r| {
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
        needed_by: Vec::new(),
        into_case: None,
        fp: String::new(),
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
    ensure_different(source, target)?;
    let s_tracks = read_tracks(source)?;
    let t_tracks = read_tracks(target)?;
    let t_by_loc: HashMap<&str, &TrackRow> = t_tracks.iter().rev().map(|t| (t.location.as_str(), t)).collect();
    let src_by_id: HashMap<i64, &TrackRow> = s_tracks.iter().map(|t| (t.id, t)).collect();
    let mut items: Vec<SyncItem> = Vec::new();

    // tracks
    let mut seen_locs: HashSet<&str> = HashSet::new();
    let mut dups: HashMap<String, usize> = HashMap::new();
    for s in &s_tracks {
        if !seen_locs.insert(s.location.as_str()) {
            let k = dups.entry(format!("track:{}", s.location)).or_insert(1);
            *k += 1;
            let mut item = blank_item(format!("track:{}#{k}", s.location), ItemKind::Track, s.label(), skip("another track at the same place is already in the plan"));
            item.location = Some(s.location.clone());
            item.source_id = Some(s.id);
            items.push(item);
            continue;
        }
        items.push(track_item(source, target, s, t_by_loc.get(s.location.as_str()).copied(), settings));
    }

    // mixes
    let t_mixes = target.mix_infos()?;
    let mut mix_ctx = MixCtx::new(&t_mixes);
    let s_mixes = source.mix_infos()?;
    let row_of = |id: i64| src_by_id.get(&id).map(|r| (r.location.clone(), r.label()));
    let mut seen_mix: HashSet<String> = HashSet::new();
    for m in &s_mixes {
        let n = norm(&m.name);
        if !seen_mix.insert(n.clone()) {
            let k = dups.entry(format!("mix:{n}")).or_insert(1);
            *k += 1;
            let mut item = blank_item(format!("mix:{n}#{k}"), ItemKind::Mix, m.name.clone(), skip("another mix with this name is already in the plan"));
            item.source_id = Some(m.id);
            items.push(item);
            continue;
        }
        items.push(mix_item(m, source.read_mix(m.id), &row_of, &mut mix_ctx, settings, source_label));
    }

    // cases
    let s_cases = source.case_infos()?;
    let t_cases = target.case_infos()?;
    let t_case_by_name = first_by_name(&t_cases);
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
            let k = dups.entry(format!("case:{n}")).or_insert(1);
            *k += 1;
            let mut item = blank_item(format!("case:{n}#{k}"), ItemKind::Case, c.name.clone(), skip("another case with this name is already in the plan"));
            item.source_id = Some(c.id);
            items.push(item);
            continue;
        }
        let mut needs: Vec<Need> = Vec::new();
        for id in source.case_track_ids(c.id)? {
            if let Some(row) = src_by_id.get(&id) {
                if !needs.iter().any(|n| n.location == row.location) {
                    needs.push(Need { location: row.location.clone(), label: row.label() });
                }
            }
        }
        let t_locs = |id: i64| -> Result<Vec<String>, Error> { Ok(target.case_track_ids(id)?.iter().filter_map(|i| t_loc_by_id.get(i).map(|s| s.to_string())).collect()) };
        items.push(case_item(c, needs, &|l| will_exist.contains(l), t_case_by_name.get(&n).copied(), &t_locs, settings)?);
    }
    // removals
    if settings.remove_missing {
        let s_locs: HashSet<&str> = s_tracks.iter().map(|t| t.location.as_str()).collect();
        let s_mix_names: HashSet<String> = s_mixes.iter().map(|m| norm(&m.name)).collect();
        let s_case_names: HashSet<String> = s_cases.iter().map(|c| norm(&c.name)).collect();
        let mut gone: HashSet<String> = HashSet::new();
        let mut removals: Vec<SyncItem> = Vec::new();
        // with "keep both" the earlier "<name> (from computer)" copies are kept, not removed
        let copy_bases: Vec<String> = if settings.mixes_same_name == MixesSameName::KeepBoth {
            s_mixes.iter().map(|m| norm(&copy_name(&m.name, source_label))).collect()
        } else {
            Vec::new()
        };
        for m in &t_mixes {
            let n = norm(&m.name);
            if !s_mix_names.contains(&n) && !copy_bases.iter().any(|b| is_copy_name(&n, b)) {
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

fn ensure_different(source: &Library, target: &Library) -> Result<(), Error> {
    let same_root = match (std::fs::canonicalize(source.root()), std::fs::canonicalize(target.root())) {
        (Ok(a), Ok(b)) => a == b,
        _ => source.root() == target.root(),
    };
    if same_root {
        return Err(Error::Invalid("the source and the target are the same library".into()));
    }
    Ok(())
}

trait Named {
    fn name(&self) -> &str;
}
impl Named for crate::library::MixInfo {
    fn name(&self) -> &str {
        &self.name
    }
}
impl Named for crate::library::CaseInfo {
    fn name(&self) -> &str {
        &self.name
    }
}

/// The first row of each (normalised) name; the list is in name order, so this is the one a
/// by-name lookup means when names repeat.
fn first_by_name<T: Named>(rows: &[T]) -> HashMap<String, &T> {
    let mut out: HashMap<String, &T> = HashMap::new();
    for r in rows.iter().rev() {
        out.insert(norm(r.name()), r);
    }
    out
}

/// What a sync does about one source track, given the target's row at the same place (if any).
/// Shared by the full plan and the selection plan.
fn track_item(source: &Library, target: &Library, s: &TrackRow, t: Option<&TrackRow>, settings: &SyncSettings) -> SyncItem {
    let mut item = blank_item(format!("track:{}", s.location), ItemKind::Track, s.label(), SyncAction::Copy);
    item.global_id = Some(s.global_id.clone()).filter(|g| !g.is_empty());
    item.location = Some(s.location.clone());
    item.source_id = Some(s.id);
    let stamp = |r: &TrackRow| format!("{}/{}/{}/{}/{}", r.id, r.int("date_modified").unwrap_or(0), r.int("times_played").unwrap_or(0), r.int("rating").unwrap_or(0), r.file_size);
    item.fp = format!("{}|{}", stamp(s), t.map(stamp).unwrap_or_default());
    let Some((audio, side)) = paths_of(source, &s.location) else {
        item.action = skip("it is not stored under /pmdb_tracks/");
        return item;
    };
    match t {
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
                if let Some(tp) = paths_of(target, &s.location) {
                    repair_check(&mut item, &audio, &side, &tp.0, &tp.1);
                }
            } else {
                let want_update = match settings.tracks_on_both {
                    TracksOnBoth::Leave => Err(LEAVE_TRACK),
                    TracksOnBoth::Source => Ok(()),
                    TracksOnBoth::Newer => {
                        if s.int("date_modified").unwrap_or(0) > t.int("date_modified").unwrap_or(0) {
                            Ok(())
                        } else {
                            Err(TARGET_NEWER_TRACK)
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
            // plays counted on the sending side come over even when nothing else changes
            if settings.play_counts == PlayCounts::Higher
                && matches!(&item.action, SyncAction::Skip { reason } if reason == SAME || reason == TARGET_NEWER_TRACK || reason.ends_with(LACK_SUFFIX))
                && plays_ahead(s.int("times_played"), s.int("last_played"), t.int("times_played"), t.int("last_played"))
            {
                item.action = SyncAction::UpdatePlays;
                item.detail = PLAYED_MORE.into();
            }
        }
    }
    item
}

const LACK_SUFFIX: &str = "is missing on both sides";
const AUDIO_MISSING: &str = "the audio file is missing on the target";
const SIDE_MISSING: &str = "the beat grid file (.str) is missing on the target";

/// For a track whose row is the same on both sides: when the target lacks the audio and/or the
/// `.str`, plans a [`SyncAction::Repair`] that copies the missing file(s) the source has. When the
/// source lacks a file the target also lacks, that file stays missing and the reason says both
/// sides lack it (a skip when nothing can be copied).
fn repair_check(item: &mut SyncItem, s_audio: &Path, s_side: &Path, t_audio: &Path, t_side: &Path) {
    let mut bytes = 0u64;
    let mut will: Vec<&str> = Vec::new();
    let mut both: Vec<&str> = Vec::new();
    for (src, dest, what) in [(s_audio, t_audio, AUDIO_MISSING), (s_side, t_side, SIDE_MISSING)] {
        if file_len(dest).is_some() {
            continue;
        }
        match file_len(src) {
            Some(n) => {
                bytes += n;
                will.push(what);
            }
            None => both.push(if what == AUDIO_MISSING { "the audio file" } else { "the beat grid file (.str)" }),
        }
    }
    let lack = if both.is_empty() { String::new() } else { format!("{} {LACK_SUFFIX}", both.join(" and ")) };
    if will.is_empty() {
        if !lack.is_empty() {
            item.action = skip(lack);
        }
        return;
    }
    item.action = SyncAction::Repair;
    item.bytes = bytes;
    item.detail = will.join("; ");
    if !lack.is_empty() {
        item.detail.push_str(&format!("; {lack}, so that one cannot be repaired"));
    }
}

/// The target's mixes as the mix planning needs them: by name, and the names a copy would take.
struct MixCtx<'a> {
    all: &'a [crate::library::MixInfo],
    by_name: HashMap<String, &'a crate::library::MixInfo>,
    taken: HashSet<String>,
}

impl<'a> MixCtx<'a> {
    fn new(all: &'a [crate::library::MixInfo]) -> Self {
        MixCtx { all, by_name: first_by_name(all), taken: all.iter().map(|m| norm(&m.name)).collect() }
    }
}

/// What a sync does about one source mix. `doc` is the mix's document as read from the source,
/// `row_of` maps a source track id to its `(location, label)`.
fn mix_item(
    m: &crate::library::MixInfo,
    doc: Result<crate::mix::Mix, Error>,
    row_of: &dyn Fn(i64) -> Option<(String, String)>,
    ctx: &mut MixCtx,
    settings: &SyncSettings,
    source_label: &str,
) -> SyncItem {
    let n = norm(&m.name);
    let mut item = blank_item(format!("mix:{n}"), ItemKind::Mix, m.name.clone(), SyncAction::Copy);
    item.source_id = Some(m.id);
    item.fp = format!("{}/{}/{}/{}/{}/{}", m.id, m.date_created, m.date_modified, m.times_played, m.last_played, m.track_count);
    match doc {
        Err(e) => {
            item.action = skip(format!("its document could not be read ({e})"));
            return item;
        }
        Ok(doc) => {
            let mut seen = HashSet::new();
            let mut bad = None;
            for t in &doc.tracks {
                match row_of(t.library_id) {
                    Some((location, label)) if t.library_id >= 0 => {
                        if seen.insert(location.clone()) {
                            item.needs.push(Need { location, label });
                        }
                    }
                    _ if t.library_id < 0 => bad = Some("it has a track that was removed from the library"),
                    _ => bad = Some("it refers to a track that is not in the library"),
                }
            }
            if let Some(b) = bad {
                item.action = skip(b);
                return item;
            }
        }
    }
    let Some(&t) = ctx.by_name.get(&n) else {
        ctx.taken.insert(n);
        return item;
    };
    item.target_id = Some(t.id);
    // the target mix whose plays a plays-only update would raise: the same-named one, or with
    // "keep both" the earlier copy that was kept
    let mut plays_t = t;
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
                    skip(TARGET_NEWER_MIX)
                }
            }
            MixesSameName::KeepBoth => {
                let base = copy_name(&m.name, source_label);
                let kept = ctx.all.iter().find(|x| is_copy_name(&norm(&x.name), &norm(&base)) && x.date_created == m.date_created && x.date_modified == m.date_modified);
                if let Some(k) = kept {
                    plays_t = k;
                    item.target_id = Some(k.id);
                    skip(SAME)
                } else {
                    let mut name = base.clone();
                    let mut k = 2;
                    while ctx.taken.contains(&norm(&name)) {
                        name = format!("{base} {k}");
                        k += 1;
                    }
                    ctx.taken.insert(norm(&name));
                    SyncAction::CopyAsNew { name }
                }
            }
        }
    };
    item.fp.push_str(&format!("|{}/{}/{}/{}/{}", plays_t.id, plays_t.date_created, plays_t.date_modified, plays_t.times_played, plays_t.last_played));
    if settings.play_counts == PlayCounts::Higher
        && matches!(&item.action, SyncAction::Skip { reason } if reason == SAME || reason == TARGET_NEWER_MIX)
        && plays_ahead(Some(m.times_played), Some(m.last_played), Some(plays_t.times_played), Some(plays_t.last_played))
    {
        item.action = SyncAction::UpdatePlays;
        item.detail = PLAYED_MORE.into();
    }
    item
}

/// What a sync does about one source case. `needs` are its tracks (distinct locations, in order),
/// `will_exist` says whether a location is or will be on the target, `t_locs` gives the locations
/// of the tracks in a target case.
fn case_item(
    c: &crate::library::CaseInfo,
    needs: Vec<Need>,
    will_exist: &dyn Fn(&str) -> bool,
    t_case: Option<&crate::library::CaseInfo>,
    t_locs: &dyn Fn(i64) -> Result<Vec<String>, Error>,
    settings: &SyncSettings,
) -> Result<SyncItem, Error> {
    let n = norm(&c.name);
    let mut item = blank_item(format!("case:{n}"), ItemKind::Case, c.name.clone(), SyncAction::Copy);
    item.source_id = Some(c.id);
    item.fp = format!("{}/{}/{}/{}/{}", c.id, c.date_modified, c.times_played, c.last_played, c.track_count);
    let avail: Vec<&String> = needs.iter().map(|n| &n.location).filter(|l| will_exist(l)).collect();
    if avail.len() < needs.len() {
        item.detail = format!("{} of {} tracks will not be there and are left out", needs.len() - avail.len(), needs.len());
    }
    if let Some(t) = t_case {
        item.target_id = Some(t.id);
        item.fp.push_str(&format!("|{}/{}/{}/{}/{}", t.id, t.date_modified, t.times_played, t.last_played, t.track_count));
        let t_locs: Vec<String> = t_locs(t.id)?;
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
        if settings.play_counts == PlayCounts::Higher
            && matches!(&item.action, SyncAction::Skip { reason } if reason == SAME)
            && plays_ahead(Some(c.times_played), Some(c.last_played), Some(t.times_played), Some(t.last_played))
        {
            item.action = SyncAction::UpdatePlays;
            item.detail = PLAYED_MORE.into();
        }
    }
    item.needs = needs;
    Ok(item)
}

/// Whether the sending side has more plays or a later last-played than the target.
fn plays_ahead(s_times: Option<i64>, s_last: Option<i64>, t_times: Option<i64>, t_last: Option<i64>) -> bool {
    s_times.unwrap_or(0) > t_times.unwrap_or(0) || s_last.unwrap_or(-1) > t_last.unwrap_or(-1)
}

/// Raises a row's play count and last-played to the given values; never lowers them. `table` and
/// `id_col` are fixed names from this file.
fn bump_plays(con: &Connection, table: &str, id_col: &str, id: i64, times: i64, last: i64) -> rusqlite::Result<()> {
    con.execute(
        &format!("UPDATE {table} SET times_played = MAX(IFNULL(times_played,0), ?1), last_played = MAX(IFNULL(last_played,-1), ?2) WHERE {id_col} = ?3"),
        params![times, last, id],
    )?;
    Ok(())
}

/// Whether `name` (normalised) is `base` (normalised, from [`copy_name`]) or `base` with a number
/// after it, the way "keep both" names its copies.
fn is_copy_name(name: &str, base: &str) -> bool {
    name == base || name.strip_prefix(&format!("{base} ")).is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
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
    ensure_fresh(&fresh, plan_seen)?;
    let target_locs: HashSet<String> = read_tracks(target)?.into_iter().map(|t| t.location).collect();
    let s_rows: HashMap<String, TrackRow> = read_tracks(source)?.into_iter().map(|t| (t.location.clone(), t)).collect();
    run_plan(source, target, fresh, excluded, &target_locs, s_rows, progress, cancel)
}

/// [`apply`] for the plan [`plan_selection`] made: plans the same selection again (looking at the
/// selected items only), refuses when it no longer matches what the user saw, then runs it. A
/// change to an unrelated track, mix or case does not make the plan stale; a change to a selected
/// item, or to the target row or case it meets, does.
pub fn apply_selection(
    source: &Library,
    target: &mut Library,
    plan_seen: &SyncPlan,
    selection: &Selection,
    progress: &mut dyn FnMut(&SyncProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<SyncReport, Error> {
    let fresh = plan_selection(source, target, &plan_seen.settings, &plan_seen.source_label, selection, None)?;
    ensure_fresh(&fresh, plan_seen)?;
    let ids: Vec<i64> = fresh.items.iter().filter(|i| i.kind == ItemKind::Track && i.action.acts()).filter_map(|i| i.source_id).collect();
    let s_rows: HashMap<String, TrackRow> = rows_by_ids(source, &ids)?.into_values().map(|t| (t.location.clone(), t)).collect();
    let locs: Vec<String> = fresh.items.iter().flat_map(|i| i.needs.iter().map(|n| n.location.clone()).chain(i.location.clone())).collect();
    let target_locs: HashSet<String> = rows_by_locations(target, &locs)?.into_keys().collect();
    run_plan(source, target, fresh, &[], &target_locs, s_rows, progress, cancel)
}

fn ensure_fresh(fresh: &SyncPlan, seen: &SyncPlan) -> Result<(), Error> {
    if fresh.digest != seen.digest {
        return Err(Error::Invalid(format!("{STALE}: a library changed since the plan was made; plan again")));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_plan(
    source: &Library,
    target: &mut Library,
    fresh: SyncPlan,
    excluded: &[String],
    target_locs: &HashSet<String>,
    s_rows: HashMap<String, TrackRow>,
    progress: &mut dyn FnMut(&SyncProgress),
    cancel: &dyn Fn() -> bool,
) -> Result<SyncReport, Error> {
    let settings = fresh.settings.clone();
    let excluded: HashSet<String> = excluded.iter().cloned().collect();
    let mut items = fresh.items;
    settle(&mut items, target_locs, &excluded);

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
    let mut done_n = 0usize;
    let mut bytes_done = 0u64;
    let mut cancelled = false;
    let mut failed_locs: HashSet<String> = HashSet::new();
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
            (_, SyncAction::UpdatePlays) => update_plays(source, target, it, &s_rows),
            (ItemKind::Track, SyncAction::Repair) => repair_track(source, target, it),
            (ItemKind::Track, _) => match s_rows.get(it.location.as_deref().unwrap_or("")) {
                Some(row) => copy_track(source, target, row, it, &settings),
                None => Err(Error::Invalid("the source track is gone".into())),
            },
            (ItemKind::Mix, SyncAction::Remove) => target.delete_mix(it.target_id.unwrap_or(-1)).map(|_| Outcome::Done),
            (ItemKind::Mix, _) => copy_mix(source, target, it, &settings),
            (ItemKind::Case, SyncAction::Remove) => target.delete_case(it.target_id.unwrap_or(-1)).map(|_| Outcome::Done),
            (ItemKind::Case, _) if it.into_case.is_some() => copy_into_case(target, it),
            (ItemKind::Case, _) => copy_case(source, target, it, &settings),
        };
        results[idx].outcome = match res {
            Ok(Outcome::Done) if it.kind == ItemKind::Case && it.action.acts() && it.action != SyncAction::Remove => {
                // a case is made from the tracks that arrived: say so when a track it needs did not
                let missing: Vec<String> = it.needs.iter().filter(|n| failed_locs.contains(&n.location)).map(|n| n.label.clone()).collect();
                if missing.is_empty() {
                    Outcome::Done
                } else {
                    Outcome::Failed { error: format!("the case was made without {}, which did not arrive on the target", names_list(&missing)) }
                }
            }
            Ok(o) => o,
            Err(e) => Outcome::Failed { error: e.to_string() },
        };
        if it.kind == ItemKind::Track && matches!(results[idx].outcome, Outcome::Failed { .. }) {
            failed_locs.extend(it.location.clone());
        }
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

fn copy_track(source: &Library, target: &mut Library, row: &TrackRow, it: &SyncItem, settings: &SyncSettings) -> Result<Outcome, Error> {
    let (s_audio, s_side) = paths_of(source, &row.location).ok_or_else(|| bad("the track is not stored under /pmdb_tracks/"))?;
    let (t_audio, t_side) = paths_of(target, &row.location).ok_or_else(|| bad("the track is not stored under /pmdb_tracks/"))?;
    if let Some(parent) = t_audio.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if it.action == SyncAction::Update {
        let tid = it.target_id.ok_or_else(|| bad("no target row"))?;
        let previous = std::fs::read(&t_side).ok();
        // copy_checked removes a copy that came out the wrong size, so the old grid goes back on that too
        let res = copy_checked(&s_side, &t_side).and_then(|_| update_row(&target.con, tid, row, settings).map_err(Error::from));
        if let Err(e) = res {
            let _ = match previous {
                Some(old) => atomic_write(&t_side, &old),
                None => std::fs::remove_file(&t_side),
            };
            return Err(e);
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

/// Copies the audio and/or `.str` the target lacks and the source has; nothing else changes (no
/// row, no grid already there, no plays). Re-checks the files now, so a file that appeared since
/// the plan is not overwritten. A failure removes what this item created.
fn repair_track(source: &Library, target: &Library, it: &SyncItem) -> Result<Outcome, Error> {
    let loc = it.location.as_deref().unwrap_or("");
    let (s_audio, s_side) = paths_of(source, loc).ok_or_else(|| bad("the track is not stored under /pmdb_tracks/"))?;
    let (t_audio, t_side) = paths_of(target, loc).ok_or_else(|| bad("the track is not stored under /pmdb_tracks/"))?;
    if let Some(parent) = t_audio.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut made: Vec<&Path> = Vec::new();
    for (src, dest) in [(&s_audio, &t_audio), (&s_side, &t_side)] {
        if file_len(dest).is_some() || file_len(src).is_none() {
            continue;
        }
        if let Err(e) = copy_checked(src, dest) {
            for p in made {
                let _ = std::fs::remove_file(p);
            }
            return Err(e);
        }
        made.push(dest);
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
    let tx = con.unchecked_transaction()?;
    tx.execute(&tx_sql, params_from_iter(vals))?;
    if settings.play_counts == PlayCounts::Higher {
        bump_plays(&tx, "tracks", "track_id", track_id, row.int("times_played").unwrap_or(0), row.int("last_played").unwrap_or(-1))?;
    }
    tx.commit()
}

/// Brings only the play counts of a track, mix or case over; the row is otherwise left alone.
fn update_plays(source: &Library, target: &Library, it: &SyncItem, s_rows: &HashMap<String, TrackRow>) -> Result<Outcome, Error> {
    let tid = it.target_id.ok_or_else(|| bad("no target row"))?;
    match it.kind {
        ItemKind::Track => {
            let row = s_rows.get(it.location.as_deref().unwrap_or("")).ok_or_else(|| bad("the source track is gone"))?;
            bump_plays(&target.con, "tracks", "track_id", tid, row.int("times_played").unwrap_or(0), row.int("last_played").unwrap_or(-1))?;
        }
        ItemKind::Mix => {
            let m = source.mix_info(it.source_id.ok_or_else(|| bad("no source mix"))?)?.ok_or_else(|| bad("the source mix is gone"))?;
            bump_plays(&target.con, "mixes", "mix_id", tid, m.times_played, m.last_played)?;
        }
        ItemKind::Case => {
            let c = source.case_info(it.source_id.ok_or_else(|| bad("no source case"))?)?.ok_or_else(|| bad("the source case is gone"))?;
            bump_plays(&target.con, "cases", "case_id", tid, c.times_played, c.last_played)?;
        }
    }
    Ok(Outcome::Done)
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
    // the shared delete: the trackmap rows go with the track, in one transaction
    crate::ops_fix::delete_track_rows(&mut target.con, &[tid])?;
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
    // only the rows this mix uses, not both libraries
    let used: Vec<i64> = doc.tracks.iter().map(|t| t.library_id).filter(|i| *i >= 0).collect();
    let by_id = rows_by_ids(source, &used)?;
    let locs: Vec<String> = by_id.values().map(|r| r.location.clone()).collect();
    let t_ids = ids_by_locations(target, &locs)?;
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
    if let Err(e) = finish_mix_row(&target.con, tid, &info, image, replacing, settings) {
        if !replacing {
            let _ = target.delete_mix(tid);
        }
        return Err(e);
    }
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
    // one transaction: a failure in the middle never leaves mix_image pointing at a deleted image
    let tx = con.unchecked_transaction()?;
    tx.execute(
        "UPDATE mixes SET genre = ?1, year = ?2, rating = ?3, producer = ?4, remixer = ?5, comments = ?6,
                date_created = ?7, date_modified = ?8, creator_id = ?9, times_played = ?10, last_played = ?11
         WHERE mix_id = ?12",
        params![s.genre, s.year, s.rating, s.producer, s.remixer, s.comments, s.date_created, s.date_modified, s.creator_id, times, last, tid],
    )?;
    if replacing {
        tx.execute(
            "DELETE FROM miximages WHERE id IN (SELECT mix_image FROM mixes WHERE mix_id = ?1 AND mix_image > 0)",
            [tid],
        )?;
        tx.execute("UPDATE mixes SET mix_image = 0 WHERE mix_id = ?1", [tid])?;
    }
    if let Some(blob) = image {
        tx.execute("INSERT INTO miximages (data) VALUES (?1)", [blob])?;
        let img = tx.last_insert_rowid();
        tx.execute("UPDATE mixes SET mix_image = ?1 WHERE mix_id = ?2", params![img, tid])?;
    }
    tx.commit()?;
    Ok(())
}

fn copy_case(source: &Library, target: &mut Library, it: &SyncItem, settings: &SyncSettings) -> Result<Outcome, Error> {
    let sid = it.source_id.ok_or_else(|| bad("no source case"))?;
    let info = source.case_info(sid)?.ok_or_else(|| bad("the source case is gone"))?;
    let s_ids = source.case_track_ids(sid)?;
    let by_id = rows_by_ids(source, &s_ids)?;
    let locs: Vec<String> = by_id.values().map(|r| r.location.clone()).collect();
    let t_ids = ids_by_locations(target, &locs)?;
    let ids: Vec<i64> = s_ids.iter().filter_map(|id| by_id.get(id).and_then(|r| t_ids.get(&r.location)).copied()).collect();
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
            let stamped = target.con.execute(
                "UPDATE cases SET date_created = ?1, date_modified = ?2, creator_id = ?3, times_played = ?4, last_played = ?5 WHERE case_id = ?6",
                params![info.date_created, info.date_modified, crate::library::CASE_CREATOR_ID, info.times_played, info.last_played, id],
            );
            if let Err(e) = stamped {
                let _ = target.delete_case(id);
                return Err(e.into());
            }
            return Ok(Outcome::Done);
        }
        _ => it.target_id.ok_or_else(|| bad("no target case"))?,
    };
    match &it.action {
        SyncAction::Merge => {
            target.add_case_tracks(tid, &ids)?;
            if settings.play_counts == PlayCounts::Higher {
                bump_plays(&target.con, "cases", "case_id", tid, info.times_played, info.last_played)?;
            }
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

/// Puts the tracks of an "into case" item (its `needs`) into the target case: appends them to the
/// existing one (nothing is taken out), or makes the case when the item is a copy.
fn copy_into_case(target: &mut Library, it: &SyncItem) -> Result<Outcome, Error> {
    let name = it.into_case.clone().ok_or_else(|| bad("no case name"))?;
    let locs: Vec<String> = it.needs.iter().map(|n| n.location.clone()).collect();
    let t_ids = ids_by_locations(target, &locs)?;
    let ids: Vec<i64> = locs.iter().filter_map(|l| t_ids.get(l).copied()).collect();
    if ids.is_empty() {
        return Ok(Outcome::Skipped { reason: "none of the tracks is on the target".into() });
    }
    match it.action {
        SyncAction::Copy => {
            if target.case_infos()?.iter().any(|c| norm(&c.name) == norm(&name)) {
                return Err(bad(format!("a case named \"{name}\" appeared on the target meanwhile")));
            }
            target.create_case_with_tracks(&name, &ids)?;
        }
        _ => {
            target.add_case_tracks(it.target_id.ok_or_else(|| bad("no target case"))?, &ids)?;
        }
    }
    Ok(Outcome::Done)
}

mod selection;
pub use selection::{drag_settings, plan_selection, IntoCase, Selection};

#[cfg(test)]
mod tests;
