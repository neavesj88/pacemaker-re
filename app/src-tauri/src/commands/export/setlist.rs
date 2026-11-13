//! Printable setlist: the app-side store for what a setlist needs beyond the library (event
//! details, a note and an energy rating per track) and the save-to-file command for the HTML
//! page the UI builds.
//!
//! None of this is on the device. The Pacemaker has no field for a note or an energy rating, so
//! they live in one JSON file per library in the app's config dir (same pattern as the smart
//! cases: file named by a hash of the volume root, temp + rename, a damaged file is reported and
//! set aside rather than silently emptied). The HTML itself is built by the UI
//! (`views/setlist/setlistLogic.ts`) so the preview, the print and the saved file are one text;
//! this side only writes it, never inside the device volume.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::store::config_dir;

/// Most characters in a one-line field (title, event, venue, DJ, date).
pub const MAX_LINE: usize = 200;
/// Most characters in the footer text.
pub const MAX_FOOTER: usize = 1000;
/// Most characters in one track note.
pub const MAX_NOTE: usize = 300;
/// Most saved entries per setlist (a case or mix this long would not print).
pub const MAX_ENTRIES: usize = 5000;
/// Most saved setlists per library.
pub const MAX_SHEETS: usize = 500;
/// Largest HTML page that is saved.
pub const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;
/// Highest energy rating.
pub const MAX_ENERGY: u8 = 10;

/// What the user typed for one track of a setlist.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub track_id: i64,
    pub note: String,
    /// 1 to [`MAX_ENERGY`], 0 = not set.
    pub energy: u8,
}

/// The extra content of one setlist (a case or a mix): header text and per-track entries.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Sheet {
    /// Heading; empty = use the name of the case or mix.
    pub title: String,
    pub event: String,
    pub venue: String,
    /// Free text ("Sat 14 Nov"): a setlist is often for a date the user writes their own way.
    pub date: String,
    pub dj: String,
    pub footer: String,
    /// "HH:MM" the set starts at, or empty. Used when the page shows clock times.
    pub start_clock: String,
    pub entries: Vec<Entry>,
}

impl Sheet {
    fn is_blank(&self) -> bool {
        *self == Sheet::default()
    }
}

/// A sheet as loaded, with a message when the store file could not be read.
#[derive(Debug, Serialize)]
pub struct LoadedSheet {
    pub sheet: Sheet,
    /// Set when the store file is damaged. Saving then moves the damaged file aside.
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SavedHtml {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    sheets: BTreeMap<String, Sheet>,
}

// ---- validation --------------------------------------------------------------------------

/// A sheet key is `case:<id>` or `mix:<id>`.
pub fn check_key(key: &str) -> Result<(), String> {
    let ok = key.split_once(':').is_some_and(|(kind, id)| {
        (kind == "case" || kind == "mix") && !id.is_empty() && id.len() <= 18 && id.bytes().all(|b| b.is_ascii_digit())
    });
    if ok { Ok(()) } else { Err(format!("'{key}' is not a setlist key (expected case:<id> or mix:<id>).")) }
}

/// One line of text: control characters become spaces, ends trimmed, cut to `max` characters.
fn clean_line(s: &str, max: usize) -> String {
    let flat: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    flat.trim().chars().take(max).collect()
}

/// Several lines: only `\n` survives (CR and other controls are dropped), cut to `max` characters.
fn clean_text(s: &str, max: usize) -> String {
    let kept: String = s.chars().filter_map(|c| if c == '\n' { Some(c) } else if c == '\r' { None } else if c.is_control() { Some(' ') } else { Some(c) }).collect();
    kept.trim().chars().take(max).collect()
}

/// "H:MM" or "HH:MM" on a 24-hour clock, normalised to "HH:MM"; empty stays empty.
fn clean_clock(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(String::new());
    }
    let bad = || format!("'{t}' is not a start time (use HH:MM on a 24-hour clock).");
    let (h, m) = t.split_once(':').ok_or_else(bad)?;
    if !(1..=2).contains(&h.len()) || m.len() != 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let (h, m): (u8, u8) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if h > 23 || m > 59 {
        return Err(bad());
    }
    Ok(format!("{h:02}:{m:02}"))
}

