//! The live mix engine: the same block mixer as [`engine::render`](crate::engine::render), pulled
//! a few hundred frames at a time by a player instead of pushed into a file.
//!
//! A [`Live`] owns a mix, its clip streams and the master limiter. `render` hands out the next
//! frames of the master output, `seek` restarts it anywhere (with the same pre-roll an offline
//! window uses), and a new document is applied by building a new [`Live`] at the position and
//! crossfading its output with what was already queued (see [`crate::player`]). Because the
//! block code is shared with the offline render ([`engine::mix_block`](crate::engine)) and the
//! limiter is the same, a `Live` that starts at frame 0 produces exactly the samples of an
//! offline render, and one that starts at frame `p` exactly those of an offline window from `p`.

use pm_formats::mix::{Mix, Track};

use crate::engine::{self, audible_clips, make_slots, mix_block, Slot, SourceState, Tally, BLOCK, PREROLL};
use crate::fx::{PeqMode, PeqPlan};
use crate::master::Limiter;
use crate::Error;

/// What a live engine renders with; the same knobs as the offline [`engine::Options`].
#[derive(Debug, Clone, Default)]
pub struct LiveOptions {
    /// What `Wah` tools render as.
    pub peq: PeqMode,
    /// See [`crate::engine::Options::vocal_tools`].
    pub vocal_tools: Option<std::sync::Arc<std::collections::HashSet<String>>>,
    /// Gain in dB per clip, by index into `mix.tracks` (missing = 0 dB).
    pub clip_gain_db: Vec<f64>,
    /// Gain in dB on the sum of all clips, before the limiter.
    pub master_gain_db: f64,
}

/// Frames over which a changed master gain moves to its new value (no click on a fader move).
const MASTER_RAMP: usize = 1024;

pub struct Live {
    mix: Mix,
    clip_gain_db: Vec<f64>,
    slots: Vec<Slot>,
    limiter: Limiter,
    peq: PeqPlan,
    end_all: i64,
    /// Start of the next internal block.
    b0: i64,
    /// First timeline frame of the output since the last seek.
    from: i64,
    /// Output frames still to drop (the pre-roll).
    skip: usize,
    /// Output frames handed out since the last seek.
    produced: u64,
    /// Frames the limiter has produced, the dropped lead-in included, and the number it will
    /// produce in all.
    emitted: u64,
    total: u64,
    drained: bool,
    master: f32,
    master_target: f32,
    ml: Vec<f32>,
    mr: Vec<f32>,
    cl: Vec<f32>,
    cr: Vec<f32>,
    stage_l: Vec<f32>,
    stage_r: Vec<f32>,
    stage_at: usize,
    tally: Tally,
    /// Warnings already handed out.
    told: usize,
}

impl Live {
    /// An engine over `mix`, positioned at `from` (timeline frames). Fails when no clip of the
    /// mix is audible.
    pub fn new(mix: &Mix, opts: &LiveOptions, from: i64) -> Result<Live, Error> {
        let mut tally = Tally::default();
        let slots = make_slots(mix, &opts.clip_gain_db, &mut tally.warnings);
        if slots.is_empty() {
            return Err(Error::Invalid("No clip in this mix can be heard: every clip is muted or has no length.".into()));
        }
        let end_all = slots.iter().map(|s| s.ve).max().unwrap();
        let g = engine::lin(opts.master_gain_db);
        let b = BLOCK as usize;
        let mut l = Live {
            mix: mix.clone(),
            clip_gain_db: opts.clip_gain_db.clone(),
            slots,
            limiter: Limiter::new(),
            peq: PeqPlan::new(opts.peq, opts.vocal_tools.clone()),
            end_all,
            b0: 0,
            from: 0,
            skip: 0,
            produced: 0,
            emitted: 0,
            total: 0,
            drained: false,
            master: g,
            master_target: g,
            ml: vec![0.0; b],
            mr: vec![0.0; b],
            cl: vec![0.0; b],
            cr: vec![0.0; b],
            stage_l: Vec::new(),
            stage_r: Vec::new(),
            stage_at: 0,
            tally,
            told: 0,
        };
        l.rewind(from);
        Ok(l)
    }

    fn rewind(&mut self, frame: i64) {
        let mut warnings = Vec::new();
        self.slots = make_slots(&self.mix, &self.clip_gain_db, &mut warnings);
        self.limiter = Limiter::new();
        let from = frame.clamp(0, self.end_all);
        // the same rule as an offline window: begin on a block boundary some way before
        let begin = if from > 0 { ((from - PREROLL).max(0) / BLOCK) * BLOCK } else { 0 };
        self.b0 = begin;
        self.from = from;
        self.skip = (from - begin) as usize;
        self.produced = 0;
        self.emitted = 0;
        self.total = (self.end_all - begin) as u64;
        self.drained = from >= self.end_all;
        self.master = self.master_target;
        self.stage_l.clear();
        self.stage_r.clear();
        self.stage_at = 0;
    }

