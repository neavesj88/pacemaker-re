//! Library read operations for play history and statistics (owner: history area).
//!
//! The device keeps only counters: `tracks.times_played` and `tracks.last_played` (Unix ms, -1
//! for never), the same pair on `cases` and `mixes` (there 0 for never). It does not log
//! individual plays, so a "session" here is reconstructed: tracks sorted by their last play and
//! cut wherever two neighbours are further apart than a gap. A track played in several sessions
//! therefore shows only in the latest one.
use crate::library::{Error, Library};

/// Default gap that ends a session: four hours without a track starting.
pub const DEFAULT_SESSION_GAP_MS: i64 = 4 * 60 * 60 * 1000;

/// The play counters of one track.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlayEntry {
    pub track_id: i64,
    pub times_played: i64,
    /// Unix ms of the last play, -1 or 0 when never played.
    pub last_played: i64,
}

impl PlayEntry {
    /// A track counts as played if either counter says so (a device that bumped one but not the
    /// other is still evidence of a play).
    pub fn was_played(&self) -> bool {
        self.times_played > 0 || self.last_played > 0
    }
}

/// A case or a mix with its play counters.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlayedCollection {
    pub id: i64,
    pub name: String,
    pub times_played: i64,
    pub last_played: i64,
    pub track_count: i64,
}

/// Tracks whose last plays are close together, in play order (oldest first).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlaySession {
    /// Unix ms of the first and last track's last play.
    pub start: i64,
    pub end: i64,
    pub track_ids: Vec<i64>,
    /// Unix ms of each track's last play, parallel to `track_ids`.
    pub played_at: Vec<i64>,
}

/// The play counters of one track together with its stable identity. `global_id` is the random
/// id the Editor assigns at import (empty when the row has none); unlike `track_id` it survives
/// the track being re-numbered, and it differs between two cards, so the app-side play log keys on it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlayCounter {
    pub track_id: i64,
    pub global_id: String,
    pub times_played: i64,
    /// Unix ms of the last play, -1 when never played.
    pub last_played: i64,
}

/// Raw counters read from the library.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayStats {
    pub tracks: Vec<PlayEntry>,
    pub cases: Vec<PlayedCollection>,
    pub mixes: Vec<PlayedCollection>,
}

/// Splits played entries into sessions, newest session first, each in play order. Entries that
/// were never played (or have no usable timestamp) are left out. `gap_ms` <= 0 puts every track
/// in a session of its own. Ties on the timestamp keep ascending `track_id` order.
pub fn group_sessions(entries: &[PlayEntry], gap_ms: i64) -> Vec<PlaySession> {
    let mut timed: Vec<(i64, i64)> = entries.iter().filter(|e| e.last_played > 0).map(|e| (e.last_played, e.track_id)).collect();
    timed.sort_unstable();
    let mut out: Vec<PlaySession> = Vec::new();
    for (t, id) in timed {
        match out.last_mut() {
            Some(s) if gap_ms > 0 && t - s.end <= gap_ms => {
                s.end = t;
                s.track_ids.push(id);
                s.played_at.push(t);
            }
            _ => out.push(PlaySession { start: t, end: t, track_ids: vec![id], played_at: vec![t] }),
        }
    }
    out.reverse();
    out
}

/// The `limit` most played entries: highest `times_played` first, then most recent, then lowest
/// id. Never-played entries are excluded.
pub fn most_played(entries: &[PlayEntry], limit: usize) -> Vec<PlayEntry> {
    let mut v: Vec<PlayEntry> = entries.iter().filter(|e| e.times_played > 0).cloned().collect();
    v.sort_by(|a, b| b.times_played.cmp(&a.times_played).then(b.last_played.cmp(&a.last_played)).then(a.track_id.cmp(&b.track_id)));
    v.truncate(limit);
    v
}

/// Ids of entries never played, ascending.
pub fn never_played(entries: &[PlayEntry]) -> Vec<i64> {
    let mut v: Vec<i64> = entries.iter().filter(|e| !e.was_played()).map(|e| e.track_id).collect();
    v.sort_unstable();
    v
}

