//! `.Pacemaker/music.db`: create a device-compatible library and check an existing one.
//!
//! Rules from pmplayer 14142's `tl::DirectAccessRepo` (notes/FINDINGS.md section 3): the schema
//! is the `tl::g_*Fields` arrays (schema.sql), `PRAGMA user_version` must be exactly 677, and
//! there are three indexes. The ICU collation is registered per connection at query time, so a
//! plain SQLite writer is enough.

use rusqlite::{Connection, OpenFlags};

pub const USER_VERSION: i32 = 677;
pub const SCHEMA: &str = include_str!("schema.sql");
pub const INDEXES: [&str; 3] = [
    "CREATE INDEX mixtracks_idx ON mixtracks (track_id)",
    "CREATE INDEX tracks_location_idx ON tracks (location)",
    "CREATE INDEX casetracks_idx ON casetracks (track_id)",
];

/// `[(table, [(column, declared type)])]` parsed from schema.sql.
pub fn expected_schema() -> Vec<(String, Vec<(String, String)>)> {
    let mut out = Vec::new();
    for part in SCHEMA.split("CREATE TABLE ").skip(1) {
        let (name, rest) = part.split_once(" (").expect("schema.sql table header");
        let body = &rest[..rest.find("\n);").expect("schema.sql table end")];
        let cols = body
            .lines()
            .map(|l| l.trim().trim_end_matches(','))
            .filter(|l| !l.is_empty())
            .map(|l| {
                let (c, t) = l.split_once(' ').unwrap_or((l, ""));
                (c.to_string(), t.to_string())
            })
            .collect();
        out.push((name.to_string(), cols));
    }
    out
}

/// Creates a new empty library, equivalent to the device's `formatEmptyDb`.
pub fn create(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let con = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    con.execute_batch(SCHEMA)?;
    for ix in INDEXES {
        con.execute(ix, [])?;
    }
    con.pragma_update(None, "user_version", USER_VERSION)?;
    Ok(con)
}

/// Every way `con` differs from what the device requires; empty means compatible.
pub fn check(con: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut problems = Vec::new();
    let uv: i32 = con.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if uv != USER_VERSION {
        problems.push(format!("user_version is {uv}, device requires {USER_VERSION}"));
    }
    for (table, cols) in expected_schema() {
        let mut st = con.prepare(&format!("PRAGMA table_info({table})"))?;
        let have: Vec<(String, String)> = st
            .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
            .collect::<Result<_, _>>()?;
        if have.is_empty() {
            problems.push(format!("missing table {table}"));
            continue;
        }
        let want: Vec<&str> = cols.iter().map(|(c, _)| c.as_str()).collect();
        let got: Vec<&str> = have.iter().map(|(c, _)| c.as_str()).collect();
        if want != got {
            problems.push(format!("{table}: columns {got:?}, device expects {want:?}"));
        }
    }
    for ix in INDEXES {
        let name = ix.split_whitespace().nth(2).unwrap();
        let n: i64 = con.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            [name],
            |r| r.get(0),
        )?;
        if n == 0 {
            problems.push(format!("missing index {name}"));
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_db_passes_check() {
        let dir = std::env::temp_dir().join(format!("pm-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("music.db");
        let _ = std::fs::remove_file(&path);
        let con = create(&path).unwrap();
        assert!(check(&con).unwrap().is_empty());
        con.pragma_update(None, "user_version", 12).unwrap();
        assert_eq!(check(&con).unwrap().len(), 1);
        drop(con);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn schema_has_seven_tables() {
        let s = expected_schema();
        assert_eq!(s.len(), 7);
        assert_eq!(s[0].0, "tracks");
        assert_eq!(s[0].1.len(), 46);
    }
}
