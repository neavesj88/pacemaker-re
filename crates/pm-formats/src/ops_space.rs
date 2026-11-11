//! Free-space planner (owner: device area): what to archive or remove from the card to reach a
//! target, an archive copy to a PC folder, and the removal of the chosen tracks.
//!
//! Three parts, all driven by the same functions the preview and the action share:
//!
//! * [`plan`] reads the library and the files (read-only) and lists the tracks that match the
//!   user's [`Rules`], each with the reasons in words, its size on the card and what keeps it from
//!   being removed (a mix uses it). The order is the order the planner would take them in, and
//!   `suggested` marks the prefix that reaches the target.
//! * [`plan_run`] says, for a set of ticked tracks, exactly what [`run`] will do with each one:
//!   the archive file name, whether the track is removed or stays and why. The confirmation shows
//!   it; [`run`] repeats it on the fresh database before it acts.
//! * [`run`] archives (a verified copy: written to a `.part` file, read back and compared with the
//!   checksum of the source, then renamed) and/or removes. A track is removed only if it is not in
//!   a mix and, when an archive was asked for, only after its audio file was archived and
//!   verified. The library rows go first (one transaction, after a `music.db` snapshot made by the
//!   caller), then the files; a failure in between leaves an unused file the Health card lists,
//!   never a row that points at nothing.
//!
//! Every path is built from `tracks.location` through [`checked_track_paths`], which refuses
//! anything that is not plain names under `.Pacemaker/Music`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::library::{atomic_write, Error, Library, Track};
use crate::ops_device::{copy_hashed, inside, sha256_file, utc_stamp, volume_space, Storage, MANIFEST_NAME};
use crate::ops_fix::{checked_track_paths, delete_track_rows, membership, track_label};

pub const DAY_MS: i64 = 86_400_000;
/// An average month (30.4375 days) in milliseconds, for "not played for N months".
pub const MONTH_MS: i64 = 2_629_800_000;
pub const GIB: u64 = 1 << 30;
/// Name of the listing written into every archive folder.
pub const ARCHIVE_LIST_NAME: &str = "ARCHIVE.csv";
/// Prefix of an archive folder (`pacemaker-archive-<UTC stamp>`).
pub const ARCHIVE_PREFIX: &str = "pacemaker-archive-";
/// Room kept free on the archive drive beyond the files themselves (listing, manifest, file system).
const ARCHIVE_MARGIN: u64 = 16 << 20;

fn invalid<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(msg.into()))
}

// ---- rules and goal -------------------------------------------------------------------------

/// How the list is ordered, and so which tracks the target is reached with.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// The clearest cases first: most reasons, then the longest unplayed, then the lowest rated.
    #[default]
    Certain,
    /// The biggest files first: the fewest tracks to reach the target.
    Largest,
}

/// The user's thresholds. Every field is a setting; the defaults are cautious.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Rules {
    /// Reason: the track was never played.
    pub never_played: bool,
    /// Reason: it was played, but not in the last N months. 0 turns the reason off.
    pub stale_months: u32,
    /// Reason: it is rated 1..=N stars. 0 turns the reason off.
    pub low_rating_max: u8,
    /// An unrated track counts as low rated.
    pub unrated_is_low: bool,
    /// Reason: another track on the card holds the same audio (byte for byte); the one that is
    /// used most is kept.
    pub duplicates: bool,
    /// How many of the reasons above a track needs (clamped to the reasons that are on).
    pub min_reasons: u8,
    /// Only tracks that are in no case.
    pub only_no_case: bool,
    /// Only tracks that are in no mix.
    pub only_no_mix: bool,
    /// Never suggest a track in the Favourites case.
    pub protect_favourites: bool,
    /// Never suggest a track rated at least this many stars. 0 turns the protection off.
    pub protect_rating_min: u8,
    /// Never suggest a track added in the last N days. 0 turns the protection off.
    pub protect_new_days: u32,
    pub order: Order,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            never_played: true,
            stale_months: 6,
            low_rating_max: 2,
            unrated_is_low: false,
            duplicates: true,
            min_reasons: 1,
            only_no_case: true,
            only_no_mix: true,
            protect_favourites: true,
            protect_rating_min: 4,
            protect_new_days: 30,
            order: Order::Certain,
        }
    }
}

impl Rules {
    /// Puts every value in its allowed range.
    pub fn clamped(mut self) -> Self {
        self.stale_months = self.stale_months.min(240);
        self.low_rating_max = self.low_rating_max.min(crate::library::MAX_RATING);
        self.protect_rating_min = self.protect_rating_min.min(crate::library::MAX_RATING);
        self.protect_new_days = self.protect_new_days.min(3650);
        self.min_reasons = self.min_reasons.clamp(1, 4);
        self
    }

    /// How many different reasons can fire for one track. "Never played" and "not played for N
    /// months" exclude each other (the second needs a play), so together they count once.
    fn reason_slots(&self) -> usize {
        usize::from(self.never_played || self.stale_months > 0) + usize::from(self.low_rating_max > 0) + usize::from(self.duplicates)
    }
}

/// What to reach.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Goal {
    /// Have at least this much free space on the card.
    Free { bytes: u64 },
    /// Make the library fit a card of this size, keeping `reserve_percent` of it free.
    Card { bytes: u64, reserve_percent: u32 },
}

/// How many bytes must go to reach `goal` on a volume that looks like `storage`.
pub fn bytes_to_free(goal: &Goal, storage: &Storage) -> Result<u64, Error> {
    match *goal {
        Goal::Free { bytes } => {
            if storage.total_bytes == 0 {
                return invalid("the free space of this drive could not be read, so a free-space target cannot be planned; plan for a card size instead");
            }
            Ok(bytes.saturating_sub(storage.free_bytes))
        }
        Goal::Card { bytes, reserve_percent } => Ok((storage.music_bytes + storage.grids_mixes_bytes).saturating_sub(usable_card_bytes(bytes, reserve_percent))),
    }
}

/// The part of a card of `card_bytes` the library may use.
pub fn usable_card_bytes(card_bytes: u64, reserve_percent: u32) -> u64 {
    let reserve = card_bytes / 100 * u64::from(reserve_percent.min(90));
    card_bytes.saturating_sub(reserve)
}

// ---- the plan -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonKind {
    NeverPlayed,
    NotPlayedRecently,
    LowRating,
    Duplicate,
    NoCase,
    NoMix,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Reason {
    pub kind: ReasonKind,
    pub text: String,
}

/// One track the rules nominate.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub track_id: i64,
    /// "Artist - Title", or whichever exists, or the file name.
    pub label: String,
    pub format: String,
    /// What removing it frees: the audio file and its `.str`, as sized on the card now.
    pub bytes: u64,
    pub play_time_secs: i64,
    pub rating: i64,
    pub times_played: i64,
    /// Unix milliseconds of the last play, none when never played.
    pub last_played_ms: Option<i64>,
    pub added_ms: Option<i64>,
    pub in_cases: Vec<String>,
    pub in_mixes: Vec<String>,
    pub reasons: Vec<Reason>,
    /// The track whose audio this one repeats, and its name.
    pub duplicate_of: Option<i64>,
    pub duplicate_of_label: Option<String>,
    /// Why the track cannot be removed (it can still be archived).
    pub blocked: Option<String>,
    /// In the prefix of the list that reaches the target.
    pub suggested: bool,
}

/// Tracks that matched the rules but are held back by a protection setting.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Hidden {
    pub favourites: usize,
    pub high_rated: usize,
    pub new_tracks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// False when no reason and no condition is on: nothing can be nominated.
    pub rules_on: bool,
    pub total_tracks: usize,
    /// Bytes to free to reach the goal (0: already there).
    pub need_bytes: u64,
    pub free_bytes: u64,
    pub total_bytes: u64,
    /// Music plus grids and the database: what a smaller card would have to hold.
    pub library_bytes: u64,
    /// Card goal: the part of the card the library may fill.
    pub usable_bytes: Option<u64>,
    pub items: Vec<Item>,
    pub candidate_bytes: u64,
    /// Candidates that can be removed (a mix does not use them).
    pub removable_bytes: u64,
    pub suggested_count: usize,
    pub suggested_bytes: u64,
    /// The removable candidates are enough to reach the goal.
    pub reachable: bool,
    pub hidden: Hidden,
    /// Tracks whose audio file is not on the card (the Health card deals with them).
    pub missing_audio: usize,
    /// Tracks whose stored location is not a plain path under the Music folder.
    pub unchecked: usize,
    /// No track in the library has a play count or a play date. The Pacemaker may not record
    /// plays at all (the notes show 0 / -1 on every track of a real card), so "never played" and
    /// "not played for N months" say nothing in that case.
    pub no_play_history: bool,
    pub warnings: Vec<String>,
}

struct Row<'a> {
    t: &'a Track,
    audio_len: u64,
    grid_len: u64,
    added_ms: i64,
}

/// `date_added` as Unix milliseconds. The Editor writes milliseconds (notes/FINDINGS.md); whether
/// the device writes the same unit is unverified, so a value too small to be a millisecond date
/// (before 1973) is read as seconds instead of making the "new track" protection miss it.
fn added_to_ms(v: i64) -> i64 {
    if v > 0 && v < 100_000_000_000 { v * 1000 } else { v }
}

fn plural(n: i64, one: &str) -> String {
    if n == 1 { format!("{n} {one}") } else { format!("{n} {one}s") }
}

type HashKey = (PathBuf, u64, Option<SystemTime>);

/// Checksums already computed in this run of the app, keyed by path, size and modification time,
/// so changing a setting does not read every candidate file from the card again. A file that is
/// replaced changes its size or time, so it is read again.
static HASHES: OnceLock<Mutex<HashMap<HashKey, String>>> = OnceLock::new();
const HASH_CACHE_MAX: usize = 100_000;

fn hash_cached(path: &Path, len: u64) -> Result<String, Error> {
    let key: HashKey = (path.to_path_buf(), len, fs::metadata(path).and_then(|m| m.modified()).ok());
    let cache = HASHES.get_or_init(Mutex::default);
    if let Some(h) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok(h.clone());
    }
    let sum = sha256_file(path)?;
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() >= HASH_CACHE_MAX {
        map.clear();
    }
    map.insert(key, sum.clone());
    Ok(sum)
}

