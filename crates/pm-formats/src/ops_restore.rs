//! Restore `music.db` from one of the app's automatic snapshots (owner: device area).
//!
//! The snapshots are made by [`ops_device::snapshot_library`] before the first write of each
//! session and live in the app's data folder, never on the device. This module reads them
//! (read-only), says in numbers and names what a restore would change compared with the
//! library on the card right now, and performs the restore.
//!
//! The preview and the restore use one function, [`diff_snapshot`]; the restore recomputes it
//! and refuses when its [`SnapshotDiff::fingerprint`] differs from the one the user confirmed,
//! so what is applied is what was shown. Only `music.db` is replaced: `settings.dat`, the audio
//! and the beat grids are not touched (so tracks added after the snapshot keep their files and
//! turn up under "unused files" in the health scan, and tracks the snapshot has but whose audio
//! was removed since are flagged in the preview).
//!
//! The caller (the app's command layer) is responsible for holding the write gate exclusively
//! while [`restore_snapshot`] runs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::library::{db_path, Error, Library};
use crate::ops_device::{self, copy_hashed, list_snapshots, pacemaker_dir, SnapshotEntry, Sha256};
use crate::ops_fix::{checked_track_paths, track_label};
use crate::db;

fn invalid<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(msg.into()))
}

/// Rows of the three tables a restore changes.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct LibraryCounts {
    pub tracks: usize,
    pub cases: usize,
    pub mixes: usize,
}

struct TrackRow {
    label: String,
    sig: String,
    location: String,
}

struct CaseRow {
    label: String,
    /// Track ids in play order (rowid order).
    members: Vec<i64>,
    sig: String,
}

struct MixRow {
    label: String,
    sig: String,
}

/// What a diff needs from one `music.db`.
struct Index {
    tracks: BTreeMap<i64, TrackRow>,
    cases: BTreeMap<i64, CaseRow>,
    mixes: BTreeMap<i64, MixRow>,
}

impl Index {
    fn counts(&self) -> LibraryCounts {
        LibraryCounts { tracks: self.tracks.len(), cases: self.cases.len(), mixes: self.mixes.len() }
    }
}

fn read_index(con: &Connection) -> Result<Index, Error> {
    let mut tracks = BTreeMap::new();
    {
        let mut st = con.prepare(
            "SELECT track_id, IFNULL(title,''), IFNULL(artist,''), IFNULL(album,''), IFNULL(genre,''), IFNULL(rating,0),
                    IFNULL(key,''), IFNULL(bpm,0), IFNULL(location,''), IFNULL(date_modified,0)
             FROM tracks WHERE track_id >= 0",
        )?;
        let rows = st.query_map([], |r| {
            let id: i64 = r.get(0)?;
            let (title, artist, album, genre): (String, String, String, String) = (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
            let (rating, key, bpm, location, modified): (i64, String, f64, String, i64) =
                (r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?);
            let sig = format!("{title}\u{1}{artist}\u{1}{album}\u{1}{genre}\u{1}{rating}\u{1}{key}\u{1}{bpm}\u{1}{location}\u{1}{modified}");
            Ok((id, TrackRow { label: track_label(&artist, &title, &location), sig, location }))
        })?;
        for row in rows {
            let (id, t) = row?;
            tracks.insert(id, t);
        }
    }
    let mut members: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    {
        let mut st = con.prepare("SELECT case_id, track_id FROM casetracks ORDER BY rowid")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (c, t) = row?;
            members.entry(c).or_default().push(t);
        }
    }
    let mut cases = BTreeMap::new();
    {
        let mut st = con.prepare("SELECT case_id, IFNULL(name,''), IFNULL(rating,0), IFNULL(comments,'') FROM cases WHERE case_id >= 0")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
        for row in rows {
            let (id, name, rating, comments) = row?;
            let m = members.remove(&id).unwrap_or_default();
            let ids: Vec<String> = m.iter().map(|x| x.to_string()).collect();
            let sig = format!("{name}\u{1}{rating}\u{1}{comments}\u{1}{}", ids.join(","));
            cases.insert(id, CaseRow { label: name, members: m, sig });
        }
    }
    let mut mixes = BTreeMap::new();
    {
        let mut st = con.prepare(
            "SELECT m.mix_id, IFNULL(m.name,''), IFNULL(m.date_modified,0), IFNULL(length(m.structured_ct),0),
                    (SELECT COUNT(*) FROM mixtracks t WHERE t.mix_id = m.mix_id)
             FROM mixes m WHERE m.mix_id >= 0",
        )?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
        })?;
        for row in rows {
            let (id, name, modified, len, n) = row?;
            let sig = format!("{name}\u{1}{modified}\u{1}{len}\u{1}{n}");
            mixes.insert(id, MixRow { label: name, sig });
        }
    }
    Ok(Index { tracks, cases, mixes })
}

