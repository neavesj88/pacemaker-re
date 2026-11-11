//! Tauri commands of the play log (the engine and the file format are in `playlog.rs`).
//!
//! Everything here reads the device library read-only and writes only the app config dir: none
//! of it is a device write, so it takes no `WriteTicket` and is never refused by Eject / backup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pm_formats::library::Library;
use serde::Serialize;
use tauri::Manager;

use super::playlog::{self, Hit, Outcome, PlayLog, Reading, Settings};
use crate::error::AppResult;

pub fn setup(app: &mut tauri::App) {
    app.manage(PlayLogGuard::default());
}

/// Serialises the read-modify-write of the log files (two library loads in quick succession
/// must not both rewrite the same file from the same starting point).
#[derive(Default)]
struct PlayLogGuard(Mutex<()>);

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn log_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir()?)
}

fn readings_of(root: &str) -> AppResult<Vec<Reading>> {
    let lib = Library::open_read_only(&PathBuf::from(root))?;
    Ok(playlog::identify(&lib.play_counters()?))
}

/// What a snapshot did, for the status line.
#[derive(Debug, Serialize, PartialEq)]
pub struct RecordReport {
    pub enabled: bool,
    /// "off", "started", "unchanged", "changed", "mismatch" or "recovered" (the old file was
    /// unreadable and was moved aside; a new log started).
    pub status: String,
    /// Tracks appended ("changed") or put in the baseline ("started").
    pub tracks: usize,
    /// Changes folded into the baseline by the retention.
    pub pruned: usize,
    /// Why the old file could not be used ("recovered") or what did not match ("mismatch").
    pub detail: Option<String>,
}

fn report(enabled: bool, status: &str, tracks: usize, pruned: usize, detail: Option<String>) -> RecordReport {
    RecordReport { enabled, status: status.into(), tracks, pruned, detail }
}

/// The blocking body of [`playlog_record`]; `now` is a parameter for the tests.
fn record_at(dir: &Path, root: &str, readings: &[Reading], now: i64) -> Result<RecordReport, String> {
    let settings = playlog::load_settings(dir);
    if !settings.enabled {
        return Ok(report(false, "off", 0, 0, None));
    }
    let (mut log, mut recovered) = match playlog::load(dir, root) {
        Ok(found) => (found.unwrap_or_default(), None),
        Err(why) => {
            // Never delete a log we cannot read: move it aside and say so.
            let kept = playlog::quarantine(dir, root, now)?;
            (PlayLog::default(), Some(format!("{why}. The old file was kept as {}.", kept.display())))
        }
    };
    let (status, tracks) = match log.record(now, readings) {
        Outcome::Mismatch { logged, present } => {
            let why = format!("only {present} of the {logged} tracks this log has seen played are in this library");
            return Ok(report(true, "mismatch", 0, 0, Some(why)));
        }
        Outcome::Started { tracks } => (if recovered.is_some() { "recovered" } else { "started" }, tracks),
        Outcome::Changed { events } => ("changed", events),
        Outcome::Unchanged => ("unchanged", 0),
    };
    let pruned = log.prune(now, settings.retention_days);
    if status != "unchanged" || pruned > 0 {
        playlog::save(dir, root, &log)?;
    }
    Ok(report(true, status, tracks, pruned, recovered.take()))
}

/// Saved settings of the play log (on by default, 730 days kept, four hour session gap).
#[tauri::command]
pub async fn playlog_settings_get(app: tauri::AppHandle) -> AppResult<Settings> {
    let dir = log_dir(&app)?;
    Ok(tauri::async_runtime::spawn_blocking(move || playlog::load_settings(&dir)).await?)
}

/// Saves the settings (clamped to sensible bounds) and returns them as stored.
#[tauri::command]
pub async fn playlog_settings_set(app: tauri::AppHandle, settings: Settings) -> AppResult<Settings> {
    let dir = log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Settings> {
        let s = settings.sanitised();
        playlog::save_settings(&dir, &s)?;
        Ok(s)
    })
    .await?
}