/// Groups of rows (indices) whose audio files are byte-identical. Files are compared by size
/// first and hashed only when another file has the same size. `cancel` is polled before each file
/// is read; a stop abandons the scan (the hashes already computed stay cached).
fn duplicate_groups(rows: &[Row], root: &Path, warnings: &mut Vec<String>, cancel: &dyn Fn() -> bool) -> Result<Vec<Vec<usize>>, Error> {
    let mut by_len: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        if r.audio_len > 0 {
            by_len.entry(r.audio_len).or_default().push(i);
        }
    }
    let mut groups = Vec::new();
    let mut unread = 0usize;
    for same_len in by_len.values().filter(|v| v.len() > 1) {
        let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in same_len {
            if cancel() {
                return Err(Error::Invalid(SCAN_STOPPED.into()));
            }
            let Some((audio, _)) = checked_track_paths(root, &rows[i].t.location) else { continue };
            match hash_cached(&audio, rows[i].audio_len) {
                Ok(h) => by_hash.entry(h).or_default().push(i),
                Err(_) => unread += 1,
            }
        }
        groups.extend(by_hash.into_values().filter(|g| g.len() > 1));
    }
    if unread > 0 {
        warnings.push(format!("{unread} audio file{} could not be read, so {} not compared for duplicates.", if unread == 1 { "" } else { "s" }, if unread == 1 { "it was" } else { "they were" }));
    }
    groups.iter_mut().for_each(|g| g.sort_unstable());
    groups.sort();
    Ok(groups)
}

/// The message of a scan that was stopped because the caller no longer wants its result.
pub const SCAN_STOPPED: &str = "the scan was stopped";

/// Lists the tracks the rules nominate, in the order the planner would take them. Read-only.
/// `storage` and `now_ms` are parameters so the same plan can be reproduced exactly in a test.
pub fn plan(lib: &Library, rules: &Rules, goal: &Goal, storage: &Storage, now_ms: i64) -> Result<Plan, Error> {
    plan_cancellable(lib, rules, goal, storage, now_ms, &|| false)
}

/// [`plan`] that polls `cancel` while it reads files for the duplicate check (the slow part on a
/// big card); a stop returns an error whose text is [`SCAN_STOPPED`].
pub fn plan_cancellable(lib: &Library, rules: &Rules, goal: &Goal, storage: &Storage, now_ms: i64, cancel: &dyn Fn() -> bool) -> Result<Plan, Error> {
    let rules = rules.clone().clamped();
    let need_bytes = bytes_to_free(goal, storage)?;
    let root = lib.root();
    let tracks = lib.tracks()?;
    let cases = membership(lib, "SELECT t.track_id, c.name FROM casetracks t JOIN cases c ON c.case_id = t.case_id WHERE c.case_id >= 0 ORDER BY c.name")?;
    let mixes = membership(lib, "SELECT t.track_id, m.name FROM mixtracks t JOIN mixes m ON m.mix_id = t.mix_id WHERE m.mix_id >= 0 ORDER BY m.name")?;
    let favourites: HashSet<i64> = lib.favourite_track_ids()?.into_iter().collect();
    let mut added: HashMap<i64, i64> = HashMap::new();
    {
        let mut st = lib.con.prepare("SELECT track_id, IFNULL(date_added, 0) FROM tracks WHERE track_id >= 0")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?;
        for r in rows {
            let (id, ms) = r?;
            added.insert(id, added_to_ms(ms as i64));
        }
    }

    let mut warnings = Vec::new();
    let no_play_history = !tracks.is_empty() && tracks.iter().all(|t| t.times_played <= 0 && t.last_played <= 0);
    if no_play_history && (rules.never_played || rules.stale_months > 0) {
        warnings.push(
            "This library has no play history (the Pacemaker may not record it), so \"Never played\" matches every track and \"Not played for N months\" can never match. Turn \"Never played\" off, or ask for two reasons, so the list is not just everything outside a case and a mix."
                .into(),
        );
    }
    let mut missing_audio = 0usize;
    let mut unchecked = 0usize;
    let mut rows: Vec<Row> = Vec::new();
    for t in &tracks {
        let Some((audio, sidecar)) = checked_track_paths(root, &t.location) else {
            unchecked += 1;
            continue;
        };
        let Some(audio_len) = fs::metadata(&audio).ok().filter(|m| m.is_file()).map(|m| m.len()) else {
            missing_audio += 1;
            continue;
        };
        let grid_len = fs::metadata(&sidecar).ok().filter(|m| m.is_file()).map_or(0, |m| m.len());
        rows.push(Row { t, audio_len, grid_len, added_ms: added.get(&t.track_id).copied().unwrap_or(0) });
    }
    if missing_audio > 0 && missing_audio * 2 > tracks.len() {
        warnings.push("More than half of the tracks have no audio file. That usually means the card is not fully mounted; nothing below is trustworthy until it is.".into());
    }

    // duplicates: which row repeats which, with the most used one of a group kept
    let mut dup_of: HashMap<usize, usize> = HashMap::new();
    if rules.duplicates {
        for g in duplicate_groups(&rows, root, &mut warnings, cancel)? {
            let keep_key = |&i: &usize| {
                let t = rows[i].t;
                (
                    mixes.contains_key(&t.track_id),
                    cases.get(&t.track_id).map_or(0, Vec::len),
                    t.rating,
                    t.times_played,
                    t.last_played,
                    -t.track_id,
                )
            };
            let keeper = *g.iter().max_by_key(|i| keep_key(i)).unwrap_or(&g[0]);
            for &i in g.iter().filter(|&&i| i != keeper) {
                dup_of.insert(i, keeper);
            }
        }
    }

    let reasons_on = rules.never_played || rules.stale_months > 0 || rules.low_rating_max > 0 || rules.duplicates;
    let rules_on = reasons_on || rules.only_no_case || rules.only_no_mix;
    let min_reasons = usize::from(rules.min_reasons).min(rules.reason_slots().max(1));
    let mut hidden = Hidden::default();
    let mut items: Vec<Item> = Vec::new();
    if rules_on {
        for (idx, row) in rows.iter().enumerate() {
            let t = row.t;
            let in_cases = cases.get(&t.track_id).cloned().unwrap_or_default();
            let in_mixes = mixes.get(&t.track_id).cloned().unwrap_or_default();
            if (rules.only_no_case && !in_cases.is_empty()) || (rules.only_no_mix && !in_mixes.is_empty()) {
                continue;
            }
            let mut reasons: Vec<Reason> = Vec::new();
            // last_played is -1 on a device row and 0 on a case row: neither is a date
            let never = t.times_played <= 0 && t.last_played <= 0;
            if rules.never_played && never {
                let text = if no_play_history { "Never played (no play history on this card)" } else { "Never played" };
                reasons.push(Reason { kind: ReasonKind::NeverPlayed, text: text.into() });
            }
            if rules.stale_months > 0 && t.last_played > 0 && now_ms - t.last_played > i64::from(rules.stale_months) * MONTH_MS {
                let months = (now_ms - t.last_played) / MONTH_MS;
                reasons.push(Reason { kind: ReasonKind::NotPlayedRecently, text: format!("Not played for {}", plural(months, "month")) });
            }
            if rules.low_rating_max > 0 {
                if (1..=i64::from(rules.low_rating_max)).contains(&t.rating) {
                    reasons.push(Reason { kind: ReasonKind::LowRating, text: format!("Rated {}", plural(t.rating, "star")) });
                } else if rules.unrated_is_low && t.rating == 0 {
                    reasons.push(Reason { kind: ReasonKind::LowRating, text: "Unrated".into() });
                }
            }
            let mut duplicate_of = None;
            let mut duplicate_of_label = None;
            if let Some(&k) = dup_of.get(&idx) {
                let kt = rows[k].t;
                let label = track_label(&kt.artist, &kt.title, &kt.location);
                reasons.push(Reason { kind: ReasonKind::Duplicate, text: format!("Same audio as {label}") });
                duplicate_of = Some(kt.track_id);
                duplicate_of_label = Some(label);
            }
            if reasons_on && reasons.len() < min_reasons {
                continue;
            }
            // the conditions are shown as reasons too, so the row says everything that holds
            if rules.only_no_case {
                reasons.push(Reason { kind: ReasonKind::NoCase, text: "In no case".into() });
            }
            if rules.only_no_mix {
                reasons.push(Reason { kind: ReasonKind::NoMix, text: "In no mix".into() });
            }
            if rules.protect_favourites && favourites.contains(&t.track_id) {
                hidden.favourites += 1;
                continue;
            }
            if rules.protect_rating_min > 0 && t.rating >= i64::from(rules.protect_rating_min) {
                hidden.high_rated += 1;
                continue;
            }
            if rules.protect_new_days > 0 && row.added_ms > 0 && now_ms - row.added_ms < i64::from(rules.protect_new_days) * DAY_MS {
                hidden.new_tracks += 1;
                continue;
            }
            items.push(Item {
                track_id: t.track_id,
                label: track_label(&t.artist, &t.title, &t.location),
                format: t.format.clone(),
                bytes: row.audio_len + row.grid_len,
                play_time_secs: t.play_time_secs,
                rating: t.rating,
                times_played: t.times_played,
                last_played_ms: (t.last_played > 0).then_some(t.last_played),
                added_ms: (row.added_ms > 0).then_some(row.added_ms),
                blocked: (!in_mixes.is_empty()).then(|| format!("Used by the mix {}: it can be archived but not removed", in_mixes.join(", "))),
                in_cases,
                in_mixes,
                reasons,
                duplicate_of,
                duplicate_of_label,
                suggested: false,
            });
        }
    }

    let real_reasons = |i: &Item| i.reasons.iter().filter(|r| !matches!(r.kind, ReasonKind::NoCase | ReasonKind::NoMix)).count();
    match rules.order {
        Order::Certain => items.sort_by(|a, b| {
            (a.blocked.is_some(), std::cmp::Reverse(real_reasons(a)), a.last_played_ms, a.rating, std::cmp::Reverse(a.bytes), a.track_id)
                .cmp(&(b.blocked.is_some(), std::cmp::Reverse(real_reasons(b)), b.last_played_ms, b.rating, std::cmp::Reverse(b.bytes), b.track_id))
        }),
        Order::Largest => items.sort_by(|a, b| {
            (a.blocked.is_some(), std::cmp::Reverse(a.bytes), std::cmp::Reverse(real_reasons(a)), a.track_id)
                .cmp(&(b.blocked.is_some(), std::cmp::Reverse(b.bytes), std::cmp::Reverse(real_reasons(b)), b.track_id))
        }),
    }

    // the suggested prefix: removable items in order until the need is covered. A track and the
    // copy that repeats it are never both suggested: one of them must stay.
    let mut suggested_bytes = 0u64;
    let mut suggested_count = 0usize;
    let mut suggested_ids: HashSet<i64> = HashSet::new();
    let mut keepers_with_a_suggested_copy: HashSet<i64> = HashSet::new();
    for it in items.iter_mut().filter(|i| i.blocked.is_none()) {
        if suggested_bytes >= need_bytes {
            break;
        }
        if it.duplicate_of.is_some_and(|k| suggested_ids.contains(&k)) || keepers_with_a_suggested_copy.contains(&it.track_id) {
            continue;
        }
        it.suggested = true;
        suggested_bytes += it.bytes;
        suggested_count += 1;
        suggested_ids.insert(it.track_id);
        if let Some(k) = it.duplicate_of {
            keepers_with_a_suggested_copy.insert(k);
        }
    }
    let candidate_bytes = items.iter().map(|i| i.bytes).sum();
    let removable_bytes: u64 = items.iter().filter(|i| i.blocked.is_none()).map(|i| i.bytes).sum();
    Ok(Plan {
        rules_on,
        total_tracks: tracks.len(),
        need_bytes,
        free_bytes: storage.free_bytes,
        total_bytes: storage.total_bytes,
        library_bytes: storage.music_bytes + storage.grids_mixes_bytes,
        usable_bytes: match *goal {
            Goal::Card { bytes, reserve_percent } => Some(usable_card_bytes(bytes, reserve_percent)),
            Goal::Free { .. } => None,
        },
        items,
        candidate_bytes,
        removable_bytes,
        suggested_count,
        suggested_bytes,
        reachable: removable_bytes >= need_bytes,
        hidden,
        missing_audio,
        unchecked,
        no_play_history,
        warnings,
    })
}

