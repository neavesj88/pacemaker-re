//! The label store: free-text tags, one colour label and an energy rating, kept by the app and
//! never written to the device (music.db has no column for them).
//!
//! One JSON file per library volume in the app config dir, written atomically (temp, fsync,
//! rename). The store is generic over the kind of thing labelled (`track`, `case`, `mix`) and a
//! stable string key per thing; only tracks have a resolver today (see `tags.rs`).
//!
//! Everything in this file is pure (no Tauri, no music.db) so it is unit tested directly. The TS
//! mirror of [`apply_ops`] lives in `app/src/lib/state/labels.ts`; both are tested against the
//! same vectors in `app/src/lib/state/labels.parity.json`.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The eight colour labels, in palette order. Names, not hex: the UI owns the actual colours
/// (`--label-<name>` in tokens.css).
pub const COLOURS: [&str; 8] = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];
pub const MIN_ENERGY: u8 = 1;
pub const MAX_ENERGY: u8 = 10;
/// Longest tag, in characters.
///
/// `MAX_TAG_CHARS` and `MAX_TAGS` are validation bounds of the file format, not user policy: they
/// are mirrored in `labels.ts` and pinned by the shared parity vectors, and a per-user setting
/// would let one install write tags another install's rules reject. They are generous (a tag is a
/// few words, 32 per track is far beyond what anyone scans) and defined once per language.
pub const MAX_TAG_CHARS: usize = 40;
/// Most tags one thing can carry.
pub const MAX_TAGS: usize = 32;

/// What can carry labels. The key of an entity is only unique within its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Track,
    // Cases and mixes reuse the store later (their UI is not built yet); the file format already
    // has room for them, so adding one is a resolver in tags.rs, not a format change.
    #[allow(dead_code)]
    Case,
    #[allow(dead_code)]
    Mix,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Track => "track",
            Kind::Case => "case",
            Kind::Mix => "mix",
        }
    }
}

/// The labels of one thing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labels {
    /// Sorted case-insensitively, no two equal ignoring case.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub colour: Option<String>,
    #[serde(default)]
    pub energy: Option<u8>,
}

impl Labels {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.colour.is_none() && self.energy.is_none()
    }

    /// A copy with anything invalid dropped: bad tags, an unknown colour, an energy outside 1-10.
    /// Used on everything read from disk so a hand-edited file cannot put junk on screen.
    fn sanitised(&self) -> Labels {
        let mut out = Labels {
            tags: Vec::new(),
            colour: self.colour.clone().filter(|c| COLOURS.contains(&c.as_str())),
            energy: self.energy.filter(|e| (MIN_ENERGY..=MAX_ENERGY).contains(e)),
        };
        for t in &self.tags {
            if let Ok(t) = normalise_tag(t) {
                add_tag(&mut out.tags, t);
            }
        }
        out.tags.truncate(MAX_TAGS);
        out
    }
}

/// One change to the labels of a set of things.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Adds the tags a thing does not have yet (compared ignoring case; the casing sent is kept).
    AddTags { tags: Vec<String> },
    /// Removes the tags (ignoring case); tags a thing does not have are skipped.
    RemoveTags { tags: Vec<String> },
    /// A colour name, or `None` to clear it.
    SetColour { colour: Option<String> },
    /// 1 to 10, or `None` to clear it.
    SetEnergy { energy: Option<u8> },
}

/// The key two tags are compared by.
pub fn tag_key(tag: &str) -> String {
    tag.to_lowercase()
}

/// A tag as it is stored: surrounding space trimmed, inner runs of whitespace collapsed to one
/// space. Fails (worded for the user) on an empty tag, a comma or control character (commas
/// separate tags when typed) or more than [`MAX_TAG_CHARS`] characters.
pub fn normalise_tag(raw: &str) -> Result<String, String> {
    if raw.chars().any(|c| c == ',' || (c.is_control() && !c.is_whitespace())) {
        return Err(format!("The tag \"{}\" contains a comma or a control character.", raw.trim()));
    }
    let tag = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if tag.is_empty() {
        return Err("A tag cannot be empty.".into());
    }
    if tag.chars().count() > MAX_TAG_CHARS {
        return Err(format!("The tag \"{tag}\" is longer than {MAX_TAG_CHARS} characters."));
    }
    Ok(tag)
}

