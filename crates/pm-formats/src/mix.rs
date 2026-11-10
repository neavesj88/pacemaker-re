//! The Pacemaker mix document (`MixDocument`): a typed model, a parser and a writer.
//!
//! A mix is a boost `xml_oarchive` of the Editor's `MixDocument` class (see notes/mix-format.md
//! for the schema and the evidence behind every field). [`crate::mixdoc`] handles the zlib+XOR
//! container; this module handles the XML inside.
//!
//! Round trip: [`Mix::parse`] followed by [`Mix::to_xml`] reproduces the Editor's bytes
//! exactly. That works because everything boost adds on top of the data (`class_id`,
//! `object_id`, `tracking_level`, `version`, `class_name`, `class_id_reference`, ...) is a pure
//! function of the document, so the model does not store it and the writer regenerates it by
//! the same rules boost applies (first appearance of a class registers it; tracked objects get
//! sequential ids in pre-order). Floating point values are printed the way the Editor's
//! `std::ostream` does (`%.17g` for doubles, `%.8g` for floats).
//!
//! The parser is strict: any element it does not know is an error rather than being dropped, so
//! a successful parse means the whole document is represented in the model.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::juce_b64;
use crate::strfile::fmt_g;

/// Engine sample rate all positions and lengths are expressed in (inferred: the Editor's
/// `getMixDocumentPlayTimeSecs` divides by the engine rate; the demo mix is consistent with
/// 44.1 kHz).
pub const SAMPLE_RATE: f64 = 44_100.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixError {
    /// The text is not the XML dialect boost writes.
    Xml(String),
    /// Well-formed XML that is not a document this model can represent.
    Format(String),
}

impl fmt::Display for MixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MixError::Xml(m) => write!(f, "mix xml: {m}"),
            MixError::Format(m) => write!(f, "mix document: {m}"),
        }
    }
}

impl std::error::Error for MixError {}

type Res<T> = Result<T, MixError>;

fn bad<T>(m: impl Into<String>) -> Res<T> {
    Err(MixError::Format(m.into()))
}

// ---------------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------------

/// A whole mix document.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mix {
    /// `DocumentName`; the Editor keeps it equal to `mixes.name`.
    pub name: String,
    /// `DeviceInfo` (empty in every known document).
    pub device_info: String,
    pub genre: String,
    pub artist: String,
    pub tracks: Vec<Track>,
    pub transitions: Vec<Transition>,
}

/// A position on the mix timeline: start and length in samples at [`SAMPLE_RATE`], both
/// fractional. This is boost's `PositionedEvent`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Position {
    pub start: f64,
    pub length: f64,
    /// Trim at the left / right edge, in samples (always 0 in known documents).
    pub l_offset: f64,
    pub r_offset: f64,
    /// `LimitSize`: 1 on a track's own event (its length is bounded by the audio file), 0 on
    /// tools and on the volume envelope.
    pub limit_size: bool,
    /// `SessionEventId`: 32 lowercase hex characters (a `juce::Uuid`) on tools, loops and
    /// transitions and tracks; empty on a track's volume envelope.
    pub session_event_id: String,
}

/// Library-independent identity of a track (`GlobalContext.fuzzyId`): what the Editor falls
/// back to when `libraryID` does not resolve.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FuzzyId {
    pub artist: String,
    pub album: String,
    pub title: String,
    pub genre: String,
}

/// One envelope node: `pos` is a sample offset from the start of the owning event, `value` the
/// parameter's real-world value (dB, Hz, ms, ... depending on the parameter).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub pos: u32,
    pub value: f32,
}

/// An automation envelope. In the document it is a single string: JUCE base64 of a little-endian
/// `u32` node count followed by `count` x (`u32` position, `f32` value).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Envelope {
    pub nodes: Vec<Node>,
}

impl Envelope {
    /// A constant envelope over an event of `length` samples: nodes at 0 and `length - 1`.
    /// That is the two-node shape most parameters have in the demo mix; a few (Echo/Roll
    /// "Time") have a single node at 0 instead, which is also valid.
    pub fn flat(length: f64, value: f32) -> Envelope {
        let last = (length.max(1.0) - 1.0).min(f64::from(u32::MAX)) as u32;
        let mut nodes = vec![Node { pos: 0, value }];
        if last > 0 {
            nodes.push(Node { pos: last, value });
        }
        Envelope { nodes }
    }

    pub fn encode(&self) -> String {
        let mut b = Vec::with_capacity(4 + 8 * self.nodes.len());
        b.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for n in &self.nodes {
            b.extend_from_slice(&n.pos.to_le_bytes());
            b.extend_from_slice(&n.value.to_le_bytes());
        }
        juce_b64::encode(&b)
    }

    pub fn decode(s: &str) -> Res<Envelope> {
        let b = juce_b64::decode(s).map_err(|e| MixError::Format(format!("envelope base64: {e:?}")))?;
        let n = read_count(&b, 8)?;
        let nodes = (0..n)
            .map(|i| {
                let o = 4 + 8 * i;
                Node { pos: u32_at(&b, o), value: f32::from_le_bytes(b[o + 4..o + 8].try_into().unwrap()) }
            })
            .collect();
        Ok(Envelope { nodes })
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Checks `b` is `4 + count * stride` bytes and returns `count`.
fn read_count(b: &[u8], stride: usize) -> Res<usize> {
    if b.len() < 4 {
        return bad("envelope shorter than its count");
    }
    let n = u32_at(b, 0) as usize;
    if b.len() != 4 + n * stride {
        return bad(format!("envelope has {} bytes, expected {} for {n} nodes", b.len(), 4 + n * stride));
    }
    Ok(n)
}

/// A node of the track's tempo (`PitchEnvelope`) curve: `speed` is the playback-rate ratio
/// (target BPM / track BPM) from `position` on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedNode {
    pub position: u32,
    pub speed: f32,
}

/// A node of the track's key (`KeyEnvelope`) curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyNode {
    pub position: u32,
    /// Pitch shift in cents (inferred: -300 seen for "3 semitones down", +100.37 for a fine
    /// tune; the Editor derives it from the speed as `log2(speed) * K`).
    pub cents: f32,
    /// Third word of the 12-byte record. Always 1 in known documents and the constant the
    /// Editor writes when it creates a node. Not part of the XML form of the same vector.
    pub flags: u32,
}

/// One effect inside a tool, boost's `*AudioProcess` classes. `name` is the displayed tool name
/// ("Reverb", "FILTER", "Roll", "Wah", "EQ", ...), which also prefixes the automation keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub name: String,
    pub bypassed: bool,
    pub kind: EffectKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectKind {
    /// `FxAudioProcess` (Reverb).
    Fx { active: bool },
    /// `FilterAudioProcess` (name "FILTER").
    Filter,
    /// `CrushAudioProcess`.
    Crush,
    /// `DelayAudioProcess`: `delay_type` is 1 for Echo and 2 for Roll in known documents
    /// (0 = Delay by `setAsDelay`/`setAsEcho`/`setAsRoll` order, unconfirmed).
    Delay { delay_type: u32, active: bool, sync_against_beat: bool },
    /// `EqAudioProcess` (name "EQ", three bands).
    Eq,
    /// `PeqAudioProcess` ("Peaking EQ", used for the Wah tool).
    Peq,
    /// `VolumeAudioProcess`.
    Volume,
    /// `TransAudioProcess` (no sample; layout from the decompiled writer).
    Trans { start_bpm: f32, active: bool },
}

impl EffectKind {
    /// The class name boost exports the effect under.
    pub fn class_name(&self) -> &'static str {
        match self {
            EffectKind::Fx { .. } => "FxAudioProcess",
            EffectKind::Filter => "FilterAudioProcess",
            EffectKind::Crush => "CrushAudioProcess",
            EffectKind::Delay { .. } => "DelayAudioProcess",
            EffectKind::Eq => "EqAudioProcess",
            EffectKind::Peq => "PeqAudioProcess",
            EffectKind::Volume => "VolumeAudioProcess",
            EffectKind::Trans { .. } => "TransAudioProcess",
        }
    }
}

impl Effect {
    pub fn reverb() -> Effect {
        Effect { name: "Reverb".into(), bypassed: false, kind: EffectKind::Fx { active: true } }
    }
    pub fn filter() -> Effect {
        Effect { name: "FILTER".into(), bypassed: false, kind: EffectKind::Filter }
    }
    pub fn crush() -> Effect {
        Effect { name: "Crush".into(), bypassed: false, kind: EffectKind::Crush }
    }
    pub fn echo() -> Effect {
        Effect { name: "Echo".into(), bypassed: false, kind: EffectKind::Delay { delay_type: 1, active: true, sync_against_beat: true } }
    }
    pub fn roll() -> Effect {
        Effect { name: "Roll".into(), bypassed: false, kind: EffectKind::Delay { delay_type: 2, active: true, sync_against_beat: true } }
    }
    pub fn eq() -> Effect {
        Effect { name: "EQ".into(), bypassed: false, kind: EffectKind::Eq }
    }
    pub fn wah() -> Effect {
        Effect { name: "Wah".into(), bypassed: false, kind: EffectKind::Peq }
    }
    pub fn volume() -> Effect {
        Effect { name: "Volume".into(), bypassed: false, kind: EffectKind::Volume }
    }

