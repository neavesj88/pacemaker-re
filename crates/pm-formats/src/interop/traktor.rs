//! Traktor `collection.nml` (Native Instruments Traktor Pro).
//!
//! The NML is XML; its layout is documented only by community notes and by what the files
//! contain. What this reader relies on, and where that comes from:
//!
//! * `NML > COLLECTION > ENTRY` per track, with `TITLE` / `ARTIST` attributes and child elements
//!   `LOCATION` (`VOLUME`, `DIR`, `FILE`), `ALBUM`, `INFO`, `TEMPO`, `CUE_V2`. The file's own path
//!   is `VOLUME` + `DIR` + `FILE`, where `DIR` separates folders with `/:`. This is how the
//!   community scripts that edit collections build their keys (e.g. haileys' gist
//!   <https://gist.github.com/haileys/631221550471a2113fdaa298c22d3660>, which concatenates
//!   exactly those three attributes) and what Native Instruments' forum threads about moving a
//!   collection show.
//! * Playlists: `NML > PLAYLISTS > NODE (TYPE=FOLDER|PLAYLIST, NAME) > SUBNODES > NODE ...`; a
//!   `TYPE=PLAYLIST` node holds `PLAYLIST > ENTRY > PRIMARYKEY (TYPE=TRACK, KEY=<the track's
//!   VOLUME+DIR+FILE>)` (same gist).
//! * UNVERIFIED (from memory of real files, no sample was at hand): `INFO RANKING` is 0..255 with
//!   51 per star; `INFO PLAYTIME` is seconds (`PLAYTIME_FLOAT` more precisely); `CUE_V2 TYPE="0"`
//!   is a plain cue and `START` its position in milliseconds; `INFO FILESIZE` is in kilobytes,
//!   which is why it is not used as a byte size for matching. Newer files may put the drive
//!   letter inside `DIR` and leave `VOLUME` empty (forum reports); both spellings are handled.
//!
//! Colours: `INFO COLOR` is read as a bare index (a positive whole number; absent, 0 or junk is "no
//! colour"). What each index is called in Traktor is UNVERIFIED (no sample file or NI document at
//! hand), so this reader does not name colours; the importer maps indexes to colour labels with a
//! table the user can change.

use std::collections::HashMap;

use super::{bad, build_node, Error, ForeignLibrary, ForeignPlaylist, ForeignTrack, Node, Reader, Source};

/// Traktor's own folders in the playlist tree that are not the user's playlists.
const INTERNAL_NODES: &[&str] = &["_LOOPS", "_RECORDINGS"];

/// `/:Users/:me/:Music/:` -> `/Users/me/Music/`.
fn dir_to_path(dir: &str) -> String {
    let d = dir.replace("/:", "/");
    // `/C:/Users/...` (drive letter inside DIR) -> `C:/Users/...`
    let b = d.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        return d[1..].to_string();
    }
    d
}

/// The file's path: a drive-letter `VOLUME` (`C:`) is prefixed; any other volume name (macOS disk
/// names) is dropped, because the file is then found by name and size wherever it is mounted.
fn location_path(volume: &str, dir: &str, file: &str) -> String {
    if file.is_empty() {
        return String::new();
    }
    let vb = volume.as_bytes();
    let drive = vb.len() == 2 && vb[0].is_ascii_alphabetic() && vb[1] == b':';
    format!("{}{}{file}", if drive { volume } else { "" }, dir_to_path(dir))
}

fn num<T: std::str::FromStr>(s: Option<&str>) -> Option<T> {
    s.and_then(|v| v.trim().parse().ok())
}

