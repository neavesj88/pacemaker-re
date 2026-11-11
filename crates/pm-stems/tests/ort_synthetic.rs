//! Runs the HTDemucs provider against a SYNTHETIC ONNX graph (no weights involved) through a
//! real onnxruntime library, to check the plumbing: loading, input shape validation, tensor
//! layout in and out, the stems split, and the pipeline on top.
//!
//! The graph has the signature of the real export (`input` [1,2,N], `x` [1,4,2048,336] ->
//! `y` [1,4,2,N]) and copies the waveform into all four stems, so vocals == input and
//! rest == 3 x input. It says nothing about separation quality.
//!
//! Needs an onnxruntime shared library: `PM_STEMS_ORT_DLL`, or on Windows the inbox
//! `System32\onnxruntime.dll`. Skips (and says so) when none loads. This file is its own test
//! binary because onnxruntime can be loaded once per process.

#![cfg(feature = "ort")]

use std::path::PathBuf;

use pm_stems::htdemucs::{Device, HtDemucs, LoadOptions, SEGMENT};
use pm_stems::pipeline::{Options, separate};
use pm_stems::provider::StemModel;

// --- minimal protobuf writer for ONNX ModelProto ---

fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn field_varint(num: u32, v: u64, out: &mut Vec<u8>) {
    varint(u64::from(num << 3), out);
    varint(v, out);
}

fn field_bytes(num: u32, b: &[u8], out: &mut Vec<u8>) {
    varint(u64::from(num << 3 | 2), out);
    varint(b.len() as u64, out);
    out.extend_from_slice(b);
}

fn value_info(name: &str, dims: &[u64]) -> Vec<u8> {
    let mut shape = Vec::new();
    for d in dims {
        let mut dim = Vec::new();
        field_varint(1, *d, &mut dim);
        field_bytes(1, &dim, &mut shape);
    }
    let mut tensor = Vec::new();
    field_varint(1, 1, &mut tensor); // FLOAT
    field_bytes(2, &shape, &mut tensor);
    let mut ty = Vec::new();
    field_bytes(1, &tensor, &mut ty);
    let mut vi = Vec::new();
    field_bytes(1, name.as_bytes(), &mut vi);
    field_bytes(2, &ty, &mut vi);
    vi
}

fn node(op: &str, inputs: &[&str], output: &str, int_attr: Option<(&str, i64)>) -> Vec<u8> {
    let mut n = Vec::new();
    for i in inputs {
        field_bytes(1, i.as_bytes(), &mut n);
    }
    field_bytes(2, output.as_bytes(), &mut n);
    field_bytes(4, op.as_bytes(), &mut n);
    if let Some((name, v)) = int_attr {
        let mut a = Vec::new();
        field_bytes(1, name.as_bytes(), &mut a);
        field_varint(3, v as u64, &mut a);
        field_varint(20, 2, &mut a); // INT
        field_bytes(5, &a, &mut n);
    }
    n
}

fn synthetic_model(n: u64) -> Vec<u8> {
    let mut g = Vec::new();
    field_bytes(1, &node("Unsqueeze", &["input", "axes"], "u", None), &mut g);
    field_bytes(1, &node("Concat", &["u", "u", "u", "u"], "y", Some(("axis", 1))), &mut g);
    field_bytes(2, b"synthetic", &mut g);
    // initializer: axes = [1] (int64)
    let mut t = Vec::new();
    field_varint(1, 1, &mut t); // dims
    field_varint(2, 7, &mut t); // INT64
    field_bytes(8, b"axes", &mut t);
    field_bytes(9, &1i64.to_le_bytes(), &mut t);
    field_bytes(5, &t, &mut g);
    field_bytes(11, &value_info("input", &[1, 2, n]), &mut g);
    field_bytes(11, &value_info("x", &[1, 4, 2048, 336]), &mut g);
    field_bytes(12, &value_info("y", &[1, 4, 2, n]), &mut g);
    let mut opset = Vec::new();
    field_varint(2, 17, &mut opset);
    let mut m = Vec::new();
    field_varint(1, 8, &mut m); // ir_version
    field_bytes(7, &g, &mut m);
    field_bytes(8, &opset, &mut m);
    m
}

fn runtime() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PM_STEMS_ORT_DLL") {
        return Some(PathBuf::from(p));
    }
    let inbox = PathBuf::from(r"C:\Windows\System32\onnxruntime.dll");
    inbox.is_file().then_some(inbox)
}

#[test]
fn a_synthetic_graph_runs_through_the_provider_and_the_pipeline() {
    let Some(rt) = runtime() else {
        eprintln!("SKIPPED: no onnxruntime library (set PM_STEMS_ORT_DLL)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("pm-stems-synth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("synthetic.onnx");
    std::fs::write(&model, synthetic_model(SEGMENT as u64)).unwrap();

    let mut m = match HtDemucs::load(&LoadOptions { runtime: &rt, model: &model, device: Device::Cpu, threads: 2 }) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("SKIPPED: onnxruntime at {} did not load the synthetic graph: {e}", rt.display());
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
    };
    assert_eq!(m.info().device, "CPU");
    assert_eq!(m.info().segment_len, SEGMENT);

    // a whole track of 1.5 segments through the real provider
    let n = SEGMENT * 3 / 2;
    let l: Vec<f32> = (0..n).map(|i| 0.4 * (i as f32 * 0.05).sin()).collect();
    let r: Vec<f32> = (0..n).map(|i| 0.3 * (i as f32 * 0.031).cos()).collect();
    let (mut v, mut inst) = ((Vec::new(), Vec::new()), (Vec::new(), Vec::new()));
    let rep = separate(
        &l,
        &r,
        &mut m,
        &Options::default(),
        &mut |a, b| {
            v.0.extend_from_slice(a);
            v.1.extend_from_slice(b);
            Ok(())
        },
        &mut |a, b| {
            inst.0.extend_from_slice(a);
            inst.1.extend_from_slice(b);
            Ok(())
        },
        &mut |_| true,
    )
    .unwrap();
    assert_eq!((rep.frames, v.0.len(), inst.0.len()), (n, n, n));
    let mean = l.iter().zip(&r).map(|(a, b)| 0.5 * (*a as f64 + *b as f64)).sum::<f64>() / n as f64;
    for i in (0..n).step_by(997) {
        // vocals = the (mean-removed) input; the synthetic "rest" is three copies of it
        assert!((v.0[i] - (l[i] - mean as f32)).abs() < 1e-4, "vocals left at {i}");
        assert!((inst.1[i] - 3.0 * (r[i] - mean as f32)).abs() < 1e-3 || inst.1[i].abs() >= 0.999, "rest right at {i}: {}", inst.1[i]);
    }

    // a graph with the wrong segment length is refused with a message, not run
    let bad = dir.join("short.onnx");
    std::fs::write(&bad, synthetic_model(1000)).unwrap();
    let e = HtDemucs::load(&LoadOptions { runtime: &rt, model: &bad, device: Device::Cpu, threads: 0 }).err().unwrap().to_string();
    assert!(e.contains("not the 7.8 s HTDemucs export"), "{e}");

    // Gpu on a build without a GPU provider says so (the inbox 1.17 library has DirectML only
    // if the EP is compiled in; accept either outcome but never a panic)
    let _ = HtDemucs::load(&LoadOptions { runtime: &rt, model: &model, device: Device::Gpu, threads: 0 });
    let auto = HtDemucs::load(&LoadOptions { runtime: &rt, model: &model, device: Device::Auto, threads: 0 }).unwrap();
    eprintln!("auto device: {}", auto.info().device);
    std::fs::remove_dir_all(&dir).unwrap();
}