fn has_tag(tags: &[String], key: &str) -> bool {
    tags.iter().any(|t| tag_key(t) == key)
}

/// Adds `tag` (already normalised) unless an equal-ignoring-case tag is there; keeps the list sorted.
fn add_tag(tags: &mut Vec<String>, tag: String) {
    if !has_tag(tags, &tag_key(&tag)) {
        tags.push(tag);
        tags.sort_by_cached_key(|t| (tag_key(t), t.clone()));
    }
}

/// The labels after `ops`, or the reason they cannot be applied. Ops run in order; nothing is
/// changed on error.
pub fn apply_ops(before: &Labels, ops: &[Op]) -> Result<Labels, String> {
    let mut l = before.clone();
    for op in ops {
        match op {
            Op::AddTags { tags } => {
                for t in tags {
                    add_tag(&mut l.tags, normalise_tag(t)?);
                }
                if l.tags.len() > MAX_TAGS {
                    return Err(format!("A track can have at most {MAX_TAGS} tags."));
                }
            }
            Op::RemoveTags { tags } => {
                let gone: HashSet<String> = tags.iter().filter_map(|t| normalise_tag(t).ok()).map(|t| tag_key(&t)).collect();
                l.tags.retain(|t| !gone.contains(&tag_key(t)));
            }
            Op::SetColour { colour } => {
                if let Some(c) = colour
                    && !COLOURS.contains(&c.as_str())
                {
                    return Err(format!("\"{c}\" is not one of the colour labels."));
                }
                l.colour = colour.clone();
            }
            Op::SetEnergy { energy } => {
                if let Some(e) = energy
                    && !(MIN_ENERGY..=MAX_ENERGY).contains(e)
                {
                    return Err(format!("Energy is {MIN_ENERGY} to {MAX_ENERGY}."));
                }
                l.energy = *energy;
            }
        }
    }
    Ok(l)
}

/// The file format version this build reads and writes. A file with a higher number was written
/// by a newer build: this one reads what it understands but never saves over it.
pub const FORMAT_VERSION: u32 = 1;

/// Everything labelled in one library: kind -> key -> labels. Empty labels are never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    entities: BTreeMap<String, BTreeMap<String, Labels>>,
}

fn version() -> u32 {
    FORMAT_VERSION
}

impl Default for Store {
    fn default() -> Self {
        Store { version: version(), entities: BTreeMap::new() }
    }
}

impl Store {
    pub fn get(&self, kind: Kind, key: &str) -> Option<&Labels> {
        self.entities.get(kind.as_str())?.get(key)
    }

    /// Every labelled thing of a kind.
    pub fn iter(&self, kind: Kind) -> impl Iterator<Item = (&String, &Labels)> {
        self.entities.get(kind.as_str()).into_iter().flat_map(|m| m.iter())
    }

    fn set(&mut self, kind: Kind, key: &str, labels: Labels) {
        let m = self.entities.entry(kind.as_str().to_string()).or_default();
        if labels.is_empty() {
            m.remove(key);
        } else {
            m.insert(key.to_string(), labels);
        }
        if m.is_empty() {
            self.entities.remove(kind.as_str());
        }
    }

    /// Applies `ops` to every key, all or nothing; returns how many things actually changed.
    pub fn apply(&mut self, kind: Kind, keys: &[String], ops: &[Op]) -> Result<usize, String> {
        let mut next = Vec::with_capacity(keys.len());
        for k in keys {
            let before = self.get(kind, k).cloned().unwrap_or_default();
            let after = apply_ops(&before, ops)?;
            next.push((k, before, after));
        }
        let mut changed = 0;
        for (k, before, after) in next {
            if before != after {
                changed += 1;
                self.set(kind, k, after);
            }
        }
        Ok(changed)
    }

