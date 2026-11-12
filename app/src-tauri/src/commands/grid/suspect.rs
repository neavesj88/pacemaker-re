//! Suspect beat grids: tracks whose grid is probably at half or double the real tempo, or that
//! the stored grid itself says were analysed poorly.
//!
//! Two rules, each switchable, each with its numbers in [`SuspectSettings`] (the frontend keeps
//! them and passes them with every call, so nothing here is hard-coded policy):
//!
//! * **Tempo window.** A track's tempo (the median of its grid, which is also `tracks.bpm`) is
//!   compared with the window for its genre; a genre with no window of its own uses the general
//!   window. Outside it, the track is flagged, and when half or double the tempo falls inside the
//!   window that is suggested as the fix.
//! * **Grid confidence.** The analyser does not store a confidence, so one is derived from the
//!   `.str` itself: how many of the gaps between beat lines stay close to the usual gap (a grid
//!   that wanders is a variable-tempo track or a tracker that lost the beat) and how much of the
//!   track the grid covers (a grid that stops early). Below the floor, the track is flagged.
//!   This is a stored-grid estimate, not a score the analyser reported.
//!
//! [`assess`] is pure so the rules are tested without a library; [`scan`] applies it to every
//! track and is what `grid_suspects` and the "Suggested fix" batch operation both call.

use std::path::Path;

use pm_formats::library::{track_bpm, Library};
use pm_formats::ops_grid::{GridDoc, MAX_BPM, MIN_BPM};
use pm_formats::strfile::Analysis;
use serde::{Deserialize, Serialize};

use crate::commands::analysis::audio_path;
use crate::error::{AppResult, IntoApp};

/// A tempo window for one genre: a track whose genre text contains `name` (any case) is expected
/// to lie between `min` and `max` BPM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenreWindow {
    pub name: String,
    pub min: f64,
    pub max: f64,
}

/// The suspect rules and their numbers. Every field has a default (see [`Default`]) so an older
/// or partial settings object from the frontend still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuspectSettings {
    /// Flag tracks whose tempo is outside the window for their genre.
    pub tempo_rule: bool,
    /// The window for a track whose genre matches none of `genres`.
    pub fallback_min: f64,
    pub fallback_max: f64,
    pub genres: Vec<GenreWindow>,
    /// Flag tracks whose derived grid confidence is below `min_confidence_pct`.
    pub quality_rule: bool,
    pub min_confidence_pct: f64,
    /// A gap between two beat lines is "steady" when it is within this many per cent of the
    /// usual (median) gap.
    pub steady_tolerance_pct: f64,
    /// The grid should span at least this per cent of the track; less lowers the confidence in
    /// proportion.
    pub coverage_target_pct: f64,
}

/// Starting windows, in the order they are shown. They are common expectations for each genre,
/// not facts about the user's music: the settings dialog edits them.
fn default_genres() -> Vec<GenreWindow> {
    [("house", 115.0, 132.0), ("techno", 120.0, 150.0), ("trance", 128.0, 145.0), ("drum & bass", 160.0, 180.0), ("dubstep", 135.0, 150.0), ("hip hop", 70.0, 105.0), ("disco", 100.0, 130.0)]
        .into_iter()
        .map(|(name, min, max)| GenreWindow { name: name.into(), min, max })
        .collect()
}

impl Default for SuspectSettings {
    fn default() -> Self {
        SuspectSettings {
            tempo_rule: true,
            fallback_min: 70.0,
            fallback_max: 180.0,
            genres: default_genres(),
            quality_rule: true,
            min_confidence_pct: 60.0,
            steady_tolerance_pct: 4.0,
            coverage_target_pct: 80.0,
        }
    }
}

const MAX_GENRES: usize = 200;