/// Opens a snapshot's `music.db` read-only and checks it is a device-compatible library.
fn open_snapshot(folder: &Path) -> Result<Connection, Error> {
    let db = folder.join("music.db");
    if !db.is_file() {
        return invalid("this snapshot has no music.db");
    }
    if folder.join("music.db-journal").exists() {
        return invalid(
            "this snapshot was taken while the card had an unfinished write (it holds a music.db-journal), so it cannot be restored safely",
        );
    }
    let con = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let problems = db::check(&con)?;
    if !problems.is_empty() {
        return invalid(format!("this snapshot is not a device-compatible library: {}", problems.join("; ")));
    }
    Ok(con)
}

/// One name in a diff list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ItemRef {
    pub id: i64,
    pub label: String,
    /// Something to know about this item, in words (for example that its audio is not on the card).
    pub note: Option<String>,
}

/// A diff list as shown: the real size and the first [`LIST_CAP`] names.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct Listed {
    pub total: usize,
    pub items: Vec<ItemRef>,
}

/// How many names a diff list carries to the screen; `total` is always the real count.
pub const LIST_CAP: usize = 300;

impl Listed {
    fn from(items: Vec<ItemRef>) -> Listed {
        Listed { total: items.len(), items: items.into_iter().take(LIST_CAP).collect() }
    }
}

/// What differs between a snapshot and the library on the card, per table.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DiffCounts {
    /// In the snapshot, not on the card now: a restore brings them back.
    pub tracks_returning: usize,
    /// On the card now, not in the snapshot: a restore removes their rows.
    pub tracks_lost: usize,
    /// In both, with different tags, rating, key, tempo or location.
    pub tracks_changed: usize,
    pub cases_returning: usize,
    pub cases_lost: usize,
    pub cases_changed: usize,
    pub mixes_returning: usize,
    pub mixes_lost: usize,
    pub mixes_changed: usize,
}

impl DiffCounts {
    pub fn is_empty(&self) -> bool {
        self.tracks_returning + self.tracks_lost + self.tracks_changed + self.cases_returning + self.cases_lost
            + self.cases_changed + self.mixes_returning + self.mixes_lost + self.mixes_changed
            == 0
    }
}

/// The full comparison of one snapshot with the card, with names.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotDiff {
    pub snapshot_name: String,
    pub snapshot: LibraryCounts,
    /// `None` when the library on the card cannot be read (see `current_problem`).
    pub current: Option<LibraryCounts>,
    pub current_problem: Option<String>,
    pub counts: DiffCounts,
    pub tracks_returning: Listed,
    pub tracks_lost: Listed,
    pub tracks_changed: Listed,
    pub cases_returning: Listed,
    pub cases_lost: Listed,
    pub cases_changed: Listed,
    pub mixes_returning: Listed,
    pub mixes_lost: Listed,
    pub mixes_changed: Listed,
    /// SHA-256 over the snapshot's name and every id in the nine lists (not just the capped
    /// names). The restore must be given the value the preview returned.
    pub fingerprint: String,
}

struct RawDiff {
    returning: Vec<i64>,
    lost: Vec<i64>,
    changed: Vec<i64>,
}

fn diff_maps<V>(snap: &BTreeMap<i64, V>, cur: &BTreeMap<i64, V>, same: impl Fn(&V, &V) -> bool) -> RawDiff {
    RawDiff {
        returning: snap.keys().filter(|k| !cur.contains_key(k)).copied().collect(),
        lost: cur.keys().filter(|k| !snap.contains_key(k)).copied().collect(),
        changed: snap.iter().filter(|(k, v)| cur.get(k).is_some_and(|c| !same(v, c))).map(|(k, _)| *k).collect(),
    }
}

