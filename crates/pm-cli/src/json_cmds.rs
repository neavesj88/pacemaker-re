//! The JSON-output subcommands of `pm`: health, backup, verify and export-playlists. Each prints
//! one JSON document on stdout and nothing else there (errors go to stderr), so a script can pipe
//! it into `jq`. They call the same `pm_formats` functions the desktop app does, so a result here
//! is the result the app shows.
//!
//! Exit codes: 0 done, 1 an error (message on stderr), 2 wrong usage, 3 done but a check found
//! problems (only where the command says so). All of them only read the volume; `backup` writes
//! to the folder it is given, never to the volume.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use pm_formats::interop::{self, Scope};
use pm_formats::library::{Library, Track};
use pm_formats::ops_device::{self, VerifyReport};
use pm_formats::ops_fix;
use serde_json::{json, Value};

/// A command line that does not fit the usage; `main` prints the usage and exits 2.
#[derive(Debug)]
pub struct Usage(pub String);

impl fmt::Display for Usage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Usage {}

pub type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// What a command found: `Clean` exits 0; `Problems` exits 3.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Done {
    Clean,
    Problems,
}

pub const USAGE: &str = "\
usage:
  pm volumes | pm ls <volume> | pm add <volume> <audio>...
  pm health <volume> [--strict]
  pm backup <volume> <dest-folder> [--audio] [--verify]
  pm verify <backup-folder> [--problems-only]
  pm export-playlists <volume> [--out <file>]
JSON on stdout for health, backup, verify and export-playlists; errors on stderr.
exit: 0 ok, 1 error, 2 usage, 3 a check found problems (health --strict, backup --verify, verify)";

/// Arguments split into positional words, switches and `--name value` options. Anything starting
/// with `--` that the command does not list is a usage error, so a typo never runs silently.
struct Parsed {
    positional: Vec<String>,
    switches: Vec<String>,
    values: HashMap<String, String>,
}

fn parse(args: &[String], switches: &[&str], options: &[&str]) -> Result<Parsed, Usage> {
    let mut p = Parsed { positional: Vec::new(), switches: Vec::new(), values: HashMap::new() };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if switches.contains(&a.as_str()) {
            p.switches.push(a.clone());
        } else if options.contains(&a.as_str()) {
            let v = it.next().ok_or_else(|| Usage(format!("{a} needs a value")))?;
            p.values.insert(a.clone(), v.clone());
        } else if a.starts_with("--") {
            return Err(Usage(format!("unknown option {a}")));
        } else {
            p.positional.push(a.clone());
        }
    }
    Ok(p)
}

impl Parsed {
    fn has(&self, s: &str) -> bool {
        self.switches.iter().any(|x| x == s)
    }

    /// The positional words, exactly `n` of them.
    fn exactly(&self, n: usize, what: &str) -> Result<&[String], Usage> {
        if self.positional.len() == n {
            Ok(&self.positional)
        } else {
            Err(Usage(format!("expected {what}")))
        }
    }
}

fn print_json(v: &Value) -> Res<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

// ---- health ---------------------------------------------------------------------------------

/// Library health: compatibility problems, tracks without audio, files nothing uses, the
/// firmware the card reports (serial masked) and where the space goes. `problems` is true when
/// the database is not device-compatible or a track's audio is missing (unused files and
/// warnings are reported but are not problems).
pub fn health_json(volume: &Path) -> Res<(Value, bool)> {
    let h = ops_device::health(volume)?;
    let storage = ops_device::storage(volume);
    let problems = !h.compatibility_problems.is_empty() || !h.missing_audio.is_empty();
    let (storage_v, storage_err) = match storage {
        Ok(s) => (serde_json::to_value(s)?, Value::Null),
        Err(e) => (Value::Null, Value::String(e.to_string())),
    };
    Ok((
        json!({
            "volume": volume.display().to_string(),
            "device_compatible": h.compatibility_problems.is_empty(),
            "problems": problems,
            "firmware": ops_device::firmware_info(volume),
            "storage": storage_v,
            "storage_error": storage_err,
            "health": h,
        }),
        problems,
    ))
}

pub fn health(args: &[String]) -> Res<Done> {
    let p = parse(args, &["--strict"], &[])?;
    let v = p.exactly(1, "pm health <volume> [--strict]")?;
    let (doc, problems) = health_json(Path::new(&v[0]))?;
    print_json(&doc)?;
    Ok(if problems && p.has("--strict") { Done::Problems } else { Done::Clean })
}

// ---- backup and verify ----------------------------------------------------------------------

