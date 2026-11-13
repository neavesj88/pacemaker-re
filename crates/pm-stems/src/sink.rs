//! Stem file writers: MP3 (LAME), Ogg Vorbis and 16-bit WAV, all 44.1 kHz stereo.
//!
//! A file is written as `<dest>.partial` and renamed to its final name only by
//! [`FileSink::finish`]; a failed, cancelled or dropped sink removes the partial file, so a
//! half-written stem never carries a real name. An existing `dest` is replaced only at that
//! final rename (the caller decides beforehand whether replacing is allowed).

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::num::{NonZeroU8, NonZeroU32};
use std::path::{Path, PathBuf};

use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Id3Tag, Quality};
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoder, VorbisEncoderBuilder};

use crate::{Error, RATE};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    /// Constant bitrate; one of the LAME rates (128, 160, 192, 224, 256, 320). `align`: LAME's
    /// encoder plus decoder delay is [`MP3_DELAY`] samples, which a decoder that does not read
    /// gapless data (and none is written here) plays as that much silence in front. With `align`
    /// the first [`MP3_DELAY`] input samples are dropped and the end padded, so everything after
    /// the first 25 ms sits at its original position.
    Mp3 { kbps: u16, align: bool },
    /// Quality-based VBR, -0.1 (smallest) to 1.0 (best).
    Ogg { quality: f32 },
    /// 16-bit PCM with TPDF dither.
    Wav,
}

impl Format {
    pub fn extension(&self) -> &'static str {
        match self {
            Format::Mp3 { .. } => "mp3",
            Format::Ogg { .. } => "ogg",
            Format::Wav => "wav",
        }
    }

    /// Whether the container carries the tags below.
    pub fn has_tags(&self) -> bool {
        !matches!(self, Format::Wav)
    }
}

/// Text tags written into the file. Empty fields are left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub comment: String,
}

fn bitrate(kbps: u16) -> Result<Bitrate, Error> {
    Ok(match kbps {
        128 => Bitrate::Kbps128,
        160 => Bitrate::Kbps160,
        192 => Bitrate::Kbps192,
        224 => Bitrate::Kbps224,
        256 => Bitrate::Kbps256,
        320 => Bitrate::Kbps320,
        other => return Err(Error::Invalid(format!("{other} kbps is not an MP3 rate this app offers (128, 160, 192, 224, 256, 320)"))),
    })
}

/// ID3 text for LAME, which takes Latin-1: characters beyond it become `?` in the TAG only (the
/// file name keeps them).
fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}

/// LAME's encoder delay (576) plus the usual decoder delay (529), in samples. Measured with
/// symphonia's MP3 decoder in the tests below; a different decoder may differ.
pub const MP3_DELAY: usize = 1105;

/// The partial file's name: `<dest>.partial`.
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

struct Dither(u32);

impl Dither {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }
}

fn wav_header(frames: u64) -> Result<[u8; 44], Error> {
    let data = frames * 4;
    if data + 36 > u64::from(u32::MAX) {
        return Err(Error::Encode("the track is too long for a WAV file (over 6 hours)".into()));
    }
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&((data + 36) as u32).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&2u16.to_le_bytes());
    h[24..28].copy_from_slice(&RATE.to_le_bytes());
    h[28..32].copy_from_slice(&(RATE * 4).to_le_bytes());
    h[32..34].copy_from_slice(&4u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&(data as u32).to_le_bytes());
    Ok(h)
}

enum Inner {
    Wav { w: BufWriter<File>, frames: u64, dl: Dither, dr: Dither },
    Ogg(Box<VorbisEncoder<BufWriter<File>>>),
    Mp3 { enc: Box<mp3lame_encoder::Encoder>, w: BufWriter<File>, buf: Vec<u8>, skip: usize, tail: usize },
}

pub struct FileSink {
    dest: PathBuf,
    tmp: PathBuf,
    inner: Option<Inner>,
}

