//! The JSON-output subcommands of `pm`: health, backup, verify, export-playlists, tags and
//! duplicates. Each prints
//! one JSON document on stdout and nothing else there (errors go to stderr), so a script can pipe
//! it into `jq`. They call the same `pm_formats` functions the desktop app does, so a result here
//! is the result the app shows.
//!
//! Exit codes: 0 done, 1 an error (message on stderr), 2 wrong usage, 3 done but a check found
//! problems (only where the command says so). All of them only read the volume; `backup` writes
//! to the folder it is given, never to the volume. `tags` and `duplicates` run the same
//! `pm_formats` code as the app's Tags and Duplicates features and read the app's own files (the
//! label store, the duplicate settings) from a config folder; they never change them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::Path;

use pm_formats::appfile;
use pm_formats::interop::{self, Scope};
use pm_formats::library::{DupRow, Library, Track};
use pm_formats::ops_dupes::{self, DupSettings};
use pm_formats::ops_labels::{self, Kind};
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
  pm volumes | pm ls <volume> | pm add <volume> [--bpm-range 92-184] <audio>...
  pm health <volume> [--strict]
  pm backup <volume> <dest-folder> [--audio] [--verify]
  pm verify <backup-folder> [--problems-only]
  pm export-playlists <volume> [--out <file>]
  pm tags <volume> --config-dir <dir> [--tag <name>] [--tracks] [--strict]
  pm duplicates <volume> [--config-dir <dir>] [--by tags|size] [--strict]
  pm sync --from <library> --to <library> [--plan-only] [--remove] [--config-dir <dir>] [--exclude <key>]... [--json]
JSON on stdout for health, backup, verify, export-playlists, tags, duplicates and sync; errors on stderr.
--config-dir is the app's config folder (the one holding labels-*.json and duplicate-settings.json).
exit: 0 ok, 1 error, 2 usage, 3 a check found problems (health --strict, backup --verify, verify,
      tags --strict when the label file cannot be used, duplicates --strict when copies are found,
      sync when an item failed)";

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
            let text = serde_json::to_string_pretty(&doc)? + "\n";
            appfile::write_file(Path::new(out), text.as_bytes())?;
        }
    }
    Ok(Done::Clean)
}

// ---- tags -----------------------------------------------------------------------------------

