//! A fingerprint of a file's audio payload, tags left out. The tag writer takes one before and one
//! after a rewrite and refuses the result unless they are equal, so no sample, frame or packet can
//! change without the write being abandoned (and the original kept).
//!
//! What counts as audio, per container (everything else may legitimately change when a tag is
//! rewritten):
//! - MP3 and ADTS AAC: every byte after the leading ID3v2 tag(s);
//! - WAV and AIFF: every chunk except the `ID3 ` one (id, size and payload of each);
//! - FLAC: STREAMINFO and SEEKTABLE blocks and everything after the last metadata block;
//! - Ogg Vorbis and Opus: every packet except the second (the comment header), so a changed page
//!   layout or sequence number does not matter, a changed packet does;
//! - MP4: the payload of each `mdat`, plus where every `stco` / `co64` chunk offset lands inside
//!   its `mdat` (a rewrite moves `moov` and has to shift the offsets by exactly the change).
//!
//! A file whose structure cannot be followed gives an error, never a fingerprint of "something",
//! and the writer treats that as "cannot prove the audio is untouched".

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use lofty::file::FileType;
use sha2::{Digest, Sha256};

pub type Fingerprint = [u8; 32];

type Res<T> = Result<T, String>;

fn io(e: std::io::Error) -> String {
    e.to_string()
}

/// Feeds `len` bytes starting at `start` into the hash; the file must hold all of them.
fn feed<R: Read + Seek>(r: &mut R, start: u64, len: u64, h: &mut Sha256) -> Res<()> {
    r.seek(SeekFrom::Start(start)).map_err(io)?;
    let mut left = len;
    let mut buf = vec![0u8; 64 * 1024];
    while left > 0 {
        let n = buf.len().min(left as usize);
        r.read_exact(&mut buf[..n]).map_err(|_| "the file ends before the length its structure promises".to_string())?;
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(())
}

fn size_of<R: Seek>(r: &mut R) -> Res<u64> {
    r.seek(SeekFrom::End(0)).map_err(io)
}

/// Bytes taken by the ID3v2 tag(s) at the start of the file (0 when there is none).
fn id3v2_len<R: Read + Seek>(r: &mut R, total: u64) -> Res<u64> {
    let mut at = 0u64;
    loop {
        if at + 10 > total {
            return Ok(at);
        }
        r.seek(SeekFrom::Start(at)).map_err(io)?;
        let mut h = [0u8; 10];
        r.read_exact(&mut h).map_err(io)?;
        if &h[..3] != b"ID3" {
            return Ok(at);
        }
        if h[6..10].iter().any(|b| b & 0x80 != 0) {
            return Err("the ID3v2 header has an invalid size".into());
        }
        let size = h[6..10].iter().fold(0u64, |acc, b| (acc << 7) | u64::from(*b));
        let footer = if h[5] & 0x10 != 0 { 10 } else { 0 };
        let end = at + 10 + size + footer;
        if end > total {
            return Err("the ID3v2 tag runs past the end of the file".into());
        }
        at = end;
    }
}

fn mpeg_like<R: Read + Seek>(r: &mut R, h: &mut Sha256) -> Res<()> {
    let total = size_of(r)?;
    let start = id3v2_len(r, total)?;
    feed(r, start, total - start, h)
}

/// WAV / AIFF: a list of `id (4) size (4) payload (+ pad)` chunks after a 12 byte header.
fn riff_like<R: Read + Seek>(r: &mut R, big_endian: bool, h: &mut Sha256) -> Res<()> {
    let total = size_of(r)?;
    r.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut head = [0u8; 12];
    r.read_exact(&mut head).map_err(|_| "too short for a RIFF / AIFF file".to_string())?;
    let ok = if big_endian { &head[..4] == b"FORM" } else { &head[..4] == b"RIFF" || &head[..4] == b"RF64" };
    if !ok {
        return Err("not a RIFF / AIFF container".into());
    }
    h.update(&head[8..12]); // the form type (WAVE, AIFF, AIFC)
    let mut at = 12u64;
    while at + 8 <= total {
        r.seek(SeekFrom::Start(at)).map_err(io)?;
        let mut ch = [0u8; 8];
        r.read_exact(&mut ch).map_err(io)?;
        let size = if big_endian { u32::from_be_bytes([ch[4], ch[5], ch[6], ch[7]]) } else { u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]) };
        let mut size = u64::from(size);
        // a data chunk that promises more than the file holds (streamed or cut files): what is there is the audio
        size = size.min(total - (at + 8));
        if &ch[..4] != b"ID3 " && &ch[..4] != b"id3 " {
            h.update(&ch[..4]);
            h.update(size.to_le_bytes());
            feed(r, at + 8, size, h)?;
        }
        at += 8 + size + (size & 1);
    }
    Ok(())
}

