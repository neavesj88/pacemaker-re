"""Rename the kernel's machine name "Tonium Pacemaker (MX31)" to "Pacemaker Revival" in boot/zImage
(notes/about-screen.md, "Tonium sweep").

The name is the `name` member of the MACHINE_START(MX31ADS, ...) struct machine_desc. The kernel prints it as
"Machine: %s" at boot (dmesg, serial console) and as the "Hardware" line of /proc/cpuinfo; nothing else reads it
(the machine is selected by the type number 0x1bf that RedBoot passes in r1, never by name). The string is 23
characters + NUL = 24 bytes in the decompressed kernel, directly followed by "pm_keypad", so the new name is
written over it in place and zero-filled: the pointer in machine_desc, the type number and every other byte of
the kernel stay as they are.

The input kernel is the one 180096073 already proved boots (patch-kernel-logo.py output, the Revival logo); the
stock zImage supplies the gzip header and the compressed span. The new stream is recompressed exactly like the
logo patch does it (same header, zero-filled up to the end of the original span, CRC32/ISIZE trailer, 7-Zip
deflate because zlib -9 alone does not fit), so the zImage stays the same size and differs from the logo-only one
only inside the payload span.

  patch-kernel-name.py STOCK_zImage LOGO_zImage OUT_zImage     build OUT (needs 7-Zip, so run it on Windows)
  patch-kernel-name.py --verify STOCK_zImage LOGO_zImage OUT_zImage
                                                               check OUT against the two inputs (no 7-Zip needed)
  defaults: work/fw-180096069/boot/zImage work/build/073/zImage-revival work/build/074/zImage-revival-name
"""
import hashlib
import importlib.util
import os
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
_spec = importlib.util.spec_from_file_location("patch_kernel_logo", os.path.join(HERE, "patch-kernel-logo.py"))
pkl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pkl)

STOCK_SHA256 = "4d7ad8312ced28d1993318619c9064150adb4f67993d9d7197e714e99dda1df3"   # stock 180096069 zImage
LOGO_SHA256 = "547d8e1ba1ba9b9c572d99ed959d0e0499122698dccc6dbf34fc2542ce8ac505"    # the 073 zImage-revival
BASE = pkl.BASE                                    # kernel VA of decompressed offset 0
MACHINE_DESC = 0x1516C                             # struct machine_desc of MX31ADS in the decompressed kernel
MACHINE_NR = 0x1BF                                 # machine type number: must not change
NAME_PTR = MACHINE_DESC + 12                       # machine_desc.name
NAME_OFF = 0x24DF14
OLD_NAME = b"Tonium Pacemaker (MX31)\0"            # 24 bytes, "pm_keypad" follows directly
NEW_NAME = b"Pacemaker Revival"                    # ASCII, 17 chars; at most 23 fit
NEXT = b"pm_keypad\0"
SLOT = len(OLD_NAME)

assert len(NEW_NAME) < SLOT and all(32 <= c < 127 for c in NEW_NAME)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def patch_kernel(k):
    """The decompressed 073 kernel with only the name slot changed. Exits on any mismatch."""
    def refuse(msg):
        sys.exit("refusing: " + msg)

    if k[NAME_OFF:NAME_OFF + SLOT] != OLD_NAME:
        refuse("machine name slot does not hold the stock name (already patched? wrong kernel?)")
    if k[NAME_OFF + SLOT:NAME_OFF + SLOT + len(NEXT)] != NEXT:
        refuse("the string after the machine name is not pm_keypad")
    if struct.unpack_from("<I", k, MACHINE_DESC)[0] != MACHINE_NR:
        refuse("machine_desc.nr is not 0x1bf")
    if struct.unpack_from("<I", k, NAME_PTR)[0] != BASE + NAME_OFF:
        refuse("machine_desc.name does not point at the name slot")
    if k.count(struct.pack("<I", BASE + NAME_OFF)) != 1:
        refuse("the name pointer is not the only word referencing the name slot")
    if k.lower().count(b"tonium") != 1:
        refuse("expected exactly one 'tonium' in the kernel")
    new = bytearray(k)
    new[NAME_OFF:NAME_OFF + SLOT] = NEW_NAME + bytes(SLOT - len(NEW_NAME))
    new = bytes(new)
    assert b"tonium" not in new.lower()
    return new


