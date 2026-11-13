//! Missing-file relocation: find the audio of tracks whose file is gone in a folder on the PC or
//! on the card, show each match with its evidence, then point the track at the file (already on
//! the card) or copy it onto the card first. The logic is in `pm_formats::ops_relink`; this file
//! is the command layer (tag reader, write gate, job lock, events) and the user-adjustable
//! settings of the feature.
//!
//! The search is read-only (no job lock, cancellable). The run is a device write job like the
//! clean-up: it holds a `WriteTicket` (refused during an eject or backup, gets the session
//! snapshot), takes a fresh snapshot of `music.db` before its first change, and re-checks every
//! pick on the fresh database with the same functions the preview used.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pm_formats::library::Library;
use pm_formats::ops_device::{self, SNAPSHOTS_KEPT};
use pm_formats::ops_relink::{self, Candidate, CardFiles, GridState, MatchOpts, Pick, Plan, ScanOpts, Tags};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::store::config_dir;
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{JobOutcome, Progress, spawn_job};
use crate::state::{JobLock, snapshots_dir};

/// The job kind of a relink run. It starts with "device-", so the device store shows its progress
/// and the Cancel button works.
pub const JOB_RELINK: &str = "device-relink";

/// Progress of the search (the run reports through the shared job events).
pub const RELINK_SCAN_PROGRESS: &str = "relink-scan-progress";

// ---- settings -------------------------------------------------------------------------------

/// Which matches start ticked.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Preselect {
    /// Nothing: every match is ticked by hand.
    None,
    /// Matches the evidence settles (same name and size, or same tags, length and size).
    #[default]
    Sure,
    /// Also matches that are probably right (same name with another size, tags with another size).
    Likely,
}

/// What the user can tune. Stored as JSON in the app's config dir (never on the device); a
/// missing or unreadable file means the defaults.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RelinkSettings {
    /// Also read title, artist and length from files that do not match by name.
    pub use_tags: bool,
    /// How far a file's length may differ from the track's, in seconds.
    pub duration_tolerance_secs: f64,
    /// Stop reading tags after this many files.
    pub max_tag_reads: u32,
    /// Search inside sub folders.
    pub subfolders: bool,
    /// Stop listing a folder after this many audio files.
    pub max_files: u32,
    /// A match already under Music: point at it where it is, or copy it to the standard name.
    pub card_files: CardFiles,
    pub preselect: Preselect,
    /// The folder searched last time.
    pub last_folder: String,
}

impl Default for RelinkSettings {
    fn default() -> Self {
        RelinkSettings {
            use_tags: true,
            duration_tolerance_secs: 2.0,
            max_tag_reads: 20_000,
            subfolders: true,
            max_files: 100_000,
            card_files: CardFiles::Reuse,
            preselect: Preselect::Sure,
            last_folder: String::new(),
        }
    }
}

impl RelinkSettings {
    /// Puts every value in its allowed range.
    pub fn clamped(mut self) -> Self {
        self.duration_tolerance_secs = if self.duration_tolerance_secs.is_finite() { self.duration_tolerance_secs.clamp(0.0, 60.0) } else { 2.0 };
        self.max_tag_reads = self.max_tag_reads.min(1_000_000);
        self.max_files = self.max_files.clamp(100, 1_000_000);
        self.last_folder = self.last_folder.trim().to_string();
        self
    }

    fn match_opts(&self) -> MatchOpts {
        MatchOpts {
            use_tags: self.use_tags,
            duration_tolerance_secs: self.duration_tolerance_secs,
            max_tag_reads: self.max_tag_reads as usize,
            card_files: self.card_files,
        }
    }

    fn scan_opts(&self) -> ScanOpts {
        ScanOpts { subfolders: self.subfolders, max_files: self.max_files as usize }
    }
}

const SETTINGS_FILE: &str = "relink.json";

