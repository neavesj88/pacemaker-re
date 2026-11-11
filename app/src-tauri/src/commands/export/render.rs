//! Glue between the app and the `pm-mixer` engine: audio lookup through the library, a small
//! cache of decoded clips for repeated previews, destination checks and the two render entry
//! points the commands call. Nothing here touches Tauri, so it is unit-testable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pm_formats::library::Library;
use pm_formats::mix::{Mix, Track};
use pm_formats::ops_device::{self, VocalMode};
use pm_mixer::audio::Source;
use pm_mixer::engine::Resolver;
use pm_mixer::{Format, Options, PeqMode};
use serde::Serialize;

use crate::commands::mixes::doc::MixDoc;
use crate::error::{AppError, AppResult};

/// Longest preview window; the temp WAV of 15 minutes is about 160 MB.
pub const MAX_PREVIEW_SECS: f64 = 900.0;
/// Rotating preview files, so a new preview never overwrites the one the player may still be
/// streaming.
const PREVIEW_SLOTS: usize = 4;
/// Decoded clips kept between previews (a decoded 6-minute track is about 64 MB).
const CACHE_SLOTS: usize = 3;

/// What a render reports to the frontend (`{ path }` is the contract; the rest is extra).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderedAudio {
    pub path: String,
    pub duration_secs: f64,
    /// Clips that could not be rendered and were left out.
    pub warnings: Vec<String>,
    /// How many clips were left out.
    pub clips_failed: usize,
}

// ---------------------------------------------------------------------------------------------
// audio lookup
// ---------------------------------------------------------------------------------------------

type CacheEntry = (PathBuf, u64, Option<SystemTime>, Arc<Source>);

fn cache() -> &'static Mutex<Vec<CacheEntry>> {
    static C: Mutex<Vec<CacheEntry>> = Mutex::new(Vec::new());
    &C
}

/// Decoded audio of a file, from the cache when the file is unchanged.
fn load_cached(path: &Path) -> Result<Arc<Source>, pm_mixer::Error> {
    let meta = std::fs::metadata(path).map_err(|e| pm_mixer::Error::Decode(format!("{}: {e}", path.display())))?;
    let (len, mtime) = (meta.len(), meta.modified().ok());
    if let Ok(c) = cache().lock()
        && let Some(e) = c.iter().find(|e| e.0 == path && e.1 == len && e.2 == mtime)
    {
        return Ok(e.3.clone());
    }
    let src = pm_mixer::audio::load(path)?;
    if let Ok(mut c) = cache().lock() {
        c.retain(|e| e.0 != path);
        c.push((path.to_path_buf(), len, mtime, src.clone()));
        while c.len() > CACHE_SLOTS {
            c.remove(0);
        }
    }
    Ok(src)
}

/// Finds each clip's audio through the library (`mixtracks.libraryID` -> `tracks.location` ->
/// file on the volume), never through the `FilePath` stored in the document.
pub struct LibResolver {
    paths: HashMap<i64, PathBuf>,
}

impl LibResolver {
    pub fn open(root: &Path, mix: &Mix) -> AppResult<LibResolver> {
        let lib = Library::open_read_only(root)?;
        let ids: Vec<i64> = mix.library_ids();
        let mut paths = HashMap::new();
        for s in lib.mix_track_sources(&ids)? {
            if let Some((audio, _)) = lib.track_paths(&s.location)
                && audio.is_file()
            {
                paths.insert(s.track_id, audio);
            }
        }
        Ok(LibResolver { paths })
    }

    #[cfg(test)]
    pub fn from_paths(paths: HashMap<i64, PathBuf>) -> LibResolver {
        LibResolver { paths }
    }
}

