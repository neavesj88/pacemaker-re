//! `MixDoc`: the mix editor's wire model, and its lossless conversion to and from
//! `pm_formats::mix::Mix` (the document the Editor and the device read).
//!
//! The semantics of every field are documented once, in `src/lib/api/mixes.ts`, which mirrors
//! these types; keep the two in step. The rules this file enforces:
//!
//! - **Lossless**: [`MixDoc::from_mix`] followed by [`MixDoc::to_mix`] gives back a `Mix` that
//!   is `==` to the original, so an unchanged mix saves to the identical blob. Everything the
//!   editor does not need to understand (names, bypass flags, the `GlobalContext` ids, the
//!   volume tool's shell, loop offsets, key-node flags, ...) is carried as pass-through fields
//!   the UI must send back untouched. Fields documented as *derived* are recomputed here and
//!   ignored when read back.
//! - **Stable ids**: every clip, process and transition has an `id`, the `SessionEventId` of
//!   its event when that is non-empty and unique, otherwise a synthetic `auto-N` (and the raw
//!   value is kept in `legacySessionId` so it still round-trips).
//! - **Nothing outside the document is stored here**: the mix tempo, beat grids, track
//!   availability and the transition-to-clip links are derived on open and not persisted.

use std::collections::{BTreeMap, HashMap, HashSet};

use pm_formats::mix::{self as m, TempoMap};
use serde::{Deserialize, Serialize};

pub const SAMPLE_RATE: f64 = m::SAMPLE_RATE;
/// Tolerance when comparing positions, in samples (about 0.02 ms).
pub const EPS: f64 = 1.0;
/// Name prefix of the synthetic ids given to events whose `SessionEventId` is empty or repeated.
pub const AUTO_PREFIX: &str = "auto-";

/// Seconds of a sample count at the engine rate.
pub fn secs(samples: f64) -> f64 {
    samples / SAMPLE_RATE
}

// ---------------------------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------------------------

/// A time range on some axis (timeline for clips and transitions, clip-local for the processes
/// of a clip), in samples at 44.1 kHz. Mirrors `PositionedEvent`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: f64,
    pub length: f64,
    /// `LOffset`: samples hidden at the left edge. Non-zero on clips that were cut (the cue).
    #[serde(default)]
    pub trim_start: f64,
    /// `ROffset`: samples hidden at the right edge.
    #[serde(default)]
    pub trim_end: f64,
    /// `LimitSize`.
    #[serde(default)]
    pub limit_size: bool,
    /// Derived: `start / 44100`.
    #[serde(default)]
    pub start_sec: f64,
    /// Derived: `length / 44100`.
    #[serde(default)]
    pub length_sec: f64,
}

impl Span {
    fn from_position(p: &m::Position) -> Span {
        Span {
            start: p.start,
            length: p.length,
            trim_start: p.l_offset,
            trim_end: p.r_offset,
            limit_size: p.limit_size,
            start_sec: secs(p.start),
            length_sec: secs(p.length),
        }
    }

    fn to_position(&self, id: &str, legacy: &Option<String>) -> m::Position {
        m::Position {
            start: self.start,
            length: self.length,
            l_offset: self.trim_start,
            r_offset: self.trim_end,
            limit_size: self.limit_size,
            session_event_id: session_id(id, legacy),
        }
    }

    fn refresh(&mut self) {
        self.start_sec = secs(self.start);
        self.length_sec = secs(self.length);
    }

    pub fn end(&self) -> f64 {
        self.start + self.length
    }
}

fn session_id(id: &str, legacy: &Option<String>) -> String {
    if id.starts_with(AUTO_PREFIX) { legacy.clone().unwrap_or_default() } else { id.to_string() }
}

/// An automation / gain envelope node: `pos` samples from the start of the owning event (the
/// tool, or the clip for the gain envelope), `value` in the parameter's own unit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnvNode {
    pub pos: u32,
    pub value: f32,
}

/// A node of a clip's tempo curve: from clip-local sample `pos` on, play at `speed` times the
/// analysed tempo (joined linearly between nodes).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoNode {
    pub pos: u32,
    pub speed: f32,
}

/// A node of a clip's key curve: pitch shift in `cents` from clip-local sample `pos`. `flags`
/// is pass-through (always 1 in known documents).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyNodeDoc {
    pub pos: u32,
    pub cents: f32,
    pub flags: u32,
}

/// A point of a transition's crossfade curve: `x` is the position across the overlap (0 = its
/// start, 1 = its end), `value` the fader position (0 = only the earlier clip, 1 = only the
/// later one).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurveNode {
    pub x: f64,
    pub value: f32,
}

/// Effect class and its extra members (pass-through except when creating a tool).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EffectClass {
    Fx { active: bool },
    Filter,
    Crush,
    Delay { delay_type: u32, active: bool, sync_against_beat: bool },
    Eq,
    Peq,
    Volume,
    Trans { start_bpm: f32, active: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectDoc {
    /// "Reverb", "FILTER", "Crush", "Echo", "Roll", "EQ", "Wah", "Volume", "Trans", ... It
    /// prefixes the keys of the tool's `params`.
    pub name: String,
    pub bypassed: bool,
    #[serde(flatten)]
    pub class: EffectClass,
}

/// A beat-FX / EQ / colour tool on a clip's tool lane (`AutomatedAudioProcess`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_session_id: Option<String>,
    /// Clip-local range (see `Clip`): the tool acts while the clip's local position is in it.
    #[serde(flatten)]
    pub span: Span,
    /// Derived: the name of the first effect ("Reverb", "EQ", ...), for display and grouping.
    #[serde(default)]
    pub tool_type: String,
    /// Name of the wrapping process ("ToolProcess"); pass-through.
    pub name: String,
    /// The tool is muted ("Mute Tool").
    pub bypassed: bool,
    pub effects: Vec<EffectDoc>,
    /// Parameter envelopes, keyed `<EffectName>-<Parameter>` (`Reverb-Wet/Dry`, `EQ-Lo`, ...;
    /// Echo/Roll have the real trailing-space key `Echo-Time `). Node positions are samples from
    /// the start of the tool and the last node of a two-node envelope sits at `length - 1`.
    pub params: BTreeMap<String, Vec<EnvNode>>,
}

/// `LoopAudioProcess`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_session_id: Option<String>,
    #[serde(flatten)]
    pub span: Span,
    pub name: String,
    pub bypassed: bool,
    /// `Length`: one iteration, in samples.
    pub iteration_length: f64,
    pub iterations: u32,
    pub re_loop: bool,
    /// Length mode of the loop tool menu.
    pub mode: bool,
    /// Id of the earlier loop of the same clip this one continues, or null.
    pub parent: Option<String>,
    pub offsets: Vec<LoopOffsetDoc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoopOffsetDoc {
    pub iteration: u32,
    pub a: f32,
    pub offset: f32,
}

/// `IgnoreAudioProcess`: a region that is skipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_session_id: Option<String>,
    #[serde(flatten)]
    pub span: Span,
    pub name: String,
    pub bypassed: bool,
    pub start_pos: u32,
}

