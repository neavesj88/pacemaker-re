//! Case operations that touch several cases at once: duplicate, merge, copy / move tracks
//! between cases and delete several cases. Each one runs in a single transaction, so a failure
//! leaves the library as it was, and each has a read-only plan (`merge_plan`, `transfer_plan`)
//! built by the same pure function the write uses (`plan_merge`, `plan_transfer`), so what a
//! confirmation dialog previews is what the write then does.
//!
//! The rows written are the ones the Editor's own case operations write (`create_case`,
//! `add_case_tracks`): `date_modified` and `play_time_secs` are refreshed on every case whose
//! membership changed. The device has no nesting of cases (the `cases` table has no parent
//! column, see notes/schema-14142.sql), so there are no folders here.

use std::collections::HashSet;

use serde::Serialize;

use super::*;

// ---- reading -------------------------------------------------------------------------------

/// A case's members in case order, each once, split into those that are still tracks and the
/// number of rows that point at a track that no longer exists.
struct Members {
    live: Vec<i64>,
    stale: usize,
}

fn read_members(con: &Connection, case_id: i64) -> Result<Members, Error> {
    let mut st = con.prepare(
        "SELECT ct.track_id, t.track_id IS NOT NULL
           FROM casetracks ct LEFT JOIN tracks t ON t.track_id = ct.track_id AND t.track_id >= 0
          WHERE ct.case_id = ?1 ORDER BY ct.rowid",
    )?;
    let rows = st.query_map([case_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?)))?;
    let mut seen = HashSet::new();
    let mut live = Vec::new();
    let mut stale = 0;
    for row in rows {
        let (id, exists) = row?;
        if !seen.insert(id) {
            continue; // the table has no unique constraint; a repeat is one member
        }
        if exists {
            live.push(id);
        } else {
            stale += 1;
        }
    }
    Ok(Members { live, stale })
}

fn dedup(ids: &[i64]) -> Vec<i64> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|i| seen.insert(*i)).collect()
}

fn track_exists(con: &Connection, id: i64) -> Result<bool, Error> {
    Ok(con.query_row("SELECT 1 FROM tracks WHERE track_id = ?1 AND track_id >= 0", [id], |_| Ok(())).optional()?.is_some())
}

fn append_members(con: &Connection, case_id: i64, ids: &[i64]) -> Result<(), Error> {
    let mut ins = con.prepare("INSERT INTO casetracks (case_id, track_id) VALUES (?1, ?2)")?;
    for id in ids {
        ins.execute(params![case_id, id])?;
    }
    Ok(())
}

fn delete_case_rows(con: &Connection, case_id: i64) -> Result<bool, Error> {
    con.execute("DELETE FROM casetracks WHERE case_id = ?1", [case_id])?;
    Ok(con.execute("DELETE FROM cases WHERE case_id = ?1 AND case_id >= 0", [case_id])? > 0)
}

// ---- merge ---------------------------------------------------------------------------------

/// What a merge will do. Counts are in tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergePlan {
    /// The tracks appended to the destination, in this order: the first source's tracks in its
    /// order, then the next source's tracks it does not have yet, and so on.
    pub added: Vec<i64>,
    /// Source tracks the destination already holds (an existing destination only).
    pub already_there: usize,
    /// Source tracks left out because an earlier source already supplied them.
    pub repeated: usize,
    /// Rows of the sources that point at a track no longer in the library; they are not copied.
    pub stale: usize,
}

/// The order of a merge. `sources` are the member lists of the cases being merged, in the order
/// the user listed them; `target` is what the destination holds already (empty for a new case).
pub fn plan_merge(sources: &[Vec<i64>], target: &[i64]) -> MergePlan {
    let mut seen: HashSet<i64> = target.iter().copied().collect();
    let mut added = Vec::new();
    let (mut already_there, mut repeated) = (0, 0);
    let in_target: HashSet<i64> = seen.clone();
    for id in sources.iter().flatten() {
        if seen.insert(*id) {
            added.push(*id);
        } else if in_target.contains(id) {
            already_there += 1;
        } else {
            repeated += 1;
        }
    }
    MergePlan { added, already_there, repeated, stale: 0 }
}

