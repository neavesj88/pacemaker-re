//! Converters to and from other DJ software formats: rekordbox XML, iTunes / Music
//! `Library.xml`, M3U8 and CSV (owner: import/export area).
//!
//! Import is two steps so the user sees exactly what will change first: [`parse_library`] reads a
//! foreign library file, [`plan_import`] matches it against a device library and lists every
//! rating change, every case to create or extend and every file that found no match (nothing is
//! written), and [`apply_plan`] then performs that plan through the [`Library`] operations so
//! the rows keep the conventions of rows the original Editor wrote.
//!
//! The XML reading is a small tolerant pull parser in this file (elements, attributes, text,
//! entities, CDATA; DTDs, comments and processing instructions are skipped) so the crate needs
//! no XML dependency.
//!
//! Sources: rekordbox XML, iTunes / Music `Library.xml`, Traktor `collection.nml` ([`traktor`]),
//! Serato crates ([`serato`], binary) and M3U / M3U8 playlists ([`m3u`]). [`read_path`] opens a
//! file or folder and picks the reader. Every path found in such a file is untrusted and is only
//! ever probed through [`paths`] (no UNC / device / URL / relative paths, no network shares).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::library::{Error, Library, Track};
use crate::textfmt::csv_text;
use crate::{naming, strfile};

pub mod m3u;
pub mod paths;
pub mod serato;
pub mod traktor;

// ---------------------------------------------------------------------------------------------
// XML pull parser
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
enum Ev {
    Start { name: String, attrs: Vec<(String, String)> },
    End,
    Text(String),
}

/// An element's name and attributes.
type StartTag = (String, Vec<(String, String)>);

struct Reader<'a> {
    src: &'a str,
    i: usize,
    pending_end: bool,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

/// Decodes the five predefined entities and numeric character references; an unknown entity is
/// kept as written.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        // only the next 13 bytes can hold the end of an entity: searching the whole rest made a
        // value of many '&' without a ';' quadratic
        match rest.as_bytes().iter().take(13).position(|b| *b == b';') {
            Some(e) => {
                let ent = &rest[1..e];
                let ch = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => ent
                        .strip_prefix("#x")
                        .or_else(|| ent.strip_prefix("#X"))
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                        .and_then(char::from_u32),
                };
                match ch {
                    Some(c) => {
                        out.push(c);
                        rest = &rest[e + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &rest[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

impl<'a> Reader<'a> {
    fn new(src: &'a str) -> Self {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        Reader { src, i: 0, pending_end: false }
    }

    fn skip_to(&mut self, pat: &str) -> Result<(), Error> {
        match self.src[self.i..].find(pat) {
            Some(p) => {
                self.i += p + pat.len();
                Ok(())
            }
            None => Err(bad(format!("XML: missing {pat}"))),
        }
    }

    fn next(&mut self) -> Result<Option<Ev>, Error> {
        if self.pending_end {
            self.pending_end = false;
            return Ok(Some(Ev::End));
        }
        loop {
            let rest = &self.src[self.i..];
            if rest.is_empty() {
                return Ok(None);
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                let text = decode_entities(&rest[..end]);
                self.i += end;
                return Ok(Some(Ev::Text(text)));
            }
            if rest.starts_with("<!--") {
                self.skip_to("-->")?;
            } else if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let end = body.find("]]>").ok_or_else(|| bad("XML: unterminated CDATA"))?;
                let text = body[..end].to_string();
                self.i += 9 + end + 3;
                return Ok(Some(Ev::Text(text)));
            } else if rest.starts_with("<?") {
                self.skip_to("?>")?;
            } else if rest.starts_with("<!") {
                // DOCTYPE, possibly with an internal subset in [...]
                let mut depth = 0i32;
                let mut end = None;
                for (k, c) in rest.char_indices() {
                    match c {
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        '>' if depth <= 0 => {
                            end = Some(k);
                            break;
                        }
                        _ => {}
                    }
                }
                self.i += end.ok_or_else(|| bad("XML: unterminated declaration"))? + 1;
            } else if rest.starts_with("</") {
                self.skip_to(">")?;
                return Ok(Some(Ev::End));
            } else {
                return self.start_tag().map(Some);
            }
        }
    }

    fn start_tag(&mut self) -> Result<Ev, Error> {
        let b = self.src.as_bytes();
        let mut j = self.i + 1;
        let name_start = j;
        while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'/' && b[j] != b'>' {
            j += 1;
        }
        let name = self.src[name_start..j].to_string();
        if name.is_empty() {
            return Err(bad("XML: element without a name"));
        }
        let mut attrs = Vec::new();
        loop {
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            match b.get(j) {
                None => return Err(bad("XML: unterminated tag")),
                Some(b'>') => {
                    j += 1;
                    break;
                }
                Some(b'/') => {
                    if b.get(j + 1) != Some(&b'>') {
                        return Err(bad("XML: stray '/' in a tag"));
                    }
                    j += 2;
                    self.pending_end = true;
                    break;
                }
                Some(_) => {
                    let an = j;
                    while j < b.len() && b[j] != b'=' && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                        j += 1;
                    }
                    let aname = self.src[an..j].to_string();
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if b.get(j) != Some(&b'=') {
                        return Err(bad(format!("XML: attribute {aname} has no value")));
                    }
                    j += 1;
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let q = *b.get(j).ok_or_else(|| bad("XML: unterminated tag"))?;
                    if q != b'"' && q != b'\'' {
                        return Err(bad(format!("XML: attribute {aname} is not quoted")));
                    }
                    j += 1;
                    let vs = j;
                    while j < b.len() && b[j] != q {
                        j += 1;
                    }
                    if j >= b.len() {
                        return Err(bad("XML: unterminated attribute value"));
                    }
                    attrs.push((aname, decode_entities(&self.src[vs..j])));
                    j += 1;
                }
            }
        }
        self.i = j;
        Ok(Ev::Start { name, attrs })
    }

    /// Consumes events up to and including the end of the element whose start was just read.
    fn skip_element(&mut self) -> Result<(), Error> {
        let mut depth = 1;
        while depth > 0 {
            match self.next()? {
                None => return Err(bad("XML: unexpected end of file")),
                Some(Ev::Start { .. }) => depth += 1,
                Some(Ev::End) => depth -= 1,
                Some(Ev::Text(_)) => {}
            }
        }
        Ok(())
    }

    /// Text of the element whose start was just read, consumed through its end.
    fn text_of_element(&mut self) -> Result<String, Error> {
        let mut s = String::new();
        let mut depth = 1;
        while depth > 0 {
            match self.next()? {
                None => return Err(bad("XML: unexpected end of file")),
                Some(Ev::Start { .. }) => depth += 1,
                Some(Ev::End) => depth -= 1,
                Some(Ev::Text(t)) => s.push_str(&t),
            }
        }
        Ok(s)
    }

    /// The next element start, skipping whitespace text; `None` at the end of the parent.
    fn next_start(&mut self) -> Result<Option<StartTag>, Error> {
        loop {
            match self.next()? {
                None => return Err(bad("XML: unexpected end of file")),
                Some(Ev::Start { name, attrs }) => return Ok(Some((name, attrs))),
                Some(Ev::End) => return Ok(None),
                Some(Ev::Text(_)) => {}
            }
        }
    }
}

const MAX_DEPTH: usize = 64;

/// An element tree for documents whose payload is in attributes (rekordbox).
#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
}

fn build_node(r: &mut Reader, name: String, attrs: Vec<(String, String)>, depth: usize) -> Result<Node, Error> {
    if depth > MAX_DEPTH {
        return Err(bad("XML: elements nested too deeply"));
    }
    let mut node = Node { name, attrs, children: Vec::new() };
    while let Some((n, a)) = r.next_start()? {
        node.children.push(build_node(r, n, a, depth + 1)?);
    }
    Ok(node)
}

// ---------------------------------------------------------------------------------------------
// Foreign library model
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Rekordbox,
    Itunes,
    Traktor,
    Serato,
    M3u,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Rekordbox => "rekordbox",
            Source::Itunes => "iTunes / Music",
            Source::Traktor => "Traktor",
            Source::Serato => "Serato",
            Source::M3u => "M3U playlist",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ForeignTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub key: String,
    pub bpm: Option<f64>,
    /// 1..=5; `None` for unrated.
    pub stars: Option<u8>,
    /// File path as the other program recorded it (decoded from its `file://` URL), "" if none.
    pub path: String,
    pub size: Option<u64>,
    pub duration_secs: Option<f64>,
    /// The first memory cue (rekordbox), in seconds.
    pub cue_secs: Option<f64>,
    /// The colour the other program gave the track, as its own index (Traktor `INFO COLOR`);
    /// `None` when it has none or the source has no track colours. What the index means is the
    /// importer's mapping to colour labels, not something this reader decides.
    pub colour: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ForeignPlaylist {
    /// Folder names and the playlist name, joined with " / ".
    pub name: String,
    /// Indexes into [`ForeignLibrary::tracks`], in playlist order.
    pub tracks: Vec<usize>,
    /// Entries that named a track the file does not contain.
    pub dangling: usize,
}

#[derive(Debug, Clone)]
pub struct ForeignLibrary {
    pub source: Source,
    pub tracks: Vec<ForeignTrack>,
    pub playlists: Vec<ForeignPlaylist>,
    /// Things the reader skipped or could not use (a damaged crate, smart crates), shown in the preview.
    pub notes: Vec<String>,
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A `file://` URL (or a plain path) as a file system path. `file://localhost/C:/Music/a%20b.mp3`
/// becomes `C:/Music/a b.mp3`; a POSIX path keeps its leading slash.
pub fn location_to_path(loc: &str) -> String {
    let loc = loc.trim();
    let rest = loc
        .strip_prefix("file://localhost")
        .or_else(|| loc.strip_prefix("file://"))
        .or_else(|| loc.strip_prefix("FILE://"));
    let Some(rest) = rest else { return loc.to_string() };
    // `file://host/share/x` has a host: keep it as `//host/share/x` (a network path, which the
    // importers refuse to probe) instead of passing it off as the relative path `host/share/x`
    let p = if rest.starts_with('/') || rest.is_empty() { pct_decode(rest) } else { format!("//{}", pct_decode(rest)) };
    let b = p.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        p[1..].to_string()
    } else {
        p
    }
}

fn file_name_of(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn num<T: std::str::FromStr>(s: Option<&str>) -> Option<T> {
    s.and_then(|v| v.trim().parse().ok())
}

/// Parses a rekordbox XML export (`DJ_PLAYLISTS`).
pub fn parse_rekordbox(text: &str) -> Result<ForeignLibrary, Error> {
    let mut r = Reader::new(text);
    let (name, attrs) = r.next_start()?.ok_or_else(|| bad("the file has no root element"))?;
    if name != "DJ_PLAYLISTS" {
        return Err(bad("not a rekordbox XML file (no DJ_PLAYLISTS element)"));
    }
    let root = build_node(&mut r, name, attrs, 0)?;

    let mut tracks = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    if let Some(col) = root.child("COLLECTION") {
        for t in col.children.iter().filter(|c| c.name == "TRACK") {
            let rating: Option<f64> = num(t.attr("Rating"));
            let stars = rating.map(|v| (v / 51.0).round().clamp(0.0, 5.0) as u8).filter(|s| *s > 0);
            let cue = t
                .children
                .iter()
                .filter(|m| m.name == "POSITION_MARK" && m.attr("Type") == Some("0") && num::<i64>(m.attr("Num")) == Some(-1))
                .filter_map(|m| num::<f64>(m.attr("Start")))
                .min_by(|a, b| a.total_cmp(b));
            let path = location_to_path(t.attr("Location").unwrap_or(""));
            let ft = ForeignTrack {
                title: t.attr("Name").unwrap_or("").to_string(),
                artist: t.attr("Artist").unwrap_or("").to_string(),
                album: t.attr("Album").unwrap_or("").to_string(),
                genre: t.attr("Genre").unwrap_or("").to_string(),
                key: t.attr("Tonality").unwrap_or("").to_string(),
                bpm: num::<f64>(t.attr("AverageBpm")).filter(|b| *b > 0.0),
                stars,
                size: num(t.attr("Size")),
                duration_secs: num::<f64>(t.attr("TotalTime")).filter(|d| *d > 0.0),
                cue_secs: cue,
                colour: None,
                path,
            };
            let idx = tracks.len();
            if let Some(id) = t.attr("TrackID") {
                by_id.insert(id.to_string(), idx);
            }
            if !ft.path.is_empty() {
                by_path.insert(ft.path.clone(), idx);
            }
            tracks.push(ft);
        }
    }

    let mut playlists = Vec::new();
    if let Some(pl) = root.child("PLAYLISTS") {
        for top in pl.children.iter().filter(|c| c.name == "NODE") {
            walk_rekordbox_nodes(top, &mut Vec::new(), true, &by_id, &by_path, &mut playlists);
        }
    }
    Ok(ForeignLibrary { source: Source::Rekordbox, tracks, playlists, notes: vec![] })
}

fn walk_rekordbox_nodes(
    node: &Node,
    path: &mut Vec<String>,
    is_root: bool,
    by_id: &HashMap<String, usize>,
    by_path: &HashMap<String, usize>,
    out: &mut Vec<ForeignPlaylist>,
) {
    let name = node.attr("Name").unwrap_or("").to_string();
    if node.attr("Type") == Some("0") {
        // folder; the ROOT folder does not contribute to names
        if !is_root {
            path.push(name);
        }
        for c in node.children.iter().filter(|c| c.name == "NODE") {
            walk_rekordbox_nodes(c, path, false, by_id, by_path, out);
        }
        if !is_root {
            path.pop();
        }
        return;
    }
    let by_location = node.attr("KeyType") == Some("1");
    let mut pl = ForeignPlaylist { name: path.iter().cloned().chain([name]).collect::<Vec<_>>().join(" / "), ..Default::default() };
    for t in node.children.iter().filter(|c| c.name == "TRACK") {
        let key = t.attr("Key").unwrap_or("");
        let idx = if by_location { by_path.get(&location_to_path(key)) } else { by_id.get(key) };
        match idx {
            Some(i) => pl.tracks.push(*i),
            None => pl.dangling += 1,
        }
    }
    out.push(pl);
}

// ---- iTunes / Music Library.xml (a property list) ----

#[derive(Debug)]
enum Value {
    Dict(Vec<(String, Value)>),
    Array(Vec<Value>),
    Str(String),
    Int(i64),
    Bool(bool),
    Other,
}

impl Value {
    fn get(&self, k: &str) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        }
    }
    fn str(&self, k: &str) -> &str {
        match self.get(k) {
            Some(Value::Str(s)) => s,
            _ => "",
        }
    }
    fn int(&self, k: &str) -> Option<i64> {
        match self.get(k) {
            Some(Value::Int(i)) => Some(*i),
            _ => None,
        }
    }
    fn flag(&self, k: &str) -> bool {
        matches!(self.get(k), Some(Value::Bool(true)))
    }
}

/// The only dictionary keys kept while reading a library: everything else (artwork, dates,
/// play counts, ...) is skipped as it is read, so a 100 000-track library stays small.
const PLIST_KEEP: &[&str] = &[
    "Tracks",
    "Playlists",
    "Playlist Items",
    "Track ID",
    "Name",
    "Artist",
    "Album",
    "Genre",
    "Rating",
    "Location",
    "Size",
    "Total Time",
    "Track Type",
    "Playlist Persistent ID",
    "Parent Persistent ID",
    "Folder",
    "Master",
    "Distinguished Kind",
    "Bpm",
];

fn plist_value(r: &mut Reader, kind: &str, depth: usize, keep_all: bool) -> Result<Value, Error> {
    if depth > MAX_DEPTH {
        return Err(bad("plist: nested too deeply"));
    }
    Ok(match kind {
        "dict" => {
            let mut out = Vec::new();
            // Track and playlist dictionaries are keyed by the other program's own names; the
            // top dictionary and the "Tracks" dictionary (keyed by id) keep every key.
            while let Some((n, _)) = r.next_start()? {
                if n != "key" {
                    r.skip_element()?;
                    continue;
                }
                let key = r.text_of_element()?;
                let Some((vk, _)) = r.next_start()? else { return Err(bad("plist: key without a value")) };
                if keep_all || PLIST_KEEP.contains(&key.as_str()) {
                    let child_keep_all = depth == 0 && key == "Tracks";
                    out.push((key, plist_value(r, &vk, depth + 1, child_keep_all)?));
                } else {
                    r.skip_element()?;
                }
            }
            Value::Dict(out)
        }
        "array" => {
            let mut out = Vec::new();
            while let Some((n, _)) = r.next_start()? {
                out.push(plist_value(r, &n, depth + 1, false)?);
            }
            Value::Array(out)
        }
        "string" => Value::Str(r.text_of_element()?),
        "integer" => Value::Int(r.text_of_element()?.trim().parse().unwrap_or(0)),
        "true" => {
            r.skip_element()?;
            Value::Bool(true)
        }
        "false" => {
            r.skip_element()?;
            Value::Bool(false)
        }
        _ => {
            r.skip_element()?;
            Value::Other
        }
    })
}

/// Parses an iTunes / Apple Music `Library.xml` export.
pub fn parse_itunes(text: &str) -> Result<ForeignLibrary, Error> {
    let mut r = Reader::new(text);
    let (name, _) = r.next_start()?.ok_or_else(|| bad("the file has no root element"))?;
    if name != "plist" {
        return Err(bad("not an iTunes library file (no plist element)"));
    }
    let Some((dk, _)) = r.next_start()? else { return Err(bad("the property list is empty")) };
    if dk != "dict" {
        return Err(bad("unexpected property list layout"));
    }
    let top = plist_value(&mut r, &dk, 0, true)?;

    let mut tracks = Vec::new();
    let mut by_id: HashMap<i64, usize> = HashMap::new();
    if let Some(Value::Dict(items)) = top.get("Tracks") {
        for (key, t) in items {
            if !matches!(t, Value::Dict(_)) {
                continue;
            }
            let id = t.int("Track ID").or_else(|| key.parse().ok());
            let rating = t.int("Rating").unwrap_or(0);
            let stars = ((rating as f64) / 20.0).round().clamp(0.0, 5.0) as u8;
            let ft = ForeignTrack {
                title: t.str("Name").to_string(),
                artist: t.str("Artist").to_string(),
                album: t.str("Album").to_string(),
                genre: t.str("Genre").to_string(),
                bpm: t.int("Bpm").filter(|b| *b > 0).map(|b| b as f64),
                stars: Some(stars).filter(|s| *s > 0),
                path: if t.str("Track Type") == "URL" { String::new() } else { location_to_path(t.str("Location")) },
                size: t.int("Size").filter(|s| *s > 0).map(|s| s as u64),
                duration_secs: t.int("Total Time").filter(|s| *s > 0).map(|ms| ms as f64 / 1000.0),
                ..Default::default()
            };
            if let Some(id) = id {
                by_id.insert(id, tracks.len());
            }
            tracks.push(ft);
        }
    }

    let mut playlists = Vec::new();
    if let Some(Value::Array(pls)) = top.get("Playlists") {
        // persistent id -> (name, parent) so folder names can prefix their playlists
        let mut names: HashMap<&str, (&str, &str)> = HashMap::new();
        for p in pls {
            names.insert(p.str("Playlist Persistent ID"), (p.str("Name"), p.str("Parent Persistent ID")));
        }
        for p in pls {
            // the built-in lists (Library, Music, Movies, Purchased, ...) are not the user's
            if p.flag("Master") || p.get("Distinguished Kind").is_some() || p.flag("Folder") {
                continue;
            }
            let mut parts = vec![p.str("Name")];
            let mut parent = p.str("Parent Persistent ID");
            for _ in 0..16 {
                match names.get(parent) {
                    Some((n, pp)) if !parent.is_empty() => {
                        parts.push(n);
                        parent = pp;
                    }
                    _ => break,
                }
            }
            parts.reverse();
            let mut pl = ForeignPlaylist { name: parts.join(" / "), ..Default::default() };
            if let Some(Value::Array(items)) = p.get("Playlist Items") {
                for it in items {
                    match it.int("Track ID").and_then(|id| by_id.get(&id)) {
                        Some(i) => pl.tracks.push(*i),
                        None => pl.dangling += 1,
                    }
                }
            }
            playlists.push(pl);
        }
    }
    Ok(ForeignLibrary { source: Source::Itunes, tracks, playlists, notes: vec![] })
}

/// Reads a rekordbox, iTunes or Traktor library file, detecting which from the root element.
pub fn parse_library(text: &str) -> Result<ForeignLibrary, Error> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut r = Reader::new(t);
    match r.next_start()? {
        Some((n, _)) if n == "DJ_PLAYLISTS" => parse_rekordbox(t),
        Some((n, _)) if n == "plist" => parse_itunes(t),
        Some((n, _)) if n == "NML" => traktor::parse(t),
        _ => Err(bad("this is not a rekordbox XML, iTunes / Music Library.xml or Traktor collection.nml file")),
    }
}

