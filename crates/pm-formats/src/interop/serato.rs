//! Serato DJ / Scratch Live crates (`_Serato_/Subcrates/<name>.crate`).
//!
//! The format is not published by Serato; this reader follows the community reverse-engineering
//! write-ups, which agree with each other:
//!
//! * pyserato, "Serato Database Format" section of its README
//!   (<https://github.com/laker-93/pyserato>): a crate is a concatenated sequence of records,
//!   each a 4-byte ASCII tag, a 4-byte big-endian length and that many data bytes. By tag
//!   prefix, `o*` holds nested records, `t*` is UTF-16BE text, `p*` is UTF-16BE text holding a
//!   path relative to the root of the drive, `u*` / `s*` are 32-bit big-endian integers and
//!   `vrsn` is the UTF-16BE version string (`1.0/Serato ScratchLive Crate`). `otrk` is one
//!   track and the `ptrk` nested in it is its path. The crate's name is the file name only; the
//!   same page notes that some files carry Latin-1 paths (an `orvc` record follows the path).
//! * kerrickstaley's `parse_serato_crates.py` gist
//!   (<https://gist.github.com/kerrickstaley/8eb04988c02fa7c62e75c4c34c04cf02>): the same
//!   record layout, with `vrsn` / `t*` / `p*` as UTF-16BE, `u*` as a big-endian u32, `o*`
//!   recursed into and every other tag kept as raw bytes.
//!
//! UNVERIFIED here (no Serato installation or sample crate was available): the `Parent%%Child`
//! file name that Serato is commonly said to use for sub-crates (it is shown as `Parent / Child`,
//! and an unsplit name only costs a flatter case name), and the Latin-1 path variant (a path
//! without a control byte, or that is not valid UTF-16, is decoded as UTF-8 then Latin-1).
//!
//! A crate holds paths only, no title, artist or length, so a track can be matched only by its
//! file (name and size on this computer). The `database V2` file would add tags for tracks whose
//! files are not on this computer; it is not read.

use std::collections::HashMap;
use std::path::Path;

use super::paths;
use super::{bad, Error, ForeignLibrary, ForeignPlaylist, ForeignTrack, Source};

/// A crate file bigger than this is refused (real ones are kilobytes).
pub const MAX_CRATE_BYTES: u64 = 64 << 20;
/// Tracks read from one crate; also bounds memory for a crafted file.
pub const MAX_TRACKS_PER_CRATE: usize = 500_000;
/// Crate files read from one folder.
pub const MAX_CRATES: usize = 5000;
/// Crate bytes read from one folder in total: the per-file cap alone would let 5000 files of 64 MB
/// be read. Crates past the budget are skipped with a note.
pub const MAX_TOTAL_CRATE_BYTES: u64 = 256 << 20;

/// One record: its tag and its data.
type Record<'a> = ([u8; 4], &'a [u8]);

fn next_record<'a>(data: &'a [u8], pos: &mut usize) -> Result<Option<Record<'a>>, Error> {
    if *pos == data.len() {
        return Ok(None);
    }
    let rest = &data[*pos..];
    if rest.len() < 8 {
        return Err(bad("the crate file is cut off in the middle of a record header"));
    }
    let tag = [rest[0], rest[1], rest[2], rest[3]];
    let n = u32::from_be_bytes([rest[4], rest[5], rest[6], rest[7]]) as usize;
    if n > rest.len() - 8 {
        return Err(bad(format!(
            "a record ({}) claims {n} bytes but only {} remain: the crate file is damaged or cut off",
            String::from_utf8_lossy(&tag).escape_debug(),
            rest.len() - 8
        )));
    }
    *pos += 8 + n;
    Ok(Some((tag, &rest[8..8 + n])))
}

/// A `ptrk` value: UTF-16BE. A Latin-1 or UTF-8 path of even length is also valid UTF-16 (as
/// nonsense), so the choice is made on structure, not on how much of the text is Latin: a UTF-8 or
/// Latin-1 path never contains a control byte (below 0x20, NUL included), while UTF-16BE text
/// always does, because the high byte of every Latin letter, digit, `/` and `.` is 0x00 and the
/// high byte of Cyrillic is 0x04. A path has a `/` or an extension dot, so even a long Cyrillic or
/// CJK name with a short ASCII part is recognised. Anything else is read as UTF-8, then Latin-1.
fn decode_path(b: &[u8]) -> String {
    let utf16_shaped = b.len().is_multiple_of(2) && b.iter().any(|&c| c < 0x20);
    let utf16 = utf16_shaped
        .then(|| char::decode_utf16(b.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]]))).collect::<Result<String, _>>().ok())
        .flatten();
    let s = utf16.unwrap_or_else(|| match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    });
    s.replace(char::from(0), "")
}

