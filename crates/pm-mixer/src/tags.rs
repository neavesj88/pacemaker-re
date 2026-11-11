//! Tags written into exported audio: title, artist, album, year, comment, cover image and the
//! chapters (the track list of the mix). Each format gets what it can hold:
//!
//! | format | text tags | cover | chapters |
//! | --- | --- | --- | --- |
//! | MP3 | ID3v2.3 `TIT2 TPE1 TALB TYER COMM TSSE` | `APIC` | `CTOC` + `CHAP` frames |
//! | FLAC | Vorbis comment block | `PICTURE` block | `CHAPTERnnn` / `CHAPTERnnnNAME` comments |
//! | Ogg Vorbis | Vorbis comment header | `METADATA_BLOCK_PICTURE` comment | same `CHAPTERnnn` comments |
//! | WAV | `LIST`/`INFO` chunk | none | `cue ` chunk + `LIST`/`adtl` `labl` labels |
//!
//! Everything here is pure byte building, so it is tested without any encoder.

use crate::Error;

/// The largest cover image taken: tags are read whole by players, a huge picture would make every
/// file slow to open.
pub const MAX_COVER_BYTES: usize = 8 * 1024 * 1024;

/// A cover image: JPEG or PNG, as the file's own bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    /// `image/jpeg` or `image/png`.
    pub mime: String,
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Bits per pixel.
    pub depth: u32,
}

impl Cover {
    /// Takes the bytes of a JPEG or PNG file. Anything else (GIF, WebP, a renamed text file) and
    /// anything over [`MAX_COVER_BYTES`] is refused with a sentence for the user.
    pub fn from_bytes(data: Vec<u8>) -> Result<Cover, Error> {
        if data.len() > MAX_COVER_BYTES {
            return Err(Error::Invalid(format!("The cover image is {} MB; the most a tag takes is {} MB. Use a smaller image.", data.len().div_ceil(1 << 20), MAX_COVER_BYTES >> 20)));
        }
        use flac_codec::metadata::{Picture, PictureType};
        let pic = Picture::new(PictureType::FrontCover, "", data).map_err(|e| Error::Invalid(format!("The cover image cannot be read ({e}). Use a JPEG or PNG file.")))?;
        if pic.media_type != "image/jpeg" && pic.media_type != "image/png" {
            return Err(Error::Invalid(format!("The cover image is {}; use a JPEG or PNG file.", pic.media_type)));
        }
        Ok(Cover { mime: pic.media_type, data: pic.data, width: pic.width, height: pic.height, depth: pic.color_depth })
    }
}

/// One chapter: a stretch of the exported audio with a name (a track of the mix).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub start_ms: u32,
    pub end_ms: u32,
    pub title: String,
}

/// What an export writes into the file's tags. Empty text is left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub comment: String,
    pub cover: Option<Cover>,
    pub chapters: Vec<Chapter>,
}

/// The tool named in the file (`TSSE` / `ENCODER` / `ISFT`).
pub const SOFTWARE: &str = "Pacemaker Revival";

impl Tags {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty() && self.artist.is_empty() && self.album.is_empty() && self.year.is_empty() && self.comment.is_empty() && self.cover.is_none() && self.chapters.is_empty()
    }

    /// Text fields with control characters (line breaks, NULs) replaced by spaces, except in the
    /// comment, where line breaks stay. A tag with a stray NUL would end early in some readers.
    pub fn cleaned(&self) -> Tags {
        let one = |s: &str| s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string();
        let multi = |s: &str| s.chars().filter(|&c| c == '\n' || !c.is_control()).collect::<String>().trim().to_string();
        Tags {
            title: one(&self.title),
            artist: one(&self.artist),
            album: one(&self.album),
            year: one(&self.year),
            comment: multi(&self.comment),
            cover: self.cover.clone(),
            chapters: self.chapters.iter().map(|c| Chapter { title: one(&c.title), ..c.clone() }).collect(),
        }
    }
}

/// Milliseconds of `frames` at the engine rate, rounded to nearest.
pub fn ms_of_frames(frames: i64) -> u32 {
    let ms = (frames.max(0) as f64 * 1000.0 / f64::from(crate::RATE)).round();
    ms.min(f64::from(u32::MAX)) as u32
}

// ---------------------------------------------------------------------------------------------
// ID3v2.3 (MP3)
// ---------------------------------------------------------------------------------------------

fn syncsafe(n: u32) -> [u8; 4] {
    [((n >> 21) & 0x7F) as u8, ((n >> 14) & 0x7F) as u8, ((n >> 7) & 0x7F) as u8, (n & 0x7F) as u8]
}

