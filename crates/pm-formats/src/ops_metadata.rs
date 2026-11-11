//! Library operations for track metadata editing (owner: metadata area).
//!
//! The Editor's metadata dialog (`EditTrackMetaDataComponent`) ends in `tl::Repo::modifyTrack`
//! (notes/FINDINGS.md section 4d): only the fields that changed are written, `date_modified`
//! is set to now (Unix ms) and `modified_by_ed` to the editor id. On top of that the lowercased
//! search columns `ind_title`, `ind_artist`, `ind_album` and `ind_genre` follow their source
//! column (section 3). `ind_bpm` is a bucket of the analysed BPM; BPM and key are read-only
//! here (the grid editor owns them), so it never changes in this module. The "The " rule
//! (`PreCollateTitle`) is a query-time sort function on the device and is not stored, so it does
//! not touch `ind_title`. `ind_*` are not cut to their declared 32 characters, consistent with
//! `add_track` (a cut is unconfirmed, FINDINGS section 3). `mixtracks.dup_*` are filled only for
//! tracks removed from the library (negative ids), never for a live track, so a metadata edit
//! leaves `mixtracks` alone.

use rusqlite::{params_from_iter, types::Value, OptionalExtension};

use crate::library::{Error, Library, EDITOR_ID, MAX_RATING};

/// Declared size of every free-text track column (VARCHAR(255)); SQLite does not enforce it, so
/// we do.
pub const MAX_TEXT_CHARS: usize = 255;
/// Largest value of the NUMBER(4) columns (year, track and disc numbers and their totals).
pub const MAX_SMALL_NUMBER: i64 = 9999;

/// The editable part of a `tracks` row plus the read-only BPM and key, as the dialog shows it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TrackDetail {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub composer: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub producer: String,
    pub comments: String,
    pub year: i64,
    pub track_number: i64,
    pub number_of_tracks: i64,
    pub disc_number: i64,
    pub number_of_discs: i64,
    pub rating: i64,
    /// Read-only: set by analysis.
    pub bpm: f64,
    /// Read-only: set by key detection.
    pub key: String,
}

/// Field changes for one track; `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct TrackEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub composer: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub remixer: Option<String>,
    pub producer: Option<String>,
    pub comments: Option<String>,
    pub year: Option<i64>,
    pub track_number: Option<i64>,
    pub number_of_tracks: Option<i64>,
    pub disc_number: Option<i64>,
    pub number_of_discs: Option<i64>,
    pub rating: Option<u8>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const SELECT_DETAIL: &str = "SELECT track_id, IFNULL(title,''), IFNULL(artist,''), IFNULL(album_artist,''),
        IFNULL(album,''), IFNULL(composer,''), IFNULL(genre,''), IFNULL(label,''), IFNULL(remixer,''),
        IFNULL(producer,''), IFNULL(comments,''), IFNULL(year,0), IFNULL(track_number,0),
        IFNULL(number_of_tracks,0), IFNULL(disc_number,0), IFNULL(number_of_discs,0),
        IFNULL(rating,0), IFNULL(bpm,0), IFNULL(key,'')
    FROM tracks WHERE track_id = ?1 AND track_id >= 0";

fn detail_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackDetail> {
    Ok(TrackDetail {
        track_id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album_artist: r.get(3)?,
        album: r.get(4)?,
        composer: r.get(5)?,
        genre: r.get(6)?,
        label: r.get(7)?,
        remixer: r.get(8)?,
        producer: r.get(9)?,
        comments: r.get(10)?,
        year: r.get(11)?,
        track_number: r.get(12)?,
        number_of_tracks: r.get(13)?,
        disc_number: r.get(14)?,
        number_of_discs: r.get(15)?,
        rating: r.get(16)?,
        bpm: r.get(17)?,
        key: r.get(18)?,
    })
}

fn check_text(field: &str, v: &str) -> Result<(), Error> {
    if v.chars().count() > MAX_TEXT_CHARS {
        return Err(Error::Invalid(format!("{field} is longer than {MAX_TEXT_CHARS} characters")));
    }
    if v.contains('\0') {
        return Err(Error::Invalid(format!("{field} contains a NUL character")));
    }
    Ok(())
}

