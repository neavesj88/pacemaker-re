//! The mix renderer: clips on a timeline, crossfades from transitions, master limiter.
//!
//! The timeline is processed in blocks of [`BLOCK`] frames. For every block each audible clip
//! renders its slice through its [`ClipStream`], the crossfader gain of its transitions is
//! applied, and the sum goes through the master [`Limiter`] to the sink. Clips are decoded when
//! their first block is reached and dropped after their last, so memory holds only the clips
//! playing now. Everything is single-threaded and uses no randomness, so a render is
//! bit-for-bit repeatable.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pm_formats::mix::{Mix, Track, Transition};

use crate::audio::{self, Source};
use crate::clip::ClipStream;
use crate::env::Env;
use crate::fx::PeqMode;
use crate::master::Limiter;
use crate::Error;

/// Timeline frames per processing block.
pub const BLOCK: i64 = 4096;

/// Provides the decoded audio of a track. The app implements it with the library lookup (and a
/// small cache for previews); tests use [`DirResolver`].
pub trait Resolver {
    fn source(&self, track: &Track) -> Result<Arc<Source>, Error>;
}

/// Resolves `Track::file_path` against a directory (or as is when absolute) and decodes it.
pub struct DirResolver(pub PathBuf);

impl Resolver for DirResolver {
    fn source(&self, track: &Track) -> Result<Arc<Source>, Error> {
        audio::load(&self.0.join(&track.file_path))
    }
}

/// What to render. The default renders the whole timeline from frame 0.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// First timeline frame (default 0).
    pub from: i64,
    /// End frame (exclusive); `None` renders to the end of the last clip.
    pub to: Option<i64>,
    /// Start at the first audible frame instead of timeline 0 (used for exports; previews keep
    /// the timeline positions the editor shows).
    pub skip_leading_silence: bool,
    /// What `Wah` tools render as. The default is the stock Wah stand-in; the app sets the mode
    /// of the library the mix belongs to (the same for a preview and an export).
    pub peq: PeqMode,
}

/// Receiver of the master output: left and right samples of one block.
pub type Sink<'a> = dyn FnMut(&[f32], &[f32]) -> Result<(), Error> + 'a;

impl Options {
    pub fn window_secs(from_sec: f64, to_sec: f64) -> Options {
        let f = |s: f64| (s.max(0.0) * crate::RATE as f64).round() as i64;
        Options { from: f(from_sec), to: Some(f(to_sec)), ..Options::default() }
    }
}

/// What a render reports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rendered {
    /// Frames written to the sink.
    pub frames: u64,
    /// Things that were skipped (a clip whose audio could not be loaded, a transition that could
    /// not be matched to two clips).
    pub warnings: Vec<String>,
    /// Clips whose audio was loaded and rendered.
    pub clips_rendered: usize,
    /// Clips left out because their audio could not be loaded.
    pub clips_failed: usize,
}

fn r(x: f64) -> i64 {
    x.round() as i64
}

/// The audible part of one mix track on the timeline, in frames at [`crate::RATE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudibleClip {
    /// Index into `mix.tracks`.
    pub index: usize,
    /// First audible frame (`start + LOffset`, rounded).
    pub start: i64,
    /// End of the audible part, exclusive (`start + length - ROffset`, rounded).
    pub end: i64,
}

/// The tracks the engine renders, in document order: not bypassed, with a non-empty audible
/// range that ends after frame 0. [`render`] builds its clips from this list, and anything that
/// has to agree with a render (the track list and cue sheet of an export, its length) reads it
/// too, so the two cannot disagree about where a clip starts.
pub fn audible_clips(mix: &Mix) -> Vec<AudibleClip> {
    mix.tracks
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            if t.clip_bypassed {
                return None;
            }
            let start = r(t.position.start + t.position.l_offset);
            let end = r(t.position.start + t.position.length - t.position.r_offset);
            (end > start && end > 0).then_some(AudibleClip { index, start, end })
        })
        .collect()
}

/// The timeline frames `[from, to)` a render with `skip_leading_silence` (every export) covers:
/// the audio file is `to - from` frames long and its frame 0 is timeline frame `from`. `None`
/// when no clip is audible.
pub fn export_span(mix: &Mix) -> Option<(i64, i64)> {
    let a = audible_clips(mix);
    Some((a.iter().map(|c| c.start).min()?, a.iter().map(|c| c.end).max()?))
}

