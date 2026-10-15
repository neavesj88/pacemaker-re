//! Byte-exact checks against files produced by Tonium's own software.
//!
//! Those files are not redistributable, so they are read from the gitignored work/ and vendor/
//! trees (populate them with scripts/fetch.sh and by running the original Editor once, see
//! notes/FINDINGS.md section 4b). Each test skips when its inputs are absent.

use std::path::{Path, PathBuf};

use pm_formats::{db, mixdoc, naming, pfw, strfile};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn editor_appdata() -> Option<PathBuf> {
    let p = std::env::var_os("PM_EDITOR_APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo().join("work/samples/editor-appdata"));
    p.join("music.db").exists().then_some(p)
}

fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn str_files_reencode_byte_identical() {
    let Some(dir) = editor_appdata() else { return eprintln!("skip: no Editor appdata") };
    let files = files_with_ext(&dir.join("str"), "str");
    assert!(!files.is_empty());
    for f in files {
        let orig = std::fs::read(&f).unwrap();
        let a = strfile::decode(std::str::from_utf8(&orig).unwrap()).unwrap();
        assert!(a.beats.len() > 100, "{f:?}");
        let ours = strfile::encode(&a, strfile::Newline::Crlf);
        assert!(ours.as_bytes() == orig, "{f:?} differs");
    }
}

#[test]
fn editor_db_matches_device_schema_and_mix_blob_round_trips() {
    let Some(dir) = editor_appdata() else { return eprintln!("skip: no Editor appdata") };
    let con = rusqlite::Connection::open_with_flags(
        dir.join("music.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(db::check(&con).unwrap(), Vec::<String>::new());
    let blob: Vec<u8> = con.query_row("SELECT structured_ct FROM mixes", [], |r| r.get(0)).unwrap();
    let xml = mixdoc::decode_blob(&blob).unwrap();
    assert!(xml.starts_with(b"<?xml"));
    assert_eq!(mixdoc::encode_blob(&xml), blob);
}

#[test]
fn editor_cache_names_match_naming_scheme() {
    let Some(dir) = editor_appdata() else { return eprintln!("skip: no Editor appdata") };
    let con = rusqlite::Connection::open_with_flags(
        dir.join("music.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut st = con.prepare("SELECT location, file_size FROM tracks").unwrap();
    let rows: Vec<(String, i64)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    for (loc, size) in rows {
        let name = loc.rsplit(['\\', '/']).next().unwrap();
        let stem = &name[..name.rfind('.').unwrap()];
        let enc = naming::encoded_name(stem, size as u64);
        let found = files_with_ext(&dir.join("str"), "str").into_iter().any(|p| {
            p.to_string_lossy().replace('\\', "/").contains(&format!("/{enc}."))
        });
        assert!(found, "no .str for {loc} at {enc}");
    }
}

#[test]
fn firmware_images_rebuild_byte_identical() {
    let dir = repo().join("vendor/firmware");
    let files = files_with_ext(&dir, "pfw");
    if files.is_empty() {
        return eprintln!("skip: run scripts/fetch.sh for vendor/firmware");
    }
    for f in files {
        let orig = std::fs::read(&f).unwrap();
        assert!(pfw::verify(&orig).unwrap(), "{f:?} md5");
        let t = pfw::parse(&orig).unwrap();
        let image = &orig[..orig.len() - pfw::TRAILER];
        let rebuilt = pfw::build(image, &t.sw_version, &t.fw_version, &t.bootloader_version, &t.uid);
        assert!(rebuilt == orig, "{f:?} rebuild differs");
    }
}
