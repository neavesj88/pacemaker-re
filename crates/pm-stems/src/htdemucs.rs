//! HTDemucs through ONNX Runtime (the `ort` crate, `load-dynamic`): nothing is linked or
//! downloaded at build time, `onnxruntime.dll` is loaded from a path the caller gives.
//!
//! The graph this is written for is the community ORT export described in the model manifest
//! (`gentij/htdemucs-ort`, notes/vocal-isolation-pc.md): inputs `input` `[1, 2, 343980]` (the
//! waveform) and `x` `[1, 4, 2048, 336]` (the complex-as-channels spectrogram, made here by
//! [`crate::demucs`]); one output `[1, 4, 2, 343980]` with the stems drums, bass, other, vocals.
//! A graph that returns the spectrogram branch and the waveform branch as two outputs (the
//! usual `demucs.onnx` export) is handled too: the spectrogram is inverted here and added.
//!
//! UNVERIFIED against a real model file: the weights were not downloaded while writing this
//! (consent-gated in the app), so the tensor handling follows the manifest and the export notes
//! only. The shape checks below turn a mismatch into a clear error instead of garbage audio.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::ep::{self, ExecutionProvider};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use crate::Error;
use crate::demucs::{self, BINS, Frontend};
use crate::provider::{ModelInfo, SegmentStems, StemModel, Stereo};

/// Frames per call of the exported graph (7.8 s at 44.1 kHz).
pub const SEGMENT: usize = 343_980;
/// drums, bass, other, vocals: the order of the stems axis.
const VOCALS: usize = 3;
const STEMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// A GPU provider when this onnxruntime has one that works, else the CPU.
    Auto,
    Cpu,
    /// Only a GPU provider; fails when none is available.
    Gpu,
}

pub struct LoadOptions<'a> {
    /// The `onnxruntime` shared library (`onnxruntime.dll` on Windows).
    pub runtime: &'a Path,
    pub model: &'a Path,
    pub device: Device,
    /// Intra-op threads for the CPU path; 0 lets onnxruntime choose.
    pub threads: usize,
}

/// onnxruntime can be loaded once per process (and only one environment exists): remember what
/// was loaded so a second request for the same library reuses it and a request for another one
/// says why it cannot be honoured.
static ENV: Mutex<Option<(PathBuf, Environment)>> = Mutex::new(None);

fn model_err(what: &str, e: impl std::fmt::Display) -> Error {
    Error::Model(format!("{what}: {e}"))
}

fn environment(runtime: &Path) -> Result<Environment, Error> {
    let mut slot = ENV.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((loaded, env)) = slot.as_ref() {
        if loaded == runtime {
            return Ok(env.clone());
        }
        return Err(Error::Model(format!(
            "onnxruntime was already loaded from {} in this session; restart the app to use {}",
            loaded.display(),
            runtime.display()
        )));
    }
    if !runtime.is_file() {
        return Err(Error::Model(format!("onnxruntime was not found at {}", runtime.display())));
    }
    let builder = ort::init_from(runtime).map_err(|e| model_err(&format!("could not load {}", runtime.display()), e))?;
    let env = builder.build().map_err(|e| model_err("could not start onnxruntime", e))?;
    *slot = Some((runtime.to_path_buf(), env.clone()));
    Ok(env)
}

/// Names of the GPU providers this onnxruntime build offers, in the order they are tried.
fn gpu_providers() -> Vec<&'static str> {
    let mut v = Vec::new();
    if ep::DirectML::is_available().unwrap_or(false) {
        v.push("DirectML");
    }
    if ep::CUDA::is_available().unwrap_or(false) {
        v.push("CUDA");
    }
    v
}

fn build_session(env: &Environment, model: &Path, gpu: Option<&str>, threads: usize) -> Result<Session, Error> {
    let mut b = Session::builder(env).map_err(|e| model_err("could not create a session", e))?;
    if threads > 0 {
        b = b.with_intra_threads(threads).map_err(|e| model_err("could not set the thread count", e))?;
    }
    match gpu {
        Some("DirectML") => {
            // DirectML needs sequential execution and no memory-pattern reuse (onnxruntime docs)
            b = b.with_memory_pattern(false).map_err(|e| model_err("session options", e))?;
            b = b.with_parallel_execution(false).map_err(|e| model_err("session options", e))?;
            b = b
                .with_execution_providers([ep::DirectML::default().build().error_on_failure()])
                .map_err(|e| model_err("could not use DirectML", e))?;
        }
        Some(_) => {
            b = b.with_execution_providers([ep::CUDA::default().build().error_on_failure()]).map_err(|e| model_err("could not use CUDA", e))?;
        }
        None => {}
    }
    b.commit_from_file(model).map_err(|e| model_err(&format!("could not open the model {}", model.display()), e))
}