/// Cleans a sheet the UI sent: trims and limits text, drops entries with nothing in them, keeps the
/// last entry for a repeated track, refuses what cannot be fixed (bad time, energy over the
/// maximum, too many entries).
pub fn sanitize(s: Sheet) -> Result<Sheet, String> {
    let mut by_track: BTreeMap<i64, Entry> = BTreeMap::new();
    for e in s.entries {
        if e.energy > MAX_ENERGY {
            return Err(format!("Energy is a rating from 1 to {MAX_ENERGY}."));
        }
        let note = clean_line(&e.note, MAX_NOTE);
        if note.is_empty() && e.energy == 0 {
            by_track.remove(&e.track_id);
        } else {
            by_track.insert(e.track_id, Entry { track_id: e.track_id, note, energy: e.energy });
        }
    }
    if by_track.len() > MAX_ENTRIES {
        return Err(format!("A setlist can keep notes for at most {MAX_ENTRIES} tracks."));
    }
    Ok(Sheet {
        title: clean_line(&s.title, MAX_LINE),
        event: clean_line(&s.event, MAX_LINE),
        venue: clean_line(&s.venue, MAX_LINE),
        date: clean_line(&s.date, MAX_LINE),
        dj: clean_line(&s.dj, MAX_LINE),
        footer: clean_text(&s.footer, MAX_FOOTER),
        start_clock: clean_clock(&s.start_clock)?,
        entries: by_track.into_values().collect(),
    })
}

// ---- storage -----------------------------------------------------------------------------

/// The file for a library: named by a hash of its root path (a drive letter or mount point), so
/// nothing about the device is written next to the setlists. The hash is the one the smart
/// cases use, so the two files of a library sit together under matching names.
pub fn store_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("setlists-{:016x}.json", crate::store::root_hash(root)))
}

/// Serialises read-modify-write of the store: the UI saves while the user types.
static STORE_LOCK: Mutex<()> = Mutex::new(());

enum Read {
    Ok(StoreFile),
    Damaged(String),
}

fn read_store(p: &Path) -> Result<Read, String> {
    match std::fs::read_to_string(p) {
        Ok(s) => Ok(serde_json::from_str::<StoreFile>(&s).map_or_else(|e| Read::Damaged(format!("{} is not readable: {e}", p.display())), Read::Ok)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Read::Ok(StoreFile::default())),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// The saved sheet for `key` (an empty one if there is none).
pub fn load(dir: &Path, root: &str, key: &str) -> Result<LoadedSheet, String> {
    check_key(key)?;
    let _g = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(match read_store(&store_path(dir, root))? {
        Read::Ok(mut f) => LoadedSheet { sheet: f.sheets.remove(key).unwrap_or_default(), warning: None },
        Read::Damaged(why) => LoadedSheet { sheet: Sheet::default(), warning: Some(format!("{why}. Your saved setlist notes could not be read; saving now moves that file aside and starts fresh.")) },
    })
}

/// Saves the sheet for `key`; a blank sheet removes it. Returns the sheet as stored (cleaned).
pub fn save(dir: &Path, root: &str, key: &str, sheet: Sheet) -> Result<Sheet, String> {
    check_key(key)?;
    let sheet = sanitize(sheet)?;
    let _g = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let p = store_path(dir, root);
    let mut file = match read_store(&p)? {
        Read::Ok(f) => f,
        Read::Damaged(_) => {
            // keep the damaged file for the user, never overwrite it (or an earlier damaged copy)
            crate::store::set_aside(&p).map_err(|e| format!("Cannot move the damaged {} aside: {e}", p.display()))?;
            StoreFile::default()
        }
    };
    if sheet.is_blank() {
        file.sheets.remove(key);
    } else {
        if !file.sheets.contains_key(key) && file.sheets.len() >= MAX_SHEETS {
            return Err(format!("Notes are saved for {MAX_SHEETS} setlists already; clear some before adding more."));
        }
        file.sheets.insert(key.to_string(), sheet.clone());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    crate::store::write_atomic(&p, text.as_bytes()).map_err(|e| format!("Cannot write {}: {e}", p.display()))?;
    Ok(sheet)
}

/// Writes the page the user saves (see `crate::store::write_file`).
fn write_page(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::store::write_file(dest, bytes).map_err(|e| format!("Cannot write {}: {e}", dest.display()))
}

// ---- saving the page ---------------------------------------------------------------------

/// Checks a save destination: an `.html` or `.htm` file name in an existing folder outside the
/// library volume `root`.
pub fn check_dest(root: &Path, dest: &Path) -> Result<(), String> {
    if dest.is_dir() {
        return Err("The destination is a folder; choose a file name.".into());
    }
    if !dest.extension().is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm")) {
        return Err("The file name must end in .html.".into());
    }
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|_| "The destination folder does not exist.".to_string())?;
    if let Ok(r) = root.canonicalize()
        && parent.starts_with(&r)
    {
        return Err("Choose a folder outside the device: a setlist page is not part of its library.".into());
    }
    Ok(())
}

/// Writes the page to `dest` (temp file, then rename).
pub fn save_html(root: &Path, dest: &Path, html: &str) -> Result<SavedHtml, String> {
    if html.len() > MAX_HTML_BYTES {
        return Err(format!("The page is larger than {} MB.", MAX_HTML_BYTES / (1024 * 1024)));
    }
    check_dest(root, dest)?;
    write_page(dest, html.as_bytes())?;
    Ok(SavedHtml { path: dest.to_string_lossy().into_owned(), bytes: html.len() as u64 })
}

// ---- commands ----------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f().map_err(AppError::from))
        .await
        .map_err(|e| AppError::from(e.to_string()))?
}