    /// Restarts at timeline frame `frame` (clamped to the mix): fresh streams and limiter, with the
    /// pre-roll an offline window has.
    pub fn seek(&mut self, frame: i64) {
        self.rewind(frame);
    }

    /// Timeline frame of the next output frame.
    pub fn position(&self) -> i64 {
        self.from + self.produced as i64
    }

    /// End of the audible mix, timeline frames.
    pub fn end(&self) -> i64 {
        self.end_all
    }

    /// True once every frame up to the end of the mix has been handed out.
    pub fn finished(&self) -> bool {
        self.drained && self.stage_at >= self.stage_l.len()
    }

    /// Moves the master gain (dB) to a new value over the next [`MASTER_RAMP`] frames.
    pub fn set_master_db(&mut self, db: f64) {
        self.master_target = engine::lin(db);
    }

    /// New warnings (a clip that cannot be loaded, corrupt frames) since the last call.
    pub fn take_warnings(&mut self) -> Vec<String> {
        let w = self.tally.warnings[self.told..].to_vec();
        self.told = self.tally.warnings.len();
        w
    }

    /// Number of clips of the mix that are audible (for progress counts).
    pub fn clips(&self) -> usize {
        self.slots.len()
    }

    /// Fills `l` / `r` (equal length) with the next output frames; returns how many were written,
    /// fewer than asked only at the end of the mix. `get` supplies the decoded audio of a clip.
    pub fn render(&mut self, get: &mut dyn FnMut(&Track) -> SourceState, l: &mut [f32], r: &mut [f32]) -> usize {
        let want = l.len().min(r.len());
        let mut done = 0;
        while done < want {
            if self.stage_at >= self.stage_l.len() {
                if !self.step(get) {
                    break;
                }
                continue;
            }
            let k = (want - done).min(self.stage_l.len() - self.stage_at);
            l[done..done + k].copy_from_slice(&self.stage_l[self.stage_at..self.stage_at + k]);
            r[done..done + k].copy_from_slice(&self.stage_r[self.stage_at..self.stage_at + k]);
            self.stage_at += k;
            self.produced += k as u64;
            done += k;
        }
        done
    }

    /// Produces the next internal block into the stage; false when there is nothing left.
    fn step(&mut self, get: &mut dyn FnMut(&Track) -> SourceState) -> bool {
        self.stage_l.clear();
        self.stage_r.clear();
        self.stage_at = 0;
        if self.b0 < self.end_all {
            let b0 = self.b0;
            let b1 = (b0 + BLOCK).min(self.end_all);
            let n = (b1 - b0) as usize;
            mix_block(&mut self.slots, b0, b1, &mut self.ml, &mut self.mr, &mut self.cl, &mut self.cr, &self.peq, get, &mut self.tally);
            if self.master != self.master_target {
                let (a, z) = (self.master, self.master_target);
                let ramp = n.min(MASTER_RAMP).max(1);
                for k in 0..n {
                    let g = if k < ramp { a + (z - a) * ((k + 1) as f32 / ramp as f32) } else { z };
                    self.ml[k] *= g;
                    self.mr[k] *= g;
                }
                self.master = z;
            } else if self.master != 1.0 {
                for k in 0..n {
                    self.ml[k] *= self.master;
                    self.mr[k] *= self.master;
                }
            }
            for k in 0..n {
                if let Some((l, r)) = self.limiter.push(self.ml[k], self.mr[k]) {
                    self.stage_l.push(l);
                    self.stage_r.push(r);
                }
            }
            self.emitted += self.stage_l.len() as u64;
            self.drop_lead_in();
            self.b0 = b1;
            true
        } else if !self.drained {
            // drain the limiter's look-ahead, as the offline render does
            while self.limiter.pending() > 0 {
                if let Some((l, r)) = self.limiter.push(0.0, 0.0) {
                    self.stage_l.push(l);
                    self.stage_r.push(r);
                }
                if self.emitted + self.stage_l.len() as u64 >= self.total {
                    break;
                }
            }
            self.drop_lead_in();
            self.drained = true;
            true
        } else {
            false
        }
    }

    fn drop_lead_in(&mut self) {
        let drop = self.skip.min(self.stage_l.len());
        if drop > 0 {
            self.skip -= drop;
            self.stage_l.drain(..drop);
            self.stage_r.drain(..drop);
        }
    }
}