impl Library {
    /// Play counters of every track, case and mix. Read-only.
    pub fn play_stats(&self) -> Result<PlayStats, Error> {
        let mut st = self
            .con
            .prepare("SELECT track_id, IFNULL(times_played,0), IFNULL(last_played,-1) FROM tracks WHERE track_id >= 0 ORDER BY track_id")?;
        let tracks = st
            .query_map([], |r| Ok(PlayEntry { track_id: r.get(0)?, times_played: r.get(1)?, last_played: r.get(2)? }))?
            .collect::<Result<_, _>>()?;
        Ok(PlayStats {
            tracks,
            cases: self.played_collections(
                "SELECT c.case_id, IFNULL(c.name,''), IFNULL(c.times_played,0), IFNULL(c.last_played,0),
                        (SELECT COUNT(*) FROM casetracks t WHERE t.case_id = c.case_id)
                 FROM cases c WHERE c.case_id >= 0 ORDER BY c.case_id",
            )?,
            mixes: self.played_collections(
                "SELECT m.mix_id, IFNULL(m.name,''), IFNULL(m.times_played,0), IFNULL(m.last_played,0),
                        (SELECT COUNT(*) FROM mixtracks t WHERE t.mix_id = m.mix_id)
                 FROM mixes m WHERE m.mix_id >= 0 ORDER BY m.mix_id",
            )?,
        })
    }

