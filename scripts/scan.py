"""Map an opaque image: non-zero extents plus offsets of known container magics.

Pure stdlib so it runs on Windows without binwalk (rea's firmware lane is Linux-only).
"""
import sys

MAGICS = {
    b"\x27\x05\x19\x56": "uImage",
    b"\x1f\x8b\x08": "gzip",
    b"BZh9": "bzip2",
    b"\xfd7zXZ\x00": "xz",
    b"hsqs": "squashfs-le",
    b"sqsh": "squashfs-be",
    b"\x45\x3d\xcd\x28": "cramfs-le",
    b"\x85\x19": "jffs2-le?",
    b"\x7fELF": "ELF",
    b"ustar": "tar(+257)",
    b"SQLite format 3\x00": "sqlite",
    b"\xeb\x3c\x90": "FAT boot?",
    b"\x55\xaa": None,
}

def extents(buf, block=4096):
    out, start = [], None
    for off in range(0, len(buf), block):
        nz = any(buf[off:off + block])
        if nz and start is None:
            start = off
        elif not nz and start is not None:
            out.append((start, off)); start = None
    if start is not None:
        out.append((start, len(buf)))
    return out

def main(path):
    buf = open(path, "rb").read()
    print(f"{path}: {len(buf)} bytes")
    for s, e in extents(buf):
        print(f"  data 0x{s:08x}-0x{e:08x} ({(e - s) // 1024} KiB)")
    for magic, name in MAGICS.items():
        if not name or name.startswith("jffs2"):
            continue
        i, hits = buf.find(magic), []
        while i != -1 and len(hits) < 12:
            hits.append(i); i = buf.find(magic, i + 1)
        if hits:
            print(f"  {name:12} " + " ".join(f"0x{h:x}" for h in hits) + (" ..." if len(hits) == 12 else ""))
    # ext2/3 superblock magic sits at +0x438 from partition start
    for s, _ in extents(buf):
        for base in (s, s - 0x400):
            if base >= 0 and buf[base + 0x438:base + 0x43a] == b"\x53\xef":
                print(f"  ext2/3/4 superblock for partition at 0x{base:x}")

if __name__ == "__main__":
    for p in sys.argv[1:]:
        main(p)
