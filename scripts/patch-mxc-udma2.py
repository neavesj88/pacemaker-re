#!/usr/bin/env python3
"""Make the i.MX31 IDE host driver switch DMA on at UDMA2 instead of UDMA3 (notes/sd-adapter-dma.md, 075).

mxc_ide_dma_on (hwif->ide_dma_on, run by `hdparm -d 1` and by the wrapper's re-enable after a standby) ignores the
drive's ID words and the mode asked for: it programs the host for UDMA3 and sends SET FEATURES UDMA3 to the drive,
both with a hard-coded 0x43, and ignores the drive's answer. The wrapper's following `hdparm -X udma2` only moves
the host if the drive accepts its SET FEATURES (ide_cmd_ioctl sets the rate only on success). On the SD adapter the
073 diag showed current_speed 67 (UDMA3) after an attempt at udma2: the self-test ran at UDMA3, loads then took
about 30 s and the next boot asked to check the drive. Three bytes change:

  mxc_ide.ko .text+0xb80   mov r1, #0x43  ->  mov r1, #0x42   host timing (mxc_ide_set_speed: UDMA path, index 2)
  mxc_ide.ko .text+0xb98   mov r1, #0x43  ->  mov r1, #0x42   SET FEATURES to the drive (ide_config_drive_speed)
  .rodata.str1.4+0x144     "<6>%s: enabling UDMA3 mode\\n" -> "...UDMA2..." so the kernel log says what happens

Nothing else in the module changes, and nothing turns DMA on by itself: the auto path still needs the DMA bit the
adapter does not report, so DMA stays off until the wrapper's revival-dma attempt asks for it. A drive that reports
DMA (the stock HDD) gets UDMA2 from `hdparm -d 1` too, then the boot-time `-X udma5` (hdpboot runs the stock
`hdparm -d 1`, pwrApplication then `-X udma5`) moves it up exactly as before when the drive accepts it.

Usage: patch-mxc-udma2.py [mxc_ide.ko [OUT.ko]]
  defaults: work/rootfs-180096069/lib/modules/2.6.19.2/kernel/drivers/ide/arm/mxc_ide.ko -> work/build/mxc_ide-udma2.ko
"""
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STOCK = os.path.join(ROOT, "work", "rootfs-180096069", "lib", "modules", "2.6.19.2", "kernel", "drivers", "ide",
                     "arm", "mxc_ide.ko")
OUT = os.path.join(ROOT, "work", "build", "mxc_ide-udma2.ko")
STOCK_SHA256 = "51e38ced2ca9b154aa631b97f26a738b1e3ad3ea4f9689cfa9de61f478931326"
STOCK_SIZE = 14016

UDMA3 = bytes.fromhex("4310a0e3")   # mov r1, #0x43
UDMA2 = bytes.fromhex("4210a0e3")   # mov r1, #0x42

# (offset within .text, what) of the two immediates, and the instructions around them as the analysis saw them
SITES = [(0x0b80, "mxc_ide_set_speed(drive, 0x43): host timing"),
         (0x0b98, "ide_config_drive_speed(drive, 0x43): SET FEATURES")]
CONTEXT = [
    (0x0b4c, "0dc0a0e1", "mov ip, sp (mxc_ide_dma_on entry)"),
    (0x0b7c, "2c6094e5", "ldr r6,[r4,#44] (drive->hwif)"),
    (0x0b84, "0400a0e1", "mov r0, r4"),
    (0x0b90, "6afeffeb", "bl mxc_ide_set_speed (0x540)"),
    (0x0b9c, "0400a0e1", "mov r0, r4"),
    (0x0ba4, "feffffeb", "bl ide_config_drive_speed (relocated)"),
    # mxc_ide_set_speed: UDMA modes 0x40..0x47 take the UDMA branch with index speed - 0x40
    (0x0550, "470051e3", "cmp r1, #0x47"),
    (0x0564, "400052e3", "cmp r2, #0x40"),
    (0x0568, "400041e2", "sub r0, r1, #0x40"),
]
MSG_OLD = b"<6>%s: enabling UDMA3 mode\n\0"
MSG_NEW = b"<6>%s: enabling UDMA2 mode\n\0"
MSG_OFF = 0x144            # within .rodata.str1.4 (the pool word at .text+0xc50 points here)