/// Takes a snapshot of the library's play counters into the log (nothing when logging is off).
/// Called each time a library is read; writes nothing unless a counter changed.
#[tauri::command]
pub async fn playlog_record(app: tauri::AppHandle, root: String) -> AppResult<RecordReport> {
    let dir = log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<RecordReport> {
        // Off is checked before the volume is touched, so a disabled log never reads the card.
        if !playlog::load_settings(&dir).enabled {
            return Ok(report(false, "off", 0, 0, None));
        }
        let readings = readings_of(&root)?;
        let guard = app.state::<PlayLogGuard>();
        let _held = guard.0.lock().unwrap_or_else(|p| p.into_inner());
        Ok(record_at(&dir, &root, &readings, now_ms())?)
    })
    .await?
}

/// One derived play, resolved to a track in the library.
#[derive(Debug, Serialize, PartialEq)]
pub struct LogHit {
    pub track_id: i64,
    /// Unix ms (device clock) of the most recent of these plays.
    pub at: i64,
    pub plays: i64,
    /// Several plays share this one time, or it was inferred (see `time_known`).
    pub approx: bool,
    pub time_known: bool,
    pub reset: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LogSession {
    pub start: i64,
    pub end: i64,
    pub plays: i64,
    /// In play order (oldest first).
    pub hits: Vec<LogHit>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LogRank {
    pub track_id: i64,
    pub plays: i64,
    /// How many separate observations the plays came from.
    pub windows: usize,
    pub first: i64,
    pub last: i64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PlayLogOverview {
    pub enabled: bool,
    pub has_log: bool,
    /// The saved file could not be read; it is left alone until the next snapshot moves it aside.
    pub problem: Option<String>,
    /// This library is not the one the log was made from (see `Outcome::Mismatch`).
    pub mismatch: bool,
    pub started_at: i64,
    pub last_change_at: i64,
    /// Changes in the file (each is one track at one snapshot).
    pub events: usize,
    pub file_bytes: u64,
    pub retention_days: i64,
    pub gap_minutes: i64,
    /// Time of the oldest and newest derived play in the whole log, 0 when none.
    pub first_play_at: i64,
    pub last_play_at: i64,
    /// Plays in the asked period for tracks in the library, and plays of tracks that are gone.
    pub total_plays: i64,
    pub removed_plays: i64,
    pub sessions_total: usize,
    /// Newest first, at most `session_limit`.
    pub sessions: Vec<LogSession>,
    pub ranking_total: usize,
    pub ranking: Vec<LogRank>,
}

fn no_log(settings: &Settings, problem: Option<String>, file_bytes: u64) -> PlayLogOverview {
    PlayLogOverview {
        enabled: settings.enabled,
        has_log: false,
        problem,
        mismatch: false,
        started_at: 0,
        last_change_at: 0,
        events: 0,
        file_bytes,
        retention_days: settings.retention_days,
        gap_minutes: settings.session_gap_minutes,
        first_play_at: 0,
        last_play_at: 0,
        total_plays: 0,
        removed_plays: 0,
        sessions_total: 0,
        sessions: vec![],
        ranking_total: 0,
        ranking: vec![],
    }
}

struct Query {
    from: Option<i64>,
    to: Option<i64>,
    gap_minutes: i64,
    session_limit: usize,
    rank_limit: usize,
}

fn to_log_hit(h: &Hit, track_id: i64) -> LogHit {
    LogHit { track_id, at: h.at, plays: h.plays, approx: h.approx, time_known: h.time_known, reset: h.reset }
}

/// Sessions and the ranking for a period, from the log and the library's current tracks. Pure.
fn overview_of(log: &PlayLog, readings: &[Reading], settings: &Settings, file_bytes: u64, q: &Query) -> PlayLogOverview {
    let track_of: HashMap<&str, i64> = readings.iter().map(|r| (r.key.as_str(), r.track_id)).collect();
    let all = log.hits();
    let in_range: Vec<&Hit> = all.iter().filter(|h| q.from.is_none_or(|f| h.at >= f) && q.to.is_none_or(|t| h.at <= t)).collect();
    let removed_plays: i64 = in_range.iter().filter(|h| !track_of.contains_key(h.key.as_str())).map(|h| h.plays).sum();
    // Plays of tracks that are no longer in the library cannot be shown or put in a case.
    let known: Vec<Hit> = in_range.into_iter().filter(|h| track_of.contains_key(h.key.as_str())).cloned().collect();
    let gap_minutes = q.gap_minutes.clamp(1, 24 * 60);

    let ranges = playlog::session_ranges(&known, gap_minutes * 60_000);
    let sessions: Vec<LogSession> = ranges
        .iter()
        .rev()
        .take(q.session_limit)
        .map(|r| {
            let hits: Vec<LogHit> = known[r.clone()].iter().map(|h| to_log_hit(h, track_of[h.key.as_str()])).collect();
            LogSession { start: known[r.start].at, end: known[r.end - 1].at, plays: hits.iter().map(|h| h.plays).sum(), hits }
        })
        .collect();

    let ranks = playlog::ranking(&known);
    PlayLogOverview {
        enabled: settings.enabled,
        has_log: true,
        problem: None,
        mismatch: log.identity_problem(readings).is_some(),
        started_at: log.started_at,
        last_change_at: log.last_change_at,
        events: log.event_count(),
        file_bytes,
        retention_days: settings.retention_days,
        gap_minutes,
        first_play_at: all.first().map(|h| h.at).unwrap_or(0),
        last_play_at: all.last().map(|h| h.at).unwrap_or(0),
        total_plays: known.iter().map(|h| h.plays).sum(),
        removed_plays,
        sessions_total: ranges.len(),
        sessions,
        ranking_total: ranks.len(),
        ranking: ranks
            .into_iter()
            .take(q.rank_limit)
            .map(|r| LogRank { track_id: track_of[r.key.as_str()], plays: r.plays, windows: r.windows, first: r.first, last: r.last })
            .collect(),
    }
}

/// Sessions (newest first) and the most played tracks for a period of the play log.
/// `from` / `to` are Unix ms bounds (either may be absent); `gap_minutes` defaults to the setting.
#[tauri::command]
pub async fn playlog_overview(
    app: tauri::AppHandle,
    root: String,
    from: Option<i64>,
    to: Option<i64>,
    gap_minutes: Option<i64>,
    session_limit: Option<usize>,
    rank_limit: Option<usize>,
) -> AppResult<PlayLogOverview> {
    let dir = log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<PlayLogOverview> {
        let settings = playlog::load_settings(&dir);
        let bytes = playlog::file_size(&dir, &root);
        let guard = app.state::<PlayLogGuard>();
        let log = {
            let _held = guard.0.lock().unwrap_or_else(|p| p.into_inner());
            playlog::load(&dir, &root)
        };
        let log = match log {
            Ok(Some(l)) if l.is_started() => l,
            Ok(_) => return Ok(no_log(&settings, None, bytes)),
            Err(why) => return Ok(no_log(&settings, Some(why), bytes)),
        };
        let readings = readings_of(&root)?;
        let q = Query {
            from,
            to,
            gap_minutes: gap_minutes.unwrap_or(settings.session_gap_minutes),
            session_limit: session_limit.unwrap_or(60),
            rank_limit: rank_limit.unwrap_or(100),
        };
        Ok(overview_of(&log, &readings, &settings, bytes, &q))
    })
    .await?
}

/// One track's history in the log.
#[derive(Debug, Serialize, PartialEq)]
pub struct TrackLog {
    pub track_id: i64,
    /// Plays and last play the log already had when it began (or before the retention), as the
    /// device counted them; when those plays happened is not known.
    pub before_plays: i64,
    pub before_last: i64,
    /// Plays derived from the log, and the device's counters now.
    pub logged_plays: i64,
    pub now_plays: i64,
    pub now_last: i64,
    /// Newest first.
    pub hits: Vec<LogHit>,
}

fn track_log_of(log: &PlayLog, readings: &[Reading], track_id: i64) -> Option<TrackLog> {
    let me = readings.iter().find(|r| r.track_id == track_id)?;
    let base = log.baseline_of(&me.key).unwrap_or(playlog::Counter::ZERO);
    let mut hits: Vec<LogHit> = log.hits().iter().filter(|h| h.key == me.key).map(|h| to_log_hit(h, track_id)).collect();
    hits.reverse();
    Some(TrackLog {
        track_id,
        before_plays: base.plays,
        before_last: base.last,
        logged_plays: hits.iter().map(|h| h.plays).sum(),
        now_plays: me.counter.plays,
        now_last: me.counter.last,
        hits,
    })
}

/// The log's plays of one track, newest first, with the counters from before the log began.
/// `None` when there is no log or the track is not in the library.
#[tauri::command]
pub async fn playlog_track(app: tauri::AppHandle, root: String, track_id: i64) -> AppResult<Option<TrackLog>> {
    let dir = log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<Option<TrackLog>> {
        let guard = app.state::<PlayLogGuard>();
        let log = {
            let _held = guard.0.lock().unwrap_or_else(|p| p.into_inner());
            playlog::load(&dir, &root)?
        };
        let Some(log) = log.filter(PlayLog::is_started) else {
            return Ok(None);
        };
        Ok(track_log_of(&log, &readings_of(&root)?, track_id))
    })
    .await?
}

/// Deletes this library's play log. The device and its play counters are not touched. Called
/// only after the user confirmed in the UI. Returns whether a file was removed.
#[tauri::command]
pub async fn playlog_clear(app: tauri::AppHandle, root: String) -> AppResult<bool> {
    let dir = log_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> AppResult<bool> {
        let guard = app.state::<PlayLogGuard>();
        let _held = guard.0.lock().unwrap_or_else(|p| p.into_inner());
        Ok(playlog::remove(&dir, &root)?)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use playlog::Counter;

    const H: i64 = 3_600_000;
    const T0: i64 = 1_700_000_000_000;

    fn r(id: i64, key: &str, plays: i64, last: i64) -> Reading {
        Reading { track_id: id, key: key.into(), counter: Counter::new(plays, last) }
    }
    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-logcmds-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn q(from: Option<i64>, to: Option<i64>) -> Query {
        Query { from, to, gap_minutes: 240, session_limit: 60, rank_limit: 100 }
    }

    #[test]
    fn record_walks_through_start_unchanged_changed_and_respects_the_off_switch() {
        let dir = temp_dir("record");
        let a = [r(1, "a", 2, T0 - H), r(2, "b", 0, -1)];
        assert_eq!(record_at(&dir, "E:", &a, T0).unwrap().status, "started");
        let file = playlog::store_path(&dir, "E:");
        let first = std::fs::read(&file).unwrap();
        assert_eq!(record_at(&dir, "E:", &a, T0 + H).unwrap().status, "unchanged");
        assert_eq!(std::fs::read(&file).unwrap(), first, "an unchanged snapshot does not rewrite the file");

        let b = [r(1, "a", 3, T0 + 2 * H), r(2, "b", 0, -1)];
        let rep = record_at(&dir, "E:", &b, T0 + 3 * H).unwrap();
        assert_eq!((rep.status.as_str(), rep.tracks), ("changed", 1));

        playlog::save_settings(&dir, &Settings { enabled: false, ..Settings::default() }).unwrap();
        let c = [r(1, "a", 9, T0 + 5 * H)];
        let off = record_at(&dir, "E:", &c, T0 + 6 * H).unwrap();
        assert_eq!((off.enabled, off.status.as_str()), (false, "off"));
        let log = playlog::load(&dir, "E:").unwrap().unwrap();
        assert_eq!(log.hits().len(), 1, "nothing was recorded while off");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_prunes_by_retention_and_a_changed_retention_applies_to_an_unchanged_snapshot() {
        let dir = temp_dir("prune");
        let day = 86_400_000;
        record_at(&dir, "E:", &[r(1, "a", 1, T0)], T0).unwrap();
        record_at(&dir, "E:", &[r(1, "a", 2, T0 + day)], T0 + day).unwrap();
        record_at(&dir, "E:", &[r(1, "a", 2, T0 + day)], T0 + 400 * day).unwrap();
        assert_eq!(playlog::load(&dir, "E:").unwrap().unwrap().event_count(), 1, "730 days keeps it");
        playlog::save_settings(&dir, &Settings { retention_days: 30, ..Settings::default() }).unwrap();
        let rep = record_at(&dir, "E:", &[r(1, "a", 2, T0 + day)], T0 + 401 * day).unwrap();
        assert_eq!((rep.status.as_str(), rep.pruned), ("unchanged", 1));
        let log = playlog::load(&dir, "E:").unwrap().unwrap();
        assert_eq!((log.event_count(), log.baseline_of("a").unwrap().plays), (0, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_log_is_moved_aside_and_a_new_one_started() {
        let dir = temp_dir("recover");
        std::fs::write(playlog::store_path(&dir, "E:"), "oops").unwrap();
        let rep = record_at(&dir, "E:", &[r(1, "a", 1, T0)], T0).unwrap();
        assert_eq!(rep.status, "recovered");
        assert!(rep.detail.unwrap().contains("kept as"));
        let kept = std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains("corrupt")).count();
        assert_eq!(kept, 1);
        assert!(playlog::load(&dir, "E:").unwrap().unwrap().is_started());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_different_card_is_reported_and_leaves_the_log_untouched() {
        let dir = temp_dir("mismatch");
        let mine: Vec<Reading> = (1..=8).map(|i| r(i, &format!("m{i}"), 3, T0)).collect();
        record_at(&dir, "E:", &mine, T0).unwrap();
        let before = std::fs::read(playlog::store_path(&dir, "E:")).unwrap();
        let other: Vec<Reading> = (1..=8).map(|i| r(i, &format!("o{i}"), 5, T0 + H)).collect();
        let rep = record_at(&dir, "E:", &other, T0 + 2 * H).unwrap();
        assert_eq!(rep.status, "mismatch");
        assert!(rep.detail.unwrap().contains("only 0 of the 8"));
        assert_eq!(std::fs::read(playlog::store_path(&dir, "E:")).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sample_log() -> (PlayLog, Vec<Reading>) {
        let base = [r(1, "a", 1, T0 - H), r(2, "b", 0, -1), r(3, "c", 0, -1), r(4, "gone", 5, T0 - H)];
        let mut log = PlayLog::default();
        log.record(T0, &base);
        // night one: b twice (one window), c once; and the removed track
        log.record(T0 + 10 * H, &[r(1, "a", 1, T0 - H), r(2, "b", 2, T0 + 3 * H), r(3, "c", 1, T0 + 2 * H), r(4, "gone", 6, T0 + 4 * H)]);
        // a week later: a and b
        log.record(T0 + 200 * H, &[r(1, "a", 2, T0 + 190 * H), r(2, "b", 3, T0 + 191 * H), r(3, "c", 1, T0 + 2 * H), r(4, "gone", 6, T0 + 4 * H)]);
        let readings = vec![r(1, "a", 2, T0 + 190 * H), r(2, "b", 3, T0 + 191 * H), r(3, "c", 1, T0 + 2 * H)];
        (log, readings)
    }

    #[test]
    fn overview_builds_sessions_newest_first_and_a_ranking_for_the_period() {
        let (log, readings) = sample_log();
        let o = overview_of(&log, &readings, &Settings::default(), 123, &q(None, None));
        assert_eq!((o.sessions_total, o.total_plays, o.removed_plays), (2, 5, 1));
        assert_eq!(o.sessions[0].hits.iter().map(|h| h.track_id).collect::<Vec<_>>(), vec![1, 2], "newest session first, in play order");
        assert_eq!(o.sessions[1].hits.iter().map(|h| h.track_id).collect::<Vec<_>>(), vec![3, 2]);
        assert_eq!(o.sessions[1].plays, 3);
        assert!(o.sessions[1].hits[1].approx && o.sessions[1].hits[1].plays == 2, "b was played twice in one window");
        // b: 2 + 1 = 3 plays; a: 1; c: 1
        assert_eq!(o.ranking.iter().map(|x| (x.track_id, x.plays)).collect::<Vec<_>>(), vec![(2, 3), (1, 1), (3, 1)], "equal plays: the newest last play first, so a (190h) before c (2h)");
        assert_eq!((o.first_play_at, o.last_play_at), (T0 + 2 * H, T0 + 191 * H));
        assert_eq!(o.file_bytes, 123);
        assert!(!o.mismatch);
    }

    #[test]
    fn the_period_filters_sessions_and_ranking_the_same_way() {
        let (log, readings) = sample_log();
        let o = overview_of(&log, &readings, &Settings::default(), 0, &q(Some(T0 + 100 * H), None));
        assert_eq!((o.sessions_total, o.total_plays, o.removed_plays), (1, 2, 0));
        assert_eq!(o.ranking.iter().map(|x| x.track_id).collect::<Vec<_>>(), vec![2, 1], "tied at one play, b (191h) is newer than a (190h)");
        let early = overview_of(&log, &readings, &Settings::default(), 0, &q(None, Some(T0 + 100 * H)));
        assert_eq!((early.sessions_total, early.total_plays, early.removed_plays), (1, 3, 1));
        let none = overview_of(&log, &readings, &Settings::default(), 0, &q(Some(T0 + 500 * H), None));
        assert_eq!((none.sessions_total, none.total_plays, none.ranking_total), (0, 0, 0));
    }

    #[test]
    fn limits_cut_the_lists_but_not_the_totals() {
        let (log, readings) = sample_log();
        let mut query = q(None, None);
        query.session_limit = 1;
        query.rank_limit = 1;
        let o = overview_of(&log, &readings, &Settings::default(), 0, &query);
        assert_eq!((o.sessions.len(), o.sessions_total, o.ranking.len(), o.ranking_total), (1, 2, 1, 3));
    }

    #[test]
    fn the_gap_joins_sessions() {
        let (log, readings) = sample_log();
        let mut query = q(None, None);
        query.gap_minutes = 24 * 60;
        let o = overview_of(&log, &readings, &Settings::default(), 0, &query);
        assert_eq!(o.sessions_total, 2, "the two nights are 7 days apart");
        query.gap_minutes = 1;
        assert_eq!(overview_of(&log, &readings, &Settings::default(), 0, &query).sessions_total, 4);
    }

    #[test]
    fn track_history_has_the_baseline_and_the_logged_plays_newest_first() {
        let (log, readings) = sample_log();
        let t = track_log_of(&log, &readings, 1).unwrap();
        assert_eq!((t.before_plays, t.before_last, t.logged_plays, t.now_plays), (1, T0 - H, 1, 2));
        assert_eq!(t.hits.len(), 1);
        let b = track_log_of(&log, &readings, 2).unwrap();
        assert_eq!((b.before_plays, b.logged_plays), (0, 3));
        assert_eq!(b.hits.iter().map(|h| h.at).collect::<Vec<_>>(), vec![T0 + 191 * H, T0 + 3 * H]);
        assert!(track_log_of(&log, &readings, 99).is_none());
    }
}
