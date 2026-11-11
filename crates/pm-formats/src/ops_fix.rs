//! Health-card fixes (owner: device area): what could be repaired in a library and the one
//! repair that edits `music.db` itself, removing the rows of tracks whose audio file is gone.
//!
//! Every path here is built from `tracks.location` through [`checked_track_paths`], which refuses
//! anything that is not plain names under `.Pacemaker/Music` (a stored location is data read
//! from a removable drive, not trusted input). The plan ([`plan_fixes`]) and the repair
//! ([`remove_dead_rows`]) use the same classification, and the repair re-plans on the fresh
//! database and only ever acts on the intersection of what was asked and what the fresh plan
//! still finds, so a stale list cannot remove a row whose audio has come back.
//!
//! Re-analysing missing beat grids is not here: it is the existing `reanalyse` job; this module
//! only says which tracks need it and why.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::params;
use serde::Serialize;

use crate::library::{Error, Library, Track};
use crate::naming;
use crate::ops_device::music_dir;
use crate::strfile;

/// The audio file and `.str` sidecar for a stored `tracks.location`, or `None` unless the
/// location is `/pmdb_tracks/` followed by plain names only. Colons, backslashes and NULs are
/// refused on every platform (they would turn the path absolute or drive-relative on Windows),
/// as are empty, `.` and `..` parts, so the result always lies inside `.Pacemaker/Music`.
pub fn checked_track_paths(root: &Path, location: &str) -> Option<(PathBuf, PathBuf)> {
    let rel = location.strip_prefix(naming::LOCATION_PREFIX)?;
    let plain = !rel.is_empty() && !rel.contains([':', '\\', '\0']) && rel.split('/').all(|c| !c.is_empty() && c != "." && c != "..");
    if !plain {
        return None;
    }
    let audio = music_dir(root).join(rel);
    let sidecar = PathBuf::from(naming::str_path(&audio.to_string_lossy()));
    Some((audio, sidecar))
}

/// "Artist - Title", the title, the artist, or the file name, whichever exist.
pub(crate) fn track_label(artist: &str, title: &str, location: &str) -> String {
    match (artist.is_empty(), title.is_empty()) {
        (false, false) => format!("{artist} - {title}"),
        (_, false) => title.to_string(),
        (false, true) => artist.to_string(),
        _ => location.rsplit('/').next().unwrap_or(location).to_string(),
    }
}

fn label_of(t: &Track) -> String {
    track_label(&t.artist, &t.title, &t.location)
}

/// A track whose audio file is not on the card.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeadRow {
    pub track_id: i64,
    pub label: String,
    pub location: String,
    /// Cases the track is in (its membership goes with the row).
    pub in_cases: Vec<String>,
    /// Mixes the track is in. A row used by a mix is never removed here.
    pub in_mixes: Vec<String>,
}

/// A track that has its audio but no usable beat grid.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GridFix {
    pub track_id: i64,
    pub label: String,
    pub reason: String,
}

/// A track the checks could not look at, because its stored location is not a plain path under
/// the Music folder.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Unchecked {
    pub track_id: i64,
    pub label: String,
    pub location: String,
}

/// Everything the Health card can offer to fix, with names.
#[derive(Debug, Clone, Serialize)]
pub struct FixPlan {
    /// False when `.Pacemaker/Music` is missing or not a folder: nothing can be judged dead then.
    pub music_folder_ok: bool,
    pub total_tracks: usize,
    /// Audio missing, in no mix: the rows a repair may remove.
    pub dead_rows: Vec<DeadRow>,
    /// Audio missing but in a mix: kept (the mix would lose a clip), listed so they are not lost sight of.
    pub kept_for_mixes: Vec<DeadRow>,
    pub unchecked: Vec<Unchecked>,
    /// Audio present, `.str` missing, empty, unreadable or never analysed: what re-analysis fixes.
    pub grids: Vec<GridFix>,
    /// The share of tracks (percent) above which a removal needs an extra acknowledgement.
    pub guard_percent: u32,
    /// More than `guard_percent` of the library looks dead: probably a card that is not fully
    /// mounted, not a library full of dead rows.
    pub mass: bool,
}

