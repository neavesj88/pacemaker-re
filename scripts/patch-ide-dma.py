#!/usr/bin/env python3
"""Let DMA be switched on by hand for the SD adapter, and keep it off at boot (notes/sd-adapter-dma.md).

The Sintech/iFlash-clone adapter's IDENTIFY block is corrupt: word 49 reads 0x6c61, so id->capability
(its high byte, 0x6c) has the DMA bit (bit 0) clear. Every DMA decision in the 2.6.19.2 IDE stack that
could be reached from userspace is gated on that bit, so `hdparm -d 1` fails with EPERM and the drive
stays in PIO. Two instructions in ide-core.ko are changed:

  ide-core.ko  set_using_dma (the HDIO_SET_DMA / /proc/ide/hda/settings "using_dma" handler)

      0de0  ldrb r3, [r3, #99]     id->capability
      0de4  tst  r3, #1
      0de8  beq  0e34              no DMA bit -> return -EPERM          <-- nop

  With the branch gone, `hdparm -d 1` runs the driver's own mxc_ide_dma_check (which, for this ID, only
  switches DMA off quietly and returns 0) and then mxc_ide_dma_on (UDMA3 on host and drive, hard-coded
  0x43). `hdparm -d 0` also stops failing, which the fallback in the wrapper needs. Nothing in the
  driver turns DMA on by itself for this ID: the auto path (mxc_ide_dma_check at module load and after a
  resume) still requires the DMA bit, so DMA stays off until userspace asks.

  ide-core.ko  ide_do_request, PM resume step (HDD standby / wake, Tonium's ide_resume)

      35f8  ldrb r3, [r3, #99]     id->capability
      35fc  tst  r3, #1
      3600  beq  361c              no DMA bit -> skip hwif->ide_dma_check  <-- nop

  The standby path cuts the drive's power. Without this change a drive with using_dma = 1 would come
  back un-negotiated while the driver still believed in DMA. With it, resume calls mxc_ide_dma_check,
  which switches DMA off for this ID (it needs the DMA bit), so the drive wakes in plain PIO; the wrapper's watcher
  (pmplayer-wrapper.sh) turns it back on within 5 s when it was verified earlier in the boot, else the
  next player start decides. Drives that report the bit take the same path as before.

pwrApplication (optional, the boot-time `hdparm -d 1`)

  HddInterface::loadHddModules runs system("hdparm -d 1 /dev/hda") on every boot, before anything is
  mounted. With ide-core patched that call would now succeed for the SD adapter and put it in UDMA3
  before the first mount, with no way to condition it on a flag file (the FAT volume is not mounted
  yet). The string is changed to "/sbin/hdpboot" (padded with NULs to the same length). That script,
  scripts/device/hdparm-boot.sh, runs the stock "hdparm -d 1 /dev/hda" for a drive that reports DMA
  (IDENTIFY word 49 bit 8) and does nothing for one that does not, so a stock HDD boots exactly as before
  whatever image is installed, and the adapter stays in PIO until the wrapper's flag-file attempt.
  The script must be in the rootfs (build-rootfs.sh "+/sbin/hdpboot=scripts/device/hdparm-boot.sh").

Usage: patch-ide-dma.py [ide-core.ko [OUT.ko [pwrApplication [OUT]]]]
  ide-core.ko     default work/rootfs-180096069/lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko
  OUT.ko          default work/build/ide-core-dma.ko
  pwrApplication  default work/build/pwrApplication-ring if it exists (the recoloured one that is
                  installed), else the stock work/rootfs-180096069/usr/local/bin/pwrApplication
  OUT             default work/build/<input name>-nodma
Pass "-" as the pwrApplication to patch only the module.
"""
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STOCK_CORE = os.path.join(ROOT, "work", "rootfs-180096069", "lib", "modules", "2.6.19.2",
                          "kernel", "drivers", "ide", "ide-core.ko")
OUT_CORE = os.path.join(ROOT, "work", "build", "ide-core-dma.ko")
STOCK_PWR = os.path.join(ROOT, "work", "rootfs-180096069", "usr", "local", "bin", "pwrApplication")
RING_PWR = os.path.join(ROOT, "work", "build", "pwrApplication-ring")

NOP = bytes.fromhex("0000a0e1")        # mov r0, r0