/// Reads the settings file. A missing file is the defaults; an unreadable one is the defaults
/// plus a warning (the file is left as it is until the user saves new settings).
pub fn load_settings(dir: &Path) -> (RelinkSettings, Option<String>) {
    let p = dir.join(SETTINGS_FILE);
    match std::fs::read_to_string(&p) {
        Ok(s) => match serde_json::from_str::<RelinkSettings>(&s) {
            Ok(v) => (v.clamped(), None),
            Err(e) => (RelinkSettings::default(), Some(format!("{} could not be read ({e}); the defaults are in use", p.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (RelinkSettings::default(), None),
        Err(e) => (RelinkSettings::default(), Some(format!("{}: {e}; the defaults are in use", p.display()))),
    }
}

/// Writes the settings atomically (see `crate::store::write_atomic`).
pub fn save_settings(dir: &Path, s: &RelinkSettings) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    crate::store::write_atomic(&dir.join(SETTINGS_FILE), text.as_bytes())
}

#[derive(Serialize)]
pub struct SettingsView {
    pub settings: RelinkSettings,
    /// Set when the settings file could not be read and the defaults are showing.
    pub warning: Option<String>,
}

/// The relink settings.
#[tauri::command]
pub fn device_relink_settings(app: AppHandle) -> SettingsView {
    match config_dir(&app) {
        Ok(dir) => {
            let (settings, warning) = load_settings(&dir);
            SettingsView { settings, warning }
        }
        Err(e) => SettingsView { settings: RelinkSettings::default(), warning: Some(format!("the config folder is unavailable ({e}); the defaults are in use")) },
    }
}

/// Saves the settings (values are clamped to their range) and returns what is in force.
#[tauri::command]
pub fn device_relink_settings_set(app: AppHandle, settings: RelinkSettings) -> AppResult<SettingsView> {
    let settings = settings.clamped();
    save_settings(&config_dir(&app)?, &settings)?;
    Ok(SettingsView { settings, warning: None })
}

// ---- tags -----------------------------------------------------------------------------------

/// Title, artist and length of one file, without decoding it. A codec panic on a broken file
/// costs that file, not the search.
fn read_tags(path: &Path) -> Option<Tags> {
    let probe = catch_unwind(AssertUnwindSafe(|| pm_analysis::probe::probe(path))).ok()?.ok()?;
    Some(Tags { title: probe.title, artist: probe.artist, duration_secs: probe.duration_secs })
}

// ---- search ---------------------------------------------------------------------------------

/// Counts the searches; one whose number is no longer the latest stops (a new search, or Cancel).
static SCAN_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Clone)]
pub struct ScanProgress {
    /// 0: listing folders, 1: comparing files.
    pub phase: u8,
    pub done: usize,
    pub total: usize,
    pub label: String,
}

/// Searches `folder` for the audio of every track whose file is missing. Read-only. Emits
/// `relink-scan-progress` while it works; a stopped search returns an error starting
/// "Cancelled".
#[tauri::command]
pub async fn device_relink_scan(app: AppHandle, root: String, folder: String, settings: RelinkSettings) -> AppResult<Plan> {
    let settings = settings.clamped();
    let mine = SCAN_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Plan> {
        let lib = Library::open_read_only(Path::new(&root))?;
        let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let plan = ops_relink::plan(
            &lib,
            Path::new(folder.trim()),
            &settings.scan_opts(),
            &settings.match_opts(),
            &read_tags,
            &mut |phase, done, total, label| {
                // a few events a second are plenty
                if last.elapsed() >= std::time::Duration::from_millis(120) {
                    last = std::time::Instant::now();
                    events::emit(&app, RELINK_SCAN_PROGRESS, ScanProgress { phase, done, total, label: label.to_string() });
                }
            },
            &|| SCAN_GENERATION.load(Ordering::SeqCst) != mine,
        )?;
        Ok(plan)
    })
    .await?
}

/// Stops the running search.
#[tauri::command]
pub fn device_relink_scan_cancel() {
    SCAN_GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// One file the user picked for one track: the evidence for and against it and what taking it
/// would do. Read-only.
#[tauri::command]
pub async fn device_relink_inspect(root: String, track_id: i64, path: String, settings: RelinkSettings) -> AppResult<Candidate> {
    let settings = settings.clamped();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Candidate> {
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(ops_relink::inspect_pick(&lib, track_id, Path::new(&path), &settings.match_opts(), &read_tags)?)
    })
    .await?
}