/// `ReversalAudioProcess`: a region played backwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_session_id: Option<String>,
    #[serde(flatten)]
    pub span: Span,
    pub name: String,
    pub bypassed: bool,
}

/// An item of a clip's process list, in document order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProcessDoc {
    Tool(ToolDoc),
    Loop(LoopDoc),
    Ignore(IgnoreDoc),
    Reverse(ReverseDoc),
}

impl ProcessDoc {
    pub fn id(&self) -> &str {
        match self {
            ProcessDoc::Tool(p) => &p.id,
            ProcessDoc::Loop(p) => &p.id,
            ProcessDoc::Ignore(p) => &p.id,
            ProcessDoc::Reverse(p) => &p.id,
        }
    }
    pub fn span(&self) -> &Span {
        match self {
            ProcessDoc::Tool(p) => &p.span,
            ProcessDoc::Loop(p) => &p.span,
            ProcessDoc::Ignore(p) => &p.span,
            ProcessDoc::Reverse(p) => &p.span,
        }
    }
    fn span_mut(&mut self) -> &mut Span {
        match self {
            ProcessDoc::Tool(p) => &mut p.span,
            ProcessDoc::Loop(p) => &mut p.span,
            ProcessDoc::Ignore(p) => &mut p.span,
            ProcessDoc::Reverse(p) => &mut p.span,
        }
    }
}

/// `GlobalContext.fuzzyId`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FuzzyIdDoc {
    pub artist: String,
    pub album: String,
    pub title: String,
    pub genre: String,
}

/// The shell of a clip's volume envelope (pass-through); the editable part is `Clip::gain`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeShell {
    pub name: String,
    pub bypassed: bool,
    pub effects: Vec<EffectDoc>,
    #[serde(flatten)]
    pub span: Span,
    /// Raw `SessionEventId` (empty in every known document).
    pub session_event_id: String,
    /// Automation keys other than `Volume-Volume` (none in known documents).
    pub other_params: BTreeMap<String, Vec<EnvNode>>,
    /// False when the document has no `Volume-Volume` envelope at all (then `gain` is empty).
    #[serde(default = "yes")]
    pub gain_key_present: bool,
}

fn yes() -> bool {
    true
}

/// One beat grid, from the track's `.str` sidecar. Positions are frames of the audio file at
/// `sourceRate` (not the engine rate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatGrid {
    pub source_rate: u32,
    pub beat_start_offset: i64,
    /// Frame position of every beat, ascending.
    pub beat_frames: Vec<i64>,
    /// Local tempo at each beat (same length as `beatFrames`).
    pub beat_bpm: Vec<f64>,
    /// Index into `beatFrames` of the first downbeat (bar 1, beat 1), when analysed.
    pub first_downbeat: Option<usize>,
}

/// What the editor knows about a library track. Read-only: derived on open and never saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub track_id: i64,
    /// The track exists in the library and its audio file is on the volume.
    pub available: bool,
    /// Why not (`"removed from the library"`, `"audio file missing"`, ...).
    pub problem: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub key: String,
    /// The library's tempo for the track.
    pub bpm: f64,
    /// Whole seconds (from the tags); the real length is only known to the decoder.
    pub duration_secs: f64,
    /// Sample rate of the audio file.
    pub source_rate: u32,
    pub location: String,
    pub grid: Option<BeatGrid>,
    /// Why there is no grid (`"no .str file"`, ...).
    pub grid_problem: Option<String>,
}

/// One track on the timeline (`Track` / `AudioClip`).
///
/// Coordinates: **timeline** samples place the clip (`start`); every other position of the clip
/// (trims, tempo / key / gain nodes, `processes`) is **clip-local**: samples of the stretched
/// audio counted from the start of the file, so the timeline position of local `p` is
/// `start + p`. The audible part is local `[trimStart, length - trimEnd]`, i.e. timeline
/// `[visibleStart, visibleEnd]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    /// Stable id (the clip's `SessionEventId`).
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_session_id: Option<String>,
    /// Library track id (`LocalContext.libraryID`); negative = a track removed from the library.
    pub track_id: i64,
    /// Deck, 0 or 1; overlapping clips go on different channels.
    pub channel: u32,
    #[serde(flatten)]
    pub span: Span,
    /// Derived: `start + trimStart`, `length - trimEnd + start`, the audible range on the timeline.
    #[serde(default)]
    pub visible_start: f64,
    #[serde(default)]
    pub visible_end: f64,
    #[serde(default)]
    pub visible_start_sec: f64,
    #[serde(default)]
    pub visible_end_sec: f64,
    /// Analysed tempo of the track as stored in the mix (`Bpm`). Effective tempo at clip-local
    /// `p` is `bpm * speed(p)`.
    pub bpm: f64,
    /// Tempo curve (`PitchEnvelope`).
    pub tempo: Vec<TempoNode>,
    /// `NonInterpolated`: pitch mode instead of time-stretch for the tempo curve.
    pub pitch_mode: bool,
    /// Key curve (`KeyEnvelope`).
    pub key: Vec<KeyNodeDoc>,
    /// Gain curve: the `Volume-Volume` envelope of the clip's volume tool, nodes in clip-local
    /// samples, `value` in dB (0 = unity; -5..0 seen).
    pub gain: Vec<EnvNode>,
    /// Tools, loops, ignore and reverse regions, in document order.
    pub processes: Vec<ProcessDoc>,
    /// Pass-through: `FilePath`.
    pub file_path: String,
    /// Pass-through: `Offset`.
    pub offset: u32,
    pub clip_name: String,
    pub clip_bypassed: bool,
    pub tonium_id: String,
    pub other_ids: BTreeMap<String, String>,
    pub fuzzy_id: FuzzyIdDoc,
    pub volume: VolumeShell,
    /// Read-only library facts and the beat grid; null until `mix_open` / `mix_track_info`
    /// fills it. Ignored on save.
    #[serde(default)]
    pub track: Option<TrackInfo>,
}

impl Clip {
    /// Effective tempo (BPM) at a clip-local position.
    pub fn effective_bpm(&self, local: f64) -> f64 {
        self.bpm * self.tempo_map().speed_at(local)
    }

    pub fn tempo_map(&self) -> TempoMap {
        TempoMap::new(&self.tempo.iter().map(|n| m::SpeedNode { position: n.pos, speed: n.speed }).collect::<Vec<_>>())
    }

    /// Timeline samples of every beat of the grid (empty without one).
    pub fn beat_times(&self) -> Vec<f64> {
        let Some(g) = self.track.as_ref().and_then(|t| t.grid.as_ref()) else { return Vec::new() };
        let tm = self.tempo_map();
        let k = SAMPLE_RATE / f64::from(g.source_rate.max(1));
        g.beat_frames.iter().map(|&f| self.span.start + tm.local_at(f as f64 * k)).collect()
    }

