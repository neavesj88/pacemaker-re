//! Score the analyser over a folder of tracks.
//!
//!   pm-bench <tracks dir> [--editor-str <dir> | --sidecar] [--kick] [--csv <file>] [--write-str <dir>]
//!
//! References, when available:
//! - the file's BPM tag (Beatport-quality tags are a good tempo reference);
//! - the original Editor's beat grid, either from the `str/` cache it writes under
//!   `%APPDATA%\Tonium\Pacemaker` after analysing the same files (`--editor-str`, found by the
//!   Editor's own naming scheme, pm_formats::naming), or as a `.str` sidecar with the same stem
//!   next to each audio file, as on a device volume (`--sidecar`).
//!
//! Beat agreement is the F-measure with a +-70 ms window, the usual MIREX tolerance.
//! `--kick` adds an audio-based check of both grids on the same audio: where the kick detector
//! peaks relative to the grid lines (pm_analysis::align); |offset| <= 15 ms counts as on the kick.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pm_analysis::{align, decode, onset, DEVICE_OFFSET_SECS};
use pm_bench::{audio_files, median, EditorSource};
use pm_formats::strfile;

const TOL: f64 = 0.070;

struct Args {
    dir: PathBuf,
    editor: Option<EditorSource>,
    kick: bool,
    csv: Option<PathBuf>,
    write_str: Option<PathBuf>,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args { dir: PathBuf::new(), editor: None, kick: false, csv: None, write_str: None };
    while let Some(s) = it.next() {
        match s.as_str() {
            "--editor-str" => a.editor = it.next().map(|d| EditorSource::Dir(PathBuf::from(d))),
            "--sidecar" => a.editor = Some(EditorSource::Sidecar),
            "--kick" => a.kick = true,
            "--csv" => a.csv = it.next().map(PathBuf::from),
            "--write-str" => a.write_str = it.next().map(PathBuf::from),
            _ => a.dir = PathBuf::from(s),
        }
    }
    if a.dir.as_os_str().is_empty() {
        eprintln!("usage: pm-bench <tracks dir> [--editor-str <dir> | --sidecar] [--kick] [--csv <file>] [--write-str <dir>]");
        std::process::exit(2);
    }
    a
}

/// (F-measure, median signed offset est-ref in ms) with one-to-one matching inside TOL.
fn f_measure(est: &[f64], refs: &[f64]) -> (f64, f64) {
    if est.is_empty() || refs.is_empty() {
        return (0.0, f64::NAN);
    }
    let (mut i, mut j, mut hits) = (0, 0, 0usize);
    let mut offs = Vec::new();
    while i < est.len() && j < refs.len() {
        let d = est[i] - refs[j];
        if d.abs() <= TOL {
            hits += 1;
            offs.push(d * 1000.0);
            i += 1;
            j += 1;
        } else if d < 0.0 {
            i += 1;
        } else {
            j += 1;
        }
    }
    let p = hits as f64 / est.len() as f64;
    let r = hits as f64 / refs.len() as f64;
    let f = if p + r > 0.0 { 2.0 * p * r / (p + r) } else { 0.0 };
    offs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (f, offs.get(offs.len() / 2).copied().unwrap_or(f64::NAN))
}

/// Tempo match allowing for the classic half/double ambiguity: "exact" within 0.5 BPM,
/// "octave" if off by a factor of 2 (or 3/2), else "wrong".
fn tempo_class(est: f64, refb: f64) -> &'static str {
    if (est - refb).abs() <= 0.5 {
        "exact"
    } else if [2.0, 0.5, 1.5, 2.0 / 3.0].iter().any(|m| (est - refb * m).abs() <= 1.0) {
        "octave"
    } else {
        "wrong"
    }
}

