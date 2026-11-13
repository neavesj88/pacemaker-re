//! The smart-case rule engine: the rule tree (rules and nested ALL / ANY / NONE groups), the
//! order and limit, validation worded for the user, and evaluation. Pure: it reads a [`Data`]
//! snapshot of the library and writes nothing, so the live preview, the sidebar counts and the
//! device copy all run exactly this code.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use pm_formats::library::{Error, Library, MAX_RATING, Track, TrackExtra};
use serde::{Deserialize, Serialize};

use crate::commands::tags::{self, LabelSnapshot};

pub const MS_PER_DAY: i64 = 86_400_000;

/// Nesting limit of groups, so a hand-edited rule file cannot overflow the stack.
pub const MAX_DEPTH: usize = 8;
/// Most rules and groups in one smart case.
pub const MAX_NODES: usize = 200;
/// Largest "first N" a smart case may ask for.
pub const MAX_LIMIT: u32 = 100_000;

/// How a group combines its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Every item must hold (AND).
    #[default]
    All,
    /// At least one item holds (OR).
    Any,
    /// No item holds (NOT any of them).
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextField {
    Title,
    Artist,
    Album,
    Genre,
    Label,
    Comments,
    Remixer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOp {
    Contains,
    Is,
    StartsWith,
}

fn yes() -> bool {
    true
}

/// How a tag rule treats its tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagMatch {
    /// The track has at least one of the tags.
    Any,
    /// The track has every one of the tags.
    All,
    /// The track has none of the tags (a track with no tags at all matches).
    None,
}

/// The `colours` entry that stands for "no colour label".
pub const NO_COLOUR: &str = "none";

/// One condition, or a group of them. A track matches a rule when the rule holds for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rule {
    /// Rating of at least `stars` (1 to 5).
    RatingMin { stars: u8 },
    /// In the "Favourites" case (`value` true) or not in it (false).
    Favourite { value: bool },
    /// Never played.
    PlaysZero,
    /// Played at least `n` times.
    PlaysMin { n: i64 },
    /// Played between `min` and `max` times, either optional (0 is a valid bound).
    PlaysRange { min: Option<i64>, max: Option<i64> },
    /// Last played within the last `days` days.
    PlayedWithin { days: i64 },
    /// Last played more than `days` days ago: stale tracks. Tracks never played count only
    /// when `include_never` is set.
    LastPlayedBefore {
        days: i64,
        #[serde(default = "yes")]
        include_never: bool,
    },
    /// BPM between `min` and `max`, either optional. Tracks without a BPM never match.
    BpmRange { min: Option<f64>, max: Option<f64> },
    /// The track's key is exactly this one (any notation, compared as Camelot).
    Key { key: String },
    /// The track's key mixes with this one: same, +-1 on the wheel, or the relative key.
    KeyCompatible { key: String },
    /// Old form of `Text { genre, contains }`; still read, rewritten on load (see `migrate`).
    Genre { text: String },
    /// Has been analysed (`true`) or has not (`false`).
    Analysed { value: bool },
    /// A text column contains / is / starts with `text`, ignoring case and surrounding blanks.
    Text { field: TextField, op: TextOp, text: String },
    /// Release year between `min` and `max`, either optional. Tracks without a year never match.
    YearRange { min: Option<i64>, max: Option<i64> },
    /// Added to the library within the last `days` days.
    AddedWithin { days: i64 },
    /// Added to the library more than `days` days ago.
    AddedBefore { days: i64 },
    /// Length between `min_secs` and `max_secs`, either optional. Tracks without one never match.
    DurationRange { min_secs: Option<i64>, max_secs: Option<i64> },
    /// File format is this one ("MP3", "FLAC"), ignoring case.
    Format { value: String },
    /// In the case with this id (`value` true) or not in it (false). `case_name` is the case's
    /// name when the rule was saved. It plays no part in matching: the editor compares it with
    /// the case that has this id now, to warn when another card (or a rename) changed what the id means.
    InCase {
        case_id: i64,
        value: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        case_name: Option<String>,
    },
    /// Tags (app-side labels, compared ignoring case): has any / all / none of `tags`.
    Tags { how: TagMatch, tags: Vec<String> },
    /// Colour label (app-side): is one of `colours` (`value` true) or is none of them (false).
    /// The entry "none" stands for a track with no colour label.
    Colour { colours: Vec<String>, value: bool },
    /// Energy as a whole level 1 to 10 under the "Energy source" setting (your rating, the
    /// estimate from the audio, or your rating else the estimate), between `min` and `max`,
    /// either optional. Tracks with no energy never match.
    EnergyRange { min: Option<i64>, max: Option<i64> },
    /// Its items combined by `mode`. A group needs at least one item.
    Group { mode: Mode, items: Vec<Rule> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    Title,
    Artist,
    Album,
    Genre,
    Label,
    Bpm,
    Key,
    Rating,
    Year,
    Duration,
    DateAdded,
    Plays,
    LastPlayed,
    Format,
    FileSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub field: SortField,
    #[serde(default)]
    pub descending: bool,
}

