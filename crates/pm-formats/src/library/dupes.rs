//! Duplicate tracks: the whole row the review shows, which cases hold a track, and the merge of
//! copies into the one that is kept (owner: cases area).
//!
//! A merge does three things to a group (one keeper and the copies, "others"), all in one
//! transaction per group so a failure leaves the library as it was:
//!
//! 1. Case memberships move. In a case that holds a copy but not the keeper, the keeper takes the
//!    copy's place in the case's order (the `casetracks` row is re-pointed, so the position is
//!    kept); in a case that already holds the keeper the copy's row is just removed.
//! 2. Rating, play count and last-played of the keeper become the best of the group (or only the
//!    keeper's, or for plays the sum, as chosen).
//! 3. Optionally the copies' rows leave the library. A copy that a mix uses is never removed: a
//!    mix clip stores the track id and the tempo it had, and re-pointing it to another file would
//!    change what the mix plays. Such a copy stays (and the plan names the mixes).
//!
//! Files are not touched here. The caller deletes the audio and `.str` of removed rows
//! afterwards, and only the ones nothing else refers to (`ops_device::delete_unreferenced`).
//!
//! [`plan_group`] is the pure part: the preview and the write both come from it, and the write
//! checks inside its transaction that the plan it computes is the one it was asked to apply.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::*;

// ---- reading -------------------------------------------------------------------------------

/// Every column the duplicate review compares (`Track` is frozen for the UI, so this is its own
/// row). Unset text is empty, unset numbers 0; `last_played` is -1 for "never played".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DupRow {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub composer: String,
    pub genre: String,
    pub label: String,
    pub key: String,
    pub comments: String,
    pub producer: String,
    pub remixer: String,
    pub year: i64,
    pub track_number: i64,
    pub disc_number: i64,
    pub format: String,
    pub bit_rate_kbps: i64,
    pub sample_rate: i64,
    pub file_size: i64,
    pub play_time_secs: i64,
    pub bpm: f64,
    pub rating: i64,
    pub times_played: i64,
    pub last_played: i64,
    pub date_added: i64,
    pub location: String,
    pub analysed: bool,
}

const DUP_SELECT: &str = "SELECT track_id, IFNULL(title,''), IFNULL(artist,''), IFNULL(album_artist,''), IFNULL(album,''),
        IFNULL(composer,''), IFNULL(genre,''), IFNULL(label,''), IFNULL(key,''), IFNULL(comments,''),
        IFNULL(producer,''), IFNULL(remixer,''), IFNULL(year,0), IFNULL(track_number,0), IFNULL(disc_number,0),
        IFNULL(format,''), IFNULL(bit_rate,0), IFNULL(sample_rate,0), IFNULL(file_size,0), IFNULL(play_time_secs,0),
        IFNULL(bpm,0), IFNULL(rating,0), IFNULL(times_played,0), IFNULL(last_played,-1), IFNULL(date_added,0),
        IFNULL(location,''), IFNULL(track_flags,0)
     FROM tracks";

fn dup_row(r: &rusqlite::Row) -> rusqlite::Result<DupRow> {
    Ok(DupRow {
        track_id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album_artist: r.get(3)?,
        album: r.get(4)?,
        composer: r.get(5)?,
        genre: r.get(6)?,
        label: r.get(7)?,
        key: r.get(8)?,
        comments: r.get(9)?,
        producer: r.get(10)?,
        remixer: r.get(11)?,
        year: r.get(12)?,
        track_number: r.get(13)?,
        disc_number: r.get(14)?,
        format: r.get(15)?,
        bit_rate_kbps: r.get(16)?,
        sample_rate: r.get(17)?,
        file_size: r.get(18)?,
        play_time_secs: r.get(19)?,
        bpm: r.get(20)?,
        rating: r.get(21)?,
        times_played: r.get(22)?,
        last_played: r.get(23)?,
        date_added: r.get(24)?,
        location: r.get(25)?,
        analysed: r.get::<_, i64>(26)? & 2 != 0,
    })
}

/// One case that holds a track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseHold {
    pub case_id: i64,
    pub name: String,
    /// Place among the case's tracks, 1 for the first.
    pub position: usize,
    /// How many tracks the case holds.
    pub of: usize,
}

/// The cases that hold one track, by case name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackHolds {
    pub track_id: i64,
    pub cases: Vec<CaseHold>,
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