/// Compares the snapshot at `snapshot_folder` with the library on `root`. Read-only on both.
/// `root` is also used to look for the audio of tracks that would come back.
pub fn diff_snapshot(root: &Path, snapshot_folder: &Path) -> Result<SnapshotDiff, Error> {
    let snap_con = open_snapshot(snapshot_folder)?;
    let snap = read_index(&snap_con)?;
    drop(snap_con);
    let cur = Library::open_read_only(root).map_err(|e| e.to_string()).and_then(|l| read_index(&l.con).map_err(|e| e.to_string()));
    let name = snapshot_folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(build_diff(root, name, &snap, cur))
}

fn build_diff(root: &Path, name: String, snap: &Index, cur: Result<Index, String>) -> SnapshotDiff {
    let (cur_idx, current_problem) = match cur {
        Ok(i) => (Some(i), None),
        Err(e) => (None, Some(e)),
    };
    let empty = Index { tracks: BTreeMap::new(), cases: BTreeMap::new(), mixes: BTreeMap::new() };
    let basis = cur_idx.as_ref().unwrap_or(&empty);
    let t = diff_maps(&snap.tracks, &basis.tracks, |a, b| a.sig == b.sig);
    let c = diff_maps(&snap.cases, &basis.cases, |a, b| a.sig == b.sig);
    let m = diff_maps(&snap.mixes, &basis.mixes, |a, b| a.sig == b.sig);

    // the audio of a track that would come back: is it still on the card (checked path only)?
    let returning_track = |id: &i64| {
        let r = &snap.tracks[id];
        let note = match checked_track_paths(root, &r.location) {
            Some((audio, _)) if audio.is_file() => None,
            Some(_) => Some("its audio file is not on the card".to_string()),
            None => Some("its location is not a plain path under the card's Music folder".to_string()),
        };
        ItemRef { id: *id, label: r.label.clone(), note }
    };
    let counts = counts_only(snap, cur_idx.as_ref());
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    // the row signatures (snapshot side and current side) go in too, so a row edited again after the preview
    // changes the fingerprint and the restore is refused instead of applying values the user never saw
    fn fold<R>(h: &mut Sha256, tag: &str, ids: &[i64], snap: &BTreeMap<i64, R>, cur: &BTreeMap<i64, R>, sig: impl Fn(&R) -> &str) {
        h.update(format!("|{tag}:").as_bytes());
        for id in ids {
            let s = snap.get(id).map(&sig).unwrap_or("");
            let c = cur.get(id).map(&sig).unwrap_or("");
            h.update(format!("{id}\u{2}{s}\u{2}{c},").as_bytes());
        }
    }
    fold(&mut h, "tr", &t.returning, &snap.tracks, &basis.tracks, |r| &r.sig);
    fold(&mut h, "tl", &t.lost, &snap.tracks, &basis.tracks, |r| &r.sig);
    fold(&mut h, "tc", &t.changed, &snap.tracks, &basis.tracks, |r| &r.sig);
    fold(&mut h, "cr", &c.returning, &snap.cases, &basis.cases, |r| &r.sig);
    fold(&mut h, "cl", &c.lost, &snap.cases, &basis.cases, |r| &r.sig);
    fold(&mut h, "cc", &c.changed, &snap.cases, &basis.cases, |r| &r.sig);
    fold(&mut h, "mr", &m.returning, &snap.mixes, &basis.mixes, |r| &r.sig);
    fold(&mut h, "ml", &m.lost, &snap.mixes, &basis.mixes, |r| &r.sig);
    fold(&mut h, "mc", &m.changed, &snap.mixes, &basis.mixes, |r| &r.sig);
    h.update(format!("|{}", current_problem.is_some()).as_bytes());
    let plain = |label: &str, id: &i64| ItemRef { id: *id, label: label.to_string(), note: None };
    SnapshotDiff {
        snapshot_name: name,
        snapshot: snap.counts(),
        current: cur_idx.as_ref().map(Index::counts),
        current_problem,
        counts,
        tracks_returning: Listed::from(t.returning.iter().map(returning_track).collect()),
        tracks_lost: Listed::from(t.lost.iter().map(|id| plain(&basis.tracks[id].label, id)).collect()),
        tracks_changed: Listed::from(t.changed.iter().map(|id| plain(&snap.tracks[id].label, id)).collect()),
        cases_returning: Listed::from(c.returning.iter().map(|id| plain(&snap.cases[id].label, id)).collect()),
        cases_lost: Listed::from(c.lost.iter().map(|id| plain(&basis.cases[id].label, id)).collect()),
        cases_changed: Listed::from(
            c.changed
                .iter()
                .map(|id| {
                    let (s, now) = (&snap.cases[id], &basis.cases[id]);
                    let what = if s.label != now.label {
                        format!("named {} in the snapshot", s.label)
                    } else if s.members != now.members {
                        format!("{} tracks in the snapshot, {} now", s.members.len(), now.members.len())
                    } else {
                        "details differ".to_string()
                    };
                    ItemRef { id: *id, label: now.label.clone(), note: Some(what) }
                })
                .collect(),
        ),
        mixes_returning: Listed::from(m.returning.iter().map(|id| plain(&snap.mixes[id].label, id)).collect()),
        mixes_lost: Listed::from(m.lost.iter().map(|id| plain(&basis.mixes[id].label, id)).collect()),
        mixes_changed: Listed::from(m.changed.iter().map(|id| plain(&snap.mixes[id].label, id)).collect()),
        fingerprint: h.finish_hex(),
    }
}