def refuse(msg):
    sys.exit("refusing: " + msg)


def sections(d):
    if d[:4] != b"\x7fELF" or d[4] != 1 or d[5] != 1:
        refuse("not an ELF32 little-endian file")
    e_shoff, = struct.unpack_from("<I", d, 0x20)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", d, 0x2e)
    hdrs = [struct.unpack_from("<10I", d, e_shoff + i * e_shentsize) for i in range(e_shnum)]
    so = hdrs[e_shstrndx][4]
    return {d[so + h[0]:d.index(b"\0", so + h[0])].decode(): h for h in hdrs}


def apply(d):
    if len(d) != STOCK_SIZE or hashlib.sha256(d).hexdigest() != STOCK_SHA256:
        refuse("input is not the stock 180096069 mxc_ide.ko (size or sha256 differs; already patched?)")
    secs = sections(d)
    t_off, t_size = secs[".text"][4], secs[".text"][5]
    s_off, s_size = secs[".rodata.str1.4"][4], secs[".rodata.str1.4"][5]
    rel = secs[".rel.text"]
    relocs = [struct.unpack_from("<II", d, rel[4] + i * 8)[0] for i in range(rel[5] // 8)]
    for va, want, what in CONTEXT:
        got = d[t_off + va:t_off + va + 4].hex()
        if got != want:
            refuse(f"context {what} at .text+{va:#x} is {got}, expected {want}")
    out = bytearray(d)
    for va, what in SITES:
        if va + 4 > t_size:
            refuse(f"{what}: outside .text")
        f = t_off + va
        if d[f:f + 4] != UDMA3:
            refuse(f"{what}: .text+{va:#x} is {d[f:f+4].hex()}, expected {UDMA3.hex()}")
        if any(va - 3 <= r <= va + 3 for r in relocs):
            refuse(f"{what}: a relocation overlaps .text+{va:#x}")
        out[f:f + 4] = UDMA2
    # the message: the pool word at .text+0xc50 is relocated against .rodata.str1.4 with addend 0x144
    if not (0x0c50 in relocs and struct.unpack_from("<I", d, t_off + 0x0c50)[0] == MSG_OFF):
        refuse("the printk pool word at .text+0xc50 does not point at .rodata.str1.4+0x144")
    m = s_off + MSG_OFF
    if MSG_OFF + len(MSG_OLD) > s_size or d[m:m + len(MSG_OLD)] != MSG_OLD:
        refuse(f"message at .rodata.str1.4+{MSG_OFF:#x} is {d[m:m+len(MSG_OLD)]!r}")
    out[m:m + len(MSG_NEW)] = MSG_NEW
    return bytes(out)


def main():
    args = sys.argv[1:]
    src = args[0] if args else STOCK
    dst = args[1] if len(args) > 1 else OUT
    d = open(src, "rb").read()
    out = apply(d)
    diff = [i for i in range(len(d)) if d[i] != out[i]]
    if len(out) != len(d) or len(diff) != 3:
        refuse(f"expected exactly 3 changed bytes, got {len(diff)}")
    os.makedirs(os.path.dirname(os.path.abspath(dst)), exist_ok=True)
    tmp = dst + ".tmp"
    with open(tmp, "wb") as f:
        f.write(out)
    os.replace(tmp, dst)
    if open(dst, "rb").read() != out:
        refuse("the written file reads back differently")
    secs = sections(out)
    t_off = secs[".text"][4]
    for va, what in SITES:
        print(f"{what}: .text+{va:#x} (file {t_off + va:#x}) {UDMA3.hex()} -> {UDMA2.hex()}")
    print(f"message: enabling UDMA2 mode; changed bytes at {', '.join(hex(i) for i in diff)}")
    print(f"{dst}  sha256 {hashlib.sha256(out).hexdigest()}")


if __name__ == "__main__":
    main()
