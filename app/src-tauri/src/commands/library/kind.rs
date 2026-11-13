//! What kind of place a library root is: a Pacemaker (a drive or card the device reads) or a
//! folder on this computer. Pure text rules, no file access, so they hold on any host OS and the
//! tests can name Windows paths on Linux and the other way round.

pub const PACEMAKER: &str = "pacemaker";
pub const COMPUTER: &str = "computer";

/// "pacemaker" when `root` is a volume root, else "computer".
///
/// A volume root is what a docked device or a card in a reader looks like: a drive root on
/// Windows (`G:`, `G:\`, `G:/`, also with the `\\?\` prefix), and on other systems a mount point
/// of the kind `pm_formats::library::find_volumes` scans (one or two levels under `/Volumes`,
/// `/media`, `/run/media` or `/mnt`). Everything else is a folder on this computer: subfolders
/// of a drive, network shares (`\\server\share`), relative paths.
pub fn library_kind(root: &str) -> &'static str {
    if is_volume_root(root) { PACEMAKER } else { COMPUTER }
}

fn is_volume_root(root: &str) -> bool {
    let s = root.trim().replace('\\', "/");
    if let Some(rest) = s.strip_prefix("//?/") {
        // \\?\G:\ is a drive; \\?\UNC\server\share is a network share
        return !rest.starts_with("UNC/") && is_drive_root(rest);
    }
    if s.starts_with("//") {
        return false;
    }
    is_drive_root(&s) || is_mount_point(&s)
}

/// `G:` with nothing after it but separators. `G:foo` is a path relative to the drive's current
/// folder, not its root.
fn is_drive_root(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && s[2..].trim_end_matches('/').is_empty()
}

fn is_mount_point(s: &str) -> bool {
    if !s.starts_with('/') {
        return false;
    }
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.iter().any(|p| *p == "." || *p == "..") {
        return false;
    }
    const BASES: [&[&str]; 4] = [&["Volumes"], &["media"], &["run", "media"], &["mnt"]];
    BASES.iter().any(|base| parts.starts_with(base) && matches!(parts.len() - base.len(), 1 | 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_roots_are_pacemakers_however_they_are_spelled() {
        for r in ["G:\\", "G:", "g:/", "G:\\\\", "  G:\\", "\\\\?\\G:\\", "//?/g:/"] {
            assert_eq!(library_kind(r), PACEMAKER, "{r:?}");
        }
    }

    #[test]
    fn subfolders_of_a_drive_are_this_computer() {
        for r in ["C:\\Users\\me\\Music\\Pacemaker Revival", "G:\\Music", "G:\\Music\\", "g:/Music/x/", "G:foo", "\\\\?\\C:\\Users\\me"] {
            assert_eq!(library_kind(r), COMPUTER, "{r:?}");
        }
    }

    #[test]
    fn network_shares_and_device_paths_are_this_computer() {
        for r in ["\\\\nas\\music", "\\\\nas\\music\\", "//nas/music", "\\\\?\\UNC\\nas\\music", "\\\\?\\UNC\\nas\\music\\lib", "\\\\.\\G:\\"] {
            assert_eq!(library_kind(r), COMPUTER, "{r:?}");
        }
    }

    #[test]
    fn mount_points_are_pacemakers_and_what_is_below_them_is_not() {
        for r in ["/Volumes/PACEMAKER", "/Volumes/PACEMAKER/", "/media/pm", "/media/jo/PACEMAKER", "/run/media/jo/PACEMAKER/", "/run/media/pm", "/mnt/usb", "/mnt/usb//"] {
            assert_eq!(library_kind(r), PACEMAKER, "{r:?}");
        }
        for r in ["/Volumes", "/media", "/mnt", "/run/media", "/Volumes/PACEMAKER/Music/x", "/media/jo/PM/Music/x", "/mnt/a/b/c", "/home/jo/Music/Pacemaker Revival", "/", "/mnt/usb/../x", "/mnt/./usb/x/..", "/volumes/pm", "/mnt2/x"] {
            assert_eq!(library_kind(r), COMPUTER, "{r:?}");
        }
    }

    #[test]
    fn relative_and_empty_paths_are_this_computer() {
        for r in ["", " ", "Music", "Music\\Pacemaker Revival", "./G:", "mnt/usb", "..\\G:\\", "G"] {
            assert_eq!(library_kind(r), COMPUTER, "{r:?}");
        }
    }
}