impl Resolver for LibResolver {
    fn source(&self, track: &Track) -> Result<Arc<Source>, pm_mixer::Error> {
        match self.paths.get(&track.library_id) {
            Some(p) => load_cached(p),
            None if track.library_id < 0 => Err(pm_mixer::Error::Invalid("the track was removed from the library".into())),
            None => Err(pm_mixer::Error::Invalid("the audio file is not on the volume".into())),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// entry points
// ---------------------------------------------------------------------------------------------

fn mixer_err(e: pm_mixer::Error) -> AppError {
    AppError::from(e.to_string())
}

/// What the `Wah` tool renders as for this library: the Vocal filter when the card is set up for
/// it (`ops_device::read_vocal`, `effective` so the kill switch counts), else the stock Wah
/// stand-in. A preview and an export both ask here, so they always sound the same for the same
/// card. An unreadable flag falls back to the stock Wah.
pub fn peq_mode(root: &Path) -> PeqMode {
    match ops_device::read_vocal(root).map(|v| v.effective) {
        Ok(VocalMode::Vocal) => PeqMode::Vocal,
        Ok(VocalMode::Inverted) => PeqMode::VocalInverted,
        Ok(VocalMode::Off) | Err(_) => PeqMode::Wah,
    }
}

const PREVIEW_DIR_PREFIX: &str = "pacemaker-revival-preview-";

/// Deletes preview directories left behind by earlier runs (untouched for a day; a running
/// instance rewrites its files far more often than that).
fn remove_stale_previews(tmp: &Path) {
    let Ok(rd) = std::fs::read_dir(tmp) else { return };
    for e in rd.flatten() {
        if !e.file_name().to_string_lossy().starts_with(PREVIEW_DIR_PREFIX) {
            continue;
        }
        // a directory's own mtime does not move when its files are overwritten, so look at those too
        let files = std::fs::read_dir(e.path()).into_iter().flatten().flatten().map(|c| c.path());
        let newest = std::iter::once(e.path()).chain(files).filter_map(|p| p.metadata().and_then(|m| m.modified()).ok()).max();
        if newest.and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 24 * 3600) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Where the next preview goes (rotates over [`PREVIEW_SLOTS`] files in the temp directory).
pub fn next_preview_path() -> AppResult<PathBuf> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    static CLEANED: std::sync::Once = std::sync::Once::new();
    let tmp = std::env::temp_dir();
    // one directory per process, so two running instances never overwrite each other's slots
    let dir = tmp.join(format!("{PREVIEW_DIR_PREFIX}{}", std::process::id()));
    CLEANED.call_once(|| remove_stale_previews(&tmp));
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("mix-preview-{}.wav", NEXT.fetch_add(1, Ordering::Relaxed) % PREVIEW_SLOTS)))
}

/// Renders `[from_sec, to_sec)` of the timeline to a WAV at `dest`.
pub fn render_preview(root: &Path, doc: &MixDoc, from_sec: f64, to_sec: f64, dest: &Path) -> AppResult<RenderedAudio> {
    if !from_sec.is_finite() || !to_sec.is_finite() || from_sec < 0.0 || to_sec <= from_sec {
        return Err("The preview range is empty.".into());
    }
    if to_sec - from_sec > MAX_PREVIEW_SECS {
        return Err(format!("A preview can be at most {} minutes; export the mix to hear more.", MAX_PREVIEW_SECS / 60.0).into());
    }
    let mix = doc.to_mix()?;
    let res = LibResolver::open(root, &mix)?;
    let opts = Options { peq: peq_mode(root), ..Options::window_secs(from_sec, to_sec) };
    let out = pm_mixer::render_to_file(&mix, &res, &opts, Format::Wav, dest, &mut |_| true).map_err(mixer_err)?;
    Ok(RenderedAudio { path: dest.to_string_lossy().into_owned(), duration_secs: out.frames as f64 / f64::from(pm_mixer::RATE), warnings: out.warnings, clips_failed: out.clips_failed })
}

/// The export format named by the frontend.
pub fn parse_format(s: &str) -> AppResult<Format> {
    match s.to_ascii_lowercase().as_str() {
        "ogg" => Ok(Format::Ogg),
        "wav" => Ok(Format::Wav),
        other => Err(format!("Unknown audio format '{other}' (use ogg or wav).").into()),
    }
}

/// Refuses destinations that would put a file inside the device volume (untracked audio there
/// would confuse the device) or that cannot be written.
pub fn check_dest(root: &Path, dest: &Path, format: Format) -> AppResult<()> {
    if dest.is_dir() {
        return Err("The destination is a folder; choose a file name.".into());
    }
    let want = format.extension();
    if !dest.extension().is_some_and(|e| e.eq_ignore_ascii_case(want)) {
        return Err(format!("The file name must end in .{want} to match the chosen format.").into());
    }
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|_| AppError::from("The destination folder does not exist."))?;
    if let Ok(r) = root.canonicalize()
        && parent.starts_with(&r)
    {
        return Err("Choose a folder outside the device: files written there would not be in its library.".into());
    }
    Ok(())
}