// ---- archive names --------------------------------------------------------------------------

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A file name stem that is valid on Windows, macOS and Linux: no path or control characters,
/// no trailing dots or spaces, not a reserved device name, at most 120 characters.
pub fn safe_stem(label: &str) -> String {
    let mut s: String = label
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    s = s.trim().trim_end_matches(['.', ' ']).chars().take(120).collect();
    s = s.trim_end_matches(['.', ' ']).to_string();
    let head = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if s.is_empty() {
        return "track".into();
    }
    if RESERVED.contains(&head.as_str()) {
        s.insert(0, '_');
    }
    s
}

/// The extension (without the dot) of a stored location, kept to letters and digits.
fn ext_of(location: &str) -> String {
    let name = location.rsplit('/').next().unwrap_or("");
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].chars().filter(char::is_ascii_alphanumeric).take(5).collect::<String>().to_ascii_lowercase(),
        _ => String::new(),
    }
}

// ---- what a run will do ---------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Where to write the archive folder (a PC folder, never on the card). None: no archive.
    pub archive_dir: Option<PathBuf>,
    /// Archive the `.str` beat grid beside each audio file.
    pub include_grids: bool,
    /// Remove the tracks from the card.
    pub remove: bool,
}

/// What a run does with one ticked track.
#[derive(Debug, Clone, Serialize)]
pub struct RunItem {
    pub track_id: i64,
    pub label: String,
    pub bytes: u64,
    /// The name of the archive file, when an archive is made.
    pub archive_name: Option<String>,
    /// The track and its files are removed from the card.
    pub remove: bool,
    /// Why it is not archived or not removed, in words.
    pub note: Option<String>,
    #[serde(skip)]
    audio: PathBuf,
    #[serde(skip)]
    sidecar: PathBuf,
    #[serde(skip)]
    track: Option<Track>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunPlan {
    pub items: Vec<RunItem>,
    /// Ticked tracks that cannot be acted on at all, with the reason.
    pub skipped: Vec<String>,
    /// Bytes the archive will hold (audio, and grids when asked).
    pub archive_bytes: u64,
    /// Bytes removing frees.
    pub remove_bytes: u64,
    /// Free bytes on the archive drive, when an archive is made and the drive could be read.
    pub archive_free_bytes: Option<u64>,
    /// Reasons the run cannot start (empty: it can).
    pub problems: Vec<String>,
}

impl RunPlan {
    pub fn archived(&self) -> usize {
        self.items.iter().filter(|i| i.archive_name.is_some()).count()
    }
    pub fn removed(&self) -> usize {
        self.items.iter().filter(|i| i.remove).count()
    }
}

/// Checks the archive folder: it exists, is a folder and is not on the card.
fn archive_dir_problem(root: &Path, dir: &Path) -> Option<String> {
    if !dir.is_dir() {
        return Some(format!("{} is not a folder", dir.display()));
    }
    if inside(root, dir) {
        return Some("choose a folder on another drive: an archive on the card itself is not an archive".into());
    }
    None
}

/// What [`run`] will do with the ticked tracks. Read-only. The confirmation shows this; `run`
/// computes it again on the fresh database and acts on that.
pub fn plan_run(lib: &Library, ids: &[i64], opts: &RunOptions) -> Result<RunPlan, Error> {
    let root = lib.root();
    let tracks: HashMap<i64, Track> = lib.tracks()?.into_iter().map(|t| (t.track_id, t)).collect();
    let mixes = membership(lib, "SELECT t.track_id, m.name FROM mixtracks t JOIN mixes m ON m.mix_id = t.mix_id WHERE m.mix_id >= 0 ORDER BY m.name")?;
    let mut plan = RunPlan { items: Vec::new(), skipped: Vec::new(), archive_bytes: 0, remove_bytes: 0, archive_free_bytes: None, problems: Vec::new() };
    if !opts.remove && opts.archive_dir.is_none() {
        plan.problems.push("nothing to do: choose an archive folder, remove, or both".into());
    }
    if ids.is_empty() {
        plan.problems.push("no tracks are ticked".into());
    }
    if let Some(dir) = &opts.archive_dir {
        if let Some(p) = archive_dir_problem(root, dir) {
            plan.problems.push(p);
        }
    }
    let mut seen: HashSet<i64> = HashSet::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(t) = tracks.get(&id) else {
            plan.skipped.push(format!("track {id}: not in the library any more"));
            continue;
        };
        let label = track_label(&t.artist, &t.title, &t.location);
        let Some((audio, sidecar)) = checked_track_paths(root, &t.location) else {
            plan.skipped.push(format!("{label}: its stored location is not a plain path under the Music folder, so no file was touched"));
            continue;
        };
        let Some(audio_len) = fs::metadata(&audio).ok().filter(|m| m.is_file()).map(|m| m.len()) else {
            plan.skipped.push(format!("{label}: its audio file is not on the card"));
            continue;
        };
        let grid_len = fs::metadata(&sidecar).ok().filter(|m| m.is_file()).map_or(0, |m| m.len());
        let mut note = None;
        let mut remove = opts.remove;
        if let Some(m) = mixes.get(&id).filter(|m| !m.is_empty()) {
            if remove {
                remove = false;
                note = Some(format!("stays on the card: the mix {} uses it", m.join(", ")));
            }
        }
        let archive_name = opts.archive_dir.as_ref().map(|_| {
            let ext = ext_of(&t.location);
            let stem = safe_stem(&label);
            let mut name = if ext.is_empty() { stem.clone() } else { format!("{stem}.{ext}") };
            // the grid is written as "<stem>.str", so two formats of one song ("A.mp3", "A.wav")
            // share a grid name even though their audio names differ: both names are reserved
            let grid_of = |stem: &str| format!("{stem}.str").to_lowercase();
            let clash = used_names.contains(&name.to_lowercase()) || (opts.include_grids && used_names.contains(&grid_of(&stem)));
            let mut used_stem = stem.clone();
            if clash {
                used_stem = format!("{stem} ({id})");
                name = if ext.is_empty() { used_stem.clone() } else { format!("{used_stem}.{ext}") };
            }
            used_names.insert(name.to_lowercase());
            if opts.include_grids {
                used_names.insert(grid_of(&used_stem));
            }
            name
        });
        let archived_bytes = if archive_name.is_some() { audio_len + if opts.include_grids { grid_len } else { 0 } } else { 0 };
        plan.archive_bytes += archived_bytes;
        if remove {
            plan.remove_bytes += audio_len + grid_len;
        }
        plan.items.push(RunItem { track_id: id, label, bytes: audio_len + grid_len, archive_name, remove, note, audio, sidecar, track: Some(t.clone()) });
    }
    if let Some(dir) = &opts.archive_dir {
        if dir.is_dir() {
            plan.archive_free_bytes = volume_space(dir).ok().map(|(_, free)| free).filter(|&f| f > 0);
            if let Some(free) = plan.archive_free_bytes {
                if free < plan.archive_bytes + ARCHIVE_MARGIN {
                    plan.problems.push(format!("the archive drive has {} free and the archive needs {}", size_text(free), size_text(plan.archive_bytes + ARCHIVE_MARGIN)));
                }
            }
        }
    }
    if plan.items.is_empty() && !ids.is_empty() {
        plan.problems.push("none of the ticked tracks can be acted on".into());
    }
    Ok(plan)
}

