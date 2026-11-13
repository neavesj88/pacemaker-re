//! The duplicate finder's rules that need no audio decoding, shared by the desktop app and
//! `pm duplicates`: its settings file, how two tracks count as copies of each other (tags, size
//! and length), how links become groups and which copy is suggested as the one to keep. No library,
//! files or app state in the matching, so every rule is tested on plain rows. (The sound check,
//! which compares fingerprints, lives with the fingerprint code in the app.)

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::library::{DupRow, MergeOptions, PlaysRule, RatingRule};

// ---- settings -----------------------------------------------------------------------------

/// Which copy is suggested as the one to keep. Ties fall to the next of: bitrate, file size,
/// plays, newest; and a copy whose file is missing from the device is never suggested while
/// another is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeeperRule {
    /// The highest bitrate (the best quality).
    #[default]
    Bitrate,
    /// The most plays.
    Plays,
    /// The most recently added.
    Newest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DupSettings {
    /// Find copies by the same artist and title.
    pub by_tags: bool,
    /// Find copies with the same file size and length.
    pub by_size: bool,
    /// Find copies by the sound itself (needs the fingerprint job to have run).
    pub by_audio: bool,
    /// Tags: ignore "feat. ..." / "ft. ..." in artist and title.
    pub strip_feat: bool,
    /// Tags: ignore text in brackets (off: "Original Mix" and "Extended Mix" stay different).
    pub ignore_brackets: bool,
    /// Tags: ignore a leading "The ".
    pub ignore_the: bool,
    /// Tags: lengths may differ by this many seconds; 0 turns the length check off.
    pub tag_duration_secs: u32,
    /// Size: lengths may differ by this many seconds.
    pub size_duration_secs: u32,
    /// Audio: how alike the sound must be, 0.5 to 1 (1 = identical shape).
    pub audio_threshold: f32,
    /// Audio: only tracks whose lengths differ by at most this many seconds are compared.
    pub audio_duration_secs: u32,
    /// Audio: one copy may start this many seconds later than the other.
    pub audio_offset_secs: u32,
    /// Audio: the part of the shorter track that must line up with the other, 0.5 to 1.
    pub audio_min_overlap: f32,
    /// Audio: tracks decoded at once while fingerprinting; 0 picks by the machine.
    pub fingerprint_threads: u32,
    pub keeper_rule: KeeperRule,
    /// Merge: copies' case memberships move to the kept track.
    pub move_cases: bool,
    pub rating_rule: RatingRule,
    pub plays_rule: PlaysRule,
    /// Merge: whether the other copies' rows may be taken out of the library at all. Off at
    /// first: deleting `tracks` rows has not been tried on a real Pacemaker. Whether a given
    /// merge does it is still the per-merge choice.
    pub allow_row_removal: bool,
}

impl Default for DupSettings {
    fn default() -> Self {
        DupSettings {
            by_tags: true,
            by_size: true,
            by_audio: false,
            strip_feat: true,
            ignore_brackets: false,
            ignore_the: true,
            tag_duration_secs: 10,
            size_duration_secs: 1,
            audio_threshold: 0.90,
            audio_duration_secs: 3,
            audio_offset_secs: 5,
            audio_min_overlap: 0.80,
            fingerprint_threads: 0,
            keeper_rule: KeeperRule::Bitrate,
            move_cases: true,
            rating_rule: RatingRule::Max,
            plays_rule: PlaysRule::Max,
            allow_row_removal: false,
        }
    }
}

impl DupSettings {
    /// Brings every number into the range the finder works in.
    pub fn clamped(mut self) -> Self {
        self.tag_duration_secs = self.tag_duration_secs.min(600);
        self.size_duration_secs = self.size_duration_secs.min(600);
        self.audio_duration_secs = self.audio_duration_secs.min(60);
        self.audio_offset_secs = self.audio_offset_secs.min(30);
        self.fingerprint_threads = self.fingerprint_threads.min(8);
        self.audio_min_overlap = if self.audio_min_overlap.is_finite() { self.audio_min_overlap.clamp(0.5, 1.0) } else { 0.80 };
        self.audio_threshold = if self.audio_threshold.is_finite() { self.audio_threshold.clamp(0.5, 1.0) } else { 0.90 };
        self
    }

