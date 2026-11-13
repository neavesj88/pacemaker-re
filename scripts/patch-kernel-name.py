"""Rename the kernel's machine name "Tonium Pacemaker (MX31)" to "Pacemaker Revival" in boot/zImage
(notes/about-screen.md, "Tonium sweep").

The name is the `name` member of the MACHINE_START(MX31ADS, ...) struct machine_desc. The kernel prints it as
"Machine: %s" at boot (dmesg, serial console) and as the "Hardware" line of /proc/cpuinfo; nothing else reads it
(the machine is selected by the type number 0x1bf that RedBoot passes in r1, never by name). The string is 23
characters + NUL = 24 bytes in the decompressed kernel, directly followed by "pm_keypad", so the new name is
written over it in place and zero-filled: the pointer in machine_desc, the type number and every other byte of
the kernel stay as they are.

The kernel also carries its build configuration (IKCONFIG, served as /proc/config.gz): a gzip stream between the
markers IKCFG_ST and IKCFG_ED inside the decompressed kernel, 7 347 bytes in the stock one. It contains the symbol
name CONFIG_LOGO_TONIUM_CLUT224=y, the only "Tonium" left once the name is changed (a literal scan of the
decompressed kernel does not see it because the stream is compressed twice). The line is rewritten as
CONFIG_LOGO_CUSTOM_CLUT224=y (same length), the stream is recompressed with zlib -9 and padded to exactly the old
length with a gzip FCOMMENT field, so the marker positions and every other kernel byte stay where they are and
`zcat /proc/config.gz` still reads one clean gzip member. The compiled-in logo identifier (logo_tonium_clut224 in
the symbol table) is not text in the image that can be edited in place and is not touched; see the notes.

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
CFG_START, CFG_END = b"IKCFG_ST", b"IKCFG_ED"       # markers around the gzip'd build config (/proc/config.gz)
OLD_CFG_LINE = b"CONFIG_LOGO_TONIUM_CLUT224=y\n"
NEW_CFG_LINE = b"CONFIG_LOGO_CUSTOM_CLUT224=y\n"    # same length

assert len(NEW_NAME) < SLOT and all(32 <= c < 127 for c in NEW_NAME)
assert len(OLD_CFG_LINE) == len(NEW_CFG_LINE)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def config_span(k):
    """(start, end) of the gzip'd build config between IKCFG_ST and IKCFG_ED, or exit."""
    if k.count(CFG_START) != 1 or k.count(CFG_END) != 1:
        sys.exit("refusing: expected exactly one IKCFG_ST and one IKCFG_ED in the kernel")
    a, b = k.index(CFG_START) + len(CFG_START), k.index(CFG_END)
    if not 0 < b - a < 65536:
        sys.exit("refusing: implausible build config span")
    return a, b


def read_config(gz):
    """The text of a gzip member that must fill `gz` exactly, as the zcat of /proc/config.gz sees it."""
    d = zlib.decompressobj(16 + zlib.MAX_WBITS)
    text = d.decompress(gz)
    if not d.eof or d.unused_data:
        sys.exit("refusing: build config is not exactly one gzip member")
    return text


def patch_config(gz):
    """The build config gzip member with the logo symbol renamed, recompressed to exactly len(gz) bytes."""
    text = read_config(gz)
    if text.count(OLD_CFG_LINE) != 1 or text.lower().count(b"tonium") != 1:
        sys.exit("refusing: build config does not hold exactly one stock CONFIG_LOGO_TONIUM_CLUT224=y line")
    if gz[3] != 0:
        sys.exit("refusing: build config gzip header has flags set")
    new_text = text.replace(OLD_CFG_LINE, NEW_CFG_LINE)
    c = zlib.compressobj(9, zlib.DEFLATED, -zlib.MAX_WBITS, 9)
    body = c.compress(new_text) + c.flush()
    trailer = struct.pack("<II", zlib.crc32(new_text) & 0xFFFFFFFF, len(new_text) & 0xFFFFFFFF)
    pad = len(gz) - (10 + len(body) + 8)
    if pad < 0:
        sys.exit(f"refusing: recompressed build config is {-pad} bytes too long")
    # pad == 0: keep the 10-byte header as is. Otherwise set FCOMMENT and let the comment fill the difference
    # (the comment is `pad - 1` filler bytes plus its NUL).
    hdr = bytearray(gz[:10])
    comment = b""
    if pad:
        hdr[3] = 0x10
        comment = b"." * (pad - 1) + b"\0"
    out = bytes(hdr) + comment + body + trailer
    assert len(out) == len(gz) and read_config(out) == new_text
    return out