    /// Renames `from` to `to` on everything of this kind (ignoring case). If a thing already has
    /// `to` the two merge into one. `to` may differ from `from` only in casing, which re-cases
    /// the tag. Returns how many things changed.
    pub fn rename_tag(&mut self, kind: Kind, from: &str, to: &str) -> Result<usize, String> {
        let to = normalise_tag(to)?;
        let from_key = tag_key(&normalise_tag(from)?);
        let mut changed = 0;
        if let Some(m) = self.entities.get_mut(kind.as_str()) {
            for l in m.values_mut() {
                if !has_tag(&l.tags, &from_key) {
                    continue;
                }
                let before = l.tags.clone();
                l.tags.retain(|t| tag_key(t) != from_key);
                // when `to` is already there (a merge) its casing wins over the one typed now
                add_tag(&mut l.tags, to.clone());
                if l.tags != before {
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }

    /// The things of this kind whose key is not in `live` (labels of tracks that are no longer on
    /// the card), in key order. The tag manager lists exactly these before they are forgotten.
    pub fn unattached(&self, kind: Kind, live: &HashSet<&str>) -> Vec<(&String, &Labels)> {
        self.iter(kind).filter(|(k, _)| !live.contains(k.as_str())).collect()
    }

    /// Drops everything of this kind whose key is not in `live`; returns how many were dropped.
    pub fn forget_unattached(&mut self, kind: Kind, live: &HashSet<&str>) -> usize {
        let gone: Vec<String> = self.unattached(kind, live).into_iter().map(|(k, _)| k.clone()).collect();
        if let Some(m) = self.entities.get_mut(kind.as_str()) {
            for k in &gone {
                m.remove(k);
            }
        }
        self.entities.retain(|_, m| !m.is_empty());
        gone.len()
    }

    /// Removes a tag (ignoring case) from everything of this kind; returns how many things
    /// lost it. Things left with no labels at all are dropped from the store.
    pub fn delete_tag(&mut self, kind: Kind, tag: &str) -> Result<usize, String> {
        let key = tag_key(&normalise_tag(tag)?);
        let mut changed = 0;
        let mut emptied = Vec::new();
        if let Some(m) = self.entities.get_mut(kind.as_str()) {
            for (k, l) in m.iter_mut() {
                if has_tag(&l.tags, &key) {
                    l.tags.retain(|t| tag_key(t) != key);
                    changed += 1;
                    if l.is_empty() {
                        emptied.push(k.clone());
                    }
                }
            }
            for k in emptied {
                m.remove(&k);
            }
        }
        Ok(changed)
    }

    /// Drops invalid labels from everything (see [`Labels::sanitised`]); empties disappear.
    fn sanitise(&mut self) {
        for m in self.entities.values_mut() {
            for l in m.values_mut() {
                *l = l.sanitised();
            }
            m.retain(|_, l| !l.is_empty());
        }
        self.entities.retain(|_, m| !m.is_empty());
        self.version = version();
    }
}

// ---- storage -----------------------------------------------------------------------------

/// The file for a library: named by a hash of its normalised root path, so nothing about the
/// device is written next to the labels. (A different card mounted at the same path shares the
/// file; its tracks have different keys, so nothing of the other card's shows up on it.)
pub fn store_path(dir: &Path, volume_key: &str) -> PathBuf {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
    for b in volume_key.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    dir.join(format!("labels-{h:016x}.json"))
}

/// What reading the file found.
pub struct Loaded {
    pub store: Store,
    /// Set when the file existed but could not be used; the store is then empty (or, for a file
    /// from a newer build, only what this build understands).
    pub notice: Option<String>,
    /// The file itself when it is not valid JSON for this format, so a writer can set it aside
    /// before replacing it. Never set for a read error or a newer file: those are not "corrupt".
    pub corrupt: Option<PathBuf>,
    /// Why a save must be refused: the file could not be read at all (a transient lock, say) or a
    /// newer build wrote it. Saving then would replace good data with a near-empty store.
    pub read_only: Option<String>,
}

/// Reads the store. A missing file is an empty store; an unreadable or corrupt one is an empty
/// store plus a notice (it is not touched here, so a read never destroys anything).
pub fn load(dir: &Path, volume_key: &str) -> Loaded {
    let p = store_path(dir, volume_key);
    let empty = |notice: Option<String>, corrupt: Option<PathBuf>, read_only: Option<String>| Loaded { store: Store::default(), notice, corrupt, read_only };
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return empty(None, None, None),
        Err(e) => {
            // an I/O error says nothing about the content: keep the file, refuse to save over it
            let why = format!("The tag file {} could not be read ({e}); tags are not available and nothing was changed. Try again.", p.display());
            return empty(Some(why.clone()), None, Some(why));
        }
    };
    let bad = |why: String| {
        empty(
            Some(format!(
                "The tag file {} could not be read ({why}); tags start empty. The file is kept as it is until you change a label, then set aside next to it.",
                p.display()
            )),
            Some(p.clone()),
            None,
        )
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return bad(e.to_string()),
    };
    let file_version = value.get("version").and_then(serde_json::Value::as_u64).unwrap_or(u64::from(FORMAT_VERSION));
    if file_version > u64::from(FORMAT_VERSION) {
        let why = format!("The tag file {} was written by a newer version of this app (format {file_version}); it is shown read-only and not changed.", p.display());
        let mut store = serde_json::from_value::<Store>(value).unwrap_or_default();
        store.sanitise();
        return Loaded { store, notice: Some(why.clone()), corrupt: None, read_only: Some(why) };
    }
    match serde_json::from_value::<Store>(value) {
        Ok(mut store) => {
            store.sanitise();
            Loaded { store, notice: None, corrupt: None, read_only: None }
        }
        Err(e) => bad(e.to_string()),
    }
}

/// Where a corrupt file is set aside: `<name>.corrupt`, or `<name>.corrupt.1`, `.2`, ... when that
/// is taken, so a second bad file never replaces the only copy of the first.
fn aside_path(bad: &Path) -> PathBuf {
    let mut base = bad.as_os_str().to_owned();
    base.push(".corrupt");
    let first = PathBuf::from(&base);
    if !first.exists() {
        return first;
    }
    (1..10_000)
        .map(|n| {
            let mut name = base.clone();
            name.push(format!(".{n}"));
            PathBuf::from(name)
        })
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Writes the store: temp file, fsync, rename. The temp file is removed on any failure.
/// A corrupt file found by [`load`] is first moved aside (see [`aside_path`]; earlier copies are
/// kept) so the data in it is still there to recover by hand. If it cannot be moved the save
/// fails rather than overwrite it.
pub fn save(dir: &Path, volume_key: &str, store: &Store, corrupt: Option<&Path>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if let Some(bad) = corrupt {
        match std::fs::rename(bad, aside_path(bad)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Could not set the unreadable tag file aside ({e}); nothing was saved.")),
        }
    }
    let p = store_path(dir, volume_key);
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    let write = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &p)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Could not save the tags: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(l: &Labels) -> Vec<&str> {
        l.tags.iter().map(String::as_str).collect()
    }
    fn add(t: &[&str]) -> Op {
        Op::AddTags { tags: t.iter().map(|s| s.to_string()).collect() }
    }
    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-tags-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn tags_are_normalised() {
        assert_eq!(normalise_tag("  warm   up ").unwrap(), "warm up");
        assert_eq!(normalise_tag("Peak").unwrap(), "Peak");
        assert!(normalise_tag("   ").is_err());
        assert!(normalise_tag("a,b").is_err());
        assert!(normalise_tag("a\u{0}b").is_err());
        assert!(normalise_tag(&"x".repeat(MAX_TAG_CHARS)).is_ok());
        assert!(normalise_tag(&"x".repeat(MAX_TAG_CHARS + 1)).is_err());
    }

    #[test]
    fn a_tab_inside_a_tag_becomes_a_space() {
        assert_eq!(normalise_tag("a\tb").unwrap(), "a b");
    }

    #[test]
    fn add_remove_ignore_case_and_stay_sorted() {
        let l = apply_ops(&Labels::default(), &[add(&["vocal", "Warm up", "Vocal", "  closer "])]).unwrap();
        assert_eq!(tags(&l), ["closer", "vocal", "Warm up"], "Vocal is the same tag as vocal");
        let l = apply_ops(&l, &[Op::RemoveTags { tags: vec!["VOCAL".into(), "never had it".into()] }]).unwrap();
        assert_eq!(tags(&l), ["closer", "Warm up"]);
    }

    #[test]
    fn colour_and_energy_are_validated() {
        let l = apply_ops(&Labels::default(), &[Op::SetColour { colour: Some("teal".into()) }, Op::SetEnergy { energy: Some(10) }]).unwrap();
        assert_eq!((l.colour.as_deref(), l.energy), (Some("teal"), Some(10)));
        assert!(apply_ops(&l, &[Op::SetColour { colour: Some("mauve".into()) }]).is_err());
        assert!(apply_ops(&l, &[Op::SetEnergy { energy: Some(0) }]).is_err());
        assert!(apply_ops(&l, &[Op::SetEnergy { energy: Some(11) }]).is_err());
        let cleared = apply_ops(&l, &[Op::SetColour { colour: None }, Op::SetEnergy { energy: None }]).unwrap();
        assert!(cleared.is_empty());
    }

    #[test]
    fn too_many_tags_is_refused() {
        let many: Vec<String> = (0..=MAX_TAGS).map(|i| format!("t{i}")).collect();
        assert!(apply_ops(&Labels::default(), &[Op::AddTags { tags: many.clone() }]).is_err());
        assert!(apply_ops(&Labels::default(), &[Op::AddTags { tags: many[..MAX_TAGS].to_vec() }]).is_ok());
    }

    #[test]
    fn apply_is_all_or_nothing_and_counts_real_changes() {
        let mut s = Store::default();
        assert_eq!(s.apply(Kind::Track, &keys(&["a", "b"]), &[add(&["x"])]).unwrap(), 2);
        assert_eq!(s.apply(Kind::Track, &keys(&["a", "b"]), &[add(&["x"])]).unwrap(), 0, "already there");
        // a bad op on the second key must leave the first alone
        let before = s.clone();
        assert!(s.apply(Kind::Track, &keys(&["a", "b"]), &[add(&["y"]), Op::SetEnergy { energy: Some(99) }]).is_err());
        assert_eq!(s, before);
        // a key that is not in the store yet is created; clearing everything removes it again
        s.apply(Kind::Track, &keys(&["c"]), &[Op::SetColour { colour: Some("red".into()) }]).unwrap();
        assert!(s.get(Kind::Track, "c").is_some());
        s.apply(Kind::Track, &keys(&["c"]), &[Op::SetColour { colour: None }]).unwrap();
        assert!(s.get(Kind::Track, "c").is_none(), "empty labels are not kept");
    }

    #[test]
    fn kinds_do_not_share_keys() {
        let mut s = Store::default();
        s.apply(Kind::Track, &keys(&["1"]), &[add(&["x"])]).unwrap();
        s.apply(Kind::Case, &keys(&["1"]), &[add(&["y"])]).unwrap();
        assert_eq!(tags(s.get(Kind::Track, "1").unwrap()), ["x"]);
        assert_eq!(tags(s.get(Kind::Case, "1").unwrap()), ["y"]);
        assert!(s.get(Kind::Mix, "1").is_none());
        s.delete_tag(Kind::Track, "x").unwrap();
        assert_eq!(tags(s.get(Kind::Case, "1").unwrap()), ["y"], "deleting a track tag leaves cases alone");
    }

    #[test]
    fn rename_merge_and_delete() {
        let mut s = Store::default();
        s.apply(Kind::Track, &keys(&["a"]), &[add(&["house", "vocal"])]).unwrap();
        s.apply(Kind::Track, &keys(&["b"]), &[add(&["House"])]).unwrap();
        s.apply(Kind::Track, &keys(&["c"]), &[add(&["Deep"]), Op::SetEnergy { energy: Some(4) }]).unwrap();
        // plain rename
        assert_eq!(s.rename_tag(Kind::Track, "vocal", "Vocals").unwrap(), 1);
        assert_eq!(tags(s.get(Kind::Track, "a").unwrap()), ["house", "Vocals"]);
        // merge into an existing tag: the target's casing wins on the things that had both
        assert_eq!(s.rename_tag(Kind::Track, "Deep", "house").unwrap(), 1);
        assert_eq!(tags(s.get(Kind::Track, "c").unwrap()), ["house"]);
        // re-case only
        assert_eq!(s.rename_tag(Kind::Track, "house", "House").unwrap(), 2, "b already said House");
        assert_eq!(tags(s.get(Kind::Track, "a").unwrap()), ["House", "Vocals"]);
        assert_eq!(tags(s.get(Kind::Track, "b").unwrap()), ["House"]);
        // a rename to a bad name changes nothing
        assert!(s.rename_tag(Kind::Track, "House", "a,b").is_err());
        assert_eq!(tags(s.get(Kind::Track, "a").unwrap()), ["House", "Vocals"]);
        // delete: a thing left with only a tag disappears, one with an energy stays
        assert_eq!(s.delete_tag(Kind::Track, "HOUSE").unwrap(), 3);
        assert!(s.get(Kind::Track, "b").is_none());
        assert_eq!(s.get(Kind::Track, "c").unwrap().energy, Some(4));
        assert_eq!(tags(s.get(Kind::Track, "a").unwrap()), ["Vocals"]);
    }

    #[test]
    fn merging_two_tags_on_one_track_leaves_one() {
        let mut s = Store::default();
        s.apply(Kind::Track, &keys(&["a"]), &[add(&["Deep", "deep house"])]).unwrap();
        assert_eq!(s.rename_tag(Kind::Track, "Deep", "deep house").unwrap(), 1);
        assert_eq!(tags(s.get(Kind::Track, "a").unwrap()), ["deep house"]);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tmp_dir("rt");
        let mut s = Store::default();
        s.apply(Kind::Track, &keys(&["g:abc"]), &[add(&["x", "y"]), Op::SetColour { colour: Some("blue".into()) }, Op::SetEnergy { energy: Some(7) }]).unwrap();
        save(&dir, "e:", &s, None).unwrap();
        let back = load(&dir, "e:");
        assert!(back.notice.is_none());
        assert_eq!(back.store, s);
        assert!(!store_path(&dir, "e:").with_extension("json.tmp").exists(), "no temp file left");
        assert_ne!(store_path(&dir, "e:"), store_path(&dir, "f:"));
        // another volume is a different, empty store
        assert_eq!(load(&dir, "f:").store, Store::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_reads_as_empty_and_is_set_aside_by_the_next_save() {
        let dir = tmp_dir("bad");
        std::fs::create_dir_all(&dir).unwrap();
        let p = store_path(&dir, "e:");
        std::fs::write(&p, "{ not json").unwrap();
        let loaded = load(&dir, "e:");
        assert!(loaded.notice.is_some());
        assert_eq!(loaded.store, Store::default());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json", "a read never touches the file");
        let mut s = loaded.store;
        s.apply(Kind::Track, &keys(&["k"]), &[add(&["x"])]).unwrap();
        save(&dir, "e:", &s, loaded.corrupt.as_deref()).unwrap();
        assert_eq!(std::fs::read_to_string(p.with_extension("json.corrupt")).unwrap(), "{ not json");
        assert!(load(&dir, "e:").notice.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn junk_inside_a_valid_file_is_dropped_on_load() {
        let dir = tmp_dir("junk");
        std::fs::create_dir_all(&dir).unwrap();
        let json = r#"{"version":1,"entities":{"track":{
            "k1":{"tags":["ok","a,b","","OK"],"colour":"mauve","energy":42},
            "k2":{"tags":[],"colour":"pink","energy":3},
            "k3":{"tags":[]}},
          "future-kind":{"z":{"tags":["t"]}}}}"#;
        std::fs::write(store_path(&dir, "e:"), json).unwrap();
        let s = load(&dir, "e:").store;
        let k1 = s.get(Kind::Track, "k1").unwrap();
        assert_eq!((tags(k1), k1.colour.as_deref(), k1.energy), (vec!["ok"], None, None));
        assert_eq!(s.get(Kind::Track, "k2").unwrap().colour.as_deref(), Some("pink"));
        assert!(s.get(Kind::Track, "k3").is_none(), "empty entries are dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_corrupt_file_does_not_replace_the_first_backup() {
        let dir = tmp_dir("bad2");
        std::fs::create_dir_all(&dir).unwrap();
        let p = store_path(&dir, "e:");
        let mut s = Store::default();
        s.apply(Kind::Track, &keys(&["k"]), &[add(&["x"])]).unwrap();
        for text in ["first bad", "second bad"] {
            std::fs::write(&p, text).unwrap();
            let l = load(&dir, "e:");
            assert!(l.corrupt.is_some() && l.read_only.is_none());
            save(&dir, "e:", &s, l.corrupt.as_deref()).unwrap();
        }
        assert_eq!(std::fs::read_to_string(p.with_extension("json.corrupt")).unwrap(), "first bad");
        assert_eq!(std::fs::read_to_string(p.with_extension("json.corrupt.1")).unwrap(), "second bad");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_error_is_not_corruption() {
        // a directory where the file should be: reading fails with an I/O error, not NotFound
        let dir = tmp_dir("ioerr");
        std::fs::create_dir_all(store_path(&dir, "e:")).unwrap();
        let l = load(&dir, "e:");
        assert!(l.notice.is_some());
        assert!(l.corrupt.is_none(), "so the writer will not rename it away");
        assert!(l.read_only.is_some(), "and the caller must refuse to save");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_format_is_read_but_never_saved_over() {
        let dir = tmp_dir("newer");
        std::fs::create_dir_all(&dir).unwrap();
        let json = r#"{"version":2,"entities":{"track":{"k":{"tags":["keep"],"future_field":1}}}}"#;
        std::fs::write(store_path(&dir, "e:"), json).unwrap();
        let l = load(&dir, "e:");
        assert!(l.read_only.is_some() && l.notice.is_some() && l.corrupt.is_none());
        assert_eq!(tags(l.store.get(Kind::Track, "k").unwrap()), ["keep"], "what this build understands is still shown");
        assert_eq!(std::fs::read_to_string(store_path(&dir, "e:")).unwrap(), json);
        // a structure this build cannot parse at all is read-only too, not "corrupt"
        std::fs::write(store_path(&dir, "e:"), r#"{"version":3,"entities":[1,2]}"#).unwrap();
        let l = load(&dir, "e:");
        assert!(l.read_only.is_some() && l.corrupt.is_none() && l.store == Store::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unattached_things_are_listed_and_forgotten_by_the_same_rule() {
        let mut s = Store::default();
        s.apply(Kind::Track, &keys(&["a", "b", "c"]), &[add(&["x"])]).unwrap();
        s.apply(Kind::Case, &keys(&["b"]), &[add(&["y"])]).unwrap();
        let live: HashSet<&str> = ["a"].into_iter().collect();
        let listed: Vec<String> = s.unattached(Kind::Track, &live).into_iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(listed, ["b", "c"]);
        assert_eq!(s.forget_unattached(Kind::Track, &live), 2);
        assert!(s.get(Kind::Track, "b").is_none() && s.get(Kind::Track, "a").is_some());
        assert!(s.get(Kind::Case, "b").is_some(), "other kinds are untouched");
        assert_eq!(s.forget_unattached(Kind::Track, &live), 0);
    }

    /// The same vectors the TS mirror (`labels.ts`) is tested against.
    #[test]
    fn parity_vectors_match_the_ts_mirror() {
        #[derive(Deserialize)]
        struct Case {
            name: String,
            before: Labels,
            ops: Vec<Op>,
            after: Option<Labels>,
        }
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../../src/lib/state/labels.parity.json")).unwrap();
        assert!(cases.len() >= 10);
        for c in cases {
            match (apply_ops(&c.before, &c.ops), c.after) {
                (Ok(got), Some(want)) => assert_eq!(got, want, "{}", c.name),
                (Err(_), None) => {}
                (got, want) => panic!("{}: got {got:?}, wanted {want:?}", c.name),
            }
        }
    }
}
