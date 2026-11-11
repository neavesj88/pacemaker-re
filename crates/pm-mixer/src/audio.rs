//! Decoding a clip's audio file to stereo f32 at the engine rate.
//!
//! `pm_analysis::decode` returns a mono mix-down at the file's rate, which is right for
//! analysis but not for a stereo render, so this module decodes with symphonia directly (the
//! same crate and version) and keeps the channels. Mono files become dual-mono, files with
//! more than two channels use the first two.

use std::path::Path;
use std::sync::Arc;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::sinc;
use crate::{Error, RATE};

/// Decimation factor of the coarse search signal.
pub const COARSE: usize = 4;

/// Decoded clip audio, resident for the time the clip is playing.
pub struct Source {
    pub l: Vec<f32>,
    pub r: Vec<f32>,
    /// Mono at `1/COARSE` of the rate (box-filtered), for the time-stretcher's coarse search.
    pub coarse: Vec<f32>,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Source({} frames)", self.l.len())
    }
}

impl Source {
    pub fn new(l: Vec<f32>, r: Vec<f32>) -> Arc<Source> {
        let n = l.len() / COARSE;
        let mut coarse = Vec::with_capacity(n);
        for c in 0..n {
            let mut s = 0.0f32;
            for i in c * COARSE..(c + 1) * COARSE {
                s += l[i] + r[i];
            }
            coarse.push(s * (0.5 / COARSE as f32));
        }
        Arc::new(Source { l, r, coarse })
    }

    pub fn len(&self) -> usize {
        self.l.len()
    }

    pub fn is_empty(&self) -> bool {
        self.l.is_empty()
    }
}

/// Decodes `path` and converts it to [`RATE`] Hz stereo.
pub fn load(path: &Path) -> Result<Arc<Source>, Error> {
    let (l, r, rate) = decode_stereo(path)?;
    if rate == RATE {
        return Ok(Source::new(l, r));
    }
    Ok(Source::new(sinc::resample(&l, rate, RATE), sinc::resample(&r, rate, RATE)))
}

fn decode_stereo(path: &Path) -> Result<(Vec<f32>, Vec<f32>, u32), Error> {
    let name = path.display().to_string();
    let bad = |e: SymErr| Error::Decode(format!("{name}: {e}"));
    let none = || Error::Decode(format!("{name}: no audio track"));
    let file = std::fs::File::open(path).map_err(|e| Error::Decode(format!("{name}: {e}")))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(bad)?;
    let track = format.default_track(TrackType::Audio).ok_or_else(none)?;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(none)?.clone();
    let rate = params.sample_rate.ok_or_else(|| Error::Decode(format!("{name}: unknown sample rate")))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(bad)?;

    let (mut l, mut r) = (Vec::new(), Vec::new());
    let mut inter: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(bad(e)),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let ch = buf.spec().channels().count().max(1);
                inter.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut inter);
                for f in inter.chunks_exact(ch) {
                    l.push(f[0]);
                    r.push(if ch > 1 { f[1] } else { f[0] });
                }
            }
            // a corrupt frame costs one frame, not the clip
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(bad(e)),
        }
    }
    Ok((l, r, rate))
}