    /// Automation keys the Editor writes for this effect (`<name>-<parameter>`). The two
    /// trailing-space keys of Echo/Roll are real (the Editor builds them from the string
    /// "Time "). Empty for effects with no sample (Trans, Delay type 0).
    pub fn automation_keys(&self) -> Vec<String> {
        let n = &self.name;
        let p: &[&str] = match &self.kind {
            EffectKind::Fx { .. } => &["Time", "Wet/Dry"],
            EffectKind::Filter => &["CutOff"],
            EffectKind::Crush => &["Crush"],
            EffectKind::Delay { .. } => &["Time", "Time ", "Wet/Dry"],
            EffectKind::Eq => &["Hi", "Lo", "Mid"],
            EffectKind::Peq => &["CutOff"],
            EffectKind::Volume => &["Volume"],
            EffectKind::Trans { .. } => &[],
        };
        p.iter().map(|k| format!("{n}-{k}")).collect()
    }
}

/// An `AutomatedAudioProcess`: a time range during which one effect runs, with its parameter
/// envelopes. Used for the tools on a track's tool lane and, with name "noname", for the
/// track's volume envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// Name of the wrapping `StreamedAudioProcess`: "ToolProcess" for tools, "noname" for the
    /// volume envelope.
    pub name: String,
    pub bypassed: bool,
    pub effects: Vec<Effect>,
    pub position: Position,
    /// Parameter key (`Reverb-Time`) to envelope. Sorted by key as in `std::map`.
    pub automation: BTreeMap<String, Envelope>,
}

impl Tool {
    /// A tool running `effect` over `[start, start + length)` with a flat envelope of
    /// `value` for each of the effect's known parameters.
    pub fn flat(effect: Effect, start: f64, length: f64, value: f32, session_event_id: String) -> Tool {
        let automation =
            effect.automation_keys().into_iter().map(|k| (k, Envelope::flat(length, value))).collect();
        Tool {
            name: "ToolProcess".into(),
            bypassed: false,
            effects: vec![effect],
            position: Position { start, length, session_event_id, ..Position::default() },
            automation,
        }
    }
}

/// `LoopAudioProcess`: repeats a slice of the track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopProc {
    pub name: String,
    pub bypassed: bool,
    pub position: Position,
    /// Length of one iteration in samples.
    pub length: f64,
    pub iterations: u32,
    pub re_loop: bool,
    /// Length mode (the loop tool's "Length" menu), 0 or 1.
    pub mode: bool,
    /// Index into the owning track's `processes` of an earlier [`Process::Loop`] this loop
    /// continues (the "Re-loop" chain), or none.
    pub parent: Option<usize>,
    pub offsets: Vec<LoopOffset>,
}

/// One entry of a loop's offset table: iteration number, then two floats (the second is the
/// sample offset applied to that iteration; the first was 0 in every known document).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoopOffset {
    pub iteration: u32,
    pub a: f32,
    pub offset: f32,
}

/// `IgnoreAudioProcess`: the tool lane's IGNORE region (no sample; layout from the decompiled
/// writer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IgnoreProc {
    pub name: String,
    pub bypassed: bool,
    pub position: Position,
    pub start_pos: u32,
}

/// `ReversalAudioProcess`: the REVERSE region (no sample; layout from the decompiled writer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverseProc {
    pub name: String,
    pub bypassed: bool,
    pub position: Position,
}

/// An item of a track's process list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Process {
    Tool(Tool),
    Loop(LoopProc),
    Ignore(IgnoreProc),
    Reverse(ReverseProc),
}

/// A track on the timeline (boost's `AudioClip`, exported as "Track", class version 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Name / bypass flag of the wrapping `StreamedAudioProcess` ("AudioClip", false).
    pub clip_name: String,
    pub clip_bypassed: bool,
    /// Tools, loops, ignore and reverse regions in the order the Editor keeps them.
    pub processes: Vec<Process>,
    pub position: Position,
    /// `LocalContext.libraryID`: the `tracks.track_id` of the library the mix was saved from.
    pub library_id: i64,
    /// `GlobalContext.toniumId` (empty in known documents).
    pub tonium_id: String,
    /// `GlobalContext.otherIds` (`graceNoteId`, `musicDnsId`; empty values in known documents).
    pub other_ids: BTreeMap<String, String>,
    pub fuzzy_id: FuzzyId,
    /// `FilePath`: hashed name in the library's `Music` folder on a device, an absolute path
    /// when saved by the Editor from its computer library.
    pub file_path: String,
    pub bpm: f64,
    /// Mixer channel (0 or 1: the two decks the Editor alternates).
    pub channel: u32,
    pub offset: u32,
    pub non_interpolated: bool,
    pub pitch: Vec<SpeedNode>,
    pub key: Vec<KeyNode>,
    pub volume: Tool,
}

/// A crossfade between two overlapping tracks (`TransitionAudioProcess`, class version 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub name: String,
    pub bypassed: bool,
    pub position: Position,
    pub beat_match: bool,
    /// Crossfade curve: `pos` is normalised over 0..=4294967295 across the overlap, `value`
    /// the fader position 0..1.
    pub envelope: Vec<Node>,
    /// 1 = fade after 1/2, 3 = fade after 3/4 (`TransitionComponent::onMenuItemClicked`).
    pub crossfade_type: u32,
}

impl Track {
    /// A track with no tools: unity volume, `speed` as its only tempo node, no key shift.
    pub fn new(library_id: i64, file_path: &str, bpm: f64, speed: f32, position: Position) -> Track {
        let length = position.length;
        Track {
            clip_name: "AudioClip".into(),
            clip_bypassed: false,
            processes: Vec::new(),
            position,
            library_id,
            tonium_id: String::new(),
            other_ids: [("graceNoteId".to_string(), String::new()), ("musicDnsId".to_string(), String::new())]
                .into_iter()
                .collect(),
            fuzzy_id: FuzzyId::default(),
            file_path: file_path.to_string(),
            bpm,
            channel: 0,
            offset: 0,
            non_interpolated: false,
            pitch: vec![SpeedNode { position: 0, speed }],
            key: vec![KeyNode { position: 0, cents: 0.0, flags: 1 }],
            volume: Tool {
                name: "noname".into(),
                bypassed: false,
                effects: vec![Effect::volume()],
                position: Position { start: 0.0, length, ..Position::default() },
                automation: [("Volume-Volume".to_string(), Envelope::flat(length, 0.0))].into_iter().collect(),
            },
        }
    }

    /// End of the track on the timeline, in samples.
    pub fn end(&self) -> f64 {
        self.position.start + self.position.length
    }
}

impl Transition {
    /// A 50/50 linear-ish crossfade over `position` (2 nodes: 0 to 1).
    pub fn new(position: Position) -> Transition {
        Transition {
            name: "Transition".into(),
            bypassed: false,
            position,
            beat_match: false,
            envelope: vec![Node { pos: 0, value: 0.0 }, Node { pos: u32::MAX, value: 1.0 }],
            crossfade_type: 1,
        }
    }
}

impl Mix {
    pub fn new(name: &str) -> Mix {
        Mix { name: name.to_string(), ..Mix::default() }
    }

    /// `getMixDocumentPlayTimeSecs`: the latest track end divided by the engine sample rate,
    /// truncated. The Editor-written demo mix stores 600 where this gives 607 (see
    /// notes/mix-format.md, open question), so treat it as the Editor's intent, not proven.
    pub fn play_time_secs(&self) -> u32 {
        let end = self.tracks.iter().map(Track::end).fold(0.0_f64, f64::max);
        (end / SAMPLE_RATE).clamp(0.0, f64::from(u32::MAX)) as u32
    }

    /// The `mixes.preview_markers` string the Editor writes: one `previewmarkers` element per
    /// transition, position -1 when the markers are hidden (inferred from
    /// `ChannelLayoutComponent::getPreviewMarkerPositions`).
    pub fn preview_markers_xml(&self) -> String {
        if self.transitions.is_empty() {
            return "<tracks/>".to_string();
        }
        let mut s = String::from("<tracks>");
        for _ in &self.transitions {
            s.push_str("<previewmarkers position=\"-1\"/>");
        }
        s.push_str("</tracks>");
        s
    }