/// The app's tags, colour labels and energy ratings for the tracks on `volume`: how many tracks
/// carry each tag and colour, and (with `list_tracks` or a `tag`) which tracks. Read-only: the
/// label file in `config_dir` is read, never written or set aside. The second value is true when
/// the file could not be used (damaged, unreadable or written by a newer app), in which case the
/// counts are of what could be read and `notice` says why.
pub fn tags_json(volume: &Path, config_dir: &Path, tag: Option<&str>, list_tracks: bool) -> Res<(Value, bool)> {
    let lib = Library::open_read_only(volume)?;
    let idents = ops_labels::build_keys(&lib.track_identities()?);
    let volume_key = appfile::volume_key(volume);
    let loaded = ops_labels::load(config_dir, &volume_key);
    let by_id: HashMap<i64, Track> = lib.tracks()?.into_iter().map(|t| (t.track_id, t)).collect();
    let want = match tag {
        Some(t) => Some(ops_labels::tag_key(&ops_labels::normalise_tag(t).map_err(Usage)?)),
        None => None,
    };
    let listing = list_tracks || want.is_some();

    let mut tag_counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut colours: BTreeMap<&str, usize> = ops_labels::COLOURS.iter().map(|c| (*c, 0)).collect();
    let mut energy: BTreeMap<u8, usize> = BTreeMap::new();
    let mut rows = Vec::new();
    let mut labelled = 0usize;
    for (id, key) in &idents {
        let Some(l) = loaded.store.get(Kind::Track, key) else { continue };
        labelled += 1;
        for t in &l.tags {
            tag_counts.entry(ops_labels::tag_key(t)).or_insert_with(|| (t.clone(), 0)).1 += 1;
        }
        if let Some(c) = l.colour.as_deref().and_then(|c| colours.get_mut(c)) {
            *c += 1;
        }
        if let Some(e) = l.energy {
            *energy.entry(e).or_default() += 1;
        }
        let matches = want.as_ref().map_or(true, |w| l.tags.iter().any(|t| &ops_labels::tag_key(t) == w));
        if listing && matches {
            let t = by_id.get(id);
            rows.push(json!({
                "track_id": id,
                "title": t.map(|t| t.title.as_str()).unwrap_or(""),
                "artist": t.map(|t| t.artist.as_str()).unwrap_or(""),
                "tags": l.tags,
                "colour": l.colour,
                "energy": l.energy,
            }));
        }
    }
    let live: HashSet<&str> = idents.iter().map(|(_, k)| k.as_str()).collect();
    let orphans = loaded.store.unattached(Kind::Track, &live).len();
    let mut tags: Vec<(String, usize)> = tag_counts.into_values().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    let mut doc = json!({
        "volume": volume.display().to_string(),
        "label_file": ops_labels::store_path(config_dir, &volume_key).display().to_string(),
        "problems": loaded.notice.is_some(),
        "notice": loaded.notice,
        "read_only": loaded.read_only.is_some(),
        "tracks": by_id.len(),
        "tracks_labelled": labelled,
        "tags": tags.iter().map(|(t, n)| json!({ "tag": t, "tracks": n })).collect::<Vec<_>>(),
        "colours": colours,
        "energy": energy,
        "labels_without_a_track": orphans,
    });
    if listing {
        doc["track_labels"] = Value::Array(rows);
    }
    Ok((doc, loaded.notice.is_some()))
}

pub fn tags(args: &[String]) -> Res<Done> {
    let p = parse(args, &["--tracks", "--strict"], &["--config-dir", "--tag"])?;
    let v = p.exactly(1, "pm tags <volume> --config-dir <dir> [--tag <name>] [--tracks] [--strict]")?;
    let dir = p.values.get("--config-dir").ok_or_else(|| Usage("pm tags needs --config-dir <the app's config folder>".into()))?;
    let (doc, problems) = tags_json(Path::new(&v[0]), Path::new(dir), p.values.get("--tag").map(String::as_str), p.has("--tracks"))?;
    print_json(&doc)?;
    Ok(if problems && p.has("--strict") { Done::Problems } else { Done::Clean })
}

// ---- duplicates -----------------------------------------------------------------------------

