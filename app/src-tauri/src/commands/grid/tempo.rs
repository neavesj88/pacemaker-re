//! Tempo edits on many tracks at once: halve, double, or set a constant BPM.
//!
//! A track's BPM is not a free field. `tracks.bpm` is derived from the `.str` beat grid
//! (`pm_formats::library::track_bpm`, the median local tempo) and the device plays the grid, so
//! editing the number alone would leave the row and the file describing different tempi. Every
//! edit here therefore goes through the grid editor's own maths (`ops_grid::apply`, the same
//! `Halve`, `Double` and `SetBpm` edits the grid editor offers) and is saved through
//! `Library::save_grid`, which rewrites the `.str` atomically and updates the row from it.
//!
//! [`plan`] is the single piece of code that decides what happens to each track. The read-only
//! preview command ([`grid_tempo_plan`]) and the write job ([`grid_tempo_apply`]) both call it, so
//! the confirmation lists exactly what the job then does.

use std::path::Path;

use pm_formats::library::{track_bpm, Library};
use pm_formats::ops_grid::{self, EditCtx, GridDoc, GridEdit};
use pm_formats::strfile::Analysis;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::commands::analysis::{audio_path, label_of};
use crate::error::{AppResult, IntoApp};
use crate::events::{self, JOB_PROGRESS};
use crate::jobs::{spawn_write_job, JobOutcome, Progress};
use crate::state::JobLock;

/// The job kind string of tempo edits (`jobs.ask({ kind: "tempo-edit" })` on the frontend).
pub const TEMPO_JOB: &str = "tempo-edit";

/// What to do to the tempo of every named track.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TempoOp {
    /// Half the tempo (the grid editor's "/2 BPM": every second beat is kept).
    Halve,
    /// Double the tempo (the grid editor's "x2 BPM": a beat is added midway between every two).
    Double,
    /// A constant grid at this tempo from the first beat (the grid editor's "Set BPM").
    Set { bpm: f64 },
}

impl TempoOp {
    fn edit(self) -> GridEdit {
        match self {
            TempoOp::Halve => GridEdit::Halve,
            TempoOp::Double => GridEdit::Double,
            TempoOp::Set { bpm } => GridEdit::SetBpm { bpm },
        }
    }
}

/// What will happen to one track.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TempoRow {
    pub track_id: i64,
    pub label: String,
    /// The tempo stored now (median of the grid), `None` when there is no readable grid.
    pub before: Option<f64>,
    /// The tempo after the edit, `None` when the track is skipped.
    pub after: Option<f64>,
    /// Why the track is left alone; `None` when it will be changed.
    pub skip: Option<String>,
}

/// A planned change: the row shown to the user and, when the track is to be changed, the grid
/// that will be saved.
pub struct Planned {
    pub row: TempoRow,
    pub doc: Option<GridDoc>,
}

fn skipped(track_id: i64, label: String, before: Option<f64>, why: impl Into<String>) -> Planned {
    Planned { row: TempoRow { track_id, label, before, after: None, skip: Some(why.into()) }, doc: None }
}

/// Decides, for each id (duplicates dropped, order kept), what the edit does. Reads only.
/// A track is skipped, with the reason in words, when it is not in the library, its stored
/// location is not a plain path below `/pmdb_tracks/`, it has no readable `.str`, the edit is
/// refused (a tempo outside 20 to 400 BPM) or the grid would not change.
pub fn plan(lib: &Library, ids: &[i64], op: TempoOp) -> AppResult<Vec<Planned>> {
    if let TempoOp::Set { bpm } = op
        && (!bpm.is_finite() || !(ops_grid::MIN_BPM..=ops_grid::MAX_BPM).contains(&bpm))
    {
        return Err(format!("The tempo must be between {} and {} BPM.", ops_grid::MIN_BPM, ops_grid::MAX_BPM).into());
    }
    let tracks = lib.tracks()?;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(t) = tracks.iter().find(|t| t.track_id == id) else {
            out.push(skipped(id, format!("track {id}"), None, "not in the library"));
            continue;
        };
        let label = label_of(t);
        // `load_grid` and `save_grid` rebuild the `.str` path with `Library::track_paths`, which
        // does not refuse `..`: check the stored location with the validated resolver first
        if let Err(e) = audio_path(lib.root(), &t.location) {
            out.push(skipped(id, label, None, e.to_string()));
            continue;
        }
        let load = match lib.load_grid(id) {
            Ok(l) => l,
            Err(e) => {
                out.push(skipped(id, label, None, e.to_string()));
                continue;
            }
        };
        let Some(doc) = load.doc else {
            let why = load.problem.unwrap_or_else(|| "no readable beat grid".into());
            out.push(skipped(id, label, None, format!("{why}; re-analyse it first")));
            continue;
        };
        let before = track_bpm(&Analysis::from(&doc));
        // the same context the grid editor passes with its edits
        let ctx = EditCtx { sample_rate: load.sample_rate, total_frames: (load.total_frames > 0).then_some(load.total_frames) };
        match ops_grid::apply(&doc, op.edit(), ctx) {
            Err(e) => out.push(skipped(id, label, Some(before), e.to_string().trim_start_matches("invalid: ").to_string())),
            Ok(next) if next == doc => out.push(skipped(id, label, Some(before), "the grid is already like that")),
            Ok(next) => {
                let after = track_bpm(&Analysis::from(&next));
                out.push(Planned { row: TempoRow { track_id: id, label, before: Some(before), after: Some(after), skip: None }, doc: Some(next) });
            }
        }
    }
    Ok(out)
}

