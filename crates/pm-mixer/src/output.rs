//! File writers: 16-bit PCM WAV and Ogg Vorbis.
//!
//! Both write to `<dest>.partial` and rename on success, so a cancelled or failed export never
//! leaves a half file under the final name.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::num::{NonZeroU32, NonZeroU8};
use std::path::{Path, PathBuf};

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoder, VorbisEncoderBuilder};

use crate::engine::{self, Options, Rendered, Resolver};
use crate::{Error, RATE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Ogg,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Ogg => "ogg",
        }
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
}

fn header(frames: u64) -> Result<[u8; 44], Error> {
    let data = frames * 4;
    if data + 36 > u64::from(u32::MAX) {
        return Err(Error::Invalid("the mix is too long for a WAV file (over 6 hours)".into()));
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

impl WavWriter {
    fn create(path: &Path) -> Result<WavWriter, Error> {
        let mut w = BufWriter::new(File::create(path)?);
        w.write_all(&header(0)?)?;
        Ok(WavWriter { w, frames: 0, dl: Dither(0x1234_5678), dr: Dither(0x9ABC_DEF1) })
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
        self.w.flush()?;
        let f = self.w.get_mut();
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&header(self.frames)?)?;
        f.flush()?;
        Ok(())
    }
}

fn partial(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

/// Renders the mix to `dest` (overwriting it) in `format`.
pub fn render_to_file(
    mix: &pm_formats::mix::Mix,
    res: &dyn Resolver,
    opts: &Options,
    format: Format,
    dest: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<Rendered, Error> {
    let tmp = partial(dest);
    let result = (|| -> Result<Rendered, Error> {
        match format {
            Format::Wav => {
                let mut w = WavWriter::create(&tmp)?;
                let out = engine::render(mix, res, opts, progress, &mut |l, r| w.write(l, r))?;
                w.finish()?;
                Ok(out)
            }
            Format::Ogg => {
                let file = BufWriter::new(File::create(&tmp)?);
                let enc_err = |e: vorbis_rs::VorbisError| Error::Encode(e.to_string());
                let mut b = VorbisEncoderBuilder::new_with_serial(NonZeroU32::new(RATE).unwrap(), NonZeroU8::new(2).unwrap(), file, 0x5043_4D58);
                b.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr { target_quality: 0.6 });
                b.comment_tag("TITLE", mix.name.as_str()).map_err(enc_err)?;
                b.comment_tag("ENCODER", "Pacemaker Revival").map_err(enc_err)?;
                let mut enc: VorbisEncoder<BufWriter<File>> = b.build().map_err(enc_err)?;
                let out = engine::render(mix, res, opts, progress, &mut |l, r| enc.encode_audio_block([l, r]).map_err(enc_err))?;
                let mut file = enc.finish().map_err(enc_err)?;
                file.flush()?;
                Ok(out)
            }
        }
    })();
    match result {
        Ok(r) => {
            if dest.exists() {
                std::fs::remove_file(dest)?;
            }
            std::fs::rename(&tmp, dest)?;
            Ok(r)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