    /// Library ids of the tracks, in document order.
    pub fn library_ids(&self) -> Vec<i64> {
        self.tracks.iter().map(|t| t.library_id).collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Number formatting (what std::ostream with the archive's precision prints)
// ---------------------------------------------------------------------------------------------

fn f64_text(v: f64) -> String {
    fmt_g(v, 17)
}

fn f32_text(v: f32) -> String {
    fmt_g(f64::from(v), 8)
}

// ---------------------------------------------------------------------------------------------
// XML reading
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct El {
    name: String,
    attrs: Vec<(String, String)>,
    kids: Vec<El>,
    /// Text of a leaf element, unescaped.
    text: String,
}

impl El {
    fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
}

fn unescape(s: &str) -> Res<String> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let j = tail.find(';').ok_or_else(|| MixError::Xml("unterminated entity".into()))?;
        let ent = &tail[1..j];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let n = ent
                    .strip_prefix("#x")
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32)
                    .ok_or_else(|| MixError::Xml(format!("unknown entity &{ent};")))?;
                out.push(n);
            }
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

const PROLOGUE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>\n<!DOCTYPE boost_serialization>\n";
const ROOT_OPEN: &str = "<boost_serialization signature=\"serialization::archive\" version=\"4\">\n";
const ROOT_CLOSE: &str = "</boost_serialization>\n";

/// Parses the element tree below the prologue.
fn parse_tree(s: &str) -> Res<El> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;
    while i < b.len() {
        if b[i] != b'<' {
            let j = s[i..].find('<').map_or(b.len(), |k| i + k);
            let text = &s[i..j];
            match stack.last_mut() {
                Some(top) => top.text.push_str(text),
                None if text.trim().is_empty() => {}
                None => return Err(MixError::Xml("text outside the root element".into())),
            }
            i = j;
            continue;
        }
        if s[i..].starts_with("</") {
            let j = s[i..].find('>').ok_or_else(|| MixError::Xml("unterminated end tag".into()))? + i;
            let name = &s[i + 2..j];
            let mut el = stack.pop().ok_or_else(|| MixError::Xml("end tag with no open element".into()))?;
            if el.name != name {
                return Err(MixError::Xml(format!("</{name}> closes <{}>", el.name)));
            }
            if el.kids.is_empty() {
                el.text = unescape(&el.text)?;
            } else {
                el.text.clear();
            }
            match stack.last_mut() {
                Some(p) => p.kids.push(el),
                None => root = Some(el),
            }
            i = j + 1;
            continue;
        }
        // start tag
        let mut j = i + 1;
        let ns = j;
        while j < b.len() && !matches!(b[j], b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        let mut el = El { name: s[ns..j].to_string(), ..El::default() };
        loop {
            while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            if j >= b.len() {
                return Err(MixError::Xml("unterminated start tag".into()));
            }
            match b[j] {
                b'>' => {
                    j += 1;
                    stack.push(el);
                    break;
                }
                b'/' => {
                    if b.get(j + 1) != Some(&b'>') {
                        return Err(MixError::Xml("stray /".into()));
                    }
                    j += 2;
                    match stack.last_mut() {
                        Some(p) => p.kids.push(el),
                        None => root = Some(el),
                    }
                    break;
                }
                _ => {
                    let ks = j;
                    while j < b.len() && b[j] != b'=' {
                        j += 1;
                    }
                    let key = s[ks..j].trim().to_string();
                    if b.get(j + 1) != Some(&b'"') {
                        return Err(MixError::Xml(format!("attribute {key} is not quoted")));
                    }
                    let vs = j + 2;
                    let ve = s[vs..].find('"').ok_or_else(|| MixError::Xml("unterminated attribute".into()))? + vs;
                    el.attrs.push((key, unescape(&s[vs..ve])?));
                    j = ve + 1;
                }
            }
        }
        i = j;
    }
    if !stack.is_empty() {
        return Err(MixError::Xml("unclosed element".into()));
    }
    root.ok_or_else(|| MixError::Xml("no root element".into()))
}

// ---------------------------------------------------------------------------------------------
// Decoding the tree
// ---------------------------------------------------------------------------------------------

/// Reads the children of an element in order, as boost wrote them.
struct Kids<'a> {
    owner: &'a str,
    kids: &'a [El],
    i: usize,
}

impl<'a> Kids<'a> {
    fn of(el: &'a El) -> Kids<'a> {
        Kids { owner: &el.name, kids: &el.kids, i: 0 }
    }

    fn next(&mut self, name: &str) -> Res<&'a El> {
        let el = self.kids.get(self.i).ok_or_else(|| MixError::Format(format!("<{}> lacks <{name}>", self.owner)))?;
        if el.name != name {
            return bad(format!("<{}> has <{}> where <{name}> was expected", self.owner, el.name));
        }
        self.i += 1;
        Ok(el)
    }

    fn text(&mut self, name: &str) -> Res<&'a str> {
        let el = self.next(name)?;
        if !el.kids.is_empty() {
            return bad(format!("<{name}> should hold text"));
        }
        Ok(&el.text)
    }

    fn end(self) -> Res<()> {
        match self.kids.get(self.i) {
            None => Ok(()),
            Some(e) => bad(format!("<{}> has an unexpected <{}>", self.owner, e.name)),
        }
    }
}

fn num<T: std::str::FromStr>(s: &str, what: &str) -> Res<T> {
    s.parse().map_err(|_| MixError::Format(format!("{what}: bad number {s:?}")))
}

fn boolean(s: &str, what: &str) -> Res<bool> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => bad(format!("{what}: bad boolean {s:?}")),
    }
}

/// Reads `count`, `item_version` and returns an iterator-like slice of the `item` elements.
fn collection(el: &El) -> Res<Vec<&El>> {
    let mut k = Kids::of(el);
    let count: usize = num(k.text("count")?, "count")?;
    let iv = k.text("item_version")?;
    if iv != "0" {
        return bad(format!("<{}> has item_version {iv}, only 0 is known", el.name));
    }
    let items: Vec<&El> = k.kids[k.i..].iter().collect();
    if items.len() != count || items.iter().any(|e| e.name != "item") {
        return bad(format!("<{}> says {count} items but holds {}", el.name, items.len()));
    }
    Ok(items)
}

struct Dec {
    /// `class_id` to exported class name, as announced by `class_name=` attributes.
    ids: HashMap<String, String>,
}

impl Dec {
    /// Class name of a polymorphic pointer item.
    fn class_of(&mut self, el: &El) -> Res<String> {
        if let Some(name) = el.attr("class_name") {
            let id = el.attr("class_id").ok_or_else(|| MixError::Format("class_name without class_id".into()))?;
            self.ids.insert(id.to_string(), name.to_string());
            Ok(name.to_string())
        } else if let Some(r) = el.attr("class_id_reference") {
            self.ids.get(r).cloned().ok_or_else(|| MixError::Format(format!("class_id_reference {r} is unknown")))
        } else {
            bad(format!("<{}> item has neither class_name nor class_id_reference", el.name))
        }
    }

    /// A by-value member that may later be referenced as a pointer: remember its class id.
    fn note_member(&mut self, el: &El, class: &str) {
        if let Some(id) = el.attr("class_id") {
            self.ids.entry(id.to_string()).or_insert_with(|| class.to_string());
        }
    }

    fn mix(&mut self, root: &El) -> Res<Mix> {
        if root.name != "boost_serialization"
            || root.attr("signature") != Some("serialization::archive")
            || root.attr("version") != Some("4")
        {
            return bad("not a boost archive version 4");
        }
        let mut rk = Kids::of(root);
        let doc = rk.next("MixDocument")?;
        rk.end()?;
        if doc.attr("version") != Some("1") {
            return bad(format!("MixDocument class version {:?}, only 1 is known", doc.attr("version")));
        }
        let mut k = Kids::of(doc);
        let name = k.text("DocumentName")?.to_string();
        let device_info = k.text("DeviceInfo")?.to_string();
        let genre = k.text("Genre")?.to_string();
        let artist = k.text("Artist")?.to_string();
        let tracks_el = k.next("Tracks")?;
        let trans_el = k.next("Transitions")?;
        k.end()?;
        let mut tracks = Vec::new();
        for item in self.mixer_items(tracks_el)? {
            if self.class_of(item)? != "Track" {
                return bad("Tracks holds something that is not a Track");
            }
            tracks.push(self.track(item)?);
        }
        let mut transitions = Vec::new();
        for item in self.mixer_items(trans_el)? {
            if self.class_of(item)? != "Transition" {
                return bad("Transitions holds something that is not a Transition");
            }
            transitions.push(self.transition(item)?);
        }
        Ok(Mix { name, device_info, genre, artist, tracks, transitions })
    }