fn walk_playlists(node: &Node, trail: &mut Vec<String>, by_key: &HashMap<String, usize>, out: &mut Vec<ForeignPlaylist>) {
    for n in node.children.iter().filter(|c| c.name == "NODE") {
        let name = n.attr("NAME").unwrap_or("").trim().to_string();
        if INTERNAL_NODES.contains(&name.as_str()) {
            continue;
        }
        match n.attr("TYPE") {
            Some("PLAYLIST") => {
                let mut pl = ForeignPlaylist { name: trail.iter().cloned().chain([name]).collect::<Vec<_>>().join(" / "), ..Default::default() };
                for e in n.child("PLAYLIST").map(|p| p.children.as_slice()).unwrap_or(&[]).iter().filter(|e| e.name == "ENTRY") {
                    match e.child("PRIMARYKEY").filter(|k| k.attr("TYPE").is_none_or(|t| t == "TRACK")).and_then(|k| k.attr("KEY")) {
                        Some(key) => match by_key.get(key) {
                            Some(i) => pl.tracks.push(*i),
                            None => pl.dangling += 1,
                        },
                        None => pl.dangling += 1,
                    }
                }
                out.push(pl);
            }
            Some("FOLDER") | None => {
                // "$ROOT" is the tree's anchor, not a folder name
                let pushed = !name.is_empty() && name != "$ROOT";
                if pushed {
                    trail.push(name);
                }
                if let Some(sub) = n.child("SUBNODES") {
                    walk_playlists(sub, trail, by_key, out);
                }
                if pushed {
                    trail.pop();
                }
            }
            // SMARTLIST and anything newer: a rule set, not a list of tracks
            _ => {}
        }
    }
}

