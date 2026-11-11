//! One track through a model: normalise, cut into overlapping segments, run the model, join the
//! answers by weighted overlap-add, scale back, and hand the two results to writers as soon as
//! frames are final (memory stays at the input plus one window, however long the track is).
//!
//! Normalisation is Demucs' own: the mono mix is shifted to zero mean and scaled to unit
//! standard deviation before the model and the stems are scaled back by the same deviation
//! afterwards. The mean is not added back (it is a DC offset of a few millionths, and adding it
//! to every stem would make the stems sum to the mix plus several offsets).

use crate::Error;
use crate::overlap::{OverlapAdd, Plan};
use crate::provider::{StemModel, Stereo, check_output};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentalMode {
    /// The model's non-vocal stems added together (drums + bass + other).
    SumOfStems,
    /// The original minus the vocals estimate: keeps the original's phase and dynamics, but any
    /// vocal estimation error stays in the instrumental as an inverted copy.
    MixMinusVocals,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Fraction of a segment shared with the next one (0.1 to 0.9).
    pub overlap: f32,
    pub instrumental: InstrumentalMode,
}

impl Default for Options {
    fn default() -> Self {
        Options { overlap: 0.5, instrumental: InstrumentalMode::SumOfStems }
    }
}

/// Reported after every segment (and once, with `segment == 0`, before the first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub segment: usize,
    pub segments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub frames: usize,
    pub segments: usize,
    /// The input was digital silence, so the model was not run and both outputs are silent.
    pub silent: bool,
    /// Samples that had to be limited to full scale, per output.
    pub clipped_vocals: u64,
    pub clipped_instrumental: u64,
}

/// What a writer receives: left and right slices of equal length.
pub type FrameSink<'a> = &'a mut dyn FnMut(&[f32], &[f32]) -> Result<(), Error>;

/// Below this the input counts as silence (about -120 dBFS).
const SILENCE: f64 = 1e-6;

fn limit(x: &mut [f32]) -> u64 {
    let mut n = 0;
    for v in x {
        if *v > 1.0 {
            *v = 1.0;
            n += 1;
        } else if *v < -1.0 {
            *v = -1.0;
            n += 1;
        } else if v.is_nan() {
            *v = 0.0;
            n += 1;
        }
    }
    n
}

