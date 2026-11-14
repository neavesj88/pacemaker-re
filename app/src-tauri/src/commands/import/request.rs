//! Track matcher I/O: read the request list the user picked (a text, CSV or playlist file) and
//! save the list of missing tracks. The matching itself runs in the UI (a pure module,
//! `views/matcher/matchLogic.ts`); these two commands only move text across the file system.
//!
//! Neither touches the device volume: reading is read-only and a save refuses any destination
//! inside the library's root (a stray text file there would only confuse the Editor's clean-up).

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Largest request list that is read or saved. A list of ten thousand tracks is well under 1 MB.
pub const MAX_LIST_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct RequestFile {
    pub text: String,
    pub bytes: u64,
    /// "utf-8", "utf-16le", "utf-16be" or "latin-1" (the fallback for text that is not valid UTF-8).
    pub encoding: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SavedText {
    pub path: String,
    pub bytes: u64,
}

/// Decodes a text file: UTF-8 (BOM stripped), UTF-16 with a BOM, else UTF-8 if valid, else
/// Latin-1 (every byte is a code point, so it never fails). Binary data (NUL bytes) is refused.
pub fn decode_text(bytes: &[u8]) -> Result<(String, &'static str), String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Ok((String::from_utf8_lossy(rest).into_owned(), "utf-8"));
    }
    for (bom, big_endian, name) in [([0xFF, 0xFE], false, "utf-16le"), ([0xFE, 0xFF], true, "utf-16be")] {
        if let Some(rest) = bytes.strip_prefix(&bom) {
            if rest.len() % 2 != 0 {
                return Err("The file looks like UTF-16 text but is cut off in the middle of a character.".into());
            }
            let units = rest.as_chunks::<2>().0.iter().map(|p| if big_endian { u16::from_be_bytes(*p) } else { u16::from_le_bytes(*p) });
            let text: String = char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect();
            return Ok((text, name));
        }
    }
    if bytes.contains(&0) {
        return Err("This does not look like a text file (it contains binary data).".into());
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok((s.to_string(), "utf-8")),
        Err(_) => Ok((bytes.iter().map(|&b| b as char).collect(), "latin-1")),
    }
}

/// Reads and decodes a request list. Refuses folders and files over [`MAX_LIST_BYTES`].
pub fn read_list(path: &Path) -> Result<RequestFile, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file.", path.display()));
    }
    if meta.len() > MAX_LIST_BYTES {
        return Err(format!("The file is {} MB; a request list can be at most {} MB.", meta.len() / (1024 * 1024), MAX_LIST_BYTES / (1024 * 1024)));
    }
    let mut buf = Vec::new();
    // `take` guards against a file that grows between the size check and the read
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_LIST_BYTES + 1).read_to_end(&mut buf))
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if buf.len() as u64 > MAX_LIST_BYTES {
        return Err(format!("The file is larger than {} MB.", MAX_LIST_BYTES / (1024 * 1024)));
    }
    let (text, encoding) = decode_text(&buf)?;
    Ok(RequestFile { text, bytes: buf.len() as u64, encoding })
}

/// Checks a save destination: a `.txt` or `.csv` file name in an existing folder outside the
/// library volume `root`.
pub fn check_dest(root: &Path, dest: &Path) -> Result<(), String> {
    if dest.is_dir() {
        return Err("The destination is a folder; choose a file name.".into());
    }
    if !dest.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt") || e.eq_ignore_ascii_case("csv")) {
        return Err("The file name must end in .txt or .csv.".into());
    }
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|_| "The destination folder does not exist.".to_string())?;
    if let Ok(r) = root.canonicalize()
        && parent.starts_with(&r)
    {
        return Err("Choose a folder outside the library: this list is not part of it.".into());
    }
    Ok(())
}

/// Writes `text` to `dest` beside-and-rename, so a failed write never leaves half a file.
pub fn save_text(root: &Path, dest: &Path, text: &str) -> Result<SavedText, String> {
    if text.len() as u64 > MAX_LIST_BYTES {
        return Err(format!("The list is larger than {} MB.", MAX_LIST_BYTES / (1024 * 1024)));
    }
    check_dest(root, dest)?;
    crate::store::write_file(dest, text.as_bytes()).map_err(|e| format!("Cannot write {}: {e}", dest.display()))?;
    Ok(SavedText { path: dest.to_string_lossy().into_owned(), bytes: text.len() as u64 })
}