/// Parses a Traktor collection.
pub fn parse(text: &str) -> Result<ForeignLibrary, Error> {
    let mut r = Reader::new(text);
    let (name, attrs) = r.next_start()?.ok_or_else(|| bad("the file has no root element"))?;
    if name != "NML" {
        return Err(bad("not a Traktor collection (no NML element)"));
    }
    let root = build_node(&mut r, name, attrs, 0)?;

    let mut tracks = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    if let Some(col) = root.child("COLLECTION") {
        for e in col.children.iter().filter(|c| c.name == "ENTRY") {
            let loc = e.child("LOCATION");
            let (vol, dir, file) = (
                loc.and_then(|l| l.attr("VOLUME")).unwrap_or(""),
                loc.and_then(|l| l.attr("DIR")).unwrap_or(""),
                loc.and_then(|l| l.attr("FILE")).unwrap_or(""),
            );
            let info = e.child("INFO");
            let stars = num::<f64>(info.and_then(|i| i.attr("RANKING"))).map(|v| (v / 51.0).round().clamp(0.0, 5.0) as u8).filter(|s| *s > 0);
            let duration = num::<f64>(info.and_then(|i| i.attr("PLAYTIME_FLOAT")))
                .filter(|d| *d > 0.0)
                .or_else(|| num::<f64>(info.and_then(|i| i.attr("PLAYTIME"))).filter(|d| *d > 0.0));
            // the first plain cue (type 0) by position; START is in milliseconds
            let cue = e
                .children
                .iter()
                .filter(|c| c.name == "CUE_V2" && c.attr("TYPE") == Some("0"))
                .filter_map(|c| num::<f64>(c.attr("START")))
                .filter(|ms| ms.is_finite() && *ms >= 0.0)
                .min_by(|a, b| a.total_cmp(b))
                .map(|ms| ms / 1000.0);
            let t = ForeignTrack {
                title: e.attr("TITLE").unwrap_or("").to_string(),
                artist: e.attr("ARTIST").unwrap_or("").to_string(),
                album: e.child("ALBUM").and_then(|a| a.attr("TITLE")).unwrap_or("").to_string(),
                genre: info.and_then(|i| i.attr("GENRE")).unwrap_or("").to_string(),
                key: info.and_then(|i| i.attr("KEY")).unwrap_or("").to_string(),
                bpm: num::<f64>(e.child("TEMPO").and_then(|t| t.attr("BPM"))).filter(|b| *b > 0.0),
                stars,
                path: location_path(vol, dir, file),
                size: None,
                duration_secs: duration,
                cue_secs: cue,
                colour: num::<u32>(info.and_then(|i| i.attr("COLOR"))).filter(|c| *c > 0),
            };
            if !file.is_empty() {
                // playlists key a track by the raw concatenation, not by the decoded path
                by_key.insert(format!("{vol}{dir}{file}"), tracks.len());
            }
            tracks.push(t);
        }
    }

    let mut playlists = Vec::new();
    if let Some(pl) = root.child("PLAYLISTS") {
        walk_playlists(pl, &mut Vec::new(), &by_key, &mut playlists);
    }
    Ok(ForeignLibrary { source: Source::Traktor, tracks, playlists, notes: vec![] })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<NML VERSION="19"><HEAD COMPANY="www.native-instruments.com" PROGRAM="Traktor"></HEAD>
<COLLECTION ENTRIES="4">
<ENTRY MODIFIED_DATE="2020/1/1" TITLE="Synth Title 0" ARTIST="Synth Artist">
<LOCATION DIR="/:Music/:Sub Folder/:" FILE="synth 0.mp3" VOLUME="C:" VOLUMEID="abc"></LOCATION>
<ALBUM TRACK="1" TITLE="An Album"></ALBUM>
<INFO BITRATE="320000" GENRE="House" KEY="4m" PLAYTIME="200" PLAYTIME_FLOAT="200.4" RANKING="153" COLOR="2" FILESIZE="7800"></INFO>
<TEMPO BPM="120.000000" BPM_QUALITY="100"></TEMPO>
<CUE_V2 NAME="AutoGrid" DISPL_ORDER="0" TYPE="4" LEN="0" START="500.0" REPEATS="-1" HOTCUE="0"></CUE_V2>
<CUE_V2 NAME="B" DISPL_ORDER="0" TYPE="0" LEN="0" START="31500.5" REPEATS="-1" HOTCUE="1"></CUE_V2>
<CUE_V2 NAME="A" DISPL_ORDER="0" TYPE="0" LEN="0" START="12250.0" REPEATS="-1" HOTCUE="-1"></CUE_V2>
</ENTRY>
<ENTRY TITLE="Synth Title 1" ARTIST="Synth Artist"><LOCATION DIR="/:C:/:Elsewhere/:" FILE="x &amp; y.mp3" VOLUME=""></LOCATION><INFO RANKING="0" PLAYTIME="201"></INFO></ENTRY>
<ENTRY TITLE="No location"><INFO RANKING="255"></INFO></ENTRY>
<ENTRY TITLE="Five" ARTIST="Z"><LOCATION DIR="/:m/:" FILE="five.mp3" VOLUME="C:"></LOCATION><INFO RANKING="255"></INFO></ENTRY>
</COLLECTION>
<PLAYLISTS>
<NODE TYPE="FOLDER" NAME="$ROOT"><SUBNODES COUNT="3">
<NODE TYPE="PLAYLIST" NAME="Warm up"><PLAYLIST ENTRIES="3" TYPE="LIST" UUID="u1">
<ENTRY><PRIMARYKEY TYPE="TRACK" KEY="C:/:Music/:Sub Folder/:synth 0.mp3"></PRIMARYKEY></ENTRY>
<ENTRY><PRIMARYKEY TYPE="TRACK" KEY="/:C:/:Elsewhere/:x &amp; y.mp3"></PRIMARYKEY></ENTRY>
<ENTRY><PRIMARYKEY TYPE="TRACK" KEY="C:/:gone/:nothing.mp3"></PRIMARYKEY></ENTRY>
</PLAYLIST></NODE>
<NODE TYPE="FOLDER" NAME="Sets"><SUBNODES COUNT="1"><NODE TYPE="PLAYLIST" NAME="Friday"><PLAYLIST ENTRIES="1" TYPE="LIST"><ENTRY><PRIMARYKEY TYPE="TRACK" KEY="C:/:m/:five.mp3"></PRIMARYKEY></ENTRY></PLAYLIST></NODE></SUBNODES></NODE>
<NODE TYPE="PLAYLIST" NAME="_LOOPS"><PLAYLIST ENTRIES="0" TYPE="LIST"></PLAYLIST></NODE>
<NODE TYPE="SMARTLIST" NAME="Smart"><SMARTLIST UUID="s"></SMARTLIST></NODE>
</SUBNODES></NODE></PLAYLISTS></NML>"#;

    #[test]
    fn parses_tracks_paths_ratings_cues_and_playlists() {
        let f = parse(NML).unwrap();
        assert_eq!(f.source, Source::Traktor);
        assert_eq!(f.tracks.len(), 4);
        let t = &f.tracks[0];
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str(), t.genre.as_str(), t.key.as_str()), ("Synth Title 0", "Synth Artist", "An Album", "House", "4m"));
        assert_eq!(t.path, "C:/Music/Sub Folder/synth 0.mp3");
        assert_eq!((t.stars, t.duration_secs, t.bpm, t.size), (Some(3), Some(200.4), Some(120.0), None));
        assert_eq!(t.cue_secs, Some(12.25), "the earliest type-0 cue, in seconds; grid markers are not cues");
        assert_eq!(f.tracks[1].path, "C:/Elsewhere/x & y.mp3", "drive letter inside DIR");
        assert_eq!(f.tracks[1].stars, None);
        assert_eq!((t.colour, f.tracks[1].colour, f.tracks[2].colour), (Some(2), None, None), "COLOR is read as an index; none where absent");
        assert_eq!(f.tracks[2].path, "", "no LOCATION");
        assert_eq!(f.tracks[3].stars, Some(5));
        assert_eq!(f.playlists.len(), 2, "internal and smart lists are not playlists");
        assert_eq!((f.playlists[0].name.as_str(), f.playlists[0].tracks.clone(), f.playlists[0].dangling), ("Warm up", vec![0, 1], 1));
        assert_eq!((f.playlists[1].name.as_str(), f.playlists[1].tracks.clone()), ("Sets / Friday", vec![3]));
    }

    #[test]
    fn rejects_other_documents_and_survives_damage() {
        assert!(parse("").is_err());
        assert!(parse("<DJ_PLAYLISTS/>").is_err());
        assert!(parse("<NML><COLLECTION><ENTRY></NML>").is_err());
        assert!(parse("<NML/>").unwrap().tracks.is_empty());
        // nonsense numbers and an empty entry do not break the entry
        let f = parse(r#"<NML><COLLECTION><ENTRY><INFO RANKING="x" PLAYTIME="-5"/><TEMPO BPM="NaN"/><CUE_V2 TYPE="0" START="inf"/></ENTRY></COLLECTION></NML>"#).unwrap();
        assert_eq!((f.tracks[0].stars, f.tracks[0].duration_secs, f.tracks[0].bpm, f.tracks[0].cue_secs), (None, None, None, None));
        for junk in ["x", "-1", "0", "2.5", "", "99999999999"] {
            let f = parse(&format!(r#"<NML><COLLECTION><ENTRY><INFO COLOR="{junk}"/></ENTRY></COLLECTION></NML>"#)).unwrap();
            assert_eq!(f.tracks[0].colour, None, "{junk:?}");
        }
        // deeply nested folders hit the depth guard, not the stack
        let deep = format!("<NML>{}{}</NML>", "<NODE>".repeat(5000), "</NODE>".repeat(5000));
        assert!(parse(&deep).is_err());
        // every prefix of a valid file is Ok or Err, never a panic
        for cut in (0..NML.len()).step_by(7) {
            if NML.is_char_boundary(cut) {
                let _ = parse(&NML[..cut]);
            }
        }
        // a huge attribute list or a million empty entries stay linear and bounded by the input
        let many = format!("<NML><COLLECTION>{}</COLLECTION></NML>", "<ENTRY/>".repeat(100_000));
        assert_eq!(parse(&many).unwrap().tracks.len(), 100_000);
    }

    #[test]
    fn dir_conversion() {
        assert_eq!(dir_to_path("/:Users/:me/:Music/:"), "/Users/me/Music/");
        assert_eq!(dir_to_path("/:C:/:x/:"), "C:/x/");
        assert_eq!(location_path("C:", "/:a/:", "b.mp3"), "C:/a/b.mp3");
        assert_eq!(location_path("Macintosh HD", "/:Users/:me/:", "b.mp3"), "/Users/me/b.mp3");
        assert_eq!(location_path("C:", "/:a/:", ""), "");
    }
}
