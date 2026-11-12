//! A Pacemaker library on a volume: read its tracks, mixes and cases, and add tracks the way
//! the original Editor does (copy to `.Pacemaker/Music/A/B/C.ext`, write the `.str` sidecar,
//! insert the row). Field conventions follow rows the Windows Editor 2.0.2.14170 wrote; see
//! notes/FINDINGS.md sections 2-4b.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::{db, naming, strfile};

/// What we write as `modified_by_ed` / `analyzed_by_ed`: honest about which software touched
/// the row (the original Editor writes its own version, e.g. "2.0.2.14170").
pub const EDITOR_ID: &str = concat!("pacemaker-re/", env!("CARGO_PKG_VERSION"));

/// "No cue point" as stored on real devices: 2^64 as a REAL (an unsigned -1 that went through a
/// double; pmplayer contains the literal "1.84467440737096e+19"). 240 of 241 tracks on a real
/// device carry it; the computer-side library uses -1 instead. Written in the INSERT below.
pub const NO_CUE: f64 = 18446744073709551615.0;

/// Highest star rating. `tracks.rating` holds the star count itself (0 = unrated): the
/// Editor's rating menu offers "0".."5" and stores menu id - 1, and its metadata field only
/// accepts one of the digits 012345 (notes/FINDINGS.md section 4d).
pub const MAX_RATING: u8 = 5;

/// `cases.name` is declared VARCHAR(255); SQLite does not enforce it, so we do.
pub const MAX_CASE_NAME_CHARS: usize = 255;

/// What we write as `cases.creator_id`. Tonium's Editor writes `Tonium;Editor;<version>;<MAC
/// address as a signed 64-bit integer>` and the device `Tonium;Device;<id>`; nothing in either
/// binary parses the value, so ours keeps the four-part shape with our own name and no machine
/// fingerprint (the last part is 0).
pub const CASE_CREATOR_ID: &str = concat!("pacemaker-re;Editor;", env!("CARGO_PKG_VERSION"), ";0");

#[derive(Debug)]
pub enum Error {
    Db(rusqlite::Error),
    Io(std::io::Error),
    NotALibrary(PathBuf),
    Incompatible(Vec<String>),
    /// A caller-supplied value the device could not use (rating out of range, unknown id, ...).
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Db(e) => write!(f, "database: {e}"),
            Error::Io(e) => write!(f, "file: {e}"),
            Error::NotALibrary(p) => write!(f, "{} has no .Pacemaker/music.db", p.display()),
            Error::Incompatible(p) => write!(f, "music.db is not device-compatible: {}", p.join("; ")),
            Error::Invalid(m) => write!(f, "invalid: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Track {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub bpm: f64,
    pub key: String,
    pub play_time_secs: i64,
    pub rating: i64,
    pub location: String,
    pub file_size: i64,
    pub format: String,
    pub analysed: bool,
    pub times_played: i64,
    /// Unix milliseconds of the last play, or -1 for "never played" (the stored sentinel).
    pub last_played: i64,
    /// Release year, 0 when not set.
    pub year: i64,
    pub label: String,
    pub remixer: String,
    pub composer: String,
    pub comments: String,
    /// Unix milliseconds the track was added (FINDINGS section 4b), or -1 when the row has none.
    pub date_added: i64,
    /// Bit rate in kilobits per second (what `TrackMeta::bit_rate_kbps` stores), 0 when unknown.
    pub bit_rate: i64,
}

/// An integer column read without trusting its storage class. These columns have NUMBER affinity,
/// so a tool that wrote text into one (an empty string, "2010s") would otherwise make the whole
/// track list fail to load instead of showing one blank cell.
fn lenient_i64(r: &rusqlite::Row<'_>, idx: usize, missing: i64) -> rusqlite::Result<i64> {
    use rusqlite::types::Value;
    Ok(match r.get::<_, Value>(idx)? {
        Value::Integer(i) => i,
        Value::Real(f) if f.is_finite() => f.round() as i64,
        Value::Text(t) => t.trim().parse::<i64>().unwrap_or(missing),
        _ => missing,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub play_time_secs: i64,
    pub track_count: i64,
}

/// Tag data for a track being added; empty strings and zeros where unknown.
#[derive(Debug, Clone, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub composer: String,
    pub genre: String,
    pub label: String,
    pub key: String,
    pub comment: String,
    pub producer: String,
    pub remixer: String,
    pub year: i64,
    pub track_number: i64,
    pub disc_number: i64,
    pub format: String,
    pub bit_rate_kbps: i64,
    pub sample_rate: i64,
    pub play_time_secs: i64,
}

pub struct Library {
    pub(crate) root: PathBuf,
    // crate-visible so per-area `impl Library` blocks (ops_*.rs) can add operations in their own files
    pub(crate) con: Connection,
}

pub fn db_path(volume_root: &Path) -> PathBuf {
    volume_root.join(".Pacemaker").join("music.db")
}

/// Mounted volumes that hold a Pacemaker library. On Windows every drive letter is checked;
/// elsewhere the usual removable-media mount points.
pub fn find_volumes() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        roots.extend((b'A'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))));
    } else {
        for base in ["/Volumes", "/media", "/run/media", "/mnt"] {
            for e in std::fs::read_dir(base).into_iter().flatten().flatten() {
                roots.push(e.path());
                // /media/<user>/<volume> and /run/media/<user>/<volume>
                roots.extend(std::fs::read_dir(e.path()).into_iter().flatten().flatten().map(|s| s.path()));
            }
        }
    }
    roots.into_iter().filter(|r| db_path(r).is_file()).collect()
}

/// How long a write waits for another connection's lock before giving up. A quick write (a
/// rating, a case edit) that lands while a long job (adding tracks) holds a short transaction
/// waits for it instead of failing with "database is locked".
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

