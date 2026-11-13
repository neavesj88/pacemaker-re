//! The plumbing every app-side file shares, in the library the desktop app and `pm` both use so
//! they read and write the same files the same way: how a per-library file is named, how one is
//! written, and what happens to one that cannot be read.
//!
//! Nothing here knows what a file holds. A store (labels, saved filters, play log, watch folders,
//! smart cases, presets, caches, settings, ...) keeps its own format, its own version check and
//! its own wording for a problem, and calls these helpers for the parts that must not differ:
//!
//! - **Names.** [`fnv1a`] is the hash in every `<kind>-<16 hex digits>.json` name. Changing it
//!   would orphan every file already on a user's disk, so it is pinned by a test.
//! - **Writes.** [`write_atomic`]: temp file in the same folder, fsync, rename; the temp file is
//!   removed on any failure, so a crash leaves the old file or the new one, never half of either.
//! - **Unreadable files.** Never overwritten and never deleted. A damaged file is moved aside
//!   ([`set_aside`], to `<name>.corrupt`, `.corrupt.1`, ... so an earlier copy is never replaced),
//!   or copied once to `<name>.bad` ([`save_keeping_bad`]) before the next save replaces it.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// A volume root spelled one way, so `E:` and `E:\` are one library, and so are `/mnt/usb` and
/// `/mnt/usb/`: separators unified, trailing ones dropped, and on Windows (case-insensitive
/// paths) the case folded. The key the label and saved-filter files are named by.
pub fn volume_key(root: &Path) -> String {
    let s = root.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) { s.to_lowercase() } else { s.to_string() }
}

