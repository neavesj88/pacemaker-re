//! Smart cases: saved rule sets over the library that can optionally be materialised as a real
//! device case. The rules live in the app's config dir (one JSON file per library), never on
//! the device; only an explicit "Put on Pacemaker" writes anything to music.db, and that goes
//! through pm-formats `Library` like every other case write.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pm_formats::library::{Error, Library, MAX_CASE_NAME_CHARS, MAX_RATING, Track};
use serde::{Deserialize, Serialize};

/// Prefix of the device case a smart case is materialised as, so it is recognisable on the
/// device and in the Cases list. (How the device's font draws the star is not verified.)
pub const DEVICE_PREFIX: &str = "\u{2605} ";

const MS_PER_DAY: i64 = 86_400_000;

/// One condition. A smart case matches a track when every one of its rules does.
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
    /// Last played within the last `days` days.
    PlayedWithin { days: i64 },
    /// BPM between `min` and `max`, either optional. Tracks without a BPM never match.
    BpmRange { min: Option<f64>, max: Option<f64> },
    /// The track's key is exactly this one (any notation, compared as Camelot).
    Key { key: String },
    /// The track's key mixes with this one: same, +-1 on the wheel, or the relative key.
    KeyCompatible { key: String },
    /// Genre contains this text, ignoring case.
    Genre { text: String },
    /// Has been analysed (`true`) or has not (`false`).
    Analysed { value: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartCase {
    /// 0 when new; the store assigns the next free id on save.
    #[serde(default)]
    pub id: u32,
    pub name: String,
    pub rules: Vec<Rule>,
    /// The user asked for this to exist as a case on the Pacemaker.
    #[serde(default)]
    pub on_device: bool,
    /// `case_id` of that device case once it exists.
    #[serde(default)]
    pub device_case_id: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SmartFile {
    smart_cases: Vec<SmartCase>,
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
}

// ---- rules -------------------------------------------------------------------------------

/// Why a rule set cannot be saved or run; worded for the user.
pub fn validate(name: &str, rules: &[Rule]) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("The smart case needs a name.".into());
    }
    // The device copy is named with the star prefix, and the device limit applies to that.
    if DEVICE_PREFIX.chars().count() + name.trim().chars().count() > MAX_CASE_NAME_CHARS {
        let max = MAX_CASE_NAME_CHARS - DEVICE_PREFIX.chars().count();
        return Err(format!("The smart case name is too long (at most {max} characters)."));
    }
    validate_rules(rules)
}

/// The rule half of [`validate`]: what the rule builder needs for its live count.
pub fn validate_rules(rules: &[Rule]) -> Result<(), String> {
    if rules.is_empty() {
        return Err("Add at least one rule.".into());
    }
    for r in rules {
        match r {
            Rule::RatingMin { stars } if !(1..=MAX_RATING).contains(stars) => {
                return Err(format!("A rating rule needs 1 to {MAX_RATING} stars."));
            }
            Rule::PlaysMin { n } if *n < 1 => return Err("A plays rule needs a count of 1 or more.".into()),
            Rule::PlayedWithin { days } if *days < 1 => return Err("A last-played rule needs 1 day or more.".into()),
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
            }
            Rule::Key { key } | Rule::KeyCompatible { key } if Camelot::parse(key).is_none() => {
                return Err(format!("\"{key}\" is not a key (try 8A or Am)."));
            }
            Rule::Genre { text } if text.trim().is_empty() => return Err("A genre rule needs some text.".into()),
            _ => {}
        }
    }
    Ok(())
}

/// What the rules need to know besides the track itself.
pub struct Context<'a> {
    pub favourites: &'a HashSet<i64>,
    pub now_ms: i64,
}

