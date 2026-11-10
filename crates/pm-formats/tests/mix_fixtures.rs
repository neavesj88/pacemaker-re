//! Byte-exact round trips of real mix documents. The documents come from the gitignored work/
//! tree (the Editor's bundled demo mix and the copy its library stored); each test skips when
//! its inputs are absent. Nothing from them is copied into the repository.

use std::path::{Path, PathBuf};

use pm_formats::{mix::Mix, mixdoc};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every real mix document available locally as raw XML bytes, with a label.
fn real_documents() -> Vec<(String, Vec<u8>)> {
    let mut docs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add = |label: String, xml: Vec<u8>| {
        if seen.insert(xml.clone()) {
            docs.push((label, xml));
        }
    };
    // .pmk prefab files anywhere under the Mac Editor tree and the samples dir.
    for root in ["work/editor-mac", "work/editor-win-files", "work/samples"] {
        let mut stack = vec![repo().join(root)];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "pmk") {
                    if let Ok(xml) = mixdoc::decode_pmk(&std::fs::read(&p).unwrap()) {
                        add(format!("pmk {}", p.file_name().unwrap().to_string_lossy()), xml);
                    }
                }
            }
        }
    }
    // mixes.structured_ct of every music.db we have (opened read-only).
    for db in [
        "work/samples/editor-appdata/music.db",
        "work/samples/editor-appdata-testset/music.db",
        "work/device-dump/music.db",
        "work/compare-vol/.Pacemaker/music.db",
    ] {
        let p = repo().join(db);
        if !p.exists() {
            continue;
        }
        let Ok(con) = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { continue };
        let Ok(mut st) = con.prepare("SELECT structured_ct FROM mixes WHERE structured_ct IS NOT NULL") else { continue };
        let blobs: Vec<Vec<u8>> = st.query_map([], |r| r.get(0)).unwrap().flatten().collect();
        for (i, b) in blobs.iter().enumerate() {
            if let Ok(xml) = mixdoc::decode_blob(b) {
                add(format!("{db} mix #{i}"), xml);
            }
        }
    }
    docs
}

#[test]
fn real_mix_documents_round_trip_byte_exact() {
    let docs = real_documents();
    if docs.is_empty() {
        return eprintln!("skip: no real mix documents under work/");
    }
    eprintln!("{} distinct real mix documents", docs.len());
    for (label, xml) in &docs {
        let mix = Mix::parse(xml).unwrap_or_else(|e| panic!("{label}: {e}"));
        let again = mix.to_xml().unwrap();
        assert!(again == *xml, "{label}: parse -> to_xml differs from the original");
        // and through both containers
        let blob = mix.to_blob().unwrap();
        assert_eq!(mixdoc::decode_blob(&blob).unwrap(), *xml, "{label}");
        let pmk = mix.to_pmk().unwrap();
        assert_eq!(mixdoc::decode_pmk(&pmk).unwrap(), *xml, "{label}");
        assert!(!mix.tracks.is_empty(), "{label}");
    }
}
