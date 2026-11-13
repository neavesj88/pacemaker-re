//! Library operation for audio files whose own tags were rewritten (owner: metadata area).
//!
//! Writing tags into a file changes its size, and `tracks.file_size` records the size the file had
//! when it was added. The device follows `tracks.location` (the hashed name is only made once, at
//! import: notes/FINDINGS.md), so nothing else needs to move; the recorded size is kept true.
//! Whether the firmware ever looks at `file_size` is not established, which is why only that one
//! column is touched.

use rusqlite::OptionalExtension;

use crate::library::{Error, Library};

impl Library {
    /// Sets `tracks.file_size` for the given `(track_id, size in bytes)` pairs in one transaction.
    /// An unknown id or a negative size is an error and nothing is written. Returns how many
    /// rows actually changed (a size that is already right is not counted).
    pub fn set_file_sizes(&mut self, sizes: &[(i64, i64)]) -> Result<usize, Error> {
        let tx = self.con.transaction()?;
        let mut changed = 0;
        for (id, size) in sizes {
            if *size < 0 {
                return Err(Error::Invalid(format!("file size {size} of track {id} is negative")));
            }
            let cur: Option<i64> = tx
                .query_row("SELECT IFNULL(file_size,0) FROM tracks WHERE track_id = ?1 AND track_id >= 0", [id], |r| r.get(0))
                .optional()?;
            let Some(cur) = cur else {
                return Err(Error::Invalid(format!("no track {id}")));
            };
            if cur != *size {
                tx.execute("UPDATE tracks SET file_size = ?1 WHERE track_id = ?2", rusqlite::params![size, id])?;
                changed += 1;
            }
        }
        tx.commit()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use crate::library::{Library, TrackMeta};
    use crate::strfile;

    #[test]
    fn file_sizes_update_only_what_differs_and_roll_back_on_error() {
        let base = std::env::temp_dir().join(format!("pm-tagfile-{}", std::process::id()));
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
        for i in 0..2 {
            let src = src_dir.join(format!("synthetic {i}.mp3"));
            std::fs::write(&src, vec![0u8; 1000 + i]).unwrap();
            let meta = TrackMeta { title: format!("T{i}"), format: "MP3".into(), play_time_secs: 10, ..Default::default() };
            ids.push(lib.add_track(&src, &meta, &analysis).unwrap());
        }
        let size = |lib: &Library, id: i64| lib.tracks().unwrap().iter().find(|t| t.track_id == id).unwrap().file_size;
        assert_eq!(size(&lib, ids[0]), 1000);
        assert_eq!(lib.set_file_sizes(&[(ids[0], 1234), (ids[1], 99)]).unwrap(), 2);
        assert_eq!(lib.set_file_sizes(&[(ids[0], 1234)]).unwrap(), 0, "same size: nothing written");
        assert!(lib.set_file_sizes(&[(ids[0], 5), (424_242, 7)]).is_err());
        assert_eq!(size(&lib, ids[0]), 1234, "rolled back");
        assert!(lib.set_file_sizes(&[(ids[0], -1)]).is_err());
        assert_eq!(size(&lib, ids[1]), 99);
        let _ = std::fs::remove_dir_all(&base);
    }
}