/// The ways of finding copies that need no audio decoding (tags, size and length), with the
/// finder's settings (`duplicate-settings.json` in `config_dir` when given, else the defaults) and
/// the copy it would suggest keeping. The sound check is not run here: it needs the app's
/// fingerprint job. `by` limits the run to one way. Groups are in the order the app lists them
/// (suggested copy's artist, title, then group id). The second value is true when copies were found.
pub fn duplicates_json(volume: &Path, config_dir: Option<&Path>, by: Option<&str>) -> Res<(Value, bool)> {
    let (mut s, settings_warning) = match config_dir {
        Some(d) => ops_dupes::load(d),
        None => (DupSettings::default(), None),
    };
    match by {
        None => {}
        Some("tags") => (s.by_tags, s.by_size) = (true, false),
        Some("size") => (s.by_tags, s.by_size) = (false, true),
        Some(other) => return Err(Usage(format!("--by takes tags or size, not {other}")).into()),
    }
    let lib = Library::open_read_only(volume)?;
    let all = lib.dup_rows()?;
    let rows: Vec<&DupRow> = all.iter().collect();
    let (mut links, mut warnings) = (Vec::new(), Vec::new());
    if s.by_tags {
        let (l, unusable) = ops_dupes::tag_links(&rows, &s);
        links.extend(l);
        if unusable > 0 {
            warnings.push(format!("{unusable} track(s) had no artist or no title, so the tag check could not compare them."));
        }
    }
    if s.by_size {
        links.extend(ops_dupes::size_links(&rows, &s));
    }
    let by_id: HashMap<i64, &DupRow> = all.iter().map(|r| (r.track_id, r)).collect();
    // the file is looked for only at a location that is plain names under Music
    let file_state = |id: i64| match ops_fix::checked_track_paths(volume, &by_id[&id].location) {
        None => "outside",
        Some((audio, _)) if audio.is_file() => "present",
        Some(_) => "missing",
    };
    let mut groups: Vec<((String, String, i64), Value)> = ops_dupes::group(links)
        .into_iter()
        .map(|g| {
            let members: Vec<&DupRow> = g.ids.iter().map(|id| by_id[id]).collect();
            let keeper = ops_dupes::pick_keeper(&members, s.keeper_rule, &|id| file_state(id) == "present");
            let mut modes: Vec<ops_dupes::Mode> = g.links.iter().map(|l| l.mode).collect();
            modes.sort();
            modes.dedup();
            let order = ops_dupes::display_order(members.iter().copied().find(|r| r.track_id == keeper), g.ids[0]);
            let doc = json!({
                "id": g.ids[0],
                "suggested_keeper": keeper,
                "modes": modes,
                "members": members.iter().map(|r| json!({
                    "track_id": r.track_id, "title": r.title, "artist": r.artist,
                    "duration_secs": r.play_time_secs, "bit_rate_kbps": r.bit_rate_kbps,
                    "file_size": r.file_size, "times_played": r.times_played,
                    "location": r.location, "file": file_state(r.track_id),
                })).collect::<Vec<_>>(),
                "links": g.links.iter().map(|l| json!({ "a": l.a, "b": l.b, "mode": l.mode })).collect::<Vec<_>>(),
            });
            (order, doc)
        })
        .collect();
    // the order the app lists them in
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    let groups: Vec<Value> = groups.into_iter().map(|(_, g)| g).collect();
    let found = !groups.is_empty();
    Ok((
        json!({
            "volume": volume.display().to_string(),
            "tracks": all.len(),
            "settings": s,
            "settings_warning": settings_warning,
            "sound_check": "not run by pm; the app's Duplicates view runs it",
            "warnings": warnings,
            "groups": groups,
        }),
        found,
    ))
}

pub fn duplicates(args: &[String]) -> Res<Done> {
    let p = parse(args, &["--strict"], &["--config-dir", "--by"])?;
    let v = p.exactly(1, "pm duplicates <volume> [--config-dir <dir>] [--by tags|size] [--strict]")?;
    let dir = p.values.get("--config-dir").map(Path::new);
    let (doc, found) = duplicates_json(Path::new(&v[0]), dir, p.values.get("--by").map(String::as_str))?;
    print_json(&doc)?;
    Ok(if found && p.has("--strict") { Done::Problems } else { Done::Clean })
}

// ---- sync -----------------------------------------------------------------------------------