fn check_number(field: &str, v: i64) -> Result<(), Error> {
    if !(0..=MAX_SMALL_NUMBER).contains(&v) {
        return Err(Error::Invalid(format!("{field} must be between 0 and {MAX_SMALL_NUMBER}")));
    }
    Ok(())
}

impl TrackEdit {
    /// Validates every present value against the schema.
    pub fn validate(&self) -> Result<(), Error> {
        let texts = [
            ("title", &self.title),
            ("artist", &self.artist),
            ("album artist", &self.album_artist),
            ("album", &self.album),
            ("composer", &self.composer),
            ("genre", &self.genre),
            ("label", &self.label),
            ("remixer", &self.remixer),
            ("producer", &self.producer),
            ("comments", &self.comments),
        ];
        for (name, v) in texts {
            if let Some(v) = v {
                check_text(name, v)?;
            }
        }
        let numbers = [
            ("year", self.year),
            ("track number", self.track_number),
            ("number of tracks", self.number_of_tracks),
            ("disc number", self.disc_number),
            ("number of discs", self.number_of_discs),
        ];
        for (name, v) in numbers {
            if let Some(v) = v {
                check_number(name, v)?;
            }
        }
        if let Some(r) = self.rating {
            if r > MAX_RATING {
                return Err(Error::Invalid(format!("rating {r} is above {MAX_RATING}")));
            }
        }
        Ok(())
    }

    /// `(column, new value)` for every field that differs from `cur`, `ind_*` columns included.
    fn changes(&self, cur: &TrackDetail) -> Vec<(&'static str, Value)> {
        let mut out: Vec<(&'static str, Value)> = Vec::new();
        let mut text = |col: &'static str, new: &Option<String>, old: &str, ind: Option<&'static str>| {
            if let Some(n) = new.as_deref().filter(|n| *n != old) {
                out.push((col, Value::Text(n.to_string())));
                if let Some(ind) = ind {
                    out.push((ind, Value::Text(n.to_lowercase())));
                }
            }
        };
        text("title", &self.title, &cur.title, Some("ind_title"));
        text("artist", &self.artist, &cur.artist, Some("ind_artist"));
        text("album_artist", &self.album_artist, &cur.album_artist, None);
        text("album", &self.album, &cur.album, Some("ind_album"));
        text("composer", &self.composer, &cur.composer, None);
        text("genre", &self.genre, &cur.genre, Some("ind_genre"));
        text("label", &self.label, &cur.label, None);
        text("remixer", &self.remixer, &cur.remixer, None);
        text("producer", &self.producer, &cur.producer, None);
        text("comments", &self.comments, &cur.comments, None);
        let mut num = |col: &'static str, new: Option<i64>, old: i64| {
            if let Some(n) = new.filter(|n| *n != old) {
                out.push((col, Value::Integer(n)));
            }
        };
        num("year", self.year, cur.year);
        num("track_number", self.track_number, cur.track_number);
        num("number_of_tracks", self.number_of_tracks, cur.number_of_tracks);
        num("disc_number", self.disc_number, cur.disc_number);
        num("number_of_discs", self.number_of_discs, cur.number_of_discs);
        num("rating", self.rating.map(i64::from), cur.rating);
        out
    }
}

impl Library {
    /// The editable fields of the given tracks, in the order asked. Errors on an unknown id.
    pub fn track_details(&self, ids: &[i64]) -> Result<Vec<TrackDetail>, Error> {
        let mut st = self.con.prepare(SELECT_DETAIL)?;
        ids.iter()
            .map(|id| st.query_row([id], detail_row).optional()?.ok_or_else(|| Error::Invalid(format!("no track {id}"))))
            .collect()
    }

