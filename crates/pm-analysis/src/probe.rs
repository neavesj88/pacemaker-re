//! A light look at an audio file: title, artist and length from the tags and the container, with
//! no decoding. Used to recognise a track's file again (the Relink search reads thousands of
//! files, so this must stay far cheaper than [`crate::decode::decode`]).

use std::path::Path;

use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::decode::{fill_meta, Error};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    pub title: String,
    pub artist: String,
    /// From the container (frame count and sample rate, or its duration field). None when the
    /// file does not say: some MP3s without a length header.
    pub duration_secs: Option<f64>,
}

/// Reads the tags and the length of `path` without decoding any audio.
pub fn probe(path: &Path) -> Result<Probe, Error> {
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
    {
        let mut md = format.metadata();
        loop {
            if let Some(rev) = md.current() {
                fill_meta(rev, &mut meta);
            }
            if md.pop().is_none() {
                break;
            }
        }
    }
    let track = format.default_track(TrackType::Audio).ok_or(Error::NoAudioTrack)?;
    let rate = track.codec_params.as_ref().and_then(|p| p.audio()).and_then(|a| a.sample_rate);
    let from_frames = match (track.num_frames, rate) {
        (Some(n), Some(r)) if r > 0 => Some(n as f64 / f64::from(r)),
        _ => None,
    };
    let from_container = || {
        let (tb, d) = (track.time_base?, track.duration?);
        tb.calc_duration(d).map(|t| t.as_secs_f64())
    };
    let duration_secs = from_frames.or_else(from_container).filter(|d| d.is_finite() && *d > 0.0);
    Ok(Probe { title: meta.title, artist: meta.artist, duration_secs })
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
    fn the_length_comes_from_the_container_without_decoding() {
        let dir = std::env::temp_dir().join(format!("pm-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("synthetic.wav");
        std::fs::write(&p, wav(3)).unwrap();
        let got = probe(&p).unwrap();
        assert!((got.duration_secs.unwrap() - 3.0).abs() < 0.01, "{got:?}");
        assert!(got.title.is_empty() && got.artist.is_empty());
        // not audio: an error, never a panic
        let bad = dir.join("bad.mp3");
        std::fs::write(&bad, b"not audio at all").unwrap();
        assert!(probe(&bad).is_err());
        assert!(probe(&dir.join("missing.wav")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