    /// Counters plus identity of every track, in `track_id` order. Read-only; the input of the
    /// app-side play log.
    pub fn play_counters(&self) -> Result<Vec<PlayCounter>, Error> {
        let mut st = self.con.prepare(
            "SELECT track_id, IFNULL(global_id,''), IFNULL(times_played,0), IFNULL(last_played,-1) FROM tracks WHERE track_id >= 0 ORDER BY track_id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(PlayCounter { track_id: r.get(0)?, global_id: r.get(1)?, times_played: r.get(2)?, last_played: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn played_collections(&self, sql: &str) -> Result<Vec<PlayedCollection>, Error> {
        let mut st = self.con.prepare(sql)?;
        let rows = st.query_map([], |r| {
            Ok(PlayedCollection { id: r.get(0)?, name: r.get(1)?, times_played: r.get(2)?, last_played: r.get(3)?, track_count: r.get(4)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn e(track_id: i64, times_played: i64, last_played: i64) -> PlayEntry {
        PlayEntry { track_id, times_played, last_played }
    }
    const H: i64 = 3_600_000;
    const T0: i64 = 1_700_000_000_000;

    #[test]
    fn sessions_split_on_gap_newest_first_each_in_play_order() {
        let entries = [e(1, 1, T0 + 2 * H), e(2, 1, T0), e(3, 1, T0 + H), e(4, 1, T0 + 20 * H), e(5, 1, T0 + 21 * H)];
        let s = group_sessions(&entries, DEFAULT_SESSION_GAP_MS);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0], PlaySession { start: T0 + 20 * H, end: T0 + 21 * H, track_ids: vec![4, 5], played_at: vec![T0 + 20 * H, T0 + 21 * H] });
        assert_eq!(s[1], PlaySession { start: T0, end: T0 + 2 * H, track_ids: vec![2, 3, 1], played_at: vec![T0, T0 + H, T0 + 2 * H] });
    }

    #[test]
    fn gap_boundary_is_inclusive_and_zero_gap_splits_everything() {
        let entries = [e(1, 1, T0), e(2, 1, T0 + 4 * H), e(3, 1, T0 + 8 * H + 1)];
        let s = group_sessions(&entries, 4 * H);
        assert_eq!(s.iter().map(|x| x.track_ids.clone()).collect::<Vec<_>>(), vec![vec![3], vec![1, 2]]);
        assert_eq!(group_sessions(&entries, 0).len(), 3);
    }

    #[test]
    fn never_played_and_untimed_entries_are_not_in_sessions() {
        let entries = [e(1, 0, -1), e(2, 3, -1), e(3, 0, 0), e(4, 1, T0)];
        let s = group_sessions(&entries, DEFAULT_SESSION_GAP_MS);
        assert_eq!(s, vec![PlaySession { start: T0, end: T0, track_ids: vec![4], played_at: vec![T0] }]);
        assert!(group_sessions(&[], DEFAULT_SESSION_GAP_MS).is_empty());
        assert!(group_sessions(&[e(1, 0, -1)], DEFAULT_SESSION_GAP_MS).is_empty());
    }

    #[test]
    fn equal_timestamps_order_by_id() {
        let s = group_sessions(&[e(9, 1, T0), e(2, 1, T0)], DEFAULT_SESSION_GAP_MS);
        assert_eq!(s[0].track_ids, vec![2, 9]);
    }

    #[test]
    fn ranking_and_never_played() {
        let entries = [e(1, 2, T0), e(2, 5, T0), e(3, 5, T0 + H), e(4, 0, -1), e(5, 0, T0), e(6, 0, 0)];
        let top = most_played(&entries, 10);
        assert_eq!(top.iter().map(|x| x.track_id).collect::<Vec<_>>(), vec![3, 2, 1]);
        assert_eq!(most_played(&entries, 2).len(), 2);
        // 5 has a timestamp but no count: played; 4 and 6 are not
        assert_eq!(never_played(&entries), vec![4, 6]);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-hist-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn lib_with_tracks(tag: &str, n: usize) -> (Library, PathBuf, Vec<i64>) {
        let vol = temp_dir(tag);
        let lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            lib.con
                .execute("INSERT INTO tracks (title, play_time_secs, times_played, last_played) VALUES (?1, 100, 0, -1)", [format!("t{i}")])
                .unwrap();
            ids.push(lib.con.last_insert_rowid());
        }
        (lib, vol, ids)
    }

    #[test]
    fn play_stats_reads_counters_for_tracks_cases_and_mixes() {
        let (mut lib, vol, ids) = lib_with_tracks("stats", 3);
        let before = lib.play_stats().unwrap();
        assert!(before.tracks.iter().all(|t| !t.was_played()), "a fresh library has no plays");
        assert_eq!(never_played(&before.tracks), ids);

        lib.con.execute("UPDATE tracks SET times_played = 4, last_played = ?1 WHERE track_id = ?2", rusqlite::params![T0, ids[1]]).unwrap();
        let case = lib.create_case("Synthetic case").unwrap();
        lib.set_case_tracks(case, &ids[..2]).unwrap();
        lib.con.execute("UPDATE cases SET times_played = 2, last_played = ?1 WHERE case_id = ?2", rusqlite::params![T0, case]).unwrap();
        let mix = lib.create_mix("Synthetic mix").unwrap();

        let s = lib.play_stats().unwrap();
        assert_eq!(s.tracks[1], e(ids[1], 4, T0));
        assert_eq!(s.cases, vec![PlayedCollection { id: case, name: "Synthetic case".into(), times_played: 2, last_played: T0, track_count: 2 }]);
        assert_eq!(s.mixes.len(), 1);
        assert_eq!((s.mixes[0].id, s.mixes[0].times_played), (mix, 0));
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
    }

    #[test]
    fn play_counters_carry_global_id_and_tolerate_nulls() {
        let (lib, vol, ids) = lib_with_tracks("counters", 2);
        lib.con.execute("UPDATE tracks SET global_id = 'abc123', times_played = 3, last_played = ?1 WHERE track_id = ?2", rusqlite::params![T0, ids[0]]).unwrap();
        lib.con.execute("UPDATE tracks SET global_id = NULL, times_played = NULL, last_played = NULL WHERE track_id = ?1", [ids[1]]).unwrap();
        let c = lib.play_counters().unwrap();
        assert_eq!(c[0], PlayCounter { track_id: ids[0], global_id: "abc123".into(), times_played: 3, last_played: T0 });
        assert_eq!(c[1], PlayCounter { track_id: ids[1], global_id: String::new(), times_played: 0, last_played: -1 });
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
    }

    #[test]
    fn create_case_with_tracks_keeps_order_and_rolls_back_on_bad_id() {
        let (mut lib, vol, ids) = lib_with_tracks("case", 3);
        let id = lib.create_case_with_tracks("From a session", &[ids[2], ids[0], ids[2]]).unwrap();
        assert_eq!(lib.case_track_ids(id).unwrap(), vec![ids[2], ids[0]]);
        assert_eq!(lib.cases().unwrap().len(), 1);

        assert!(lib.create_case_with_tracks("Bad", &[ids[0], 9999]).is_err());
        assert!(lib.create_case_with_tracks("  ", &[ids[0]]).is_err());
        assert_eq!(lib.cases().unwrap().len(), 1, "failed saves leave no case behind");
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
    }
}
