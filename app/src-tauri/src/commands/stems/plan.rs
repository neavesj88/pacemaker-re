//! What a stem run will do, worked out in one place: the preview the user confirms and the job
//! that runs both call [`build_plan`], so they cannot disagree. Pure (reads the library and the
//! destination folder, writes nothing), so it is unit-tested without Tauri.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pm_formats::library::{Library, Track};
use serde::Serialize;

use super::settings::StemSettings;

/// What happens to one track.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Both result files are written (none exists yet).
    Make,
    /// At least one result file exists and the settings say replace: needs the user's confirmation.
    Replace,
    /// Only the missing result file is written; the other already exists and is kept.
    Partial,
    /// Both result files exist and are kept; nothing is done.
    Skip,
    /// Cannot be done; `problem` says why.
    Refuse,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PlanItem {
    pub track_id: i64,
    pub label: String,
    pub minutes: f64,
    /// The track's audio file on the card (read only).
    pub source: String,
    pub vocals_path: String,
    pub instrumental_path: String,
    pub vocals_exists: bool,
    pub instrumental_exists: bool,
    pub write_vocals: bool,
    pub write_instrumental: bool,
    pub action: Action,
    /// The consequence in words, shown beside the row.
    pub what: String,
    pub problem: Option<String>,
    /// Model calls this track needs (an estimate when the track's length is unknown).
    pub segments: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Plan {
    pub dest: String,
    pub format_label: String,
    pub items: Vec<PlanItem>,
    /// Counts by action.
    pub make: usize,
    pub replace: usize,
    pub partial: usize,
    pub skip: usize,
    pub refuse: usize,
    /// Minutes of audio that will actually be separated.
    pub minutes: f64,
    /// Existing files that would be overwritten, by full path: the confirmation lists each.
    pub replaced_files: Vec<String>,
    /// Fingerprint of everything the confirmation showed (the items with their paths and what is
    /// there now, the replaced files). `stems_run` refuses a plan with another digest, so what is
    /// replaced is what the user confirmed.
    pub digest: String,
}

impl Plan {
    /// Items that run the model.
    pub fn runs(&self) -> impl Iterator<Item = &PlanItem> {
        self.items.iter().filter(|i| matches!(i.action, Action::Make | Action::Replace | Action::Partial))
    }
}

/// The audio file for a stored `location`, which must lie inside the volume's music folder: a
/// location that is absolute, has a drive prefix or climbs with `..` is refused. The rule is
/// `analysis::audio_path`'s, so a change to it lands in one place.
pub fn audio_in_volume(lib: &Library, location: &str) -> Result<PathBuf, String> {
    let audio = crate::commands::analysis::audio_path(lib.root(), location).map_err(|_| "The track's location points outside the device.".to_string())?;
    if !audio.is_file() {
        return Err("The audio file is missing from the device.".into());
    }
    Ok(audio)
}

const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];

