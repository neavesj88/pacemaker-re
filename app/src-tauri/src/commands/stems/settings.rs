//! Stem separation settings and the record of licence consents. App-side only, one file in the
//! app config folder (`stems-settings.json`): nothing here is about a particular card, and
//! nothing is written to a device.
//!
//! Every policy choice lives here with a default: which device to compute on, the output format
//! and quality, how segments overlap, how the instrumental is made, the length limit, the file
//! name suffixes, what to do about files that already exist.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pm_stems::pipeline::InstrumentalMode;
use pm_stems::sink::Format;
use serde::{Deserialize, Serialize};

pub const FILE: &str = "stems-settings.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Consent {
    pub notice_version: u32,
    /// Unix seconds when the user accepted.
    pub accepted_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct StemSettings {
    /// "auto" (a GPU provider when there is one that works, else CPU), "cpu" or "gpu" (fails when none).
    pub device: String,
    /// CPU threads for the model; 0 lets onnxruntime choose.
    pub threads: u32,
    /// "mp3", "ogg" or "wav".
    pub format: String,
    pub mp3_kbps: u16,
    /// Drop LAME's 1105-sample delay so an MP3 stem lines up with its source (see pm-stems sink).
    pub mp3_align: bool,
    /// Vorbis quality, -0.1 to 1.0.
    pub ogg_quality: f32,
    /// Share of a segment that overlaps the next one, 10 to 90 percent.
    pub overlap_percent: u32,
    /// "sum" (drums + bass + other) or "minus" (original minus vocals).
    pub instrumental: String,
    /// Tracks longer than this are not separated (the decoded track is held in memory); 0 = no limit.
    pub max_minutes: u32,
    pub vocals_suffix: String,
    pub instrumental_suffix: String,
    /// What to do when an output file already exists: "skip" or "replace" (replace is asked about by name).
    pub existing: String,
    /// A specific onnxruntime library to use instead of the downloaded / system one ("" = none).
    pub runtime_path: String,
    pub last_folder: String,
    /// Consent per downloadable component id.
    pub consents: BTreeMap<String, Consent>,
}

impl Default for StemSettings {
    fn default() -> Self {
        StemSettings {
            device: "auto".into(),
            threads: 0,
            format: "mp3".into(),
            mp3_kbps: 320,
            mp3_align: true,
            ogg_quality: 0.6,
            overlap_percent: 50,
            instrumental: "sum".into(),
            max_minutes: 30,
            vocals_suffix: "(Acapella)".into(),
            instrumental_suffix: "(Instrumental)".into(),
            existing: "skip".into(),
            runtime_path: String::new(),
            last_folder: String::new(),
            consents: BTreeMap::new(),
        }
    }
}

const BAD_NAME_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

fn check_suffix(label: &str, s: &str) -> Result<(), String> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > 40 {
        return Err(format!("{label} must be 1 to 40 characters."));
    }
    if t.chars().any(|c| c.is_control() || BAD_NAME_CHARS.contains(&c)) {
        return Err(format!("{label} cannot contain any of \\ / : * ? \" < > | or control characters."));
    }
    Ok(())
}