/// What the edit would do to each track, without writing anything. The confirmation shows this.
#[tauri::command]
pub async fn grid_tempo_plan(root: String, track_ids: Vec<i64>, op: TempoOp) -> AppResult<Vec<TempoRow>> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        Ok(plan(&lib, &track_ids, op)?.into_iter().map(|p| p.row).collect())
    })
    .await
    .app()?
}

/// Starts the "tempo-edit" write job: plans the edit again with the same code as the preview,
/// then saves each changed grid (the `.str` and the row's BPM together). Tracks that are skipped
/// are reported, not failed. One progress event per track; `bpm` carries the new tempo.
#[tauri::command]
pub fn grid_tempo_apply(app: AppHandle, lock: State<'_, JobLock>, root: String, track_ids: Vec<i64>, op: TempoOp) -> AppResult<()> {
    if track_ids.is_empty() {
        return Err("No tracks to change.".into());
    }
    spawn_write_job(app, &lock, TEMPO_JOB, root.clone(), move |app| {
        let mut lib = Library::open(Path::new(&root))?;
        run(&mut lib, &track_ids, op, |p| events::emit(app, JOB_PROGRESS, p))
    })
}

/// The body of the job, apart from Tauri so it can be tested on a temp library.
fn run(lib: &mut Library, ids: &[i64], op: TempoOp, mut progress: impl FnMut(Progress)) -> AppResult<JobOutcome> {
    // refuse before touching anything: a library the device cannot read must not get new .str files
    let problems = lib.compatibility_problems()?;
    if !problems.is_empty() {
        return Err(format!("music.db is not device-compatible, nothing was changed: {}", problems.join("; ")).into());
    }
    let planned = plan(lib, ids, op)?;
    let total = planned.len();
    let (mut ok, mut failed, mut skipped_n) = (0usize, 0usize, 0usize);
    for (i, p) in planned.into_iter().enumerate() {
        let done = i + 1;
        let mut ev = Progress::new(TEMPO_JOB, p.row.label.clone(), done, total, "");
        match (&p.doc, &p.row.skip) {
            (Some(doc), None) => match lib.save_grid(p.row.track_id, doc) {
                Ok(()) => {
                    ev.status = "retimed".into();
                    ev.bpm = p.row.after;
                    ok += 1;
                }
                Err(e) => {
                    ev.status = "failed".into();
                    ev.error = Some(e.to_string());
                    failed += 1;
                }
            },
            (_, why) => {
                ev.status = "skipped".into();
                ev.detail = why.clone();
                skipped_n += 1;
            }
        }
        progress(ev);
    }
    let problems = lib.compatibility_problems()?;
    if !problems.is_empty() {
        return Err(format!("music.db is no longer device-compatible: {}", problems.join("; ")).into());
    }
    let mut parts = Vec::new();
    if ok > 0 {
        parts.push(format!("{ok} track{} retimed", if ok == 1 { "" } else { "s" }));
    }
    if skipped_n > 0 {
        parts.push(format!("{skipped_n} skipped"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    let summary = if parts.is_empty() { "nothing to do".to_string() } else { parts.join(", ") };
    Ok(JobOutcome { ok, failed, summary })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use std::path::PathBuf;

    const SR: u32 = 44_100;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-app-tempo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A constant grid at `bpm` over `secs` seconds, first beat at `first` frames.
    fn constant(bpm: f64, first: i64, secs: i64) -> Analysis {
        let period = f64::from(SR) * 60.0 / bpm;
        let n = ((secs * i64::from(SR) - first) as f64 / period).floor() as usize + 1;
        Analysis {
            normalize_factor: 1.0,
            beat_start_offset: first,
            beats: (0..n).map(|i| pm_formats::strfile::Beat { pos: first + (i as f64 * period).round() as i64, bpm }).collect(),
            first_downbeat: Some(0),
        }
    }

    /// A library of tracks, one per `(bpm, secs)`. The audio is not decodable and does not need
    /// to be: tempo edits work on the `.str` and the row.
    fn library(tag: &str, specs: &[(f64, i64)]) -> (Library, PathBuf, Vec<i64>) {
        let base = temp_dir(tag);
        let vol = base.join("vol");
        let src = base.join("src");
        std::fs::create_dir_all(&vol).unwrap();
        std::fs::create_dir_all(&src).unwrap();
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, (bpm, secs)) in specs.iter().enumerate() {
            let p = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&p, vec![0u8; 2048 + i]).unwrap();
            // the stored length and rate are what the edits take their limits from
            let meta = TrackMeta { title: format!("Synthetic {i}"), format: "MP3".into(), sample_rate: i64::from(SR), play_time_secs: *secs, ..Default::default() };
            ids.push(lib.add_track(&p, &meta, &constant(*bpm, 3000, *secs)).unwrap());
        }
        (lib, base, ids)
    }

    fn bpm_of(lib: &Library, id: i64) -> f64 {
        lib.tracks().unwrap().into_iter().find(|t| t.track_id == id).unwrap().bpm
    }

    fn grid_of(lib: &Library, id: i64) -> GridDoc {
        lib.load_grid(id).unwrap().doc.expect("a readable grid")
    }

    fn cleanup(lib: Library, base: PathBuf) {
        drop(lib); // Windows will not delete a database that is still open
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn the_op_arrives_as_the_frontend_sends_it() {
        assert_eq!(serde_json::from_str::<TempoOp>(r#"{"kind":"halve"}"#).unwrap(), TempoOp::Halve);
        assert_eq!(serde_json::from_str::<TempoOp>(r#"{"kind":"double"}"#).unwrap(), TempoOp::Double);
        assert_eq!(serde_json::from_str::<TempoOp>(r#"{"kind":"set","bpm":127.5}"#).unwrap(), TempoOp::Set { bpm: 127.5 });
    }

    #[test]
    fn halve_and_double_keep_the_row_and_the_grid_in_step() {
        let (mut lib, base, ids) = library("halve", &[(172.0, 20), (90.0, 20)]);
        let mut events = Vec::new();
        let out = run(&mut lib, &ids, TempoOp::Halve, |p| events.push((p.status.clone(), p.bpm))).unwrap();
        assert_eq!((out.ok, out.failed), (2, 0), "{}", out.summary);
        assert_eq!(events.len(), 2, "one event per track");
        for (id, want) in ids.iter().zip([86.0, 45.0]) {
            assert!((bpm_of(&lib, *id) - want).abs() < 1e-9, "row bpm of {id}");
            let doc = grid_of(&lib, *id);
            assert!((track_bpm(&Analysis::from(&doc)) - want).abs() < 1e-9, "the .str says the same tempo as the row");
        }
        assert_eq!(events[0].1, Some(86.0), "progress carries the new tempo");
        assert!(lib.compatibility_problems().unwrap().is_empty());

        // and back: the doubled grid has the original tempo again
        let back = run(&mut lib, &ids, TempoOp::Double, |_| {}).unwrap();
        assert_eq!((back.ok, back.failed), (2, 0), "{}", back.summary);
        assert!((bpm_of(&lib, ids[0]) - 172.0).abs() < 1e-9);
        assert!((bpm_of(&lib, ids[1]) - 90.0).abs() < 1e-9);
        cleanup(lib, base);
    }

    #[test]
    fn the_plan_is_what_the_job_does() {
        let (mut lib, base, ids) = library("plan", &[(100.0, 30), (240.0, 30), (30.0, 30)]);
        // 240 x 2 = 480 is over the 400 BPM maximum, so that track is refused and the others go through
        let rows = plan(&lib, &ids, TempoOp::Double).unwrap().into_iter().map(|p| p.row).collect::<Vec<_>>();
        assert_eq!(rows[0].after, Some(200.0));
        assert!(rows[1].skip.as_deref().unwrap().contains("above the maximum"), "{:?}", rows[1]);
        assert_eq!(rows[2].after, Some(60.0));

        let out = run(&mut lib, &ids, TempoOp::Double, |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (2, 0), "{}", out.summary);
        assert!(out.summary.contains("1 skipped"), "{}", out.summary);
        // the tempi in the rows are exactly the previewed ones, the skipped track is untouched
        assert_eq!(bpm_of(&lib, ids[0]), rows[0].after.unwrap());
        assert_eq!(bpm_of(&lib, ids[1]), 240.0);
        assert_eq!(bpm_of(&lib, ids[2]), rows[2].after.unwrap());
        cleanup(lib, base);
    }

    #[test]
    fn halving_below_the_minimum_is_skipped_with_the_reason() {
        let (lib, base, ids) = library("min", &[(30.0, 20)]);
        let rows = plan(&lib, &ids, TempoOp::Halve).unwrap();
        assert_eq!(rows[0].row.after, None);
        assert!(rows[0].row.skip.as_deref().unwrap().contains("below the minimum"), "{:?}", rows[0].row);
        cleanup(lib, base);
    }

    #[test]
    fn set_replaces_the_grid_with_a_constant_one_from_the_first_beat() {
        let (mut lib, base, ids) = library("set", &[(120.0, 30)]);
        let before = grid_of(&lib, ids[0]);
        let out = run(&mut lib, &ids, TempoOp::Set { bpm: 127.5 }, |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (1, 0), "{}", out.summary);
        let after = grid_of(&lib, ids[0]);
        assert_eq!(after.beats[0].pos, before.beats[0].pos, "the first beat does not move");
        assert!(after.beats.iter().all(|b| b.bpm == 127.5));
        assert_eq!(bpm_of(&lib, ids[0]), 127.5);
        let gap = f64::from(SR) * 60.0 / 127.5;
        let g1 = (after.beats[1].pos - after.beats[0].pos) as f64;
        assert!((g1 - gap).abs() <= 1.0, "beats are one period apart");
        // setting the tempo it already has changes nothing and says so
        let again = plan(&lib, &ids, TempoOp::Set { bpm: 127.5 }).unwrap();
        assert_eq!(again[0].row.skip.as_deref(), Some("the grid is already like that"));
        cleanup(lib, base);
    }

    #[test]
    fn set_refuses_a_tempo_outside_the_range_before_looking_at_any_track() {
        let (lib, base, ids) = library("range", &[(120.0, 20)]);
        for bad in [0.0, 19.9, 400.1, f64::NAN, f64::INFINITY, -120.0] {
            assert!(plan(&lib, &ids, TempoOp::Set { bpm: bad }).is_err(), "{bad} must be refused");
        }
        assert!(plan(&lib, &ids, TempoOp::Set { bpm: 20.0 }).is_ok());
        assert!(plan(&lib, &ids, TempoOp::Set { bpm: 400.0 }).is_ok());
        cleanup(lib, base);
    }

    #[test]
    fn unknown_duplicate_and_unreadable_tracks_are_handled_one_by_one() {
        let (mut lib, base, ids) = library("odd", &[(100.0, 20), (110.0, 20)]);
        // wreck one track's .str
        let location = lib.tracks().unwrap().into_iter().find(|t| t.track_id == ids[1]).unwrap().location;
        let (_, sidecar) = lib.track_paths(&location).unwrap();
        std::fs::write(&sidecar, b"").unwrap();

        let asked = [ids[0], ids[0], 987_654, ids[1]];
        let rows = plan(&lib, &asked, TempoOp::Double).unwrap().into_iter().map(|p| p.row).collect::<Vec<_>>();
        assert_eq!(rows.len(), 3, "a repeated id is planned once");
        assert_eq!(rows[0].after, Some(200.0));
        assert_eq!(rows[1].skip.as_deref(), Some("not in the library"));
        assert!(rows[2].skip.as_deref().unwrap().contains("empty .str"), "{:?}", rows[2]);

        let out = run(&mut lib, &asked, TempoOp::Double, |_| {}).unwrap();
        assert_eq!((out.ok, out.failed), (1, 0), "{}", out.summary);
        assert!(out.summary.contains("2 skipped"), "{}", out.summary);
        cleanup(lib, base);
    }

    #[test]
    fn a_variable_tempo_grid_follows_the_median_like_every_other_writer() {
        let (mut lib, base, ids) = library("var", &[(100.0, 20)]);
        // replace the grid with one that drifts: the stored tempo is the median local tempo
        let mut doc = grid_of(&lib, ids[0]);
        for (i, b) in doc.beats.iter_mut().enumerate() {
            b.bpm = 100.0 + (i % 3) as f64;
        }
        lib.save_grid(ids[0], &doc).unwrap();
        let first = track_bpm(&Analysis::from(&doc));
        assert_eq!(bpm_of(&lib, ids[0]), first, "saving a grid stores its median tempo");
        let out = run(&mut lib, &ids, TempoOp::Halve, |_| {}).unwrap();
        assert_eq!(out.ok, 1, "{}", out.summary);
        let after = grid_of(&lib, ids[0]);
        assert_eq!(bpm_of(&lib, ids[0]), track_bpm(&Analysis::from(&after)), "row and .str agree after the edit");
        assert!(bpm_of(&lib, ids[0]) < first * 0.55 && bpm_of(&lib, ids[0]) > first * 0.45, "about half of {first}");
        cleanup(lib, base);
    }
}