# (offset within .text, expected bytes, what) - every one must match before anything is written
CORE_SITES = [
    {
        "name": "set_using_dma: DMA bit gate",
        "patch": (0x0de8, bytes.fromhex("1100000a"), NOP),               # beq 0xe34 -> nop
        "context": [
            (0x0dd0, bytes.fromhex("203090e5"), "ldr r3,[r0,#32]  (drive->id)"),
            (0x0de0, bytes.fromhex("6330d3e5"), "ldrb r3,[r3,#99] (id->capability)"),
            (0x0de4, bytes.fromhex("010013e3"), "tst r3,#1"),
            (0x0dec, bytes.fromhex("2c3090e5"), "ldr r3,[r0,#44]  (drive->hwif)"),
            (0x0df0, bytes.fromhex("143593e5"), "ldr r3,[r3,#1300] (hwif->ide_dma_check)"),
            (0x0e34, bytes.fromhex("0000e0e3"), "mvn r0,#0 (-EPERM return)"),
        ],
    },
    {
        "name": "ide_do_request: DMA bit gate on PM resume",
        "patch": (0x3600, bytes.fromhex("0500000a"), NOP),               # beq 0x361c -> nop
        "context": [
            (0x35f4, bytes.fromhex("203097e5"), "ldr r3,[r7,#32]  (drive->id)"),
            (0x35f8, bytes.fromhex("6330d3e5"), "ldrb r3,[r3,#99] (id->capability)"),
            (0x35fc, bytes.fromhex("010013e3"), "tst r3,#1"),
            (0x3604, bytes.fromhex("2c3097e5"), "ldr r3,[r7,#44]  (drive->hwif)"),
            (0x3608, bytes.fromhex("143593e5"), "ldr r3,[r3,#1300] (hwif->ide_dma_check)"),
            (0x361c, bytes.fromhex("0030e0e3"), "mvn r3,#0"),
        ],
    },
]

PWR_VA = 0x1bad48                       # string address; file offset = va - 0x8000 (text LOAD segment)
PWR_OFF = PWR_VA - 0x8000
PWR_OLD = b"hdparm -d 1 /dev/hda\0"
PWR_NEW = b"/sbin/hdpboot\0".ljust(len(PWR_OLD), b"\0")   # scripts/device/hdparm-boot.sh, installed there


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