/// The track paths of one crate, in order, exactly as stored (relative to the drive root).
pub fn parse_crate_paths(data: &[u8]) -> Result<Vec<String>, Error> {
    let mut pos = 0;
    match next_record(data, &mut pos)? {
        Some((tag, _)) if &tag == b"vrsn" => {}
        _ => return Err(bad("this is not a Serato crate (it does not start with a vrsn record)")),
    }
    let mut out = Vec::new();
    while let Some((tag, body)) = next_record(data, &mut pos)? {
        if &tag != b"otrk" {
            continue; // osrt, ovct, ... (sorting and column settings)
        }
        let mut p = 0;
        while let Some((t, v)) = next_record(body, &mut p)? {
            if &t == b"ptrk" {
                if out.len() >= MAX_TRACKS_PER_CRATE {
                    return Err(bad(format!("the crate has more than {MAX_TRACKS_PER_CRATE} tracks")));
                }
                out.push(decode_path(v));
                break;
            }
        }
    }
    Ok(out)
}

/// `Parent%%Child` -> `Parent / Child` (the crate's name lives in its file name only).
pub fn crate_display_name(stem: &str) -> String {
    let parts: Vec<&str> = stem.split("%%").map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        "Crate".to_string()
    } else {
        parts.join(" / ")
    }
}

fn slash(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// Folders a crate's relative paths may be relative to: the folder that holds `_Serato_` (an
/// external drive's root, or a Music folder), then the root of the drive that folder is on (the
/// home-folder case, where Serato writes `Users/<name>/Music/...`).
fn bases_for(crate_file: &Path) -> Vec<String> {
    let dir = crate_file.parent().unwrap_or(Path::new(""));
    let holder = dir
        .ancestors()
        .find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("_Serato_")))
        .and_then(|a| a.parent())
        .unwrap_or(dir);
    let mut bases = vec![slash(holder)];
    if let Some(root) = holder.ancestors().last() {
        let r = slash(root);
        if !r.is_empty() && !bases.contains(&r) {
            bases.push(r);
        }
    }
    bases
}

/// A stored relative path as an absolute one: the first base under which a file exists, else the
/// last base (so the preview shows where it looked). A path with `..` is left relative, which the
/// planner never probes.
fn resolve_rel(rel: &str, bases: &[String]) -> String {
    let r = rel.replace('\\', "/");
    let r = r.trim_start_matches('/');
    if r.is_empty() || r.split('/').any(|s| s == "..") {
        return r.to_string();
    }
    // a stored drive letter ("C:/Music/a.mp3") is already absolute
    if r.len() >= 3 && r.as_bytes()[0].is_ascii_alphabetic() && r.as_bytes()[1] == b':' && r.as_bytes()[2] == b'/' {
        return r.to_string();
    }
    let join = |b: &String| format!("{}/{}", b.trim_end_matches('/'), r);
    bases.iter().map(join).find(|c| paths::local_is_file(c)).unwrap_or_else(|| bases.last().map(join).unwrap_or_else(|| r.to_string()))
}

/// Builds the library model from `(crate file stem, parsed relative paths, crate file path)` triples.
/// Each crate is parsed as soon as it is read (see [`read_path`]) so its bytes are not kept.
fn build(crates: Vec<(String, Vec<String>, std::path::PathBuf)>, notes: Vec<String>) -> Result<ForeignLibrary, Error> {
    let mut tracks: Vec<ForeignTrack> = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut playlists = Vec::new();
    for (stem, rels, file) in crates {
        let bases = bases_for(&file);
        let mut pl = ForeignPlaylist { name: crate_display_name(&stem), ..Default::default() };
        for rel in rels {
            let path = resolve_rel(&rel, &bases);
            if path.is_empty() {
                pl.dangling += 1;
                continue;
            }
            let idx = *by_path.entry(path.clone()).or_insert_with(|| {
                let name = path.rsplit('/').next().unwrap_or(&path);
                let title = name.rsplit_once('.').map_or(name, |(s, _)| s).to_string();
                tracks.push(ForeignTrack { title, path: path.clone(), ..Default::default() });
                tracks.len() - 1
            });
            pl.tracks.push(idx);
        }
        playlists.push(pl);
    }
    if playlists.is_empty() {
        return Err(bad(match notes.first() {
            Some(n) => n.clone(),
            None => "no crates were found".to_string(),
        }));
    }
    Ok(ForeignLibrary { source: Source::Serato, tracks, playlists, notes })
}