pub struct HtDemucs {
    info: ModelInfo,
    session: Session,
    front: Frontend,
    wave_input: String,
    spec_input: String,
}

fn dims_of(o: &ort::value::Outlet) -> Vec<i64> {
    o.dtype().tensor_shape().map(|s| s.iter().copied().collect()).unwrap_or_default()
}

impl HtDemucs {
    pub fn load(opts: &LoadOptions) -> Result<HtDemucs, Error> {
        if !opts.model.is_file() {
            return Err(Error::Model(format!("the model file is missing: {}", opts.model.display())));
        }
        let env = environment(opts.runtime)?;
        let (session, device) = match opts.device {
            Device::Cpu => (build_session(&env, opts.model, None, opts.threads)?, "CPU".to_string()),
            Device::Gpu => {
                let gpus = gpu_providers();
                let first = gpus.first().ok_or_else(|| Error::Model("this onnxruntime has no GPU provider (DirectML or CUDA); choose CPU or install a build that has one".into()))?;
                (build_session(&env, opts.model, Some(first), opts.threads)?, format!("GPU ({first})"))
            }
            Device::Auto => {
                let mut why = String::new();
                let mut got = None;
                for g in gpu_providers() {
                    match build_session(&env, opts.model, Some(g), opts.threads) {
                        Ok(s) => {
                            got = Some((s, format!("GPU ({g})")));
                            break;
                        }
                        Err(e) => why = format!("{g}: {e}"),
                    }
                }
                match got {
                    Some(g) => g,
                    None if why.is_empty() => (build_session(&env, opts.model, None, opts.threads)?, "CPU (no GPU provider in this onnxruntime)".to_string()),
                    None => (build_session(&env, opts.model, None, opts.threads)?, format!("CPU (GPU failed: {why})")),
                }
            }
        };

        // find the waveform input (rank 3) and the spectrogram input (rank 4), by name first
        let inputs = session.inputs();
        let by_rank = |rank: usize| inputs.iter().find(|i| dims_of(i).len() == rank).map(|i| i.name().to_string());
        let wave_input = inputs.iter().find(|i| i.name() == "input").map(|i| i.name().to_string()).or_else(|| by_rank(3));
        let spec_input = inputs.iter().find(|i| i.name() == "x").map(|i| i.name().to_string()).or_else(|| by_rank(4));
        let (Some(wave_input), Some(spec_input)) = (wave_input, spec_input) else {
            return Err(Error::Model("the model does not have a waveform input and a spectrogram input; it is not the HTDemucs export this app expects".into()));
        };
        for o in inputs.iter() {
            let d = dims_of(o);
            let want: Vec<i64> = if o.name() == wave_input { vec![1, 2, SEGMENT as i64] } else { vec![1, 4, BINS as i64, demucs::frames_for(SEGMENT) as i64] };
            if d != want && d.iter().all(|x| *x > 0) {
                return Err(Error::Model(format!("input {} has shape {d:?}, expected {want:?}: this is not the 7.8 s HTDemucs export", o.name())));
            }
        }
        let n_out = session.outputs().len();
        if n_out != 1 && n_out != 2 {
            return Err(Error::Model(format!("the model has {n_out} outputs; expected 1 (waveform) or 2 (spectrogram and waveform)")));
        }

        let info = ModelInfo { id: "htdemucs-ort".into(), name: "HTDemucs (ONNX Runtime)".into(), sample_rate: crate::RATE, segment_len: SEGMENT, device };
        Ok(HtDemucs { info, session, front: Frontend::new(), wave_input, spec_input })
    }
}

/// Splits the `[1, 4, 2, N]` stems tensor into vocals and the sum of the other three.
fn split_stems(data: &[f32], n: usize) -> Result<SegmentStems, Error> {
    if data.len() != STEMS * 2 * n {
        return Err(Error::Model(format!("the model returned {} values, expected {}", data.len(), STEMS * 2 * n)));
    }
    let chan = |s: usize, c: usize| &data[(s * 2 + c) * n..(s * 2 + c + 1) * n];
    let vocals = [chan(VOCALS, 0).to_vec(), chan(VOCALS, 1).to_vec()];
    let mut rest = [vec![0.0f32; n], vec![0.0f32; n]];
    for s in (0..STEMS).filter(|s| *s != VOCALS) {
        for (c, out) in rest.iter_mut().enumerate() {
            for (o, v) in out.iter_mut().zip(chan(s, c)) {
                *o += v;
            }
        }
    }
    Ok(SegmentStems { vocals, rest })
}

