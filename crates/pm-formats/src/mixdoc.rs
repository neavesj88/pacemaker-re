//! Mix document containers: the `mixes.structured_ct` blob and `.pmk` prefab files.
//!
//! Both hold a boost-XML `MixDocument`, zlib-compressed and then XORed with an 18-byte table
//! (pmplayer's `MixDocumentObfuscatorFilter`). The table index starts at `start` and advances
//! by 2 before each byte, wrapping at 18. The DB blob starts at `len % 18`
//! (`MixDocument::loadDocumentFromBlob`) and `.pmk` files at 12. Both are verified byte-exact
//! against output of the original Editor.

use std::io::{Read, Write};

pub const KEY: &[u8; 18] = b"y7W6GSRycjV96qh3R\0";
const PMK_START: usize = 12;

pub fn xor(data: &[u8], start: usize) -> Vec<u8> {
    let mut s = start;
    data.iter()
        .map(|b| {
            s += 2;
            if s >= KEY.len() {
                s -= KEY.len();
            }
            b ^ KEY[s]
        })
        .collect()
}

fn deflate(xml: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(xml).expect("writing to a Vec cannot fail");
    z.finish().expect("writing to a Vec cannot fail")
}

fn inflate(z: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(z).read_to_end(&mut out)?;
    Ok(out)
}

pub fn decode_blob(blob: &[u8]) -> std::io::Result<Vec<u8>> {
    inflate(&xor(blob, blob.len() % KEY.len()))
}

pub fn encode_blob(xml: &[u8]) -> Vec<u8> {
    let z = deflate(xml);
    let start = z.len() % KEY.len();
    xor(&z, start)
}

/// Decodes a `.pmk`. Start index 12 is verified on one sample only, so other phases are tried
/// too, accepting the first that inflates.
pub fn decode_pmk(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut last = None;
    for start in std::iter::once(PMK_START).chain((0..KEY.len()).filter(|&s| s != PMK_START)) {
        match inflate(&xor(data, start)) {
            Ok(xml) => return Ok(xml),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap())
}

pub fn encode_pmk(xml: &[u8]) -> Vec<u8> {
    xor(&deflate(xml), PMK_START)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_round_trip() {
        let xml = b"<?xml version=\"1.0\"?><MixDocument>example</MixDocument>".repeat(20);
        assert_eq!(decode_blob(&encode_blob(&xml)).unwrap(), xml);
        assert_eq!(decode_pmk(&encode_pmk(&xml)).unwrap(), xml);
    }
}