fn verify_value(rep: &VerifyReport, problems_only: bool) -> Res<Value> {
    let mut v = serde_json::to_value(rep)?;
    v["problems"] = json!(rep.problems());
    if problems_only {
        let bad: Vec<_> = rep.files.iter().filter(|f| f.status != ops_device::CheckStatus::Ok).collect();
        v["files"] = serde_json::to_value(bad)?;
    }
    Ok(v)
}

/// Checks a backup folder against its manifest, file by file.
pub fn verify_json(folder: &Path, problems_only: bool) -> Res<(Value, bool)> {
    let rep = ops_device::verify_manifest_detailed(folder, &mut |_, _, _| {}, &|| false)?;
    let problems = rep.problems() > 0;
    Ok((verify_value(&rep, problems_only)?, problems))
}

pub fn verify(args: &[String]) -> Res<Done> {
    let p = parse(args, &["--problems-only"], &[])?;
    let v = p.exactly(1, "pm verify <backup-folder> [--problems-only]")?;
    let (doc, problems) = verify_json(Path::new(&v[0]), p.has("--problems-only"))?;
    print_json(&doc)?;
    Ok(if problems { Done::Problems } else { Done::Clean })
}

/// A backup snapshot of the library (database, settings, system file, mix document, every beat
/// grid, and the audio when asked) into a new `pacemaker-backup-<UTC stamp>` folder inside
/// `dest`, every file read back and checksummed. With `verify`, the finished folder is then
/// checked against its manifest as `pm verify` would. The backup is built in a `.partial`
/// folder and renamed when complete, so a folder with the final name is always whole; a process
/// killed mid-copy leaves the `.partial` folder behind, which is safe to delete.
pub fn backup_json(volume: &Path, dest: &Path, audio: bool, verify: bool) -> Res<(Value, bool)> {
    let rep = ops_device::backup_snapshot(volume, dest, audio, &mut |_, _, _| {}, &|| false)?;
    let mut doc = json!({ "backup": rep });
    let mut problems = false;
    if verify {
        let v = ops_device::verify_manifest_detailed(Path::new(&rep.folder), &mut |_, _, _| {}, &|| false)?;
        problems = v.problems() > 0;
        doc["verify"] = verify_value(&v, true)?;
    }
    Ok((doc, problems))
}

pub fn backup(args: &[String]) -> Res<Done> {
    let p = parse(args, &["--audio", "--verify"], &[])?;
    let v = p.exactly(2, "pm backup <volume> <dest-folder> [--audio] [--verify]")?;
    let (doc, problems) = backup_json(Path::new(&v[0]), Path::new(&v[1]), p.has("--audio"), p.has("--verify"))?;
    print_json(&doc)?;
    Ok(if problems { Done::Problems } else { Done::Clean })
}

// ---- playlists ------------------------------------------------------------------------------

fn track_json(volume: &Path, t: &Track) -> Value {
    // the file is built only from a location that is plain names under Music; anything else is null
    let file = ops_fix::checked_track_paths(volume, &t.location).map(|(a, _)| a.display().to_string());
    json!({
        "track_id": t.track_id,
        "title": t.title,
        "artist": t.artist,
        "album": t.album,
        "genre": t.genre,
        "bpm": t.bpm,
        "key": t.key,
        "rating": t.rating,
        "duration_secs": t.play_time_secs,
        "location": t.location,
        "file": file,
    })
}

/// Every case and mix with its tracks in order (the same membership the app's M3U / CSV export
/// uses, from `interop::scope_tracks`: a mix keeps repeats and leaves out tracks since removed
/// from the library).
pub fn playlists_json(volume: &Path) -> Res<Value> {
    let lib = Library::open_read_only(volume)?;
    let mut cases = Vec::new();
    for c in lib.cases()? {
        let (name, tracks) = interop::scope_tracks(&lib, Scope::Case(c.id))?;
        cases.push(json!({
            "id": c.id, "name": name, "duration_secs": c.play_time_secs,
            "tracks": tracks.iter().map(|t| track_json(volume, t)).collect::<Vec<_>>(),
        }));
    }
    let mut mixes = Vec::new();
    for m in lib.mixes()? {
        let (name, tracks) = interop::scope_tracks(&lib, Scope::Mix(m.id))?;
        mixes.push(json!({
            "id": m.id, "name": name, "duration_secs": m.play_time_secs,
            "tracks": tracks.iter().map(|t| track_json(volume, t)).collect::<Vec<_>>(),
        }));
    }
    Ok(json!({ "volume": volume.display().to_string(), "cases": cases, "mixes": mixes }))
}

