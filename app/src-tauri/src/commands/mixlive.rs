//! The live mix player: plays the open mix in real time from the editor's document, with no
//! render step. Edits are applied while it plays.
//!
//! ```text
//! MixDoc --to_mix--> pm_mixer::player (worker thread: Live engine -> ring)  --> device thread (WASAPI)
//!                          ^ clip audio                                         copies the ring out
//!                    loader thread (decodes the clips nearest the play position first)
//! ```
//!
//! Frontend contract (`src/lib/api/mixLive.ts`): commands `mix_live_load`, `mix_live_update`,
//! `mix_live_play`, `mix_live_pause`, `mix_live_seek`, `mix_live_set_master`, `mix_live_unload`;
//! events `mix-live-position` (`{ sec, playing }`, about 30 Hz while playing and on every change)
//! and `mix-live-status` (`{ state, loaded, total, message }`).
//!
//! The DSP is the one the export uses (`pm_mixer::live` shares the block mixer with the offline
//! render), so what plays is what exports.

mod device;
mod loader;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering::*};
use std::sync::{Arc, Mutex, OnceLock};

use pm_formats::mix::Mix;
use pm_mixer::live::LiveOptions;
use pm_mixer::player::{Command, Event, Player};
use pm_mixer::ring::Ring;
use pm_mixer::RATE;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::export::loudness::{apply_gains, PreviewGains};
use crate::commands::export::render::{peq_mode, LibResolver};
use crate::commands::mixes::doc::MixDoc;
use crate::error::{AppError, AppResult};
use crate::events;

use device::Device;
use loader::{Loader, Wanted};

pub const MIX_LIVE_POSITION: &str = "mix-live-position";
pub const MIX_LIVE_STATUS: &str = "mix-live-status";

/// Default for the decoded audio kept in memory (see [`loader`]): 1 GiB holds about seven 6-minute
/// tracks at the loader's 9 bytes a frame; the rest is decoded as the playhead nears it. The user
/// sets it in Settings (Mixes, Tempo matching); the page sends it with the load.
const DEFAULT_DECODE_BUDGET_MB: u64 = 1024;

/// The decode budget in bytes from the setting in MB, kept within 256 MB to 64 GB.
fn decode_budget_bytes(mb: Option<u64>) -> u64 {
    mb.unwrap_or(DEFAULT_DECODE_BUDGET_MB).clamp(256, 65_536) * 1024 * 1024
}
/// Frames the ring holds (about 0.74 s; the player keeps 250 ms of it filled).
const RING_FRAMES: usize = 32_768;

#[derive(Default)]
pub struct MixLive(Mutex<Option<Session>>);

