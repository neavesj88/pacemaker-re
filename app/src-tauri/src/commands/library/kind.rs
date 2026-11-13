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
    if pm_formats::library::is_volume_root(root) { PACEMAKER } else { COMPUTER }
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
