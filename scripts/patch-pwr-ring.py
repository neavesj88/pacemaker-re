#!/usr/bin/env python3
"""Recolour the power-on spinning ring in pwrApplication (firmware 180096069) to the L2 magenta scheme.

PwrComponent::render (0x61dd0) draws 8 arcs and reads each colour with memcpy(.., ptr, 4) from a
literal-pool pointer to a du::donut*Color global in .bss (set by _INIT_5 at run time). Those
globals are also used by ErrorComponent / ErrorWindowComponent (donutEQCursorColor), so the
initialiser is left alone. Instead the 8 pool pointers are repointed at 8 new RGBA constants
stored in the spare slots of .dynamic that follow DT_NULL (ld.so stops at the first DT_NULL,
the binary is not prelinked). Only 8 pool words + 32 data bytes change; size is unchanged.

Usage: patch-pwr-ring.py [stock [out]]
  stock  default work/rootfs-180096069/usr/local/bin/pwrApplication
  out    default work/build/pwrApplication-ring
"""
import hashlib, os, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STOCK = os.path.join(ROOT, 'work/rootfs-180096069/usr/local/bin/pwrApplication')
OUT = os.path.join(ROOT, 'work/build/pwrApplication-ring')

# vaddr -> file offset for the two LOAD segments: file = va - 0x8000 (text) / va - 0x10000 (data)
def text_off(va): return va - 0x8000
def data_off(va): return va - 0x10000

# (pool word vaddr in render's literal pool, stock pointer, role, quadrant, colour R,G,B)
# dim ring = the bright L2 colour at 55% (33% read murky purple on the washed-out panel)
# draw order in render: 4 dim arcs (sweep pi/2) then 4 bright cursor ticks (sweep 0.001 rad),
# each set at start angle +0, +pi/2, +pi, +3pi/2.
SITES = [
    (0x62220, 0x1f8d15, 'dim    Q0 (donutGainPassedColor)', (0x7b, 0x3e, 0x8c)),
    (0x62230, 0x1f8d39, 'dim    Q1 (donut2pPassedColor)',   (0x8c, 0x2c, 0x8c)),
    (0x62234, 0x1f8d2d, 'dim    Q2 (donut1pPassedColor)',   (0x8c, 0x2c, 0x72)),
    (0x62238, 0x1f8d21, 'dim    Q3 (donutEQPassedColor)',   (0x8c, 0x28, 0x5c)),
    (0x6223c, 0x1f8d19, 'bright Q0 (donutGainCursorColor)', (0xe0, 0x70, 0xff)),
    (0x62240, 0x1f8d3d, 'bright Q1 (donut2pCursorColor)',   (0xff, 0x50, 0xff)),
    (0x62244, 0x1f8d31, 'bright Q2 (donut1pCursorColor)',   (0xff, 0x50, 0xd0)),
    (0x62248, 0x1f8d25, 'bright Q3 (donutEQCursorColor)',   (0xff, 0x48, 0xa8)),
]
NEW_BASE = 0x1f7ab4          # vaddr of the first new colour (file 0x1e7ab4)
DYN_OFF, DYN_END = 0x1e79ac, 0x1e7ae4   # .dynamic file range (vaddr 0x1f79ac..0x1f7ae4)


def build(stock):
    d = bytearray(stock)
    # 1. the spare area must follow DT_NULL and be all zero
    off, null_at = DYN_OFF, None
    while off < DYN_END:
        if struct.unpack_from('<I', d, off)[0] == 0:
            null_at = off
            break
        off += 8
    if null_at is None or null_at + 8 + 32 > DYN_END:
        sys.exit('refuse: no DT_NULL / not enough spare .dynamic room (%r)' % null_at)
    if data_off(NEW_BASE) < null_at + 8:
        sys.exit('refuse: new data would overwrite the DT_NULL terminator')
    if any(d[null_at:DYN_END]):
        sys.exit('refuse: .dynamic after DT_NULL (0x%x..0x%x) is not all zero' % (null_at, DYN_END))
    # 2. stock pool words must match
    for va, ptr, role, _ in SITES:
        got = struct.unpack_from('<I', d, text_off(va))[0]
        if got != ptr:
            sys.exit('refuse: word at 0x%x is 0x%08x, expected 0x%08x (%s)' % (va, got, ptr, role))
    # 3. apply
    for i, (va, ptr, role, rgb) in enumerate(SITES):
        new_va = NEW_BASE + 4 * i
        struct.pack_into('<I', d, text_off(va), new_va)
        d[data_off(new_va):data_off(new_va) + 4] = bytes(rgb) + b'\xff'   # R,G,B,A in memory
    return bytes(d)


def main():
    stock_p = sys.argv[1] if len(sys.argv) > 1 else STOCK
    out_p = sys.argv[2] if len(sys.argv) > 2 else OUT
    stock = open(stock_p, 'rb').read()
    out = build(stock)
    # prove: same size, and only the intended bytes differ
    assert len(out) == len(stock), 'size changed'
    expect = set()
    for i, (va, _, _, _) in enumerate(SITES):
        expect.update(range(text_off(va), text_off(va) + 4))
        expect.update(range(data_off(NEW_BASE + 4 * i), data_off(NEW_BASE + 4 * i) + 4))
    diff = {i for i in range(len(stock)) if stock[i] != out[i]}
    assert diff <= expect, 'unexpected bytes differ: %s' % sorted(hex(i) for i in diff - expect)
    os.makedirs(os.path.dirname(out_p), exist_ok=True)
    open(out_p, 'wb').write(out)
    print('stock  %s  %d bytes  sha256 %s' % (stock_p, len(stock), hashlib.sha256(stock).hexdigest()))
    print('patched %s  %d bytes  sha256 %s' % (out_p, len(out), hashlib.sha256(out).hexdigest()))
    print('%d bytes differ (of %d allowed): 8 pool words + 8 colours' % (len(diff), len(expect)))
    for i, (va, ptr, role, rgb) in enumerate(SITES):
        new_va = NEW_BASE + 4 * i
        o = text_off(va)
        print('  pool 0x%05x (file 0x%06x): %s -> %s  | 0x%x := %02x%02x%02x ff   %s' % (
            va, o, stock[o:o + 4].hex(), out[o:o + 4].hex(), new_va, *rgb, role))
    # re-read the written file and check it end to end
    chk = open(out_p, 'rb').read()
    assert chk == out
    for i, (va, _, _, rgb) in enumerate(SITES):
        p = struct.unpack_from('<I', chk, text_off(va))[0]
        assert p == NEW_BASE + 4 * i and tuple(chk[data_off(p):data_off(p) + 3]) == rgb
        assert chk[data_off(p) + 3] == 0xff
    print('verify: size unchanged, only intended bytes differ, pool -> colours read back OK')


if __name__ == '__main__':
    main()