/// Which clips of a mix are audible, for callers that only need the count.
pub fn audible_count(mix: &Mix) -> usize {
    audible_clips(mix).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Source;
    use crate::engine::{render_to_vecs, Options, Resolver};
    use pm_formats::mix::{Position, Transition};
    use std::sync::Arc;

    /// A source of `secs` seconds: a tone plus a clicky envelope so tempo changes are audible.
    fn tone(freq: f32, secs: f32, amp: f32) -> Arc<Source> {
        let n = (secs * 44100.0) as usize;
        let l: Vec<f32> = (0..n).map(|i| ((i as f32) * freq * std::f32::consts::TAU / 44100.0).sin() * amp * (1.0 + 0.3 * ((i as f32) * 0.0007).sin())).collect();
        let r: Vec<f32> = (0..n).map(|i| ((i as f32) * (freq * 1.01) * std::f32::consts::TAU / 44100.0).sin() * amp).collect();
        Source::new(l, r)
    }

    fn track(id: i64, start: f64, len: f64, speed: f32) -> Track {
        Track::new(id, "x.wav", 120.0, speed, Position { start, length: len, limit_size: true, ..Position::default() })
    }

    fn mix3() -> Mix {
        let s = 44100.0;
        let mut m = Mix::default();
        m.tracks = vec![track(1, 0.0, 6.0 * s, 1.0), track(2, 4.0 * s, 6.0 * s, 1.0), track(3, 8.0 * s, 5.0 * s, 1.0)];
        for (a, b) in [(4.0, 6.0), (8.0, 10.0)] {
            m.transitions.push(Transition::new(Position { start: a * s, length: (b - a) * s, ..Position::default() }));
        }
        m
    }

    struct Res(Vec<(i64, Arc<Source>)>);
    impl Resolver for Res {
        fn source(&self, t: &Track) -> Result<Arc<Source>, Error> {
            self.0.iter().find(|(i, _)| *i == t.library_id).map(|(_, s)| s.clone()).ok_or_else(|| Error::Invalid("missing".into()))
        }
    }

    fn res() -> Res {
        Res(vec![(1, tone(220.0, 8.0, 0.6)), (2, tone(330.0, 8.0, 0.6)), (3, tone(440.0, 8.0, 0.6))])
    }

    fn pull(live: &mut Live, res: &Res, total: usize, chunk: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut ol, mut or) = (Vec::new(), Vec::new());
        let (mut a, mut b) = (vec![0.0; chunk], vec![0.0; chunk]);
        let mut get = |t: &Track| match res.source(t) {
            Ok(s) => SourceState::Ready(s),
            Err(e) => SourceState::Failed(e.to_string()),
        };
        while ol.len() < total {
            let n = live.render(&mut get, &mut a, &mut b);
            ol.extend_from_slice(&a[..n]);
            or.extend_from_slice(&b[..n]);
            if n < chunk {
                break;
            }
        }
        (ol, or)
    }

    #[test]
    fn live_equals_offline_from_the_start() {
        let (mix, res) = (mix3(), res());
        let opts = Options { clip_gain_db: vec![1.5, -2.0, 0.5], master_gain_db: 2.0, ..Options::default() };
        let (ol, or, _) = render_to_vecs(&mix, &res, &opts).unwrap();
        let lo = LiveOptions { peq: opts.peq, vocal_tools: None, clip_gain_db: opts.clip_gain_db.clone(), master_gain_db: 2.0 };
        // odd chunk sizes on purpose: the player pulls whatever the device asks for
        for chunk in [1000usize, 333, 4096, 5001] {
            let mut live = Live::new(&mix, &lo, 0).unwrap();
            let (ll, lr) = pull(&mut live, &res, usize::MAX, chunk);
            assert_eq!(ll.len(), ol.len(), "length, chunk {chunk}");
            assert!(ll == ol && lr == or, "live differs from the offline render (chunk {chunk})");
            assert!(live.finished());
        }
    }

    #[test]
    fn seek_equals_an_offline_window() {
        let (mix, res) = (mix3(), res());
        let lo = LiveOptions::default();
        let mut live = Live::new(&mix, &lo, 0).unwrap();
        let (full_l, _) = pull(&mut live, &res, usize::MAX, 4096);
        for p in [0usize, 5000, 44100 * 5 + 17, 44100 * 9, 44100 * 12 + 3] {
            live.seek(p as i64);
            let (ll, lr) = pull(&mut live, &res, usize::MAX, 777);
            let (wl, wr, _) = render_to_vecs(&mix, &res, &Options { from: p as i64, ..Options::default() }).unwrap();
            assert!(ll == wl && lr == wr, "seek {p}: differs from the offline window");
            // and the full render, once the pre-roll has settled the filters, within rounding
            let n = ll.len();
            assert_eq!(n, full_l.len() - p, "seek {p}: length");
            let worst = ll.iter().zip(&full_l[p..]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 1e-3, "seek {p}: worst difference to the full render {worst}");
        }
    }

    #[test]
    fn seek_past_the_end_is_finished() {
        let (mix, res) = (mix3(), res());
        let mut live = Live::new(&mix, &LiveOptions::default(), 10_000_000).unwrap();
        let (l, _) = pull(&mut live, &res, 10, 100);
        assert!(l.is_empty() && live.finished());
    }

    #[test]
    fn pending_clips_are_silent_and_picked_up_later() {
        let mix = mix3();
        let lo = LiveOptions::default();
        let res = res();
        let mut live = Live::new(&mix, &lo, 0).unwrap();
        let (mut a, mut b) = (vec![0.0; 4096], vec![0.0; 4096]);
        let mut ready = false;
        let mut get = |t: &Track| if ready { SourceState::Ready(res.source(t).unwrap()) } else { SourceState::Pending };
        live.render(&mut get, &mut a, &mut b);
        assert!(a.iter().all(|&x| x == 0.0), "a pending clip must be silent");
        ready = true;
        let mut get = |t: &Track| if ready { SourceState::Ready(res.source(t).unwrap()) } else { SourceState::Pending };
        let mut peak = 0.0f32;
        for _ in 0..4 {
            live.render(&mut get, &mut a, &mut b);
            peak = a.iter().fold(peak, |m, x| m.max(x.abs()));
        }
        assert!(peak > 0.1, "the clip must start once its audio is there");
    }

    #[test]
    fn a_missing_clip_is_reported_once_and_skipped() {
        let mix = mix3();
        let res = Res(vec![(1, tone(220.0, 8.0, 0.6))]);
        let mut live = Live::new(&mix, &LiveOptions::default(), 0).unwrap();
        let _ = pull(&mut live, &res, usize::MAX, 4096);
        let w = live.take_warnings();
        assert_eq!(w.iter().filter(|s| s.contains("Skipped")).count(), 2, "{w:?}");
        assert!(live.take_warnings().is_empty());
    }

    #[test]
    fn a_skipped_clip_is_named_by_its_track_not_a_slot_number() {
        let mut mix = mix3();
        mix.tracks[1].fuzzy_id.artist = "Some Artist".into();
        mix.tracks[1].fuzzy_id.title = "Some Title".into();
        let res = Res(vec![(1, tone(220.0, 8.0, 0.6)), (3, tone(330.0, 8.0, 0.6))]);
        let mut live = Live::new(&mix, &LiveOptions::default(), 0).unwrap();
        let _ = pull(&mut live, &res, usize::MAX, 4096);
        let w = live.take_warnings();
        assert!(w.iter().any(|s| s.starts_with("Skipped \"Some Artist - Some Title\":")), "{w:?}");
        assert!(w.iter().all(|s| !s.contains("clip 2")), "no slot number when the track has a name: {w:?}");
    }

    #[test]
    fn master_gain_moves_without_a_step() {
        let mix = mix3();
        let res = res();
        let mut live = Live::new(&mix, &LiveOptions::default(), 0).unwrap();
        let (a, _) = pull(&mut live, &res, 44100, 1024);
        live.set_master_db(-20.0);
        let (b, _) = pull(&mut live, &res, 44100, 1024);
        // largest sample-to-sample step stays near that of the signal (no jump from the gain change)
        let step = |v: &[f32]| v.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(step(&b) <= step(&a) * 1.2, "{} vs {}", step(&b), step(&a));
        let peak = b[b.len() - 4000..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak < 0.12, "{peak}");
    }

    #[test]
    fn tempo_stretched_clips_stay_equal_to_offline() {
        let s = 44100.0;
        let mut m = Mix::default();
        m.tracks = vec![track(1, 0.0, 6.0 * s, 1.1), track(2, 3.0 * s, 5.0 * s, 0.9)];
        m.transitions.push(Transition::new(Position { start: 3.0 * s, length: 2.0 * s, ..Position::default() }));
        let res = Res(vec![(1, tone(200.0, 8.0, 0.5)), (2, tone(300.0, 8.0, 0.5))]);
        let (ol, _, _) = render_to_vecs(&m, &res, &Options::default()).unwrap();
        let mut live = Live::new(&m, &LiveOptions::default(), 0).unwrap();
        let (ll, _) = pull(&mut live, &res, usize::MAX, 1500);
        assert!(ll == ol);
    }
}
