//! `pm`: command-line access to a Pacemaker library on a mounted volume.
//!
//!   pm volumes                     list mounted volumes holding a Pacemaker library
//!   pm ls <volume>                 list tracks, mixes and cases; check device compatibility
//!   pm add <volume> <audio>...     analyse and add tracks (creates the library if missing)
//!
//! `<volume>` is the volume root (the folder that contains `.Pacemaker`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pm_formats::library::{find_volumes, Library};

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
    let result = match args.first().map(String::as_str) {
        Some("volumes") => volumes(),
        Some("ls") if args.len() == 2 => ls(Path::new(&args[1])),
        Some("add") if args.len() >= 3 => add(Path::new(&args[1]), &args[2..]),
        _ => {
            eprintln!("usage: pm volumes | pm ls <volume> | pm add <volume> <audio>...");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
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

fn add(volume: &Path, files: &[String]) -> Res {
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
        let r = pm_analysis::analyze(&d.mono, d.sample_rate, d.tag_bpm);
        let id = lib.add_track(&path, &d.meta, &r.to_str_analysis())?;
        println!("{:>4}  {:<50} {:7.2} bpm", id, path.file_name().unwrap().to_string_lossy(), r.bpm);
    }
    Ok(())
}
