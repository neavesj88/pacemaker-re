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

    let tag_bpm = {
        let mut md = format.metadata();
        let mut bpm = md.current().and_then(bpm_from);
        while bpm.is_none() && md.pop().is_some() {
            bpm = md.current().and_then(bpm_from);
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
    Ok(Decoded { mono, sample_rate, tag_bpm })
}