/// Where a merge puts its tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeDest {
    /// A new case with this name (empty metadata, like "New case").
    New(String),
    /// An existing case, which keeps its own tracks and order; the merged ones are appended.
    Existing(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergeDone {
    /// The destination case.
    pub case_id: i64,
    /// Tracks appended to it.
    pub added: usize,
    /// Source cases deleted afterwards (0 unless asked for).
    pub deleted_sources: usize,
}

fn merge_inputs(con: &Connection, sources: &[i64], target: Option<i64>) -> Result<(Vec<i64>, MergePlan), Error> {
    let sources = dedup(sources);
    if sources.is_empty() {
        return Err(Error::Invalid("pick at least one case to merge".into()));
    }
    if let Some(t) = target {
        require_case(con, t)?;
        if sources.contains(&t) {
            return Err(Error::Invalid("a case cannot be merged into itself".into()));
        }
    }
    let mut lists = Vec::new();
    let mut stale = 0;
    for s in &sources {
        require_case(con, *s)?;
        let m = read_members(con, *s)?;
        stale += m.stale;
        lists.push(m.live);
    }
    let existing = match target {
        Some(t) => read_members(con, t)?.live,
        None => Vec::new(),
    };
    let mut plan = plan_merge(&lists, &existing);
    plan.stale = stale;
    Ok((sources, plan))
}

// ---- copy / move ---------------------------------------------------------------------------

/// What copying or moving a selection of tracks will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferPlan {
    /// Appended to the destination, in selection order.
    pub to_add: Vec<i64>,
    /// Selected tracks the destination already holds.
    pub already_there: Vec<i64>,
    /// Removed from the source (a move only): every selected track the source holds, including
    /// those the destination already had.
    pub to_remove: Vec<i64>,
    /// Selected tracks the source does not hold (left alone).
    pub not_in_source: Vec<i64>,
    /// Selected ids that are not tracks of the library (left alone).
    pub missing: Vec<i64>,
}

