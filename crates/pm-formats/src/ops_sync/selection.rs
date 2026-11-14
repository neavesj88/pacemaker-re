//! A sync of only what the user picked: some tracks, mixes and cases of the source, and optionally
//! "put these tracks into this case on the target". The plan is a normal [`SyncPlan`] holding
//! the picked items, the tracks the picked mixes and cases need (marked with
//! [`SyncItem::needed_by`]), and at most one case item for the target case. It never removes
//! or overwrites anything on the target: the saved Sync settings are clamped by
//! [`drag_settings`] so a drag only adds (a case is merged into, a same-named mix is kept beside
//! the target's, a track already there is left as it is). The track, mix and case helpers are the
//! full plan's, so every other rule is the same.
//!
//! The libraries are not walked: tracks are looked up by location and by id (both indexed) and
//! the target by location, one query per few hundred picked items, so a drag of three tracks into
//! a library of 30 000 reads about three rows of it. Mixes and cases are listed whole (they are a
//! few hundred rows with no audio behind them).
//!
//! [`super::apply_selection`] plans the same selection again and compares digests. The digest of
//! a selection plan holds the selected items only (with the stamps of the rows they were decided
//! from), not the stamps of the whole libraries, so an unrelated change does not make it stale.

use super::*;

/// The target case the picked tracks should end up in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntoCase {
    /// The case's name on the target.
    pub name: String,
    /// Make the case when the target has none of that name. Without it a missing case is a skip.
    #[serde(default)]
    pub create: bool,
}

/// What the user picked on the source side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Selection {
    /// `tracks.location` of each picked track.
    pub tracks: Vec<String>,
    /// Mix names.
    pub mixes: Vec<String>,
    /// Case names.
    pub cases: Vec<String>,
    pub into_case: Option<IntoCase>,
}

const GONE: &str = "it is not in the sending library any more";

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

fn push_unique<T: Clone + Eq + std::hash::Hash>(out: &mut Vec<T>, seen: &mut HashSet<T>, v: &T) {
    if seen.insert(v.clone()) {
        out.push(v.clone());
    }
}

/// The saved settings as a drag applies them. A drag adds to the target and never overwrites or
/// takes out what is there, so the settings that would are replaced by their adding twin; the
/// ones that already skip stay. (Plays are only ever raised, as in a full sync.)
pub fn drag_settings(s: &SyncSettings) -> SyncSettings {
    SyncSettings {
        tracks_on_both: TracksOnBoth::Leave,
        mixes_same_name: if s.mixes_same_name == MixesSameName::Skip { MixesSameName::Skip } else { MixesSameName::KeepBoth },
        cases_same_name: if s.cases_same_name == CasesSameName::Skip { CasesSameName::Skip } else { CasesSameName::Merge },
        remove_missing: false,
        play_counts: s.play_counts,
    }
}

