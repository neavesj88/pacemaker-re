//! Library operations for analysis results such as the detected key (owner: analysis area).

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};

use crate::library::{Error, Library, EDITOR_ID};

/// `tracks.key` is declared `VARCHAR(4)`; SQLite does not enforce it, so we do.
pub const MAX_KEY_CHARS: usize = 4;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Library {
    /// Sets a track's musical key (short notation such as "Am" or "F#m", at most
    /// [`MAX_KEY_CHARS`] characters) with the Editor's field conventions: `date_modified` (Unix
    /// ms) and `modified_by_ed` change only when the stored value changes. Returns whether the
    /// row changed. Errors on an empty or over-long key and on an unknown (or negative) track id.
    pub fn set_track_key(&mut self, track_id: i64, key: &str) -> Result<bool, Error> {
        let tx = self.con.transaction()?;
        let changed = write_key(&tx, track_id, key)?;
        tx.commit()?;
        Ok(changed)
    }

    /// [`Library::set_track_key`] for many tracks in one transaction (one disk sync instead of
    /// one per track, which matters on a USB drive). All or nothing: one invalid entry rolls the
    /// whole batch back. Returns how many rows changed.
    pub fn set_track_keys(&mut self, keys: &[(i64, String)]) -> Result<usize, Error> {
        let tx = self.con.transaction()?;
        let mut changed = 0;
        for (id, key) in keys {
            if write_key(&tx, *id, key)? {
                changed += 1;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Ids of tracks with no key (NULL, empty or blank), in id order.
    pub fn track_ids_without_key(&self) -> Result<Vec<i64>, Error> {
        let mut st = self
            .con
            .prepare("SELECT track_id FROM tracks WHERE track_id >= 0 AND TRIM(IFNULL(key,'')) = '' ORDER BY track_id")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

fn write_key(tx: &rusqlite::Transaction<'_>, track_id: i64, key: &str) -> Result<bool, Error> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::Invalid("key is empty".into()));
    }
    if key.chars().count() > MAX_KEY_CHARS {
        return Err(Error::Invalid(format!("key {key:?} is longer than {MAX_KEY_CHARS} characters")));
    }
    let current: Option<Option<String>> = tx
        .query_row("SELECT key FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |r| r.get(0))
        .optional()?;
    let Some(current) = current else {
        return Err(Error::Invalid(format!("no track {track_id}")));
    };
    if current.as_deref() == Some(key) {
        return Ok(false);
    }
    tx.execute(
        "UPDATE tracks SET key = ?1, date_modified = ?2, modified_by_ed = ?3 WHERE track_id = ?4",
        params![key, now_ms(), EDITOR_ID, track_id],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::library::TrackMeta;
    use crate::strfile;

    fn library_with_tracks(tag: &str, n: usize) -> (Library, PathBuf, Vec<i64>) {
        let base = std::env::temp_dir().join(format!("pm-keyops-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let src_dir = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src_dir).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let src = src_dir.join(format!("synthetic {i}.mp3"));
            std::fs::write(&src, vec![0u8; 1000 + i]).unwrap();
            let meta = TrackMeta { title: format!("Synthetic {i}"), format: "MP3".into(), ..Default::default() };
            ids.push(lib.add_track(&src, &meta, &analysis).unwrap());
        }
        (lib, base, ids)
    }

    fn row(lib: &Library, id: i64) -> (Option<String>, Option<i64>, Option<String>) {
        lib.con
            .query_row("SELECT key, date_modified, modified_by_ed FROM tracks WHERE track_id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
    }

    #[test]
    fn sets_key_with_editor_conventions_and_skips_unchanged() {
        let (mut lib, base, ids) = library_with_tracks("set", 2);
        assert_eq!(lib.track_ids_without_key().unwrap(), ids);
        assert!(lib.set_track_key(ids[0], "F#m").unwrap());
        let (key, modified, by) = row(&lib, ids[0]);
        assert_eq!(key.as_deref(), Some("F#m"));
        assert!(modified.unwrap() > 1_600_000_000_000);
        assert_eq!(by.as_deref(), Some(EDITOR_ID));
        assert_eq!(lib.track_ids_without_key().unwrap(), vec![ids[1]]);

        // unchanged value: no write, date_modified untouched
        let before = row(&lib, ids[0]);
        assert!(!lib.set_track_key(ids[0], "F#m").unwrap());
        assert_eq!(row(&lib, ids[0]), before);

        assert_eq!(lib.tracks().unwrap()[0].key, "F#m");
        assert!(lib.compatibility_problems().unwrap().is_empty());
        drop(lib); // Windows won't delete a database file that is still open
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rejects_bad_input_and_batch_is_atomic() {
        let (mut lib, base, ids) = library_with_tracks("bad", 2);
        assert!(lib.set_track_key(ids[0], "").is_err());
        assert!(lib.set_track_key(ids[0], "  ").is_err());
        assert!(lib.set_track_key(ids[0], "C#maj").is_err());
        assert!(lib.set_track_key(9999, "Am").is_err());
        assert!(lib.set_track_key(-1, "Am").is_err());
        // second entry invalid: the first must be rolled back
        assert!(lib.set_track_keys(&[(ids[0], "Am".into()), (9999, "C".into())]).is_err());
        assert!(row(&lib, ids[0]).0.unwrap_or_default().is_empty());
        assert_eq!(lib.set_track_keys(&[(ids[0], "Am".into()), (ids[1], "C".into())]).unwrap(), 2);
        assert_eq!(lib.set_track_keys(&[(ids[0], "Am".into())]).unwrap(), 0);
        assert!(lib.track_ids_without_key().unwrap().is_empty());
        drop(lib);
        std::fs::remove_dir_all(base).unwrap();
    }
}