impl FileSink {
    /// Starts `<dest>.partial`. Does not touch `dest`.
    pub fn create(dest: &Path, format: Format, tags: &Tags) -> Result<FileSink, Error> {
        let tmp = partial_path(dest);
        let inner = Self::open(&tmp, format, tags);
        match inner {
            Ok(inner) => Ok(FileSink { dest: dest.to_path_buf(), tmp, inner: Some(inner) }),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn open(tmp: &Path, format: Format, tags: &Tags) -> Result<Inner, Error> {
        let file = File::create(tmp)?;
        match format {
            Format::Wav => {
                let mut w = BufWriter::new(file);
                w.write_all(&wav_header(0)?)?;
                Ok(Inner::Wav { w, frames: 0, dl: Dither(0x1234_5678), dr: Dither(0x9ABC_DEF1) })
            }
            Format::Ogg { quality } => {
                let enc_err = |e: vorbis_rs::VorbisError| Error::Encode(e.to_string());
                let mut b = VorbisEncoderBuilder::new_with_serial(NonZeroU32::new(RATE).unwrap(), NonZeroU8::new(2).unwrap(), BufWriter::new(file), 0x5354_454D);
                b.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr { target_quality: quality.clamp(-0.1, 1.0) });
                for (k, v) in [("TITLE", &tags.title), ("ARTIST", &tags.artist), ("ALBUM", &tags.album), ("DATE", &tags.year), ("COMMENT", &tags.comment)] {
                    if !v.is_empty() {
                        b.comment_tag(k, v.as_str()).map_err(enc_err)?;
                    }
                }
                b.comment_tag("ENCODER", "Pacemaker Revival").map_err(enc_err)?;
                Ok(Inner::Ogg(Box::new(b.build().map_err(enc_err)?)))
            }
            Format::Mp3 { kbps, align } => {
                let be = |e: mp3lame_encoder::BuildError| Error::Encode(format!("{e:?}"));
                let mut b = Builder::new().ok_or_else(|| Error::Encode("the MP3 encoder could not be created".into()))?;
                b.set_num_channels(2).map_err(be)?;
                b.set_sample_rate(RATE).map_err(be)?;
                b.set_brate(bitrate(kbps)?).map_err(be)?;
                b.set_quality(Quality::Best).map_err(be)?;
                // no Xing / Info frame: it would need a seek back to the start after the last
                // frame, and its gapless data is not something the device is known to read
                b.set_to_write_vbr_tag(false).map_err(be)?;
                let (title, artist, album, year, comment) = (latin1(&tags.title), latin1(&tags.artist), latin1(&tags.album), latin1(&tags.year), latin1(&tags.comment));
                b.set_id3_tag(Id3Tag { title: &title, artist: &artist, album: &album, album_art: &[], year: &year, comment: &comment })
                    .map_err(|e| Error::Encode(format!("{e:?}")))?;
                let enc = b.build().map_err(be)?;
                // zeros fed after the last sample so the decoder's delay does not cut the end off
                let (skip, tail) = if align { (MP3_DELAY, MP3_DELAY + 2 * 1152) } else { (0, 0) };
                Ok(Inner::Mp3 { enc: Box::new(enc), w: BufWriter::new(file), buf: Vec::new(), skip, tail })
            }
        }
    }

    pub fn write(&mut self, l: &[f32], r: &[f32]) -> Result<(), Error> {
        if l.len() != r.len() {
            return Err(Error::Invalid("the two channels differ in length".into()));
        }
        match self.inner.as_mut().ok_or_else(|| Error::Invalid("the file is already finished".into()))? {
            Inner::Wav { w, frames, dl, dr } => {
                let mut buf = Vec::with_capacity(l.len() * 4);
                for (a, b) in l.iter().zip(r) {
                    for (x, d) in [(a, &mut *dl), (b, &mut *dr)] {
                        let v = (x * 32767.0 + d.next() + d.next()).round().clamp(-32768.0, 32767.0) as i16;
                        buf.extend_from_slice(&v.to_le_bytes());
                    }
                }
                *frames += l.len() as u64;
                w.write_all(&buf)?;
            }
            Inner::Ogg(enc) => {
                if !l.is_empty() {
                    enc.encode_audio_block([l, r]).map_err(|e| Error::Encode(e.to_string()))?;
                }
            }
            Inner::Mp3 { enc, w, buf, skip, .. } => {
                let drop = (*skip).min(l.len());
                *skip -= drop;
                let (l, r) = (&l[drop..], &r[drop..]);
                for (cl, cr) in l.chunks(16_384).zip(r.chunks(16_384)) {
                    buf.clear();
                    buf.reserve(mp3lame_encoder::max_required_buffer_size(cl.len()));
                    enc.encode_to_vec(DualPcm { left: cl, right: cr }, buf).map_err(|e| Error::Encode(format!("{e:?}")))?;
                    w.write_all(buf)?;
                }
            }
        }
        Ok(())
    }

    /// Completes the file and moves it to its final name (replacing any file already there).
    pub fn finish(mut self) -> Result<PathBuf, Error> {
        let inner = self.inner.take().ok_or_else(|| Error::Invalid("the file is already finished".into()))?;
        let done = (|| -> Result<(), Error> {
            match inner {
                Inner::Wav { mut w, frames, .. } => {
                    w.flush()?;
                    let f = w.get_mut();
                    f.seek(SeekFrom::Start(0))?;
                    f.write_all(&wav_header(frames)?)?;
                    f.flush()?;
                }
                Inner::Ogg(enc) => {
                    let mut w = enc.finish().map_err(|e| Error::Encode(e.to_string()))?;
                    w.flush()?;
                }
                Inner::Mp3 { mut enc, mut w, mut buf, tail, .. } => {
                    if tail > 0 {
                        let zeros = vec![0.0f32; tail];
                        buf.clear();
                        buf.reserve(mp3lame_encoder::max_required_buffer_size(tail));
                        enc.encode_to_vec(DualPcm { left: &zeros, right: &zeros }, &mut buf).map_err(|e| Error::Encode(format!("{e:?}")))?;
                        w.write_all(&buf)?;
                    }
                    buf.clear();
                    buf.reserve(7200);
                    enc.flush_to_vec::<FlushNoGap>(&mut buf).map_err(|e| Error::Encode(format!("{e:?}")))?;
                    w.write_all(&buf)?;
                    w.flush()?;
                }
            }
            Ok(())
        })();
        let result = done.and_then(|_| std::fs::rename(&self.tmp, &self.dest).map_err(Error::from));
        if result.is_err() {
            let _ = std::fs::remove_file(&self.tmp);
        }
        result.map(|_| self.dest.clone())
    }
}

impl Drop for FileSink {
    fn drop(&mut self) {
        // still open: finish() was not reached (error, cancel). Close the handle first so the
        // removal also works on Windows.
        if self.inner.take().is_some() {
            let _ = std::fs::remove_file(&self.tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::{MetadataOptions, StandardTag};

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pm-stems-sink-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sine(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / RATE as f32).sin()).collect()
    }

    /// Decodes a file back: (frames per channel, rms of left, title tag).
    fn decode(path: &Path) -> (usize, f32, Option<String>, u32, usize) {
        let file = File::open(path).unwrap();
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension(path.extension().unwrap().to_str().unwrap());
        let mut format = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default()).unwrap();
        let mut title = None;
        {
            let mut md = format.metadata();
            loop {
                if let Some(rev) = md.current() {
                    for t in &rev.media.tags {
                        if let Some(StandardTag::TrackTitle(v)) = &t.std {
                            title = Some(v.to_string());
                        }
                    }
                }
                if md.pop().is_none() {
                    break;
                }
            }
        }
        let track = format.default_track(TrackType::Audio).unwrap();
        let params = track.codec_params.as_ref().unwrap().audio().unwrap().clone();
        let (rate, channels) = (params.sample_rate.unwrap(), params.channels.as_ref().map(|c| c.count()).unwrap_or(0));
        let id = track.id;
        let mut dec = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).unwrap();
        let (mut frames, mut sum, mut inter) = (0usize, 0.0f64, Vec::new());
        while let Ok(Some(p)) = format.next_packet() {
            if p.track_id != id {
                continue;
            }
            let buf = dec.decode(&p).unwrap();
            let ch = buf.spec().channels().count();
            inter.resize(buf.samples_interleaved(), 0.0f32);
            buf.copy_to_slice_interleaved(&mut inter);
            for f in inter.chunks_exact(ch) {
                sum += (f[0] as f64).powi(2);
                frames += 1;
            }
        }
        (frames, (sum / frames.max(1) as f64).sqrt() as f32, title, rate, channels)
    }