impl StemModel for HtDemucs {
    fn info(&self) -> &ModelInfo {
        &self.info
    }

    fn separate(&mut self, seg: &Stereo) -> Result<SegmentStems, Error> {
        let n = SEGMENT;
        if seg[0].len() != n || seg[1].len() != n {
            return Err(Error::Invalid(format!("HTDemucs takes {n} frames per call")));
        }
        let zl = self.front.spec(&seg[0])?;
        let zr = self.front.spec(&seg[1])?;
        let frames = demucs::frames_for(n);
        let x = demucs::pack_cac(&zl, &zr);
        let mut wave = Vec::with_capacity(2 * n);
        wave.extend_from_slice(&seg[0]);
        wave.extend_from_slice(&seg[1]);

        let t_wave = Tensor::from_array(([1usize, 2, n], wave)).map_err(|e| model_err("input tensor", e))?;
        let t_spec = Tensor::from_array(([1usize, 4, BINS, frames], x)).map_err(|e| model_err("input tensor", e))?;
        let outputs = self
            .session
            .run(ort::inputs![self.wave_input.as_str() => t_wave, self.spec_input.as_str() => t_spec])
            .map_err(|e| model_err("the model failed to run", e))?;

        if outputs.len() == 1 {
            let (_, data) = outputs[0].try_extract_tensor::<f32>().map_err(|e| model_err("output tensor", e))?;
            return split_stems(data, n);
        }

        // two outputs: the spectrogram branch (5-D) and the waveform branch (4-D), in either order
        let (mut spec, mut time) = (None, None);
        for i in 0..outputs.len() {
            let (shape, data) = outputs[i].try_extract_tensor::<f32>().map_err(|e| model_err("output tensor", e))?;
            if shape.len() == 5 {
                spec = Some(data.to_vec());
            } else {
                time = Some(data.to_vec());
            }
        }
        let (Some(spec), Some(time)) = (spec, time) else {
            return Err(Error::Model("the two model outputs are not a spectrogram and a waveform".into()));
        };
        let plane = 4 * BINS * frames;
        if spec.len() != STEMS * plane || time.len() != STEMS * 2 * n {
            return Err(Error::Model("the model outputs have unexpected sizes".into()));
        }
        let mut sum = time;
        for s in 0..STEMS {
            let (l, r) = demucs::unpack_cac(&spec[s * plane..(s + 1) * plane])?;
            for (c, z) in [l, r].iter().enumerate() {
                let y = self.front.ispec(z, n)?;
                for (o, v) in sum[(s * 2 + c) * n..(s * 2 + c + 1) * n].iter_mut().zip(&y) {
                    *o += v;
                }
            }
        }
        split_stems(&sum, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_vocals_from_the_other_three_stems() {
        let n = 3;
        // stem s, channel c holds the constant 10*s + c
        let mut data = Vec::new();
        for s in 0..STEMS {
            for c in 0..2 {
                data.extend(std::iter::repeat_n((10 * s + c) as f32, n));
            }
        }
        let out = split_stems(&data, n).unwrap();
        assert_eq!(out.vocals[0], vec![30.0; n]);
        assert_eq!(out.vocals[1], vec![31.0; n]);
        assert_eq!(out.rest[0], vec![(10 + 20) as f32; n]);
        assert_eq!(out.rest[1], vec![(1 + 11 + 21) as f32; n]);
        assert!(split_stems(&data[1..], n).is_err());
    }

    #[test]
    fn a_missing_runtime_or_model_is_a_clear_error() {
        let dir = std::env::temp_dir().join(format!("pm-stems-ort-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join("m.ort");
        let rt = dir.join("onnxruntime.dll");
        let opts = LoadOptions { runtime: &rt, model: &model, device: Device::Cpu, threads: 0 };
        let e = HtDemucs::load(&opts).err().unwrap().to_string();
        assert!(e.contains("model file is missing"), "{e}");
        std::fs::write(&model, b"x").unwrap();
        let e = HtDemucs::load(&opts).err().unwrap().to_string();
        assert!(e.contains("was not found"), "{e}");
        // a file that is not a shared library
        std::fs::write(&rt, b"not a library").unwrap();
        let e = HtDemucs::load(&opts).err().unwrap().to_string();
        assert!(e.contains("could not load"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
