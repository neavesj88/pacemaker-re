"""List every "Tonium" in an unpacked firmware and fail on any that is not on the keep list
(notes/about-screen.md, "Tonium sweep").

Looks in every file under the given directories, case-insensitively, as narrow text, UTF-16LE and UTF-32LE, and in
file names. Every gzip member found by its magic inside a file (the kernel payload of a zImage, and the compressed
build config /proc/config.gz inside that kernel, which is gzip inside gzip) is decompressed and swept the same way,
recursively; the label shows the path, e.g. zImage[gz@12696][gz@2363580]. Each hit is a whole printable token, so the
keep list names exact strings, not files. The keep list is what must not change or was deliberately left:

  functional  toniumId (boost serialization tag of every mix), Tonium;Device; (creator id in music.db),
              com.tonium.peer / com.tonium.app_pid (DirectFB window property keys), setToniumId / getToniumId
              (C++ symbol names inside pmplayer's hash tables)
  left alone  "Tonium" in boot/redboot.bin (the DFU gadget's USB manufacturer string: a patched bootloader would
              only be flashed if the image's bootloaderver changed, and RedBoot is the only DFU recovery),
              "Tonium AB" in /bin/resources/licenses/THIRD-PARTY-NOTICES.txt (the verbatim original legal text)

  sweep-tonium.py [--zimage FILE] DIR [DIR ...]       exit 0 = nothing outside the keep list
  (--zimage FILE is the same as one more file to sweep; nested gzip members are looked at for every file anyway)
"""
import os
import re
import sys
import zlib

FUNCTIONAL_PLAYER = {
    ("narrow", "toniumId"), ("utf-32-le", "toniumId"), ("utf-32-le", "Tonium;Device;"),
    ("narrow", "com.tonium.peer"), ("narrow", "com.tonium.app_pid"),
    ("narrow", "_ZN13GlobalContext11setToniumIdERKSbIwSt11char_traitsIwESaIwEE"),
    ("narrow", "_ZNK13GlobalContext11getToniumIdEv"),
}
FUNCTIONAL_PWR = {("narrow", "com.tonium.peer"), ("narrow", "com.tonium.app_pid")}
KEEP = {   # file name -> tokens that may stay
    "pmplayer": FUNCTIONAL_PLAYER,
    "pmplayer.real": FUNCTIONAL_PLAYER,
    "pwrApplication": FUNCTIONAL_PWR,
    "redboot.bin": {("narrow", "Tonium")},
    "THIRD-PARTY-NOTICES.txt": {("narrow", "Tonium")},
}


def tokens(d):
    """Set of (encoding, whole printable token containing 'tonium') in `d`."""
    low = d.lower()
    out = set()
    if b"tonium" in low:
        out |= {("narrow", m.group().decode()) for m in
                re.finditer(rb"[\x21-\x7e]*[Tt][Oo][Nn][Ii][Uu][Mm][\x21-\x7e]*", d)}
    for enc, w in (("utf-16-le", 2), ("utf-32-le", 4)):
        pad = b"\x00" * (w - 1)
        if "tonium".encode(enc) in low:
            ch = b"[\\x21-\\x7e]" + pad
            pat = b"(?:" + ch + b")*" + b"".join(b"[" + bytes([c]) + bytes([c ^ 32]) + b"]" + pad for c in b"tonium") + b"(?:" + ch + b")*"
            for m in re.finditer(pat, d):
                if m.start() % w == 0:
                    out.add((enc, m.group().decode(enc)))
    return out


MAX_DEPTH = 4
GZIP_MAGIC = bytes([0x1F, 0x8B, 0x08])


def nested_gzip(d, label="", depth=0):
    """(label, decompressed bytes) of every complete gzip member inside `d`, found by magic, and of the gzip members
    inside those, up to MAX_DEPTH levels. Magic bytes that are not a whole valid member (compressed data, text) are skipped."""
    if depth >= MAX_DEPTH:
        return
    view = memoryview(d)
    pos = 0
    while True:
        i = d.find(GZIP_MAGIC, pos)
        if i < 0:
            return
        pos = i + 1
        o = zlib.decompressobj(16 + zlib.MAX_WBITS)
        try:
            out = o.decompress(view[i:])
        except zlib.error:
            continue
        if not o.eof or not out:
            continue
        pos = len(d) - len(o.unused_data)
        yield f"{label}[gz@{i}]", out
        yield from nested_gzip(out, f"{label}[gz@{i}]", depth + 1)


def main():
    args = sys.argv[1:]
    zimage = None
    if args[:1] == ["--zimage"]:
        zimage, args = args[1], args[2:]
    if not args and not zimage:
        sys.exit(__doc__)
    bad, kept = [], []
    targets = []
    for top in args:
        for root, dirs, files in os.walk(top):
            for n in dirs + files:
                if "tonium" in n.lower():
                    bad.append((os.path.join(root, n), "file name"))
            for n in files:
                p = os.path.join(root, n)
                if os.path.isfile(p) and not os.path.islink(p):
                    targets.append((p, None))
    if zimage:
        targets.append((zimage, None))
    for p, data in targets:
        d = data if data is not None else open(p, "rb").read()
        allowed = KEEP.get(os.path.basename(p), ())
        for label, blob in [("", d)] + list(nested_gzip(d)):
            for t in sorted(tokens(blob)):
                (kept if t in allowed else bad).append((p + label, t))
    for p, t in kept:
        print("kept   ", p, t)
    for p, t in bad:
        print("TONIUM ", p, t)
    print(f"{len(kept)} kept (functional or deliberately left), {len(bad)} unexpected")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
