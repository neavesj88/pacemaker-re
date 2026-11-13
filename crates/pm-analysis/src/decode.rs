//! Audio decoding to mono f32 at the file's native rate, plus the tags the analyser cares about.
//! Written against symphonia 0.6.

use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTag};

pub struct Decoded {
    pub mono: Vec<f32>,
    pub sample_rate: u32,
    /// BPM tag (ID3 TBPM etc.) if present: a reference for evaluation, never used for analysis.
    pub tag_bpm: Option<f64>,
    /// Tags and stream facts in the shape the library writer takes.
    pub meta: pm_formats::library::TrackMeta,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Decode(SymErr),
    NoAudioTrack,
    NoSampleRate,
    /// Longer than the limit given to [`decode_limited`]; nothing was analysed.
    TooLong { limit_minutes: u32 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TooLong { limit_minutes } => {
                write!(f, "longer than {limit_minutes} minutes, so it is not analysed (the whole track is held in memory)")
            }
            _ => write!(f, "{self:?}"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn fill_meta(rev: &MetadataRevision, m: &mut pm_formats::library::TrackMeta) {
    for t in &rev.media.tags {
        let set = |field: &mut String, v: &str| {
            if field.is_empty() {
                *field = v.trim().to_string();
            }
        };
        match &t.std {
            Some(StandardTag::TrackTitle(v)) => set(&mut m.title, v),
            Some(StandardTag::Artist(v)) => set(&mut m.artist, v),
            Some(StandardTag::AlbumArtist(v)) => set(&mut m.album_artist, v),
            Some(StandardTag::Album(v)) => set(&mut m.album, v),
            Some(StandardTag::Composer(v)) => set(&mut m.composer, v),
            Some(StandardTag::Genre(v)) => set(&mut m.genre, v),
            Some(StandardTag::Label(v)) => set(&mut m.label, v),
            Some(StandardTag::InitialKey(v)) => set(&mut m.key, v),
            Some(StandardTag::Comment(v)) => set(&mut m.comment, v),
            Some(StandardTag::Producer(v)) => set(&mut m.producer, v),
            Some(StandardTag::Remixer(v)) => set(&mut m.remixer, v),
            Some(StandardTag::TrackNumber(n)) if m.track_number == 0 => m.track_number = *n as i64,
            Some(StandardTag::DiscNumber(n)) if m.disc_number == 0 => m.disc_number = *n as i64,
            Some(StandardTag::ReleaseYear(y)) | Some(StandardTag::RecordingYear(y)) if m.year == 0 => m.year = *y as i64,
            Some(StandardTag::ReleaseDate(v)) | Some(StandardTag::RecordingDate(v)) if m.year == 0 => {
                m.year = v.get(..4).and_then(|y| y.parse().ok()).unwrap_or(0)
            }
            _ => {}
        }
    }
}

fn bpm_from(rev: &MetadataRevision) -> Option<f64> {
    rev.media.tags.iter().find_map(|t| match t.std {
        Some(StandardTag::Bpm(b)) if b > 0 => Some(b as f64),
        _ => None,
    })
}

/// How long a track the background analyses (energy, sections, sound check) take in: the decoded
/// track is one mono `f32` buffer (about 635 MB an hour at 44.1 kHz) and several run at once.
pub const ANALYSIS_MAX_MINUTES: u32 = 60;

pub fn decode(path: &Path) -> Result<Decoded, Error> {
    decode_inner(path, None)
}

/// [`decode`] that stops, before the memory is spent, once the track is longer than
/// `limit_minutes` (the container's length is not trusted: some files do not state one).
pub fn decode_limited(path: &Path, limit_minutes: u32) -> Result<Decoded, Error> {
    decode_inner(path, Some(limit_minutes))
}

fn decode_inner(path: &Path, limit_minutes: Option<u32>) -> Result<Decoded, Error> {
    let file = std::fs::File::open(path).map_err(Error::Io)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(Error::Decode)?;

    let mut meta = pm_formats::library::TrackMeta::default();
    let tag_bpm = {
        let mut md = format.metadata();
        let mut bpm = None;
        loop {
            if let Some(rev) = md.current() {
                fill_meta(rev, &mut meta);
                bpm = bpm.or_else(|| bpm_from(rev));
            }
            if md.pop().is_none() {
                break;
            }
        }
        bpm
    };

    let track = format.default_track(TrackType::Audio).ok_or(Error::NoAudioTrack)?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(Error::NoAudioTrack)?
        .clone();
    let sample_rate = params.sample_rate.ok_or(Error::NoSampleRate)?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(Error::Decode)?;

    let mut mono = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    // audio payload bytes only: tags and embedded artwork must not inflate the bitrate
    let mut audio_bytes = 0u64;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(Error::Decode(e)),
        };
        if packet.track_id != track_id {
            continue;
        }
        audio_bytes += packet.data.len() as u64;
        match decoder.decode(&packet) {
            Ok(buf) => {
                let ch = buf.spec().channels().count().max(1);
                interleaved.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut interleaved);
                mono.extend(interleaved.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
                if limit_minutes.is_some_and(|m| mono.len() as u64 > u64::from(m) * 60 * u64::from(sample_rate)) {
                    return Err(Error::TooLong { limit_minutes: limit_minutes.unwrap_or_default() });
                }
            }
            // a corrupt frame in a real-world file should cost one frame, not the track
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(Error::Decode(e)),
        }
    }
    let secs = mono.len() as f64 / sample_rate as f64;
    meta.sample_rate = sample_rate as i64;
    meta.play_time_secs = secs.round() as i64;
    meta.format = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_uppercase();
    if secs > 0.0 {
        meta.bit_rate_kbps = (audio_bytes as f64 * 8.0 / secs / 1000.0).round() as i64;
    }
    if meta.title.is_empty() {
        meta.title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    }
    Ok(Decoded { mono, sample_rate, tag_bpm, meta })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit mono PCM WAV of `secs` seconds of silence at 8 kHz.
    fn wav(secs: u32) -> Vec<u8> {
        let rate = 8000u32;
        let data = rate * 2 * secs;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&(rate * 2).to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data.to_le_bytes());
        v.resize(v.len() + data as usize, 0);
        v
    }

    #[test]
    fn a_track_over_the_limit_is_refused_before_it_is_all_in_memory() {
        let dir = std::env::temp_dir().join(format!("pm-decode-limit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("synthetic.wav");
        std::fs::write(&p, wav(70)).unwrap();
        let short = decode_limited(&p, 1).err().unwrap();
        assert!(matches!(short, Error::TooLong { limit_minutes: 1 }), "{short:?}");
        assert!(short.to_string().contains("longer than 1 minutes"), "{short}");
        assert_eq!(decode_limited(&p, 2).unwrap().mono.len(), 70 * 8000, "under the limit it is the same as decode");
        assert_eq!(decode(&p).unwrap().mono.len(), 70 * 8000);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
