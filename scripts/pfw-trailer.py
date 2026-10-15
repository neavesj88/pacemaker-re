"""Parse, verify and rebuild the 512-byte trailer ltibCreateDmgFile (GPL pmib) appends to a .pfw.

Layout: ext3 image (50000 KiB) + fields ver(4) tag(128) swver(32) fwver(32) bootloaderver(32)
uid(38) empty(..) md5sum(34). Each field is a 2-hex-digit length followed by the string,
zero-padded. md5sum covers everything before it.
"""
import hashlib, sys, uuid

FIELDS = [("ver", 4), ("tag", 128), ("swver", 32), ("fwver", 32), ("bootloaderver", 32), ("uid", 38)]
EMPTY = 512 - sum(n for _, n in FIELDS) - 34

def block(value, size):
    """createBlock from ltibCreateDmgFile: 2 uppercase hex digits of length (dc output), value, zero padding.
    The md5 block is written without padding (its size is exactly 2 + 32)."""
    raw = f"{len(value):02X}{value}".encode("ascii")
    return raw + b"\0" * (size - len(raw))

def build(image, sw, fw, bootloader, uid=None):
    """Return a .pfw: ext image + trailer, as ltibCreateDmgFile would produce it."""
    uid = uid or str(uuid.uuid4())
    vals = {"ver": "01", "tag": f"dfw_{sw}", "swver": str(sw), "fwver": str(fw), "bootloaderver": str(bootloader), "uid": uid}
    body = image + b"".join(block(vals[n], size) for n, size in FIELDS) + block("", EMPTY)  # the padding is itself a createBlock: ASCII "00" + NULs
    return body + block(hashlib.md5(body).hexdigest(), 34)

def field(b):
    n = int(b[:2], 16)
    return b[2:2 + n].decode("ascii", "replace")

def verify(paths):
  for path in paths:
    d = open(path, "rb").read()
    img, tail = d[:-512], d[-512:]
    out, o = {}, 0
    for name, n in FIELDS:
        out[name] = field(tail[o:o + n]); o += n
    o += EMPTY
    stored = field(tail[o:o + 34])
    calc = hashlib.md5(d[:-34]).hexdigest()
    rebuilt = build(img, out["swver"], out["fwver"], out["bootloaderver"], out["uid"]) == d
    print(f"{path.split('/')[-1]:24} img={len(img)} " + " ".join(f"{k}={v}" for k, v in out.items())
          + f" md5={'OK' if stored == calc else 'MISMATCH ' + stored + ' vs ' + calc}"
          + f" rebuild={'identical' if rebuilt else 'DIFFERS'}")

if __name__ == "__main__":
    verify(sys.argv[1:])
