"""Patch the 2.6.19.2 ide-disk.ko so an LBA-capable drive whose IDENTIFY word 49 is garbled is
still treated as LBA (notes/sd-adapter-lba.md).

The SD-to-ZIF adapter's model string overflows into words 48-53, so word 49 (capabilities) reads
0x6c61 and the LBA bit looks clear. In ide_disk_probe (.text 0x1528, init_idedisk_capacity inlined)
the check `if (id->capability & 2) && lba_capacity_is_ok(id)` is compiled as

    1d2c  ldrb r3, [r4, #99]     id->capability
    1d30  tst  r3, #2
    1d34  beq  1a88              no LBA bit -> CHS capacity           <-- patched to nop
    1d38  b    19b8              lba_capacity_is_ok() inlined, then LBA28 capacity

With the branch gone the driver always runs the stock lba_capacity_is_ok() sanity check on words
60-61; drives that report LBA take exactly the same path as before (the branch was not taken for
them). Nothing else changes (DMA, LBA48 and the HPA logic are untouched).

  patch-ide-disk.py [STOCK.ko [OUT.ko]]
  defaults: work/rootfs-180096069/lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko
            -> work/build/ide-disk-lba.ko
"""
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STOCK = os.path.join(ROOT, "work", "rootfs-180096069", "lib", "modules", "2.6.19.2",
                     "kernel", "drivers", "ide", "ide-disk.ko")
OUT = os.path.join(ROOT, "work", "build", "ide-disk-lba.ko")

TEXT_VA = 0x1d34                       # `beq 1a88` inside ide_disk_probe
# (offset within .text, expected bytes) - all must match before anything is written
CONTEXT = [
    (0x1d2c, bytes.fromhex("6330d4e5"), "ldrb r3,[r4,#99]"),
    (0x1d30, bytes.fromhex("020013e3"), "tst r3,#2"),
    (0x1d38, bytes.fromhex("1effffea"), "b 19b8"),
]
OLD = bytes.fromhex("53ffff0a")        # beq 0x1a88
NEW = bytes.fromhex("0000a0e1")        # mov r0, r0 (nop)


def sections(d):
    assert d[:4] == b"\x7fELF" and d[4] == 1 and d[5] == 1, "not ELF32 little-endian"
    e_shoff, = struct.unpack_from("<I", d, 0x20)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", d, 0x2e)
    hdrs = [struct.unpack_from("<10I", d, e_shoff + i * e_shentsize) for i in range(e_shnum)]
    strtab = hdrs[e_shstrndx]
    names = []
    for h in hdrs:
        s = strtab[4] + h[0]
        names.append(d[s:d.index(b"\0", s)].decode())
    return {n: h for n, h in zip(names, hdrs)}, d[e_shoff:e_shoff + e_shentsize * e_shnum]


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else STOCK
    dst = sys.argv[2] if len(sys.argv) > 2 else OUT
    d = open(src, "rb").read()
    secs, shtab = sections(d)
    text = secs[".text"]
    t_off, t_size = text[4], text[5]
    assert TEXT_VA + 4 <= t_size
    f = t_off + TEXT_VA                # file offset of the patch site

    # 1. stock bytes must be exactly what the analysis saw
    if d[f:f + 4] != OLD:
        sys.exit(f"refusing: bytes at .text+{TEXT_VA:#x} (file {f:#x}) are {d[f:f+4].hex()}, "
                 f"expected {OLD.hex()} (already patched or a different module)")
    for va, exp, what in CONTEXT:
        got = d[t_off + va:t_off + va + 4]
        if got != exp:
            sys.exit(f"refusing: context {what} at .text+{va:#x} is {got.hex()}, expected {exp.hex()}")

    # 2. the site must not be covered by a relocation
    rel = secs[".rel.text"]
    for i in range(rel[5] // 8):
        r_off, r_info = struct.unpack_from("<II", d, rel[4] + i * 8)
        if f - t_off - 3 <= r_off <= f - t_off + 3:
            sys.exit(f"refusing: relocation at .text+{r_off:#x} overlaps the patch site")

    # 3. patch and verify
    out = bytearray(d)
    out[f:f + 4] = NEW
    out = bytes(out)
    assert len(out) == len(d)
    diff = [i for i in range(len(d)) if d[i] != out[i]]
    assert diff and diff[0] >= f and diff[-1] < f + 4, "unexpected change outside the site"
    secs2, shtab2 = sections(out)
    assert shtab2 == shtab and secs2 == secs, "section table changed"
    for name in ("__versions", ".modinfo", ".rel.text", ".symtab", ".strtab", ".data",
                 ".rodata.str1.4", ".gnu.linkonce.this_module"):
        h = secs[name]
        assert out[h[4]:h[4] + h[5]] == d[h[4]:h[4] + h[5]], f"{name} changed"
    assert out[f:f + 4] == NEW

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    open(dst, "wb").write(out)
    back = open(dst, "rb").read()
    assert back == out and back[f:f + 4] == NEW
    print(f"ok: {os.path.relpath(dst, ROOT)}  {len(back)} bytes, 4 bytes changed at file offset "
          f"{f:#x} (.text+{TEXT_VA:#x}): {OLD.hex()} -> {NEW.hex()}")
    print("sha256 stock", hashlib.sha256(d).hexdigest())
    print("sha256 patch", hashlib.sha256(out).hexdigest())


if __name__ == "__main__":
    main()
