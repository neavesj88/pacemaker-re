//! Paths read out of other programs' files are untrusted: a library file from another machine
//! (or a crafted one) can name `\\host\share\x.mp3`, `file:////host/share/x.mp3`, a device path
//! such as `\\.\pipe\x` or `\\?\C:\x`, or a relative path. Touching those from here would make
//! Windows reach out to a network host (SMB authentication and all), so every file system probe
//! in the importers goes through [`local_file_len`] / [`local_is_file`], which refuse anything that
//! is not a plain absolute path on a local-looking volume, without calling into the OS.
//!
//! Limits, stated plainly: a mapped network drive letter (`Z:\...`) and a POSIX path under a
//! network mount look local and are probed like any other path; nothing here asks the OS what a
//! drive letter is.

use std::path::Path;

/// Why a foreign path is not probed, or `None` when it is a plain absolute local path.
pub fn rejection(path: &str) -> Option<&'static str> {
    let p = path.trim();
    if p.is_empty() {
        return Some("it is empty");
    }
    if p.contains('\0') {
        return Some("it contains a NUL character");
    }
    let b = p.as_bytes();
    let sep = |c: u8| c == b'/' || c == b'\\';
    // UNC (`\\host\share`, `//host/share`) and every device form (`\\.\`, `\\?\`, `//./`)
    if b.len() >= 2 && sep(b[0]) && sep(b[1]) {
        return Some("it points at a network or device location");
    }
    if p.starts_with("\\??\\") || p.starts_with("/??/") {
        return Some("it points at a device location");
    }
    // a URL scheme that survived (smb:, http:, ftp:...): two or more scheme characters before ':'
    if let Some(colon) = p.find(':') {
        if colon >= 2 && p[..colon].bytes().all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.') {
            return Some("it is a URL, not a file path");
        }
    }
    // `C:` followed by a separator is an absolute drive path; a bare `C:foo` is drive-relative
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return if b.len() >= 3 && sep(b[2]) { None } else { Some("it is relative to a drive's current folder") };
    }
    if sep(b[0]) {
        return None;
    }
    Some("it is a relative path")
}

/// True for a plain absolute path on a local-looking volume.
pub fn is_local(path: &str) -> bool {
    rejection(path).is_none()
}

/// Size of the regular file at `path`; `None` when it is not a plain local path, does not exist
/// or is not a file. A symlink is followed only if it ends at a regular file.
pub fn local_file_len(path: &str) -> Option<u64> {
    if !is_local(path) {
        return None;
    }
    std::fs::metadata(Path::new(path)).ok().filter(|m| m.is_file()).map(|m| m.len())
}

pub fn local_is_file(path: &str) -> bool {
    local_file_len(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_network_device_url_and_relative_paths() {
        for bad in [
            "",
            "   ",
            "\\\\host\\share\\a.mp3",
            "//host/share/a.mp3",
            "\\\\.\\pipe\\x",
            "\\\\?\\C:\\a.mp3",
            "\\\\?\\UNC\\host\\share\\a.mp3",
            "//./COM1",
            "\\??\\C:\\a.mp3",
            "smb://host/share/a.mp3",
            "http://example.invalid/a.mp3",
            "file://host/a.mp3",
            "C:a.mp3",
            "music/a.mp3",
            "./a.mp3",
            "a.mp3",
            "C:\\a\0b.mp3",
        ] {
            assert!(rejection(bad).is_some(), "{bad:?} must be refused");
            assert!(!is_local(bad));
        }
    }

    #[test]
    fn accepts_plain_absolute_paths() {
        for ok in ["C:\\Music\\a.mp3", "C:/Music/a.mp3", "d:\\x", "/home/u/a.mp3", "/Volumes/DISK/a.mp3", "/music/a: b.mp3"] {
            assert_eq!(rejection(ok), None, "{ok:?}");
        }
    }

    #[test]
    fn refused_paths_are_never_probed() {
        assert_eq!(local_file_len("\\\\invalid-host-for-test\\share\\a.mp3"), None);
        let f = std::env::temp_dir().join(format!("pm-paths-{}.bin", std::process::id()));
        std::fs::write(&f, [0u8; 7]).unwrap();
        assert_eq!(local_file_len(&f.display().to_string()), Some(7));
        assert!(!local_is_file(&std::env::temp_dir().display().to_string()), "a folder is not a file");
        let _ = std::fs::remove_file(&f);
    }
}