/// What a smart case asks for: the rule tree's top level (`mode` over `rules`), then the
/// order, then "only the first N".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Query {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// `None` keeps the default order: artist, then title, then id.
    #[serde(default)]
    pub sort: Option<Sort>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// What a preview or count reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Eval {
    /// The tracks, in display order, after the limit.
    pub ids: Vec<i64>,
    /// How many matched before the limit cut the list.
    pub matched: usize,
    /// For an order by a column the track table does not show (year, date added, last played,
    /// label, rating, plays, format, size): each track's value, aligned with `ids`, so the
    /// preview can show what the order is based on. Empty for the other orders.
    pub values: Vec<String>,
}

// ---- keys --------------------------------------------------------------------------------

/// A key on the Camelot wheel: number 1 to 12 and `minor` (the "A" ring) or major ("B").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camelot {
    pub number: u8,
    pub minor: bool,
}

// Camelot number by pitch class (C = 0 ... B = 11).
const MINOR_NUMBER: [u8; 12] = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];
const MAJOR_NUMBER: [u8; 12] = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];

impl Camelot {
    /// Parses "8A", "08b", "Am", "A minor", "F#m", "Ab Minor", "C", "C major", "Cmaj", "Dbmin".
    /// `None` for anything else, including the empty string.
    pub fn parse(text: &str) -> Option<Camelot> {
        let t = text.trim();
        let b = t.as_bytes();
        let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
        if (1..=2).contains(&digits) && b.len() == digits + 1 {
            let number: u8 = t[..digits].parse().ok()?;
            let minor = match b[digits].to_ascii_lowercase() {
                b'a' => true,
                b'b' => false,
                _ => return None,
            };
            return ((1..=12).contains(&number)).then_some(Camelot { number, minor });
        }
        let mut chars = t.chars();
        let mut pc: i32 = match chars.next()?.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest = chars.as_str();
        let (accidental, rest) = match rest.chars().next() {
            Some('#' | '\u{266f}') => (1, &rest[rest.chars().next()?.len_utf8()..]),
            Some('b' | '\u{266d}') => (-1, &rest[rest.chars().next()?.len_utf8()..]),
            _ => (0, rest),
        };
        pc = (pc + accidental).rem_euclid(12);
        let minor = match rest.trim().to_lowercase().as_str() {
            "" | "maj" | "major" => false,
            "m" | "min" | "minor" => true,
            _ => return None,
        };
        let number = if minor { MINOR_NUMBER[pc as usize] } else { MAJOR_NUMBER[pc as usize] };
        Some(Camelot { number, minor })
    }

    /// Same key, one step either way round the wheel, or the relative major/minor.
    pub fn compatible(self, other: Camelot) -> bool {
        if self.number == other.number {
            return true;
        }
        let step = (i32::from(self.number) - i32::from(other.number)).rem_euclid(12);
        self.minor == other.minor && (step == 1 || step == 11)
    }

    /// Position on the wheel for sorting: 1A, 1B, 2A, 2B ...
    fn rank(self) -> f64 {
        f64::from(self.number) * 2.0 + if self.minor { 0.0 } else { 1.0 }
    }
}

// ---- migration -----------------------------------------------------------------------------

