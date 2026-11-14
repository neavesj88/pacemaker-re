//! `pm`: command-line access to a Pacemaker library on a mounted volume.
//!
//!   pm volumes                     list mounted volumes holding a Pacemaker library
//!   pm ls <volume>                 list tracks, mixes and cases; check device compatibility
//!   pm add <volume> [--bpm-range 88-176] <audio>...
//!                                  analyse and add tracks (creates the library if missing);
//!                                  the BPM range is 68-136, 78-156 (the Editor), 88-176 (default) or 98-196
//!   pm health <volume>             JSON: library health, firmware, storage (--strict: exit 3 on problems)
//!   pm backup <volume> <folder>    JSON: back the library up into a new folder (--audio, --verify)
//!   pm verify <backup-folder>      JSON: check a backup against its manifest (exit 3 on problems)
//!   pm export-playlists <volume>   JSON: every case and mix with its tracks (--out <file>)
//!   pm tags <volume>               JSON: the app's tags, colour labels, energy (--config-dir <dir>)
//!   pm duplicates <volume>         JSON: copies by tags / size and length (--config-dir <dir>)
//!   pm sync --from A --to B        JSON: plan, and unless --plan-only the run, of the app's library sync (writes B)
//!
//! `<volume>` is the volume root (the folder that contains `.Pacemaker`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pm_formats::library::{find_volumes, Library};

mod json_cmds;
use json_cmds::Done;

fn main() -> ExitCode {
    // A closed stdout (e.g. `pm ls | head`) should end quietly, not panic in println!.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.payload().downcast_ref::<String>().map(String::as_str).unwrap_or("");
        if msg.contains("failed printing to stdout") {
            std::process::exit(0);
        }
        default_hook(info);
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result: json_cmds::Res<Done> = match args.first().map(String::as_str) {
        Some("volumes") => volumes().map(|()| Done::Clean),
        Some("ls") if args.len() == 2 => ls(Path::new(&args[1])).map(|()| Done::Clean),
        Some("add") if args.len() >= 3 => add(Path::new(&args[1]), &args[2..]).map(|()| Done::Clean),
        Some("health") => json_cmds::health(&args[1..]),
        Some("backup") => json_cmds::backup(&args[1..]),
        Some("verify") => json_cmds::verify(&args[1..]),
        Some("export-playlists") => json_cmds::export_playlists(&args[1..]),
        Some("tags") => json_cmds::tags(&args[1..]),
        Some("duplicates") => json_cmds::duplicates(&args[1..]),
        Some("sync") => json_cmds::sync(&args[1..]),
        _ => {
            eprintln!("{}", json_cmds::USAGE);
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(Done::Clean) => ExitCode::SUCCESS,
        Ok(Done::Problems) => ExitCode::from(3),
        Err(e) if e.downcast_ref::<json_cmds::Usage>().is_some() => {
            eprintln!("error: {e}\n{}", json_cmds::USAGE);
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Res = Result<(), Box<dyn std::error::Error>>;

fn volumes() -> Res {
    let v = find_volumes();
    if v.is_empty() {
        println!("no Pacemaker library found on mounted volumes");
    }
    for p in v {
        println!("{}", p.display());
    }
    Ok(())
}

fn mmss(secs: i64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn ls(volume: &Path) -> Res {
    let lib = Library::open_read_only(volume)?;
    let problems = lib.compatibility_problems()?;
    if problems.is_empty() {
        println!("music.db: device-compatible");
    } else {
        println!("music.db: NOT device-compatible:");
        for p in problems {
            println!("  - {p}");
        }
    }
    let tracks = lib.tracks()?;
    println!("\n{} tracks", tracks.len());
    for t in &tracks {
        println!(
            "  {:>4}  {:<36} {:<28} {:>7.2}  {:>5}  {}",
            t.track_id,
            t.title.chars().take(36).collect::<String>(),
            t.artist.chars().take(28).collect::<String>(),
            t.bpm,
            mmss(t.play_time_secs),
            if t.analysed { "" } else { "(not analysed)" }
        );
    }
    for (label, cols) in [("mixes", lib.mixes()?), ("cases", lib.cases()?)] {
        println!("\n{} {label}", cols.len());
        for c in cols {
            println!("  {:<40} {:>3} tracks  {:>6}", c.name, c.track_count, mmss(c.play_time_secs));
        }
    }
    Ok(())
}

/// `--bpm-range 88-176` (or just `88`) as a window the app offers; it is the first of the extra
/// arguments after the volume and is not an audio file.
fn split_bpm_range(args: &[String]) -> Result<(pm_analysis::tempo::BpmRange, &[String]), Box<dyn std::error::Error>> {
    let range = pm_analysis::tempo::BpmRange::PRESETS;
    if args.first().map(String::as_str) != Some("--bpm-range") {
        return Ok((pm_analysis::tempo::BpmRange::default(), args));
    }
    let value = args.get(1).ok_or("--bpm-range needs a value such as 88-176")?;
    let lo: f64 = value.split('-').next().unwrap_or("").parse().map_err(|_| format!("--bpm-range {value}: not a number"))?;
    let chosen = pm_analysis::tempo::BpmRange::from_lo(lo).ok_or_else(|| {
        let all: Vec<String> = range.iter().map(|r| format!("{}-{}", r.lo, r.hi())).collect();
        format!("--bpm-range {value}: choose one of {}", all.join(", "))
    })?;
    Ok((chosen, &args[2..]))
}

fn add(volume: &Path, args: &[String]) -> Res {
    let (range, files) = split_bpm_range(args)?;
    if files.is_empty() {
        return Err("no audio files given".into());
    }
    let mut lib = Library::create(volume)?;
    for f in files {
        let path = PathBuf::from(f);
        if let Some(id) = lib.existing_track(&path)? {
            println!("{:>4}  {:<50} already on the volume", id, path.file_name().unwrap().to_string_lossy());
            continue;
        }
        let d = match pm_analysis::decode::decode(&path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skip {}: {e}", path.display());
                continue;
            }
        };
        let r = pm_analysis::analyze_with(&d.mono, d.sample_rate, d.tag_bpm, range);
        let id = lib.add_track(&path, &d.meta, &r.to_str_analysis())?;
        println!("{:>4}  {:<50} {:7.2} bpm", id, path.file_name().unwrap().to_string_lossy(), r.bpm);
    }
    Ok(())
}