def patch_kernel(k):
    """The decompressed 073 kernel with only the name slot and the build config's logo symbol changed. Exits on any mismatch."""
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
    a, b = config_span(k)
    new[a:b] = patch_config(bytes(k[a:b]))
    new = bytes(new)
    assert b"tonium" not in new.lower()
    assert b"tonium" not in read_config(new[a:b]).lower()
    return new


def check_kernel(lk, ok):
    """Raise AssertionError unless `ok` is the logo kernel `lk` with only the name slot and the build config's logo
    symbol changed. Structural on purpose (no recompression), so it gives the same answer on any zlib build.
    Returns (bytes changed in the name slot, bytes changed in the config stream)."""
    assert len(ok) == len(lk), "kernel size differs"
    a, b = config_span(lk)
    assert config_span(ok) == (a, b), "IKCFG markers moved"
    assert ok[NAME_OFF:NAME_OFF + SLOT] == NEW_NAME + bytes(SLOT - len(NEW_NAME)), "machine name wrong"
    assert lk[NAME_OFF + SLOT:NAME_OFF + SLOT + len(NEXT)] == ok[NAME_OFF + SLOT:NAME_OFF + SLOT + len(NEXT)] == NEXT
    assert struct.unpack_from("<I", ok, MACHINE_DESC)[0] == MACHINE_NR, "machine type number changed"
    assert struct.unpack_from("<I", ok, NAME_PTR)[0] == BASE + NAME_OFF, "machine_desc.name pointer changed"
    diff = [i for i in range(len(lk)) if lk[i] != ok[i]]
    in_name = [i for i in diff if NAME_OFF <= i < NAME_OFF + SLOT]
    in_cfg = [i for i in diff if a <= i < b]
    assert in_name and in_cfg and len(in_name) + len(in_cfg) == len(diff), "kernel changed outside the name slot and the config"
    assert read_config(ok[a:b]) == read_config(lk[a:b]).replace(OLD_CFG_LINE, NEW_CFG_LINE), "config text wrong"
    assert b"tonium" not in ok.lower() and b"tonium" not in read_config(ok[a:b]).lower()
    return len(in_name), len(in_cfg)


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
    n_name, n_cfg = check_kernel(lk, ok)
    trailer = struct.unpack_from("<II", out, oend - 8)
    assert trailer == (zlib.crc32(ok) & 0xFFFFFFFF, len(ok)), "gzip trailer wrong"
    return n_name, n_cfg, oend - start, end - start


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
        n, c, used, span = verify(stock, logo, open(out_p, "rb").read())
        print(f"verified: {out_p}: {n} kernel bytes differ from the logo kernel in the name slot and {c} in the "
              f"build config stream, payload {used} of {span} bytes, zImage {len(stock)} bytes, sha256 {sha(open(out_p, 'rb').read())}")
        return
    out, used, span = make(stock, logo)
    open(out_p, "wb").write(out)
    n, c, _, _ = verify(stock, logo, out)
    print(f"machine name: {n} kernel bytes changed in the name slot, {c} in the build config stream; payload {used} of {span} bytes ({span - used} zero-filled); "
          f"zImage {len(out)} bytes, sha256 {sha(out)}")


if __name__ == "__main__":
    main()