def check_inputs(stock, logo):
    if sha(stock) != STOCK_SHA256:
        sys.exit("refusing: STOCK_zImage is not the stock 180096069 zImage")
    if sha(logo) != LOGO_SHA256:
        sys.exit("refusing: LOGO_zImage is not the 180096073 zImage-revival (logo-only kernel)")
    if len(stock) != len(logo):
        sys.exit("refusing: zImage sizes differ")
    start, hdr_end, end, _ = pkl.gzip_parts(stock)
    lstart, lhdr_end, lend, lk = pkl.gzip_parts(logo)
    if (lstart, lhdr_end) != (start, hdr_end) or logo[:start] != stock[:start] or logo[end:] != stock[end:]:
        sys.exit("refusing: logo zImage differs from stock outside the payload span")
    if lend > end:
        sys.exit("refusing: logo payload overruns the stock span")
    return start, hdr_end, end, lk


def make(stock, logo):
    start, hdr_end, end, lk = check_inputs(stock, logo)
    new = patch_kernel(lk)
    body = pkl.deflate(new)
    stream = stock[start:hdr_end] + body + struct.pack("<II", zlib.crc32(new) & 0xFFFFFFFF, len(new) & 0xFFFFFFFF)
    span = end - start
    if len(stream) > span:
        sys.exit(f"recompressed kernel is {len(stream)} bytes, {len(stream) - span} more than the original span {span}")
    out = stock[:start] + stream + bytes(span - len(stream)) + stock[end:]
    return out, len(stream), span


def verify(stock, logo, out):
    """Raise AssertionError unless `out` is the logo kernel with only the name changed, recompressed into the stock span."""
    start, hdr_end, end, lk = check_inputs(stock, logo)
    assert len(out) == len(stock), "size differs from stock"
    assert out[:start] == stock[:start] and out[end:] == stock[end:], "bytes outside the payload span differ from stock"
    assert out[start:hdr_end] == stock[start:hdr_end], "gzip header differs from stock"
    ostart, ohdr_end, oend, ok = pkl.gzip_parts(out)
    assert (ostart, ohdr_end) == (start, hdr_end) and oend <= end, "payload does not fit the stock span"
    assert out[oend:end] == bytes(end - oend), "span tail is not zero-filled"
    assert len(ok) == len(lk) and ok == patch_kernel(lk), "decompressed kernel is not logo kernel + name only"
    diff = [i for i in range(len(lk)) if lk[i] != ok[i]]
    assert diff and NAME_OFF <= diff[0] and diff[-1] < NAME_OFF + SLOT, "kernel changed outside the name slot"
    trailer = struct.unpack_from("<II", out, oend - 8)
    assert trailer == (zlib.crc32(ok) & 0xFFFFFFFF, len(ok)), "gzip trailer wrong"
    return len(diff), oend - start, end - start


def main():
    args = sys.argv[1:]
    check = bool(args and args[0] == "--verify")
    if check:
        args = args[1:]
    d = lambda *p: os.path.join(ROOT, *p)
    stock_p = args[0] if len(args) > 0 else d("work", "fw-180096069", "boot", "zImage")
    logo_p = args[1] if len(args) > 1 else d("work", "build", "073", "zImage-revival")
    out_p = args[2] if len(args) > 2 else d("work", "build", "074", "zImage-revival-name")
    stock, logo = open(stock_p, "rb").read(), open(logo_p, "rb").read()
    if check:
        n, used, span = verify(stock, logo, open(out_p, "rb").read())
        print(f"verified: {out_p}: {n} kernel bytes differ from the logo kernel (all in the name slot), "
              f"payload {used} of {span} bytes, zImage {len(stock)} bytes, sha256 {sha(open(out_p, 'rb').read())}")
        return
    out, used, span = make(stock, logo)
    open(out_p, "wb").write(out)
    n, _, _ = verify(stock, logo, out)
    print(f"machine name: {n} kernel bytes changed; payload {used} of {span} bytes ({span - used} zero-filled); "
          f"zImage {len(out)} bytes, sha256 {sha(out)}")


if __name__ == "__main__":
    main()
