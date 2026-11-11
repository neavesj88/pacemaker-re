//! What an audio export writes besides the sound: the format's own settings (MP3 bitrate, FLAC
//! sample size and effort) and the tags (title, artist, album, year, comment, cover, and the track
//! list as chapters). The wire types are what the dialog sends; [`build_encoding`] turns them into
//! the `pm_mixer::Encoding` the writers take. The chapters are the entries of the same track list
//! the dialog shows and the CUE sheet uses (`tracklist::build_in`), so the three always agree.

use std::path::Path;

use pm_mixer::tags::{ms_of_frames, Chapter, Cover, Tags, MAX_COVER_BYTES};
use pm_mixer::{Encoding, FlacLevel, Format, Mp3Rate};
use serde::Deserialize;

use super::tracklist::{self, Tracklist, TracklistOptions};
use crate::commands::mixes::doc::MixDoc;
use crate::error::AppResult;

/// How an MP3 is encoded: `{ "kind": "cbr", "kbps": 192 }` or `{ "kind": "vbr", "quality": 2 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Mp3Spec {
    Cbr { kbps: u16 },
    Vbr { quality: u8 },
}

impl Default for Mp3Spec {
    fn default() -> Self {
        Mp3Spec::Cbr { kbps: 192 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FlacLevelSpec {
    Fast,
    #[default]
    Standard,
    Best,
}

/// The settings of the formats that have any. Every field has a default, and the settings of a
/// format other than the one exported are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EncodingSpec {
    pub mp3: Mp3Spec,
    /// FLAC sample size, 16 or 24.
    pub flac_bits: u8,
    pub flac_level: FlacLevelSpec,
}

impl Default for EncodingSpec {
    fn default() -> Self {
        EncodingSpec { mp3: Mp3Spec::default(), flac_bits: 16, flac_level: FlacLevelSpec::default() }
    }
}

/// The tags to write. Text left empty is not written (the title is the mix name when empty).
/// `cover_path` is an image file the user picked. `chapters` carries the track list settings when
/// the tracks should be chapters of the file; absent = no chapters.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TagSpec {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub comment: String,
    pub cover_path: Option<String>,
    pub chapters: Option<TracklistOptions>,
}

/// The chapters of a file from its track list: each track runs to the start of the next, the last
/// to the end of the audio. Names are "Artist - Title" (the title alone without an artist).
pub fn chapters_of(tl: &Tracklist) -> Vec<Chapter> {
    let end_all = ms_of_frames(tl.total_frames);
    tl.entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let start_ms = ms_of_frames(e.start_frames);
            let end_ms = tl.entries.get(i + 1).map_or(end_all, |n| ms_of_frames(n.start_frames)).max(start_ms);
            let title = if e.artist.is_empty() { e.title.clone() } else { format!("{} - {}", e.artist, e.title) };
            Chapter { start_ms, end_ms, title }
        })
        .collect()
}

/// Reads and checks a cover image file. Only a regular JPEG or PNG file up to
/// [`MAX_COVER_BYTES`] is taken; the sentence says what is wrong otherwise.
pub fn read_cover(path: &str) -> AppResult<Cover> {
    let p = Path::new(path);
    let name = p.file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
    let meta = std::fs::metadata(p).map_err(|e| format!("The cover image {name} cannot be read: {e}. Choose another image or remove the cover."))?;
    if !meta.is_file() {
        return Err(format!("The cover image {name} is not a file.").into());
    }
    if meta.len() > MAX_COVER_BYTES as u64 {
        return Err(format!("The cover image {name} is {} MB; the most a tag takes is {} MB. Use a smaller image.", meta.len().div_ceil(1 << 20), MAX_COVER_BYTES >> 20).into());
    }
    let data = std::fs::read(p).map_err(|e| format!("The cover image {name} cannot be read: {e}."))?;
    Ok(Cover::from_bytes(data).map_err(|e| format!("{name}: {e}"))?)
}

/// The tags for an export: the user's texts, the cover, and the chapters of the track list as the
/// export will play it (`span` = the exported part, as for the CUE sheet). Fails before anything is
/// rendered when the cover cannot be used or the track list cannot be made.
pub fn build_tags(spec: &TagSpec, mix_name: &str, doc: &MixDoc, has_audio: &dyn Fn(i64) -> bool, span: Option<(i64, i64)>) -> AppResult<Tags> {
    let title = if spec.title.trim().is_empty() { mix_name.trim() } else { spec.title.trim() };
    let cover = match spec.cover_path.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(p) => Some(read_cover(p)?),
        None => None,
    };
    let chapters = match &spec.chapters {
        Some(opts) => chapters_of(&tracklist::build_in(doc, opts, has_audio, span)?),
        None => Vec::new(),
    };
    Ok(Tags { title: title.to_string(), artist: spec.artist.clone(), album: spec.album.clone(), year: spec.year.clone(), comment: spec.comment.clone(), cover, chapters })
}

/// The writer's `Encoding` for `format`, its settings and the tags (`None` = the plain export).
pub fn build_encoding(format: Format, spec: &EncodingSpec, tags: Option<Tags>) -> AppResult<Encoding> {
    let enc = Encoding {
        format,
        mp3: match spec.mp3 {
            Mp3Spec::Cbr { kbps } => Mp3Rate::Cbr(kbps),
            Mp3Spec::Vbr { quality } => Mp3Rate::Vbr(quality),
        },
        flac_bits: spec.flac_bits,
        flac_level: match spec.flac_level {
            FlacLevelSpec::Fast => FlacLevel::Fast,
            FlacLevelSpec::Standard => FlacLevel::Standard,
            FlacLevelSpec::Best => FlacLevel::Best,
        },
        tags,
    };
    enc.validate().map_err(|e| e.to_string())?;
    Ok(enc)
}