impl SuspectSettings {
    /// Refuses numbers that would make the rules meaningless, with the reason in words.
    pub fn validate(&self) -> Result<(), String> {
        let window = |what: &str, lo: f64, hi: f64| {
            if !lo.is_finite() || !hi.is_finite() || lo < MIN_BPM || hi > MAX_BPM || lo >= hi {
                Err(format!("The {what} tempo window must run from {MIN_BPM} up to {MAX_BPM} BPM, with the first number below the second."))
            } else {
                Ok(())
            }
        };
        window("general", self.fallback_min, self.fallback_max)?;
        if self.genres.len() > MAX_GENRES {
            return Err(format!("At most {MAX_GENRES} genre windows."));
        }
        for g in &self.genres {
            if g.name.trim().is_empty() {
                return Err("A genre window needs a name.".into());
            }
            window(&format!("“{}”", g.name.trim()), g.min, g.max)?;
        }
        let pct = |what: &str, v: f64, lo: f64, hi: f64| {
            if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(format!("{what} must be between {lo} and {hi} per cent.")) }
        };
        pct("The confidence floor", self.min_confidence_pct, 0.0, 100.0)?;
        pct("The steady-gap tolerance", self.steady_tolerance_pct, 0.5, 50.0)?;
        pct("The coverage target", self.coverage_target_pct, 10.0, 100.0)?;
        Ok(())
    }

    /// The window for a genre: the longest window name found in the genre text wins (so
    /// "drum & bass" beats a shorter name it contains); no match means the general window.
    /// Returns `(min, max, name of the matched genre window)`.
    pub fn window_for<'a>(&'a self, genre: &str) -> (f64, f64, Option<&'a str>) {
        let g = genre.to_lowercase();
        let best = self
            .genres
            .iter()
            .filter(|w| !w.name.trim().is_empty() && g.contains(&w.name.trim().to_lowercase()))
            .max_by_key(|w| w.name.trim().len());
        match best {
            Some(w) => (w.min, w.max, Some(w.name.trim())),
            None => (self.fallback_min, self.fallback_max, None),
        }
    }
}

/// How trustworthy a stored grid looks, derived from the `.str` alone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GridQuality {
    /// Share of the gaps between beat lines that are within the tolerance of the usual gap, 0 to 1.
    pub steady: f64,
    /// Span of the grid over the length of the track, 0 to 1 (1 when the length is unknown).
    pub coverage: f64,
    /// `steady` times the coverage factor, in per cent.
    pub confidence_pct: f64,
}

/// Derives the quality of `doc`. `duration_secs` is the track length (0 when unknown).
pub fn grid_quality(doc: &GridDoc, sample_rate: u32, duration_secs: f64, s: &SuspectSettings) -> GridQuality {
    let gaps: Vec<i64> = doc.beats.windows(2).map(|w| w[1].pos - w[0].pos).collect();
    if gaps.is_empty() || sample_rate == 0 {
        return GridQuality { steady: 0.0, coverage: 0.0, confidence_pct: 0.0 };
    }
    let mut sorted = gaps.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2] as f64;
    let tol = s.steady_tolerance_pct / 100.0;
    let steady = gaps.iter().filter(|&&g| (g as f64 - median).abs() <= median * tol).count() as f64 / gaps.len() as f64;
    let coverage = if duration_secs > 0.0 {
        let span = (doc.beats[doc.beats.len() - 1].pos - doc.beats[0].pos) as f64 / f64::from(sample_rate);
        (span / duration_secs).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let target = s.coverage_target_pct / 100.0;
    let factor = (coverage / target).min(1.0);
    GridQuality { steady, coverage, confidence_pct: (100.0 * steady * factor * 10.0).round() / 10.0 }
}

/// The tempo fix a flagged track would get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    Halve,
    Double,
}

/// One flagged track.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suspect {
    pub track_id: i64,
    /// The grid's tempo (median), the number the rules looked at.
    pub bpm: f64,
    /// Every reason, in words.
    pub reasons: Vec<String>,
    /// Outside the genre window.
    pub tempo_flag: bool,
    /// Below the confidence floor.
    pub quality_flag: bool,
    /// Half or double fits the window; `None` when the tempo is fine or neither fits.
    pub fix: Option<Fix>,
    /// The window the tempo was compared with.
    pub window: [f64; 2],
    pub confidence_pct: Option<f64>,
}

fn range_text(lo: f64, hi: f64) -> String {
    format!("{}–{}", trim_num(lo), trim_num(hi))
}

fn trim_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