/// A clip as the engine sees it.
struct Slot<'a> {
    track: &'a Track,
    /// Audible range on the timeline, `[vs, ve)`.
    vs: i64,
    ve: i64,
    fades: Vec<Fade>,
    stream: Option<ClipStream>,
    dead: bool,
    finished: bool,
}

/// One crossfade a clip takes part in.
struct Fade {
    ts: i64,
    te: i64,
    later: bool,
    curve: Env,
}

fn curve_of(t: &Transition) -> Env {
    if t.envelope.is_empty() {
        return Env::from_points(&[(0.0, 0.0), (1.0, 1.0)]);
    }
    Env::from_points(&t.envelope.iter().map(|n| (f64::from(n.pos) / f64::from(u32::MAX), n.value.clamp(0.0, 1.0))).collect::<Vec<_>>())
}

impl Fade {
    /// Equal-power gain at timeline frame `t`. The earlier clip is untouched before the
    /// transition starts; the later one sits at the fader's first position until it does.
    fn gain(&self, t: i64) -> f32 {
        let x = ((t - self.ts) as f64 / (self.te - self.ts) as f64).clamp(0.0, 1.0);
        let v = self.curve.at(x, if self.later { 1.0 } else { 0.0 });
        let a = v * std::f32::consts::FRAC_PI_2;
        if self.later {
            a.sin()
        } else if t < self.ts {
            1.0
        } else {
            a.cos()
        }
    }
}

/// Matches each transition to the two clips it joins. The document does not link them (see
/// notes/mix-format.md), so the rule is geometric: of the clips audible during the transition,
/// the one that starts last is the incoming clip and, of the rest, the one that ends last is the
/// outgoing clip.
fn assign_fades(slots: &mut [Slot], mix: &Mix, warnings: &mut Vec<String>) {
    for (ti, tr) in mix.transitions.iter().enumerate() {
        if tr.bypassed {
            continue;
        }
        let (ts, te) = (r(tr.position.start), r(tr.position.start + tr.position.length));
        if te <= ts {
            continue;
        }
        let mut cand: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].vs < te && slots[i].ve > ts).collect();
        if cand.len() < 2 {
            warnings.push(format!("transition {} overlaps fewer than two clips and was ignored", ti + 1));
            continue;
        }
        cand.sort_by_key(|&i| (slots[i].vs, i));
        let later = *cand.last().unwrap();
        let earlier = cand[..cand.len() - 1].iter().copied().max_by_key(|&i| (slots[i].ve, i)).unwrap();
        let curve = curve_of(tr);
        slots[earlier].fades.push(Fade { ts, te, later: false, curve: curve.clone() });
        slots[later].fades.push(Fade { ts, te, later: true, curve });
    }
}