/// Rewrites forms older versions saved into the current ones, in place: the old genre rule
/// becomes a text rule on the genre column. Idempotent; groups are walked.
pub fn migrate(rules: &mut [Rule]) {
    for r in rules {
        match r {
            Rule::Genre { text } => {
                *r = Rule::Text { field: TextField::Genre, op: TextOp::Contains, text: std::mem::take(text) };
            }
            Rule::Group { items, .. } => migrate(items),
            _ => {}
        }
    }
}

// ---- validation ----------------------------------------------------------------------------

/// Why a query cannot be saved or run; worded for the user.
pub fn validate_query(q: &Query) -> Result<(), String> {
    if q.rules.is_empty() && q.limit.is_none() {
        return Err("Add at least one rule, or set a limit to take the first tracks of the whole library.".into());
    }
    if let Some(n) = q.limit
        && !(1..=MAX_LIMIT).contains(&n)
    {
        return Err(format!("The limit must be between 1 and {MAX_LIMIT} tracks."));
    }
    let mut nodes = 0;
    validate_items(&q.rules, 1, &mut nodes)
}

fn validate_items(items: &[Rule], depth: usize, nodes: &mut usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("Groups can be nested at most {MAX_DEPTH} deep."));
    }
    for r in items {
        *nodes += 1;
        if *nodes > MAX_NODES {
            return Err(format!("A smart case can have at most {MAX_NODES} rules and groups."));
        }
        validate_rule(r, depth, nodes)?;
    }
    Ok(())
}

fn check_range(label: &str, min: Option<i64>, max: Option<i64>, lowest: i64) -> Result<(), String> {
    if min.is_none() && max.is_none() {
        let a = if label.starts_with('e') { "An" } else { "A" };
        return Err(format!("{a} {label} rule needs a minimum, a maximum or both."));
    }
    if min.is_some_and(|v| v < lowest) || max.is_some_and(|v| v < lowest) {
        return Err(format!("{label} values must be {lowest} or more."));
    }
    if let (Some(lo), Some(hi)) = (min, max)
        && lo > hi
    {
        return Err(format!("The {label} minimum is above the maximum."));
    }
    Ok(())
}

fn validate_rule(r: &Rule, depth: usize, nodes: &mut usize) -> Result<(), String> {
    match r {
        Rule::RatingMin { stars } if !(1..=MAX_RATING).contains(stars) => Err(format!("A rating rule needs 1 to {MAX_RATING} stars.")),
        Rule::PlaysMin { n } if *n < 1 => Err("A plays rule needs a count of 1 or more.".into()),
        Rule::PlaysRange { min, max } => check_range("play count", *min, *max, 0),
        Rule::PlayedWithin { days } if *days < 1 => Err("A last-played rule needs 1 day or more.".into()),
        Rule::LastPlayedBefore { days, .. } if *days < 1 => Err("A not-played-since rule needs 1 day or more.".into()),
        Rule::BpmRange { min, max } => {
            if min.is_none() && max.is_none() {
                return Err("A BPM rule needs a minimum, a maximum or both.".into());
            }
            if min.is_some_and(|v| !v.is_finite() || v < 0.0) || max.is_some_and(|v| !v.is_finite() || v < 0.0) {
                return Err("BPM values must be positive numbers.".into());
            }
            if let (Some(lo), Some(hi)) = (min, max)
                && lo > hi
            {
                return Err("The BPM minimum is above the maximum.".into());
            }
            Ok(())
        }
        Rule::Key { key } | Rule::KeyCompatible { key } if Camelot::parse(key).is_none() => Err(format!("\"{key}\" is not a key (try 8A or Am).")),
        Rule::Genre { text } if text.trim().is_empty() => Err("A genre rule needs some text.".into()),
        Rule::Text { field, text, .. } if text.trim().is_empty() => Err(format!("A {} rule needs some text.", field_name(*field))),
        Rule::YearRange { min, max } => check_range("year", *min, *max, 1),
        Rule::AddedWithin { days } | Rule::AddedBefore { days } if *days < 1 => Err("A date-added rule needs 1 day or more.".into()),
        Rule::DurationRange { min_secs, max_secs } => check_range("duration", *min_secs, *max_secs, 0),
        Rule::Format { value } if value.trim().is_empty() => Err("A format rule needs a format such as MP3.".into()),
        Rule::InCase { case_id, .. } if *case_id < 0 => Err("A case rule needs a case.".into()),
        Rule::Tags { tags: list, .. } => {
            if list.is_empty() {
                return Err("A tag rule needs at least one tag.".into());
            }
            for t in list {
                tags::normalise_tag(t)?;
            }
            Ok(())
        }
        Rule::Colour { colours, .. } => {
            if colours.is_empty() {
                return Err("A colour rule needs at least one colour.".into());
            }
            match colours.iter().find(|c| c.as_str() != NO_COLOUR && !tags::COLOURS.contains(&c.as_str())) {
                Some(c) => Err(format!("\"{c}\" is not one of the colour labels.")),
                None => Ok(()),
            }
        }
        Rule::EnergyRange { min, max } => {
            check_range("energy", *min, *max, 1)?;
            if min.is_some_and(|v| v > i64::from(tags::MAX_ENERGY)) || max.is_some_and(|v| v > i64::from(tags::MAX_ENERGY)) {
                return Err(format!("Energy values must be {} or less.", tags::MAX_ENERGY));
            }
            Ok(())
        }
        Rule::Group { items, .. } => {
            if items.is_empty() {
                return Err("A group needs at least one rule.".into());
            }
            validate_items(items, depth + 1, nodes)
        }
        _ => Ok(()),
    }
}

