//! Mixes: the mix editor's backend (open, edit, save, import / export, beat matching).
//!
//! The editor works on [`doc::MixDoc`], a lossless, id-addressed view of
//! `pm_formats::mix::Mix`; its fields are documented in `src/lib/api/mixes.ts`. All database
//! writes go through the `pm-formats` `Library` (`create_mix`, `save_mix`, `rename_mix`,
//! `set_mix_meta`, `delete_mix`), so rows stay what the Editor and the device expect.
//!
//! Submodules: [`doc`] (the model and its conversion), [`timing`] (beat times, beat matching,
//! automatic transitions; pure), [`info`] (library lookups: track facts, beat grids, new clips,
//! `.pmk` re-linking). The mix audio area converts the editor's document with
//! `doc::MixDoc::to_mix`.
//!
//! To add a command: write a `#[tauri::command] pub async fn` here, add its full path to the
//! list in `mixes_commands!` below, then mirror it in `src/lib/api/mixes.ts` and
//! `src/lib/devmock/mixes.ts`.

pub mod doc;
pub mod info;
pub mod timing;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pm_formats::library::{Collection, Library};
use pm_formats::mix::Mix;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use doc::{Clip, MixDoc, MixMeta, TrackInfo};
use timing::{BeatmatchSuggestion, TransitionSuggestion};

/// Managed state private to this area goes here (`app.manage(..)`).
pub fn setup(_app: &mut tauri::App) {}

/// The mixes on the volume (read-only), summarised.
#[tauri::command]
pub async fn mix_list(root: String) -> AppResult<Vec<Collection>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<Collection>> {
        Ok(Library::open_read_only(&PathBuf::from(root))?.mixes()?)
    })
    .await?
}

/// What a save reports back.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixSaved {
    pub id: i64,
    pub name: String,
    pub play_time_secs: i64,
    pub track_count: i64,
    /// `mixes.date_modified`, Unix milliseconds.
    pub date_modified: i64,
}

fn many(problems: &[String]) -> String {
    let shown: Vec<&str> = problems.iter().take(8).map(String::as_str).collect();
    format!(
        "The mix cannot be saved: {}{}",
        shown.join("; "),
        if problems.len() > shown.len() { format!("; and {} more", problems.len() - shown.len()) } else { String::new() }
    )
}

// The `*_in` functions hold the logic so tests can call them without the Tauri runtime.

pub(crate) fn open_in(lib: &Library, id: i64) -> AppResult<MixDoc> {
    let info = lib.mix_info(id)?.ok_or_else(|| AppError::from(format!("there is no mix {id}")))?;
    let mix = lib.read_mix(id)?;
    let rows = lib.mix_track_rows(id)?;
    let mut doc = MixDoc::from_mix(id, &mix);
    doc.meta = Some(MixMeta::from_info(&info));
    info::attach_track_info(lib, &mut doc, &rows)?;
    Ok(doc)
}

/// Everything that would stop a save: the document's own problems plus a name another mix uses
/// (checked here because the document alone cannot know).
pub(crate) fn problems_in(lib: &Library, doc: &MixDoc) -> AppResult<Vec<String>> {
    let ids: Vec<i64> = doc.clips.iter().map(|c| c.track_id).filter(|t| *t >= 0).collect();
    let known: HashSet<i64> = lib.mix_track_sources(&ids)?.into_iter().map(|s| s.track_id).collect();
    let mut problems = doc.validate(Some(&known));
    if lib.mix_infos()?.iter().any(|m| m.id != doc.id && m.name == doc.name) {
        problems.push(format!("a mix called {:?} already exists", doc.name));
    }
    Ok(problems)
}

pub(crate) fn save_in(lib: &mut Library, id: i64, doc: &MixDoc) -> AppResult<MixSaved> {
    if doc.id != id {
        return Err(format!("this document belongs to mix {}, not mix {id}", doc.id).into());
    }
    let problems = problems_in(lib, doc)?;
    if !problems.is_empty() {
        return Err(many(&problems).into());
    }
    let mix = doc.to_mix()?;
    lib.save_mix(id, &mix)?;
    let info = lib.mix_info(id)?.ok_or_else(|| AppError::from("the mix vanished while saving"))?;
    Ok(MixSaved { id, name: info.name, play_time_secs: info.play_time_secs, track_count: info.track_count, date_modified: info.date_modified })
}