    fn refresh(&mut self) {
        self.span.refresh();
        self.visible_start = self.span.start + self.span.trim_start;
        self.visible_end = self.span.start + self.span.length - self.span.trim_end;
        self.visible_start_sec = secs(self.visible_start);
        self.visible_end_sec = secs(self.visible_end);
        for p in &mut self.processes {
            p.span_mut().refresh();
            if let ProcessDoc::Tool(t) = p {
                t.tool_type = t.effects.first().map(|e| e.name.clone()).unwrap_or_default();
            }
        }
        self.volume.span.refresh();
    }
}

/// Which two clips a transition joins. Derived by geometry on open (the document itself does
/// not link a transition to its clips); not saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Between {
    pub from: String,
    pub to: String,
}

/// A crossfade over the overlap of two clips (`TransitionAudioProcess`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionDoc {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_session_id: Option<String>,
    /// Timeline range of the overlap.
    #[serde(flatten)]
    pub span: Span,
    pub name: String,
    pub bypassed: bool,
    /// Beat-match flag of the transition.
    pub beat_match: bool,
    /// 1 = fade after 1/2, 3 = fade after 3/4 (the only values seen).
    pub crossfade_type: u32,
    pub curve: Vec<CurveNode>,
    #[serde(default)]
    pub between: Option<Between>,
}

/// The library row of the mix (not part of the document). Only the editable fields are written
/// back, by `mix_set_meta`; the rest is read-only.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixMeta {
    pub genre: String,
    pub year: i64,
    pub rating: i64,
    pub producer: String,
    pub remixer: String,
    pub comments: String,
    #[serde(default)]
    pub date_created: i64,
    #[serde(default)]
    pub date_modified: i64,
    #[serde(default)]
    pub times_played: i64,
    #[serde(default)]
    pub last_played: i64,
    #[serde(default)]
    pub play_time_secs: i64,
    #[serde(default)]
    pub creator_id: String,
}

impl MixMeta {
    pub fn from_info(i: &pm_formats::library::MixInfo) -> MixMeta {
        MixMeta {
            genre: i.genre.clone(),
            year: i.year,
            rating: i.rating,
            producer: i.producer.clone(),
            remixer: i.remixer.clone(),
            comments: i.comments.clone(),
            date_created: i.date_created,
            date_modified: i.date_modified,
            times_played: i.times_played,
            last_played: i.last_played,
            play_time_secs: i.play_time_secs,
            creator_id: i.creator_id.clone(),
        }
    }

    pub fn to_update(&self) -> pm_formats::library::MixMetaUpdate {
        pm_formats::library::MixMetaUpdate {
            genre: self.genre.clone(),
            year: self.year,
            rating: self.rating,
            producer: self.producer.clone(),
            remixer: self.remixer.clone(),
            comments: self.comments.clone(),
        }
    }
}

/// The whole mix as the editor sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixDoc {
    /// `mixes.mix_id`.
    pub id: i64,
    /// `DocumentName`, kept equal to the row's name; saving renames the mix.
    pub name: String,
    /// Document-level `Genre` and `Artist` (not the library row's genre).
    pub genre: String,
    pub artist: String,
    /// Pass-through: `DeviceInfo`.
    pub device_info: String,
    /// Always 44100.
    pub sample_rate: u32,
    /// The mix's reference tempo in BPM, used as the beat-match target and the ruler grid. The
    /// format has no such field: derived on open as the effective tempo of the first clip, and
    /// kept by the UI while editing. Not saved.
    pub tempo_bpm: Option<f64>,
    pub clips: Vec<Clip>,
    pub transitions: Vec<TransitionDoc>,
    /// Derived: end of the last audible clip (timeline samples) and in seconds.
    #[serde(default)]
    pub length: f64,
    #[serde(default)]
    pub length_sec: f64,
    /// The library row; null for a document that is not from the library. Not saved by `mix_save`.
    #[serde(default)]
    pub meta: Option<MixMeta>,
}

// ---------------------------------------------------------------------------------------------
// Id generation
// ---------------------------------------------------------------------------------------------