/// An ID3v2.3 text string with its encoding byte: ISO-8859-1 when every character fits, else
/// UTF-16 with a byte-order mark (the two encodings v2.3 has). `terminated` adds the end marker
/// (needed where a text is followed by more fields).
fn id3_text(s: &str, terminated: bool) -> Vec<u8> {
    if s.chars().all(|c| (c as u32) < 0x100) {
        let mut v = vec![0u8];
        v.extend(s.chars().map(|c| c as u8));
        if terminated {
            v.push(0);
        }
        v
    } else {
        let mut v = vec![1u8, 0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        if terminated {
            v.extend_from_slice(&[0, 0]);
        }
        v
    }
}

fn id3_frame(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(10 + body.len());
    f.extend_from_slice(id);
    f.extend_from_slice(&(body.len() as u32).to_be_bytes());
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(body);
    f
}

/// Latin-1 text of a frame id such as a CHAP element id, NUL-terminated.
fn latin1_z(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }).collect();
    v.push(0);
    v
}

/// The ID3v2.3 tag for `tags` (an empty vector when there is nothing to write). Chapters become one
/// `CHAP` frame each plus a top-level ordered `CTOC`; the cover is an `APIC` frame (front cover).
pub fn id3v23(tags: &Tags) -> Vec<u8> {
    let t = tags.cleaned();
    if t.is_empty() {
        return Vec::new();
    }
    let mut frames: Vec<u8> = Vec::new();
    let mut text = |id: &[u8; 4], s: &str| {
        if !s.is_empty() {
            frames.extend(id3_frame(id, &id3_text(s, false)));
        }
    };
    text(b"TIT2", &t.title);
    text(b"TPE1", &t.artist);
    text(b"TALB", &t.album);
    text(b"TYER", &t.year);
    text(b"TSSE", SOFTWARE);
    if !t.comment.is_empty() {
        // encoding, language, short description (empty, terminated), text
        let enc = id3_text(&t.comment, false);
        let mut body = vec![enc[0]];
        body.extend_from_slice(b"eng");
        if enc[0] == 0 {
            body.push(0);
        } else {
            body.extend_from_slice(&[0xFF, 0xFE, 0, 0]);
        }
        body.extend_from_slice(&enc[1..]);
        frames.extend(id3_frame(b"COMM", &body));
    }
    if let Some(c) = &t.cover {
        let mut body = vec![0u8];
        body.extend(latin1_z(&c.mime));
        body.push(3); // front cover
        body.push(0); // empty description
        body.extend_from_slice(&c.data);
        frames.extend(id3_frame(b"APIC", &body));
    }
    if !t.chapters.is_empty() {
        let ids: Vec<String> = (1..=t.chapters.len()).map(|i| format!("chp{i}")).collect();
        let mut toc = latin1_z("toc");
        toc.push(0b11); // top level, ordered
        toc.push(ids.len().min(255) as u8);
        for id in ids.iter().take(255) {
            toc.extend(latin1_z(id));
        }
        frames.extend(id3_frame(b"CTOC", &toc));
        for (c, id) in t.chapters.iter().zip(&ids) {
            let mut body = latin1_z(id);
            body.extend_from_slice(&c.start_ms.to_be_bytes());
            body.extend_from_slice(&c.end_ms.max(c.start_ms).to_be_bytes());
            body.extend_from_slice(&[0xFF; 8]); // no byte offsets
            body.extend(id3_frame(b"TIT2", &id3_text(&c.title, false)));
            frames.extend(id3_frame(b"CHAP", &body));
        }
    }
    if frames.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(10 + frames.len());
    out.extend_from_slice(b"ID3");
    out.extend_from_slice(&[3, 0, 0]);
    out.extend_from_slice(&syncsafe(frames.len() as u32));
    out.extend(frames);
    out
}

// ---------------------------------------------------------------------------------------------
// Vorbis comments (FLAC, Ogg Vorbis)
// ---------------------------------------------------------------------------------------------