/// Applies the rules to one track; `None` when it is not suspect. `quality` is `None` when the
/// grid could not be read (then only the tempo rule can run, on `bpm`).
pub fn assess(track_id: i64, genre: &str, bpm: f64, quality: Option<GridQuality>, s: &SuspectSettings) -> Option<Suspect> {
    let (lo, hi, named) = s.window_for(genre);
    let mut reasons = Vec::new();
    let mut fix = None;
    let mut tempo_flag = false;
    if s.tempo_rule && bpm > 0.0 && (bpm < lo || bpm > hi) {
        tempo_flag = true;
        let which = match named {
            Some(n) => format!("the {} BPM window for “{n}”", range_text(lo, hi)),
            None => format!("the general {} BPM window", range_text(lo, hi)),
        };
        let side = if bpm > hi { "above" } else { "below" };
        let mut why = format!("{bpm:.2} BPM is {side} {which}");
        let fits = |v: f64| (lo..=hi).contains(&v) && (MIN_BPM..=MAX_BPM).contains(&v);
        if bpm > hi && fits(bpm / 2.0) {
            fix = Some(Fix::Halve);
            why.push_str(&format!("; half of it, {:.2} BPM, fits", bpm / 2.0));
        } else if bpm < lo && fits(bpm * 2.0) {
            fix = Some(Fix::Double);
            why.push_str(&format!("; double it, {:.2} BPM, fits", bpm * 2.0));
        } else {
            why.push_str("; neither half nor double fits");
        }
        reasons.push(why);
    }
    let mut quality_flag = false;
    let mut confidence_pct = None;
    if let Some(q) = quality {
        confidence_pct = Some(q.confidence_pct);
        if s.quality_rule && q.confidence_pct < s.min_confidence_pct {
            quality_flag = true;
            let mut parts = Vec::new();
            if q.steady < 0.9 {
                parts.push(format!("only {:.0}% of the gaps between beat lines are steady (within {}%)", q.steady * 100.0, trim_num(s.steady_tolerance_pct)));
            }
            if q.coverage < s.coverage_target_pct / 100.0 {
                parts.push(format!("the grid covers {:.0}% of the track", q.coverage * 100.0));
            }
            let detail = if parts.is_empty() { String::new() } else { format!(": {}", parts.join(", ")) };
            reasons.push(format!("grid confidence {:.0}% is below {}%{detail}", q.confidence_pct, trim_num(s.min_confidence_pct)));
        }
    }
    (tempo_flag || quality_flag).then_some(Suspect { track_id, bpm, reasons, tempo_flag, quality_flag, fix, window: [lo, hi], confidence_pct })
}

/// What a scan found.
#[derive(Debug, Clone, Serialize)]
pub struct SuspectReport {
    pub suspects: Vec<Suspect>,
    /// Tracks with a readable grid that the rules were applied to.
    pub checked: usize,
    /// Tracks left out because their grid is missing or unreadable (the Missing filter lists them).
    pub unreadable: usize,
}

/// Applies the rules to every track of the library. Reads the `.str` of each track through the
/// validated location check, so a stored location with `..` in it is left out, not followed.
pub fn scan(lib: &Library, s: &SuspectSettings) -> AppResult<SuspectReport> {
    s.validate()?;
    let mut report = SuspectReport { suspects: Vec::new(), checked: 0, unreadable: 0 };
    for t in lib.tracks()? {
        if audio_path(lib.root(), &t.location).is_err() {
            report.unreadable += 1;
            continue;
        }
        let load = match lib.load_grid(t.track_id) {
            Ok(l) => l,
            Err(_) => {
                report.unreadable += 1;
                continue;
            }
        };
        let Some(doc) = load.doc else {
            report.unreadable += 1;
            continue;
        };
        report.checked += 1;
        let bpm = track_bpm(&Analysis::from(&doc));
        let quality = grid_quality(&doc, load.sample_rate, load.duration_secs, s);
        if let Some(sus) = assess(t.track_id, &t.genre, bpm, Some(quality), s) {
            report.suspects.push(sus);
        }
    }
    Ok(report)
}

