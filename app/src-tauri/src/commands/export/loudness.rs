//! Loudness policy for audio exports: the target loudness, per-clip levelling, and the analysis
//! that finds the gains before the export (or a range preview) uses them.
//!
//! The mechanisms live in `pm-mixer` (the BS.1770 meter, `Options::clip_gain_db` /
//! `master_gain_db`, `engine::measure_clips`); this file decides what to do with the readings.
//! Every choice is a field of [`LevelSettings`], which is the user's setting with a default.
//!
//! **How the gains are found**
//!
//! 1. *Per-clip levelling* (optional). Each clip heard in the export is rendered alone, exactly as
//!    the mix plays it (tempo, key, tools and its own volume envelope included, crossfades not),
//!    and its integrated loudness is measured. Its gain is the distance to a reference (the median
//!    of the clips, or a fixed LUFS value), limited to a maximum cut or boost.
//! 2. *Target loudness* (optional). The mix is rendered once with the clip gains and no master
//!    gain, and the loudness of the signal entering the master limiter is measured over the
//!    exported range. The master gain is `target - measured`, limited to a maximum boost and cut
//!    and, if asked, so that the true peak stays under a ceiling. It is applied before the limiter:
//!    the limiter stays the safety net, and when the gain pushes peaks into it the loudness
//!    reached is below the target (the report says so; it never claims the target was reached).
//!
//! Gains are applied by the same engine options in the range preview and in the export, so what is
//! heard in the preview is what is written. The export measures its own output again and reports
//! it. Nothing here touches Tauri or the device.

use pm_formats::mix::Mix;
use pm_mixer::engine::{self, Options, Resolver};
use pm_mixer::loudness::Measure;
use pm_mixer::{Error, PeqMode};
use serde::{Deserialize, Serialize};

use crate::commands::mixes::doc::MixDoc;

use super::range::Resolved;

/// How clips are levelled against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipLevel {
    /// Clips keep the level they have.
    #[default]
    Off,
    /// Every clip is moved toward the median loudness of the clips in the export.
    Median,
    /// Every clip is moved toward `clip_fixed_lufs`.
    Fixed,
}

/// The loudness settings of an export (and of the preview that must match it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelSettings {
    /// Integrated loudness to reach, in LUFS; `None` = no target (the master gain stays 0 dB).
    pub target_lufs: Option<f64>,
    /// The most the master gain may raise the mix, in dB.
    pub max_boost_db: f64,
    /// The most the master gain may lower the mix, in dB.
    pub max_cut_db: f64,
    /// Never raise the mix so far that its true peak passes `peak_ceiling_db` (otherwise the
    /// limiter is allowed to work for the extra loudness).
    pub cap_at_peak: bool,
    /// True-peak ceiling for `cap_at_peak`, in dBTP.
    pub peak_ceiling_db: f64,
    pub clip_level: ClipLevel,
    /// Reference for `ClipLevel::Fixed`, in LUFS.
    pub clip_fixed_lufs: f64,
    /// The most a clip may be raised or lowered by levelling, in dB.
    pub clip_max_db: f64,
}

impl Default for LevelSettings {
    fn default() -> Self {
        LevelSettings { target_lufs: None, max_boost_db: 12.0, max_cut_db: 24.0, cap_at_peak: false, peak_ceiling_db: -1.0, clip_level: ClipLevel::Off, clip_fixed_lufs: -14.0, clip_max_db: 9.0 }
    }
}

impl LevelSettings {
    /// Anything to do at all?
    pub fn active(&self) -> bool {
        self.target_lufs.is_some() || self.clip_level != ClipLevel::Off
    }