/// Separates `left`/`right` (at the model's sample rate) into vocals and an instrumental,
/// writing both through the sinks. `progress` returns `false` to cancel; the call then returns
/// `Err(Error::Cancelled)` with whatever was already written left to the caller to discard.
pub fn separate(
    left: &[f32],
    right: &[f32],
    model: &mut dyn StemModel,
    opts: &Options,
    vocals_out: FrameSink,
    instrumental_out: FrameSink,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<Report, Error> {
    if left.len() != right.len() {
        return Err(Error::Invalid("the two channels differ in length".into()));
    }
    if left.is_empty() {
        return Err(Error::Invalid("there is no audio to separate".into()));
    }
    let info = model.info().clone();
    if info.sample_rate != crate::RATE {
        return Err(Error::Invalid(format!("the model works at {} Hz, not {} Hz", info.sample_rate, crate::RATE)));
    }
    let total = left.len();
    let plan = Plan::new(info.segment_len, opts.overlap, total);
    let segments = plan.segments();
    let mut report = Report { frames: total, segments, ..Default::default() };

    if !progress(Progress { segment: 0, segments }) {
        return Err(Error::Cancelled);
    }

    // mono mean and deviation over the whole track
    let n = total as f64;
    let mean = left.iter().zip(right).map(|(l, r)| 0.5 * (*l as f64 + *r as f64)).sum::<f64>() / n;
    let var = left.iter().zip(right).map(|(l, r)| (0.5 * (*l as f64 + *r as f64) - mean).powi(2)).sum::<f64>() / n;
    let std = var.sqrt();

    if std < SILENCE {
        report.silent = true;
        let zeros = vec![0.0f32; 65_536];
        let mut done = 0;
        while done < total {
            let k = (total - done).min(zeros.len());
            vocals_out(&zeros[..k], &zeros[..k])?;
            instrumental_out(&zeros[..k], &zeros[..k])?;
            done += k;
        }
        if !progress(Progress { segment: segments, segments }) {
            return Err(Error::Cancelled);
        }
        return Ok(report);
    }

    let (mean32, std32) = (mean as f32, std as f32);
    let mut oa = OverlapAdd::new(plan, 4);
    let mut clipped = (0u64, 0u64);
    for k in 0..segments {
        let start = plan.start(k);
        let take = |ch: &[f32]| -> Vec<f32> {
            let mut seg = vec![0.0f32; plan.window];
            let end = (start + plan.window).min(total);
            for (o, v) in seg.iter_mut().zip(&ch[start..end]) {
                *o = (v - mean32) / std32;
            }
            seg
        };
        let seg: Stereo = [take(left), take(right)];
        let out = model.separate(&seg)?;
        check_output(&out, plan.window)?;
        let chans = [&out.vocals[0][..], &out.vocals[1][..], &out.rest[0][..], &out.rest[1][..]];
        oa.push(k, &chans, &mut |at, done| {
            let len = done[0].len();
            let mut v = [done[0].iter().map(|x| x * std32).collect::<Vec<f32>>(), done[1].iter().map(|x| x * std32).collect::<Vec<f32>>()];
            let mut inst = match opts.instrumental {
                InstrumentalMode::SumOfStems => [done[2].iter().map(|x| x * std32).collect::<Vec<f32>>(), done[3].iter().map(|x| x * std32).collect::<Vec<f32>>()],
                InstrumentalMode::MixMinusVocals => [(0..len).map(|i| left[at + i] - v[0][i]).collect(), (0..len).map(|i| right[at + i] - v[1][i]).collect()],
            };
            clipped.0 += limit(&mut v[0]) + limit(&mut v[1]);
            clipped.1 += limit(&mut inst[0]) + limit(&mut inst[1]);
            vocals_out(&v[0], &v[1])?;
            instrumental_out(&inst[0], &inst[1])
        })?;
        if !progress(Progress { segment: k + 1, segments }) {
            return Err(Error::Cancelled);
        }
    }
    report.clipped_vocals = clipped.0;
    report.clipped_instrumental = clipped.1;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{FailingModel, IdentityModel, MidSideModel};
    use crate::provider::{ModelInfo, SegmentStems};
    use crate::stft::tests::noise;

    type Out = (Vec<f32>, Vec<f32>);

    fn run(l: &[f32], r: &[f32], model: &mut dyn StemModel, opts: &Options) -> Result<(Report, Out, Out), Error> {
        let (mut v, mut i) = ((Vec::new(), Vec::new()), (Vec::new(), Vec::new()));
        let rep = separate(
            l,
            r,
            model,
            opts,
            &mut |a, b| {
                v.0.extend_from_slice(a);
                v.1.extend_from_slice(b);
                Ok(())
            },
            &mut |a, b| {
                i.0.extend_from_slice(a);
                i.1.extend_from_slice(b);
                Ok(())
            },
            &mut |_| true,
        )?;
        Ok((rep, v, i))
    }

    fn mean_of(l: &[f32], r: &[f32]) -> f32 {
        l.iter().zip(r).map(|(a, b)| 0.5 * (*a as f64 + *b as f64)).sum::<f64>() as f32 / l.len() as f32
    }

    #[test]
    fn identity_model_returns_the_input_for_every_length() {
        for total in [1usize, 2, 999, 1000, 1001, 2500, 7919] {
            let l: Vec<f32> = noise(total, 5).iter().map(|v| v * 0.8).collect();
            let r: Vec<f32> = noise(total, 6).iter().map(|v| v * 0.8).collect();
            let (rep, v, i) = run(&l, &r, &mut IdentityModel::new(1000), &Options::default()).unwrap();
            assert_eq!((v.0.len(), v.1.len(), i.0.len(), i.1.len()), (total, total, total, total), "length {total}");
            assert_eq!(rep.frames, total);
            let m = mean_of(&l, &r);
            // an input of one frame has no deviation, which counts as silence
            assert_eq!(rep.silent, total == 1);
            if total > 1 {
                let err = l.iter().zip(&v.0).map(|(a, b)| (a - m - b).abs()).fold(0.0f32, f32::max);
                assert!(err < 2e-5, "length {total}: error {err}");
                assert!(i.0.iter().chain(&i.1).all(|x| *x == 0.0));
            }
        }
    }

    #[test]
    fn vocals_plus_instrumental_is_the_input_in_both_modes() {
        let (l, r) = (noise(4321, 11), noise(4321, 12));
        let m = mean_of(&l, &r);
        for mode in [InstrumentalMode::SumOfStems, InstrumentalMode::MixMinusVocals] {
            let opts = Options { overlap: 0.25, instrumental: mode };
            let (_, v, i) = run(&l, &r, &mut MidSideModel::new(1024), &opts).unwrap();
            for c in 0..2 {
                let (src, voc, ins) = if c == 0 { (&l, &v.0, &i.0) } else { (&r, &v.1, &i.1) };
                let err = (0..src.len()).map(|k| (voc[k] + ins[k] - src[k]).abs()).fold(0.0f32, f32::max);
                // sum mode: the mean was removed before the model; minus mode: nothing was removed
                let allowed = if mode == InstrumentalMode::SumOfStems { m.abs() + 3e-5 } else { 3e-5 };
                assert!(err <= allowed, "{mode:?} channel {c}: error {err}");
            }
            // the "vocals" of this model are the same in both channels (mid)
            assert!(v.0.iter().zip(&v.1).all(|(a, b)| (a - b).abs() < 1e-6));
        }
    }

    #[test]
    fn mix_minus_vocals_uses_the_original_not_the_normalised_signal() {
        let (l, r) = (noise(3000, 21), noise(3000, 22));
        let opts = Options { overlap: 0.5, instrumental: InstrumentalMode::MixMinusVocals };
        let (_, v, i) = run(&l, &r, &mut IdentityModel::new(1000), &opts).unwrap();
        let m = mean_of(&l, &r);
        // identity vocals = input - mean, so mix - vocals is the mean everywhere
        assert!(i.0.iter().all(|x| (x - m).abs() < 3e-5));
        assert!(v.0.len() == 3000);
    }

    #[test]
    fn progress_counts_segments_and_cancel_stops_the_run() {
        let (l, r) = (noise(5000, 1), noise(5000, 2));
        let plan = Plan::new(1000, 0.5, 5000);
        let mut seen = Vec::new();
        let ok = separate(&l, &r, &mut IdentityModel::new(1000), &Options::default(), &mut |_, _| Ok(()), &mut |_, _| Ok(()), &mut |p| {
            seen.push(p);
            true
        });
        assert!(ok.is_ok());
        assert_eq!(seen.first(), Some(&Progress { segment: 0, segments: plan.segments() }));
        assert_eq!(seen.last(), Some(&Progress { segment: plan.segments(), segments: plan.segments() }));
        assert_eq!(seen.len(), plan.segments() + 1);

        let mut calls = 0;
        let cancelled = separate(&l, &r, &mut IdentityModel::new(1000), &Options::default(), &mut |_, _| Ok(()), &mut |_, _| Ok(()), &mut |_| {
            calls += 1;
            calls < 3
        });
        assert_eq!(cancelled.unwrap_err(), Error::Cancelled);
        assert_eq!(calls, 3, "stops at the first refusal, not at the end");

        // cancelling before the first segment never calls the model
        let early = separate(&l, &r, &mut FailingModel::new(1000, 0), &Options::default(), &mut |_, _| Ok(()), &mut |_, _| Ok(()), &mut |_| false);
        assert_eq!(early.unwrap_err(), Error::Cancelled);
    }

    #[test]
    fn model_and_writer_errors_propagate() {
        let (l, r) = (noise(3000, 1), noise(3000, 2));
        let e = run(&l, &r, &mut FailingModel::new(1000, 2), &Options::default()).unwrap_err();
        assert_eq!(e, Error::Model("synthetic model failure".into()));
        let w = separate(&l, &r, &mut IdentityModel::new(1000), &Options::default(), &mut |_, _| Err(Error::Io("disk full".into())), &mut |_, _| Ok(()), &mut |_| true);
        assert_eq!(w.unwrap_err(), Error::Io("disk full".into()));
    }

    #[test]
    fn silence_skips_the_model() {
        let z = vec![0.0f32; 150_000];
        let (rep, v, i) = run(&z, &z, &mut FailingModel::new(1000, 0), &Options::default()).unwrap();
        assert!(rep.silent);
        assert_eq!((v.0.len(), i.1.len()), (150_000, 150_000));
        assert!(v.0.iter().chain(&i.0).all(|x| *x == 0.0));
    }

    struct Loud(ModelInfo);
    impl StemModel for Loud {
        fn info(&self) -> &ModelInfo {
            &self.0
        }
        fn separate(&mut self, s: &Stereo) -> Result<SegmentStems, Error> {
            let boost = |c: &Vec<f32>| c.iter().map(|x| x * 40.0).collect::<Vec<f32>>();
            Ok(SegmentStems { vocals: [boost(&s[0]), boost(&s[1])], rest: [s[0].clone(), s[1].clone()] })
        }
    }

    #[test]
    fn overload_is_limited_and_counted() {
        let (l, r) = (noise(2000, 8), noise(2000, 9));
        let info = ModelInfo { id: "loud".into(), name: "loud".into(), sample_rate: crate::RATE, segment_len: 1000, device: "CPU".into() };
        let (rep, v, _) = run(&l, &r, &mut Loud(info), &Options::default()).unwrap();
        assert!(rep.clipped_vocals > 0, "40x gain must clip");
        assert!(v.0.iter().chain(&v.1).all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn bad_requests_are_refused() {
        let mut m = IdentityModel::new(1000);
        assert!(run(&[0.1; 10], &[0.1; 9], &mut m, &Options::default()).is_err());
        assert!(run(&[], &[], &mut m, &Options::default()).is_err());
        let mut wrong_rate = IdentityModel::new(1000);
        let info = ModelInfo { sample_rate: 48_000, ..wrong_rate.info().clone() };
        struct Rate(ModelInfo);
        impl StemModel for Rate {
            fn info(&self) -> &ModelInfo {
                &self.0
            }
            fn separate(&mut self, s: &Stereo) -> Result<SegmentStems, Error> {
                Ok(SegmentStems { vocals: s.clone(), rest: s.clone() })
            }
        }
        let _ = &mut wrong_rate;
        assert!(run(&[0.1, 0.2, 0.3], &[0.1, 0.2, 0.3], &mut Rate(info), &Options::default()).is_err());
    }
}
