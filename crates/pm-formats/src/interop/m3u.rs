//! M3U / M3U8 playlists, plain or extended (`#EXTM3U`, `#EXTINF:<seconds>,<display title>`).
//!
//! Format notes, as the de-facto convention goes (there is no standard): `.m3u8` is UTF-8, a
//! plain `.m3u` is usually the writer's legacy code page; blank lines and unknown `#` lines are
//! ignored; an entry is a path (absolute, or relative to the playlist's own folder), a
//! `file://` URL or a stream URL. `#EXTINF` describes the entry on the next path line.
//! `#PLAYLIST:<name>` (an extension of some writers) names the list; otherwise the file name does.

use std::collections::HashMap;

use super::{bad, location_to_path, Error, ForeignLibrary, ForeignPlaylist, ForeignTrack, Source};

/// Entries beyond this are not read; a real playlist is far smaller.
pub const MAX_ENTRIES: usize = 200_000;
/// A line longer than this is not a path (it counts as an entry that could not be read).
const MAX_LINE_CHARS: usize = 4096;

/// UTF-8 (with or without a byte-order mark) when the bytes are valid UTF-8, otherwise Latin-1
/// (each byte one character), which is what a legacy `.m3u` from a Western writer is.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// `123,Artist - Title` / `123 tvg-id="x",Artist - Title` -> (seconds, display).
fn parse_extinf(rest: &str) -> (Option<f64>, String) {
    // the length ends at the first comma; attributes (if any) sit between the length and it
    let (head, display) = match rest.split_once(',') {
        Some((h, d)) => (h, d.trim()),
        None => (rest, ""),
    };
    let secs = head.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0);
    (secs, display.to_string())
}

/// "Artist - Title" -> (artist, title); without the separator everything is the title.
fn split_display(display: &str) -> (String, String) {
    match display.split_once(" - ") {
        Some((a, t)) if !a.trim().is_empty() && !t.trim().is_empty() => (a.trim().to_string(), t.trim().to_string()),
        _ => (String::new(), display.trim().to_string()),
    }
}

fn is_stream(entry: &str) -> bool {
    let lower = entry.to_ascii_lowercase();
    ["http://", "https://", "mms://", "mmsh://", "rtsp://", "rtmp://", "ftp://", "smb://", "icy://"].iter().any(|p| lower.starts_with(p))
}

/// Joins a relative entry onto the playlist's folder (`base`), resolving `.` and `..` textually.
fn resolve(entry: &str, base: Option<&str>) -> String {
    let e = entry.replace('\\', "/");
    let absolute = e.starts_with('/') || (e.len() >= 2 && e.as_bytes()[0].is_ascii_alphabetic() && e.as_bytes()[1] == b':');
    if absolute {
        // keep the writer's own spelling of an absolute path
        return entry.to_string();
    }
    let Some(base) = base.filter(|b| !b.is_empty()) else { return e };
    let joined = format!("{}/{}", base.replace('\\', "/").trim_end_matches('/'), e);
    // textual normalisation; a leading `//` (UNC base) is kept so it is still refused downstream
    let lead = if joined.starts_with("//") {
        "//"
    } else if joined.starts_with('/') {
        "/"
    } else {
        ""
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != ".." && !p.ends_with(':')) {
                    parts.pop();
                }
            }
            s => parts.push(s),
        }
    }
    format!("{lead}{}", parts.join("/"))
}

