//! Metadata clean-up recipes: whitespace, Title Case, leading track numbers, promo text, feat.
//! forms, remixer extraction and the genre / artist maps. The engine is pure (strings in, strings
//! out) so the preview and the apply run exactly the same code; `preview` is what the table
//! shows, `verify_accepted` re-runs it to check that what the user accepted is still what the
//! recipes produce for the library as it is now. Recipes and maps are saved settings (a JSON file
//! in the app config dir); nothing about them is written to the device.
//!
//! The same recipes exist in `lib/devmock/cleanupEngine.ts` for the browser mock; both are held to
//! `lib/devmock/cleanupGolden.json` by their tests.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use pm_formats::ops_metadata::{MAX_TEXT_CHARS, TrackDetail};
use serde::{Deserialize, Serialize};

/// The text columns a recipe may change, in the order the table shows them.
pub const TEXT_FIELDS: [&str; 10] =
    ["title", "artist", "album_artist", "album", "composer", "genre", "label", "remixer", "producer", "comments"];

/// Spellings of "featuring" the user can normalise to.
pub const FEAT_FORMS: [&str; 4] = ["feat.", "feat", "ft.", "featuring"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapEntry {
    pub from: String,
    pub to: String,
}

fn map(pairs: &[(&str, &str)]) -> Vec<MapEntry> {
    pairs.iter().map(|(f, t)| MapEntry { from: (*f).into(), to: (*t).into() }).collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

/// Every choice of the clean-up tool. All of it is saved; nothing is hard-coded in the recipes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupSettings {
    /// Trim the ends and collapse runs of white space, in every text field.
    pub trim: bool,
    /// Title Case in `case_fields`.
    pub title_case: bool,
    pub case_fields: Vec<String>,
    /// Words that are always written like this in Title Case (DJ, UK, VIP), matched ignoring case.
    pub keep_upper: Vec<String>,
    /// Words Title Case keeps in lower case inside a title (a, of, the, feat., ...), matched
    /// ignoring case; the first and last word, and the first word after a colon, dash or bracket,
    /// are capitalised whatever this says.
    pub small_words: Vec<String>,
    /// Strip a leading "01 - " / "01. " / "1)" from the title.
    pub strip_number: bool,
    /// The most digits a leading number may have to count as a track number ("2020 Vision" has
    /// four and is left alone with the default of 3).
    pub number_max_digits: usize,
    /// Also strip "01 Title" (digits and a space only); off because "99 Luftballons" is a title.
    pub number_space_only: bool,
    /// Strip URLs and promo bracket text from `promo_fields`.
    pub strip_promo: bool,
    pub promo_fields: Vec<String>,
    /// A bracket made up of nothing but these phrases is promo text ("free download"); a bracket that
    /// also holds other words ("Exclusive Remix") is kept.
    pub promo_phrases: Vec<String>,
    /// Write feat. / ft. / featuring in one form, in title, artist and album artist.
    pub feat_normalise: bool,
    /// One of [`FEAT_FORMS`].
    pub feat_form: String,
    /// Move "feat. X" from the title to the artist.
    pub feat_to_artist: bool,
    /// Copy the remixer of "(X Remix)" into the remixer field.
    pub extract_remixer: bool,
    /// The last word of a bracket that makes it a remix credit ("remix", "edit", ...).
    pub remix_words: Vec<String>,
    /// Words that do not name a person ("Radio Edit", "Extended Mix" have no remixer).
    pub remixer_ignore: Vec<String>,
    /// Replace a remixer that is already filled in.
    pub overwrite_remixer: bool,
    pub genre_map_on: bool,
    pub genre_map: Vec<MapEntry>,
    pub artist_map_on: bool,
    pub artist_map: Vec<MapEntry>,
    /// What separates the names in an artist field, for the artist map to replace one name in
    /// "A & B feat. C": a comma goes straight after a name ("A, B"), every other separator has a
    /// space on both sides (" & ", " feat. ", " x "). Matched ignoring case.
    pub artist_separators: Vec<String>,
    /// Also write the changed fields into the audio files' own tags (off by default).
    pub write_back: bool,
    /// The fields the stand-alone "write library tags to files" tool offers by default.
    pub write_fields: Vec<String>,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        CleanupSettings {
            trim: true,
            title_case: false,
            case_fields: strings(&["title", "artist", "album"]),
            keep_upper: strings(&["DJ", "MC", "UK", "USA", "VIP", "EP", "LP", "TV", "II", "III", "IV"]),
            small_words: strings(&[
                "a", "an", "the", "and", "but", "or", "nor", "for", "so", "yet", "as", "at", "by", "in", "of", "on", "to",
                "up", "via", "vs", "vs.", "with", "from", "into", "onto", "per", "feat", "feat.", "ft", "ft.", "featuring",
                "if", "off",
            ]),
            strip_number: true,
            number_max_digits: 3,
            number_space_only: false,
            strip_promo: true,
            promo_fields: strings(&["title", "artist", "album", "comments"]),
            promo_phrases: strings(&[
                "free download",
                "free dl",
                "free track",
                "out now",
                "premiere",
                "download",
                "buy",
                "buy now",
                "exclusive",
                "official video",
                "official audio",
                "official music video",
                "lyric video",
                "music video",
                "youtube",
                "soundcloud",
                "bandcamp",
                "beatport",
                "hd",
                "hq",
            ]),
            feat_normalise: true,
            feat_form: "feat.".into(),
            feat_to_artist: false,
            extract_remixer: false,
            remix_words: strings(&["remix", "edit", "rework", "bootleg", "flip", "refix", "re-edit", "mashup"]),
            remixer_ignore: strings(&[
                "original", "radio", "extended", "club", "dub", "single", "album", "short", "instrumental", "vocal",
                "acapella", "a", "cappella", "clean", "dirty", "explicit", "main", "full", "long", "the", "new",
                "official", "remastered", "video", "version", "vip",
            ]),
            overwrite_remixer: false,
            genre_map_on: false,
            genre_map: map(&[
                ("Tech-House", "Tech House"),
                ("Deep-House", "Deep House"),
                ("Progressive-House", "Progressive House"),
                ("Drum and Bass", "Drum & Bass"),
                ("Drum n Bass", "Drum & Bass"),
                ("DnB", "Drum & Bass"),
                ("Hip-Hop", "Hip Hop"),
                ("Hiphop", "Hip Hop"),
                ("Electronica", "Electronic"),
            ]),
            artist_map_on: false,
            artist_map: Vec::new(),
            artist_separators: strings(&[",", "&", "/", "feat.", "feat", "ft.", "ft", "featuring", "vs.", "vs", "x", "and"]),
            write_back: false,
            write_fields: strings(&[
                "title", "artist", "album_artist", "album", "composer", "genre", "label", "remixer", "producer",
                "comments", "year",
            ]),
        }
    }
}

impl CleanupSettings {
    /// Every recipe off, lists as in the defaults: the base the golden tests switch recipes on from.
    #[cfg(test)]
    pub fn none() -> CleanupSettings {
        CleanupSettings {
            trim: false,
            strip_number: false,
            strip_promo: false,
            feat_normalise: false,
            ..CleanupSettings::default()
        }
    }

    /// Drops what cannot be used (unknown field names, an unknown feat. form, empty map
    /// entries, duplicates) so a hand-edited or old file never makes a recipe misbehave.
    pub fn sanitised(mut self) -> CleanupSettings {
        let known = |v: &mut Vec<String>| {
            let mut seen = Vec::new();
            v.retain(|f| {
                let ok = TEXT_FIELDS.contains(&f.as_str()) && !seen.contains(f);
                if ok {
                    seen.push(f.clone());
                }
                ok
            });
        };
        known(&mut self.case_fields);
        known(&mut self.promo_fields);
        let mut seen_write: Vec<String> = Vec::new();
        self.write_fields.retain(|f| {
            let ok = crate::commands::metadata::tagwrite::is_writable_field(f) && !seen_write.contains(f);
            if ok {
                seen_write.push(f.clone());
            }
            ok
        });
        if !FEAT_FORMS.contains(&self.feat_form.as_str()) {
            self.feat_form = "feat.".into();
        }
        let tidy = |v: &mut Vec<String>| {
            for s in v.iter_mut() {
                *s = collapse_ws(s);
            }
            v.retain(|s| !s.is_empty());
        };
        self.number_max_digits = self.number_max_digits.clamp(1, 6);
        tidy(&mut self.keep_upper);
        tidy(&mut self.small_words);
        tidy(&mut self.artist_separators);
        tidy(&mut self.promo_phrases);
        tidy(&mut self.remix_words);
        tidy(&mut self.remixer_ignore);
        for m in [&mut self.genre_map, &mut self.artist_map] {
            for e in m.iter_mut() {
                e.from = collapse_ws(&e.from);
                e.to = collapse_ws(&e.to);
            }
            m.retain(|e| !e.from.is_empty() && !e.to.is_empty());
        }
        self
    }
}