/// One snapshot in the list, with what a restore from it would change.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotRow {
    pub name: String,
    pub folder: String,
    /// The UTC time in the folder name (`YYYYMMDDTHHMMSSZ`), or empty if the name has none.
    pub stamp: String,
    pub files: Vec<String>,
    pub music_db_bytes: u64,
    /// Why it cannot be restored (or compared), when it cannot.
    pub problem: Option<String>,
    pub snapshot: Option<LibraryCounts>,
    pub counts: Option<DiffCounts>,
}

fn stamp_of(name: &str) -> String {
    let rest = name.strip_prefix(ops_device::SNAPSHOT_PREFIX).unwrap_or(name);
    rest.split('-').next().unwrap_or("").to_string()
}

/// Every automatic snapshot (newest first) with its counts and its differences from the card.
/// Read-only. The card's index is read once for all of them.
pub fn snapshot_rows(root: &Path, snapshots_dir: &Path) -> Vec<SnapshotRow> {
    let current = Library::open_read_only(root).map_err(|e| e.to_string()).and_then(|l| read_index(&l.con).map_err(|e| e.to_string()));
    list_snapshots(snapshots_dir)
        .into_iter()
        .map(|e| {
            let folder = PathBuf::from(&e.folder);
            let bytes = fs::metadata(folder.join("music.db")).map(|m| m.len()).unwrap_or(0);
            let mut row = SnapshotRow {
                stamp: stamp_of(&e.name),
                name: e.name.clone(),
                folder: e.folder.clone(),
                files: e.files.clone(),
                music_db_bytes: bytes,
                problem: None,
                snapshot: None,
                counts: None,
            };
            match open_snapshot(&folder).and_then(|con| read_index(&con)) {
                Ok(snap) => {
                    // the counts need neither names nor file checks, so build the cheap form
                    row.counts = Some(counts_only(&snap, current.as_ref().ok()));
                    row.snapshot = Some(snap.counts());
                }
                Err(e) => row.problem = Some(e.to_string()),
            }
            row
        })
        .collect()
}

fn counts_only(snap: &Index, cur: Option<&Index>) -> DiffCounts {
    let empty = Index { tracks: BTreeMap::new(), cases: BTreeMap::new(), mixes: BTreeMap::new() };
    let basis = cur.unwrap_or(&empty);
    let t = diff_maps(&snap.tracks, &basis.tracks, |a, b| a.sig == b.sig);
    let c = diff_maps(&snap.cases, &basis.cases, |a, b| a.sig == b.sig);
    let m = diff_maps(&snap.mixes, &basis.mixes, |a, b| a.sig == b.sig);
    DiffCounts {
        tracks_returning: t.returning.len(),
        tracks_lost: t.lost.len(),
        tracks_changed: t.changed.len(),
        cases_returning: c.returning.len(),
        cases_lost: c.lost.len(),
        cases_changed: c.changed.len(),
        mixes_returning: m.returning.len(),
        mixes_lost: m.lost.len(),
        mixes_changed: m.changed.len(),
    }
}

/// The snapshot called `name` in `snapshots_dir`. Only a name that [`list_snapshots`] itself
/// returns is accepted, so no path can be smuggled in through it.
pub fn find_snapshot(snapshots_dir: &Path, name: &str) -> Result<SnapshotEntry, Error> {
    list_snapshots(snapshots_dir)
        .into_iter()
        .find(|e| e.name == name)
        .map_or_else(|| invalid(format!("there is no snapshot called {name}")), Ok)
}