fn flac<R: Read + Seek>(r: &mut R, h: &mut Sha256) -> Res<()> {
    let total = size_of(r)?;
    let mut at = id3v2_len(r, total)?;
    r.seek(SeekFrom::Start(at)).map_err(io)?;
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).map_err(|_| "too short for a FLAC file".to_string())?;
    if &magic != b"fLaC" {
        return Err("not a FLAC stream".into());
    }
    at += 4;
    loop {
        r.seek(SeekFrom::Start(at)).map_err(io)?;
        let mut b = [0u8; 4];
        r.read_exact(&mut b).map_err(|_| "the FLAC metadata ends early".to_string())?;
        let last = b[0] & 0x80 != 0;
        let kind = b[0] & 0x7f;
        let len = u64::from(u32::from_be_bytes([0, b[1], b[2], b[3]]));
        if at + 4 + len > total {
            return Err("a FLAC metadata block runs past the end of the file".into());
        }
        if kind == 0 || kind == 3 {
            h.update([kind]);
            h.update(len.to_le_bytes());
            feed(r, at + 4, len, h)?;
        }
        at += 4 + len;
        if last {
            break;
        }
    }
    feed(r, at, total - at, h)
}

/// Ogg: pages are re-cut when a comment header changes size, so the packets are what is compared.
fn ogg<R: Read>(r: &mut R, h: &mut Sha256) -> Res<()> {
    let mut index = 0u64;
    let mut cur: Vec<u8> = Vec::new();
    let mut pages = 0u64;
    let take = |index: &mut u64, cur: &mut Vec<u8>, h: &mut Sha256| {
        if *index != 1 {
            h.update(index.to_le_bytes());
            h.update((cur.len() as u64).to_le_bytes());
            h.update(&cur[..]);
        }
        *index += 1;
        cur.clear();
    };
    loop {
        let mut head = [0u8; 27];
        match r.read_exact(&mut head) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(io(e)),
        }
        if &head[..4] != b"OggS" {
            return Err("an Ogg page does not start with OggS".into());
        }
        let nsegs = usize::from(head[26]);
        let mut lacing = vec![0u8; nsegs];
        r.read_exact(&mut lacing).map_err(|_| "an Ogg page ends early".to_string())?;
        let len: usize = lacing.iter().map(|l| usize::from(*l)).sum();
        let mut payload = vec![0u8; len];
        r.read_exact(&mut payload).map_err(|_| "an Ogg page ends early".to_string())?;
        let mut pos = 0usize;
        for l in lacing {
            let l = usize::from(l);
            cur.extend_from_slice(&payload[pos..pos + l]);
            pos += l;
            if l < 255 {
                take(&mut index, &mut cur, h);
            }
        }
        pages += 1;
    }
    if pages == 0 {
        return Err("no Ogg pages".into());
    }
    if !cur.is_empty() {
        take(&mut index, &mut cur, h);
    }
    if index < 2 {
        return Err("an Ogg stream without a comment header".into());
    }
    Ok(())
}

