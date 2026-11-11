//! Undo for the edits a DJ makes all day: ratings, tag edits and favourite toggles
//! (owner: device area).
//!
//! The pieces here are pure data plus functions over a [`Library`]; the stack itself lives in
//! the app (one per library, in memory), and the three commands that make these edits call
//! [`record_edit`] / [`record_favourite_toggle`] around their write.
//!
//! An entry stores, per track, only the fields that changed with their value before and after.
//! Reversing is guarded the way a text editor's undo is not: a field is put back only if it
//! still holds the value the edit gave it. A field changed again since (by a later edit that
//! was not undone first, by a restore, by another tool) is left alone and named in the plan, so
//! undo can never overwrite work that came after the edit it reverses. [`plan_undo`] decides
//! this for a run of entries (newest first, each seeing the effect of the ones before it) and
//! [`apply_undo`] executes exactly that plan, so the preview is the action.
//!
//! The reversal writes through the same `Library` methods the edits used
//! ([`Library::edit_tracks`], [`Library::toggle_favourite`]).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::library::{Error, Library};
use crate::ops_fix::track_label;
use crate::ops_metadata::{TrackDetail, TrackEdit};

/// A field an undo entry can restore.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Title,
    Artist,
    AlbumArtist,
    Album,
    Composer,
    Genre,
    Label,
    Remixer,
    Producer,
    Comments,
    Year,
    TrackNumber,
    NumberOfTracks,
    DiscNumber,
    NumberOfDiscs,
    Rating,
}

impl Field {
    pub const ALL: [Field; 16] = [
        Field::Title,
        Field::Artist,
        Field::AlbumArtist,
        Field::Album,
        Field::Composer,
        Field::Genre,
        Field::Label,
        Field::Remixer,
        Field::Producer,
        Field::Comments,
        Field::Year,
        Field::TrackNumber,
        Field::NumberOfTracks,
        Field::DiscNumber,
        Field::NumberOfDiscs,
        Field::Rating,
    ];