pub fn export_playlists(args: &[String]) -> Res<Done> {
    let p = parse(args, &[], &["--out"])?;
    let v = p.exactly(1, "pm export-playlists <volume> [--out <file>]")?;
    let doc = playlists_json(Path::new(&v[0]))?;
    match p.values.get("--out") {
        None => print_json(&doc)?,
        Some(out) => {
            // beside the destination and renamed, so a failed write never leaves half a file
            let tmp = format!("{out}.part");
            let text = serde_json::to_string_pretty(&doc)? + "\n";
            std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, out)).inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })?;
        }
    }
    Ok(Done::Clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_formats::library::TrackMeta;
    use pm_formats::strfile::{Analysis, Beat};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pm-cli-test-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A volume with `n` synthetic tracks, a case holding the first two (in reverse) and an
    /// empty mix.
    fn volume(tag: &str, n: usize) -> (PathBuf, Vec<i64>) {
        let vol = scratch(&format!("{tag}-vol"));
        let src = scratch(&format!("{tag}-src"));
        let analysis = Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 0,
            beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(),
            first_downbeat: Some(0),
        };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for i in 0..n {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![i as u8; 3000 + i]).unwrap();
            let meta = TrackMeta { title: format!("Title {i}"), artist: format!("Artist {i}"), format: "MP3".into(), play_time_secs: 100 + i as i64, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        lib.create_case_with_tracks("Demo case", &[ids[1], ids[0]]).unwrap();
        lib.create_mix("Demo mix").unwrap();
        (vol, ids)
    }

    #[test]
    fn health_reports_the_card_as_json() {
        let (vol, _) = volume("health", 2);
        let (doc, problems) = health_json(&vol).unwrap();
        assert!(!problems);
        assert_eq!(doc["device_compatible"], true);
        assert_eq!(doc["problems"], false);
        assert_eq!(doc["health"]["missing_audio"], json!([]));
        assert!(doc["storage"]["total_bytes"].is_u64(), "{doc}");
        assert!(doc["firmware"]["serial_masked"].is_null(), "no system.xml on a fresh library");
        // the document round-trips through text, as a script would read it
        let back: Value = serde_json::from_str(&serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        assert_eq!(back, doc);
        let _ = std::fs::remove_dir_all(vol);
    }

    #[test]
    fn health_flags_a_missing_audio_file_and_strict_makes_it_exit_3() {
        let (vol, _) = volume("missing", 2);
        // delete one audio file behind the database's back
        let music = vol.join(".Pacemaker").join("Music");
        let mut gone = None;
        for e in walk(&music) {
            if e.extension().is_some_and(|x| x == "mp3") {
                std::fs::remove_file(&e).unwrap();
                gone = Some(e);
                break;
            }
        }
        assert!(gone.is_some());
        let (doc, problems) = health_json(&vol).unwrap();
        assert!(problems);
        assert_eq!(doc["problems"], true);
        assert_eq!(doc["health"]["missing_audio"].as_array().unwrap().len(), 1);
        let args = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let v = vol.to_string_lossy().to_string();
        assert_eq!(health(&args(&[&v])).unwrap(), Done::Clean, "without --strict a finding is not a failure");
        assert_eq!(health(&args(&[&v, "--strict"])).unwrap(), Done::Problems);
        let _ = std::fs::remove_dir_all(vol);
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
        out
    }

    #[test]
    fn backup_then_verify_agree_and_a_changed_file_is_found() {
        let (vol, _) = volume("backup", 2);
        let dest = scratch("backup-dest");
        let (doc, problems) = backup_json(&vol, &dest, true, true).unwrap();
        assert!(!problems, "{doc}");
        assert_eq!(doc["backup"]["includes_audio"], true);
        assert!(doc["backup"]["files"].as_u64().unwrap() >= 5);
        assert_eq!(doc["verify"]["problems"], 0);
        assert_eq!(doc["verify"]["files"], json!([]), "--verify lists only problems");
        let folder = PathBuf::from(doc["backup"]["folder"].as_str().unwrap());
        assert!(folder.join("MANIFEST.sha256").is_file());
        assert!(!folder.to_string_lossy().ends_with(".partial"));

        let (v, problems) = verify_json(&folder, false).unwrap();
        assert!(!problems);
        assert_eq!(v["ok"], v["listed"]);
        assert!(!v["files"].as_array().unwrap().is_empty(), "the full list without --problems-only");

        // damage one backed-up file: verify names it, exits 3
        let victim = walk(&folder).into_iter().find(|p| p.extension().is_some_and(|x| x == "str")).unwrap();
        std::fs::write(&victim, "changed").unwrap();
        let (v, problems) = verify_json(&folder, true).unwrap();
        assert!(problems);
        assert_eq!(v["problems"], 1);
        assert_eq!(v["files"].as_array().unwrap().len(), 1);
        assert_eq!(v["files"][0]["status"], "differs");
        let args = vec![folder.to_string_lossy().to_string()];
        assert_eq!(verify(&args).unwrap(), Done::Problems);
        for d in [vol, dest] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn backup_without_audio_leaves_the_audio_out_and_refuses_a_destination_on_the_card() {
        let (vol, _) = volume("noaudio", 2);
        let dest = scratch("noaudio-dest");
        let (doc, _) = backup_json(&vol, &dest, false, false).unwrap();
        assert_eq!(doc["backup"]["includes_audio"], false);
        assert!(doc.get("verify").is_none());
        let folder = PathBuf::from(doc["backup"]["folder"].as_str().unwrap());
        assert!(walk(&folder).iter().all(|p| p.extension().is_none_or(|x| x != "mp3")));
        assert!(backup_json(&vol, &vol, false, false).is_err(), "a backup on the device itself is refused");
        assert!(backup_json(&vol, &dest.join("not-a-folder"), false, false).is_err());
        assert!(verify_json(&scratch("empty"), false).is_err(), "not a backup folder");
        for d in [vol, dest] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn playlists_list_cases_and_mixes_in_order_with_validated_file_paths() {
        let (vol, ids) = volume("playlists", 3);
        let doc = playlists_json(&vol).unwrap();
        let case = &doc["cases"][0];
        assert_eq!(case["name"], "Demo case");
        let order: Vec<i64> = case["tracks"].as_array().unwrap().iter().map(|t| t["track_id"].as_i64().unwrap()).collect();
        assert_eq!(order, vec![ids[1], ids[0]], "the case order, not the library order");
        let t = &case["tracks"][0];
        assert_eq!((t["title"].as_str(), t["artist"].as_str(), t["duration_secs"].as_i64()), (Some("Title 1"), Some("Artist 1"), Some(101)));
        let file = t["file"].as_str().unwrap();
        assert!(file.contains(".Pacemaker") && file.ends_with(".mp3") && std::path::Path::new(file).is_file(), "{file}");
        assert_eq!(doc["mixes"][0]["name"], "Demo mix");
        assert_eq!(doc["mixes"][0]["tracks"], json!([]));

        // a location that is not plain names under Music gets no file path
        let tr = Track { location: "/pmdb_tracks/../outside.mp3".into(), ..sample_track() };
        assert!(track_json(&vol, &tr)["file"].is_null());
        let tr = Track { location: "/elsewhere/x.mp3".into(), ..sample_track() };
        assert!(track_json(&vol, &tr)["file"].is_null());
        let _ = std::fs::remove_dir_all(vol);
    }

    fn sample_track() -> Track {
        Track {
            track_id: 1, title: "t".into(), artist: "a".into(), album: String::new(), genre: String::new(), bpm: 120.0, key: String::new(),
            play_time_secs: 1, rating: 0, location: String::new(), file_size: 1, format: "MP3".into(), analysed: true, times_played: 0, last_played: -1,
        }
    }

    #[test]
    fn export_playlists_writes_a_file_atomically() {
        let (vol, _) = volume("export", 2);
        let out = scratch("export-out").join("playlists.json");
        let args = vec![vol.to_string_lossy().to_string(), "--out".into(), out.to_string_lossy().to_string()];
        assert_eq!(export_playlists(&args).unwrap(), Done::Clean);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["cases"][0]["name"], "Demo case");
        assert!(!out.with_file_name("playlists.json.part").exists());
        // a destination that cannot be written: error, no stray .part
        let bad = out.parent().unwrap().join("no-such-dir").join("x.json");
        assert!(export_playlists(&[vol.to_string_lossy().to_string(), "--out".into(), bad.to_string_lossy().to_string()]).is_err());
        let _ = std::fs::remove_dir_all(vol);
    }

    #[test]
    fn bad_command_lines_are_usage_errors() {
        let a = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        for (f, args) in [
            (health as fn(&[String]) -> Res<Done>, a(&[])),
            (health, a(&["v", "w"])),
            (health, a(&["v", "--stict"])),
            (backup, a(&["v"])),
            (backup, a(&["v", "d", "--audio", "extra"])),
            (verify, a(&[])),
            (export_playlists, a(&["v", "--out"])),
        ] {
            let e = f(&args).unwrap_err();
            assert!(e.downcast_ref::<Usage>().is_some(), "{args:?}: {e}");
        }
    }
}
