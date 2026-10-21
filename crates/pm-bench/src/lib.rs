//! Helpers shared by the bench binaries: finding audio files and the original Editor's analysis.

use std::path::{Path, PathBuf};

use pm_formats::{naming, strfile};

const AUDIO_EXT: [&str; 7] = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff"];

/// Where the original Editor's `.str` for an audio file lives.
#[derive(Debug, Clone)]
pub enum EditorSource {
    /// The Editor's own `str/` cache (`%APPDATA%\Tonium\Pacemaker\str`), found by its naming scheme.
    Dir(PathBuf),
    /// Next to the audio file with the same stem, as on a device volume (`Music/A/B/<hash>.str`).
    Sidecar,
}

impl EditorSource {
    /// The Editor's analysis of `audio`; `None` if it is missing, empty (0-byte) or undecodable.
    pub fn analysis(&self, audio: &Path) -> Option<strfile::Analysis> {
        let path = match self {
            EditorSource::Sidecar => audio.with_extension("str"),
            EditorSource::Dir(dir) => {
                let size = std::fs::metadata(audio).ok()?.len();
                let enc = naming::encoded_name(audio.file_stem()?.to_str()?, size);
                let (ab, c) = enc.rsplit_once('/')?;
                std::fs::read_dir(dir.join(ab)).ok()?.flatten().map(|e| e.path()).find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(&format!("{c}.")) && n.ends_with(".str"))
                })?
            }
        };
        strfile::decode(&std::fs::read_to_string(path).ok()?).ok()
    }
}

/// Every audio file under `dir`, sorted.
pub fn audio_files(dir: &Path) -> Vec<PathBuf> {
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

pub fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}
