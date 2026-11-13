//! Which energy a track "has": the rating the user set, the estimate made from the audio, or the
//! first of the two that exists. The one rule every consumer shares (Tracks search, smart-case
//! rules, the gap map, the set builder, the printable setlist). The UI keeps a mirror of
//! `resolve_level` in `app/src/lib/state/energyResolve.ts`; both are run against the vectors in
//! `app/src/lib/state/energy.parity.json`, so a smart case rewritten on the card and the same rule
//! previewed in the window cannot disagree.
//!
//! The choice is the app's own preference (one JSON file in the app config dir, the same for
//! every library). A missing file is the default; an unreadable or damaged one is also the
//! default, with a warning for the UI, and the damaged file is kept as `.bad` when it is written
//! over.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergySource {
    /// Only the user's own 1-10 rating.
    Mine,
    /// Only the estimate made from the audio.
    Auto,
    /// The user's rating, else the estimate.
    #[default]
    MineElseAuto,
}

/// An energy estimate (1.0 to 10.0) as a whole level 1 to 10; `None` for one that is not a
/// usable number or rounds below 1.
pub fn auto_level(auto: f32) -> Option<u8> {
    if !auto.is_finite() {
        return None;
    }
    let r = auto.round();
    (r >= 1.0).then(|| r.min(10.0) as u8)
}

/// The whole-level energy of a track under `source`.
pub fn resolve_level(mine: Option<u8>, auto: Option<f32>, source: EnergySource) -> Option<u8> {
    let auto = auto.and_then(auto_level);
    match source {
        EnergySource::Mine => mine,
        EnergySource::Auto => auto,
        EnergySource::MineElseAuto => mine.or(auto),
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join("energy-source.json")
}

#[derive(Serialize, Deserialize)]
struct File {
    source: EnergySource,
}

/// The saved choice and, when the file could not be used, a sentence saying so.
pub fn load(dir: &Path) -> (EnergySource, Option<String>) {
    let p = path(dir);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (EnergySource::default(), None),
        Err(e) => return (EnergySource::default(), Some(format!("{} could not be read ({e}); the default energy source is in use.", p.display()))),
    };
    match serde_json::from_str::<File>(&text) {
        Ok(f) => (f.source, None),
        Err(_) => (EnergySource::default(), Some(format!("{} could not be understood; the default energy source is in use.", p.display()))),
    }
}

/// Writes the choice (temp file, then rename). A file that could not be read is copied to
/// `energy-source.json.bad` first, once.
pub fn save(dir: &Path, source: EnergySource) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = path(dir);
    if p.exists() && load(dir).1.is_some() {
        let bad = p.with_extension("json.bad");
        if !bad.exists() {
            std::fs::copy(&p, &bad).map_err(|e| e.to_string())?;
        }
    }
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&File { source }).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-energysrc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[derive(Deserialize)]
    struct Vector {
        name: String,
        mine: Option<u8>,
        auto: Option<f32>,
        source: EnergySource,
        level: Option<u8>,
    }

    #[test]
    fn parity_vectors_match_the_ts_mirror() {
        let v: Vec<Vector> = serde_json::from_str(include_str!("../../../../src/lib/state/energy.parity.json")).unwrap();
        assert!(v.len() >= 12);
        for c in v {
            assert_eq!(resolve_level(c.mine, c.auto, c.source), c.level, "{}", c.name);
        }
    }

    #[test]
    fn auto_levels_round_and_clamp() {
        assert_eq!(auto_level(7.4), Some(7));
        assert_eq!(auto_level(7.5), Some(8));
        assert_eq!(auto_level(0.4), None);
        assert_eq!(auto_level(0.5), Some(1));
        assert_eq!(auto_level(99.0), Some(10));
        assert_eq!(auto_level(f32::NAN), None);
        assert_eq!(auto_level(f32::INFINITY), None);
    }

    #[test]
    fn the_choice_round_trips_and_defaults() {
        let dir = temp_dir("rt");
        assert_eq!(load(&dir), (EnergySource::MineElseAuto, None), "no file = the default");
        save(&dir, EnergySource::Auto).unwrap();
        assert_eq!(load(&dir), (EnergySource::Auto, None));
        assert!(std::fs::read_to_string(path(&dir)).unwrap().contains("\"auto\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_is_the_default_with_a_warning_and_is_kept() {
        let dir = temp_dir("bad");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path(&dir), "{\"source\": \"sideways\"}").unwrap();
        let (s, warn) = load(&dir);
        assert_eq!(s, EnergySource::MineElseAuto);
        assert!(warn.is_some());
        save(&dir, EnergySource::Mine).unwrap();
        assert!(dir.join("energy-source.json.bad").exists());
        assert_eq!(load(&dir), (EnergySource::Mine, None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