/// Things asked for that the chosen format cannot hold, for the end-of-job summary.
pub fn format_notes(format: Format, tags: &Tags) -> Vec<String> {
    let mut n = Vec::new();
    if format == Format::Wav && tags.cover.is_some() {
        n.push("A WAV file cannot hold a cover image, so it was left out.".to_string());
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::export::tracklist::Entry;

    fn entry(n: usize, artist: &str, title: &str, start_frames: i64) -> Entry {
        Entry {
            number: n,
            clip_id: format!("c{n}"),
            track_id: n as i64,
            artist: artist.into(),
            title: title.into(),
            album: String::new(),
            key: String::new(),
            bpm: 120.0,
            available: true,
            start_frames,
            start_secs: start_frames as f64 / 44100.0,
            start_label: String::new(),
            overlaps_previous: false,
        }
    }

    #[test]
    fn chapters_run_from_one_track_to_the_next_and_the_last_to_the_end() {
        let tl = Tracklist {
            entries: vec![entry(1, "Artist A", "Song A", 0), entry(2, "", "Song B", 44100 * 61 + 22050), entry(3, "Artist C", "Song C", 44100 * 61 + 22050)],
            total_frames: 44100 * 180,
            warnings: vec![],
        };
        let c = chapters_of(&tl);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].start_ms, c[0].end_ms, c[0].title.as_str()), (0, 61_500, "Artist A - Song A"));
        assert_eq!((c[1].start_ms, c[1].end_ms, c[1].title.as_str()), (61_500, 61_500, "Song B"), "tracks that start together have an empty first chapter, never a negative one");
        assert_eq!((c[2].start_ms, c[2].end_ms), (61_500, 180_000));
    }

    #[test]
    fn the_wire_forms_have_defaults_and_read_the_dialogs_json() {
        let d: EncodingSpec = serde_json::from_str("{}").unwrap();
        assert_eq!(d, EncodingSpec::default());
        assert_eq!((d.mp3, d.flac_bits, d.flac_level), (Mp3Spec::Cbr { kbps: 192 }, 16, FlacLevelSpec::Standard));
        let e: EncodingSpec = serde_json::from_str(r#"{"mp3":{"kind":"vbr","quality":2},"flacBits":24,"flacLevel":"best"}"#).unwrap();
        assert_eq!((e.mp3, e.flac_bits, e.flac_level), (Mp3Spec::Vbr { quality: 2 }, 24, FlacLevelSpec::Best));
        let t: TagSpec = serde_json::from_str(r#"{"artist":"DJ Example","chapters":{"rounding":"nearest"}}"#).unwrap();
        assert_eq!(t.artist, "DJ Example");
        assert_eq!(t.chapters.unwrap().rounding, tracklist::Rounding::Nearest);
        assert!(t.cover_path.is_none() && t.title.is_empty());
    }

    #[test]
    fn settings_out_of_range_are_refused_with_a_sentence() {
        let bad = |spec: EncodingSpec| build_encoding(Format::Mp3, &spec, None).unwrap_err().to_string();
        assert!(bad(EncodingSpec { mp3: Mp3Spec::Cbr { kbps: 100 }, ..EncodingSpec::default() }).contains("100 kbit/s"));
        assert!(bad(EncodingSpec { mp3: Mp3Spec::Vbr { quality: 12 }, ..EncodingSpec::default() }).contains("out of range"));
        assert!(bad(EncodingSpec { flac_bits: 32, ..EncodingSpec::default() }).contains("32 bits"));
        let ok = build_encoding(Format::Flac, &EncodingSpec { flac_bits: 24, flac_level: FlacLevelSpec::Best, ..EncodingSpec::default() }, None).unwrap();
        assert_eq!((ok.format, ok.flac_bits, ok.flac_level), (Format::Flac, 24, FlacLevel::Best));
    }

    #[test]
    fn a_cover_must_be_a_real_small_image_file() {
        let d = std::env::temp_dir().join(format!("pmrev-cover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let msg = |p: &Path| read_cover(p.to_str().unwrap()).unwrap_err().to_string();
        assert!(msg(&d.join("missing.png")).contains("missing.png cannot be read"));
        assert!(msg(&d).contains("is not a file"));
        std::fs::write(d.join("notes.png"), b"just text").unwrap();
        assert!(msg(&d.join("notes.png")).contains("JPEG or PNG"));
        std::fs::write(d.join("huge.png"), vec![0u8; MAX_COVER_BYTES + 1]).unwrap();
        assert!(msg(&d.join("huge.png")).contains("smaller image"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_wav_says_it_cannot_hold_a_cover() {
        let with = Tags { cover: Some(Cover { mime: "image/png".into(), data: vec![1], width: 1, height: 1, depth: 24 }), ..Tags::default() };
        assert_eq!(format_notes(Format::Wav, &with).len(), 1);
        assert!(format_notes(Format::Flac, &with).is_empty());
        assert!(format_notes(Format::Wav, &Tags::default()).is_empty());
    }
}