impl StemSettings {
    /// Checks every field and trims the text ones. The caller's consents are not trusted: the
    /// stored ones are kept by [`save`] regardless of what a client sends.
    pub fn validated(mut self) -> Result<StemSettings, String> {
        let one_of = |label: &str, v: &str, allowed: &[&str]| -> Result<(), String> {
            if allowed.contains(&v) { Ok(()) } else { Err(format!("{label} must be one of {}.", allowed.join(", "))) }
        };
        one_of("Device", &self.device, &["auto", "cpu", "gpu"])?;
        one_of("Format", &self.format, &["mp3", "ogg", "wav"])?;
        one_of("Instrumental", &self.instrumental, &["sum", "minus"])?;
        one_of("Existing files", &self.existing, &["skip", "replace"])?;
        if ![128u16, 160, 192, 224, 256, 320].contains(&self.mp3_kbps) {
            return Err("MP3 bitrate must be 128, 160, 192, 224, 256 or 320 kbps.".into());
        }
        if !(-0.1..=1.0).contains(&self.ogg_quality) || self.ogg_quality.is_nan() {
            return Err("Ogg quality must be between -0.1 and 1.0.".into());
        }
        if !(10..=90).contains(&self.overlap_percent) {
            return Err("Overlap must be between 10 and 90 percent.".into());
        }
        if self.threads > 64 {
            return Err("Threads must be 64 or fewer (0 = automatic).".into());
        }
        if self.max_minutes > 600 {
            return Err("The length limit must be 600 minutes or fewer (0 = no limit).".into());
        }
        self.vocals_suffix = self.vocals_suffix.trim().to_string();
        self.instrumental_suffix = self.instrumental_suffix.trim().to_string();
        check_suffix("The vocals suffix", &self.vocals_suffix)?;
        check_suffix("The instrumental suffix", &self.instrumental_suffix)?;
        if self.vocals_suffix.eq_ignore_ascii_case(&self.instrumental_suffix) {
            return Err("The two suffixes must differ, or both results would get the same file name.".into());
        }
        self.runtime_path = self.runtime_path.trim().to_string();
        self.last_folder = self.last_folder.trim().to_string();
        Ok(self)
    }

    /// The output format as the writers take it.
    pub fn out_format(&self) -> Format {
        match self.format.as_str() {
            "ogg" => Format::Ogg { quality: self.ogg_quality },
            "wav" => Format::Wav,
            _ => Format::Mp3 { kbps: self.mp3_kbps, align: self.mp3_align },
        }
    }

    pub fn instrumental_mode(&self) -> InstrumentalMode {
        if self.instrumental == "minus" { InstrumentalMode::MixMinusVocals } else { InstrumentalMode::SumOfStems }
    }

    pub fn overlap(&self) -> f32 {
        self.overlap_percent as f32 / 100.0
    }