/// The tempo stored in `tracks.bpm` for an analysis: the median local tempo. On a variable-tempo
/// grid the first beat's own value can be far off (seen: 132.86 vs 135.24), and the median matches
/// the rows of a real device (notes/FINDINGS.md 4c). One function for every writer, so adding a
/// track and re-analysing or saving a grid can never store different values for the same grid.
pub fn track_bpm(analysis: &strfile::Analysis) -> f64 {
    let mut v: Vec<f64> = analysis.beats.iter().map(|b| b.bpm).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

/// `tracks.ind_bpm` for a tempo: `floor(bpm / 10) * 10` (FINDINGS 4c).
pub fn ind_bpm_of(bpm: f64) -> i64 {
    (bpm / 10.0).floor() as i64 * 10
}

/// A temp file name next to `dest`, unique within this process (two writers of the same target
/// never share a temp file) and ending in `.tmp`.
pub(crate) fn temp_sibling(dest: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{}.tmp", NEXT.fetch_add(1, Ordering::Relaxed)));
    dest.with_file_name(name)
}

/// A temp file this old belongs to a write that never finished (an unplug between create and
/// rename); a live writer renames its temp within seconds.
const STALE_TEMP_AGE: Duration = Duration::from_secs(600);

/// Removes the leftovers of earlier writes of `dest` (`<name>.<n>.tmp` beside it, older than
/// [`STALE_TEMP_AGE`]). Temp names carry a per-process counter, so without this a temp left by an
/// unplug would stay forever, in folders (like `.Pacemaker`) no clean-up scan looks at.
fn remove_stale_temps(dest: &Path) {
    let (Some(dir), Some(name)) = (dest.parent(), dest.file_name()) else { return };
    let prefix = format!("{}.", name.to_string_lossy());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let fname = e.file_name().to_string_lossy().into_owned();
        let Some(mid) = fname.strip_prefix(&prefix).and_then(|r| r.strip_suffix(".tmp")) else { continue };
        if mid.is_empty() || !mid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= STALE_TEMP_AGE);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Writes `dest` atomically: `fill` writes a temp file beside it, which is fsynced and renamed
/// over `dest` only when `fill` succeeded. On any failure the temp file is removed and `dest` is
/// untouched, so an unplug or an error mid-write never leaves a truncated file under the real name.
pub(crate) fn atomic_write_with(dest: &Path, fill: impl FnOnce(&mut File) -> std::io::Result<()>) -> std::io::Result<()> {
    remove_stale_temps(dest);
    let tmp = temp_sibling(dest);
    let res = (|| {
        let mut f = File::create(&tmp)?;
        fill(&mut f)?;
        f.flush()?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, dest)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// [`atomic_write_with`] for bytes in memory.
pub(crate) fn atomic_write(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    atomic_write_with(dest, |f| f.write_all(bytes))
}

/// [`atomic_write_with`] copying `src` into `dest`.
pub(crate) fn atomic_copy(src: &Path, dest: &Path) -> std::io::Result<()> {
    atomic_write_with(dest, |f| {
        let mut input = File::open(src)?;
        std::io::copy(&mut input, f).map(|_| ())
    })
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Calendar year (UTC) of a Unix-millisecond timestamp (Hinnant's civil-from-days).
fn year_of_ms(ms: i64) -> i64 {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + i64::from(month <= 2)
}

impl Library {
    /// Opens an existing library read-only (for browsing; nothing on the volume changes).
    pub fn open_read_only(volume_root: &Path) -> Result<Library, Error> {
        let p = db_path(volume_root);
        if !p.is_file() {
            return Err(Error::NotALibrary(volume_root.to_path_buf()));
        }
        let con = Connection::open_with_flags(&p, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Library { root: volume_root.to_path_buf(), con })
    }

    /// Opens an existing library for writing, refusing one that isn't device-compatible.
    pub fn open(volume_root: &Path) -> Result<Library, Error> {
        let p = db_path(volume_root);
        if !p.is_file() {
            return Err(Error::NotALibrary(volume_root.to_path_buf()));
        }
        let con = Connection::open(&p)?;
        con.busy_timeout(BUSY_TIMEOUT)?;
        let problems = db::check(&con)?;
        if !problems.is_empty() {
            return Err(Error::Incompatible(problems));
        }
        Ok(Library { root: volume_root.to_path_buf(), con })
    }

    /// Creates `.Pacemaker/`, `.Pacemaker/Music/` and an empty device-compatible `music.db`.
    pub fn create(volume_root: &Path) -> Result<Library, Error> {
        std::fs::create_dir_all(volume_root.join(naming::MUSIC_DIR))?;
        let p = db_path(volume_root);
        if p.exists() {
            return Library::open(volume_root);
        }
        let con = db::create(&p)?;
        Ok(Library { root: volume_root.to_path_buf(), con })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ways the database differs from what the device requires (empty = compatible).
    pub fn compatibility_problems(&self) -> Result<Vec<String>, Error> {
        Ok(db::check(&self.con)?)
    }

    pub fn tracks(&self) -> Result<Vec<Track>, Error> {
        let mut st = self.con.prepare(
            "SELECT track_id, IFNULL(title,''), IFNULL(artist,''), IFNULL(album,''), IFNULL(genre,''),
                    IFNULL(bpm,0), IFNULL(key,''), IFNULL(play_time_secs,0), IFNULL(rating,0),
                    IFNULL(location,''), IFNULL(file_size,0), IFNULL(format,''),
                    IFNULL(track_flags,0), IFNULL(times_played,0), IFNULL(last_played,-1),
                    year, CAST(IFNULL(label,'') AS TEXT), CAST(IFNULL(remixer,'') AS TEXT),
                    CAST(IFNULL(composer,'') AS TEXT), CAST(IFNULL(comments,'') AS TEXT), date_added, bit_rate
             FROM tracks WHERE track_id >= 0 ORDER BY track_id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Track {
                track_id: r.get(0)?,
                title: r.get(1)?,
                artist: r.get(2)?,
                album: r.get(3)?,
                genre: r.get(4)?,
                bpm: r.get(5)?,
                key: r.get(6)?,
                play_time_secs: r.get(7)?,
                rating: r.get(8)?,
                location: r.get(9)?,
                file_size: r.get(10)?,
                format: r.get(11)?,
                analysed: r.get::<_, i64>(12)? & 2 != 0,
                times_played: r.get(13)?,
                last_played: r.get(14)?,
                year: lenient_i64(r, 15, 0)?.max(0),
                label: r.get(16)?,
                remixer: r.get(17)?,
                composer: r.get(18)?,
                comments: r.get(19)?,
                date_added: lenient_i64(r, 20, -1)?,
                bit_rate: lenient_i64(r, 21, 0)?.max(0),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mixes(&self) -> Result<Vec<Collection>, Error> {
        self.collections(
            "SELECT m.mix_id, IFNULL(m.name,''), IFNULL(m.play_time_secs,0),
                    (SELECT COUNT(*) FROM mixtracks t WHERE t.mix_id = m.mix_id)
             FROM mixes m WHERE m.mix_id >= 0 ORDER BY m.name",
        )
    }

    pub fn cases(&self) -> Result<Vec<Collection>, Error> {
        self.collections(
            "SELECT c.case_id, IFNULL(c.name,''), IFNULL(c.play_time_secs,0),
                    (SELECT COUNT(*) FROM casetracks t WHERE t.case_id = c.case_id)
             FROM cases c WHERE c.case_id >= 0 ORDER BY c.name",
        )
    }

    fn collections(&self, sql: &str) -> Result<Vec<Collection>, Error> {
        let mut st = self.con.prepare(sql)?;
        let rows = st.query_map([], |r| {
            Ok(Collection { id: r.get(0)?, name: r.get(1)?, play_time_secs: r.get(2)?, track_count: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The id of the track `source` would become, if it is already on the volume.
    pub fn existing_track(&self, source: &Path) -> Result<Option<i64>, Error> {
        let Some(name) = source.file_name().and_then(|n| n.to_str()) else { return Ok(None) };
        let (_, location) = naming::device_paths(name, std::fs::metadata(source)?.len());
        Ok(self
            .con
            .query_row("SELECT track_id FROM tracks WHERE location = ?1", [&location], |r| r.get(0))
            .optional()?)
    }

    /// Copies `source` onto the volume, writes its `.str` and inserts the track row.
    /// Returns the new `track_id`. If a track with the same device location already exists,
    /// returns its id without copying again (adding is idempotent).
    pub fn add_track(&mut self, source: &Path, meta: &TrackMeta, analysis: &strfile::Analysis) -> Result<i64, Error> {
        let file_name = source.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "source has no UTF-8 file name"))
        })?;
        let size = std::fs::metadata(source)?.len();
        let (rel, location) = naming::device_paths(file_name, size);
        if let Some(id) = self
            .con
            .query_row("SELECT track_id FROM tracks WHERE location = ?1", [&location], |r| r.get(0))
            .optional()?
        {
            return Ok(id);
        }
        let dest = self.root.join(naming::MUSIC_DIR).join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // temp file + fsync + rename for both files, so a cable pulled mid-copy leaves no
        // truncated file under a real name (a leftover `.tmp` is an unreferenced file the
        // clean-up lists). Both are removed again if anything later fails.
        let str_dest = PathBuf::from(naming::str_path(&dest.to_string_lossy()));
        atomic_copy(source, &dest)?;
        if let Err(e) = atomic_write(&str_dest, strfile::encode(analysis, strfile::Newline::Crlf).as_bytes()) {
            let _ = std::fs::remove_file(&dest);
            return Err(e.into());
        }

        let bpm = track_bpm(analysis);
        let now = now_ms();
        let lower = |s: &str| s.to_lowercase();
        let ind_bpm = ind_bpm_of(bpm);
        let res = self.con.execute(
            "INSERT INTO tracks (artist, album_artist, title, composer, album, track_number, year, genre,
                is_part_of_c, location, format, bit_rate, sample_rate, file_size, play_time_secs, rating,
                comments, date_added, last_played, times_played, cue_point, rc_mixes, bpm, label,
                track_flags, global_id, loop_in, loop_out, structured_ct, ind_title, ind_artist,
                ind_album, ind_genre, ind_bpm, discid, producer, remixer, key, number_of_tracks,
                disc_number, number_of_discs, date_modified, modified_by_ed, analyzed_by_ed, analysis_ver)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?12, ?13, ?14, 0, ?15, ?16, -1, 0, 18446744073709551615.0,
                0, ?17, ?18, 2, ?19, -1, -1, '', ?20, ?21, ?22, ?23, ?24, '', ?25, ?26, ?27, 0, ?28, 0,
                ?16, ?29, ?29, 1)",
            params![
                meta.artist,
                meta.album_artist,
                meta.title,
                meta.composer,
                meta.album,
                meta.track_number,
                meta.year,
                meta.genre,
                location,
                meta.format,
                meta.bit_rate_kbps,
                meta.sample_rate,
                size as i64,
                meta.play_time_secs,
                meta.comment,
                now,
                bpm,
                meta.label,
                uuid::Uuid::new_v4().simple().to_string(),
                lower(&meta.title),
                lower(&meta.artist),
                lower(&meta.album),
                lower(&meta.genre),
                ind_bpm,
                meta.producer,
                meta.remixer,
                meta.key,
                meta.disc_number,
                EDITOR_ID,
            ],
        );
        match res {
            Ok(_) => Ok(self.con.last_insert_rowid()),
            Err(e) => {
                // don't leave orphaned files on the volume when the row can't be written
                let _ = std::fs::remove_file(&dest);
                let _ = std::fs::remove_file(&str_dest);
                Err(e.into())
            }
        }
    }

    /// `(audio file, .str sidecar)` on this volume for a `tracks.location`, or `None` when the
    /// location is not a device-side `/pmdb_tracks/...` path.
    pub fn track_paths(&self, location: &str) -> Option<(PathBuf, PathBuf)> {
        let rel = location.strip_prefix(naming::LOCATION_PREFIX)?;
        let audio = self.root.join(naming::MUSIC_DIR).join(rel);
        let sidecar = PathBuf::from(naming::str_path(&audio.to_string_lossy()));
        Some((audio, sidecar))
    }

    /// Replaces the `.str` of an existing track and updates its row (bpm, analysed flag). The
    /// audio file is not touched. `location` must be the track's `tracks.location`.
    pub fn store_analysis(&mut self, track_id: i64, location: &str, analysis: &strfile::Analysis) -> Result<(), Error> {
        let (_, sidecar) = self.track_paths(location).ok_or_else(|| {
            Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "track is not stored under /pmdb_tracks/"))
        })?;
        // the file and the row must describe the same grid: keep the old file's bytes so that a row
        // update that fails (busy timeout, unplug) puts the old grid back instead of leaving the
        // new .str next to the old bpm / flags
        let previous = match std::fs::read(&sidecar) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        atomic_write(&sidecar, strfile::encode(analysis, strfile::Newline::Crlf).as_bytes())?;
        let bpm = track_bpm(analysis);
        let ind_bpm = ind_bpm_of(bpm);
        let updated = self.con.execute(
            "UPDATE tracks SET bpm = ?1, ind_bpm = ?2, track_flags = IFNULL(track_flags,0) | 2,
                    date_modified = ?3, modified_by_ed = ?4, analyzed_by_ed = ?4, analysis_ver = 1
             WHERE track_id = ?5",
            params![bpm, ind_bpm, now_ms(), EDITOR_ID, track_id],
        );
        if let Err(e) = updated {
            let _ = match previous {
                Some(old) => atomic_write(&sidecar, &old),
                None => std::fs::remove_file(&sidecar),
            };
            return Err(e.into());
        }
        Ok(())
    }

    /// Sets a track's star rating (0 = unrated, up to [`MAX_RATING`]) the way the Editor's
    /// rating menu does: `UPDATE tracks SET rating = N`, plus `date_modified` (Unix ms) and
    /// `modified_by_ed`, but only when the value actually changes. Returns whether the row
    /// changed. Errors on an out-of-range rating or an unknown (or negative) track id.
    pub fn set_track_rating(&mut self, track_id: i64, stars: u8) -> Result<bool, Error> {
        if stars > MAX_RATING {
            return Err(Error::Invalid(format!("rating {stars} is above {MAX_RATING}")));
        }
        let current: Option<i64> = self
            .con
            .query_row("SELECT IFNULL(rating,0) FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |r| r.get(0))
            .optional()?;
        let Some(current) = current else {
            return Err(Error::Invalid(format!("no track {track_id}")));
        };
        if current == i64::from(stars) {
            return Ok(false);
        }
        self.con.execute(
            "UPDATE tracks SET rating = ?1, date_modified = ?2, modified_by_ed = ?3 WHERE track_id = ?4",
            params![stars, now_ms(), EDITOR_ID, track_id],
        )?;
        Ok(true)
    }

    /// Creates an empty case and returns its `case_id`. The row matches what the Editor's
    /// `CaseLibraryComponent::createCase` writes: `date_created` = `date_modified` = now (Unix
    /// ms), `year` = the current year, `last_played` 0, `structured_ct` NULL, everything else
    /// zero or empty. `case_id` is left to SQLite (the column is AUTOINCREMENT, as on the
    /// device, so ids of deleted cases are never reused). Duplicate names are allowed, as on
    /// the device; empty names and names over [`MAX_CASE_NAME_CHARS`] are rejected.
    pub fn create_case(&mut self, name: &str) -> Result<i64, Error> {
        insert_case_row(&self.con, name)
    }

    /// Replaces a case's membership with `track_ids`, in that order (the device lists a case
    /// in `casetracks` rowid order). Repeated ids are kept once, at their first position, and
    /// every id must be a real track. Like the device's `insertCasetracks`/`deleteCasetracks`
    /// it refreshes the derived fields in the same transaction: `date_modified` = now and
    /// `play_time_secs` = the sum of the members' `play_time_secs` (0 for an empty case).
    pub fn set_case_tracks(&mut self, case_id: i64, track_ids: &[i64]) -> Result<(), Error> {
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<i64> = track_ids.iter().copied().filter(|id| seen.insert(*id)).collect();
        let tx = self.con.transaction()?;
        if tx.query_row("SELECT 1 FROM cases WHERE case_id = ?1 AND case_id >= 0", [case_id], |_| Ok(())).optional()?.is_none() {
            return Err(Error::Invalid(format!("no case {case_id}")));
        }
        for id in &ids {
            if tx.query_row("SELECT 1 FROM tracks WHERE track_id = ?1 AND track_id >= 0", [id], |_| Ok(())).optional()?.is_none() {
                return Err(Error::Invalid(format!("no track {id}")));
            }
        }
        tx.execute("DELETE FROM casetracks WHERE case_id = ?1", [case_id])?;
        {
            let mut ins = tx.prepare("INSERT INTO casetracks (case_id, track_id) VALUES (?1, ?2)")?;
            for id in &ids {
                ins.execute(params![case_id, id])?;
            }
        }
        tx.execute(
            "UPDATE cases SET date_modified = ?1,
                    play_time_secs = (SELECT IFNULL(SUM(t.play_time_secs), 0) FROM casetracks c
                                      JOIN tracks t ON t.track_id = c.track_id WHERE c.case_id = ?2)
             WHERE case_id = ?2",
            params![now_ms(), case_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a case and its membership rows (the device's `deleteCase` runs the same two
    /// DELETEs, `casetracks` first). Returns whether the case existed.
    pub fn delete_case(&mut self, case_id: i64) -> Result<bool, Error> {
        let tx = self.con.transaction()?;
        tx.execute("DELETE FROM casetracks WHERE case_id = ?1", [case_id])?;
        let n = tx.execute("DELETE FROM cases WHERE case_id = ?1", [case_id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Ids of the tracks in a case, in the order they were added.
    pub fn case_track_ids(&self, case_id: i64) -> Result<Vec<i64>, Error> {
        let mut st = self.con.prepare("SELECT track_id FROM casetracks WHERE case_id = ?1 ORDER BY rowid")?;
        let rows = st.query_map([case_id], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

// ---- Cases: metadata, membership edits, favourites ----------------------------------------

#[cfg(test)]
mod case_ops_tests;

mod track_extra;
mod dupes;
pub use dupes::{check_disjoint, plan_group, Blocked, CaseEdit, CaseEditKind, CaseHold, Change, Computed, DupRow, Facts, GroupPlan, MergeGroup, MergeOptions, PlaysRule, RatingRule, RemovedRow, StatEdit, TrackHolds};
pub use track_extra::TrackExtra;

mod case_bulk;
pub use case_bulk::{plan_merge, plan_transfer, MergeDest, MergeDone, MergePlan, TransferDone, TransferNewDone, TransferPlan};
#[cfg(test)]
mod case_bulk_tests;

impl Library {
    /// Every case with its metadata, ordered by name (the same order as [`Library::cases`]).
    pub fn case_infos(&self) -> Result<Vec<CaseInfo>, Error> {
        let mut st = self.con.prepare(&format!("{CASE_INFO_SELECT} WHERE c.case_id >= 0 ORDER BY c.name"))?;
        let rows = st.query_map([], case_info_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// One case's metadata, or `None` if there is no such case.
    pub fn case_info(&self, case_id: i64) -> Result<Option<CaseInfo>, Error> {
        Ok(self
            .con
            .query_row(&format!("{CASE_INFO_SELECT} WHERE c.case_id = ?1 AND c.case_id >= 0"), [case_id], case_info_row)
            .optional()?)
    }

    /// The first (lowest `case_id`) case with exactly this name. Names are not unique on the
    /// device; the Editor's importers merge by name the same way.
    pub fn case_id_by_name(&self, name: &str) -> Result<Option<i64>, Error> {
        Ok(self
            .con
            .query_row("SELECT case_id FROM cases WHERE name = ?1 AND case_id >= 0 ORDER BY case_id LIMIT 1", [name], |r| r.get(0))
            .optional()?)
    }

    /// Creates a case already holding `track_ids` (in order, repeats once). Every id is
    /// checked before anything is written, so a bad id leaves the library untouched.
    pub fn create_case_with_tracks(&mut self, name: &str, track_ids: &[i64]) -> Result<i64, Error> {
        check_case_name(name)?;
        for id in track_ids {
            self.require_track(*id)?;
        }
        let case_id = self.create_case(name)?;
        if let Err(e) = self.set_case_tracks(case_id, track_ids) {
            let _ = self.delete_case(case_id);
            return Err(e);
        }
        Ok(case_id)
    }

    /// Appends tracks to the end of a case, skipping any it already holds and repeats in
    /// `track_ids` (existing rows keep their `rowid`, so the current order is untouched). Like
    /// the Editor's insert it refreshes `date_modified` and `play_time_secs` in the same
    /// transaction. Returns how many were added; an unknown case or track errors and nothing
    /// is written.
    pub fn add_case_tracks(&mut self, case_id: i64, track_ids: &[i64]) -> Result<usize, Error> {
        let tx = self.con.transaction()?;
        require_case(&tx, case_id)?;
        let mut have: std::collections::HashSet<i64> = {
            let mut st = tx.prepare("SELECT track_id FROM casetracks WHERE case_id = ?1")?;
            let rows = st.query_map([case_id], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        let mut added = 0;
        {
            let mut ins = tx.prepare("INSERT INTO casetracks (case_id, track_id) VALUES (?1, ?2)")?;
            for id in track_ids {
                require_track_in(&tx, *id)?;
                if have.insert(*id) {
                    ins.execute(params![case_id, id])?;
                    added += 1;
                }
            }
        }
        if added > 0 {
            refresh_case_derived(&tx, case_id)?;
        }
        tx.commit()?;
        Ok(added)
    }

    /// Removes tracks from a case (`DELETE ... WHERE case_id = X AND track_id = Y`, as the
    /// Editor does); the rest keep their order. Returns how many rows went. An unknown case
    /// errors.
    pub fn remove_case_tracks(&mut self, case_id: i64, track_ids: &[i64]) -> Result<usize, Error> {
        let tx = self.con.transaction()?;
        require_case(&tx, case_id)?;
        let mut removed = 0;
        for id in track_ids {
            removed += tx.execute("DELETE FROM casetracks WHERE case_id = ?1 AND track_id = ?2", params![case_id, id])?;
        }
        if removed > 0 {
            refresh_case_derived(&tx, case_id)?;
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Adds the track to the case if it is not in it, removes it if it is; returns whether it
    /// is in the case afterwards.
    pub fn toggle_case_track(&mut self, case_id: i64, track_id: i64) -> Result<bool, Error> {
        if self.case_track_ids(case_id)?.contains(&track_id) {
            self.remove_case_tracks(case_id, &[track_id])?;
            Ok(false)
        } else {
            self.add_case_tracks(case_id, &[track_id])?;
            Ok(true)
        }
    }

    /// Edits a case's name and metadata (the fields of the Editor's `EditCaseMetaDataComponent`:
    /// name, genre, year, rating, producer, remixer, comments). The name follows the
    /// [`Library::create_case`] rules and the rating is 0 to [`MAX_RATING`]. `date_modified` is
    /// set to now only when something actually changed (the rule `modifyTrack` follows for
    /// tracks; that the case dialog behaves alike is inferred, not verified). Returns whether
    /// the row changed.
    pub fn set_case_meta(&mut self, case_id: i64, meta: &CaseMeta) -> Result<bool, Error> {
        check_case_name(&meta.name)?;
        if meta.rating > MAX_RATING {
            return Err(Error::Invalid(format!("rating {} is above {MAX_RATING}", meta.rating)));
        }
        if !(0..=9999).contains(&meta.year) {
            return Err(Error::Invalid(format!("year {} is out of range", meta.year)));
        }
        let Some(cur) = self.case_info(case_id)? else {
            return Err(Error::Invalid(format!("no case {case_id}")));
        };
        if cur.meta() == *meta {
            return Ok(false);
        }
        self.con.execute(
            "UPDATE cases SET name = ?1, genre = ?2, year = ?3, rating = ?4, producer = ?5, remixer = ?6,
                    comments = ?7, date_modified = ?8 WHERE case_id = ?9",
            params![meta.name, meta.genre, meta.year, meta.rating, meta.producer, meta.remixer, meta.comments, now_ms(), case_id],
        )?;
        Ok(true)
    }

    /// Renames a case, leaving its other metadata alone. Returns whether the name changed.
    pub fn rename_case(&mut self, case_id: i64, name: &str) -> Result<bool, Error> {
        let Some(cur) = self.case_info(case_id)? else {
            return Err(Error::Invalid(format!("no case {case_id}")));
        };
        self.set_case_meta(case_id, &CaseMeta { name: name.to_string(), ..cur.meta() })
    }

    /// Adds the track to the "Favourites" case (created on first use) or removes it; returns
    /// whether it is now a favourite. Errors on an unknown track before creating anything.
    pub fn toggle_favourite(&mut self, track_id: i64) -> Result<bool, Error> {
        self.require_track(track_id)?;
        let case_id = match self.case_id_by_name(FAVOURITES_CASE_NAME)? {
            Some(id) => id,
            None => self.create_case(FAVOURITES_CASE_NAME)?,
        };
        self.toggle_case_track(case_id, track_id)
    }

    /// Ids in the "Favourites" case, in case order; empty if the case does not exist.
    pub fn favourite_track_ids(&self) -> Result<Vec<i64>, Error> {
        match self.case_id_by_name(FAVOURITES_CASE_NAME)? {
            Some(id) => self.case_track_ids(id),
            None => Ok(Vec::new()),
        }
    }

    fn require_track(&self, track_id: i64) -> Result<(), Error> {
        require_track_in(&self.con, track_id)
    }
}

/// The case that holds a library's favourites, created on first use. The same name is used on
/// the device, so a favourite also shows up in the device's own case list.
pub const FAVOURITES_CASE_NAME: &str = "Favourites";

/// A case with the metadata the Editor's case table and dialog show.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaseInfo {
    pub id: i64,
    pub name: String,
    pub genre: String,
    pub year: i64,
    pub rating: i64,
    pub producer: String,
    pub remixer: String,
    pub comments: String,
    pub date_created: i64,
    pub date_modified: i64,
    pub times_played: i64,
    /// Unix milliseconds, 0 = never (cases store 0, tracks -1).
    pub last_played: i64,
    pub play_time_secs: i64,
    pub track_count: i64,
}

impl CaseInfo {
    /// The editable part, for [`Library::set_case_meta`].
    pub fn meta(&self) -> CaseMeta {
        CaseMeta {
            name: self.name.clone(),
            genre: self.genre.clone(),
            year: self.year,
            rating: self.rating.clamp(0, i64::from(MAX_RATING)) as u8,
            producer: self.producer.clone(),
            remixer: self.remixer.clone(),
            comments: self.comments.clone(),
        }
    }
}

/// What the case metadata dialog edits.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct CaseMeta {
    pub name: String,
    pub genre: String,
    pub year: i64,
    pub rating: u8,
    pub producer: String,
    pub remixer: String,
    pub comments: String,
}

const CASE_INFO_SELECT: &str = "SELECT c.case_id, IFNULL(c.name,''), IFNULL(c.genre,''), IFNULL(c.year,0), IFNULL(c.rating,0),
        IFNULL(c.producer,''), IFNULL(c.remixer,''), IFNULL(c.comments,''), IFNULL(c.date_created,0),
        IFNULL(c.date_modified,0), IFNULL(c.times_played,0), IFNULL(c.last_played,0), IFNULL(c.play_time_secs,0),
        (SELECT COUNT(*) FROM casetracks t WHERE t.case_id = c.case_id)
     FROM cases c";

fn case_info_row(r: &rusqlite::Row) -> rusqlite::Result<CaseInfo> {
    Ok(CaseInfo {
        id: r.get(0)?,
        name: r.get(1)?,
        genre: r.get(2)?,
        year: r.get(3)?,
        rating: r.get(4)?,
        producer: r.get(5)?,
        remixer: r.get(6)?,
        comments: r.get(7)?,
        date_created: r.get(8)?,
        date_modified: r.get(9)?,
        times_played: r.get(10)?,
        last_played: r.get(11)?,
        play_time_secs: r.get(12)?,
        track_count: r.get(13)?,
    })
}

/// The row [`Library::create_case`] writes, on any connection or open transaction (the bulk
/// case operations in `case_bulk.rs` create a case inside their own transaction).
fn insert_case_row(con: &Connection, name: &str) -> Result<i64, Error> {
    check_case_name(name)?;
    let now = now_ms();
    con.execute(
        "INSERT INTO cases (case_id, name, date_created, times_played, structured_ct, image_id, genre, year,
            rating, producer, remixer, play_time_secs, last_played, date_modified, comments, creator_id)
         VALUES (NULL, ?1, ?2, 0, NULL, 0, '', ?3, 0, '', '', 0, 0, ?2, '', ?4)",
        params![name, now, year_of_ms(now), CASE_CREATOR_ID],
    )?;
    Ok(con.last_insert_rowid())
}

fn check_case_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        return Err(Error::Invalid("case name is empty".into()));
    }
    if name.chars().count() > MAX_CASE_NAME_CHARS {
        return Err(Error::Invalid(format!("case name is longer than {MAX_CASE_NAME_CHARS} characters")));
    }
    Ok(())
}

fn require_case(con: &Connection, case_id: i64) -> Result<(), Error> {
    match con.query_row("SELECT 1 FROM cases WHERE case_id = ?1 AND case_id >= 0", [case_id], |_| Ok(())).optional()? {
        Some(()) => Ok(()),
        None => Err(Error::Invalid(format!("no case {case_id}"))),
    }
}

fn require_track_in(con: &Connection, track_id: i64) -> Result<(), Error> {
    match con.query_row("SELECT 1 FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |_| Ok(())).optional()? {
        Some(()) => Ok(()),
        None => Err(Error::Invalid(format!("no track {track_id}"))),
    }
}

/// After a membership change: `date_modified` = now and `play_time_secs` = the members' sum
/// (`updateCaseDateModified` + `updateCasePlayTimeSecs`, FINDINGS 4d).
fn refresh_case_derived(con: &Connection, case_id: i64) -> Result<(), Error> {
    con.execute(
        "UPDATE cases SET date_modified = ?1,
                play_time_secs = (SELECT IFNULL(SUM(t.play_time_secs), 0) FROM casetracks c
                                  JOIN tracks t ON t.track_id = c.track_id WHERE c.case_id = ?2)
         WHERE case_id = ?2",
        params![now_ms(), case_id],
    )?;
    Ok(())
}

/// `mixes.name` is declared VARCHAR(255); SQLite does not enforce it, so we do.
pub const MAX_MIX_NAME_CHARS: usize = 255;

/// What we write as `mixes.creator_id`; same shape and reasoning as [`CASE_CREATOR_ID`]
/// (the Editor's `Utils::getCreatorId` feeds both).
pub const MIX_CREATOR_ID: &str = CASE_CREATOR_ID;

/// A row of `mixes` without the document blob.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MixInfo {
    pub id: i64,
    pub name: String,
    pub genre: String,
    pub year: i64,
    pub rating: i64,
    pub producer: String,
    pub remixer: String,
    pub comments: String,
    /// Unix milliseconds.
    pub date_created: i64,
    pub date_modified: i64,
    pub times_played: i64,
    pub last_played: i64,
    pub play_time_secs: i64,
    pub creator_id: String,
    /// `miximages.id` of the sleeve, 0 for none.
    pub image_id: i64,
    pub preview_markers: String,
    pub track_count: i64,
}

/// A row of `mixtracks`. A negative `track_id` is the placeholder the Editor leaves when a
/// track is removed from the library (notes/FINDINGS.md section 3); the `dup_*` fields then
/// hold what the track was called.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MixTrackRow {
    pub track_id: i64,
    pub dup_title: Option<String>,
    pub dup_artist: Option<String>,
    pub dup_album: Option<String>,
    pub dup_play_time_secs: Option<i64>,
    pub track_index: i64,
    pub cue_point: i64,
}

/// The row columns of a mix that the metadata dialog edits (`EditMixMetaDataComponent`); the
/// name has its own path ([`Library::rename_mix`]) and the document is never touched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MixMetaUpdate {
    pub genre: String,
    pub year: i64,
    pub rating: i64,
    pub producer: String,
    pub remixer: String,
    pub comments: String,
}

/// What the mix editor needs to know about a library track besides its mix-document entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MixTrackSource {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub bpm: f64,
    pub key: String,
    pub play_time_secs: i64,
    pub location: String,
    /// `tracks.sample_rate` of the audio file (the `.str` beat positions are frames at this rate).
    pub sample_rate: i64,
    pub analysed: bool,
}

impl From<crate::mix::MixError> for Error {
    fn from(e: crate::mix::MixError) -> Self {
        Error::Invalid(e.to_string())
    }
}

impl Library {
    /// Every mix row, ordered by name.
    pub fn mix_infos(&self) -> Result<Vec<MixInfo>, Error> {
        let mut st = self.con.prepare(&format!("{MIX_INFO_SQL} WHERE m.mix_id >= 0 ORDER BY m.name"))?;
        let rows = st.query_map([], mix_info_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mix_info(&self, mix_id: i64) -> Result<Option<MixInfo>, Error> {
        Ok(self
            .con
            .query_row(&format!("{MIX_INFO_SQL} WHERE m.mix_id = ?1 AND m.mix_id >= 0"), [mix_id], mix_info_row)
            .optional()?)
    }

    /// The `mixtracks` rows of a mix in `track_index` order.
    /// `(mix_id, name)` of every mix that has the track in it, by name. Used before a beat grid
    /// is saved: a mix clip stores the track's tempo as it was when the mix was made, so a new
    /// grid leaves those mixes out of step until they are beatmatched again.
    pub fn mixes_using_track(&self, track_id: i64) -> Result<Vec<(i64, String)>, Error> {
        let mut st = self.con.prepare(
            "SELECT DISTINCT m.mix_id, IFNULL(m.name,'') FROM mixes m JOIN mixtracks t ON t.mix_id = m.mix_id
             WHERE t.track_id = ?1 ORDER BY m.name COLLATE NOCASE, m.mix_id",
        )?;
        let rows = st.query_map([track_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mix_track_rows(&self, mix_id: i64) -> Result<Vec<MixTrackRow>, Error> {
        let mut st = self.con.prepare(
            "SELECT track_id, dup_title, dup_artist, dup_album, dup_play_time_secs, IFNULL(track_index,-1),
                    IFNULL(cue_point,-1)
             FROM mixtracks WHERE mix_id = ?1 ORDER BY track_index, rowid",
        )?;
        let rows = st.query_map([mix_id], |r| {
            Ok(MixTrackRow {
                track_id: r.get(0)?,
                dup_title: r.get(1)?,
                dup_artist: r.get(2)?,
                dup_album: r.get(3)?,
                dup_play_time_secs: r.get(4)?,
                track_index: r.get(5)?,
                cue_point: r.get::<_, f64>(6)? as i64,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Reads and parses the document of a mix. Errors if the mix does not exist or has no
    /// document (`structured_ct` NULL or empty).
    pub fn read_mix(&self, mix_id: i64) -> Result<crate::mix::Mix, Error> {
        let blob: Option<Option<Vec<u8>>> = self
            .con
            .query_row("SELECT structured_ct FROM mixes WHERE mix_id = ?1 AND mix_id >= 0", [mix_id], |r| r.get(0))
            .optional()?;
        match blob {
            None => Err(Error::Invalid(format!("no mix {mix_id}"))),
            Some(None) => Err(Error::Invalid(format!("mix {mix_id} has no document"))),
            Some(Some(b)) if b.is_empty() => Err(Error::Invalid(format!("mix {mix_id} has no document"))),
            Some(Some(b)) => Ok(crate::mix::Mix::from_blob(&b)?),
        }
    }

    /// Creates a mix with an empty document and returns its `mix_id`, the way the Editor's
    /// `MixLibraryComponent::createMix` does: a row with `date_created` = now, `year` = the
    /// current year, zero counters, then a first save of an empty document (which sets
    /// `date_modified`, the blob and `preview_markers`). Names must be non-empty, at most
    /// [`MAX_MIX_NAME_CHARS`] characters and unused (the Editor's Save As refuses a name that
    /// exists).
    pub fn create_mix(&mut self, name: &str) -> Result<i64, Error> {
        check_mix_name(name)?;
        if self.mix_name_taken(name, None)? {
            return Err(Error::Invalid(format!("a mix called {name:?} already exists")));
        }
        let now = now_ms();
        self.con.execute(
            "INSERT INTO mixes (mix_id, name, date_created, times_played, structured_ct, mix_image, genre, year,
                rating, producer, remixer, play_time_secs, last_played, date_modified, comments, creator_id,
                preview_markers)
             VALUES (NULL, ?1, ?2, 0, NULL, 0, '', ?3, 0, '', '', 0, 0, 0, '', ?4, NULL)",
            params![name, now, year_of_ms(now), MIX_CREATOR_ID],
        )?;
        let id = self.con.last_insert_rowid();
        self.save_mix(id, &crate::mix::Mix::new(name))?;
        Ok(id)
    }

    /// Saves `mix` as the document of `mix_id`, with everything the Editor's `saveMixWithId`
    /// updates in one transaction: the row's `name` (taken from `mix.name`), `play_time_secs`,
    /// `preview_markers`, `date_modified` (now, Unix ms) and the blob, then `mixtracks`
    /// rebuilt from the document's tracks in order (`track_index` = position in the list).
    /// Every `library_id` must be a real track, or a negative placeholder id that the mix
    /// already had a row for (its `dup_*` fields are kept). Genre/year/rating/... stay as they
    /// are (the Editor edits those in its metadata dialog, not on save).
    pub fn save_mix(&mut self, mix_id: i64, mix: &crate::mix::Mix) -> Result<(), Error> {
        check_mix_name(&mix.name)?;
        let old_rows = self.mix_track_rows(mix_id)?;
        if self.mix_info(mix_id)?.is_none() {
            return Err(Error::Invalid(format!("no mix {mix_id}")));
        }
        if self.mix_name_taken(&mix.name, Some(mix_id))? {
            return Err(Error::Invalid(format!("a mix called {:?} already exists", mix.name)));
        }
        for t in &mix.tracks {
            let id = t.library_id;
            let known = if id >= 0 {
                self.con.query_row("SELECT 1 FROM tracks WHERE track_id = ?1", [id], |_| Ok(())).optional()?.is_some()
            } else {
                old_rows.iter().any(|r| r.track_id == id)
            };
            if !known {
                return Err(Error::Invalid(format!("mix track refers to library id {id}, which is not a track")));
            }
        }
        let blob = mix.to_blob()?;
        let now = now_ms();
        let tx = self.con.transaction()?;
        tx.execute(
            "UPDATE mixes SET name = ?1, play_time_secs = ?2, preview_markers = ?3, date_modified = ?4,
                    structured_ct = ?5 WHERE mix_id = ?6",
            params![mix.name, mix.play_time_secs(), mix.preview_markers_xml(), now, blob, mix_id],
        )?;
        tx.execute("DELETE FROM mixtracks WHERE mix_id = ?1", [mix_id])?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO mixtracks (mix_id, track_id, dup_title, dup_artist, dup_album, dup_play_time_secs,
                    track_index, cue_point) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (i, t) in mix.tracks.iter().enumerate() {
                let kept = old_rows.iter().find(|r| r.track_id == t.library_id && t.library_id < 0);
                ins.execute(params![
                    mix_id,
                    t.library_id,
                    kept.and_then(|r| r.dup_title.clone()),
                    kept.and_then(|r| r.dup_artist.clone()),
                    kept.and_then(|r| r.dup_album.clone()),
                    kept.and_then(|r| r.dup_play_time_secs),
                    i as i64,
                    mix_cue_point(t)
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes a mix: its sleeve image (when it has one), `mixtracks` rows and the row itself,
    /// the three statements the Editor's `deleteMix` path runs. Returns whether it existed.
    pub fn delete_mix(&mut self, mix_id: i64) -> Result<bool, Error> {
        let tx = self.con.transaction()?;
        tx.execute(
            "DELETE FROM miximages WHERE EXISTS (SELECT mixes.mix_image FROM mixes
                    WHERE mixes.mix_image = miximages.id AND mixes.mix_id = ?1)",
            [mix_id],
        )?;
        tx.execute("DELETE FROM mixtracks WHERE mix_id = ?1", [mix_id])?;
        let n = tx.execute("DELETE FROM mixes WHERE mix_id = ?1", [mix_id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Renames a mix: the row and `DocumentName` change together (the Editor keeps them equal),
    /// through [`Library::save_mix`], so `date_modified` moves and `mixtracks` are rewritten
    /// exactly as for any save. Same name rules as [`Library::create_mix`].
    pub fn rename_mix(&mut self, mix_id: i64, name: &str) -> Result<(), Error> {
        let mut mix = self.read_mix(mix_id)?;
        mix.name = name.to_string();
        self.save_mix(mix_id, &mix)
    }

    /// Updates the metadata columns of a mix (genre, year, rating 0..=[`MAX_RATING`], producer,
    /// remixer, comments) and `date_modified`; the document and `mixtracks` stay as they are.
    pub fn set_mix_meta(&mut self, mix_id: i64, meta: &MixMetaUpdate) -> Result<(), Error> {
        if !(0..=i64::from(MAX_RATING)).contains(&meta.rating) {
            return Err(Error::Invalid(format!("mix rating {} is outside 0..={MAX_RATING}", meta.rating)));
        }
        if !(0..=9999).contains(&meta.year) {
            return Err(Error::Invalid(format!("mix year {} is outside 0..=9999", meta.year)));
        }
        let n = self.con.execute(
            "UPDATE mixes SET genre = ?1, year = ?2, rating = ?3, producer = ?4, remixer = ?5, comments = ?6,
                    date_modified = ?7 WHERE mix_id = ?8 AND mix_id >= 0",
            params![meta.genre, meta.year, meta.rating, meta.producer, meta.remixer, meta.comments, now_ms(), mix_id],
        )?;
        if n == 0 {
            return Err(Error::Invalid(format!("no mix {mix_id}")));
        }
        Ok(())
    }

    /// Library rows for the given track ids, in the order asked; ids that are not tracks
    /// (including the negative placeholders of removed tracks) are skipped.
    pub fn mix_track_sources(&self, ids: &[i64]) -> Result<Vec<MixTrackSource>, Error> {
        let mut st = self.con.prepare(
            "SELECT track_id, IFNULL(title,''), IFNULL(artist,''), IFNULL(album,''), IFNULL(genre,''),
                    IFNULL(bpm,0), IFNULL(key,''), IFNULL(play_time_secs,0), IFNULL(location,''),
                    IFNULL(sample_rate,0), IFNULL(track_flags,0)
             FROM tracks WHERE track_id = ?1 AND track_id >= 0",
        )?;
        let mut out = Vec::new();
        for &id in ids {
            let row = st
                .query_row([id], |r| {
                    Ok(MixTrackSource {
                        track_id: r.get(0)?,
                        title: r.get(1)?,
                        artist: r.get(2)?,
                        album: r.get(3)?,
                        genre: r.get(4)?,
                        bpm: r.get(5)?,
                        key: r.get(6)?,
                        play_time_secs: r.get(7)?,
                        location: r.get(8)?,
                        sample_rate: r.get(9)?,
                        analysed: r.get::<_, i64>(10)? & 2 != 0,
                    })
                })
                .optional()?;
            out.extend(row);
        }
        Ok(out)
    }

    fn mix_name_taken(&self, name: &str, except: Option<i64>) -> Result<bool, Error> {
        Ok(self
            .con
            .query_row(
                "SELECT 1 FROM mixes WHERE name = ?1 AND mix_id >= 0 AND mix_id != ?2",
                params![name, except.unwrap_or(-1)],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

fn check_mix_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        return Err(Error::Invalid("mix name is empty".into()));
    }
    if name.chars().count() > MAX_MIX_NAME_CHARS {
        return Err(Error::Invalid(format!("mix name is longer than {MAX_MIX_NAME_CHARS} characters")));
    }
    Ok(())
}

/// `mixtracks.cue_point` for a track: the source position (unstretched audio, samples at the
/// engine rate) of the track's left trim, i.e. the tempo curve applied to `LOffset`. This
/// reproduces all three rows of the Editor's demo mix (0, 10 166 523 = 9 925 797.47 x 1.0242525,
/// 9 589 191); 0 when the trim is not positive. For curves that are not constant the use of the
/// tempo map is a hypothesis (see [`crate::mix::TempoMap`]).
fn mix_cue_point(t: &crate::mix::Track) -> i64 {
    let p = crate::mix::TempoMap::new(&t.pitch).source_at(t.position.l_offset);
    if p > 0.0 { p.round() as i64 } else { 0 }
}

const MIX_INFO_SQL: &str = "SELECT m.mix_id, IFNULL(m.name,''), IFNULL(m.genre,''), IFNULL(m.year,0),
        IFNULL(m.rating,0), IFNULL(m.producer,''), IFNULL(m.remixer,''), IFNULL(m.comments,''),
        IFNULL(m.date_created,0), IFNULL(m.date_modified,0), IFNULL(m.times_played,0),
        IFNULL(m.last_played,0), IFNULL(m.play_time_secs,0), IFNULL(m.creator_id,''),
        IFNULL(m.mix_image,0), IFNULL(m.preview_markers,''),
        (SELECT COUNT(*) FROM mixtracks t WHERE t.mix_id = m.mix_id)
     FROM mixes m";

fn mix_info_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MixInfo> {
    Ok(MixInfo {
        id: r.get(0)?,
        name: r.get(1)?,
        genre: r.get(2)?,
        year: r.get(3)?,
        rating: r.get(4)?,
        producer: r.get(5)?,
        remixer: r.get(6)?,
        comments: r.get(7)?,
        date_created: r.get(8)?,
        date_modified: r.get(9)?,
        times_played: r.get(10)?,
        last_played: r.get(11)?,
        play_time_secs: r.get(12)?,
        creator_id: r.get(13)?,
        image_id: r.get(14)?,
        preview_markers: r.get(15)?,
        track_count: r.get(16)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn create_add_list_round_trip() {
        let vol = temp_dir("vol");
        let src_dir = temp_dir("src");
        let src = src_dir.join("Some Artist - Some Track.mp3");
        std::fs::write(&src, vec![0u8; 12345]).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 100,
            beats: (0..32).map(|i| strfile::Beat { pos: 100 + i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let meta = TrackMeta { title: "Some Track".into(), artist: "Some Artist".into(), genre: "House".into(), format: "MP3".into(), ..Default::default() };

        let mut lib = Library::create(&vol).unwrap();
        let id = lib.add_track(&src, &meta, &analysis).unwrap();
        assert_eq!(lib.add_track(&src, &meta, &analysis).unwrap(), id, "adding twice is idempotent");
        assert!(lib.compatibility_problems().unwrap().is_empty());
        drop(lib);

        let lib = Library::open_read_only(&vol).unwrap();
        let tracks = lib.tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        let t = &tracks[0];
        assert_eq!(t.title, "Some Track");
        assert!(t.analysed);
        assert_eq!(t.bpm, 128.0);
        let (kind, cue): (String, f64) = lib
            .con
            .query_row("SELECT typeof(cue_point), cue_point FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((kind.as_str(), cue), ("real", NO_CUE), "device-style 'no cue' sentinel");
        let (rel, loc) = naming::device_paths("Some Artist - Some Track.mp3", 12345);
        assert_eq!(t.location, loc);
        let audio = vol.join(naming::MUSIC_DIR).join(&rel);
        assert_eq!(std::fs::metadata(&audio).unwrap().len(), 12345);
        let sidecar = std::fs::read_to_string(naming::str_path(&audio.to_string_lossy())).unwrap();
        assert_eq!(strfile::decode(&sidecar).unwrap(), analysis);
        assert!(find_volumes().iter().all(|v| v != &vol), "temp dirs are not mounted volumes");

        drop(lib); // Windows won't delete a database file that is still open
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src_dir).unwrap();
    }

    #[test]
    fn tracks_carry_the_columns_the_table_sorts_by_and_survive_odd_storage() {
        let vol = temp_dir("cols-vol");
        let src_dir = temp_dir("cols-src");
        let src = src_dir.join("Demo.mp3");
        std::fs::write(&src, vec![0u8; 777]).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 120.0 }).collect(),
            first_downbeat: Some(0),
        };
        let meta = TrackMeta {
            title: "Demo".into(),
            label: "Example Label".into(),
            remixer: "Example Remixer".into(),
            composer: "Example Composer".into(),
            comment: "A note".into(),
            year: 2012,
            bit_rate_kbps: 320,
            format: "MP3".into(),
            ..Default::default()
        };
        let mut lib = Library::create(&vol).unwrap();
        let id = lib.add_track(&src, &meta, &analysis).unwrap();
        let t = lib.tracks().unwrap().remove(0);
        assert_eq!((t.year, t.bit_rate), (2012, 320));
        assert_eq!((t.label.as_str(), t.remixer.as_str(), t.composer.as_str(), t.comments.as_str()), ("Example Label", "Example Remixer", "Example Composer", "A note"));
        assert!(t.date_added > 1_500_000_000_000, "date_added is Unix milliseconds, got {}", t.date_added);

        // a row another tool wrote with text, NULLs and odd types in the numeric columns still loads
        lib.con
            .execute("UPDATE tracks SET year = '2010s', bit_rate = '256', date_added = NULL, label = 42, comments = NULL WHERE track_id = ?1", [id])
            .unwrap();
        let t = lib.tracks().unwrap().remove(0);
        assert_eq!((t.year, t.bit_rate, t.date_added), (0, 256, -1));
        assert_eq!((t.label.as_str(), t.comments.as_str()), ("42", ""));
        lib.con.execute("UPDATE tracks SET year = -5, bit_rate = 128.6 WHERE track_id = ?1", [id]).unwrap();
        let t = lib.tracks().unwrap().remove(0);
        assert_eq!((t.year, t.bit_rate), (0, 129), "a negative year is no year; a real bit rate rounds");

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src_dir).unwrap();
    }

    fn tmp_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(tmp_files(&p));
            } else if p.to_string_lossy().ends_with(".tmp") {
                out.push(p);
            }
        }
        out
    }

    fn variable_tempo() -> strfile::Analysis {
        // first beat's own tempo (131.0) is far from the median (135.0): the two derivations differ
        let bpms = [131.0, 135.0, 135.0, 135.5, 136.0, 134.0, 135.0];
        strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 100,
            beats: bpms.iter().enumerate().map(|(i, b)| strfile::Beat { pos: 100 + i as i64 * 20000, bpm: *b }).collect(),
            first_downbeat: Some(0),
        }
    }

    #[test]
    fn add_track_and_store_analysis_store_the_same_bpm() {
        let vol = temp_dir("bpm-vol");
        let src_dir = temp_dir("bpm-src");
        let src = src_dir.join("Variable.mp3");
        std::fs::write(&src, vec![0u8; 2222]).unwrap();
        let analysis = variable_tempo();
        assert_ne!(analysis.beats[0].bpm, track_bpm(&analysis), "the fixture must tell the two apart");
        let mut lib = Library::create(&vol).unwrap();
        let id = lib.add_track(&src, &TrackMeta { title: "v".into(), format: "MP3".into(), ..Default::default() }, &analysis).unwrap();
        let row = |lib: &Library| -> (f64, i64) {
            lib.con.query_row("SELECT bpm, ind_bpm FROM tracks WHERE track_id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let added = row(&lib);
        assert_eq!(added, (135.0, 130));

        // wreck the row, then re-store the same analysis: it must land on the same values
        lib.con.execute("UPDATE tracks SET bpm = 1, ind_bpm = 0", []).unwrap();
        let location: String = lib.con.query_row("SELECT location FROM tracks", [], |r| r.get(0)).unwrap();
        lib.store_analysis(id, &location, &analysis).unwrap();
        assert_eq!(row(&lib), added, "add_track and store_analysis agree");
        assert_eq!(ind_bpm_of(track_bpm(&analysis)), 130);

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src_dir).unwrap();
    }

    #[test]
    fn a_failed_row_update_puts_the_old_grid_file_back() {
        let vol = temp_dir("rollback-vol");
        let src_dir = temp_dir("rollback-src");
        let src = src_dir.join("Roll.mp3");
        std::fs::write(&src, vec![0u8; 2222]).unwrap();
        let old = variable_tempo();
        let mut lib = Library::create(&vol).unwrap();
        let id = lib.add_track(&src, &TrackMeta { title: "r".into(), format: "MP3".into(), ..Default::default() }, &old).unwrap();
        let location: String = lib.con.query_row("SELECT location FROM tracks", [], |r| r.get(0)).unwrap();
        let (_, sidecar) = lib.track_paths(&location).unwrap();
        let before = std::fs::read(&sidecar).unwrap();
        let row_before: (f64, i64) = lib.con.query_row("SELECT bpm, ind_bpm FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();

        // the row update fails (as a busy timeout or an unplug would make it)
        lib.con.execute_batch("CREATE TRIGGER refuse BEFORE UPDATE ON tracks BEGIN SELECT RAISE(ABORT, 'locked'); END;").unwrap();
        let mut new = variable_tempo();
        new.beats.iter_mut().for_each(|b| b.bpm = 100.0);
        assert!(lib.store_analysis(id, &location, &new).is_err());
        assert_eq!(std::fs::read(&sidecar).unwrap(), before, "the .str still holds the grid the row describes");
        let row_after: (f64, i64) = lib.con.query_row("SELECT bpm, ind_bpm FROM tracks", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(row_after, row_before);
        assert!(tmp_files(&vol).is_empty(), "{:?}", tmp_files(&vol));

        // a track with no .str yet: the failed update leaves none behind
        std::fs::remove_file(&sidecar).unwrap();
        assert!(lib.store_analysis(id, &location, &new).is_err());
        assert!(!sidecar.exists());

        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src_dir).unwrap();
    }

    #[test]
    fn a_temp_left_by_an_unplug_is_removed_by_the_next_write_of_that_file_only() {
        let dir = temp_dir("stale-tmp");
        let dest = dir.join("settings.dat");
        let stale = dir.join("settings.dat.7.tmp");
        let fresh = dir.join("settings.dat.8.tmp");
        let other = dir.join("other.dat.9.tmp");
        for p in [&stale, &fresh, &other] {
            std::fs::write(p, b"x").unwrap();
        }
        let long_ago = SystemTime::now() - STALE_TEMP_AGE - Duration::from_secs(60);
        File::options().write(true).open(&stale).unwrap().set_modified(long_ago).unwrap();
        File::options().write(true).open(&other).unwrap().set_modified(long_ago).unwrap();
        atomic_write(&dest, b"new").unwrap();
        assert!(!stale.exists(), "an old temp of this file is cleaned up");
        assert!(fresh.exists(), "a recent one may belong to a writer still running");
        assert!(other.exists(), "another file's temp is not touched");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn atomic_write_never_leaves_a_truncated_file_or_a_temp() {
        let dir = temp_dir("atomic");
        let dest = dir.join("track.str");
        atomic_write(&dest, b"old").unwrap();

        // the writer dies half way: the old content survives and no temp file is left
        let err = atomic_write_with(&dest, |f| {
            f.write_all(b"half of the new con")?;
            Err(std::io::Error::other("cable pulled"))
        });
        assert!(err.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");
        assert!(tmp_files(&dir).is_empty(), "{:?}", tmp_files(&dir));

        // a target that cannot be replaced (a directory is in the way): error, temp removed
        let blocked = dir.join("blocked.str");
        std::fs::create_dir(&blocked).unwrap();
        assert!(atomic_write(&blocked, b"x").is_err());
        assert!(tmp_files(&dir).is_empty(), "{:?}", tmp_files(&dir));

        // success replaces the content
        atomic_write(&dest, b"new").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        // a missing source never creates the target
        assert!(atomic_copy(&dir.join("nope.mp3"), &dir.join("nope-copy.mp3")).is_err());
        assert!(!dir.join("nope-copy.mp3").exists());
        assert!(tmp_files(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn add_track_failures_leave_no_files_behind() {
        let vol = temp_dir("addfail-vol");
        let src_dir = temp_dir("addfail-src");
        let src = src_dir.join("Blocked.mp3");
        std::fs::write(&src, vec![0u8; 3333]).unwrap();
        let analysis = variable_tempo();
        let meta = TrackMeta { title: "b".into(), format: "MP3".into(), ..Default::default() };
        let mut lib = Library::create(&vol).unwrap();
        let (rel, _) = naming::device_paths("Blocked.mp3", 3333);
        let audio = vol.join(naming::MUSIC_DIR).join(&rel);
        let sidecar = PathBuf::from(naming::str_path(&audio.to_string_lossy()));

        // 1. the .str cannot be written (a directory sits on its name): the audio copied before
        //    it is removed again, and no temp file stays
        std::fs::create_dir_all(&sidecar).unwrap();
        assert!(lib.add_track(&src, &meta, &analysis).is_err());
        assert!(!audio.exists(), "audio without a row or sidecar must not stay");
        assert!(tmp_files(&vol).is_empty(), "{:?}", tmp_files(&vol));
        std::fs::remove_dir(&sidecar).unwrap();

        // 2. the audio cannot be written (a directory on its name): error, nothing else created
        std::fs::create_dir_all(&audio).unwrap();
        assert!(lib.add_track(&src, &meta, &analysis).is_err());
        assert!(!sidecar.exists());
        assert!(tmp_files(&vol).is_empty(), "{:?}", tmp_files(&vol));
        std::fs::remove_dir(&audio).unwrap();

        // 3. with the way clear it works, and still leaves no temp file
        let id = lib.add_track(&src, &meta, &analysis).unwrap();
        assert!(id > 0 && audio.is_file() && sidecar.is_file());
        assert!(tmp_files(&vol).is_empty());
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
        std::fs::remove_dir_all(&src_dir).unwrap();
    }

    /// A library in a temp dir holding one synthetic track per entry of `secs`, returning the
    /// library, the temp dirs to clean up and the track ids.
    fn library_with_tracks(tag: &str, secs: &[i64]) -> (Library, Vec<PathBuf>, Vec<i64>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src_dir = temp_dir(&format!("{tag}-src"));
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, s) in secs.iter().enumerate() {
            let src = src_dir.join(format!("synthetic {i}.mp3"));
            std::fs::write(&src, vec![0u8; 1000 + i]).unwrap();
            let meta = TrackMeta { title: format!("t{i}"), format: "MP3".into(), play_time_secs: *s, ..Default::default() };
            ids.push(lib.add_track(&src, &meta, &analysis).unwrap());
        }
        (lib, vec![vol, src_dir], ids)
    }

    #[test]
    fn year_of_ms_matches_known_dates() {
        assert_eq!(year_of_ms(0), 1970);
        assert_eq!(year_of_ms(1_706_247_828_709), 2024); // late Jan 2024
        assert_eq!(year_of_ms(1_735_689_599_999), 2024); // 2024-12-31T23:59:59.999Z
        assert_eq!(year_of_ms(1_735_689_600_000), 2025);
        assert_eq!(year_of_ms(-1), 1969);
    }

    #[test]
    fn rating_round_trip_and_validation() {
        let (mut lib, dirs, ids) = library_with_tracks("rate", &[100, 200]);
        let before = lib.tracks().unwrap();
        assert!(before.iter().all(|t| t.rating == 0 && t.last_played == -1 && t.times_played == 0));

        assert!(lib.set_track_rating(ids[0], 4).unwrap());
        assert!(!lib.set_track_rating(ids[0], 4).unwrap(), "same value is a no-op");
        let t = lib.tracks().unwrap();
        assert_eq!((t[0].rating, t[1].rating), (4, 0));
        let (modified, by): (i64, String) = lib
            .con
            .query_row("SELECT date_modified, modified_by_ed FROM tracks WHERE track_id = ?1", [ids[0]], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert!(modified > 1_600_000_000_000, "date_modified is in milliseconds");
        assert_eq!(by, EDITOR_ID);
        let untouched: String = lib.con.query_row("SELECT modified_by_ed FROM tracks WHERE track_id = ?1", [ids[1]], |r| r.get(0)).unwrap();
        assert_eq!(untouched, EDITOR_ID, "add_track wrote it; set_track_rating did not touch the other row");

        assert!(lib.set_track_rating(ids[0], 0).unwrap());
        assert_eq!(lib.tracks().unwrap()[0].rating, 0);
        assert!(matches!(lib.set_track_rating(ids[0], 6), Err(Error::Invalid(_))));
        assert!(matches!(lib.set_track_rating(999_999, 1), Err(Error::Invalid(_))));
        assert!(matches!(lib.set_track_rating(-ids[0], 1), Err(Error::Invalid(_))));
        assert!(lib.compatibility_problems().unwrap().is_empty());

        drop(lib);
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn case_lifecycle_matches_editor_rows() {
        let (mut lib, dirs, ids) = library_with_tracks("case", &[100, 250, 40]);

        let a = lib.create_case("Synthetic A").unwrap();
        let b = lib.create_case("Synthetic B").unwrap();
        assert!(b > a);
        use rusqlite::types::Value::{Integer as I, Null, Text as T};
        let col = |c: &str| -> rusqlite::types::Value {
            lib.con.query_row(&format!("SELECT {c} FROM cases WHERE case_id = {a}"), [], |r| r.get(0)).unwrap()
        };
        assert_eq!(col("name"), T("Synthetic A".into()));
        let (I(created), I(modified), I(year)) = (col("date_created"), col("date_modified"), col("year")) else { panic!("integer columns") };
        assert!(created > 1_600_000_000_000 && created == modified, "created == modified, in ms");
        assert_eq!(year, year_of_ms(created));
        for (c, want) in [("times_played", I(0)), ("structured_ct", Null), ("image_id", I(0)), ("genre", T(String::new())), ("rating", I(0)),
                          ("play_time_secs", I(0)), ("last_played", I(0)), ("comments", T(String::new()))] {
            assert_eq!(col(c), want, "{c}");
        }
        assert_eq!(col("creator_id"), T(CASE_CREATOR_ID.into()));
        assert_eq!(CASE_CREATOR_ID.split(';').count(), 4);

        // membership keeps the given order and drops repeats; play time is the member sum
        lib.set_case_tracks(a, &[ids[2], ids[0], ids[2], ids[1]]).unwrap();
        assert_eq!(lib.case_track_ids(a).unwrap(), vec![ids[2], ids[0], ids[1]]);
        let listed = lib.cases().unwrap();
        let ca = listed.iter().find(|c| c.id == a).unwrap();
        assert_eq!((ca.track_count, ca.play_time_secs), (3, 390));

        // replacing shrinks it and refreshes the sum; the other case is untouched
        lib.set_case_tracks(b, &[ids[1]]).unwrap();
        lib.set_case_tracks(a, &[ids[1], ids[0]]).unwrap();
        assert_eq!(lib.case_track_ids(a).unwrap(), vec![ids[1], ids[0]]);
        assert_eq!(lib.case_track_ids(b).unwrap(), vec![ids[1]]);
        let listed = lib.cases().unwrap();
        assert_eq!(listed.iter().find(|c| c.id == a).unwrap().play_time_secs, 350);
        assert_eq!(listed.iter().find(|c| c.id == b).unwrap().play_time_secs, 250);

        // emptying a case gives 0, not NULL
        lib.set_case_tracks(a, &[]).unwrap();
        let (n, secs): (i64, i64) = lib
            .con
            .query_row("SELECT (SELECT COUNT(*) FROM casetracks WHERE case_id = ?1), play_time_secs FROM cases WHERE case_id = ?1", [a], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((n, secs), (0, 0));

        // validation: bad ids change nothing
        lib.set_case_tracks(a, &[ids[0]]).unwrap();
        assert!(matches!(lib.set_case_tracks(a, &[ids[1], 999_999]), Err(Error::Invalid(_))));
        assert!(matches!(lib.set_case_tracks(999_999, &[ids[0]]), Err(Error::Invalid(_))));
        assert_eq!(lib.case_track_ids(a).unwrap(), vec![ids[0]], "failed replace rolled back");
        assert!(matches!(lib.create_case("   "), Err(Error::Invalid(_))));
        assert!(matches!(lib.create_case(&"x".repeat(MAX_CASE_NAME_CHARS + 1)), Err(Error::Invalid(_))));
        lib.create_case(&"x".repeat(MAX_CASE_NAME_CHARS)).unwrap();
        lib.create_case("Synthetic A").unwrap(); // duplicate names are allowed, as on the device

        // delete removes the membership rows too, leaves the other case, and ids are not reused
        assert!(lib.delete_case(a).unwrap());
        assert!(!lib.delete_case(a).unwrap());
        assert!(lib.case_track_ids(a).unwrap().is_empty());
        assert_eq!(lib.case_track_ids(b).unwrap(), vec![ids[1]]);
        let orphans: i64 = lib.con.query_row("SELECT COUNT(*) FROM casetracks WHERE case_id = ?1", [a], |r| r.get(0)).unwrap();
        assert_eq!(orphans, 0);
        let c = lib.create_case("Synthetic C").unwrap();
        assert!(c > b, "AUTOINCREMENT: the next id is above every id ever used");
        assert!(lib.compatibility_problems().unwrap().is_empty());

        drop(lib);
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    fn synthetic_mix(name: &str, ids: &[i64]) -> crate::mix::Mix {
        use crate::mix::*;
        let mut m = Mix::new(name);
        let mut start = 0.0;
        for (i, id) in ids.iter().enumerate() {
            let pos = Position {
                start,
                length: 4_410_000.0,
                limit_size: true,
                session_event_id: format!("{:032x}", i + 1),
                ..Position::default()
            };
            let mut t = Track::new(*id, &format!("synthetic{i}.mp3"), 128.0, 1.0, pos);
            t.channel = (i % 2) as u32;
            t.processes.push(Process::Tool(Tool::flat(Effect::reverb(), start + 1000.0, 88_200.0, 0.5, format!("{:032x}", 100 + i))));
            m.tracks.push(t);
            start += 3_000_000.0;
        }
        for i in 1..ids.len() {
            let overlap = Position { start: 3_000_000.0 * i as f64, length: 1_410_000.0, ..Position::default() };
            m.transitions.push(Transition::new(overlap));
        }
        m
    }

    #[test]
    fn mix_lifecycle_matches_editor_rows() {
        let (mut lib, dirs, ids) = library_with_tracks("mix", &[100, 250, 40]);

        let a = lib.create_mix("Synthetic Mix A").unwrap();
        let info = lib.mix_info(a).unwrap().unwrap();
        assert_eq!(info.name, "Synthetic Mix A");
        assert!(info.date_created > 1_600_000_000_000 && info.date_modified >= info.date_created);
        assert_eq!((info.times_played, info.rating, info.last_played, info.play_time_secs, info.track_count), (0, 0, 0, 0, 0));
        assert_eq!(info.year, year_of_ms(info.date_created));
        assert_eq!(info.preview_markers, "<tracks/>");
        assert_eq!(info.creator_id, MIX_CREATOR_ID);
        assert_eq!(lib.read_mix(a).unwrap(), crate::mix::Mix::new("Synthetic Mix A"));
        assert!(lib.create_mix("Synthetic Mix A").is_err(), "duplicate names are refused");
        assert!(lib.create_mix("  ").is_err());

        // add three tracks and two transitions
        let m = synthetic_mix("Synthetic Mix A", &[ids[0], ids[2], ids[1]]);
        lib.save_mix(a, &m).unwrap();
        assert_eq!(lib.read_mix(a).unwrap(), m);
        let info = lib.mix_info(a).unwrap().unwrap();
        assert_eq!(info.track_count, 3);
        assert_eq!(info.play_time_secs, i64::from(m.play_time_secs()));
        assert_eq!(info.preview_markers, "<tracks><previewmarkers position=\"-1\"/><previewmarkers position=\"-1\"/></tracks>");
        let rows = lib.mix_track_rows(a).unwrap();
        assert_eq!(rows.iter().map(|r| r.track_id).collect::<Vec<_>>(), vec![ids[0], ids[2], ids[1]]);
        assert_eq!(rows.iter().map(|r| r.track_index).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert!(rows.iter().all(|r| r.dup_title.is_none() && r.dup_play_time_secs.is_none()));

        // edit: drop a track, rename
        let mut m2 = lib.read_mix(a).unwrap();
        m2.tracks.remove(1);
        m2.transitions.pop();
        m2.name = "Synthetic Mix B".into();
        lib.save_mix(a, &m2).unwrap();
        assert_eq!(lib.mix_info(a).unwrap().unwrap().name, "Synthetic Mix B");
        assert_eq!(lib.mix_track_rows(a).unwrap().len(), 2);
        assert_eq!(lib.mix_infos().unwrap().len(), 1);

        // unknown library id and clashing name are refused, and nothing changes
        let mut bad = m2.clone();
        bad.tracks[0].library_id = 9999;
        assert!(lib.save_mix(a, &bad).is_err());
        let other = lib.create_mix("Other").unwrap();
        let mut clash = m2.clone();
        clash.name = "Other".into();
        assert!(lib.save_mix(a, &clash).is_err());
        assert_eq!(lib.read_mix(a).unwrap(), m2);

        // removed-from-library placeholder keeps its dup_* fields across a save
        lib.con
            .execute(
                "UPDATE mixtracks SET track_id = -track_id, dup_title = 'gone', dup_artist = 'x', dup_album = 'y',
                        dup_play_time_secs = 7 WHERE mix_id = ?1 AND track_index = 1",
                [a],
            )
            .unwrap();
        let mut m3 = lib.read_mix(a).unwrap();
        m3.tracks[1].library_id = -m3.tracks[1].library_id;
        lib.save_mix(a, &m3).unwrap();
        let rows = lib.mix_track_rows(a).unwrap();
        assert!(rows[1].track_id < 0);
        assert_eq!((rows[1].dup_title.as_deref(), rows[1].dup_play_time_secs), (Some("gone"), Some(7)));

        assert!(lib.delete_mix(a).unwrap());
        assert!(!lib.delete_mix(a).unwrap());
        assert!(lib.read_mix(a).is_err());
        assert!(lib.mix_track_rows(a).unwrap().is_empty());
        assert!(lib.mix_info(other).unwrap().is_some());
        drop(lib); // Windows won't delete a database file that is still open
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn mixes_using_a_track_are_listed_by_name() {
        let (mut lib, dirs, ids) = library_with_tracks("mixuse", &[100, 250, 300]);
        let b = lib.create_mix("Bravo").unwrap();
        let a = lib.create_mix("alpha").unwrap();
        let c = lib.create_mix("Charlie").unwrap();
        lib.save_mix(b, &synthetic_mix("Bravo", &[ids[0], ids[1]])).unwrap();
        lib.save_mix(a, &synthetic_mix("alpha", &[ids[0]])).unwrap();
        lib.save_mix(c, &synthetic_mix("Charlie", &[ids[1]])).unwrap();
        let names = |t: i64| lib.mixes_using_track(t).unwrap().into_iter().map(|(_, n)| n).collect::<Vec<_>>();
        assert_eq!(names(ids[0]), ["alpha", "Bravo"], "case-insensitive name order");
        assert_eq!(names(ids[1]), ["Bravo", "Charlie"]);
        assert!(names(ids[2]).is_empty());
        drop(lib);
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_quick_write_waits_for_a_long_jobs_transaction_instead_of_failing() {
        let (job, dirs, ids) = library_with_tracks("busy", &[100]);
        let vol = job.root().to_path_buf();
        job.con.execute_batch("BEGIN IMMEDIATE").unwrap(); // the long job is mid-transaction
        let id = ids[0];
        let quick = std::thread::spawn(move || {
            let mut lib = Library::open(&vol).unwrap();
            lib.set_track_rating(id, 4)
        });
        std::thread::sleep(Duration::from_millis(400));
        job.con.execute_batch("COMMIT").unwrap();
        assert!(quick.join().unwrap().unwrap(), "the rating was written once the job's transaction ended");
        drop(job);
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn mix_meta_rename_and_sources() {
        let (mut lib, dirs, ids) = library_with_tracks("mixmeta", &[100, 250]);
        let a = lib.create_mix("Meta Mix").unwrap();
        let other = lib.create_mix("Other Mix").unwrap();
        let m = synthetic_mix("Meta Mix", &[ids[0], ids[1]]);
        lib.save_mix(a, &m).unwrap();

        let meta = MixMetaUpdate {
            genre: "House".into(),
            year: 2024,
            rating: 4,
            producer: "P".into(),
            remixer: "R".into(),
            comments: "c".into(),
        };
        lib.set_mix_meta(a, &meta).unwrap();
        let info = lib.mix_info(a).unwrap().unwrap();
        assert_eq!((info.genre.as_str(), info.year, info.rating, info.producer.as_str()), ("House", 2024, 4, "P"));
        assert_eq!((info.remixer.as_str(), info.comments.as_str()), ("R", "c"));
        assert_eq!(lib.read_mix(a).unwrap(), m, "metadata leaves the document alone");
        assert_eq!(lib.mix_track_rows(a).unwrap().len(), 2);
        assert!(lib.set_mix_meta(a, &MixMetaUpdate { rating: 6, ..meta.clone() }).is_err());
        assert!(lib.set_mix_meta(a, &MixMetaUpdate { year: -1, ..meta.clone() }).is_err());
        assert!(lib.set_mix_meta(9999, &meta).is_err());

        lib.rename_mix(a, "Renamed Mix").unwrap();
        assert_eq!(lib.mix_info(a).unwrap().unwrap().name, "Renamed Mix");
        assert_eq!(lib.read_mix(a).unwrap().name, "Renamed Mix");
        assert!(lib.rename_mix(a, "Other Mix").is_err());
        assert!(lib.rename_mix(a, "  ").is_err());
        assert!(lib.rename_mix(9999, "Nope").is_err());
        let _ = other;

        let src = lib.mix_track_sources(&[ids[1], -5, ids[0], 424242]).unwrap();
        assert_eq!(src.iter().map(|s| s.track_id).collect::<Vec<_>>(), vec![ids[1], ids[0]]);
        assert!(lib.compatibility_problems().unwrap().is_empty());
        drop(lib);
        for d in dirs {
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}