/// Opens a mix as the editor's document: clips with their tools, the library facts and beat
/// grid of every track, and the library row's metadata.
#[tauri::command]
pub async fn mix_open(root: String, id: i64) -> AppResult<MixDoc> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<MixDoc> { open_in(&Library::open_read_only(Path::new(&root))?, id) }).await?
}

/// Creates an empty mix and returns its id. Fails if the name is empty, too long or taken.
#[tauri::command]
pub async fn mix_new(root: String, name: String) -> AppResult<i64> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<i64> { Ok(Library::open(Path::new(&root))?.create_mix(&name)?) }).await?
}

/// Validates the document and writes it as the mix's content (name, blob, `mixtracks`, play
/// time, preview markers, `date_modified`). Refuses, naming each problem, when a track is not in
/// the library, clips overlap on one channel without a transition, a tool lacks a parameter
/// envelope, positions are out of order and so on. An unchanged document writes the identical
/// blob.
#[tauri::command]
pub async fn mix_save(root: String, id: i64, doc: MixDoc) -> AppResult<MixSaved> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<MixSaved> { save_in(&mut Library::open(Path::new(&root))?, id, &doc) }).await?
}

/// The problems `mix_save` would refuse the document for (empty = it would save), without
/// writing anything.
#[tauri::command]
pub async fn mix_validate(root: String, doc: MixDoc) -> AppResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<String>> {
        let lib = Library::open_read_only(Path::new(&root))?;
        problems_in(&lib, &doc)
    })
    .await?
}

/// Renames a mix (the row and the document name together).
#[tauri::command]
pub async fn mix_rename(root: String, id: i64, name: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> { Ok(Library::open(Path::new(&root))?.rename_mix(id, &name)?) }).await?
}

/// Deletes a mix. Irreversible: the caller must have asked the user, naming the mix. Returns
/// whether it existed.
#[tauri::command]
pub async fn mix_delete(root: String, id: i64) -> AppResult<bool> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<bool> { Ok(Library::open(Path::new(&root))?.delete_mix(id)?) }).await?
}

/// Writes the metadata fields of the library row (genre, year, rating, producer, remixer,
/// comments; the other fields of `meta` are ignored).
#[tauri::command]
pub async fn mix_set_meta(root: String, id: i64, meta: MixMeta) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> { Ok(Library::open(Path::new(&root))?.set_mix_meta(id, &meta.to_update())?) }).await?
}

fn import_in(lib: &mut Library, bytes: &[u8], fallback_name: &str) -> AppResult<i64> {
    let mut mix = Mix::from_pmk(bytes).map_err(|e| AppError::from(format!("not a mix file: {e}")))?;
    info::relink(lib, &mut mix)?;
    let base = if mix.name.trim().is_empty() { fallback_name.to_string() } else { mix.name.trim().to_string() };
    let taken: HashSet<String> = lib.mix_infos()?.into_iter().map(|m| m.name).collect();
    let mut name = base.clone();
    let mut n = 1;
    while taken.contains(&name) {
        n += 1;
        name = format!("{base} ({n})");
    }
    mix.name = name.clone();
    let id = lib.create_mix(&name)?;
    if let Err(e) = lib.save_mix(id, &mix) {
        let _ = lib.delete_mix(id); // do not leave an empty mix behind
        return Err(e.into());
    }
    Ok(id)
}

/// Imports a `.pmk` mix file as a new mix and returns its id. Its tracks are matched to the
/// library by title and artist (the ids inside the file belong to the library it came from);
/// the import is refused, listing them, if some are missing. A name already in use gets " (2)".
#[tauri::command]
pub async fn mix_import_pmk(root: String, path: String) -> AppResult<i64> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<i64> {
        let bytes = std::fs::read(&path)?;
        let stem = Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported mix".into());
        import_in(&mut Library::open(Path::new(&root))?, &bytes, &stem)
    })
    .await?
}

