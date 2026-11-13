"""Rename the "Tonium" text in the audio plugin libraries of firmware 180096069 (notes/about-screen.md, "Tonium sweep").

Fourteen libraries in /bin carry description strings as UTF-32LE (wchar_t, 4 bytes per character, packed back to
back, one NUL cell after each):

  libcontainer_<fmt>.so   "<FMT> container plugin by Tonium AB"                                  (8 libraries)
                          "(C) 2007, Tonium AB. All rights reserved - all wrongs reversed"       (same 8)
  libdecoder_<fmt>.so     "<FMT> decoder plugin by Tonium"                                        (6 libraries)

They are filled into a plugin description by <Fmt>Container::info / <Fmt>Decoder::info, which nothing in the
player ever reads back or shows; the patch only keeps the files from saying Tonium. The new text is never longer
than the old one, so nothing moves: the characters are overwritten in place and the rest of the old string is
zero-filled (the NUL cell that ends the string stays).

  container description   "... container plugin by Tonium AB"  ->  "... container plugin (Revival)"
  decoder description     "... decoder plugin by Tonium"       ->  "... decoder plugin (Revival)"   (same length)
  copyright line          "(C) 2007, Tonium AB. All rights reserved - all wrongs reversed"
                                                               ->  "Pacemaker Revival - all wrongs reversed"
                          (the (C) sign and "all rights reserved" go too: this is a name tag, not a claim of
                          copyright over Tonium's plugin code)

Every library is checked against its stock sha256, every string against its exact stock text and offset, and the
output may differ from the input only inside those strings. Any mismatch refuses.

  patch-plugin-strings.py [STOCK_BIN_DIR [OUT_DIR]]
  defaults: work/fw-180096069/bin -> work/build/074/plugins
"""
import hashlib
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

COPYRIGHT = "(C) 2007, Tonium AB. All rights reserved - all wrongs reversed"
NEW_COPYRIGHT = "Pacemaker Revival - all wrongs reversed"

# lib -> (format name, sha256 of the stock library, [(file offset, old text, new text), ...])
def _container(fmt, sha, desc_off, copy_off):
    return (f"libcontainer_{fmt[0]}.so", (sha, [
        (desc_off, f"{fmt[1]} container plugin by Tonium AB", f"{fmt[1]} container plugin (Revival)"),
        (copy_off, COPYRIGHT, NEW_COPYRIGHT)]))


def _decoder(fmt, sha, off):
    return (f"libdecoder_{fmt[0]}.so", (sha, [
        (off, f"{fmt[1]} decoder plugin by Tonium", f"{fmt[1]} decoder plugin (Revival)")]))


LIBS = dict([
    _container(("aiffc", "AIFF"), "1ccc352322b37fabbc5c5928b0ccaeb63460bef4ace7ee2e47562c66efa23df6", 0x3F54, 0x3FF4),
    _container(("flac", "FLAC"), "98fb95afb4142ea5a7e60ab7fee31d26e99c6b821205e2264eb1bcc728275cb9", 0x55B4, 0x5654),
    _container(("id3", "ID3"), "a96dd9f84de8ac97de797c50a43256985876cb49d240302b8b649174889bbd88", 0x89A4, 0x8A3C),
    _container(("mp3", "MP3"), "a2c68628fddfc178c8074451e957fef2f1d5d06f69ecf5f5dd34246e0590a019", 0x70FC, 0x7194),
    _container(("mpeg4", "MPEG-4"), "f5e3572e93d888dbe28d10dafc421db712607f91c9495c4367a4cb652bb60c1b", 0x6E2C, 0x6EDC),
    _container(("ogg", "OGG"), "3d9769f920078c60c25789f681c298a15e9e73db2ebf4f20e11fab0258e4c59e", 0x5814, 0x58AC),
    _container(("snd", "SND"), "632c572650da352c6729879573a345f164f68b3c17b4b635c10a4b79a48939d0", 0x3388, 0x3420),
    _container(("wav", "WAV"), "8d5f4acfcdf579b4d7116ad24f332b50e59cdfcb6c7b88dca9eff5cd86939fa6", 0x4A14, 0x4AAC),
    _decoder(("aac", "AAC"), "26346a854e364aa41341f5ecaea27390e8ab6d39bf8a039de2da5fde2b885281", 0x56ED8),
    _decoder(("au", "AU"), "69b99dcbefe08612d49c14ea495490b2778ffa9c880c748ed6c1a7e05ea1833c", 0x1694),
    _decoder(("flac", "FLAC"), "d24d40a32ed7795eb450e8503975651e5b17db00a8a8d880f582003ff6daf34c", 0x14698),
    _decoder(("mp3", "MP3"), "72605e3e8f262d43e3beae7ade704f2d3c233febfee8f2b7ec7c9e0512ccfc8d", 0x6B88),
    _decoder(("pcm", "PCM"), "16f448f6d6b6ca966c323d6b2aebc6cd7e92eb17d74890f557438bea321d1401", 0x1C28),
    _decoder(("vorbis", "Vorbis"), "116576e893861194664450c47b0a60df31089335fb5219dd5e09048517140882", 0x1B63C),
])


def u32(s):
    return s.encode("utf-32-le")


def patch(name, d):
    """Return the patched library. Exits on any mismatch."""
    sha, strings = LIBS[name]
    if hashlib.sha256(d).hexdigest() != sha:
        sys.exit(f"refusing: {name} is not the stock 180096069 library (sha256 differs; already patched?)")
    out = bytearray(d)
    for off, old, new in strings:
        if len(new) > len(old) or not new.isascii() or "\0" in new:
            sys.exit(f"refusing: bad replacement for {name} {old!r}")
        if off % 4 or d[off:off + 4 * len(old) + 4] != u32(old) + bytes(4):
            sys.exit(f"refusing: {name} 0x{off:x} does not hold the stock text {old!r}")
        out[off:off + 4 * (len(old) + 1)] = u32(new) + bytes(4 * (len(old) - len(new) + 1))
    out = bytes(out)
    assert len(out) == len(d)
    assert b"tonium" not in out.lower() and "tonium".encode("utf-32-le") not in out.lower()
    changed = [i for i in range(len(d)) if d[i] != out[i]]
    lo, hi = min(o for o, _, _ in strings), max(o + 4 * len(t) for o, t, _ in strings)
    assert all(lo <= i < hi for i in changed), "changed outside the planned strings"
    for off, old, new in strings:
        assert out[off:off + 4 * len(new) + 4] == u32(new) + bytes(4)
    return out


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "work", "fw-180096069", "bin")
    dst = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "work", "build", "074", "plugins")
    os.makedirs(dst, exist_ok=True)
    for name in sorted(LIBS):
        d = open(os.path.join(src, name), "rb").read()
        open(os.path.join(dst, name), "wb").write(patch(name, d))
    print(f"{len(LIBS)} plugin libraries patched into {dst}")


if __name__ == "__main__":
    main()