fn field_name(f: TextField) -> &'static str {
    match f {
        TextField::Title => "title",
        TextField::Artist => "artist",
        TextField::Album => "album",
        TextField::Genre => "genre",
        TextField::Label => "label",
        TextField::Comments => "comments",
        TextField::Remixer => "remixer",
    }
}

/// Ids of the cases any rule in `rules` refers to, groups included.
pub fn referenced_cases(rules: &[Rule], out: &mut HashSet<i64>) {
    for r in rules {
        match r {
            Rule::InCase { case_id, .. } => {
                out.insert(*case_id);
            }
            Rule::Group { items, .. } => referenced_cases(items, out),
            _ => {}
        }
    }
}

// ---- data and evaluation -------------------------------------------------------------------

/// A snapshot of everything the rules read. Loaded once, then any number of queries run on it.
pub struct Data {
    pub tracks: Vec<Track>,
    pub extras: HashMap<i64, TrackExtra>,
    pub favourites: HashSet<i64>,
    /// Members of every case a query refers to; `None` for a case that does not exist.
    pub cases: HashMap<i64, Option<HashSet<i64>>>,
    /// Labels, auto energy and the energy choice; empty unless a rule reads them (see `load`).
    pub labels: LabelSnapshot,
    pub now_ms: i64,
}

/// Does any rule (groups included) read the app-side labels?
fn uses_labels(rules: &[Rule]) -> bool {
    rules.iter().any(|r| match r {
        Rule::Tags { .. } | Rule::Colour { .. } | Rule::EnergyRange { .. } => true,
        Rule::Group { items, .. } => uses_labels(items),
        _ => false,
    })
}

/// Does any rule (groups included) read energy (the energy choice and the estimates as well as
/// the labels)?
fn uses_energy(rules: &[Rule]) -> bool {
    rules.iter().any(|r| match r {
        Rule::EnergyRange { .. } => true,
        Rule::Group { items, .. } => uses_energy(items),
        _ => false,
    })
}