// ---- text helpers --------------------------------------------------------------------------

fn is_ws(c: char) -> bool {
    c.is_whitespace()
}

/// Trims and collapses runs of white space to one space. Only the zero-width space and the byte
/// order mark are dropped: the zero-width (non-)joiners are part of Persian, Indic and emoji text.
pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = false;
    for c in s.chars() {
        if matches!(c, '\u{200b}' | '\u{feff}') {
            continue;
        }
        if is_ws(c) {
            pending = true;
        } else {
            if pending && !out.is_empty() {
                out.push(' ');
            }
            pending = false;
            out.push(c);
        }
    }
    out
}

/// [`collapse_ws`] for one field: a comment may run over several lines, and those line breaks stay;
/// each line is tidied on its own and blank lines at either end go. Every other field is one line.
pub fn tidy_field(field: &str, s: &str) -> String {
    if field != "comments" || !s.contains(['\n', '\r']) {
        return collapse_ws(s);
    }
    let unified = s.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<String> = unified.split('\n').map(collapse_ws).collect();
    let first = lines.iter().position(|l| !l.is_empty());
    let last = lines.iter().rposition(|l| !l.is_empty());
    match (first, last) {
        (Some(a), Some(b)) => lines[a..=b].join("\n"),
        _ => String::new(),
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

fn words_lower(s: &str) -> Vec<String> {
    s.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

fn has_letter(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic())
}

/// Characters a dangling separator left behind by a removal is made of.
fn is_sep_char(c: char) -> bool {
    matches!(c, '-' | '–' | '—' | '|' | ':' | '/' | '~' | '•' | '·' | ',' | ';')
}

/// Trims separators that a removal left at either end ("Title - " after dropping a URL).
fn trim_dangling(s: &str) -> String {
    s.trim_matches(|c: char| is_ws(c) || is_sep_char(c)).to_string()
}

// ---- 1. leading track number ---------------------------------------------------------------

/// "01 - Title", "01. Title", "01_Title", "1) Title" lose the number. Needs a separator after
/// the digits unless `space_only`; keeps decimals ("1.5 Hours") and titles that are only
/// numbers; the rest must have a letter.
pub fn strip_track_number(s: &str, space_only: bool, max_digits: usize) -> String {
    let t = s.trim_start();
    let digits: usize = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > max_digits {
        return s.to_string();
    }
    let after = &t[digits..];
    let rest_ws = after.trim_start_matches(is_ws);
    let had_space = rest_ws.len() != after.len();
    let mut chars = rest_ws.chars();
    let sep = chars.next();
    let stripped = match sep {
        Some(c @ ('-' | '–' | '—' | '.' | '_' | ')' | ':')) => {
            let after_sep = chars.as_str();
            let spaced_after = after_sep.starts_with(is_ws);
            let rest = after_sep.trim_start_matches(|c: char| is_ws(c) || c == '_');
            // "1.5 Hours", "2-Step", "24-7": a number glued to the rest is not a track number
            // unless it is zero padded ("01-Title")
            let glued_ok = digits >= 2 || had_space || spaced_after;
            let decimal = c == '.' && rest.starts_with(|d: char| d.is_ascii_digit());
            if glued_ok && !decimal && has_letter(rest) { Some(rest) } else { None }
        }
        Some(_) if space_only && had_space && digits == 2 => {
            if has_letter(rest_ws) && !rest_ws.starts_with(|d: char| d.is_ascii_digit()) { Some(rest_ws) } else { None }
        }
        _ => None,
    };
    stripped.map(|r| r.to_string()).unwrap_or_else(|| s.to_string())
}

// ---- 2. URLs and promo text ----------------------------------------------------------------

const TLDS: [&str; 34] = [
    "com", "net", "org", "info", "biz", "ru", "fm", "co", "io", "me", "tv", "to", "ws", "cc", "us", "uk", "de", "nl",
    "fr", "it", "es", "pl", "se", "ch", "at", "be", "dk", "no", "fi", "jp", "cn", "br", "au", "nz",
];

/// Top-level domains that are also ordinary words ("Hello.It", "Say.No"): a bare address ending in
/// one of them counts only as the whole content of a bracket, where promo text lives.
const WORD_TLDS: [&str; 8] = ["it", "me", "to", "us", "no", "at", "be", "es"];

/// Is this one white-space-free token a web address?
/// - "http://...", "https://...", anything with "://", and "www.x.y" always;
/// - a bare "name.tld" or "name.tld/path" only when it is written in lower case, as addresses are
///   ("Rock.It", "Dance.With.Me" and "Big Beat.Net" are titles, not addresses), has a name of at
///   least three characters, a known top-level domain, and no more than one dot unless a path
///   follows; a bare address on a word-like domain (`WORD_TLDS`) only counts `in_bracket` (the whole content
///   of a bracket, such as "[djsite.me]").
fn looks_like_url(token: &str, in_bracket: bool) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.').trim_end_matches('.');
    let lower = t.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.") || lower.contains("://") {
        return true;
    }
    if t != lower {
        return false;
    }
    let (host, path) = match t.split_once('/') {
        Some((h, p)) => (h, p),
        None => (t, ""),
    };
    let has_path = path.chars().any(|c| c.is_alphanumeric());
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) || (parts.len() > 2 && !has_path) {
        return false;
    }
    let tld = parts[parts.len() - 1];
    let label = parts[parts.len() - 2];
    if !TLDS.contains(&tld) || label.chars().count() < 3 || !label.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return false;
    }
    has_path || in_bracket || !WORD_TLDS.contains(&tld)
}

fn contains_url(text: &str, in_bracket: bool) -> bool {
    text.split(is_ws).any(|t| looks_like_url(t, in_bracket))
}

/// Is the text inside a bracket promo: a URL, or nothing but whole-word promo phrases?
fn is_promo(content: &str, phrases: &[Vec<String>]) -> bool {
    // a bracket holding nothing but one address ("[djsite.me]") is promo even on a word-like domain
    let single = content.split(is_ws).filter(|t| !t.is_empty()).count() == 1;
    if contains_url(content, single) {
        return true;
    }
    // promo phrases only count when they are the whole bracket: "(Free Download)" goes, but
    // "(Exclusive Remix)" and "(Beatport Edit)" carry a mix credit and stay
    let words = words_lower(content);
    !words.is_empty() && covered_by_phrases(&words, phrases)
}

/// Can every word be accounted for by whole promo phrases laid end to end (longest phrase first)?
fn covered_by_phrases(words: &[String], phrases: &[Vec<String>]) -> bool {
    let mut i = 0;
    while i < words.len() {
        let best = phrases
            .iter()
            .filter(|p| !p.is_empty() && words[i..].starts_with(p))
            .map(|p| p.len())
            .max();
        match best {
            Some(n) => i += n,
            None => return false,
        }
    }
    true
}

fn closer_of(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// Index range `(start_of_content, end_of_content, index_after_closer)` of the group that opens at
/// char index `i`, or `None` when it never closes.
fn find_group(chars: &[char], i: usize) -> Option<(usize, usize, usize)> {
    let open = chars[i];
    let close = closer_of(open)?;
    let mut depth = 0;
    for (j, &c) in chars.iter().enumerate().skip(i) {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some((i + 1, j, j + 1));
            }
        }
    }
    None
}