/// Everything about an export that can fail fast, checked before the job starts.
pub struct ExportPlan {
    pub mix: Mix,
    pub res: LibResolver,
    pub format: Format,
    pub dest: PathBuf,
    /// What `Wah` tools render as, read from the card when the export was planned.
    pub peq: PeqMode,
}

impl ExportPlan {
    /// Seconds of audio the export renders: from the first audible frame to the end of the last
    /// audible clip (what the engine does with `skip_leading_silence`), so progress reaches its
    /// total.
    pub fn span_secs(&self) -> f64 {
        let audible = self.mix.tracks.iter().filter(|t| !t.clip_bypassed).map(|t| (t.position.start + t.position.l_offset, t.position.start + t.position.length - t.position.r_offset)).filter(|(a, b)| b > a);
        let (lo, hi) = audible.fold((f64::MAX, 0.0_f64), |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        if hi > lo { (hi - lo) / f64::from(pm_mixer::RATE) } else { 0.0 }
    }
}

pub fn plan_export(root: &Path, doc: &MixDoc, dest: &str, format: &str) -> AppResult<ExportPlan> {
    let format = parse_format(format)?;
    let dest = PathBuf::from(dest);
    check_dest(root, &dest, format)?;
    let mix = doc.to_mix()?;
    let res = LibResolver::open(root, &mix)?;
    Ok(ExportPlan { mix, res, format, dest, peq: peq_mode(root) })
}

/// Renders the whole mix to the planned file (starting at the first audible frame). `progress`
/// gets 0..1 and returns `false` to cancel.
pub fn run_export(plan: &ExportPlan, progress: &mut dyn FnMut(f64) -> bool) -> AppResult<RenderedAudio> {
    let opts = Options { skip_leading_silence: true, peq: plan.peq, ..Options::default() };
    let out = pm_mixer::render_to_file(&plan.mix, &plan.res, &opts, plan.format, &plan.dest, progress).map_err(mixer_err)?;
    Ok(RenderedAudio { path: plan.dest.to_string_lossy().into_owned(), duration_secs: out.frames as f64 / f64::from(pm_mixer::RATE), warnings: out.warnings, clips_failed: out.clips_failed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::mix::Position;
    use pm_formats::strfile;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmrev-export-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 16-bit stereo WAV of a sine.
    fn write_wav(path: &Path, secs: f32, freq: f32) -> usize {
        let n = (secs * 44100.0) as usize;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n as u32 * 4).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&44100u32.to_le_bytes());
        b.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n as u32 * 4).to_le_bytes());
        for i in 0..n {
            let v = ((2.0 * std::f32::consts::PI * freq * i as f32 / 44100.0).sin() * 12000.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
        n
    }

    /// A temp volume with one synthetic track; returns (volume, scratch dir, track id, frames).
    fn library(tag: &str) -> (PathBuf, PathBuf, i64, usize) {
        let vol = temp(&format!("{tag}-vol"));
        let scratch = temp(&format!("{tag}-src"));
        let mut lib = Library::create(&vol).unwrap();
        let f = scratch.join("synthetic tone.wav");
        let frames = write_wav(&f, 6.0, 440.0);
        let a = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|k| strfile::Beat { pos: (k as f64 * 0.5 * 44100.0) as i64, bpm: 120.0 }).collect(),
            first_downbeat: Some(0),
        };
        let meta = TrackMeta { title: "Tone".into(), format: "WAV".into(), sample_rate: 44100, play_time_secs: 6, ..Default::default() };
        let id = lib.add_track(&f, &meta, &a).unwrap();
        (vol, scratch, id, frames)
    }

    fn doc_for(track_id: i64, frames: usize, speed: f32) -> MixDoc {
        let pos = Position { start: 0.0, length: frames as f64 / f64::from(speed), limit_size: true, session_event_id: "a".repeat(32), ..Position::default() };
        let mut mix = Mix::new("Export Test");
        mix.tracks.push(Track::new(track_id, "/pmdb_tracks/unused", 120.0, speed, pos));
        MixDoc::from_mix(7, &mix)
    }

    #[test]
    fn preview_and_export_use_the_library_audio() {
        let (vol, scratch, id, frames) = library("e2e");
        let doc = doc_for(id, frames, 1.0);

        let dest = scratch.join("preview.wav");
        let r = render_preview(&vol, &doc, 1.0, 3.0, &dest).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!((r.duration_secs - 2.0).abs() < 0.001);
        let back = pm_mixer::audio::load(&dest).unwrap();
        assert_eq!(back.len(), 2 * 44100);
        let rms = (back.l.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / back.len() as f64).sqrt();
        assert!((rms - 12000.0 / 32768.0 / 2f64.sqrt()).abs() < 0.01, "{rms}");

        // stretched export to Ogg, through the same plan the command uses
        let doc = doc_for(id, frames, 1.25);
        let out = scratch.join("mix.ogg");
        let plan = plan_export(&vol, &doc, out.to_str().unwrap(), "ogg").unwrap();
        let mut last = 0.0;
        let r = run_export(&plan, &mut |p| {
            last = p;
            true
        })
        .unwrap();
        assert!((last - 1.0).abs() < 1e-9);
        assert!((r.duration_secs - 6.0 / 1.25).abs() < 0.01);
        assert!(out.is_file() && std::fs::metadata(&out).unwrap().len() > 1000);

        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// The Wah tool sounds like the card says: stock Wah without the flag, the Vocal filter with
    /// it (inverted with `i`), stock again under the kill switch, and a preview of a window sounds
    /// like the same window of the export.
    #[test]
    fn preview_and_export_follow_the_cards_vocal_flag() {
        use pm_formats::mix::{Effect, Envelope, Process, Tool};
        let (vol, scratch, id, frames) = library("vocal");
        let pos = Position { start: 0.0, length: frames as f64, limit_size: true, session_event_id: "a".repeat(32), ..Position::default() };
        let mut t = Track::new(id, "/pmdb_tracks/unused", 120.0, 1.0, pos);
        let len = 3.0 * 44100.0;
        let mut tool = Tool::flat(Effect::wah(), 44100.0, len, 0.0, "t".into());
        tool.automation.insert("Wah-CutOff".into(), Envelope::flat(len, -100.0));
        t.processes.push(Process::Tool(tool));
        let mut mix = Mix::new("Vocal Test");
        mix.tracks.push(t);
        let doc = MixDoc::from_mix(7, &mix);

        let flag = vol.join(".Pacemaker").join("revival-vocal");
        let kill = vol.join(".Pacemaker").join("revival-no-preload");
        let rms_db = |v: &[f32]| 20.0 * (v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / v.len() as f64).sqrt().max(1e-9).log10();
        // (preview level, export level) over the same stretch of the tool, 3.0 s to 3.5 s
        let levels = |tag: &str| -> (f64, f64) {
            let pv = scratch.join(format!("pv-{tag}.wav"));
            render_preview(&vol, &doc, 2.0, 3.5, &pv).unwrap();
            let p = pm_mixer::audio::load(&pv).unwrap();
            let ex = scratch.join(format!("ex-{tag}.wav"));
            let plan = plan_export(&vol, &doc, ex.to_str().unwrap(), "wav").unwrap();
            run_export(&plan, &mut |_| true).unwrap();
            let e = pm_mixer::audio::load(&ex).unwrap();
            (rms_db(&p.l[44100..]), rms_db(&e.l[3 * 44100..(3.5 * 44100.0) as usize]))
        };
        let near = |a: f64, b: f64| (a - b).abs() < 0.3;

        let _ = std::fs::remove_file(&flag);
        assert_eq!(peq_mode(&vol), PeqMode::Wah);
        let (pv_wah, ex_wah) = levels("wah");
        assert!(near(pv_wah, ex_wah), "{pv_wah} vs {ex_wah}");

        std::fs::write(&flag, b"").unwrap();
        assert_eq!(peq_mode(&vol), PeqMode::Vocal);
        let (pv_voc, ex_voc) = levels("vocal");
        assert!(near(pv_voc, ex_voc), "preview {pv_voc} vs export {ex_voc}");
        assert!(pv_voc < pv_wah - 25.0, "vocal cut {pv_voc} dB should be far below the stock wah {pv_wah} dB");

        std::fs::write(&flag, b"i").unwrap();
        assert_eq!(peq_mode(&vol), PeqMode::VocalInverted);
        let (pv_inv, ex_inv) = levels("inverted");
        assert!(near(pv_inv, ex_inv), "{pv_inv} vs {ex_inv}");
        assert!(pv_inv > pv_voc + 30.0, "inverted -100 keeps the centre: {pv_inv} vs {pv_voc}");

        // the kill switch wins: stock Wah, the same sound as with no flag
        std::fs::write(&kill, b"").unwrap();
        assert_eq!(peq_mode(&vol), PeqMode::Wah);
        let (pv_kill, ex_kill) = levels("kill");
        assert!(near(pv_kill, pv_wah) && near(ex_kill, ex_wah), "{pv_kill} {ex_kill} vs {pv_wah} {ex_wah}");

        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn a_cancelled_export_stops_early_and_leaves_no_file() {
        let (vol, scratch, id, frames) = library("cancel");
        let doc = doc_for(id, frames, 1.0);
        let out = scratch.join("cancelled.wav");
        let plan = plan_export(&vol, &doc, out.to_str().unwrap(), "wav").unwrap();
        let mut calls = 0;
        let err = run_export(&plan, &mut |_| {
            calls += 1;
            calls < 2 // cancel at the second progress report
        })
        .unwrap_err();
        assert!(err.to_string().contains("cancelled"), "{err}");
        assert_eq!(calls, 2, "no more rendering after the cancel");
        assert!(!out.exists(), "no half-written file under the real name");
        let mut left: Vec<_> = std::fs::read_dir(&scratch).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.retain(|n| n.ends_with(".partial"));
        assert!(left.is_empty(), "{left:?}");
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn bad_requests_are_refused_before_rendering() {
        let (vol, scratch, id, frames) = library("bad");
        let doc = doc_for(id, frames, 1.0);
        let dest = scratch.join("x.wav");
        assert!(render_preview(&vol, &doc, 3.0, 2.0, &dest).is_err());
        assert!(render_preview(&vol, &doc, 0.0, 5000.0, &dest).is_err());
        assert!(render_preview(&vol, &doc, f64::NAN, 5.0, &dest).is_err());
        assert!(!dest.exists());
        assert!(parse_format("mp3").is_err());
        assert_eq!(parse_format("OGG").unwrap(), Format::Ogg);
        // never inside the device volume, never over a folder, folder must exist
        let inside = vol.join("Music").join("x.ogg");
        assert!(plan_export(&vol, &doc, inside.to_str().unwrap(), "ogg").is_err());
        assert!(plan_export(&vol, &doc, scratch.to_str().unwrap(), "ogg").is_err());
        assert!(plan_export(&vol, &doc, scratch.join("nope").join("x.ogg").to_str().unwrap(), "ogg").is_err());
        assert!(plan_export(&vol, &doc, dest.to_str().unwrap(), "wav").is_ok());
        // the extension must match the format (case does not matter)
        assert!(plan_export(&vol, &doc, dest.to_str().unwrap(), "ogg").is_err());
        assert!(plan_export(&vol, &doc, scratch.join("noext").to_str().unwrap(), "ogg").is_err());
        assert!(plan_export(&vol, &doc, scratch.join("X.OGG").to_str().unwrap(), "ogg").is_ok());
        let plan = plan_export(&vol, &doc, dest.to_str().unwrap(), "wav").unwrap();
        assert!((plan.span_secs() - 6.0).abs() < 0.01, "{}", plan.span_secs());
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn a_removed_track_is_a_warning_and_the_rest_still_renders() {
        let (vol, scratch, id, frames) = library("removed");
        let mut doc = doc_for(id, frames, 1.0);
        let mut ghost = doc.clone();
        ghost.clips[0].track_id = -5;
        ghost.clips[0].id = "b".repeat(32);
        doc.clips.push(ghost.clips.remove(0));
        doc.refresh();
        let dest = scratch.join("w.wav");
        let r = render_preview(&vol, &doc, 0.0, 2.0, &dest).unwrap();
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("removed from the library"), "{:?}", r.warnings);
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn preview_paths_rotate() {
        let a = next_preview_path().unwrap();
        let mut seen = std::collections::HashSet::new();
        seen.insert(a);
        for _ in 0..PREVIEW_SLOTS * 2 {
            seen.insert(next_preview_path().unwrap());
        }
        assert_eq!(seen.len(), PREVIEW_SLOTS);
    }

    #[test]
    fn resolver_maps_ids_without_the_library() {
        let d = temp("resolver");
        let f = d.join("t.wav");
        write_wav(&f, 1.0, 300.0);
        let r = LibResolver::from_paths([(3, f.clone())].into_iter().collect());
        let t = |id| Track::new(id, "x", 120.0, 1.0, Position::default());
        assert_eq!(r.source(&t(3)).unwrap().len(), 44100);
        assert!(r.source(&t(4)).is_err());
        assert!(r.source(&t(-1)).unwrap_err().to_string().contains("removed"));
        std::fs::remove_dir_all(d).unwrap();
    }
}