/// "12.3 GB", "4.5 MB": binary units, the same wording the storage card uses.
pub fn size_text(n: u64) -> String {
    if n >= GIB {
        format!("{:.1} GB", n as f64 / GIB as f64)
    } else if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else if n >= 1 << 10 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

// ---- archive --------------------------------------------------------------------------------

/// One archived file: its checksum, for the manifest and the listing.
#[derive(Debug, Clone)]
struct Archived {
    track_id: i64,
    name: String,
    sum: String,
    bytes: u64,
    grid: Option<String>,
}

/// Copies the audio (and grid) of each item into `folder` and verifies the copy. Returns the
/// items that are safely archived and a message for each one that is not. `cancel` is polled
/// before each file; stopping leaves the verified files in place.
fn archive_items(
    items: &[&RunItem],
    folder: &Path,
    include_grids: bool,
    done_offset: usize,
    total: usize,
    progress: &mut dyn FnMut(usize, usize, &str),
    cancel: &dyn Fn() -> bool,
) -> (Vec<Archived>, Vec<String>, bool) {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    let mut cancelled = false;
    let mut grid_names: HashSet<String> = HashSet::new();
    for (i, it) in items.iter().enumerate() {
        if cancel() {
            cancelled = true;
            break;
        }
        progress(done_offset + i, total, &format!("Archiving {}", it.label));
        let name = it.archive_name.clone().unwrap_or_else(|| safe_stem(&it.label));
        let dest = folder.join(&name);
        let part = folder.join(format!("{name}.part"));
        let copied = copy_hashed(&it.audio, &part).and_then(|r| {
            fs::rename(&part, &dest)?;
            Ok(r)
        });
        let (sum, bytes) = match copied {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&part);
                failed.push(format!("{}: not archived ({e})", it.label));
                continue;
            }
        };
        let mut grid = None;
        if include_grids && it.sidecar.is_file() {
            let stem = name.rfind('.').map_or(name.as_str(), |p| &name[..p]);
            let gname = format!("{stem}.str");
            // plan_run keeps grid names apart; this is the backstop, so a grid already written
            // in this run is never replaced (a rename would silently overwrite it)
            if !grid_names.insert(gname.to_lowercase()) {
                failed.push(format!("{}: the audio was archived but its beat grid was not (another grid is already archived as {gname})", it.label));
                ok.push(Archived { track_id: it.track_id, name, sum, bytes, grid: None });
                continue;
            }
            let gpart = folder.join(format!("{gname}.part"));
            let r = copy_hashed(&it.sidecar, &gpart).and_then(|r| {
                fs::rename(&gpart, folder.join(&gname))?;
                Ok(r)
            });
            match r {
                Ok((gsum, _)) => grid = Some(format!("{gsum}  {gname}")),
                Err(e) => {
                    let _ = fs::remove_file(&gpart);
                    failed.push(format!("{}: the audio was archived but its beat grid was not ({e})", it.label));
                }
            }
        }
        ok.push(Archived { track_id: it.track_id, name, sum, bytes, grid });
    }
    (ok, failed, cancelled)
}

fn csv_cell(s: &str, text: bool) -> String {
    // a text cell that starts like a formula would run as one when the file is opened in a spreadsheet
    let guarded = if text && s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

fn iso(ms: i64) -> String {
    if ms < 0 {
        return String::new();
    }
    let s = utc_stamp((ms / 1000) as u64);
    // "YYYYMMDDTHHMMSSZ" -> "YYYY-MM-DDTHH:MM:SSZ"
    format!("{}-{}-{}T{}:{}:{}Z", &s[0..4], &s[4..6], &s[6..8], &s[9..11], &s[11..13], &s[13..15])
}

/// Writes `ARCHIVE.csv` and `MANIFEST.sha256` (sha256sum format, so the Backup card's "check a
/// backup" verifies an archive folder too) for what was archived.
fn write_listing(folder: &Path, archived: &[Archived], by_id: &HashMap<i64, &RunItem>) -> Result<(), Error> {
    let mut csv = String::from("file,artist,title,album,genre,bpm,key,rating,times_played,last_played_utc,original_location,bytes,sha256\r\n");
    for a in archived {
        let Some(t) = by_id.get(&a.track_id).and_then(|i| i.track.as_ref()) else { continue };
        let row = [
            csv_cell(&a.name, true),
            csv_cell(&t.artist, true),
            csv_cell(&t.title, true),
            csv_cell(&t.album, true),
            csv_cell(&t.genre, true),
            csv_cell(&format!("{:.2}", t.bpm), false),
            csv_cell(&t.key, true),
            csv_cell(&t.rating.to_string(), false),
            csv_cell(&t.times_played.to_string(), false),
            csv_cell(&iso(t.last_played), false),
            csv_cell(&t.location, true),
            csv_cell(&a.bytes.to_string(), false),
            csv_cell(&a.sum, false),
        ];
        csv.push_str(&row.join(","));
        csv.push_str("\r\n");
    }
    atomic_write(&folder.join(ARCHIVE_LIST_NAME), csv.as_bytes())?;
    let mut manifest = String::new();
    for a in archived {
        manifest.push_str(&format!("{}  {}\n", a.sum, a.name));
        if let Some(g) = &a.grid {
            manifest.push_str(g);
            manifest.push('\n');
        }
    }
    manifest.push_str(&format!("{}  {ARCHIVE_LIST_NAME}\n", sha256_file(&folder.join(ARCHIVE_LIST_NAME))?));
    atomic_write(&folder.join(MANIFEST_NAME), manifest.as_bytes())?;
    Ok(())
}

// ---- run ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunReport {
    pub archive_folder: Option<String>,
    pub archived: usize,
    pub archived_bytes: u64,
    pub removed: usize,
    pub freed_bytes: u64,
    /// Tracks left alone, with the reason (a mix uses them, the archive failed, ...).
    pub skipped: Vec<String>,
    /// Things that went wrong after the work had started (a file that could not be deleted).
    pub failed: Vec<String>,
    /// The user stopped it; nothing was removed.
    pub cancelled: bool,
}