    fn mixer_items<'a>(&mut self, el: &'a El) -> Res<Vec<&'a El>> {
        let mut k = Kids::of(el);
        let procs = k.next("Processes")?;
        k.end()?;
        collection(procs)
    }

    fn audio_process(el: &El) -> Res<(String, bool)> {
        let mut k = Kids::of(el);
        let n = k.text("Name")?.to_string();
        let b = boolean(k.text("Bypassed")?, "Bypassed")?;
        k.end()?;
        Ok((n, b))
    }

    fn position(el: &El) -> Res<Position> {
        let mut k = Kids::of(el);
        let p = Position {
            start: num(k.text("Start")?, "Start")?,
            length: num(k.text("Length")?, "Length")?,
            l_offset: num(k.text("LOffset")?, "LOffset")?,
            r_offset: num(k.text("ROffset")?, "ROffset")?,
            limit_size: boolean(k.text("LimitSize")?, "LimitSize")?,
            session_event_id: k.text("SessionEventId")?.to_string(),
        };
        k.end()?;
        Ok(p)
    }

    fn effect(&mut self, item: &El) -> Res<Effect> {
        let class = self.class_of(item)?;
        let mut k = Kids::of(item);
        let (name, bypassed) = Self::audio_process(k.next("AudioProcess")?)?;
        let kind = match class.as_str() {
            "FxAudioProcess" => EffectKind::Fx { active: boolean(k.text("Active")?, "Active")? },
            "FilterAudioProcess" => EffectKind::Filter,
            "CrushAudioProcess" => EffectKind::Crush,
            "DelayAudioProcess" => EffectKind::Delay {
                delay_type: num(k.text("Type")?, "Type")?,
                active: boolean(k.text("Active")?, "Active")?,
                sync_against_beat: boolean(k.text("SyncAgainstBeat")?, "SyncAgainstBeat")?,
            },
            "EqAudioProcess" => EffectKind::Eq,
            "PeqAudioProcess" => EffectKind::Peq,
            "VolumeAudioProcess" => EffectKind::Volume,
            "TransAudioProcess" => EffectKind::Trans {
                start_bpm: num(k.text("StartBPM")?, "StartBPM")?,
                active: boolean(k.text("Active")?, "Active")?,
            },
            other => return bad(format!("unknown effect class {other}")),
        };
        k.end()?;
        let version = item.attr("version");
        if item.attr("class_name").is_some() {
            let want = if matches!(kind, EffectKind::Delay { .. }) { "1" } else { "0" };
            if version != Some(want) {
                return bad(format!("{class} class version {version:?}, only {want} is known"));
            }
        }
        Ok(Effect { name, bypassed, kind })
    }

    fn tool(&mut self, el: &El) -> Res<Tool> {
        let mut k = Kids::of(el);
        let sap = k.next("StreamedAudioProcess")?;
        let position = Self::position(k.next("PositionedEvent")?)?;
        let map = k.next("AutomationMap")?;
        k.end()?;
        let mut sk = Kids::of(sap);
        let (name, bypassed) = Self::audio_process(sk.next("AudioProcess")?)?;
        let cont = sk.next("AudioProcessContainer")?;
        sk.end()?;
        let mut ck = Kids::of(cont);
        let procs = ck.next("Processes")?;
        ck.end()?;
        let mut effects = Vec::new();
        for it in collection(procs)? {
            effects.push(self.effect(it)?);
        }
        let mut automation = BTreeMap::new();
        for it in collection(map)? {
            let mut ik = Kids::of(it);
            let key = ik.text("first")?.to_string();
            let second = ik.next("second")?;
            ik.end()?;
            if let Some(v) = second.attr("version") {
                if v != "1" {
                    return bad(format!("AutomationItem version {v}: the old XML node format is not supported"));
                }
            }
            let mut sk = Kids::of(second);
            let env = Envelope::decode(sk.text("Envelope")?)?;
            sk.end()?;
            if automation.insert(key.clone(), env).is_some() {
                return bad(format!("duplicate automation key {key:?}"));
            }
        }
        Ok(Tool { name, bypassed, effects, position, automation })
    }

    fn track(&mut self, item: &El) -> Res<Track> {
        if item.attr("version").is_some_and(|v| v != "2") {
            return bad(format!("Track class version {:?}, only 2 is known", item.attr("version")));
        }
        let mut k = Kids::of(item);
        let sap = k.next("StreamedAudioProcess")?;
        let position = Self::position(k.next("PositionedEvent")?)?;
        let library_id = {
            let lc = k.next("LocalContext")?;
            let mut lk = Kids::of(lc);
            let id = num(lk.text("libraryID")?, "libraryID")?;
            lk.end()?;
            id
        };
        let gc = k.next("GlobalContext")?;
        let file_path = k.text("FilePath")?.to_string();
        let bpm = num(k.text("Bpm")?, "Bpm")?;
        let channel = num(k.text("Channel")?, "Channel")?;
        let offset = num(k.text("Offset")?, "Offset")?;
        let non_interpolated = boolean(k.text("NonInterpolated")?, "NonInterpolated")?;
        let pe = k.next("PitchEnvelope")?;
        let key_text = k.text("KeyEnvelope")?;
        let vol = k.next("VolumeEnvelope")?;
        k.end()?;

        let mut gk = Kids::of(gc);
        let tonium_id = gk.text("toniumId")?.to_string();
        let oi = gk.next("otherIds")?;
        let fz = gk.next("fuzzyId")?;
        gk.end()?;
        let mut other_ids = BTreeMap::new();
        for it in collection(oi)? {
            let mut ik = Kids::of(it);
            let a = ik.text("first")?.to_string();
            let b = ik.text("second")?.to_string();
            ik.end()?;
            other_ids.insert(a, b);
        }
        let mut fk = Kids::of(fz);
        let fuzzy_id = FuzzyId {
            artist: fk.text("artist")?.to_string(),
            album: fk.text("album")?.to_string(),
            title: fk.text("title")?.to_string(),
            genre: fk.text("genre")?.to_string(),
        };
        fk.end()?;

        let mut pitch = Vec::new();
        for it in collection(pe)? {
            let mut ik = Kids::of(it);
            pitch.push(SpeedNode { position: num(ik.text("position")?, "position")?, speed: num(ik.text("speed")?, "speed")? });
            ik.end()?;
        }
        let kb = juce_b64::decode(key_text).map_err(|e| MixError::Format(format!("KeyEnvelope base64: {e:?}")))?;
        let kn = read_count(&kb, 12)?;
        let key = (0..kn)
            .map(|i| {
                let o = 4 + 12 * i;
                KeyNode {
                    position: u32_at(&kb, o),
                    cents: f32::from_le_bytes(kb[o + 4..o + 8].try_into().unwrap()),
                    flags: u32_at(&kb, o + 8),
                }
            })
            .collect();

        // The wrapping StreamedAudioProcess and its process list.
        let mut sk = Kids::of(sap);
        let (clip_name, clip_bypassed) = Self::audio_process(sk.next("AudioProcess")?)?;
        let cont = sk.next("AudioProcessContainer")?;
        sk.end()?;
        let mut ck = Kids::of(cont);
        let procs_el = ck.next("Processes")?;
        ck.end()?;
        let mut processes: Vec<Process> = Vec::new();
        let mut objs: HashMap<String, usize> = HashMap::new();
        for (idx, it) in collection(procs_el)?.into_iter().enumerate() {
            let class = self.class_of(it)?;
            if let Some(o) = it.attr("object_id") {
                objs.insert(o.to_string(), idx);
            }
            processes.push(match class.as_str() {
                "AutomatedAudioProcess" => Process::Tool(self.tool(it)?),
                "LoopAudioProcess" => Process::Loop(self.loop_proc(it, &objs, &processes)?),
                "IgnoreAudioProcess" => {
                    let mut ik = Kids::of(it);
                    let (name, bypassed) = Self::audio_process(ik.next("AudioProcess")?)?;
                    let position = Self::position(ik.next("PositionedEvent")?)?;
                    let start_pos = num(ik.text("StartPos")?, "StartPos")?;
                    ik.end()?;
                    Process::Ignore(IgnoreProc { name, bypassed, position, start_pos })
                }
                "ReversalAudioProcess" => {
                    let mut ik = Kids::of(it);
                    let (name, bypassed) = Self::audio_process(ik.next("AudioProcess")?)?;
                    let position = Self::position(ik.next("PositionedEvent")?)?;
                    ik.end()?;
                    Process::Reverse(ReverseProc { name, bypassed, position })
                }
                other => return bad(format!("unknown track process class {other}")),
            });
        }

        self.note_member(vol, "AutomatedAudioProcess");
        let volume = self.tool(vol)?;
        Ok(Track {
            clip_name,
            clip_bypassed,
            processes,
            position,
            library_id,
            tonium_id,
            other_ids,
            fuzzy_id,
            file_path,
            bpm,
            channel,
            offset,
            non_interpolated,
            pitch,
            key,
            volume,
        })
    }

    fn loop_proc(&mut self, it: &El, objs: &HashMap<String, usize>, so_far: &[Process]) -> Res<LoopProc> {
        if it.attr("version").is_some_and(|v| v != "2") {
            return bad(format!("LoopAudioProcess class version {:?}, only 2 is known", it.attr("version")));
        }
        let mut k = Kids::of(it);
        let (name, bypassed) = Self::audio_process(k.next("AudioProcess")?)?;
        let position = Self::position(k.next("PositionedEvent")?)?;
        let length = num(k.text("Length")?, "Length")?;
        let iterations = num(k.text("Iterations")?, "Iterations")?;
        let re_loop = boolean(k.text("ReLoop")?, "ReLoop")?;
        let mode = boolean(k.text("Mode")?, "Mode")?;
        let pel = k.next("Parent")?;
        let off = k.next("Offsets")?;
        k.end()?;
        let parent = if pel.attr("class_id") == Some("-1") {
            None
        } else {
            let r = pel.attr("object_id_reference").ok_or_else(|| MixError::Format("Parent has no object_id_reference".into()))?;
            let idx = *objs.get(r).ok_or_else(|| MixError::Format(format!("Parent refers to {r}, not an earlier process of this track")))?;
            if !matches!(so_far.get(idx), Some(Process::Loop(_))) {
                return bad("Parent does not refer to a loop");
            }
            Some(idx)
        };
        let mut offsets = Vec::new();
        for o in collection(off)? {
            let mut ok = Kids::of(o);
            let iteration = num(ok.text("first")?, "first")?;
            let second = ok.next("second")?;
            ok.end()?;
            let mut sk = Kids::of(second);
            let a = num(sk.text("first")?, "first")?;
            let offset = num(sk.text("second")?, "second")?;
            sk.end()?;
            offsets.push(LoopOffset { iteration, a, offset });
        }
        Ok(LoopProc { name, bypassed, position, length, iterations, re_loop, mode, parent, offsets })
    }

    fn transition(&mut self, it: &El) -> Res<Transition> {
        if it.attr("version").is_some_and(|v| v != "3") {
            return bad(format!("Transition class version {:?}, only 3 is known", it.attr("version")));
        }
        let mut k = Kids::of(it);
        let (name, bypassed) = Self::audio_process(k.next("AudioProcess")?)?;
        let position = Self::position(k.next("PositionedEvent")?)?;
        let beat_match = boolean(k.text("BeatMatch")?, "BeatMatch")?;
        let env = k.next("Envelope")?;
        let crossfade_type = num(k.text("CrossfadeType")?, "CrossfadeType")?;
        k.end()?;
        let mut envelope = Vec::new();
        for n in collection(env)? {
            let mut nk = Kids::of(n);
            envelope.push(Node { pos: num(nk.text("first")?, "first")?, value: num(nk.text("second")?, "second")? });
            nk.end()?;
        }
        Ok(Transition { name, bypassed, position, beat_match, envelope, crossfade_type })
    }
}