/// `hh:mm:ss.mmm`, the form the `CHAPTERnnn` convention uses.
pub fn chapter_stamp(ms: u32) -> String {
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// The Vorbis comment fields for `tags`, in write order. Chapters follow the widespread
/// `CHAPTER001=00:00:00.000` / `CHAPTER001NAME=...` convention (read by ffmpeg, mkvtoolnix and
/// most chapter-aware players; plain players ignore the fields). The cover is not here: FLAC takes
/// a `PICTURE` block, Ogg a `METADATA_BLOCK_PICTURE` field ([`ogg_picture_field`]).
pub fn vorbis_fields(tags: &Tags) -> Vec<(String, String)> {
    let t = tags.cleaned();
    let mut f: Vec<(String, String)> = Vec::new();
    for (k, v) in [("TITLE", &t.title), ("ARTIST", &t.artist), ("ALBUM", &t.album), ("DATE", &t.year), ("COMMENT", &t.comment)] {
        if !v.is_empty() {
            f.push((k.to_string(), v.clone()));
        }
    }
    f.push(("ENCODER".into(), SOFTWARE.into()));
    for (i, c) in t.chapters.iter().enumerate() {
        f.push((format!("CHAPTER{:03}", i + 1), chapter_stamp(c.start_ms)));
        f.push((format!("CHAPTER{:03}NAME", i + 1), c.title.clone()));
    }
    f
}

/// The body of a FLAC `PICTURE` metadata block (also the payload of Ogg's picture field).
pub fn flac_picture_body(c: &Cover) -> Vec<u8> {
    let mut b = Vec::with_capacity(32 + c.mime.len() + c.data.len());
    b.extend_from_slice(&3u32.to_be_bytes()); // front cover
    b.extend_from_slice(&(c.mime.len() as u32).to_be_bytes());
    b.extend_from_slice(c.mime.as_bytes());
    b.extend_from_slice(&0u32.to_be_bytes()); // description length
    b.extend_from_slice(&c.width.to_be_bytes());
    b.extend_from_slice(&c.height.to_be_bytes());
    b.extend_from_slice(&c.depth.to_be_bytes());
    b.extend_from_slice(&0u32.to_be_bytes()); // colours used (0 = not indexed)
    b.extend_from_slice(&(c.data.len() as u32).to_be_bytes());
    b.extend_from_slice(&c.data);
    b
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n = (u32::from(ch[0]) << 16) | (u32::from(*ch.get(1).unwrap_or(&0)) << 8) | u32::from(*ch.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if ch.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if ch.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// The `METADATA_BLOCK_PICTURE` value Ogg Vorbis files carry a cover in.
pub fn ogg_picture_field(c: &Cover) -> String {
    base64(&flac_picture_body(c))
}

// ---------------------------------------------------------------------------------------------
// WAV (RIFF) chunks
// ---------------------------------------------------------------------------------------------

fn riff_chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(8 + body.len() + 1);
    c.extend_from_slice(id);
    c.extend_from_slice(&(body.len() as u32).to_le_bytes());
    c.extend_from_slice(body);
    if body.len() % 2 == 1 {
        c.push(0);
    }
    c
}

/// A NUL-terminated string in the file's text, UTF-8 (the form modern tools read).
fn riff_text(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

/// The chunks appended after the `data` chunk of a WAV file: `LIST`/`INFO` text tags, then a
/// `cue ` chunk with one point per chapter and a `LIST`/`adtl` chunk with the chapter names as
/// `labl` labels. Empty when `tags` has nothing a WAV can hold (a cover cannot go in a WAV).
/// Cue point positions are sample frames into the audio.
pub fn wav_chunks(tags: &Tags) -> Vec<u8> {
    let t = tags.cleaned();
    let mut out = Vec::new();
    let mut info = b"INFO".to_vec();
    for (id, v) in [(b"INAM", &t.title), (b"IART", &t.artist), (b"IPRD", &t.album), (b"ICRD", &t.year), (b"ICMT", &t.comment)] {
        if !v.is_empty() {
            info.extend(riff_chunk(id, &riff_text(v)));
        }
    }
    if info.len() > 4 || !t.chapters.is_empty() {
        info.extend(riff_chunk(b"ISFT", &riff_text(SOFTWARE)));
        out.extend(riff_chunk(b"LIST", &info));
    }
    if !t.chapters.is_empty() {
        let mut cue = (t.chapters.len() as u32).to_le_bytes().to_vec();
        let mut adtl = b"adtl".to_vec();
        for (i, c) in t.chapters.iter().enumerate() {
            let id = (i + 1) as u32;
            let sample = (u64::from(c.start_ms) * u64::from(crate::RATE) / 1000) as u32;
            cue.extend_from_slice(&id.to_le_bytes());
            cue.extend_from_slice(&sample.to_le_bytes()); // position (play order)
            cue.extend_from_slice(b"data");
            cue.extend_from_slice(&0u32.to_le_bytes()); // chunk start
            cue.extend_from_slice(&0u32.to_le_bytes()); // block start
            cue.extend_from_slice(&sample.to_le_bytes()); // sample offset
            let mut label = id.to_le_bytes().to_vec();
            label.extend(riff_text(&c.title));
            adtl.extend(riff_chunk(b"labl", &label));
        }
        out.extend(riff_chunk(b"cue ", &cue));
        out.extend(riff_chunk(b"LIST", &adtl));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags() -> Tags {
        Tags {
            title: "Test Mix".into(),
            artist: "DJ Example".into(),
            album: "Fixture Sessions".into(),
            year: "2026".into(),
            comment: "line one\nline two".into(),
            cover: None,
            chapters: vec![Chapter { start_ms: 0, end_ms: 61_500, title: "Artist A - Song A".into() }, Chapter { start_ms: 61_500, end_ms: 120_000, title: "Song B".into() }],
        }
    }

    /// Reads the frames of an ID3v2.3 tag back: (id, body).
    fn frames(tag: &[u8]) -> Vec<(String, Vec<u8>)> {
        assert_eq!(&tag[..3], b"ID3");
        assert_eq!(&tag[3..6], &[3, 0, 0]);
        let size = tag[6..10].iter().fold(0usize, |a, &b| (a << 7) | b as usize);
        assert_eq!(tag.len(), 10 + size, "the header size covers exactly the frames");
        let mut out = Vec::new();
        let mut p = 10;
        while p < tag.len() {
            let id = String::from_utf8(tag[p..p + 4].to_vec()).unwrap();
            let n = u32::from_be_bytes(tag[p + 4..p + 8].try_into().unwrap()) as usize;
            out.push((id, tag[p + 10..p + 10 + n].to_vec()));
            p += 10 + n;
        }
        assert_eq!(p, tag.len());
        out
    }

    #[test]
    fn id3_has_the_text_frames_the_chapters_and_the_table_of_contents() {
        let f = frames(&id3v23(&tags()));
        let ids: Vec<&str> = f.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(ids, ["TIT2", "TPE1", "TALB", "TYER", "TSSE", "COMM", "CTOC", "CHAP", "CHAP"]);
        assert_eq!(f[0].1, [&[0u8][..], b"Test Mix"].concat(), "ASCII text is ISO-8859-1");
        let comm = &f[5].1;
        assert_eq!(&comm[..5], &[0, b'e', b'n', b'g', 0]);
        assert_eq!(&comm[5..], b"line one\nline two", "line breaks stay in the comment");
        // CTOC: id "toc", flags, count, child ids
        assert_eq!(f[6].1, [&b"toc\0"[..], &[3, 2], b"chp1\0", b"chp2\0"].concat());
        // CHAP 2: id, start, end, no offsets, then a TIT2 sub-frame
        let c = &f[8].1;
        assert_eq!(&c[..5], b"chp2\0");
        assert_eq!(u32::from_be_bytes(c[5..9].try_into().unwrap()), 61_500);
        assert_eq!(u32::from_be_bytes(c[9..13].try_into().unwrap()), 120_000);
        assert_eq!(&c[13..21], &[0xFF; 8]);
        assert_eq!(&c[21..25], b"TIT2");
        assert_eq!(&c[31..], [&[0u8][..], b"Song B"].concat());
    }

    #[test]
    fn non_latin1_text_is_utf16_with_a_byte_order_mark() {
        let t = Tags { title: "Mix \u{4e16}\u{754c}".into(), comment: "\u{4e16}".into(), ..Tags::default() };
        let f = frames(&id3v23(&t));
        let tit = &f[0].1;
        assert_eq!(&tit[..3], &[1, 0xFF, 0xFE]);
        let units: Vec<u16> = tit[3..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "Mix \u{4e16}\u{754c}");
        let comm = &f.iter().find(|(i, _)| i == "COMM").unwrap().1;
        assert_eq!(&comm[..10], &[1, b'e', b'n', b'g', 0xFF, 0xFE, 0, 0, 0xFF, 0xFE], "the empty description is a terminated UTF-16 string");
    }

    #[test]
    fn an_empty_tag_set_writes_nothing_and_control_characters_never_reach_the_file() {
        assert!(id3v23(&Tags::default()).is_empty());
        assert!(Tags::default().is_empty());
        assert!(wav_chunks(&Tags::default()).is_empty());
        let t = Tags { title: "a\0b\nc".into(), ..Tags::default() };
        let f = frames(&id3v23(&t));
        assert_eq!(f[0].1, [&[0u8][..], b"a b c"].concat());
        assert!(vorbis_fields(&t).contains(&("TITLE".into(), "a b c".into())));
    }

    #[test]
    fn the_cover_goes_in_as_a_front_cover_picture() {
        let c = Cover { mime: "image/png".into(), data: vec![1, 2, 3], width: 2, height: 2, depth: 24 };
        let t = Tags { cover: Some(c.clone()), ..Tags::default() };
        let f = frames(&id3v23(&t));
        let apic = &f.iter().find(|(i, _)| i == "APIC").unwrap().1;
        assert_eq!(apic, &[&[0u8][..], b"image/png\0", &[3, 0], &[1, 2, 3]].concat());
        let body = flac_picture_body(&c);
        assert_eq!(&body[..4], &3u32.to_be_bytes());
        assert_eq!(&body[body.len() - 7..], &[0, 0, 0, 3, 1, 2, 3], "length-prefixed data ends the block");
        assert_eq!(body.len(), 4 + 4 + 9 + 4 + 16 + 4 + 3);
    }

    #[test]
    fn vorbis_fields_carry_the_chapters_in_the_chapter_convention() {
        let f = vorbis_fields(&tags());
        let get = |k: &str| f.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("TITLE"), Some("Test Mix"));
        assert_eq!(get("ARTIST"), Some("DJ Example"));
        assert_eq!(get("DATE"), Some("2026"));
        assert_eq!(get("ENCODER"), Some(SOFTWARE));
        assert_eq!(get("CHAPTER001"), Some("00:00:00.000"));
        assert_eq!(get("CHAPTER002"), Some("00:01:01.500"));
        assert_eq!(get("CHAPTER002NAME"), Some("Song B"));
        assert_eq!(chapter_stamp(3_723_004), "01:02:03.004");
    }

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(i.as_bytes()), o);
        }
    }

    #[test]
    fn wav_chunks_hold_info_text_and_the_chapters_as_cue_points() {
        let b = wav_chunks(&tags());
        let find = |id: &[u8]| b.windows(4).position(|w| w == id);
        assert_eq!(&b[..4], b"LIST");
        assert_eq!(&b[8..12], b"INFO");
        assert!(find(b"INAM").is_some() && find(b"IART").is_some() && find(b"ICMT").is_some());
        let cue = find(b"cue ").unwrap();
        assert_eq!(u32::from_le_bytes(b[cue + 8..cue + 12].try_into().unwrap()), 2, "two cue points");
        // second point: id 2 at 61.5 s = 2712150 frames
        let p2 = cue + 12 + 24;
        assert_eq!(u32::from_le_bytes(b[p2..p2 + 4].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(b[p2 + 4..p2 + 8].try_into().unwrap()), 2_712_150);
        assert!(find(b"adtl").is_some() && find(b"labl").is_some());
        // every chunk is word aligned, so the total is even
        assert_eq!(b.len() % 2, 0);
        // text only, no chapters: just INFO
        let only = wav_chunks(&Tags { title: "x".into(), ..Tags::default() });
        assert!(only.windows(4).all(|w| w != b"cue "));
    }

    #[test]
    fn a_cover_must_be_a_small_jpeg_or_png() {
        assert!(Cover::from_bytes(b"not an image".to_vec()).is_err());
        let big = vec![0u8; MAX_COVER_BYTES + 1];
        let e = Cover::from_bytes(big).unwrap_err().to_string();
        assert!(e.contains("smaller"), "{e}");
        // a 1x1 PNG
        let png: Vec<u8> = vec![
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 0x90, 0x77, 0x53, 0xde, 0, 0, 0, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7,
            0x63, 0xf8, 0xcf, 0xc0, 0, 0, 0x03, 0x01, 0x01, 0, 0x18, 0xdd, 0x8d, 0xb0, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let c = Cover::from_bytes(png).unwrap();
        assert_eq!((c.mime.as_str(), c.width, c.height, c.depth), ("image/png", 1, 1, 24));
        let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;".to_vec();
        assert!(Cover::from_bytes(gif).is_err(), "GIF is not taken (not every tag reader shows it)");
    }

    #[test]
    fn frames_are_converted_to_milliseconds() {
        assert_eq!(ms_of_frames(0), 0);
        assert_eq!(ms_of_frames(44_100), 1000);
        assert_eq!(ms_of_frames(-5), 0);
        assert_eq!(ms_of_frames(2_712_150), 61_500);
    }
}