fn read_crate_file(p: &Path) -> Result<Vec<u8>, Error> {
    let len = std::fs::metadata(p)?.len();
    if len > MAX_CRATE_BYTES {
        return Err(bad(format!("{} is larger than {} MB; it is not a crate", p.display(), MAX_CRATE_BYTES >> 20)));
    }
    Ok(std::fs::read(p)?)
}

/// The folder that holds the `.crate` files for a picked folder: the `_Serato_` folder, the folder
/// that contains it, or `Subcrates` itself.
fn subcrates_dir(dir: &Path) -> Option<std::path::PathBuf> {
    let is_named = |p: &Path, n: &str| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(n));
    if is_named(dir, "Subcrates") {
        return Some(dir.to_path_buf());
    }
    [dir.join("Subcrates"), dir.join("_Serato_").join("Subcrates")].into_iter().find(|c| c.is_dir())
}

/// Reads one `.crate` file, or every crate of a picked `_Serato_` / `Subcrates` / music-drive folder.
pub fn read_path(path: &Path) -> Result<ForeignLibrary, Error> {
    let meta = std::fs::metadata(path)?;
    let stem_of = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if meta.is_file() {
        let data = read_crate_file(path)?;
        return match parse_crate_paths(&data) {
            Ok(rels) => build(vec![(stem_of(path), rels, path.to_path_buf())], vec![]),
            Err(e) => Err(bad(format!("Skipped {}.crate: {e}", stem_of(path)))),
        };
    }
    let Some(sub) = subcrates_dir(path) else {
        return Err(bad("that folder has no _Serato_/Subcrates folder; pick the _Serato_ folder, or a .crate file"));
    };
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&sub)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("crate")))
        // symlink_metadata: a link loop or a link to elsewhere is not followed
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
        .collect();
    files.sort();
    let mut notes = Vec::new();
    if files.len() > MAX_CRATES {
        notes.push(format!("Only the first {MAX_CRATES} of {} crates were read.", files.len()));
        files.truncate(MAX_CRATES);
    }
    if sub.parent().is_some_and(|s| s.join("SmartCrates").is_dir()) {
        notes.push("Smart crates (rule-based) are not imported; only ordinary crates are.".to_string());
    }
    let crates = read_crates(files, MAX_TOTAL_CRATE_BYTES, &mut notes);
    build(crates, notes)
}

