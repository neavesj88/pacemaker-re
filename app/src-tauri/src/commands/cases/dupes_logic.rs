//! The sound check of the duplicate finder: which tracks are worth fingerprinting and which pairs
//! sound alike. The rest of the rules (tags, size and length, grouping, the copy to keep) need no
//! audio and live in `pm_formats::ops_dupes`, shared with `pm duplicates`; they are re-exported
//! here so the finder keeps one import.

use std::collections::HashMap;

use pm_analysis::fingerprint::{self, Fingerprint};
use pm_formats::library::DupRow;

pub use pm_formats::ops_dupes::{display_order, group, pick_keeper, size_links, tag_links, DupSettings, Link, Mode};

// ---- sound ---------------------------------------------------------------------------------

/// The tracks worth fingerprinting: those with another track of nearly the same length, since
/// copies of one recording last the same. A track alone in its length needs no decoding.
pub fn audio_candidates(rows: &[&DupRow], s: &DupSettings) -> Vec<i64> {
    let mut by_len: Vec<(i64, i64)> = rows.iter().filter(|r| r.play_time_secs > 0).map(|r| (r.play_time_secs, r.track_id)).collect();
    by_len.sort_unstable();
    let tol = i64::from(s.audio_duration_secs);
    let mut keep = vec![false; by_len.len()];
    for i in 0..by_len.len() {
        if i + 1 < by_len.len() && by_len[i + 1].0 - by_len[i].0 <= tol {
            keep[i] = true;
            keep[i + 1] = true;
        }
    }
    let mut ids: Vec<i64> = by_len.iter().zip(&keep).filter(|(_, k)| **k).map(|((_, id), _)| *id).collect();
    ids.sort_unstable();
    ids
}

/// Pairs of tracks whose lengths agree and whose fingerprints are alike enough.
pub fn audio_links(rows: &[&DupRow], fps: &HashMap<i64, &Fingerprint>, s: &DupSettings) -> Vec<Link> {
    let mut by_len: Vec<&DupRow> = rows.iter().copied().filter(|r| r.play_time_secs > 0 && fps.get(&r.track_id).is_some_and(|f| !f.frames.is_empty())).collect();
    by_len.sort_by_key(|r| (r.play_time_secs, r.track_id));
    let tol = i64::from(s.audio_duration_secs);
    let max_offset = (s.audio_offset_secs as f32 / fingerprint::HOP_SECS).round() as usize;
    let mut links = Vec::new();
    for (i, a) in by_len.iter().enumerate() {
        for b in by_len[i + 1..].iter().take_while(|b| b.play_time_secs - a.play_time_secs <= tol) {
            let (fa, fb) = (fps[&a.track_id], fps[&b.track_id]);
            if let Some(m) = fingerprint::similarity(fa, fb, max_offset, s.audio_min_overlap)
                && m.score >= s.audio_threshold
            {
                links.push(Link { a: a.track_id, b: b.track_id, mode: Mode::Audio, score: Some(m.score), offset_secs: Some(m.offset_frames as f32 * fingerprint::HOP_SECS) });
            }
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use fingerprint::BANDS;

    fn row(id: i64, secs: i64) -> DupRow {
        DupRow {
            track_id: id,
            title: String::new(),
            artist: String::new(),
            album_artist: String::new(),
            album: String::new(),
            composer: String::new(),
            genre: String::new(),
            label: String::new(),
            key: String::new(),
            comments: String::new(),
            producer: String::new(),
            remixer: String::new(),
            year: 0,
            track_number: 0,
            disc_number: 0,
            format: "MP3".into(),
            bit_rate_kbps: 0,
            sample_rate: 0,
            file_size: 0,
            play_time_secs: secs,
            bpm: 0.0,
            rating: 0,
            times_played: 0,
            last_played: -1,
            date_added: 0,
            location: format!("/pmdb_tracks/{id}.mp3"),
            analysed: true,
        }
    }

    fn refs(v: &[DupRow]) -> Vec<&DupRow> {
        v.iter().collect()
    }

    fn pairs(links: &[Link]) -> Vec<(i64, i64)> {
        links.iter().map(|l| (l.a, l.b)).collect()
    }

    fn fp(seed: u64, n: usize) -> Fingerprint {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
        let frames = (0..n)
            .map(|_| {
                std::array::from_fn::<i8, BANDS, _>(|_| {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    ((s >> 33) % 120) as i8 - 60
                })
            })
            .collect();
        Fingerprint { frames }
    }

    #[test]
    fn audio_candidates_skip_tracks_alone_in_their_length() {
        let rows = vec![row(1, 100), row(2, 102), row(3, 250), row(4, 0), row(5, 0), row(6, 400), row(7, 404)];
        let s = DupSettings::default(); // 3 s
        assert_eq!(audio_candidates(&refs(&rows), &s), vec![1, 2], "400 and 404 are 4 s apart; unknown lengths never qualify");
        let wide = DupSettings { audio_duration_secs: 5, ..Default::default() };
        assert_eq!(audio_candidates(&refs(&rows), &wide), vec![1, 2, 6, 7]);
    }

    #[test]
    fn audio_links_connect_alike_sound_within_the_length_window() {
        let rows = vec![row(1, 100), row(2, 101), row(3, 101), row(4, 160), row(5, 101)];
        let (a, other, empty) = (fp(1, 200), fp(2, 200), Fingerprint { frames: vec![] });
        // 1 and 2 are the same sound; 3 is different; 4 is the same sound but 60 s longer; 5 has no data
        let shifted = a.clone();
        let fps: HashMap<i64, &Fingerprint> = [(1, &a), (2, &shifted), (3, &other), (4, &a), (5, &empty)].into();
        let links = audio_links(&refs(&rows), &fps, &DupSettings::default());
        assert_eq!(pairs(&links), vec![(1, 2)]);
        assert!(links[0].score.unwrap() > 0.99 && links[0].offset_secs == Some(0.0) && links[0].mode == Mode::Audio);
        // a threshold of 1.0 still accepts an exact copy; an impossible window finds nothing
        assert_eq!(audio_links(&refs(&rows), &fps, &DupSettings { audio_threshold: 1.0, ..Default::default() }).len(), 1);
        assert!(audio_links(&refs(&rows), &HashMap::new(), &DupSettings::default()).is_empty());
    }

    #[test]
    fn the_overlap_a_sound_match_needs_is_a_setting() {
        // b is 50 s of other sound followed by the first 50 s of a: only half of the shorter track lines up
        let a = fp(1, 200);
        let mut frames = fp(2, 100).frames;
        frames.extend_from_slice(&a.frames[..100]);
        let b = Fingerprint { frames };
        let rows = vec![row(1, 100), row(2, 100)];
        let fps: HashMap<i64, &Fingerprint> = [(1, &a), (2, &b)].into();
        let base = DupSettings { audio_offset_secs: 60, ..Default::default() };
        assert!(audio_links(&refs(&rows), &fps, &base).is_empty(), "80% of the shorter track must line up by default");
        let loose = DupSettings { audio_min_overlap: 0.5, ..base };
        let links = audio_links(&refs(&rows), &fps, &loose);
        assert_eq!(pairs(&links), vec![(1, 2)], "half is enough when the setting says so");
        assert_eq!(links[0].offset_secs, Some(50.0));
    }
}