impl Data {
    /// Reads the library and the membership of every case any of `queries` mentions. When a rule
    /// reads labels, they are read too from the app config dir `config` (the labels file, the
    /// energy estimates and the energy choice); `None` (tests) means no labels exist. Every caller
    /// that has a config dir must pass it: without it a "tag none of" rule would match everything.
    pub fn load(lib: &Library, queries: &[&Query], now_ms: i64, config: Option<&Path>) -> Result<Data, Error> {
        let mut wanted = HashSet::new();
        for q in queries {
            referenced_cases(&q.rules, &mut wanted);
        }
        let mut cases = HashMap::new();
        for id in wanted {
            let members = match lib.case_info(id)? {
                Some(_) => Some(lib.case_track_ids(id)?.into_iter().collect()),
                None => None,
            };
            cases.insert(id, members);
        }
        let labels = match config {
            Some(dir) if queries.iter().any(|q| uses_labels(&q.rules)) => tags::snapshot(dir, lib, queries.iter().any(|q| uses_energy(&q.rules))).map_err(Error::Invalid)?,
            _ => LabelSnapshot::default(),
        };
        Ok(Data {
            tracks: lib.tracks()?,
            extras: lib.track_extras()?,
            favourites: lib.favourite_track_ids()?.into_iter().collect(),
            cases,
            labels,
            now_ms,
        })
    }
}

/// What a rule needs besides the track itself.
pub struct Context<'a> {
    pub favourites: &'a HashSet<i64>,
    pub cases: &'a HashMap<i64, Option<HashSet<i64>>>,
    pub labels: &'a LabelSnapshot,
    pub now_ms: i64,
}

fn text_of<'a>(f: TextField, t: &'a Track, x: &'a TrackExtra) -> &'a str {
    match f {
        TextField::Title => &t.title,
        TextField::Artist => &t.artist,
        TextField::Album => &t.album,
        TextField::Genre => &t.genre,
        TextField::Label => &x.label,
        TextField::Comments => &x.comments,
        TextField::Remixer => &x.remixer,
    }
}

fn in_range(v: i64, min: Option<i64>, max: Option<i64>) -> bool {
    min.is_none_or(|lo| v >= lo) && max.is_none_or(|hi| v <= hi)
}

pub fn rule_matches(rule: &Rule, t: &Track, x: &TrackExtra, cx: &Context) -> bool {
    let ago = |days: i64| cx.now_ms - days.saturating_mul(MS_PER_DAY);
    match rule {
        Rule::RatingMin { stars } => t.rating >= i64::from(*stars),
        Rule::Favourite { value } => cx.favourites.contains(&t.track_id) == *value,
        Rule::PlaysZero => t.times_played == 0,
        Rule::PlaysMin { n } => t.times_played >= *n,
        Rule::PlaysRange { min, max } => in_range(t.times_played, *min, *max),
        Rule::PlayedWithin { days } => t.last_played > 0 && t.last_played >= ago(*days),
        Rule::LastPlayedBefore { days, include_never } => {
            if t.last_played > 0 {
                t.last_played < ago(*days)
            } else {
                *include_never
            }
        }
        Rule::BpmRange { min, max } => t.bpm > 0.0 && min.is_none_or(|lo| t.bpm >= lo) && max.is_none_or(|hi| t.bpm <= hi),
        Rule::Key { key } => matches!((Camelot::parse(key), Camelot::parse(&t.key)), (Some(a), Some(b)) if a == b),
        Rule::KeyCompatible { key } => {
            matches!((Camelot::parse(key), Camelot::parse(&t.key)), (Some(a), Some(b)) if a.compatible(b))
        }
        Rule::Genre { text } => t.genre.to_lowercase().contains(&text.trim().to_lowercase()),
        Rule::Analysed { value } => t.analysed == *value,
        Rule::Text { field, op, text } => {
            let have = text_of(*field, t, x).trim().to_lowercase();
            let want = text.trim().to_lowercase();
            match op {
                TextOp::Contains => have.contains(&want),
                TextOp::Is => have == want,
                TextOp::StartsWith => have.starts_with(&want),
            }
        }
        Rule::YearRange { min, max } => x.year > 0 && in_range(x.year, *min, *max),
        Rule::AddedWithin { days } => x.date_added > 0 && x.date_added >= ago(*days),
        Rule::AddedBefore { days } => x.date_added > 0 && x.date_added < ago(*days),
        Rule::DurationRange { min_secs, max_secs } => t.play_time_secs > 0 && in_range(t.play_time_secs, *min_secs, *max_secs),
        Rule::Format { value } => t.format.trim().eq_ignore_ascii_case(value.trim()),
        Rule::InCase { case_id, value, .. } => {
            // a missing case is refused before evaluation starts (see `evaluate`)
            let inside = cx.cases.get(case_id).and_then(|m| m.as_ref()).is_some_and(|m| m.contains(&t.track_id));
            inside == *value
        }
        Rule::Tags { how, tags: wanted } => {
            let have: HashSet<String> = cx.labels.labels.get(&t.track_id).map(|l| l.tags.iter().map(|g| tags::tag_key(g)).collect()).unwrap_or_default();
            let wanted: Vec<String> = wanted.iter().map(|g| tags::tag_key(&tags::normalise_tag(g).unwrap_or_else(|_| g.clone()))).collect();
            match how {
                TagMatch::Any => wanted.iter().any(|g| have.contains(g)),
                TagMatch::All => wanted.iter().all(|g| have.contains(g)),
                TagMatch::None => !wanted.iter().any(|g| have.contains(g)),
            }
        }
        Rule::Colour { colours, value } => {
            let mine = cx.labels.labels.get(&t.track_id).and_then(|l| l.colour.as_deref()).unwrap_or(NO_COLOUR);
            colours.iter().any(|c| c == mine) == *value
        }
        Rule::EnergyRange { min, max } => cx.labels.energy_level(t.track_id).is_some_and(|e| in_range(i64::from(e), *min, *max)),
        Rule::Group { mode, items } => group_matches(*mode, items, t, x, cx),
    }
}

