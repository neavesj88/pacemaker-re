//! Export several cases (and smart cases) to a folder: one playlist file per case in the chosen
//! format, optionally with the audio files copied beside them and the M3U8 pointing at the
//! copies by relative path.
//!
//! Three parts, one code path:
//!
//! * [`build`] decides everything: which lists, their file names, which audio files are copied
//!   and where, and what happens where a file already exists. The preview (`cases_export_plan`)
//!   shows exactly this, and the job ([`execute`]) is run from the same function, so the dialog
//!   cannot promise something the export then does differently.
//! * [`execute`] does it: audio copies first, then the playlists (so a playlist never points at
//!   a copy that was not made yet), checking the cancel flag between files and inside a copy.
//! * The text of each list comes from the converters in `pm_formats::interop` (the same ones
//!   the Import view and the single-case export use); this file only chooses the audio paths
//!   they write.
//!
//! Nothing here writes to the device, and the destination folder may not be inside it. A file
//! that already exists is never changed unless the user chose "replace" in the dialog, which
//! listed every file it would replace; a file that appears between the preview and the write is
//! left alone. Audio is read through [`audio_source`], which refuses a location that would
//! leave the music folder.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pm_formats::interop::{self, ExtraPlaylist, PathMap, XmlOptions};
use pm_formats::library::{Error, Library, Track};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

use super::export::{extension, left_out_escaping, plain_location, write_out};
use super::export_prefs::{Conflict, Layout, NameBy};
use super::smart::{self, SmartCase};
use super::smart_export::evaluate_all;

/// The `job` name in `job-progress` / `job-done` events.
pub const JOB: &str = "cases-export";

/// Folder (beside the playlists) that holds the copied audio in the shared layout.
pub const AUDIO_DIR: &str = "Audio";

/// Set by `cases_export_cancel`, polled between files and between blocks of a copy.
#[derive(Default)]
pub struct Cancel(pub AtomicBool);

/// What the dialog asks for.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub folder: String,
    /// "m3u8", "csv" or "rekordbox".
    pub format: String,
    pub case_ids: Vec<i64>,
    pub smart_ids: Vec<u32>,
    pub relative_paths: bool,
    pub copy_audio: bool,
    pub audio_layout: Layout,
    pub audio_names: NameBy,
    pub conflict: Conflict,
}

/// What will happen to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// No file there yet: it is written.
    Write,
    /// A file is there and will be written over (the user chose "replace").
    Replace,
    /// A file is there and is kept; this one is written under a numbered name.
    Renamed,
    /// A file is there and is kept; this one is not written.
    Skip,
    /// An audio copy that is already there with the same size: left as it is.
    Same,
    /// Audio whose file is not on the device: nothing to copy.
    Missing,
    /// Something that is not a file is in the way (a folder of that name).
    Blocked,
}

impl Action {
    fn writes(self) -> bool {
        matches!(self, Action::Write | Action::Replace | Action::Renamed)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListEntry {
    pub name: String,
    pub smart: bool,
    /// Path inside the export folder, `/`-separated.
    pub file: String,
    /// Full path of the file.
    pub path: String,
    pub tracks: usize,
    pub action: Action,
    /// For `Renamed`: the name the file would have had.
    pub wanted: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioEntry {
    pub title: String,
    pub file: String,
    pub path: String,
    pub size: u64,
    pub action: Action,
    pub wanted: Option<String>,
    /// How many of the exported lists use this file.
    pub used_by: usize,
    /// Why it cannot be copied (`Missing` / `Blocked`).
    pub problem: Option<String>,
}

/// The preview: everything the export will do, file by file.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub folder: String,
    pub lists: Vec<ListEntry>,
    pub audio: Vec<AudioEntry>,
    /// Things the user should know (cases left out and why), in words.
    pub notes: Vec<String>,
    /// Bytes the audio copies will write.
    pub copy_bytes: u64,
    /// The clock the plan was made with (smart cases with date rules match against it). Sent
    /// back with the start so the job matches against the same moment.
    pub now_ms: i64,
    /// Fingerprint of everything the plan holds: which tracks go in each list, which files are
    /// written, replaced, kept. Sent back with the start; the job refuses to run when a fresh
    /// plan no longer has the same fingerprint.
    pub digest: String,
}

/// What the dialog confirmed: the moment and fingerprint of the plan it showed.
#[derive(Debug, Clone, Deserialize)]
pub struct Confirmed {
    pub now_ms: i64,
    pub digest: String,
}

#[derive(Debug, Clone)]
enum Kind {
    Case(i64),
    Smart,
}

/// One track of a list, where its audio is and which copy (if any) stands in for it.
struct Target {
    track_id: i64,
    /// The file on the device, `None` when the location cannot be resolved safely.
    source: Option<PathBuf>,
    /// Index into [`Built::audio`].
    copy: Option<usize>,
}

struct ListJob {
    entry: ListEntry,
    kind: Kind,
    tracks: Vec<Track>,
    excluded: HashSet<i64>,
    /// The folder the playlist file ends up in.
    dir: PathBuf,
    targets: Vec<Target>,
}

struct AudioOp {
    entry: AudioEntry,
    src: Option<PathBuf>,
    dst: PathBuf,
}

/// A plan together with what the job needs to carry it out.
pub struct Built {
    folder: PathBuf,
    req: Request,
    lists: Vec<ListJob>,
    audio: Vec<AudioOp>,
    notes: Vec<String>,
    now_ms: i64,
}

/// FNV-1a, 64 bit: a small stable fingerprint (not security, only "is this the same plan").
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf29ce484222325)
    }
    fn put(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
        // a separator so ("ab","c") and ("a","bc") differ
        self.0 ^= 0xff;
        self.0 = self.0.wrapping_mul(0x100000001b3);
    }
    fn str(&mut self, s: &str) {
        self.put(s.as_bytes());
    }
    fn num(&mut self, n: u64) {
        self.put(&n.to_le_bytes());
    }
}

impl Built {
    /// Fingerprint of the plan: the lists with their track ids, the audio copies with sizes, every
    /// action and path, the notes and the options. Two builds are the same plan exactly when
    /// this matches.
    pub fn digest(&self) -> String {
        let mut h = Fnv::new();
        h.str(&self.req.format);
        h.num(self.req.relative_paths as u64);
        h.num(self.req.copy_audio as u64);
        h.str(&format!("{:?}{:?}{:?}", self.req.audio_layout, self.req.audio_names, self.req.conflict));
        for l in &self.lists {
            h.str(&l.entry.path);
            h.str(&format!("{:?}", l.entry.action));
            for t in &l.tracks {
                h.num(t.track_id as u64);
            }
            // which copy stands in for each track changes what the playlist says
            for t in &l.targets {
                h.num(t.copy.map_or(u64::MAX, |c| c as u64));
            }
        }
        for a in &self.audio {
            h.str(&a.entry.path);
            h.str(&format!("{:?}", a.entry.action));
            h.num(a.entry.size);
            h.str(&a.src.as_deref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
        }
        for n in &self.notes {
            h.str(n);
        }
        format!("{:016x}", h.0)
    }

    pub fn plan(&self) -> Plan {
        Plan {
            folder: self.folder.to_string_lossy().into_owned(),
            lists: self.lists.iter().map(|l| l.entry.clone()).collect(),
            audio: self.audio.iter().map(|a| a.entry.clone()).collect(),
            notes: self.notes.clone(),
            copy_bytes: self.audio.iter().filter(|a| a.entry.action.writes()).map(|a| a.entry.size).sum(),
            now_ms: self.now_ms,
            digest: self.digest(),
        }
    }

    /// Files the job will write: audio copies plus playlists.
    pub fn writes(&self) -> usize {
        self.audio.iter().filter(|a| a.entry.action.writes()).count() + self.lists.iter().filter(|l| l.entry.action.writes()).count()
    }
}

// ---- names and paths ------------------------------------------------------------------------------

/// A file or folder name made from `name`: characters Windows and exFAT refuse become `_`, no
/// leading or trailing space, no trailing dot, not a reserved device name, at most 100 characters.
pub fn safe_component(name: &str, fallback: &str) -> String {
    let mut s: String = name.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    s = s.chars().take(100).collect::<String>();
    s = s.trim_matches(|c: char| c == ' ' || c == '.').to_string();
    if s.is_empty() {
        s = fallback.to_string();
    }
    let base = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let numbered = |p: &str| base.len() == 4 && base.starts_with(p) && base.as_bytes()[3].is_ascii_digit();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT") {
        s = format!("_{s}");
    }
    s
}