/// Writes the saved mix as a `.pmk` file at `path` (overwriting; the caller asks first).
#[tauri::command]
pub async fn mix_export_pmk(root: String, id: i64, path: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let mix = Library::open_read_only(Path::new(&root))?.read_mix(id)?;
        let bytes = mix.to_pmk().map_err(|e| AppError::from(e.to_string()))?;
        std::fs::write(&path, bytes)?;
        Ok(())
    })
    .await?
}

/// Library facts and beat grids for tracks (for adding them to a mix, or refreshing a clip's
/// `track`), in the order asked. Unknown ids come back with `available: false`.
#[tauri::command]
pub async fn mix_track_info(root: String, track_ids: Vec<i64>) -> AppResult<Vec<TrackInfo>> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<TrackInfo>> {
        Ok(info::track_infos(&Library::open_read_only(Path::new(&root))?, &track_ids)?)
    })
    .await?
}

/// A new clip for a library track, ready to be positioned and added to a document: channel 0 at
/// timeline 0, speed set for `tempo_bpm` (the mix tempo) when given, with the track's grid.
#[tauri::command]
pub async fn mix_make_clip(root: String, track_id: i64, tempo_bpm: Option<f64>) -> AppResult<Clip> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Clip> {
        Ok(info::make_clip(&Library::open_read_only(Path::new(&root))?, track_id, tempo_bpm)?)
    })
    .await?
}

/// Grids for clips whose `track` the caller did not fill.
fn with_grids(root: &str, mut doc: MixDoc) -> AppResult<MixDoc> {
    if doc.clips.iter().any(|c| c.track.as_ref().is_none_or(|t| t.grid.is_none())) {
        let lib = Library::open_read_only(Path::new(root))?;
        let ids: Vec<i64> = doc.clips.iter().map(|c| c.track_id).collect();
        let infos = info::track_infos(&lib, &ids)?;
        for (c, i) in doc.clips.iter_mut().zip(infos) {
            if c.track.as_ref().is_none_or(|t| t.grid.is_none()) {
                c.track = Some(i);
            }
        }
    }
    doc.refresh();
    Ok(doc)
}

/// Suggests how to align clip `clip_id` to the clip before it: a constant tempo curve that
/// brings it to the earlier clip's tempo, and a start position that puts its beat nearest the
/// cue on a matching beat (and bar, when both grids have a downbeat). Changes nothing.
#[tauri::command]
pub async fn mix_beatmatch(root: String, doc: MixDoc, clip_id: String) -> AppResult<BeatmatchSuggestion> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<BeatmatchSuggestion> {
        Ok(timing::beatmatch(&with_grids(&root, doc)?, &clip_id)?)
    })
    .await?
}

/// Suggests a transition between two clips: a 16-bar (or the largest that fits) linear
/// crossfade starting on the incoming clip's first beat, plus a bass-swap EQ tool for each clip.
/// Changes nothing.
#[tauri::command]
pub async fn mix_auto_transition(root: String, doc: MixDoc, from_clip_id: String, to_clip_id: String) -> AppResult<TransitionSuggestion> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<TransitionSuggestion> {
        Ok(timing::auto_transition(&with_grids(&root, doc)?, &from_clip_id, &to_clip_id)?)
    })
    .await?
}

