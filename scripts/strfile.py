"""Read/write Pacemaker per-track analysis sidecars (<track>.str next to the audio file).

Verified by decompilation (pmplayer 14142 reader, Mac Editor 2.0.2.14170 writer):
- Location: audio path up to the last '.' + ".str" (PromotedLoadTrackWorker, osal::getExtension(0)).
- boost xml archive, root NVP "TrackAnalysedData", class version 0, members in order
  normalizeFactor (float), beatStartOffset (long), realBeatLocations (string).
- realBeatLocations = juce::MemoryBlock::toBase64Encoding(zlib(XML)). This is JUCE's own format,
  "<byte count>." + 6-bit groups packed LSB-first using the alphabet below, not RFC 4648.
  The device decodes it with MemoryBlock::fromBase64Encoding.
- The XML is <beats> with <beat pos=INT bpm=DOUBLE [firstDownBeat="1"]/> children
  (BpmAnalysis::postProcess). pos is in frames; the device scales it by the clip's rate ratio.

Byte-exact against files written by the Windows Editor 2.0.2.14170 (its AppData str/ cache):
boost archive version "4"; no closing </boost_serialization> (save() takes the stream before the
archive destructor writes it); CRLF line ends on Windows; the beat XML laid out exactly as
juce::XmlElement::createDocument does it (see beats_xml); bpm as %.9g; zlib (78 9c) at the default
level. The decoder also accepts a closing tag and gzip.
"""
import gzip, sys, zlib, xml.etree.ElementTree as ET

JUCE_B64 = ".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+"
CRLF = "\r\n"

ARCHIVE = [
    '<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>',
    "<!DOCTYPE boost_serialization>",
    '<boost_serialization signature="serialization::archive" version="4">',
    '<TrackAnalysedData class_id="0" tracking_level="0" version="0">',
    "\t<normalizeFactor>{nf}</normalizeFactor>",
    "\t<beatStartOffset>{off}</beatStartOffset>",
    "\t<realBeatLocations>{beats}</realBeatLocations>",
    "</TrackAnalysedData>",
]

def juce_b64encode(data):
    bits = int.from_bytes(data, "little")
    n = (len(data) * 8 + 5) // 6
    return f"{len(data)}." + "".join(JUCE_B64[(bits >> (6 * i)) & 63] for i in range(n))

def juce_b64decode(text):
    size, _, body = text.partition(".")
    bits = 0
    for i, ch in enumerate(body):
        bits |= JUCE_B64.index(ch) << (6 * i)
    return (bits & ((1 << (8 * int(size))) - 1)).to_bytes(int(size), "little")

def beats_xml(beats, first_downbeat=None):
    # createDocument: XML header, a blank line, then CRLF-terminated, two-space-indented children
    lines = ['<?xml version="1.0" encoding="UTF-8"?>\n\n<beats>' + CRLF]
    for i, (pos, bpm) in enumerate(beats):
        fdb = ' firstDownBeat="1"' if i == first_downbeat else ""
        lines.append(f'  <beat pos="{int(pos)}" bpm="{float(bpm):.9g}"{fdb}/>' + CRLF)
    lines.append("</beats>" + CRLF)
    return "".join(lines).encode("utf-8")

def encode(normalize_factor, beat_start_offset, beats, first_downbeat=None, newline=CRLF):
    """beats: iterable of (pos_frames:int, bpm:float); first_downbeat: index into beats or None.
    newline=CRLF reproduces the Windows Editor byte-for-byte; "\\n" matches the Mac Editor."""
    blob = juce_b64encode(zlib.compress(beats_xml(list(beats), first_downbeat)))
    vals = {"nf": f"{float(normalize_factor):.8g}", "off": int(beat_start_offset), "beats": blob}
    return "".join(line.format(**vals) + newline for line in ARCHIVE)

def decode(text):
    """Return (normalize_factor, beat_start_offset, [(pos, bpm)], first_downbeat_index or None)."""
    if "</boost_serialization>" not in text:
        text += "</boost_serialization>"
    tad = ET.fromstring(text).find("TrackAnalysedData")
    raw = juce_b64decode(tad.findtext("realBeatLocations") or "0.")
    xml = gzip.decompress(raw) if raw[:2] == b"\x1f\x8b" else zlib.decompress(raw)
    beats, first = [], None
    for i, b in enumerate(ET.fromstring(xml).iter("beat")):
        beats.append((int(b.get("pos")), float(b.get("bpm"))))
        if b.get("firstDownBeat") == "1":
            first = i
    return float(tad.findtext("normalizeFactor")), int(tad.findtext("beatStartOffset")), beats, first

if __name__ == "__main__":
    with open(sys.argv[1], encoding="utf-8", newline="") as fh:
        nf, off, beats, first = decode(fh.read())
    print(f"normalizeFactor={nf} beatStartOffset={off} beats={len(beats)} firstDownBeat={first}")
    for pos, bpm in beats[:8]:
        print(f"  pos={pos} bpm={bpm}")