pub fn setup(app: &mut tauri::App) {
    app.manage(MixLive::default());
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct PositionEvent {
    sec: f64,
    playing: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct StatusEvent {
    state: &'static str,
    loaded: usize,
    total: usize,
    message: String,
}

/// What the callbacks of the worker, loader and device threads share.
struct Shared {
    app: AppHandle,
    loader: OnceLock<Arc<Loader>>,
    playing: AtomicBool,
    /// The device failed (removed, closed by Windows): the next play reopens it.
    device_failed: AtomicBool,
    /// Last problem to show; cleared by the next play.
    message: Mutex<String>,
    /// A fatal problem (no output device).
    error: Mutex<String>,
    loaded_any: AtomicBool,
    pos_frame: AtomicI64,
}

impl Shared {
    fn status(&self) -> StatusEvent {
        let p = self.loader.get().map(|l| l.progress()).unwrap_or_default();
        let message = self.message.lock().map(|m| m.clone()).unwrap_or_default();
        let error = self.error.lock().map(|m| m.clone()).unwrap_or_default();
        status_of(&error, message, self.loaded_any.load(Acquire), self.playing.load(Acquire), p)
    }

    fn emit_status(&self) {
        events::emit(&self.app, MIX_LIVE_STATUS, self.status());
    }

    fn on_event(&self, e: Event) {
        match e {
            Event::Position { sec, playing } => {
                let was = self.playing.swap(playing, AcqRel);
                self.pos_frame.store((sec * f64::from(RATE)) as i64, Release);
                if let Some(l) = self.loader.get() {
                    l.set_position((sec * f64::from(RATE)) as i64);
                }
                events::emit(&self.app, MIX_LIVE_POSITION, PositionEvent { sec, playing });
                if was != playing {
                    self.emit_status();
                }
            }
            Event::Note(n) => {
                if let Ok(mut m) = self.message.lock() {
                    *m = n;
                }
                self.emit_status();
            }
        }
    }
}

struct Session {
    player: Player,
    loader: Arc<Loader>,
    device: Option<Device>,
    shared: Arc<Shared>,
    ring: Arc<Ring>,
    root: PathBuf,
    peq: pm_mixer::PeqMode,
    /// Document ids of the Wah tools the app marks as Vocal tools; `None` = the card decides for all.
    vocal_ids: Option<Vec<String>>,
    master_db: f64,
    /// Loudness gains of the export settings (per clip and master), applied on top of the fader.
    gains: PreviewGains,
    /// Library id -> file, `None` for a track that is not on the volume; each id is looked up once.
    paths: HashMap<i64, Option<PathBuf>>,
}

impl Session {
    /// Stops everything (blocking: joins the decode and worker threads).
    fn shutdown(self) {
        drop(self);
    }
}

/// However a session is dropped (replaced by an overlapping load, app exit) its decode thread and
/// the decoded audio (up to the budget) go with it. The loader and `Shared` reference each other,
/// so only the explicit stop breaks the cycle. The player joins its worker in its own drop, after this.
impl Drop for Session {
    fn drop(&mut self) {
        self.loader.shutdown();
        // fade out before the device goes: a hard cut mid-signal clicks
        if self.device.is_some() && !self.ring.stopped() {
            self.player.send(Command::Pause);
            let t = std::time::Instant::now();
            while !self.ring.stopped() && t.elapsed() < std::time::Duration::from_millis(200) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        self.device.take();
    }
}

/// Resolves the audio files of `mix` that the session has not looked up yet and returns the
/// files the mix needs with their place on the timeline.
fn wanted_for(root: &Path, paths: &mut HashMap<i64, Option<PathBuf>>, mix: &Mix) -> AppResult<Vec<Wanted>> {
    if mix.tracks.iter().any(|t| !paths.contains_key(&t.library_id)) {
        let res = LibResolver::open(root, mix)?;
        for t in &mix.tracks {
            paths.entry(t.library_id).or_insert_with(|| res.path(t.library_id).map(Path::to_path_buf));
        }
    }
    Ok(mix
        .tracks
        .iter()
        .map(|t| Wanted { id: t.library_id, path: paths.get(&t.library_id).cloned().flatten(), start: t.position.start as i64, end: (t.position.start + t.position.length) as i64 })
        .collect())
}

/// The engine options for a document: the card's Vocal mode, the loudness gains of the export
/// settings (the same `apply_gains` the export uses, so a preview at export level equals the
/// export) and the master fader on top.
fn options(doc: &MixDoc, peq: pm_mixer::PeqMode, vocal_ids: &Option<Vec<String>>, master_db: f64, gains: &PreviewGains) -> LiveOptions {
    let mut o = pm_mixer::Options::default();
    apply_gains(doc, gains, &mut o);
    let vocal_tools = vocal_ids.as_ref().map(|ids| Arc::new(doc.tool_session_ids(ids)));
    LiveOptions { peq, vocal_tools, clip_gain_db: o.clip_gain_db, master_gain_db: o.master_gain_db + master_db }
}

fn not_loaded() -> AppError {
    AppError::from("No mix is loaded in the live player.")
}

/// Starts decoding the clips of `doc` in the background and readies the player (stopped at 0).
/// Replaces a mix loaded before.
#[tauri::command]
pub async fn mix_live_load(app: AppHandle, state: State<'_, MixLive>, root: String, doc: MixDoc, gains: Option<PreviewGains>, vocal_tool_ids: Option<Vec<String>>, decode_budget_mb: Option<u64>) -> AppResult<()> {
    let old = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?.take();
    if let Some(s) = old {
        tauri::async_runtime::spawn_blocking(move || s.shutdown()).await.ok();
    }
    let app2 = app.clone();
    let budget = decode_budget_bytes(decode_budget_mb);
    let session = tauri::async_runtime::spawn_blocking(move || -> AppResult<Session> {
        let root = PathBuf::from(root);
        let mix = doc.to_mix()?;
        let peq = peq_mode(&root);
        let mut paths = HashMap::new();
        let wanted = wanted_for(&root, &mut paths, &mix)?;
        let shared = Arc::new(Shared {
            app: app2,
            loader: OnceLock::new(),
            playing: AtomicBool::new(false),
            device_failed: AtomicBool::new(false),
            message: Mutex::new(String::new()),
            error: Mutex::new(String::new()),
            loaded_any: AtomicBool::new(true),
            pos_frame: AtomicI64::new(0),
        });
        let sh = shared.clone();
        let loader = Loader::start(budget, Arc::new(move || sh.emit_status()));
        let _ = shared.loader.set(loader.clone());
        loader.set_wanted(wanted);
        let ring = Arc::new(Ring::new(RING_FRAMES));
        let (sh, ld) = (shared.clone(), loader.clone());
        let player = Player::spawn(ring.clone(), Box::new(move |t| ld.get(t.library_id)), Arc::new(move |e| sh.on_event(e)));
        let gains = gains.unwrap_or_default();
        player.send(Command::Load(mix, options(&doc, peq, &vocal_tool_ids, 0.0, &gains)));
        shared.emit_status();
        Ok(Session { player, loader, device: None, shared, ring, root, peq, vocal_ids: vocal_tool_ids, master_db: 0.0, gains, paths })
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))??;
    // a load that overlapped this one may have stored a session meanwhile: stop that one too
    let replaced = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?.replace(session);
    if let Some(s) = replaced {
        tauri::async_runtime::spawn_blocking(move || s.shutdown()).await.ok();
    }
    Ok(())
}

/// An edit: applied while stopped or playing, without a click.
#[tauri::command]
pub async fn mix_live_update(state: State<'_, MixLive>, doc: MixDoc, gains: Option<PreviewGains>, refresh_vocal: Option<bool>, vocal_tool_ids: Option<Vec<String>>) -> AppResult<()> {
    let mix = doc.to_mix()?;
    // look the new clips up on a blocking thread (the library is opened), then hand over
    let taken = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?.as_mut().map(|s| (s.root.clone(), std::mem::take(&mut s.paths)));
    let Some((root, mut paths)) = taken else { return Err(not_loaded()) };
    let (mix, paths, wanted, peq) = tauri::async_runtime::spawn_blocking(move || {
        let w = wanted_for(&root, &mut paths, &mix);
        // the card's Vocal flag decides what Wah tools sound like; read again only when asked (it is a file on the volume)
        let peq = refresh_vocal.unwrap_or(false).then(|| peq_mode(&root));
        (mix, paths, w, peq)
    })
    .await
    .map_err(|e| AppError::from(e.to_string()))?;
    let mut g = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?;
    let s = g.as_mut().ok_or_else(not_loaded)?;
    s.paths = paths;
    if let Some(p) = peq {
        s.peq = p;
    }
    let wanted = wanted?;
    s.loader.set_wanted(wanted);
    if let Some(g) = gains {
        s.gains = g;
    }
    if vocal_tool_ids.is_some() {
        s.vocal_ids = vocal_tool_ids;
    }
    s.player.send(Command::Update(mix, options(&doc, s.peq, &s.vocal_ids, s.master_db, &s.gains)));
    s.shared.emit_status();
    Ok(())
}

#[tauri::command]
pub async fn mix_live_play(state: State<'_, MixLive>, from_sec: f64) -> AppResult<()> {
    if !from_sec.is_finite() {
        return Err("The start position is not a number.".into());
    }
    let mut g = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?;
    let s = g.as_mut().ok_or_else(not_loaded)?;
    if s.device.is_none() || s.shared.device_failed.load(Acquire) {
        // a device that failed is dropped first (its thread has ended)
        s.device.take();
        s.shared.device_failed.store(false, Release);
        let sh = s.shared.clone();
        let on_err = Arc::new(move |e: String| {
            sh.device_failed.store(true, Release);
            sh.playing.store(false, Release);
            if let Ok(mut m) = sh.error.lock() {
                *m = e;
            }
            sh.emit_status();
        });
        match Device::open(s.ring.clone(), on_err) {
            Ok(d) => {
                s.device = Some(d);
                if let Ok(mut m) = s.shared.error.lock() {
                    m.clear();
                }
            }
            Err(e) => {
                if let Ok(mut m) = s.shared.error.lock() {
                    *m = e.clone();
                }
                s.shared.emit_status();
                return Err(e.into());
            }
        }
    }
    if let Ok(mut m) = s.shared.message.lock() {
        m.clear();
    }
    s.player.send(Command::Play((from_sec.max(0.0) * f64::from(RATE)).round() as i64));
    Ok(())
}

#[tauri::command]
pub async fn mix_live_pause(state: State<'_, MixLive>) -> AppResult<()> {
    let g = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?;
    g.as_ref().ok_or_else(not_loaded)?.player.send(Command::Pause);
    Ok(())
}

#[tauri::command]
pub async fn mix_live_seek(state: State<'_, MixLive>, sec: f64) -> AppResult<()> {
    if !sec.is_finite() {
        return Err("The position is not a number.".into());
    }
    let g = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?;
    g.as_ref().ok_or_else(not_loaded)?.player.send(Command::Seek((sec.max(0.0) * f64::from(RATE)).round() as i64));
    Ok(())
}

#[tauri::command]
pub async fn mix_live_set_master(state: State<'_, MixLive>, gain_db: f64) -> AppResult<()> {
    if !gain_db.is_finite() {
        return Err("The gain is not a number.".into());
    }
    let mut g = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?;
    let s = g.as_mut().ok_or_else(not_loaded)?;
    s.master_db = gain_db.clamp(-60.0, 24.0);
    s.player.send(Command::SetMaster(s.gains.master_gain_db + s.master_db));
    Ok(())
}

/// Stops, frees the decoded audio and closes the audio device. Fine to call with nothing loaded.
#[tauri::command]
pub async fn mix_live_unload(app: AppHandle, state: State<'_, MixLive>) -> AppResult<()> {
    let old = state.0.lock().map_err(|_| AppError::from("The live player is locked."))?.take();
    if let Some(s) = old {
        let sh = s.shared.clone();
        tauri::async_runtime::spawn_blocking(move || s.shutdown()).await.ok();
        sh.loaded_any.store(false, Release);
        sh.playing.store(false, Release);
        if let Ok(mut m) = sh.message.lock() {
            m.clear();
        }
        if let Ok(mut m) = sh.error.lock() {
            m.clear();
        }
        sh.emit_status();
    } else {
        events::emit(&app, MIX_LIVE_STATUS, StatusEvent { state: "idle", loaded: 0, total: 0, message: String::new() });
    }
    Ok(())
}

#[macro_export]
macro_rules! mixlive_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::mixlive::mix_live_load,
                $crate::commands::mixlive::mix_live_update,
                $crate::commands::mixlive::mix_live_play,
                $crate::commands::mixlive::mix_live_pause,
                $crate::commands::mixlive::mix_live_seek,
                $crate::commands::mixlive::mix_live_set_master,
                $crate::commands::mixlive::mix_live_unload,
            ];
            $($rest),*
        }
    };
}