// ---------------------------------------------------------------------------------------------
// Writing: boost's bookkeeping
// ---------------------------------------------------------------------------------------------

/// Classes boost registers while writing a mix. Each has the properties the archive prints on
/// first use: whether it carries class info at all, whether its objects are tracked, and the
/// class version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Cls {
    MixDocument,
    Mixer,
    ProcVec,
    Track,
    Streamed,
    AudioProcess,
    Container,
    Automated,
    Fx,
    Filter,
    Crush,
    Delay,
    Eq,
    Peq,
    Volume,
    Trans,
    Loop,
    Ignore,
    Reversal,
    Transition,
    Positioned,
    AutoMap,
    AutoPair,
    AutoItem,
    LocalCtx,
    GlobalCtx,
    OtherIds,
    OtherPair,
    Fuzzy,
    PitchVec,
    SpeedEntry,
    OffsetsVec,
    OffsetPair,
    OffsetInner,
    EnvVec,
    EnvPair,
}

struct Info {
    /// Exported (polymorphic) name, printed as `class_name` when saved through a pointer.
    export: &'static str,
    class_info: bool,
    tracked: bool,
    version: u32,
}

const fn info(export: &'static str, class_info: bool, tracked: bool, version: u32) -> Info {
    Info { export, class_info, tracked, version }
}

fn info_of(c: Cls) -> Info {
    use Cls::*;
    match c {
        MixDocument => info("", true, true, 1),
        Mixer => info("", true, true, 0),
        ProcVec | AutoMap | AutoPair | OtherIds | OtherPair | PitchVec | OffsetsVec | OffsetPair | OffsetInner
        | EnvVec | EnvPair => info("", true, false, 0),
        Track => info("Track", true, true, 2),
        Streamed => info("", true, true, 0),
        AudioProcess | Positioned | LocalCtx | GlobalCtx | Fuzzy | SpeedEntry => info("", true, false, 0),
        Container => info("", false, true, 0),
        Automated => info("AutomatedAudioProcess", true, true, 0),
        Fx => info("FxAudioProcess", true, true, 0),
        Filter => info("FilterAudioProcess", true, true, 0),
        Crush => info("CrushAudioProcess", true, true, 0),
        Delay => info("DelayAudioProcess", true, true, 1),
        Eq => info("EqAudioProcess", true, true, 0),
        Peq => info("PeqAudioProcess", true, true, 0),
        Volume => info("VolumeAudioProcess", true, true, 0),
        // Trans, Ignore and Reversal never appear in a known document; tracked + version 0 is
        // what every other pointer-saved process class shows.
        Trans => info("TransAudioProcess", true, true, 0),
        Loop => info("LoopAudioProcess", true, true, 2),
        Ignore => info("IgnoreAudioProcess", true, true, 0),
        Reversal => info("ReversalAudioProcess", true, true, 0),
        Transition => info("Transition", true, false, 3),
        AutoItem => info("", true, false, 1),
    }
}

fn effect_cls(k: &EffectKind) -> Cls {
    match k {
        EffectKind::Fx { .. } => Cls::Fx,
        EffectKind::Filter => Cls::Filter,
        EffectKind::Crush => Cls::Crush,
        EffectKind::Delay { .. } => Cls::Delay,
        EffectKind::Eq => Cls::Eq,
        EffectKind::Peq => Cls::Peq,
        EffectKind::Volume => Cls::Volume,
        EffectKind::Trans { .. } => Cls::Trans,
    }
}

struct W {
    out: String,
    depth: usize,
    class_ids: HashMap<Cls, u32>,
    next_class: u32,
    next_obj: u32,
}

