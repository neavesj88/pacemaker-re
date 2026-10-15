//! Score the analyser over a folder of tracks.
//!
//!   pm-bench <tracks dir> [--editor-str <dir>] [--csv <file>] [--write-str <dir>]
//!
//! References, when available:
//! - the file's BPM tag (Beatport-quality tags are a good tempo reference);
//! - the original Editor's beat grid, from the `str/` cache it writes under
//!   `%APPDATA%\Tonium\Pacemaker` after analysing the same files. Its `.str` files are found by
//!   the Editor's own naming scheme (pm_formats::naming).
//!
//! Beat agreement is the F-measure with a +-70 ms window, the usual MIREX tolerance.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pm_formats::{naming, strfile};

const AUDIO_EXT: [&str; 7] = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff"];
const TOL: f64 = 0.070;

struct Args {
    dir: PathBuf,
    editor_str: Option<PathBuf>,
    csv: Option<PathBuf>,
    write_str: Option<PathBuf>,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args { dir: PathBuf::new(), editor_str: None, csv: None, write_str: None };
    while let Some(s) = it.next() {
        match s.as_str() {
            "--editor-str" => a.editor_str = it.next().map(PathBuf::from),
            "--csv" => a.csv = it.next().map(PathBuf::from),
            "--write-str" => a.write_str = it.next().map(PathBuf::from),
            _ => a.dir = PathBuf::from(s),
        }
    }
    if a.dir.as_os_str().is_empty() {
        eprintln!("usage: pm-bench <tracks dir> [--editor-str <dir>] [--csv <file>] [--write-str <dir>]");
        std::process::exit(2);
    }
    a
}

fn audio_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| AUDIO_EXT.contains(&x.to_ascii_lowercase().as_str()))
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The Editor's cached analysis for `audio`, located by its naming scheme: `A/B/C.<hash64>.str`.
fn editor_analysis(str_dir: &Path, audio: &Path) -> Option<strfile::Analysis> {
    let size = std::fs::metadata(audio).ok()?.len();
    let stem = audio.file_stem()?.to_str()?;
    let enc = naming::encoded_name(stem, size);
    let (ab, c) = enc.rsplit_once('/')?;
    let dir = str_dir.join(ab);
    let hit = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("{c}.")) && n.ends_with(".str"))
    })?;
    strfile::decode(&std::fs::read_to_string(hit).ok()?).ok()
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

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
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

fn main() {
    let a = args();
    let files = audio_files(&a.dir);
    let mut csv = a.csv.as_ref().map(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut f = std::fs::File::create(p).expect("create csv");
        writeln!(f, "file,folder,tag_bpm,our_bpm,editor_bpm,vs_tag,vs_editor,f_measure,offset_ms,locked,snapped,residual_ms,secs").unwrap();
        f
    });
    println!(
        "{:<44} {:>7} {:>8} {:>8} {:>6} {:>7} {:>5} {:>5}",
        "track", "tag", "ours", "editor", "F", "off ms", "grid", "sec"
    );
    let (mut n_tag, mut n_tag_exact, mut n_ed, mut f_sum, mut n_ed_exact) = (0, 0, 0, 0.0, 0);
    for f in &files {
        let rel = f.strip_prefix(&a.dir).unwrap_or(f);
        let folder = rel.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let t0 = Instant::now();
        let r = match pm_analysis::analyze_file(f) {
            Ok(r) => r,
            Err(e) => {
                println!("{:<44} decode error: {e}", short(rel));
                continue;
            }
        };
        let secs = t0.elapsed().as_secs_f64();
        let sr = r.sample_rate as f64;
        // compare in the device convention, which is what gets exported
        let ours: Vec<f64> = r.device_beats().iter().map(|&p| p as f64 / sr).collect();
        let ed = a.editor_str.as_deref().and_then(|d| editor_analysis(d, f));
        let (ed_bpm, fm, off) = match &ed {
            Some(e) => {
                let refs: Vec<f64> = e.beats.iter().map(|b| b.pos as f64 / sr).collect();
                let (fm, off) = f_measure(&ours, &refs);
                (Some(median(e.beats.iter().map(|b| b.bpm).collect())), Some(fm), Some(off))
            }
            None => (None, None, None),
        };
        let vs_tag = r.tag_bpm.map(|t| tempo_class(r.bpm, t));
        let vs_ed = ed_bpm.map(|e| tempo_class(r.bpm, e));
        if let Some(c) = vs_tag {
            n_tag += 1;
            n_tag_exact += (c == "exact") as usize;
        }
        if let (Some(c), Some(fmv)) = (vs_ed, fm) {
            n_ed += 1;
            n_ed_exact += (c == "exact") as usize;
            f_sum += fmv;
        }
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
                "\"{}\",\"{}\",{},{:.4},{},{},{},{},{},{},{},{:.2},{:.2}",
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
                secs
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
    println!("tracks: {}", files.len());
    if n_tag > 0 {
        println!("tempo vs file tag: {n_tag_exact}/{n_tag} within 0.5 BPM");
    }
    if n_ed > 0 {
        println!(
            "vs original Editor: tempo {n_ed_exact}/{n_ed} within 0.5 BPM, mean beat F-measure {:.3}",
            f_sum / n_ed as f64
        );
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