    /// What a merge does with the group, as pm-formats takes it. `remove_rows` is the per-merge
    /// choice, never a stored setting.
    pub fn merge_options(&self, remove_rows: bool) -> MergeOptions {
        MergeOptions { move_cases: self.move_cases, rating: self.rating_rule, plays: self.plays_rule, remove_rows }
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join("duplicate-settings.json")
}

/// Reads one field at a time: a value of the wrong type is dropped (the default stays) and named.
fn from_value(v: &Value) -> (DupSettings, Vec<String>) {
    let mut base = serde_json::to_value(DupSettings::default()).unwrap_or(Value::Null);
    let mut ignored = Vec::new();
    if let (Some(obj), Some(file)) = (base.as_object_mut(), v.as_object()) {
        for (key, value) in file {
            let Some(old) = obj.get(key).cloned() else { continue }; // a key from a newer version
            obj.insert(key.clone(), value.clone());
            if serde_json::from_value::<DupSettings>(Value::Object(obj.clone())).is_err() {
                obj.insert(key.clone(), old);
                ignored.push(key.clone());
            }
        }
    }
    let settings = serde_json::from_value(base).unwrap_or_default();
    (DupSettings::clamped(settings), ignored)
}

/// The saved settings and, when something in the file could not be used, a sentence saying so.
pub fn load(dir: &Path) -> (DupSettings, Option<String>) {
    let p = path(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (DupSettings::default(), None),
        Err(e) => return (DupSettings::default(), Some(format!("{} could not be read ({e}); the default duplicate settings are in use.", p.display()))),
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.is_object() => {
            let (s, ignored) = from_value(&v);
            let warn = (!ignored.is_empty()).then(|| format!("{}: {} could not be understood and {} the default.", p.display(), ignored.join(", "), if ignored.len() == 1 { "uses" } else { "use" }));
            (s, warn)
        }
        _ => (DupSettings::default(), Some(format!("{} is not a settings file; the default duplicate settings are in use.", p.display()))),
    }
}

/// Writes the settings (temp file, then rename). A file that could not be read is copied to
/// `duplicate-settings.json.bad` first, once, so a hand edit is never silently lost.
pub fn save(dir: &Path, s: &DupSettings) -> Result<(), String> {
    crate::appfile::save_keeping_bad(&path(dir), &s.clone().clamped(), load(dir).1.is_some())
}

// ---- matching ---------------------------------------------------------------------------

/// The way two tracks were found to be copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Tags,
    Size,
    Audio,
}

/// Two tracks that count as copies, and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Link {
    pub a: i64,
    pub b: i64,
    pub mode: Mode,
    /// Audio only: how alike the sound is, 0 to 1.
    pub score: Option<f32>,
    /// Audio only: seconds the second starts after the first (negative: before).
    pub offset_secs: Option<f32>,
}

// ---- tags ----------------------------------------------------------------------------------

const FEAT_MARKERS: [&str; 5] = ["feat.", "feat ", "featuring ", "ft.", "ft "];

fn starts_with_marker(rest: &[char]) -> bool {
    FEAT_MARKERS.iter().any(|m| {
        let m: Vec<char> = m.chars().collect();
        rest.len() >= m.len() && rest[..m.len()] == m[..]
    })
}

/// Removes "feat. X" / "ft. X" / "featuring X": inside a bracket up to its closing bracket,
/// otherwise up to the next opening bracket or the end. Input is already lowercase.
fn drop_feat(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let (mut out, mut i, mut depth) = (String::new(), 0, 0i32);
    while i < chars.len() {
        let c = chars[i];
        let boundary = i == 0 || !chars[i - 1].is_alphanumeric();
        if boundary && starts_with_marker(&chars[i..]) {
            if depth > 0 {
                while i < chars.len() && !matches!(chars[i], ')' | ']' | '}') {
                    i += 1;
                }
            } else {
                while i < chars.len() && !matches!(chars[i], '(' | '[' | '{') {
                    i += 1;
                }
            }
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ => {}
        }
        out.push(c);
        i += 1;
    }
    out
}