/// The status the screen shows: an error beats everything, then idle (nothing loaded), playing,
/// loading (clips still decoding) and ready.
fn status_of(error: &str, message: String, loaded_any: bool, playing: bool, p: loader::Progress) -> StatusEvent {
    let state = if !error.is_empty() {
        "error"
    } else if !loaded_any {
        "idle"
    } else if playing {
        "playing"
    } else if p.loaded < p.total {
        "loading"
    } else {
        "ready"
    };
    StatusEvent { state, loaded: p.loaded, total: p.total, message: if error.is_empty() { message } else { error.to_string() } }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn decode_budget_comes_from_the_setting_within_limits() {
        assert_eq!(decode_budget_bytes(None), 1024 * 1024 * 1024);
        assert_eq!(decode_budget_bytes(Some(2048)), 2048 * 1024 * 1024);
        assert_eq!(decode_budget_bytes(Some(1)), 256 * 1024 * 1024);
        assert_eq!(decode_budget_bytes(Some(u64::MAX)), 65_536 * 1024 * 1024);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loader::Progress;

    #[test]
    fn status_states() {
        let p = |loaded, total| Progress { loaded, total };
        assert_eq!(status_of("", String::new(), false, false, p(0, 0)).state, "idle");
        assert_eq!(status_of("", String::new(), true, false, p(1, 3)).state, "loading");
        assert_eq!(status_of("", String::new(), true, false, p(3, 3)).state, "ready");
        // playing wins over loading: clips that are still decoding play silent and join in
        assert_eq!(status_of("", String::new(), true, true, p(1, 3)).state, "playing");
        let e = status_of("No audio output device was found.", "note".into(), true, true, p(3, 3));
        assert_eq!((e.state, e.message.as_str()), ("error", "No audio output device was found."));
    }

    /// Plays a quiet 220 Hz tone through the real default output for 3 s, with a crossfaded edit and a
    /// seek in the middle. Ignored: it needs an audio device and makes a sound.
    /// `cargo test --offline --release real_device -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_device_plays_with_an_edit_and_a_seek() {
        use pm_formats::mix::{Position, Track};
        use pm_mixer::audio::Source;
        use pm_mixer::engine::SourceState;
        use std::sync::atomic::Ordering::Relaxed;
        use std::time::{Duration, Instant};
        let n = 44100 * 8;
        let tone: Vec<f32> = (0..n).map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / 44100.0).sin() * 0.05).collect();
        let src = Source::new(tone.clone(), tone);
        let mut mix = Mix::default();
        mix.tracks.push(Track::new(1, "x.wav", 120.0, 1.0, Position { start: 0.0, length: n as f64, limit_size: true, ..Position::default() }));
        let ring = Arc::new(Ring::new(RING_FRAMES));
        let notes = Arc::new(Mutex::new(Vec::<String>::new()));
        let n2 = notes.clone();
        let player = Player::spawn(ring.clone(), Box::new(move |_| SourceState::Ready(src.clone())), Arc::new(move |e| {
            if let Event::Note(s) = e {
                n2.lock().unwrap().push(s);
            }
        }));
        let dev = Device::open(ring.clone(), Arc::new(|e| panic!("device error: {e}"))).expect("no audio output device");
        player.send(Command::Load(mix.clone(), LiveOptions::default()));
        let t = Instant::now();
        player.send(Command::Play(0));
        while player.stats.first_audio_us.load(Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        println!("play -> device released: {} us (command to first block queued, incl. engine build)", player.stats.first_audio_us.load(Relaxed));
        std::thread::sleep(Duration::from_millis(1000));
        mix.tracks[0].position.length = n as f64 - 44100.0;
        player.send(Command::Update(mix.clone(), LiveOptions::default()));
        std::thread::sleep(Duration::from_millis(800));
        player.send(Command::Seek(44100 * 4));
        std::thread::sleep(Duration::from_millis(1200));
        println!("ran {:.2} s, underruns {}, ring tail {} frames, notes {:?}", t.elapsed().as_secs_f64(), ring.underruns(), ring.tail(), notes.lock().unwrap());
        assert_eq!(ring.underruns(), 0);
        drop(dev);
        drop(player);
    }

    #[test]
    fn wanted_marks_a_clip_without_a_file_and_remembers_lookups() {
        use pm_formats::mix::{Position, Track};
        let mut mix = Mix::default();
        mix.tracks.push(Track::new(7, "x.wav", 120.0, 1.0, Position { start: 44100.0, length: 88200.0, limit_size: true, ..Position::default() }));
        let mut paths = HashMap::new();
        paths.insert(7, Some(PathBuf::from("a.wav")));
        // everything already looked up: the library is not opened (the root does not exist)
        let w = wanted_for(Path::new("Z:/no/such/root"), &mut paths, &mix).unwrap();
        assert_eq!(w, vec![Wanted { id: 7, path: Some(PathBuf::from("a.wav")), start: 44100, end: 132300 }]);
    }
}
