"""Recover static pointer tables (e.g. {column, type} schema arrays) from a 32-bit LE ELF.

Given an anchor string, find every word in loaded data that points at it, then walk outward
over consecutive words that also point at C strings, printing the decoded table.
"""
import struct, sys

def load(path):
    d = open(path, "rb").read()
    assert d[:4] == b"\x7fELF" and d[4] == 1 and d[5] == 1, "need ELF32 little-endian"
    phoff, = struct.unpack_from("<I", d, 0x1c); phent, phnum = struct.unpack_from("<HH", d, 0x2a)
    segs = []
    for i in range(phnum):
        t, off, va, _pa, fsz, _msz = struct.unpack_from("<IIIIII", d, phoff + i * phent)
        if t == 1:
            segs.append((va, off, fsz))
    return d, segs

def v2o(segs, va):
    for s, off, sz in segs:
        if s <= va < s + sz:
            return off + va - s
    return None

def o2v(segs, o):
    for s, off, sz in segs:
        if off <= o < off + sz:
            return s + o - off
    return None

def cstr(d, segs, va):
    o = v2o(segs, va)
    if o is None:
        return None
    e = d.find(b"\0", o, o + 600)
    if e <= o:
        return None
    s = d[o:e]
    return s.decode() if all(32 <= c < 127 for c in s) else None

def table(d, segs, at, span):
    rows = []
    for k in range(-span, span):
        o = at + 4 * k
        va, = struct.unpack_from("<I", d, o)
        rows.append((o, va, cstr(d, segs, va)))
    return rows

def main(path, anchor, span=80):
    d, segs = load(path)
    so = d.find(anchor.encode() + b"\0")
    while so != -1 and d[so - 1] != 0:
        so = d.find(anchor.encode() + b"\0", so + 1)
    va = o2v(segs, so)
    print(f"anchor {anchor!r} at file 0x{so:x} va 0x{va:x}")
    needle = struct.pack("<I", va)
    i = d.find(needle)
    while i != -1:
        print(f"--- referenced from file 0x{i:x}")
        for o, v, s in table(d, segs, i, span):
            print(f"  0x{o:08x} {v:08x} {s!r}" if s is not None else f"  0x{o:08x} {v:08x}")
        i = d.find(needle, i + 4)

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 80)