fn drop_brackets(s: &str) -> String {
    let (mut out, mut depth) = (String::new(), 0i32);
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// A tag reduced to what makes two spellings the same: lowercase, no punctuation, one space
/// between words, "&" read as "and", optionally without "feat." parts, bracketed parts and a
/// leading "the".
pub fn normalise(text: &str, strip_feat: bool, ignore_brackets: bool, ignore_the: bool) -> String {
    let mut s = text.to_lowercase().replace('&', " and ");
    if strip_feat {
        s = drop_feat(&s);
    }
    if ignore_brackets {
        s = drop_brackets(&s);
    }
    let spaced: String = s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let mut words: Vec<&str> = spaced.split_whitespace().collect();
    if ignore_the && words.len() > 1 && words[0] == "the" {
        words.remove(0);
    }
    words.join(" ")
}

/// Splits `(id, seconds)` into runs whose lengths all lie within `tol` of the run's shortest
/// track, so no two tracks of a run are further apart than `tol` (`None`: no length check,
/// everything is one run). Comparing with the neighbour instead would let 200 s, 207 s and 214 s
/// chain into one set under a 10 s tolerance.
fn runs_by_length(mut items: Vec<(i64, i64)>, tol: Option<i64>) -> Vec<Vec<i64>> {
    items.sort_by_key(|&(id, secs)| (secs, id));
    let mut runs: Vec<Vec<i64>> = Vec::new();
    let mut start = i64::MIN;
    for (id, secs) in items {
        let starts_new = match (tol, runs.last()) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(t), Some(_)) => secs - start > t,
        };
        if starts_new {
            runs.push(Vec::new());
            start = secs;
        }
        runs.last_mut().expect("a run exists").push(id);
    }
    runs
}

fn chain_links(run: &[i64], mode: Mode) -> impl Iterator<Item = Link> + '_ {
    run.windows(2).map(move |w| Link { a: w[0], b: w[1], mode, score: None, offset_secs: None })
}

/// Tracks with the same normalised artist and title (both must be known) whose lengths agree.
/// Also returns how many tracks were left out for want of an artist or a title.
pub fn tag_links(rows: &[&DupRow], s: &DupSettings) -> (Vec<Link>, usize) {
    let mut by_key: BTreeMap<(String, String), Vec<(i64, i64)>> = BTreeMap::new();
    let mut unusable = 0;
    for r in rows {
        let artist = normalise(&r.artist, s.strip_feat, s.ignore_brackets, s.ignore_the);
        let title = normalise(&r.title, s.strip_feat, s.ignore_brackets, false);
        if artist.is_empty() || title.is_empty() {
            unusable += 1;
            continue;
        }
        by_key.entry((artist, title)).or_default().push((r.track_id, r.play_time_secs));
    }
    let tol = (s.tag_duration_secs > 0).then_some(i64::from(s.tag_duration_secs));
    let links = by_key.into_values().filter(|v| v.len() > 1).flat_map(|v| runs_by_length(v, tol)).flat_map(|run| chain_links(&run, Mode::Tags).collect::<Vec<_>>()).collect();
    (links, unusable)
}

/// Tracks with the same file size (not 0) and lengths within the tolerance.
pub fn size_links(rows: &[&DupRow], s: &DupSettings) -> Vec<Link> {
    let mut by_size: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.file_size > 0) {
        by_size.entry(r.file_size).or_default().push((r.track_id, r.play_time_secs));
    }
    by_size
        .into_values()
        .filter(|v| v.len() > 1)
        .flat_map(|v| runs_by_length(v, Some(i64::from(s.size_duration_secs))))
        .flat_map(|run| chain_links(&run, Mode::Size).collect::<Vec<_>>())
        .collect()
}

// ---- groups --------------------------------------------------------------------------------

/// Tracks that are copies of each other, directly or through a chain, with the links that say so.
#[derive(Debug, Clone, PartialEq)]
pub struct RawGroup {
    /// Sorted.
    pub ids: Vec<i64>,
    pub links: Vec<Link>,
}

/// Joins links into groups (union-find); the groups come out ordered by their lowest track id.
pub fn group(links: Vec<Link>) -> Vec<RawGroup> {
    let mut parent: HashMap<i64, i64> = HashMap::new();
    fn find(parent: &mut HashMap<i64, i64>, x: i64) -> i64 {
        let p = *parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let root = find(parent, p);
        parent.insert(x, root);
        root
    }
    for l in &links {
        let (ra, rb) = (find(&mut parent, l.a), find(&mut parent, l.b));
        if ra != rb {
            parent.insert(ra.max(rb), ra.min(rb));
        }
    }
    let mut groups: BTreeMap<i64, RawGroup> = BTreeMap::new();
    let ids: Vec<i64> = parent.keys().copied().collect();
    for id in ids {
        let root = find(&mut parent, id);
        groups.entry(root).or_insert_with(|| RawGroup { ids: Vec::new(), links: Vec::new() }).ids.push(id);
    }
    for l in links {
        let root = find(&mut parent, l.a);
        groups.get_mut(&root).expect("every linked track has a group").links.push(l);
    }
    let mut out: Vec<RawGroup> = groups.into_values().collect();
    for g in &mut out {
        g.ids.sort_unstable();
    }
    out.sort_by_key(|g| g.ids[0]);
    out
}

