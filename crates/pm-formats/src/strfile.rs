//! Per-track analysis sidecar (`<track>.str`), the beat grid and gain the device plays from.
//!
//! A boost `xml_oarchive` of `TrackAnalysedData` (class version 0) with `normalizeFactor`,
//! `beatStartOffset` and `realBeatLocations`; the last is JUCE base64 of zlib of a `<beats>` XML
//! document. [`encode`] reproduces files written by the Windows Editor 2.0.2.14170
//! byte-for-byte, including its quirks: no closing `</boost_serialization>`, CRLF line ends and
//! JUCE's `createDocument` layout. See notes/FINDINGS.md section 4.

use std::io::{Read, Write};

use crate::juce_b64;

/// One beat: position in source sample frames and the local tempo there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    pub pos: i64,
    pub bpm: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub normalize_factor: f32,
    pub beat_start_offset: i64,
    pub beats: Vec<Beat>,
    /// Index into `beats` of the first downbeat, written as `firstDownBeat="1"`.
    pub first_downbeat: Option<usize>,
}

#[derive(Debug)]
pub enum Error {
    MissingElement(&'static str),
    Base64(juce_b64::DecodeError),
    Inflate(std::io::Error),
    BadNumber(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

const ARCHIVE_HEAD: [&str; 4] = [
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#,
    "<!DOCTYPE boost_serialization>",
    r#"<boost_serialization signature="serialization::archive" version="4">"#,
    r#"<TrackAnalysedData class_id="0" tracking_level="0" version="0">"#,
];

/// Line ending for the outer archive: the Windows Editor writes CRLF, the Mac Editor LF.
/// The device reads either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newline {
    Crlf,
    Lf,
}

pub fn beats_xml(beats: &[Beat], first_downbeat: Option<usize>) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n<beats>\r\n");
    for (i, b) in beats.iter().enumerate() {
        let fdb = if Some(i) == first_downbeat { r#" firstDownBeat="1""# } else { "" };
        s.push_str(&format!(
            "  <beat pos=\"{}\" bpm=\"{}\"{}/>\r\n",
            b.pos,
            fmt_g(b.bpm, 9),
            fdb
        ));
    }
    s.push_str("</beats>\r\n");
    s
}

pub fn encode(a: &Analysis, newline: Newline) -> String {
    let nl = match newline {
        Newline::Crlf => "\r\n",
        Newline::Lf => "\n",
    };
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(beats_xml(&a.beats, a.first_downbeat).as_bytes())
        .expect("writing to a Vec cannot fail");
    let blob = juce_b64::encode(&z.finish().expect("writing to a Vec cannot fail"));
    let mut out = String::new();
    for line in ARCHIVE_HEAD {
        out.push_str(line);
        out.push_str(nl);
    }
    out.push_str(&format!("\t<normalizeFactor>{}</normalizeFactor>{nl}", fmt_g(a.normalize_factor as f64, 8)));
    out.push_str(&format!("\t<beatStartOffset>{}</beatStartOffset>{nl}", a.beat_start_offset));
    out.push_str(&format!("\t<realBeatLocations>{blob}</realBeatLocations>{nl}"));
    out.push_str(&format!("</TrackAnalysedData>{nl}"));
    out
}

pub fn decode(text: &str) -> Result<Analysis, Error> {
    let nf = element(text, "normalizeFactor")?;
    let off = element(text, "beatStartOffset")?;
    let blob = element(text, "realBeatLocations")?;
    let raw = juce_b64::decode(blob.trim()).map_err(Error::Base64)?;
    let mut xml = String::new();
    if raw.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut xml)
    } else {
        flate2::read::ZlibDecoder::new(&raw[..]).read_to_string(&mut xml)
    }
    .map_err(Error::Inflate)?;
    let mut beats = Vec::new();
    let mut first_downbeat = None;
    for tag in xml.split("<beat ").skip(1) {
        let tag = &tag[..tag.find("/>").unwrap_or(tag.len())];
        let pos = attr(tag, "pos").ok_or(Error::MissingElement("beat pos"))?;
        let bpm = attr(tag, "bpm").ok_or(Error::MissingElement("beat bpm"))?;
        if attr(tag, "firstDownBeat") == Some("1") {
            first_downbeat = Some(beats.len());
        }
        beats.push(Beat { pos: num(pos)?, bpm: num(bpm)? });
    }
    Ok(Analysis {
        normalize_factor: num(nf.trim())?,
        beat_start_offset: num(off.trim())?,
        beats,
        first_downbeat,
    })
}

fn element<'a>(text: &'a str, name: &'static str) -> Result<&'a str, Error> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = text.find(&open).ok_or(Error::MissingElement(name))? + open.len();
    let end = text[start..].find(&close).ok_or(Error::MissingElement(name))? + start;
    Ok(&text[start..end])
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, Error> {
    s.parse().map_err(|_| Error::BadNumber(s.to_string()))
}

/// C `printf("%.{precision}g")`, which is how the Editor formats bpm (9) and normalizeFactor (8).
pub fn fmt_g(v: f64, precision: usize) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let p = precision.max(1);
    let exp = v.abs().log10().floor() as i32;
    // Rounding can carry into the next decade (9.9999 -> 10.000), so re-derive the exponent
    // from the %e rendering, exactly as C does.
    let e_repr = format!("{:.*e}", p - 1, v);
    let exp = e_repr
        .rsplit_once('e')
        .and_then(|(_, e)| e.parse::<i32>().ok())
        .unwrap_or(exp);
    let s = if exp < -4 || exp >= p as i32 {
        let (mant, _) = e_repr.split_once('e').unwrap();
        let mant = trim_zeros(mant);
        format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{:.*}", decimals, v)).to_string()
    };
    s
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_g_matches_printf() {
        assert_eq!(fmt_g(126.994225, 9), "126.994225");
        assert_eq!(fmt_g(126.99382, 9), "126.99382");
        assert_eq!(fmt_g(128.0, 9), "128");
        assert_eq!(fmt_g(1.0, 8), "1");
        assert_eq!(fmt_g(0.87654321, 8), "0.87654321");
        assert_eq!(fmt_g(99.99999999, 9), "100");
    }

    #[test]
    fn round_trip() {
        let a = Analysis {
            normalize_factor: 1.0,
            beat_start_offset: 1710,
            beats: (0..64)
                .map(|i| Beat { pos: 1710 + i * 20834, bpm: 127.0 + (i % 3) as f64 * 0.01 })
                .collect(),
            first_downbeat: Some(0),
        };
        for nl in [Newline::Crlf, Newline::Lf] {
            assert_eq!(decode(&encode(&a, nl)).unwrap(), a);
        }
    }
}