pub(crate) fn membership(lib: &Library, sql: &str) -> Result<BTreeMap<i64, Vec<String>>, Error> {
    let mut st = lib.con.prepare(sql)?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    let mut out: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (id, name) = row?;
        let v = out.entry(id).or_default();
        if !v.contains(&name) {
            v.push(name);
        }
    }
    Ok(out)
}

/// Why a track whose audio is present has no usable beat grid, or `None` if it is fine. The
/// reasons are the wording `pm_analysis::ingest::analysis_problem` uses.
fn grid_problem(sidecar: &Path, analysed: bool) -> Option<String> {
    match std::fs::read(sidecar) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some("no .str file".into()),
        Err(e) => Some(format!("cannot read .str: {e}")),
        Ok(b) if b.is_empty() => Some("empty .str file (0 bytes)".into()),
        Ok(b) => match strfile::decode(&String::from_utf8_lossy(&b)) {
            Err(e) => Some(format!("unreadable .str: {e:?}")),
            Ok(_) if !analysed => Some("never analysed".into()),
            Ok(_) => None,
        },
    }
}

/// Classifies every track. Read-only.
pub fn plan_fixes(lib: &Library, guard_percent: u32) -> Result<FixPlan, Error> {
    let root = lib.root();
    let tracks = lib.tracks()?;
    let music_folder_ok = music_dir(root).is_dir();
    let cases = membership(lib, "SELECT t.track_id, c.name FROM casetracks t JOIN cases c ON c.case_id = t.case_id WHERE c.case_id >= 0 ORDER BY c.name")?;
    let mixes = membership(lib, "SELECT t.track_id, m.name FROM mixtracks t JOIN mixes m ON m.mix_id = t.mix_id WHERE m.mix_id >= 0 ORDER BY m.name")?;
    let mut plan = FixPlan {
        music_folder_ok,
        total_tracks: tracks.len(),
        dead_rows: Vec::new(),
        kept_for_mixes: Vec::new(),
        unchecked: Vec::new(),
        grids: Vec::new(),
        guard_percent,
        mass: false,
    };
    for t in &tracks {
        let Some((audio, sidecar)) = checked_track_paths(root, &t.location) else {
            plan.unchecked.push(Unchecked { track_id: t.track_id, label: label_of(t), location: t.location.clone() });
            continue;
        };
        if audio.is_file() {
            if let Some(reason) = grid_problem(&sidecar, t.analysed) {
                plan.grids.push(GridFix { track_id: t.track_id, label: label_of(t), reason });
            }
        } else if music_folder_ok {
            let row = DeadRow {
                track_id: t.track_id,
                label: label_of(t),
                location: t.location.clone(),
                in_cases: cases.get(&t.track_id).cloned().unwrap_or_default(),
                in_mixes: mixes.get(&t.track_id).cloned().unwrap_or_default(),
            };
            if row.in_mixes.is_empty() {
                plan.dead_rows.push(row);
            } else {
                plan.kept_for_mixes.push(row);
            }
        }
    }
    let dead = plan.dead_rows.len() + plan.kept_for_mixes.len();
    plan.mass = !tracks.is_empty() && (dead as u64) * 100 > u64::from(guard_percent) * tracks.len() as u64;
    Ok(plan)
}

/// Deletes the library rows of the given tracks in one transaction: the track rows, their case
/// memberships and the `trackmap` rows that point at them (row conventions: see
/// [`remove_dead_rows`]). Each case that lost a member gets its `date_modified` and
/// `play_time_secs` refreshed. Returns how many cases were touched. Files and mix rows are not
/// touched: the caller decides whether a track may go and never passes one a mix uses.
pub(crate) fn delete_track_rows(con: &mut rusqlite::Connection, ids: &[i64]) -> Result<usize, Error> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let tx = con.transaction()?;
    let mut touched: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for id in ids {
        {
            let mut st = tx.prepare("SELECT DISTINCT case_id FROM casetracks WHERE track_id = ?1")?;
            let rows = st.query_map([id], |r| r.get::<_, i64>(0))?;
            for c in rows {
                touched.insert(c?);
            }
        }
        tx.execute("DELETE FROM casetracks WHERE track_id = ?1", [id])?;
        tx.execute("DELETE FROM trackmap WHERE int_id = ?1", [id])?;
        tx.execute("DELETE FROM tracks WHERE track_id = ?1 AND track_id >= 0", [id])?;
    }
    for c in &touched {
        tx.execute(
            "UPDATE cases SET date_modified = ?1,
                    play_time_secs = (SELECT IFNULL(SUM(t.play_time_secs), 0) FROM casetracks c
                                      JOIN tracks t ON t.track_id = c.track_id WHERE c.case_id = ?2)
             WHERE case_id = ?2",
            params![now, c],
        )?;
    }
    tx.commit()?;
    Ok(touched.len())
}