// ---- the copy to keep ----------------------------------------------------------------------

/// The copy suggested for keeping. A copy whose file is missing from the device is passed over
/// while another is present; then the rule decides, ties falling to the other qualities and
/// finally to the lowest track id (so the answer never depends on the order given).
pub fn pick_keeper(members: &[&DupRow], rule: KeeperRule, present: &dyn Fn(i64) -> bool) -> i64 {
    let key = |r: &&DupRow| {
        let tail = match rule {
            KeeperRule::Bitrate => [r.bit_rate_kbps, r.file_size, r.times_played, r.date_added],
            KeeperRule::Plays => [r.times_played, r.bit_rate_kbps, r.file_size, r.date_added],
            KeeperRule::Newest => [r.date_added, r.bit_rate_kbps, r.file_size, r.times_played],
        };
        (present(r.track_id), tail, std::cmp::Reverse(r.track_id))
    };
    members.iter().copied().max_by_key(key).map(|r| r.track_id).expect("a group has members")
}

/// Where a group goes in a list: by the suggested copy's artist, then title (case ignored), then
/// the group's id. The Duplicates view and `pm duplicates` both sort by this, so the first group
/// is the same one in both.
pub fn display_order(keeper: Option<&DupRow>, group_id: i64) -> (String, String, i64) {
    (keeper.map(|r| r.artist.to_lowercase()).unwrap_or_default(), keeper.map(|r| r.title.to_lowercase()).unwrap_or_default(), group_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, artist: &str, title: &str, secs: i64) -> DupRow {
        DupRow {
            track_id: id,
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: String::new(),
            composer: String::new(),
            genre: String::new(),
            label: String::new(),
            key: String::new(),
            comments: String::new(),
            producer: String::new(),
            remixer: String::new(),
            year: 0,
            track_number: 0,
            disc_number: 0,
            format: "MP3".into(),
            bit_rate_kbps: 0,
            sample_rate: 0,
            file_size: 0,
            play_time_secs: secs,
            bpm: 0.0,
            rating: 0,
            times_played: 0,
            last_played: -1,
            date_added: 0,
            location: format!("/pmdb_tracks/{id}.mp3"),
            analysed: true,
        }
    }

    fn refs(v: &[DupRow]) -> Vec<&DupRow> {
        v.iter().collect()
    }

    fn pairs(links: &[Link]) -> Vec<(i64, i64)> {
        links.iter().map(|l| (l.a, l.b)).collect()
    }

    #[test]
    fn normalising_makes_spellings_equal_and_keeps_versions_apart() {
        let n = |t: &str| normalise(t, true, false, true);
        assert_eq!(n("The Sample  Band"), n("sample band"));
        assert_eq!(n("Sample & Other"), n("SAMPLE AND OTHER"));
        assert_eq!(n("Sample Artist feat. Guest One"), "sample artist");
        assert_eq!(n("Sample Artist ft. Guest One"), "sample artist");
        assert_eq!(n("Sample Title (feat. Guest One) [Extended Mix]"), n("Sample Title [Extended Mix]"));
        assert_eq!(n("Sample Title feat. Guest (Original Mix)"), n("Sample Title (Original Mix)"));
        assert_ne!(n("Sample Title (Original Mix)"), n("Sample Title (Extended Mix)"), "versions stay apart by default");
        assert_eq!(normalise("Sample Title (Original Mix)", true, true, true), normalise("Sample Title [Extended Mix]", true, true, true), "unless brackets are ignored");
        assert_eq!(n("Café Déjà-Vu!"), "café déjà vu", "letters with accents are kept");
        assert_eq!(n("Defeat Right"), "defeat right", "feat inside a word is not a marker");
        assert_eq!(n("the"), "the", "a title that is only 'the' is not emptied");
        assert_eq!(normalise("The Sample", false, false, false), "the sample");
        assert_eq!(n("   "), "");
    }

    #[test]
    fn tag_links_need_artist_title_and_agreeing_length() {
        let rows = vec![
            row(1, "Sample Artist", "Sample Title", 200),
            row(2, "sample artist", "Sample Title (feat. Guest)", 203),
            row(3, "Sample Artist", "Sample Title", 400), // the long version: not a copy
            row(4, "", "Sample Title", 200),               // no artist: left out
            row(5, "Sample Artist", "Other Title", 200),
            row(6, "Sample Artist", "Sample Title", 207),
        ];
        let mut s = DupSettings::default();
        let (links, unusable) = tag_links(&refs(&rows), &s);
        assert_eq!(pairs(&links), vec![(1, 2), (2, 6)], "a chain of lengths within 10 s is one run, 400 s is another");
        assert_eq!(unusable, 1);
        assert!(links.iter().all(|l| l.mode == Mode::Tags && l.score.is_none()));
        s.tag_duration_secs = 0;
        assert_eq!(pairs(&tag_links(&refs(&rows), &s).0), vec![(1, 2), (2, 6), (6, 3)], "length check off");
        s.tag_duration_secs = 2;
        assert_eq!(pairs(&tag_links(&refs(&rows), &s).0), Vec::<(i64, i64)>::new(), "3 s apart is too far for 2 s");
    }

    #[test]
    fn a_set_never_spreads_wider_than_the_length_tolerance() {
        // 200, 207 and 214 s: each neighbour is within 10 s, but the ends are 14 s apart
        let rows = vec![row(1, "Sample Artist", "Sample Title", 200), row(2, "Sample Artist", "Sample Title", 207), row(3, "Sample Artist", "Sample Title", 214)];
        let s = DupSettings::default();
        assert_eq!(pairs(&tag_links(&refs(&rows), &s).0), vec![(1, 2)], "214 s is 14 s from the shortest and starts its own run");
        assert_eq!(runs_by_length(vec![(1, 200), (2, 207), (3, 214), (4, 215)], Some(10)), vec![vec![1, 2], vec![3, 4]]);
        assert_eq!(runs_by_length(vec![(1, 200), (2, 207), (3, 214)], None), vec![vec![1, 2, 3]]);
    }

    #[test]
    fn size_links_need_the_same_nonzero_size() {
        let mut rows = vec![row(1, "", "", 200), row(2, "", "", 200), row(3, "", "", 230), row(4, "", "", 200), row(5, "", "", 200)];
        for (r, size) in rows.iter_mut().zip([5_000_000, 5_000_000, 5_000_000, 0, 0]) {
            r.file_size = size;
        }
        let s = DupSettings::default();
        assert_eq!(pairs(&size_links(&refs(&rows), &s)), vec![(1, 2)], "a different length or no size is not a copy");
        let loose = DupSettings { size_duration_secs: 60, ..Default::default() };
        assert_eq!(pairs(&size_links(&refs(&rows), &loose)), vec![(1, 2), (2, 3)]);
    }

    #[test]
    fn groups_join_chains_across_ways_and_order_by_lowest_id() {
        let l = |a, b, mode| Link { a, b, mode, score: None, offset_secs: None };
        let groups = group(vec![l(5, 9, Mode::Tags), l(2, 3, Mode::Size), l(3, 5, Mode::Audio), l(20, 21, Mode::Tags)]);
        assert_eq!(groups.iter().map(|g| g.ids.clone()).collect::<Vec<_>>(), vec![vec![2, 3, 5, 9], vec![20, 21]]);
        assert_eq!(groups[0].links.len(), 3);
        assert!(group(vec![]).is_empty());
    }

    #[test]
    fn the_keeper_follows_the_rule_then_the_other_qualities() {
        let mut rows = vec![row(1, "", "", 0), row(2, "", "", 0), row(3, "", "", 0)];
        rows[0].bit_rate_kbps = 128;
        rows[0].times_played = 9;
        rows[0].date_added = 100;
        rows[1].bit_rate_kbps = 320;
        rows[1].times_played = 1;
        rows[1].date_added = 50;
        rows[2].bit_rate_kbps = 320;
        rows[2].times_played = 1;
        rows[2].date_added = 300;
        rows[2].file_size = 10;
        let all = |_| true;
        let m = refs(&rows);
        assert_eq!(pick_keeper(&m, KeeperRule::Bitrate, &all), 3, "320 ties, the larger file wins");
        assert_eq!(pick_keeper(&m, KeeperRule::Plays, &all), 1);
        assert_eq!(pick_keeper(&m, KeeperRule::Newest, &all), 3);
        // a missing file is passed over whatever the rule says
        assert_eq!(pick_keeper(&m, KeeperRule::Plays, &|id| id != 1), 3);
        // all missing: the rule still decides; full tie: the lowest id
        assert_eq!(pick_keeper(&m, KeeperRule::Plays, &|_| false), 1);
        let same = vec![row(8, "", "", 0), row(4, "", "", 0)];
        assert_eq!(pick_keeper(&refs(&same), KeeperRule::Bitrate, &all), 4);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-dupset-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let (s, warn) = load(&temp_dir("defaults"));
        assert!(warn.is_none());
        assert!(s.by_tags && s.by_size && !s.by_audio, "the sound check is opt-in: it needs the fingerprint job");
        assert_eq!((s.keeper_rule, s.move_cases, s.rating_rule, s.plays_rule), (KeeperRule::Bitrate, true, RatingRule::Max, PlaysRule::Max));
        assert_eq!((s.tag_duration_secs, s.size_duration_secs, s.audio_duration_secs, s.audio_offset_secs), (10, 1, 3, 5));
        assert_eq!((s.audio_min_overlap, s.allow_row_removal), (0.80, false), "removing rows is opt-in");
        assert!(!s.merge_options(false).remove_rows && s.merge_options(true).remove_rows);
    }

    #[test]
    fn saved_settings_come_back_snake_case_and_clamped() {
        let d = temp_dir("roundtrip");
        let s = DupSettings { by_audio: true, keeper_rule: KeeperRule::Newest, plays_rule: PlaysRule::Sum, audio_threshold: 7.0, audio_offset_secs: 999, fingerprint_threads: 99, audio_min_overlap: 0.1, allow_row_removal: true, ..Default::default() };
        save(&d, &s).unwrap();
        let (back, warn) = load(&d);
        assert!(warn.is_none());
        assert_eq!((back.keeper_rule, back.plays_rule, back.by_audio), (KeeperRule::Newest, PlaysRule::Sum, true));
        assert_eq!((back.audio_threshold, back.audio_offset_secs, back.fingerprint_threads), (1.0, 30, 8));
        assert_eq!((back.audio_min_overlap, back.allow_row_removal), (0.5, true));
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path(&d)).unwrap()).unwrap();
        assert_eq!(raw["keeper_rule"], "newest");
        assert_eq!(raw["plays_rule"], "sum");
        assert!(!path(&d).with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn one_bad_value_costs_only_that_setting() {
        let d = temp_dir("partial");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(path(&d), r#"{"keeper_rule":"loudest","by_size":false,"tag_duration_secs":"ten","future_key":1,"audio_threshold":0.8}"#).unwrap();
        let (s, warn) = load(&d);
        assert_eq!((s.keeper_rule, s.by_size, s.tag_duration_secs, s.audio_threshold), (KeeperRule::Bitrate, false, 10, 0.8));
        let warn = warn.unwrap();
        assert!(warn.contains("keeper_rule") && warn.contains("tag_duration_secs") && !warn.contains("future_key") && !warn.contains("by_size"), "{warn}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_damaged_file_gives_defaults_a_warning_and_is_kept_once() {
        let d = temp_dir("corrupt");
        std::fs::create_dir_all(&d).unwrap();
        for junk in ["not json", "[1,2]", ""] {
            std::fs::write(path(&d), junk).unwrap();
            let (s, warn) = load(&d);
            assert_eq!(s, DupSettings::default());
            assert!(warn.is_some(), "{junk:?}");
        }
        std::fs::write(path(&d), "hand edited {").unwrap();
        save(&d, &DupSettings { by_tags: false, ..Default::default() }).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d).with_extension("json.bad")).unwrap(), "hand edited {");
        assert!(!load(&d).0.by_tags);
        save(&d, &DupSettings::default()).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d).with_extension("json.bad")).unwrap(), "hand edited {", "a healthy file does not replace the kept copy");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