impl W {
    fn new() -> W {
        W { out: String::new(), depth: 0, class_ids: HashMap::new(), next_class: 0, next_obj: 0 }
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push('\t');
        }
    }

    /// Attribute text boost prints for an object of class `c`, and its object id if tracked.
    /// `ptr` is true for an item saved through a base-class pointer.
    fn attrs(&mut self, c: Cls, ptr: bool) -> (String, Option<u32>) {
        let i = info_of(c);
        let mut a = String::new();
        if i.class_info {
            match self.class_ids.get(&c) {
                None => {
                    let id = self.next_class;
                    self.next_class += 1;
                    self.class_ids.insert(c, id);
                    a.push_str(&format!(" class_id=\"{id}\""));
                    if ptr {
                        a.push_str(&format!(" class_name=\"{}\"", i.export));
                    }
                    a.push_str(&format!(" tracking_level=\"{}\" version=\"{}\"", u8::from(i.tracked), i.version));
                }
                Some(id) if ptr => a.push_str(&format!(" class_id_reference=\"{id}\"")),
                Some(_) => {}
            }
        }
        let oid = i.tracked.then(|| {
            let o = self.next_obj;
            self.next_obj += 1;
            a.push_str(&format!(" object_id=\"_{o}\""));
            o
        });
        (a, oid)
    }

    /// Opens a container element and returns its object id.
    fn open(&mut self, tag: &str, c: Cls, ptr: bool) -> Option<u32> {
        let (a, oid) = self.attrs(c, ptr);
        self.indent();
        self.out.push_str(&format!("<{tag}{a}>\n"));
        self.depth += 1;
        oid
    }

    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.indent();
        self.out.push_str(&format!("</{tag}>\n"));
    }

    fn leaf(&mut self, tag: &str, text: &str) {
        self.indent();
        self.out.push_str(&format!("<{tag}>"));
        escape(text, &mut self.out);
        self.out.push_str(&format!("</{tag}>\n"));
    }

    fn leaf_attrs(&mut self, tag: &str, attrs: &str) {
        self.indent();
        self.out.push_str(&format!("<{tag}{attrs}></{tag}>\n"));
    }

    fn b(&mut self, tag: &str, v: bool) {
        self.leaf(tag, if v { "1" } else { "0" });
    }

    /// Opens a collection (`count`, `item_version`) member.
    fn open_vec(&mut self, tag: &str, c: Cls, count: usize) {
        self.open(tag, c, false);
        self.leaf("count", &count.to_string());
        self.leaf("item_version", "0");
    }

    fn audio_process(&mut self, name: &str, bypassed: bool) {
        self.open("AudioProcess", Cls::AudioProcess, false);
        self.leaf("Name", name);
        self.b("Bypassed", bypassed);
        self.close("AudioProcess");
    }

    fn position(&mut self, p: &Position) {
        self.open("PositionedEvent", Cls::Positioned, false);
        self.leaf("Start", &f64_text(p.start));
        self.leaf("Length", &f64_text(p.length));
        self.leaf("LOffset", &f64_text(p.l_offset));
        self.leaf("ROffset", &f64_text(p.r_offset));
        self.b("LimitSize", p.limit_size);
        self.leaf("SessionEventId", &p.session_event_id);
        self.close("PositionedEvent");
    }

    fn effect(&mut self, e: &Effect) {
        self.open("item", effect_cls(&e.kind), true);
        self.audio_process(&e.name, e.bypassed);
        match &e.kind {
            EffectKind::Fx { active } => self.b("Active", *active),
            EffectKind::Delay { delay_type, active, sync_against_beat } => {
                self.leaf("Type", &delay_type.to_string());
                self.b("Active", *active);
                self.b("SyncAgainstBeat", *sync_against_beat);
            }
            EffectKind::Trans { start_bpm, active } => {
                self.leaf("StartBPM", &f32_text(*start_bpm));
                self.b("Active", *active);
            }
            EffectKind::Filter | EffectKind::Crush | EffectKind::Eq | EffectKind::Peq | EffectKind::Volume => {}
        }
        self.close("item");
    }

    /// An `AutomatedAudioProcess` under `tag` (`item` in a process list, `VolumeEnvelope` for a
    /// track's volume).
    fn tool(&mut self, tag: &str, t: &Tool, ptr: bool) -> Option<u32> {
        let oid = self.open(tag, Cls::Automated, ptr);
        self.open("StreamedAudioProcess", Cls::Streamed, false);
        self.audio_process(&t.name, t.bypassed);
        self.open("AudioProcessContainer", Cls::Container, false);
        self.open_vec("Processes", Cls::ProcVec, t.effects.len());
        for e in &t.effects {
            self.effect(e);
        }
        self.close("Processes");
        self.close("AudioProcessContainer");
        self.close("StreamedAudioProcess");
        self.position(&t.position);
        self.open_vec("AutomationMap", Cls::AutoMap, t.automation.len());
        for (k, env) in &t.automation {
            self.open("item", Cls::AutoPair, false);
            self.leaf("first", k);
            self.open("second", Cls::AutoItem, false);
            self.leaf("Envelope", &env.encode());
            self.close("second");
            self.close("item");
        }
        self.close("AutomationMap");
        self.close(tag);
        oid
    }

    fn track(&mut self, t: &Track) -> Res<()> {
        self.open("item", Cls::Track, true);
        self.open("StreamedAudioProcess", Cls::Streamed, false);
        self.audio_process(&t.clip_name, t.clip_bypassed);
        self.open("AudioProcessContainer", Cls::Container, false);
        self.open_vec("Processes", Cls::ProcVec, t.processes.len());
        let mut oids: Vec<Option<u32>> = Vec::new();
        for (i, p) in t.processes.iter().enumerate() {
            let oid = match p {
                Process::Tool(tool) => self.tool("item", tool, true),
                Process::Loop(l) => {
                    let oid = self.open("item", Cls::Loop, true);
                    self.audio_process(&l.name, l.bypassed);
                    self.position(&l.position);
                    self.leaf("Length", &f64_text(l.length));
                    self.leaf("Iterations", &l.iterations.to_string());
                    self.b("ReLoop", l.re_loop);
                    self.b("Mode", l.mode);
                    match l.parent {
                        None => self.leaf_attrs("Parent", " class_id=\"-1\""),
                        Some(pi) => {
                            let target = match (pi < i, t.processes.get(pi)) {
                                (true, Some(Process::Loop(_))) => oids[pi],
                                _ => None,
                            };
                            let Some(o) = target else {
                                return bad(format!("loop {i} has parent {pi}, which is not an earlier loop"));
                            };
                            let cid = self.class_ids[&Cls::Loop];
                            self.leaf_attrs("Parent", &format!(" class_id_reference=\"{cid}\" object_id_reference=\"_{o}\""));
                        }
                    }
                    self.open_vec("Offsets", Cls::OffsetsVec, l.offsets.len());
                    for o in &l.offsets {
                        self.open("item", Cls::OffsetPair, false);
                        self.leaf("first", &o.iteration.to_string());
                        self.open("second", Cls::OffsetInner, false);
                        self.leaf("first", &f32_text(o.a));
                        self.leaf("second", &f32_text(o.offset));
                        self.close("second");
                        self.close("item");
                    }
                    self.close("Offsets");
                    self.close("item");
                    oid
                }
                Process::Ignore(g) => {
                    let oid = self.open("item", Cls::Ignore, true);
                    self.audio_process(&g.name, g.bypassed);
                    self.position(&g.position);
                    self.leaf("StartPos", &g.start_pos.to_string());
                    self.close("item");
                    oid
                }
                Process::Reverse(r) => {
                    let oid = self.open("item", Cls::Reversal, true);
                    self.audio_process(&r.name, r.bypassed);
                    self.position(&r.position);
                    self.close("item");
                    oid
                }
            };
            oids.push(oid);
        }
        self.close("Processes");
        self.close("AudioProcessContainer");
        self.close("StreamedAudioProcess");
        self.position(&t.position);
        self.open("LocalContext", Cls::LocalCtx, false);
        self.leaf("libraryID", &t.library_id.to_string());
        self.close("LocalContext");
        self.open("GlobalContext", Cls::GlobalCtx, false);
        self.leaf("toniumId", &t.tonium_id);
        self.open_vec("otherIds", Cls::OtherIds, t.other_ids.len());
        for (k, v) in &t.other_ids {
            self.open("item", Cls::OtherPair, false);
            self.leaf("first", k);
            self.leaf("second", v);
            self.close("item");
        }
        self.close("otherIds");
        self.open("fuzzyId", Cls::Fuzzy, false);
        self.leaf("artist", &t.fuzzy_id.artist);
        self.leaf("album", &t.fuzzy_id.album);
        self.leaf("title", &t.fuzzy_id.title);
        self.leaf("genre", &t.fuzzy_id.genre);
        self.close("fuzzyId");
        self.close("GlobalContext");
        self.leaf("FilePath", &t.file_path);
        self.leaf("Bpm", &f64_text(t.bpm));
        self.leaf("Channel", &t.channel.to_string());
        self.leaf("Offset", &t.offset.to_string());
        self.b("NonInterpolated", t.non_interpolated);
        self.open_vec("PitchEnvelope", Cls::PitchVec, t.pitch.len());
        for n in &t.pitch {
            self.open("item", Cls::SpeedEntry, false);
            self.leaf("position", &n.position.to_string());
            self.leaf("speed", &f32_text(n.speed));
            self.close("item");
        }
        self.close("PitchEnvelope");
        let mut kb = Vec::with_capacity(4 + 12 * t.key.len());
        kb.extend_from_slice(&(t.key.len() as u32).to_le_bytes());
        for n in &t.key {
            kb.extend_from_slice(&n.position.to_le_bytes());
            kb.extend_from_slice(&n.cents.to_le_bytes());
            kb.extend_from_slice(&n.flags.to_le_bytes());
        }
        self.leaf("KeyEnvelope", &juce_b64::encode(&kb));
        self.tool("VolumeEnvelope", &t.volume, false);
        self.close("item");
        Ok(())
    }

    fn transition(&mut self, t: &Transition) {
        self.open("item", Cls::Transition, true);
        self.audio_process(&t.name, t.bypassed);
        self.position(&t.position);
        self.b("BeatMatch", t.beat_match);
        self.open_vec("Envelope", Cls::EnvVec, t.envelope.len());
        for n in &t.envelope {
            self.open("item", Cls::EnvPair, false);
            self.leaf("first", &n.pos.to_string());
            self.leaf("second", &f32_text(n.value));
            self.close("item");
        }
        self.close("Envelope");
        self.leaf("CrossfadeType", &t.crossfade_type.to_string());
        self.close("item");
    }
}

impl Mix {
    /// Parses the XML of a mix document (the output of [`crate::mixdoc::decode_blob`] or
    /// [`crate::mixdoc::decode_pmk`]).
    pub fn parse(xml: &[u8]) -> Result<Mix, MixError> {
        let s = std::str::from_utf8(xml).map_err(|_| MixError::Xml("not UTF-8".into()))?;
        let body = s.strip_prefix(PROLOGUE).ok_or_else(|| MixError::Xml("unexpected XML declaration / DOCTYPE".into()))?;
        let root = parse_tree(body)?;
        Dec { ids: HashMap::new() }.mix(&root)
    }

    /// Like [`Mix::parse`], but also fails unless [`Mix::to_xml`] reproduces `xml` byte for
    /// byte. Use it when the file must survive a rewrite unchanged.
    pub fn parse_exact(xml: &[u8]) -> Result<Mix, MixError> {
        let m = Mix::parse(xml)?;
        let again = m.to_xml()?;
        if again != xml {
            let at = again.iter().zip(xml).position(|(a, b)| a != b).unwrap_or(again.len().min(xml.len()));
            return bad(format!("document does not re-serialise identically (first difference at byte {at})"));
        }
        Ok(m)
    }