/// `stem` plus `ext` (with its dot, or empty), numbered " (2)", " (3)"... until the name is not in
/// `used` (compared lower-case) and `taken` says no; the chosen name is added to `used`.
fn unique_name(stem: &str, ext: &str, used: &mut HashSet<String>, taken: impl Fn(&str) -> bool) -> String {
    let mut n = 1;
    loop {
        let cand = if n == 1 { format!("{stem}{ext}") } else { format!("{stem} ({n}){ext}") };
        if !used.contains(&cand.to_lowercase()) && !taken(&cand) {
            used.insert(cand.to_lowercase());
            return cand;
        }
        n += 1;
    }
}

/// `to` as a path relative to the directory `from_dir`, `/`-separated, or `None` when there is no
/// such path (different drives, a relative input, `..` in either, or the same place). `ci`
/// compares names ignoring ASCII case, for file systems that do.
pub fn relative_path(from_dir: &Path, to: &Path, ci: bool) -> Option<String> {
    let a: Vec<Component> = from_dir.components().collect();
    let b: Vec<Component> = to.components().collect();
    let rooted = |v: &[Component]| matches!(v.first(), Some(Component::Prefix(_) | Component::RootDir));
    if !rooted(&a) || !rooted(&b) || a.iter().chain(b.iter()).any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return None;
    }
    let same = |x: &Component, y: &Component| {
        if ci { x.as_os_str().eq_ignore_ascii_case(y.as_os_str()) } else { x.as_os_str() == y.as_os_str() }
    };
    let common = a.iter().zip(b.iter()).take_while(|(x, y)| same(x, y)).count();
    // a drive prefix is followed by the root: sharing only the prefix, or nothing, is not a shared place
    let root_len = if matches!(a[0], Component::Prefix(_)) { 2 } else { 1 };
    if common < root_len.min(a.len()).min(b.len()) {
        return None;
    }
    let mut parts: Vec<String> = a[common..].iter().map(|_| "..".to_string()).collect();
    for c in &b[common..] {
        match c {
            Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
            _ => return None,
        }
    }
    (!parts.is_empty() && parts.last().is_some_and(|l| l != "..")).then(|| parts.join("/"))
}

/// The audio file for a stored `location`, which must lie inside the volume's music folder. The
/// same rule as the player's and the tracks area's checks (a location that is rooted, has a drive
/// prefix or climbs with `..` is refused), kept here so this area edits no other area's file.
pub fn audio_source(lib: &Library, location: &str) -> Result<PathBuf, String> {
    if !plain_location(location) {
        return Err("the track's location points outside the device".into());
    }
    let (audio, _) = lib.track_paths(location).ok_or("the track has no file on the device")?;
    let music = lib.root().join(pm_formats::naming::MUSIC_DIR);
    let inside = audio.strip_prefix(&music).is_ok_and(|rel| rel.components().all(|c| matches!(c, Component::Normal(_))));
    if inside { Ok(audio) } else { Err("the track's location points outside the device".into()) }
}