/// Flags the tracks of the library, without writing anything. Needs the rules because they are
/// the user's settings, kept by the frontend.
#[tauri::command]
pub async fn grid_suspects(root: String, settings: SuspectSettings) -> AppResult<SuspectReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let lib = Library::open_read_only(Path::new(&root))?;
        scan(&lib, &settings)
    })
    .await
    .app()?
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::ops_grid::GridBeat;

    const SR: u32 = 44_100;

    fn grid(bpm: f64, secs: f64, jitter: impl Fn(usize) -> i64) -> GridDoc {
        let period = f64::from(SR) * 60.0 / bpm;
        let n = (secs * f64::from(SR) / period) as usize;
        GridDoc {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..n).map(|i| GridBeat { pos: (i as f64 * period).round() as i64 + jitter(i), bpm }).collect(),
            first_downbeat: Some(0),
        }
    }

    fn defaults() -> SuspectSettings {
        SuspectSettings::default()
    }

    #[test]
    fn the_longest_matching_genre_name_picks_the_window() {
        let mut s = defaults();
        s.genres.push(GenreWindow { name: "deep house".into(), min: 118.0, max: 124.0 });
        assert_eq!(s.window_for("Deep House"), (118.0, 124.0, Some("deep house")));
        assert_eq!(s.window_for("Progressive HOUSE"), (115.0, 132.0, Some("house")));
        assert_eq!(s.window_for("Drum & Bass / Jungle").2, Some("drum & bass"));
        assert_eq!(s.window_for(""), (70.0, 180.0, None));
        assert_eq!(s.window_for("Polka"), (70.0, 180.0, None));
    }

    #[test]
    fn a_tempo_outside_the_window_is_flagged_with_the_fix_that_fits() {
        let s = defaults();
        let q = Some(grid_quality(&grid(172.0, 120.0, |_| 0), SR, 120.0, &s));
        // house at 172: half is 86, which is below the house window, so neither fits
        let a = assess(1, "House", 172.0, q, &s).unwrap();
        assert!(a.tempo_flag && !a.quality_flag);
        assert_eq!(a.fix, None);
        assert!(a.reasons[0].contains("above the 115–132 BPM window for “house”") && a.reasons[0].contains("neither half nor double fits"), "{:?}", a.reasons);
        // techno at 250 halves to 125, which fits
        let a = assess(2, "Techno", 250.0, q, &s).unwrap();
        assert_eq!(a.fix, Some(Fix::Halve));
        assert!(a.reasons[0].contains("half of it, 125.00 BPM, fits"), "{:?}", a.reasons);
        // drum & bass read at 87 doubles to 174
        let a = assess(3, "Drum & Bass", 87.0, q, &s).unwrap();
        assert_eq!(a.fix, Some(Fix::Double));
        assert_eq!(a.window, [160.0, 180.0]);
        // inside the window, and the edges are inside
        assert!(assess(4, "House", 128.0, q, &s).is_none());
        assert!(assess(5, "House", 115.0, q, &s).is_none());
        assert!(assess(6, "House", 132.0, q, &s).is_none());
        assert!(assess(7, "House", 132.01, q, &s).is_some());
        // no genre window: the general one, named as such
        let a = assess(8, "Polka", 60.0, q, &s).unwrap();
        assert!(a.reasons[0].contains("below the general 70–180 BPM window") && a.fix == Some(Fix::Double), "{:?}", a.reasons);
    }

    #[test]
    fn switching_a_rule_off_or_moving_a_window_changes_the_outcome() {
        let mut s = defaults();
        let q = None;
        assert!(assess(1, "House", 172.0, q, &s).is_some());
        s.tempo_rule = false;
        assert!(assess(1, "House", 172.0, q, &s).is_none(), "rule off");
        s.tempo_rule = true;
        s.genres[0].max = 180.0;
        assert!(assess(1, "House", 172.0, q, &s).is_none(), "wider window");
        // an unknown tempo (0) is never flagged by the tempo rule
        assert!(assess(1, "House", 0.0, q, &defaults()).is_none());
    }

    #[test]
    fn a_steady_grid_over_the_whole_track_has_full_confidence() {
        let s = defaults();
        let q = grid_quality(&grid(126.0, 180.0, |_| 0), SR, 180.0, &s);
        assert_eq!(q.steady, 1.0);
        assert!(q.coverage > 0.99);
        assert_eq!(q.confidence_pct, 100.0);
        assert!(assess(1, "House", 126.0, Some(q), &s).is_none());
    }

    #[test]
    fn a_wandering_grid_loses_confidence_and_is_flagged_with_the_reason() {
        let s = defaults();
        // every other gap is 15% off: half the gaps are steady
        let doc = grid(126.0, 180.0, |i| if i % 2 == 0 { 0 } else { 3000 });
        let q = grid_quality(&doc, SR, 180.0, &s);
        assert!(q.steady < 0.6, "{q:?}");
        let a = assess(1, "House", 126.0, Some(q), &s).unwrap();
        assert!(a.quality_flag && !a.tempo_flag);
        assert!(a.reasons[0].contains("grid confidence") && a.reasons[0].contains("gaps between beat lines are steady"), "{:?}", a.reasons);
        // a looser floor accepts the same grid
        let mut loose = s.clone();
        loose.min_confidence_pct = 10.0;
        assert!(assess(1, "House", 126.0, Some(q), &loose).is_none());
        let mut off = s;
        off.quality_rule = false;
        assert!(assess(1, "House", 126.0, Some(q), &off).is_none());
    }

    #[test]
    fn a_grid_that_stops_early_is_penalised_in_proportion() {
        let s = defaults();
        // beats only over the first 60 s of a 240 s track: coverage 25%, target 80%
        let q = grid_quality(&grid(126.0, 60.0, |_| 0), SR, 240.0, &s);
        assert!((q.coverage - 0.25).abs() < 0.01, "{q:?}");
        assert!((q.confidence_pct - 31.25).abs() < 1.0, "{q:?}");
        let a = assess(1, "House", 126.0, Some(q), &s).unwrap();
        assert!(a.reasons[0].contains("the grid covers 25% of the track"), "{:?}", a.reasons);
        // an unknown length cannot fault the coverage
        let q = grid_quality(&grid(126.0, 60.0, |_| 0), SR, 0.0, &s);
        assert_eq!(q.coverage, 1.0);
    }

    #[test]
    fn degenerate_grids_have_no_confidence() {
        let s = defaults();
        let one = GridDoc { normalize_factor: 1.0, beat_start_offset: 0, beats: vec![GridBeat { pos: 100, bpm: 120.0 }], first_downbeat: None };
        assert_eq!(grid_quality(&one, SR, 100.0, &s).confidence_pct, 0.0);
    }

    #[test]
    fn the_frontends_defaults_file_is_the_default_settings() {
        // the settings dialog starts from state/suspectDefaults.json; this keeps it from drifting
        let file: SuspectSettings = serde_json::from_str(include_str!("../../../../src/lib/state/suspectDefaults.json")).unwrap();
        assert_eq!(file, SuspectSettings::default());
    }

    #[test]
    fn settings_are_checked_and_partial_objects_take_the_defaults() {
        assert!(defaults().validate().is_ok());
        let parsed: SuspectSettings = serde_json::from_str(r#"{"tempo_rule":false}"#).unwrap();
        assert!(!parsed.tempo_rule);
        assert_eq!(parsed.fallback_min, 70.0);
        assert_eq!(parsed.genres.len(), defaults().genres.len());
        for bad in [
            SuspectSettings { fallback_min: 100.0, fallback_max: 100.0, ..defaults() },
            SuspectSettings { fallback_min: 10.0, ..defaults() },
            SuspectSettings { fallback_max: 500.0, ..defaults() },
            SuspectSettings { fallback_min: f64::NAN, ..defaults() },
            SuspectSettings { min_confidence_pct: 101.0, ..defaults() },
            SuspectSettings { steady_tolerance_pct: 0.0, ..defaults() },
            SuspectSettings { coverage_target_pct: 5.0, ..defaults() },
            SuspectSettings { genres: vec![GenreWindow { name: " ".into(), min: 100.0, max: 120.0 }], ..defaults() },
            SuspectSettings { genres: vec![GenreWindow { name: "x".into(), min: 130.0, max: 120.0 }], ..defaults() },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
    }
}