pub fn rule_matches(rule: &Rule, t: &Track, cx: &Context) -> bool {
    match rule {
        Rule::RatingMin { stars } => t.rating >= i64::from(*stars),
        Rule::Favourite { value } => cx.favourites.contains(&t.track_id) == *value,
        Rule::PlaysZero => t.times_played == 0,
        Rule::PlaysMin { n } => t.times_played >= *n,
        Rule::PlayedWithin { days } => t.last_played > 0 && t.last_played >= cx.now_ms - days.saturating_mul(MS_PER_DAY),
        Rule::BpmRange { min, max } => t.bpm > 0.0 && min.is_none_or(|lo| t.bpm >= lo) && max.is_none_or(|hi| t.bpm <= hi),
        Rule::Key { key } => matches!((Camelot::parse(key), Camelot::parse(&t.key)), (Some(a), Some(b)) if a == b),
        Rule::KeyCompatible { key } => {
            matches!((Camelot::parse(key), Camelot::parse(&t.key)), (Some(a), Some(b)) if a.compatible(b))
        }
        Rule::Genre { text } => t.genre.to_lowercase().contains(&text.trim().to_lowercase()),
        Rule::Analysed { value } => t.analysed == *value,
    }
}

/// Ids of the tracks matching every rule, ordered by artist, title, then id so a materialised
/// case comes out in a stable, readable order. Invalid rules match nothing.
pub fn evaluate(rules: &[Rule], tracks: &[Track], cx: &Context) -> Vec<i64> {
    let mut hits: Vec<&Track> = tracks.iter().filter(|t| rules.iter().all(|r| rule_matches(r, t, cx))).collect();
    hits.sort_by_cached_key(|t| (t.artist.to_lowercase(), t.title.to_lowercase(), t.track_id));
    hits.into_iter().map(|t| t.track_id).collect()
}

// ---- storage -----------------------------------------------------------------------------

/// The file for a library: named by a hash of its root path (a drive letter or mount point),
/// so nothing about the device is written next to the rules.
pub fn store_path(dir: &Path, root: &str) -> PathBuf {
    let norm = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
    for b in norm.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    dir.join(format!("smart-cases-{h:016x}.json"))
}