fn rel_display(folder: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(folder).unwrap_or(p);
    rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

/// The folder must exist and may not be inside the device volume (a file written there is not
/// part of its library).
pub fn check_folder(root: &Path, folder: &str) -> AppResult<PathBuf> {
    if folder.trim().is_empty() {
        return Err("Choose a folder to export to.".into());
    }
    let p = PathBuf::from(folder);
    if p.is_file() {
        return Err("The export destination is a file; choose a folder.".into());
    }
    let canon = p.canonicalize().map_err(|_| AppError::from("The export folder does not exist."))?;
    if !canon.is_dir() {
        return Err("The export folder does not exist.".into());
    }
    if let Ok(r) = root.canonicalize()
        && canon.starts_with(&r)
    {
        return Err("Choose a folder outside the device: files written there are not part of its library.".into());
    }
    Ok(p)
}

// ---- the plan ---------------------------------------------------------------------------------------

/// A case or smart case with its tracks, ready to be named.
struct Source {
    name: String,
    kind: Kind,
    tracks: Vec<Track>,
    excluded: HashSet<i64>,
}

fn quoted(s: &str) -> String {
    format!("“{s}”")
}

/// Resolves the cases and smart cases in `req` to their tracks, with a note for everything left
/// out. Cases come first, in the order asked for, then smart cases.
fn sources(lib: &Library, config_dir: &Path, root_str: &str, req: &Request, now_ms: i64, notes: &mut Vec<String>) -> AppResult<Vec<Source>> {
    let all = lib.tracks()?;
    let by_id: HashMap<i64, &Track> = all.iter().map(|t| (t.track_id, t)).collect();
    let pick = |ids: &[i64]| -> Vec<Track> { ids.iter().filter_map(|i| by_id.get(i).map(|t| (*t).clone())).collect() };

    let mut smart_matched = Vec::new();
    if !req.smart_ids.is_empty() {
        let stored = smart::load(config_dir, root_str)?;
        let mut chosen: Vec<SmartCase> = Vec::new();
        for id in &req.smart_ids {
            match stored.iter().find(|s| s.id == *id) {
                Some(s) if !chosen.iter().any(|c| c.id == s.id) => chosen.push(s.clone()),
                Some(_) => {}
                None => notes.push(format!("Smart case {id} no longer exists, so it was left out.")),
            }
        }
        let ev = evaluate_all(lib, &chosen, now_ms, Some(config_dir))?;
        notes.extend(ev.left_out);
        smart_matched = ev.matched;
    }
    // the Pacemaker copy (★) of a smart case that is exported as itself says the same thing a second time
    let mut copies: HashMap<i64, String> = HashMap::new();
    for m in &smart_matched {
        if let Some(copy) = smart::owned_device_case(lib, &m.sc)? {
            copies.insert(copy, m.sc.name.trim().to_string());
        }
    }

    let infos = lib.cases()?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for id in &req.case_ids {
        if !seen.insert(*id) {
            continue;
        }
        let Some(info) = infos.iter().find(|c| c.id == *id) else {
            notes.push(format!("Case {id} no longer exists, so it was left out."));
            continue;
        };
        if let Some(smart_name) = copies.get(id) {
            notes.push(format!("{} is the Pacemaker copy of the smart case {}, which is exported as itself, so it was not written a second time.", quoted(&info.name), quoted(smart_name)));
            continue;
        }
        let ids = lib.case_track_ids(*id)?;
        out.push(Source { name: info.name.clone(), kind: Kind::Case(*id), tracks: pick(&ids), excluded: HashSet::new() });
    }
    for m in smart_matched {
        out.push(Source { name: m.sc.name.trim().to_string(), kind: Kind::Smart, tracks: pick(&m.ids), excluded: HashSet::new() });
    }
    for s in &mut out {
        if let Some((ids, note)) = left_out_escaping(&s.tracks, &quoted(&s.name)) {
            s.excluded = ids.into_iter().collect();
            s.tracks.retain(|t| !s.excluded.contains(&t.track_id));
            notes.push(note);
        }
    }
    Ok(out)
}

/// Whether `dst` holds the same bytes as `src` as far as a quick look can tell: same size, and the
/// first, middle and last 64 KiB agree (a file up to three blocks long is compared whole). Not a
/// full compare: reading every audio file twice on each preview would make the dialog crawl.
/// Any read error counts as "different".
fn looks_identical(src: &Path, dst: &Path, len: u64) -> bool {
    use std::io::{Seek, SeekFrom};
    const BLOCK: u64 = 64 * 1024;
    let check = || -> std::io::Result<bool> {
        let (mut a, mut b) = (std::fs::File::open(src)?, std::fs::File::open(dst)?);
        if a.metadata()?.len() != len || b.metadata()?.len() != len {
            return Ok(false);
        }
        let spans: Vec<(u64, u64)> = if len <= 3 * BLOCK { vec![(0, len)] } else { vec![(0, BLOCK), ((len - BLOCK) / 2, BLOCK), (len - BLOCK, BLOCK)] };
        let (mut x, mut y) = (Vec::new(), Vec::new());
        for (at, n) in spans {
            x.resize(n as usize, 0);
            y.resize(n as usize, 0);
            a.seek(SeekFrom::Start(at))?;
            b.seek(SeekFrom::Start(at))?;
            a.read_exact(&mut x)?;
            b.read_exact(&mut y)?;
            if x != y {
                return Ok(false);
            }
        }
        Ok(true)
    };
    check().unwrap_or(false)
}

/// What to do about `dst` given the conflict policy; for `Number` the numbered name is chosen
/// with `used` and the disk. `same_as` is the audio source and its size: a file already at `dst`
/// that [`looks_identical`] is the copy and is left alone whatever the policy; one that only has
/// the same size is a different file and follows the policy. Returns the final path, the action
/// and the wanted name if renamed.
fn settle(dst: PathBuf, same_as: Option<(&Path, u64)>, policy: Conflict, used: &mut HashSet<String>) -> (PathBuf, Action, Option<String>) {
    if dst.is_dir() {
        return (dst, Action::Blocked, None);
    }
    if !dst.exists() {
        return (dst, Action::Write, None);
    }
    if let Some((src, want)) = same_as
        && std::fs::metadata(&dst).is_ok_and(|m| m.len() == want)
        && looks_identical(src, &dst, want)
    {
        return (dst, Action::Same, None);
    }
    match policy {
        Conflict::Skip => (dst, Action::Skip, None),
        Conflict::Replace => (dst, Action::Replace, None),
        Conflict::Number => {
            let dir = dst.parent().map(Path::to_path_buf).unwrap_or_default();
            let wanted = dst.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let (stem, ext) = split_name(&wanted);
            // the plain name is already in `used`; the numbered one must be free on disk and unused
            let name = unique_name(&stem, &ext, used, |c| dir.join(c).exists());
            (dir.join(name), Action::Renamed, Some(wanted))
        }
    }
}

/// `("name", ".ext")` of a file name; a leading dot is part of the stem.
fn split_name(file: &str) -> (String, String) {
    match file.rfind('.') {
        Some(i) if i > 0 && file.len() - i <= 9 => (file[..i].to_string(), file[i..].to_string()),
        _ => (file.to_string(), String::new()),
    }
}

/// Plans an export. Reads the library, the smart-case store and the destination folder; writes
/// nothing.
pub fn build(lib: &Library, config_dir: &Path, root_str: &str, req: &Request, now_ms: i64) -> AppResult<Built> {
    let ext = extension(&req.format).ok_or_else(|| AppError::from(format!("unknown export format {}", req.format)))?;
    let folder = check_folder(Path::new(root_str), &req.folder)?;
    if req.case_ids.is_empty() && req.smart_ids.is_empty() {
        return Err("Tick at least one case to export.".into());
    }
    let mut notes = Vec::new();
    let srcs = sources(lib, config_dir, root_str, req, now_ms, &mut notes)?;
    let copy = req.copy_audio;
    let per_case = copy && req.audio_layout == Layout::PerCase;

    // list files
    let mut stems: HashSet<String> = HashSet::new();
    let mut used_in: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut lists: Vec<ListJob> = Vec::new();
    for s in srcs {
        if s.tracks.is_empty() {
            notes.push(format!("{} has no tracks to export, so no file was written for it.", quoted(&s.name)));
            continue;
        }
        let base = safe_component(&s.name, "case");
        // a smart case that shares a case's name is told apart before the numbers are tried
        let mut stem = base.clone();
        if matches!(s.kind, Kind::Smart) && stems.contains(&base.to_lowercase()) {
            stem = format!("{base} (smart)");
        }
        let stem = unique_name(&stem, "", &mut stems, |_| false);
        let dir = if per_case { folder.join(&stem) } else { folder.clone() };
        let wanted_name = format!("{stem}.{ext}");
        let dst = dir.join(&wanted_name);
        let used = used_in.entry(dir.clone()).or_default();
        used.insert(wanted_name.to_lowercase());
        let (dst, action, wanted) = if per_case && dir.exists() && !dir.is_dir() { (dst, Action::Blocked, None) } else { settle(dst, None, req.conflict, used) };
        let dir = dst.parent().map(Path::to_path_buf).unwrap_or(dir);
        let targets = s
            .tracks
            .iter()
            .map(|t| Target { track_id: t.track_id, source: audio_source(lib, &t.location).ok(), copy: None })
            .collect();
        lists.push(ListJob {
            entry: ListEntry {
                name: s.name.clone(),
                smart: matches!(s.kind, Kind::Smart),
                file: rel_display(&folder, &dst),
                path: dst.to_string_lossy().into_owned(),
                tracks: s.tracks.len(),
                action,
                wanted,
            },
            kind: s.kind,
            tracks: s.tracks,
            excluded: s.excluded,
            dir,
            targets,
        });
    }
    if lists.is_empty() {
        let why = if notes.is_empty() { String::new() } else { format!(" {}", notes.join(" ")) };
        return Err(format!("Nothing to export: none of the chosen cases has tracks.{why}").into());
    }

    // audio copies
    let mut audio: Vec<AudioOp> = Vec::new();
    if copy {
        let mut index: HashMap<(usize, String), usize> = HashMap::new();
        let mut lib_used: HashMap<PathBuf, HashSet<String>> = HashMap::new();
        let shared_dir = folder.join(AUDIO_DIR);
        for (li, list) in lists.iter_mut().enumerate() {
            let dir = if per_case { list.dir.clone() } else { shared_dir.clone() };
            for (ti, t) in list.tracks.iter().enumerate() {
                let key = (if per_case { li } else { 0 }, t.location.clone());
                let opi = match index.get(&key) {
                    Some(i) => *i,
                    None => {
                        let src = list.targets[ti].source.clone();
                        let file = t.location.rsplit('/').next().unwrap_or("audio");
                        let (device_stem, ext) = split_name(file);
                        let stem = match (req.audio_names, t.artist.trim().is_empty(), t.title.trim().is_empty()) {
                            (NameBy::Device, _, _) | (_, true, true) => device_stem,
                            (_, true, false) => t.title.trim().to_string(),
                            (_, false, true) => t.artist.trim().to_string(),
                            (_, false, false) => format!("{} - {}", t.artist.trim(), t.title.trim()),
                        };
                        let name = unique_name(&safe_component(&stem, "track"), &ext, lib_used.entry(dir.clone()).or_default(), |_| false);
                        let dst = dir.join(&name);
                        let title = if t.title.trim().is_empty() { file.to_string() } else { t.title.clone() };
                        let mut op = AudioOp {
                            entry: AudioEntry { title, file: rel_display(&folder, &dst), path: dst.to_string_lossy().into_owned(), size: 0, action: Action::Write, wanted: None, used_by: 0, problem: None },
                            src: src.clone(),
                            dst: dst.clone(),
                        };
                        match src.as_ref().map(std::fs::metadata) {
                            None => {
                                op.entry.action = Action::Missing;
                                op.entry.problem = Some("the track's location points outside the device".into());
                            }
                            Some(Err(_)) => {
                                op.entry.action = Action::Missing;
                                op.entry.problem = Some("the audio file is missing from the device".into());
                            }
                            Some(Ok(m)) if !m.is_file() => {
                                op.entry.action = Action::Missing;
                                op.entry.problem = Some("the audio file is missing from the device".into());
                            }
                            Some(Ok(m)) => {
                                op.entry.size = m.len();
                                let used = lib_used.entry(dir.clone()).or_default();
                                let (final_dst, action, wanted) = settle(dst, src.as_deref().map(|p| (p, m.len())), req.conflict, used);
                                if action == Action::Blocked {
                                    op.entry.problem = Some("a folder with that name is in the way".into());
                                }
                                op.entry.file = rel_display(&folder, &final_dst);
                                op.entry.path = final_dst.to_string_lossy().into_owned();
                                op.entry.action = action;
                                op.entry.wanted = wanted;
                                op.dst = final_dst;
                            }
                        }
                        audio.push(op);
                        index.insert(key, audio.len() - 1);
                        audio.len() - 1
                    }
                };
                audio[opi].entry.used_by += 1;
                list.targets[ti].copy = Some(opi);
            }
        }
    }

    let mut built = Built { folder, req: req.clone(), lists, audio, notes, now_ms };
    // a relative path needs the audio and the playlist on one drive: say how many were not
    if built.req.relative_paths && built.req.format == "m3u8" {
        let mut absolute = 0;
        for l in &built.lists {
            absolute += paths_for(&built, l, &HashSet::new()).1;
        }
        if absolute > 0 {
            built.notes.push(format!(
                "{absolute} track{} could not be given a relative path, because {} audio file{} on another drive than the playlist; {} written with the full path.",
                if absolute == 1 { "" } else { "s" },
                if absolute == 1 { "its" } else { "their" },
                if absolute == 1 { " is" } else { "s are" },
                if absolute == 1 { "it is" } else { "they are" },
            ));
        }
    }
    Ok(built)
}

/// The audio paths the list `l` writes, and how many of them are full paths although a relative
/// one was asked for. `failed` holds the copies that could not be made: those entries point at
/// the original file. The preview and the job both ask this.
fn paths_for(b: &Built, l: &ListJob, failed: &HashSet<usize>) -> (PathMap, usize) {
    let relative = b.req.relative_paths && b.req.format == "m3u8";
    let ci = cfg!(windows);
    let mut map = PathMap::new();
    let mut absolute = 0;
    for t in &l.targets {
        // a copy stands in for the file only when it is the copy this export made or found identical
        let copy = t.copy.filter(|i| !failed.contains(i)).map(|i| &b.audio[i]).filter(|a| matches!(a.entry.action, Action::Write | Action::Replace | Action::Renamed | Action::Same));
        let target = match copy {
            Some(a) => Some(a.dst.clone()),
            None => t.source.clone(),
        };
        let Some(target) = target else { continue };
        if relative {
            match relative_path(&l.dir, &target, ci) {
                Some(r) => {
                    map.insert(t.track_id, r);
                }
                None => {
                    absolute += 1;
                    if copy.is_some() {
                        map.insert(t.track_id, target.display().to_string());
                    }
                }
            }
        } else if copy.is_some() {
            map.insert(t.track_id, target.display().to_string());
        }
    }
    (map, absolute)
}

// ---- running it -------------------------------------------------------------------------------------

/// One step of progress for the job strip.
pub struct Step {
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub status: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub lists_written: usize,
    pub audio_copied: usize,
    pub audio_same: usize,
    /// Files that were already there and were kept (playlists and audio).
    pub kept: usize,
    pub failed: Vec<String>,
    pub cancelled: bool,
}

impl Outcome {
    pub fn summary(&self, folder: &str) -> String {
        let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
        let mut parts = vec![format!("{} written", plural(self.lists_written, "playlist"))];
        if self.audio_copied > 0 {
            parts.push(format!("{} copied", plural(self.audio_copied, "audio file")));
        }
        if self.audio_same > 0 {
            parts.push(format!("{} already there", plural(self.audio_same, "audio file")));
        }
        if self.kept > 0 {
            parts.push(format!("{} left alone", plural(self.kept, "existing file")));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} failed", self.failed.len()));
        }
        let head = if self.cancelled { "Cancelled. Files already written stay in the folder" } else { "Exported" };
        format!("{head}: {} ({folder}).", parts.join(", "))
    }
}