/// Kick offset of our grid and of the Editor's, on the same audio. The Editor's beats are shifted
/// back by the device offset so both are on the audio's own timeline.
fn kick_pair(
    mono: &[f32],
    r: &pm_analysis::Result,
    ed: Option<(&strfile::Analysis, f64)>,
) -> (Option<align::KickOffset>, Option<align::KickOffset>) {
    let env = onset::envelope(mono, r.sample_rate);
    let ours = align::kick_offset(&env, &r.beats, r.bpm);
    let theirs = ed.and_then(|(e, bpm)| {
        let shift = (DEVICE_OFFSET_SECS * r.sample_rate as f64).round() as i64;
        let beats: Vec<i64> = e.beats.iter().map(|b| b.pos - shift).collect();
        align::kick_offset(&env, &beats, bpm)
    });
    (ours, theirs)
}

fn main() {
    let a = args();
    let files = audio_files(&a.dir);
    let mut csv = a.csv.as_ref().map(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut f = std::fs::File::create(p).expect("create csv");
        writeln!(f, "file,folder,tag_bpm,our_bpm,editor_bpm,vs_tag,vs_editor,f_measure,offset_ms,locked,snapped,residual_ms,secs,sample_rate,editor_bpm_spread,our_kick_ms,our_kick_contrast,editor_kick_ms,editor_kick_contrast").unwrap();
        f
    });
    println!(
        "{:<44} {:>7} {:>8} {:>8} {:>6} {:>7} {:>5} {:>5}",
        "track", "tag", "ours", "editor", "F", "off ms", "grid", "sec"
    );
    let (mut n_tag, mut n_tag_exact, mut n_ed, mut f_sum, mut n_ed_exact) = (0, 0, 0, 0.0, 0);
    let (mut n_decode_err, mut n_no_editor) = (0, 0);
    // tempo-agreeing tracks (within 0.5 BPM): offsets, and kick stats for both grids
    let mut agree_off = Vec::new();
    let mut disagree = Vec::new();
    let (mut kick_ours, mut kick_ed, mut kick_n) = (0, 0, 0);
    let (mut kick_ours_agree, mut kick_ed_agree, mut kick_n_agree) = (0, 0, 0);
    for f in &files {
        let rel = f.strip_prefix(&a.dir).unwrap_or(f);
        let folder = rel.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let t0 = Instant::now();
        let d = match decode::decode(f) {
            Ok(d) => d,
            Err(e) => {
                n_decode_err += 1;
                println!("{:<44} decode error: {e}", short(rel));
                continue;
            }
        };
        let r = pm_analysis::analyze(&d.mono, d.sample_rate, d.tag_bpm);
        let secs = t0.elapsed().as_secs_f64();
        let sr = r.sample_rate as f64;
        // compare in the device convention, which is what gets exported
        let ours: Vec<f64> = r.device_beats().iter().map(|&p| p as f64 / sr).collect();
        let ed = a.editor.as_ref().and_then(|src| src.analysis(f));
        if a.editor.is_some() && ed.is_none() {
            n_no_editor += 1;
            println!("{:<44} no usable Editor grid (missing, empty or undecodable .str)", short(rel));
        }
        let (ed_bpm, ed_spread, fm, off) = match &ed {
            Some(e) => {
                let refs: Vec<f64> = e.beats.iter().map(|b| b.pos as f64 / sr).collect();
                let (fm, off) = f_measure(&ours, &refs);
                let bpms: Vec<f64> = e.beats.iter().map(|b| b.bpm).collect();
                let spread = bpms.iter().copied().fold(f64::MIN, f64::max) - bpms.iter().copied().fold(f64::MAX, f64::min);
                (Some(median(bpms)), Some(spread), Some(fm), Some(off))
            }
            None => (None, None, None, None),
        };
        let vs_tag = r.tag_bpm.map(|t| tempo_class(r.bpm, t));
        let vs_ed = ed_bpm.map(|e| tempo_class(r.bpm, e));
        if let Some(c) = vs_tag {
            n_tag += 1;
            n_tag_exact += (c == "exact") as usize;
        }
        let agrees = vs_ed == Some("exact");
        if let (Some(c), Some(fmv), Some(eb)) = (vs_ed, fm, ed_bpm) {
            n_ed += 1;
            n_ed_exact += agrees as usize;
            f_sum += fmv;
            if agrees {
                if let Some(o) = off.filter(|o| o.is_finite()) {
                    agree_off.push(o);
                }
            } else {
                disagree.push((short(rel), r.bpm, eb, r.bpm / eb, c));
            }
        }
        let (kick_o, kick_e) = if a.kick {
            let k = kick_pair(&d.mono, &r, ed.as_ref().zip(ed_bpm));
            if let (Some(ko), Some(ke)) = k {
                kick_n += 1;
                kick_ours += ko.on_kick() as usize;
                kick_ed += ke.on_kick() as usize;
                if agrees {
                    kick_n_agree += 1;
                    kick_ours_agree += ko.on_kick() as usize;
                    kick_ed_agree += ke.on_kick() as usize;
                }
            }
            k
        } else {
            (None, None)
        };
        let opt = |v: Option<f64>, p: usize| v.map_or("-".to_string(), |x| format!("{x:.p$}"));
        let grid = if r.locked_grid { if r.snapped { "snap" } else { "lock" } } else { "var" };
        println!(
            "{:<44} {:>7} {:>8.3} {:>8} {:>6} {:>7} {:>5} {:>5.1}",
            short(rel),
            opt(r.tag_bpm, 1),
            r.bpm,
            opt(ed_bpm, 3),
            opt(fm, 3),
            opt(off, 1),
            grid,
            secs
        );
        if let Some(out) = csv.as_mut() {
            writeln!(
                out,
                "\"{}\",\"{}\",{},{:.4},{},{},{},{},{},{},{},{:.2},{:.2},{},{},{},{},{},{}",
                rel.display(),
                folder,
                opt(r.tag_bpm, 2),
                r.bpm,
                opt(ed_bpm, 4),
                vs_tag.unwrap_or("-"),
                vs_ed.unwrap_or("-"),
                opt(fm, 4),
                opt(off, 2),
                r.locked_grid,
                r.snapped,
                r.residual_ms,
                secs,
                r.sample_rate,
                opt(ed_spread, 4),
                opt(kick_o.map(|k| k.offset_ms), 0),
                opt(kick_o.map(|k| k.contrast), 2),
                opt(kick_e.map(|k| k.offset_ms), 0),
                opt(kick_e.map(|k| k.contrast), 2),
            )
            .unwrap();
        }
        if let Some(dir) = &a.write_str {
            let p = dir.join(rel).with_extension("str");
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(p, strfile::encode(&r.to_str_analysis(), strfile::Newline::Crlf)).unwrap();
        }
    }
    println!();
    println!("tracks: {} ({n_decode_err} decode errors)", files.len());
    if n_tag > 0 {
        println!("tempo vs file tag: {n_tag_exact}/{n_tag} within 0.5 BPM");
    }
    if a.editor.is_some() {
        println!("no usable Editor grid: {n_no_editor}");
    }
    if n_ed > 0 {
        println!(
            "vs original Editor: tempo {n_ed_exact}/{n_ed} within 0.5 BPM, mean beat F-measure {:.3}",
            f_sum / n_ed as f64
        );
        println!(
            "median offset (ours - Editor, ms) on tempo-agreeing tracks: {:.1} (n={})",
            median(agree_off.clone()),
            agree_off.len()
        );
        for (name, ours, eb, ratio, class) in &disagree {
            println!("tempo disagreement: {name} ours {ours:.3} editor {eb:.3} ratio {ratio:.3} ({class})");
        }
    }
    if kick_n > 0 {
        let ms = align::ON_KICK_MS;
        println!("kick within {ms} ms of grid, ours:   {kick_ours}/{kick_n} (tempo-agreeing: {kick_ours_agree}/{kick_n_agree})");
        println!("kick within {ms} ms of grid, Editor: {kick_ed}/{kick_n} (tempo-agreeing: {kick_ed_agree}/{kick_n_agree})");
    }
}

fn short(p: &Path) -> String {
    let s = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if s.chars().count() > 44 {
        format!("{}…", s.chars().take(43).collect::<String>())
    } else {
        s
    }
}