    /// Whether the user has accepted the CURRENT notice of this component.
    pub fn consented(&self, id: &str, notice_version: u32) -> bool {
        self.consents.get(id).is_some_and(|c| c.notice_version == notice_version)
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Reads the settings. A missing file is the defaults; an unreadable one is also the defaults,
/// with a warning to show (the file is left alone until the next save).
pub fn load(dir: &Path) -> (StemSettings, Option<String>) {
    let p = path(dir);
    match std::fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<StemSettings>(&text) {
            Ok(s) => match s.clone().validated() {
                Ok(v) => (StemSettings { consents: s.consents, ..v }, None),
                Err(e) => (StemSettings { consents: s.consents, ..StemSettings::default() }, Some(format!("{} has an invalid value ({e}); defaults are used until you save.", p.display()))),
            },
            Err(e) => (StemSettings::default(), Some(format!("{} is not readable ({e}); defaults are used until you save.", p.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (StemSettings::default(), None),
        Err(e) => (StemSettings::default(), Some(format!("{}: {e}; defaults are used.", p.display()))),
    }
}

fn write(dir: &Path, s: &StemSettings) -> Result<(), String> {
    crate::store::write_json(&path(dir), s)
}

/// Saves settings from the UI. Consents are kept from the stored file: they change only through
/// [`record_consent`], never because a client sent them.
pub fn save(dir: &Path, incoming: StemSettings) -> Result<StemSettings, String> {
    let (current, _) = load(dir);
    let mut next = incoming.validated()?;
    next.consents = current.consents;
    write(dir, &next)?;
    Ok(next)
}

/// Remembers that the user accepted notice `notice_version` of component `id` now.
pub fn record_consent(dir: &Path, id: &str, notice_version: u32, now_secs: u64) -> Result<StemSettings, String> {
    let (mut s, _) = load(dir);
    s.consents.insert(id.to_string(), Consent { notice_version, accepted_at: now_secs });
    write(dir, &s)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pmr-stems-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_are_valid_and_match_the_documented_choices() {
        let d = StemSettings::default();
        assert_eq!(d.clone().validated().unwrap(), d);
        assert_eq!((d.format.as_str(), d.mp3_kbps, d.overlap_percent, d.existing.as_str()), ("mp3", 320, 50, "skip"));
        assert_eq!(d.out_format(), Format::Mp3 { kbps: 320, align: true });
        assert_eq!(d.instrumental_mode(), InstrumentalMode::SumOfStems);
        assert!((d.overlap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bad_values_are_refused_with_a_reason() {
        let bad = |f: fn(&mut StemSettings)| {
            let mut s = StemSettings::default();
            f(&mut s);
            s.validated().unwrap_err()
        };
        assert!(bad(|s| s.device = "tpu".into()).contains("Device"));
        assert!(bad(|s| s.format = "flac".into()).contains("Format"));
        assert!(bad(|s| s.mp3_kbps = 100).contains("MP3"));
        assert!(bad(|s| s.ogg_quality = 2.0).contains("Ogg"));
        assert!(bad(|s| s.ogg_quality = f32::NAN).contains("Ogg"));
        assert!(bad(|s| s.overlap_percent = 5).contains("Overlap"));
        assert!(bad(|s| s.threads = 500).contains("Threads"));
        assert!(bad(|s| s.max_minutes = 9999).contains("limit"));
        assert!(bad(|s| s.vocals_suffix = "  ".into()).contains("vocals suffix"));
        assert!(bad(|s| s.instrumental_suffix = "a/b".into()).contains("instrumental suffix"));
        assert!(bad(|s| s.vocals_suffix = "x".repeat(41)).contains("1 to 40"));
        assert!(bad(|s| s.instrumental_suffix = "(acapella)".into()).contains("must differ"));
        assert!(bad(|s| s.existing = "ask".into()).contains("Existing"));
    }

    #[test]
    fn save_load_round_trip_and_consents_are_not_client_controlled() {
        let dir = temp_dir("rt");
        let (s, warn) = load(&dir);
        assert!(warn.is_none() && s == StemSettings::default(), "no file is the defaults");

        record_consent(&dir, "htdemucs-ort", 1, 1_700_000_000).unwrap();
        let mut edit = StemSettings { format: "ogg".into(), overlap_percent: 25, vocals_suffix: "  (Vocals) ".into(), ..Default::default() };
        // a client that tries to forge or erase consent
        edit.consents.insert("onnxruntime-win-x64".into(), Consent { notice_version: 1, accepted_at: 5 });
        let saved = save(&dir, edit).unwrap();
        assert_eq!(saved.vocals_suffix, "(Vocals)", "text is trimmed");
        assert!(saved.consented("htdemucs-ort", 1));
        assert!(!saved.consented("onnxruntime-win-x64", 1), "a sent consent is ignored");
        let (back, warn) = load(&dir);
        assert!(warn.is_none());
        assert_eq!(back, saved);
        assert_eq!(back.out_format(), Format::Ogg { quality: 0.6 });

        // a newer notice invalidates the old consent
        assert!(!back.consented("htdemucs-ort", 2));
        assert!(save(&dir, StemSettings { overlap_percent: 99, ..Default::default() }).is_err());
        assert_eq!(load(&dir).0, saved, "a refused save changes nothing");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_corrupt_file_gives_defaults_and_a_warning_but_is_not_overwritten() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path(&dir), "not json").unwrap();
        let (s, warn) = load(&dir);
        assert_eq!(s, StemSettings::default());
        assert!(warn.unwrap().contains("not readable"));
        assert_eq!(std::fs::read_to_string(path(&dir)).unwrap(), "not json");

        // valid JSON with an out-of-range value keeps the consents it holds
        std::fs::write(path(&dir), r#"{"overlap_percent": 3, "consents": {"htdemucs-ort": {"notice_version": 1, "accepted_at": 9}}}"#).unwrap();
        let (s, warn) = load(&dir);
        assert!(warn.unwrap().contains("invalid value"));
        assert!(s.consented("htdemucs-ort", 1));
        assert_eq!(s.overlap_percent, 50);

        // unknown / missing fields are fine
        std::fs::write(path(&dir), r#"{"format": "wav", "future_field": 1}"#).unwrap();
        let (s, warn) = load(&dir);
        assert!(warn.is_none() && s.format == "wav");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