/// Outcome of [`restore_snapshot`].
#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub restored_from: String,
    /// The snapshot made of the card's own music.db just before it was replaced: restoring from
    /// it undoes this restore.
    pub safety_copy: String,
    /// What the library on the card holds now (read back).
    pub counts: LibraryCounts,
}

/// Replaces `<root>/.Pacemaker/music.db` with a verified copy of the snapshot's, after making a
/// fresh safety snapshot of the card's current one.
///
/// Refuses (changing nothing) when the snapshot is not a sound, device-compatible library
/// (`PRAGMA integrity_check`), when its diff no longer has the `expected_fingerprint` the user
/// confirmed, or when the card has a write-ahead log. The new file is copied beside the old one,
/// read back and checksummed, then renamed over it, so an unplug leaves either the old or the
/// new database whole. A `music.db-journal` of the old database is moved aside across the
/// rename (it must never be replayed into the new file) and put back if the rename fails. If
/// the restored library does not read back as the snapshot's, the safety copy is put back.
///
/// The caller must hold the device's write gate exclusively.
pub fn restore_snapshot(
    root: &Path,
    snapshots_dir: &Path,
    name: &str,
    expected_fingerprint: &str,
    keep: usize,
) -> Result<RestoreReport, Error> {
    let entry = find_snapshot(snapshots_dir, name)?;
    let folder = PathBuf::from(&entry.folder);
    let diff = diff_snapshot(root, &folder)?;
    if diff.fingerprint != expected_fingerprint {
        return invalid("the library on the card or the snapshot changed since the preview was made; review the differences again before restoring");
    }
    let pm = pacemaker_dir(root);
    if pm.join("music.db-wal").exists() || pm.join("music.db-shm").exists() {
        return invalid("the card's database has a write-ahead log (music.db-wal); it is not restored over because the log would be replayed into the wrong file");
    }
    {
        let con = open_snapshot(&folder)?;
        let verdict: String = con.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if verdict != "ok" {
            return invalid(format!("this snapshot's database is damaged (integrity check: {verdict})"));
        }
    }
    let snap_db = folder.join("music.db");
    let safety = ops_device::snapshot_library_protecting(root, snapshots_dir, keep, &[name])?;
    install_db(root, &snap_db)?;
    let verified = Library::open_read_only(root).map_err(|e| e.to_string()).and_then(|l| {
        let problems = l.compatibility_problems().map_err(|e| e.to_string())?;
        if !problems.is_empty() {
            return Err(format!("not device-compatible: {}", problems.join("; ")));
        }
        read_index(&l.con).map(|i| i.counts()).map_err(|e| e.to_string())
    });
    match verified {
        Ok(counts) if counts == diff.snapshot => Ok(RestoreReport { restored_from: name.to_string(), safety_copy: safety.name, counts }),
        other => {
            let why = match other {
                Ok(c) => format!("it holds {} tracks, {} cases, {} mixes instead of the snapshot's {}, {}, {}", c.tracks, c.cases, c.mixes, diff.snapshot.tracks, diff.snapshot.cases, diff.snapshot.mixes),
                Err(e) => e,
            };
            match install_db(root, &Path::new(&safety.folder).join("music.db")) {
                Ok(()) => invalid(format!("the restored database did not read back correctly ({why}); the previous database was put back")),
                Err(e) => invalid(format!(
                    "the restored database did not read back correctly ({why}) and putting the previous one back failed ({e}); the safety copy is in {}",
                    safety.folder
                )),
            }
        }
    }
}

/// Copies `src` (a database file) over the card's `music.db`: beside it first, read back and
/// checksummed, then renamed over it.
fn install_db(root: &Path, src: &Path) -> Result<(), Error> {
    let dest = db_path(root);
    let pm = pacemaker_dir(root);
    let staged = pm.join("music.db.restoring");
    let _ = fs::remove_file(&staged);
    if let Err(e) = copy_hashed(src, &staged) {
        let _ = fs::remove_file(&staged);
        return Err(e);
    }
    let journal = pm.join("music.db-journal");
    let parked = pm.join("music.db-journal.parked");
    let had_journal = journal.exists();
    if had_journal {
        let _ = fs::remove_file(&parked);
        if let Err(e) = fs::rename(&journal, &parked) {
            let _ = fs::remove_file(&staged);
            return Err(e.into());
        }
    }
    if let Err(e) = fs::rename(&staged, &dest) {
        let _ = fs::remove_file(&staged);
        if had_journal {
            let _ = fs::rename(&parked, &journal);
        }
        return invalid(format!("could not replace music.db ({e}); something is probably still using the file. Nothing was changed, try again in a moment"));
    }
    if had_journal {
        let _ = fs::remove_file(&parked);
    }
    Ok(())
}