def patch_core(src, dst):
    d = open(src, "rb").read()
    secs, shtab = sections(d)
    t_off, t_size = secs[".text"][4], secs[".text"][5]
    rel = secs[".rel.text"]
    reloc_offs = [struct.unpack_from("<II", d, rel[4] + i * 8)[0] for i in range(rel[5] // 8)]

    out = bytearray(d)
    sites = []
    for site in CORE_SITES:
        va, old, new = site["patch"]
        assert va + 4 <= t_size
        f = t_off + va
        # 1. stock bytes and the instructions around the site must be exactly what the analysis saw
        if d[f:f + 4] != old:
            sys.exit(f"refusing: [{site['name']}] bytes at .text+{va:#x} (file {f:#x}) are "
                     f"{d[f:f+4].hex()}, expected {old.hex()} (already patched or a different module)")
        for cva, exp, what in site["context"]:
            got = d[t_off + cva:t_off + cva + 4]
            if got != exp:
                sys.exit(f"refusing: [{site['name']}] context {what} at .text+{cva:#x} is {got.hex()}, "
                         f"expected {exp.hex()}")
        # 2. neither the site nor the capability load/test before it may be covered by a relocation
        if any(va - 8 <= r <= va + 3 for r in reloc_offs):
            sys.exit(f"refusing: [{site['name']}] a relocation overlaps .text+{va - 8:#x}..{va + 3:#x}")
        out[f:f + 4] = new
        sites.append((va, f, old, new))
    out = bytes(out)

    # 3. verify: same size, only the patch bytes differ, everything else byte-identical
    assert len(out) == len(d)
    diff = [i for i in range(len(d)) if d[i] != out[i]]
    allowed = {f + k for _, f, _, _ in sites for k in range(4)}
    assert diff and all(i in allowed for i in diff), "unexpected change outside the sites"
    secs2, shtab2 = sections(out)
    assert shtab2 == shtab and secs2 == secs, "section table changed"
    for name in ("__versions", ".modinfo", ".rel.text", ".symtab", ".strtab", ".data",
                 ".rodata", ".rodata.str1.4", ".gnu.linkonce.this_module"):
        if name in secs:
            h = secs[name]
            assert out[h[4]:h[4] + h[5]] == d[h[4]:h[4] + h[5]], f"{name} changed"
    for _, f, _, new in sites:
        assert out[f:f + 4] == new

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    open(dst, "wb").write(out)
    back = open(dst, "rb").read()
    assert back == out
    print(f"ok: {os.path.relpath(dst, ROOT)}  {len(back)} bytes, {len(diff)} bytes changed")
    for va, f, old, new in sites:
        print(f"    .text+{va:#x} (file {f:#x}): {old.hex()} -> {new.hex()}")
    print("sha256 stock", hashlib.sha256(d).hexdigest())
    print("sha256 patch", hashlib.sha256(out).hexdigest())


def patch_pwr(src, dst):
    d = open(src, "rb").read()
    assert d[:4] == b"\x7fELF"
    # the text LOAD segment must map file 0 at va 0x8000 so that va - 0x8000 is the file offset
    e_phoff, = struct.unpack_from("<I", d, 0x1c)
    e_phentsize, e_phnum = struct.unpack_from("<HH", d, 0x2a)
    loads = [struct.unpack_from("<8I", d, e_phoff + i * e_phentsize) for i in range(e_phnum)]
    text = [p for p in loads if p[0] == 1 and p[1] == 0 and p[2] == 0x8000]
    if not text or not (text[0][2] <= PWR_VA < text[0][2] + text[0][4]):
        sys.exit("refusing: pwrApplication segment layout is not the 180096069 one")
    if d[PWR_OFF:PWR_OFF + len(PWR_OLD)] != PWR_OLD:
        sys.exit(f"refusing: bytes at file {PWR_OFF:#x} are {d[PWR_OFF:PWR_OFF+len(PWR_OLD)]!r}, "
                 f"expected {PWR_OLD!r} (already patched or a different binary)")
    if d.count(b"hdparm -d 1 /dev/hda") != 1:
        sys.exit("refusing: the string occurs more than once")
    # the neighbours in the string table, and exactly one literal-pool word pointing at the string
    before = d[PWR_OFF - 32:PWR_OFF]
    after = d[PWR_OFF + 24:PWR_OFF + 24 + 24]
    if b"modprobe mxc_ide > /dev/null" not in before or not after.startswith(b"hdparm -X udma5 /dev/hda"):
        sys.exit("refusing: the strings around it are not the expected boot sequence")
    refs = [i for i in range(0, len(d) - 3, 4) if d[i:i + 4] == struct.pack("<I", PWR_VA)]
    if refs != [0x61dac]:
        sys.exit(f"refusing: pool references to the string are at {[hex(r) for r in refs]}, expected 0x61dac "
                 f"(HddInterface::loadHddModules)")
    out = bytearray(d)
    out[PWR_OFF:PWR_OFF + len(PWR_NEW)] = PWR_NEW
    out = bytes(out)
    diff = [i for i in range(len(d)) if d[i] != out[i]]
    assert len(out) == len(d) and diff and all(PWR_OFF <= i < PWR_OFF + len(PWR_OLD) for i in diff), diff
    assert out[PWR_OFF:PWR_OFF + len(PWR_OLD)] == PWR_NEW

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    open(dst, "wb").write(out)
    assert open(dst, "rb").read() == out
    print(f"ok: {os.path.relpath(dst, ROOT)}  {len(out)} bytes, {len(diff)} bytes changed at file offset "
          f"{PWR_OFF:#x}: {PWR_OLD[:-1].decode()!r} -> {PWR_NEW.rstrip(bytes(1)).decode()!r}")
    print("sha256 in   ", hashlib.sha256(d).hexdigest())
    print("sha256 patch", hashlib.sha256(out).hexdigest())


def main():
    a = sys.argv[1:]
    core_src = a[0] if len(a) > 0 else STOCK_CORE
    core_dst = a[1] if len(a) > 1 else OUT_CORE
    patch_core(core_src, core_dst)
    pwr_src = a[2] if len(a) > 2 else (RING_PWR if os.path.exists(RING_PWR) else STOCK_PWR)
    if pwr_src != "-":
        pwr_dst = a[3] if len(a) > 3 else os.path.join(ROOT, "work", "build", os.path.basename(pwr_src) + "-nodma")
        patch_pwr(pwr_src, pwr_dst)


if __name__ == "__main__":
    main()
