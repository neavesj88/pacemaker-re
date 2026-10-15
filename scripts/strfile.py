"""Read/write Pacemaker per-track analysis sidecars (<track>.str next to the audio file).

Verified from pmplayer 14142 (PromotedLoadTrackWorker -> futile::fileToString(path minus ext + ".str")
-> AudioClip::readBeatLocations -> TrackAnalysedData::load):
  boost xml_iarchive, root NVP "TrackAnalysedData", class version 0, members in order
  normalizeFactor (float), beatStartOffset (long), realBeatLocations (string);
  realBeatLocations = base64(compressed XML), whose root element has <beat pos=INT bpm=DOUBLE/> children
  (pos in frames, scaled on load by the clip's rate ratio).
Inferred, not yet checked against an Editor-written file: archive version "4" (what the Editor's
own boost writes, see paceStartupData.xml), the beat XML root name "beats" (the device accepts any
root), and zlib framing: juce's GZIPDecompressorInputStream is built with noWrap=false, and the
Editor's .pmk mixes from the same codebase are zlib (78 9c). The decoder accepts gzip too.
"""
import base64, gzip, sys, zlib, xml.etree.ElementTree as ET

ARCHIVE = """<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<!DOCTYPE boost_serialization>
<boost_serialization signature="serialization::archive" version="4">
<TrackAnalysedData class_id="0" tracking_level="0" version="0">
\t<normalizeFactor>{nf}</normalizeFactor>
\t<beatStartOffset>{off}</beatStartOffset>
\t<realBeatLocations>{beats}</realBeatLocations>
</TrackAnalysedData>
</boost_serialization>
"""

def encode(normalize_factor, beat_start_offset, beats):
    """beats: iterable of (pos_frames:int, bpm:float)."""
    root = ET.Element("beats")
    for pos, bpm in beats:
        ET.SubElement(root, "beat", pos=str(int(pos)), bpm=repr(float(bpm)))
    blob = base64.b64encode(zlib.compress(ET.tostring(root, encoding="utf-8"), 9)).decode()
    return ARCHIVE.format(nf=f"{float(normalize_factor):.8g}", off=int(beat_start_offset), beats=blob)

def decode(text):
    tad = ET.fromstring(text).find("TrackAnalysedData")
    raw = base64.b64decode(tad.findtext("realBeatLocations") or "")
    xml = gzip.decompress(raw) if raw[:2] == b"\x1f\x8b" else zlib.decompress(raw)
    beats = [(int(b.get("pos")), float(b.get("bpm"))) for b in ET.fromstring(xml).iter("beat")]
    return float(tad.findtext("normalizeFactor")), int(tad.findtext("beatStartOffset")), beats

if __name__ == "__main__":
    nf, off, beats = decode(open(sys.argv[1], encoding="utf-8").read())
    print(f"normalizeFactor={nf} beatStartOffset={off} beats={len(beats)}")
    for pos, bpm in beats[:8]:
        print(f"  pos={pos} bpm={bpm}")