/// Library files bigger than this are refused instead of read into memory.
pub const MAX_LIBRARY_FILE: u64 = 2 << 30;
/// Playlists and crates are small; a bigger one is not one.
const MAX_SMALL_FILE: u64 = 256 << 20;

fn lossy(bytes: Vec<u8>) -> String {
    // no second copy of a big file when it is valid UTF-8 (the usual case)
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Opens whatever the user picked and reads it with the right reader: a folder is a Serato
/// `_Serato_` folder; a file is recognised by its extension (`.crate`, `.m3u` / `.m3u8`, `.nml`,
/// `.xml`) and, failing that, by its first bytes.
pub fn read_path(path: &Path) -> Result<ForeignLibrary, Error> {
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        return serato::read_path(path);
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let cap = if matches!(ext.as_str(), "xml" | "nml") { MAX_LIBRARY_FILE } else { MAX_SMALL_FILE };
    if meta.len() > cap {
        return Err(bad(format!("that file is larger than {} MB; it is not a library export", cap >> 20)));
    }
    if ext == "crate" {
        return serato::read_path(path);
    }
    let bytes = std::fs::read(path)?;
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let base = path.parent().map(|p| p.display().to_string());
    let head = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let looks_xml = head.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<');
    match ext.as_str() {
        "m3u" | "m3u8" => m3u::parse(&bytes, &name, base.as_deref()),
        "xml" | "nml" => parse_library(&lossy(bytes)),
        _ if head.starts_with(b"vrsn") => serato::read_path(path),
        _ if looks_xml => parse_library(&lossy(bytes)),
        _ if head.starts_with(b"#EXTM3U") => m3u::parse(&bytes, &name, base.as_deref()),
        _ => Err(bad("this is not a library file the importer knows (rekordbox XML, iTunes XML, Traktor NML, Serato crate, M3U / M3U8)")),
    }
}

// ---------------------------------------------------------------------------------------------
// Matching and the import plan
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ImportOptions {
    /// Copy star ratings onto matched tracks (unrated tracks in the source never clear a rating).
    pub ratings: bool,
    /// Create or extend cases from playlists.
    pub playlists: bool,
    /// Which playlists (indexes into the plan's `cases`' `index`); `None` = all.
    pub playlist_indexes: Option<Vec<usize>>,
    /// What to do with the first cue point of a matched track (see [`CueMode`]).
    pub cues: CueMode,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { ratings: true, playlists: true, playlist_indexes: None, cues: CueMode::Off }
    }
}

/// The device keeps one cue point per track. Writing it changes what the device cues to, so the
/// default is to leave cues alone; the user picks how far an import may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CueMode {
    /// Report cues, write none.
    #[default]
    Off,
    /// Set the cue only on tracks that have none.
    Empty,
    /// Also replace a cue the track already has.
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CueAction {
    /// The track has no cue yet.
    Set,
    /// The track has a different cue.
    Replace,
    /// The track already has this cue.
    Same,
}

