"""Parse and verify the 512-byte trailer ltibCreateDmgFile (GPL pmib) appends to a .pfw.

Layout: ext3 image (50000 KiB) + fields ver(4) tag(128) swver(32) fwver(32) bootloaderver(32)
uid(38) empty(..) md5sum(34). Each field is a 2-hex-digit length followed by the string,
zero-padded. md5sum covers everything before it.
"""
import hashlib, sys

FIELDS = [("ver", 4), ("tag", 128), ("swver", 32), ("fwver", 32), ("bootloaderver", 32), ("uid", 38)]
EMPTY = 512 - sum(n for _, n in FIELDS) - 34

def field(b):
    n = int(b[:2], 16)
    return b[2:2 + n].decode("ascii", "replace")

for path in sys.argv[1:]:
    d = open(path, "rb").read()
    img, tail = d[:-512], d[-512:]
    out, o = {}, 0
    for name, n in FIELDS:
        out[name] = field(tail[o:o + n]); o += n
    o += EMPTY
    stored = field(tail[o:o + 34])
    calc = hashlib.md5(d[:-34]).hexdigest()
    print(f"{path.split('/')[-1]:24} img={len(img)} " + " ".join(f"{k}={v}" for k, v in out.items())
          + f" md5={'OK' if stored == calc else 'MISMATCH ' + stored + ' vs ' + calc}")