/// Parses a playlist. `name` is the fallback list name (the file name without extension) and
/// `base` the playlist's folder, used for relative entries.
pub fn parse(bytes: &[u8], name: &str, base: Option<&str>) -> Result<ForeignLibrary, Error> {
    let text = decode(bytes);
    if text.contains('\0') {
        return Err(bad("this is not a text playlist (it contains NUL bytes)"));
    }
    let mut tracks: Vec<ForeignTrack> = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut list = ForeignPlaylist { name: name.trim().to_string(), ..Default::default() };
    let mut pending: Option<(Option<f64>, String)> = None;
    let mut saw_entry = false;
    let mut saw_header = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            if rest.eq_ignore_ascii_case("EXTM3U") {
                saw_header = true;
            } else if let Some(r) = rest.strip_prefix("EXTINF:") {
                pending = Some(parse_extinf(r));
            } else if let Some(r) = rest.strip_prefix("PLAYLIST:") {
                if !r.trim().is_empty() {
                    list.name = r.trim().chars().take(200).collect();
                }
            }
            continue;
        }
        saw_entry = true;
        let info = pending.take();
        if list.tracks.len() + list.dangling >= MAX_ENTRIES {
            return Err(bad(format!("the playlist has more than {MAX_ENTRIES} entries")));
        }
        if line.chars().count() > MAX_LINE_CHARS || is_stream(line) {
            list.dangling += 1;
            continue;
        }
        let path = if line.len() >= 5 && line.is_char_boundary(5) && line[..5].eq_ignore_ascii_case("file:") {
            location_to_path(line)
        } else {
            resolve(line, base)
        };
        // a path the planner will refuse to probe (network, device, relative) is still listed so
        // the preview can say so; it just never matches by file
        let idx = match by_path.get(&path) {
            Some(&i) => i,
            None => {
                let (secs, display) = info.unwrap_or((None, String::new()));
                let (artist, title) = split_display(&display);
                let stem = path.rsplit(['/', '\\']).next().unwrap_or(&path);
                let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
                let i = tracks.len();
                tracks.push(ForeignTrack {
                    // without an #EXTINF the file name is the only label there is
                    title: if title.is_empty() { stem.to_string() } else { title },
                    artist,
                    duration_secs: secs,
                    path: path.clone(),
                    ..Default::default()
                });
                by_path.insert(path, i);
                i
            }
        };
        list.tracks.push(idx);
    }
    if !saw_header && !saw_entry {
        return Err(bad("the file has no playlist entries"));
    }
    if list.name.is_empty() {
        list.name = "Imported playlist".into();
    }
    Ok(ForeignLibrary { source: Source::M3u, tracks, playlists: vec![list], notes: vec![] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interop::paths;

    #[test]
    fn extended_playlist_with_absolute_relative_and_url_entries() {
        let text = "\u{feff}#EXTM3U\r\n#PLAYLIST:Warm Up\r\n#EXTINF:215,Example Artist - Example Tune\r\nC:\\Music\\a.mp3\r\n\r\n#EXTINF:-1,Stream\r\nhttp://example.invalid/live\r\n#EXTINF:100 tvg-id=\"x\",Solo Title\r\n..\\other\\b.flac\r\nfile:///D:/My%20Music/c.mp3\r\nC:\\Music\\a.mp3\r\n";
        let f = parse(text.as_bytes(), "ignored", Some("C:\\Users\\u\\lists")).unwrap();
        assert_eq!(f.source, Source::M3u);
        assert_eq!(f.playlists.len(), 1);
        let p = &f.playlists[0];
        assert_eq!(p.name, "Warm Up");
        assert_eq!(p.dangling, 1, "the stream is not a file");
        assert_eq!(p.tracks, vec![0, 1, 2, 0], "a repeated path is one track listed twice");
        assert_eq!(f.tracks.len(), 3);
        assert_eq!(
            (f.tracks[0].artist.as_str(), f.tracks[0].title.as_str(), f.tracks[0].duration_secs),
            ("Example Artist", "Example Tune", Some(215.0))
        );
        assert_eq!((f.tracks[1].title.as_str(), f.tracks[1].duration_secs), ("Solo Title", Some(100.0)));
        assert_eq!(f.tracks[1].path, "C:/Users/u/other/b.flac");
        assert_eq!((f.tracks[2].path.as_str(), f.tracks[2].title.as_str()), ("D:/My Music/c.mp3", "c"));
    }

    #[test]
    fn plain_playlist_falls_back_to_the_file_name_and_latin1() {
        let mut bytes = b"/music/caf".to_vec();
        bytes.push(0xE9); // Latin-1 e-acute, not valid UTF-8
        bytes.extend_from_slice(b".mp3\n");
        let f = parse(&bytes, "Friday", None).unwrap();
        assert_eq!(f.playlists[0].name, "Friday");
        assert_eq!(f.tracks[0].path, "/music/caf\u{e9}.mp3");
        assert_eq!(f.tracks[0].title, "caf\u{e9}");
    }

    #[test]
    fn relative_entries_resolve_textually_and_a_unc_base_stays_refusable() {
        assert_eq!(resolve("x/y.mp3", Some("/a/b")), "/a/b/x/y.mp3");
        assert_eq!(resolve("../../../y.mp3", Some("/a")), "/y.mp3");
        assert_eq!(resolve("y.mp3", None), "y.mp3");
        assert!(paths::rejection(&resolve("y.mp3", Some("//host/share/lists"))).is_some());
        assert!(paths::rejection(&resolve("y.mp3", None)).is_some(), "no base: stays relative, never probed");
    }

    #[test]
    fn robust_against_garbage_truncation_and_huge_input() {
        assert!(parse(b"", "x", None).is_err());
        assert!(parse(b"   \n\n", "x", None).is_err());
        assert!(parse(&[0u8; 64], "x", None).is_err(), "binary data is not a playlist");
        // header only is a valid, empty list
        assert_eq!(parse(b"#EXTM3U\n", "x", None).unwrap().playlists[0].tracks.len(), 0);
        // a 1 MB line is an unreadable entry, not an allocation bomb or a track
        let mut long = b"#EXTM3U\n".to_vec();
        long.extend(std::iter::repeat_n(b'a', 1 << 20));
        let f = parse(&long, "x", None).unwrap();
        assert_eq!((f.tracks.len(), f.playlists[0].dangling), (0, 1));
        // EXTINF with nonsense numbers
        let f = parse(b"#EXTINF:NaN,\n#EXTINF:1e999,A - B\n/x/a.mp3\n", "x", None).unwrap();
        assert_eq!(f.tracks[0].duration_secs, None);
        // a truncated multi-byte sequence at the end
        let f = parse(b"/x/a.mp3\n\xE2\x82", "x", None).unwrap();
        assert!(!f.tracks.is_empty());
        // every prefix of a valid file parses or errors, never panics
        let src = "#EXTM3U\n#EXTINF:5,A - B\nC:\\x\\a.mp3\n#EXTINF:6,C\n/y/b.mp3\n".as_bytes();
        for cut in 0..=src.len() {
            let _ = parse(&src[..cut], "x", Some("/base"));
        }
    }

    #[test]
    fn entry_cap_is_enforced() {
        let mut s = String::from("#EXTM3U\n");
        for i in 0..=MAX_ENTRIES {
            s.push_str(&format!("/m/{i}.mp3\n"));
        }
        assert!(parse(s.as_bytes(), "x", None).is_err());
    }
}