pub fn load(dir: &Path, root: &str) -> Result<Vec<SmartCase>, String> {
    let p = store_path(dir, root);
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str::<SmartFile>(&s)
            .map(|f| f.smart_cases)
            .map_err(|e| format!("{} is not readable: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

fn save_all(dir: &Path, root: &str, list: Vec<SmartCase>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = store_path(dir, root);
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&SmartFile { smart_cases: list }).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// Saves a new (id 0) or existing smart case. The on-device fields of an existing one are kept
/// from the store, not taken from the caller, so an editor holding a stale copy cannot detach
/// or re-attach a device case. Returns the stored smart case.
pub fn save(dir: &Path, root: &str, incoming: SmartCase) -> Result<SmartCase, String> {
    validate(&incoming.name, &incoming.rules)?;
    let mut list = load(dir, root)?;
    let name = incoming.name.trim().to_string();
    let stored = if let Some(cur) = list.iter_mut().find(|s| s.id == incoming.id && incoming.id != 0) {
        cur.name = name;
        cur.rules = incoming.rules;
        cur.clone()
    } else {
        let id = list.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let fresh = SmartCase { id, name, rules: incoming.rules, on_device: false, device_case_id: None };
        list.push(fresh.clone());
        fresh
    };
    save_all(dir, root, list)?;
    Ok(stored)
}

/// Replaces the stored copy of a smart case (used after a device write updates its fields).
pub fn update(dir: &Path, root: &str, sc: &SmartCase) -> Result<(), String> {
    let mut list = load(dir, root)?;
    match list.iter_mut().find(|s| s.id == sc.id) {
        Some(slot) => *slot = sc.clone(),
        None => return Err(format!("no smart case {}", sc.id)),
    }
    save_all(dir, root, list)
}

/// Removes a smart case from the store; returns it, or `None` if it was not there.
pub fn remove(dir: &Path, root: &str, id: u32) -> Result<Option<SmartCase>, String> {
    let mut list = load(dir, root)?;
    let Some(pos) = list.iter().position(|s| s.id == id) else {
        return Ok(None);
    };
    let gone = list.remove(pos);
    save_all(dir, root, list)?;
    Ok(Some(gone))
}

// ---- the device copy ----------------------------------------------------------------------

/// The name of the device case for a smart case.
pub fn device_name(sc: &SmartCase) -> String {
    format!("{DEVICE_PREFIX}{}", sc.name.trim())
}

/// The device case a smart case remembers, but only while it is still ours: the id must exist
/// and the case must still carry [`DEVICE_PREFIX`]. A different card mounted at the same path,
/// or a case the user renamed and repurposed, fails this and is left alone.
fn owned_device_case(lib: &Library, sc: &SmartCase) -> Result<Option<i64>, Error> {
    let Some(id) = sc.device_case_id else {
        return Ok(None);
    };
    Ok(lib.case_info(id)?.filter(|c| c.name.starts_with(DEVICE_PREFIX)).map(|_| id))
}

/// Creates or refreshes the device case for `sc` so it holds exactly the current matches, in
/// match order; returns the case id and how many tracks it holds. The case is ours only while
/// it still carries the star prefix: it is recreated if it was deleted on the device or no
/// longer looks like ours, and a prefixed case that was merely renamed is renamed back.
pub fn put_on_device(lib: &mut Library, sc: &SmartCase, ids: &[i64]) -> Result<(i64, usize), Error> {
    let name = device_name(sc);
    let case_id = match owned_device_case(lib, sc)? {
        Some(id) => {
            // Members first: if that fails nothing has been renamed yet.
            lib.set_case_tracks(id, ids)?;
            lib.rename_case(id, &name)?;
            id
        }
        None => lib.create_case_with_tracks(&name, ids)?,
    };
    Ok((case_id, ids.len()))
}

/// Deletes the device case of `sc` if it still exists and is still ours (see
/// [`owned_device_case`]); returns whether one was deleted.
pub fn take_off_device(lib: &mut Library, sc: &SmartCase) -> Result<bool, Error> {
    match owned_device_case(lib, sc)? {
        Some(id) => lib.delete_case(id),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64) -> Track {
        Track {
            track_id: id,
            title: format!("T{id}"),
            artist: "A".into(),
            album: String::new(),
            genre: "House".into(),
            bpm: 128.0,
            key: "8A".into(),
            play_time_secs: 200,
            rating: 3,
            location: String::new(),
            file_size: 0,
            format: "MP3".into(),
            analysed: true,
            times_played: 0,
            last_played: -1,
            year: 0,
            label: String::new(),
            remixer: String::new(),
            composer: String::new(),
            comments: String::new(),
            date_added: -1,
            bit_rate: 0,
        }
    }

    fn cx(favs: &HashSet<i64>) -> Context<'_> {
        Context { favourites: favs, now_ms: 100 * MS_PER_DAY }
    }

    fn only(rule: Rule, t: &Track) -> bool {
        let favs = HashSet::from([t.track_id]);
        rule_matches(&rule, t, &cx(&favs))
    }

    #[test]
    fn camelot_parsing_covers_every_notation() {
        let c = |n, minor| Some(Camelot { number: n, minor });
        assert_eq!(Camelot::parse("8A"), c(8, true));
        assert_eq!(Camelot::parse(" 08b "), c(8, false));
        assert_eq!(Camelot::parse("12a"), c(12, true));
        assert_eq!(Camelot::parse("13A"), None);
        assert_eq!(Camelot::parse("0A"), None);
        assert_eq!(Camelot::parse("8C"), None);
        assert_eq!(Camelot::parse("Am"), c(8, true));
        assert_eq!(Camelot::parse("A minor"), c(8, true));
        assert_eq!(Camelot::parse("C"), c(8, false));
        assert_eq!(Camelot::parse("C major"), c(8, false));
        assert_eq!(Camelot::parse("Cmaj"), c(8, false));
        assert_eq!(Camelot::parse("F#m"), c(11, true));
        assert_eq!(Camelot::parse("Gbm"), c(11, true));
        assert_eq!(Camelot::parse("Ab Minor"), c(1, true));
        assert_eq!(Camelot::parse("G#min"), c(1, true));
        assert_eq!(Camelot::parse("Bb"), c(6, false));
        assert_eq!(Camelot::parse("B"), c(1, false));
        assert_eq!(Camelot::parse("Bm"), c(10, true));
        assert_eq!(Camelot::parse("Cb"), c(1, false)); // Cb = B
        assert_eq!(Camelot::parse("E#m"), c(4, true)); // E# = F
        assert_eq!(Camelot::parse("A\u{266d}m"), c(1, true));
        assert_eq!(Camelot::parse(""), None);
        assert_eq!(Camelot::parse("H"), None);
        assert_eq!(Camelot::parse("Amajor7"), None);
        // relative keys share a number
        assert_eq!(Camelot::parse("Am").unwrap().number, Camelot::parse("C").unwrap().number);
        assert_eq!(Camelot::parse("F#m").unwrap().number, Camelot::parse("A").unwrap().number);
    }

    #[test]
    fn camelot_compatibility_wraps_the_wheel() {
        let k = |s: &str| Camelot::parse(s).unwrap();
        assert!(k("8A").compatible(k("8A")));
        assert!(k("8A").compatible(k("9A")));
        assert!(k("8A").compatible(k("7A")));
        assert!(k("8A").compatible(k("8B")));
        assert!(!k("8A").compatible(k("9B")));
        assert!(!k("8A").compatible(k("10A")));
        assert!(k("12A").compatible(k("1A")), "wraps 12 -> 1");
        assert!(k("1B").compatible(k("12B")));
        assert!(!k("12A").compatible(k("2A")));
        assert!(k("Am").compatible(k("Dm")), "7A is next to 8A");
    }

    #[test]
    fn each_rule_matches_what_it_says() {
        let mut t = track(1);
        assert!(only(Rule::RatingMin { stars: 3 }, &t));
        assert!(!only(Rule::RatingMin { stars: 4 }, &t));
        assert!(only(Rule::Favourite { value: true }, &t));
        assert!(!only(Rule::Favourite { value: false }, &t));
        let none = HashSet::new();
        assert!(rule_matches(&Rule::Favourite { value: false }, &t, &cx(&none)));

        assert!(only(Rule::PlaysZero, &t));
        assert!(!only(Rule::PlaysMin { n: 1 }, &t));
        t.times_played = 5;
        assert!(!only(Rule::PlaysZero, &t));
        assert!(only(Rule::PlaysMin { n: 5 }, &t));
        assert!(!only(Rule::PlaysMin { n: 6 }, &t));

        // last played: never (-1) and 0 never match; the window is inclusive
        assert!(!only(Rule::PlayedWithin { days: 30 }, &t));
        t.last_played = 0;
        assert!(!only(Rule::PlayedWithin { days: 30 }, &t));
        t.last_played = 100 * MS_PER_DAY - 10 * MS_PER_DAY;
        assert!(only(Rule::PlayedWithin { days: 10 }, &t));
        assert!(!only(Rule::PlayedWithin { days: 9 }, &t));

        assert!(only(Rule::BpmRange { min: Some(128.0), max: Some(128.0) }, &t));
        assert!(only(Rule::BpmRange { min: Some(120.0), max: None }, &t));
        assert!(only(Rule::BpmRange { min: None, max: Some(130.0) }, &t));
        assert!(!only(Rule::BpmRange { min: Some(128.5), max: None }, &t));
        assert!(!only(Rule::BpmRange { min: None, max: Some(127.9) }, &t));
        t.bpm = 0.0;
        assert!(!only(Rule::BpmRange { min: None, max: Some(200.0) }, &t), "no BPM never matches");

        t.key = "Am".into();
        assert!(only(Rule::Key { key: "8A".into() }, &t), "notation does not matter");
        assert!(!only(Rule::Key { key: "9A".into() }, &t));
        assert!(only(Rule::KeyCompatible { key: "9A".into() }, &t));
        assert!(only(Rule::KeyCompatible { key: "C".into() }, &t));
        assert!(!only(Rule::KeyCompatible { key: "3B".into() }, &t));
        t.key = String::new();
        assert!(!only(Rule::Key { key: "8A".into() }, &t), "unknown key never matches");
        assert!(!only(Rule::KeyCompatible { key: "8A".into() }, &t));
        assert!(!only(Rule::Key { key: "nonsense".into() }, &track(2)), "invalid rule key matches nothing");

        assert!(only(Rule::Genre { text: "hou".into() }, &t));
        assert!(only(Rule::Genre { text: " HOUSE ".into() }, &t));
        assert!(!only(Rule::Genre { text: "techno".into() }, &t));

        assert!(only(Rule::Analysed { value: true }, &t));
        assert!(!only(Rule::Analysed { value: false }, &t));
        t.analysed = false;
        assert!(only(Rule::Analysed { value: false }, &t));
    }

    #[test]
    fn evaluate_ands_the_rules_and_orders_by_artist_title() {
        let mut a = track(1);
        a.artist = "zed".into();
        a.rating = 5;
        let mut b = track(2);
        b.artist = "Alpha".into();
        b.title = "second".into();
        b.rating = 5;
        let mut c = track(3);
        c.artist = "alpha".into();
        c.title = "First".into();
        c.rating = 5;
        let mut d = track(4);
        d.rating = 2;
        let tracks = vec![a, b, c, d];
        let favs = HashSet::from([1, 2, 3]);
        let rules = [Rule::RatingMin { stars: 4 }, Rule::Favourite { value: true }];
        assert_eq!(evaluate(&rules, &tracks, &cx(&favs)), vec![3, 2, 1]);
        let rules = [Rule::RatingMin { stars: 4 }, Rule::BpmRange { min: Some(200.0), max: None }];
        assert!(evaluate(&rules, &tracks, &cx(&favs)).is_empty());
        assert_eq!(evaluate(&[], &tracks, &cx(&favs)).len(), 4, "no rules = every track (validate refuses to save that)");
    }

    #[test]
    fn validate_explains_every_refusal() {
        let ok = [Rule::PlaysZero];
        assert!(validate("Name", &ok).is_ok());
        assert!(validate("  ", &ok).is_err());
        assert!(validate("Name", &[]).is_err());
        for bad in [
            Rule::RatingMin { stars: 0 },
            Rule::RatingMin { stars: 6 },
            Rule::PlaysMin { n: 0 },
            Rule::PlayedWithin { days: 0 },
            Rule::BpmRange { min: None, max: None },
            Rule::BpmRange { min: Some(140.0), max: Some(120.0) },
            Rule::BpmRange { min: Some(-1.0), max: None },
            Rule::BpmRange { min: Some(f64::NAN), max: None },
            Rule::Key { key: "x".into() },
            Rule::KeyCompatible { key: String::new() },
            Rule::Genre { text: "  ".into() },
        ] {
            assert!(validate("Name", std::slice::from_ref(&bad)).is_err(), "{bad:?}");
        }
        assert!(validate("Name", &[Rule::BpmRange { min: Some(120.0), max: Some(120.0) }, Rule::Key { key: "Am".into() }]).is_ok());
    }

    #[test]
    fn rules_round_trip_through_json_in_the_shape_the_ui_sends() {
        let json = r#"[{"kind":"rating_min","stars":4},{"kind":"plays_zero"},{"kind":"bpm_range","min":120,"max":null},
                       {"kind":"bpm_range"},{"kind":"key_compatible","key":"8A"},{"kind":"played_within","days":30}]"#;
        let rules: Vec<Rule> = serde_json::from_str(json).unwrap();
        assert_eq!(rules[0], Rule::RatingMin { stars: 4 });
        assert_eq!(rules[1], Rule::PlaysZero);
        assert_eq!(rules[2], Rule::BpmRange { min: Some(120.0), max: None });
        assert_eq!(rules[3], Rule::BpmRange { min: None, max: None });
        let back: Vec<Rule> = serde_json::from_str(&serde_json::to_string(&rules).unwrap()).unwrap();
        assert_eq!(back, rules);
        assert!(serde_json::from_str::<Rule>(r#"{"kind":"nope"}"#).is_err());
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-smart-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn sc(name: &str) -> SmartCase {
        SmartCase { id: 0, name: name.into(), rules: vec![Rule::PlaysZero], on_device: false, device_case_id: None }
    }

    #[test]
    fn store_is_per_library_and_keeps_device_fields_out_of_the_callers_hands() {
        let dir = temp_dir("store");
        assert!(load(&dir, "P:\\").unwrap().is_empty(), "no file yet is an empty list");
        assert_eq!(store_path(&dir, "P:\\"), store_path(&dir, "p:/"), "case and slash style do not matter");
        assert_ne!(store_path(&dir, "P:\\"), store_path(&dir, "Q:\\"));

        let a = save(&dir, "P:\\", sc("One")).unwrap();
        let b = save(&dir, "P:\\", sc("Two")).unwrap();
        assert_eq!((a.id, b.id), (1, 2));
        assert!(save(&dir, "Q:\\", sc("Elsewhere")).is_ok());
        assert_eq!(load(&dir, "P:\\").unwrap().len(), 2);
        assert_eq!(load(&dir, "Q:\\").unwrap().len(), 1);

        // device fields set by the store survive a save from a stale editor copy
        let mut attached = a.clone();
        attached.on_device = true;
        attached.device_case_id = Some(9);
        update(&dir, "P:\\", &attached).unwrap();
        let edited = save(&dir, "P:\\", SmartCase { id: 1, name: " Renamed ".into(), rules: vec![Rule::PlaysMin { n: 3 }], on_device: false, device_case_id: None }).unwrap();
        assert_eq!((edited.name.as_str(), edited.on_device, edited.device_case_id), ("Renamed", true, Some(9)));
        assert_eq!(edited.rules, vec![Rule::PlaysMin { n: 3 }]);

        // an unknown id is stored as a new one rather than overwriting; invalid input never writes
        let c = save(&dir, "P:\\", SmartCase { id: 77, ..sc("Three") }).unwrap();
        assert_eq!(c.id, 3);
        assert!(save(&dir, "P:\\", sc("")).is_err());
        assert!(save(&dir, "P:\\", SmartCase { rules: vec![], ..sc("Empty") }).is_err());
        assert_eq!(load(&dir, "P:\\").unwrap().len(), 3);
        assert!(update(&dir, "P:\\", &SmartCase { id: 99, ..sc("x") }).is_err());

        assert_eq!(remove(&dir, "P:\\", 2).unwrap().unwrap().name, "Two");
        assert!(remove(&dir, "P:\\", 2).unwrap().is_none());
        assert_eq!(load(&dir, "P:\\").unwrap().iter().map(|s| s.id).collect::<Vec<_>>(), vec![1, 3]);
        let d = save(&dir, "P:\\", sc("Four")).unwrap();
        assert_eq!(d.id, 4, "ids come from the highest in use");

        std::fs::write(store_path(&dir, "P:\\"), "not json").unwrap();
        assert!(load(&dir, "P:\\").is_err(), "a corrupt file is reported, not silently emptied");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn library(tag: &str, n: usize) -> (Library, PathBuf, Vec<i64>) {
        use pm_formats::{library::TrackMeta, strfile};
        let base = std::env::temp_dir();
        let vol = base.join(format!("pmr-smart-{tag}-vol-{}", std::process::id()));
        let src = base.join(format!("pmr-smart-{tag}-src-{}", std::process::id()));
        for d in [&vol, &src] {
            let _ = std::fs::remove_dir_all(d);
            std::fs::create_dir_all(d).unwrap();
        }
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![0u8; 3000 + i]).unwrap();
            let meta = TrackMeta { title: format!("t{i}"), format: "MP3".into(), play_time_secs: 60 * (i as i64 + 1), ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        std::fs::remove_dir_all(&src).unwrap();
        (lib, vol, ids)
    }

    #[test]
    fn device_copy_is_created_refreshed_renamed_back_and_removed() {
        let (mut lib, vol, ids) = library("device", 3);
        let mut s = SmartCase { id: 1, name: "Warm".into(), ..sc("x") };
        assert_eq!(device_name(&s), "\u{2605} Warm");

        let (case_id, n) = put_on_device(&mut lib, &s, &[ids[2], ids[0]]).unwrap();
        assert_eq!(n, 2);
        s.on_device = true;
        s.device_case_id = Some(case_id);
        let info = lib.case_info(case_id).unwrap().unwrap();
        assert_eq!((info.name.as_str(), info.track_count, info.play_time_secs), ("\u{2605} Warm", 2, 240));
        assert_eq!(lib.case_track_ids(case_id).unwrap(), vec![ids[2], ids[0]]);

        // refresh replaces the membership in place (same case) and restores a changed name
        lib.rename_case(case_id, "\u{2605} Someone renamed it").unwrap();
        let (again, _) = put_on_device(&mut lib, &s, &[ids[1]]).unwrap();
        assert_eq!(again, case_id);
        assert_eq!(lib.case_track_ids(case_id).unwrap(), vec![ids[1]]);
        assert_eq!(lib.case_info(case_id).unwrap().unwrap().name, "\u{2605} Warm");
        assert_eq!(lib.cases().unwrap().len(), 1);

        // an empty result is a valid, empty case
        put_on_device(&mut lib, &s, &[]).unwrap();
        assert_eq!(lib.case_info(case_id).unwrap().unwrap().track_count, 0);

        // renaming the smart case renames the device case on the next refresh
        s.name = "Warm 2".into();
        put_on_device(&mut lib, &s, &[ids[0]]).unwrap();
        assert_eq!(lib.case_info(case_id).unwrap().unwrap().name, "\u{2605} Warm 2");

        // removal deletes it; a refresh after the user deleted it on the device recreates it
        assert!(take_off_device(&mut lib, &s).unwrap());
        assert!(!take_off_device(&mut lib, &s).unwrap());
        assert!(lib.case_info(case_id).unwrap().is_none());
        let (fresh, _) = put_on_device(&mut lib, &s, &[ids[0]]).unwrap();
        assert_ne!(fresh, case_id, "ids are never reused");
        assert!(!take_off_device(&mut lib, &sc("never on device")).unwrap());

        // a case at the remembered id that is not ours (other card, renamed away) is never
        // overwritten or deleted
        let foreign = lib.create_case_with_tracks("Plain case", &[ids[1]]).unwrap();
        let stale = SmartCase { device_case_id: Some(foreign), ..s.clone() };
        assert!(!take_off_device(&mut lib, &stale).unwrap());
        assert_eq!(lib.case_info(foreign).unwrap().unwrap().name, "Plain case");
        let (made, _) = put_on_device(&mut lib, &stale, &[ids[0], ids[2]]).unwrap();
        assert_ne!(made, foreign);
        assert_eq!(lib.case_info(foreign).unwrap().unwrap().name, "Plain case");
        assert_eq!(lib.case_track_ids(foreign).unwrap(), vec![ids[1]]);

        // the name limit counts the star prefix
        assert!(validate(&"x".repeat(MAX_CASE_NAME_CHARS - 2), &[Rule::PlaysZero]).is_ok());
        assert!(validate(&"x".repeat(MAX_CASE_NAME_CHARS - 1), &[Rule::PlaysZero]).is_err());

        // an over-long name is refused before any write
        let long = SmartCase { name: "x".repeat(300), device_case_id: None, ..s.clone() };
        let before = lib.cases().unwrap().len();
        assert!(put_on_device(&mut lib, &long, &[ids[0]]).is_err());
        assert_eq!(lib.cases().unwrap().len(), before);
        assert!(lib.compatibility_problems().unwrap().is_empty());
        drop(lib);
        std::fs::remove_dir_all(vol).unwrap();
    }
}