    /// Applies field edits to tracks in one transaction: any error (unknown id, over-long
    /// value, rating above 5) rolls every track back. Per track only the fields whose value
    /// actually changes are written, together with the matching `ind_*` column, `date_modified`
    /// and `modified_by_ed`; a track with nothing to change is not touched at all. Returns how
    /// many tracks changed.
    pub fn edit_tracks(&mut self, edits: &[(i64, TrackEdit)]) -> Result<usize, Error> {
        let now = now_ms();
        let tx = self.con.transaction()?;
        let mut changed = 0;
        for (id, edit) in edits {
            edit.validate()?;
            let cur = tx
                .query_row(SELECT_DETAIL, [id], detail_row)
                .optional()?
                .ok_or_else(|| Error::Invalid(format!("no track {id}")))?;
            let mut set = edit.changes(&cur);
            if set.is_empty() {
                continue;
            }
            set.push(("date_modified", Value::Integer(now)));
            set.push(("modified_by_ed", Value::Text(EDITOR_ID.to_string())));
            let cols: Vec<String> = set.iter().enumerate().map(|(i, (c, _))| format!("{c} = ?{}", i + 1)).collect();
            let sql = format!("UPDATE tracks SET {} WHERE track_id = ?{}", cols.join(", "), set.len() + 1);
            let mut values: Vec<Value> = set.into_iter().map(|(_, v)| v).collect();
            values.push(Value::Integer(*id));
            tx.execute(&sql, params_from_iter(values))?;
            changed += 1;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Applies one edit to several tracks ("Multiple Item Information"): the same changed
    /// fields go to every track, everything else on each track is kept.
    pub fn edit_tracks_same(&mut self, ids: &[i64], edit: &TrackEdit) -> Result<usize, Error> {
        let edits: Vec<(i64, TrackEdit)> = ids.iter().map(|id| (*id, edit.clone())).collect();
        self.edit_tracks(&edits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;
    use crate::strfile;
    use std::path::PathBuf;

    struct Fixture {
        lib: Library,
        dirs: Vec<PathBuf>,
        ids: Vec<i64>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for d in &self.dirs {
                let _ = std::fs::remove_dir_all(d);
            }
        }
    }

    fn fixture(tag: &str, n: usize) -> Fixture {
        let base = std::env::temp_dir().join(format!("pm-meta-{tag}-{}", std::process::id()));
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
            let meta = TrackMeta {
                title: format!("Title {i}"),
                artist: format!("Artist {i}"),
                album: format!("Album {i}"),
                genre: "House".into(),
                year: 2001,
                key: "Am".into(),
                format: "MP3".into(),
                play_time_secs: 100,
                ..Default::default()
            };
            ids.push(lib.add_track(&src, &meta, &analysis).unwrap());
        }
        Fixture { lib, dirs: vec![base], ids }
    }

    fn row(lib: &Library, id: i64, cols: &str) -> Vec<Value> {
        let n = cols.split(',').count();
        lib.con
            .query_row(&format!("SELECT {cols} FROM tracks WHERE track_id = ?1"), [id], |r| {
                (0..n).map(|i| r.get::<_, Value>(i)).collect()
            })
            .unwrap()
    }

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn every_field_is_written() {
        let mut f = fixture("fields", 1);
        let id = f.ids[0];
        let edit = TrackEdit {
            title: Some("New Title".into()),
            artist: Some("New Artist".into()),
            album_artist: Some("Various".into()),
            album: Some("New Album".into()),
            composer: Some("Comp".into()),
            genre: Some("Techno".into()),
            label: Some("Lbl".into()),
            remixer: Some("Rmx".into()),
            producer: Some("Prod".into()),
            comments: Some("a comment".into()),
            year: Some(1999),
            track_number: Some(3),
            number_of_tracks: Some(12),
            disc_number: Some(1),
            number_of_discs: Some(2),
            rating: Some(4),
        };
        assert_eq!(f.lib.edit_tracks(&[(id, edit)]).unwrap(), 1);
        let d = f.lib.track_details(&[id]).unwrap().remove(0);
        assert_eq!(
            (d.title.as_str(), d.artist.as_str(), d.album_artist.as_str(), d.album.as_str(), d.composer.as_str()),
            ("New Title", "New Artist", "Various", "New Album", "Comp")
        );
        assert_eq!((d.genre.as_str(), d.label.as_str(), d.remixer.as_str(), d.producer.as_str()), ("Techno", "Lbl", "Rmx", "Prod"));
        assert_eq!(d.comments, "a comment");
        assert_eq!((d.year, d.track_number, d.number_of_tracks, d.disc_number, d.number_of_discs, d.rating), (1999, 3, 12, 1, 2, 4));
        assert_eq!((d.bpm, d.key.as_str()), (128.0, "Am"), "bpm and key are read-only here");
        assert!(f.lib.compatibility_problems().unwrap().is_empty());
    }

    #[test]
    fn ind_columns_follow_their_source_and_stamp_is_set() {
        let mut f = fixture("ind", 1);
        let id = f.ids[0];
        let before = row(&f.lib, id, "ind_bpm");
        let edit = TrackEdit {
            title: Some("The Big ÉCHO".into()),
            artist: Some("DJ Example".into()),
            album: Some("LP One".into()),
            genre: Some("Drum & Bass".into()),
            ..Default::default()
        };
        f.lib.edit_tracks(&[(id, edit)]).unwrap();
        let r = row(&f.lib, id, "ind_title, ind_artist, ind_album, ind_genre, ind_bpm, modified_by_ed");
        // lowercased, and "The " is not stripped: PreCollateTitle is a query-time function
        assert_eq!(r[0], text("the big écho"));
        assert_eq!(r[1], text("dj example"));
        assert_eq!(r[2], text("lp one"));
        assert_eq!(r[3], text("drum & bass"));
        assert_eq!(r[4], before[0], "ind_bpm is untouched by a tag edit");
        assert_eq!(r[5], text(EDITOR_ID));
        let Value::Integer(m) = row(&f.lib, id, "date_modified")[0] else { panic!("integer") };
        assert!(m > 1_600_000_000_000, "date_modified is Unix milliseconds");
    }

    #[test]
    fn non_indexed_fields_leave_ind_columns_alone() {
        let mut f = fixture("noind", 1);
        let id = f.ids[0];
        let before = row(&f.lib, id, "ind_title, ind_artist, ind_album, ind_genre");
        f.lib.edit_tracks(&[(id, TrackEdit { comments: Some("x".into()), year: Some(5), ..Default::default() })]).unwrap();
        assert_eq!(row(&f.lib, id, "ind_title, ind_artist, ind_album, ind_genre"), before);
    }

    #[test]
    fn multi_edit_only_touches_changed_fields() {
        let mut f = fixture("multi", 3);
        let ids = f.ids.clone();
        let before = f.lib.track_details(&ids).unwrap();
        let edit = TrackEdit { genre: Some("Trance".into()), rating: Some(3), ..Default::default() };
        assert_eq!(f.lib.edit_tracks_same(&ids, &edit).unwrap(), 3);
        for (b, a) in before.iter().zip(f.lib.track_details(&ids).unwrap()) {
            assert_eq!((a.genre.as_str(), a.rating), ("Trance", 3));
            let mut restored = a.clone();
            restored.genre = b.genre.clone();
            restored.rating = b.rating;
            assert_eq!(&restored, b, "every other field keeps its own value");
        }
        for id in &ids {
            assert_eq!(row(&f.lib, *id, "ind_genre")[0], text("trance"));
            assert_eq!(row(&f.lib, *id, "ind_title")[0], row(&f.lib, *id, "lower(title)")[0]);
        }
        // the same edit again changes nothing and stamps nothing
        let stamps: Vec<_> = ids.iter().map(|i| row(&f.lib, *i, "date_modified")).collect();
        assert_eq!(f.lib.edit_tracks_same(&ids, &edit).unwrap(), 0);
        let again: Vec<_> = ids.iter().map(|i| row(&f.lib, *i, "date_modified")).collect();
        assert_eq!(stamps, again);
    }

    #[test]
    fn unchanged_track_is_not_stamped() {
        let mut f = fixture("nochange", 2);
        let (a, b) = (f.ids[0], f.ids[1]);
        let stamp_a = row(&f.lib, a, "date_modified");
        let stamp_b = row(&f.lib, b, "date_modified");
        let same_title = f.lib.track_details(&[a]).unwrap()[0].title.clone();
        let n = f
            .lib
            .edit_tracks(&[(a, TrackEdit { title: Some(same_title), ..Default::default() }), (b, TrackEdit::default())])
            .unwrap();
        assert_eq!(n, 0);
        assert_eq!(row(&f.lib, a, "date_modified"), stamp_a);
        assert_eq!(row(&f.lib, b, "date_modified"), stamp_b);
    }

    #[test]
    fn per_track_edits_in_one_call() {
        let mut f = fixture("per", 2);
        let (a, b) = (f.ids[0], f.ids[1]);
        let n = f
            .lib
            .edit_tracks(&[
                (a, TrackEdit { title: Some("A2".into()), ..Default::default() }),
                (b, TrackEdit { title: Some("B2".into()), ..Default::default() }),
            ])
            .unwrap();
        assert_eq!(n, 2);
        let d = f.lib.track_details(&[b, a]).unwrap();
        assert_eq!((d[0].title.as_str(), d[1].title.as_str()), ("B2", "A2"));
    }

    #[test]
    fn rejects_bad_values() {
        let mut f = fixture("bad", 1);
        let id = f.ids[0];
        let long = "x".repeat(MAX_TEXT_CHARS + 1);
        let edge = "y".repeat(MAX_TEXT_CHARS);
        for bad in [
            TrackEdit { title: Some(long.clone()), ..Default::default() },
            TrackEdit { comments: Some(long), ..Default::default() },
            TrackEdit { artist: Some("a\0b".into()), ..Default::default() },
            TrackEdit { year: Some(10_000), ..Default::default() },
            TrackEdit { track_number: Some(-1), ..Default::default() },
            TrackEdit { rating: Some(6), ..Default::default() },
        ] {
            assert!(matches!(f.lib.edit_tracks(&[(id, bad)]), Err(Error::Invalid(_))));
        }
        assert!(matches!(f.lib.edit_tracks(&[(999_999, TrackEdit::default())]), Err(Error::Invalid(_))));
        assert!(matches!(f.lib.edit_tracks(&[(-id, TrackEdit::default())]), Err(Error::Invalid(_))));
        assert!(matches!(f.lib.track_details(&[999_999]), Err(Error::Invalid(_))));
        // the boundary values are accepted
        f.lib.edit_tracks(&[(id, TrackEdit { title: Some(edge.clone()), year: Some(9999), ..Default::default() })]).unwrap();
        assert_eq!(f.lib.track_details(&[id]).unwrap()[0].title, edge);
    }

    #[test]
    fn error_rolls_back_every_track() {
        let mut f = fixture("rollback", 2);
        let (a, b) = (f.ids[0], f.ids[1]);
        let snapshot = |lib: &Library| (lib.track_details(&[a, b]).unwrap(), row(lib, a, "ind_title, date_modified, modified_by_ed"));
        let before = snapshot(&f.lib);
        let res = f.lib.edit_tracks(&[
            (a, TrackEdit { title: Some("Changed".into()), ..Default::default() }),
            (b, TrackEdit { title: Some("z".repeat(300)), ..Default::default() }),
        ]);
        assert!(matches!(res, Err(Error::Invalid(_))));
        let res = f.lib.edit_tracks(&[
            (a, TrackEdit { title: Some("Changed".into()), ..Default::default() }),
            (424_242, TrackEdit { title: Some("Nope".into()), ..Default::default() }),
        ]);
        assert!(matches!(res, Err(Error::Invalid(_))));
        assert_eq!(before, snapshot(&f.lib));
    }

    #[test]
    fn database_failure_mid_transaction_rolls_back() {
        // a trigger that fails the second UPDATE proves the rollback covers real SQL errors too
        let mut f = fixture("trigger", 2);
        let (a, b) = (f.ids[0], f.ids[1]);
        f.lib
            .con
            .execute_batch(&format!(
                "CREATE TRIGGER boom BEFORE UPDATE ON tracks WHEN NEW.track_id = {b} BEGIN SELECT RAISE(ABORT, 'boom'); END;"
            ))
            .unwrap();
        let before = f.lib.track_details(&[a, b]).unwrap();
        let edits = [
            (a, TrackEdit { title: Some("First".into()), ..Default::default() }),
            (b, TrackEdit { title: Some("Second".into()), ..Default::default() }),
        ];
        assert!(matches!(f.lib.edit_tracks(&edits), Err(Error::Db(_))));
        assert_eq!(f.lib.track_details(&[a, b]).unwrap(), before);
    }

    #[test]
    fn mixtracks_are_not_touched() {
        let mut f = fixture("mix", 1);
        let id = f.ids[0];
        f.lib
            .con
            .execute("INSERT INTO mixtracks (mix_id, track_id, track_index, cue_point) VALUES (1, ?1, 0, 0)", [id])
            .unwrap();
        f.lib.edit_tracks(&[(id, TrackEdit { title: Some("Renamed".into()), ..Default::default() })]).unwrap();
        let (t, dt): (i64, Option<String>) = f
            .lib
            .con
            .query_row("SELECT track_id, dup_title FROM mixtracks WHERE mix_id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((t, dt), (id, None));
    }
}