/// Outcome of [`remove_dead_rows`].
#[derive(Debug, Clone, Serialize, Default)]
pub struct RemoveReport {
    /// The rows removed, by name.
    pub removed: Vec<String>,
    /// Asked for but left alone, with the reason: audio is back, now in a mix, or not a track.
    pub skipped: Vec<String>,
    /// Cases that lost a member (their track count and play time were refreshed).
    pub cases_touched: usize,
}

/// Removes the `tracks` rows (and their case memberships) of the requested tracks, but only
/// those a fresh [`plan_fixes`] still lists as dead rows. Files are not touched: the track's
/// `.str` stays and shows up under "unused files" in the health scan. All or nothing, in one
/// transaction. Refuses when the plan says `mass` unless `acknowledge_mass` is true.
///
/// Row conventions: `casetracks` rows of the track are deleted and each affected case gets its
/// `date_modified` and `play_time_secs` refreshed the way a case edit does; `trackmap` rows that
/// map an external id to the track are deleted (an inference: they are keyed by the internal
/// id). A track in a mix is never removed: how the Editor's own removal reconciles the mix
/// document (it turns `mixtracks.track_id` negative and fills the `dup_*` columns, FINDINGS
/// section 3) is not reproduced.
pub fn remove_dead_rows(lib: &mut Library, requested: &[i64], guard_percent: u32, acknowledge_mass: bool) -> Result<RemoveReport, Error> {
    let plan = plan_fixes(lib, guard_percent)?;
    if plan.mass && !acknowledge_mass {
        return Err(Error::Invalid(format!(
            "{} of {} tracks look dead, more than {}% of the library. That usually means the card is not fully mounted, not that the library is full of dead rows. Nothing was removed.",
            plan.dead_rows.len() + plan.kept_for_mixes.len(),
            plan.total_tracks,
            plan.guard_percent
        )));
    }
    let mut report = RemoveReport::default();
    let mut doomed: Vec<&DeadRow> = Vec::new();
    for id in requested {
        if let Some(d) = plan.dead_rows.iter().find(|d| d.track_id == *id) {
            if !doomed.iter().any(|x| x.track_id == *id) {
                doomed.push(d);
            }
        } else if let Some(k) = plan.kept_for_mixes.iter().find(|d| d.track_id == *id) {
            report.skipped.push(format!("{}: kept because the mix {} uses it", k.label, k.in_mixes.join(", ")));
        } else {
            report.skipped.push(format!("track {id}: not a dead row any more (its audio is back, or it is gone)"));
        }
    }
    report.removed = doomed.iter().map(|d| d.label.clone()).collect();
    let ids: Vec<i64> = doomed.iter().map(|d| d.track_id).collect();
    report.cases_touched = delete_track_rows(&mut lib.con, &ids)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;

    struct Fx {
        base: PathBuf,
        lib: Library,
        ids: Vec<i64>,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn fixture(tag: &str, n: usize) -> Fx {
        let base = std::env::temp_dir().join(format!("pm-fix-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
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
            std::fs::write(&f, vec![0u8; 1500 + i]).unwrap();
            let meta = TrackMeta { title: format!("Title {i}"), artist: format!("Artist {i}"), format: "MP3".into(), play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        Fx { base, lib, ids }
    }

    fn kill_audio(fx: &Fx, id: i64) {
        let t = fx.lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap();
        let (audio, _) = checked_track_paths(fx.lib.root(), &t.location).unwrap();
        std::fs::remove_file(audio).unwrap();
    }

    #[test]
    fn only_plain_names_under_music_resolve() {
        let root = Path::new("/vol");
        for ok in ["/pmdb_tracks/6/8/722ef9a6.mp3", "/pmdb_tracks/a/b/c.flac"] {
            let (audio, sidecar) = checked_track_paths(root, ok).unwrap();
            assert!(audio.starts_with("/vol/.Pacemaker/Music"), "{audio:?}");
            assert!(sidecar.to_string_lossy().ends_with(".str"));
        }
        for bad in [
            "",
            "/pmdb_tracks/",
            "/pmdb_tracks/../x.mp3",
            "/pmdb_tracks/a/../../x.mp3",
            "/pmdb_tracks/./x.mp3",
            "/pmdb_tracks//x.mp3",
            "/pmdb_tracks/C:/x.mp3",
            "/pmdb_tracks/C:x.mp3",
            "/pmdb_tracks/a\\b.mp3",
            "/pmdb_tracks/a/b.mp3/",
            "/other/6/8/x.mp3",
            "pmdb_tracks/6/8/x.mp3",
            "/pmdb_tracks/a\0b.mp3",
        ] {
            assert!(checked_track_paths(root, bad).is_none(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn the_plan_separates_dead_rows_mix_kept_rows_grids_and_unchecked_locations() {
        let mut fx = fixture("plan", 6);
        let ids = fx.ids.clone();
        // 0: dead, in a case. 1: dead, in a mix. 2: has audio, grid deleted. 3: has audio, empty grid.
        // 4: bad location. 5: fine.
        kill_audio(&fx, ids[0]);
        kill_audio(&fx, ids[1]);
        fx.lib.create_case_with_tracks("Warm up", &[ids[0], ids[5]]).unwrap();
        let mix = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", params![mix, ids[1]]).unwrap();
        let tracks = fx.lib.tracks().unwrap();
        let sidecar_of = |i: usize| checked_track_paths(fx.lib.root(), &tracks[i].location).unwrap().1;
        std::fs::remove_file(sidecar_of(2)).unwrap();
        std::fs::write(sidecar_of(3), b"").unwrap();
        fx.lib.con.execute("UPDATE tracks SET location = '/pmdb_tracks/../../outside.mp3' WHERE track_id = ?1", [ids[4]]).unwrap();

        let plan = plan_fixes(&fx.lib, 50).unwrap();
        assert!(plan.music_folder_ok);
        assert_eq!(plan.total_tracks, 6);
        assert_eq!(plan.dead_rows.iter().map(|d| d.track_id).collect::<Vec<_>>(), [ids[0]]);
        assert_eq!(plan.dead_rows[0].in_cases, ["Warm up"]);
        assert_eq!(plan.kept_for_mixes.iter().map(|d| d.track_id).collect::<Vec<_>>(), [ids[1]]);
        assert_eq!(plan.kept_for_mixes[0].in_mixes, ["Late set"]);
        let grids: Vec<(i64, &str)> = plan.grids.iter().map(|g| (g.track_id, g.reason.as_str())).collect();
        assert_eq!(grids, [(ids[2], "no .str file"), (ids[3], "empty .str file (0 bytes)")]);
        assert_eq!(plan.unchecked.iter().map(|u| u.track_id).collect::<Vec<_>>(), [ids[4]]);
        assert!(!plan.mass, "2 of 6 is under 50%");
    }

    #[test]
    fn removing_dead_rows_takes_the_row_and_its_case_membership_and_nothing_else() {
        let mut fx = fixture("remove", 4);
        let ids = fx.ids.clone();
        kill_audio(&fx, ids[0]);
        kill_audio(&fx, ids[1]);
        let case = fx.lib.create_case_with_tracks("Warm up", &ids).unwrap();
        let mix = fx.lib.create_mix("Late set").unwrap();
        fx.lib.con.execute("INSERT INTO mixtracks (mix_id, track_id, track_index) VALUES (?1, ?2, 0)", params![mix, ids[1]]).unwrap();
        fx.lib.con.execute("INSERT INTO trackmap (ext_db, ext_id, int_id) VALUES ('x', 1, ?1)", [ids[0]]).unwrap();
        let before_time: i64 = fx.lib.con.query_row("SELECT play_time_secs FROM cases WHERE case_id = ?1", [case], |r| r.get(0)).unwrap();
        assert_eq!(before_time, 100 + 101 + 102 + 103);
        let tracks = fx.lib.tracks().unwrap();
        let sidecar0 = checked_track_paths(fx.lib.root(), &tracks[0].location).unwrap().1;

        // the mix-kept row and an unknown id are named in `skipped`, never removed
        let rep = remove_dead_rows(&mut fx.lib, &[ids[0], ids[1], 9999, ids[0]], 50, false).unwrap();
        assert_eq!(rep.removed, ["Artist 0 - Title 0"]);
        assert_eq!(rep.skipped.len(), 2, "{:?}", rep.skipped);
        assert!(rep.skipped[0].contains("Late set"), "{:?}", rep.skipped);
        assert_eq!(rep.cases_touched, 1);

        let left: Vec<i64> = fx.lib.tracks().unwrap().iter().map(|t| t.track_id).collect();
        assert_eq!(left, [ids[1], ids[2], ids[3]]);
        assert_eq!(fx.lib.case_track_ids(case).unwrap(), [ids[1], ids[2], ids[3]]);
        let after_time: i64 = fx.lib.con.query_row("SELECT play_time_secs FROM cases WHERE case_id = ?1", [case], |r| r.get(0)).unwrap();
        assert_eq!(after_time, 101 + 102 + 103, "the case's play time follows its members");
        let mapped: i64 = fx.lib.con.query_row("SELECT COUNT(*) FROM trackmap", [], |r| r.get(0)).unwrap();
        assert_eq!(mapped, 0);
        let in_mix: i64 = fx.lib.con.query_row("SELECT COUNT(*) FROM mixtracks WHERE track_id = ?1", [ids[1]], |r| r.get(0)).unwrap();
        assert_eq!(in_mix, 1, "the mix still has its clip");
        assert!(sidecar0.is_file(), "the beat grid file stays; the unused-files scan lists it");
    }

    #[test]
    fn a_row_whose_audio_came_back_is_not_removed() {
        let mut fx = fixture("back", 2);
        let id = fx.ids[0];
        let t = fx.lib.tracks().unwrap().remove(0);
        let (audio, _) = checked_track_paths(fx.lib.root(), &t.location).unwrap();
        let bytes = std::fs::read(&audio).unwrap();
        std::fs::remove_file(&audio).unwrap();
        let plan = plan_fixes(&fx.lib, 90).unwrap();
        assert_eq!(plan.dead_rows.len(), 1);
        // the user previewed the plan, then the card was replugged and the file is back
        std::fs::write(&audio, bytes).unwrap();
        let rep = remove_dead_rows(&mut fx.lib, &[id], 90, false).unwrap();
        assert!(rep.removed.is_empty() && rep.skipped.len() == 1);
        assert_eq!(fx.lib.tracks().unwrap().len(), 2);
    }

    #[test]
    fn a_card_that_looks_mostly_dead_needs_an_acknowledgement() {
        let mut fx = fixture("mass", 4);
        for i in 0..3 {
            kill_audio(&fx, fx.ids[i]);
        }
        let plan = plan_fixes(&fx.lib, 50).unwrap();
        assert!(plan.mass, "3 of 4 is 75%");
        let ids = fx.ids[..3].to_vec();
        let e = remove_dead_rows(&mut fx.lib, &ids, 50, false).unwrap_err().to_string();
        assert!(e.contains("not fully mounted"), "{e}");
        assert_eq!(fx.lib.tracks().unwrap().len(), 4, "nothing removed");
        // the threshold is a setting: at 80% the same library is not flagged
        assert!(!plan_fixes(&fx.lib, 80).unwrap().mass);
        let rep = remove_dead_rows(&mut fx.lib, &ids, 50, true).unwrap();
        assert_eq!(rep.removed.len(), 3);
    }

    #[test]
    fn with_no_music_folder_nothing_is_dead() {
        let mut fx = fixture("nomusic", 3);
        std::fs::remove_dir_all(music_dir(fx.lib.root())).unwrap();
        let plan = plan_fixes(&fx.lib, 50).unwrap();
        assert!(!plan.music_folder_ok);
        assert!(plan.dead_rows.is_empty() && plan.kept_for_mixes.is_empty() && plan.grids.is_empty());
        let ids = fx.ids.clone();
        let rep = remove_dead_rows(&mut fx.lib, &ids, 50, false).unwrap();
        assert!(rep.removed.is_empty());
        assert_eq!(fx.lib.tracks().unwrap().len(), 3);
    }
}