fn group_matches(mode: Mode, items: &[Rule], t: &Track, x: &TrackExtra, cx: &Context) -> bool {
    match mode {
        Mode::All => items.iter().all(|r| rule_matches(r, t, x, cx)),
        Mode::Any => items.iter().any(|r| rule_matches(r, t, x, cx)),
        Mode::None => !items.iter().any(|r| rule_matches(r, t, x, cx)),
    }
}

/// One track's value for a sort field.
enum Key {
    /// No value: sorts after every value, whichever way the order runs.
    Missing,
    Num(f64),
    Text(String),
}

fn sort_key(field: SortField, t: &Track, x: &TrackExtra) -> Key {
    let text = |s: &str| if s.trim().is_empty() { Key::Missing } else { Key::Text(s.trim().to_lowercase()) };
    let num = |v: f64, known: bool| if known { Key::Num(v) } else { Key::Missing };
    match field {
        SortField::Title => text(&t.title),
        SortField::Artist => text(&t.artist),
        SortField::Album => text(&t.album),
        SortField::Genre => text(&t.genre),
        SortField::Label => text(&x.label),
        SortField::Format => text(&t.format),
        SortField::Bpm => num(t.bpm, t.bpm > 0.0),
        SortField::Key => match Camelot::parse(&t.key) {
            Some(c) => Key::Num(c.rank()),
            None => Key::Missing,
        },
        SortField::Rating => Key::Num(t.rating as f64),
        SortField::Year => num(x.year as f64, x.year > 0),
        SortField::Duration => num(t.play_time_secs as f64, t.play_time_secs > 0),
        SortField::DateAdded => num(x.date_added as f64, x.date_added > 0),
        SortField::Plays => Key::Num(t.times_played as f64),
        SortField::LastPlayed => num(t.last_played as f64, t.last_played > 0),
        SortField::FileSize => num(t.file_size as f64, t.file_size > 0),
    }
}

fn cmp_keys(a: &Key, b: &Key, descending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let ord = match (a, b) {
        (Key::Missing, Key::Missing) => return Equal,
        (Key::Missing, _) => return Greater,
        (_, Key::Missing) => return Less,
        (Key::Num(p), Key::Num(q)) => p.total_cmp(q),
        (Key::Text(p), Key::Text(q)) => p.cmp(q),
        _ => Equal,
    };
    if descending { ord.reverse() } else { ord }
}

type TieKey = (String, String, i64);