impl CueMode {
    /// Whether a cue with this action is written in this mode (the preview and the apply both ask).
    pub fn applies(self, a: CueAction) -> bool {
        matches!((self, a), (CueMode::Empty, CueAction::Set) | (CueMode::Overwrite, CueAction::Set | CueAction::Replace))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RatingChange {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub from: i64,
    pub to: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedTrack {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseAction {
    /// A new case.
    Create,
    /// A case of this name exists; the missing tracks are appended, none removed.
    Extend,
    /// A case of this name already holds every matched track.
    Unchanged,
    /// None of the playlist's tracks are on the device.
    Skip,
}

#[derive(Debug, Clone, Serialize)]
pub struct CasePlan {
    /// Index of the playlist in the source file.
    pub index: usize,
    pub name: String,
    pub action: CaseAction,
    pub case_id: Option<i64>,
    /// Tracks this step adds, in playlist order.
    pub tracks: Vec<PlannedTrack>,
    pub already_in: usize,
    /// Playlist entries with no counterpart on the device (or in the file).
    pub unmatched: usize,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CueNote {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub cue_secs: f64,
    /// `cue_secs` in source frames at the track's sample rate, the unit the device row stores.
    pub cue_frames: i64,
    /// The cue the track has now, in source frames.
    pub existing_frames: Option<i64>,
    /// The track's sample rate, the unit `cue_frames` and `existing_frames` are counted in.
    pub sample_rate: u32,
    pub action: CueAction,
}

/// A matched track the other program gave a colour. Colour labels are not stored on the device,
/// so the plan only reports them (the UI turns the index into a label colour and writes the
/// app-side label store when the user asks).
#[derive(Debug, Clone, Serialize)]
pub struct ColourNote {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    /// The other program's own colour index.
    pub index: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnmatchedTrack {
    pub title: String,
    pub artist: String,
    pub path: String,
    pub reason: String,
    /// Whether `path` was looked up on this computer (false for a network, device, URL or relative
    /// location, which is never probed); `on_disk` only means something when it is true.
    pub path_checked: bool,
    pub on_disk: bool,
    pub in_playlist: bool,
}

/// How many unmatched tracks are listed in a plan (the count and `addable_paths` are complete).
pub const MAX_LISTED_UNMATCHED: usize = 1000;

#[derive(Debug, Clone, Serialize)]
pub struct ImportPlan {
    pub source: Source,
    pub foreign_tracks: usize,
    pub matched: usize,
    pub matched_by_file: usize,
    pub matched_by_tags: usize,
    pub ratings: Vec<RatingChange>,
    pub ratings_unchanged: usize,
    pub cases: Vec<CasePlan>,
    /// The first cue found for each matched track and what writing it would do; which of them are
    /// written is the [`CueMode`] option.
    pub cues: Vec<CueNote>,
    /// The colour each matched track has in the other program (Traktor only today), whatever
    /// the options: they are never written to the device.
    pub colours: Vec<ColourNote>,
    /// What the reader skipped (damaged crates, smart crates...).
    pub notes: Vec<String>,
    pub unmatched_total: usize,
    pub unmatched: Vec<UnmatchedTrack>,
    /// Unmatched files that exist on this computer: what the add-tracks job could bring over.
    pub addable_paths: Vec<String>,
}

enum Matched {
    ByFile(i64),
    ByTags(i64),
    No(String),
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn match_tracks(lib_tracks: &[Track], foreign: &[ForeignTrack]) -> Vec<Matched> {
    let mut by_loc: HashMap<&str, i64> = HashMap::new();
    let mut by_tags: HashMap<(String, String), Vec<&Track>> = HashMap::new();
    for t in lib_tracks {
        by_loc.insert(t.location.as_str(), t.track_id);
        if !t.title.trim().is_empty() {
            by_tags.entry((norm(&t.title), norm(&t.artist))).or_default().push(t);
        }
    }
    foreign
        .iter()
        .map(|f| {
            // 1. the device names a file after its name and size, so the same file maps to the
            //    same location whatever folder it came from
            if !f.path.is_empty() {
                // a file already inside this library's own music folder (our own export)
                let p = f.path.replace('\\', "/");
                let marker = format!("{}/", naming::MUSIC_DIR);
                if let Some(at) = p.find(&marker) {
                    let loc = format!("{}{}", naming::LOCATION_PREFIX, &p[at + marker.len()..]);
                    if let Some(id) = by_loc.get(loc.as_str()) {
                        return Matched::ByFile(*id);
                    }
                }
                // the length comes from the file only when it is a plain local path (never a share)
                let size = f.size.or_else(|| paths::local_file_len(&f.path));
                if let Some(size) = size {
                    let (_, loc) = naming::device_paths(file_name_of(&f.path), size);
                    if let Some(id) = by_loc.get(loc.as_str()) {
                        return Matched::ByFile(*id);
                    }
                }
            }
            // 2. same title and artist, and the same length when both are known
            if f.title.trim().is_empty() {
                return Matched::No("no file match and no title to compare".into());
            }
            if f.duration_secs.is_none() {
                return Matched::No("not in the library (the file gives no length to confirm a title match)".into());
            }
            let cands: Vec<&&Track> = by_tags
                .get(&(norm(&f.title), norm(&f.artist)))
                .map(|v| {
                    v.iter()
                        .filter(|t| match f.duration_secs {
                            // a tag match without a length on both sides is too weak to write ratings on
                            Some(d) if t.play_time_secs > 0 => (d - t.play_time_secs as f64).abs() <= 2.5,
                            _ => false,
                        })
                        .collect()
                })
                .unwrap_or_default();
            match cands.len() {
                0 => Matched::No("not in the library (or no length to confirm a title match)".into()),
                1 => Matched::ByTags(cands[0].track_id),
                n => Matched::No(format!("{n} device tracks share this title and artist")),
            }
        })
        .collect()
}

/// Matches `foreign` against the device library and works out every change. Writes nothing.
pub fn plan_import(lib: &Library, foreign: &ForeignLibrary, opts: &ImportOptions) -> Result<ImportPlan, Error> {
    let lib_tracks = lib.tracks()?;
    let by_id: HashMap<i64, &Track> = lib_tracks.iter().map(|t| (t.track_id, t)).collect();
    let matches = match_tracks(&lib_tracks, &foreign.tracks);

    let id_of = |m: &Matched| match m {
        Matched::ByFile(i) | Matched::ByTags(i) => Some(*i),
        Matched::No(_) => None,
    };
    let (mut by_file, mut by_tags) = (0, 0);
    let mut ratings = Vec::new();
    let mut unchanged = 0;
    let mut cues = Vec::new();
    let mut colours = Vec::new();
    let mut seen_colour: HashSet<i64> = HashSet::new();
    let mut seen_rating: HashSet<i64> = HashSet::new();
    let mut seen_cue: HashSet<i64> = HashSet::new();
    let mut cues_past_end = 0usize;
    for (f, m) in foreign.tracks.iter().zip(&matches) {
        match m {
            Matched::ByFile(_) => by_file += 1,
            Matched::ByTags(_) => by_tags += 1,
            Matched::No(_) => continue,
        }
        let id = id_of(m).unwrap_or(-1);
        let t = by_id[&id];
        if let Some(stars) = f.stars {
            if seen_rating.insert(id) {
                if t.rating == i64::from(stars) {
                    unchanged += 1;
                } else {
                    ratings.push(RatingChange { track_id: id, title: t.title.clone(), artist: t.artist.clone(), from: t.rating, to: stars });
                }
            }
        }
        if let Some(index) = f.colour {
            if seen_colour.insert(id) {
                colours.push(ColourNote { track_id: id, title: t.title.clone(), artist: t.artist.clone(), index });
            }
        }
        if let Some(c) = f.cue_secs.filter(|c| c.is_finite() && *c >= 0.0 && *c < 1e6) {
            if seen_cue.insert(id) {
                // a cue beyond the end of the track (another edit of the song, or a damaged
                // file) would put the device's cue where there is no audio: left out, and said so
                if t.play_time_secs > 0 && c > t.play_time_secs as f64 + 1.0 {
                    cues_past_end += 1;
                    continue;
                }
                let rate = lib.sample_rate_of(id)?;
                let frames = (c * f64::from(rate)).round() as i64;
                let existing = lib.track_cue(id)?;
                let action = match existing {
                    None => CueAction::Set,
                    Some(e) if e == frames => CueAction::Same,
                    Some(_) => CueAction::Replace,
                };
                cues.push(CueNote {
                    track_id: id,
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    cue_secs: c,
                    cue_frames: frames,
                    existing_frames: existing,
                    sample_rate: rate,
                    action,
                });
            }
        }
    }

    let existing_cases = lib.cases()?;
    // The device allows several cases with one name: the Nth playlist of a name maps to the Nth
    // case of that name (names compared trimmed), so importing the same file again finds the
    // cases it made the first time.
    let mut claimed: HashSet<i64> = HashSet::new();
    let mut cases = Vec::new();
    for (index, pl) in foreign.playlists.iter().enumerate() {
        let mut name: String = pl.name.trim().chars().take(crate::library::MAX_CASE_NAME_CHARS).collect();
        if name.is_empty() {
            name = "Imported playlist".into();
        }
        let mut ids: Vec<i64> = Vec::new();
        let mut unmatched = pl.dangling;
        for ti in &pl.tracks {
            match id_of(&matches[*ti]) {
                Some(id) => {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                None => unmatched += 1,
            }
        }
        let existing = existing_cases.iter().find(|c| !claimed.contains(&c.id) && c.name.trim() == name);
        if let Some(c) = existing {
            claimed.insert(c.id);
        }
        let (action, case_id, already_in, add): (CaseAction, Option<i64>, usize, Vec<i64>) = match existing {
            _ if ids.is_empty() => (CaseAction::Skip, None, 0, vec![]),
            Some(c) => {
                let have: HashSet<i64> = lib.case_track_ids(c.id)?.into_iter().collect();
                let add: Vec<i64> = ids.iter().copied().filter(|i| !have.contains(i)).collect();
                let already = ids.len() - add.len();
                let act = if add.is_empty() { CaseAction::Unchanged } else { CaseAction::Extend };
                (act, Some(c.id), already, add)
            }
            None => (CaseAction::Create, None, 0, ids),
        };
        let selected = opts.playlists && opts.playlist_indexes.as_ref().is_none_or(|v| v.contains(&index));
        cases.push(CasePlan {
            index,
            name,
            action,
            case_id,
            tracks: add
                .iter()
                .map(|i| PlannedTrack { track_id: *i, title: by_id[i].title.clone(), artist: by_id[i].artist.clone() })
                .collect(),
            already_in,
            unmatched,
            selected,
        });
    }

    let in_playlist: HashSet<usize> = foreign.playlists.iter().flat_map(|p| p.tracks.iter().copied()).collect();
    let mut unmatched: Vec<(usize, UnmatchedTrack)> = Vec::new();
    for (i, (f, m)) in foreign.tracks.iter().zip(&matches).enumerate() {
        if let Matched::No(reason) = m {
            let on_disk = paths::local_is_file(&f.path);
            let reason = if f.path.is_empty() && reason.starts_with("not in the library") {
                "not in the library (and the file has no location)".to_string()
            } else {
                reason.clone()
            };
            // a location that was never probed says so, instead of looking like "file not found"
            let reason = match (f.path.is_empty(), paths::rejection(&f.path)) {
                (false, Some(why)) => format!("{reason}; the file location was not checked because {why}"),
                _ => reason,
            };
            unmatched.push((
                i,
                UnmatchedTrack {
                    title: f.title.clone(),
                    artist: f.artist.clone(),
                    path: f.path.clone(),
                    reason,
                    path_checked: paths::is_local(&f.path),
                    on_disk,
                    in_playlist: in_playlist.contains(&i),
                },
            ));
        }
    }
    let addable_paths: Vec<String> = unmatched.iter().filter(|(_, u)| u.on_disk).map(|(_, u)| u.path.clone()).collect();
    let unmatched_total = unmatched.len();
    unmatched.sort_by_key(|(i, u)| (!u.in_playlist, *i));
    let unmatched = unmatched.into_iter().take(MAX_LISTED_UNMATCHED).map(|(_, u)| u).collect();

    let mut notes = foreign.notes.clone();
    if cues_past_end > 0 {
        let (s, verb, were) = if cues_past_end == 1 { ("", "lies", "was") } else { ("s", "lie", "were") };
        notes.push(format!("{cues_past_end} cue point{s} {verb} beyond the end of the matching track and {were} left out."));
    }
    Ok(ImportPlan {
        source: foreign.source,
        foreign_tracks: foreign.tracks.len(),
        matched: by_file + by_tags,
        matched_by_file: by_file,
        matched_by_tags: by_tags,
        ratings: if opts.ratings { ratings } else { vec![] },
        ratings_unchanged: unchanged,
        cases,
        cues,
        colours,
        notes,
        unmatched_total,
        unmatched,
        addable_paths,
    })
}

impl Library {
    /// A track's sample rate; a row without one (never written by the Editor) is read as CD rate,
    /// as the grid editor does.
    fn sample_rate_of(&self, track_id: i64) -> Result<u32, Error> {
        use rusqlite::OptionalExtension;
        let r: Option<i64> =
            self.con.query_row("SELECT IFNULL(sample_rate,0) FROM tracks WHERE track_id = ?1 AND track_id >= 0", [track_id], |r| r.get(0)).optional()?;
        Ok(r.filter(|r| *r > 0 && *r <= 768_000).unwrap_or(44_100) as u32)
    }

    /// Sets several ratings in one transaction (all or none), each exactly as
    /// [`Library::set_track_rating`] does. Returns how many rows changed.
    pub fn set_track_ratings(&mut self, changes: &[(i64, u8)]) -> Result<usize, Error> {
        self.con.execute_batch("BEGIN IMMEDIATE")?;
        let mut changed = 0;
        for (id, stars) in changes {
            match self.set_track_rating(*id, *stars) {
                Ok(c) => changed += usize::from(c),
                Err(e) => {
                    let _ = self.con.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
        }
        if let Err(e) = self.con.execute_batch("COMMIT") {
            let _ = self.con.execute_batch("ROLLBACK");
            return Err(e.into());
        }
        Ok(changed)
    }

    /// Sets several cue points in one transaction (all or none), each exactly as
    /// [`Library::set_track_cue`] does. Returns how many rows changed.
    pub fn set_track_cues(&mut self, changes: &[(i64, i64)]) -> Result<usize, Error> {
        self.con.execute_batch("BEGIN IMMEDIATE")?;
        let mut changed = 0;
        for (id, frames) in changes {
            match self.set_track_cue(*id, Some(*frames)) {
                Ok(c) => changed += usize::from(c),
                Err(e) => {
                    let _ = self.con.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
        }
        if let Err(e) = self.con.execute_batch("COMMIT") {
            let _ = self.con.execute_batch("ROLLBACK");
            return Err(e.into());
        }
        Ok(changed)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ApplyReport {
    pub ratings_set: usize,
    pub cases_created: usize,
    pub cases_extended: usize,
    pub cues_set: usize,
    pub tracks_added_to_cases: usize,
    pub failures: Vec<(String, String)>,
}

/// One progress step of [`apply_plan`].
pub struct Step<'a> {
    pub label: &'a str,
    pub done: usize,
    pub total: usize,
    pub error: Option<&'a str>,
}

/// Performs a plan. Ratings go in one transaction and cue points in another; each case is created and filled, and a case
/// whose membership cannot be written is deleted again, so a failure never leaves an empty
/// case behind. A failing step is reported and the rest carries on.
pub fn apply_plan(
    lib: &mut Library,
    plan: &ImportPlan,
    opts: &ImportOptions,
    mut progress: impl FnMut(Step),
) -> Result<ApplyReport, Error> {
    let mut rep = ApplyReport::default();
    let do_ratings = opts.ratings && !plan.ratings.is_empty();
    let cue_changes: Vec<&CueNote> = plan.cues.iter().filter(|c| opts.cues.applies(c.action)).collect();
    let cases: Vec<&CasePlan> =
        plan.cases.iter().filter(|c| c.selected && matches!(c.action, CaseAction::Create | CaseAction::Extend)).collect();
    let total = usize::from(do_ratings) + usize::from(!cue_changes.is_empty()) + cases.len();
    let mut done = 0;

    if do_ratings {
        let changes: Vec<(i64, u8)> = plan.ratings.iter().map(|r| (r.track_id, r.to)).collect();
        let label = format!("{} ratings", changes.len());
        done += 1;
        match lib.set_track_ratings(&changes) {
            Ok(n) => {
                rep.ratings_set = n;
                progress(Step { label: &label, done, total, error: None });
            }
            Err(e) => {
                let msg = e.to_string();
                progress(Step { label: &label, done, total, error: Some(&msg) });
                rep.failures.push((label, msg));
            }
        }
    }

    if !cue_changes.is_empty() {
        let label = format!("{} cue point{}", cue_changes.len(), if cue_changes.len() == 1 { "" } else { "s" });
        done += 1;
        let changes: Vec<(i64, i64)> = cue_changes.iter().map(|c| (c.track_id, c.cue_frames)).collect();
        let mut failed = None;
        match lib.set_track_cues(&changes) {
            Ok(n) => rep.cues_set = n,
            Err(e) => failed = Some(e.to_string()),
        }
        progress(Step { label: &label, done, total, error: failed.as_deref() });
        if let Some(msg) = failed {
            rep.failures.push((label, msg));
        }
    }

    for c in cases {
        done += 1;
        let result = (|| -> Result<(bool, usize), Error> {
            let ids: Vec<i64> = c.tracks.iter().map(|t| t.track_id).collect();
            match c.action {
                CaseAction::Create => {
                    let id = lib.create_case(&c.name)?;
                    if let Err(e) = lib.set_case_tracks(id, &ids) {
                        let _ = lib.delete_case(id);
                        return Err(e);
                    }
                    Ok((true, ids.len()))
                }
                _ => {
                    let id = c.case_id.ok_or_else(|| bad("case to extend has no id"))?;
                    let mut all = lib.case_track_ids(id)?;
                    let have: HashSet<i64> = all.iter().copied().collect();
                    let new: Vec<i64> = ids.into_iter().filter(|i| !have.contains(i)).collect();
                    all.extend(&new);
                    lib.set_case_tracks(id, &all)?;
                    Ok((false, new.len()))
                }
            }
        })();
        match result {
            Ok((created, n)) => {
                if created {
                    rep.cases_created += 1;
                } else {
                    rep.cases_extended += 1;
                }
                rep.tracks_added_to_cases += n;
                progress(Step { label: &c.name, done, total, error: None });
            }
            Err(e) => {
                let msg = e.to_string();
                progress(Step { label: &c.name, done, total, error: Some(&msg) });
                rep.failures.push((c.name.clone(), msg));
            }
        }
    }
    Ok(rep)
}

impl ApplyReport {
    /// The finished line shown to the user.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.ratings_set > 0 {
            parts.push(format!("{} rating{} set", self.ratings_set, if self.ratings_set == 1 { "" } else { "s" }));
        }
        if self.cues_set > 0 {
            parts.push(format!("{} cue point{} set", self.cues_set, if self.cues_set == 1 { "" } else { "s" }));
        }
        if self.cases_created > 0 {
            parts.push(format!("{} case{} created", self.cases_created, if self.cases_created == 1 { "" } else { "s" }));
        }
        if self.cases_extended > 0 {
            parts.push(format!("{} case{} extended", self.cases_extended, if self.cases_extended == 1 { "" } else { "s" }));
        }
        if !self.failures.is_empty() {
            parts.push(format!("{} failed", self.failures.len()));
        }
        if parts.is_empty() {
            "nothing to change".into()
        } else {
            parts.join(", ")
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Export: M3U8, CSV, rekordbox XML
// ---------------------------------------------------------------------------------------------

/// What a list export covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Library,
    Case(i64),
    Mix(i64),
}

/// The name of a scope and its tracks in order (a mix keeps repeats; tracks removed from the
/// library, which a mix remembers only by name, are left out).
pub fn scope_tracks(lib: &Library, scope: Scope) -> Result<(String, Vec<Track>), Error> {
    let all = lib.tracks()?;
    let by_id: HashMap<i64, &Track> = all.iter().map(|t| (t.track_id, t)).collect();
    let pick = |ids: Vec<i64>| -> Vec<Track> { ids.iter().filter_map(|i| by_id.get(i).map(|t| (*t).clone())).collect() };
    match scope {
        Scope::Library => Ok(("Library".into(), all.clone())),
        Scope::Case(id) => {
            let name = lib.cases()?.into_iter().find(|c| c.id == id).map(|c| c.name).ok_or_else(|| bad(format!("no case {id}")))?;
            Ok((name, pick(lib.case_track_ids(id)?)))
        }
        Scope::Mix(id) => {
            let info = lib.mix_info(id)?.ok_or_else(|| bad(format!("no mix {id}")))?;
            let ids = lib.mix_track_rows(id)?.into_iter().map(|r| r.track_id).collect();
            Ok((info.name, pick(ids)))
        }
    }
}

fn audio_path(lib: &Library, t: &Track) -> String {
    lib.track_paths(&t.location).map(|(a, _)| a.display().to_string()).unwrap_or_else(|| t.location.clone())
}

/// Where a list export says each track's audio file is: by track id, the text to write in place
/// of the file's path on the volume (the app uses it for copies of the audio made beside the
/// export, and for paths relative to the playlist). A track that is not in the map keeps its
/// path on the volume.
pub type PathMap = HashMap<i64, String>;

fn audio_path_in(lib: &Library, t: &Track, paths: &PathMap) -> String {
    paths.get(&t.track_id).cloned().unwrap_or_else(|| audio_path(lib, t))
}

fn one_line(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// An extended M3U in UTF-8 (the `.m3u8` convention), one entry per track with its length and
/// "artist - title", pointing at the audio files on the volume.
pub fn export_m3u8(lib: &Library, tracks: &[Track]) -> String {
    export_m3u8_with(lib, tracks, &PathMap::new())
}

/// [`export_m3u8`] with the audio paths in `paths` written instead of the paths on the volume.
pub fn export_m3u8_with(lib: &Library, tracks: &[Track], paths: &PathMap) -> String {
    let mut s = String::from("#EXTM3U\n");
    for t in tracks {
        let label = match (t.artist.trim().is_empty(), t.title.trim().is_empty()) {
            (true, true) => file_name_of(&t.location).to_string(),
            (true, false) => t.title.clone(),
            (false, true) => t.artist.clone(),
            (false, false) => format!("{} - {}", t.artist, t.title),
        };
        s.push_str(&format!("#EXTINF:{},{}\n{}\n", t.play_time_secs.max(-1), one_line(&label), one_line(&audio_path_in(lib, t, paths))));
    }
    s
}

/// RFC 4180 CSV (CRLF rows, quoted where needed) with a UTF-8 byte-order mark so spreadsheet
/// programs read the accents correctly.
pub fn export_csv(lib: &Library, tracks: &[Track]) -> String {
    export_csv_with(lib, tracks, &PathMap::new())
}

/// [`export_csv`] with the audio paths in `paths` written instead of the paths on the volume.
pub fn export_csv_with(lib: &Library, tracks: &[Track], paths: &PathMap) -> String {
    let mut s = String::from("\u{feff}Title,Artist,Album,Genre,BPM,Key,Rating,Duration (s),Format,File\r\n");
    for t in tracks {
        let row = [
            csv_text(&t.title),
            csv_text(&t.artist),
            csv_text(&t.album),
            csv_text(&t.genre),
            format!("{:.2}", t.bpm),
            csv_text(&t.key),
            t.rating.to_string(),
            t.play_time_secs.to_string(),
            csv_text(&t.format),
            csv_text(&audio_path_in(lib, t, paths)),
        ];
        s.push_str(&row.join(","));
        s.push_str("\r\n");
    }
    s
}

/// Text for an XML element or attribute value in the rekordbox-style XML export: the four markup
/// characters escaped, line breaks and other control characters turned into spaces (tabs stay).
/// The VirtualDJ writer has its own rule (`commands/export/vdj.rs`) because that reader wants
/// apostrophes escaped and controls dropped, and `mix.rs` writes the Pacemaker's fixed format.
fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if c.is_control() && c != '\t' => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

fn file_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    let mut s = String::from("file://localhost");
    if !p.starts_with('/') {
        s.push('/');
    }
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// `YYYY-MM-DD` (UTC) of a Unix-millisecond timestamp.
fn ymd(ms: i64) -> String {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", yoe + era * 400 + i64::from(m <= 2), m, d)
}

struct Extras {
    sample_rate: i64,
    bit_rate: i64,
    year: i64,
    track_number: i64,
    composer: String,
    comments: String,
    label: String,
    date_added: i64,
}

impl Library {
    fn interop_extras(&self) -> Result<HashMap<i64, Extras>, Error> {
        let mut st = self.con.prepare(
            "SELECT track_id, IFNULL(sample_rate,0), IFNULL(bit_rate,0), IFNULL(year,0), IFNULL(track_number,0),
                    IFNULL(composer,''), IFNULL(comments,''), IFNULL(label,''), IFNULL(date_added,0)
             FROM tracks WHERE track_id >= 0",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                Extras {
                    sample_rate: r.get(1)?,
                    bit_rate: r.get(2)?,
                    year: r.get(3)?,
                    track_number: r.get(4)?,
                    composer: r.get(5)?,
                    comments: r.get(6)?,
                    label: r.get(7)?,
                    date_added: r.get(8)?,
                },
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// `TEMPO` elements for a beat grid: one at the first beat and one wherever the local tempo
/// moves by more than 0.01 BPM. `Battito` is the beat's number in its bar, counted from the
/// grid's first downbeat (beat 1 when the grid records none).
fn tempo_elements(a: &strfile::Analysis, sample_rate: f64) -> String {
    let mut s = String::new();
    let down = a.first_downbeat.unwrap_or(0) as i64;
    let mut last = f64::NAN;
    for (i, b) in a.beats.iter().enumerate() {
        if last.is_nan() || (b.bpm - last).abs() > 0.01 {
            let battito = (i as i64 - down).rem_euclid(4) + 1;
            s.push_str(&format!(
                "      <TEMPO Inizio=\"{:.3}\" Bpm=\"{:.2}\" Metro=\"4/4\" Battito=\"{}\"/>\n",
                b.pos as f64 / sample_rate,
                b.bpm,
                battito
            ));
            last = b.bpm;
        }
    }
    s
}

/// A rekordbox XML document of the whole library: every track with its tags, rating, key,
/// location and tempo map (from the `.str` grids), then every case (and mix, under a "Mixes"
/// folder) as a playlist. The device's single cue point is not exported: its unit is not
/// confirmed (notes/FINDINGS.md), so a guess could put cues in the wrong place.
pub fn export_rekordbox(lib: &Library) -> Result<String, Error> {
    export_rekordbox_with(lib, &XmlOptions::default())
}

/// [`export_rekordbox`] for one case: the collection holds only that case's tracks (in case
/// order) and the playlist tree only that case, so other software imports just this list.
pub fn export_rekordbox_case(lib: &Library, case_id: i64) -> Result<String, Error> {
    export_rekordbox_with(lib, &XmlOptions { only_case: Some(case_id), ..Default::default() })
}

/// A playlist written into a rekordbox export besides the device's own cases and mixes (the app
/// uses it for smart cases: rekordbox XML has only folders and plain playlists, so a smart
/// case travels as the list of tracks it matches right now).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraPlaylist {
    pub name: String,
    /// Library track ids in playlist order; ids not in the library are left out.
    pub track_ids: Vec<i64>,
}

/// What [`export_rekordbox_with`] writes. The default is the whole library with every case
/// and mix, exactly [`export_rekordbox`].
#[derive(Debug, Clone, Default)]
pub struct XmlOptions {
    /// Only this case (and its tracks), no mixes.
    pub only_case: Option<i64>,
    /// Write no cases and no mixes, only `extra`; the collection holds just the tracks the
    /// extra playlists use.
    pub extras_only: bool,
    /// Cases left out of the playlist tree (their tracks stay in the collection).
    pub skip_cases: HashSet<i64>,
    /// Tracks left out of the file altogether: not in the collection, not in any playlist
    /// (the app uses it for tracks whose file location is not a plain path in the music folder).
    pub exclude_tracks: HashSet<i64>,
    pub extra: Vec<ExtraPlaylist>,
    /// Put the extra playlists in a folder of this name instead of beside the cases.
    pub extra_folder: Option<String>,
    /// Audio paths to write for these tracks instead of their paths on the volume.
    pub paths: PathMap,
}

/// [`export_rekordbox`] with the choices in [`XmlOptions`].
pub fn export_rekordbox_with(lib: &Library, opts: &XmlOptions) -> Result<String, Error> {
    let mut tracks = lib.tracks()?;
    tracks.retain(|t| !opts.exclude_tracks.contains(&t.track_id));
    let mut cases = lib.cases()?;
    let mut mixes = lib.mix_infos()?;
    let restricted = opts.only_case.is_some() || opts.extras_only;
    if restricted {
        // the collection: the chosen case's tracks in case order, then the extras' tracks
        let mut order: Vec<i64> = Vec::new();
        if let Some(cid) = opts.only_case {
            if !cases.iter().any(|c| c.id == cid) {
                return Err(bad(format!("no case {cid}")));
            }
            cases.retain(|c| c.id == cid);
            order = lib.case_track_ids(cid)?;
        } else {
            cases.clear();
        }
        mixes.clear();
        for e in &opts.extra {
            order.extend(e.track_ids.iter().copied());
        }
        let mut seen = HashSet::new();
        order.retain(|id| seen.insert(*id));
        let pos: HashMap<i64, usize> = order.iter().copied().enumerate().map(|(i, id)| (id, i)).collect();
        tracks.retain(|t| pos.contains_key(&t.track_id));
        tracks.sort_by_key(|t| pos[&t.track_id]);
    }
    cases.retain(|c| !opts.skip_cases.contains(&c.id));
    let extras = lib.interop_extras()?;
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\">\n");
    s.push_str("  <PRODUCT Name=\"Pacemaker Revival\" Version=\"0.1.0\" Company=\"\"/>\n");
    s.push_str(&format!("  <COLLECTION Entries=\"{}\">\n", tracks.len()));
    for t in &tracks {
        let e = extras.get(&t.track_id);
        let kind = if t.format.is_empty() { String::new() } else { format!("{} File", t.format.to_uppercase()) };
        s.push_str(&format!(
            "    <TRACK TrackID=\"{}\" Name=\"{}\" Artist=\"{}\" Composer=\"{}\" Album=\"{}\" Genre=\"{}\" Kind=\"{}\" Size=\"{}\" TotalTime=\"{}\" TrackNumber=\"{}\" Year=\"{}\" AverageBpm=\"{:.2}\" DateAdded=\"{}\" BitRate=\"{}\" SampleRate=\"{}\" Comments=\"{}\" PlayCount=\"{}\" Rating=\"{}\" Location=\"{}\" Label=\"{}\" Tonality=\"{}\">\n",
            t.track_id,
            xml_escape(&t.title),
            xml_escape(&t.artist),
            xml_escape(e.map_or("", |e| e.composer.as_str())),
            xml_escape(&t.album),
            xml_escape(&t.genre),
            xml_escape(&kind),
            t.file_size,
            t.play_time_secs,
            e.map_or(0, |e| e.track_number),
            e.map_or(0, |e| e.year),
            t.bpm,
            e.map_or_else(String::new, |e| if e.date_added > 0 { ymd(e.date_added) } else { String::new() }),
            e.map_or(0, |e| e.bit_rate),
            e.map_or(0, |e| e.sample_rate),
            xml_escape(e.map_or("", |e| e.comments.as_str())),
            t.times_played,
            t.rating.clamp(0, 5) * 51,
            xml_escape(&file_url(&audio_path_in(lib, t, &opts.paths))),
            xml_escape(e.map_or("", |e| e.label.as_str())),
            xml_escape(&t.key),
        ));
        let sr = e.map_or(0, |e| e.sample_rate);
        if sr > 0 {
            if let Some((_, sidecar)) = lib.track_paths(&t.location) {
                if let Ok(a) = std::fs::read_to_string(&sidecar).map_err(|_| ()).and_then(|x| strfile::decode(&x).map_err(|_| ())) {
                    s.push_str(&tempo_elements(&a, sr as f64));
                }
            }
        }
        s.push_str("    </TRACK>\n");
    }
    s.push_str("  </COLLECTION>\n  <PLAYLISTS>\n");

    let known: HashSet<i64> = tracks.iter().map(|t| t.track_id).collect();
    let playlist = |s: &mut String, indent: &str, name: &str, ids: Vec<i64>| {
        let ids: Vec<i64> = ids.into_iter().filter(|i| known.contains(i)).collect();
        s.push_str(&format!("{indent}<NODE Name=\"{}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">\n", xml_escape(name), ids.len()));
        for id in ids {
            s.push_str(&format!("{indent}  <TRACK Key=\"{id}\"/>\n"));
        }
        s.push_str(&format!("{indent}</NODE>\n"));
    };
    let extra_nodes = match (&opts.extra_folder, opts.extra.is_empty()) {
        (_, true) => 0,
        (Some(_), false) => 1,
        (None, false) => opts.extra.len(),
    };
    let root_count = cases.len() + extra_nodes + usize::from(!mixes.is_empty());
    s.push_str(&format!("    <NODE Type=\"0\" Name=\"ROOT\" Count=\"{root_count}\">\n"));
    for c in &cases {
        playlist(&mut s, "      ", &c.name, lib.case_track_ids(c.id)?);
    }
    match &opts.extra_folder {
        Some(folder) if !opts.extra.is_empty() => {
            s.push_str(&format!("      <NODE Type=\"0\" Name=\"{}\" Count=\"{}\">
", xml_escape(folder), opts.extra.len()));
            for e in &opts.extra {
                playlist(&mut s, "        ", &e.name, e.track_ids.clone());
            }
            s.push_str("      </NODE>
");
        }
        _ => {
            for e in &opts.extra {
                playlist(&mut s, "      ", &e.name, e.track_ids.clone());
            }
        }
    }
    if !mixes.is_empty() {
        s.push_str(&format!("      <NODE Type=\"0\" Name=\"Mixes\" Count=\"{}\">\n", mixes.len()));
        for m in &mixes {
            let ids = lib.mix_track_rows(m.id)?.into_iter().map(|r| r.track_id).collect();
            playlist(&mut s, "        ", &m.name, ids);
        }
        s.push_str("      </NODE>\n");
    }
    s.push_str("    </NODE>\n  </PLAYLISTS>\n</DJ_PLAYLISTS>\n");
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// Tests (synthetic documents and temp-dir libraries only)
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-interop-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A library of synthetic tracks "Synth Title N" by "Synth Artist" (file `synth N.mp3`,
    /// size 2000 + N), returning it with the temp dirs and the file name / size of each source.
    fn library(tag: &str, n: usize) -> (Library, Vec<PathBuf>, Vec<(String, u64)>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src = temp_dir(&format!("{tag}-src"));
        let mut lib = Library::create(&vol).unwrap();
        let mut files = Vec::new();
        for i in 0..n {
            let name = format!("synth {i}.mp3");
            let p = src.join(&name);
            std::fs::write(&p, vec![0u8; 2000 + i]).unwrap();
            let a = strfile::Analysis {
                normalize_factor: 1.0,
                beat_start_offset: 0,
                beats: (0..16).map(|b| strfile::Beat { pos: 22050 + b * 22050, bpm: if b < 8 { 120.0 } else { 122.0 } }).collect(),
                first_downbeat: Some(1),
            };
            let meta = TrackMeta {
                title: format!("Synth Title {i}"),
                artist: "Synth Artist".into(),
                genre: "House".into(),
                key: "Am".into(),
                format: "MP3".into(),
                sample_rate: 44100,
                play_time_secs: 200 + i as i64,
                ..Default::default()
            };
            lib.add_track(&p, &meta, &a).unwrap();
            files.push((name, 2000 + i as u64));
        }
        (lib, vec![vol, src], files)
    }

    fn clean(lib: Library, dirs: Vec<PathBuf>) {
        drop(lib);
        for d in dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    const REKORDBOX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <PRODUCT Name="rekordbox" Version="6.0.0" Company="X"/>
  <COLLECTION Entries="4">
    <TRACK TrackID="11" Name="Renamed &amp; Different" Artist="Other" Size="2000" TotalTime="200" AverageBpm="120.00" Rating="153" Tonality="8A" Location="file://localhost/C:/Somewhere/synth%200.mp3">
      <TEMPO Inizio="0.500" Bpm="120.00" Metro="4/4" Battito="1"/>
      <POSITION_MARK Name="" Type="0" Start="31.5" Num="-1"/>
      <POSITION_MARK Name="" Type="0" Start="12.25" Num="-1"/>
      <POSITION_MARK Name="" Type="0" Start="3.0" Num="0"/>
    </TRACK>
    <TRACK TrackID="12" Name="Synth Title 1" Artist="Synth Artist" TotalTime="201" Rating="255" Location="file://localhost/D:/Elsewhere/nothing%20like%20it.mp3"/>
    <TRACK TrackID="13" Name="Gone Track" Artist="Nobody" Size="9999" Rating="51" Location="file://localhost/D:/Nowhere/gone.mp3"/>
    <TRACK TrackID="14" Name="Unrated" Artist="Synth Artist" Size="2002" Rating="0" Location="file://localhost/D:/x/synth%202.mp3"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT" Count="2">
      <NODE Name="Warm up" Type="1" KeyType="0" Entries="3"><TRACK Key="11"/><TRACK Key="12"/><TRACK Key="13"/></NODE>
      <NODE Type="0" Name="Folder" Count="1">
        <NODE Name="Inner" Type="1" KeyType="1" Entries="2"><TRACK Key="file://localhost/D:/x/synth%202.mp3"/><TRACK Key="file://localhost/D:/missing.mp3"/></NODE>
      </NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    const ITUNES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Major Version</key><integer>1</integer>
  <key>Tracks</key>
  <dict>
    <key>101</key>
    <dict>
      <key>Track ID</key><integer>101</integer>
      <key>Name</key><string>Synth Title 0</string>
      <key>Artist</key><string>synth  artist</string>
      <key>Total Time</key><integer>200500</integer>
      <key>Rating</key><integer>80</integer>
      <key>Date Added</key><date>2020-01-01T00:00:00Z</date>
      <key>Location</key><string>file://localhost/C:/Music/some%20where/other%20name.mp3</string>
    </dict>
    <key>102</key>
    <dict>
      <key>Track ID</key><integer>102</integer>
      <key>Name</key><string>Fish &amp; Chips</string>
      <key>Size</key><integer>2001</integer>
      <key>Rating</key><integer>100</integer>
      <key>Location</key><string>file://localhost/C:/Music/synth%201.mp3</string>
    </dict>
    <key>103</key>
    <dict>
      <key>Track ID</key><integer>103</integer>
      <key>Name</key><string>Not Here</string>
      <key>Rating</key><integer>20</integer>
      <key>Disabled</key><true/>
    </dict>
  </dict>
  <key>Playlists</key>
  <array>
    <dict><key>Name</key><string>Library</string><key>Master</key><true/><key>Playlist Persistent ID</key><string>AA</string>
      <key>Playlist Items</key><array><dict><key>Track ID</key><integer>101</integer></dict></array></dict>
    <dict><key>Name</key><string>Music</string><key>Distinguished Kind</key><integer>4</integer><key>Playlist Persistent ID</key><string>AB</string>
      <key>Playlist Items</key><array><dict><key>Track ID</key><integer>101</integer></dict></array></dict>
    <dict><key>Name</key><string>Sets</string><key>Folder</key><true/><key>Playlist Persistent ID</key><string>F1</string></dict>
    <dict><key>Name</key><string>Friday</string><key>Playlist Persistent ID</key><string>P1</string><key>Parent Persistent ID</key><string>F1</string>
      <key>Playlist Items</key><array>
        <dict><key>Track ID</key><integer>102</integer></dict>
        <dict><key>Track ID</key><integer>101</integer></dict>
        <dict><key>Track ID</key><integer>103</integer></dict>
        <dict><key>Track ID</key><integer>999</integer></dict>
      </array></dict>
  </array>
</dict>
</plist>"#;

    #[test]
    fn entity_decoding_is_linear_on_a_value_of_ampersands() {
        let big = "&".repeat(2_000_000);
        let t = std::time::Instant::now();
        assert_eq!(decode_entities(&big), big, "an '&' with no ';' within reach stays as written");
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "took {:?}", t.elapsed());
        assert_eq!(decode_entities("a &amp; b &#65; &nope; &lt"), "a & b A &nope; &lt");
        // an entity just inside and just outside the 12-byte reach
        assert_eq!(decode_entities("&#x1F600;"), "\u{1F600}");
        assert_eq!(decode_entities("&#0000000065;"), "A", "the ; is the 13th byte: in reach");
        assert_eq!(decode_entities("&#00000000065;"), "&#00000000065;", "one byte further: not an entity");
    }

    #[test]
    fn xml_reader_handles_entities_cdata_comments_and_doctype() {
        let mut r = Reader::new("\u{feff}<?xml version='1.0'?><!DOCTYPE a [<!ENTITY x 'y'>]><!-- c --><a k='v &lt; &#65;&#x42;'><b/><c><![CDATA[<raw>]]> t</c></a>");
        let (n, attrs) = r.next_start().unwrap().unwrap();
        assert_eq!((n.as_str(), attrs[0].1.as_str()), ("a", "v < AB"));
        assert_eq!(r.next_start().unwrap().unwrap().0, "b");
        r.skip_element().unwrap();
        assert_eq!(r.next_start().unwrap().unwrap().0, "c");
        assert_eq!(r.text_of_element().unwrap(), "<raw> t");
        assert!(r.next_start().unwrap().is_none());
        assert!(parse_library("<a><b></a>").is_err());
        assert!(parse_library("<a k=v/>").is_err());
        assert!(parse_library("").is_err());
        assert!(parse_library("<html></html>").is_err());
    }

    #[test]
    fn location_urls_become_paths() {
        assert_eq!(location_to_path("file://localhost/C:/My%20Music/%C3%A9.mp3"), "C:/My Music/é.mp3");
        assert_eq!(location_to_path("file:///home/u/a.mp3"), "/home/u/a.mp3");
        assert_eq!(location_to_path("D:\\plain\\a.mp3"), "D:\\plain\\a.mp3");
        assert_eq!(location_to_path("file://localhost/a%zz%"), "/a%zz%");
    }

    #[test]
    fn parses_rekordbox() {
        let f = parse_library(REKORDBOX).unwrap();
        assert_eq!(f.source, Source::Rekordbox);
        assert_eq!(f.tracks.len(), 4);
        let t = &f.tracks[0];
        assert_eq!((t.title.as_str(), t.path.as_str(), t.size, t.stars), ("Renamed & Different", "C:/Somewhere/synth 0.mp3", Some(2000), Some(3)));
        assert_eq!(t.cue_secs, Some(12.25), "the first memory cue, not the hot cue or the later one");
        assert_eq!(f.tracks[1].stars, Some(5));
        assert_eq!(f.tracks[3].stars, None, "rating 0 is unrated");
        assert_eq!(f.playlists.len(), 2);
        assert_eq!((f.playlists[0].name.as_str(), f.playlists[0].tracks.clone()), ("Warm up", vec![0, 1, 2]));
        assert_eq!(f.playlists[1].name, "Folder / Inner");
        assert_eq!((f.playlists[1].tracks.clone(), f.playlists[1].dangling), (vec![3], 1));
    }

    #[test]
    fn parses_itunes() {
        let f = parse_library(ITUNES).unwrap();
        assert_eq!(f.source, Source::Itunes);
        assert_eq!(f.tracks.len(), 3);
        assert_eq!(f.tracks[0].stars, Some(4), "80 of 100 is 4 stars");
        assert_eq!(f.tracks[1].stars, Some(5));
        assert_eq!(f.tracks[2].stars, Some(1));
        assert_eq!(f.tracks[1].title, "Fish & Chips");
        assert_eq!(f.tracks[0].duration_secs, Some(200.5));
        assert_eq!(f.playlists.len(), 1, "Library, Music and the folder itself are not playlists");
        assert_eq!(f.playlists[0].name, "Sets / Friday");
        assert_eq!((f.playlists[0].tracks.clone(), f.playlists[0].dangling), (vec![1, 0, 2], 1));
    }

    #[test]
    fn rekordbox_plan_and_apply() {
        let (mut lib, dirs, _) = library("rb", 4);
        let f = parse_library(REKORDBOX).unwrap();
        let opts = ImportOptions::default();
        let plan = plan_import(&lib, &f, &opts).unwrap();

        // track 0 by file name + size (tags differ), track 1 by tags (no size, different file),
        // track 3 by file name + size
        assert_eq!((plan.matched, plan.matched_by_file, plan.matched_by_tags), (3, 2, 1));
        assert_eq!(plan.unmatched_total, 1);
        assert_eq!(plan.unmatched[0].title, "Gone Track");
        assert!(!plan.unmatched[0].on_disk);
        let r: Vec<(String, i64, u8)> = plan.ratings.iter().map(|r| (r.title.clone(), r.from, r.to)).collect();
        assert_eq!(r, vec![("Synth Title 0".to_string(), 0, 3), ("Synth Title 1".to_string(), 0, 5)]);
        assert_eq!(plan.cues.len(), 1);
        assert_eq!(plan.cues[0].cue_secs, 12.25);
        let names: Vec<(&str, CaseAction, usize, usize)> = plan.cases.iter().map(|c| (c.name.as_str(), c.action, c.tracks.len(), c.unmatched)).collect();
        assert_eq!(names, vec![("Warm up", CaseAction::Create, 2, 1), ("Folder / Inner", CaseAction::Create, 1, 1)]);

        // planning wrote nothing
        assert!(lib.cases().unwrap().is_empty());
        assert!(lib.tracks().unwrap().iter().all(|t| t.rating == 0));

        let mut steps = 0;
        let rep = apply_plan(&mut lib, &plan, &opts, |s| {
            steps += 1;
            assert!(s.error.is_none());
        })
        .unwrap();
        assert_eq!((rep.ratings_set, rep.cases_created, rep.cases_extended, rep.tracks_added_to_cases), (2, 2, 0, 3));
        assert_eq!(steps, 3);
        assert!(rep.failures.is_empty());
        assert!(lib.compatibility_problems().unwrap().is_empty());
        let tracks = lib.tracks().unwrap();
        assert_eq!(tracks[0].rating, 3);
        assert_eq!(tracks[1].rating, 5);
        assert_eq!(tracks[3].rating, 0);
        let cases = lib.cases().unwrap();
        let warm = cases.iter().find(|c| c.name == "Warm up").unwrap();
        assert_eq!(lib.case_track_ids(warm.id).unwrap(), vec![tracks[0].track_id, tracks[1].track_id]);
        assert_eq!(warm.play_time_secs, tracks[0].play_time_secs + tracks[1].play_time_secs);

        // importing the same file again changes nothing
        let again = plan_import(&lib, &f, &opts).unwrap();
        assert!(again.ratings.is_empty());
        assert_eq!(again.ratings_unchanged, 2);
        assert!(again.cases.iter().all(|c| c.action == CaseAction::Unchanged));
        clean(lib, dirs);
    }

    #[test]
    fn existing_case_is_extended_never_shrunk_and_options_are_honoured() {
        let (mut lib, dirs, _) = library("ext", 3);
        let t = lib.tracks().unwrap();
        let cid = lib.create_case("Sets / Friday").unwrap();
        lib.set_case_tracks(cid, &[t[2].track_id, t[1].track_id]).unwrap();
        let f = parse_library(ITUNES).unwrap();

        let opts = ImportOptions { ratings: false, playlists: true, playlist_indexes: None, ..Default::default() };
        let plan = plan_import(&lib, &f, &opts).unwrap();
        assert!(plan.ratings.is_empty(), "ratings switched off");
        assert_eq!(plan.matched, 2);
        let c = &plan.cases[0];
        assert_eq!((c.action, c.case_id, c.already_in), (CaseAction::Extend, Some(cid), 1));
        assert_eq!(c.tracks.len(), 1);
        assert_eq!(c.tracks[0].track_id, t[0].track_id);

        let rep = apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap();
        assert_eq!((rep.ratings_set, rep.cases_created, rep.cases_extended), (0, 0, 1));
        assert_eq!(lib.case_track_ids(cid).unwrap(), vec![t[2].track_id, t[1].track_id, t[0].track_id]);
        assert!(lib.tracks().unwrap().iter().all(|t| t.rating == 0));

        // deselecting the playlist applies nothing
        let (mut lib2, dirs2, _) = library("ext2", 3);
        let opts = ImportOptions { ratings: true, playlists: true, playlist_indexes: Some(vec![]), ..Default::default() };
        let plan = plan_import(&lib2, &f, &opts).unwrap();
        let rep = apply_plan(&mut lib2, &plan, &opts, |_| {}).unwrap();
        assert_eq!((rep.ratings_set, rep.cases_created), (2, 0));
        assert!(lib2.cases().unwrap().is_empty());
        clean(lib, dirs);
        clean(lib2, dirs2);
    }

    #[test]
    fn itunes_plan() {
        let (mut lib, dirs, _) = library("it", 3);
        let f = parse_library(ITUNES).unwrap();
        let opts = ImportOptions::default();
        let plan = plan_import(&lib, &f, &opts).unwrap();
        // 101 by tags (case/space-insensitive artist, 200.5 s vs 200 s), 102 by file name + size
        assert_eq!((plan.matched_by_file, plan.matched_by_tags), (1, 1));
        let r: Vec<u8> = plan.ratings.iter().map(|r| r.to).collect();
        assert_eq!(r, vec![4, 5]);
        assert_eq!(plan.cases[0].name, "Sets / Friday");
        let order: Vec<&str> = plan.cases[0].tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(order, vec!["Synth Title 1", "Synth Title 0"], "playlist order is kept");
        assert_eq!(plan.cases[0].unmatched, 2, "one unmatched track and one dangling entry");
        apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap();
        assert!(lib.compatibility_problems().unwrap().is_empty());
        clean(lib, dirs);
    }

    #[test]
    fn tag_matching_ignores_case_and_rejects_different_lengths() {
        let (lib, dirs, _) = library("amb", 2);
        let f = ForeignLibrary {
            source: Source::Itunes,
            tracks: vec![ForeignTrack { title: "synth title 0".into(), artist: "SYNTH ARTIST".into(), duration_secs: Some(200.0), ..Default::default() }],
            playlists: vec![],
            notes: vec![],
        };
        let plan = plan_import(&lib, &f, &ImportOptions::default()).unwrap();
        assert_eq!(plan.matched, 1);
        let mut nolen = f.clone();
        nolen.tracks[0].duration_secs = None;
        assert_eq!(plan_import(&lib, &nolen, &ImportOptions::default()).unwrap().matched, 0, "a title match needs a length on both sides");
        let f2 = ForeignLibrary {
            tracks: vec![ForeignTrack { title: "Synth Title 0".into(), artist: "Synth Artist".into(), duration_secs: Some(900.0), ..Default::default() }],
            ..f
        };
        let plan = plan_import(&lib, &f2, &ImportOptions::default()).unwrap();
        assert_eq!(plan.matched, 0, "a very different length is a different recording");
        clean(lib, dirs);
    }

    #[test]
    fn unmatched_file_on_disk_is_offered_for_adding() {
        let (lib, dirs, _) = library("add", 1);
        let extra = dirs[1].join("brand new.mp3");
        std::fs::write(&extra, [0u8; 10]).unwrap();
        let f = ForeignLibrary {
            source: Source::Rekordbox,
            tracks: vec![ForeignTrack { title: "Brand New".into(), path: extra.display().to_string(), ..Default::default() }],
            playlists: vec![ForeignPlaylist { name: "P".into(), tracks: vec![0], dangling: 0 }],
            notes: vec![],
        };
        let plan = plan_import(&lib, &f, &ImportOptions::default()).unwrap();
        assert_eq!(plan.addable_paths, vec![extra.display().to_string()]);
        assert!(plan.unmatched[0].on_disk && plan.unmatched[0].in_playlist);
        assert_eq!(plan.cases[0].action, CaseAction::Skip);
        clean(lib, dirs);
    }

    #[test]
    fn rating_batch_rolls_back_on_error() {
        let (mut lib, dirs, _) = library("rb2", 2);
        let t = lib.tracks().unwrap();
        assert!(lib.set_track_ratings(&[(t[0].track_id, 4), (9999, 3)]).is_err());
        assert_eq!(lib.tracks().unwrap()[0].rating, 0, "the first change was rolled back");
        assert_eq!(lib.set_track_ratings(&[(t[0].track_id, 4), (t[1].track_id, 2)]).unwrap(), 2);
        clean(lib, dirs);
    }

    #[test]
    fn rekordbox_export_round_trips() {
        let (mut lib, dirs, _) = library("exp", 3);
        let t = lib.tracks().unwrap();
        lib.set_track_rating(t[0].track_id, 4).unwrap();
        lib.set_track_rating(t[2].track_id, 1).unwrap();
        let cid = lib.create_case("Fish & \"Chips\" <1>").unwrap();
        lib.set_case_tracks(cid, &[t[2].track_id, t[0].track_id]).unwrap();
        let mid = lib.create_mix("A mix").unwrap();
        let _ = mid;

        let xml = export_rekordbox(&lib).unwrap();
        assert!(xml.contains("<TEMPO Inizio=\"0.500\" Bpm=\"120.00\" Metro=\"4/4\" Battito=\"4\"/>"), "first beat is the bar's 4th: the downbeat is beat 1\n{xml}");
        assert!(xml.contains("Bpm=\"122.00\""), "tempo change gets its own TEMPO");
        assert_eq!(xml.matches("<TEMPO").count(), 3 * 2);
        assert!(xml.contains("Rating=\"204\"") && xml.contains("Rating=\"51\""));
        assert!(xml.contains("Tonality=\"Am\""));

        let back = parse_library(&xml).unwrap();
        assert_eq!(back.source, Source::Rekordbox);
        assert_eq!(back.tracks.len(), 3);
        assert_eq!(back.tracks[0].title, "Synth Title 0");
        assert_eq!(back.tracks[0].stars, Some(4));
        assert_eq!(back.tracks[1].stars, None);
        assert_eq!(back.tracks[0].size, Some(2000));
        assert_eq!(back.tracks[0].key, "Am");
        assert_eq!(back.playlists.len(), 2, "the case and the mix");
        assert_eq!(back.playlists[0].name, "Fish & \"Chips\" <1>");
        assert_eq!(back.playlists[0].tracks, vec![2, 0]);
        assert_eq!(back.playlists[1].name, "Mixes / A mix");

        // and the device accepts its own export back: same ratings, same case, nothing to do
        let plan = plan_import(&lib, &back, &ImportOptions::default()).unwrap();
        assert_eq!((plan.matched, plan.matched_by_file), (3, 3), "files inside the library's own folder map straight to their rows");
        assert!(plan.ratings.is_empty());
        assert_eq!(plan.cases[0].action, CaseAction::Unchanged);
        clean(lib, dirs);
    }

    #[test]
    fn rekordbox_export_of_one_case_holds_only_that_case() {
        let (mut lib, dirs, _) = library("expcase", 4);
        let t = lib.tracks().unwrap();
        let wanted = lib.create_case("Wanted").unwrap();
        let other = lib.create_case("Other").unwrap();
        lib.set_case_tracks(wanted, &[t[3].track_id, t[1].track_id]).unwrap();
        lib.set_case_tracks(other, &[t[0].track_id]).unwrap();
        lib.create_mix("A mix").unwrap();

        let xml = export_rekordbox_case(&lib, wanted).unwrap();
        let back = parse_library(&xml).unwrap();
        assert_eq!(back.tracks.len(), 2, "only the case's tracks");
        assert_eq!(back.tracks[0].title, "Synth Title 3", "in case order");
        assert_eq!(back.tracks[1].title, "Synth Title 1");
        assert_eq!(back.playlists.len(), 1, "one playlist, no other case, no mixes");
        assert_eq!(back.playlists[0].name, "Wanted");
        assert_eq!(back.playlists[0].tracks, vec![0, 1]);
        assert_eq!(xml.matches("<TEMPO").count(), 2 * 2, "tempo maps of the two tracks");
        assert!(export_rekordbox_case(&lib, 9999).is_err());
        // the whole-library export is unchanged by the refactor
        let all = parse_library(&export_rekordbox(&lib).unwrap()).unwrap();
        assert_eq!((all.tracks.len(), all.playlists.len()), (4, 3));
        clean(lib, dirs);
    }

    #[test]
    fn rekordbox_export_with_extra_playlists_and_skipped_cases() {
        let (mut lib, dirs, _) = library("expextra", 4);
        let t = lib.tracks().unwrap();
        let id = |n: usize| t[n].track_id;
        let plain = lib.create_case("Plain").unwrap();
        let copy = lib.create_case("Copy of a rule").unwrap();
        lib.set_case_tracks(plain, &[id(0)]).unwrap();
        lib.set_case_tracks(copy, &[id(1)]).unwrap();
        lib.create_mix("A mix").unwrap();
        let extra = vec![
            ExtraPlaylist { name: "Rule one".into(), track_ids: vec![id(3), id(1), 9999] },
            ExtraPlaylist { name: "Rule \"two\" & more".into(), track_ids: vec![] },
        ];

        // whole library, extras beside the cases; the skipped case is not a playlist but its track stays
        let opts = XmlOptions { skip_cases: HashSet::from([copy]), extra: extra.clone(), ..Default::default() };
        let xml = export_rekordbox_with(&lib, &opts).unwrap();
        assert!(xml.contains("Name=\"ROOT\" Count=\"4\""), "Plain + two extras + Mixes folder
{xml}");
        let back = parse_library(&xml).unwrap();
        assert_eq!(back.tracks.len(), 4);
        let names: Vec<&str> = back.playlists.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Plain", "Rule one", "Rule \"two\" & more", "Mixes / A mix"]);
        assert_eq!(back.playlists[1].tracks, vec![3, 1], "order kept, the unknown id left out");
        assert!(back.playlists[2].tracks.is_empty());

        // in a folder: the folder is one root child and the parser reads "Folder / name"
        let opts = XmlOptions { extra: extra.clone(), extra_folder: Some("Smart cases".into()), ..Default::default() };
        let xml = export_rekordbox_with(&lib, &opts).unwrap();
        assert!(xml.contains("Name=\"ROOT\" Count=\"4\""), "Plain, Copy of a rule, Smart cases folder, Mixes folder");
        assert!(xml.contains("<NODE Type=\"0\" Name=\"Smart cases\" Count=\"2\">"));
        let names: Vec<String> = parse_library(&xml).unwrap().playlists.into_iter().map(|p| p.name).collect();
        assert!(names.contains(&"Smart cases / Rule one".to_string()), "{names:?}");

        // extras only: no cases, no mixes, the collection is the union of the extras' tracks in order
        let opts = XmlOptions { extras_only: true, extra: extra.clone(), ..Default::default() };
        let back = parse_library(&export_rekordbox_with(&lib, &opts).unwrap()).unwrap();
        assert_eq!(back.tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Synth Title 3", "Synth Title 1"]);
        assert_eq!(back.playlists.len(), 2);

        // excluded tracks are in neither the collection nor any playlist (cases, extras, mixes)
        let opts = XmlOptions { exclude_tracks: HashSet::from([id(1)]), extra: extra.clone(), ..Default::default() };
        let xml = export_rekordbox_with(&lib, &opts).unwrap();
        let back = parse_library(&xml).unwrap();
        assert_eq!(back.tracks.len(), 3);
        assert!(back.tracks.iter().all(|t| t.title != "Synth Title 1"), "{xml}");
        assert_eq!(back.playlists.iter().find(|p| p.name == "Rule one").unwrap().tracks.len(), 1, "the other id of the rule is kept");
        assert_eq!(back.playlists.iter().find(|p| p.name == "Copy of a rule").unwrap().tracks.len(), 0);

        // nothing extra and no options is exactly the plain export
        assert_eq!(export_rekordbox_with(&lib, &XmlOptions::default()).unwrap(), export_rekordbox(&lib).unwrap());
        // one case plus an extra: the case's tracks first, then the extra's
        let opts = XmlOptions { only_case: Some(plain), extra, ..Default::default() };
        let back = parse_library(&export_rekordbox_with(&lib, &opts).unwrap()).unwrap();
        assert_eq!(back.tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Synth Title 0", "Synth Title 3", "Synth Title 1"]);
        assert_eq!(back.playlists.len(), 3);
        clean(lib, dirs);
    }

    #[test]
    fn m3u8_and_csv_exports() {
        let (mut lib, dirs, _) = library("lists", 3);
        let t = lib.tracks().unwrap();
        let cid = lib.create_case("Order").unwrap();
        lib.set_case_tracks(cid, &[t[1].track_id, t[0].track_id]).unwrap();
        let (name, tracks) = scope_tracks(&lib, Scope::Case(cid)).unwrap();
        assert_eq!((name.as_str(), tracks.len()), ("Order", 2));
        let m3u = export_m3u8(&lib, &tracks);
        let lines: Vec<&str> = m3u.lines().collect();
        assert_eq!(lines[0], "#EXTM3U");
        assert_eq!(lines[1], "#EXTINF:201,Synth Artist - Synth Title 1");
        assert!(lines[2].contains(".Pacemaker") && Path::new(lines[2]).is_file(), "{}", lines[2]);
        assert_eq!(lines.len(), 5);

        let (_, all) = scope_tracks(&lib, Scope::Library).unwrap();
        assert_eq!(all.len(), 3);
        let mut odd = all[0].clone();
        odd.title = "Comma, \"quote\"\nline".into();
        let csv = export_csv(&lib, &[odd]);
        assert!(csv.starts_with("\u{feff}Title,Artist,Album,Genre,BPM,Key,Rating,Duration (s),Format,File\r\n"));
        assert!(csv.contains("\"Comma, \"\"quote\"\"\nline\",Synth Artist,"));
        assert!(csv.contains(",122.00,Am,0,200,MP3,"), "{csv}");

        let mid = lib.create_mix("Empty mix").unwrap();
        assert_eq!(scope_tracks(&lib, Scope::Mix(mid)).unwrap().1.len(), 0);
        assert!(scope_tracks(&lib, Scope::Case(9999)).is_err());
        clean(lib, dirs);
    }

    #[test]
    fn a_path_map_replaces_the_audio_path_in_every_list_format() {
        let (lib, dirs, _) = library("pathmap", 2);
        let tracks = lib.tracks().unwrap();
        let mut paths = PathMap::new();
        paths.insert(tracks[0].track_id, "Audio/copy one.mp3".to_string());
        // a track that is not in the map keeps its path on the volume
        let m3u = export_m3u8_with(&lib, &tracks, &paths);
        let lines: Vec<&str> = m3u.lines().collect();
        assert_eq!(lines[2], "Audio/copy one.mp3");
        assert!(lines[4].contains(".Pacemaker"), "{}", lines[4]);
        let csv = export_csv_with(&lib, &tracks, &paths);
        assert!(csv.contains(",Audio/copy one.mp3\r\n"), "{csv}");
        let xml = export_rekordbox_with(&lib, &XmlOptions { paths, ..Default::default() }).unwrap();
        assert!(xml.contains("Location=\"file://localhost/Audio/copy%20one.mp3\""), "{xml}");
        // the plain writers are the same as writers with an empty map
        assert_eq!(export_m3u8(&lib, &tracks), export_m3u8_with(&lib, &tracks, &PathMap::new()));
        clean(lib, dirs);
    }

    #[test]
    fn same_named_playlists_import_once_each_and_reimport_cleanly() {
        let (mut lib, dirs, _) = library("dupes", 3);
        let xml = r#"<?xml version="1.0"?><DJ_PLAYLISTS Version="1.0.0"><COLLECTION Entries="3">
<TRACK TrackID="1" Name="a" Size="2000" Location="file://localhost/D:/x/synth%200.mp3"/>
<TRACK TrackID="2" Name="b" Size="2001" Location="file://localhost/D:/x/synth%201.mp3"/>
<TRACK TrackID="3" Name="c" Size="2002" Location="file://localhost/D:/x/synth%202.mp3"/>
</COLLECTION><PLAYLISTS><NODE Type="0" Name="ROOT" Count="3">
<NODE Name="Set" Type="1" KeyType="0" Entries="2"><TRACK Key="1"/><TRACK Key="2"/></NODE>
<NODE Name="Set" Type="1" KeyType="0" Entries="1"><TRACK Key="3"/></NODE>
<NODE Name="  Pad " Type="1" KeyType="0" Entries="1"><TRACK Key="1"/></NODE>
</NODE></PLAYLISTS></DJ_PLAYLISTS>"#;
        let foreign = parse_library(xml).unwrap();
        let opts = ImportOptions::default();
        let plan = plan_import(&lib, &foreign, &opts).unwrap();
        assert!(plan.cases.iter().all(|c| c.action == CaseAction::Create));
        let rep = apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap();
        assert_eq!((rep.cases_created, rep.failures.len()), (3, 0));
        let again = plan_import(&lib, &foreign, &opts).unwrap();
        assert!(again.cases.iter().all(|c| c.action == CaseAction::Unchanged), "{:?}", again.cases.iter().map(|c| c.action).collect::<Vec<_>>());
        assert_ne!(again.cases[0].case_id, again.cases[1].case_id);
        clean(lib, dirs);
    }

    // ---- the new sources, end to end -----------------------------------------------------

    #[test]
    fn network_device_and_url_paths_are_not_probed_and_say_so() {
        assert_eq!(location_to_path("file:////host/share/x.mp3"), "//host/share/x.mp3");
        assert_eq!(location_to_path("file://host/share/x.mp3"), "//host/share/x.mp3", "a host is not a relative folder");
        let (lib, dirs, _) = library("unsafe", 1);
        let f = ForeignLibrary {
            source: Source::M3u,
            tracks: [
                "\\\\invalid-host-for-test\\share\\a.mp3",
                "//invalid-host-for-test/share/b.mp3",
                "\\\\.\\pipe\\c",
                "smb://invalid-host-for-test/d.mp3",
                "rel/e.mp3",
            ]
            .iter()
            .map(|p| ForeignTrack { title: format!("T {p}"), path: (*p).into(), ..Default::default() })
            .collect(),
            playlists: vec![],
            notes: vec![],
        };
        let plan = plan_import(&lib, &f, &ImportOptions::default()).unwrap();
        assert_eq!(plan.matched, 0);
        assert!(plan.addable_paths.is_empty(), "nothing from a refused location is offered to the add job");
        assert!(
            plan.unmatched.iter().all(|u| !u.on_disk && u.reason.contains("not checked")),
            "{:?}",
            plan.unmatched.iter().map(|u| &u.reason).collect::<Vec<_>>()
        );
        clean(lib, dirs);
    }

    #[test]
    fn m3u_playlist_matches_by_file_and_becomes_a_case() {
        let (mut lib, dirs, _) = library("m3u", 3);
        // relative entries next to the audio, a missing file, and a stream
        let list = dirs[1].join("Friday.m3u8");
        std::fs::write(&list, "#EXTM3U\n#EXTINF:200,Whatever - Else\nsynth 2.mp3\nsynth 0.mp3\nmissing.mp3\nhttp://example.invalid/s\n").unwrap();
        let f = read_path(&list).unwrap();
        assert_eq!(f.source, Source::M3u);
        let opts = ImportOptions::default();
        let plan = plan_import(&lib, &f, &opts).unwrap();
        assert_eq!((plan.matched, plan.matched_by_file), (2, 2));
        assert_eq!(plan.cases.len(), 1);
        let c = &plan.cases[0];
        assert_eq!((c.name.as_str(), c.action, c.tracks.len(), c.unmatched), ("Friday", CaseAction::Create, 2, 2));
        assert_eq!(c.tracks[0].title, "Synth Title 2", "playlist order is kept");
        let rep = apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap();
        assert_eq!((rep.cases_created, rep.failures.len()), (1, 0));
        clean(lib, dirs);
    }

    fn crate_file(paths: &[&str]) -> Vec<u8> {
        let rec = |tag: &[u8; 4], d: &[u8]| {
            let mut v = tag.to_vec();
            v.extend((d.len() as u32).to_be_bytes());
            v.extend(d);
            v
        };
        let u16be = |s: &str| s.encode_utf16().flat_map(|c| c.to_be_bytes()).collect::<Vec<u8>>();
        let mut v = rec(b"vrsn", &u16be("1.0/Serato ScratchLive Crate"));
        for p in paths {
            v.extend(rec(b"otrk", &rec(b"ptrk", &u16be(p))));
        }
        v
    }

    #[test]
    fn serato_crates_match_by_file_through_the_serato_folder() {
        let (lib, dirs, _) = library("serato", 3);
        // `_Serato_` sits in the folder that holds the music, so the stored paths are relative to it
        let sub = dirs[1].join("_Serato_").join("Subcrates");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("Sets%%Friday.crate"), crate_file(&["synth 1.mp3", "synth 0.mp3", "nope.mp3"])).unwrap();
        let f = read_path(&dirs[1]).unwrap();
        assert_eq!(f.source, Source::Serato);
        let plan = plan_import(&lib, &f, &ImportOptions::default()).unwrap();
        assert_eq!((plan.matched, plan.matched_by_file, plan.unmatched_total), (2, 2, 1));
        assert_eq!((plan.cases[0].name.as_str(), plan.cases[0].tracks.len()), ("Sets / Friday", 2));
        assert!(plan.ratings.is_empty() && plan.cues.is_empty(), "a crate carries paths only");
        clean(lib, dirs);
    }

    const NML: &str = r#"<NML VERSION="19"><COLLECTION ENTRIES="2">
<ENTRY TITLE="Synth Title 0" ARTIST="Synth Artist"><LOCATION DIR="/:gone/:" FILE="x0.mp3" VOLUME="C:"/><INFO RANKING="204" PLAYTIME="200"/><CUE_V2 TYPE="0" START="1500" HOTCUE="0"/></ENTRY>
<ENTRY TITLE="Synth Title 1" ARTIST="Synth Artist"><LOCATION DIR="/:gone/:" FILE="x1.mp3" VOLUME="C:"/><INFO RANKING="51" PLAYTIME="201"/><CUE_V2 TYPE="0" START="2500" HOTCUE="0"/></ENTRY>
</COLLECTION><PLAYLISTS><NODE TYPE="FOLDER" NAME="$ROOT"><SUBNODES COUNT="1"><NODE TYPE="PLAYLIST" NAME="Peak"><PLAYLIST ENTRIES="2" TYPE="LIST">
<ENTRY><PRIMARYKEY TYPE="TRACK" KEY="C:/:gone/:x1.mp3"/></ENTRY><ENTRY><PRIMARYKEY TYPE="TRACK" KEY="C:/:gone/:x0.mp3"/></ENTRY>
</PLAYLIST></NODE></SUBNODES></NODE></PLAYLISTS></NML>"#;

    #[test]
    fn traktor_collection_ratings_playlists_and_cue_modes() {
        let (mut lib, dirs, _) = library("nml", 2);
        let f = parse_library(NML).unwrap();
        assert_eq!(f.source, Source::Traktor);
        let t = lib.tracks().unwrap();
        let id_of = |title: &str| t.iter().find(|x| x.title == title).unwrap().track_id;
        let (id0, id1) = (id_of("Synth Title 0"), id_of("Synth Title 1"));

        // default: ratings and a case, cues reported but not written
        let opts = ImportOptions::default();
        let plan = plan_import(&lib, &f, &opts).unwrap();
        assert_eq!((plan.matched, plan.matched_by_tags), (2, 2));
        assert_eq!(plan.ratings.iter().map(|r| (r.track_id, r.to)).collect::<Vec<_>>(), vec![(id0, 4), (id1, 1)]);
        assert_eq!(plan.cases[0].tracks.iter().map(|p| p.track_id).collect::<Vec<_>>(), vec![id1, id0]);
        assert_eq!(plan.cues.len(), 2);
        assert!(plan.cues.iter().all(|c| c.action == CueAction::Set && c.existing_frames.is_none()));
        assert_eq!(plan.cues[0].cue_frames, 66_150, "1.5 s at 44.1 kHz");
        apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap();
        assert_eq!((lib.track_cue(id0).unwrap(), lib.track_cue(id1).unwrap()), (None, None), "cue mode off writes no cue");

        // "empty only": sets both; a second run sees them as the same
        let opts = ImportOptions { cues: CueMode::Empty, ratings: false, playlists: false, ..Default::default() };
        let plan = plan_import(&lib, &f, &opts).unwrap();
        let rep = apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap();
        assert_eq!((rep.cues_set, rep.failures.len()), (2, 0));
        assert_eq!(lib.track_cue(id0).unwrap(), Some(66_150));
        let again = plan_import(&lib, &f, &opts).unwrap();
        assert!(again.cues.iter().all(|c| c.action == CueAction::Same));
        assert_eq!(apply_plan(&mut lib, &again, &opts, |_| {}).unwrap().cues_set, 0);

        // a different cue already there: kept by "empty only", replaced by "overwrite"
        lib.set_track_cue(id0, Some(1000)).unwrap();
        let plan = plan_import(&lib, &f, &opts).unwrap();
        let c0 = plan.cues.iter().find(|c| c.track_id == id0).unwrap();
        assert_eq!((c0.action, c0.existing_frames), (CueAction::Replace, Some(1000)));
        assert_eq!(apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap().cues_set, 0);
        assert_eq!(lib.track_cue(id0).unwrap(), Some(1000));
        let opts = ImportOptions { cues: CueMode::Overwrite, ..opts };
        let plan = plan_import(&lib, &f, &opts).unwrap();
        assert_eq!(apply_plan(&mut lib, &plan, &opts, |_| {}).unwrap().cues_set, 1);
        assert_eq!(lib.track_cue(id0).unwrap(), Some(66_150));
        clean(lib, dirs);
    }


    #[test]
    fn traktor_colours_are_reported_for_matched_tracks_and_never_written() {
        let (lib, dirs, _) = library("nml-col", 2);
        let text = NML.replace(r#"<INFO RANKING="204""#, r#"<INFO COLOR="3" RANKING="204""#);
        let f = parse_library(&text).unwrap();
        let t = lib.tracks().unwrap();
        let id0 = t.iter().find(|x| x.title == "Synth Title 0").unwrap().track_id;
        let plan = plan_import(&lib, &f, &ImportOptions { ratings: false, playlists: false, cues: CueMode::Off, playlist_indexes: None }).unwrap();
        assert_eq!(plan.colours.len(), 1, "only the entry with a colour");
        let c = &plan.colours[0];
        assert_eq!((c.track_id, c.index, c.title.as_str()), (id0, 3, "Synth Title 0"));
        // reported whatever the options say: the plan never carries them into a device write
        assert!(plan_import(&lib, &parse_library(NML).unwrap(), &ImportOptions::default()).unwrap().colours.is_empty());
        clean(lib, dirs);
    }
    #[test]
    fn cues_beyond_the_track_end_are_left_out_and_noted() {
        let (lib, dirs, _) = library("nml-end", 2);
        let f = parse_library(&NML.replace("START=\"2500\"", "START=\"9000000\"")).unwrap();
        let plan = plan_import(&lib, &f, &ImportOptions::default()).unwrap();
        assert_eq!(plan.cues.len(), 1, "the 9000 s cue on a 201 s track is dropped");
        assert_eq!(plan.cues[0].cue_frames, 66_150);
        assert_eq!(plan.cues[0].sample_rate, 44_100);
        assert!(plan.notes.iter().any(|n| n.starts_with("1 cue point lies beyond the end")), "{:?}", plan.notes);
        clean(lib, dirs);
    }

    #[test]
    fn cue_points_are_written_all_or_none() {
        let (mut lib, dirs, _) = library("cue-batch", 2);
        let ids: Vec<i64> = lib.tracks().unwrap().iter().map(|t| t.track_id).collect();
        assert!(lib.set_track_cues(&[(ids[0], 5000), (ids[1], 7000), (987_654, 1)]).is_err(), "unknown track");
        assert_eq!((lib.track_cue(ids[0]).unwrap(), lib.track_cue(ids[1]).unwrap()), (None, None), "rolled back");
        assert!(lib.set_track_cues(&[(ids[0], -1)]).is_err());
        assert_eq!(lib.track_cue(ids[0]).unwrap(), None);
        assert_eq!(lib.set_track_cues(&[(ids[0], 5000), (ids[1], 7000)]).unwrap(), 2);
        assert_eq!(lib.set_track_cues(&[(ids[0], 5000)]).unwrap(), 0, "same value: no write");
        assert_eq!(lib.track_cue(ids[1]).unwrap(), Some(7000));
        clean(lib, dirs);
    }

    #[test]
    fn cue_mode_decides_what_is_written() {
        use CueAction::*;
        for (mode, set, replace, same) in [(CueMode::Off, false, false, false), (CueMode::Empty, true, false, false), (CueMode::Overwrite, true, true, false)] {
            assert_eq!((mode.applies(Set), mode.applies(Replace), mode.applies(Same)), (set, replace, same), "{mode:?}");
        }
    }

    #[test]
    fn read_path_recognises_files_by_content_and_refuses_others() {
        let dir = temp_dir("sniff");
        let w = |n: &str, b: &[u8]| {
            let p = dir.join(n);
            std::fs::write(&p, b).unwrap();
            p
        };
        assert_eq!(read_path(&w("noext1", &crate_file(&["a.mp3"]))).unwrap().source, Source::Serato);
        assert_eq!(read_path(&w("noext2", b"#EXTM3U\n/x/a.mp3\n")).unwrap().source, Source::M3u);
        assert_eq!(read_path(&w("noext3", b"<NML></NML>")).unwrap().source, Source::Traktor);
        assert_eq!(read_path(&w("a.xml", NML.as_bytes())).unwrap().source, Source::Traktor);
        assert!(read_path(&w("noext4", b"\x00\x01\x02 binary")).is_err());
        assert!(read_path(&w("empty.m3u", b"")).is_err());
        assert!(read_path(&w("trunc.crate", &crate_file(&["a.mp3"])[..20])).is_err());
        assert!(read_path(&dir.join("does-not-exist.xml")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
