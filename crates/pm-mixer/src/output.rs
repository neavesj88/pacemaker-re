//! File writers: 16-bit PCM WAV, Ogg Vorbis, FLAC (16 or 24 bit) and MP3 (LAME, CBR or VBR), each
//! with the tags of [`crate::tags`].
//!
//! All write to `<dest>.partial` and rename on success, so a cancelled or failed export never
//! leaves a half file under the final name.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::num::{NonZeroU32, NonZeroU8};
use std::path::{Path, PathBuf};

use flac_codec::encode::{FlacChannelWriter, Options as FlacOptions};
use flac_codec::metadata::{Picture, PictureType};
use mp3lame_encoder::{Bitrate, Builder as LameBuilder, DualPcm, FlushGap, Mode as LameMode, Quality as LameQuality, VbrMode};
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoder, VorbisEncoderBuilder};

use crate::engine::{self, Options, Rendered, Resolver};
use crate::tags::{self, Tags};
use crate::{Error, RATE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Ogg,
    Flac,
    Mp3,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Ogg => "ogg",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
        }
    }
}

/// How an MP3 is encoded: a constant bitrate in kbit/s, or LAME's variable bitrate at quality
/// 0 (best, about 245 kbit/s) to 9 (smallest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mp3Rate {
    Cbr(u16),
    Vbr(u8),
}

/// The constant bitrates offered (MPEG-1 Layer III values at 44.1 kHz).
pub const MP3_CBR_KBPS: [u16; 8] = [96, 112, 128, 160, 192, 224, 256, 320];

impl Default for Mp3Rate {
    fn default() -> Self {
        Mp3Rate::Cbr(192)
    }
}

impl Mp3Rate {
    pub fn validate(self) -> Result<(), Error> {
        match self {
            Mp3Rate::Cbr(k) if MP3_CBR_KBPS.contains(&k) => Ok(()),
            Mp3Rate::Cbr(k) => Err(Error::Invalid(format!("{k} kbit/s is not an MP3 bitrate here (use one of {MP3_CBR_KBPS:?})."))),
            Mp3Rate::Vbr(q) if q <= 9 => Ok(()),
            Mp3Rate::Vbr(q) => Err(Error::Invalid(format!("MP3 variable-bitrate quality {q} is out of range (0 is best, 9 smallest)."))),
        }
    }
}

/// How hard the FLAC encoder works for a smaller file. The audio is identical at every level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlacLevel {
    Fast,
    #[default]
    Standard,
    Best,
}

/// A format with its settings and tags. `tags: None` keeps what exports have always done (Ogg
/// gets the mix name as its title, nothing else is tagged).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoding {
    pub format: Format,
    pub mp3: Mp3Rate,
    /// FLAC sample size: 16 (dithered, like the WAV) or 24.
    pub flac_bits: u8,
    pub flac_level: FlacLevel,
    pub tags: Option<Tags>,
}

impl Encoding {
    pub fn new(format: Format) -> Encoding {
        Encoding { format, mp3: Mp3Rate::default(), flac_bits: 16, flac_level: FlacLevel::default(), tags: None }
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.mp3.validate()?;
        if self.flac_bits != 16 && self.flac_bits != 24 {
            return Err(Error::Invalid(format!("{} bits is not a FLAC sample size here (use 16 or 24).", self.flac_bits)));
        }
        Ok(())
    }
}

/// Deterministic TPDF dither source (xorshift), one per channel.
struct Dither(u32);

impl Dither {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        // uniform in [-0.5, 0.5)
        (x >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }
}

struct WavWriter {
    w: BufWriter<File>,
    frames: u64,
    dl: Dither,
    dr: Dither,
    /// Chunks after `data` (tags, cue points), written when the audio is complete.
    extra: Vec<u8>,
}

fn header(frames: u64, extra: u64) -> Result<[u8; 44], Error> {
    let data = frames * 4;
    if data + 36 + extra > u64::from(u32::MAX) {
        return Err(Error::Invalid("the mix is too long for a WAV file (over 6 hours)".into()));
    }
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&((data + 36 + extra) as u32).to_le_bytes());
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

impl WavWriter {
    fn create(path: &Path, extra: Vec<u8>) -> Result<WavWriter, Error> {
        let mut w = BufWriter::new(File::create(path)?);
        w.write_all(&header(0, 0)?)?;
        Ok(WavWriter { w, frames: 0, dl: Dither(0x1234_5678), dr: Dither(0x9ABC_DEF1), extra })
    }