fn strip_bracket_promo(s: &str, phrases: &[Vec<String>]) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some((a, b, next)) = find_group(&chars, i) {
            let inner: String = chars[a..b].iter().collect();
            // nested promo goes first, then the bracket is judged on what is left of it
            let cleaned = strip_bracket_promo(&inner, phrases);
            if is_promo(&cleaned, phrases) {
                // the space before the group goes with it
                while out.ends_with(' ') {
                    out.pop();
                }
                i = next;
                continue;
            }
            out.push(c);
            out.push_str(if cleaned == inner { &cleaned } else { cleaned.trim_end() });
            out.push(chars[b]);
            i = next;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Removes promo brackets and bare web addresses.
pub fn strip_promo(s: &str, phrases: &[String]) -> String {
    let phrases: Vec<Vec<String>> = phrases.iter().map(|p| words_lower(p)).filter(|p| !p.is_empty()).collect();
    let no_brackets = strip_bracket_promo(s, &phrases);
    let kept: Vec<&str> = no_brackets.split(is_ws).filter(|t| !t.is_empty() && !looks_like_url(t, false)).collect();
    let joined = kept.join(" ");
    if joined == collapse_ws(s) {
        return s.to_string();
    }
    trim_dangling(&joined)
}

/// [`strip_promo`] for one field. A comment is judged line by line, so its line breaks stay and a
/// line that was nothing but promo disappears whole.
pub fn strip_promo_field(field: &str, s: &str, phrases: &[String]) -> String {
    if field != "comments" || !s.contains('\n') {
        return strip_promo(s, phrases);
    }
    s.split('\n')
        .filter_map(|line| {
            let cleaned = strip_promo(line, phrases);
            if cleaned.trim().is_empty() && !line.trim().is_empty() { None } else { Some(cleaned) }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- 3. feat. forms ------------------------------------------------------------------------

fn feat_word(w: &str) -> bool {
    matches!(w.to_lowercase().as_str(), "feat" | "ft" | "featuring")
}

/// Positions `(start, end_of_word, end_including_dot)` of every "feat" token in `chars`: preceded
/// by white space or an opening bracket (never at the very start of the text), followed by a dot
/// or white space and then something.
fn feat_tokens(chars: &[char]) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    let mut i = 1;
    while i < chars.len() {
        let prev = chars[i - 1];
        if (is_ws(prev) || matches!(prev, '(' | '[' | '{')) && chars[i].is_alphabetic() {
            let mut j = i;
            while j < chars.len() && chars[j].is_alphabetic() {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            if feat_word(&word) {
                let (end, dot) = if j < chars.len() && chars[j] == '.' { (j + 1, true) } else { (j, false) };
                let follows_ok = end < chars.len() && (is_ws(chars[end]) || (dot && chars[end].is_alphanumeric()));
                if follows_ok {
                    out.push((i, j, end));
                    i = end;
                    continue;
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

/// Rewrites every feat. / ft. / featuring token to `form`.
pub fn normalise_feat(s: &str, form: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let tokens = feat_tokens(&chars);
    if tokens.is_empty() {
        return s.to_string();
    }
    let mut out = String::new();
    let mut at = 0;
    for (start, _word_end, end) in tokens {
        out.extend(&chars[at..start]);
        out.push_str(form);
        // "feat.Name" gets its space
        if end < chars.len() && !is_ws(chars[end]) {
            out.push(' ');
        }
        at = end;
    }
    out.extend(&chars[at..]);
    out
}

/// How many brackets are still open at the end of `chars` (never below zero).
fn bracket_depth(chars: &[char]) -> usize {
    let mut depth = 0usize;
    for &c in chars {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Takes "feat. X" out of a title. Returns the title without it and the names, or `None`.
/// A bracketed "(feat. X)" anywhere, or a bare "feat. X" that runs to the next bracket or the end.
pub fn split_feat(title: &str) -> Option<(String, String)> {
    let chars: Vec<char> = title.chars().collect();
    // bracketed form first
    let mut i = 0;
    while i < chars.len() {
        if let Some((a, b, next)) = find_group(&chars, i) {
            let inner: String = chars[a..b].iter().collect();
            let inner_chars: Vec<char> = format!(" {}", inner.trim()).chars().collect();
            if let Some(&(start, _, end)) = feat_tokens(&inner_chars).first() {
                let names = collapse_ws(&inner_chars[end..].iter().collect::<String>());
                // the bracket must open with the feat word: "(feat. X)", not "(Song feat. X)"
                if start == 1 && !names.is_empty() {
                    let mut left: String = chars[..i].iter().collect();
                    while left.ends_with(' ') {
                        left.pop();
                    }
                    let right: String = chars[next..].iter().collect();
                    return Some((collapse_ws(&format!("{left} {right}")), names));
                }
            }
            i = next;
            continue;
        }
        i += 1;
    }
    // bare form: only a token outside every bracket. Inside "(Foo feat. Bar Remix)" the names run
    // into the bracket's other words, and cutting mid-bracket would leave it unbalanced.
    let tokens = feat_tokens(&chars);
    let &(start, _, end) = tokens.iter().find(|&&(start, _, _)| bracket_depth(&chars[..start]) == 0)?;
    let rest: Vec<char> = chars[end..].to_vec();
    let stop = rest.iter().position(|c| matches!(c, '(' | '[' | '{')).unwrap_or(rest.len());
    let names = collapse_ws(&rest[..stop].iter().collect::<String>());
    let names = trim_dangling(&names);
    if names.is_empty() {
        return None;
    }
    let left: String = chars[..start].iter().collect();
    let right: String = rest[stop..].iter().collect();
    Some((collapse_ws(&format!("{left} {right}")), names))
}

// ---- 4. remixer ----------------------------------------------------------------------------

/// The remixer named by a "(X Remix)" style bracket in the title, if any.
pub fn extract_remixer(title: &str, remix_words: &[String], ignore: &[String]) -> Option<String> {
    let chars: Vec<char> = title.chars().collect();
    let remix: Vec<String> = remix_words.iter().map(|w| w.to_lowercase()).collect();
    let ignore: Vec<String> = ignore.iter().map(|w| w.to_lowercase()).collect();
    let mut i = 0;
    while i < chars.len() {
        let Some((a, b, next)) = find_group(&chars, i) else {
            i += 1;
            continue;
        };
        let inner: String = chars[a..b].iter().collect();
        let tokens: Vec<&str> = inner.split(is_ws).filter(|t| !t.is_empty()).collect();
        i = next;
        let Some((last, names)) = tokens.split_last() else { continue };
        let last_clean = last.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if !remix.contains(&last_clean) {
            continue;
        }
        let mut names: Vec<&str> = names.to_vec();
        while let Some(w) = names.last() {
            let wl = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            if ignore.contains(&wl) { names.pop(); } else { break }
        }
        if names.is_empty() {
            continue;
        }
        let mut name = names.join(" ");
        for suffix in ["'s", "\u{2019}s"] {
            if let Some(stripped) = name.strip_suffix(suffix) {
                name = stripped.to_string();
            }
        }
        let name = name.trim().to_string();
        if has_letter(&name) {
            return Some(name);
        }
    }
    None
}

// ---- 5. Title Case -------------------------------------------------------------------------

fn is_small_word(w: &str, small: &[String]) -> bool {
    let l = w.to_lowercase();
    small.iter().any(|s| s.to_lowercase() == l)
}

fn capitalise(w: &str) -> String {
    let mut out = String::with_capacity(w.len());
    let mut done = false;
    for c in w.chars() {
        if !done && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            done = true;
        } else if done {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn lowercase(w: &str) -> String {
    w.to_lowercase()
}

#[derive(PartialEq, Clone, Copy)]
enum Shape {
    NoLetters,
    Lower,
    Upper,
    Capitalised,
    Mixed,
}

fn shape(w: &str) -> Shape {
    let letters: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return Shape::NoLetters;
    }
    if letters.iter().all(|c| c.is_lowercase()) {
        return Shape::Lower;
    }
    if letters.iter().all(|c| c.is_uppercase()) {
        return Shape::Upper;
    }
    let first_upper = letters[0].is_uppercase();
    if first_upper && letters[1..].iter().all(|c| c.is_lowercase()) {
        return Shape::Capitalised;
    }
    Shape::Mixed
}

/// Title Case. `keep_upper` spellings win. Words that already mix cases (McFly, iPhone) are left
/// alone, and an upper-case word inside a mixed-case text is taken for an acronym; a text that is
/// all upper case is converted. Small words (`small_words`: a, of, the, feat., ...) stay lower case except at
/// the start or end, after a colon, dash, opening bracket or sentence end.
pub fn title_case(s: &str, keep_upper: &[String], small_words: &[String]) -> String {
    let keep: HashMap<String, &str> = keep_upper.iter().map(|k| (k.to_lowercase(), k.as_str())).collect();
    let all_upper = {
        let mut letters = s.chars().filter(|c| c.is_alphabetic()).peekable();
        letters.peek().is_some() && letters.all(|c| !c.is_lowercase())
    };
    // split into words (alphanumerics, apostrophes and dots between letters) and separators
    let chars: Vec<char> = s.chars().collect();
    let mut tokens: Vec<(bool, String)> = Vec::new(); // (is_word, text)
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let word_start = is_word_char(c) || ((c == '\'' || c == '\u{2019}') && i + 1 < chars.len() && is_word_char(chars[i + 1]));
        if word_start {
            let mut j = i + 1;
            while j < chars.len() {
                let d = chars[j];
                let apostrophe_inside = (d == '\'' || d == '\u{2019}') && j + 1 < chars.len() && is_word_char(chars[j + 1]);
                // "feat." / "vs." keep their dot so the small-word list can see it
                if is_word_char(d) || apostrophe_inside {
                    j += 1;
                } else {
                    break;
                }
            }
            let mut text: String = chars[i..j].iter().collect();
            if j < chars.len() && chars[j] == '.' {
                let l = text.to_lowercase();
                if l == "feat" || l == "ft" || l == "vs" {
                    text.push('.');
                    j += 1;
                }
            }
            tokens.push((true, text));
            i = j;
        } else {
            let mut j = i + 1;
            while j < chars.len() && !(is_word_char(chars[j]) || ((chars[j] == '\'' || chars[j] == '\u{2019}') && j + 1 < chars.len() && is_word_char(chars[j + 1]))) {
                j += 1;
            }
            tokens.push((false, chars[i..j].iter().collect()));
            i = j;
        }
    }
    // the first and last word count numbers too: "3 of 4" keeps its "of" small
    let all_words: Vec<usize> = tokens.iter().enumerate().filter(|(_, (w, _))| *w).map(|(i, _)| i).collect();
    let last_word = all_words.last().copied();
    let first_word = all_words.first().copied();
    let mut out = String::with_capacity(s.len());
    let mut prev_sep = String::new();
    for (idx, (is_word, text)) in tokens.iter().enumerate() {
        if !*is_word {
            out.push_str(text);
            prev_sep = text.clone();
            continue;
        }
        if !has_letter(text) {
            out.push_str(text);
            continue;
        }
        let word = text.as_str();
        // possessive: "dj's" keeps "DJ" + 's
        let (base, tail) = match word.strip_suffix("'s").or_else(|| word.strip_suffix("\u{2019}s")) {
            Some(b) => (b, &word[b.len()..]),
            None => (word, ""),
        };
        if let Some(spelling) = keep.get(&base.to_lowercase()) {
            out.push_str(spelling);
            out.push_str(tail);
            continue;
        }
        // does a new clause start here?
        let clause_start = Some(idx) == first_word
            || Some(idx) == last_word
            || prev_sep.chars().any(|c| matches!(c, '(' | '[' | '{' | ':' | '"' | '\u{201c}' | '!' | '?'))
            || prev_sep.contains(" - ")
            || prev_sep.contains(" – ")
            || prev_sep.contains(" — ")
            || prev_sep.contains(". ")
            || (!prev_sep.is_empty() && !prev_sep.chars().any(is_ws)); // hyphenated compound: Tech-House
        // "1st", "2pac": a word that starts with a digit is left as written
        let converted = match shape(word) {
            _ if word.starts_with(|c: char| c.is_ascii_digit()) => word.to_string(),
            Shape::NoLetters | Shape::Mixed => word.to_string(),
            Shape::Upper if !all_upper && word.chars().filter(|c| c.is_alphabetic()).count() > 1 => word.to_string(),
            Shape::Upper if !all_upper => capitalise(word), // a lone capital letter is just a letter
            Shape::Upper | Shape::Lower | Shape::Capitalised => {
                if !clause_start && is_small_word(word, small_words) { lowercase(word) } else if shape(word) == Shape::Capitalised { word.to_string() } else { capitalise(word) }
            }
        };
        out.push_str(&converted);
    }
    out
}

// ---- 6. maps -------------------------------------------------------------------------------

fn lookup<'a>(map: &'a [MapEntry], value: &str) -> Option<&'a str> {
    let key = collapse_ws(value).to_lowercase();
    map.iter().find(|e| collapse_ws(&e.from).to_lowercase() == key).map(|e| e.to.as_str())
}

/// The text each separator of the settings stands for, longest first: a comma (or semicolon) goes
/// straight after a name, anything else has a space on both sides.
fn separator_patterns(separators: &[String]) -> Vec<Vec<char>> {
    let mut v: Vec<Vec<char>> = separators
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let s = s.to_lowercase();
            if s == "," || s == ";" { format!("{s} ") } else { format!(" {s} ") }.chars().collect()
        })
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(p.len()));
    v
}

/// Splits "A & B feat. C" into [("A", " & "), ("B", " feat. "), ("C", "")] so a name can be
/// replaced without touching the separators.
fn split_artists(s: &str, separators: &[String]) -> Vec<(String, String)> {
    let patterns = separator_patterns(separators);
    let chars: Vec<char> = s.chars().collect();
    let lower: Vec<char> = s.to_lowercase().chars().collect();
    // lowercasing can change the length for a few characters; then do not split at all
    if lower.len() != chars.len() {
        return vec![(s.to_string(), String::new())];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        let mut hit: Option<usize> = None;
        for sc in &patterns {
            if i + sc.len() <= chars.len() && lower[i..i + sc.len()] == sc[..] {
                hit = Some(sc.len());
                break;
            }
        }
        if let Some(n) = hit {
            parts.push((chars[start..i].iter().collect::<String>(), chars[i..i + n].iter().collect::<String>()));
            i += n;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push((chars[start..].iter().collect::<String>(), String::new()));
    parts
}

fn apply_artist_map(value: &str, map: &[MapEntry], separators: &[String]) -> String {
    if let Some(to) = lookup(map, value) {
        return to.to_string();
    }
    let parts = split_artists(value, separators);
    if parts.len() == 1 {
        return value.to_string();
    }
    parts
        .into_iter()
        .map(|(name, sep)| format!("{}{}", lookup(map, &name).map(str::to_string).unwrap_or(name), sep))
        .collect()
}

// ---- the pipeline --------------------------------------------------------------------------

pub type Fields = BTreeMap<String, String>;

pub fn fields_of(d: &TrackDetail) -> Fields {
    let pairs = [
        ("title", &d.title),
        ("artist", &d.artist),
        ("album_artist", &d.album_artist),
        ("album", &d.album),
        ("composer", &d.composer),
        ("genre", &d.genre),
        ("label", &d.label),
        ("remixer", &d.remixer),
        ("producer", &d.producer),
        ("comments", &d.comments),
    ];
    pairs.into_iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// Runs the ticked recipes over one track's text fields and returns all of them (changed or not).
pub fn clean_fields(cur: &Fields, s: &CleanupSettings) -> Fields {
    let mut w = cur.clone();
    let get = |w: &Fields, f: &str| w.get(f).cloned().unwrap_or_default();
    let put = |w: &mut Fields, f: &str, v: String| {
        w.insert(f.to_string(), v);
    };
    // text a recipe changed is tidied; text it left alone is not touched
    let edit = |w: &mut Fields, f: &str, new: String| {
        let old = get(w, f);
        if new != old {
            put(w, f, tidy_field(f, &new));
        }
    };

    if s.trim {
        for f in TEXT_FIELDS {
            let v = tidy_field(f, &get(&w, f));
            put(&mut w, f, v);
        }
    }
    if s.strip_promo {
        for f in &s.promo_fields {
            let v = strip_promo_field(f, &get(&w, f), &s.promo_phrases);
            edit(&mut w, f, v);
        }
    }
    if s.strip_number {
        let v = strip_track_number(&get(&w, "title"), s.number_space_only, s.number_max_digits);
        edit(&mut w, "title", v);
    }
    if s.feat_normalise {
        for f in ["title", "artist", "album_artist"] {
            let v = normalise_feat(&get(&w, f), &s.feat_form);
            edit(&mut w, f, v);
        }
    }
    if s.feat_to_artist {
        let artist = get(&w, "artist");
        if !artist.is_empty() {
            if let Some((title, names)) = split_feat(&get(&w, "title")) {
                if !title.is_empty() {
                    let form = if s.feat_normalise { s.feat_form.as_str() } else { "feat." };
                    put(&mut w, "title", title);
                    if !artist.to_lowercase().contains(&names.to_lowercase()) {
                        put(&mut w, "artist", format!("{artist} {form} {names}"));
                    }
                }
            }
        }
    }
    if s.extract_remixer {
        if let Some(name) = extract_remixer(&get(&w, "title"), &s.remix_words, &s.remixer_ignore) {
            let cur_remixer = get(&w, "remixer");
            if cur_remixer.is_empty() || s.overwrite_remixer {
                put(&mut w, "remixer", name);
            }
        }
    }
    if s.title_case {
        for f in &s.case_fields {
            let v = title_case(&get(&w, f), &s.keep_upper, &s.small_words);
            edit(&mut w, f, v);
        }
    }
    if s.genre_map_on {
        if let Some(to) = lookup(&s.genre_map, &get(&w, "genre")) {
            put(&mut w, "genre", to.to_string());
        }
    }
    if s.artist_map_on {
        for f in ["artist", "album_artist"] {
            let v = apply_artist_map(&get(&w, f), &s.artist_map, &s.artist_separators);
            edit(&mut w, f, v);
        }
    }
    if s.trim {
        for f in TEXT_FIELDS {
            let v = tidy_field(f, &get(&w, f));
            put(&mut w, f, v);
        }
    }
    w
}

// ---- preview and apply ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub field: String,
    pub old: String,
    pub new: String,
    /// Why this change cannot be applied (the result would not fit the device's column).
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub track_id: i64,
    pub artist: String,
    pub title: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preview {
    /// Tracks looked at.
    pub scanned: usize,
    /// Only the tracks something would change on.
    pub rows: Vec<Row>,
}

/// Why a change cannot be applied, if it cannot: too long for the column, a NUL, or a title or
/// artist that would be left empty (a recipe that wipes a name is a mistake in the recipe).
fn problem_of(field: &str, old: &str, new: &str) -> Option<String> {
    if matches!(field, "title" | "artist") && new.trim().is_empty() && !old.trim().is_empty() {
        return Some(format!("{field} would be left empty"));
    }
    if new.chars().count() > MAX_TEXT_CHARS {
        return Some(format!("{field} would be longer than {MAX_TEXT_CHARS} characters"));
    }
    if new.contains('\0') {
        return Some(format!("{field} would contain a NUL character"));
    }
    None
}

/// What the recipes would change on these tracks.
pub fn preview(details: &[TrackDetail], s: &CleanupSettings) -> Preview {
    let mut rows = Vec::new();
    for d in details {
        let cur = fields_of(d);
        let new = clean_fields(&cur, s);
        let mut changes = Vec::new();
        for f in TEXT_FIELDS {
            let (old, now) = (&cur[f], &new[f]);
            if old != now {
                changes.push(Change { field: f.to_string(), old: old.clone(), new: now.clone(), problem: problem_of(f, old, now) });
            }
        }
        if !changes.is_empty() {
            rows.push(Row { track_id: d.track_id, artist: d.artist.clone(), title: d.title.clone(), changes });
        }
    }
    Preview { scanned: details.len(), rows }
}

/// One change the user left ticked in the preview.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Accepted {
    pub track_id: i64,
    pub field: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Verified {
    /// Accepted changes to write: `(track_id, field, new)`.
    pub ok: Vec<(i64, String, String)>,
    /// The library or the recipes no longer give what the preview showed.
    pub stale: usize,
    /// Changes the device column cannot hold.
    pub rejected: usize,
}

/// Re-runs the recipes on the current library values and keeps only the accepted changes that
/// still read exactly as previewed (same old value, same new value, one that fits).
pub fn verify_accepted(details: &[TrackDetail], s: &CleanupSettings, accepted: &[Accepted]) -> Verified {
    let now: HashMap<(i64, String), Change> = preview(details, s)
        .rows
        .into_iter()
        .flat_map(|r| r.changes.into_iter().map(move |c| ((r.track_id, c.field.clone()), c)))
        .collect();
    let mut out = Verified::default();
    for a in accepted {
        match now.get(&(a.track_id, a.field.clone())) {
            Some(c) if c.old == a.old && c.new == a.new => {
                if c.problem.is_some() {
                    out.rejected += 1;
                } else {
                    out.ok.push((a.track_id, a.field.clone(), a.new.clone()));
                }
            }
            _ => out.stale += 1,
        }
    }
    out
}

// ---- saved settings ------------------------------------------------------------------------

const FILE_NAME: &str = "metadata-cleanup.json";

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The saved settings, or the defaults plus a note when the file is unreadable (it is left alone,
/// so nothing is lost; the next save replaces it).
pub fn load_settings(dir: &Path) -> (CleanupSettings, Option<String>) {
    let p = settings_path(dir);
    match std::fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<CleanupSettings>(&text) {
            Ok(s) => (s.sanitised(), None),
            Err(e) => (CleanupSettings::default(), Some(format!("{} is not readable ({e}); using the defaults", p.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (CleanupSettings::default(), None),
        Err(e) => (CleanupSettings::default(), Some(format!("{}: {e}; using the defaults", p.display()))),
    }
}

/// Saves temp + fsync + rename so a crash never leaves half a file.
pub fn save_settings(dir: &Path, s: CleanupSettings) -> Result<CleanupSettings, String> {
    use std::io::Write;
    let s = s.sanitised();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = settings_path(dir);
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &p)
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(s)
}

// ---- commands ------------------------------------------------------------------------------

use pm_formats::library::Library;
use pm_formats::ops_metadata::TrackEdit;
use tauri::Manager;

use crate::error::AppResult;

/// The rows of the tracks that still exist (an id removed since the caller looked is skipped, not an error).
fn details_of(lib: &Library, ids: &[i64]) -> Vec<TrackDetail> {
    ids.iter().filter_map(|id| lib.track_details(&[*id]).ok().and_then(|mut v| v.pop())).collect()
}

fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

#[derive(Serialize)]
pub struct SettingsReply {
    pub settings: CleanupSettings,
    /// Set when the saved file could not be read and the defaults are shown instead.
    pub warning: Option<String>,
}

/// The saved clean-up recipes and maps (defaults when nothing was saved).
#[tauri::command]
pub async fn metadata_cleanup_settings(app: tauri::AppHandle) -> AppResult<SettingsReply> {
    let dir = config_dir(&app)?;
    let (settings, warning) = tauri::async_runtime::spawn_blocking(move || load_settings(&dir)).await?;
    Ok(SettingsReply { settings, warning })
}

/// The built-in recipes and maps (what "Reset to defaults" puts back).
#[tauri::command]
pub async fn metadata_cleanup_defaults() -> CleanupSettings {
    CleanupSettings::default()
}

/// Saves the recipes and maps; returns them as stored (unusable entries dropped).
#[tauri::command]
pub async fn metadata_cleanup_save_settings(app: tauri::AppHandle, settings: CleanupSettings) -> AppResult<CleanupSettings> {
    let dir = config_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || save_settings(&dir, settings)).await??)
}

/// What the recipes would change on these tracks, old and new per field. Read-only.
#[tauri::command]
pub async fn metadata_cleanup_preview(root: String, track_ids: Vec<i64>, settings: CleanupSettings) -> AppResult<Preview> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Preview> {
        let lib = Library::open_read_only(Path::new(&root))?;
        let details = details_of(&lib, &track_ids);
        Ok(preview(&details, &settings.sanitised()))
    })
    .await?
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ApplyResult {
    pub tracks: usize,
    pub fields: usize,
    /// Accepted changes dropped because the library or the recipes no longer give the preview.
    pub stale: usize,
    /// Accepted changes dropped because the result does not fit the device's column.
    pub rejected: usize,
}

fn set_field(e: &mut TrackEdit, field: &str, v: String) {
    let slot = match field {
        "title" => &mut e.title,
        "artist" => &mut e.artist,
        "album_artist" => &mut e.album_artist,
        "album" => &mut e.album,
        "composer" => &mut e.composer,
        "genre" => &mut e.genre,
        "label" => &mut e.label,
        "remixer" => &mut e.remixer,
        "producer" => &mut e.producer,
        "comments" => &mut e.comments,
        _ => return,
    };
    *slot = Some(v);
}

/// Applies the ticked changes of a preview to music.db, in one transaction, through the write
/// gate (snapshot first). The recipes run again on the library as it is now; a change is written
/// only if it still reads exactly as previewed.
#[tauri::command]
pub async fn metadata_cleanup_apply(
    app: tauri::AppHandle,
    root: String,
    settings: CleanupSettings,
    accepted: Vec<Accepted>,
) -> AppResult<ApplyResult> {
    let settings = settings.sanitised();
    let handle = app.clone();
    crate::state::quick_write(app, root.clone(), move || -> AppResult<ApplyResult> {
        let mut lib = Library::open(Path::new(&root))?;
        // recorded for undo like every other tag edit (device area: commands/device/undo.rs)
        Ok(apply_with(&mut lib, &settings, &accepted, |lib, list| {
            let ids: Vec<i64> = list.iter().map(|(id, _)| *id).collect();
            crate::commands::device::undo::tracked_edit(&handle, &root, lib, &ids, |l| l.edit_tracks(list))
        })?)
    })
    .await
}

/// The library side of [`metadata_cleanup_apply`]: verify, then one transaction.
#[cfg(test)]
fn apply_in(lib: &mut Library, settings: &CleanupSettings, accepted: &[Accepted]) -> Result<ApplyResult, pm_formats::library::Error> {
    apply_with(lib, settings, accepted, |lib, list| lib.edit_tracks(list))
}

/// [`apply_in`] with the write step supplied, so the command can run it through the undo history.
fn apply_with(
    lib: &mut Library,
    settings: &CleanupSettings,
    accepted: &[Accepted],
    write: impl FnOnce(&mut Library, &[(i64, TrackEdit)]) -> Result<usize, pm_formats::library::Error>,
) -> Result<ApplyResult, pm_formats::library::Error> {
    let mut ids: Vec<i64> = accepted.iter().map(|a| a.track_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let details = details_of(lib, &ids);
    let v = verify_accepted(&details, settings, accepted);
    let mut edits: BTreeMap<i64, TrackEdit> = BTreeMap::new();
    for (id, field, new) in &v.ok {
        set_field(edits.entry(*id).or_default(), field, new.clone());
    }
    let list: Vec<(i64, TrackEdit)> = edits.into_iter().collect();
    let tracks = if list.is_empty() { 0 } else { write(lib, &list)? };
    Ok(ApplyResult { tracks, fields: v.ok.len(), stale: v.stale, rejected: v.rejected })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        strings(items)
    }

    #[test]
    fn whitespace_is_trimmed_and_collapsed() {
        assert_eq!(collapse_ws("  A \t B\u{a0}\u{a0}C \n"), "A B C");
        assert_eq!(collapse_ws("zero\u{200b}width"), "zerowidth");
        assert_eq!(collapse_ws("\u{feff}bom"), "bom");
        assert_eq!(collapse_ws("   "), "");
    }

    #[test]
    fn joiners_are_text_and_comments_keep_their_lines() {
        // Persian half-space and emoji joiner sequences are not decoration
        assert_eq!(collapse_ws("می\u{200c}خواهم"), "می\u{200c}خواهم");
        assert_eq!(collapse_ws("\u{1f469}\u{200d}\u{1f4bb}"), "\u{1f469}\u{200d}\u{1f4bb}");
        assert_eq!(tidy_field("title", "Y\u{200c}Z \u{200d}"), "Y\u{200c}Z \u{200d}");
        // a comment keeps its line breaks; only the ends and the runs inside a line are tidied
        assert_eq!(tidy_field("comments", "Line one\nLine two\n\nRipped by me"), "Line one\nLine two\n\nRipped by me");
        assert_eq!(tidy_field("comments", "\n\n  Line  one \r\n Line two\r\n\n"), "Line one\nLine two");
        assert_eq!(tidy_field("comments", "  one  line "), "one line");
        assert_eq!(tidy_field("comments", " \n \n"), "");
        assert_eq!(tidy_field("title", "Two\nLines"), "Two Lines", "other fields are one line");
    }

    #[test]
    fn track_numbers() {
        assert_eq!(strip_track_number("01 - Title", false, 3), "Title");
        assert_eq!(strip_track_number("01. Title", false, 3), "Title");
        assert_eq!(strip_track_number("01_Title", false, 3), "Title");
        assert_eq!(strip_track_number("1) Title", false, 3), "Title");
        assert_eq!(strip_track_number("12-Title", false, 3), "Title");
        assert_eq!(strip_track_number("2-Step", false, 3), "2-Step", "one digit glued to a word is not a number");
        assert_eq!(strip_track_number("1.5 Hours", false, 3), "1.5 Hours");
        assert_eq!(strip_track_number("24-7", false, 3), "24-7");
        assert_eq!(strip_track_number("1999", false, 3), "1999");
        assert_eq!(strip_track_number("01 - 02", false, 3), "01 - 02", "nothing but numbers stays");
        assert_eq!(strip_track_number("99 Problems", false, 3), "99 Problems");
        assert_eq!(strip_track_number("01 Title", true, 3), "Title");
        assert_eq!(strip_track_number("2020 Vision", true, 3), "2020 Vision");
        assert_eq!(strip_track_number("Title 01", false, 3), "Title 01");
        assert_eq!(strip_track_number("01 - ", false, 3), "01 - ");
    }

    #[test]
    fn ordinary_words_with_dots_are_not_web_addresses() {
        let p = CleanupSettings::default().promo_phrases;
        for title in ["Dance.With.Me", "Hello.It", "Rock.It Up", "Song feat.Me", "Big Beat.Net", "Say.No", "Mr.Me", "A.B.C", "St.Louis Blues", "Vol.2 Mix", "Go.To Hell", "Hey.Us All", "Beat.Co Remix"] {
            assert_eq!(strip_promo(title, &p), title, "{title}");
        }
        // lower case addresses still go, bare or with a path or www.
        assert_eq!(strip_promo("Song djsite.com", &p), "Song");
        assert_eq!(strip_promo("Song - radio.fm", &p), "Song");
        assert_eq!(strip_promo("Song music.example.com/page", &p), "Song");
        assert_eq!(strip_promo("Song www.Example.Com", &p), "Song");
        assert_eq!(strip_promo("Song example.com.", &p), "Song");
        // a word-like domain counts only as the whole content of a bracket
        assert_eq!(strip_promo("Song [djsite.me]", &p), "Song");
        assert_eq!(strip_promo("Song djsite.me", &p), "Song djsite.me");
        assert_eq!(strip_promo("Song (rock.it remix)", &p), "Song (rock.it remix)");
        // more than one dot needs a path
        assert_eq!(strip_promo("Song dance.with.me", &p), "Song dance.with.me");
    }

    #[test]
    fn a_change_that_would_empty_a_title_or_artist_is_blocked() {
        let mut st = CleanupSettings::none();
        st.strip_promo = true;
        let ds = vec![detail(1, "www.example.com", "Artist"), detail(2, "Plain", "[www.example.org]"), detail(3, "  ", "x")];
        let pv = preview(&ds, &st);
        let problem = |id: i64, f: &str| pv.rows.iter().find(|r| r.track_id == id).unwrap().changes.iter().find(|c| c.field == f).unwrap().problem.clone();
        assert_eq!(problem(1, "title").as_deref(), Some("title would be left empty"));
        assert_eq!(problem(2, "artist").as_deref(), Some("artist would be left empty"));
        // a title that already was blank is not "emptied" by tidying it
        assert_eq!(problem_of("title", "  ", ""), None);
        assert_eq!(problem_of("album", "Old", ""), None, "other fields may be cleared");
    }

    #[test]
    fn promo_in_a_comment_is_removed_line_by_line() {
        let p = CleanupSettings::default().promo_phrases;
        assert_eq!(strip_promo_field("comments", "Ripped by me\nhttp://example.com/x\nGood one [Free DL]", &p), "Ripped by me\nGood one");
        assert_eq!(strip_promo_field("comments", "Plain\nlines", &p), "Plain\nlines");
        assert_eq!(strip_promo_field("title", "A (Free Download)", &p), "A");
    }

    #[test]
    fn urls_and_promo() {
        let p = CleanupSettings::default().promo_phrases;
        assert_eq!(strip_promo("Song [www.example.com]", &p), "Song");
        assert_eq!(strip_promo("Song (Free Download)", &p), "Song");
        assert_eq!(strip_promo("Song (Original Mix) [Free DL]", &p), "Song (Original Mix)");
        assert_eq!(strip_promo("Song - www.example.com", &p), "Song");
        assert_eq!(strip_promo("Song http://example.net/x?y=1 live", &p), "Song live");
        assert_eq!(strip_promo("Song (Premiere)", &p), "Song");
        assert_eq!(strip_promo("Song (Radio Edit)", &p), "Song (Radio Edit)");
        // promo words beside a mix credit are not promo
        for title in [
            "Song (Exclusive Remix)", "Song (Premiere Edit)", "Song (Club Exclusive Mix)", "Song (Download Mix)",
            "Song (Beatport Edit)", "Song (Buy Me A Drink)", "Song (Free Download Mix)", "Song [SoundCloud Edit]",
        ] {
            assert_eq!(strip_promo(title, &p), title, "{title}");
        }
        assert_eq!(strip_promo("Song (Free Download) [Out Now]", &p), "Song");
        assert_eq!(strip_promo("Song (Buy Now Exclusive)", &p), "Song", "several phrases, still nothing else");
        assert_eq!(strip_promo("Song (HD)", &p), "Song");
        assert_eq!(strip_promo("Song (feat. Mr.Me)", &p), "Song (feat. Mr.Me)", "short labels are not domains");
        assert_eq!(strip_promo("Song (Buyer's Remix)", &p), "Song (Buyer's Remix)", "whole words only");
        assert_eq!(strip_promo("Song (Mix [www.example.org])", &p), "Song (Mix)", "nested promo goes, the outer bracket stays");
        assert_eq!(strip_promo("Song (Mix [Free DL])", &p), "Song (Mix)");
        assert_eq!(strip_promo("Song (Free Download [www.example.org])", &p), "Song", "nothing but promo: the bracket goes");
        assert_eq!(strip_promo("Song ((unclosed", &p), "Song ((unclosed");
        assert_eq!(strip_promo("Plain", &p), "Plain");
    }

    #[test]
    fn feat_forms() {
        assert_eq!(normalise_feat("Song ft. Name", "feat."), "Song feat. Name");
        assert_eq!(normalise_feat("Song (Featuring Name)", "feat."), "Song (feat. Name)");
        assert_eq!(normalise_feat("Song [FEAT Name]", "feat."), "Song [feat. Name]");
        assert_eq!(normalise_feat("Song feat. Name", "ft."), "Song ft. Name");
        assert_eq!(normalise_feat("Song feat.Name", "feat."), "Song feat. Name");
        assert_eq!(normalise_feat("Soft Landing", "feat."), "Soft Landing");
        assert_eq!(normalise_feat("Ft. Worth", "feat."), "Ft. Worth", "never at the start of the text");
        assert_eq!(normalise_feat("Song feat", "feat."), "Song feat", "nothing follows");
        assert_eq!(normalise_feat("A ft Name and B feat. C", "featuring"), "A featuring Name and B featuring C");
    }

    #[test]
    fn feat_moves_to_artist() {
        assert_eq!(split_feat("Song (feat. Name)"), Some(("Song".into(), "Name".into())));
        assert_eq!(split_feat("Song [ft. A & B] (Radio Edit)"), Some(("Song (Radio Edit)".into(), "A & B".into())));
        assert_eq!(split_feat("Song feat. Name"), Some(("Song".into(), "Name".into())));
        assert_eq!(split_feat("Song feat. Name (Radio Edit)"), Some(("Song (Radio Edit)".into(), "Name".into())));
        assert_eq!(split_feat("Song (Radio Edit)"), None);
        assert_eq!(split_feat("Song (Name Remix)"), None);
        assert_eq!(split_feat("Song feat."), None);
        // inside a bracket that holds other words the title is left alone, never cut mid-bracket
        assert_eq!(split_feat("Song (Foo feat. Bar Remix)"), None);
        assert_eq!(split_feat("Song [Artist feat. X]"), None);
        assert_eq!(split_feat("Song (Foo (feat. X))"), None);
        assert_eq!(split_feat("Song (Foo feat. X) feat. Y"), Some(("Song (Foo feat. X)".into(), "Y".into())));
    }

    #[test]
    fn remixers() {
        let d = CleanupSettings::default();
        let r = |t: &str| extract_remixer(t, &d.remix_words, &d.remixer_ignore);
        assert_eq!(r("Song (Name Remix)"), Some("Name".into()));
        assert_eq!(r("Song [Name Edit]"), Some("Name".into()));
        assert_eq!(r("Song (A & B Remix)"), Some("A & B".into()));
        assert_eq!(r("Song (Name's Remix)"), Some("Name".into()));
        assert_eq!(r("Song (Name Extended Remix)"), Some("Name".into()));
        assert_eq!(r("Song (Name VIP Remix)"), Some("Name".into()));
        assert_eq!(r("Song (Radio Edit)"), None);
        assert_eq!(r("Song (Extended Edit)"), None);
        assert_eq!(r("Song (Original Mix)"), None);
        assert_eq!(r("Song (Remix)"), None);
        assert_eq!(r("Song (Live) (Name Rework)"), Some("Name".into()), "later brackets are looked at too");
        assert_eq!(r("Song"), None);
    }

    #[test]
    fn title_casing() {
        let k = CleanupSettings::default().keep_upper;
        let sw = CleanupSettings::default().small_words;
        assert_eq!(title_case("the art of war", &k, &sw), "The Art of War");
        assert_eq!(title_case("A SONG FOR THE DJ", &k, &sw), "A Song for the DJ");
        assert_eq!(title_case("tech-house anthem", &k, &sw), "Tech-House Anthem");
        assert_eq!(title_case("don't stop (club mix)", &k, &sw), "Don't Stop (Club Mix)");
        assert_eq!(title_case("song feat. name", &k, &sw), "Song feat. Name");
        assert_eq!(title_case("Song Ft. Name", &k, &sw), "Song ft. Name");
        assert_eq!(title_case("McFly and iPhone", &k, &sw), "McFly and iPhone", "mixed case words are left");
        assert_eq!(title_case("Live in the UK", &k, &sw), "Live in the UK", "an acronym in a mixed text stays");
        assert_eq!(title_case("live in the uk", &k, &sw), "Live in the UK");
        assert_eq!(title_case("dj's choice", &k, &sw), "DJ's Choice");
        assert_eq!(title_case("one: the beginning", &k, &sw), "One: The Beginning");
        assert_eq!(title_case("song - the remix", &k, &sw), "Song - The Remix");
        assert_eq!(title_case("1st of 3", &k, &sw), "1st of 3");
        assert_eq!(title_case("something to", &k, &sw), "Something To", "the last word is always capitalised");
        assert_eq!(title_case("", &k, &sw), "");
        assert_eq!(title_case("école de la nuit", &k, &sw), "École De La Nuit", "non-ASCII letters");
        assert_eq!(title_case("ÉCOLE DE LA NUIT", &k, &sw), "École De La Nuit");
        assert_eq!(title_case("rockin'", &k, &sw), "Rockin'");
        assert_eq!(title_case("I LOVE IT", &[], &sw), "I Love It");
    }

    #[test]
    fn maps() {
        let g = map(&[("Tech-House", "Tech House"), ("dnb", "Drum & Bass")]);
        assert_eq!(lookup(&g, " tech-house "), Some("Tech House"));
        assert_eq!(lookup(&g, "DNB"), Some("Drum & Bass"));
        assert_eq!(lookup(&g, "House"), None);
        let sep = CleanupSettings::default().artist_separators;
        let a = map(&[("Old Name", "New Name"), ("Earth, Wind & Fire", "Earth Wind & Fire")]);
        assert_eq!(apply_artist_map("old name", &a, &sep), "New Name");
        assert_eq!(apply_artist_map("Old Name & Other", &a, &sep), "New Name & Other");
        assert_eq!(apply_artist_map("Other feat. Old Name", &a, &sep), "Other feat. New Name");
        assert_eq!(apply_artist_map("Earth, Wind & Fire", &a, &sep), "Earth Wind & Fire", "the whole field wins over its parts");
        assert_eq!(apply_artist_map("Nobody", &a, &sep), "Nobody");
        assert_eq!(split_artists("A & B, C ft. D", &sep), vec![
            ("A".to_string(), " & ".to_string()),
            ("B".to_string(), ", ".to_string()),
            ("C".to_string(), " ft. ".to_string()),
            ("D".to_string(), String::new()),
        ]);
    }

    fn fields(pairs: &[(&str, &str)]) -> Fields {
        let mut f: Fields = TEXT_FIELDS.iter().map(|k| (k.to_string(), String::new())).collect();
        for (k, v) in pairs {
            f.insert(k.to_string(), v.to_string());
        }
        f
    }

    #[test]
    fn pipeline_order_and_idempotence() {
        let mut st = CleanupSettings::none();
        st.trim = true;
        st.strip_number = true;
        st.strip_promo = true;
        st.feat_normalise = true;
        st.feat_to_artist = true;
        st.extract_remixer = true;
        st.title_case = true;
        st.genre_map_on = true;
        st.artist_map_on = true;
        st.artist_map = map(&[("example artist", "Example Artist")]);
        let cur = fields(&[
            ("title", "  03 -  some  song (ft. guest one) (Remixer Name Remix) [www.example.com] "),
            ("artist", "example artist"),
            ("genre", "tech-house"),
        ]);
        let new = clean_fields(&cur, &st);
        assert_eq!(new["title"], "Some Song (Remixer Name Remix)");
        assert_eq!(new["artist"], "Example Artist feat. Guest One");
        assert_eq!(new["remixer"], "Remixer Name");
        assert_eq!(new["genre"], "Tech House");
        // a second run on the result changes nothing
        let again = clean_fields(&new, &st);
        assert_eq!(again, new);
    }

    #[test]
    fn existing_remixer_is_kept_unless_overwrite() {
        let mut st = CleanupSettings::none();
        st.extract_remixer = true;
        let cur = fields(&[("title", "Song (New Name Remix)"), ("remixer", "Old Name")]);
        assert_eq!(clean_fields(&cur, &st)["remixer"], "Old Name");
        st.overwrite_remixer = true;
        assert_eq!(clean_fields(&cur, &st)["remixer"], "New Name");
    }

    #[test]
    fn small_words_and_separators_are_settings() {
        let mut st = CleanupSettings::none();
        st.title_case = true;
        let cur = fields(&[("title", "the art of war")]);
        assert_eq!(clean_fields(&cur, &st)["title"], "The Art of War");
        st.small_words = s(&["the", "art", "OF"]);
        assert_eq!(clean_fields(&cur, &st)["title"], "The art of War", "the list is the user's, matched ignoring case");
        st.small_words.clear();
        assert_eq!(clean_fields(&cur, &st)["title"], "The Art Of War");
        // artist separators: a name next to a custom separator is replaced on its own
        let mut st = CleanupSettings::none();
        st.artist_map_on = true;
        st.artist_map = map(&[("Old Name", "New Name")]);
        let cur = fields(&[("artist", "Other + Old Name")]);
        assert_eq!(clean_fields(&cur, &st)["artist"], "Other + Old Name", "'+' is not a separator yet");
        st.artist_separators.push("+".into());
        assert_eq!(clean_fields(&cur, &st)["artist"], "Other + New Name");
        // the number of digits is a setting too
        let mut st = CleanupSettings::none();
        st.strip_number = true;
        let cur = fields(&[("title", "2020 - Vision")]);
        assert_eq!(clean_fields(&cur, &st)["title"], "2020 - Vision");
        st.number_max_digits = 4;
        assert_eq!(clean_fields(&cur, &st)["title"], "Vision");
        let saved = CleanupSettings { number_max_digits: 99, artist_separators: s(&["", " + "]), small_words: s(&[" of "]), ..CleanupSettings::default() }.sanitised();
        assert_eq!((saved.number_max_digits, saved.artist_separators, saved.small_words), (6, s(&["+"]), s(&["of"])));
    }

    #[test]
    fn nothing_ticked_changes_nothing() {
        let st = CleanupSettings::none();
        let cur = fields(&[("title", "  01 - messy   [www.example.com] "), ("genre", "tech-house")]);
        assert_eq!(clean_fields(&cur, &st), cur);
    }

    fn detail(id: i64, title: &str, artist: &str) -> TrackDetail {
        TrackDetail {
            track_id: id,
            title: title.into(),
            artist: artist.into(),
            album_artist: String::new(),
            album: String::new(),
            composer: String::new(),
            genre: String::new(),
            label: String::new(),
            remixer: String::new(),
            producer: String::new(),
            comments: String::new(),
            year: 0,
            track_number: 0,
            number_of_tracks: 0,
            disc_number: 0,
            number_of_discs: 0,
            rating: 0,
            bpm: 0.0,
            key: String::new(),
        }
    }

    #[test]
    fn preview_lists_only_changed_tracks_and_flags_overlong_results() {
        let mut st = CleanupSettings::none();
        st.feat_normalise = true;
        st.feat_form = "featuring".into();
        let long_artist = "a".repeat(250);
        let ds = vec![detail(1, "Plain", "Artist"), detail(2, "Song ft. Name", &long_artist), detail(3, "Song ft. X", "Artist ft. Y")];
        let p = preview(&ds, &st);
        assert_eq!(p.scanned, 3);
        assert_eq!(p.rows.iter().map(|r| r.track_id).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(p.rows[0].changes.len(), 1);
        assert_eq!(p.rows[1].changes.len(), 2);
        assert!(p.rows.iter().flat_map(|r| &r.changes).all(|c| c.problem.is_none()));
        // artist 250 chars + " ft. Name" would be fine; push one over the limit
        st.feat_to_artist = true;
        let p = preview(&ds, &st);
        let c = p.rows.iter().find(|r| r.track_id == 2).unwrap().changes.iter().find(|c| c.field == "artist").unwrap();
        assert!(c.problem.as_deref().unwrap().contains("255"), "{c:?}");
    }

    #[test]
    fn accepted_changes_are_checked_against_the_current_library() {
        let mut st = CleanupSettings::none();
        st.trim = true;
        let ds = vec![detail(1, " A ", "x"), detail(2, "B  b", "y")];
        let acc = |id, field: &str, old: &str, new: &str| Accepted { track_id: id, field: field.into(), old: old.into(), new: new.into() };
        let v = verify_accepted(
            &ds,
            &st,
            &[
                acc(1, "title", " A ", "A"),      // as previewed
                acc(2, "title", "B  b", "B b"),   // as previewed
                acc(2, "title", "B  b", "Evil"),  // the new value is not what the recipes produce
                acc(1, "artist", "x", "X"),       // nothing to change there
                acc(9, "title", "z", "Z"),        // not in the scanned tracks
            ],
        );
        assert_eq!(v.ok, vec![(1, "title".to_string(), "A".to_string()), (2, "title".to_string(), "B b".to_string())]);
        assert_eq!(v.stale, 3);
        assert_eq!(v.rejected, 0);
        // the library changed after the preview: the old value no longer matches
        let moved = vec![detail(1, "A", "x")];
        let v = verify_accepted(&moved, &st, &[acc(1, "title", " A ", "A")]);
        assert_eq!((v.ok.len(), v.stale), (0, 1));
    }

    #[test]
    fn settings_round_trip_and_survive_a_corrupt_file() {
        let dir = std::env::temp_dir().join(format!("pm-cleanup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (d, warn) = load_settings(&dir);
        assert_eq!((d, warn), (CleanupSettings::default(), None));
        let mut st = CleanupSettings::default();
        st.title_case = true;
        st.feat_form = "bogus".into();
        st.case_fields = s(&["title", "nonsense", "title"]);
        st.genre_map = map(&[("  A  ", " B "), ("", "x")]);
        let saved = save_settings(&dir, st).unwrap();
        assert_eq!(saved.feat_form, "feat.");
        assert_eq!(saved.case_fields, s(&["title"]));
        assert_eq!(saved.genre_map, map(&[("A", "B")]));
        assert!(saved.title_case);
        assert!(!saved.write_back, "write-back stays off unless asked for");
        let (loaded, warn) = load_settings(&dir);
        assert_eq!((loaded, warn), (saved, None));
        assert!(!dir.join("metadata-cleanup.json.tmp").exists());
        std::fs::write(settings_path(&dir), "not json").unwrap();
        let (d, warn) = load_settings(&dir);
        assert_eq!(d, CleanupSettings::default());
        assert!(warn.unwrap().contains("not readable"));
        // a partial file from an older version fills the rest with defaults
        std::fs::write(settings_path(&dir), r#"{"title_case":true}"#).unwrap();
        let (d, _) = load_settings(&dir);
        assert!(d.title_case && d.trim && !d.write_back);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod golden {
    use super::*;

    const GOLDEN: &str = include_str!("../../../../src/lib/devmock/cleanupGolden.json");

    #[derive(Deserialize)]
    struct File {
        defaults: CleanupSettings,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        /// Overrides on top of [`CleanupSettings::none`].
        #[serde(default)]
        settings: serde_json::Map<String, serde_json::Value>,
        input: BTreeMap<String, String>,
        /// Only the fields that change.
        expect: BTreeMap<String, String>,
    }

    fn fields_from(input: &BTreeMap<String, String>) -> Fields {
        let mut f: Fields = TEXT_FIELDS.iter().map(|k| (k.to_string(), String::new())).collect();
        for (k, v) in input {
            f.insert(k.clone(), v.clone());
        }
        f
    }

    /// The same table runs against the TypeScript port (`views/metadata/cleanup.test.mjs`).
    #[test]
    fn golden_cases() {
        let file: File = serde_json::from_str(GOLDEN).unwrap();
        assert_eq!(file.defaults, CleanupSettings::default(), "cleanupGolden.json `defaults` is the Rust default");
        assert!(file.cases.len() >= 30);
        for c in file.cases {
            let mut base = serde_json::to_value(CleanupSettings::none()).unwrap();
            base.as_object_mut().unwrap().extend(c.settings.clone());
            let st: CleanupSettings = serde_json::from_value(base).unwrap();
            let cur = fields_from(&c.input);
            let new = clean_fields(&cur, &st);
            let changed: BTreeMap<String, String> =
                TEXT_FIELDS.iter().filter(|f| cur[**f] != new[**f]).map(|f| (f.to_string(), new[*f].clone())).collect();
            assert_eq!(changed, c.expect, "case: {}", c.name);
        }
    }

    /// Prints the default settings as JSON, to refresh the `defaults` of the golden file.
    #[test]
    #[ignore]
    fn dump_defaults() {
        println!("{}", serde_json::to_string_pretty(&CleanupSettings::default()).unwrap());
    }

    #[test]
    fn apply_writes_only_verified_changes_through_the_library() {
        use pm_formats::library::TrackMeta;
        use pm_formats::strfile;
        let base = std::env::temp_dir().join(format!("pm-cleanup-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let analysis = strfile::Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| strfile::Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, (title, genre)) in [("01 - First  Song [www.example.com]", "tech-house"), ("Second Song", "House")].iter().enumerate() {
            let p = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&p, vec![0u8; 800 + i]).unwrap();
            let meta = TrackMeta { title: (*title).into(), artist: "Example Artist".into(), genre: (*genre).into(), format: "MP3".into(), play_time_secs: 5, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &analysis).unwrap());
        }
        let mut st = CleanupSettings::none();
        st.trim = true;
        st.strip_number = true;
        st.strip_promo = true;
        st.genre_map_on = true;
        let pv = preview(&lib.track_details(&ids).unwrap(), &st);
        assert_eq!(pv.rows.len(), 1);
        let accepted: Vec<Accepted> = pv
            .rows
            .iter()
            .flat_map(|r| r.changes.iter().map(move |c| Accepted { track_id: r.track_id, field: c.field.clone(), old: c.old.clone(), new: c.new.clone() }))
            .collect();
        assert_eq!(accepted.len(), 2, "title and genre of the first track");
        // the user unticks the genre row
        let only_title: Vec<Accepted> = accepted.iter().filter(|a| a.field == "title").cloned().collect();
        let r = apply_in(&mut lib, &st, &only_title).unwrap();
        assert_eq!(r, ApplyResult { tracks: 1, fields: 1, stale: 0, rejected: 0 });
        let d = lib.track_details(&ids).unwrap();
        assert_eq!(d[0].title, "First Song");
        assert_eq!(d[0].genre, "tech-house", "the unticked field is not written");
        assert_eq!(d[1].title, "Second Song");
        // applying the same preview again: the title is already clean, so it is stale, not rewritten
        let r = apply_in(&mut lib, &st, &accepted).unwrap();
        assert_eq!((r.tracks, r.fields, r.stale), (1, 1, 1), "{r:?}");
        assert_eq!(lib.track_details(&ids).unwrap()[0].genre, "Tech House");
        drop(lib);
        let _ = std::fs::remove_dir_all(&base);
    }
}