fn be32(b: &[u8]) -> u64 {
    u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Walks the atoms of `buf`; collects the chunk offsets of every `stco` / `co64` below the
/// container atoms on the way to them.
fn chunk_offsets(buf: &[u8], depth: u32, out: &mut Vec<u64>) -> Res<()> {
    if depth > 8 {
        return Err("MP4 atoms nested too deeply".into());
    }
    let mut at = 0usize;
    while at + 8 <= buf.len() {
        let mut size = be32(&buf[at..]) as usize;
        let kind = &buf[at + 4..at + 8];
        let mut head = 8usize;
        if size == 1 {
            if at + 16 > buf.len() {
                return Err("a truncated MP4 atom".into());
            }
            size = be64(&buf[at + 8..]) as usize;
            head = 16;
        } else if size == 0 {
            size = buf.len() - at;
        }
        if size < head || at + size > buf.len() {
            return Err("an MP4 atom runs past its container".into());
        }
        let body = &buf[at + head..at + size];
        match kind {
            b"trak" | b"mdia" | b"minf" | b"stbl" => chunk_offsets(body, depth + 1, out)?,
            b"stco" | b"co64" => {
                let wide = kind == b"co64";
                if body.len() < 8 {
                    return Err("a truncated chunk offset table".into());
                }
                let count = be32(&body[4..]) as usize;
                let w = if wide { 8 } else { 4 };
                if body.len() < 8 + count * w {
                    return Err("a truncated chunk offset table".into());
                }
                for i in 0..count {
                    let p = 8 + i * w;
                    out.push(if wide { be64(&body[p..]) } else { be32(&body[p..]) });
                }
            }
            _ => {}
        }
        at += size;
    }
    Ok(())
}

fn mp4<R: Read + Seek>(r: &mut R, h: &mut Sha256) -> Res<()> {
    let total = size_of(r)?;
    let mut at = 0u64;
    let mut mdats: Vec<(u64, u64)> = Vec::new(); // (payload start, payload length)
    let mut moov: Option<Vec<u8>> = None;
    while at + 8 <= total {
        r.seek(SeekFrom::Start(at)).map_err(io)?;
        let mut b = [0u8; 8];
        r.read_exact(&mut b).map_err(io)?;
        let mut size = be32(&b);
        let kind = [b[4], b[5], b[6], b[7]];
        let mut head = 8u64;
        if size == 1 {
            let mut ext = [0u8; 8];
            r.read_exact(&mut ext).map_err(|_| "a truncated MP4 atom".to_string())?;
            size = be64(&ext);
            head = 16;
        } else if size == 0 {
            size = total - at;
        }
        if size < head || at + size > total {
            return Err("an MP4 atom runs past the end of the file".into());
        }
        match &kind {
            b"mdat" => {
                let len = size - head;
                h.update(b"mdat");
                h.update((mdats.len() as u64).to_le_bytes());
                h.update(len.to_le_bytes());
                feed(r, at + head, len, h)?;
                mdats.push((at + head, len));
            }
            b"moov" => {
                if moov.is_some() {
                    return Err("two moov atoms".into());
                }
                let len = size - head;
                if len > 256 * 1024 * 1024 {
                    return Err("an MP4 moov atom of more than 256 MB".into());
                }
                let mut v = vec![0u8; len as usize];
                r.seek(SeekFrom::Start(at + head)).map_err(io)?;
                r.read_exact(&mut v).map_err(io)?;
                moov = Some(v);
            }
            _ => {}
        }
        at += size;
    }
    let moov = moov.ok_or("an MP4 file without a moov atom")?;
    let mut offsets = Vec::new();
    chunk_offsets(&moov, 0, &mut offsets)?;
    h.update(b"stco");
    h.update((offsets.len() as u64).to_le_bytes());
    for off in offsets {
        let (i, (start, _)) = mdats
            .iter()
            .enumerate()
            .find(|(_, (s, l))| off >= *s && off < s + l)
            .ok_or("a chunk offset points outside the media data")?;
        h.update((i as u64).to_le_bytes());
        h.update((off - start).to_le_bytes());
    }
    Ok(())
}

/// The fingerprint of `path`'s audio payload for a file of type `ft`.
pub fn fingerprint(path: &Path, ft: FileType) -> Res<Fingerprint> {
    let mut r = BufReader::new(File::open(path).map_err(io)?);
    let mut h = Sha256::new();
    match ft {
        FileType::Mpeg | FileType::Aac => mpeg_like(&mut r, &mut h)?,
        FileType::Wav => riff_like(&mut r, false, &mut h)?,
        FileType::Aiff => riff_like(&mut r, true, &mut h)?,
        FileType::Flac => flac(&mut r, &mut h)?,
        FileType::Vorbis | FileType::Opus => ogg(&mut r, &mut h)?,
        FileType::Mp4 => mp4(&mut r, &mut h)?,
        other => return Err(format!("no way to check the audio of {other:?} files")),
    }
    Ok(h.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pm-audiohash-{}-{}", std::process::id(), name));
        std::fs::write(&d, bytes).unwrap();
        d
    }

    fn ogg_page(seq: u32, header_type: u8, packets: &[&[u8]], continued_last: bool) -> Vec<u8> {
        let mut lacing = Vec::new();
        let mut payload = Vec::new();
        for (i, p) in packets.iter().enumerate() {
            let mut n = p.len();
            while n >= 255 {
                lacing.push(255);
                n -= 255;
            }
            if !(continued_last && i == packets.len() - 1) {
                lacing.push(n as u8);
            }
            payload.extend_from_slice(p);
        }
        let mut v = b"OggS".to_vec();
        v.push(0);
        v.push(header_type);
        v.extend(0u64.to_le_bytes());
        v.extend(1u32.to_le_bytes());
        v.extend(seq.to_le_bytes());
        v.extend(0u32.to_le_bytes()); // crc: not checked here
        v.push(lacing.len() as u8);
        v.extend(lacing);
        v.extend(payload);
        v
    }

    #[test]
    fn ogg_ignores_the_comment_packet_and_the_page_layout_but_not_audio() {
        let id = [1u8; 30];
        let setup = [5u8; 40];
        let audio_a = [7u8; 300]; // longer than one lacing value
        let audio_b = [9u8; 20];
        let short = ogg_page(0, 2, &[&id], false)
            .into_iter()
            .chain(ogg_page(1, 0, &[b"comment-one", &setup], false))
            .chain(ogg_page(2, 0, &[&audio_a, &audio_b], false))
            .collect::<Vec<u8>>();
        // a longer comment, the same packets re-cut over more pages with other sequence numbers
        let long_comment = [3u8; 700];
        let recut = ogg_page(0, 2, &[&id], false)
            .into_iter()
            .chain(ogg_page(1, 0, &[&long_comment], false))
            .chain(ogg_page(2, 0, &[&setup], false))
            .chain(ogg_page(3, 0, &[&audio_a], false))
            .chain(ogg_page(4, 0, &[&audio_b], false))
            .collect::<Vec<u8>>();
        let a = fingerprint(&scratch("a.ogg", &short), FileType::Vorbis).unwrap();
        let b = fingerprint(&scratch("b.ogg", &recut), FileType::Vorbis).unwrap();
        assert_eq!(a, b);
        let mut changed = recut.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        assert_ne!(a, fingerprint(&scratch("c.ogg", &changed), FileType::Vorbis).unwrap());
        assert!(fingerprint(&scratch("d.ogg", b"not ogg at all, not ogg at all."), FileType::Vorbis).is_err());
    }

    #[test]
    fn mp3_skips_every_leading_id3v2_tag() {
        let audio = vec![0xAAu8; 500];
        let tag = |size: u8| {
            let mut t = b"ID3".to_vec();
            t.extend([4, 0, 0, 0, 0, 0, size]);
            t.extend(vec![0u8; size as usize]);
            t
        };
        let bare = fingerprint(&scratch("m0.mp3", &audio), FileType::Mpeg).unwrap();
        let one = [tag(20), audio.clone()].concat();
        let two = [tag(20), tag(7), audio.clone()].concat();
        assert_eq!(bare, fingerprint(&scratch("m1.mp3", &one), FileType::Mpeg).unwrap());
        assert_eq!(bare, fingerprint(&scratch("m2.mp3", &two), FileType::Mpeg).unwrap());
        let mut bad = one.clone();
        bad[40] ^= 1;
        assert_ne!(bare, fingerprint(&scratch("m3.mp3", &bad), FileType::Mpeg).unwrap());
        // a tag that claims more bytes than the file has is an error, not a guess
        assert!(fingerprint(&scratch("m4.mp3", &tag(20)[..15]), FileType::Mpeg).is_err());
    }

    #[test]
    fn mp4_offsets_are_compared_relative_to_their_mdat() {
        fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            v.extend(kind);
            v.extend(body);
            v
        }
        fn file(moov_first: bool, pad: usize, bad_offset: bool) -> Vec<u8> {
            let ftyp = atom(b"ftyp", b"M4A \0\0\0\0M4A ");
            let mdat_body: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
            let build_moov = |mdat_at: u32| {
                let off = if bad_offset { mdat_at + 8 + 5000 } else { mdat_at + 8 + 100 };
                let mut stco = vec![0u8; 4];
                stco.extend(1u32.to_be_bytes());
                stco.extend(off.to_be_bytes());
                atom(b"moov", &atom(b"trak", &atom(b"mdia", &atom(b"minf", &atom(b"stbl", &atom(b"stco", &stco))))))
            };
            let mdat = atom(b"mdat", &mdat_body);
            let mut free = atom(b"free", &vec![0u8; pad]);
            if pad == 0 {
                free.clear();
            }
            if moov_first {
                // the moov's own size does not depend on the offset it holds
                let moov_len = build_moov(0).len();
                let mdat_at = ftyp.len() + free.len() + moov_len;
                [ftyp, free, build_moov(mdat_at as u32), mdat].concat()
            } else {
                let mdat_at = ftyp.len() + free.len();
                [ftyp, free, mdat, build_moov(mdat_at as u32)].concat()
            }
        }
        let after_mdat = fingerprint(&scratch("p1.m4a", &file(false, 0, false)), FileType::Mp4).unwrap();
        let before_mdat = fingerprint(&scratch("p2.m4a", &file(true, 24, false)), FileType::Mp4).unwrap();
        assert_eq!(after_mdat, before_mdat, "moving moov and shifting the offsets is not a change of the audio");
        // an offset that is not shifted along with its mdat is
        let mut skewed = file(true, 24, false);
        let at = skewed.windows(4).position(|w| w == b"stco").unwrap() + 4 + 8;
        skewed[at + 3] += 1;
        assert_ne!(after_mdat, fingerprint(&scratch("p3.m4a", &skewed), FileType::Mp4).unwrap());
        assert!(fingerprint(&scratch("p4.m4a", &file(true, 0, true)), FileType::Mp4).is_err(), "an offset outside every mdat");
    }

    #[test]
    fn riff_skips_only_the_id3_chunk() {
        let chunk = |id: &[u8; 4], body: &[u8]| {
            let mut v = id.to_vec();
            v.extend((body.len() as u32).to_le_bytes());
            v.extend(body);
            if body.len() % 2 == 1 {
                v.push(0);
            }
            v
        };
        let wav = |extra: Vec<u8>, data: &[u8]| {
            let body = [b"WAVE".to_vec(), chunk(b"fmt ", &[1; 16]), chunk(b"data", data), extra].concat();
            [b"RIFF".to_vec(), (body.len() as u32).to_le_bytes().to_vec(), body].concat()
        };
        let plain = fingerprint(&scratch("w1.wav", &wav(vec![], &[5; 101])), FileType::Wav).unwrap();
        let tagged = fingerprint(&scratch("w2.wav", &wav(chunk(b"ID3 ", b"whatever tag"), &[5; 101])), FileType::Wav).unwrap();
        let listed = fingerprint(&scratch("w3.wav", &wav(chunk(b"LIST", b"INFO"), &[5; 101])), FileType::Wav).unwrap();
        let other = fingerprint(&scratch("w4.wav", &wav(vec![], &[6; 101])), FileType::Wav).unwrap();
        assert_eq!(plain, tagged);
        assert_ne!(plain, listed, "any other chunk counts");
        assert_ne!(plain, other);
    }
}
