//! Track columns that the `Track` list payload does not carry (it is frozen for the UI) but that
//! smart-case rules need: year, date added, comments, label and remixer. Read-only.

use std::collections::HashMap;

use super::{Error, Library};

/// The extra columns of one track. Unknown / unset values are 0 and the empty string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackExtra {
    pub year: i64,
    /// Unix milliseconds the track was added to the library (0 when the row has none).
    pub date_added: i64,
    pub comments: String,
    pub label: String,
    pub remixer: String,
}

impl Library {
    /// The extra columns of every track, by track id (same rows as [`Library::tracks`]).
    pub fn track_extras(&self) -> Result<HashMap<i64, TrackExtra>, Error> {
        let mut st = self.con.prepare(
            "SELECT track_id, IFNULL(year,0), IFNULL(date_added,0), IFNULL(comments,''), IFNULL(label,''), IFNULL(remixer,'')
             FROM tracks WHERE track_id >= 0",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                TrackExtra { year: r.get(1)?, date_added: r.get(2)?, comments: r.get(3)?, label: r.get(4)?, remixer: r.get(5)? },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::case_ops_tests::{cleanup, library_with_tracks};
    use super::*;

    #[test]
    fn extras_come_from_the_row_and_default_when_null() {
        let (lib, dirs, ids) = library_with_tracks("extras", &[100, 200]);
        lib.con
            .execute(
                "UPDATE tracks SET year = 1999, comments = 'synthetic note', label = 'Synthetic Label', remixer = 'Synthetic Remixer' WHERE track_id = ?1",
                [ids[0]],
            )
            .unwrap();
        lib.con.execute("UPDATE tracks SET year = NULL, comments = NULL, label = NULL, remixer = NULL, date_added = NULL WHERE track_id = ?1", [ids[1]]).unwrap();
        let x = lib.track_extras().unwrap();
        assert_eq!(x.len(), 2);
        let a = &x[&ids[0]];
        assert_eq!((a.year, a.comments.as_str(), a.label.as_str(), a.remixer.as_str()), (1999, "synthetic note", "Synthetic Label", "Synthetic Remixer"));
        assert!(a.date_added > 1_600_000_000_000, "add_track stamps the time in milliseconds");
        assert_eq!(x[&ids[1]], TrackExtra::default(), "NULL columns read as 0 / empty");
        cleanup(lib, dirs);
    }
}