enum CopyEnd {
    Done,
    Cancelled,
    Failed(String),
}

/// Copies `src` to `dst` through `dst.part` and a rename; the copy is checked against the source's
/// size. Without `replace` an existing `dst` is left alone. Stops (removing the partial file) when
/// `cancel` is set.
fn copy_file(src: &Path, dst: &Path, replace: bool, cancel: &AtomicBool) -> CopyEnd {
    if !replace && dst.exists() {
        return CopyEnd::Failed("a file of that name appeared after the preview, so it was left alone".into());
    }
    if let Some(parent) = dst.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return CopyEnd::Failed(format!("could not create the folder: {e}"));
    }
    let tmp = {
        let mut n = dst.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        n.push(".part");
        dst.with_file_name(n)
    };
    let attempt = || -> std::io::Result<Option<()>> {
        let mut input = std::fs::File::open(src)?;
        let want = input.metadata()?.len();
        let mut out = std::fs::File::create(&tmp)?;
        let mut buf = vec![0u8; 1 << 20];
        let mut total = 0u64;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            total += n as u64;
        }
        out.flush()?;
        out.sync_all()?;
        drop(out);
        if total != want {
            return Err(std::io::Error::other(format!("copied {total} of {want} bytes")));
        }
        Ok(Some(()))
    };
    match attempt() {
        Ok(Some(())) => match std::fs::rename(&tmp, dst) {
            Ok(()) => CopyEnd::Done,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                CopyEnd::Failed(e.to_string())
            }
        },
        Ok(None) => {
            let _ = std::fs::remove_file(&tmp);
            CopyEnd::Cancelled
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            CopyEnd::Failed(e.to_string())
        }
    }
}

fn render(lib: &Library, l: &ListJob, format: &str, paths: PathMap) -> Result<String, Error> {
    match format {
        "m3u8" => Ok(interop::export_m3u8_with(lib, &l.tracks, &paths)),
        "csv" => Ok(interop::export_csv_with(lib, &l.tracks, &paths)),
        _ => {
            let opts = match l.kind {
                Kind::Case(id) => XmlOptions { only_case: Some(id), exclude_tracks: l.excluded.clone(), paths, ..Default::default() },
                Kind::Smart => XmlOptions {
                    extras_only: true,
                    exclude_tracks: l.excluded.clone(),
                    extra: vec![ExtraPlaylist { name: l.entry.name.trim().to_string(), track_ids: l.tracks.iter().map(|t| t.track_id).collect() }],
                    paths,
                    ..Default::default()
                },
            };
            interop::export_rekordbox_with(lib, &opts)
        }
    }
}

