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
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

fn fill_meta(rev: &MetadataRevision, m: &mut pm_formats::library::TrackMeta) {
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

pub fn decode(path: &Path) -> Result<Decoded, Error> {
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