/// 64-bit FNV-1a: stable across Rust versions and runs (unlike the std hasher), which is what a
/// file name that has to be found again needs.
pub fn fnv1a(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A library root spelled one way: separators unified, trailing ones dropped, case folded, so
/// `E:` and `e:/` are one library.
pub fn root_key(root: &str) -> String {
    root.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// [`fnv1a`] of [`root_key`]: the per-library part of the file names of the smart cases, setlists,
/// play log, watch state and the energy and section caches.
pub fn root_hash(root: &str) -> u64 {
    fnv1a(&root_key(root))
}

/// Temp file next to `dest` (`dest` + `suffix`), written, flushed to disk, renamed over `dest`;
/// removed on any failure.
fn write_via(dest: &Path, bytes: &[u8], suffix: &str) -> std::io::Result<()> {
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(suffix);
    let tmp = PathBuf::from(tmp);
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, dest)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Writes an app-side file: `bytes` to `path` through `<path>.tmp` in the same folder (the
/// folder is created if needed), flushed to disk, then renamed over `path`. The temp file is
/// removed on any failure.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    write_via(path, bytes, ".tmp")
}

/// Writes a file the user chose (a playlist, a track list, a printable page, a mix file): through
/// `<dest>.part` beside it, so a failed or stopped write never leaves half a file under the real
/// name. The folder must exist already; a wrong destination is an error, not a new folder.
pub fn write_file(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_via(dest, bytes, ".part")
}

/// [`write_atomic`] with the error as `<path>: <reason>`.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_atomic(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// [`write_atomic`] of `value` as pretty JSON, with both kinds of error as plain text.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    write_atomic(path, text.as_bytes()).map_err(|e| e.to_string())
}

/// Where a damaged file is set aside: `<name>.corrupt`, or `<name>.corrupt.1`, `.2`, ... when that
/// is taken, so a second bad file never replaces the only copy of the first.
pub fn aside_path(bad: &Path) -> PathBuf {
    let mut base = bad.as_os_str().to_owned();
    base.push(".corrupt");
    let first = PathBuf::from(&base);
    if !first.exists() {
        return first;
    }
    (1..10_000)
        .map(|n| {
            let mut name = base.clone();
            name.push(format!(".{n}"));
            PathBuf::from(name)
        })
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Moves a damaged file to [`aside_path`]. A file that is already gone is fine. The caller must
/// not save over the damaged file when this fails.
pub fn set_aside(bad: &Path) -> std::io::Result<()> {
    match std::fs::rename(bad, aside_path(bad)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `<name>.bad`: the one copy of an unreadable file kept before it is replaced.
pub fn bad_copy_path(p: &Path) -> PathBuf {
    let mut name = p.as_os_str().to_owned();
    name.push(".bad");
    PathBuf::from(name)
}

/// Saves a settings file: when `unreadable` (the store's own load said the file could not be
/// used) and the file exists, it is first copied to [`bad_copy_path`], once, so a hand edit is
/// never silently lost; then `value` is written with [`write_json`].
pub fn save_keeping_bad<T: Serialize>(path: &Path, value: &T, unreadable: bool) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if unreadable && path.exists() {
        let bad = bad_copy_path(path);
        if !bad.exists() {
            std::fs::copy(path, &bad).map_err(|e| e.to_string())?;
        }
    }
    write_json(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_volume_is_one_library_however_it_is_spelled() {
        assert_eq!(volume_key(Path::new("/mnt/usb")), volume_key(Path::new("/mnt/usb/")));
        assert_eq!(volume_key(Path::new("E:")), volume_key(Path::new("E:\\")));
    }

    #[test]
    fn file_names_are_stable() {
        // These published FNV-1a vectors pin the hash that names files already on users' disks: they must never change.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn a_root_is_one_library_however_it_is_spelled() {
        assert_eq!(root_hash("E:\\"), root_hash("e:/"));
        assert_eq!(root_hash("E:"), root_hash("e:"));
        assert_ne!(root_hash("E:"), root_hash("F:"));
        assert_eq!(root_key("C:\\Music\\"), "c:/music");
    }

    #[test]
    fn write_atomic_creates_the_folder_replaces_the_file_and_leaves_no_temp() {
        let d = temp_dir("atomic");
        let p = d.join("sub").join("x.json");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        assert!(std::fs::read_dir(p.parent().unwrap()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_chosen_file_goes_through_a_part_file_and_never_creates_a_folder() {
        let d = temp_dir("file");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("list.m3u8");
        write_file(&f, b"one").unwrap();
        write_file(&f, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "two");
        assert!(write_file(&d.join("missing").join("x.txt"), b"x").is_err());
        assert!(!d.join("missing").exists());
        // the rename fails (a folder holds the name): the .part file is removed
        let blocked = d.join("blocked.m3u8");
        std::fs::create_dir_all(blocked.join("inner")).unwrap();
        assert!(write_file(&blocked, b"new").is_err());
        assert!(std::fs::read_dir(&d).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".part")));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_failed_write_removes_its_temp_file_and_keeps_the_old_file() {
        let d = temp_dir("fail");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("x.json");
        write_atomic(&p, b"old").unwrap();
        // a folder in the way of the rename: the write fails after the temp file exists
        let blocked = d.join("blocked.json");
        std::fs::create_dir_all(blocked.join("inner")).unwrap();
        assert!(write_atomic(&blocked, b"new").is_err());
        assert!(!d.join("blocked.json.tmp").exists(), "no temp file left");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "old");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_second_damaged_file_never_replaces_the_first_copy() {
        let d = temp_dir("aside");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("x.json");
        std::fs::write(&p, "first").unwrap();
        set_aside(&p).unwrap();
        std::fs::write(&p, "second").unwrap();
        set_aside(&p).unwrap();
        assert!(!p.exists());
        assert_eq!(std::fs::read_to_string(d.join("x.json.corrupt")).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(d.join("x.json.corrupt.1")).unwrap(), "second");
        set_aside(&p).unwrap(); // nothing there: fine
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_unreadable_settings_file_is_copied_once_before_it_is_replaced() {
        let d = temp_dir("bad");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("s.json");
        std::fs::write(&p, "{ hand edit").unwrap();
        save_keeping_bad(&p, &serde_json::json!({ "a": 1 }), true).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("s.json.bad")).unwrap(), "{ hand edit");
        // the file is readable now; a later unreadable one does not replace the first copy
        std::fs::write(&p, "{ again").unwrap();
        save_keeping_bad(&p, &serde_json::json!({ "a": 2 }), true).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("s.json.bad")).unwrap(), "{ hand edit");
        // a readable file leaves no copy
        let q = d.join("t.json");
        std::fs::write(&q, "{}").unwrap();
        save_keeping_bad(&q, &serde_json::json!({}), false).unwrap();
        assert!(!d.join("t.json.bad").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