/// The pure part of a copy / move. `selected` may repeat ids. `source` is the case the tracks
/// come from (`None` when they come from the library, which only a copy allows); `is_track`
/// says whether an id is a real track.
pub fn plan_transfer(selected: &[i64], source: Option<&[i64]>, dest: &[i64], remove_from_source: bool, is_track: impl Fn(i64) -> bool) -> TransferPlan {
    let dest: HashSet<i64> = dest.iter().copied().collect();
    let source: Option<HashSet<i64>> = source.map(|s| s.iter().copied().collect());
    let mut plan = TransferPlan { to_add: vec![], already_there: vec![], to_remove: vec![], not_in_source: vec![], missing: vec![] };
    for id in dedup(selected) {
        if !is_track(id) {
            plan.missing.push(id);
            continue;
        }
        if source.as_ref().is_some_and(|s| !s.contains(&id)) {
            plan.not_in_source.push(id);
            continue;
        }
        if dest.contains(&id) {
            plan.already_there.push(id);
        } else {
            plan.to_add.push(id);
        }
        if remove_from_source && source.is_some() {
            plan.to_remove.push(id);
        }
    }
    plan
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferDone {
    pub added: usize,
    pub removed: usize,
}

fn transfer_inputs(con: &Connection, from: Option<i64>, to: i64, ids: &[i64], remove_from_source: bool) -> Result<TransferPlan, Error> {
    require_case(con, to)?;
    if remove_from_source && from.is_none() {
        return Err(Error::Invalid("a move needs the case the tracks come from".into()));
    }
    if from == Some(to) {
        return Err(Error::Invalid("the tracks are already in that case".into()));
    }
    let source = match from {
        Some(f) => {
            require_case(con, f)?;
            Some(read_members(con, f)?.live)
        }
        None => None,
    };
    let dest = read_members(con, to)?.live;
    let mut known = HashSet::new();
    for id in dedup(ids) {
        if track_exists(con, id)? {
            known.insert(id);
        }
    }
    Ok(plan_transfer(ids, source.as_deref(), &dest, remove_from_source, |i| known.contains(&i)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferNewDone {
    pub case_id: i64,
    pub added: usize,
    pub removed: usize,
}

/// The write half of a transfer, inside the caller's transaction.
fn transfer_in(tx: &Connection, from: Option<i64>, to: i64, ids: &[i64], remove_from_source: bool) -> Result<TransferDone, Error> {
    let plan = transfer_inputs(tx, from, to, ids, remove_from_source)?;
    append_members(tx, to, &plan.to_add)?;
    if !plan.to_add.is_empty() {
        refresh_case_derived(tx, to)?;
    }
    if let (Some(f), false) = (from, plan.to_remove.is_empty()) {
        for id in &plan.to_remove {
            tx.execute("DELETE FROM casetracks WHERE case_id = ?1 AND track_id = ?2", params![f, id])?;
        }
        refresh_case_derived(tx, f)?;
    }
    Ok(TransferDone { added: plan.to_add.len(), removed: plan.to_remove.len() })
}

// ---- the Library operations ----------------------------------------------------------------

impl Library {
    /// A copy of a case under `name`: the same tracks in the same order and the same genre,
    /// year, rating, producer, remixer and comments. Play statistics start at zero and the
    /// dates are now. Tracks that are no longer in the library are not copied. Returns the new
    /// case id.
    pub fn duplicate_case(&mut self, case_id: i64, name: &str) -> Result<i64, Error> {
        check_case_name(name)?;
        let tx = self.con.transaction()?;
        require_case(&tx, case_id)?;
        let members = read_members(&tx, case_id)?;
        let new_id = insert_case_row(&tx, name)?;
        let src = tx
            .query_row(&format!("{CASE_INFO_SELECT} WHERE c.case_id = ?1"), [case_id], case_info_row)?;
        tx.execute(
            "UPDATE cases SET genre = ?1, year = ?2, rating = ?3, producer = ?4, remixer = ?5, comments = ?6 WHERE case_id = ?7",
            params![src.genre, src.year, src.rating, src.producer, src.remixer, src.comments, new_id],
        )?;
        append_members(&tx, new_id, &members.live)?;
        refresh_case_derived(&tx, new_id)?;
        tx.commit()?;
        Ok(new_id)
    }

    /// What [`Library::merge_cases`] would do with the same arguments. Writes nothing.
    pub fn merge_plan(&self, sources: &[i64], target: Option<i64>) -> Result<MergePlan, Error> {
        Ok(merge_inputs(&self.con, sources, target)?.1)
    }

    /// Merges `sources` into a new or an existing case without repeats (see [`plan_merge`] for
    /// the order). With `delete_sources` the source cases are deleted afterwards, in the same
    /// transaction; their tracks stay in the library. A source cannot be its own destination.
    pub fn merge_cases(&mut self, sources: &[i64], dest: &MergeDest, delete_sources: bool) -> Result<MergeDone, Error> {
        let tx = self.con.transaction()?;
        let target = match dest {
            MergeDest::Existing(id) => Some(*id),
            MergeDest::New(name) => {
                check_case_name(name)?;
                None
            }
        };
        let (sources, plan) = merge_inputs(&tx, sources, target)?;
        let case_id = match dest {
            MergeDest::Existing(id) => *id,
            MergeDest::New(name) => insert_case_row(&tx, name)?,
        };
        append_members(&tx, case_id, &plan.added)?;
        if !plan.added.is_empty() || target.is_none() {
            refresh_case_derived(&tx, case_id)?;
        }
        let mut deleted_sources = 0;
        if delete_sources {
            for s in &sources {
                if delete_case_rows(&tx, *s)? {
                    deleted_sources += 1;
                }
            }
        }
        tx.commit()?;
        Ok(MergeDone { case_id, added: plan.added.len(), deleted_sources })
    }

    /// What [`Library::transfer_tracks`] would do with the same arguments. Writes nothing.
    pub fn transfer_plan(&self, from: Option<i64>, to: i64, ids: &[i64], remove_from_source: bool) -> Result<TransferPlan, Error> {
        transfer_inputs(&self.con, from, to, ids, remove_from_source)
    }

    /// Copies (`remove_from_source` false) or moves (true) tracks into case `to`, appended at
    /// the end and skipping any it holds. `from` is the case they come from; a copy may leave
    /// it `None` (tracks picked from the library). A move removes the tracks from `from` even
    /// when `to` already held them. Both cases' `date_modified` and `play_time_secs` follow.
    pub fn transfer_tracks(&mut self, from: Option<i64>, to: i64, ids: &[i64], remove_from_source: bool) -> Result<TransferDone, Error> {
        let tx = self.con.transaction()?;
        let done = transfer_in(&tx, from, to, ids, remove_from_source)?;
        tx.commit()?;
        Ok(done)
    }

    /// Like [`Library::transfer_tracks`] into a case created here under `name`, in one
    /// transaction: if the transfer fails no empty case is left behind. Returns the new case's
    /// id and what was transferred.
    pub fn transfer_to_new_case(&mut self, from: Option<i64>, name: &str, ids: &[i64], remove_from_source: bool) -> Result<TransferNewDone, Error> {
        check_case_name(name)?;
        let tx = self.con.transaction()?;
        let case_id = insert_case_row(&tx, name)?;
        let done = transfer_in(&tx, from, case_id, ids, remove_from_source)?;
        refresh_case_derived(&tx, case_id)?;
        tx.commit()?;
        Ok(TransferNewDone { case_id, added: done.added, removed: done.removed })
    }

    /// Deletes several cases and their membership rows in one transaction (the tracks stay).
    /// Returns how many existed and went; ids that are not cases are skipped.
    pub fn delete_cases(&mut self, case_ids: &[i64]) -> Result<usize, Error> {
        let tx = self.con.transaction()?;
        let mut gone = 0;
        for id in dedup(case_ids) {
            if delete_case_rows(&tx, id)? {
                gone += 1;
            }
        }
        tx.commit()?;
        Ok(gone)
    }
}