    fn write(&mut self, l: &[f32], r: &[f32]) -> Result<(), Error> {
        let mut buf = Vec::with_capacity(l.len() * 4);
        for (a, b) in l.iter().zip(r) {
            for (x, d) in [(a, &mut self.dl), (b, &mut self.dr)] {
                let v = (x * 32767.0 + d.next() + d.next()).round().clamp(-32768.0, 32767.0) as i16;
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
        self.frames += l.len() as u64;
        self.w.write_all(&buf)?;
        Ok(())
    }

    fn finish(mut self) -> Result<(), Error> {
        // the header is checked against the final size first, so a too-long file fails before
        // anything is appended
        let head = header(self.frames, self.extra.len() as u64)?;
        self.w.write_all(&self.extra)?;
        self.w.flush()?;
        let f = self.w.get_mut();
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&head)?;
        f.flush()?;
        Ok(())
    }
}

/// A render in which every clip failed is silence, not a mix: refuse it so no file is written.
fn check_rendered(out: &Rendered) -> Result<(), Error> {
    if out.clips_rendered == 0 && out.clips_failed > 0 {
        return Err(Error::Invalid(format!("none of the clips could be rendered: {}", out.warnings.join("; "))));
    }
    Ok(())
}

fn partial(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

/// Renders the mix to `dest` (overwriting it) in `format`, with the default settings and, for Ogg,
/// the mix name as title.
pub fn render_to_file(
    mix: &pm_formats::mix::Mix,
    res: &dyn Resolver,
    opts: &Options,
    format: Format,
    dest: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<Rendered, Error> {
    render_to_file_with(mix, res, opts, &Encoding::new(format), dest, progress)
}

/// 16-bit samples with the same dither as the WAV, or 24-bit rounded, as the `i32`s FLAC takes.
fn to_pcm(x: &[f32], bits: u8, dither: &mut Dither, out: &mut Vec<i32>) {
    out.clear();
    if bits == 16 {
        out.extend(x.iter().map(|v| (v * 32767.0 + dither.next() + dither.next()).round().clamp(-32768.0, 32767.0) as i32));
    } else {
        out.extend(x.iter().map(|v| (v * 8_388_607.0).round().clamp(-8_388_608.0, 8_388_607.0) as i32));
    }
}

fn flac_err(e: flac_codec::Error) -> Error {
    Error::Encode(format!("FLAC: {e}"))
}

fn write_flac(mix: &pm_formats::mix::Mix, res: &dyn Resolver, opts: &Options, enc: &Encoding, tmp: &Path, progress: &mut dyn FnMut(f64) -> bool) -> Result<Rendered, Error> {
    let mut o = match enc.flac_level {
        FlacLevel::Fast => FlacOptions::fast(),
        FlacLevel::Standard => FlacOptions::default(),
        FlacLevel::Best => FlacOptions::best(),
    };
    if let Some(t) = &enc.tags {
        for (k, v) in tags::vorbis_fields(t) {
            o = o.tag(&k, v);
        }
        if let Some(c) = &t.cover {
            let pic = Picture::new(PictureType::FrontCover, "", c.data.clone()).map_err(|e| Error::Invalid(format!("The cover image cannot be read ({e}).")))?;
            o = o.picture(pic);
        }
    }
    let mut file = BufWriter::new(File::create(tmp)?);
    let bits = enc.flac_bits;
    let mut w = FlacChannelWriter::new(&mut file, o, RATE, u32::from(bits), 2, None).map_err(flac_err)?;
    let (mut dl, mut dr) = (Dither(0x1234_5678), Dither(0x9ABC_DEF1));
    let (mut li, mut ri) = (Vec::<i32>::new(), Vec::<i32>::new());
    let out = engine::render(mix, res, opts, progress, &mut |l, r| {
        to_pcm(l, bits, &mut dl, &mut li);
        to_pcm(r, bits, &mut dr, &mut ri);
        w.write([&li[..], &ri[..]]).map_err(flac_err)
    })?;
    check_rendered(&out)?;
    w.finalize().map_err(flac_err)?;
    // flushed here, not by the drop, so a full disk is an error and not a silently short file
    file.flush()?;
    Ok(out)
}

fn write_mp3(mix: &pm_formats::mix::Mix, res: &dyn Resolver, opts: &Options, enc: &Encoding, tmp: &Path, progress: &mut dyn FnMut(f64) -> bool) -> Result<Rendered, Error> {
    let lame = |e: mp3lame_encoder::BuildError| Error::Encode(format!("MP3: {e}"));
    let mut b = LameBuilder::new().ok_or_else(|| Error::Encode("MP3: the encoder could not be created".into()))?;
    b.set_num_channels(2).map_err(lame)?;
    b.set_sample_rate(RATE).map_err(lame)?;
    b.set_mode(LameMode::JointStereo).map_err(lame)?;
    b.set_quality(LameQuality::NearBest).map_err(lame)?;
    match enc.mp3 {
        Mp3Rate::Cbr(k) => {
            b.set_vbr_mode(VbrMode::Off).map_err(lame)?;
            b.set_brate(match k {
                96 => Bitrate::Kbps96,
                112 => Bitrate::Kbps112,
                128 => Bitrate::Kbps128,
                160 => Bitrate::Kbps160,
                192 => Bitrate::Kbps192,
                224 => Bitrate::Kbps224,
                256 => Bitrate::Kbps256,
                _ => Bitrate::Kbps320,
            })
            .map_err(lame)?;
        }
        Mp3Rate::Vbr(q) => {
            b.set_vbr_mode(VbrMode::Mtrh).map_err(lame)?;
            b.set_vbr_quality(match q {
                0 => LameQuality::Best,
                1 => LameQuality::SecondBest,
                2 => LameQuality::NearBest,
                3 => LameQuality::VeryNice,
                4 => LameQuality::Nice,
                5 => LameQuality::Good,
                6 => LameQuality::Decent,
                7 => LameQuality::Ok,
                8 => LameQuality::SecondWorst,
                _ => LameQuality::Worst,
            })
            .map_err(lame)?;
        }
    }
    // the Xing / Info frame carries the length and the encoder delay, so players seek and play
    // gaplessly; LAME reserves its place at the start of the stream and it is filled in at the end
    b.set_to_write_vbr_tag(true).map_err(lame)?;
    let mut lame_enc = b.build().map_err(lame)?;

    let mut file = BufWriter::new(File::create(tmp)?);
    let id3 = enc.tags.as_ref().map(tags::id3v23).unwrap_or_default();
    file.write_all(&id3)?;
    let mut buf: Vec<u8> = Vec::new();
    let out = engine::render(mix, res, opts, progress, &mut |l, r| {
        buf.clear();
        buf.reserve(mp3lame_encoder::max_required_buffer_size(l.len()));
        lame_enc.encode_to_vec(DualPcm { left: l, right: r }, &mut buf).map_err(|e| Error::Encode(format!("MP3: {e}")))?;
        file.write_all(&buf)?;
        Ok(())
    })?;
    check_rendered(&out)?;
    buf.clear();
    buf.reserve(7200);
    lame_enc.flush_to_vec::<FlushGap>(&mut buf).map_err(|e| Error::Encode(format!("MP3: {e}")))?;
    file.write_all(&buf)?;
    buf.clear();
    buf.reserve(lame_enc.lame_tag_size().max(1));
    if lame_enc.lame_tag_encode_to_vec(&mut buf).is_some() {
        file.flush()?;
        let f = file.get_mut();
        f.seek(SeekFrom::Start(id3.len() as u64))?;
        f.write_all(&buf)?;
    }
    file.flush()?;
    Ok(out)
}

/// [`render_to_file`] with an [`Encoding`]: the format's settings and the tags to write.
pub fn render_to_file_with(
    mix: &pm_formats::mix::Mix,
    res: &dyn Resolver,
    opts: &Options,
    enc: &Encoding,
    dest: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<Rendered, Error> {
    enc.validate()?;
    let tmp = partial(dest);
    let result = (|| -> Result<Rendered, Error> {
        match enc.format {
            Format::Wav => {
                let extra = enc.tags.as_ref().map(tags::wav_chunks).unwrap_or_default();
                let mut w = WavWriter::create(&tmp, extra)?;
                let out = engine::render(mix, res, opts, progress, &mut |l, r| w.write(l, r))?;
                w.finish()?;
                check_rendered(&out)?;
                Ok(out)
            }
            Format::Ogg => {
                let file = BufWriter::new(File::create(&tmp)?);
                let enc_err = |e: vorbis_rs::VorbisError| Error::Encode(e.to_string());
                let mut b = VorbisEncoderBuilder::new_with_serial(NonZeroU32::new(RATE).unwrap(), NonZeroU8::new(2).unwrap(), file, 0x5043_4D58);
                b.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr { target_quality: 0.6 });
                match &enc.tags {
                    Some(t) => {
                        for (k, v) in tags::vorbis_fields(t) {
                            b.comment_tag(&k, v.as_str()).map_err(enc_err)?;
                        }
                        if let Some(c) = &t.cover {
                            b.comment_tag("METADATA_BLOCK_PICTURE", tags::ogg_picture_field(c).as_str()).map_err(enc_err)?;
                        }
                    }
                    None => {
                        b.comment_tag("TITLE", mix.name.as_str()).map_err(enc_err)?;
                        b.comment_tag("ENCODER", tags::SOFTWARE).map_err(enc_err)?;
                    }
                }
                let mut venc: VorbisEncoder<BufWriter<File>> = b.build().map_err(enc_err)?;
                let out = engine::render(mix, res, opts, progress, &mut |l, r| venc.encode_audio_block([l, r]).map_err(enc_err))?;
                let mut file = venc.finish().map_err(enc_err)?;
                file.flush()?;
                check_rendered(&out)?;
                Ok(out)
            }
            Format::Flac => write_flac(mix, res, opts, enc, &tmp, progress),
            Format::Mp3 => write_mp3(mix, res, opts, enc, &tmp, progress),
        }
    })();
    match result {
        Ok(r) => {
            // rename replaces an existing file on Windows and Unix, so the old export survives
            // until the new one is in place
            if let Err(e) = std::fs::rename(&tmp, dest) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
            Ok(r)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