/// A fresh 32-character lowercase hex id, the shape of a `juce::Uuid` string.
pub fn fresh_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let mut out = String::with_capacity(32);
    for k in 0..2u64 {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(n);
        h.write_u64(t);
        h.write_u64(k);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

#[derive(Default)]
struct Ids {
    seen: HashSet<String>,
    n: usize,
}

impl Ids {
    /// `(id, legacy)` for a raw `SessionEventId`.
    fn pick(&mut self, raw: &str) -> (String, Option<String>) {
        if !raw.is_empty() && !raw.starts_with(AUTO_PREFIX) && self.seen.insert(raw.to_string()) {
            (raw.to_string(), None)
        } else {
            self.n += 1;
            (format!("{AUTO_PREFIX}{}", self.n), Some(raw.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mix -> MixDoc
// ---------------------------------------------------------------------------------------------

fn effect_doc(e: &m::Effect) -> EffectDoc {
    let class = match &e.kind {
        m::EffectKind::Fx { active } => EffectClass::Fx { active: *active },
        m::EffectKind::Filter => EffectClass::Filter,
        m::EffectKind::Crush => EffectClass::Crush,
        m::EffectKind::Delay { delay_type, active, sync_against_beat } => {
            EffectClass::Delay { delay_type: *delay_type, active: *active, sync_against_beat: *sync_against_beat }
        }
        m::EffectKind::Eq => EffectClass::Eq,
        m::EffectKind::Peq => EffectClass::Peq,
        m::EffectKind::Volume => EffectClass::Volume,
        m::EffectKind::Trans { start_bpm, active } => EffectClass::Trans { start_bpm: *start_bpm, active: *active },
    };
    EffectDoc { name: e.name.clone(), bypassed: e.bypassed, class }
}

fn effect_of(d: &EffectDoc) -> m::Effect {
    let kind = match &d.class {
        EffectClass::Fx { active } => m::EffectKind::Fx { active: *active },
        EffectClass::Filter => m::EffectKind::Filter,
        EffectClass::Crush => m::EffectKind::Crush,
        EffectClass::Delay { delay_type, active, sync_against_beat } => {
            m::EffectKind::Delay { delay_type: *delay_type, active: *active, sync_against_beat: *sync_against_beat }
        }
        EffectClass::Eq => m::EffectKind::Eq,
        EffectClass::Peq => m::EffectKind::Peq,
        EffectClass::Volume => m::EffectKind::Volume,
        EffectClass::Trans { start_bpm, active } => m::EffectKind::Trans { start_bpm: *start_bpm, active: *active },
    };
    m::Effect { name: d.name.clone(), bypassed: d.bypassed, kind }
}

fn params_of(a: &BTreeMap<String, m::Envelope>) -> BTreeMap<String, Vec<EnvNode>> {
    a.iter().map(|(k, e)| (k.clone(), e.nodes.iter().map(|n| EnvNode { pos: n.pos, value: n.value }).collect())).collect()
}

fn automation_of(p: &BTreeMap<String, Vec<EnvNode>>) -> BTreeMap<String, m::Envelope> {
    p.iter()
        .map(|(k, v)| (k.clone(), m::Envelope { nodes: v.iter().map(|n| m::Node { pos: n.pos, value: n.value }).collect() }))
        .collect()
}

const GAIN_KEY: &str = "Volume-Volume";

impl Clip {
    fn from_track(t: &m::Track, ids: &mut Ids) -> Clip {
        let (id, legacy) = ids.pick(&t.position.session_event_id);
        let proc_ids: Vec<(String, Option<String>)> = t
            .processes
            .iter()
            .map(|p| {
                ids.pick(match p {
                    m::Process::Tool(x) => &x.position.session_event_id,
                    m::Process::Loop(x) => &x.position.session_event_id,
                    m::Process::Ignore(x) => &x.position.session_event_id,
                    m::Process::Reverse(x) => &x.position.session_event_id,
                })
            })
            .collect();
        let processes = t
            .processes
            .iter()
            .zip(&proc_ids)
            .map(|(p, (pid, plegacy))| match p {
                m::Process::Tool(x) => ProcessDoc::Tool(ToolDoc {
                    id: pid.clone(),
                    legacy_session_id: plegacy.clone(),
                    span: Span::from_position(&x.position),
                    tool_type: String::new(),
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                    effects: x.effects.iter().map(effect_doc).collect(),
                    params: params_of(&x.automation),
                }),
                m::Process::Loop(x) => ProcessDoc::Loop(LoopDoc {
                    id: pid.clone(),
                    legacy_session_id: plegacy.clone(),
                    span: Span::from_position(&x.position),
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                    iteration_length: x.length,
                    iterations: x.iterations,
                    re_loop: x.re_loop,
                    mode: x.mode,
                    parent: x.parent.and_then(|i| proc_ids.get(i)).map(|p| p.0.clone()),
                    offsets: x.offsets.iter().map(|o| LoopOffsetDoc { iteration: o.iteration, a: o.a, offset: o.offset }).collect(),
                }),
                m::Process::Ignore(x) => ProcessDoc::Ignore(IgnoreDoc {
                    id: pid.clone(),
                    legacy_session_id: plegacy.clone(),
                    span: Span::from_position(&x.position),
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                    start_pos: x.start_pos,
                }),
                m::Process::Reverse(x) => ProcessDoc::Reverse(ReverseDoc {
                    id: pid.clone(),
                    legacy_session_id: plegacy.clone(),
                    span: Span::from_position(&x.position),
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                }),
            })
            .collect();
        let mut other = params_of(&t.volume.automation);
        let gain = other.remove(GAIN_KEY);
        let mut clip = Clip {
            id,
            legacy_session_id: legacy,
            track_id: t.library_id,
            channel: t.channel,
            span: Span::from_position(&t.position),
            visible_start: 0.0,
            visible_end: 0.0,
            visible_start_sec: 0.0,
            visible_end_sec: 0.0,
            bpm: t.bpm,
            tempo: t.pitch.iter().map(|n| TempoNode { pos: n.position, speed: n.speed }).collect(),
            pitch_mode: t.non_interpolated,
            key: t.key.iter().map(|n| KeyNodeDoc { pos: n.position, cents: n.cents, flags: n.flags }).collect(),
            gain: gain.clone().unwrap_or_default(),
            processes,
            file_path: t.file_path.clone(),
            offset: t.offset,
            clip_name: t.clip_name.clone(),
            clip_bypassed: t.clip_bypassed,
            tonium_id: t.tonium_id.clone(),
            other_ids: t.other_ids.clone(),
            fuzzy_id: FuzzyIdDoc {
                artist: t.fuzzy_id.artist.clone(),
                album: t.fuzzy_id.album.clone(),
                title: t.fuzzy_id.title.clone(),
                genre: t.fuzzy_id.genre.clone(),
            },
            volume: VolumeShell {
                name: t.volume.name.clone(),
                bypassed: t.volume.bypassed,
                effects: t.volume.effects.iter().map(effect_doc).collect(),
                span: Span::from_position(&t.volume.position),
                session_event_id: t.volume.position.session_event_id.clone(),
                other_params: other,
                gain_key_present: gain.is_some(),
            },
            track: None,
        };
        clip.refresh();
        clip
    }

    fn to_track(&self, process_ids: &[String]) -> Result<m::Track, String> {
        let label = &self.id;
        let mut processes = Vec::with_capacity(self.processes.len());
        for (i, p) in self.processes.iter().enumerate() {
            processes.push(match p {
                ProcessDoc::Tool(x) => m::Process::Tool(m::Tool {
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                    effects: x.effects.iter().map(effect_of).collect(),
                    position: x.span.to_position(&x.id, &x.legacy_session_id),
                    automation: automation_of(&x.params),
                }),
                ProcessDoc::Loop(x) => {
                    let parent = match &x.parent {
                        None => None,
                        Some(pid) => {
                            let idx = process_ids.iter().position(|q| q == pid);
                            match idx {
                                Some(j) if j < i && matches!(self.processes[j], ProcessDoc::Loop(_)) => Some(j),
                                _ => return Err(format!("clip {label}: loop {} continues {pid}, which is not an earlier loop of the clip", x.id)),
                            }
                        }
                    };
                    m::Process::Loop(m::LoopProc {
                        name: x.name.clone(),
                        bypassed: x.bypassed,
                        position: x.span.to_position(&x.id, &x.legacy_session_id),
                        length: x.iteration_length,
                        iterations: x.iterations,
                        re_loop: x.re_loop,
                        mode: x.mode,
                        parent,
                        offsets: x.offsets.iter().map(|o| m::LoopOffset { iteration: o.iteration, a: o.a, offset: o.offset }).collect(),
                    })
                }
                ProcessDoc::Ignore(x) => m::Process::Ignore(m::IgnoreProc {
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                    position: x.span.to_position(&x.id, &x.legacy_session_id),
                    start_pos: x.start_pos,
                }),
                ProcessDoc::Reverse(x) => m::Process::Reverse(m::ReverseProc {
                    name: x.name.clone(),
                    bypassed: x.bypassed,
                    position: x.span.to_position(&x.id, &x.legacy_session_id),
                }),
            });
        }
        let mut automation = automation_of(&self.volume.other_params);
        if self.volume.gain_key_present || !self.gain.is_empty() {
            automation.insert(
                GAIN_KEY.to_string(),
                m::Envelope { nodes: self.gain.iter().map(|n| m::Node { pos: n.pos, value: n.value }).collect() },
            );
        }
        let mut vpos = self.volume.span.to_position("", &None);
        vpos.session_event_id = self.volume.session_event_id.clone();
        Ok(m::Track {
            clip_name: self.clip_name.clone(),
            clip_bypassed: self.clip_bypassed,
            processes,
            position: self.span.to_position(&self.id, &self.legacy_session_id),
            library_id: self.track_id,
            tonium_id: self.tonium_id.clone(),
            other_ids: self.other_ids.clone(),
            fuzzy_id: m::FuzzyId {
                artist: self.fuzzy_id.artist.clone(),
                album: self.fuzzy_id.album.clone(),
                title: self.fuzzy_id.title.clone(),
                genre: self.fuzzy_id.genre.clone(),
            },
            file_path: self.file_path.clone(),
            bpm: self.bpm,
            channel: self.channel,
            offset: self.offset,
            non_interpolated: self.pitch_mode,
            pitch: self.tempo.iter().map(|n| m::SpeedNode { position: n.pos, speed: n.speed }).collect(),
            key: self.key.iter().map(|n| m::KeyNode { position: n.pos, cents: n.cents, flags: n.flags }).collect(),
            volume: m::Tool {
                name: self.volume.name.clone(),
                bypassed: self.volume.bypassed,
                effects: self.volume.effects.iter().map(effect_of).collect(),
                position: vpos,
                automation,
            },
        })
    }
}

impl TransitionDoc {
    fn from_transition(t: &m::Transition, ids: &mut Ids) -> TransitionDoc {
        let (id, legacy) = ids.pick(&t.position.session_event_id);
        TransitionDoc {
            id,
            legacy_session_id: legacy,
            span: Span::from_position(&t.position),
            name: t.name.clone(),
            bypassed: t.bypassed,
            beat_match: t.beat_match,
            crossfade_type: t.crossfade_type,
            curve: t.envelope.iter().map(|n| CurveNode { x: f64::from(n.pos) / f64::from(u32::MAX), value: n.value }).collect(),
            between: None,
        }
    }

    fn to_transition(&self) -> m::Transition {
        m::Transition {
            name: self.name.clone(),
            bypassed: self.bypassed,
            position: self.span.to_position(&self.id, &self.legacy_session_id),
            beat_match: self.beat_match,
            envelope: self
                .curve
                .iter()
                .map(|n| m::Node { pos: (n.x.clamp(0.0, 1.0) * f64::from(u32::MAX)).round() as u32, value: n.value })
                .collect(),
            crossfade_type: self.crossfade_type,
        }
    }
}

impl MixDoc {
    /// The editor model of `mix`. `id` is the library's `mix_id`.
    pub fn from_mix(id: i64, mix: &m::Mix) -> MixDoc {
        let mut ids = Ids::default();
        let clips: Vec<Clip> = mix.tracks.iter().map(|t| Clip::from_track(t, &mut ids)).collect();
        let transitions = mix.transitions.iter().map(|t| TransitionDoc::from_transition(t, &mut ids)).collect();
        let mut doc = MixDoc {
            id,
            name: mix.name.clone(),
            genre: mix.genre.clone(),
            artist: mix.artist.clone(),
            device_info: mix.device_info.clone(),
            sample_rate: SAMPLE_RATE as u32,
            tempo_bpm: None,
            clips,
            transitions,
            length: 0.0,
            length_sec: 0.0,
            meta: None,
        };
        doc.tempo_bpm = doc.derive_tempo();
        doc.refresh();
        doc
    }

    /// The `Mix` this document describes. Fails only on structural nonsense (a loop whose parent
    /// is not an earlier loop of the same clip); [`MixDoc::validate`] covers everything else.
    pub fn to_mix(&self) -> Result<m::Mix, String> {
        let mut tracks = Vec::with_capacity(self.clips.len());
        for c in &self.clips {
            let ids: Vec<String> = c.processes.iter().map(|p| p.id().to_string()).collect();
            tracks.push(c.to_track(&ids)?);
        }
        Ok(m::Mix {
            name: self.name.clone(),
            device_info: self.device_info.clone(),
            genre: self.genre.clone(),
            artist: self.artist.clone(),
            tracks,
            transitions: self.transitions.iter().map(TransitionDoc::to_transition).collect(),
        })
    }

    /// Effective tempo of the clip that starts first, at its cue.
    fn derive_tempo(&self) -> Option<f64> {
        let first = self
            .clips
            .iter()
            .min_by(|a, b| (a.span.start + a.span.trim_start).total_cmp(&(b.span.start + b.span.trim_start)))?;
        let t = first.effective_bpm(first.span.trim_start);
        (t.is_finite() && t > 0.0).then_some(t)
    }

    /// Recomputes every derived field (seconds, visible ranges, tool types, mix length, the
    /// transition-to-clip links).
    pub fn refresh(&mut self) {
        for c in &mut self.clips {
            c.refresh();
        }
        for t in &mut self.transitions {
            t.span.refresh();
        }
        self.length = self.clips.iter().map(|c| c.visible_end).fold(0.0, f64::max);
        self.length_sec = secs(self.length);
        self.link_transitions();
    }

    /// Sets `between` on every transition: the clip whose audible start is nearest the
    /// transition's start (within the transition's length) and the other clip with the largest
    /// overlap with it that started earlier.
    fn link_transitions(&mut self) {
        let links: Vec<Option<Between>> = self
            .transitions
            .iter()
            .map(|t| {
                let (a, b) = (t.span.start, t.span.end());
                let to = self
                    .clips
                    .iter()
                    .filter(|c| (c.visible_start - a).abs() <= t.span.length.max(SAMPLE_RATE))
                    .min_by(|x, y| (x.visible_start - a).abs().total_cmp(&(y.visible_start - a).abs()))?;
                let from = self
                    .clips
                    .iter()
                    .filter(|c| c.id != to.id && c.visible_start < to.visible_start)
                    .map(|c| (c, (c.visible_end.min(b) - c.visible_start.max(a)).max(0.0)))
                    .filter(|(_, o)| *o > 0.0)
                    .max_by(|x, y| x.1.total_cmp(&y.1))?
                    .0;
                Some(Between { from: from.id.clone(), to: to.id.clone() })
            })
            .collect();
        for (t, l) in self.transitions.iter_mut().zip(links) {
            t.between = l;
        }
    }

    pub fn clip(&self, id: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }
}

// ---------------------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------------------

fn finite(v: f64) -> bool {
    v.is_finite()
}

fn envelope_problems(what: &str, nodes: &[u32], out: &mut Vec<String>) {
    if nodes.windows(2).any(|w| w[1] < w[0]) {
        out.push(format!("{what}: nodes are not in position order"));
    }
}

impl MixDoc {
    /// Everything that would make the document unsafe to write (empty = fine). `known_tracks`
    /// are the library's track ids; pass `None` to skip the library check.
    pub fn validate(&self, known_tracks: Option<&HashSet<i64>>) -> Vec<String> {
        let mut out = Vec::new();
        let name = self.name.trim();
        if name.is_empty() {
            out.push("the mix name is empty".into());
        } else if self.name.chars().count() > pm_formats::library::MAX_MIX_NAME_CHARS {
            out.push(format!("the mix name is longer than {} characters", pm_formats::library::MAX_MIX_NAME_CHARS));
        }
        if self.sample_rate != SAMPLE_RATE as u32 {
            out.push(format!("sample rate {} is not {}", self.sample_rate, SAMPLE_RATE as u32));
        }

        // ids: non-empty and unique across clips, processes and transitions
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut note_id = |id: &str, out: &mut Vec<String>| {
            if id.is_empty() {
                out.push("an item has an empty id".into());
            } else {
                let n = seen.entry(id.to_string()).or_default();
                *n += 1;
                if *n == 2 {
                    out.push(format!("id {id} is used twice"));
                }
            }
        };
        for c in &self.clips {
            note_id(&c.id, &mut out);
            for p in &c.processes {
                note_id(p.id(), &mut out);
            }
        }
        for t in &self.transitions {
            note_id(&t.id, &mut out);
        }

        for (n, c) in self.clips.iter().enumerate() {
            let label = match &c.track {
                Some(t) if !t.title.is_empty() => format!("clip {} ({})", n + 1, t.title),
                _ => format!("clip {} (track {})", n + 1, c.track_id),
            };
            if let Some(known) = known_tracks
                && c.track_id >= 0
                && !known.contains(&c.track_id)
            {
                out.push(format!("{label}: track {} is not in the library", c.track_id));
            }
            if c.channel > 1 {
                out.push(format!("{label}: channel {} (only 0 and 1 exist)", c.channel));
            }
            let s = &c.span;
            if ![s.start, s.length, s.trim_start, s.trim_end, c.bpm].iter().all(|v| finite(*v)) {
                out.push(format!("{label}: a position is not a number"));
                continue;
            }
            if s.length <= 0.0 || s.trim_start < 0.0 || s.trim_end < 0.0 || s.trim_start + s.trim_end >= s.length {
                out.push(format!("{label}: length {} with trims {} / {} leaves nothing to play", s.length, s.trim_start, s.trim_end));
            }
            if c.visible_start_of() < -EPS {
                out.push(format!("{label}: starts before the beginning of the mix"));
            }
            if c.bpm <= 0.0 {
                out.push(format!("{label}: the track tempo must be positive"));
            }
            if c.tempo.is_empty() {
                out.push(format!("{label}: the tempo curve has no node"));
            }
            if c.tempo.iter().any(|t| !t.speed.is_finite() || t.speed <= 0.0) {
                out.push(format!("{label}: a tempo node has a speed that is not positive"));
            }
            envelope_problems(&format!("{label} tempo"), &c.tempo.iter().map(|n| n.pos).collect::<Vec<_>>(), &mut out);
            envelope_problems(&format!("{label} key"), &c.key.iter().map(|n| n.pos).collect::<Vec<_>>(), &mut out);
            envelope_problems(&format!("{label} gain"), &c.gain.iter().map(|n| n.pos).collect::<Vec<_>>(), &mut out);
            if c.key.iter().any(|n| !n.cents.is_finite()) || c.gain.iter().any(|n| !n.value.is_finite()) {
                out.push(format!("{label}: an envelope value is not a number"));
            }

            let local_end = s.length;
            let mut tools: Vec<&ToolDoc> = Vec::new();
            for p in &c.processes {
                let ps = p.span();
                if ![ps.start, ps.length].iter().all(|v| finite(*v)) || ps.length <= 0.0 || ps.start < -EPS {
                    out.push(format!("{label}: process {} has an invalid range", p.id()));
                    continue;
                }
                if ps.end() > local_end + EPS {
                    out.push(format!("{label}: process {} runs past the end of the clip", p.id()));
                }
                match p {
                    ProcessDoc::Tool(t) => {
                        if t.effects.is_empty() {
                            out.push(format!("{label}: tool {} has no effect", t.id));
                        }
                        for e in &t.effects {
                            let keys = effect_of(e).automation_keys();
                            for k in &keys {
                                if !t.params.contains_key(k) {
                                    out.push(format!("{label}: tool {} ({}) lacks the envelope {k:?}", t.id, e.name));
                                }
                            }
                            if !keys.is_empty() {
                                for k in t.params.keys() {
                                    if !keys.contains(k) {
                                        out.push(format!("{label}: tool {} ({}) has an unknown envelope {k:?}", t.id, e.name));
                                    }
                                }
                            }
                        }
                        for (k, v) in &t.params {
                            envelope_problems(&format!("{label} tool {} {k}", t.id), &v.iter().map(|n| n.pos).collect::<Vec<_>>(), &mut out);
                            if v.iter().any(|n| !n.value.is_finite()) {
                                out.push(format!("{label}: tool {} {k} has a value that is not a number", t.id));
                            }
                        }
                        tools.push(t);
                    }
                    ProcessDoc::Loop(l) if l.iterations == 0 || l.iteration_length.is_nan() || l.iteration_length <= 0.0 => {
                        out.push(format!("{label}: loop {} needs at least one iteration of positive length", l.id));
                    }
                    _ => {}
                }
            }
            // tools of one kind do not overlap (ToolOperations::adjustNotAllowedToOverlappTools)
            for (i, a) in tools.iter().enumerate() {
                for b in &tools[i + 1..] {
                    if a.tool_name() == b.tool_name()
                        && a.span.start < b.span.end() - EPS
                        && b.span.start < a.span.end() - EPS
                    {
                        out.push(format!("{label}: two {} tools overlap", a.tool_name()));
                    }
                }
            }
            if let Err(e) = c.to_track(&c.processes.iter().map(|p| p.id().to_string()).collect::<Vec<_>>()) {
                out.push(e);
            }
        }

        // clips on one channel may only overlap where a transition covers the overlap
        for (i, a) in self.clips.iter().enumerate() {
            for b in &self.clips[i + 1..] {
                if a.channel != b.channel {
                    continue;
                }
                let (lo, hi) = (a.visible_start_of().max(b.visible_start_of()), a.visible_end_of().min(b.visible_end_of()));
                if hi - lo > EPS {
                    let covered = self.transitions.iter().any(|t| {
                        (t.span.end().min(hi) - t.span.start.max(lo)) >= 0.5 * (hi - lo)
                    });
                    if !covered {
                        out.push(format!(
                            "two clips overlap on channel {} from {:.2}s to {:.2}s without a transition (move one to the other channel)",
                            a.channel,
                            secs(lo),
                            secs(hi)
                        ));
                    }
                }
            }
        }

        for t in &self.transitions {
            let s = &t.span;
            if ![s.start, s.length].iter().all(|v| finite(*v)) || s.length <= 0.0 || s.start < -EPS {
                out.push(format!("transition {} has an invalid range", t.id));
            }
            if t.curve.iter().any(|n| !(0.0..=1.0).contains(&n.x) || !(0.0..=1.0).contains(&n.value)) {
                out.push(format!("transition {}: curve points must lie within 0..1", t.id));
            }
            if t.curve.windows(2).any(|w| w[1].x < w[0].x) {
                out.push(format!("transition {}: curve points are not in order", t.id));
            }
            if t.curve.is_empty() {
                out.push(format!("transition {} has no curve", t.id));
            }
        }
        out
    }
}

impl Clip {
    fn visible_start_of(&self) -> f64 {
        self.span.start + self.span.trim_start
    }
    fn visible_end_of(&self) -> f64 {
        self.span.start + self.span.length - self.span.trim_end
    }
}

impl ToolDoc {
    /// The tool's kind for the "no two of a kind overlap" rule.
    fn tool_name(&self) -> &str {
        self.effects.first().map(|e| e.name.as_str()).unwrap_or("")
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn pos(start: f64, length: f64, id: &str) -> m::Position {
        m::Position { start, length, limit_size: true, session_event_id: id.into(), ..m::Position::default() }
    }

    /// A synthetic mix that touches every shape the model carries: three clips on two channels
    /// with tempo ramps, trims, key and gain envelopes, one of every tool, a loop chain, ignore
    /// and reverse regions, and two transitions.
    pub fn synthetic_mix() -> m::Mix {
        let mut mix = m::Mix::new("Synthetic Test Mix");
        mix.genre = "House".into();
        mix.artist = "Example".into();
        let lens = [13_000_000.0, 17_000_000.0, 12_000_000.0];
        let starts = [0.0, 1_000_000.0, 6_000_000.0];
        let lo = [0.0, 9_900_000.0, 5_000_000.0];
        let ids = ["a1", "b2", "c3"];
        for i in 0..3 {
            let mut p = pos(starts[i], lens[i], &format!("{:0>32}", ids[i]));
            p.l_offset = lo[i];
            let mut t = m::Track::new(10 + i as i64, &format!("{i}/{i}/file{i}.mp3"), 126.5 + i as f64, 1.012, p);
            t.channel = (i % 2) as u32;
            t.fuzzy_id = m::FuzzyId { artist: format!("Artist {i}"), album: "Album".into(), title: format!("Title {i}"), genre: "House".into() };
            t.pitch = vec![
                m::SpeedNode { position: 0, speed: 1.012 },
                m::SpeedNode { position: 5_000_000, speed: 1.012 },
                m::SpeedNode { position: 6_000_000, speed: 1.05 },
            ];
            t.key = vec![m::KeyNode { position: 0, cents: 0.0, flags: 1 }, m::KeyNode { position: 7_000_000, cents: -300.5, flags: 1 }];
            t.volume.automation.insert(
                "Volume-Volume".into(),
                m::Envelope { nodes: vec![m::Node { pos: 0, value: 0.0 }, m::Node { pos: 8_000_000, value: -3.25 }] },
            );
            let mut st = 100.0;
            let mut tool = |e: m::Effect, len: f64, n: &mut u32| {
                *n += 1;
                let tl = m::Tool::flat(e, st, len, 0.3, format!("{:032x}", 0x1000 * (i as u32 + 1) + *n));
                st += len + 10.0;
                m::Process::Tool(tl)
            };
            let mut n = 0;
            for e in [m::Effect::reverb(), m::Effect::filter(), m::Effect::crush(), m::Effect::echo(), m::Effect::roll(), m::Effect::eq(), m::Effect::wah()] {
                let tl = tool(e, 50_000.0, &mut n);
                t.processes.push(tl);
            }
            let mut bypassed = m::Tool::flat(m::Effect::volume(), 2_000_000.0, 30_000.0, 0.0, format!("{:032x}", 0xfff0 + i));
            bypassed.bypassed = true;
            t.processes.push(m::Process::Tool(bypassed));
            if i == 1 {
                let lp = |parent: Option<usize>, st: f64, id: &str| m::LoopProc {
                    name: "LOOP".into(),
                    bypassed: false,
                    position: pos(st, 90_000.0, id),
                    length: 45_000.0,
                    iterations: 2,
                    re_loop: parent.is_some(),
                    mode: false,
                    parent,
                    offsets: vec![m::LoopOffset { iteration: 1, a: 0.0, offset: -42100.23 }],
                };
                let first = t.processes.len();
                t.processes.push(m::Process::Loop(lp(None, 3_000_000.0, &format!("{:032x}", 0xa001))));
                t.processes.push(m::Process::Loop(lp(Some(first), 3_090_000.0, &format!("{:032x}", 0xa002))));
                t.processes.push(m::Process::Ignore(m::IgnoreProc {
                    name: "IGNORE".into(),
                    bypassed: false,
                    position: pos(4_000_000.0, 20_000.0, &format!("{:032x}", 0xa003)),
                    start_pos: 4_000_000,
                }));
                t.processes.push(m::Process::Reverse(m::ReverseProc {
                    name: "REVERSE".into(),
                    bypassed: true,
                    position: pos(4_500_000.0, 20_000.0, &format!("{:032x}", 0xa004)),
                }));
                t.processes.push(m::Process::Tool(m::Tool {
                    name: "ToolProcess".into(),
                    bypassed: false,
                    effects: vec![m::Effect {
                        name: "Trans".into(),
                        bypassed: false,
                        kind: m::EffectKind::Trans { start_bpm: 128.5, active: true },
                    }],
                    position: pos(5_000_000.0, 40_000.0, &format!("{:032x}", 0xa005)),
                    automation: BTreeMap::new(),
                }));
            }
            mix.tracks.push(t);
        }
        let mut tr1 = m::Transition::new(m::Position { start: 10_900_000.0, length: 2_100_000.0, session_event_id: format!("{:032x}", 0xb001), ..Default::default() });
        tr1.envelope = vec![m::Node { pos: 0, value: 0.0 }, m::Node { pos: 1_234_567_891, value: 0.340_740_7 }, m::Node { pos: u32::MAX, value: 1.0 }];
        tr1.crossfade_type = 3;
        tr1.beat_match = true;
        mix.transitions.push(tr1);
        mix.transitions.push(m::Transition::new(m::Position { start: 11_000_000.0, length: 2_000_000.0, session_event_id: format!("{:032x}", 0xb002), ..Default::default() }));
        mix
    }

    #[test]
    fn round_trip_is_identical() {
        let mix = synthetic_mix();
        let doc = MixDoc::from_mix(7, &mix);
        assert_eq!(doc.to_mix().unwrap(), mix);
        assert_eq!(doc.to_mix().unwrap().to_blob().unwrap(), mix.to_blob().unwrap());
        // through JSON, as the UI sends it
        let json = serde_json::to_string(&doc).unwrap();
        let back: MixDoc = serde_json::from_str(&json).unwrap();
        let (a, b) = (format!("{doc:?}"), format!("{back:?}"));
        if a != b {
            let i = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap();
            panic!("first difference: {} <> {}", &a[i.saturating_sub(80)..(i + 60).min(a.len())], &b[i.saturating_sub(80)..(i + 60).min(b.len())]);
        }
        assert_eq!(back, doc);
        assert_eq!(back.to_mix().unwrap(), mix);
        // the UI may drop every derived field and the library facts
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for c in v["clips"].as_array_mut().unwrap() {
            for k in ["startSec", "lengthSec", "visibleStart", "visibleEnd", "visibleStartSec", "visibleEndSec", "track"] {
                c.as_object_mut().unwrap().remove(k);
            }
        }
        v.as_object_mut().unwrap().remove("meta");
        let back: MixDoc = serde_json::from_value(v).unwrap();
        assert_eq!(back.to_mix().unwrap(), mix);
    }

    #[test]
    fn loop_chain_uses_ids_not_indexes() {
        let mix = synthetic_mix();
        let doc = MixDoc::from_mix(1, &mix);
        let loops: Vec<&LoopDoc> = doc.clips[1].processes.iter().filter_map(|p| if let ProcessDoc::Loop(l) = p { Some(l) } else { None }).collect();
        assert_eq!(loops.len(), 2);
        assert_eq!(loops[1].parent.as_deref(), Some(loops[0].id.as_str()));
        // inserting a process before the chain moves indexes but not the link
        let mut d = doc.clone();
        let extra = d.clips[1].processes[0].clone();
        let mut extra = extra;
        if let ProcessDoc::Tool(t) = &mut extra {
            t.id = "ff".repeat(16);
            t.span.start = 700_000.0;
        }
        d.clips[1].processes.insert(0, extra);
        let m2 = d.to_mix().unwrap();
        let lps: Vec<&m::LoopProc> = m2.tracks[1].processes.iter().filter_map(|p| if let m::Process::Loop(l) = p { Some(l) } else { None }).collect();
        let first_idx = m2.tracks[1].processes.iter().position(|p| matches!(p, m::Process::Loop(_))).unwrap();
        assert_eq!(lps[1].parent, Some(first_idx));
        assert!(m2.to_blob().is_ok());
        // a parent that comes later is refused
        let mut bad = doc.clone();
        let n = bad.clips[1].processes.len();
        let later = bad.clips[1].processes[n - 3].id().to_string();
        if let ProcessDoc::Loop(l) = &mut bad.clips[1].processes[n - 4] {
            l.parent = Some(later);
        }
        assert!(bad.to_mix().is_err());
    }

    #[test]
    fn duplicate_and_empty_session_ids_still_round_trip() {
        let mut mix = synthetic_mix();
        let same = mix.tracks[0].position.session_event_id.clone();
        mix.tracks[1].position.session_event_id = same;
        mix.transitions[0].position.session_event_id = String::new();
        let doc = MixDoc::from_mix(1, &mix);
        assert!(doc.clips[1].id.starts_with(AUTO_PREFIX) && doc.transitions[0].id.starts_with(AUTO_PREFIX));
        assert_ne!(doc.clips[0].id, doc.clips[1].id);
        assert_eq!(doc.to_mix().unwrap(), mix);
        assert!(doc.validate(None).is_empty(), "{:?}", doc.validate(None));
    }

    #[test]
    fn derived_fields_and_links() {
        let doc = MixDoc::from_mix(1, &synthetic_mix());
        let c = &doc.clips[1];
        assert_eq!(c.visible_start, 1_000_000.0 + 9_900_000.0);
        assert_eq!(c.visible_end, 1_000_000.0 + 17_000_000.0);
        assert!((c.visible_start_sec - 10_900_000.0 / 44100.0).abs() < 1e-9);
        assert_eq!(doc.length, 18_000_000.0);
        match &c.processes[0] {
            ProcessDoc::Tool(t) => assert_eq!(t.tool_type, "Reverb"),
            _ => panic!(),
        }
        // transition 1 starts where clip 2 becomes audible and sits inside clip 1's tail
        // (clip 1 ends at 13 000 000)
        let b = doc.transitions[0].between.as_ref().unwrap();
        assert_eq!((b.from.as_str(), b.to.as_str()), (doc.clips[0].id.as_str(), doc.clips[1].id.as_str()));
        let expect = doc.clips[1].effective_bpm(9_900_000.0);
        assert!((doc.tempo_bpm.unwrap() - doc.clips[0].effective_bpm(0.0)).abs() < 1e-9);
        assert!(expect > 0.0);
    }

    #[test]
    fn validation_accepts_the_synthetic_mix_and_names_each_problem() {
        let doc = MixDoc::from_mix(1, &synthetic_mix());
        let known: HashSet<i64> = [10, 11, 12].into_iter().collect();
        assert_eq!(doc.validate(Some(&known)), Vec::<String>::new());

        let mut d = doc.clone();
        d.name = "  ".into();
        d.clips[0].track_id = 99;
        d.clips[1].channel = 2;
        d.clips[2].span.length = -1.0;
        d.clips[0].tempo.clear();
        let probs = d.validate(Some(&known)).join(" | ");
        for needle in ["name is empty", "track 99 is not in the library", "channel 2", "leaves nothing to play", "no node"] {
            assert!(probs.contains(needle), "{needle} not in {probs}");
        }

        // same-channel overlap: refused without a covering transition, fine with one
        let mut d = doc.clone();
        d.clips[1].channel = 0;
        assert!(d.validate(None).iter().any(|p| p.contains("overlap on channel 0")));
        d.transitions.clear();
        assert!(d.validate(None).iter().any(|p| p.contains("overlap on channel 0")));
        let mut d = doc.clone();
        d.clips[1].channel = 0;
        d.clips[2].channel = 1;
        d.transitions[0].span.start = d.clips[1].visible_start;
        d.transitions[0].span.length = d.clips[0].visible_end - d.clips[1].visible_start;
        assert!(d.validate(None).is_empty(), "{:?}", d.validate(None));

        // two tools of one kind overlapping; a missing envelope; unsorted nodes
        let mut d = doc.clone();
        let t0 = d.clips[0].processes[0].clone();
        let mut dup = t0.clone();
        if let ProcessDoc::Tool(t) = &mut dup {
            t.id = "ee".repeat(16);
            t.span.start += 10.0;
        }
        d.clips[0].processes.push(dup);
        if let ProcessDoc::Tool(t) = &mut d.clips[0].processes[1] {
            t.params.remove("FILTER-CutOff");
        }
        if let ProcessDoc::Tool(t) = &mut d.clips[0].processes[2] {
            t.params.get_mut("Crush-Crush").unwrap().reverse();
        }
        let probs = d.validate(None).join(" | ");
        for needle in ["two Reverb tools overlap", "lacks the envelope", "not in position order"] {
            assert!(probs.contains(needle), "{needle} not in {probs}");
        }

        // duplicate id
        let mut d = doc.clone();
        d.transitions[1].id = d.transitions[0].id.clone();
        assert!(d.validate(None).iter().any(|p| p.contains("used twice")));
    }

    #[test]
    fn crossfade_curve_positions_survive_exactly() {
        let mut mix = synthetic_mix();
        for pos in [0u32, 1, 2, 4_294_967_294, u32::MAX, 123_456_789, 2_147_483_648, 3_000_000_001] {
            mix.transitions[0].envelope = vec![m::Node { pos, value: 0.5 }];
            assert_eq!(MixDoc::from_mix(1, &mix).to_mix().unwrap(), mix, "{pos}");
        }
    }

    #[test]
    fn fresh_ids_look_like_juce_uuids() {
        let a = fresh_id();
        let b = fresh_id();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b);
    }
}