    #[test]
    fn every_format_round_trips_level_length_rate_and_title() {
        let dir = temp_dir("rt");
        let n = RATE as usize * 2;
        let x = sine(n, 0.5);
        let tags = Tags { title: "Demo Track (Acapella)".into(), artist: "Demo Artist".into(), ..Default::default() };
        for (fmt, name, tol) in [(Format::Wav, "a.wav", 0.01f32), (Format::Ogg { quality: 0.6 }, "a.ogg", 0.03), (Format::Mp3 { kbps: 320, align: true }, "a.mp3", 0.03)] {
            let dest = dir.join(name);
            let mut s = FileSink::create(&dest, fmt, &tags).unwrap();
            // in uneven pieces, as the pipeline delivers them
            for (a, b) in [(0usize, 1usize), (1, 30_000), (30_000, n)] {
                s.write(&x[a..b], &x[a..b]).unwrap();
            }
            assert!(!dest.exists(), "nothing under the final name before finish");
            assert!(partial_path(&dest).exists());
            assert_eq!(s.finish().unwrap(), dest);
            assert!(!partial_path(&dest).exists());
            let (frames, rms, title, rate, ch) = decode(&dest);
            let want_rms = 0.5 / 2f32.sqrt();
            assert_eq!((rate, ch), (RATE, 2), "{name}");
            assert!((rms - want_rms).abs() < tol * 3.0, "{name}: rms {rms} vs {want_rms}");
            // codecs pad: MP3 by up to a couple of frames, Vorbis by a block
            assert!(frames + 200 >= n && frames <= n + 6000, "{name}: {frames} frames for {n}");
            if fmt.has_tags() {
                assert_eq!(title.as_deref(), Some("Demo Track (Acapella)"), "{name}");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Position of the loudest sample of the left channel and the total frames decoded.
    fn impulse_position(path: &Path) -> (usize, usize) {
        let file = File::open(path).unwrap();
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension(path.extension().unwrap().to_str().unwrap());
        let mut format = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default()).unwrap();
        let track = format.default_track(TrackType::Audio).unwrap();
        let params = track.codec_params.as_ref().unwrap().audio().unwrap().clone();
        let id = track.id;
        let mut dec = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).unwrap();
        let (mut pos, mut peak, mut at, mut inter) = (0usize, 0.0f32, 0usize, Vec::new());
        while let Ok(Some(p)) = format.next_packet() {
            if p.track_id != id {
                continue;
            }
            let buf = dec.decode(&p).unwrap();
            let ch = buf.spec().channels().count();
            inter.resize(buf.samples_interleaved(), 0.0f32);
            buf.copy_to_slice_interleaved(&mut inter);
            for f in inter.chunks_exact(ch) {
                if f[0].abs() > peak {
                    peak = f[0].abs();
                    at = pos;
                }
                pos += 1;
            }
        }
        (at, pos)
    }

    #[test]
    fn stems_stay_on_the_original_sample_grid() {
        // the beat grid of the source is in samples, so a stem must not move the music
        let dir = temp_dir("align");
        let n = RATE as usize * 2;
        let mut x = vec![0.0f32; n];
        x[20_000] = 0.9;
        for (fmt, lag) in [(Format::Wav, 0i64), (Format::Ogg { quality: 0.6 }, 0), (Format::Mp3 { kbps: 320, align: true }, 0), (Format::Mp3 { kbps: 320, align: false }, MP3_DELAY as i64)] {
            let dest = dir.join(format!("p{}.{}", lag, fmt.extension()));
            let mut s = FileSink::create(&dest, fmt, &Tags::default()).unwrap();
            for (a, b) in [(0usize, 700usize), (700, n)] {
                s.write(&x[a..b], &x[a..b]).unwrap();
            }
            s.finish().unwrap();
            let (at, total) = impulse_position(&dest);
            assert!((at as i64 - 20_000 - lag).abs() <= 1, "{fmt:?}: impulse at {at}");
            assert!(total + MP3_DELAY + 2304 >= n && total <= n + 6000, "{fmt:?}: {total} frames for {n}");
            if matches!(fmt, Format::Mp3 { align: true, .. }) {
                assert!(total >= n - 100, "aligned MP3 keeps the full length: {total} for {n}");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_dropped_sink_leaves_no_file_and_keeps_the_old_one() {
        let dir = temp_dir("drop");
        let dest = dir.join("keep.wav");
        std::fs::write(&dest, b"old").unwrap();
        let mut s = FileSink::create(&dest, Format::Wav, &Tags::default()).unwrap();
        s.write(&[0.1; 100], &[0.1; 100]).unwrap();
        drop(s);
        assert_eq!(std::fs::read(&dest).unwrap(), b"old", "the existing file is untouched");
        assert!(!partial_path(&dest).exists(), "no partial left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finish_replaces_the_existing_file_and_bad_input_is_refused() {
        let dir = temp_dir("replace");
        let dest = dir.join("r.wav");
        std::fs::write(&dest, b"old").unwrap();
        let mut s = FileSink::create(&dest, Format::Wav, &Tags::default()).unwrap();
        assert!(s.write(&[0.0; 3], &[0.0; 2]).is_err());
        s.write(&[0.0; 4], &[0.0; 4]).unwrap();
        s.finish().unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), 44 + 16);
        assert!(FileSink::create(&dir.join("x.mp3"), Format::Mp3 { kbps: 123, align: true }, &Tags::default()).is_err());
        assert!(!partial_path(&dir.join("x.mp3")).exists(), "a failed create leaves nothing");
        assert!(FileSink::create(&dir.join("nodir").join("x.wav"), Format::Wav, &Tags::default()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn id3_text_is_latin1_with_a_marker_for_the_rest() {
        assert_eq!(latin1("Caf\u{e9} \u{4e16}"), vec![b'C', b'a', b'f', 0xE9, b' ', b'?']);
    }
}
