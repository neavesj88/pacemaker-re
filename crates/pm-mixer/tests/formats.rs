//! FLAC and MP3 export (and tags in every format), on a synthetic sine written as a float WAV into
//! a temp directory. The files are checked by reading the container bytes directly and by decoding
//! them again with symphonia (`pm_mixer::audio::load`), so what a player would see is what is
//! asserted.

use std::path::{Path, PathBuf};

use pm_formats::mix::{Mix, Position, Track};
use pm_mixer::tags::{Chapter, Cover, Tags};
use pm_mixer::{engine, Encoding, FlacLevel, Format, Mp3Rate, Mp3Stereo, Options, RATE};

const SR: usize = RATE as usize;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pm-mixer-formats-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put_sine(d: &Path, name: &str, freq: f32, secs: f32, amp: f32) -> usize {
    let n = (secs * SR as f32) as usize;
    let data = (n * 8) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&(SR as u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for i in 0..n {
        let v = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin();
        b.extend_from_slice(&v.to_le_bytes());
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(d.join(name), b).unwrap();
    n
}

fn mix_of(file: &str, frames: usize) -> Mix {
    let pos = Position { start: 0.0, length: frames as f64, limit_size: true, ..Position::default() };
    let mut m = Mix::new("Fixture Mix");
    m.tracks.push(Track::new(1, file, 120.0, 1.0, pos));
    m
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

fn crossing_freq(v: &[f32]) -> f64 {
    let mut crossings = 0usize;
    for w in v.windows(2) {
        if w[0] <= 0.0 && w[1] > 0.0 {
            crossings += 1;
        }
    }
    crossings as f64 * SR as f64 / v.len() as f64
}

fn tags() -> Tags {
    Tags {
        title: "Fixture Mix".into(),
        artist: "DJ Example".into(),
        album: "Fixture Sessions".into(),
        year: "2026".into(),
        comment: "made by a test".into(),
        cover: None,
        chapters: vec![Chapter { start_ms: 0, end_ms: 2000, title: "First".into() }, Chapter { start_ms: 2000, end_ms: 4000, title: "Second".into() }],
    }
}

fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 0x90, 0x77, 0x53, 0xde, 0, 0, 0, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7,
        0x63, 0xf8, 0xcf, 0xc0, 0, 0, 0x03, 0x01, 0x01, 0, 0x18, 0xdd, 0x8d, 0xb0, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}

fn render(d: &Path, m: &Mix, enc: &Encoding, name: &str) -> (PathBuf, pm_mixer::Rendered) {
    let dest = d.join(name);
    let r = pm_mixer::render_to_file_with(m, &engine::dir_resolver(d), &Options::default(), enc, &dest, &mut |_| true).unwrap();
    assert!(!d.join(format!("{name}.partial")).exists(), "no temp file is left");
    (dest, r)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// The decoded audio must be the 440 Hz sine at about the level that went in.
fn check_audio(path: &Path, src_frames: usize, amp: f32, tol_frames: i64, name: &str) {
    let back = pm_mixer::audio::load(path).unwrap();
    assert!((back.len() as i64 - src_frames as i64).abs() <= tol_frames, "{name}: {} frames, expected {src_frames}", back.len());
    let mid = &back.l[SR..back.len() - SR];
    assert!((crossing_freq(mid) - 440.0).abs() < 1.5, "{name}: {} Hz", crossing_freq(mid));
    let want = f64::from(amp) / 2f64.sqrt();
    assert!((rms(mid) - want).abs() < 0.02, "{name}: rms {} vs {want}", rms(mid));
}

#[test]
fn flac_is_lossless_at_16_and_24_bit_with_a_correct_streaminfo() {
    let d = dir("flac");
    let n = put_sine(&d, "a.wav", 440.0, 5.0, 0.5);
    let m = mix_of("a.wav", n);
    for bits in [16u8, 24] {
        let enc = Encoding { flac_bits: bits, ..Encoding::new(Format::Flac) };
        let (dest, r) = render(&d, &m, &enc, &format!("out{bits}.flac"));
        assert_eq!(r.frames as usize, n);
        let bytes = std::fs::read(&dest).unwrap();
        assert_eq!(&bytes[..4], b"fLaC");
        // STREAMINFO is the first block: type 0, 34 bytes; sample rate (20 bits), channels-1 (3),
        // bits-1 (5), total samples (36) follow the block sizes and frame sizes
        assert_eq!(bytes[4] & 0x7F, 0, "first block is STREAMINFO");
        let si = &bytes[8..8 + 34];
        let packed = u64::from_be_bytes(si[10..18].try_into().unwrap());
        assert_eq!(packed >> 44, u64::from(RATE), "sample rate");
        assert_eq!((packed >> 41) & 7, 1, "two channels");
        assert_eq!(((packed >> 36) & 31) + 1, u64::from(bits), "bits per sample");
        assert_eq!(packed & ((1 << 36) - 1), n as u64, "total samples were filled in at the end");
        assert!(si[18..34].iter().any(|b| *b != 0), "the MD5 of the audio was filled in");
        // decodes back to exactly the right length (lossless: no priming frames)
        check_audio(&dest, n, 0.5, 0, &format!("flac{bits}"));
    }
    let wav = std::fs::metadata(render(&d, &m, &Encoding::new(Format::Wav), "ref.wav").0).unwrap().len();
    let flac = std::fs::metadata(d.join("out16.flac")).unwrap().len();
    assert!(flac < wav / 2, "a sine compresses well: {flac} vs {wav}");
}

#[test]
fn flac_levels_give_the_same_audio_and_best_is_not_larger_than_fast() {
    let d = dir("flac-levels");
    let n = put_sine(&d, "a.wav", 440.0, 3.0, 0.5);
    let m = mix_of("a.wav", n);
    let mut sizes = Vec::new();
    let mut audio = Vec::new();
    for level in [FlacLevel::Fast, FlacLevel::Standard, FlacLevel::Best] {
        let enc = Encoding { flac_level: level, ..Encoding::new(Format::Flac) };
        let (dest, _) = render(&d, &m, &enc, &format!("{level:?}.flac"));
        sizes.push(std::fs::metadata(&dest).unwrap().len());
        audio.push(pm_mixer::audio::load(&dest).unwrap().l.clone());
    }
    assert!(audio[0] == audio[1] && audio[1] == audio[2], "the audio is identical at every level");
    assert!(sizes[2] <= sizes[0], "{sizes:?}");
}

#[test]
fn flac_carries_the_tags_the_chapters_and_the_cover() {
    let d = dir("flac-tags");
    let n = put_sine(&d, "a.wav", 440.0, 4.0, 0.5);
    let m = mix_of("a.wav", n);
    let t = Tags { cover: Some(Cover::from_bytes(png()).unwrap()), ..tags() };
    let (dest, _) = render(&d, &m, &Encoding { tags: Some(t), ..Encoding::new(Format::Flac) }, "t.flac");
    let b = std::fs::read(&dest).unwrap();
    // walk the metadata blocks
    let (mut p, mut types) = (4usize, Vec::new());
    loop {
        let h = b[p];
        let len = (usize::from(b[p + 1]) << 16) | (usize::from(b[p + 2]) << 8) | usize::from(b[p + 3]);
        types.push(h & 0x7F);
        p += 4 + len;
        if h & 0x80 != 0 {
            break;
        }
    }
    assert!(types.contains(&4), "VORBIS_COMMENT block: {types:?}");
    assert!(types.contains(&6), "PICTURE block: {types:?}");
    for want in ["TITLE=Fixture Mix", "ARTIST=DJ Example", "ALBUM=Fixture Sessions", "DATE=2026", "COMMENT=made by a test", "CHAPTER001=00:00:00.000", "CHAPTER002=00:00:02.000", "CHAPTER002NAME=Second"] {
        assert!(contains(&b[..p], want.as_bytes()), "missing {want}");
    }
    assert!(contains(&b[..p], b"image/png"));
    check_audio(&dest, n, 0.5, 0, "flac tags");
}

#[test]
fn mp3_cbr_has_the_requested_bitrate_a_patched_info_frame_and_the_id3_tag() {
    let d = dir("mp3-cbr");
    let n = put_sine(&d, "a.wav", 440.0, 6.0, 0.5);
    let m = mix_of("a.wav", n);
    let t = Tags { cover: Some(Cover::from_bytes(png()).unwrap()), ..tags() };
    let enc = Encoding { mp3: Mp3Rate::Cbr(128), tags: Some(t.clone()), ..Encoding::new(Format::Mp3) };
    let (dest, r) = render(&d, &m, &enc, "o.mp3");
    assert_eq!(r.frames as usize, n);
    let b = std::fs::read(&dest).unwrap();
    assert_eq!(&b[..3], b"ID3");
    let id3_len = 10 + b[6..10].iter().fold(0usize, |a, &x| (a << 7) | x as usize);
    assert_eq!(b[id3_len], 0xFF, "an MPEG frame follows the tag directly");
    assert_eq!(id3_len, pm_mixer::tags::id3v23(&t).len());
    for want in ["TIT2", "TPE1", "TALB", "COMM", "APIC", "CTOC", "CHAP", "Fixture Mix", "DJ Example"] {
        assert!(contains(&b[..id3_len], want.as_bytes()), "missing {want}");
    }
    // the first frame is LAME's Info frame (CBR) and was filled in: it names the encoder and the
    // frame count is non-zero
    let first = &b[id3_len..id3_len + 200];
    assert!(contains(first, b"Info") || contains(first, b"Xing"), "Info/Xing frame at the start");
    assert!(contains(first, b"LAME"), "the LAME tag was patched in");
    // MPEG-1 Layer III header: 128 kbit/s is index 9 (1001), 44.1 kHz is 00
    let h = &b[id3_len..id3_len + 4];
    assert_eq!(h[0], 0xFF);
    assert_eq!(h[1] & 0xFE, 0xFA, "MPEG-1 Layer III");
    assert_eq!(h[2] >> 4, 0b1001, "128 kbit/s");
    assert_eq!((h[2] >> 2) & 3, 0, "44.1 kHz");
    // no ID3v1 tag at the end
    assert_ne!(&b[b.len() - 128..b.len() - 125], b"TAG");
    // CBR size is the bitrate times the length, within a frame or two of rounding
    let secs = n as f64 / SR as f64;
    let want = 128_000.0 / 8.0 * secs;
    let got = (b.len() - id3_len) as f64;
    assert!((got - want).abs() < want * 0.03, "{got} vs {want}");
    // decodes to the same sine (the decoder trims the LAME delay when it reads the Info frame)
    check_audio(&dest, n, 0.5, 2400, "mp3 cbr");
}

#[test]
fn mp3_vbr_quality_changes_the_size_and_writes_a_xing_frame() {
    let d = dir("mp3-vbr");
    // a noisy-ish signal so quality matters: three tones
    let n = put_sine(&d, "a.wav", 440.0, 6.0, 0.4);
    let m = mix_of("a.wav", n);
    let mut sizes = Vec::new();
    for q in [0u8, 9] {
        let enc = Encoding { mp3: Mp3Rate::Vbr(q), ..Encoding::new(Format::Mp3) };
        let (dest, _) = render(&d, &m, &enc, &format!("v{q}.mp3"));
        let b = std::fs::read(&dest).unwrap();
        assert!(contains(&b[..400], b"Xing"), "VBR files start with a Xing frame (V{q})");
        sizes.push(b.len());
        check_audio(&dest, n, 0.4, 2400, &format!("mp3 V{q}"));
    }
    assert!(sizes[0] > sizes[1], "V0 is larger than V9: {sizes:?}");
}

#[test]
fn mp3_stereo_mode_and_effort_are_settings_and_every_combination_decodes() {
    let d = dir("mp3-opts");
    let n = put_sine(&d, "a.wav", 440.0, 3.0, 0.5);
    let m = mix_of("a.wav", n);
    for (stereo, effort) in [(Mp3Stereo::Joint, 2u8), (Mp3Stereo::Stereo, 2), (Mp3Stereo::Joint, 9), (Mp3Stereo::Stereo, 0)] {
        let enc = Encoding { mp3: Mp3Rate::Cbr(192), mp3_stereo: stereo, mp3_effort: effort, ..Encoding::new(Format::Mp3) };
        let (dest, _) = render(&d, &m, &enc, &format!("o-{stereo:?}-{effort}.mp3"));
        check_audio(&dest, n, 0.5, 2400, &format!("mp3 {stereo:?} effort {effort}"));
    }
    // the first frame header says which mode was written: bits 7-6 of byte 3 (00 stereo, 01 joint)
    let mode_of = |name: &str| {
        let b = std::fs::read(d.join(name)).unwrap();
        b[3] >> 6
    };
    assert_eq!(mode_of("o-Joint-2.mp3"), 1, "joint stereo");
    assert_eq!(mode_of("o-Stereo-2.mp3"), 0, "plain stereo");
}

#[test]
fn ogg_quality_is_a_setting_and_higher_is_larger() {
    let d = dir("ogg-quality");
    // several tones at once so the encoder has something to spend bits on
    let n = put_sine(&d, "a.wav", 440.0, 6.0, 0.4);
    let m = mix_of("a.wav", n);
    let mut sizes = Vec::new();
    for q in [-1i8, 10] {
        let enc = Encoding { ogg_quality: q, ..Encoding::new(Format::Ogg) };
        let (dest, _) = render(&d, &m, &enc, &format!("q{q}.ogg"));
        sizes.push(std::fs::metadata(&dest).unwrap().len());
    }
    assert!(sizes[0] < sizes[1], "quality -1 is smaller than quality 10: {sizes:?}");
}

#[test]
fn mp3_without_tags_has_no_id3_header_and_bad_settings_are_refused_before_any_file() {
    let d = dir("mp3-plain");
    let n = put_sine(&d, "a.wav", 440.0, 3.0, 0.5);
    let m = mix_of("a.wav", n);
    let (dest, _) = render(&d, &m, &Encoding::new(Format::Mp3), "plain.mp3");
    let b = std::fs::read(&dest).unwrap();
    assert_eq!(b[0], 0xFF, "starts with a frame, no ID3 tag");
    for enc in [
        Encoding { mp3: Mp3Rate::Cbr(100), ..Encoding::new(Format::Mp3) },
        Encoding { mp3: Mp3Rate::Vbr(10), ..Encoding::new(Format::Mp3) },
        Encoding { flac_bits: 20, ..Encoding::new(Format::Flac) },
        Encoding { mp3_effort: 10, ..Encoding::new(Format::Mp3) },
        Encoding { ogg_quality: 11, ..Encoding::new(Format::Ogg) },
        Encoding { ogg_quality: -2, ..Encoding::new(Format::Ogg) },
    ] {
        let dest = d.join("bad.out");
        let e = pm_mixer::render_to_file_with(&m, &engine::dir_resolver(&d), &Options::default(), &enc, &dest, &mut |_| true);
        assert!(matches!(e, Err(pm_mixer::Error::Invalid(_))));
        assert!(!dest.exists() && !d.join("bad.out.partial").exists());
    }
}

#[test]
fn cancelling_leaves_no_file_in_any_format() {
    let d = dir("cancel");
    let n = put_sine(&d, "a.wav", 440.0, 5.0, 0.5);
    let m = mix_of("a.wav", n);
    for f in [Format::Wav, Format::Ogg, Format::Flac, Format::Mp3] {
        let dest = d.join(format!("c.{}", f.extension()));
        let e = pm_mixer::render_to_file_with(&m, &engine::dir_resolver(&d), &Options::default(), &Encoding::new(f), &dest, &mut |p| p < 0.3);
        assert!(matches!(e, Err(pm_mixer::Error::Cancelled)), "{f:?}");
        assert!(!dest.exists() && !d.join(format!("c.{}.partial", f.extension())).exists(), "{f:?}");
    }
}

#[test]
fn an_existing_file_survives_a_cancelled_export() {
    let d = dir("keep");
    let n = put_sine(&d, "a.wav", 440.0, 4.0, 0.5);
    let m = mix_of("a.wav", n);
    let dest = d.join("keep.flac");
    std::fs::write(&dest, b"previous export").unwrap();
    let e = pm_mixer::render_to_file_with(&m, &engine::dir_resolver(&d), &Options::default(), &Encoding::new(Format::Flac), &dest, &mut |p| p < 0.3);
    assert!(matches!(e, Err(pm_mixer::Error::Cancelled)));
    assert_eq!(std::fs::read(&dest).unwrap(), b"previous export");
}

#[test]
fn ogg_tags_chapters_and_cover_are_in_the_comment_header() {
    let d = dir("ogg");
    let n = put_sine(&d, "a.wav", 440.0, 3.0, 0.5);
    let m = mix_of("a.wav", n);
    let t = Tags { cover: Some(Cover::from_bytes(png()).unwrap()), ..tags() };
    let (dest, _) = render(&d, &m, &Encoding { tags: Some(t.clone()), ..Encoding::new(Format::Ogg) }, "t.ogg");
    let b = std::fs::read(&dest).unwrap();
    let head = &b[..b.len().min(8000)];
    for want in ["TITLE=Fixture Mix", "ARTIST=DJ Example", "ALBUM=Fixture Sessions", "DATE=2026", "CHAPTER002NAME=Second", "ENCODER=Pacemaker Revival"] {
        assert!(contains(head, want.as_bytes()), "missing {want}");
    }
    let pic = format!("METADATA_BLOCK_PICTURE={}", pm_mixer::tags::ogg_picture_field(t.cover.as_ref().unwrap()));
    assert!(contains(head, pic.as_bytes()), "picture field");
    // no tags: the old behaviour, the mix name as title
    let (plain, _) = render(&d, &m, &Encoding::new(Format::Ogg), "p.ogg");
    let pb = std::fs::read(plain).unwrap();
    assert!(contains(&pb[..pb.len().min(8000)], b"TITLE=Fixture Mix"));
}

#[test]
fn wav_tags_are_riff_chunks_after_the_audio_and_the_header_sizes_cover_them() {
    let d = dir("wav");
    let n = put_sine(&d, "a.wav", 440.0, 3.0, 0.5);
    let m = mix_of("a.wav", n);
    let (dest, _) = render(&d, &m, &Encoding { tags: Some(tags()), ..Encoding::new(Format::Wav) }, "t.wav");
    let b = std::fs::read(&dest).unwrap();
    let riff = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
    assert_eq!(riff + 8, b.len(), "the RIFF size counts the appended chunks");
    let data = u32::from_le_bytes(b[40..44].try_into().unwrap()) as usize;
    assert_eq!(data, n * 4);
    let tail = &b[44 + data..];
    assert_eq!(&tail[..4], b"LIST");
    for want in ["INFO", "INAM", "IART", "IPRD", "Fixture Mix", "DJ Example", "cue ", "adtl", "labl", "Second"] {
        assert!(contains(tail, want.as_bytes()), "missing {want}");
    }
    // still decodes as audio of the right length
    check_audio(&dest, n, 0.5, 0, "wav tags");
    // untagged WAV is byte-for-byte what it was: 44-byte header and the data
    let (plain, _) = render(&d, &m, &Encoding::new(Format::Wav), "p.wav");
    assert_eq!(std::fs::metadata(plain).unwrap().len() as usize, 44 + n * 4);
}