    pub fn validate(&self) -> Result<(), String> {
        let finite = |name: &str, v: f64, lo: f64, hi: f64| {
            if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(format!("{name} must be between {lo} and {hi}.")) }
        };
        if let Some(t) = self.target_lufs {
            finite("The target loudness (LUFS)", t, -60.0, 0.0)?;
        }
        finite("The maximum boost (dB)", self.max_boost_db, 0.0, 40.0)?;
        finite("The maximum cut (dB)", self.max_cut_db, 0.0, 60.0)?;
        finite("The true-peak ceiling (dBTP)", self.peak_ceiling_db, -20.0, 0.0)?;
        finite("The clip reference (LUFS)", self.clip_fixed_lufs, -60.0, 0.0)?;
        finite("The maximum clip gain (dB)", self.clip_max_db, 0.0, 30.0)
    }
}

// ---------------------------------------------------------------------------------------------
// the plan
// ---------------------------------------------------------------------------------------------

/// A loudness or peak reading on the wire: `null` where the value is not defined (silence has no
/// loudness and a peak of minus infinity).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
    pub sample_peak_db: Option<f64>,
}

impl From<Measure> for Reading {
    fn from(m: Measure) -> Reading {
        let f = |v: f64| v.is_finite().then_some(round2(v));
        Reading { lufs: m.lufs.and_then(f), true_peak_db: f(m.true_peak_db), sample_peak_db: f(m.sample_peak_db) }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// One clip's row of the levelling table.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipRow {
    pub clip_id: String,
    /// The clip's own integrated loudness (`None`: too short, silent, or its audio is missing).
    pub lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
    /// The gain levelling applies to it.
    pub gain_db: f64,
    pub note: Option<String>,
}

/// What the analysis found and the export will apply.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessReport {
    /// "analysis" (a measuring pass) or "export" (the end of an export).
    pub source: String,
    /// The part of the timeline measured, in seconds.
    pub from_secs: f64,
    pub to_secs: f64,
    /// Levelling table; empty when clip levelling is off.
    pub clips: Vec<ClipRow>,
    /// What the clips were levelled to, when levelling is on.
    pub clip_reference_lufs: Option<f64>,
    /// The mix entering the master limiter, with the clip gains and no master gain.
    pub before_limiter: Option<Reading>,
    /// The mix after the limiter, same conditions.
    pub after_limiter: Option<Reading>,
    pub target_lufs: Option<f64>,
    /// The master gain the export applies, in dB.
    pub master_gain_db: f64,
    /// The highest true peak entering the limiter with the master gain (predicted from
    /// `before_limiter`), in dBTP.
    pub predicted_peak_db: Option<f64>,
    /// With the master gain the signal passes the limiter's ceiling, so the limiter will work and
    /// the loudness reached will be below the target.
    pub limiter_will_act: bool,
    /// The finished file's loudness (only for `source == "export"`).
    pub result: Option<Reading>,
    pub notes: Vec<String>,
    /// Clip gains by clip id and the master gain: pass these to `export_render_preview` to hear
    /// the levels the export will use.
    pub gains: PreviewGains,
}

/// The gains of a plan, as the range preview takes them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PreviewGains {
    /// dB per clip id; a clip that is not listed has 0 dB.
    pub clip_gain_db: std::collections::BTreeMap<String, f64>,
    pub master_gain_db: f64,
}

/// The engine options for a set of gains: `clip_gain_db` is indexed by track, like the engine
/// wants it, from the clip ids of the document.
pub fn apply_gains(doc: &MixDoc, gains: &PreviewGains, opts: &mut Options) {
    opts.clip_gain_db = doc.clips.iter().map(|c| gains.clip_gain_db.get(&c.id).copied().filter(|g| g.is_finite()).unwrap_or(0.0)).collect();
    opts.master_gain_db = if gains.master_gain_db.is_finite() { gains.master_gain_db } else { 0.0 };
}

/// Median of the values (the mean of the two middle ones for an even count).
pub fn median(values: &[f64]) -> Option<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