/// This area's command paths, one per line, written `$crate::commands::mixes::name,`.
#[macro_export]
macro_rules! mixes_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::mixes::mix_list,
                $crate::commands::mixes::mix_open,
                $crate::commands::mixes::mix_new,
                $crate::commands::mixes::mix_save,
                $crate::commands::mixes::mix_validate,
                $crate::commands::mixes::mix_rename,
                $crate::commands::mixes::mix_delete,
                $crate::commands::mixes::mix_set_meta,
                $crate::commands::mixes::mix_import_pmk,
                $crate::commands::mixes::mix_export_pmk,
                $crate::commands::mixes::mix_track_info,
                $crate::commands::mixes::mix_make_clip,
                $crate::commands::mixes::mix_beatmatch,
                $crate::commands::mixes::mix_auto_transition,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::doc::tests::synthetic_mix;
    use super::info::support::{cleanup, library};
    use super::*;
    use pm_formats::mix as m;

    fn real_fixtures() -> Vec<(String, Mix)> {
        // The Editor's bundled Example Mix: raw XML under the gitignored work/ tree of the main
        // checkout (this may be a worktree of it). Skipped silently when absent.
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut out = Vec::new();
        for up in ["../..", "../../../../.."] {
            for f in ["work/inv/example.xml", "work/samples/bin.pmk.xml", "work/samples/editor-db-mix.xml"] {
                let p = here.join(up).join(f);
                if let Ok(xml) = std::fs::read(&p)
                    && let Ok(mix) = Mix::parse_exact(&xml)
                    && !out.iter().any(|(_, m): &(String, Mix)| *m == mix)
                {
                    out.push((f.to_string(), mix));
                }
            }
        }
        out
    }

    #[test]
    fn real_editor_mix_round_trips_through_the_doc() {
        let fixtures = real_fixtures();
        if fixtures.is_empty() {
            eprintln!("skipped: no Editor example mix under work/");
            return;
        }
        for (name, mix) in fixtures {
            let doc = MixDoc::from_mix(1, &mix);
            assert_eq!(doc.to_mix().unwrap(), mix, "{name}");
            assert_eq!(doc.to_mix().unwrap().to_xml().unwrap(), mix.to_xml().unwrap(), "{name}: bytes");
            let json = serde_json::to_string(&doc).unwrap();
            let back: MixDoc = serde_json::from_str(&json).unwrap();
            assert_eq!(back.to_mix().unwrap().to_blob().unwrap(), mix.to_blob().unwrap(), "{name}: via JSON");
            // the Editor's own document is something we would let a user save
            let known: HashSet<i64> = doc.clips.iter().map(|c| c.track_id).collect();
            assert_eq!(doc.validate(Some(&known)), Vec::<String>::new(), "{name}");
            // geometry: the two transitions link the three tracks in order
            assert_eq!(doc.clips.len(), 3);
            let links: Vec<_> = doc.transitions.iter().map(|t| t.between.clone().unwrap()).collect();
            assert_eq!((&links[0].from, &links[0].to), (&doc.clips[0].id, &doc.clips[1].id));
            assert_eq!((&links[1].from, &links[1].to), (&doc.clips[1].id, &doc.clips[2].id));
            // the mix tempo comes out near 128 (the demo is a 128 BPM mix)
            assert!((doc.tempo_bpm.unwrap() - 128.0).abs() < 0.1, "{:?}", doc.tempo_bpm);
        }
    }

    #[test]
    fn open_save_unchanged_keeps_the_blob_and_edits_land() {
        let (mut lib, dirs, ids) = library("cmd");
        let id = lib.create_mix("Command Test").unwrap();
        // a mix over the two library tracks
        let mut mix = synthetic_mix();
        mix.name = "Command Test".into();
        mix.tracks.truncate(2);
        mix.transitions.truncate(1);
        mix.tracks[0].library_id = ids[0];
        mix.tracks[1].library_id = ids[1];
        mix.tracks[0].processes.retain(|p| matches!(p, m::Process::Tool(_)));
        lib.save_mix(id, &mix).unwrap();
        let before = lib.read_mix(id).unwrap();

        let doc = open_in(&lib, id).unwrap();
        assert_eq!(doc.meta.as_ref().unwrap().rating, 0);
        assert!(doc.clips.iter().all(|c| c.track.as_ref().is_some_and(|t| t.available && t.grid.is_some())));
        assert_eq!(doc.id, id);

        // unchanged save: identical blob, rows re-derived
        let saved = save_in(&mut lib, id, &doc).unwrap();
        assert_eq!(lib.read_mix(id).unwrap(), before);
        assert_eq!((saved.track_count, saved.name.as_str()), (2, "Command Test"));

        // through JSON, as the UI does it
        let json = serde_json::to_string(&doc).unwrap();
        let doc2: MixDoc = serde_json::from_str(&json).unwrap();
        save_in(&mut lib, id, &doc2).unwrap();
        assert_eq!(lib.read_mix(id).unwrap(), before);

        // an edit lands: move a clip, rename, drop the transition
        let mut edited = doc.clone();
        edited.clips[1].span.start += 44_100.0;
        edited.name = "Command Test 2".into();
        edited.transitions.clear();
        save_in(&mut lib, id, &edited).unwrap();
        let reread = lib.read_mix(id).unwrap();
        assert_eq!(reread.name, "Command Test 2");
        assert_eq!(reread.tracks[1].position.start, mix.tracks[1].position.start + 44_100.0);
        assert!(reread.transitions.is_empty());
        assert_eq!(lib.mix_info(id).unwrap().unwrap().name, "Command Test 2");
        assert!(lib.compatibility_problems().unwrap().is_empty());

        // refused: wrong mix, unknown track, overlap
        assert!(save_in(&mut lib, id + 1, &doc).is_err());
        let mut bad = doc.clone();
        bad.clips[0].track_id = 4242;
        let e = save_in(&mut lib, id, &bad).unwrap_err().to_string();
        assert!(e.contains("4242"), "{e}");
        let mut bad = doc.clone();
        bad.clips[1].channel = 0;
        bad.transitions.clear();
        assert!(save_in(&mut lib, id, &bad).unwrap_err().to_string().contains("overlap on channel"));
        assert_eq!(lib.read_mix(id).unwrap(), reread, "a refused save changes nothing");
        cleanup(lib, dirs);
    }

    #[test]
    fn import_export_pmk_and_name_clash() {
        let (mut lib, dirs, ids) = library("pmk");
        let id = lib.create_mix("Original").unwrap();
        let mut mix = synthetic_mix();
        mix.name = "Original".into();
        mix.tracks.truncate(2);
        mix.transitions.truncate(1);
        for (i, t) in mix.tracks.iter_mut().enumerate() {
            t.library_id = ids[i];
            t.fuzzy_id = m::FuzzyId { artist: format!("Artist {i}"), album: "Album".into(), title: format!("Title {i}"), genre: String::new() };
        }
        lib.save_mix(id, &mix).unwrap();
        let pmk = lib.read_mix(id).unwrap().to_pmk().unwrap();

        let new_id = import_in(&mut lib, &pmk, "file").unwrap();
        assert_ne!(new_id, id);
        let imported = lib.read_mix(new_id).unwrap();
        assert_eq!(imported.name, "Original (2)");
        assert_eq!(imported.library_ids(), vec![ids[0], ids[1]]);
        assert_eq!(imported.tracks[0].position, mix.tracks[0].position, "everything else is untouched");
        let third = import_in(&mut lib, &pmk, "file").unwrap();
        assert_eq!(lib.read_mix(third).unwrap().name, "Original (3)");

        // a mix with a track we do not have is refused and leaves no empty mix behind
        let mut foreign = mix.clone();
        foreign.tracks[1].fuzzy_id.title = "Nowhere".into();
        let before = lib.mix_infos().unwrap().len();
        let e = import_in(&mut lib, &foreign.to_pmk().unwrap(), "f").unwrap_err().to_string();
        assert!(e.contains("Nowhere"), "{e}");
        assert_eq!(lib.mix_infos().unwrap().len(), before);
        assert!(import_in(&mut lib, b"not a mix", "f").is_err());

        // a name another mix uses is a live validation problem, not only a save-time refusal
        let mut clash = open_in(&lib, third).unwrap();
        assert!(problems_in(&lib, &clash).unwrap().is_empty());
        clash.name = "Original (2)".into();
        let p = problems_in(&lib, &clash).unwrap();
        assert!(p.iter().any(|x| x.contains("already exists")), "{p:?}");
        assert!(save_in(&mut lib, third, &clash).unwrap_err().to_string().contains("already exists"));
        cleanup(lib, dirs);
    }
}