/// `pm sync --from <library> --to <library>`: the plan, and unless `--plan-only` the run, of the
/// same engine the app's sync uses (`pm_formats::ops_sync`). It WRITES the target (the other
/// commands here only read); it takes no snapshot of the target's music.db (the app does), and
/// does not move the app's tags and labels. `--config-dir` reads the app's `sync.json` for the
/// choices (defaults without it); the "remove what the other side lacks" choice is ignored there and
/// is on only with `--remove`, because this command runs the plan at once (list it first with
/// `--plan-only --remove`). `--exclude <key>` leaves an item out, as unticking it does.
/// `--json` is accepted for symmetry: the output is always JSON. Exit 3 when an item failed.
pub fn sync(args: &[String]) -> Res<Done> {
    use pm_formats::ops_sync::{self, SyncSettings};
    let mut excluded: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--exclude" {
            excluded.push(it.next().ok_or_else(|| Usage("--exclude needs a value".into()))?.clone());
        } else {
            rest.push(a.clone());
        }
    }
    let q = parse(&rest, &["--plan-only", "--json", "--remove"], &["--from", "--to", "--config-dir"])?;
    if !q.positional.is_empty() {
        return Err(Usage("expected pm sync --from <library> --to <library>".into()).into());
    }
    let from = q.values.get("--from").ok_or_else(|| Usage("--from <library> is required".into()))?;
    let to = q.values.get("--to").ok_or_else(|| Usage("--to <library> is required".into()))?;
    let (mut settings, mut warning) = (SyncSettings::default(), None);
    if let Some(dir) = q.values.get("--config-dir") {
        let file = Path::new(dir).join("sync.json");
        match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<SyncSettings>(&text) {
                Ok(s) => settings = s,
                Err(e) => warning = Some(format!("{} could not be read ({e}); the defaults are in use", file.display())),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => warning = Some(format!("{}: {e}; the defaults are in use", file.display())),
        }
    }
    // a removal is never taken from a settings file: it needs --remove on this command line
    settings.remove_missing = q.has("--remove");
    // the same text rule as the app's library kind, so the labels agree for every spelling of a root
    let label = if pm_formats::library::is_volume_root(from) { "Pacemaker" } else { "computer" };
    let source = Library::open_read_only(Path::new(from))?;
    let mut target_lib = if q.has("--plan-only") { Library::open_read_only(Path::new(to))? } else { Library::open(Path::new(to))? };
    let plan = ops_sync::plan(&source, &target_lib, &settings, label)?;
    let report = if q.has("--plan-only") { None } else { Some(ops_sync::apply(&source, &mut target_lib, &plan, &excluded, &mut |_| {}, &|| false)?) };
    let failed = report.as_ref().is_some_and(|r| r.failed > 0);
    print_json(&json!({
        "from": from, "to": to, "plan_only": report.is_none(), "settings_warning": warning,
        "plan": plan, "report": report,
    }))?;
    Ok(if failed { Done::Problems } else { Done::Clean })
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
            year: 0, label: String::new(), remixer: String::new(), composer: String::new(), comments: String::new(), date_added: -1, bit_rate: 0,
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

    /// A volume whose tracks are given as `(title, artist, seconds, bytes)`.
    fn volume_of(tag: &str, tracks: &[(&str, &str, i64, usize)]) -> (PathBuf, Vec<i64>) {
        let vol = scratch(&format!("{tag}-vol"));
        let src = scratch(&format!("{tag}-src"));
        let analysis = Analysis { normalize_factor: 1.0, beat_start_offset: 0, beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(), first_downbeat: Some(0) };
        let mut lib = Library::create(&vol).unwrap();
        let mut ids = Vec::new();
        for (i, (title, artist, secs, bytes)) in tracks.iter().enumerate() {
            let f = src.join(format!("synthetic {i}.mp3"));
            std::fs::write(&f, vec![i as u8; *bytes]).unwrap();
            let meta = TrackMeta { title: title.to_string(), artist: artist.to_string(), format: "MP3".into(), play_time_secs: *secs, ..Default::default() };
            ids.push(lib.add_track(&f, &meta, &analysis).unwrap());
        }
        (vol, ids)
    }

    fn label(vol: &Path, cfg: &Path, ids: &[i64], ops: &[ops_labels::Op]) {
        let lib = Library::open_read_only(vol).unwrap();
        let keys = ops_labels::build_keys(&lib.track_identities().unwrap());
        let keys: Vec<String> = keys.into_iter().filter(|(id, _)| ids.contains(id)).map(|(_, k)| k).collect();
        let key = appfile::volume_key(vol);
        let mut loaded = ops_labels::load(cfg, &key);
        loaded.store.apply(Kind::Track, &keys, ops).unwrap();
        ops_labels::save(cfg, &key, &loaded.store, None).unwrap();
    }

    #[test]
    fn tags_count_what_the_app_stored_and_filter_by_tag_ignoring_case() {
        let (vol, ids) = volume_of("tags", &[("Title A", "Artist A", 100, 3000), ("Title B", "Artist B", 110, 3001), ("Title C", "Artist C", 120, 3002)]);
        let cfg = scratch("tags-cfg");
        label(&vol, &cfg, &ids[..2], &[ops_labels::Op::AddTags { tags: vec!["Warm-up".into()] }, ops_labels::Op::SetColour { colour: Some("red".into()) }]);
        label(&vol, &cfg, &ids[1..2], &[ops_labels::Op::AddTags { tags: vec!["peak".into()] }, ops_labels::Op::SetEnergy { energy: Some(8) }]);

        let (doc, problems) = tags_json(&vol, &cfg, None, false).unwrap();
        assert!(!problems);
        assert_eq!((doc["tracks"].as_u64(), doc["tracks_labelled"].as_u64(), doc["labels_without_a_track"].as_u64()), (Some(3), Some(2), Some(0)));
        assert_eq!(doc["tags"], json!([{ "tag": "Warm-up", "tracks": 2 }, { "tag": "peak", "tracks": 1 }]));
        assert_eq!(doc["colours"]["red"], 2);
        assert_eq!(doc["colours"]["blue"], 0, "every colour is listed, so the shape never changes");
        assert_eq!(doc["energy"], json!({ "8": 1 }));
        assert!(doc.get("track_labels").is_none(), "tracks are listed only when asked for");

        let (doc, _) = tags_json(&vol, &cfg, Some("PEAK"), false).unwrap();
        let rows = doc["track_labels"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0]["track_id"].as_i64(), rows[0]["title"].as_str(), rows[0]["energy"].as_u64()), (Some(ids[1]), Some("Title B"), Some(8)));
        let (doc, _) = tags_json(&vol, &cfg, None, true).unwrap();
        assert_eq!(doc["track_labels"].as_array().unwrap().len(), 2);
        assert!(tags_json(&vol, &cfg, Some("   "), false).unwrap_err().downcast_ref::<Usage>().is_some(), "an empty tag is a usage error, not 'every track'");
        let _ = std::fs::remove_dir_all(vol);
    }

    #[test]
    fn tags_with_no_label_file_are_an_empty_report_and_a_damaged_file_is_reported_not_changed() {
        let (vol, _) = volume_of("tags-bad", &[("Title A", "Artist A", 100, 3000)]);
        let cfg = scratch("tags-bad-cfg");
        let (doc, problems) = tags_json(&vol, &cfg, None, false).unwrap();
        assert!(!problems && doc["tracks_labelled"] == 0 && doc["notice"].is_null());

        let file = ops_labels::store_path(&cfg, &appfile::volume_key(&vol));
        std::fs::write(&file, "{ not json").unwrap();
        let (doc, problems) = tags_json(&vol, &cfg, None, false).unwrap();
        assert!(problems && doc["problems"] == true && doc["notice"].is_string());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json", "pm only reads: the file is neither replaced nor set aside");
        assert_eq!(std::fs::read_dir(&cfg).unwrap().count(), 1);

        let args = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let v = vol.to_string_lossy().to_string();
        let c = cfg.to_string_lossy().to_string();
        assert_eq!(tags(&args(&[&v, "--config-dir", &c])).unwrap(), Done::Clean);
        assert_eq!(tags(&args(&[&v, "--config-dir", &c, "--strict"])).unwrap(), Done::Problems);
        let _ = std::fs::remove_dir_all(vol);
    }

    #[test]
    fn duplicates_find_copies_by_tags_and_by_size_and_suggest_the_one_with_a_file() {
        let (vol, ids) = volume_of(
            "dups",
            &[
                ("Same Title", "Same Artist", 200, 5000),
                ("same title (feat. Guest)", "SAME ARTIST", 202, 5001), // tags match after normalising
                ("Other One", "Aaa Artist", 300, 7000),
                ("Different Name", "Aab Artist", 300, 7000), // same size and length as the one above
                ("Unrelated", "Nobody", 90, 1234),
            ],
        );
        let (doc, found) = duplicates_json(&vol, None, None).unwrap();
        assert!(found);
        assert_eq!(doc["tracks"], 5);
        assert_eq!(doc["settings_warning"], Value::Null);
        let groups = doc["groups"].as_array().unwrap();
        let ids_of = |g: &Value| g["members"].as_array().unwrap().iter().map(|m| m["track_id"].as_i64().unwrap()).collect::<Vec<_>>();
        // listed as the app lists them: by the suggested copy's artist, so the size group
        // ("Aaa/Aab Artist") comes before the tag group ("Same Artist") although its ids are higher
        assert_eq!(groups.iter().map(ids_of).collect::<Vec<_>>(), vec![vec![ids[2], ids[3]], vec![ids[0], ids[1]]]);
        assert_eq!(groups[0]["modes"], json!(["size"]));
        assert_eq!(groups[1]["modes"], json!(["tags"]));
        assert!(groups[1]["members"][0]["file"] == "present");

        // one way at a time
        assert_eq!(duplicates_json(&vol, None, Some("tags")).unwrap().0["groups"].as_array().unwrap().len(), 1);
        assert_eq!(duplicates_json(&vol, None, Some("size")).unwrap().0["groups"][0]["modes"], json!(["size"]));
        assert!(duplicates_json(&vol, None, Some("sound")).unwrap_err().downcast_ref::<Usage>().is_some());

        // the suggested keeper never has its file missing while another copy has one
        let music = vol.join(".Pacemaker").join("Music");
        let keeper = groups[1]["suggested_keeper"].as_i64().unwrap();
        let lib = Library::open_read_only(&vol).unwrap();
        let loc = lib.tracks().unwrap().into_iter().find(|t| t.track_id == keeper).unwrap().location;
        let (audio, _) = ops_fix::checked_track_paths(&vol, &loc).unwrap();
        assert!(audio.starts_with(&music));
        std::fs::remove_file(&audio).unwrap();
        let (doc, _) = duplicates_json(&vol, None, Some("tags")).unwrap();
        let g = &doc["groups"][0];
        assert_ne!(g["suggested_keeper"].as_i64().unwrap(), keeper);
        assert!(g["members"].as_array().unwrap().iter().any(|m| m["track_id"].as_i64() == Some(keeper) && m["file"] == "missing"));
        let _ = std::fs::remove_dir_all(vol);
    }

    #[test]
    fn duplicates_use_the_apps_saved_settings_and_strict_exits_3_only_when_copies_exist() {
        let (vol, _) = volume_of("dups-cfg", &[("Same Title", "Same Artist", 200, 5000), ("Same Title", "Same Artist", 300, 5001), ("Alone", "Solo", 50, 10)]);
        let cfg = scratch("dups-cfg-dir");
        let args = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let v = vol.to_string_lossy().to_string();
        let c = cfg.to_string_lossy().to_string();
        // 100 s apart: not copies under the default 10 s tolerance
        assert!(!duplicates_json(&vol, Some(&cfg), None).unwrap().1);
        assert_eq!(duplicates(&args(&[&v, "--config-dir", &c, "--strict"])).unwrap(), Done::Clean);
        // the app's setting "length check off" (0) makes them copies; pm reads the app's file
        ops_dupes::save(&cfg, &DupSettings { tag_duration_secs: 0, ..Default::default() }).unwrap();
        let (doc, found) = duplicates_json(&vol, Some(&cfg), None).unwrap();
        assert!(found && doc["settings"]["tag_duration_secs"] == 0);
        assert_eq!(duplicates(&args(&[&v, "--config-dir", &c])).unwrap(), Done::Clean);
        assert_eq!(duplicates(&args(&[&v, "--config-dir", &c, "--strict"])).unwrap(), Done::Problems);
        // an unreadable settings file is the defaults plus a warning, and is left alone
        std::fs::write(cfg.join("duplicate-settings.json"), "nope").unwrap();
        let (doc, found) = duplicates_json(&vol, Some(&cfg), None).unwrap();
        assert!(!found && doc["settings_warning"].is_string());
        assert_eq!(std::fs::read_to_string(cfg.join("duplicate-settings.json")).unwrap(), "nope");
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
            (tags, a(&["v"])),
            (tags, a(&["v", "--config-dir"])),
            (duplicates, a(&["v", "--by"])),
            (duplicates, a(&["v", "--bogus"])),
        ] {
            let e = f(&args).unwrap_err();
            assert!(e.downcast_ref::<Usage>().is_some(), "{args:?}: {e}");
        }
    }

    #[test]
    fn sync_plans_and_then_applies_between_two_libraries() {
        let (from, _) = volume("sync-from", 3);
        let to = scratch("sync-to");
        Library::create(&to).unwrap();
        let (f, t) = (from.to_string_lossy().to_string(), to.to_string_lossy().to_string());
        let a = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // plan only: the target is untouched
        assert_eq!(sync(&a(&["--from", &f, "--to", &t, "--plan-only", "--json"])).unwrap(), Done::Clean);
        assert!(Library::open_read_only(&to).unwrap().tracks().unwrap().is_empty());
        // the run copies the three tracks and the case; the empty mix has nothing to need and copies too
        assert_eq!(sync(&a(&["--from", &f, "--to", &t])).unwrap(), Done::Clean);
        let lib = Library::open_read_only(&to).unwrap();
        assert_eq!(lib.tracks().unwrap().len(), 3);
        assert_eq!((lib.cases().unwrap().len(), lib.mixes().unwrap().len()), (1, 1));
        // removals only with --remove, even when a settings file asks for them
        let cfg = scratch("sync-cfg");
        std::fs::write(cfg.join("sync.json"), r#"{"remove_missing": true}"#).unwrap();
        let c = cfg.to_string_lossy().to_string();
        let mut extra = Library::open(&to).unwrap();
        let src_dir = scratch("sync-extra");
        let file = src_dir.join("only there.mp3");
        std::fs::write(&file, vec![9u8; 3100]).unwrap();
        let analysis = Analysis { normalize_factor: 1.0, beat_start_offset: 0, beats: (0..8).map(|i| Beat { pos: i * 20000, bpm: 128.0 }).collect(), first_downbeat: Some(0) };
        extra.add_track(&file, &TrackMeta { title: "Only there".into(), artist: "X".into(), format: "MP3".into(), play_time_secs: 90, ..Default::default() }, &analysis).unwrap();
        drop(extra);
        assert_eq!(sync(&a(&["--from", &f, "--to", &t, "--config-dir", &c])).unwrap(), Done::Clean);
        assert_eq!(Library::open_read_only(&to).unwrap().tracks().unwrap().len(), 4, "no --remove, nothing removed");
        assert_eq!(sync(&a(&["--from", &f, "--to", &t, "--config-dir", &c, "--remove"])).unwrap(), Done::Clean);
        assert_eq!(Library::open_read_only(&to).unwrap().tracks().unwrap().len(), 3, "--remove removes the unlisted track");
        let _ = std::fs::remove_dir_all(cfg);
        let _ = std::fs::remove_dir_all(src_dir);
        // a second run has nothing to do
        assert_eq!(sync(&a(&["--from", &f, "--to", &t])).unwrap(), Done::Clean);
        assert_eq!(Library::open_read_only(&to).unwrap().tracks().unwrap().len(), 3);
        // usage errors
        for bad in [a(&["--from", &f]), a(&["--to", &t]), a(&["--from", &f, "--to", &t, "extra"]), a(&["--from", &f, "--to", &t, "--bogus"]), a(&["--from", &f, "--to", &t, "--exclude"])] {
            let e = sync(&bad).unwrap_err();
            assert!(e.downcast_ref::<Usage>().is_some(), "{bad:?}: {e}");
        }
        let _ = std::fs::remove_dir_all(from);
        let _ = std::fs::remove_dir_all(to);
    }
}