/// The saved header text, notes and energy ratings of a case (`case:<id>`) or mix (`mix:<id>`) of
/// the library at `root`. Read-only; nothing on the device is touched.
#[tauri::command]
pub async fn setlist_sheet_load(app: tauri::AppHandle, root: String, key: String) -> AppResult<LoadedSheet> {
    let dir = config_dir(&app)?;
    blocking(move || load(&dir, &root, &key)).await
}

/// Saves them in the app's config dir (not on the device). Returns the cleaned sheet.
#[tauri::command]
pub async fn setlist_sheet_save(app: tauri::AppHandle, root: String, key: String, sheet: Sheet) -> AppResult<Sheet> {
    let dir = config_dir(&app)?;
    blocking(move || save(&dir, &root, &key, sheet)).await
}

/// Saves the setlist page to `dest`, a path the user chose in the save dialog. A destination
/// inside the device volume at `root` is refused.
#[tauri::command]
pub async fn setlist_save_html(root: String, dest: String, html: String) -> AppResult<SavedHtml> {
    blocking(move || save_html(Path::new(&root), Path::new(&dest), &html)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-setlist-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn entry(track_id: i64, note: &str, energy: u8) -> Entry {
        Entry { track_id, note: note.into(), energy }
    }

    #[test]
    fn keys_are_case_or_mix_with_a_number() {
        assert!(check_key("case:12").is_ok());
        assert!(check_key("mix:0").is_ok());
        for bad in ["", "case", "case:", "case:x", "track:1", "case:1:2", "case:-1", "case:1234567890123456789", "../x:1"] {
            assert!(check_key(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn clock_is_normalised_or_refused() {
        assert_eq!(clean_clock(" 9:05 ").unwrap(), "09:05");
        assert_eq!(clean_clock("23:59").unwrap(), "23:59");
        assert_eq!(clean_clock("").unwrap(), "");
        for bad in ["24:00", "12:60", "12", "1:5", "ab:cd", "12:345", "-1:00", "12:3a"] {
            assert!(clean_clock(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sanitize_cleans_text_and_drops_empty_entries() {
        let s = sanitize(Sheet {
            title: "  Late\tset\n".into(),
            footer: "line one\r\nline two\u{7}".into(),
            entries: vec![entry(2, "  first ", 0), entry(2, "second", 3), entry(5, "   ", 0), entry(7, "", 4), entry(9, "keep then clear", 2), entry(9, "", 0)],
            ..Sheet::default()
        })
        .unwrap();
        assert_eq!(s.title, "Late set");
        assert_eq!(s.footer, "line one\nline two");
        assert_eq!(s.entries, vec![entry(2, "second", 3), entry(7, "", 4)], "the last entry of a track wins, empty ones are dropped");
    }

    #[test]
    fn sanitize_limits_lengths_and_refuses_bad_energy() {
        let long = "x".repeat(1000);
        let s = sanitize(Sheet { event: long.clone(), footer: long.clone(), entries: vec![entry(1, &long, 1)], ..Sheet::default() }).unwrap();
        assert_eq!(s.event.chars().count(), MAX_LINE);
        assert_eq!(s.footer.chars().count(), MAX_FOOTER);
        assert_eq!(s.entries[0].note.chars().count(), MAX_NOTE);
        assert!(sanitize(Sheet { entries: vec![entry(1, "a", 11)], ..Sheet::default() }).is_err());
        assert!(sanitize(Sheet { start_clock: "25:00".into(), ..Sheet::default() }).is_err());
        let many = (0..=MAX_ENTRIES as i64).map(|i| entry(i, "n", 0)).collect();
        assert!(sanitize(Sheet { entries: many, ..Sheet::default() }).unwrap_err().contains("at most"));
        let multibyte = sanitize(Sheet { venue: "é".repeat(300), ..Sheet::default() }).unwrap();
        assert_eq!(multibyte.venue.chars().count(), MAX_LINE, "cut on characters, never inside one");
    }

    #[test]
    fn store_path_ignores_case_and_trailing_slash_but_not_the_volume() {
        let dir = Path::new("cfg");
        assert_eq!(store_path(dir, "P:\\"), store_path(dir, "p:/"));
        assert_ne!(store_path(dir, "P:\\"), store_path(dir, "Q:\\"));
        assert!(store_path(dir, "P:\\").file_name().unwrap().to_string_lossy().starts_with("setlists-"));
    }

    #[test]
    fn save_then_load_round_trips_and_a_blank_sheet_removes_the_key() {
        let dir = scratch("store");
        assert_eq!(load(&dir, "P:\\", "case:1").unwrap().sheet, Sheet::default(), "nothing saved yet");
        let want = Sheet { title: "Friday".into(), venue: "Example Room".into(), start_clock: "21:30".into(), entries: vec![entry(3, "intro", 5)], ..Sheet::default() };
        assert_eq!(save(&dir, "P:\\", "case:1", want.clone()).unwrap(), want);
        save(&dir, "P:\\", "mix:1", Sheet { dj: "Example DJ".into(), ..Sheet::default() }).unwrap();
        assert_eq!(load(&dir, "p:/", "case:1").unwrap().sheet, want, "same volume under another spelling");
        assert_eq!(load(&dir, "P:\\", "mix:1").unwrap().sheet.dj, "Example DJ");
        assert_eq!(load(&dir, "Q:\\", "case:1").unwrap().sheet, Sheet::default(), "another library has its own file");
        assert!(!store_path(&dir, "P:\\").with_extension("json.tmp").exists(), "no temp file is left");

        save(&dir, "P:\\", "case:1", Sheet::default()).unwrap();
        assert_eq!(load(&dir, "P:\\", "case:1").unwrap().sheet, Sheet::default());
        assert_eq!(load(&dir, "P:\\", "mix:1").unwrap().sheet.dj, "Example DJ", "the other sheet stays");
        assert!(save(&dir, "P:\\", "bogus", Sheet::default()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_store_is_reported_then_set_aside_on_save() {
        let dir = scratch("damaged");
        let p = store_path(&dir, "P:\\");
        std::fs::write(&p, "not json").unwrap();
        let l = load(&dir, "P:\\", "case:1").unwrap();
        assert_eq!(l.sheet, Sheet::default());
        assert!(l.warning.unwrap().contains("not readable"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "not json", "loading does not touch the file");
        save(&dir, "P:\\", "case:1", Sheet { dj: "Example DJ".into(), ..Sheet::default() }).unwrap();
        let kept = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains(".corrupt")).count();
        assert_eq!(kept, 1, "the damaged file is kept");
        let l = load(&dir, "P:\\", "case:1").unwrap();
        assert!(l.warning.is_none());
        assert_eq!(l.sheet.dj, "Example DJ");
        // a second damaged file must not replace the first copy
        std::fs::write(&p, "second").unwrap();
        save(&dir, "P:\\", "case:1", Sheet { dj: "Example DJ".into(), ..Sheet::default() }).unwrap();
        let mut texts: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains(".corrupt")).map(|e| std::fs::read_to_string(e.path()).unwrap()).collect();
        texts.sort();
        assert_eq!(texts, vec!["not json".to_string(), "second".to_string()], "both damaged files are kept");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_sheet_count_is_capped() {
        let dir = scratch("cap");
        let sheet = Sheet { dj: "d".into(), ..Sheet::default() };
        for i in 0..MAX_SHEETS {
            save(&dir, "P:\\", &format!("case:{i}"), sheet.clone()).unwrap();
        }
        assert!(save(&dir, "P:\\", "case:99999", sheet.clone()).unwrap_err().contains("already"));
        assert!(save(&dir, "P:\\", "case:0", Sheet { dj: "again".into(), ..Sheet::default() }).is_ok(), "changing an existing one is fine at the cap");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_html_writes_atomically_and_replaces() {
        let d = scratch("html");
        let root = scratch("html-root");
        let dest = d.join("set.html");
        let s = save_html(&root, &dest, "<!doctype html><p>one</p>").unwrap();
        assert_eq!(s.bytes, 25);
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "<!doctype html><p>one</p>");
        save_html(&root, &dest, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "two");
        assert!(!d.join("set.html.part").exists());
        assert!(save_html(&root, &d.join("SET.HTM"), "x").is_ok(), "extension check ignores case");
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn save_html_refuses_bad_destinations_and_oversize_pages() {
        let d = scratch("html-bad");
        let root = scratch("html-bad-root");
        assert!(save_html(&root, &d, "x").unwrap_err().contains("folder"));
        assert!(save_html(&root, &d.join("set.txt"), "x").unwrap_err().contains(".html"));
        assert!(save_html(&root, &d.join("nope").join("set.html"), "x").unwrap_err().contains("does not exist"));
        assert!(save_html(&root, &root.join("set.html"), "x").unwrap_err().contains("outside the device"));
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(save_html(&root, &sub.join("set.html"), "x").is_err());
        assert!(!root.join("set.html").exists());
        assert!(save_html(&root, &d.join("big.html"), &"a".repeat(MAX_HTML_BYTES + 1)).unwrap_err().contains("larger"));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&root);
    }
}