    /// Serialises the document the way the Editor's boost archive does.
    pub fn to_xml(&self) -> Result<Vec<u8>, MixError> {
        let mut w = W::new();
        w.out.push_str(PROLOGUE);
        w.out.push_str(ROOT_OPEN);
        w.open("MixDocument", Cls::MixDocument, false);
        w.leaf("DocumentName", &self.name);
        w.leaf("DeviceInfo", &self.device_info);
        w.leaf("Genre", &self.genre);
        w.leaf("Artist", &self.artist);
        w.open("Tracks", Cls::Mixer, false);
        w.open_vec("Processes", Cls::ProcVec, self.tracks.len());
        for t in &self.tracks {
            w.track(t)?;
        }
        w.close("Processes");
        w.close("Tracks");
        w.open("Transitions", Cls::Mixer, false);
        w.open_vec("Processes", Cls::ProcVec, self.transitions.len());
        for t in &self.transitions {
            w.transition(t);
        }
        w.close("Processes");
        w.close("Transitions");
        w.close("MixDocument");
        w.out.push_str(ROOT_CLOSE);
        Ok(w.out.into_bytes())
    }

    /// Decodes a `mixes.structured_ct` blob.
    pub fn from_blob(blob: &[u8]) -> Result<Mix, MixError> {
        let xml = crate::mixdoc::decode_blob(blob).map_err(|e| MixError::Xml(format!("container: {e}")))?;
        Mix::parse(&xml)
    }

    /// Encodes the document as a `mixes.structured_ct` blob.
    pub fn to_blob(&self) -> Result<Vec<u8>, MixError> {
        Ok(crate::mixdoc::encode_blob(&self.to_xml()?))
    }

    /// Decodes a `.pmk` prefab file.
    pub fn from_pmk(data: &[u8]) -> Result<Mix, MixError> {
        let xml = crate::mixdoc::decode_pmk(data).map_err(|e| MixError::Xml(format!("container: {e}")))?;
        Mix::parse(&xml)
    }

    pub fn to_pmk(&self) -> Result<Vec<u8>, MixError> {
        Ok(crate::mixdoc::encode_pmk(&self.to_xml()?))
    }
}

// ---------------------------------------------------------------------------------------------
// Tempo curve
// ---------------------------------------------------------------------------------------------

/// A track's tempo curve (`PitchEnvelope`) as a mapping between two time axes, both in samples
/// at [`SAMPLE_RATE`]:
///
/// - **local**: the stretched clip as it plays; the axis of `Position.start`/`length`/offsets,
///   of tool and loop positions and of the nodes of the tempo and key envelopes. Local 0 is the
///   start of the audio file, local `l_offset` is the cue (left trim).
/// - **source**: the unstretched audio file, expressed at the engine rate (a file frame `f` of
///   a file recorded at `sr` Hz is source `f * 44100 / sr`).
///
/// `speed` is the playback ratio (target tempo / analysed tempo), so `d source / d local =
/// speed`. Nodes are joined **linearly** and the curve is flat before the first and after the
/// last node (hypothesis: the Editor draws these as ordinary envelopes; constant curves, the
/// only kind in the known documents, are unaffected). Check on the Editor's demo mix: the
/// `mixtracks.cue_point` it stores for a track equals `source_at(l_offset)` (0, 10 166 523 =
/// 9 925 797.47 x 1.0242525, 9 589 191).
#[derive(Debug, Clone, PartialEq)]
pub struct TempoMap {
    pos: Vec<f64>,
    speed: Vec<f64>,
    /// `source_at(pos[i])`.
    cum: Vec<f64>,
}

impl TempoMap {
    /// Builds the map. Nodes are sorted by position; non-finite or non-positive speeds are
    /// replaced by 1; an empty curve is the constant speed 1.
    pub fn new(nodes: &[SpeedNode]) -> TempoMap {
        let mut v: Vec<(f64, f64)> = nodes
            .iter()
            .map(|n| (f64::from(n.position), if n.speed.is_finite() && n.speed > 0.0 { f64::from(n.speed) } else { 1.0 }))
            .collect();
        if v.is_empty() {
            v.push((0.0, 1.0));
        }
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let pos: Vec<f64> = v.iter().map(|n| n.0).collect();
        let speed: Vec<f64> = v.iter().map(|n| n.1).collect();
        let mut cum = Vec::with_capacity(pos.len());
        cum.push(speed[0] * pos[0]);
        for i in 1..pos.len() {
            cum.push(cum[i - 1] + 0.5 * (speed[i - 1] + speed[i]) * (pos[i] - pos[i - 1]));
        }
        TempoMap { pos, speed, cum }
    }

    /// Playback ratio at a local position.
    pub fn speed_at(&self, local: f64) -> f64 {
        let n = self.pos.len();
        if local <= self.pos[0] {
            return self.speed[0];
        }
        let i = self.pos.partition_point(|&p| p <= local) - 1;
        if i == n - 1 {
            return self.speed[i];
        }
        let t = (local - self.pos[i]) / (self.pos[i + 1] - self.pos[i]);
        self.speed[i] + (self.speed[i + 1] - self.speed[i]) * t
    }

    /// Source position (engine-rate samples of the unstretched audio) played at a local position.
    pub fn source_at(&self, local: f64) -> f64 {
        let n = self.pos.len();
        if local <= self.pos[0] {
            return self.speed[0] * local;
        }
        let i = self.pos.partition_point(|&p| p <= local) - 1;
        let dx = local - self.pos[i];
        if i == n - 1 {
            return self.cum[i] + self.speed[i] * dx;
        }
        let l = self.pos[i + 1] - self.pos[i];
        self.cum[i] + self.speed[i] * dx + (self.speed[i + 1] - self.speed[i]) / (2.0 * l) * dx * dx
    }

    /// Local position at which a source position is played (inverse of [`TempoMap::source_at`]).
    pub fn local_at(&self, source: f64) -> f64 {
        let n = self.pos.len();
        if source <= self.cum[0] {
            return source / self.speed[0];
        }
        let i = self.cum.partition_point(|&c| c <= source) - 1;
        let d = source - self.cum[i];
        if i == n - 1 {
            return self.pos[i] + d / self.speed[i];
        }
        let l = self.pos[i + 1] - self.pos[i];
        let (s0, s1) = (self.speed[i], self.speed[i + 1]);
        let a = (s1 - s0) / (2.0 * l);
        let disc = (s0 * s0 + 4.0 * a * d).max(0.0);
        self.pos[i] + 2.0 * d / (s0 + disc.sqrt())
    }
}

#[cfg(test)]
mod tempo_tests {
    use super::*;

    fn sn(position: u32, speed: f32) -> SpeedNode {
        SpeedNode { position, speed }
    }

    #[test]
    fn constant_curve_is_a_plain_ratio() {
        let m = TempoMap::new(&[sn(0, 1.25)]);
        assert_eq!(m.source_at(1000.0), 1250.0);
        assert_eq!(m.local_at(1250.0), 1000.0);
        assert_eq!(m.speed_at(-5.0), 1.25);
        // the Editor's demo mix: cue point = source position of the left trim
        let m = TempoMap::new(&[sn(0, 1.0242525), sn(9925796, 1.0242525), sn(9925797, 1.0242525)]);
        assert_eq!(m.source_at(9_925_797.474_441_8).round() as i64, 10_166_523);
        assert_eq!(TempoMap::new(&[]).source_at(77.0), 77.0);
    }

    #[test]
    fn ramp_integrates_and_inverts() {
        // 1.0 -> 2.0 over 1000 samples: source = x + x^2 / 2000
        let m = TempoMap::new(&[sn(0, 1.0), sn(1000, 2.0)]);
        assert!((m.source_at(500.0) - (500.0 + 125.0)).abs() < 1e-9);
        assert!((m.source_at(1000.0) - 1500.0).abs() < 1e-9);
        assert!((m.source_at(1200.0) - (1500.0 + 400.0)).abs() < 1e-9, "flat after the last node");
        assert!((m.speed_at(250.0) - 1.25).abs() < 1e-9);
        for x in [-30.0, 0.0, 1.0, 333.3, 999.9, 1000.0, 5000.5] {
            let back = m.local_at(m.source_at(x));
            assert!((back - x).abs() < 1e-6, "{x} -> {back}");
        }
    }