/// The levelling gain of each measured clip: `(clip index, loudness)` in, `(gain, note)` out, and
/// the reference used. Pure; the unit tests in this file cover it.
pub fn clip_gains(measured: &[(Option<f64>, Option<String>)], s: &LevelSettings) -> (Vec<(f64, Option<String>)>, Option<f64>) {
    let reference = match s.clip_level {
        ClipLevel::Off => None,
        ClipLevel::Fixed => Some(s.clip_fixed_lufs),
        ClipLevel::Median => median(&measured.iter().filter_map(|(l, _)| *l).collect::<Vec<_>>()),
    };
    let rows = measured
        .iter()
        .map(|(lufs, err)| match (reference, lufs) {
            (Some(r), Some(l)) => {
                let want = r - l;
                let g = want.clamp(-s.clip_max_db, s.clip_max_db);
                let note = ((g - want).abs() > 1e-9).then(|| format!("limited to {:+.1} dB (it would need {:+.1})", g, want));
                (g, note)
            }
            (_, None) => (0.0, Some(err.clone().unwrap_or_else(|| "too short or silent to measure; left as it is".into()))),
            (None, Some(_)) => (0.0, None),
        })
        .collect();
    (rows, reference)
}

/// The master gain for a target: `(gain dB, notes)`. Pure.
pub fn master_gain(before: &Measure, s: &LevelSettings) -> (f64, Vec<String>) {
    let mut notes = Vec::new();
    let Some(target) = s.target_lufs else { return (0.0, notes) };
    let Some(now) = before.lufs else {
        notes.push("The mix is silent or shorter than 0.4 seconds, so its loudness cannot be measured and no gain was applied.".into());
        return (0.0, notes);
    };
    let want = target - now;
    let mut g = want.clamp(-s.max_cut_db, s.max_boost_db);
    if (g - want).abs() > 1e-9 {
        notes.push(format!("The gain needed for {target:.1} LUFS is {want:+.1} dB; it was limited to {g:+.1} dB by the maximum {}.", if want > 0.0 { "boost" } else { "cut" }));
    }
    if s.cap_at_peak && before.true_peak_db.is_finite() {
        let room = s.peak_ceiling_db - before.true_peak_db;
        if g > room {
            notes.push(format!("The gain was limited to {room:+.1} dB so that the true peak stays at {:.1} dBTP; the target will not be reached.", s.peak_ceiling_db));
            g = room;
        }
    }
    (round2(g), notes)
}

// ---------------------------------------------------------------------------------------------
// the analysis
// ---------------------------------------------------------------------------------------------

/// What the analysis needs to know about the export it serves.
pub struct Subject<'a> {
    pub mix: &'a Mix,
    pub doc: &'a MixDoc,
    pub res: &'a dyn Resolver,
    pub peq: PeqMode,
    /// Which `Wah` tools are the Vocal filter (`render::vocal_tools_for`); `None` = `peq` for all.
    pub vocal_tools: Option<std::sync::Arc<std::collections::HashSet<String>>>,
    /// The part of the timeline to measure; `None` = the whole export (from the first audible frame).
    pub range: Option<&'a Resolved>,
}

impl Subject<'_> {
    /// Engine options for a render of the subject.
    pub fn options(&self) -> Options {
        match self.range {
            Some(r) => Options { from: r.from, to: Some(r.to), edge_fade_frames: r.edge_fade_frames, peq: self.peq, vocal_tools: self.vocal_tools.clone(), ..Options::default() },
            None => Options { skip_leading_silence: true, peq: self.peq, vocal_tools: self.vocal_tools.clone(), ..Options::default() },
        }
    }
}

