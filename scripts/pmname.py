"""Device-side file naming and tracks.location, as the original Editor does it.

Verified by decompiling the Mac Editor 2.0.2.14170 (TrackLibrarySyncer::transferTrackToDevice,
createHashedFilePath, osal::getEncodedFilePath):
  file     = <device root>/.Pacemaker/Music/ + A + "/" + B + "/" + C + "." + ext
             then juce getNonexistentChildFile if that name is taken
  location = "/pmdb_tracks/" + A + "/" + B + "/" + C + "." + ext   ("before_url_device")
  sidecar  = same path with ".str" (section 4 of FINDINGS)
where, over S = str(int_arg) and N = filename without extension (UTF-8 bytes as *signed* chars):
  A = last hex digit of oaat(S), B = last hex digit of elf(S), C = hex(oaat(N)).
Both hashes are the decompiled variants. They use arithmetic (signed) shifts, so they differ
from the textbook one-at-a-time and ELF hashes for some inputs.
Inferred, not yet checked against a device dump: int_arg is the source file size (it is in
getEncodedAbsoluteFilePath, which shares this routine), and juce::String::toHexString prints
lowercase without leading zeros.
"""
import os, sys

M32 = 0xFFFFFFFF

def _s32(x):
    x &= M32
    return x - (1 << 32) if x & 0x80000000 else x

def _chars(text):
    return [b - 256 if b > 127 else b for b in text.encode("utf-8")]

def oaat(text):
    h = 0
    cs = _chars(text)
    for c in cs:
        h = ((h + c) * 0x401) & M32
        h = ((_s32(h) >> 6) ^ h) & M32
    t = (h * 8) & M32 if cs else 0
    u = (t + h) & M32
    return (((_s32(u) >> 11) ^ u) * 0x8001) & M32

def elf(text):
    h = 0
    for c in _chars(text):
        h = (h * 16 + c) & M32
        g = h & 0xF0000000
        if g:
            h = (h ^ (_s32(g) >> 24)) & M32
        h &= ~g & M32
    return h

def encoded_name(stem, int_arg):
    s = str(int_arg)
    return f"{oaat(s):x}"[-1] + "/" + f"{elf(s):x}"[-1] + "/" + f"{oaat(stem):x}"

def device_paths(source_file):
    """(relative path under .Pacemaker/Music/, tracks.location) for a source audio file."""
    stem, ext = os.path.splitext(os.path.basename(source_file))
    rel = encoded_name(stem, os.path.getsize(source_file)) + ext
    return rel, "/pmdb_tracks/" + rel

if __name__ == "__main__":
    for f in sys.argv[1:]:
        rel, loc = device_paths(f)
        print(f"{f}\n  file: .Pacemaker/Music/{rel}\n  location: {loc}")
