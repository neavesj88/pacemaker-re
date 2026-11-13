//! Running a stem plan: decode each track, push it through the model, write the result files.
//! No Tauri here (progress goes out through a callback), so the whole batch is tested with a
//! mock model on generated audio.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pm_formats::library::Track;
use pm_stems::Error as StemError;
use pm_stems::pipeline::{self, Options, Report};
use pm_stems::provider::StemModel;
use pm_stems::sink::{FileSink, Format, Tags};

use super::plan::{Action, Plan, PlanItem, display_label};
use super::settings::StemSettings;

/// What happened to one planned track.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Written { vocals: Option<String>, instrumental: Option<String>, seconds: f64, report: Report },
    Failed(String),
    Cancelled,
    /// Not run (the plan said skip or refuse).
    NotRun,
}

/// Progress callback payload.
#[derive(Debug, Clone, PartialEq)]
pub enum Event<'a> {
    /// A track is about to be decoded.
    Track { index: usize, count: usize, label: &'a str },
    /// A segment of the current track finished (`segment` 0 = starting).
    Segment { index: usize, segment: usize, segments: usize },
}

/// Text tags for a result file: the source's tags with the role added to the title.
pub fn tags_for(t: &Track, suffix: &str, fallback_label: &str, format: &Format) -> Tags {
    if !format.has_tags() {
        return Tags::default();
    }
    let base = if t.title.trim().is_empty() { fallback_label.to_string() } else { t.title.trim().to_string() };
    Tags {
        title: format!("{base} {suffix}"),
        artist: t.artist.trim().to_string(),
        album: t.album.trim().to_string(),
        year: String::new(),
        comment: "Made with Pacemaker Revival (HTDemucs). Not the original recording.".into(),
    }
}