/// Runs the measuring passes and works out the gains. `progress` receives 0..1 over all passes and
/// returns `false` to cancel. Needs [`LevelSettings::active`] to be true to be useful, but works
/// (and reports the mix's loudness with no gains) when it is not.
pub fn analyse(subject: &Subject, s: &LevelSettings, source: &str, progress: &mut dyn FnMut(f64) -> bool) -> Result<LoudnessReport, Error> {
    s.validate().map_err(Error::Invalid)?;
    let base = subject.options();
    let mut report = LoudnessReport { source: source.into(), target_lufs: s.target_lufs, ..LoudnessReport::default() };
    let mut gains = PreviewGains::default();

    // pass 1: every clip heard in the export, alone
    let render_share = if s.clip_level == ClipLevel::Off { 1.0 } else { 0.5 };
    if s.clip_level != ClipLevel::Off {
        let measured = engine::measure_clips(subject.mix, subject.res, &base, &mut |p| progress(p * (1.0 - render_share)))?;
        let inputs: Vec<(Option<f64>, Option<String>)> = measured.iter().map(|m| (m.measure.and_then(|x| x.lufs), m.error.clone())).collect();
        let (rows, reference) = clip_gains(&inputs, s);
        for (m, (gain, note)) in measured.iter().zip(rows) {
            let id = subject.doc.clips[m.index].id.clone();
            gains.clip_gain_db.insert(id.clone(), round2(gain));
            let r = m.measure.map(Reading::from);
            report.clips.push(ClipRow { clip_id: id, lufs: r.and_then(|r| r.lufs), true_peak_db: r.and_then(|r| r.true_peak_db), gain_db: round2(gain), note });
        }
        report.clip_reference_lufs = reference.map(round2);
        if reference.is_none() {
            report.notes.push("No clip could be measured, so no clip was levelled.".into());
        }
    }

    // pass 2: the mix with the clip gains and no master gain
    let mut opts = Options { measure: true, ..base };
    apply_gains(subject.doc, &gains, &mut opts);
    opts.master_gain_db = 0.0;
    let offset = 1.0 - render_share;
    let mut sink = |_: &[f32], _: &[f32]| Ok(());
    let rendered = engine::render(subject.mix, subject.res, &opts, &mut |p| progress(offset + p * render_share), &mut sink)?;
    let loud = rendered.loudness.ok_or_else(|| Error::Invalid("the render did not measure".into()))?;
    if rendered.clips_failed > 0 {
        report.notes.push(format!("{} clip(s) could not be loaded and are not in these readings.", rendered.clips_failed));
    }
    report.from_secs = subject.range.map(|r| r.from_secs).or_else(|| engine::export_span(subject.mix).map(|(f, _)| f as f64 / f64::from(pm_mixer::RATE))).unwrap_or(0.0);
    report.to_secs = report.from_secs + rendered.frames as f64 / f64::from(pm_mixer::RATE);
    report.before_limiter = Some(loud.pre_limiter.into());
    report.after_limiter = Some(loud.output.into());

    // the master gain
    let (g, notes) = master_gain(&loud.pre_limiter, s);
    report.notes.extend(notes);
    gains.master_gain_db = g;
    report.master_gain_db = g;
    if loud.pre_limiter.true_peak_db.is_finite() {
        let peak = loud.pre_limiter.true_peak_db + g;
        report.predicted_peak_db = Some(round2(peak));
        let ceiling = 20.0 * f64::from(pm_mixer::master::CEILING).log10();
        report.limiter_will_act = loud.pre_limiter.sample_peak_db + g > ceiling + 1e-6;
        if report.limiter_will_act && s.target_lufs.is_some() {
            report.notes.push(format!(
                "With {g:+.1} dB the peaks pass the limiter's ceiling ({ceiling:.2} dBFS), so the limiter will work and the loudness reached will be below {:.1} LUFS. The finished file's real loudness is reported after the export.",
                s.target_lufs.unwrap_or_default()
            ));
        }
    }
    report.gains = gains;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> LevelSettings {
        LevelSettings::default()
    }

    fn measure(lufs: Option<f64>, tp: f64) -> Measure {
        Measure { lufs, true_peak_db: tp, sample_peak_db: tp }
    }

    #[test]
    fn the_defaults_change_nothing() {
        assert!(!s().active());
        assert!(s().validate().is_ok());
        assert_eq!(master_gain(&measure(Some(-18.0), -3.0), &s()), (0.0, vec![]));
    }

    #[test]
    fn the_master_gain_is_the_distance_to_the_target() {
        let t = |x| LevelSettings { target_lufs: Some(x), ..s() };
        assert_eq!(master_gain(&measure(Some(-18.0), -9.0), &t(-14.0)), (4.0, vec![]));
        assert_eq!(master_gain(&measure(Some(-9.0), -1.0), &t(-14.0)), (-5.0, vec![]));
        assert_eq!(master_gain(&measure(Some(-14.0), -1.0), &t(-14.0)).0, 0.0);
        // rounded to 0.01 dB, the precision the report and the preview use
        assert_eq!(master_gain(&measure(Some(-17.234_567), -9.0), &t(-14.0)).0, 3.23);
    }

    #[test]
    fn the_boost_and_cut_limits_hold_and_say_so() {
        let t = LevelSettings { target_lufs: Some(-9.0), max_boost_db: 6.0, max_cut_db: 3.0, ..s() };
        let (g, notes) = master_gain(&measure(Some(-30.0), -20.0), &t);
        assert_eq!(g, 6.0);
        assert!(notes[0].contains("+21.0") && notes[0].contains("maximum boost"), "{notes:?}");
        let t = LevelSettings { target_lufs: Some(-30.0), ..t };
        let (g, notes) = master_gain(&measure(Some(-9.0), -1.0), &t);
        assert_eq!(g, -3.0);
        assert!(notes[0].contains("maximum cut"), "{notes:?}");
    }

    #[test]
    fn capping_at_the_peak_ceiling_stops_the_boost_where_the_peak_reaches_it() {
        let t = LevelSettings { target_lufs: Some(-9.0), cap_at_peak: true, peak_ceiling_db: -1.0, ..s() };
        // needs +8 dB for the target but the true peak is at -4.5 dBTP: only 3.5 dB of room
        let (g, notes) = master_gain(&measure(Some(-17.0), -4.5), &t);
        assert_eq!(g, 3.5);
        assert!(notes[0].contains("will not be reached"), "{notes:?}");
        // without the cap the limiter is left to work
        let free = LevelSettings { cap_at_peak: false, ..t };
        assert_eq!(master_gain(&measure(Some(-17.0), -4.5), &free).0, 8.0);
        // cutting is never held back by the ceiling
        assert_eq!(master_gain(&measure(Some(-5.0), 0.0), &t).0, -4.0);
    }

    #[test]
    fn a_silent_mix_gets_no_gain_and_a_reason() {
        let t = LevelSettings { target_lufs: Some(-14.0), ..s() };
        let (g, notes) = master_gain(&measure(None, f64::NEG_INFINITY), &t);
        assert_eq!(g, 0.0);
        assert!(notes[0].contains("cannot be measured"));
    }

    #[test]
    fn the_median_of_an_even_and_an_odd_count() {
        assert_eq!(median(&[-10.0, -20.0, -12.0]), Some(-12.0));
        assert_eq!(median(&[-10.0, -20.0, -12.0, -14.0]), Some(-13.0));
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[f64::NAN, -9.0]), Some(-9.0));
    }

    #[test]
    fn clips_move_toward_the_median_and_the_loudest_and_quietest_are_capped() {
        let m = |l| (Some(l), None);
        let lv = LevelSettings { clip_level: ClipLevel::Median, clip_max_db: 4.0, ..s() };
        // median -14: -10 is lowered 4 (exactly the cap), -20 would need +6 but is capped at +4
        let (rows, reference) = clip_gains(&[m(-10.0), m(-14.0), m(-20.0)], &lv);
        assert_eq!(reference, Some(-14.0));
        assert_eq!(rows[0], (-4.0, None));
        assert_eq!(rows[1], (0.0, None));
        assert_eq!(rows[2].0, 4.0);
        assert!(rows[2].1.as_ref().unwrap().contains("+6.0"), "{:?}", rows[2]);
    }

    #[test]
    fn a_fixed_reference_ignores_the_other_clips() {
        let m = |l| (Some(l), None);
        let lv = LevelSettings { clip_level: ClipLevel::Fixed, clip_fixed_lufs: -16.0, ..s() };
        let (rows, reference) = clip_gains(&[m(-10.0), m(-20.0)], &lv);
        assert_eq!(reference, Some(-16.0));
        assert_eq!((rows[0].0, rows[1].0), (-6.0, 4.0));
    }

    #[test]
    fn a_clip_that_cannot_be_measured_is_left_alone_and_does_not_shift_the_median() {
        let lv = LevelSettings { clip_level: ClipLevel::Median, ..s() };
        let (rows, reference) = clip_gains(&[(Some(-12.0), None), (None, Some("the audio file is not on the volume".into())), (Some(-16.0), None)], &lv);
        assert_eq!(reference, Some(-14.0));
        assert_eq!(rows[1], (0.0, Some("the audio file is not on the volume".into())));
        assert_eq!((rows[0].0, rows[2].0), (-2.0, 2.0));
        let (rows, reference) = clip_gains(&[(None, None)], &lv);
        assert_eq!(reference, None);
        assert_eq!(rows[0].0, 0.0);
    }

    #[test]
    fn levelling_off_leaves_every_clip_at_zero() {
        let (rows, reference) = clip_gains(&[(Some(-10.0), None), (Some(-30.0), None)], &s());
        assert_eq!(reference, None);
        assert!(rows.iter().all(|r| *r == (0.0, None)));
    }

    #[test]
    fn settings_are_validated_and_read_from_partial_json() {
        let ok: LevelSettings = serde_json::from_str(r#"{"targetLufs":-14,"clipLevel":"median"}"#).unwrap();
        assert_eq!((ok.target_lufs, ok.clip_level, ok.max_boost_db), (Some(-14.0), ClipLevel::Median, 12.0));
        assert!(ok.active());
        let none: LevelSettings = serde_json::from_str(r#"{"targetLufs":null}"#).unwrap();
        assert_eq!(none.target_lufs, None);
        for bad in [r#"{"targetLufs":5}"#, r#"{"targetLufs":-100}"#, r#"{"maxBoostDb":-1}"#, r#"{"clipMaxDb":99}"#, r#"{"peakCeilingDb":3}"#] {
            let v: LevelSettings = serde_json::from_str(bad).unwrap();
            assert!(v.validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn readings_leave_out_what_is_not_a_number() {
        let r: Reading = measure(None, f64::NEG_INFINITY).into();
        assert_eq!(r, Reading::default());
        let r: Reading = Measure { lufs: Some(-14.123_456), true_peak_db: -1.006, sample_peak_db: -1.5 }.into();
        assert_eq!((r.lufs, r.true_peak_db), (Some(-14.12), Some(-1.01)));
        assert!(serde_json::to_string(&Reading::default()).unwrap().contains("null"));
    }

    #[test]
    fn gains_by_clip_id_become_engine_gains_by_track() {
        let doc = crate::commands::mixes::doc::MixDoc::from_mix(1, &{
            let mut m = Mix::new("g");
            for i in 0..3 {
                let pos = pm_formats::mix::Position { start: 0.0, length: 44100.0, session_event_id: format!("{:032}", i + 1), ..Default::default() };
                m.tracks.push(pm_formats::mix::Track::new(i + 1, "/x", 120.0, 1.0, pos));
            }
            m
        });
        let mut gains = PreviewGains { master_gain_db: 2.5, ..PreviewGains::default() };
        gains.clip_gain_db.insert(doc.clips[1].id.clone(), -3.0);
        gains.clip_gain_db.insert("not-a-clip".into(), 9.0);
        let mut o = Options::default();
        apply_gains(&doc, &gains, &mut o);
        assert_eq!(o.clip_gain_db, vec![0.0, -3.0, 0.0]);
        assert_eq!(o.master_gain_db, 2.5);
        gains.master_gain_db = f64::NAN;
        apply_gains(&doc, &gains, &mut o);
        assert_eq!(o.master_gain_db, 0.0);
    }
}