// ---- run ------------------------------------------------------------------------------------

/// One ticked match, as the frontend sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct PickArg {
    pub track_id: i64,
    pub path: String,
    /// Chosen by hand rather than found by the search.
    pub manual: bool,
    /// The `new_location` the preview showed.
    pub expect_location: String,
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

fn mb(bytes: u64) -> String {
    super::mb(bytes)
}

/// The sentence a finished run is reported with.
fn summarise(rep: &ops_relink::RunReport) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !rep.relinked.is_empty() {
        let copied = rep.relinked.iter().filter(|d| d.copied).count();
        let mut s = format!("Relinked {}", plural(rep.relinked.len(), "track"));
        if copied > 0 {
            s.push_str(&format!(" ({} copied onto the card, {})", copied, mb(rep.copied_bytes)));
        }
        parts.push(s);
        let grid_copied = rep.relinked.iter().filter(|d| d.grid == GridState::Copied).count();
        if grid_copied > 0 {
            parts.push(format!("{} had its beat grid copied next to the new location", plural(grid_copied, "track")));
        }
        let no_grid: Vec<&str> = rep.relinked.iter().filter(|d| d.grid == GridState::Missing).map(|d| d.label.as_str()).collect();
        if !no_grid.is_empty() {
            parts.push(format!("{} without a beat grid, to analyse: {}", plural(no_grid.len(), "track"), no_grid.join("; ")));
        }
    }
    if rep.cancelled {
        parts.push("Stopped at your request; the rest were not touched".into());
    }
    if !rep.skipped.is_empty() {
        parts.push(format!("{} left alone: {}", plural(rep.skipped.len(), "track"), rep.skipped.join("; ")));
    }
    if !rep.failed.is_empty() {
        parts.push(format!("{} could not be relinked: {}", plural(rep.failed.len(), "track"), rep.failed.join("; ")));
    }
    if parts.is_empty() {
        parts.push("Nothing was relinked".into());
    }
    parts.join(". ")
}