/// Carries out `b`. `step` is called after each file; `cancel` stops it between files and inside
/// a copy. Playlists are written after the audio, each only if it would not replace a file that
/// the plan said was free.
pub fn execute(lib: &Library, b: &Built, cancel: &AtomicBool, step: &mut dyn FnMut(Step)) -> Outcome {
    let total = b.writes();
    let mut done = 0;
    let mut out = Outcome::default();
    let mut failed_copies: HashSet<usize> = HashSet::new();

    for (i, a) in b.audio.iter().enumerate() {
        match a.entry.action {
            Action::Same => out.audio_same += 1,
            Action::Skip => out.kept += 1,
            Action::Missing | Action::Blocked => {
                let why = a.entry.problem.clone().unwrap_or_else(|| "not copied".into());
                out.failed.push(format!("{}: {why}", a.entry.title));
                failed_copies.insert(i);
                step(Step { label: a.entry.title.clone(), done: 0, total, status: "failed", error: Some(why) });
            }
            _ => {}
        }
    }
    for (i, a) in b.audio.iter().enumerate() {
        if !a.entry.action.writes() {
            continue;
        }
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            return out;
        }
        let src = a.src.as_deref().expect("a copy that writes has a source");
        let (status, error) = match copy_file(src, &a.dst, a.entry.action == Action::Replace, cancel) {
            CopyEnd::Done => {
                out.audio_copied += 1;
                ("copied", None)
            }
            CopyEnd::Cancelled => {
                out.cancelled = true;
                return out;
            }
            CopyEnd::Failed(e) => {
                out.failed.push(format!("{}: {e}", a.entry.file));
                failed_copies.insert(i);
                ("failed", Some(e))
            }
        };
        done += 1;
        step(Step { label: a.entry.file.clone(), done, total, status, error });
    }

    for l in &b.lists {
        match l.entry.action {
            Action::Skip => out.kept += 1,
            Action::Blocked => {
                let why = "a folder with that name is in the way".to_string();
                out.failed.push(format!("{}: {why}", l.entry.file));
                step(Step { label: l.entry.file.clone(), done, total, status: "failed", error: Some(why) });
            }
            Action::Write | Action::Replace | Action::Renamed => {
                if cancel.load(Ordering::SeqCst) {
                    out.cancelled = true;
                    return out;
                }
                let dest = Path::new(&l.entry.path);
                let result = (|| -> Result<(), String> {
                    if l.entry.action != Action::Replace && dest.exists() {
                        return Err("a file of that name appeared after the preview, so it was left alone".into());
                    }
                    let text = render(lib, l, &b.req.format, paths_for(b, l, &failed_copies).0).map_err(|e| e.to_string())?;
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| format!("could not create the folder: {e}"))?;
                    }
                    write_out(&l.entry.path, &text).map_err(|e| e.to_string())
                })();
                done += 1;
                match result {
                    Ok(()) => {
                        out.lists_written += 1;
                        step(Step { label: l.entry.file.clone(), done, total, status: "written", error: None });
                    }
                    Err(e) => {
                        out.failed.push(format!("{}: {e}", l.entry.file));
                        step(Step { label: l.entry.file.clone(), done, total, status: "failed", error: Some(e) });
                    }
                }
            }
            Action::Same | Action::Missing => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cases::smart::{Query, Rule};
    use pm_formats::{library::TrackMeta, strfile};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-bulkexp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A volume with `n` synthetic tracks (file `i` holds `2048 + i` zero bytes).
    fn volume(tag: &str, n: usize) -> (Library, PathBuf, Vec<i64>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src = temp_dir(&format!("{tag}-src"));
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: 22050 + i * 22050, bpm: 120.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let p = src.join(format!("bulk {i}.mp3"));
            std::fs::write(&p, vec![i as u8 + 1; 2048 + i]).unwrap();
            let meta = TrackMeta { title: format!("Bulk Title {i}"), artist: "Bulk Artist".into(), format: "MP3".into(), sample_rate: 44100, play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        let _ = std::fs::remove_dir_all(&src);
        (lib, vol, ids)
    }

    fn req(folder: &Path, format: &str, cases: Vec<i64>) -> Request {
        Request {
            folder: folder.to_string_lossy().into_owned(),
            format: format.into(),
            case_ids: cases,
            smart_ids: Vec::new(),
            relative_paths: false,
            copy_audio: false,
            audio_layout: Layout::Shared,
            audio_names: NameBy::Tags,
            conflict: Conflict::Skip,
        }
    }

    fn run(lib: &Library, b: &Built) -> Outcome {
        execute(lib, b, &AtomicBool::new(false), &mut |_| {})
    }

    fn files(dir: &Path) -> Vec<String> {
        fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(base, &p, out);
                } else {
                    out.push(rel_display(base, &p));
                }
            }
        }
        let mut v = Vec::new();
        walk(dir, dir, &mut v);
        v.sort();
        v
    }

    #[test]
    fn names_are_made_safe_for_windows_and_exfat() {
        assert_eq!(safe_component("Warm-up: 5/6?", "x"), "Warm-up_ 5_6_");
        assert_eq!(safe_component("  dots.. ", "x"), "dots");
        assert_eq!(safe_component("", "case"), "case");
        assert_eq!(safe_component("...", "case"), "case");
        assert_eq!(safe_component("con", "x"), "_con");
        assert_eq!(safe_component("LPT3.txt", "x"), "_LPT3.txt");
        assert_eq!(safe_component("COM10", "x"), "COM10");
        assert_eq!(safe_component(&"a".repeat(300), "x").chars().count(), 100);
        assert_eq!(safe_component("tab\there", "x"), "tab_here");
        assert_eq!(split_name("a.b.mp3"), ("a.b".to_string(), ".mp3".to_string()));
        assert_eq!(split_name(".hidden"), (".hidden".to_string(), String::new()));
        assert_eq!(split_name("noext"), ("noext".to_string(), String::new()));
    }

    #[test]
    fn unique_names_count_up_and_ignore_case() {
        let mut used = HashSet::new();
        assert_eq!(unique_name("Set", ".m3u8", &mut used, |_| false), "Set.m3u8");
        assert_eq!(unique_name("set", ".m3u8", &mut used, |_| false), "set (2).m3u8");
        assert_eq!(unique_name("SET", ".m3u8", &mut used, |_| false), "SET (3).m3u8");
        assert_eq!(unique_name("Other", "", &mut used, |c| c == "Other"), "Other (2)");
    }

    #[test]
    fn relative_paths_climb_and_descend() {
        let r = |a: &str, b: &str| relative_path(Path::new(a), Path::new(b), false);
        assert_eq!(r("/out", "/out/Audio/a.mp3").as_deref(), Some("Audio/a.mp3"));
        assert_eq!(r("/out/Set", "/out/Audio/a.mp3").as_deref(), Some("../Audio/a.mp3"));
        assert_eq!(r("/out/a/b", "/x/y.mp3").as_deref(), Some("../../../x/y.mp3"));
        assert_eq!(r("/out", "/out"), None, "the same place has no file name");
        assert_eq!(r("/out/a", "/out"), None, "a folder is not a file");
        assert_eq!(r("out", "/out/a.mp3"), None, "relative input");
        assert_eq!(r("/out", "/out/../a.mp3"), None, "parent segments");
        assert_eq!(relative_path(Path::new("/OUT"), Path::new("/out/a.mp3"), true).as_deref(), Some("a.mp3"));
        assert_eq!(relative_path(Path::new("/OUT"), Path::new("/out/a.mp3"), false).as_deref(), Some("../out/a.mp3"));
    }

    #[cfg(windows)]
    #[test]
    fn relative_paths_need_one_drive() {
        let r = |a: &str, b: &str| relative_path(Path::new(a), Path::new(b), true);
        assert_eq!(r("C:\\Out", "C:\\Out\\Audio\\a.mp3").as_deref(), Some("Audio/a.mp3"));
        assert_eq!(r("c:\\Out", "C:\\out\\a.mp3").as_deref(), Some("a.mp3"));
        assert_eq!(r("C:\\Out", "E:\\Out\\a.mp3"), None, "another drive");
        assert_eq!(r("C:\\Out\\x", "C:\\y\\a.mp3").as_deref(), Some("../../y/a.mp3"));
    }

    #[test]
    fn locations_that_leave_the_music_folder_are_refused() {
        let (lib, vol, ids) = volume("src", 1);
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == ids[0]).unwrap().location;
        assert!(audio_source(&lib, &loc).unwrap().is_file());
        for bad in ["/pmdb_tracks/../x.mp3", "/pmdb_tracks//x.mp3", "/pmdb_tracks/C:/x.mp3", "/pmdb_tracks/a\\b.mp3", "/elsewhere/x.mp3", "", "/pmdb_tracks/"] {
            assert!(audio_source(&lib, bad).is_err(), "{bad}");
        }
        drop(lib);
        std::fs::remove_dir_all(&vol).unwrap();
    }

    #[test]
    fn the_folder_must_exist_and_be_outside_the_device() {
        let (lib, vol, ids) = volume("folder", 2);
        let cfg = temp_dir("folder-cfg");
        let out = temp_dir("folder-out");
        let case = {
            let mut l = Library::open(&vol).unwrap();
            l.create_case_with_tracks("One", &ids).unwrap()
        };
        let root = vol.to_string_lossy().into_owned();
        let try_it = |folder: &Path, fmt: &str| build(&lib, &cfg, &root, &req(folder, fmt, vec![case]), 0).err().map(|e| e.to_string());
        assert!(try_it(&vol, "m3u8").unwrap().contains("outside the device"));
        assert!(try_it(&vol.join(".Pacemaker"), "m3u8").unwrap().contains("outside the device"));
        assert!(try_it(&out.join("nowhere"), "m3u8").unwrap().contains("does not exist"));
        assert!(try_it(&out, "pdf").unwrap().contains("unknown export format"));
        assert!(try_it(Path::new(""), "m3u8").unwrap().contains("Choose a folder"));
        assert!(build(&lib, &cfg, &root, &req(&out, "m3u8", vec![]), 0).err().unwrap().to_string().contains("Tick at least one"));
        assert!(try_it(&out, "m3u8").is_none());
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn one_file_per_case_in_each_format_and_the_empty_case_is_named_not_written() {
        let (mut lib, vol, ids) = volume("fmt", 3);
        let cfg = temp_dir("fmt-cfg");
        let out = temp_dir("fmt-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Warm-up: set/1", &[ids[2], ids[0]]).unwrap();
        let b = lib.create_case_with_tracks("Closer", &[ids[1]]).unwrap();
        let empty = lib.create_case("Nothing yet").unwrap();
        for (fmt, ext) in [("m3u8", "m3u8"), ("csv", "csv"), ("rekordbox", "xml")] {
            let dir = out.join(fmt);
            std::fs::create_dir_all(&dir).unwrap();
            let built = build(&lib, &cfg, &root, &req(&dir, fmt, vec![a, b, empty]), 0).unwrap();
            let plan = built.plan();
            assert_eq!(plan.lists.iter().map(|l| l.file.clone()).collect::<Vec<_>>(), [format!("Warm-up_ set_1.{ext}"), format!("Closer.{ext}")]);
            assert_eq!(plan.lists.iter().map(|l| l.tracks).collect::<Vec<_>>(), [2, 1]);
            assert!(plan.lists.iter().all(|l| l.action == Action::Write));
            assert!(plan.notes.iter().any(|n| n.contains("“Nothing yet”") && n.contains("no tracks")), "{:?}", plan.notes);
            assert!(plan.audio.is_empty() && plan.copy_bytes == 0);
            let o = run(&lib, &built);
            assert_eq!((o.lists_written, o.failed.len(), o.cancelled), (2, 0, false), "{o:?}");
            assert_eq!(files(&dir), {
                let mut v = vec![format!("Closer.{ext}"), format!("Warm-up_ set_1.{ext}")];
                v.sort();
                v
            });
            let text = std::fs::read_to_string(dir.join(format!("Warm-up_ set_1.{ext}"))).unwrap();
            let (p2, p0) = (text.find("Bulk Title 2").unwrap(), text.find("Bulk Title 0").unwrap());
            assert!(p2 < p0 && !text.contains("Bulk Title 1"), "{fmt}: case order, only this case");
        }
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn names_that_clash_after_cleaning_are_numbered_and_a_smart_case_is_told_apart() {
        let (mut lib, vol, ids) = volume("clash", 2);
        let cfg = temp_dir("clash-cfg");
        let out = temp_dir("clash-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("A/B", &[ids[0]]).unwrap();
        let b = lib.create_case_with_tracks("a_b", &[ids[1]]).unwrap();
        lib.set_track_ratings(&[(ids[0], 5)]).unwrap();
        let sc = smart::save(&cfg, &root, SmartCase { id: 0, name: "A_B".into(), query: Query { rules: vec![Rule::RatingMin { stars: 5 }], ..Default::default() }, on_device: false, device_case_id: None }).unwrap();
        let mut r = req(&out, "m3u8", vec![a, b]);
        r.smart_ids = vec![sc.id];
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        let names: Vec<String> = built.plan().lists.iter().map(|l| l.file.clone()).collect();
        assert_eq!(names, ["A_B.m3u8", "a_b (2).m3u8", "A_B (smart).m3u8"]);
        assert_eq!(built.plan().lists.iter().map(|l| l.smart).collect::<Vec<_>>(), [false, false, true]);
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_smart_case_exports_what_it_matches_and_its_pacemaker_copy_is_not_written_twice() {
        let (mut lib, vol, ids) = volume("smart", 3);
        let cfg = temp_dir("smart-cfg");
        let out = temp_dir("smart-out");
        let root = vol.to_string_lossy().into_owned();
        lib.set_track_ratings(&[(ids[0], 5), (ids[2], 5)]).unwrap();
        let sc = smart::save(&cfg, &root, SmartCase { id: 0, name: "Top".into(), query: Query { rules: vec![Rule::RatingMin { stars: 5 }], ..Default::default() }, on_device: true, device_case_id: None }).unwrap();
        let (copy, _) = smart::put_on_device(&mut lib, &sc, &[ids[0]]).unwrap();
        smart::update(&cfg, &root, &SmartCase { device_case_id: Some(copy), ..sc.clone() }).unwrap();
        let plain = lib.create_case_with_tracks("Plain", &[ids[1]]).unwrap();

        // the copy is exported as an ordinary case when the smart case is not part of the export
        let built = build(&lib, &cfg, &root, &req(&out, "csv", vec![copy, plain]), 0).unwrap();
        assert_eq!(built.plan().lists.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), ["★ Top", "Plain"]);

        // ...and left out, with a note, when the smart case is
        let mut r = req(&out, "csv", vec![copy, plain]);
        r.smart_ids = vec![sc.id, 77];
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        let plan = built.plan();
        assert_eq!(plan.lists.iter().map(|l| (l.name.clone(), l.tracks)).collect::<Vec<_>>(), [("Plain".to_string(), 1), ("Top".to_string(), 2)]);
        assert!(plan.notes.iter().any(|n| n.contains("“★ Top”") && n.contains("not written a second time")), "{:?}", plan.notes);
        assert!(plan.notes.iter().any(|n| n.contains("Smart case 77 no longer exists")), "{:?}", plan.notes);
        run(&lib, &built);
        let csv = std::fs::read_to_string(out.join("Top.csv")).unwrap();
        assert!(csv.contains("Bulk Title 0") && csv.contains("Bulk Title 2") && !csv.contains("Bulk Title 1"));
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_smart_case_that_cannot_run_is_named_and_the_rest_still_export() {
        let (mut lib, vol, ids) = volume("smartbad", 2);
        let cfg = temp_dir("smartbad-cfg");
        let out = temp_dir("smartbad-out");
        let root = vol.to_string_lossy().into_owned();
        let gone = lib.create_case("Soon gone").unwrap();
        let broken = smart::save(&cfg, &root, SmartCase { id: 0, name: "Depends".into(), query: Query { rules: vec![Rule::InCase { case_id: gone, value: true, case_name: None }], ..Default::default() }, on_device: false, device_case_id: None }).unwrap();
        lib.delete_case(gone).unwrap();
        let plain = lib.create_case_with_tracks("Plain", &[ids[0]]).unwrap();
        let mut r = req(&out, "m3u8", vec![plain]);
        r.smart_ids = vec![broken.id];
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        let plan = built.plan();
        assert_eq!(plan.lists.len(), 1);
        assert!(plan.notes.iter().any(|n| n.contains("“Depends” was left out")), "{:?}", plan.notes);
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn copying_the_audio_shares_one_folder_and_the_playlist_uses_relative_paths_to_it() {
        let (mut lib, vol, ids) = volume("copy", 3);
        let cfg = temp_dir("copy-cfg");
        let out = temp_dir("copy-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0], ids[1]]).unwrap();
        let b = lib.create_case_with_tracks("Beta", &[ids[1], ids[2]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a, b]);
        r.copy_audio = true;
        r.relative_paths = true;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        let plan = built.plan();
        assert_eq!(plan.audio.len(), 3, "the track in both cases is copied once");
        assert_eq!(plan.audio.iter().map(|x| x.used_by).collect::<Vec<_>>(), [1, 2, 1]);
        assert_eq!(plan.copy_bytes, (2048 + 2049 + 2050) as u64);
        assert!(plan.audio.iter().all(|x| x.file.starts_with("Audio/") && x.action == Action::Write));
        assert!(!out.join("Audio").exists(), "planning writes nothing");
        assert!(files(&out).is_empty());

        let mut steps = Vec::new();
        let o = execute(&lib, &built, &AtomicBool::new(false), &mut |s| steps.push((s.done, s.total, s.status)));
        assert_eq!((o.lists_written, o.audio_copied, o.failed.len()), (2, 3, 0), "{o:?}");
        assert_eq!(steps.len(), 5);
        assert_eq!(steps.last().copied(), Some((5, 5, "written")), "playlists come after the audio");
        assert_eq!(steps.iter().take(3).map(|s| s.2).collect::<Vec<_>>(), ["copied"; 3]);

        let listing = files(&out);
        assert_eq!(listing.len(), 5, "{listing:?}");
        // every copy is byte-identical to the original
        for (i, e) in plan.audio.iter().enumerate() {
            let want = vec![ids.iter().position(|x| *x == ids[i]).unwrap() as u8 + 1; 2048 + i];
            assert_eq!(std::fs::read(&e.path).unwrap(), want, "{}", e.file);
        }
        let text = std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap();
        let entries: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(entries.len(), 2);
        for e in &entries {
            assert!(e.starts_with("Audio/") && e.ends_with(".mp3") && !e.contains(':'), "{e}");
            assert!(out.join(e).is_file(), "{e} resolves from the playlist's folder");
        }

        // run again: everything is already there, nothing is written
        let again = build(&lib, &cfg, &root, &r, 0).unwrap();
        let p2 = again.plan();
        assert!(p2.audio.iter().all(|x| x.action == Action::Same) && p2.lists.iter().all(|l| l.action == Action::Skip), "{:?}", p2.lists.iter().map(|l| l.action).collect::<Vec<_>>());
        assert_eq!(p2.copy_bytes, 0);
        let o = run(&lib, &again);
        assert_eq!((o.lists_written, o.audio_copied, o.audio_same, o.kept), (0, 0, 3, 2));
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn absolute_paths_to_the_copies_in_csv_and_xml() {
        let (mut lib, vol, ids) = volume("abscopy", 2);
        let cfg = temp_dir("abscopy-cfg");
        let out = temp_dir("abscopy-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0], ids[1]]).unwrap();
        for fmt in ["csv", "rekordbox"] {
            let dir = out.join(fmt);
            std::fs::create_dir_all(&dir).unwrap();
            let mut r = req(&dir, fmt, vec![a]);
            r.copy_audio = true;
            r.relative_paths = true; // M3U8 only: ignored here
            let built = build(&lib, &cfg, &root, &r, 0).unwrap();
            run(&lib, &built);
            let file = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).find(|p| p.extension().is_some_and(|e| e == "csv" || e == "xml")).unwrap();
            let text = std::fs::read_to_string(&file).unwrap();
            assert!(text.contains("Audio") && !text.contains(".Pacemaker"), "{fmt}: points at the copies, not the device\n{text}");
        }
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn per_case_layout_puts_a_playlist_and_its_own_audio_in_a_folder_each() {
        let (mut lib, vol, ids) = volume("percase", 2);
        let cfg = temp_dir("percase-cfg");
        let out = temp_dir("percase-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0], ids[1]]).unwrap();
        let b = lib.create_case_with_tracks("Beta", &[ids[1]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a, b]);
        r.copy_audio = true;
        r.relative_paths = true;
        r.audio_layout = Layout::PerCase;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert_eq!(built.plan().audio.len(), 3, "the shared track is copied into each folder");
        let o = run(&lib, &built);
        assert_eq!((o.lists_written, o.audio_copied), (2, 3));
        let listing = files(&out);
        assert_eq!(listing.iter().filter(|f| f.starts_with("Alpha/")).count(), 3);
        assert_eq!(listing.iter().filter(|f| f.starts_with("Beta/")).count(), 2);
        let text = std::fs::read_to_string(out.join("Beta").join("Beta.m3u8")).unwrap();
        let entry = text.lines().find(|l| !l.starts_with('#')).unwrap();
        assert!(!entry.contains('/'), "alongside the playlist: {entry}");
        assert!(out.join("Beta").join(entry).is_file());
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn relative_paths_without_copying_point_at_the_device_when_the_drive_allows() {
        let (mut lib, vol, ids) = volume("relsrc", 1);
        let cfg = temp_dir("relsrc-cfg");
        let out = temp_dir("relsrc-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.relative_paths = true;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        run(&lib, &built);
        let text = std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap();
        let entry = text.lines().find(|l| !l.starts_with('#')).unwrap();
        // the temp folders share a drive, so a relative path exists and resolves to the audio
        assert!(entry.starts_with("../"), "{entry}");
        assert!(out.join(entry).is_file(), "{entry}");
        assert!(built.plan().notes.is_empty());
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn an_existing_file_is_kept_unless_replace_was_chosen_and_numbering_keeps_both() {
        let (mut lib, vol, ids) = volume("conflict", 2);
        let cfg = temp_dir("conflict-cfg");
        let out = temp_dir("conflict-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0]]).unwrap();
        let b = lib.create_case_with_tracks("Beta", &[ids[1]]).unwrap();
        std::fs::write(out.join("Alpha.m3u8"), "MINE").unwrap();

        // skip (the default): the file stays as it is, the other one is written
        let built = build(&lib, &cfg, &root, &req(&out, "m3u8", vec![a, b]), 0).unwrap();
        assert_eq!(built.plan().lists.iter().map(|l| l.action).collect::<Vec<_>>(), [Action::Skip, Action::Write]);
        let o = run(&lib, &built);
        assert_eq!((o.lists_written, o.kept), (1, 1));
        assert_eq!(std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap(), "MINE");

        // number: both kept
        let mut r = req(&out, "m3u8", vec![a, b]);
        r.conflict = Conflict::Number;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        let plan = built.plan();
        assert_eq!(plan.lists.iter().map(|l| (l.file.clone(), l.action, l.wanted.clone())).collect::<Vec<_>>(), [
            ("Alpha (2).m3u8".to_string(), Action::Renamed, Some("Alpha.m3u8".to_string())),
            ("Beta (2).m3u8".to_string(), Action::Renamed, Some("Beta.m3u8".to_string())),
        ]);
        run(&lib, &built);
        assert_eq!(std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap(), "MINE");
        assert!(out.join("Alpha (2).m3u8").is_file() && out.join("Beta.m3u8").is_file());
        // a third time finds (2) taken and uses (3)
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert_eq!(built.plan().lists[0].file, "Alpha (3).m3u8");

        // replace: only now is the file written over, and the plan said so
        let mut r = req(&out, "m3u8", vec![a]);
        r.conflict = Conflict::Replace;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert_eq!(built.plan().lists[0].action, Action::Replace);
        assert_eq!(std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap(), "MINE", "planning changes nothing");
        run(&lib, &built);
        assert!(std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap().starts_with("#EXTM3U"));
        assert!(files(&out).iter().all(|f| !f.ends_with(".part")));
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_file_that_appears_after_the_preview_is_not_written_over() {
        let (mut lib, vol, ids) = volume("race", 1);
        let cfg = temp_dir("race-cfg");
        let out = temp_dir("race-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.copy_audio = true;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert!(built.plan().lists[0].action == Action::Write && built.plan().audio[0].action == Action::Write);
        // after the preview, the user (or another program) puts files in the way
        std::fs::write(out.join("Alpha.m3u8"), "MINE").unwrap();
        std::fs::create_dir_all(out.join("Audio")).unwrap();
        let audio_path = PathBuf::from(&built.plan().audio[0].path);
        std::fs::write(&audio_path, "MINE TOO").unwrap();
        let o = run(&lib, &built);
        assert_eq!((o.lists_written, o.audio_copied, o.failed.len()), (0, 0, 2), "{o:?}");
        assert!(o.failed.iter().all(|f| f.contains("appeared after the preview")), "{:?}", o.failed);
        assert_eq!(std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap(), "MINE");
        assert_eq!(std::fs::read_to_string(&audio_path).unwrap(), "MINE TOO");
        assert!(files(&out).iter().all(|f| !f.ends_with(".part")));
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn an_audio_file_of_another_size_follows_the_policy_and_a_folder_in_the_way_is_blocked() {
        let (mut lib, vol, ids) = volume("audioconf", 2);
        let cfg = temp_dir("audioconf-cfg");
        let out = temp_dir("audioconf-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0], ids[1]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.copy_audio = true;
        r.relative_paths = true;
        let first = build(&lib, &cfg, &root, &r, 0).unwrap().plan();
        std::fs::create_dir_all(out.join("Audio")).unwrap();
        let (p0, p1) = (PathBuf::from(&first.audio[0].path), PathBuf::from(&first.audio[1].path));
        std::fs::write(&p0, "different").unwrap();
        std::fs::create_dir_all(&p1).unwrap();

        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert_eq!(built.plan().audio.iter().map(|x| x.action).collect::<Vec<_>>(), [Action::Skip, Action::Blocked]);
        let o = run(&lib, &built);
        assert_eq!((o.audio_copied, o.kept, o.failed.len(), o.lists_written), (0, 1, 1, 1), "{o:?}");
        assert_eq!(std::fs::read_to_string(&p0).unwrap(), "different", "kept");
        // the playlist does not claim the kept file or the blocked name is a copy: it points at the originals
        let text = std::fs::read_to_string(out.join("Alpha.m3u8")).unwrap();
        assert!(text.lines().filter(|l| !l.starts_with('#')).all(|l| l.starts_with("../")), "{text}");

        r.conflict = Conflict::Replace;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert_eq!(built.plan().audio.iter().map(|x| x.action).collect::<Vec<_>>(), [Action::Replace, Action::Blocked]);
        run(&lib, &built);
        assert_eq!(std::fs::read(&p0).unwrap().len(), 2048);
        assert!(p1.is_dir(), "never touched");

        r.conflict = Conflict::Number;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        // p0 now has the right size (Same); p1 is a folder (Blocked) whatever the policy
        assert_eq!(built.plan().audio.iter().map(|x| x.action).collect::<Vec<_>>(), [Action::Same, Action::Blocked]);
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_file_of_the_same_size_but_other_content_is_not_taken_for_the_copy() {
        let (mut lib, vol, ids) = volume("samesize", 1);
        let cfg = temp_dir("samesize-cfg");
        let out = temp_dir("samesize-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.copy_audio = true;
        let p0 = PathBuf::from(build(&lib, &cfg, &root, &r, 0).unwrap().plan().audio[0].path.clone());
        std::fs::create_dir_all(p0.parent().unwrap()).unwrap();

        // 2048 bytes (a file shorter than three blocks is compared whole), one byte different at the end
        let mut other = vec![1u8; 2048];
        *other.last_mut().unwrap() = 9;
        std::fs::write(&p0, &other).unwrap();
        for (policy, want) in [(Conflict::Skip, Action::Skip), (Conflict::Replace, Action::Replace), (Conflict::Number, Action::Renamed)] {
            r.conflict = policy;
            assert_eq!(build(&lib, &cfg, &root, &r, 0).unwrap().plan().audio[0].action, want, "{policy:?}: same size is not the same file");
        }

        // the real copy is recognised under every policy, even Replace
        std::fs::write(&p0, vec![1u8; 2048]).unwrap();
        for policy in [Conflict::Skip, Conflict::Replace, Conflict::Number] {
            r.conflict = policy;
            assert_eq!(build(&lib, &cfg, &root, &r, 0).unwrap().plan().audio[0].action, Action::Same, "{policy:?}");
        }

        // a big file is sampled at its start, middle and end: a difference in any of them shows
        let big = 3 * 64 * 1024 + 5000;
        let src = vol.join("big.bin");
        let mut data: Vec<u8> = (0..big).map(|i| (i % 251) as u8).collect();
        std::fs::write(&src, &data).unwrap();
        let dst = out.join("big-copy.bin");
        std::fs::write(&dst, &data).unwrap();
        assert!(looks_identical(&src, &dst, big as u64));
        for at in [3, big / 2 + 7, big - 2] {
            let was = data[at];
            data[at] ^= 0xff;
            std::fs::write(&dst, &data).unwrap();
            assert!(!looks_identical(&src, &dst, big as u64), "difference at {at}");
            data[at] = was;
        }
        assert!(!looks_identical(&src, &out.join("missing.bin"), big as u64), "unreadable counts as different");
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn the_plan_carries_a_fingerprint_that_changes_when_anything_it_shows_would_change() {
        let (mut lib, vol, ids) = volume("digest", 3);
        let cfg = temp_dir("digest-cfg");
        let out = temp_dir("digest-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0], ids[1]]).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.copy_audio = true;
        let first = build(&lib, &cfg, &root, &r, 123).unwrap().plan();
        assert_eq!(first.now_ms, 123, "the plan says which moment it was made for");
        assert_eq!(build(&lib, &cfg, &root, &r, 123).unwrap().plan().digest, first.digest, "same inputs, same fingerprint");

        // a file shows up in the folder: the action for that name changes
        std::fs::write(out.join("Alpha.m3u8"), "MINE").unwrap();
        let with_file = build(&lib, &cfg, &root, &r, 123).unwrap().plan();
        assert_ne!(with_file.digest, first.digest);
        std::fs::remove_file(out.join("Alpha.m3u8")).unwrap();
        assert_eq!(build(&lib, &cfg, &root, &r, 123).unwrap().plan().digest, first.digest, "and back again");

        // the case gets another track: same file names, different playlist
        lib.set_case_tracks(a, &[ids[0], ids[1], ids[2]]).unwrap();
        assert_ne!(build(&lib, &cfg, &root, &r, 123).unwrap().plan().digest, first.digest);
        lib.set_case_tracks(a, &[ids[1], ids[0]]).unwrap();
        let swapped = build(&lib, &cfg, &root, &r, 123).unwrap().plan().digest;
        lib.set_case_tracks(a, &[ids[0], ids[1]]).unwrap();
        assert_ne!(swapped, first.digest, "order counts");

        // an option changes what is written
        let mut other = r.clone();
        other.relative_paths = true;
        assert_ne!(build(&lib, &cfg, &root, &other, 123).unwrap().plan().digest, first.digest);
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_smart_case_with_a_date_rule_matches_against_the_moment_the_plan_was_made_for() {
        let (lib, vol, ids) = volume("moment", 2);
        let cfg = temp_dir("moment-cfg");
        let out = temp_dir("moment-out");
        let root = vol.to_string_lossy().into_owned();
        let sc = smart::save(&cfg, &root, SmartCase { id: 0, name: "Fresh".into(), query: Query { rules: vec![Rule::AddedWithin { days: 1 }], ..Default::default() }, on_device: false, device_case_id: None }).unwrap();
        let mut r = req(&out, "csv", vec![]);
        r.smart_ids = vec![sc.id];
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let day = 86_400_000;
        let at_preview = build(&lib, &cfg, &root, &r, now).unwrap().plan();
        assert_eq!(at_preview.lists[0].tracks, ids.len(), "added just now");
        // the job asked for the preview's moment gets the preview's list, however much later it starts
        let replay = build(&lib, &cfg, &root, &r, now).unwrap().plan();
        assert_eq!(replay.digest, at_preview.digest);
        // a clock three days on (a fresh now_ms) would match nothing: that is what the job no longer does
        let later = build(&lib, &cfg, &root, &r, now + 3 * day);
        assert!(later.is_err(), "no track is within a day any more, so nothing would be exported");
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn cancel_stops_before_the_next_file_and_leaves_no_partial_file() {
        let (mut lib, vol, ids) = volume("cancel", 3);
        let cfg = temp_dir("cancel-cfg");
        let out = temp_dir("cancel-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &ids).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.copy_audio = true;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        assert_eq!(built.writes(), 4);
        let cancel = AtomicBool::new(false);
        let o = execute(&lib, &built, &cancel, &mut |s| {
            if s.done == 2 {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        assert!(o.cancelled);
        assert_eq!((o.audio_copied, o.lists_written), (2, 0));
        assert!(o.summary("x").starts_with("Cancelled"));
        let listing = files(&out);
        assert_eq!(listing.len(), 2, "{listing:?}");
        assert!(listing.iter().all(|f| !f.ends_with(".part") && !f.ends_with(".m3u8")));

        // cancelled before it starts
        let o = execute(&lib, &built, &AtomicBool::new(true), &mut |_| panic!("no step"));
        assert!(o.cancelled && o.audio_copied == 0);

        // a cancel inside a copy removes the partial file
        let src = vol.join("big.bin");
        std::fs::write(&src, vec![7u8; 3 << 20]).unwrap();
        let dst = out.join("big copy.bin");
        assert!(matches!(copy_file(&src, &dst, false, &AtomicBool::new(true)), CopyEnd::Cancelled));
        assert!(!dst.exists() && !out.join("big copy.bin.part").exists());
        assert!(matches!(copy_file(&src, &dst, false, &AtomicBool::new(false)), CopyEnd::Done));
        assert_eq!(std::fs::metadata(&dst).unwrap().len(), 3 << 20);
        assert!(matches!(copy_file(&src, &dst, false, &AtomicBool::new(false)), CopyEnd::Failed(_)), "never over an existing file");
        assert!(matches!(copy_file(&src, &dst, true, &AtomicBool::new(false)), CopyEnd::Done));
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn a_missing_audio_file_is_reported_and_its_playlist_still_points_at_the_original() {
        let (mut lib, vol, ids) = volume("missing", 2);
        let cfg = temp_dir("missing-cfg");
        let out = temp_dir("missing-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &[ids[0], ids[1]]).unwrap();
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == ids[0]).unwrap().location;
        std::fs::remove_file(audio_source(&lib, &loc).unwrap()).unwrap();
        let mut r = req(&out, "m3u8", vec![a]);
        r.copy_audio = true;
        let built = build(&lib, &cfg, &root, &r, 0).unwrap();
        let plan = built.plan();
        assert_eq!(plan.audio.iter().map(|x| x.action).collect::<Vec<_>>(), [Action::Missing, Action::Write]);
        assert!(plan.audio[0].problem.as_deref().unwrap().contains("missing"));
        assert_eq!(plan.copy_bytes, 2049);
        let o = run(&lib, &built);
        assert_eq!((o.audio_copied, o.lists_written, o.failed.len()), (1, 1, 1), "{o:?}");
        assert!(o.failed[0].contains("Bulk Title 0"));
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    /// Rewrites the stored location of track `id` in `music.db` to a path that climbs out of the
    /// music folder, keeping the byte length so the file layout stays valid. (The library API
    /// never writes such a location; a hand-edited or foreign database could hold one.)
    fn corrupt_location(vol: &Path, lib: Library, id: i64) -> String {
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap().location;
        drop(lib);
        let bad = format!("/pmdb_tracks/../{}", "x".repeat(loc.len() - 16));
        let db = pm_formats::library::db_path(vol);
        let bytes = std::fs::read(&db).unwrap();
        let (from, to) = (loc.as_bytes(), bad.as_bytes());
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        let mut hits = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(from) {
                out.extend_from_slice(to);
                i += from.len();
                hits += 1;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        assert!(hits > 0);
        std::fs::write(&db, out).unwrap();
        bad
    }

    #[test]
    fn a_track_whose_location_escapes_is_left_out_named_and_never_read_or_copied() {
        let (mut lib, vol, ids) = volume("escape", 3);
        let cfg = temp_dir("escape-cfg");
        let out = temp_dir("escape-out");
        let root = vol.to_string_lossy().into_owned();
        let a = lib.create_case_with_tracks("Alpha", &ids).unwrap();
        let b = lib.create_case_with_tracks("Only bad", &[ids[1]]).unwrap();
        let bad = corrupt_location(&vol, lib, ids[1]);
        let lib = Library::open_read_only(&vol).unwrap();
        for fmt in ["m3u8", "csv", "rekordbox"] {
            let dir = out.join(fmt);
            std::fs::create_dir_all(&dir).unwrap();
            let mut r = req(&dir, fmt, vec![a, b]);
            r.copy_audio = true;
            let built = build(&lib, &cfg, &root, &r, 0).unwrap();
            let plan = built.plan();
            assert_eq!(plan.lists.len(), 1, "the case holding only the bad track has nothing left");
            assert_eq!(plan.lists[0].tracks, 2);
            assert_eq!(plan.audio.len(), 2);
            assert!(plan.notes.iter().any(|n| n.contains("“Bulk Title 1”") && n.contains(&bad) && n.contains("“Alpha”")), "{:?}", plan.notes);
            assert!(plan.notes.iter().any(|n| n.contains("“Only bad”") && n.contains("no tracks")), "{:?}", plan.notes);
            let o = run(&lib, &built);
            assert_eq!((o.lists_written, o.audio_copied, o.failed.len()), (1, 2, 0), "{fmt}: {o:?}");
            let listing = files(&dir);
            let text = std::fs::read_to_string(dir.join(listing.iter().find(|f| !f.starts_with("Audio/")).unwrap())).unwrap();
            assert!(!text.contains("Bulk Title 1") && !text.contains("xxxx"), "{fmt}: the bad row appears nowhere");
        }
        drop(lib);
        for d in [vol, cfg, out] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn the_summary_says_what_happened_in_words() {
        let o = Outcome { lists_written: 1, audio_copied: 2, audio_same: 3, kept: 1, failed: vec!["x".into()], cancelled: false };
        assert_eq!(o.summary("D:\\Out"), "Exported: 1 playlist written, 2 audio files copied, 3 audio files already there, 1 existing file left alone, 1 failed (D:\\Out).");
        assert_eq!(Outcome::default().summary("f"), "Exported: 0 playlists written (f).");
    }
}
