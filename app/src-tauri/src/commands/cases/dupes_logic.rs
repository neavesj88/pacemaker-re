//! The pure part of the duplicate finder: how two tracks count as copies of each other (tags,
//! size and length, sound), how links become groups, and which copy is suggested as the one to
//! keep. No library, files or app state in here, so every rule is tested on plain rows.

use std::collections::{BTreeMap, HashMap};

use pm_analysis::fingerprint::{self, Fingerprint};
use pm_formats::library::DupRow;
use serde::Serialize;

use super::dupes_settings::{DupSettings, KeeperRule};

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

/// Splits `(id, seconds)` into runs where neighbouring lengths differ by at most `tol`
/// (`None`: no length check, everything is one run).
fn runs_by_length(mut items: Vec<(i64, i64)>, tol: Option<i64>) -> Vec<Vec<i64>> {
    items.sort_by_key(|&(id, secs)| (secs, id));
    let mut runs: Vec<Vec<i64>> = Vec::new();
    let mut last = i64::MIN;
    for (id, secs) in items {
        let starts_new = match (tol, runs.last()) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(t), Some(_)) => secs - last > t,
        };
        if starts_new {
            runs.push(Vec::new());
        }
        runs.last_mut().expect("a run exists").push(id);
        last = secs;
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

// ---- sound ---------------------------------------------------------------------------------

/// The tracks worth fingerprinting: those with another track of nearly the same length, since
/// copies of one recording last the same. A track alone in its length needs no decoding.
pub fn audio_candidates(rows: &[&DupRow], s: &DupSettings) -> Vec<i64> {
    let mut by_len: Vec<(i64, i64)> = rows.iter().filter(|r| r.play_time_secs > 0).map(|r| (r.play_time_secs, r.track_id)).collect();
    by_len.sort_unstable();
    let tol = i64::from(s.audio_duration_secs);
    let mut keep = vec![false; by_len.len()];
    for i in 0..by_len.len() {
        if i + 1 < by_len.len() && by_len[i + 1].0 - by_len[i].0 <= tol {
            keep[i] = true;
            keep[i + 1] = true;
        }
    }
    let mut ids: Vec<i64> = by_len.iter().zip(&keep).filter(|(_, k)| **k).map(|((_, id), _)| *id).collect();
    ids.sort_unstable();
    ids
}

/// Pairs of tracks whose lengths agree and whose fingerprints are alike enough.
pub fn audio_links(rows: &[&DupRow], fps: &HashMap<i64, &Fingerprint>, s: &DupSettings) -> Vec<Link> {
    let mut by_len: Vec<&DupRow> = rows.iter().copied().filter(|r| r.play_time_secs > 0 && fps.get(&r.track_id).is_some_and(|f| !f.frames.is_empty())).collect();
    by_len.sort_by_key(|r| (r.play_time_secs, r.track_id));
    let tol = i64::from(s.audio_duration_secs);
    let max_offset = (s.audio_offset_secs as f32 / fingerprint::HOP_SECS).round() as usize;
    let mut links = Vec::new();
    for (i, a) in by_len.iter().enumerate() {
        for b in by_len[i + 1..].iter().take_while(|b| b.play_time_secs - a.play_time_secs <= tol) {
            let (fa, fb) = (fps[&a.track_id], fps[&b.track_id]);
            if let Some(m) = fingerprint::similarity(fa, fb, max_offset, 0.8)
                && m.score >= s.audio_threshold
            {
                links.push(Link { a: a.track_id, b: b.track_id, mode: Mode::Audio, score: Some(m.score), offset_secs: Some(m.offset_frames as f32 * fingerprint::HOP_SECS) });
            }
        }
    }
    links
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

#[cfg(test)]
mod tests {
    use super::*;
    use fingerprint::BANDS;

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

    fn fp(seed: u64, n: usize) -> Fingerprint {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
        let frames = (0..n)
            .map(|_| {
                std::array::from_fn::<i8, BANDS, _>(|_| {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    ((s >> 33) % 120) as i8 - 60
                })
            })
            .collect();
        Fingerprint { frames }
    }

    #[test]
    fn audio_candidates_skip_tracks_alone_in_their_length() {
        let rows = vec![row(1, "", "", 100), row(2, "", "", 102), row(3, "", "", 250), row(4, "", "", 0), row(5, "", "", 0), row(6, "", "", 400), row(7, "", "", 404)];
        let s = DupSettings::default(); // 3 s
        assert_eq!(audio_candidates(&refs(&rows), &s), vec![1, 2], "400 and 404 are 4 s apart; unknown lengths never qualify");
        let wide = DupSettings { audio_duration_secs: 5, ..Default::default() };
        assert_eq!(audio_candidates(&refs(&rows), &wide), vec![1, 2, 6, 7]);
    }

    #[test]
    fn audio_links_connect_alike_sound_within_the_length_window() {
        let rows = vec![row(1, "", "", 100), row(2, "", "", 101), row(3, "", "", 101), row(4, "", "", 160), row(5, "", "", 101)];
        let (a, other, empty) = (fp(1, 200), fp(2, 200), Fingerprint { frames: vec![] });
        // 1 and 2 are the same sound; 3 is different; 4 is the same sound but 60 s longer; 5 has no data
        let shifted = a.clone();
        let fps: HashMap<i64, &Fingerprint> = [(1, &a), (2, &shifted), (3, &other), (4, &a), (5, &empty)].into();
        let links = audio_links(&refs(&rows), &fps, &DupSettings::default());
        assert_eq!(pairs(&links), vec![(1, 2)]);
        assert!(links[0].score.unwrap() > 0.99 && links[0].offset_secs == Some(0.0) && links[0].mode == Mode::Audio);
        // a threshold of 1.0 still accepts an exact copy; an impossible window finds nothing
        assert_eq!(audio_links(&refs(&rows), &fps, &DupSettings { audio_threshold: 1.0, ..Default::default() }).len(), 1);
        assert!(audio_links(&refs(&rows), &HashMap::new(), &DupSettings::default()).is_empty());
    }
}