    /// The name shown to the user.
    pub fn name(self) -> &'static str {
        match self {
            Field::Title => "Title",
            Field::Artist => "Artist",
            Field::AlbumArtist => "Album artist",
            Field::Album => "Album",
            Field::Composer => "Composer",
            Field::Genre => "Genre",
            Field::Label => "Label",
            Field::Remixer => "Remixer",
            Field::Producer => "Producer",
            Field::Comments => "Comments",
            Field::Year => "Year",
            Field::TrackNumber => "Track number",
            Field::NumberOfTracks => "Number of tracks",
            Field::DiscNumber => "Disc number",
            Field::NumberOfDiscs => "Number of discs",
            Field::Rating => "Rating",
        }
    }

    /// The field's value in a row, as text (numbers in decimal).
    pub fn get(self, d: &TrackDetail) -> String {
        match self {
            Field::Title => d.title.clone(),
            Field::Artist => d.artist.clone(),
            Field::AlbumArtist => d.album_artist.clone(),
            Field::Album => d.album.clone(),
            Field::Composer => d.composer.clone(),
            Field::Genre => d.genre.clone(),
            Field::Label => d.label.clone(),
            Field::Remixer => d.remixer.clone(),
            Field::Producer => d.producer.clone(),
            Field::Comments => d.comments.clone(),
            Field::Year => d.year.to_string(),
            Field::TrackNumber => d.track_number.to_string(),
            Field::NumberOfTracks => d.number_of_tracks.to_string(),
            Field::DiscNumber => d.disc_number.to_string(),
            Field::NumberOfDiscs => d.number_of_discs.to_string(),
            Field::Rating => d.rating.to_string(),
        }
    }

    /// Puts `value` (as [`Field::get`] wrote it) into `edit`.
    fn put(self, edit: &mut TrackEdit, value: &str) -> Result<(), Error> {
        let num = || value.parse::<i64>().map_err(|_| Error::Invalid(format!("{} is not a number", self.name())));
        match self {
            Field::Title => edit.title = Some(value.into()),
            Field::Artist => edit.artist = Some(value.into()),
            Field::AlbumArtist => edit.album_artist = Some(value.into()),
            Field::Album => edit.album = Some(value.into()),
            Field::Composer => edit.composer = Some(value.into()),
            Field::Genre => edit.genre = Some(value.into()),
            Field::Label => edit.label = Some(value.into()),
            Field::Remixer => edit.remixer = Some(value.into()),
            Field::Producer => edit.producer = Some(value.into()),
            Field::Comments => edit.comments = Some(value.into()),
            Field::Year => edit.year = Some(num()?),
            Field::TrackNumber => edit.track_number = Some(num()?),
            Field::NumberOfTracks => edit.number_of_tracks = Some(num()?),
            Field::DiscNumber => edit.disc_number = Some(num()?),
            Field::NumberOfDiscs => edit.number_of_discs = Some(num()?),
            Field::Rating => edit.rating = Some(u8::try_from(num()?).map_err(|_| Error::Invalid("rating out of range".into()))?),
        }
        Ok(())
    }

    /// How a value reads on screen (ratings as stars, empty text as "empty").
    pub fn show(self, value: &str) -> String {
        match self {
            Field::Rating => match value {
                "0" => "unrated".to_string(),
                n => format!("{n} star{}", if n == "1" { "" } else { "s" }),
            },
            _ if value.is_empty() => "empty".to_string(),
            _ => value.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditKind {
    Rating,
    Metadata,
    Favourite,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldChange {
    pub field: Field,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrackChange {
    pub track_id: i64,
    pub label: String,
    pub fields: Vec<FieldChange>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FavouriteChange {
    pub track_id: i64,
    pub label: String,
    /// Whether the track was a favourite before the toggle (it is `!was_favourite` after).
    pub was_favourite: bool,
}

/// One undoable edit: a rating, a tag edit (one or many tracks) or a favourite toggle.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UndoEntry {
    /// Assigned by the stack; unique within it.
    pub id: u64,
    pub kind: EditKind,
    /// Unix milliseconds.
    pub at_ms: i64,
    pub summary: String,
    #[serde(skip)]
    pub tracks: Vec<TrackChange>,
    #[serde(skip)]
    pub favourites: Vec<FavouriteChange>,
}

impl UndoEntry {
    /// How many tracks the edit touched.
    pub fn track_count(&self) -> usize {
        self.tracks.len() + self.favourites.len()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn detail_label(d: &TrackDetail) -> String {
    let l = track_label(&d.artist, &d.title, "");
    if l.is_empty() { format!("track {}", d.track_id) } else { l }
}

/// The entry for a rating or tag edit that turned `before` into `after` (same tracks, same
/// order), or `None` if nothing changed. The kind is `Rating` when the rating was the only
/// field that changed anywhere.
pub fn entry_from_details(before: &[TrackDetail], after: &[TrackDetail]) -> Option<UndoEntry> {
    let mut tracks = Vec::new();
    for (b, a) in before.iter().zip(after) {
        let fields: Vec<FieldChange> = Field::ALL
            .iter()
            .filter_map(|f| {
                let (x, y) = (f.get(b), f.get(a));
                (x != y).then_some(FieldChange { field: *f, before: x, after: y })
            })
            .collect();
        if !fields.is_empty() {
            tracks.push(TrackChange { track_id: b.track_id, label: detail_label(b), fields });
        }
    }
    if tracks.is_empty() {
        return None;
    }
    let only_rating = tracks.iter().all(|t| t.fields.iter().all(|f| f.field == Field::Rating));
    let kind = if only_rating { EditKind::Rating } else { EditKind::Metadata };
    let summary = if tracks.len() == 1 {
        let t = &tracks[0];
        if only_rating {
            format!("Rated {} {} (was {})", t.label, Field::Rating.show(&t.fields[0].after), Field::Rating.show(&t.fields[0].before))
        } else {
            format!("Edited {}: {}", t.label, field_names(&tracks))
        }
    } else if only_rating {
        format!("Rated {} tracks", tracks.len())
    } else {
        format!("Edited {} tracks: {}", tracks.len(), field_names(&tracks))
    };
    Some(UndoEntry { id: 0, kind, at_ms: now_ms(), summary, tracks, favourites: Vec::new() })
}

fn field_names(tracks: &[TrackChange]) -> String {
    let mut seen: Vec<Field> = Vec::new();
    for t in tracks {
        for f in &t.fields {
            if !seen.contains(&f.field) {
                seen.push(f.field);
            }
        }
    }
    let names: Vec<String> = seen.iter().map(|f| f.name().to_lowercase()).collect();
    if names.len() > 4 { format!("{} and {} more fields", names[..4].join(", "), names.len() - 4) } else { names.join(", ") }
}

/// The entry for a favourite toggle.
pub fn entry_from_favourite(track_id: i64, label: String, was_favourite: bool) -> UndoEntry {
    let summary = if was_favourite { format!("Removed {label} from Favourites") } else { format!("Added {label} to Favourites") };
    UndoEntry {
        id: 0,
        kind: EditKind::Favourite,
        at_ms: now_ms(),
        summary,
        tracks: Vec::new(),
        favourites: vec![FavouriteChange { track_id, label, was_favourite }],
    }
}

/// Runs `op` (a rating or tag edit of `ids`) and returns its result with the undo entry for it.
/// If the tracks cannot be read first (an unknown id) `op` still runs and reports its own error;
/// no entry is made then. A failed `op` makes no entry.
pub fn record_edit<T>(
    lib: &mut Library,
    ids: &[i64],
    op: impl FnOnce(&mut Library) -> Result<T, Error>,
) -> Result<(T, Option<UndoEntry>), Error> {
    let before = lib.track_details(ids).ok();
    let out = op(lib)?;
    let entry = match before {
        Some(b) => lib.track_details(ids).ok().and_then(|a| entry_from_details(&b, &a)),
        None => None,
    };
    Ok((out, entry))
}

/// [`Library::toggle_favourite`] with its undo entry.
pub fn record_favourite_toggle(lib: &mut Library, track_id: i64) -> Result<(bool, Option<UndoEntry>), Error> {
    let label = lib.track_details(&[track_id]).ok().map(|d| detail_label(&d[0]));
    let was = lib.favourite_track_ids()?.contains(&track_id);
    let now = lib.toggle_favourite(track_id)?;
    let entry = label.filter(|_| now != was).map(|l| entry_from_favourite(track_id, l, was));
    Ok((now, entry))
}

/// The undo history of one library, newest last. Bounded by a limit the user sets.
#[derive(Debug, Default)]
pub struct UndoStack {
    entries: VecDeque<UndoEntry>,
    next_id: u64,
}

impl UndoStack {
    /// Adds an entry (giving it an id) and drops the oldest ones beyond `limit`. A limit of 0
    /// records nothing. Returns the id, or 0 if nothing was kept.
    pub fn push(&mut self, mut e: UndoEntry, limit: usize) -> u64 {
        if limit == 0 {
            self.entries.clear();
            return 0;
        }
        self.next_id += 1;
        e.id = self.next_id;
        self.entries.push_back(e);
        self.trim(limit);
        self.next_id
    }

    pub fn trim(&mut self, limit: usize) {
        while self.entries.len() > limit {
            self.entries.pop_front();
        }
    }

    /// Newest first.
    pub fn newest_first(&self) -> impl Iterator<Item = &UndoEntry> {
        self.entries.iter().rev()
    }

    /// The newest `n` entries, newest first.
    pub fn top(&self, n: usize) -> Vec<&UndoEntry> {
        self.newest_first().take(n).collect()
    }

    /// Removes the entries with these ids (those just reversed), wherever they now sit.
    pub fn remove(&mut self, ids: &[u64]) {
        self.entries.retain(|e| !ids.contains(&e.id));
    }

    /// Removes the newest `n` entries.
    pub fn pop(&mut self, n: usize) {
        for _ in 0..n {
            self.entries.pop_back();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One line of an undo preview.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlanItem {
    pub entry_id: u64,
    pub track_id: i64,
    pub label: String,
    /// "Rating", "Genre", ..., or "Favourites".
    pub what: String,
    #[serde(skip)]
    pub field: Option<Field>,
    /// The value now, as shown ("3 stars", "House", "in Favourites").
    pub current: String,
    /// The value after the undo.
    pub becomes: String,
    #[serde(skip)]
    raw_becomes: String,
    /// Set when this line is left alone, and why.
    pub skip: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UndoPlan {
    pub entries: usize,
    pub items: Vec<PlanItem>,
    pub reverts: usize,
    pub skipped: usize,
}

/// What undoing `entries` (newest first) would do to the library as it is now. Read-only. Each
/// entry sees the library as the entries before it in the list would leave it, so undoing an
/// edit and the edit before it on the same field puts back the older value.
pub fn plan_undo(lib: &Library, entries: &[&UndoEntry]) -> Result<UndoPlan, Error> {
    let mut details: HashMap<i64, Option<TrackDetail>> = HashMap::new();
    let mut virtual_fields: HashMap<(i64, Field), String> = HashMap::new();
    let favs: HashSet<i64> = lib.favourite_track_ids()?.into_iter().collect();
    let mut virtual_favs: HashMap<i64, bool> = HashMap::new();
    let mut items = Vec::new();
    for e in entries {
        for t in &e.tracks {
            let current_detail = details
                .entry(t.track_id)
                .or_insert_with(|| lib.track_details(&[t.track_id]).ok().and_then(|mut v| v.pop()))
                .clone();
            for f in &t.fields {
                let mk = |current: String, skip: Option<String>| PlanItem {
                    entry_id: e.id,
                    track_id: t.track_id,
                    label: t.label.clone(),
                    what: f.field.name().to_string(),
                    field: Some(f.field),
                    current: f.field.show(&current),
                    becomes: f.field.show(&f.before),
                    raw_becomes: f.before.clone(),
                    skip,
                };
                let Some(d) = &current_detail else {
                    items.push(mk(f.after.clone(), Some("the track is no longer in the library".into())));
                    continue;
                };
                let now = virtual_fields.get(&(t.track_id, f.field)).cloned().unwrap_or_else(|| f.field.get(d));
                if now == f.after {
                    virtual_fields.insert((t.track_id, f.field), f.before.clone());
                    items.push(mk(now, None));
                } else {
                    let why = format!("changed since this edit (now {}); left alone", f.field.show(&now));
                    items.push(mk(now, Some(why)));
                }
            }
        }
        for fav in &e.favourites {
            let exists = details
                .entry(fav.track_id)
                .or_insert_with(|| lib.track_details(&[fav.track_id]).ok().and_then(|mut v| v.pop()))
                .is_some();
            let now = virtual_favs.get(&fav.track_id).copied().unwrap_or_else(|| favs.contains(&fav.track_id));
            let show = |b: bool| if b { "in Favourites" } else { "not in Favourites" }.to_string();
            let mut item = PlanItem {
                entry_id: e.id,
                track_id: fav.track_id,
                label: fav.label.clone(),
                what: "Favourites".into(),
                field: None,
                current: show(now),
                becomes: show(fav.was_favourite),
                raw_becomes: fav.was_favourite.to_string(),
                skip: None,
            };
            if !exists {
                item.skip = Some("the track is no longer in the library".into());
            } else if now == fav.was_favourite {
                item.skip = Some(format!("already {}; left alone", show(now)));
            } else {
                virtual_favs.insert(fav.track_id, fav.was_favourite);
            }
            items.push(item);
        }
    }
    let skipped = items.iter().filter(|i| i.skip.is_some()).count();
    Ok(UndoPlan { entries: entries.len(), reverts: items.len() - skipped, skipped, items })
}

/// Outcome of [`apply_undo`].
#[derive(Debug, Clone, Serialize)]
pub struct UndoReport {
    pub entries: usize,
    pub reverted: usize,
    pub skipped: usize,
    /// The plan that was executed, line by line.
    pub items: Vec<PlanItem>,
}

/// Executes [`plan_undo`] for `entries` (newest first): every field line without a skip is
/// written (one transaction for all tags and ratings), then favourites are toggled.
pub fn apply_undo(lib: &mut Library, entries: &[&UndoEntry]) -> Result<UndoReport, Error> {
    let plan = plan_undo(lib, entries)?;
    // the plan is in newest-first order, so the last line for a field is the oldest entry's
    // value: that is where the field ends up
    let mut edits: Vec<(i64, TrackEdit)> = Vec::new();
    let mut fav_final: Vec<(i64, bool)> = Vec::new();
    for item in plan.items.iter().filter(|i| i.skip.is_none()) {
        match item.field {
            Some(f) => {
                let at = match edits.iter().position(|(id, _)| *id == item.track_id) {
                    Some(p) => p,
                    None => {
                        edits.push((item.track_id, TrackEdit::default()));
                        edits.len() - 1
                    }
                };
                f.put(&mut edits[at].1, &item.raw_becomes)?;
            }
            None => {
                let want = item.raw_becomes == "true";
                match fav_final.iter_mut().find(|(id, _)| *id == item.track_id) {
                    Some(slot) => slot.1 = want,
                    None => fav_final.push((item.track_id, want)),
                }
            }
        }
    }
    if !edits.is_empty() {
        lib.edit_tracks(&edits)?;
    }
    if !fav_final.is_empty() {
        let have: HashSet<i64> = lib.favourite_track_ids()?.into_iter().collect();
        for (id, want) in fav_final {
            if have.contains(&id) != want {
                lib.toggle_favourite(id)?;
            }
        }
    }
    Ok(UndoReport { entries: plan.entries, reverted: plan.reverts, skipped: plan.skipped, items: plan.items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::TrackMeta;
    use crate::strfile;
    use std::path::PathBuf;

    struct Fx {
        base: PathBuf,
        lib: Library,
        ids: Vec<i64>,
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn fixture(tag: &str, n: usize) -> Fx {
        let base = std::env::temp_dir().join(format!("pm-undo-{tag}-{}", std::process::id()));
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
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![0u8; 1200 + i]).unwrap();
            let meta = TrackMeta {
                title: format!("Title {i}"),
                artist: format!("Artist {i}"),
                genre: "House".into(),
                year: 2001,
                format: "MP3".into(),
                play_time_secs: 100,
                ..Default::default()
            };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        Fx { base, lib, ids }
    }

    fn rate(fx: &mut Fx, id: i64, stars: u8) -> Option<UndoEntry> {
        record_edit(&mut fx.lib, &[id], |l| l.set_track_rating(id, stars)).unwrap().1
    }

    fn rating(fx: &Fx, id: i64) -> i64 {
        fx.lib.track_details(&[id]).unwrap()[0].rating
    }

    #[test]
    fn a_rating_is_recorded_with_both_values_and_undone() {
        let mut fx = fixture("rate", 2);
        let id = fx.ids[0];
        let e = rate(&mut fx, id, 4).expect("an entry");
        assert_eq!(e.kind, EditKind::Rating);
        assert_eq!(e.summary, "Rated Artist 0 - Title 0 4 stars (was unrated)");
        assert_eq!(rating(&fx, id), 4);
        let rep = apply_undo(&mut fx.lib, &[&e]).unwrap();
        assert_eq!((rep.reverted, rep.skipped), (1, 0));
        assert_eq!(rating(&fx, id), 0);
        // no change, no entry
        assert!(rate(&mut fx, id, 0).is_none());
    }

    #[test]
    fn a_tag_edit_over_several_tracks_names_the_fields_and_restores_each_track() {
        let mut fx = fixture("tags", 3);
        let ids = fx.ids.clone();
        fx.lib.edit_tracks(&[(ids[1], TrackEdit { genre: Some("Techno".into()), ..Default::default() })]).unwrap();
        let edit = TrackEdit { genre: Some("Disco".into()), year: Some(1978), ..Default::default() };
        let (n, e) = record_edit(&mut fx.lib, &ids, |l| l.edit_tracks_same(&ids, &edit)).unwrap();
        assert_eq!(n, 3);
        let e = e.unwrap();
        assert_eq!((e.kind, e.tracks.len()), (EditKind::Metadata, 3));
        assert_eq!(e.summary, "Edited 3 tracks: genre, year");
        // the middle track had a different genre before: each track gets its own value back
        apply_undo(&mut fx.lib, &[&e]).unwrap();
        let d = fx.lib.track_details(&ids).unwrap();
        assert_eq!((d[0].genre.as_str(), d[1].genre.as_str(), d[2].genre.as_str()), ("House", "Techno", "House"));
        assert!(d.iter().all(|x| x.year == 2001));
    }

    #[test]
    fn a_field_changed_since_is_left_alone_and_named() {
        let mut fx = fixture("conflict", 2);
        let id = fx.ids[0];
        let e = rate(&mut fx, id, 3).unwrap();
        // something else rates it again afterwards, without going through the stack
        fx.lib.set_track_rating(id, 5).unwrap();
        let plan = plan_undo(&fx.lib, &[&e]).unwrap();
        assert_eq!((plan.reverts, plan.skipped), (0, 1));
        assert!(plan.items[0].skip.as_deref().unwrap().contains("5 stars"), "{:?}", plan.items[0].skip);
        let rep = apply_undo(&mut fx.lib, &[&e]).unwrap();
        assert_eq!(rep.reverted, 0);
        assert_eq!(rating(&fx, id), 5, "the later rating survives");
    }

    #[test]
    fn undoing_two_edits_of_one_field_goes_back_through_both() {
        let mut fx = fixture("chain", 1);
        let id = fx.ids[0];
        let a = rate(&mut fx, id, 2).unwrap();
        let b = rate(&mut fx, id, 5).unwrap();
        // undoing only the older one while the newer stands: conflict, nothing happens
        let plan = plan_undo(&fx.lib, &[&a]).unwrap();
        assert_eq!(plan.skipped, 1);
        // undoing both, newest first: 5 -> 2 -> 0
        let plan = plan_undo(&fx.lib, &[&b, &a]).unwrap();
        assert_eq!(plan.reverts, 2);
        assert_eq!((plan.items[0].current.as_str(), plan.items[0].becomes.as_str()), ("5 stars", "2 stars"));
        assert_eq!((plan.items[1].current.as_str(), plan.items[1].becomes.as_str()), ("2 stars", "unrated"));
        apply_undo(&mut fx.lib, &[&b, &a]).unwrap();
        assert_eq!(rating(&fx, id), 0);
    }

    #[test]
    fn favourites_toggle_back_and_a_removed_track_is_skipped() {
        let mut fx = fixture("fav", 2);
        let (a, b) = (fx.ids[0], fx.ids[1]);
        let (now, e1) = record_favourite_toggle(&mut fx.lib, a).unwrap();
        assert!(now);
        let e1 = e1.unwrap();
        assert_eq!(e1.summary, "Added Artist 0 - Title 0 to Favourites");
        let (_, e2) = record_favourite_toggle(&mut fx.lib, b).unwrap();
        let e2 = e2.unwrap();
        let (now, e3) = record_favourite_toggle(&mut fx.lib, b).unwrap();
        assert!(!now);
        let e3 = e3.unwrap();
        assert_eq!(e3.summary, "Removed Artist 1 - Title 1 from Favourites");
        assert_eq!(fx.lib.favourite_track_ids().unwrap(), [a]);

        // undo the removal of b, then its addition, then a's addition: all three back to "none"
        let rep = apply_undo(&mut fx.lib, &[&e3, &e2, &e1]).unwrap();
        assert_eq!((rep.reverted, rep.skipped), (3, 0));
        assert!(fx.lib.favourite_track_ids().unwrap().is_empty());

        // a track deleted from the library: its entry cannot be reversed
        let (_, e4) = record_favourite_toggle(&mut fx.lib, a).unwrap();
        let e4 = e4.unwrap();
        fx.lib.con.execute("DELETE FROM casetracks WHERE track_id = ?1", [a]).unwrap();
        fx.lib.con.execute("DELETE FROM tracks WHERE track_id = ?1", [a]).unwrap();
        let plan = plan_undo(&fx.lib, &[&e4]).unwrap();
        assert_eq!(plan.skipped, 1);
        assert!(plan.items[0].skip.as_deref().unwrap().contains("no longer in the library"));
        assert!(apply_undo(&mut fx.lib, &[&e4]).is_ok());
    }

    #[test]
    fn a_favourite_set_by_hand_since_is_not_toggled_twice() {
        let mut fx = fixture("fav2", 1);
        let a = fx.ids[0];
        let (_, e) = record_favourite_toggle(&mut fx.lib, a).unwrap();
        let e = e.unwrap();
        // it is un-favourited by another route; undoing "added" would now be a no-op
        fx.lib.toggle_favourite(a).unwrap();
        let plan = plan_undo(&fx.lib, &[&e]).unwrap();
        assert_eq!(plan.skipped, 1);
        assert!(plan.items[0].skip.as_deref().unwrap().contains("already"));
        apply_undo(&mut fx.lib, &[&e]).unwrap();
        assert!(fx.lib.favourite_track_ids().unwrap().is_empty());
    }

    #[test]
    fn the_stack_is_bounded_newest_first_and_a_limit_of_zero_records_nothing() {
        let mut fx = fixture("stack", 1);
        let id = fx.ids[0];
        let mut st = UndoStack::default();
        let mut ids = Vec::new();
        for stars in 1..=5u8 {
            let e = rate(&mut fx, id, stars).unwrap();
            ids.push(st.push(e, 3));
        }
        assert_eq!(st.len(), 3);
        assert_eq!(st.newest_first().map(|e| e.id).collect::<Vec<_>>(), [ids[4], ids[3], ids[2]]);
        assert_eq!(st.top(2).len(), 2);
        st.pop(1);
        assert_eq!(st.newest_first().next().unwrap().id, ids[3]);
        st.remove(&[ids[3], 9999]);
        assert_eq!(st.newest_first().map(|e| e.id).collect::<Vec<_>>(), [ids[2]]);
        let e = rate(&mut fx, id, 2).unwrap();
        let again = st.push(e, 3);
        assert!(again > ids[4], "ids keep rising after removals");
        st.remove(&[again]);
        st.trim(1);
        assert_eq!(st.len(), 1);
        let e = rate(&mut fx, id, 1).unwrap();
        assert_eq!(st.push(e, 0), 0);
        assert!(st.is_empty(), "limit 0 turns the history off and empties it");
    }

    #[test]
    fn a_failed_edit_leaves_no_entry() {
        let mut fx = fixture("fail", 1);
        let id = fx.ids[0];
        let r = record_edit(&mut fx.lib, &[id], |l| l.set_track_rating(id, 9));
        assert!(r.is_err());
        let r = record_edit(&mut fx.lib, &[777], |l| l.set_track_rating(777, 2));
        assert!(r.is_err(), "unknown track: the edit's own error");
    }
}