/// Renders `mix` and feeds the master output to `sink` block by block (left, right).
/// `progress` receives 0..1 and returns `false` to cancel.
pub fn render(
    mix: &Mix,
    res: &dyn Resolver,
    opts: &Options,
    progress: &mut dyn FnMut(f64) -> bool,
    sink: &mut Sink,
) -> Result<Rendered, Error> {
    let mut warnings = Vec::new();
    let mut slots: Vec<Slot> = audible_clips(mix)
        .into_iter()
        .map(|a| Slot { track: &mix.tracks[a.index], vs: a.start, ve: a.end, fades: Vec::new(), stream: None, dead: false, finished: false })
        .collect();
    if slots.is_empty() {
        return Err(Error::Invalid("the mix has no audible clips".into()));
    }
    assign_fades(&mut slots, mix, &mut warnings);

    let end_all = slots.iter().map(|s| s.ve).max().unwrap();
    let mut from = opts.from.max(0);
    if opts.skip_leading_silence && opts.from <= 0 {
        from = slots.iter().map(|s| s.vs).min().unwrap();
    }
    let to = opts.to.map_or(end_all, |t| t.min(end_all));
    if to <= from {
        return Err(Error::Invalid("nothing to render: the range is empty or past the end of the mix".into()));
    }
    let total = (to - from) as f64;

    let mut limiter = Limiter::new();
    let (mut ml, mut mr) = (vec![0.0f32; BLOCK as usize], vec![0.0f32; BLOCK as usize]);
    let (mut cl, mut cr) = (vec![0.0f32; BLOCK as usize], vec![0.0f32; BLOCK as usize]);
    let (mut ol, mut or) = (Vec::<f32>::new(), Vec::<f32>::new());
    let mut written = 0u64;
    let (mut clips_rendered, mut clips_failed) = (0usize, 0usize);

    let mut b0 = from;
    while b0 < to {
        let b1 = (b0 + BLOCK).min(to);
        let n = (b1 - b0) as usize;
        ml[..n].fill(0.0);
        mr[..n].fill(0.0);
        for (si, slot) in slots.iter_mut().enumerate() {
            if slot.dead || slot.finished || slot.vs >= b1 {
                continue;
            }
            if slot.ve <= b0 {
                slot.stream = None;
                slot.finished = true;
                continue;
            }
            let (lo, hi) = (slot.vs.max(b0), slot.ve.min(b1));
            let start = r(slot.track.position.start);
            if slot.stream.is_none() {
                match res.source(slot.track) {
                    Ok(src) => {
                        if src.glitches > 0 {
                            warnings.push(format!("clip {}: {} corrupt audio frame(s) were replaced by silence", si + 1, src.glitches));
                        }
                        clips_rendered += 1;
                        slot.stream = Some(ClipStream::new(slot.track, src, lo - start, opts.peq));
                    }
                    Err(e) => {
                        warnings.push(format!("clip {} skipped: {e}", si + 1));
                        clips_failed += 1;
                        slot.dead = true;
                        continue;
                    }
                }
            }
            let m = (hi - lo) as usize;
            let off = (lo - b0) as usize;
            let stream = slot.stream.as_mut().unwrap();
            stream.render(lo - start, &mut cl[..m], &mut cr[..m]);
            if slot.fades.is_empty() {
                for k in 0..m {
                    ml[off + k] += cl[k];
                    mr[off + k] += cr[k];
                }
            } else {
                for k in 0..m {
                    let t = lo + k as i64;
                    let g: f32 = slot.fades.iter().map(|f| f.gain(t)).product();
                    ml[off + k] += cl[k] * g;
                    mr[off + k] += cr[k] * g;
                }
            }
            if hi >= slot.ve {
                slot.stream = None;
                slot.finished = true;
            }
        }
        ol.clear();
        or.clear();
        for k in 0..n {
            if let Some((l, rr)) = limiter.push(ml[k], mr[k]) {
                ol.push(l);
                or.push(rr);
            }
        }
        if !ol.is_empty() {
            sink(&ol, &or)?;
            written += ol.len() as u64;
        }
        b0 = b1;
        if !progress((b0 - from) as f64 / total) {
            return Err(Error::Cancelled);
        }
    }
    // drain the limiter's look-ahead
    ol.clear();
    or.clear();
    while limiter.pending() > 0 {
        if let Some((l, rr)) = limiter.push(0.0, 0.0) {
            ol.push(l);
            or.push(rr);
        }
        if written + ol.len() as u64 >= (to - from) as u64 {
            break;
        }
    }
    if !ol.is_empty() {
        sink(&ol, &or)?;
        written += ol.len() as u64;
    }
    Ok(Rendered { frames: written, warnings, clips_rendered, clips_failed })
}

/// Renders into memory (tests, short previews): planar left and right.
pub fn render_to_vecs(mix: &Mix, res: &dyn Resolver, opts: &Options) -> Result<(Vec<f32>, Vec<f32>, Rendered), Error> {
    let (mut l, mut rr) = (Vec::new(), Vec::new());
    let out = render(mix, res, opts, &mut |_| true, &mut |a, b| {
        l.extend_from_slice(a);
        rr.extend_from_slice(b);
        Ok(())
    })?;
    Ok((l, rr, out))
}

/// A path to the audio file for tests and the CLI: `dir/file_path`.
pub fn dir_resolver(dir: &Path) -> DirResolver {
    DirResolver(dir.to_path_buf())
}
