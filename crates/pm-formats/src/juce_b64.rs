//! `juce::MemoryBlock::toBase64Encoding` / `fromBase64Encoding`.
//!
//! Not RFC 4648: the output is `<byte count>.` followed by 6-bit groups taken LSB-first from the
//! data, mapped through [`ALPHABET`]. Verified against pmplayer's `getBitRange` and against
//! `.str` files written by the original Editor.

pub const ALPHABET: &[u8; 64] = b".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+";

pub fn encode(data: &[u8]) -> String {
    let chars = (data.len() * 8).div_ceil(6);
    let mut out = format!("{}.", data.len());
    out.reserve(chars);
    for i in 0..chars {
        out.push(ALPHABET[bit_range(data, i * 6, 6) as usize] as char);
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    MissingLength,
    BadLength,
    BadChar(char),
}

pub fn decode(text: &str) -> Result<Vec<u8>, DecodeError> {
    let (len, body) = text.split_once('.').ok_or(DecodeError::MissingLength)?;
    let len: usize = len.trim().parse().map_err(|_| DecodeError::BadLength)?;
    let mut out = vec![0u8; len];
    for (i, ch) in body.chars().enumerate() {
        let v = ALPHABET
            .iter()
            .position(|&a| a as char == ch)
            .ok_or(DecodeError::BadChar(ch))? as u32;
        set_bit_range(&mut out, i * 6, 6, v);
    }
    Ok(out)
}

fn bit_range(data: &[u8], start: usize, bits: usize) -> u32 {
    let mut res = 0u32;
    for b in 0..bits {
        let pos = start + b;
        if pos / 8 < data.len() && data[pos / 8] >> (pos % 8) & 1 == 1 {
            res |= 1 << b;
        }
    }
    res
}

fn set_bit_range(data: &mut [u8], start: usize, bits: usize, value: u32) {
    for b in 0..bits {
        let pos = start + b;
        if pos / 8 >= data.len() {
            return;
        }
        if value >> b & 1 == 1 {
            data[pos / 8] |= 1 << (pos % 8);
        } else {
            data[pos / 8] &= !(1 << (pos % 8));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // 0x0201 LSB-first in 6-bit groups is 1, 8, 0
        assert_eq!(encode(&[0x01, 0x02]), "2.AH.");
    }

    #[test]
    fn round_trip_lengths() {
        for n in 0..64usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(decode(&encode(&data)).unwrap(), data);
        }
    }
}
