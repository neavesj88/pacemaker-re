//! History: what the device played, read back from the library counters. Read-only apart from
//! `history_save_as_case`, which creates one case through the `pm-formats` `Library`.

use std::path::PathBuf;

use pm_formats::library::Library;
use pm_formats::ops_history::{self, PlayEntry, PlaySession, PlayedCollection};
use serde::Serialize;

use crate::error::AppResult;

/// How many rows the "most played" list carries.
const MOST_PLAYED_LIMIT: usize = 100;

pub fn setup(_app: &mut tauri::App) {}

/// Everything the History view shows, from one read of the library.
#[derive(Debug, Serialize)]
pub struct HistoryOverview {
    /// False when every counter on the device is still zero (nothing recorded yet).
    pub any_plays: bool,
    pub tracks_total: usize,
    pub tracks_played: usize,
    pub total_plays: i64,
    /// Reconstructed sessions, newest first, tracks in play order.
    pub sessions: Vec<PlaySession>,
    pub gap_minutes: i64,
    pub most_played: Vec<PlayEntry>,
    pub never_played: Vec<i64>,
    pub cases: Vec<PlayedCollection>,
    pub mixes: Vec<PlayedCollection>,
}

pub fn overview_of(stats: ops_history::PlayStats, gap_minutes: i64) -> HistoryOverview {
    let gap_minutes = gap_minutes.clamp(1, 24 * 60);
    let gap_ms = gap_minutes * 60_000;
    let tracks_played = stats.tracks.iter().filter(|t| t.was_played()).count();
    let any_plays = tracks_played > 0
        || stats.cases.iter().chain(&stats.mixes).any(|c| c.times_played > 0 || c.last_played > 0);
    HistoryOverview {
        any_plays,
        tracks_total: stats.tracks.len(),
        tracks_played,
        total_plays: stats.tracks.iter().map(|t| t.times_played.max(0)).sum(),
        sessions: ops_history::group_sessions(&stats.tracks, gap_ms),
        gap_minutes,
        most_played: ops_history::most_played(&stats.tracks, MOST_PLAYED_LIMIT),
        never_played: ops_history::never_played(&stats.tracks),
        cases: stats.cases,
        mixes: stats.mixes,
    }
}

/// Play counters, sessions and rankings. `gap_minutes` is the pause that ends a session
/// (default four hours).
#[tauri::command]
pub async fn history_overview(root: String, gap_minutes: Option<i64>) -> AppResult<HistoryOverview> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<HistoryOverview> {
        let lib = Library::open_read_only(&PathBuf::from(root))?;
        let gap = gap_minutes.unwrap_or(ops_history::DEFAULT_SESSION_GAP_MS / 60_000);
        Ok(overview_of(lib.play_stats()?, gap))
    })
    .await?
}

/// Creates a case named `name` holding `track_ids` in that order; returns its id. Called only
/// after the user confirmed the name in the UI.
#[tauri::command]
pub async fn history_save_as_case(root: String, name: String, track_ids: Vec<i64>) -> AppResult<i64> {
    tauri::async_runtime::spawn_blocking(move || -> AppResult<i64> {
        if track_ids.is_empty() {
            return Err(pm_formats::library::Error::Invalid("no tracks to put in the case".into()).into());
        }
        let mut lib = Library::open(&PathBuf::from(root))?;
        Ok(lib.create_case_with_tracks(name.trim(), &track_ids)?)
    })
    .await?
}

/// This area's command paths, one per line, written `$crate::commands::history::name,`.
#[macro_export]
macro_rules! history_commands {
    ([$($acc:tt)*]; $next:ident $(, $rest:ident)*) => {
        $crate::$next! {
            [
                $($acc)*
                $crate::commands::history::history_overview,
                $crate::commands::history::history_save_as_case,
            ];
            $($rest),*
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use ops_history::PlayStats;

    fn e(track_id: i64, n: i64, t: i64) -> PlayEntry {
        PlayEntry { track_id, times_played: n, last_played: t }
    }
    fn stats(tracks: Vec<PlayEntry>) -> PlayStats {
        PlayStats { tracks, cases: vec![], mixes: vec![] }
    }

    #[test]
    fn all_zero_device_reports_no_plays() {
        let o = overview_of(stats(vec![e(1, 0, -1), e(2, 0, -1)]), 240);
        assert!(!o.any_plays);
        assert_eq!((o.tracks_total, o.tracks_played, o.total_plays), (2, 0, 0));
        assert!(o.sessions.is_empty() && o.most_played.is_empty());
        assert_eq!(o.never_played, vec![1, 2]);
    }

    #[test]
    fn counts_and_gap_clamp() {
        let t = 1_700_000_000_000;
        let o = overview_of(stats(vec![e(1, 3, t), e(2, 1, t + 60_000), e(3, 0, -1)]), 0);
        assert!(o.any_plays);
        assert_eq!((o.tracks_played, o.total_plays, o.gap_minutes), (2, 4, 1));
        assert_eq!(o.sessions.len(), 1, "one minute apart is inside the clamped one minute gap");
        assert_eq!(o.most_played[0].track_id, 1);
        assert_eq!(o.never_played, vec![3]);
    }

    #[test]
    fn a_played_case_alone_counts_as_plays() {
        let mut s = stats(vec![e(1, 0, -1)]);
        s.cases.push(PlayedCollection { id: 1, name: "x".into(), times_played: 2, last_played: 0, track_count: 1 });
        assert!(overview_of(s, 240).any_plays);
    }
}