/// Runs one item. The partial files of a failed or cancelled item are removed by the sinks.
pub fn run_item(
    item: &PlanItem,
    track: &Track,
    s: &StemSettings,
    model: &mut dyn StemModel,
    cancel: &AtomicBool,
    index: usize,
    on_event: &mut dyn FnMut(Event),
) -> Outcome {
    if !matches!(item.action, Action::Make | Action::Replace | Action::Partial) {
        return Outcome::NotRun;
    }
    let started = std::time::Instant::now();
    let format = s.out_format();
    let src = match pm_mixer::audio::load(Path::new(&item.source)) {
        Ok(src) => src,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    if cancel.load(Ordering::SeqCst) {
        return Outcome::Cancelled;
    }
    if s.max_minutes > 0 && src.len() as u64 > u64::from(s.max_minutes) * 60 * u64::from(pm_stems::RATE) {
        return Outcome::Failed(format!("longer than the {}-minute limit in the settings", s.max_minutes));
    }
    if src.is_empty() {
        return Outcome::Failed("the file has no audio".into());
    }
    if src.glitches > 0 {
        // not fatal: the decoder replaced corrupt frames with silence; the result carries the same gaps
    }

    let mut v_sink = None;
    let mut i_sink = None;
    if item.write_vocals {
        match FileSink::create(Path::new(&item.vocals_path), format, &tags_for(track, &s.vocals_suffix, &display_label(track), &format)) {
            Ok(k) => v_sink = Some(k),
            Err(e) => return Outcome::Failed(e.to_string()),
        }
    }
    if item.write_instrumental {
        match FileSink::create(Path::new(&item.instrumental_path), format, &tags_for(track, &s.instrumental_suffix, &display_label(track), &format)) {
            Ok(k) => i_sink = Some(k),
            Err(e) => return Outcome::Failed(e.to_string()),
        }
    }

    let opts = Options { overlap: s.overlap(), instrumental: s.instrumental_mode() };
    let result = pipeline::separate(
        &src.l,
        &src.r,
        model,
        &opts,
        &mut |l, r| v_sink.as_mut().map_or(Ok(()), |k| k.write(l, r)),
        &mut |l, r| i_sink.as_mut().map_or(Ok(()), |k| k.write(l, r)),
        &mut |p| {
            on_event(Event::Segment { index, segment: p.segment, segments: p.segments });
            !cancel.load(Ordering::SeqCst)
        },
    );
    let report = match result {
        Ok(r) => r,
        Err(StemError::Cancelled) => return Outcome::Cancelled,
        Err(e) => return Outcome::Failed(e.to_string()),
    };

    // Both results are complete. A file the plan saw nothing at must still be absent: the
    // separation takes minutes, and the rename below replaces whatever is there.
    for (writes, existed, path) in [(item.write_vocals, item.vocals_exists, &item.vocals_path), (item.write_instrumental, item.instrumental_exists, &item.instrumental_path)] {
        if writes && !existed && Path::new(path).exists() {
            return Outcome::Failed(format!("a file appeared at {path} while this track was being separated; it was left alone"));
        }
    }
    // move them to their final names
    let mut vocals = None;
    let mut instrumental = None;
    if let Some(k) = v_sink {
        match k.finish() {
            Ok(p) => vocals = Some(p.to_string_lossy().into_owned()),
            Err(e) => return Outcome::Failed(format!("could not finish the acapella: {e}")),
        }
    }
    if let Some(k) = i_sink {
        match k.finish() {
            Ok(p) => instrumental = Some(p.to_string_lossy().into_owned()),
            Err(e) => {
                let note = if vocals.is_some() { " (the acapella file was written)" } else { "" };
                return Outcome::Failed(format!("could not finish the instrumental: {e}{note}"));
            }
        }
    }
    Outcome::Written { vocals, instrumental, seconds: started.elapsed().as_secs_f64(), report }
}

/// Runs every runnable item of `plan` in order and stops at the first cancel. Returns one
/// outcome per plan item (`NotRun` for the ones the plan skipped or refused, and `Cancelled` for
/// everything from the cancel on).
pub fn run_batch(
    plan: &Plan,
    tracks: &[Track],
    s: &StemSettings,
    model: &mut dyn StemModel,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(Event),
) -> Vec<Outcome> {
    let count = plan.items.len();
    let mut out = Vec::with_capacity(count);
    let mut stopped = false;
    for (index, item) in plan.items.iter().enumerate() {
        if !matches!(item.action, Action::Make | Action::Replace | Action::Partial) {
            out.push(Outcome::NotRun);
            continue;
        }
        if stopped || cancel.load(Ordering::SeqCst) {
            stopped = true;
            out.push(Outcome::Cancelled);
            continue;
        }
        on_event(Event::Track { index, count, label: &item.label });
        let outcome = match tracks.iter().find(|t| t.track_id == item.track_id) {
            Some(t) => run_item(item, t, s, model, cancel, index, on_event),
            None => Outcome::Failed("the track is no longer in the library".into()),
        };
        stopped = outcome == Outcome::Cancelled;
        out.push(outcome);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::stems::plan::build_plan;
    use pm_formats::library::{Library, TrackMeta};
    use pm_formats::strfile::{Analysis, Beat};
    use pm_stems::mock::{FailingModel, IdentityModel, MidSideModel};
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-stems-run-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Writes a stereo WAV of `secs` seconds (a 440 Hz tone left, 660 Hz right).
    fn write_wav(path: &Path, secs: f32) {
        let n = (secs * 44_100.0) as usize;
        let l: Vec<f32> = (0..n).map(|i| 0.4 * (i as f32 * 0.0627).sin()).collect();
        let r: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.094).sin()).collect();
        let mut k = FileSink::create(path, Format::Wav, &Tags::default()).unwrap();
        k.write(&l, &r).unwrap();
        k.finish().unwrap();
    }

    /// A library of `n` real (WAV) tracks named "Demo Artist - Tune i".
    fn library(tag: &str, n: usize, secs: f32) -> (PathBuf, Vec<i64>) {
        let vol = temp_dir(&format!("{tag}-vol"));
        let src = temp_dir(&format!("{tag}-src"));
        let analysis = Analysis { normalize_factor: 1.0, beat_start_offset: 0, beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(), first_downbeat: Some(0) };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.wav"));
            write_wav(&f, secs + i as f32 * 0.1);
            let meta = TrackMeta { title: format!("Tune {i}"), artist: "Demo Artist".into(), format: "WAV".into(), play_time_secs: secs as i64, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        std::fs::remove_dir_all(&src).unwrap();
        (vol, ids)
    }

    struct Fixture {
        lib: Library,
        ids: Vec<i64>,
        plan: Plan,
        tracks: Vec<Track>,
        dest: PathBuf,
    }

    fn setup(tag: &str, n: usize, s: &StemSettings) -> Fixture {
        let (vol, ids) = library(tag, n, 3.0);
        let dest = temp_dir(&format!("{tag}-dest"));
        let lib = Library::open_read_only(&vol).unwrap();
        let plan = build_plan(&lib, &ids, &dest, s).unwrap();
        let tracks = lib.tracks().unwrap();
        Fixture { lib, ids, plan, tracks, dest }
    }

    #[test]
    fn a_batch_writes_both_files_per_track_and_they_decode() {
        let s = StemSettings { format: "wav".into(), ..Default::default() };
        let Fixture { plan, tracks, dest, .. } = setup("batch", 2, &s);
        let mut events = Vec::new();
        let out = run_batch(&plan, &tracks, &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), &mut |e| events.push(format!("{e:?}")));
        assert_eq!(out.len(), 2);
        for (o, item) in out.iter().zip(&plan.items) {
            match o {
                Outcome::Written { vocals, instrumental, report, .. } => {
                    assert_eq!(vocals.as_deref(), Some(item.vocals_path.as_str()));
                    assert_eq!(instrumental.as_deref(), Some(item.instrumental_path.as_str()));
                    assert!(!report.silent && report.segments >= 2);
                }
                other => panic!("{other:?}"),
            }
            // 44-byte header + 4 bytes per frame; the source is 3.0 / 3.1 s
            let frames = (std::fs::metadata(&item.vocals_path).unwrap().len() - 44) / 4;
            assert_eq!(frames, pm_mixer::audio::load(Path::new(&item.source)).unwrap().len() as u64, "the stem has the length of its source");
            assert!(std::fs::metadata(&item.instrumental_path).unwrap().len() > 44);
        }
        assert!(events.iter().any(|e| e.starts_with("Track { index: 1, count: 2")));
        assert!(events.iter().any(|e| e.contains("Segment { index: 0, segment: 0")));
        let mut names: Vec<_> = std::fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, vec!["Demo Artist - Tune 0 (Acapella).wav", "Demo Artist - Tune 0 (Instrumental).wav", "Demo Artist - Tune 1 (Acapella).wav", "Demo Artist - Tune 1 (Instrumental).wav"]);
    }

    #[test]
    fn mp3_results_carry_tags_and_the_role_in_the_title() {
        let s = StemSettings::default();
        let Fixture { plan, tracks, .. } = setup("mp3", 1, &s);
        let out = run_batch(&plan, &tracks, &s, &mut MidSideModel::new(44_100), &AtomicBool::new(false), &mut |_| {});
        let Outcome::Written { vocals, .. } = &out[0] else { panic!("{out:?}") };
        let bytes = std::fs::read(vocals.as_ref().unwrap()).unwrap();
        assert_eq!(&bytes[..3], b"ID3", "an ID3v2 tag comes first");
        let head = String::from_utf8_lossy(&bytes[..400]).into_owned();
        assert!(head.contains("Tune 0 (Acapella)") && head.contains("Demo Artist"), "{head:?}");
    }

    #[test]
    fn only_the_missing_file_is_written_and_the_existing_one_is_untouched() {
        let s = StemSettings { format: "wav".into(), ..Default::default() };
        let Fixture { lib, ids, plan: plan0, tracks, dest } = setup("partial", 1, &s);
        std::fs::write(&plan0.items[0].vocals_path, b"keep me").unwrap();
        let plan = build_plan(&lib, &ids, &dest, &s).unwrap();
        assert_eq!(plan.items[0].action, Action::Partial);
        let out = run_batch(&plan, &tracks, &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), &mut |_| {});
        let Outcome::Written { vocals, instrumental, .. } = &out[0] else { panic!("{out:?}") };
        assert!(vocals.is_none() && instrumental.is_some());
        assert_eq!(std::fs::read(&plan.items[0].vocals_path).unwrap(), b"keep me");
    }

    #[test]
    fn a_file_that_appears_while_a_track_is_separated_is_not_overwritten() {
        let s = StemSettings { format: "wav".into(), ..Default::default() };
        let Fixture { plan, tracks, .. } = setup("race", 1, &s);
        assert_eq!(plan.items[0].action, Action::Make);
        // the plan saw nothing there; the user (or another program) saves a file during the run
        std::fs::write(&plan.items[0].vocals_path, b"theirs").unwrap();
        let out = run_batch(&plan, &tracks, &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), &mut |_| {});
        assert!(matches!(&out[0], Outcome::Failed(e) if e.contains("appeared")), "{out:?}");
        assert_eq!(std::fs::read(&plan.items[0].vocals_path).unwrap(), b"theirs");
        assert!(!pm_stems::sink::partial_path(Path::new(&plan.items[0].vocals_path)).exists(), "no partial file is left");
        assert!(!Path::new(&plan.items[0].instrumental_path).exists(), "nothing of the pair was finished");
    }

    #[test]
    fn replacing_overwrites_only_after_the_new_file_is_complete() {
        let s = StemSettings { format: "wav".into(), existing: "replace".into(), ..Default::default() };
        let Fixture { lib, ids, plan: plan0, tracks, dest } = setup("replace", 1, &s);
        std::fs::write(&plan0.items[0].vocals_path, b"old vocals").unwrap();
        // the plan is computed again once the file exists, as the real run does
        let plan = build_plan(&lib, &ids, &dest, &s).unwrap();
        assert_eq!(plan.items[0].action, Action::Replace);
        // a model that fails halfway: the old file is still there afterwards
        let out = run_batch(&plan, &tracks, &s, &mut FailingModel::new(44_100, 1), &AtomicBool::new(false), &mut |_| {});
        assert!(matches!(&out[0], Outcome::Failed(e) if e.contains("synthetic model failure")), "{out:?}");
        assert_eq!(std::fs::read(&plan.items[0].vocals_path).unwrap(), b"old vocals");
        assert!(!pm_stems::sink::partial_path(Path::new(&plan.items[0].vocals_path)).exists());
        // and a good run replaces it
        let out = run_batch(&plan, &tracks, &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), &mut |_| {});
        assert!(matches!(out[0], Outcome::Written { .. }));
        assert!(std::fs::metadata(&plan.items[0].vocals_path).unwrap().len() > 1000);
    }

    #[test]
    fn cancel_mid_track_leaves_no_partial_file_and_skips_the_rest() {
        let s = StemSettings { format: "wav".into(), ..Default::default() };
        let Fixture { plan, tracks, dest, .. } = setup("cancel", 3, &s);
        let cancel = AtomicBool::new(false);
        let mut model = IdentityModel::new(44_100);
        // the first track completes; cancel is asked for during the second
        let out = run_batch(&plan, &tracks, &s, &mut model, &cancel, &mut |e| {
            if let Event::Segment { index: 1, segment: 1, .. } = e {
                cancel.store(true, Ordering::SeqCst);
            }
        });
        assert!(matches!(out[0], Outcome::Written { .. }));
        assert_eq!(out[1], Outcome::Cancelled);
        assert_eq!(out[2], Outcome::Cancelled);
        let names: Vec<_> = std::fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "only the finished track's two files remain: {names:?}");
        assert!(names.iter().all(|n| n.contains("Tune 0") && !n.ends_with(".partial")));
    }

    #[test]
    fn one_failing_track_does_not_stop_the_batch() {
        let s = StemSettings { format: "wav".into(), ..Default::default() };
        let Fixture { mut plan, tracks, .. } = setup("fail", 2, &s);
        plan.items[0].source = plan.items[0].source.replace("Music", "MusicGone");
        let out = run_batch(&plan, &tracks, &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), &mut |_| {});
        assert!(matches!(&out[0], Outcome::Failed(_)), "{:?}", out[0]);
        assert!(matches!(out[1], Outcome::Written { .. }));
        assert!(!Path::new(&plan.items[0].vocals_path).exists());
    }

    #[test]
    fn skipped_and_refused_items_are_not_run() {
        let s = StemSettings { format: "wav".into(), max_minutes: 0, ..Default::default() };
        let Fixture { mut plan, tracks, .. } = setup("notrun", 2, &s);
        plan.items[0].action = Action::Skip;
        plan.items[1].action = Action::Refuse;
        let out = run_batch(&plan, &tracks, &s, &mut FailingModel::new(44_100, 0), &AtomicBool::new(false), &mut |_| {});
        assert_eq!(out, vec![Outcome::NotRun, Outcome::NotRun]);
    }

    #[test]
    fn a_track_over_the_length_limit_fails_at_run_time_too() {
        // the plan trusts the stored length; the decoded length is what is held in memory
        let s = StemSettings { format: "wav".into(), max_minutes: 1, ..Default::default() };
        let Fixture { mut plan, tracks, dest, .. } = setup("long", 1, &StemSettings { max_minutes: 0, ..s.clone() });
        let short = run_item(&plan.items[0], &tracks[0], &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), 0, &mut |_| {});
        assert!(matches!(short, Outcome::Written { .. }), "a 3 second track is within a 1 minute limit: {short:?}");
        let big = dest.join("big.wav");
        write_wav(&big, 70.0);
        plan.items[0].source = big.to_string_lossy().into_owned();
        let out = run_item(&plan.items[0], &tracks[0], &s, &mut IdentityModel::new(44_100), &AtomicBool::new(false), 0, &mut |_| {});
        assert!(matches!(&out, Outcome::Failed(e) if e.contains("1-minute limit")), "{out:?}");
    }
}
