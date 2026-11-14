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
use pm_mixer::{Encoding, Format, Options, PeqMode};
use serde::Serialize;

use super::encoding::{self, EncodingSpec, TagSpec};
use super::loudness::{self, LevelSettings, LoudnessReport, PreviewGains, Reading, Subject};
use super::range::{self, RangePrefs, RangeSpec, Resolved};
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
            // The validated helper: a stored location that climbs out of the music folder is
            // treated as "not on the volume", exactly as a missing file is.
            if let Ok(audio) = crate::commands::tracks::audio_in_volume(&lib, &s.location)
                && audio.is_file()
            {
                paths.insert(s.track_id, audio);
            }
        }
        Ok(LibResolver { paths })
    }

    /// The audio file of a library track on the volume, for callers that decode it themselves
    /// (the live player keeps its own, bigger, decode cache).
    pub fn path(&self, track_id: i64) -> Option<&Path> {
        self.paths.get(&track_id).map(PathBuf::as_path)
    }

    /// Whether the engine will find audio for this library track id (false for a removed track or
    /// a file that is not on the volume: the engine leaves such a clip out of the render).
    pub fn has(&self, track_id: i64) -> bool {
        self.paths.contains_key(&track_id)
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
/// it (`ops_device::read_vocal`, `effective`, so the kill switch and a firmware known to be older
/// than the vocal hook both count as off), else the stock Wah stand-in. A preview and an export
/// both ask here, so they always sound the same for the same card, and like the device plays it.
/// An unreadable flag falls back to the stock Wah, as the device does when it cannot open it.
pub fn peq_mode(root: &Path) -> PeqMode {
    match ops_device::read_vocal(root).map(|v| v.effective) {
        Ok(VocalMode::Vocal) => PeqMode::Vocal,
        Ok(VocalMode::Inverted) => PeqMode::VocalInverted,
        Ok(VocalMode::Off) | Err(_) => PeqMode::Wah,
    }
}

/// The Wah tools that render as the Vocal filter, from the ids the app marks as Vocal tools (the file
/// has no Vocal tool, so only the app knows). On a card whose Vocal flag is on, the card decides for every
/// Wah tool, as the device does, and the marks change nothing; on stock Wah only the marked tools are
/// vocal filters. The live player, the range preview and the export all ask here, so the file sounds like
/// the preview. `None` = every Wah tool follows `peq`.
pub fn vocal_tools_for(doc: &MixDoc, peq: PeqMode, marked: Option<&[String]>) -> Option<Arc<std::collections::HashSet<String>>> {
    if peq != PeqMode::Wah {
        return None;
    }
    marked.map(|ids| Arc::new(doc.tool_session_ids(ids)))
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
#[cfg(test)]
pub fn render_preview(root: &Path, doc: &MixDoc, from_sec: f64, to_sec: f64, dest: &Path) -> AppResult<RenderedAudio> {
    render_preview_with(root, doc, from_sec, to_sec, dest, None, None, None)
}

/// [`render_preview`] with the gains of a loudness analysis (per-clip levelling and master gain),
/// applied by the same engine options an export uses, so the preview plays at the export's level.
/// `edge_fade_ms` fades the window's cuts the way a partial export does (omit for plain playback).
/// `vocal_tool_ids`: the Wah tools the app marks as Vocal tools (see [`vocal_tools_for`]).
#[allow(clippy::too_many_arguments)]
pub fn render_preview_with(root: &Path, doc: &MixDoc, from_sec: f64, to_sec: f64, dest: &Path, gains: Option<&PreviewGains>, edge_fade_ms: Option<f64>, vocal_tool_ids: Option<&[String]>) -> AppResult<RenderedAudio> {
    if !from_sec.is_finite() || !to_sec.is_finite() || from_sec < 0.0 || to_sec <= from_sec {
        return Err("The preview range is empty.".into());
    }
    if to_sec - from_sec > MAX_PREVIEW_SECS {
        return Err(format!("A preview can be at most {} minutes; export the mix to hear more.", MAX_PREVIEW_SECS / 60.0).into());
    }
    let mix = doc.to_mix()?;
    let res = LibResolver::open(root, &mix)?;
    let peq = peq_mode(root);
    let mut opts = Options { peq, vocal_tools: vocal_tools_for(doc, peq, vocal_tool_ids), ..Options::window_secs(from_sec, to_sec) };
    if let Some(g) = gains {
        loudness::apply_gains(doc, g, &mut opts);
    }
    if let Some(ms) = edge_fade_ms {
        if !ms.is_finite() || !(0.0..=2000.0).contains(&ms) {
            return Err("The fade at the cuts must be between 0 and 2000 milliseconds.".into());
        }
        opts.edge_fade_frames = (ms / 1000.0 * f64::from(pm_mixer::RATE)).round() as i64;
    }
    let out = pm_mixer::render_to_file(&mix, &res, &opts, Format::Wav, dest, &mut |_| true).map_err(mixer_err)?;
    Ok(RenderedAudio { path: dest.to_string_lossy().into_owned(), duration_secs: out.frames as f64 / f64::from(pm_mixer::RATE), warnings: out.warnings, clips_failed: out.clips_failed })
}

/// The export format named by the frontend.
pub fn parse_format(s: &str) -> AppResult<Format> {
    match s.to_ascii_lowercase().as_str() {
        "ogg" => Ok(Format::Ogg),
        "wav" => Ok(Format::Wav),
        "flac" => Ok(Format::Flac),
        "mp3" => Ok(Format::Mp3),
        other => Err(format!("Unknown audio format '{other}' (use ogg, wav, flac or mp3).").into()),
    }
}

/// Refuses destinations that would put a file inside the device volume (untracked audio there
/// would confuse the device) or that cannot be written.
pub fn check_dest(root: &Path, dest: &Path, format: Format) -> AppResult<()> {
    check_dest_ext(root, dest, format.extension())
}

/// [`check_dest`] for any file type: `want` is the extension the name must end in (no dot).
pub fn check_dest_ext(root: &Path, dest: &Path, want: &str) -> AppResult<()> {
    if dest.is_dir() {
        return Err("The destination is a folder; choose a file name.".into());
    }
    if !dest.extension().is_some_and(|e| e.eq_ignore_ascii_case(want)) {
        return Err(format!("The file name must end in .{want} to match the chosen format.").into());
    }
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(|_| AppError::from("The destination folder does not exist."))?;
    if let Ok(r) = root.canonicalize()
        && parent.starts_with(&r)
    {
        return Err("Choose a folder outside the library: files written there would not be part of it.".into());
    }
    Ok(())
}

/// What the user asked of an export beyond the file name and format: which part of the mix, the
/// loudness settings, the format's own settings and the tags. Every field has a default (the whole
/// mix, no loudness processing, 192 kbit/s MP3 / 16-bit FLAC, no tags).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportRequest {
    pub range: RangeSpec,
    pub range_prefs: RangePrefs,
    pub levels: LevelSettings,
    pub encoding: EncodingSpec,
    pub tags: Option<TagSpec>,
    /// The Wah tools (document ids) the app marks as Vocal tools; `None` = the card alone decides.
    pub vocal_tool_ids: Option<Vec<String>>,
}

/// Everything about an export that can fail fast, checked before the job starts.
pub struct ExportPlan {
    pub mix: Mix,
    /// The editor's document the plan was made from (clip ids, for the loudness gains).
    pub doc: MixDoc,
    pub res: LibResolver,
    pub format: Format,
    /// The format with its settings and the tags to write (cover read, chapters built).
    pub encoding: Encoding,
    pub dest: PathBuf,
    /// What `Wah` tools render as, read from the card when the export was planned.
    pub peq: PeqMode,
    /// Which `Wah` tools are the Vocal filter, see [`vocal_tools_for`].
    pub vocal_tools: Option<Arc<std::collections::HashSet<String>>>,
    /// The part of the timeline to export; `None` = the whole mix from its first audible frame.
    pub range: Option<Resolved>,
    pub levels: LevelSettings,
}

impl ExportPlan {
    /// Seconds of audio the export renders: from the first audible frame to the end of the last
    /// audible clip (what the engine does with `skip_leading_silence`), or the chosen range cut to
    /// the end of the last clip, so progress reaches its total.
    pub fn span_secs(&self) -> f64 {
        let rate = f64::from(pm_mixer::RATE);
        let whole = pm_mixer::engine::export_span(&self.mix);
        match (&self.range, whole) {
            (Some(r), Some((_, end))) => (r.to.min(end) - r.from).max(0) as f64 / rate,
            (_, Some((from, to))) => (to - from) as f64 / rate,
            _ => 0.0,
        }
    }

    /// The range as the track list takes it (timeline frames), when this is a partial export.
    pub fn span(&self) -> Option<(i64, i64)> {
        self.range.as_ref().map(|r| (r.from, r.to))
    }
}

#[cfg(test)]
pub fn plan_export(root: &Path, doc: &MixDoc, dest: &str, format: &str) -> AppResult<ExportPlan> {
    plan_export_with(root, doc, dest, format, &ExportRequest::default())
}

pub fn plan_export_with(root: &Path, doc: &MixDoc, dest: &str, format: &str, req: &ExportRequest) -> AppResult<ExportPlan> {
    let format = parse_format(format)?;
    let dest = PathBuf::from(dest);
    check_dest(root, &dest, format)?;
    req.levels.validate()?;
    let mix = doc.to_mix()?;
    let range = range::resolve_mix(doc, &mix, &req.range, &req.range_prefs)?;
    let res = LibResolver::open(root, &mix)?;
    // the tags are made now, before any audio is rendered: an unreadable cover or a track list
    // that cannot be made stops the export here instead of after minutes of rendering
    let span = range.as_ref().map(|r| (r.from, r.to));
    let tags = match &req.tags {
        Some(spec) => Some(encoding::build_tags(spec, &doc.name, doc, &|id| res.has(id), span)?),
        None => None,
    };
    let encoding = encoding::build_encoding(format, &req.encoding, tags)?;
    let peq = peq_mode(root);
    let vocal_tools = vocal_tools_for(doc, peq, req.vocal_tool_ids.as_deref());
    Ok(ExportPlan { mix, doc: doc.clone(), res, format, encoding, dest, peq, vocal_tools, range, levels: req.levels.clone() })
}

/// An export that has finished: the file and what it measured.
pub struct Exported {
    pub audio: RenderedAudio,
    pub report: LoudnessReport,
}

/// Renders the planned export (the whole mix from its first audible frame, or the chosen range).
/// With loudness settings active it first runs the analysis passes (about half of `progress`),
/// then renders with the gains they found. `progress` gets 0..1 and returns `false` to cancel.
#[cfg(test)]
pub fn run_export(plan: &ExportPlan, progress: &mut dyn FnMut(f64) -> bool) -> AppResult<RenderedAudio> {
    Ok(run_export_full(plan, progress)?.audio)
}

/// [`run_export`] that also returns the loudness report (the analysis, when there was one, and the
/// loudness of the finished audio, measured before it is encoded).
pub fn run_export_full(plan: &ExportPlan, progress: &mut dyn FnMut(f64) -> bool) -> AppResult<Exported> {
    let subject = Subject { mix: &plan.mix, doc: &plan.doc, res: &plan.res, peq: plan.peq, vocal_tools: plan.vocal_tools.clone(), range: plan.range.as_ref() };
    let mut opts = Options { measure: true, ..subject.options() };
    let share = if plan.levels.active() { 0.5 } else { 0.0 };
    let mut report = if plan.levels.active() {
        let r = loudness::analyse(&subject, &plan.levels, "export", &mut |p| progress(p * share)).map_err(mixer_err)?;
        loudness::apply_gains(&plan.doc, &r.gains, &mut opts);
        r
    } else {
        LoudnessReport { source: "export".into(), ..LoudnessReport::default() }
    };
    let out = pm_mixer::render_to_file_with(&plan.mix, &plan.res, &opts, &plan.encoding, &plan.dest, &mut |p| progress(share + (1.0 - share) * p)).map_err(mixer_err)?;
    let done = out.loudness.map(|l| Reading::from(l.output));
    if report.from_secs == 0.0 && report.to_secs == 0.0 {
        report.from_secs = plan.range.as_ref().map(|r| r.from_secs).or_else(|| pm_mixer::engine::export_span(&plan.mix).map(|(f, _)| f as f64 / f64::from(pm_mixer::RATE))).unwrap_or(0.0);
        report.to_secs = report.from_secs + out.frames as f64 / f64::from(pm_mixer::RATE);
    }
    if let (Some(target), Some(r)) = (plan.levels.target_lufs, done.and_then(|d| d.lufs))
        && (r - target).abs() > 0.5
    {
        report.notes.push(format!("The file measures {r:.1} LUFS, not the {target:.1} LUFS target: {}.", if r < target { "the limiter held the peaks, or the boost limit applied" } else { "the cut limit applied" }));
    }
    report.result = done;
    Ok(Exported {
        audio: RenderedAudio { path: plan.dest.to_string_lossy().into_owned(), duration_secs: out.frames as f64 / f64::from(pm_mixer::RATE), warnings: out.warnings, clips_failed: out.clips_failed },
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::export::loudness::ClipLevel;
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
        write_wav_amp(path, secs, freq, 12000.0)
    }

    /// 16-bit stereo WAV of a sine with peak value `amp` (of 32767).
    fn write_wav_amp(path: &Path, secs: f32, freq: f32, amp: f32) -> usize {
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
            let v = ((2.0 * std::f32::consts::PI * freq * i as f32 / 44100.0).sin() * amp) as i16;
            b.extend_from_slice(&v.to_le_bytes());
            b.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
        n
    }

    /// A temp volume with two synthetic tracks of different level (peak 12000 and 3000 of 32767,
    /// 20 s each, 1 kHz); returns (volume, scratch dir, their ids).
    fn library_two(tag: &str) -> (PathBuf, PathBuf, [i64; 2]) {
        let vol = temp(&format!("{tag}-vol"));
        let scratch = temp(&format!("{tag}-src"));
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = [0i64; 2];
        for (k, amp) in [12000.0, 3000.0].into_iter().enumerate() {
            let f = scratch.join(format!("synthetic tone {k}.wav"));
            write_wav_amp(&f, 20.0, 1000.0, amp);
            let a = strfile::Analysis {
                normalize_factor: 1.0,
                beat_start_offset: 0,
                beats: (0..8).map(|k| strfile::Beat { pos: (k as f64 * 0.5 * 44100.0) as i64, bpm: 120.0 }).collect(),
                first_downbeat: Some(0),
            };
            let meta = TrackMeta { title: format!("Tone {k}"), format: "WAV".into(), sample_rate: 44100, play_time_secs: 20, ..Default::default() };
            ids[k] = lib.add_track(&f, &meta, &a).unwrap();
        }
        (vol, scratch, ids)
    }

    /// Two 20 s clips one after the other with a 4 s gap (clip ids "a..." and "b...").
    fn two_clip_doc(ids: [i64; 2]) -> MixDoc {
        let mut mix = Mix::new("Level Test");
        for (k, id) in ids.into_iter().enumerate() {
            let pos = Position { start: k as f64 * 24.0 * 44100.0, length: 20.0 * 44100.0, limit_size: true, session_event_id: format!("{:032}", k + 1), ..Position::default() };
            mix.tracks.push(Track::new(id, "/pmdb_tracks/unused", 120.0, 1.0, pos));
        }
        MixDoc::from_mix(7, &mix)
    }

    fn lufs_of(path: &Path, from_sec: f64, to_sec: f64) -> f64 {
        let a = pm_mixer::audio::load(path).unwrap();
        let (f, t) = ((from_sec * 44100.0) as usize, (to_sec * 44100.0) as usize);
        pm_mixer::loudness::measure(&a.l[f..t], &a.r[f..t], 44100).lufs.expect("a loudness")
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

        // a firmware older than the vocal hook ignores the flag, so the app renders the stock Wah too
        let sysxml = vol.join(".Pacemaker").join("system.xml");
        std::fs::write(&sysxml, br#"<SYSTEM FWVERSION="180096070"/>"#).unwrap();
        assert_eq!(peq_mode(&vol), PeqMode::Wah);
        std::fs::write(&sysxml, br#"<SYSTEM FWVERSION="180096073"/>"#).unwrap();
        assert_eq!(peq_mode(&vol), PeqMode::VocalInverted);
        std::fs::remove_file(&sysxml).unwrap();

        // the kill switch wins: stock Wah, the same sound as with no flag
        std::fs::write(&kill, b"").unwrap();
        assert_eq!(peq_mode(&vol), PeqMode::Wah);
        let (pv_kill, ex_kill) = levels("kill");
        assert!(near(pv_kill, pv_wah) && near(ex_kill, ex_wah), "{pv_kill} {ex_kill} vs {pv_wah} {ex_wah}");

        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// A Wah tool the app marks as a Vocal tool is the vocal filter in the live player, in a range preview and
    /// in the export alike (stock Wah card), and the card's own flag decides when it is on, whatever the marks.
    #[test]
    fn an_export_plays_vocal_marked_tools_as_the_preview_does() {
        use pm_formats::mix::{Effect, Envelope, Process, Tool};
        let (vol, scratch, id, frames) = library("vocalmark");
        let pos = Position { start: 0.0, length: frames as f64, limit_size: true, session_event_id: "a".repeat(32), ..Position::default() };
        let mut t = Track::new(id, "/pmdb_tracks/unused", 120.0, 1.0, pos);
        let len = 3.0 * 44100.0;
        let mut tool = Tool::flat(Effect::wah(), 44100.0, len, 0.0, "t".into());
        tool.automation.insert("Wah-CutOff".into(), Envelope::flat(len, -100.0));
        t.processes.push(Process::Tool(tool));
        let mut mix = Mix::new("Vocal Mark Test");
        mix.tracks.push(t);
        let doc = MixDoc::from_mix(7, &mix);
        let tool_id = doc.clips[0].processes.iter().find_map(|p| if let crate::commands::mixes::doc::ProcessDoc::Tool(x) = p { Some(x.id.clone()) } else { None }).unwrap();
        let rms_db = |v: &[f32]| 20.0 * (v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / v.len() as f64).sqrt().max(1e-9).log10();
        let export_level = |tag: &str, marked: Option<Vec<String>>| -> f64 {
            let ex = scratch.join(format!("ex-{tag}.wav"));
            let req = ExportRequest { vocal_tool_ids: marked, ..ExportRequest::default() };
            let plan = plan_export_with(&vol, &doc, ex.to_str().unwrap(), "wav", &req).unwrap();
            run_export(&plan, &mut |_| true).unwrap();
            let e = pm_mixer::audio::load(&ex).unwrap();
            rms_db(&e.l[3 * 44100..(3.5 * 44100.0) as usize])
        };
        let preview_level = |tag: &str, marked: Option<&[String]>| -> f64 {
            let pv = scratch.join(format!("pv-{tag}.wav"));
            render_preview_with(&vol, &doc, 2.0, 3.5, &pv, None, None, marked).unwrap();
            rms_db(&pm_mixer::audio::load(&pv).unwrap().l[44100..])
        };
        let flag = vol.join(".Pacemaker").join("revival-vocal");
        let _ = std::fs::remove_file(&flag);
        let marks = vec![tool_id];
        let wah = export_level("unmarked", None);
        let voc = export_level("marked", Some(marks.clone()));
        assert!(voc < wah - 25.0, "marked tool exports as the vocal cut: {voc} vs wah {wah}");
        assert!((preview_level("marked", Some(&marks)) - voc).abs() < 0.3, "the range preview sounds like the export");
        assert!((export_level("empty", Some(vec![])) - wah).abs() < 0.3, "no marks, stock Wah");
        // the card's flag on: the card decides for every Wah tool, the marks change nothing
        std::fs::write(&flag, b"").unwrap();
        let card = export_level("card", None);
        assert!((export_level("card-empty", Some(vec![])) - card).abs() < 0.3, "an empty mark list does not undo the card's Vocal filter");
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
        assert!(parse_format("m4a").is_err());
        assert_eq!(parse_format("OGG").unwrap(), Format::Ogg);
        assert_eq!((parse_format("Flac").unwrap(), parse_format("MP3").unwrap()), (Format::Flac, Format::Mp3));
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

    /// The track list says what the file contains: the second clip starts in the rendered audio
    /// where the list (and the CUE sheet written by an export) says, a clip the render leaves out
    /// is not listed, and the file is exactly as long as the list believes.
    #[test]
    fn tracklist_times_are_the_times_in_the_rendered_file() {
        use crate::commands::export::tracklist::{self, TracklistOptions};
        let (vol, scratch, id, frames) = library("tracklist");
        let mut doc = doc_for(id, frames, 1.0);
        doc.clips[0].span.start = 4.0 * 44100.0;
        let mut second = doc.clips[0].clone();
        second.id = "b".repeat(32);
        second.span.start = 20.0 * 44100.0;
        second.fuzzy_id.title = "Second".into();
        doc.clips[0].fuzzy_id.title = "First".into();
        // a clip whose track was removed from the library: silent, not listed, but the file
        // starts at it (the engine counts its slot), so every other time is relative to 1.0 s
        let mut ghost = doc.clips[0].clone();
        ghost.id = "c".repeat(32);
        ghost.track_id = -5;
        ghost.span.start = 1.0 * 44100.0;
        ghost.span.length = 1.0 * 44100.0;
        doc.clips.extend([second, ghost]);
        doc.refresh();

        let out = scratch.join("mix.wav");
        let plan = plan_export(&vol, &doc, out.to_str().unwrap(), "wav").unwrap();
        run_export(&plan, &mut |_| true).unwrap();
        let rendered = pm_mixer::audio::load(&out).unwrap();

        let opts = TracklistOptions::default();
        let list = tracklist::render_all(&doc, &opts, "mix.wav", &|id| plan.res.has(id)).unwrap();
        assert_eq!(list.entries.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["First", "Second"]);
        assert!(list.warnings.iter().any(|w| w.contains("left out")), "{:?}", list.warnings);
        assert_eq!(list.entries[0].start_frames, 3 * 44100, "4.0 s on the timeline, the file starts at 1.0 s");
        assert_eq!(list.entries[1].start_frames, 19 * 44100);
        assert_eq!(list.duration_secs, rendered.len() as f64 / 44100.0, "the list's length is the file's");
        // where the sound really starts in the file
        let onset = |from: usize| from + rendered.l[from..].iter().position(|x| x.abs() > 0.05).unwrap();
        let first_end = list.entries[0].start_frames as usize + 6 * 44100;
        assert!(onset(2 * 44100).abs_diff(list.entries[0].start_frames as usize) < 100);
        assert!(onset(first_end).abs_diff(list.entries[1].start_frames as usize) < 100, "{} vs {}", onset(first_end), list.entries[1].start_frames);

        // the sheet an export writes beside the audio is the one the dialog shows
        let (path, bytes) = crate::commands::export::plan_cue(&plan, &doc, &opts, false).unwrap();
        assert_eq!(path, scratch.join("mix.cue"));
        assert_eq!(bytes, list.file_bytes(tracklist::Format::Cue, &opts));
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("FILE \"mix.wav\" WAVE") && text.contains("INDEX 01 00:03:00") && text.contains("INDEX 01 00:19:00"), "{text}");
        std::fs::create_dir(scratch.join("folder.cue")).unwrap();
        let in_the_way = plan_export(&vol, &doc, scratch.join("folder.wav").to_str().unwrap(), "wav").unwrap();
        assert!(crate::commands::export::plan_cue(&in_the_way, &doc, &opts, true).unwrap_err().to_string().contains("is a folder"));
        // a sheet that is already there is never replaced unless the caller said so
        std::fs::write(&path, b"hand-edited").unwrap();
        let err = crate::commands::export::plan_cue(&plan, &doc, &opts, false).unwrap_err().to_string();
        assert!(err.contains("mix.cue is already there"), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), b"hand-edited", "planning must not touch the file");
        assert_eq!(crate::commands::export::plan_cue(&plan, &doc, &opts, true).unwrap().0, path);
        let target = crate::commands::export::export_cue_target(plan.dest.to_string_lossy().into_owned());
        assert!(target.exists && target.cue_path.ends_with("mix.cue"));
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// The target loudness is reached in the file itself, and the number the export reports is the
    /// number an independent measurement of the written file gives.
    #[test]
    fn an_export_with_a_target_reaches_it_and_reports_the_files_real_loudness() {
        let (vol, scratch, ids) = library_two("target");
        let doc = two_clip_doc(ids);
        let out = scratch.join("target.wav");
        let req = ExportRequest { levels: LevelSettings { target_lufs: Some(-20.0), ..LevelSettings::default() }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, out.to_str().unwrap(), "wav", &req).unwrap();
        let mut seen = Vec::new();
        let done = run_export_full(&plan, &mut |p| {
            seen.push(p);
            true
        })
        .unwrap();
        assert!(seen.windows(2).all(|w| w[1] >= w[0] - 1e-9), "progress never goes back across the passes");
        assert!((seen.last().unwrap() - 1.0).abs() < 1e-9);
        let r = &done.report;
        assert_eq!(r.source, "export");
        let measured = pm_mixer::audio::load(&out).unwrap();
        let file = pm_mixer::loudness::measure(&measured.l, &measured.r, 44100).lufs.unwrap();
        assert!((file + 20.0).abs() < 0.15, "the file measures {file} LUFS, wanted -20");
        let reported = r.result.unwrap().lufs.unwrap();
        assert!((reported - file).abs() < 0.1, "reported {reported}, file {file}");
        assert!((r.target_lufs.unwrap() + 20.0).abs() < 1e-9 && r.master_gain_db != 0.0);
        assert!(!r.limiter_will_act, "{:?}", r.notes);
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// A target the limiter cannot deliver is reported as missed, not claimed.
    #[test]
    fn a_target_beyond_what_the_limiter_allows_is_reported_as_missed() {
        let (vol, scratch, ids) = library_two("missed");
        let doc = two_clip_doc(ids);
        let out = scratch.join("missed.wav");
        // a 1 kHz sine at -0.2 dBFS peak is already near the ceiling; asking for 0 LUFS cannot be met
        let req = ExportRequest { levels: LevelSettings { target_lufs: Some(-1.0), max_boost_db: 40.0, ..LevelSettings::default() }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, out.to_str().unwrap(), "wav", &req).unwrap();
        let done = run_export_full(&plan, &mut |_| true).unwrap();
        let reached = done.report.result.unwrap().lufs.unwrap();
        assert!(reached < -1.0 - 0.5, "reached {reached}");
        assert!(done.report.limiter_will_act);
        assert!(done.report.notes.iter().any(|n| n.contains("The file measures") && n.contains("limiter")), "{:?}", done.report.notes);
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// Per-clip levelling brings a loud and a quiet clip together, in the file.
    #[test]
    fn clip_levelling_evens_out_the_clips_in_the_exported_file() {
        let (vol, scratch, ids) = library_two("levelling");
        let doc = two_clip_doc(ids);
        let plain = scratch.join("plain.wav");
        run_export(&plan_export(&vol, &doc, plain.to_str().unwrap(), "wav").unwrap(), &mut |_| true).unwrap();
        let gap_before = lufs_of(&plain, 1.0, 19.0) - lufs_of(&plain, 25.0, 43.0);
        assert!((gap_before - 12.04).abs() < 0.2, "the clips start {gap_before} LU apart");

        let out = scratch.join("levelled.wav");
        let req = ExportRequest { levels: LevelSettings { clip_level: ClipLevel::Median, ..LevelSettings::default() }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, out.to_str().unwrap(), "wav", &req).unwrap();
        let done = run_export_full(&plan, &mut |_| true).unwrap();
        let gap_after = lufs_of(&out, 1.0, 19.0) - lufs_of(&out, 25.0, 43.0);
        assert!(gap_after.abs() < 0.2, "after levelling the clips are {gap_after} LU apart");
        let rows = &done.report.clips;
        assert_eq!(rows.len(), 2);
        assert!((rows[0].gain_db + 6.02).abs() < 0.1 && (rows[1].gain_db - 6.02).abs() < 0.1, "{rows:?}");
        assert!(done.report.clip_reference_lufs.is_some());
        // no target, so the master gain stays at 0 dB
        assert_eq!(done.report.master_gain_db, 0.0);
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// Preview equals action: the gains an analysis finds, played through a range preview, sound
    /// like the same stretch of the export made with the same settings.
    #[test]
    fn a_preview_with_the_analysed_gains_matches_the_export() {
        let (vol, scratch, ids) = library_two("previewmatch");
        let doc = two_clip_doc(ids);
        let req = ExportRequest { levels: LevelSettings { target_lufs: Some(-16.0), clip_level: ClipLevel::Median, ..LevelSettings::default() }, ..ExportRequest::default() };

        // the dialog's analysis
        let mix = doc.to_mix().unwrap();
        let res = LibResolver::open(&vol, &mix).unwrap();
        let subject = Subject { mix: &mix, doc: &doc, res: &res, peq: peq_mode(&vol), vocal_tools: None, range: None };
        let analysis = loudness::analyse(&subject, &req.levels, "analysis", &mut |_| true).unwrap();
        assert_eq!(analysis.source, "analysis");

        // the preview of a stretch of the second clip, at those gains
        let pv = scratch.join("pv.wav");
        render_preview_with(&vol, &doc, 26.0, 30.0, &pv, Some(&analysis.gains), None, None).unwrap();
        // the export with the same settings
        let ex = scratch.join("ex.wav");
        let plan = plan_export_with(&vol, &doc, ex.to_str().unwrap(), "wav", &req).unwrap();
        let done = run_export_full(&plan, &mut |_| true).unwrap();
        assert_eq!(done.report.gains, analysis.gains, "the export finds the same gains as the analysis");
        // timeline 26 s is 2 s into the second clip; the export starts at timeline 0 here
        let p = pm_mixer::audio::load(&pv).unwrap();
        let e = pm_mixer::audio::load(&ex).unwrap();
        let rms = |v: &[f32]| 20.0 * (v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / v.len() as f64).sqrt().log10();
        let (pr, er) = (rms(&p.l[44100..3 * 44100]), rms(&e.l[27 * 44100..29 * 44100]));
        assert!((pr - er).abs() < 0.1, "preview {pr} dB vs export {er} dB");
        // and without the gains it would have been a different level
        let flat = scratch.join("flat.wav");
        render_preview_with(&vol, &doc, 26.0, 30.0, &flat, None, None, None).unwrap();
        let f = pm_mixer::audio::load(&flat).unwrap();
        assert!((rms(&f.l[44100..3 * 44100]) - er).abs() > 3.0, "the gains must make an audible difference in this fixture");
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    /// A partial export is that part of the mix: right length, right audio, right track list.
    #[test]
    fn a_partial_export_is_the_chosen_range_with_its_own_track_list() {
        use crate::commands::export::tracklist::{self, TracklistOptions};
        let (vol, scratch, ids) = library_two("partial");
        let doc = two_clip_doc(ids);
        let whole = scratch.join("whole.wav");
        run_export(&plan_export(&vol, &doc, whole.to_str().unwrap(), "wav").unwrap(), &mut |_| true).unwrap();
        let full = pm_mixer::audio::load(&whole).unwrap();

        // from clip b to clip b: 24 s .. 44 s on the timeline
        let part = scratch.join("part.wav");
        let req = ExportRequest { range: RangeSpec::Clips { from_clip: doc.clips[1].id.clone(), to_clip: doc.clips[1].id.clone() }, range_prefs: RangePrefs { edge_fade_ms: 0.0, ..RangePrefs::default() }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, part.to_str().unwrap(), "wav", &req).unwrap();
        assert!((plan.span_secs() - 20.0).abs() < 1e-9);
        let done = run_export_full(&plan, &mut |_| true).unwrap();
        assert!((done.audio.duration_secs - 20.0).abs() < 1e-6);
        let p = pm_mixer::audio::load(&part).unwrap();
        assert_eq!(p.len(), 20 * 44100);
        // the same audio as that stretch of the full export (the whole export starts at timeline 0 here)
        let off = 24 * 44100;
        let worst = (0..p.len()).map(|i| (p.l[i] - full.l[off + i]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1e-4, "the partial file differs from the same stretch of the full file by {worst}");

        // a time range inside the first clip, with the default 20 ms fade at both cuts
        let cut = scratch.join("cut.wav");
        let req = ExportRequest { range: RangeSpec::Time { from_sec: 2.0, to_sec: 6.0 }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, cut.to_str().unwrap(), "wav", &req).unwrap();
        run_export_full(&plan, &mut |_| true).unwrap();
        let c = pm_mixer::audio::load(&cut).unwrap();
        assert_eq!(c.len(), 4 * 44100);
        assert!(c.l[0].abs() < 0.01 && c.l[c.len() - 1].abs() < 0.01, "faded at the cuts");
        assert!((c.l[22050..22100].iter().map(|x| x.abs()).fold(0.0, f32::max) - full.l[2 * 44100 + 22050..2 * 44100 + 22100].iter().map(|x| x.abs()).fold(0.0, f32::max)).abs() < 1e-4, "untouched in the middle");

        // previewing the range with the same fade is the same samples as exporting it
        let pv = scratch.join("range-preview.wav");
        render_preview_with(&vol, &doc, 2.0, 6.0, &pv, None, Some(20.0), None).unwrap();
        let q = pm_mixer::audio::load(&pv).unwrap();
        assert_eq!(q.l, c.l, "a range preview with the export's fade is the export");
        assert!(render_preview_with(&vol, &doc, 2.0, 6.0, &pv, None, Some(-1.0), None).is_err());
        let plain = scratch.join("range-plain.wav");
        render_preview_with(&vol, &doc, 2.0, 6.0, &plain, None, None, None).unwrap();
        let head = |v: &[f32]| v[..441].iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        assert!(head(&pm_mixer::audio::load(&plain).unwrap().l) > head(&c.l) + 0.05, "no fade unless asked");

        // the CUE sheet of a partial export counts from the start of the range
        let doc2 = {
            let mut d = doc.clone();
            // make clip b start while a is still playing so the range from 10 s catches both
            d.clips[1].span.start = 15.0 * 44100.0;
            d.refresh();
            d
        };
        let req = ExportRequest { range: RangeSpec::Time { from_sec: 10.0, to_sec: 30.0 }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc2, scratch.join("listed.wav").to_str().unwrap(), "wav", &req).unwrap();
        assert_eq!(plan.span(), Some((10 * 44100, 30 * 44100)));
        let list = tracklist::render_all_in(&doc2, &TracklistOptions::default(), "listed.wav", &|id| plan.res.has(id), plan.span()).unwrap();
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0].start_frames, 0, "already playing when the range starts");
        assert_eq!(list.entries[1].start_frames, 5 * 44100, "15 s on the timeline, the file starts at 10 s");
        assert_eq!(list.duration_secs, 20.0);
        // a range that hears only the first clip lists only it
        let req = ExportRequest { range: RangeSpec::Time { from_sec: 2.0, to_sec: 6.0 }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, scratch.join("x.wav").to_str().unwrap(), "wav", &req).unwrap();
        let list = tracklist::render_all_in(&doc, &TracklistOptions::default(), "x.wav", &|id| plan.res.has(id), plan.span()).unwrap();
        assert_eq!(list.entries.len(), 1);
        assert!(list.warnings.iter().any(|w| w.contains("outside the exported range")), "{:?}", list.warnings);
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn bad_loudness_settings_and_ranges_are_refused_before_the_job_starts() {
        let (vol, scratch, ids) = library_two("badreq");
        let doc = two_clip_doc(ids);
        let dest = scratch.join("x.wav");
        let d = dest.to_str().unwrap();
        let levels = |l: LevelSettings| ExportRequest { levels: l, ..ExportRequest::default() };
        assert!(plan_export_with(&vol, &doc, d, "wav", &levels(LevelSettings { target_lufs: Some(3.0), ..LevelSettings::default() })).is_err());
        assert!(plan_export_with(&vol, &doc, d, "wav", &levels(LevelSettings { max_boost_db: f64::NAN, ..LevelSettings::default() })).is_err());
        let bad_range = ExportRequest { range: RangeSpec::Time { from_sec: 5.0, to_sec: 1.0 }, ..ExportRequest::default() };
        assert!(plan_export_with(&vol, &doc, d, "wav", &bad_range).is_err());
        let silent_gap = ExportRequest { range: RangeSpec::Time { from_sec: 21.0, to_sec: 23.0 }, ..ExportRequest::default() };
        assert!(plan_export_with(&vol, &doc, d, "wav", &silent_gap).err().unwrap().to_string().contains("No track is heard"));
        assert!(!dest.exists());
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }

    #[test]
    fn a_cancel_during_the_measuring_pass_leaves_no_file() {
        let (vol, scratch, ids) = library_two("cancelmeasure");
        let doc = two_clip_doc(ids);
        let out = scratch.join("c.wav");
        let req = ExportRequest { levels: LevelSettings { target_lufs: Some(-14.0), clip_level: ClipLevel::Median, ..LevelSettings::default() }, ..ExportRequest::default() };
        let plan = plan_export_with(&vol, &doc, out.to_str().unwrap(), "wav", &req).unwrap();
        let mut calls = 0;
        let err = run_export_full(&plan, &mut |_| {
            calls += 1;
            calls < 3
        })
        .err()
        .unwrap();
        assert!(err.to_string().contains("cancelled"), "{err}");
        assert!(!out.exists());
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

    /// 1x1 PNG.
    fn tiny_png() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 0x90, 0x77, 0x53, 0xde, 0, 0, 0, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0, 0, 0x03, 0x01, 0x01, 0, 0x18, 0xdd, 0x8d, 0xb0, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ]
    }

    fn has(hay: &[u8], needle: &str) -> bool {
        hay.windows(needle.len()).any(|w| w == needle.as_bytes())
    }

    /// FLAC and MP3 (and Ogg, WAV) exports through the command's own plan: the file decodes, the
    /// tags are in it, and the chapters are the track list of the exported part: the same times a
    /// CUE sheet of that export has.
    #[test]
    fn every_format_exports_with_tags_and_the_track_list_as_chapters() {
        use crate::commands::export::encoding::{EncodingSpec, FlacLevelSpec, Mp3Spec, TagSpec};
        use crate::commands::export::tracklist::TracklistOptions;
        let (vol, scratch, ids) = library_two("tagged");
        let mut doc = two_clip_doc(ids);
        doc.clips[0].fuzzy_id.title = "First".into();
        doc.clips[0].fuzzy_id.artist = "Artist One".into();
        doc.clips[1].fuzzy_id.title = "Second".into();
        let cover = scratch.join("cover.png");
        std::fs::write(&cover, tiny_png()).unwrap();
        let tags = TagSpec {
            title: String::new(),
            artist: "DJ Example".into(),
            album: "Fixture Sessions".into(),
            year: "2026".into(),
            comment: "tagged by a test".into(),
            cover_path: Some(cover.to_string_lossy().into_owned()),
            chapters: Some(TracklistOptions::default()),
        };
        let encoding = EncodingSpec { mp3: Mp3Spec::Vbr { quality: 4 }, flac_bits: 24, flac_level: FlacLevelSpec::Fast, ..EncodingSpec::default() };
        // the whole mix (First at 0:00, Second 24 s in) as FLAC, and a time range of 15 s .. 30 s of
        // it (First 5 s, Second 6 s) in the other formats; debug builds are slow, so not every
        // format is rendered twice
        let whole = (RangeSpec::Whole, 24_000u32, "00:00:24.000", 44.0);
        let part = (RangeSpec::Time { from_sec: 15.0, to_sec: 30.0 }, 9_000u32, "00:00:09.000", 15.0);
        for (fmt, (range, second_at_ms, second_stamp, total_secs)) in [("flac", whole), ("mp3", part.clone()), ("ogg", part.clone()), ("wav", part)] {
            {
                let out = scratch.join(format!("tagged-{}.{fmt}", second_at_ms));
                let req = ExportRequest { range: range.clone(), encoding: encoding.clone(), tags: Some(tags.clone()), ..ExportRequest::default() };
                let plan = plan_export_with(&vol, &doc, out.to_str().unwrap(), fmt, &req).unwrap();
                let done = run_export_full(&plan, &mut |_| true).unwrap();
                assert!((done.audio.duration_secs - total_secs).abs() < 0.01, "{fmt}: {}", done.audio.duration_secs);
                let bytes = std::fs::read(&out).unwrap();
                // decodes to the length of the part
                let back = pm_mixer::audio::load(&out).unwrap();
                assert!((back.len() as f64 / 44100.0 - total_secs).abs() < 0.1, "{fmt}: {}", back.len());
                // tags: every format carries the artist and the first track name as chapter
                assert!(has(&bytes, "DJ Example") && has(&bytes, "Artist One - First") && has(&bytes, "Second"), "{fmt}");
                match fmt {
                    "flac" | "ogg" => {
                        assert!(has(&bytes, "CHAPTER001=00:00:00.000") && has(&bytes, &format!("CHAPTER002={second_stamp}")), "{fmt}");
                        assert!(has(&bytes, "TITLE=Level Test"), "the title defaults to the mix name");
                    }
                    "mp3" => {
                        assert!(has(&bytes, "chp2\0"), "CHAP frames");
                        // the CHAP frame of chapter 2 (the table of contents lists the id too)
                        let frame = bytes.windows(4).enumerate().find(|(i, w)| *w == b"CHAP" && bytes[i + 10..].starts_with(b"chp2\0")).unwrap().0;
                        let at = frame + 15;
                        assert_eq!(u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()), second_at_ms);
                        assert_eq!(u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()), (total_secs * 1000.0) as u32, "the last chapter ends with the audio");
                        assert!(has(&bytes, "Level Test"));
                    }
                    _ => assert!(has(&bytes, "cue ") && has(&bytes, "Level Test")),
                }
            }
        }
        // a WAV cannot hold the cover; that is reported, not silently dropped
        let plan = plan_export_with(&vol, &doc, scratch.join("n.wav").to_str().unwrap(), "wav", &ExportRequest { tags: Some(tags.clone()), ..ExportRequest::default() }).unwrap();
        assert_eq!(crate::commands::export::encoding::format_notes(plan.format, plan.encoding.tags.as_ref().unwrap()).len(), 1);

        // a cover that cannot be used stops the export before anything is written
        let bad = TagSpec { cover_path: Some(scratch.join("missing.png").to_string_lossy().into_owned()), ..tags.clone() };
        let out = scratch.join("never.flac");
        let err = plan_export_with(&vol, &doc, out.to_str().unwrap(), "flac", &ExportRequest { tags: Some(bad), ..ExportRequest::default() }).err().unwrap().to_string();
        assert!(err.contains("missing.png"), "{err}");
        assert!(!out.exists() && !scratch.join("never.flac.partial").exists());
        // bad settings too
        let req = ExportRequest { encoding: EncodingSpec { flac_bits: 12, ..EncodingSpec::default() }, ..ExportRequest::default() };
        assert!(plan_export_with(&vol, &doc, out.to_str().unwrap(), "flac", &req).is_err());
        // and no tags at all is the plain export: no chapters, Ogg keeps the old mix-name title
        let plain = scratch.join("plain.ogg");
        run_export(&plan_export(&vol, &doc, plain.to_str().unwrap(), "ogg").unwrap(), &mut |_| true).unwrap();
        let pb = std::fs::read(&plain).unwrap();
        assert!(has(&pb, "TITLE=Level Test") && !has(&pb, "CHAPTER001"));
        std::fs::remove_dir_all(vol).unwrap();
        std::fs::remove_dir_all(scratch).unwrap();
    }
}