/// Reads a request list the user picked in the file dialog (text, CSV, TSV, M3U/M3U8 or any
/// text file). UTF-8, UTF-16 and Latin-1 are understood; nothing is written.
#[tauri::command]
pub async fn matcher_read_list(path: String) -> AppResult<RequestFile> {
    tauri::async_runtime::spawn_blocking(move || read_list(Path::new(&path)).map_err(AppError::from))
        .await
        .map_err(|e| AppError::from(e.to_string()))?
}

/// Saves text (the missing-tracks list) to `dest`, a path the user chose in the save dialog.
/// Nothing on the volume at `root` is changed; a destination inside it is refused.
#[tauri::command]
pub async fn matcher_save_text(root: String, dest: String, text: String) -> AppResult<SavedText> {
    tauri::async_runtime::spawn_blocking(move || save_text(Path::new(&root), Path::new(&dest), &text).map_err(AppError::from))
        .await
        .map_err(|e| AppError::from(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-matcher-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn decodes_utf8_with_and_without_bom() {
        assert_eq!(decode_text("Sample Artist - Tune".as_bytes()).unwrap(), ("Sample Artist - Tune".to_string(), "utf-8"));
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice("Béla - Tune".as_bytes());
        assert_eq!(decode_text(&b).unwrap(), ("Béla - Tune".to_string(), "utf-8"));
    }

    #[test]
    fn decodes_utf16_both_orders() {
        let units: Vec<u16> = "Zoë - Tune".encode_utf16().collect();
        let mut le = vec![0xFF, 0xFE];
        le.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        let mut be = vec![0xFE, 0xFF];
        be.extend(units.iter().flat_map(|u| u.to_be_bytes()));
        assert_eq!(decode_text(&le).unwrap(), ("Zoë - Tune".to_string(), "utf-16le"));
        assert_eq!(decode_text(&be).unwrap(), ("Zoë - Tune".to_string(), "utf-16be"));
        assert!(decode_text(&[0xFF, 0xFE, 0x41]).is_err(), "odd-length UTF-16 is damaged");
    }

    #[test]
    fn invalid_utf8_falls_back_to_latin1_and_binary_is_refused() {
        // 0xE9 is "é" in Latin-1 and an invalid UTF-8 sequence on its own
        assert_eq!(decode_text(&[b'C', b'a', b'f', 0xE9]).unwrap(), ("Café".to_string(), "latin-1"));
        assert!(decode_text(&[b'M', b'Z', 0, 3, 0, 0]).is_err());
    }

    #[test]
    fn read_list_checks_kind_and_size() {
        let d = scratch("read");
        let f = d.join("list.txt");
        std::fs::write(&f, "A - B\r\nC - D\r\n").unwrap();
        let r = read_list(&f).unwrap();
        assert_eq!((r.text.as_str(), r.bytes, r.encoding), ("A - B\r\nC - D\r\n", 14, "utf-8"));
        assert!(read_list(&d).unwrap_err().contains("not a file"));
        assert!(read_list(&d.join("missing.txt")).is_err());
        let big = d.join("big.txt");
        std::fs::write(&big, vec![b'a'; (MAX_LIST_BYTES + 1) as usize]).unwrap();
        assert!(read_list(&big).unwrap_err().contains("at most"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_writes_atomically_and_replaces_an_existing_file() {
        let d = scratch("save");
        let root = scratch("save-root");
        let dest = d.join("missing.csv");
        let s = save_text(&root, &dest, "Artist,Title\n").unwrap();
        assert_eq!(s.bytes, 13);
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "Artist,Title\n");
        save_text(&root, &dest, "x").unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "x");
        assert!(!d.join("missing.csv.part").exists(), "no temp file is left");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_refuses_bad_destinations() {
        let d = scratch("dest");
        let root = scratch("dest-root");
        assert!(save_text(&root, &d, "x").unwrap_err().contains("folder"));
        assert!(save_text(&root, &d.join("list.exe"), "x").unwrap_err().contains(".txt or .csv"));
        assert!(save_text(&root, &d.join("nope").join("list.txt"), "x").unwrap_err().contains("does not exist"));
        // inside the device volume
        assert!(save_text(&root, &root.join("list.txt"), "x").unwrap_err().contains("outside the library"));
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(save_text(&root, &sub.join("list.txt"), "x").is_err());
        assert!(!root.join("list.txt").exists());
        assert!(save_text(&root, &d.join("list.TXT"), "x").is_ok(), "the extension check ignores case");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&root);
    }
}