/// Job `device-relink`: relinks the ticked tracks. Each pick is checked again on the fresh
/// database; a track whose audio is back, whose file no longer fits or whose outcome differs from
/// the preview is left alone and named. A fresh snapshot of `music.db` is saved before the first
/// change; Restore from a snapshot undoes the row changes (copied files stay, and show up under
/// unused files). Stopped with `device_cancel`: finished tracks stay finished.
#[tauri::command]
pub fn device_relink_run(app: AppHandle, lock: State<'_, JobLock>, root: String, picks: Vec<PickArg>, settings: RelinkSettings) -> AppResult<()> {
    if picks.is_empty() {
        return Err("No matches are ticked.".into());
    }
    let settings = settings.clamped();
    spawn_job(app, &lock, JOB_RELINK, move |app| {
        // relinking is a device write: refused during an eject, backup or firmware copy, and the
        // first write of the session gets its safety copy
        let _ticket = crate::state::begin_write(app, Path::new(&root))?;
        super::start_job_cancel(app);
        let picks: Vec<Pick> = picks
            .into_iter()
            .map(|p| Pick { track_id: p.track_id, path: PathBuf::from(p.path), manual: p.manual, expect_location: p.expect_location })
            .collect();
        let snap_dir = snapshots_dir(app, Path::new(&root))?;
        let mut lib = Library::open(Path::new(&root))?;
        let rep = ops_relink::run(
            &mut lib,
            &picks,
            &settings.match_opts(),
            &read_tags,
            // the session's first-write copy may be hours old: this run gets its own
            &|| ops_device::snapshot_library(Path::new(&root), &snap_dir, SNAPSHOTS_KEPT).map(|_| ()),
            &mut |done, total, label| {
                let status = if label.starts_with("Copying") { "copying" } else { "relinking" };
                events::emit(app, JOB_PROGRESS, Progress::new(JOB_RELINK, label, done, total, status));
            },
            &|| super::cancelled(app),
        )?;
        Ok(JobOutcome { ok: rep.relinked.len(), failed: rep.failed.len(), summary: summarise(&rep) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::ops_relink::{Done, RunReport};

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-relink-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn defaults_are_cautious() {
        let d = RelinkSettings::default();
        assert!(d.use_tags && d.subfolders);
        assert_eq!((d.preselect, d.card_files), (Preselect::Sure, CardFiles::Reuse));
        let o = d.match_opts();
        assert_eq!((o.use_tags, o.duration_tolerance_secs, o.card_files), (true, 2.0, CardFiles::Reuse));
    }

    #[test]
    fn values_are_clamped_and_junk_numbers_fall_back() {
        let c = RelinkSettings { duration_tolerance_secs: f64::NAN, max_files: 3, max_tag_reads: u32::MAX, last_folder: "  C:\\Music  ".into(), ..RelinkSettings::default() }.clamped();
        assert_eq!((c.duration_tolerance_secs, c.max_files, c.max_tag_reads), (2.0, 100, 1_000_000));
        assert_eq!(c.last_folder, "C:\\Music");
        let c = RelinkSettings { duration_tolerance_secs: -5.0, ..RelinkSettings::default() }.clamped();
        assert_eq!(c.duration_tolerance_secs, 0.0, "0 is a valid choice: lengths must agree to the second");
        let c = RelinkSettings { duration_tolerance_secs: 1e9, ..RelinkSettings::default() }.clamped();
        assert_eq!(c.duration_tolerance_secs, 60.0);
    }

    #[test]
    fn settings_round_trip_and_tolerate_a_missing_or_corrupt_file() {
        let dir = temp("settings");
        assert_eq!(load_settings(&dir), (RelinkSettings::default(), None), "no file: defaults, no warning");
        let s = RelinkSettings { use_tags: false, card_files: CardFiles::Standard, preselect: Preselect::Likely, last_folder: "D:\\Backups".into(), ..RelinkSettings::default() };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), (s.clone(), None));
        assert!(!dir.join("relink.json.tmp").exists(), "no temp file left behind");

        // an older file with fewer keys: the missing ones default
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"use_tags": false}"#).unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!((v.use_tags, v.duration_tolerance_secs, v.preselect), (false, 2.0, Preselect::Sure));
        assert!(w.is_none());

        // a corrupt file: defaults and a warning, and the file is left for the user to see
        std::fs::write(dir.join(SETTINGS_FILE), "not json").unwrap();
        let (v, w) = load_settings(&dir);
        assert_eq!(v, RelinkSettings::default());
        assert!(w.unwrap().contains("could not be read"));
        assert_eq!(std::fs::read_to_string(dir.join(SETTINGS_FILE)).unwrap(), "not json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_wire_format_is_what_the_frontend_sends() {
        let json = serde_json::to_value(RelinkSettings::default()).unwrap();
        assert_eq!((json["card_files"].as_str(), json["preselect"].as_str()), (Some("reuse"), Some("sure")));
        let back: RelinkSettings = serde_json::from_value(json).unwrap();
        assert_eq!(back, RelinkSettings::default());
        let p: PickArg = serde_json::from_str(r#"{"track_id":3,"path":"C:\\x\\a.mp3","manual":false,"expect_location":"/pmdb_tracks/1/2/3.mp3"}"#).unwrap();
        assert_eq!((p.track_id, p.manual), (3, false));
        assert!(serde_json::from_str::<PickArg>(r#"{"track_id":3}"#).is_err(), "a missing field is an error, not a silent default");
    }

    #[test]
    fn the_summary_names_what_happened_to_every_track() {
        let done = |label: &str, copied: bool, grid: GridState| Done {
            track_id: 1,
            label: label.into(),
            from: "/pmdb_tracks/1/1/a.mp3".into(),
            to: "/pmdb_tracks/1/1/a.mp3".into(),
            copied,
            bytes: if copied { 5 << 20 } else { 0 },
            grid,
            row_changed: false,
        };
        assert_eq!(summarise(&RunReport::default()), "Nothing was relinked");
        let rep = RunReport {
            relinked: vec![done("A - One", true, GridState::InPlace), done("B - Two", false, GridState::Missing), done("C - Three", false, GridState::Copied)],
            skipped: vec!["D - Four: its audio is back".into()],
            failed: vec!["E - Five: disk full".into()],
            cancelled: true,
            copied_bytes: 5 << 20,
        };
        let s = summarise(&rep);
        assert!(s.starts_with("Relinked 3 tracks (1 copied onto the card, 5.0 MB)"), "{s}");
        assert!(s.contains("1 track had its beat grid copied"), "{s}");
        assert!(s.contains("1 track without a beat grid, to analyse: B - Two"), "{s}");
        assert!(s.contains("Stopped at your request"), "{s}");
        assert!(s.contains("1 track left alone: D - Four") && s.contains("1 track could not be relinked: E - Five"), "{s}");
    }

    /// A 16-bit mono PCM WAV of `secs` seconds of silence at 8 kHz.
    fn wav(secs: u32) -> Vec<u8> {
        let rate = 8000u32;
        let data = rate * 2 * secs;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        for x in [16u32, 0x0001_0001, rate, rate * 2, 0x0010_0002] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data.to_le_bytes());
        v.resize(v.len() + data as usize, 0);
        v
    }

    #[test]
    fn the_real_tag_reader_decides_the_length_check_of_a_same_name_match() {
        use pm_formats::library::TrackMeta;
        use pm_formats::ops_relink::{Confidence, Kind};
        let base = temp("real-probe");
        let vol = base.join("vol");
        let src = base.join("src");
        let pc = base.join("pc");
        for d in [&vol, &src, &pc] {
            std::fs::create_dir_all(d).unwrap();
        }
        // a 3 s track is added, then its audio file disappears from the card
        std::fs::write(src.join("synthetic.wav"), wav(3)).unwrap();
        let analysis = pm_formats::strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| pm_formats::strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let meta = TrackMeta { title: "Demo".into(), artist: "Example".into(), format: "WAV".into(), play_time_secs: 3, ..Default::default() };
        let id = lib.add_track(&src.join("synthetic.wav"), &meta, &analysis).unwrap();
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap().location;
        std::fs::remove_file(vol.join(".Pacemaker/Music").join(loc.trim_start_matches("/pmdb_tracks/"))).unwrap();

        // the copy on the PC has the same name and length but 40 trailing bytes more (a tag block)
        let mut longer = wav(3);
        longer.extend_from_slice(&[0u8; 40]);
        std::fs::write(pc.join("synthetic.wav"), &longer).unwrap();
        let o = RelinkSettings::default().match_opts();
        let plan = ops_relink::plan(&lib, &pc, &ScanOpts::default(), &o, &read_tags, &mut |_, _, _, _| {}, &|| false).unwrap();
        let c = &plan.rows[0].candidates[0];
        assert_eq!((c.kind, c.confidence), (Kind::SameName, Confidence::Likely), "{c:?}");
        assert!(c.notes.iter().any(|n| n.contains("length fits")), "{:?}", c.notes);
        assert!((c.duration_secs.unwrap() - 3.0).abs() < 0.05);

        // a 6 s file under the same name is flagged: the length does not fit
        std::fs::write(pc.join("synthetic.wav"), wav(6)).unwrap();
        let plan = ops_relink::plan(&lib, &pc, &ScanOpts::default(), &o, &read_tags, &mut |_, _, _, _| {}, &|| false).unwrap();
        assert_eq!(plan.rows[0].candidates[0].confidence, Confidence::Check);
        drop(lib); // Windows cannot delete music.db while it is open
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_broken_file_costs_its_tags_not_the_search() {
        let dir = temp("tags");
        let bad = dir.join("bad.mp3");
        std::fs::write(&bad, b"not audio").unwrap();
        assert!(read_tags(&bad).is_none());
        assert!(read_tags(&dir.join("nope.mp3")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