/// Plans a sync of the `selection` of `source` into `target`. Reads only what the selection
/// touches; writes nothing. `free` is the target's free space (the caller reads it).
pub fn plan_selection(
    source: &Library,
    target: &Library,
    settings: &SyncSettings,
    source_label: &str,
    selection: &Selection,
    free: Option<u64>,
) -> Result<SyncPlan, Error> {
    ensure_different(source, target)?;
    let clamped = drag_settings(settings);
    let settings = &clamped;

    // what was picked, once each, in the order given
    let (mut sel_locs, mut seen) = (Vec::new(), HashSet::new());
    for l in &selection.tracks {
        push_unique(&mut sel_locs, &mut seen, l);
    }
    let s_mixes = source.mix_infos()?;
    let s_cases = source.case_infos()?;
    let (mix_by, case_by) = (first_by_name(&s_mixes), first_by_name(&s_cases));
    let (mut mix_names, mut seen) = (Vec::new(), HashSet::new());
    for n in selection.mixes.iter().map(|n| norm(n)) {
        push_unique(&mut mix_names, &mut seen, &n);
    }
    let (mut case_names, mut seen) = (Vec::new(), HashSet::new());
    for n in selection.cases.iter().map(|n| norm(n)) {
        push_unique(&mut case_names, &mut seen, &n);
    }

    // what the picked mixes and cases hold: the library ids they name
    let mut mix_docs: Vec<(&str, Option<(&crate::library::MixInfo, Result<crate::mix::Mix, Error>)>)> = Vec::new();
    let mut ids: Vec<i64> = Vec::new();
    for n in &mix_names {
        match mix_by.get(n) {
            Some(m) => {
                let doc = source.read_mix(m.id);
                if let Ok(d) = &doc {
                    ids.extend(d.tracks.iter().map(|t| t.library_id).filter(|i| *i >= 0));
                }
                mix_docs.push((n.as_str(), Some((m, doc))));
            }
            None => mix_docs.push((n.as_str(), None)),
        }
    }
    let mut case_ids: Vec<(&str, Option<(&crate::library::CaseInfo, Vec<i64>)>)> = Vec::new();
    for n in &case_names {
        match case_by.get(n) {
            Some(c) => {
                let held = source.case_track_ids(c.id)?;
                ids.extend(held.iter().copied());
                case_ids.push((n.as_str(), Some((c, held))));
            }
            None => case_ids.push((n.as_str(), None)),
        }
    }
    let by_id = rows_by_ids(source, &ids)?;

    // the tracks to plan: the picked ones, then the ones the mixes and cases need
    let mut all_locs = sel_locs.clone();
    let mut known: HashSet<String> = sel_locs.iter().cloned().collect();
    let mut needed_by: HashMap<String, Vec<String>> = HashMap::new();
    let mut note = |loc: &str, by: String, all: &mut Vec<String>| {
        if known.insert(loc.to_string()) {
            all.push(loc.to_string());
        }
        let list = needed_by.entry(loc.to_string()).or_default();
        if !list.contains(&by) {
            list.push(by);
        }
    };
    for (_, m) in &mix_docs {
        if let Some((info, Ok(doc))) = m {
            for t in &doc.tracks {
                if let Some(r) = by_id.get(&t.library_id) {
                    note(&r.location, format!("mix \"{}\"", info.name), &mut all_locs);
                }
            }
        }
    }
    for (_, c) in &case_ids {
        if let Some((info, held)) = c {
            for id in held {
                if let Some(r) = by_id.get(id) {
                    note(&r.location, format!("case \"{}\"", info.name), &mut all_locs);
                }
            }
        }
    }
    let s_rows = rows_by_locations(source, &all_locs)?;
    let t_rows = rows_by_locations(target, &all_locs)?;

    let mut items: Vec<SyncItem> = Vec::new();

    // tracks
    for loc in &all_locs {
        let picked = sel_locs.contains(loc);
        let mut item = match s_rows.get(loc) {
            Some(s) => {
                let mut it = track_item(source, target, s, t_rows.get(loc), settings);
                if matches!(&it.action, SyncAction::Skip { reason } if reason == LEAVE_TRACK) {
                    it.action = skip("it is already there, and a drag never changes a track in the receiving library");
                }
                it
            }
            None => {
                let name = loc.rsplit('/').next().unwrap_or("").to_string();
                let mut it = blank_item(format!("track:{loc}"), ItemKind::Track, name, skip(GONE));
                it.location = Some(loc.clone());
                it
            }
        };
        if !picked {
            item.needed_by = needed_by.get(loc).cloned().unwrap_or_default();
        }
        items.push(item);
    }

    // mixes
    let t_mixes = target.mix_infos()?;
    let mut mix_ctx = MixCtx::new(&t_mixes);
    let row_of = |id: i64| by_id.get(&id).map(|r| (r.location.clone(), r.label()));
    for (n, m) in mix_docs {
        match m {
            Some((info, doc)) => items.push(mix_item(info, doc, &row_of, &mut mix_ctx, settings, source_label)),
            None => items.push(blank_item(format!("mix:{n}"), ItemKind::Mix, n.to_string(), skip(GONE))),
        }
    }

    // cases
    let t_cases = target.case_infos()?;
    let t_case_by_name = first_by_name(&t_cases);
    let t_locs = |id: i64| -> Result<Vec<String>, Error> {
        let held = target.case_track_ids(id)?;
        let rows = rows_by_ids(target, &held)?;
        Ok(held.iter().filter_map(|i| rows.get(i).map(|r| r.location.clone())).collect())
    };
    let will_exist: HashSet<String> = t_rows
        .keys()
        .cloned()
        .chain(items.iter().filter(|i| i.kind == ItemKind::Track && i.action == SyncAction::Copy).filter_map(|i| i.location.clone()))
        .collect();
    for (n, c) in case_ids {
        let Some((info, held)) = c else {
            items.push(blank_item(format!("case:{n}"), ItemKind::Case, n.to_string(), skip(GONE)));
            continue;
        };
        let mut needs: Vec<Need> = Vec::new();
        for id in &held {
            if let Some(r) = by_id.get(id) {
                if !needs.iter().any(|x| x.location == r.location) {
                    needs.push(Need { location: r.location.clone(), label: r.label() });
                }
            }
        }
        items.push(case_item(info, needs, &|l| will_exist.contains(l), t_case_by_name.get(n).copied(), &t_locs, settings)?);
    }

    // the target case the picked tracks go into
    if let Some(ic) = &selection.into_case {
        let name = ic.name.trim().to_string();
        let needs: Vec<Need> = sel_locs.iter().filter_map(|l| s_rows.get(l).map(|r| Need { location: r.location.clone(), label: r.label() })).collect();
        let avail: Vec<&Need> = needs.iter().filter(|n| will_exist.contains(&n.location)).collect();
        let (mut item, new) = match t_case_by_name.get(&norm(&name)) {
            Some(t) => {
                let have: HashSet<String> = t_locs(t.id)?.into_iter().collect();
                let new = avail.iter().filter(|n| !have.contains(&n.location)).count();
                let shown = if new > 0 { new } else { avail.len() };
                let mut it = blank_item(format!("into-case:{}", norm(&name)), ItemKind::Case, format!("Add {} to case {name}", plural(shown, "track")), SyncAction::Merge);
                it.target_id = Some(t.id);
                it.fp = format!("{}/{}/{}", t.id, t.date_modified, t.track_count);
                if new == 0 {
                    it.action = skip(SAME);
                }
                (it, new)
            }
            None if ic.create => {
                let it = blank_item(format!("into-case:{}", norm(&name)), ItemKind::Case, format!("New case {name} with {}", plural(avail.len(), "track")), SyncAction::Copy);
                (it, avail.len())
            }
            None => {
                let it = blank_item(format!("into-case:{}", norm(&name)), ItemKind::Case, format!("Add {} to case {name}", plural(avail.len(), "track")), skip("the case is not in the receiving library any more"));
                (it, 0)
            }
        };
        item.into_case = Some(name.clone());
        if name.is_empty() || name.chars().count() > crate::library::MAX_CASE_NAME_CHARS {
            item.action = skip("that is not a usable case name");
        } else if avail.is_empty() && item.action.acts() {
            item.action = skip("none of the tracks will be in the receiving library");
        } else if new == 0 && item.action.acts() {
            item.action = skip(SAME);
        }
        let mut notes: Vec<String> = Vec::new();
        if new > 0 && new < avail.len() {
            let have = avail.len() - new;
            notes.push(format!("{have} of the {} {} already in it", avail.len(), if have == 1 { "is" } else { "are" }));
        }
        if avail.len() < needs.len() {
            notes.push(format!("{} of {} tracks will not be there and are left out", needs.len() - avail.len(), needs.len()));
        }
        item.detail = notes.join("; ");
        item.needs = needs;
        items.push(item);
    }

    let target_locs: HashSet<String> = t_rows.keys().cloned().collect();
    settle(&mut items, &target_locs, &HashSet::new());
    let bytes: u64 = items.iter().filter(|i| i.kind == ItemKind::Track && i.action.acts()).map(|i| i.bytes).sum();
    let fits = bytes == 0 || free.is_none_or(|f| bytes + SPACE_MARGIN <= f);
    let notes = vec!["Smart cases are kept by the app on this computer and are not synced.".to_string()];
    let digest = selection_digest(&items, settings, source_label, selection);
    Ok(SyncPlan { items, settings: settings.clone(), source_label: source_label.to_string(), bytes_to_copy: bytes, free_bytes: free, fits, digest, notes })
}

/// The digest of a selection plan: the settings, the selection and each item with the stamps of
/// the rows it was decided from. Nothing about the rest of either library.
fn selection_digest(items: &[SyncItem], settings: &SyncSettings, label: &str, selection: &Selection) -> String {
    let mut text = String::new();
    text.push_str(&serde_json::to_string(settings).unwrap_or_default());
    text.push('\n');
    text.push_str(label);
    text.push('\n');
    text.push_str(&serde_json::to_string(selection).unwrap_or_default());
    for it in items {
        text.push('\n');
        text.push_str(&format!(
            "{}|{}|{}|{}|{:?}|{:?}|{}|{}|{}",
            it.key,
            serde_json::to_string(&it.action).unwrap_or_default(),
            it.bytes,
            it.needs.iter().map(|n| n.location.as_str()).collect::<Vec<_>>().join(","),
            it.source_id,
            it.target_id,
            it.fp,
            it.detail,
            it.needed_by.join(",")
        ));
    }
    format!("{:016x}", appfile::fnv1a(&text))
}