/// A file-name-safe version of `s`: no path separators or characters Windows forbids, no control
/// characters, no trailing dots or spaces, not a reserved device name, at most 100 characters.
pub fn safe_name(s: &str) -> String {
    let mut out = String::new();
    let mut last_space = true;
    for c in s.chars() {
        let c = if c.is_control() || ['\\', '/', ':', '*', '?', '"', '<', '>', '|'].contains(&c) { '_' } else { c };
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    let mut out: String = out.trim().chars().take(100).collect();
    while out.ends_with(['.', ' ']) {
        out.pop();
    }
    let stem = out.split('.').next().unwrap_or("");
    if out.is_empty() {
        return String::new();
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        out.insert(0, '_');
    }
    out
}

/// "Artist - Title", or the title, or the artist; else the device file's name.
pub fn display_label(t: &Track) -> String {
    match (t.artist.trim().is_empty(), t.title.trim().is_empty()) {
        (false, false) => format!("{} - {}", t.artist.trim(), t.title.trim()),
        (true, false) => t.title.trim().to_string(),
        (false, true) => t.artist.trim().to_string(),
        _ => t.location.rsplit('/').next().unwrap_or("track").to_string(),
    }
}

/// Segments the model will be called for on a track of `secs` seconds (4 minutes when unknown).
pub fn segments_for(secs: i64, overlap: f32) -> usize {
    let frames = if secs > 0 { secs as usize * pm_stems::RATE as usize } else { 240 * pm_stems::RATE as usize };
    pm_stems::overlap::Plan::new(pm_stems::htdemucs::SEGMENT, overlap, frames).segments()
}

fn same_or_inside(dest: &Path, root: &Path) -> bool {
    match (std::fs::canonicalize(dest), std::fs::canonicalize(root)) {
        (Ok(d), Ok(r)) => d.starts_with(&r),
        _ => false,
    }
}

/// Builds the plan for `track_ids` (in the order given, repeats dropped) into `dest`.
pub fn build_plan(lib: &Library, track_ids: &[i64], dest: &Path, s: &StemSettings) -> Result<Plan, String> {
    if track_ids.is_empty() {
        return Err("Select at least one track.".into());
    }
    if !dest.is_dir() {
        return Err(format!("{} is not a folder that exists.", dest.display()));
    }
    if same_or_inside(dest, lib.root()) {
        return Err("Choose a folder on this computer, not on the device. To put the results on the card, make them here first and then add them (the same step as adding any audio files).".into());
    }
    let tracks: HashMap<i64, Track> = lib.tracks().map_err(|e| e.to_string())?.into_iter().map(|t| (t.track_id, t)).collect();
    let ext = s.out_format().extension();
    let mut seen_ids = HashSet::new();
    let mut claimed: HashMap<String, String> = HashMap::new();
    let mut items = Vec::new();

    for &id in track_ids {
        if !seen_ids.insert(id) {
            continue;
        }
        let Some(t) = tracks.get(&id) else {
            items.push(refused(id, format!("track {id}"), "That track is not in the library.".into()));
            continue;
        };
        let label = display_label(t);
        let source = match audio_in_volume(lib, &t.location) {
            Ok(p) => p,
            Err(e) => {
                items.push(refused(id, label, e));
                continue;
            }
        };
        let minutes = t.play_time_secs as f64 / 60.0;
        let mut base = safe_name(&label);
        if base.is_empty() {
            base = format!("track {id}");
        }
        let vpath = dest.join(format!("{base} {}.{ext}", s.vocals_suffix));
        let ipath = dest.join(format!("{base} {}.{ext}", s.instrumental_suffix));
        let mut problem = None;
        if s.max_minutes > 0 && t.play_time_secs > i64::from(s.max_minutes) * 60 {
            problem = Some(format!("{:.0} minutes is longer than the {}-minute limit in the settings (the whole track is held in memory while it is separated).", minutes, s.max_minutes));
        }
        for p in [&vpath, &ipath] {
            let key = p.to_string_lossy().to_lowercase();
            if let Some(other) = claimed.get(&key) {
                problem.get_or_insert_with(|| format!("{other} would be written to the same file name; rename one of the two tracks first."));
            }
        }
        if problem.is_none() {
            for p in [&vpath, &ipath] {
                claimed.insert(p.to_string_lossy().to_lowercase(), label.clone());
            }
        }
        let (ve, ie) = (vpath.exists(), ipath.exists());
        let mut item = PlanItem {
            track_id: id,
            label,
            minutes,
            source: source.to_string_lossy().into_owned(),
            vocals_path: vpath.to_string_lossy().into_owned(),
            instrumental_path: ipath.to_string_lossy().into_owned(),
            vocals_exists: ve,
            instrumental_exists: ie,
            write_vocals: true,
            write_instrumental: true,
            action: Action::Make,
            what: String::new(),
            problem: problem.clone(),
            segments: segments_for(t.play_time_secs, s.overlap()),
        };
        if let Some(p) = problem {
            item.action = Action::Refuse;
            item.write_vocals = false;
            item.write_instrumental = false;
            item.what = format!("Not done: {p}");
        } else {
            decide(&mut item, s);
        }
        items.push(item);
    }

    let count = |a: Action| items.iter().filter(|i| i.action == a).count();
    let replaced_files: Vec<String> = items
        .iter()
        .filter(|i| i.action == Action::Replace)
        .flat_map(|i| {
            let mut v = Vec::new();
            if i.vocals_exists && i.write_vocals {
                v.push(i.vocals_path.clone());
            }
            if i.instrumental_exists && i.write_instrumental {
                v.push(i.instrumental_path.clone());
            }
            v
        })
        .collect();
    let minutes = items.iter().filter(|i| matches!(i.action, Action::Make | Action::Replace | Action::Partial)).map(|i| i.minutes).sum();
    let digest = format!("{:016x}", crate::store::fnv1a(&serde_json::to_string(&(&items, &replaced_files, dest)).map_err(|e| e.to_string())?));
    Ok(Plan {
        digest,
        dest: dest.to_string_lossy().into_owned(),
        format_label: format_label(s),
        make: count(Action::Make),
        replace: count(Action::Replace),
        partial: count(Action::Partial),
        skip: count(Action::Skip),
        refuse: count(Action::Refuse),
        minutes,
        replaced_files,
        items,
    })
}

fn refused(id: i64, label: String, problem: String) -> PlanItem {
    PlanItem {
        track_id: id,
        label,
        minutes: 0.0,
        source: String::new(),
        vocals_path: String::new(),
        instrumental_path: String::new(),
        vocals_exists: false,
        instrumental_exists: false,
        write_vocals: false,
        write_instrumental: false,
        action: Action::Refuse,
        what: format!("Not done: {problem}"),
        problem: Some(problem),
        segments: 0,
    }
}

/// Sets the action and wording of an item whose own checks passed, from which result files exist.
fn decide(item: &mut PlanItem, s: &StemSettings) {
    let (ve, ie) = (item.vocals_exists, item.instrumental_exists);
    if s.existing == "replace" {
        match (ve, ie) {
            (false, false) => {
                item.action = Action::Make;
                item.what = "Writes the acapella and the instrumental.".into();
            }
            _ => {
                item.action = Action::Replace;
                let n = ve as usize + ie as usize;
                item.what = format!("Writes both files and REPLACES {} existing file{}.", n, if n == 1 { "" } else { "s" });
            }
        }
        return;
    }
    match (ve, ie) {
        (false, false) => {
            item.action = Action::Make;
            item.what = "Writes the acapella and the instrumental.".into();
        }
        (true, true) => {
            item.action = Action::Skip;
            item.write_vocals = false;
            item.write_instrumental = false;
            item.what = "Skipped: both files already exist and are kept.".into();
        }
        (true, false) => {
            item.action = Action::Partial;
            item.write_vocals = false;
            item.what = "Writes the instrumental only; the acapella already exists and is kept.".into();
        }
        (false, true) => {
            item.action = Action::Partial;
            item.write_instrumental = false;
            item.what = "Writes the acapella only; the instrumental already exists and is kept.".into();
        }
    }
}

pub fn format_label(s: &StemSettings) -> String {
    match s.format.as_str() {
        "ogg" => format!("Ogg Vorbis, quality {:.1}", s.ogg_quality),
        "wav" => "WAV, 16-bit".to_string(),
        _ => format!("MP3, {} kbps", s.mp3_kbps),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::strfile::{Analysis, Beat};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-stems-plan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A library with `metas` as (artist, title, seconds); returns (volume, ids).
    fn library(tag: &str, metas: &[(&str, &str, i64)]) -> (PathBuf, Vec<i64>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src = temp_dir(&format!("{tag}-src"));
        let analysis = Analysis { normalize_factor: 1.0, beat_start_offset: 0, beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(), first_downbeat: Some(0) };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, (artist, title, secs)) in metas.iter().enumerate() {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![i as u8; 2000 + i]).unwrap();
            let meta = TrackMeta { title: title.to_string(), artist: artist.to_string(), format: "MP3".into(), play_time_secs: *secs, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        std::fs::remove_dir_all(&src).unwrap();
        (vol, ids)
    }

    #[test]
    fn safe_names_cannot_escape_or_be_reserved() {
        assert_eq!(safe_name("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(safe_name("  a   b  "), "a b");
        assert_eq!(safe_name("name. "), "name");
        assert_eq!(safe_name("con"), "_con");
        assert_eq!(safe_name("Nul.txt"), "_Nul.txt");
        assert_eq!(safe_name("../../x"), ".._.._x");
        assert_eq!(safe_name("...."), "");
        assert_eq!(safe_name(&"x".repeat(300)).len(), 100);
        assert!(!safe_name("a\u{0}b\tc").contains(['\0', '\t']));
    }

    #[test]
    fn names_use_the_label_suffixes_and_format_extension() {
        let (vol, ids) = library("names", &[("Demo Artist", "Demo Tune", 200), ("", "Only Title", 100), ("Only Artist", "", 100)]);
        let dest = temp_dir("names-dest");
        let lib = Library::open_read_only(&vol).unwrap();
        let p = build_plan(&lib, &ids, &dest, &StemSettings::default()).unwrap();
        assert_eq!(p.items[0].vocals_path, dest.join("Demo Artist - Demo Tune (Acapella).mp3").to_string_lossy());
        assert_eq!(p.items[0].instrumental_path, dest.join("Demo Artist - Demo Tune (Instrumental).mp3").to_string_lossy());
        assert!(p.items[1].vocals_path.ends_with("Only Title (Acapella).mp3"));
        assert!(p.items[2].vocals_path.ends_with("Only Artist (Acapella).mp3"));
        assert_eq!((p.make, p.replace, p.skip, p.refuse, p.partial), (3, 0, 0, 0, 0));
        assert!(p.items.iter().all(|i| i.what.starts_with("Writes the acapella and the instrumental")));
        assert!((p.minutes - (200.0 + 100.0 + 100.0) / 60.0).abs() < 1e-9);

        let wav = StemSettings { format: "wav".into(), vocals_suffix: "(Vocals only)".into(), ..Default::default() };
        let p = build_plan(&lib, &ids[..1], &dest, &wav).unwrap();
        assert!(p.items[0].vocals_path.ends_with("Demo Artist - Demo Tune (Vocals only).wav"));
        assert_eq!(p.format_label, "WAV, 16-bit");
    }

    #[test]
    fn existing_files_are_skipped_by_default_and_replacing_is_named() {
        let (vol, ids) = library("exist", &[("A", "One", 100), ("B", "Two", 100), ("C", "Three", 100)]);
        let dest = temp_dir("exist-dest");
        let lib = Library::open_read_only(&vol).unwrap();
        let base = build_plan(&lib, &ids, &dest, &StemSettings::default()).unwrap();
        std::fs::write(&base.items[0].vocals_path, b"x").unwrap();
        std::fs::write(&base.items[0].instrumental_path, b"x").unwrap();
        std::fs::write(&base.items[1].vocals_path, b"x").unwrap();

        let p = build_plan(&lib, &ids, &dest, &StemSettings::default()).unwrap();
        assert_eq!((p.items[0].action, p.items[1].action, p.items[2].action), (Action::Skip, Action::Partial, Action::Make));
        assert!(p.items[0].what.contains("both files already exist"));
        assert!(!p.items[1].write_vocals && p.items[1].write_instrumental);
        assert!(p.items[1].what.contains("instrumental only"));
        assert_eq!((p.skip, p.partial, p.make, p.replace), (1, 1, 1, 0));
        assert!(p.replaced_files.is_empty());
        assert_eq!(p.runs().count(), 2);

        let rep = StemSettings { existing: "replace".into(), ..Default::default() };
        let p = build_plan(&lib, &ids, &dest, &rep).unwrap();
        assert_eq!((p.items[0].action, p.items[1].action, p.items[2].action), (Action::Replace, Action::Replace, Action::Make));
        assert!(p.items[0].what.contains("REPLACES 2 existing files") && p.items[1].what.contains("REPLACES 1 existing file."));
        assert_eq!(p.replaced_files.len(), 3, "the confirmation can list every file by path");
        assert!(p.replaced_files.contains(&p.items[1].vocals_path) && !p.replaced_files.contains(&p.items[1].instrumental_path));

        // the digest follows what is on disk: a file that appears or goes changes it
        let before = p.digest.clone();
        assert_eq!(before, build_plan(&lib, &ids, &dest, &rep).unwrap().digest, "the same plan has the same digest");
        std::fs::write(&p.items[2].vocals_path, b"x").unwrap();
        assert_ne!(before, build_plan(&lib, &ids, &dest, &rep).unwrap().digest);
    }

    #[test]
    fn problems_refuse_only_the_affected_track() {
        let (vol, ids) = library("prob", &[("A", "Same", 100), ("A", "Same", 120), ("B", "Long", 31 * 60), ("C", "Fine", 100)]);
        let dest = temp_dir("prob-dest");
        let lib = Library::open_read_only(&vol).unwrap();
        let mut ask = ids.clone();
        ask.push(ids[3]); // a repeat is dropped
        ask.push(9999); // unknown
        let p = build_plan(&lib, &ask, &dest, &StemSettings::default()).unwrap();
        assert_eq!(p.items.len(), 5);
        // two tracks with the same label would write the same files: the second is refused
        assert_eq!(p.items[0].action, Action::Make);
        assert_eq!(p.items[1].action, Action::Refuse);
        assert!(p.items[1].problem.as_ref().unwrap().contains("same file name"));
        assert!(p.items[2].problem.as_ref().unwrap().contains("31 minutes is longer than the 30-minute limit"));
        assert_eq!(p.items[3].action, Action::Make);
        assert!(p.items[4].problem.as_ref().unwrap().contains("not in the library"));
        assert_eq!((p.make, p.refuse), (2, 3));
        // no limit setting lets the long one through
        let unlimited = StemSettings { max_minutes: 0, ..Default::default() };
        assert_eq!(build_plan(&lib, &ids[2..3], &dest, &unlimited).unwrap().items[0].action, Action::Make);
    }

    #[test]
    fn the_destination_must_be_a_real_folder_outside_the_device() {
        let (vol, ids) = library("dest", &[("A", "One", 100)]);
        let lib = Library::open_read_only(&vol).unwrap();
        let s = StemSettings::default();
        assert!(build_plan(&lib, &ids, &vol, &s).unwrap_err().contains("not on the device"));
        assert!(build_plan(&lib, &ids, &vol.join(".Pacemaker").join("Music"), &s).unwrap_err().contains("not on the device"));
        std::fs::create_dir_all(vol.join("sub")).unwrap();
        assert!(build_plan(&lib, &ids, &vol.join("sub"), &s).unwrap_err().contains("not on the device"));
        assert!(build_plan(&lib, &ids, &vol.join("missing-folder"), &s).unwrap_err().contains("not a folder that exists"));
        assert!(build_plan(&lib, &[], &temp_dir("dest-ok"), &s).unwrap_err().contains("at least one"));
    }

    #[test]
    fn a_location_that_escapes_the_music_folder_is_refused() {
        let (vol, _) = library("esc", &[("A", "One", 100)]);
        let lib = Library::open_read_only(&vol).unwrap();
        assert!(audio_in_volume(&lib, "/pmdb_tracks//etc/x").is_err());
        assert!(audio_in_volume(&lib, "/pmdb_tracks/../outside.mp3").is_err());
        assert!(audio_in_volume(&lib, "/pmdb_tracks/a/../../outside.mp3").is_err());
        assert!(audio_in_volume(&lib, "/elsewhere/x.mp3").is_err());
        #[cfg(windows)]
        assert!(audio_in_volume(&lib, "/pmdb_tracks/C:\\x\\y.mp3").is_err());
        assert!(audio_in_volume(&lib, "/pmdb_tracks/a/missing.mp3").unwrap_err().contains("missing"));
    }

    #[test]
    fn segment_estimates_follow_the_overlap_setting() {
        let a = segments_for(240, 0.5);
        let b = segments_for(240, 0.25);
        assert!(a > b && b > 0, "more overlap means more model calls: {a} vs {b}");
        assert_eq!(segments_for(0, 0.5), a, "an unknown length assumes four minutes");
        assert_eq!(segments_for(1, 0.5), 1);
    }
}