/// Archives and/or removes the ticked tracks. `snapshot` is called once, just before the library
/// is opened for writing, and must make the safety copy of `music.db`; if it fails nothing is
/// removed. `progress(done, total, label)` and `cancel()` as in the other device jobs.
pub fn run(
    root: &Path,
    ids: &[i64],
    opts: &RunOptions,
    snapshot: &dyn Fn() -> Result<(), Error>,
    progress: &mut dyn FnMut(usize, usize, &str),
    cancel: &dyn Fn() -> bool,
) -> Result<RunReport, Error> {
    let plan = {
        let lib = Library::open_read_only(root)?;
        plan_run(&lib, ids, opts)?
    };
    if !plan.problems.is_empty() {
        return invalid(plan.problems.join("; "));
    }
    let mut report = RunReport { skipped: plan.skipped.clone(), ..RunReport::default() };
    let to_archive: Vec<&RunItem> = plan.items.iter().filter(|i| i.archive_name.is_some()).collect();
    let total = to_archive.len() + plan.removed();
    let by_id: HashMap<i64, &RunItem> = plan.items.iter().map(|i| (i.track_id, i)).collect();

    // ---- archive: a verified copy of every ticked track, before anything is removed
    let mut archived_ids: HashSet<i64> = HashSet::new();
    if let (Some(dir), false) = (&opts.archive_dir, to_archive.is_empty()) {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let folder = dir.join(format!("{ARCHIVE_PREFIX}{}", utc_stamp(secs)));
        if folder.exists() {
            return invalid(format!("cannot create {}: it already exists", folder.display()));
        }
        fs::create_dir(&folder).map_err(|e| Error::Invalid(format!("cannot create {}: {e}", folder.display())))?;
        let (ok, failed, cancelled) = archive_items(&to_archive, &folder, opts.include_grids, 0, total, progress, cancel);
        report.failed.extend(failed);
        if ok.is_empty() {
            // an empty folder helps nobody
            let _ = fs::remove_dir(&folder);
        } else {
            write_listing(&folder, &ok, &by_id)?;
            report.archive_folder = Some(folder.display().to_string());
            report.archived = ok.len();
            report.archived_bytes = ok.iter().map(|a| a.bytes).sum();
            archived_ids = ok.iter().map(|a| a.track_id).collect();
        }
        if cancelled {
            report.cancelled = true;
            report.skipped.push("stopped at your request: nothing was removed".into());
            return Ok(report);
        }
    }

    // ---- remove: only what is archived (when an archive was asked for) and not in a mix
    let archiving = opts.archive_dir.is_some();
    let mut removable: Vec<i64> = Vec::new();
    for it in plan.items.iter().filter(|i| i.remove) {
        if archiving && !archived_ids.contains(&it.track_id) {
            report.skipped.push(format!("{}: stays on the card because its archive copy was not made", it.label));
        } else {
            removable.push(it.track_id);
        }
    }
    for it in plan.items.iter().filter(|i| !i.remove && opts.remove) {
        if let Some(n) = &it.note {
            report.skipped.push(format!("{}: {n}", it.label));
        }
    }
    if removable.is_empty() {
        return Ok(report);
    }
    if cancel() {
        report.cancelled = true;
        report.skipped.push("stopped at your request: nothing was removed".into());
        return Ok(report);
    }
    snapshot().map_err(|e| Error::Invalid(format!("nothing was removed: the safety copy of music.db failed ({e})")))?;
    let mut lib = Library::open(root)?;
    // the fresh check, on the database as it is now: a track that joined a mix meanwhile stays
    let fresh = plan_run(&lib, &removable, &RunOptions { archive_dir: None, include_grids: false, remove: true })?;
    let mut doomed: Vec<&RunItem> = Vec::new();
    for it in &fresh.items {
        if it.remove {
            doomed.push(it);
        } else if let Some(n) = &it.note {
            report.skipped.push(format!("{}: {n}", it.label));
        }
    }
    report.skipped.extend(fresh.skipped.iter().cloned());
    let ids: Vec<i64> = doomed.iter().map(|d| d.track_id).collect();
    delete_track_rows(&mut lib.con, &ids)?;
    drop(lib);
    // the rows are gone; now the files. Not cancellable: stopping half way would leave files
    // nothing uses, which is exactly what this step is cleaning up.
    for (i, d) in doomed.iter().enumerate() {
        progress(to_archive.len() + i, total, &format!("Removing {}", d.label));
        let mut freed = 0u64;
        for p in [&d.audio, &d.sidecar] {
            let len = fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len());
            match fs::remove_file(p) {
                Ok(()) => freed += len.unwrap_or(0),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    report.failed.push(format!("{}: the track was removed but {} could not be deleted ({e}); it is listed under unused files", d.label, p.display()));
                }
            }
        }
        report.freed_bytes += freed;
        // the row is gone either way; a file that stayed behind is named in `failed`
        report.removed += 1;
    }
    progress(total, total, "");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;
    use crate::ops_device::verify_manifest;
    use crate::strfile;

    const NOW: i64 = 1_800_000_000_000;

    struct Fx {
        base: PathBuf,
        lib: Library,
        ids: Vec<i64>,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn fixture(tag: &str, n: usize) -> Fx {
        let base = std::env::temp_dir().join(format!("pm-space-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let src = base.join("src");
        fs::create_dir_all(&vol).unwrap();
        fs::create_dir_all(&src).unwrap();
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
            // distinct sizes and contents, so no two are duplicates unless a test makes them so
            fs::write(&f, vec![i as u8 + 1; 10_000 + i * 1000]).unwrap();
            let meta = TrackMeta { title: format!("Title {i}"), artist: format!("Artist {i}"), format: "MP3".into(), play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        Fx { base, lib, ids }
    }

    impl Fx {
        fn set(&self, id: i64, plays: i64, last_played: i64, rating: i64, added: i64) {
            self.lib
                .con
                .execute("UPDATE tracks SET times_played = ?2, last_played = ?3, rating = ?4, date_added = ?5 WHERE track_id = ?1", rusqlite::params![id, plays, last_played, rating, added])
                .unwrap();
        }
        fn old(&self, id: i64) {
            // added long ago, so the "new track" protection does not apply
            self.set(id, 0, -1, 0, NOW - 400 * DAY_MS);
        }
        fn paths(&self, id: i64) -> (PathBuf, PathBuf) {
            self.try_paths(id).unwrap()
        }
        fn try_paths(&self, id: i64) -> Option<(PathBuf, PathBuf)> {
            let t = self.lib.tracks().unwrap().into_iter().find(|t| t.track_id == id)?;
            checked_track_paths(self.lib.root(), &t.location)
        }
        fn storage(&self, free: u64) -> Storage {
            let music: u64 = self.ids.iter().filter_map(|&i| self.try_paths(i)).map(|p| fs::metadata(p.0).map_or(0, |m| m.len())).sum();
            Storage { total_bytes: 1 << 30, free_bytes: free, music_bytes: music, grids_mixes_bytes: 0, other_bytes: 0 }
        }
        fn snapshot_ok() -> impl Fn() -> Result<(), Error> {
            || Ok(())
        }
    }

    fn only(rules: impl FnOnce(&mut Rules)) -> Rules {
        let mut r = Rules {
            never_played: false,
            stale_months: 0,
            low_rating_max: 0,
            unrated_is_low: false,
            duplicates: false,
            min_reasons: 1,
            only_no_case: false,
            only_no_mix: false,
            protect_favourites: false,
            protect_rating_min: 0,
            protect_new_days: 0,
            order: Order::Certain,
        };
        rules(&mut r);
        r
    }

    fn goal() -> Goal {
        Goal::Free { bytes: 0 }
    }

    #[test]
    fn the_target_is_converted_to_bytes_to_free() {
        let s = Storage { total_bytes: 64 * GIB, free_bytes: 5 * GIB, music_bytes: 50 * GIB, grids_mixes_bytes: GIB, other_bytes: 0 };
        assert_eq!(bytes_to_free(&Goal::Free { bytes: 20 * GIB }, &s).unwrap(), 15 * GIB);
        assert_eq!(bytes_to_free(&Goal::Free { bytes: GIB }, &s).unwrap(), 0, "already enough free space");
        // a 32 GB card keeping 5% free holds 30.4 of the 51 the library takes
        let card = 32_000_000_000u64;
        assert_eq!(usable_card_bytes(card, 5), card - card / 100 * 5);
        assert_eq!(bytes_to_free(&Goal::Card { bytes: card, reserve_percent: 5 }, &s).unwrap(), 51 * GIB - (card - card / 100 * 5));
        assert_eq!(bytes_to_free(&Goal::Card { bytes: 128_000_000_000, reserve_percent: 0 }, &s).unwrap(), 0, "it already fits");
        // free space unreadable: a free-space target cannot be planned, a card target can
        let unknown = Storage { total_bytes: 0, free_bytes: 0, music_bytes: 3 * GIB, ..Storage::default() };
        assert!(bytes_to_free(&Goal::Free { bytes: GIB }, &unknown).unwrap_err().to_string().contains("could not be read"));
        assert_eq!(bytes_to_free(&Goal::Card { bytes: GIB, reserve_percent: 0 }, &unknown).unwrap(), 2 * GIB);
        assert_eq!(usable_card_bytes(100, 500), 10, "the reserve is capped at 90%");
    }

    #[test]
    fn rules_are_clamped_and_the_defaults_are_cautious() {
        let d = Rules::default();
        assert!(d.protect_favourites && d.only_no_mix && d.only_no_case);
        let c = Rules { stale_months: 9999, low_rating_max: 9, protect_rating_min: 9, protect_new_days: 99_999, min_reasons: 0, ..Rules::default() }.clamped();
        assert_eq!((c.stale_months, c.low_rating_max, c.protect_rating_min, c.protect_new_days, c.min_reasons), (240, 5, 5, 3650, 1));
        assert_eq!(Rules { min_reasons: 40, ..Rules::default() }.clamped().min_reasons, 4);
        // never played + stale count as one slot; low rating and duplicates are the others
        assert_eq!(Rules::default().reason_slots(), 3);
        assert_eq!(only(|r| r.never_played = true).reason_slots(), 1);
        assert_eq!(only(|_| {}).reason_slots(), 0);
    }

    #[test]
    fn safe_stems_are_valid_file_names() {
        assert_eq!(safe_stem("Artist - Title"), "Artist - Title");
        assert_eq!(safe_stem("AC/DC: Back <in> \"Black\"?"), "AC_DC_ Back _in_ _Black__");
        assert_eq!(safe_stem("  trailing dots... "), "trailing dots");
        assert_eq!(safe_stem(""), "track");
        assert_eq!(safe_stem("..."), "track");
        assert_eq!(safe_stem("CON"), "_CON");
        assert_eq!(safe_stem("nul.txt"), "_nul.txt");
        assert_eq!(safe_stem("a\nb\tc\0d"), "a_b_c_d");
        assert_eq!(safe_stem(&"x".repeat(500)).chars().count(), 120);
        assert_eq!(safe_stem(&"é".repeat(200)).chars().count(), 120, "cut on characters, never inside one");
        assert_eq!(ext_of("/pmdb_tracks/6/8/722ef9a6.MP3"), "mp3");
        assert_eq!(ext_of("/pmdb_tracks/6/8/noext"), "");
        assert_eq!(ext_of("/pmdb_tracks/6/8/x.m4a/"), "");
    }

    #[test]
    fn each_reason_fires_on_its_own_threshold() {
        let fx = fixture("reasons", 6);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        // 0 never played; 1 played a year ago; 2 played last week; 3 rated 2 stars; 4 rated 3; 5 unrated, played
        fx.set(ids[1], 5, NOW - 12 * MONTH_MS, 3, NOW - 400 * DAY_MS);
        fx.set(ids[2], 5, NOW - 7 * DAY_MS, 3, NOW - 400 * DAY_MS);
        fx.set(ids[3], 5, NOW - DAY_MS, 2, NOW - 400 * DAY_MS);
        fx.set(ids[4], 5, NOW - DAY_MS, 3, NOW - 400 * DAY_MS);
        fx.set(ids[5], 5, NOW - DAY_MS, 0, NOW - 400 * DAY_MS);
        let st = fx.storage(0);
        let names = |p: &Plan| p.items.iter().map(|i| i.track_id).collect::<Vec<_>>();

        let p = plan(&fx.lib, &only(|r| r.never_played = true), &goal(), &st, NOW).unwrap();
        assert_eq!(names(&p), [ids[0]]);
        assert_eq!(p.items[0].reasons[0].text, "Never played");

        let p = plan(&fx.lib, &only(|r| r.stale_months = 6), &goal(), &st, NOW).unwrap();
        assert_eq!(names(&p), [ids[1]]);
        assert_eq!(p.items[0].reasons[0].text, "Not played for 12 months");
        let p = plan(&fx.lib, &only(|r| r.stale_months = 13), &goal(), &st, NOW).unwrap();
        assert!(p.items.is_empty(), "13 months is more than a year");

        let p = plan(&fx.lib, &only(|r| r.low_rating_max = 2), &goal(), &st, NOW).unwrap();
        assert_eq!(names(&p), [ids[3]], "unrated tracks are not low rated unless the setting says so");
        assert_eq!(p.items[0].reasons[0].text, "Rated 2 stars");
        let p = plan(&fx.lib, &only(|r| { r.low_rating_max = 2; r.unrated_is_low = true; }), &goal(), &st, NOW).unwrap();
        let mut got = names(&p);
        got.sort();
        assert_eq!(got, [ids[0], ids[3], ids[5]]);
        assert!(p.items.iter().any(|i| i.reasons[0].text == "Unrated"));

        // nothing on: nothing nominated, and the plan says why
        let p = plan(&fx.lib, &only(|_| {}), &goal(), &st, NOW).unwrap();
        assert!(!p.rules_on && p.items.is_empty());
    }

    #[test]
    fn conditions_protections_and_min_reasons_narrow_the_list() {
        let mut fx = fixture("narrow", 6);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        // 0 in a case; 1 favourite; 2 in a mix; 3 rated 5; 4 added yesterday; 5 plain
        fx.lib.create_case_with_tracks("Warm up", &[ids[0]]).unwrap();
        fx.lib.toggle_favourite(ids[1]).unwrap();
        let mix = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", rusqlite::params![mix, ids[2]]).unwrap();
        fx.set(ids[3], 0, -1, 5, NOW - 400 * DAY_MS);
        fx.set(ids[4], 0, -1, 0, NOW - DAY_MS);
        let st = fx.storage(0);
        let ids_of = |p: &Plan| {
            let mut v: Vec<i64> = p.items.iter().map(|i| i.track_id).collect();
            v.sort();
            v
        };
        let base = only(|r| r.never_played = true);

        // every track is unplayed; no conditions or protections: all six
        assert_eq!(plan(&fx.lib, &base, &goal(), &st, NOW).unwrap().items.len(), 6);

        // "in no case" drops track 0 and the favourite (which is in the Favourites case)
        let p = plan(&fx.lib, &Rules { only_no_case: true, ..base.clone() }, &goal(), &st, NOW).unwrap();
        assert_eq!(ids_of(&p), [ids[2], ids[3], ids[4], ids[5]]);
        assert!(p.items.iter().all(|i| i.reasons.iter().any(|r| r.kind == ReasonKind::NoCase)));

        // "in no mix" drops track 2
        let p = plan(&fx.lib, &Rules { only_no_mix: true, ..base.clone() }, &goal(), &st, NOW).unwrap();
        assert!(!ids_of(&p).contains(&ids[2]) && p.items.len() == 5);

        // without the condition the mix track is listed but cannot be removed
        let p = plan(&fx.lib, &base, &goal(), &st, NOW).unwrap();
        let blocked: Vec<_> = p.items.iter().filter(|i| i.blocked.is_some()).collect();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].track_id, ids[2]);
        assert!(blocked[0].blocked.as_ref().unwrap().contains("Late set"));
        assert_eq!(p.items.last().unwrap().track_id, ids[2], "blocked rows sort last");
        assert!(p.removable_bytes < p.candidate_bytes);

        // protections hide tracks and count them
        let p = plan(&fx.lib, &Rules { protect_favourites: true, protect_rating_min: 4, protect_new_days: 30, ..base.clone() }, &goal(), &st, NOW).unwrap();
        assert_eq!(p.hidden, Hidden { favourites: 1, high_rated: 1, new_tracks: 1 });
        assert_eq!(ids_of(&p), [ids[0], ids[2], ids[5]]);
        // a protection set to 0 is off
        let p = plan(&fx.lib, &Rules { protect_favourites: false, ..base.clone() }, &goal(), &st, NOW).unwrap();
        assert!(ids_of(&p).contains(&ids[1]));

        // min_reasons: two reasons needed, only the never-played AND low-rated one qualifies
        fx.set(ids[5], 0, -1, 1, NOW - 400 * DAY_MS);
        let both = Rules { never_played: true, low_rating_max: 2, min_reasons: 2, ..only(|_| {}) };
        let p = plan(&fx.lib, &both, &goal(), &st, NOW).unwrap();
        assert_eq!(ids_of(&p), [ids[5]]);
        // asking for more reasons than can exist does not empty the list: it is clamped to the slots
        let p = plan(&fx.lib, &Rules { min_reasons: 4, ..both.clone() }, &goal(), &st, NOW).unwrap();
        assert_eq!(ids_of(&p), [ids[5]]);
    }

    #[test]
    fn duplicates_are_found_by_content_and_the_most_used_copy_is_kept() {
        let mut fx = fixture("dups", 4);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        // track 1 and 2 hold the same bytes as track 0; track 3 has the same length but other bytes
        let bytes0 = fs::read(fx.paths(ids[0]).0).unwrap();
        fs::write(fx.paths(ids[1]).0, &bytes0).unwrap();
        fs::write(fx.paths(ids[2]).0, &bytes0).unwrap();
        let mut other = bytes0.clone();
        other[0] ^= 0xff;
        fs::write(fx.paths(ids[3]).0, other).unwrap();
        // track 2 is the most played, but track 0 is in a case: a copy somebody filed is the one to keep
        fx.set(ids[2], 9, NOW - DAY_MS, 3, NOW - 400 * DAY_MS);
        fx.lib.create_case_with_tracks("Warm up", &[ids[0]]).unwrap();
        let st = fx.storage(0);
        let p = plan(&fx.lib, &only(|r| r.duplicates = true), &goal(), &st, NOW).unwrap();
        let mut got: Vec<(i64, i64)> = p.items.iter().map(|i| (i.track_id, i.duplicate_of.unwrap())).collect();
        got.sort();
        assert_eq!(got, [(ids[1], ids[0]), (ids[2], ids[0])], "both other copies point at the keeper; the different file is not a duplicate");
        assert!(p.items[0].reasons[0].text.starts_with("Same audio as Artist 0 - Title 0"));
        // the keeper is never listed for being a duplicate
        assert!(!p.items.iter().any(|i| i.track_id == ids[0]));
        // with nothing filed, the most played copy is the keeper
        fx.lib.delete_case(1).unwrap();
        let p = plan(&fx.lib, &only(|r| r.duplicates = true), &goal(), &st, NOW).unwrap();
        assert!(p.items.iter().all(|i| i.duplicate_of == Some(ids[2])));

        // a keeper in a mix outranks everything else
        let mix = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", rusqlite::params![mix, ids[1]]).unwrap();
        let p = plan(&fx.lib, &only(|r| r.duplicates = true), &goal(), &st, NOW).unwrap();
        let mut got: Vec<i64> = p.items.iter().map(|i| i.track_id).collect();
        got.sort();
        assert_eq!(got, [ids[0], ids[2]]);
        assert!(p.items.iter().all(|i| i.duplicate_of == Some(ids[1])));
        // the rule off: no hashing, no duplicates
        assert!(plan(&fx.lib, &only(|r| r.never_played = true), &goal(), &st, NOW).unwrap().items.iter().all(|i| i.duplicate_of.is_none()));
    }

    #[test]
    fn the_order_and_the_suggested_prefix_reach_the_target() {
        let fx = fixture("order", 5);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        // 0 never played, 1 played long ago AND rated 1 (two reasons), 2 never played, 3, 4 played recently
        fx.set(ids[1], 3, NOW - 24 * MONTH_MS, 1, NOW - 400 * DAY_MS);
        fx.set(ids[3], 3, NOW - DAY_MS, 3, NOW - 400 * DAY_MS);
        fx.set(ids[4], 3, NOW - DAY_MS, 3, NOW - 400 * DAY_MS);
        let rules = only(|r| {
            r.never_played = true;
            r.stale_months = 6;
            r.low_rating_max = 2;
        });
        let size = |i: usize| 10_000 + i as u64 * 1000;
        // need 15 000 bytes: track 1 (two reasons) is first, then the never played ones by size (2 before 0)
        let st = Storage { total_bytes: 1 << 30, free_bytes: 100, music_bytes: 1 << 20, grids_mixes_bytes: 0, other_bytes: 0 };
        let p = plan(&fx.lib, &rules, &Goal::Free { bytes: 100 + 15_000 }, &st, NOW).unwrap();
        let order: Vec<i64> = p.items.iter().map(|i| i.track_id).collect();
        assert_eq!(order, [ids[1], ids[2], ids[0]]);
        assert_eq!(p.need_bytes, 15_000);
        // grids (written by add_track) are part of what a removal frees
        let grid1 = fs::metadata(fx.paths(ids[1]).1).unwrap().len();
        assert_eq!(p.items[0].bytes, size(1) + grid1);
        let flags: Vec<bool> = p.items.iter().map(|i| i.suggested).collect();
        assert_eq!(flags, [true, true, false], "track 1 + track 2 reach 15 000; track 0 is not needed");
        assert!(p.suggested_bytes >= 15_000 && p.suggested_count == 2);
        assert!(p.reachable);

        // largest first: the fewest tracks
        let big = Rules { order: Order::Largest, ..rules.clone() };
        let p = plan(&fx.lib, &big, &Goal::Free { bytes: 100 + 12_000 }, &st, NOW).unwrap();
        assert_eq!(p.items[0].track_id, ids[2], "12 000 bytes: the biggest unplayed file");
        assert_eq!(p.suggested_count, 1);

        // already enough room: nothing is suggested, the list is still there
        let p = plan(&fx.lib, &rules, &Goal::Free { bytes: 50 }, &st, NOW).unwrap();
        assert_eq!(p.need_bytes, 0);
        assert_eq!(p.suggested_count, 0);
        assert_eq!(p.items.len(), 3);

        // a target the candidates cannot reach
        let p = plan(&fx.lib, &rules, &Goal::Free { bytes: 100 + 10_000_000 }, &st, NOW).unwrap();
        assert!(!p.reachable);
        assert_eq!(p.suggested_count, 3, "everything removable is suggested");
    }

    #[test]
    fn a_track_and_its_copy_are_never_both_suggested() {
        let fx = fixture("dupsuggest", 3);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        // all three hold the same bytes; track 0 is the most played, so it is the keeper
        let bytes0 = fs::read(fx.paths(ids[0]).0).unwrap();
        fs::write(fx.paths(ids[1]).0, &bytes0).unwrap();
        fs::write(fx.paths(ids[2]).0, &bytes0).unwrap();
        fx.set(ids[0], 9, NOW - DAY_MS, 3, NOW - 400 * DAY_MS);
        let rules = only(|r| {
            r.duplicates = true;
            r.stale_months = 1;
        });
        let st = Storage { total_bytes: 1 << 30, free_bytes: 0, music_bytes: 1 << 20, ..Storage::default() };
        // a target that needs everything: the keeper still stays
        let p = plan(&fx.lib, &rules, &Goal::Free { bytes: 1 << 29 }, &st, NOW).unwrap();
        let suggested: Vec<i64> = p.items.iter().filter(|i| i.suggested).map(|i| i.track_id).collect();
        assert_eq!(suggested.len(), 2, "the two copies");
        assert!(!suggested.contains(&ids[0]));

        // when the keeper is itself nominated for another reason, it is still not suggested next to its copies
        fx.set(ids[0], 9, NOW - 20 * MONTH_MS, 3, NOW - 400 * DAY_MS);
        let p = plan(&fx.lib, &rules, &Goal::Free { bytes: 1 << 29 }, &st, NOW).unwrap();
        let keeper = p.items.iter().find(|i| i.track_id == ids[0]).expect("the keeper is nominated: not played for months");
        assert!(keeper.reasons.iter().any(|r| r.kind == ReasonKind::NotPlayedRecently));
        assert!(p.items.iter().filter(|i| i.track_id != ids[0]).all(|i| i.suggested), "both copies go");
        assert!(!keeper.suggested);
    }

    #[test]
    fn blocked_tracks_never_count_toward_the_target() {
        let fx = fixture("blockedgoal", 2);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        let mut fx = fx;
        let m = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", rusqlite::params![m, ids[0]]).unwrap();
        let st = Storage { total_bytes: 1 << 30, free_bytes: 0, music_bytes: 1 << 20, ..Storage::default() };
        let p = plan(&fx.lib, &only(|r| r.never_played = true), &Goal::Free { bytes: 1_000_000 }, &st, NOW).unwrap();
        assert_eq!(p.items.len(), 2);
        let blocked = p.items.iter().find(|i| i.track_id == ids[0]).unwrap();
        assert!(!blocked.suggested);
        assert!(!p.reachable, "only the unblocked track can free space");
    }

    #[test]
    fn missing_audio_and_odd_locations_are_counted_not_listed() {
        let fx = fixture("missing", 3);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        fs::remove_file(fx.paths(ids[0]).0).unwrap();
        fx.lib.con.execute("UPDATE tracks SET location = '/pmdb_tracks/../../outside.mp3' WHERE track_id = ?1", [ids[1]]).unwrap();
        let p = plan(&fx.lib, &only(|r| r.never_played = true), &goal(), &fx.storage(0), NOW).unwrap();
        assert_eq!((p.missing_audio, p.unchecked), (1, 1));
        assert_eq!(p.items.iter().map(|i| i.track_id).collect::<Vec<_>>(), [ids[2]]);
        assert!(!p.warnings.iter().any(|w| w.contains("not fully mounted")), "{:?}", p.warnings);
        // most of the library gone: probably a card that is not fully mounted
        fs::remove_file(fx.paths(ids[2]).0).unwrap();
        let p = plan(&fx.lib, &only(|r| r.never_played = true), &goal(), &fx.storage(0), NOW).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("not fully mounted")), "{:?}", p.warnings);
    }

    #[test]
    fn a_library_with_no_play_history_says_so_and_a_zero_date_is_not_a_play() {
        let fx = fixture("noplays", 3);
        let ids = fx.ids.clone();
        for &i in &ids {
            fx.old(i);
        }
        let st = fx.storage(0);
        let rules = only(|r| {
            r.never_played = true;
            r.stale_months = 6;
        });
        let p = plan(&fx.lib, &rules, &goal(), &st, NOW).unwrap();
        assert!(p.no_play_history);
        assert!(p.warnings.iter().any(|w| w.contains("no play history")), "{:?}", p.warnings);
        assert!(p.items.iter().all(|i| i.reasons[0].text.contains("no play history on this card")), "the reason does not claim more than the data shows");
        // with the reasons off there is nothing to warn about
        let quiet = plan(&fx.lib, &only(|r| r.low_rating_max = 2), &goal(), &st, NOW).unwrap();
        assert!(quiet.no_play_history && !quiet.warnings.iter().any(|w| w.contains("no play history")));

        // one real play anywhere: the data exists, so the plain reason comes back
        fx.set(ids[0], 3, NOW - DAY_MS, 0, NOW - 400 * DAY_MS);
        let p = plan(&fx.lib, &rules, &goal(), &st, NOW).unwrap();
        assert!(!p.no_play_history && !p.warnings.iter().any(|w| w.contains("no play history")));
        assert!(p.items.iter().all(|i| i.reasons[0].text == "Never played"), "{:?}", p.items.iter().map(|i| &i.reasons).collect::<Vec<_>>());

        // last_played 0 (the value a case row holds) is no date: not "650 months ago", just never played
        fx.set(ids[1], 0, 0, 0, NOW - 400 * DAY_MS);
        let p = plan(&fx.lib, &rules, &goal(), &st, NOW).unwrap();
        let it = p.items.iter().find(|i| i.track_id == ids[1]).unwrap();
        assert_eq!(it.reasons[0].kind, ReasonKind::NeverPlayed);
        assert_eq!(it.last_played_ms, None);
    }

    #[test]
    fn a_stopped_scan_reads_no_more_files() {
        let fx = fixture("stop", 3);
        // all three share a size, so the duplicate check has to read them
        let dup = fx.paths(fx.ids[0]).0;
        for &i in &fx.ids[1..] {
            fs::copy(&dup, fx.paths(i).0).unwrap();
        }
        let rules = Rules { duplicates: true, ..only(|r| r.never_played = true) };
        let st = fx.storage(0);
        let polled = std::cell::Cell::new(0usize);
        let e = plan_cancellable(&fx.lib, &rules, &goal(), &st, NOW, &|| {
            polled.set(polled.get() + 1);
            polled.get() > 1
        })
        .unwrap_err()
        .to_string();
        assert!(e.contains(SCAN_STOPPED), "{e}");
        assert_eq!(polled.get(), 2, "it stopped at the first poll that said so");
        // not stopped: the same call finishes and finds the copies
        let p = plan_cancellable(&fx.lib, &rules, &goal(), &st, NOW, &|| false).unwrap();
        assert_eq!(p.items.iter().filter(|i| i.duplicate_of.is_some()).count(), 2);
    }

    #[test]
    fn date_added_in_seconds_still_protects_a_new_track() {
        let fx = fixture("seconds", 2);
        let ids = fx.ids.clone();
        let rules = only(|r| {
            r.never_played = true;
            r.protect_new_days = 30;
        });
        // two days old, written in seconds and in milliseconds
        fx.set(ids[0], 0, -1, 0, (NOW - 2 * DAY_MS) / 1000);
        fx.set(ids[1], 0, -1, 0, NOW - 2 * DAY_MS);
        let p = plan(&fx.lib, &rules, &goal(), &fx.storage(0), NOW).unwrap();
        assert!(p.items.is_empty() && p.hidden.new_tracks == 2, "{:?} {:?}", p.items.len(), p.hidden);
        assert_eq!(added_to_ms(0), 0);
        assert_eq!(added_to_ms(1_700_000_000), 1_700_000_000_000);
        assert_eq!(added_to_ms(1_700_000_000_000), 1_700_000_000_000);
    }

    fn opts(dir: &Path, remove: bool) -> RunOptions {
        RunOptions { archive_dir: Some(dir.to_path_buf()), include_grids: true, remove }
    }

    fn run_all(fx: &Fx, ids: &[i64], o: &RunOptions) -> Result<RunReport, Error> {
        run(fx.lib.root(), ids, o, &Fx::snapshot_ok(), &mut |_, _, _| {}, &|| false)
    }

    #[test]
    fn archiving_makes_a_verified_copy_and_touches_nothing_on_the_card() {
        let fx = fixture("archive", 3);
        let ids = fx.ids.clone();
        let dest = fx.base.join("pc");
        fs::create_dir_all(&dest).unwrap();
        let before = fx.lib.tracks().unwrap().len();
        let audio0 = fs::read(fx.paths(ids[0]).0).unwrap();
        let rep = run_all(&fx, &ids[..2], &opts(&dest, false)).unwrap();
        assert_eq!((rep.archived, rep.removed), (2, 0));
        let folder = PathBuf::from(rep.archive_folder.unwrap());
        assert!(folder.file_name().unwrap().to_string_lossy().starts_with(ARCHIVE_PREFIX));
        assert_eq!(fs::read(folder.join("Artist 0 - Title 0.mp3")).unwrap(), audio0);
        assert!(folder.join("Artist 0 - Title 0.str").is_file(), "the beat grid comes along");
        assert!(folder.join(ARCHIVE_LIST_NAME).is_file());
        // the manifest is in the format the Backup card's checker reads: every file verifies
        assert_eq!(verify_manifest(&folder).unwrap(), Vec::<String>::new());
        let csv = fs::read_to_string(folder.join(ARCHIVE_LIST_NAME)).unwrap();
        assert!(csv.starts_with("file,artist,title"), "{csv}");
        assert!(csv.contains("\"Artist 1\"") && csv.contains("/pmdb_tracks/"), "{csv}");
        // no temp files, and the card is as it was
        assert!(!fs::read_dir(&folder).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".part")));
        assert_eq!(fx.lib.tracks().unwrap().len(), before);
        assert!(fx.paths(ids[0]).0.is_file() && fx.paths(ids[1]).0.is_file());
        // a copy that is damaged afterwards is caught by the same check
        fs::write(folder.join("Artist 1 - Title 1.mp3"), b"damaged").unwrap();
        assert_eq!(verify_manifest(&folder).unwrap().len(), 1);
    }

    #[test]
    fn archive_without_grids_and_with_a_name_clash() {
        let fx = fixture("clash", 2);
        let ids = fx.ids.clone();
        // two tracks that would get the same file name
        fx.lib.con.execute("UPDATE tracks SET artist = 'Same', title = 'Same'", []).unwrap();
        let dest = fx.base.join("pc");
        fs::create_dir_all(&dest).unwrap();
        let o = RunOptions { include_grids: false, ..opts(&dest, false) };
        let rep = run_all(&fx, &ids, &o).unwrap();
        let folder = PathBuf::from(rep.archive_folder.unwrap());
        let mut names: Vec<String> = fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, [ARCHIVE_LIST_NAME.to_string(), MANIFEST_NAME.to_string(), "Same - Same (2).mp3".to_string(), "Same - Same.mp3".to_string()]);
        assert_eq!(verify_manifest(&folder).unwrap().len(), 0);
    }

    #[test]
    fn two_formats_of_one_song_keep_two_beat_grids() {
        let mut fx = fixture("formats", 1);
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        // the same song as a WAV, with a grid of its own (a different tempo, so the two differ)
        let wav = fx.base.join("src").join("synthetic wav.wav");
        fs::write(&wav, vec![9u8; 12_345]).unwrap();
        let meta = TrackMeta { title: "Title".into(), artist: "Artist".into(), format: "WAV".into(), play_time_secs: 100, ..Default::default() };
        let other = strfile::Analysis { beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 90.0 }).collect(), ..analysis };
        let wav_id = fx.lib.add_track(&wav, &meta, &other).unwrap();
        let mp3_id = fx.ids[0];
        fx.lib.con.execute("UPDATE tracks SET artist = 'Same', title = 'Same'", []).unwrap();
        fx.ids.push(wav_id);
        let dest = fx.base.join("pc");
        fs::create_dir_all(&dest).unwrap();

        let preview = plan_run(&fx.lib, &[mp3_id, wav_id], &opts(&dest, false)).unwrap();
        let names: Vec<String> = preview.items.iter().map(|i| i.archive_name.clone().unwrap()).collect();
        assert_eq!(names, ["Same - Same.mp3".to_string(), format!("Same - Same ({wav_id}).wav")], "the clash is seen from the grid name too");

        let rep = run_all(&fx, &[mp3_id, wav_id], &opts(&dest, false)).unwrap();
        assert_eq!((rep.archived, rep.failed.len()), (2, 0), "{:?}", rep.failed);
        let folder = PathBuf::from(rep.archive_folder.unwrap());
        let mut grids: Vec<String> =
            fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".str")).collect();
        grids.sort();
        assert_eq!(grids, [format!("Same - Same ({wav_id}).str"), "Same - Same.str".to_string()]);
        assert_ne!(fs::read(folder.join(&grids[0])).unwrap(), fs::read(folder.join(&grids[1])).unwrap(), "each track keeps its own grid");
        let manifest = fs::read_to_string(folder.join(MANIFEST_NAME)).unwrap();
        assert_eq!(manifest.matches(".str").count(), 2, "{manifest}");
        assert_eq!(verify_manifest(&folder).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn the_archive_folder_must_be_a_real_folder_off_the_card() {
        let fx = fixture("dest", 2);
        let ids = fx.ids.clone();
        let on_card = fx.lib.root().join("archive");
        fs::create_dir_all(&on_card).unwrap();
        let e = run_all(&fx, &ids, &opts(&on_card, false)).unwrap_err().to_string();
        assert!(e.contains("another drive"), "{e}");
        let e = run_all(&fx, &ids, &opts(&fx.base.join("nowhere"), false)).unwrap_err().to_string();
        assert!(e.contains("not a folder"), "{e}");
        let e = run_all(&fx, &ids, &RunOptions::default()).unwrap_err().to_string();
        assert!(e.contains("nothing to do"), "{e}");
        let e = run_all(&fx, &[], &opts(&fx.base, false)).unwrap_err().to_string();
        assert!(e.contains("no tracks are ticked"), "{e}");
        assert_eq!(fx.lib.tracks().unwrap().len(), 2);
    }

    #[test]
    fn archive_then_remove_takes_rows_cases_files_and_grids_and_keeps_mix_tracks() {
        let mut fx = fixture("both", 4);
        let ids = fx.ids.clone();
        let dest = fx.base.join("pc");
        fs::create_dir_all(&dest).unwrap();
        let case = fx.lib.create_case_with_tracks("Warm up", &ids).unwrap();
        let m = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", rusqlite::params![m, ids[1]]).unwrap();
        let (audio0, grid0) = fx.paths(ids[0]);
        let (audio1, _) = fx.paths(ids[1]);
        let size0 = fs::metadata(&audio0).unwrap().len() + fs::metadata(&grid0).unwrap().len();

        // the confirmation shows what the run will do, in words
        let preview = plan_run(&fx.lib, &[ids[0], ids[1], ids[0], 9999], &opts(&dest, true)).unwrap();
        assert!(preview.problems.is_empty(), "{:?}", preview.problems);
        assert_eq!(preview.items.len(), 2, "a repeated id counts once");
        assert!(preview.items[0].remove && !preview.items[1].remove);
        assert!(preview.items[1].note.as_ref().unwrap().contains("Late set"));
        assert_eq!(preview.removed(), 1);
        assert_eq!(preview.archived(), 2);
        assert_eq!(preview.skipped.len(), 1);
        assert_eq!(preview.remove_bytes, size0);

        let rep = run_all(&fx, &[ids[0], ids[1]], &opts(&dest, true)).unwrap();
        assert_eq!((rep.archived, rep.removed), (2, 1));
        assert_eq!(rep.freed_bytes, size0);
        assert!(rep.skipped.iter().any(|s| s.contains("Late set")), "{:?}", rep.skipped);
        assert!(!audio0.exists() && !grid0.exists(), "files gone");
        assert!(audio1.is_file(), "the mix track's audio stays");
        let left: Vec<i64> = fx.lib.tracks().unwrap().iter().map(|t| t.track_id).collect();
        assert_eq!(left, [ids[1], ids[2], ids[3]]);
        assert_eq!(fx.lib.case_track_ids(case).unwrap(), [ids[1], ids[2], ids[3]]);
        let secs: i64 = fx.lib.con.query_row("SELECT play_time_secs FROM cases WHERE case_id = ?1", [case], |r| r.get(0)).unwrap();
        assert_eq!(secs, 101 + 102 + 103, "the case's play time follows its members");
        let in_mix: i64 = fx.lib.con.query_row("SELECT COUNT(*) FROM mixtracks WHERE track_id = ?1", [ids[1]], |r| r.get(0)).unwrap();
        assert_eq!(in_mix, 1);
        // both were archived, including the one that stayed
        assert_eq!(verify_manifest(Path::new(&rep.archive_folder.unwrap())).unwrap().len(), 0);
    }

    #[test]
    fn remove_without_an_archive_is_allowed_by_the_library_and_leaves_no_row_or_file() {
        let fx = fixture("noarchive", 3);
        let ids = fx.ids.clone();
        let (audio, grid) = fx.paths(ids[2]);
        let rep = run_all(&fx, &[ids[2]], &RunOptions { remove: true, ..RunOptions::default() }).unwrap();
        assert_eq!((rep.archived, rep.removed, rep.archive_folder), (0, 1, None));
        assert!(!audio.exists() && !grid.exists());
        assert_eq!(fx.lib.tracks().unwrap().len(), 2);
    }

    #[test]
    fn a_failed_snapshot_removes_nothing() {
        let fx = fixture("snapfail", 2);
        let ids = fx.ids.clone();
        let (audio, _) = fx.paths(ids[0]);
        let e = run(fx.lib.root(), &[ids[0]], &RunOptions { remove: true, ..RunOptions::default() }, &|| Err(Error::Invalid("disk full".into())), &mut |_, _, _| {}, &|| false)
            .unwrap_err()
            .to_string();
        assert!(e.contains("nothing was removed") && e.contains("disk full"), "{e}");
        assert!(audio.is_file());
        assert_eq!(fx.lib.tracks().unwrap().len(), 2);
    }

    #[test]
    fn a_cancel_during_the_archive_removes_nothing_and_keeps_the_verified_copies() {
        let fx = fixture("cancel", 3);
        let ids = fx.ids.clone();
        let dest = fx.base.join("pc");
        fs::create_dir_all(&dest).unwrap();
        let calls = std::cell::Cell::new(0);
        // the user presses cancel after the first file
        let cancel = || {
            calls.set(calls.get() + 1);
            calls.get() > 1
        };
        let rep = run(fx.lib.root(), &ids, &opts(&dest, true), &Fx::snapshot_ok(), &mut |_, _, _| {}, &cancel).unwrap();
        assert!(rep.cancelled);
        assert_eq!((rep.archived, rep.removed), (1, 0));
        assert_eq!(fx.lib.tracks().unwrap().len(), 3, "no row removed");
        for &i in &ids {
            assert!(fx.paths(i).0.is_file(), "no file removed");
        }
        let folder = PathBuf::from(rep.archive_folder.unwrap());
        assert_eq!(verify_manifest(&folder).unwrap().len(), 0, "what was copied is listed and verifies");
    }

    #[test]
    fn a_track_whose_archive_copy_failed_is_not_removed() {
        let fx = fixture("archfail", 2);
        let ids = fx.ids.clone();
        let plan = plan_run(&fx.lib, &ids, &opts(&fx.base.join("src"), true)).unwrap();
        // a "folder" that is really a file: every copy into it fails
        let blocker = fx.base.join("blocker");
        fs::write(&blocker, b"x").unwrap();
        let items: Vec<&RunItem> = plan.items.iter().collect();
        let (ok, failed, cancelled) = archive_items(&items, &blocker, true, 0, 2, &mut |_, _, _| {}, &|| false);
        assert!(ok.is_empty() && !cancelled);
        assert_eq!(failed.len(), 2);
        assert!(failed[0].contains("not archived"), "{failed:?}");
    }

    #[test]
    fn a_track_that_joined_a_mix_after_the_preview_stays() {
        let mut fx = fixture("latemix", 2);
        let ids = fx.ids.clone();
        let o = RunOptions { remove: true, ..RunOptions::default() };
        let preview = plan_run(&fx.lib, &[ids[0]], &o).unwrap();
        assert!(preview.items[0].remove);
        // the mix is created between the preview and the run
        let m = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", rusqlite::params![m, ids[0]]).unwrap();
        let rep = run_all(&fx, &[ids[0]], &o).unwrap();
        assert_eq!(rep.removed, 0);
        assert!(rep.skipped.iter().any(|s| s.contains("Late set")), "{:?}", rep.skipped);
        assert!(fx.paths(ids[0]).0.is_file());
        assert_eq!(fx.lib.tracks().unwrap().len(), 2);
    }

    #[test]
    fn a_ticked_track_with_no_audio_or_a_bad_location_is_skipped_by_name() {
        let fx = fixture("skips", 3);
        let ids = fx.ids.clone();
        fs::remove_file(fx.paths(ids[0]).0).unwrap();
        fx.lib.con.execute("UPDATE tracks SET location = '/pmdb_tracks/../../outside.mp3' WHERE track_id = ?1", [ids[1]]).unwrap();
        let p = plan_run(&fx.lib, &ids, &RunOptions { remove: true, ..RunOptions::default() }).unwrap();
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.skipped.len(), 2);
        assert!(p.skipped.iter().any(|s| s.contains("audio file is not on the card")));
        assert!(p.skipped.iter().any(|s| s.contains("not a plain path")));
        // a hostile row can make the plan skip a track, never reach outside the Music folder
        assert!(p.items.iter().all(|i| i.audio.starts_with(fx.lib.root().join(".Pacemaker/Music"))));
    }

    #[test]
    fn checksums_are_reused_until_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("pm-space-hash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.mp3");
        fs::write(&f, b"first contents").unwrap();
        let t0 = fs::metadata(&f).unwrap().modified().unwrap();
        let h1 = hash_cached(&f, 14).unwrap();
        assert_eq!(h1, crate::ops_device::sha256_hex(b"first contents"));
        // same size, same time: the cached sum is returned without reading the file
        fs::write(&f, b"other contents").unwrap();
        fs::File::options().write(true).open(&f).unwrap().set_modified(t0).unwrap();
        assert_eq!(hash_cached(&f, 14).unwrap(), h1, "served from the cache");
        // a newer modification time: read again
        fs::File::options().write(true).open(&f).unwrap().set_modified(t0 + std::time::Duration::from_secs(60)).unwrap();
        assert_eq!(hash_cached(&f, 14).unwrap(), crate::ops_device::sha256_hex(b"other contents"));
        // a different size is a different file
        fs::write(&f, b"longer contents now").unwrap();
        assert_eq!(hash_cached(&f, 19).unwrap(), crate::ops_device::sha256_hex(b"longer contents now"));
        assert!(hash_cached(&dir.join("missing.mp3"), 1).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn csv_cells_are_quoted_and_cannot_run_as_formulas() {
        assert_eq!(csv_cell("a,\"b\"", true), "\"a,\"\"b\"\"\"");
        assert_eq!(csv_cell("=1+1", true), "\"'=1+1\"");
        assert_eq!(csv_cell("-5", false), "\"-5\"", "numbers are left alone");
        assert_eq!(iso(-1), "");
        assert_eq!(iso(NOW), "2027-01-15T08:00:00Z");
    }

    #[test]
    fn sizes_read_like_the_storage_card() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(2048), "2.0 KB");
        assert_eq!(size_text(3 << 20), "3.0 MB");
        assert_eq!(size_text(5 * GIB / 2), "2.5 GB");
    }
}