/// Names (not ids) of the ids in a diff list, for tests and messages.
#[cfg(test)]
fn labels(l: &Listed) -> Vec<String> {
    l.items.iter().map(|i| i.label.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;
    use crate::ops_metadata::TrackEdit;
    use crate::strfile;

    struct Fx {
        base: PathBuf,
        vol: PathBuf,
        snaps: PathBuf,
        lib: Library,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn analysis() -> strfile::Analysis {
        strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        }
    }

    fn add(fx: &mut Fx, n: usize) -> i64 {
        let src = fx.base.join("src");
        fs::create_dir_all(&src).unwrap();
        let f = src.join(format!("synthetic {n}.mp3"));
        fs::write(&f, vec![0u8; 2000 + n]).unwrap();
        let meta = TrackMeta { title: format!("Title {n}"), artist: format!("Artist {n}"), format: "MP3".into(), play_time_secs: 90, ..Default::default() };
        fx.lib.add_track(&f, &meta, &analysis()).unwrap()
    }

    fn fixture(tag: &str, tracks: usize) -> Fx {
        let base = std::env::temp_dir().join(format!("pm-restore-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let snaps = base.join("snaps");
        fs::create_dir_all(&vol).unwrap();
        let lib = Library::create(&vol).unwrap();
        let mut fx = Fx { base, vol, snaps, lib };
        for i in 0..tracks {
            add(&mut fx, i);
        }
        fx
    }

    impl Fx {
        /// Drops the fixture's own connection: Windows will not rename over an open database
        /// (the app opens and closes a connection per command, so it never holds one).
        fn close(&mut self) {
            self.lib = Library { root: self.vol.clone(), con: Connection::open_in_memory().unwrap() };
        }
    }

    fn snap(fx: &Fx) -> SnapshotEntry {
        ops_device::snapshot_library(&fx.vol, &fx.snaps, 10).unwrap()
    }

    #[test]
    fn the_diff_names_what_returns_what_is_lost_and_what_changed() {
        let mut fx = fixture("diff", 3);
        let ids: Vec<i64> = fx.lib.tracks().unwrap().iter().map(|t| t.track_id).collect();
        let case = fx.lib.create_case_with_tracks("Warm up", &ids[..2]).unwrap();
        let s = snap(&fx);

        // after the snapshot: a track is added, one is re-rated, a case is deleted, a mix is made
        let new_id = add(&mut fx, 9);
        fx.lib.set_track_rating(ids[0], 4).unwrap();
        fx.lib.delete_case(case).unwrap();
        fx.lib.create_mix("Late set").unwrap();

        let d = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        assert_eq!(d.snapshot, LibraryCounts { tracks: 3, cases: 1, mixes: 0 });
        assert_eq!(d.current, Some(LibraryCounts { tracks: 4, cases: 0, mixes: 1 }));
        assert_eq!(labels(&d.tracks_lost), ["Artist 9 - Title 9"]);
        assert_eq!(d.tracks_lost.items[0].id, new_id);
        assert_eq!(labels(&d.tracks_changed), ["Artist 0 - Title 0"]);
        assert_eq!(labels(&d.cases_returning), ["Warm up"]);
        assert_eq!(labels(&d.mixes_lost), ["Late set"]);
        assert_eq!((d.counts.tracks_returning, d.counts.cases_lost, d.counts.mixes_returning), (0, 0, 0));
        assert!(!d.counts.is_empty());

        // the row listing computes the same numbers as the full preview
        let rows = snapshot_rows(&fx.vol, &fx.snaps);
        assert_eq!(rows.len(), 1);
        let c = rows[0].counts.as_ref().unwrap();
        assert_eq!((c.tracks_lost, c.tracks_changed, c.cases_returning, c.mixes_lost), (1, 1, 1, 1));
        assert_eq!(rows[0].snapshot, Some(LibraryCounts { tracks: 3, cases: 1, mixes: 0 }));
        assert_eq!(rows[0].stamp.len(), 16, "{}", rows[0].stamp);
    }

    #[test]
    fn a_track_that_would_return_without_its_audio_is_flagged() {
        let fx = fixture("audio", 2);
        let s = snap(&fx);
        let t = fx.lib.tracks().unwrap().remove(0);
        // after the snapshot the row is removed and its audio deleted
        let (audio, _) = fx.lib.track_paths(&t.location).unwrap();
        fx.lib.con.execute("DELETE FROM tracks WHERE track_id = ?1", [t.track_id]).unwrap();
        fs::remove_file(audio).unwrap();
        let d = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        assert_eq!(d.tracks_returning.total, 1);
        assert_eq!(d.tracks_returning.items[0].note.as_deref(), Some("its audio file is not on the card"));
    }

    #[test]
    fn restore_replaces_the_database_after_a_safety_copy_and_the_safety_copy_undoes_it() {
        let mut fx = fixture("restore", 3);
        let ids: Vec<i64> = fx.lib.tracks().unwrap().iter().map(|t| t.track_id).collect();
        let s = snap(&fx);
        fx.lib.edit_tracks(&[(ids[0], TrackEdit { title: Some("Edited".into()), ..Default::default() })]).unwrap();
        add(&mut fx, 7);
        fx.lib.create_mix("Made later").unwrap();
        let before_restore = fx.lib.tracks().unwrap();
        assert_eq!(before_restore.len(), 4);

        let d = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        fx.close();
        let rep = restore_snapshot(&fx.vol, &fx.snaps, &s.name, &d.fingerprint, 10).unwrap();
        assert_eq!(rep.counts, LibraryCounts { tracks: 3, cases: 0, mixes: 0 });
        assert_ne!(rep.safety_copy, s.name);

        let fresh = Library::open_read_only(&fx.vol).unwrap();
        let titles: Vec<String> = fresh.tracks().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["Title 0", "Title 1", "Title 2"], "the edit and the added track are gone");
        assert_eq!(fresh.mixes().unwrap().len(), 0);
        assert!(!pacemaker_dir(&fx.vol).join("music.db.restoring").exists(), "no staging file left behind");
        drop(fresh);

        // the safety copy holds the state from just before the restore, so restoring from it
        // brings the later work back
        let all = list_snapshots(&fx.snaps);
        assert!(all.iter().any(|e| e.name == rep.safety_copy));
        let back = diff_snapshot(&fx.vol, &Path::new(&fx.snaps).join(&rep.safety_copy)).unwrap();
        assert_eq!(back.counts.tracks_returning, 1);
        assert_eq!(back.counts.mixes_returning, 1);
        let rep2 = restore_snapshot(&fx.vol, &fx.snaps, &rep.safety_copy, &back.fingerprint, 10).unwrap();
        assert_eq!(rep2.counts, LibraryCounts { tracks: 4, cases: 0, mixes: 1 });
        let titles: Vec<String> = Library::open_read_only(&fx.vol).unwrap().tracks().unwrap().into_iter().map(|t| t.title).collect();
        assert!(titles.contains(&"Edited".to_string()) && titles.contains(&"Title 7".to_string()), "{titles:?}");
    }

    #[test]
    fn re_editing_an_already_changed_track_after_the_preview_changes_the_fingerprint() {
        let mut fx = fixture("reedit", 2);
        let id = fx.lib.tracks().unwrap()[0].track_id;
        let s = snap(&fx);
        fx.lib.set_track_rating(id, 3).unwrap();
        let d = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        assert_eq!(d.tracks_changed.items.len(), 1, "the track is already in the changed list");
        // edited again after the preview: same id in the same list, different values
        fx.lib.set_track_rating(id, 5).unwrap();
        let d2 = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        assert_eq!(d2.tracks_changed.items.len(), 1);
        assert_ne!(d.fingerprint, d2.fingerprint);
        let e = restore_snapshot(&fx.vol, &fx.snaps, &s.name, &d.fingerprint, 10).unwrap_err().to_string();
        assert!(e.contains("changed since the preview"), "{e}");
    }

    #[test]
    fn a_stale_fingerprint_changes_nothing() {
        let mut fx = fixture("stale", 2);
        let s = snap(&fx);
        let d = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        // the card changes after the preview
        add(&mut fx, 5);
        let before = fs::read(db_path(&fx.vol)).unwrap();
        let e = restore_snapshot(&fx.vol, &fx.snaps, &s.name, &d.fingerprint, 10).unwrap_err().to_string();
        assert!(e.contains("changed since the preview"), "{e}");
        assert_eq!(fs::read(db_path(&fx.vol)).unwrap(), before, "the database was not touched");
        assert_eq!(list_snapshots(&fx.snaps).len(), 1, "and no safety copy was made for a refused restore");
    }

    #[test]
    fn only_listed_snapshot_names_are_accepted() {
        let fx = fixture("names", 1);
        snap(&fx);
        for bad in ["..", "../vol", "music-db-", "nope", "", "music-db-20200101T000000Z/../../x"] {
            assert!(find_snapshot(&fx.snaps, bad).is_err(), "{bad}");
            assert!(restore_snapshot(&fx.vol, &fx.snaps, bad, "x", 10).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_damaged_or_foreign_snapshot_is_listed_with_its_problem_and_refused() {
        let fx = fixture("bad", 1);
        let good = snap(&fx);
        // a snapshot that is not a library at all
        let junk = fx.snaps.join("music-db-20200101T000000Z");
        fs::create_dir_all(&junk).unwrap();
        fs::write(junk.join("music.db"), b"this is not sqlite").unwrap();
        // one taken with an unfinished write
        let hot = fx.snaps.join("music-db-20200202T000000Z");
        fs::create_dir_all(&hot).unwrap();
        fs::copy(Path::new(&good.folder).join("music.db"), hot.join("music.db")).unwrap();
        fs::write(hot.join("music.db-journal"), b"x").unwrap();

        let rows = snapshot_rows(&fx.vol, &fx.snaps);
        assert_eq!(rows.len(), 3);
        let by = |n: &str| rows.iter().find(|r| r.name == n).unwrap();
        assert!(by("music-db-20200101T000000Z").problem.is_some() && by("music-db-20200101T000000Z").counts.is_none());
        assert!(by("music-db-20200202T000000Z").problem.as_deref().unwrap().contains("unfinished write"));
        assert!(by(&good.name).problem.is_none());

        let before = fs::read(db_path(&fx.vol)).unwrap();
        assert!(restore_snapshot(&fx.vol, &fx.snaps, "music-db-20200101T000000Z", "x", 10).is_err());
        assert!(restore_snapshot(&fx.vol, &fx.snaps, "music-db-20200202T000000Z", "x", 10).is_err());
        assert_eq!(fs::read(db_path(&fx.vol)).unwrap(), before);
    }

    #[test]
    fn restore_works_when_the_card_database_is_unreadable_and_moves_a_hot_journal_aside() {
        let mut fx = fixture("broken", 2);
        let s = snap(&fx);
        // close the fixture's own connection (Windows will not overwrite an open database)
        fx.close();
        let (vol, snaps) = (fx.vol.clone(), fx.snaps.clone());
        // the card's database is destroyed and a stale journal sits beside it
        fs::write(db_path(&vol), b"garbage").unwrap();
        fs::write(pacemaker_dir(&vol).join("music.db-journal"), b"old journal").unwrap();

        let d = diff_snapshot(&vol, Path::new(&s.folder)).unwrap();
        assert!(d.current.is_none() && d.current_problem.is_some());
        assert_eq!(d.snapshot.tracks, 2);
        assert_eq!(d.counts.tracks_returning, 2, "everything counts as coming back");

        // the snapshot being restored from must survive its own safety-copy retention
        let rep = restore_snapshot(&vol, &snaps, &s.name, &d.fingerprint, 1);
        // the safety copy of a destroyed database is refused by nothing but its own checks; the
        // restore itself must succeed
        let rep = rep.unwrap();
        assert_eq!(rep.counts.tracks, 2);
        assert!(!pacemaker_dir(&vol).join("music.db-journal").exists(), "the old journal is not replayed into the new file");
        assert!(!pacemaker_dir(&vol).join("music.db-journal.parked").exists());
        assert!(list_snapshots(&snaps).iter().any(|e| e.name == s.name), "keep = 1 did not delete the source");
        assert_eq!(Library::open_read_only(&vol).unwrap().tracks().unwrap().len(), 2);
    }

    #[test]
    fn a_write_ahead_log_on_the_card_blocks_the_restore() {
        let fx = fixture("wal", 1);
        let s = snap(&fx);
        let d = diff_snapshot(&fx.vol, Path::new(&s.folder)).unwrap();
        fs::write(pacemaker_dir(&fx.vol).join("music.db-wal"), b"x").unwrap();
        let e = restore_snapshot(&fx.vol, &fx.snaps, &s.name, &d.fingerprint, 10).unwrap_err().to_string();
        assert!(e.contains("write-ahead log"), "{e}");
    }
}