/// Reads and parses each crate in turn, dropping its bytes before the next is read, so memory is
/// bounded by one crate (`MAX_CRATE_BYTES`) plus the parsed paths. `budget` caps the bytes read in
/// total; a crate that would pass it, and every later one, is skipped with one note.
fn read_crates(files: Vec<std::path::PathBuf>, budget: u64, notes: &mut Vec<String>) -> Vec<(String, Vec<String>, std::path::PathBuf)> {
    let mut out = Vec::new();
    let mut left = budget;
    let mut over = 0usize;
    for f in files {
        let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let len = match std::fs::metadata(&f) {
            Ok(m) => m.len(),
            Err(e) => {
                notes.push(format!("Skipped {}: {e}", f.display()));
                continue;
            }
        };
        if len > left {
            over += 1;
            continue;
        }
        match read_crate_file(&f) {
            Ok(d) => {
                left = left.saturating_sub(d.len() as u64);
                match parse_crate_paths(&d) {
                    Ok(rels) => out.push((stem, rels, f)),
                    Err(e) => notes.push(format!("Skipped {stem}.crate: {e}")),
                }
            }
            Err(e) => notes.push(format!("Skipped {}: {e}", f.display())),
        }
    }
    if over > 0 {
        notes.push(format!(
            "{over} crate file{} not read: the folder holds more than {} MB of crates in total.",
            if over == 1 { " was" } else { "s were" },
            budget >> 20
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_be_bytes());
        v.extend(data);
        v
    }
    fn u16be(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|c| c.to_be_bytes()).collect()
    }
    fn crate_bytes(paths: &[&str]) -> Vec<u8> {
        let mut v = rec(b"vrsn", &u16be("1.0/Serato ScratchLive Crate"));
        v.extend(rec(b"osrt", &[rec(b"tvcn", &u16be("key")), rec(b"brev", &[0])].concat()));
        for p in paths {
            v.extend(rec(b"otrk", &[rec(b"ptrk", &u16be(p)), rec(b"utme", &[0, 0, 0, 1])].concat()));
        }
        v
    }

    #[test]
    fn reads_track_paths_and_skips_other_records() {
        let b = crate_bytes(&["Music/caf\u{e9} one.mp3", "Music/two.mp3"]);
        assert_eq!(parse_crate_paths(&b).unwrap(), vec!["Music/caf\u{e9} one.mp3", "Music/two.mp3"]);
        assert_eq!(parse_crate_paths(&crate_bytes(&[])).unwrap(), Vec::<String>::new());
        // a Latin-1 / odd-length path is still read
        let mut v = rec(b"vrsn", &u16be("1.0"));
        v.extend(rec(b"otrk", &rec(b"ptrk", b"Music/caf\xE9.mp3")));
        assert_eq!(parse_crate_paths(&v).unwrap(), vec!["Music/caf\u{e9}.mp3"]);
    }

    #[test]
    fn utf16_paths_decode_whatever_the_script_and_however_short_the_ascii_part() {
        for p in [
            "Музыка/Трек.mp3",
            "Артист - Очень длинное название песни.mp3",
            "音楽/日本語の曲名.mp3",
            "/音楽/日本語の曲名がとても長い.flac",
            "Μουσική/Τραγούδι.mp3",
            "Music/Музыка/Трек.mp3",
            "Users/x/Music/a.mp3",
            "a.mp3",
            "Music/caf\u{e9}.mp3",
            "🎵/x.mp3", // a surrogate pair
        ] {
            assert_eq!(decode_path(&u16be(p)), p, "{p}");
        }
        // a UTF-8 or Latin-1 path of even length is not mistaken for UTF-16
        assert_eq!(decode_path("Music/Музыка.mp3".as_bytes()), "Music/Музыка.mp3");
        assert_eq!(decode_path("Music/ab.mp3".as_bytes()), "Music/ab.mp3");
        assert_eq!(decode_path(b"Music/caf\xE9.mp3"), "Music/caf\u{e9}.mp3");
        // an unpaired surrogate is not valid UTF-16: it falls back instead of panicking
        let _ = decode_path(&[0x00, 0x41, 0xD8, 0x00, 0x00, 0x2E]);
        assert_eq!(decode_path(b""), "");
    }

    #[test]
    fn a_total_byte_budget_stops_the_folder_read_and_says_so() {
        let dir = std::env::temp_dir().join(format!("pm-serato-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let one = crate_bytes(&["Music/a.mp3"]);
        let files: Vec<std::path::PathBuf> = (0..4)
            .map(|i| {
                let p = dir.join(format!("c{i}.crate"));
                std::fs::write(&p, &one).unwrap();
                p
            })
            .collect();
        let mut notes = Vec::new();
        // room for two crates and a bit
        let got = read_crates(files.clone(), one.len() as u64 * 2 + 1, &mut notes);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, vec!["Music/a.mp3"]);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("2 crate files were not read"), "{notes:?}");
        // a generous budget reads all, with no note
        let mut notes = Vec::new();
        assert_eq!(read_crates(files, 1 << 20, &mut notes).len(), 4);
        assert!(notes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_crates_are_errors_never_panics_or_huge_allocations() {
        assert!(parse_crate_paths(b"").is_err(), "empty");
        assert!(parse_crate_paths(b"not a crate at all").is_err());
        assert!(parse_crate_paths(&rec(b"otrk", b"")).is_err(), "must start with vrsn");
        // a length far beyond the data
        let mut v = rec(b"vrsn", &u16be("1.0"));
        v.extend(b"otrk");
        v.extend(u32::MAX.to_be_bytes());
        v.extend([1, 2, 3]);
        assert!(parse_crate_paths(&v).is_err());
        // a nested record longer than its parent
        let mut v = rec(b"vrsn", &u16be("1.0"));
        v.extend(rec(b"otrk", &[b'p', b't', b'r', b'k', 0x7f, 0xff, 0xff, 0xff]));
        assert!(parse_crate_paths(&v).is_err());
        // every prefix of a valid file is Ok or Err, never a panic; the full file is Ok
        let full = crate_bytes(&["Music/a.mp3", "Music/b.mp3"]);
        for cut in 0..full.len() {
            let _ = parse_crate_paths(&full[..cut]);
        }
        assert!(parse_crate_paths(&full[..full.len() - 3]).is_err(), "cut mid-record is an error, not a partial import");
        // pseudo-random garbage behind a valid header
        let mut seed = 0x1234_5678u32;
        for _ in 0..200 {
            let mut g = rec(b"vrsn", &u16be("1.0"));
            for _ in 0..64 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                g.push((seed >> 24) as u8);
            }
            let _ = parse_crate_paths(&g);
        }
    }

    #[test]
    fn names_nest_with_double_percent() {
        assert_eq!(crate_display_name("Warm up"), "Warm up");
        assert_eq!(crate_display_name("Sets%%Friday%%Late"), "Sets / Friday / Late");
        assert_eq!(crate_display_name("%%"), "Crate");
    }

    #[test]
    fn relative_paths_resolve_against_the_serato_holder_then_the_drive_root() {
        let root = std::env::temp_dir().join(format!("pm-serato-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let sub = root.join("_Serato_").join("Subcrates");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(root.join("Music")).unwrap();
        std::fs::write(root.join("Music").join("a.mp3"), [0u8; 5]).unwrap();
        // the same file named from the drive root (the home-folder layout)
        let abs = slash(&root.join("Music").join("a.mp3"));
        let from_drive_root = if abs.as_bytes().get(1) == Some(&b':') { abs[3..].to_string() } else { abs.trim_start_matches('/').to_string() };
        std::fs::write(sub.join("Sets%%Friday.crate"), crate_bytes(&["Music/a.mp3", "Music/missing.mp3", "../etc/x.mp3"])).unwrap();
        std::fs::write(sub.join("Other.crate"), crate_bytes(&[&from_drive_root])).unwrap();
        std::fs::write(sub.join("Broken.crate"), b"garbage").unwrap();
        std::fs::write(sub.join("readme.txt"), b"x").unwrap();

        for pick in [root.clone(), root.join("_Serato_"), sub.clone()] {
            let f = read_path(&pick).unwrap();
            assert_eq!(f.source, Source::Serato);
            let names: Vec<&str> = f.playlists.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["Other", "Sets / Friday"], "sorted by file name, broken one skipped");
            assert!(f.notes.iter().any(|n| n.contains("Broken.crate")), "{:?}", f.notes);
            // the shared file is one track in both crates
            let a = f.tracks.iter().position(|t| t.path == abs).expect("resolved to the real file");
            assert_eq!(f.playlists[0].tracks, vec![a]);
            assert_eq!(f.playlists[1].tracks[0], a);
            assert_eq!(f.tracks[a].title, "a");
            // a missing file keeps the place it was looked for; a `..` path stays relative (never probed)
            assert!(f.tracks.iter().any(|t| t.path.ends_with("/Music/missing.mp3")));
            assert!(f.tracks.iter().any(|t| t.path == "../etc/x.mp3"), "a path with .. is kept relative");
        }
        // a single crate file
        let one = read_path(&sub.join("Other.crate")).unwrap();
        assert_eq!(one.playlists.len(), 1);
        // a folder without Serato data, and a lone broken crate, are clear errors
        assert!(read_path(&root.join("Music")).is_err());
        assert!(read_path(&sub.join("Broken.crate")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_crate_on_a_network_share_resolves_nothing() {
        let bases = vec!["//host/share".to_string()];
        let r = resolve_rel("Music/a.mp3", &bases);
        assert!(paths::rejection(&r).is_some(), "{r}");
        assert!(!paths::local_is_file(&r));
    }
}