/// "2023-11-14" for Unix milliseconds (UTC), an en dash for none.
fn date_label(ms: i64) -> String {
    if ms <= 0 {
        return "\u{2013}".into();
    }
    // civil-from-days (H. Hinnant), days since 1970-01-01
    let z = ms.div_euclid(MS_PER_DAY) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// What the preview shows for a sort field the track table has no column for.
fn value_label(field: SortField, t: &Track, x: &TrackExtra) -> Option<String> {
    let dash = || "\u{2013}".to_string();
    let or_dash = |s: &str| if s.trim().is_empty() { dash() } else { s.trim().to_string() };
    Some(match field {
        SortField::Label => or_dash(&x.label),
        SortField::Format => or_dash(&t.format),
        SortField::Year => if x.year > 0 { x.year.to_string() } else { dash() },
        SortField::DateAdded => date_label(x.date_added),
        SortField::LastPlayed => if t.last_played > 0 { date_label(t.last_played) } else { "never".into() },
        SortField::Rating => if t.rating > 0 { "\u{2605}".repeat(t.rating as usize) } else { dash() },
        SortField::Plays => t.times_played.to_string(),
        SortField::FileSize => format!("{:.1} MB", t.file_size as f64 / 1_048_576.0),
        // title, artist, album, genre, BPM, key and length are columns of the table already
        SortField::Title | SortField::Artist | SortField::Album | SortField::Genre | SortField::Bpm | SortField::Key | SortField::Duration => return None,
    })
}

/// The tracks `q` matches, in order, cut to its limit. Refuses a query whose rules name a case
/// that no longer exists (an "is not in" rule would otherwise quietly match everything).
pub fn evaluate(q: &Query, d: &Data) -> Result<Eval, String> {
    let mut wanted = HashSet::new();
    referenced_cases(&q.rules, &mut wanted);
    let mut missing: Vec<i64> = wanted.into_iter().filter(|id| !matches!(d.cases.get(id), Some(Some(_)))).collect();
    if !missing.is_empty() {
        missing.sort_unstable();
        return Err(format!("A rule refers to a case that no longer exists (case {}). Pick another case.", missing[0]));
    }
    // a labels source that could not be read is not "no labels": refuse, so "tags: none of X"
    // never matches everything and a smart case's device copy is never rewritten from it
    if uses_labels(&q.rules) {
        if let Some(why) = &d.labels.problem {
            return Err(format!("Tags, colours and energy cannot be read right now, so this smart case was not worked out. {why}"));
        }
    }
    if uses_energy(&q.rules) {
        if let Some(why) = &d.labels.energy_problem {
            return Err(format!("Energy cannot be read right now, so this smart case was not worked out. {why}"));
        }
    }
    let cx = Context { favourites: &d.favourites, cases: &d.cases, labels: &d.labels, now_ms: d.now_ms };
    let no_extra = TrackExtra::default();
    let mut hits: Vec<(&Track, &TrackExtra)> = d
        .tracks
        .iter()
        .map(|t| (t, d.extras.get(&t.track_id).unwrap_or(&no_extra)))
        // no rules at all (only possible with a limit) takes the whole library
        .filter(|(t, x)| q.rules.is_empty() || group_matches(q.mode, &q.rules, t, x, &cx))
        .collect();
    let matched = hits.len();
    // the default order, and the tie-break of every other
    let tie = |t: &Track| -> TieKey { (t.artist.to_lowercase(), t.title.to_lowercase(), t.track_id) };
    match q.sort {
        None => hits.sort_by_cached_key(|(t, _)| tie(t)),
        Some(s) => {
            let mut keyed: Vec<(Key, TieKey, (&Track, &TrackExtra))> = hits.into_iter().map(|(t, x)| (sort_key(s.field, t, x), tie(t), (t, x))).collect();
            keyed.sort_by(|a, b| cmp_keys(&a.0, &b.0, s.descending).then_with(|| a.1.cmp(&b.1)));
            hits = keyed.into_iter().map(|k| k.2).collect();
        }
    }
    if let Some(n) = q.limit {
        hits.truncate(n as usize);
    }
    let values = match q.sort {
        Some(s) => hits.iter().map(|(t, x)| value_label(s.field, t, x)).collect::<Option<Vec<_>>>().unwrap_or_default(),
        None => Vec::new(),
    };
    let ids = hits.into_iter().map(|(t, _)| t.track_id).collect();
    Ok(Eval { ids, matched, values })
}