impl Library {
    /// The full row of every track, by track id.
    pub fn dup_rows(&self) -> Result<Vec<DupRow>, Error> {
        let mut st = self.con.prepare(&format!("{DUP_SELECT} WHERE track_id >= 0 ORDER BY track_id"))?;
        let rows = st.query_map([], dup_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// For each requested track (in the order asked, unknown ids left out) the cases that hold
    /// it, by case name: the answer to "which cases have this track". A case counts a track once
    /// however many rows point at it, and rows for tracks that no longer exist take no place.
    pub fn cases_holding(&self, track_ids: &[i64]) -> Result<Vec<TrackHolds>, Error> {
        let mut wanted: Vec<i64> = Vec::new();
        for id in track_ids {
            if !wanted.contains(id) && self.con.query_row("SELECT 1 FROM tracks WHERE track_id = ?1 AND track_id >= 0", [id], |_| Ok(())).optional()?.is_some() {
                wanted.push(*id);
            }
        }
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let names: HashMap<i64, String> = {
            let mut st = self.con.prepare("SELECT case_id, IFNULL(name,'') FROM cases WHERE case_id >= 0")?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        // every live member of every case that holds a wanted track, in case order
        let mut st = self.con.prepare(&format!(
            "SELECT ct.case_id, ct.track_id FROM casetracks ct JOIN tracks t ON t.track_id = ct.track_id AND t.track_id >= 0
              WHERE ct.case_id IN (SELECT case_id FROM casetracks WHERE track_id IN ({})) ORDER BY ct.case_id, ct.rowid",
            placeholders(wanted.len())
        ))?;
        let rows = st.query_map(rusqlite::params_from_iter(wanted.iter()), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut by_case: Vec<(i64, Vec<i64>)> = Vec::new();
        let mut seen: HashSet<(i64, i64)> = HashSet::new();
        for row in rows {
            let (case, track) = row?;
            if !seen.insert((case, track)) {
                continue;
            }
            match by_case.last_mut() {
                Some((c, members)) if *c == case => members.push(track),
                _ => by_case.push((case, vec![track])),
            }
        }
        let mut out: Vec<TrackHolds> = wanted.iter().map(|&track_id| TrackHolds { track_id, cases: Vec::new() }).collect();
        for (case, members) in &by_case {
            let Some(name) = names.get(case) else { continue };
            for (i, t) in members.iter().enumerate() {
                if let Some(h) = out.iter_mut().find(|h| h.track_id == *t) {
                    h.cases.push(CaseHold { case_id: *case, name: name.clone(), position: i + 1, of: members.len() });
                }
            }
        }
        for h in &mut out {
            h.cases.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.case_id.cmp(&b.case_id)));
        }
        Ok(out)
    }

    /// The names of the mixes that use each track (only tracks that some mix uses appear).
    pub fn mix_names_using(&self, track_ids: &[i64]) -> Result<HashMap<i64, Vec<String>>, Error> {
        let mut out = HashMap::new();
        for id in track_ids {
            let names: Vec<String> = self.mixes_using_track(*id)?.into_iter().map(|(_, n)| n).collect();
            if !names.is_empty() {
                out.insert(*id, names);
            }
        }
        Ok(out)
    }
}

// ---- the merge plan ------------------------------------------------------------------------

/// How the keeper's rating is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RatingRule {
    /// The highest rating in the group.
    #[default]
    Max,
    /// The keeper's own, as it is.
    Keeper,
}

/// How the keeper's play count (and last-played) are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlaysRule {
    /// The highest count in the group; the latest last-played.
    #[default]
    Max,
    /// The counts added up (the copies were played separately); the latest last-played.
    Sum,
    /// The keeper's own, as they are.
    Keeper,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeOptions {
    /// Move the copies' case memberships to the keeper.
    pub move_cases: bool,
    pub rating: RatingRule,
    pub plays: PlaysRule,
    /// Take the copies' rows out of the library (except copies a mix uses).
    pub remove_rows: bool,
}

impl Default for MergeOptions {
    fn default() -> Self {
        MergeOptions { move_cases: true, rating: RatingRule::Max, plays: PlaysRule::Max, remove_rows: false }
    }
}

/// One keeper and the copies merged into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeGroup {
    pub keeper: i64,
    pub others: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseEditKind {
    /// The keeper was not in the case: it takes the place of the first copy; further copies go.
    Replace,
    /// The keeper is in the case already: the copies' rows are removed.
    AlreadyThere,
    /// Memberships are not moved and the copies are being removed: they leave the case.
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseEdit {
    pub case_id: i64,
    pub case_name: String,
    pub kind: CaseEditKind,
    /// The copies this edit concerns.
    pub others: Vec<i64>,
}

/// A value before and after the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Change {
    pub before: i64,
    pub after: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatEdit {
    pub rating: Change,
    pub times_played: Change,
    pub last_played: Change,
}

impl StatEdit {
    pub fn changed(&self) -> bool {
        [self.rating, self.times_played, self.last_played].iter().any(|c| c.before != c.after)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemovedRow {
    pub track_id: i64,
    pub location: String,
}

/// A copy that stays because a mix uses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blocked {
    pub track_id: i64,
    pub mixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupPlan {
    pub keeper: i64,
    pub others: Vec<i64>,
    pub case_edits: Vec<CaseEdit>,
    pub stats: StatEdit,
    pub remove: Vec<RemovedRow>,
    pub blocked: Vec<Blocked>,
}

/// What the plan needs to know about the library.
#[derive(Debug, Default)]
pub struct Facts {
    pub rows: HashMap<i64, DupRow>,
    /// `(rowid, case_id, track_id)` of every membership row of the cases that hold a track of the
    /// group, in rowid order.
    pub members: Vec<(i64, i64, i64)>,
    pub case_names: HashMap<i64, String>,
    pub mixes: HashMap<i64, Vec<String>>,
}

/// A plan with the `casetracks` rows it will change.
#[derive(Debug)]
pub struct Computed {
    pub plan: GroupPlan,
    /// Rows re-pointed at the keeper.
    repoint: Vec<i64>,
    /// Rows removed.
    delete: Vec<i64>,
}

/// The pure part of a merge: what happens to one group. Errors for a group that cannot be
/// merged (no copies, the keeper among the copies, a repeated or unknown track).
pub fn plan_group(group: &MergeGroup, opts: &MergeOptions, facts: &Facts) -> Result<Computed, Error> {
    let MergeGroup { keeper, others } = group;
    if others.is_empty() {
        return Err(Error::Invalid("a duplicate group needs at least one copy besides the one kept".into()));
    }
    let mut seen = HashSet::new();
    for id in others {
        if id == keeper {
            return Err(Error::Invalid(format!("track {id} is both the one kept and a copy")));
        }
        if !seen.insert(*id) {
            return Err(Error::Invalid(format!("track {id} is listed twice")));
        }
    }
    let Some(k) = facts.rows.get(keeper) else {
        return Err(Error::Invalid(format!("no track {keeper}")));
    };
    let mut copies: Vec<&DupRow> = Vec::new();
    for id in others {
        copies.push(facts.rows.get(id).ok_or_else(|| Error::Invalid(format!("no track {id}")))?);
    }

    // which copies leave the library, and which stay because a mix uses them
    let mut remove = Vec::new();
    let mut blocked = Vec::new();
    if opts.remove_rows {
        for c in &copies {
            match facts.mixes.get(&c.track_id).filter(|m| !m.is_empty()) {
                Some(m) => blocked.push(Blocked { track_id: c.track_id, mixes: m.clone() }),
                None => remove.push(RemovedRow { track_id: c.track_id, location: c.location.clone() }),
            }
        }
    }
    let removed: HashSet<i64> = remove.iter().map(|r| r.track_id).collect();
    let other_set: HashSet<i64> = others.iter().copied().collect();

    // case memberships, case by case
    let mut cases: BTreeSet<i64> = BTreeSet::new();
    for (_, case, track) in &facts.members {
        if other_set.contains(track) {
            cases.insert(*case);
        }
    }
    let mut case_edits = Vec::new();
    let (mut repoint, mut delete) = (Vec::new(), Vec::new());
    for case in cases {
        let rows: Vec<(i64, i64, i64)> = facts.members.iter().filter(|(_, c, _)| *c == case).copied().collect();
        let keeper_in = rows.iter().any(|(_, _, t)| t == keeper);
        let (kind, affected): (CaseEditKind, Vec<(i64, i64, i64)>) = if opts.move_cases {
            (if keeper_in { CaseEditKind::AlreadyThere } else { CaseEditKind::Replace }, rows.iter().filter(|(_, _, t)| other_set.contains(t)).copied().collect())
        } else {
            (CaseEditKind::Drop, rows.iter().filter(|(_, _, t)| removed.contains(t)).copied().collect())
        };
        if affected.is_empty() {
            continue;
        }
        let mut ids: Vec<i64> = Vec::new();
        for (i, (rowid, _, track)) in affected.iter().enumerate() {
            if kind == CaseEditKind::Replace && i == 0 {
                repoint.push(*rowid);
            } else {
                delete.push(*rowid);
            }
            if !ids.contains(track) {
                ids.push(*track);
            }
        }
        case_edits.push(CaseEdit { case_id: case, case_name: facts.case_names.get(&case).cloned().unwrap_or_default(), kind, others: ids });
    }

    // the keeper's rating and play statistics
    let everyone: Vec<&DupRow> = std::iter::once(k).chain(copies.iter().copied()).collect();
    let rating = match opts.rating {
        RatingRule::Max => everyone.iter().map(|r| r.rating).max().unwrap_or(k.rating),
        RatingRule::Keeper => k.rating,
    };
    let (times_played, last_played) = match opts.plays {
        PlaysRule::Keeper => (k.times_played, k.last_played),
        PlaysRule::Max => (everyone.iter().map(|r| r.times_played).max().unwrap_or(0), everyone.iter().map(|r| r.last_played).max().unwrap_or(-1)),
        PlaysRule::Sum => (everyone.iter().map(|r| r.times_played).sum(), everyone.iter().map(|r| r.last_played).max().unwrap_or(-1)),
    };
    let stats = StatEdit {
        rating: Change { before: k.rating, after: rating },
        times_played: Change { before: k.times_played, after: times_played },
        last_played: Change { before: k.last_played, after: last_played },
    };
    Ok(Computed { plan: GroupPlan { keeper: *keeper, others: others.clone(), case_edits, stats, remove, blocked }, repoint, delete })
}

/// Checks that no track appears in two groups (a keeper of one cannot be a copy of another).
pub fn check_disjoint(groups: &[MergeGroup]) -> Result<(), Error> {
    let mut seen = HashSet::new();
    for g in groups {
        for id in std::iter::once(&g.keeper).chain(&g.others) {
            if !seen.insert(*id) {
                return Err(Error::Invalid(format!("track {id} is in more than one group")));
            }
        }
    }
    Ok(())
}

fn load_facts(con: &Connection, ids: &[i64]) -> Result<Facts, Error> {
    let mut facts = Facts::default();
    let mut st = con.prepare(&format!("{DUP_SELECT} WHERE track_id >= 0 AND track_id IN ({})", placeholders(ids.len())))?;
    for row in st.query_map(rusqlite::params_from_iter(ids.iter()), dup_row)? {
        let r = row?;
        facts.rows.insert(r.track_id, r);
    }
    let mut st = con.prepare(&format!(
        "SELECT rowid, case_id, track_id FROM casetracks
          WHERE case_id IN (SELECT case_id FROM casetracks WHERE track_id IN ({})) ORDER BY rowid",
        placeholders(ids.len())
    ))?;
    for row in st.query_map(rusqlite::params_from_iter(ids.iter()), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
        facts.members.push(row?);
    }
    let mut st = con.prepare("SELECT case_id, IFNULL(name,'') FROM cases WHERE case_id >= 0")?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        facts.case_names.insert(id, name);
    }
    let mut st = con.prepare(&format!(
        "SELECT DISTINCT t.track_id, IFNULL(m.name,'') FROM mixtracks t JOIN mixes m ON m.mix_id = t.mix_id
          WHERE t.track_id IN ({}) ORDER BY m.name COLLATE NOCASE, m.mix_id",
        placeholders(ids.len())
    ))?;
    for row in st.query_map(rusqlite::params_from_iter(ids.iter()), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        facts.mixes.entry(id).or_default().push(name);
    }
    Ok(facts)
}

fn ids_of(g: &MergeGroup) -> Vec<i64> {
    std::iter::once(g.keeper).chain(g.others.iter().copied()).collect()
}

impl Library {
    /// What [`Library::merge_duplicates`] would do with the same arguments. Writes nothing.
    pub fn merge_plan_for(&self, groups: &[MergeGroup], opts: &MergeOptions) -> Result<Vec<GroupPlan>, Error> {
        check_disjoint(groups)?;
        groups
            .iter()
            .map(|g| Ok(plan_group(g, opts, &load_facts(&self.con, &ids_of(g))?)?.plan))
            .collect()
    }

    /// Merges one group in one transaction (see the module notes). With `expected`, the plan
    /// computed inside the transaction must equal it, or nothing is written: the library changed
    /// since the preview. Returns the plan that was carried out.
    pub fn merge_duplicates(&mut self, group: &MergeGroup, opts: &MergeOptions, expected: Option<&GroupPlan>) -> Result<GroupPlan, Error> {
        let tx = self.con.transaction()?;
        let computed = plan_group(group, opts, &load_facts(&tx, &ids_of(group))?)?;
        if expected.is_some_and(|e| e != &computed.plan) {
            return Err(Error::Invalid("the library changed after the preview was made".into()));
        }
        let plan = &computed.plan;
        for rowid in &computed.repoint {
            tx.execute("UPDATE casetracks SET track_id = ?1 WHERE rowid = ?2", params![plan.keeper, rowid])?;
        }
        for rowid in &computed.delete {
            tx.execute("DELETE FROM casetracks WHERE rowid = ?1", [rowid])?;
        }
        if plan.stats.changed() {
            tx.execute(
                "UPDATE tracks SET rating = ?1, times_played = ?2, last_played = ?3, date_modified = ?4, modified_by_ed = ?5
                  WHERE track_id = ?6",
                params![plan.stats.rating.after, plan.stats.times_played.after, plan.stats.last_played.after, now_ms(), EDITOR_ID, plan.keeper],
            )?;
        }
        for r in &plan.remove {
            // memberships of a removed copy that were not moved or dropped above (a stale row)
            tx.execute("DELETE FROM casetracks WHERE track_id = ?1", [r.track_id])?;
            tx.execute("DELETE FROM tracks WHERE track_id = ?1 AND track_id >= 0", [r.track_id])?;
        }
        for e in &plan.case_edits {
            refresh_case_derived(&tx, e.case_id)?;
        }
        tx.commit()?;
        Ok(computed.plan)
    }
}

#[cfg(test)]
mod tests {
    use super::super::case_ops_tests::{cleanup, library_with_tracks};
    use super::*;

    fn row(id: i64, rating: i64, plays: i64, last: i64) -> DupRow {
        DupRow {
            track_id: id,
            title: format!("t{id}"),
            artist: String::new(),
            album_artist: String::new(),
            album: String::new(),
            composer: String::new(),
            genre: String::new(),
            label: String::new(),
            key: String::new(),
            comments: String::new(),
            producer: String::new(),
            remixer: String::new(),
            year: 0,
            track_number: 0,
            disc_number: 0,
            format: "MP3".into(),
            bit_rate_kbps: 0,
            sample_rate: 0,
            file_size: 0,
            play_time_secs: 100,
            bpm: 0.0,
            rating,
            times_played: plays,
            last_played: last,
            date_added: 0,
            location: format!("/pmdb_tracks/{id}.mp3"),
            analysed: true,
        }
    }

    fn facts(rows: Vec<DupRow>, members: &[(i64, i64, i64)]) -> Facts {
        Facts {
            rows: rows.into_iter().map(|r| (r.track_id, r)).collect(),
            members: members.to_vec(),
            case_names: [(10, "Ten".to_string()), (11, "Eleven".to_string()), (12, "Twelve".to_string())].into(),
            mixes: HashMap::new(),
        }
    }

    fn opts(move_cases: bool, remove_rows: bool) -> MergeOptions {
        MergeOptions { move_cases, remove_rows, ..Default::default() }
    }

    #[test]
    fn the_keeper_takes_the_first_copys_place_and_the_rest_leave() {
        // case 10: [other 2, other 3]; case 11: [keeper 1, other 2]; case 12: [9, other 3, 8]
        let f = facts(
            vec![row(1, 0, 0, -1), row(2, 0, 0, -1), row(3, 0, 0, -1)],
            &[(1, 10, 2), (2, 10, 3), (3, 11, 1), (4, 11, 2), (5, 12, 9), (6, 12, 3), (7, 12, 8)],
        );
        let c = plan_group(&MergeGroup { keeper: 1, others: vec![2, 3] }, &opts(true, false), &f).unwrap();
        assert_eq!(c.repoint, vec![1, 6], "the first copy's row in each case without the keeper");
        assert_eq!(c.delete, vec![2, 4]);
        let kinds: Vec<_> = c.plan.case_edits.iter().map(|e| (e.case_id, e.kind, e.others.clone())).collect();
        assert_eq!(
            kinds,
            vec![(10, CaseEditKind::Replace, vec![2, 3]), (11, CaseEditKind::AlreadyThere, vec![2]), (12, CaseEditKind::Replace, vec![3])]
        );
        assert_eq!(c.plan.case_edits[0].case_name, "Ten");
        assert!(c.plan.remove.is_empty());
    }

    #[test]
    fn without_moving_cases_only_removed_copies_leave_their_cases() {
        let f = facts(vec![row(1, 0, 0, -1), row(2, 0, 0, -1)], &[(1, 10, 2), (2, 11, 1)]);
        let g = MergeGroup { keeper: 1, others: vec![2] };
        let c = plan_group(&g, &opts(false, false), &f).unwrap();
        assert!(c.plan.case_edits.is_empty() && c.repoint.is_empty() && c.delete.is_empty(), "nothing moves, nothing is removed");
        let c = plan_group(&g, &opts(false, true), &f).unwrap();
        assert_eq!(c.plan.case_edits.len(), 1);
        assert_eq!((c.plan.case_edits[0].kind, c.delete.clone()), (CaseEditKind::Drop, vec![1]));
        assert!(c.repoint.is_empty());
        assert_eq!(c.plan.remove, vec![RemovedRow { track_id: 2, location: "/pmdb_tracks/2.mp3".into() }]);
    }

    #[test]
    fn a_copy_that_a_mix_uses_stays_and_the_mix_is_named() {
        let mut f = facts(vec![row(1, 0, 0, -1), row(2, 0, 0, -1), row(3, 0, 0, -1)], &[(1, 10, 2)]);
        f.mixes.insert(2, vec!["Sample Mix".into()]);
        let c = plan_group(&MergeGroup { keeper: 1, others: vec![2, 3] }, &opts(true, true), &f).unwrap();
        assert_eq!(c.plan.remove.iter().map(|r| r.track_id).collect::<Vec<_>>(), vec![3]);
        assert_eq!(c.plan.blocked, vec![Blocked { track_id: 2, mixes: vec!["Sample Mix".into()] }]);
        // its membership still moved to the keeper
        assert_eq!(c.plan.case_edits[0].kind, CaseEditKind::Replace);
        // not removing: nothing is blocked because nothing is being removed
        assert!(plan_group(&MergeGroup { keeper: 1, others: vec![2] }, &opts(true, false), &f).unwrap().plan.blocked.is_empty());
    }

    #[test]
    fn rating_and_plays_follow_the_chosen_rules() {
        let f = facts(vec![row(1, 2, 5, 1000), row(2, 4, 3, 5000), row(3, 1, 9, -1)], &[]);
        let g = MergeGroup { keeper: 1, others: vec![2, 3] };
        let s = |o: &MergeOptions| plan_group(&g, o, &f).unwrap().plan.stats;
        let max = s(&MergeOptions::default());
        assert_eq!((max.rating.after, max.times_played.after, max.last_played.after), (4, 9, 5000));
        assert!(max.changed());
        let sum = s(&MergeOptions { plays: PlaysRule::Sum, ..Default::default() });
        assert_eq!((sum.times_played.after, sum.last_played.after), (17, 5000));
        let keep = s(&MergeOptions { rating: RatingRule::Keeper, plays: PlaysRule::Keeper, ..Default::default() });
        assert!(!keep.changed());
        assert_eq!((keep.rating.after, keep.times_played.after, keep.last_played.after), (2, 5, 1000));
        // "never played" (-1) in every row stays -1
        let f = facts(vec![row(1, 0, 0, -1), row(2, 0, 0, -1)], &[]);
        let st = plan_group(&MergeGroup { keeper: 1, others: vec![2] }, &MergeOptions::default(), &f).unwrap().plan.stats;
        assert!(!st.changed() && st.last_played.after == -1);
    }

    #[test]
    fn bad_groups_are_refused() {
        let f = facts(vec![row(1, 0, 0, -1), row(2, 0, 0, -1)], &[]);
        let o = MergeOptions::default();
        let err = |g: MergeGroup| plan_group(&g, &o, &f).unwrap_err().to_string();
        assert!(err(MergeGroup { keeper: 1, others: vec![] }).contains("at least one copy"));
        assert!(err(MergeGroup { keeper: 1, others: vec![1] }).contains("both the one kept"));
        assert!(err(MergeGroup { keeper: 1, others: vec![2, 2] }).contains("twice"));
        assert!(err(MergeGroup { keeper: 1, others: vec![9] }).contains("no track 9"));
        assert!(err(MergeGroup { keeper: 9, others: vec![1] }).contains("no track 9"));
        let overlap = [MergeGroup { keeper: 1, others: vec![2] }, MergeGroup { keeper: 3, others: vec![2] }];
        assert!(check_disjoint(&overlap).unwrap_err().to_string().contains("more than one group"));
        assert!(check_disjoint(&[MergeGroup { keeper: 1, others: vec![2] }, MergeGroup { keeper: 3, others: vec![4] }]).is_ok());
    }

    #[test]
    fn merging_moves_cases_in_place_keeps_best_stats_and_removes_rows() {
        let (mut lib, dirs, t) = library_with_tracks("dx-merge", &[100, 100, 100, 50]);
        let (a, b, c) = (lib.create_case("Case A").unwrap(), lib.create_case("Case B").unwrap(), lib.create_case("Case C").unwrap());
        lib.set_case_tracks(a, &[t[3], t[1], t[2]]).unwrap();
        lib.set_case_tracks(b, &[t[0], t[1]]).unwrap();
        lib.set_case_tracks(c, &[t[1]]).unwrap();
        lib.set_track_rating(t[0], 2).unwrap();
        lib.set_track_rating(t[1], 5).unwrap();
        lib.con.execute("UPDATE tracks SET times_played = 3, last_played = 100 WHERE track_id = ?1", [t[0]]).unwrap();
        lib.con.execute("UPDATE tracks SET times_played = 7, last_played = 900 WHERE track_id = ?1", [t[1]]).unwrap();
        let group = MergeGroup { keeper: t[0], others: vec![t[1], t[2]] };
        let o = MergeOptions { remove_rows: true, ..Default::default() };

        let preview = lib.merge_plan_for(std::slice::from_ref(&group), &o).unwrap();
        let before = lib.dup_rows().unwrap();
        assert_eq!(before.len(), 4, "a preview writes nothing");

        let done = lib.merge_duplicates(&group, &o, Some(&preview[0])).unwrap();
        assert_eq!(done, preview[0], "the write does what the preview said");
        // A: the keeper took copy 1's place (second); copy 2 left; B: keeper was there; C: keeper replaced the copy
        assert_eq!(lib.case_track_ids(a).unwrap(), vec![t[3], t[0]]);
        assert_eq!(lib.case_track_ids(b).unwrap(), vec![t[0]]);
        assert_eq!(lib.case_track_ids(c).unwrap(), vec![t[0]]);
        let rows = lib.dup_rows().unwrap();
        assert_eq!(rows.iter().map(|r| r.track_id).collect::<Vec<_>>(), vec![t[0], t[3]]);
        let k = &rows[0];
        assert_eq!((k.rating, k.times_played, k.last_played), (5, 7, 900));
        assert_eq!(lib.case_info(a).unwrap().unwrap().play_time_secs, 150, "case sums follow the membership");
        assert_eq!(lib.case_info(c).unwrap().unwrap().track_count, 1);
        assert!(lib.compatibility_problems().unwrap().is_empty());
        cleanup(lib, dirs);
    }

    #[test]
    fn a_changed_library_or_a_mix_stops_the_write_and_changes_nothing() {
        let (mut lib, dirs, t) = library_with_tracks("dx-stale", &[100, 100]);
        let a = lib.create_case("Case A").unwrap();
        lib.set_case_tracks(a, &[t[1]]).unwrap();
        let group = MergeGroup { keeper: t[0], others: vec![t[1]] };
        let o = MergeOptions { remove_rows: true, ..Default::default() };
        let preview = lib.merge_plan_for(std::slice::from_ref(&group), &o).unwrap();
        // the user rates the copy after the preview: the stats part of the plan no longer matches
        lib.set_track_rating(t[1], 4).unwrap();
        let err = lib.merge_duplicates(&group, &o, Some(&preview[0])).unwrap_err();
        assert!(err.to_string().contains("changed after the preview"), "{err}");
        assert_eq!(lib.dup_rows().unwrap().len(), 2);
        assert_eq!(lib.case_track_ids(a).unwrap(), vec![t[1]]);
        // a mix using the copy keeps its row, and the merge says so
        lib.con.execute("INSERT INTO mixes (name) VALUES ('Sample Mix')", []).unwrap();
        let mix = lib.con.last_insert_rowid();
        lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", params![mix, t[1]]).unwrap();
        let done = lib.merge_duplicates(&group, &o, None).unwrap();
        assert_eq!(done.blocked, vec![Blocked { track_id: t[1], mixes: vec!["Sample Mix".into()] }]);
        assert_eq!(lib.dup_rows().unwrap().len(), 2, "the copy stays");
        assert_eq!(lib.case_track_ids(a).unwrap(), vec![t[0]], "but its place in the case went to the keeper");
        assert_eq!(lib.mix_names_using(&[t[0], t[1]]).unwrap().get(&t[1]).unwrap(), &vec!["Sample Mix".to_string()]);
        cleanup(lib, dirs);
    }

    #[test]
    fn cases_holding_lists_each_case_once_with_the_place_in_it() {
        let (mut lib, dirs, t) = library_with_tracks("dx-holds", &[10, 20, 30]);
        let b = lib.create_case("beta").unwrap();
        let a = lib.create_case("Alpha").unwrap();
        lib.set_case_tracks(b, &[t[0], t[1]]).unwrap();
        lib.set_case_tracks(a, &[t[2], t[1]]).unwrap();
        // a repeat row and a row for a track that is gone take no extra place
        lib.con.execute("INSERT INTO casetracks (case_id, track_id) VALUES (?1, ?2)", params![a, t[1]]).unwrap();
        lib.con.execute("INSERT INTO casetracks (case_id, track_id) VALUES (?1, 9999)", params![a]).unwrap();
        let h = lib.cases_holding(&[t[1], 424242, t[0], t[1]]).unwrap();
        assert_eq!(h.iter().map(|x| x.track_id).collect::<Vec<_>>(), vec![t[1], t[0]], "asked order, unknown and repeated ids dropped");
        assert_eq!(
            h[0].cases,
            vec![
                CaseHold { case_id: a, name: "Alpha".into(), position: 2, of: 2 },
                CaseHold { case_id: b, name: "beta".into(), position: 2, of: 2 },
            ]
        );
        assert_eq!(h[1].cases, vec![CaseHold { case_id: b, name: "beta".into(), position: 1, of: 2 }]);
        assert!(lib.cases_holding(&[]).unwrap().is_empty());
        assert_eq!(lib.cases_holding(&[t[2]]).unwrap()[0].cases.len(), 1);
        let none = lib.cases_holding(&[t[2]]).unwrap();
        assert_eq!(none[0].cases[0].position, 1);
        cleanup(lib, dirs);
    }

    #[test]
    fn dup_rows_read_every_column_and_default_nulls() {
        let (lib, dirs, t) = library_with_tracks("dx-rows", &[111]);
        lib.con
            .execute(
                "UPDATE tracks SET artist = 'Synthetic Artist', year = 2001, label = 'Synthetic Label', bit_rate = 256, sample_rate = 44100 WHERE track_id = ?1",
                [t[0]],
            )
            .unwrap();
        let r = &lib.dup_rows().unwrap()[0];
        assert_eq!((r.artist.as_str(), r.year, r.label.as_str(), r.bit_rate_kbps, r.sample_rate, r.play_time_secs), ("Synthetic Artist", 2001, "Synthetic Label", 256, 44100, 111));
        assert!(r.analysed && r.file_size > 0 && r.date_added > 0 && r.location.starts_with("/pmdb_tracks/"));
        lib.con.execute("UPDATE tracks SET title = NULL, bit_rate = NULL, last_played = NULL, comments = NULL WHERE track_id = ?1", [t[0]]).unwrap();
        let r = &lib.dup_rows().unwrap()[0];
        assert_eq!((r.title.as_str(), r.bit_rate_kbps, r.last_played, r.comments.as_str()), ("", 0, -1, ""));
        cleanup(lib, dirs);
    }
}