    #[test]
    fn nodes_with_equal_positions_and_unsorted_input() {
        let m = TempoMap::new(&[sn(2000, 2.0), sn(1000, 1.0), sn(1000, 0.5), sn(0, 1.0)]);
        for x in [0.0, 999.0, 1000.0, 1500.0, 2000.0, 3000.0] {
            let back = m.local_at(m.source_at(x));
            assert!((back - x).abs() < 1e-6, "{x} -> {back}");
        }
        let mut prev = f64::MIN;
        for k in 0..400 {
            let v = m.source_at(f64::from(k) * 10.0);
            assert!(v >= prev, "source must not run backwards");
            prev = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format_matches_c() {
        assert_eq!(fmt_g(1136.1108685731888, 17), "1136.1108685731888");
        assert_eq!(fmt_g(13232037.851094251, 17), "13232037.851094251");
        assert_eq!(fmt_g(0.0, 17), "0");
        assert_eq!(fmt_g(2666659.0, 17), "2666659");
        assert_eq!(fmt_g(f64::from(1.0080842_f32), 8), "1.0080842");
        assert_eq!(fmt_g(f64::from(0.340_740_7_f32), 8), "0.34074071");
        assert_eq!(fmt_g(f64::from(-42100.23_f32), 8), "-42100.23");
        assert_eq!(fmt_g(1e17, 17), "1e+17");
        assert_eq!(fmt_g(0.0001, 17), "0.0001");
        assert_eq!(fmt_g(0.00001, 17), "1.0000000000000001e-05");
        assert_eq!(fmt_g(f64::from(f32::from_bits(0x34c19cf1)), 8), "3.6063241e-07");
    }

    #[test]
    fn envelope_codec() {
        // "20.B.........fcIzVPhBKJ.rWBsGD" from the Editor's demo mix: two nodes.
        let e = Envelope::decode("20.B.........fcIzVPhBKJ.rWBsGD").unwrap();
        assert_eq!(e.nodes.len(), 2);
        assert_eq!(e.nodes[0].pos, 0);
        assert_eq!(e.nodes[1].pos, 2666658);
        assert_eq!(e.encode(), "20.B.........fcIzVPhBKJ.rWBsGD");
        assert!(Envelope::decode("8.A...").is_err());
    }

    #[test]
    fn unescape_round_trip() {
        let mut s = String::new();
        escape("a&b<c>\"d'", &mut s);
        assert_eq!(s, "a&amp;b&lt;c&gt;&quot;d&apos;");
        assert_eq!(unescape(&s).unwrap(), "a&b<c>\"d'");
    }

    fn pos(start: f64, length: f64, id: &str) -> Position {
        Position { start, length, session_event_id: id.to_string(), ..Position::default() }
    }

    /// A synthetic document using every process kind, loop chains, a track with no tools
    /// before one with tools, and strings that need escaping.
    fn synthetic() -> Mix {
        let mut m = Mix::new("Synthetic <mix> & \"co\" 'x' \u{e9}");
        m.genre = "House".into();
        m.artist = "Nobody".into();
        // a track without tools first: its volume envelope is the first AutomatedAudioProcess
        let mut a = Track::new(1, "a.mp3", 120.5, 1.0, Position { limit_size: true, ..pos(0.0, 1_000_000.25, "a1") });
        a.fuzzy_id = FuzzyId { artist: "A & B".into(), album: "".into(), title: "T".into(), genre: "g".into() };
        let mut b = Track::new(2, "b.mp3", 128.0, 1.0078125, Position { limit_size: true, ..pos(500_000.5, 2_000_000.0, "b1") });
        b.channel = 1;
        b.pitch.push(SpeedNode { position: 1000, speed: 0.99 });
        b.key = vec![KeyNode { position: 0, cents: -300.0, flags: 1 }, KeyNode { position: 5000, cents: 100.375, flags: 1 }];
        b.non_interpolated = true;
        let tool = |e: Effect, s: f64, l: f64| Tool::flat(e, s, l, 1.5, format!("{:032x}", (s as u64) + 7));
        b.processes.push(Process::Tool(tool(Effect::reverb(), 600_000.0, 90_000.0)));
        b.processes.push(Process::Tool(tool(Effect::filter(), 700_000.0, 90_000.0)));
        b.processes.push(Process::Tool(tool(Effect::eq(), 800_000.0, 90_000.0)));
        b.processes.push(Process::Tool(tool(Effect::echo(), 900_000.0, 90_000.0)));
        b.processes.push(Process::Tool(tool(Effect::roll(), 1_000_000.0, 90_000.0)));
        b.processes.push(Process::Tool(tool(Effect::wah(), 1_100_000.0, 90_000.0)));
        b.processes.push(Process::Tool(tool(Effect::crush(), 1_200_000.0, 90_000.0)));
        let mut trans = tool(Effect::reverb(), 1_300_000.0, 90_000.0);
        trans.effects = vec![Effect { name: "Trans".into(), bypassed: true, kind: EffectKind::Trans { start_bpm: 127.5, active: false } }];
        trans.automation.clear();
        b.processes.push(Process::Tool(trans));
        let lp = |parent: Option<usize>, st: f64| LoopProc {
            name: "LOOP".into(),
            bypassed: false,
            position: pos(st, 40_000.5, &format!("{:032x}", st as u64)),
            length: 40_000.5,
            iterations: 3,
            re_loop: parent.is_some(),
            mode: false,
            parent,
            offsets: vec![LoopOffset { iteration: 1, a: 0.0, offset: -42100.23 }],
        };
        b.processes.push(Process::Loop(lp(None, 1_400_000.0)));
        b.processes.push(Process::Loop(lp(Some(8), 1_500_000.0)));
        b.processes.push(Process::Loop(lp(Some(9), 1_600_000.0)));
        b.processes.push(Process::Ignore(IgnoreProc { name: "IGNORE".into(), bypassed: false, position: pos(1_700_000.0, 100.0, "ig"), start_pos: 42 }));
        b.processes.push(Process::Reverse(ReverseProc { name: "REVERSE".into(), bypassed: false, position: pos(1_800_000.0, 100.0, "rv") }));
        b.volume.automation.insert("Volume-Volume".into(), Envelope { nodes: vec![Node { pos: 0, value: 0.0 }, Node { pos: 9999, value: -12.5 }] });
        m.tracks = vec![a, b];
        let mut t = Transition::new(pos(500_000.5, 500_000.0, "t1"));
        t.beat_match = true;
        t.crossfade_type = 3;
        t.envelope.insert(1, Node { pos: 123_456, value: 0.25 });
        m.transitions = vec![t, Transition::new(pos(900_000.0, 1.0, "t2"))];
        m
    }

    #[test]
    fn synthetic_document_round_trips() {
        let m = synthetic();
        let xml = m.to_xml().unwrap();
        let back = Mix::parse_exact(&xml).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.to_xml().unwrap(), xml);
        let text = String::from_utf8(xml).unwrap();
        // first appearance of a class by value registers it without a class_name; later
        // pointers to it carry a class_id_reference
        assert!(text.contains("<VolumeEnvelope class_id=\""), "{text}");
        assert!(text.contains("Synthetic &lt;mix&gt; &amp; &quot;co&quot; &apos;x&apos; \u{e9}"));
        // the container round trips too
        assert_eq!(Mix::from_blob(&m.to_blob().unwrap()).unwrap(), m);
        assert_eq!(Mix::from_pmk(&m.to_pmk().unwrap()).unwrap(), m);
    }

    #[test]
    fn bookkeeping_ids_are_sequential() {
        let text = String::from_utf8(synthetic().to_xml().unwrap()).unwrap();
        let ids = |attr: &str| -> Vec<u32> {
            text.split(&format!(" {attr}=\"")).skip(1).filter_map(|r| r.split('"').next().unwrap().trim_start_matches('_').parse::<u32>().ok()).collect()
        };
        let objs = ids("object_id");
        assert_eq!(objs, (0..objs.len() as u32).collect::<Vec<_>>());
        let classes = ids("class_id");
        let mut next = 0;
        for c in classes {
            assert!(c <= next, "class ids are handed out in order");
            next = next.max(c + 1);
        }
    }

    #[test]
    fn empty_and_edited_documents() {
        let m = Mix::new("");
        let xml = m.to_xml().unwrap();
        assert_eq!(Mix::parse_exact(&xml).unwrap(), m);
        let mut m = synthetic();
        m.tracks[1].processes.retain(|p| !matches!(p, Process::Loop(_)));
        m.tracks.swap(0, 1);
        m.transitions.clear();
        assert_eq!(Mix::parse_exact(&m.to_xml().unwrap()).unwrap(), m);
        assert_eq!(m.library_ids(), vec![2, 1]);
        assert_eq!(m.play_time_secs(), 56); // floor(2_500_000.5 / 44_100): the longest end wins
    }

    #[test]
    fn rejects_what_it_cannot_represent() {
        let good = String::from_utf8(synthetic().to_xml().unwrap()).unwrap();
        let unknown = good.replacen("<Genre>", "<Mystery>1</Mystery>\n\t<Genre>", 1);
        assert!(Mix::parse(unknown.as_bytes()).is_err());
        let old = good.replacen("version=\"1\" object_id=\"_0\"", "version=\"0\" object_id=\"_0\"", 1);
        assert!(Mix::parse(old.as_bytes()).is_err());
        assert!(Mix::parse(b"not xml").is_err());
        assert!(Mix::parse(good.replace("</MixDocument>", "").as_bytes()).is_err());
        // a loop whose parent is not an earlier loop cannot be written
        let mut m = synthetic();
        if let Process::Loop(l) = &mut m.tracks[1].processes[8] {
            l.parent = Some(3);
        }
        assert!(m.to_xml().is_err());
    }
}
