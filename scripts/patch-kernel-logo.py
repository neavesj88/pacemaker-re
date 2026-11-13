"""Replace the 240x320 CLUT224 boot logo inside the device kernel (boot/zImage).

The logo is a `struct linux_logo` in the gzip-compressed kernel payload (FINDINGS 5c): type 3,
240x320, 81 palette entries stored immediately before 76,800 pixel bytes (palette index + 32).
This rewrites only that palette and those pixels (at most 81 colours, so nothing moves),
recompresses with the original gzip header, and requires the new stream to fit inside the
original compressed span; the rest of the span is zero-filled so every byte of the zImage outside
it (decompressor code, data after the payload) stays identical. The ARM decompressor stops at the
end of the gzip stream, so the padding is never read.

Checks: the stock image is the expected one, only the logo's palette/pixels changed in the
kernel, the new zImage is the same size and differs only inside the payload span, and the payload
decompresses (CRC and length) to exactly the patched kernel.

  patch-kernel-logo.py STOCK_zImage LOGO.png OUT_zImage [PREVIEW.png]
"""
import os
import struct
import subprocess
import sys
import tempfile
import zlib

BASE = 0xC0008000           # kernel virtual address of the decompressed image's first byte
LOGO_OFF = 99072            # struct linux_logo of the 240x320 CLUT224 logo, verified for 180096069
W, H, MAX_COLOURS = 240, 320, 81


def gzip_parts(z: bytes):
    start = z.find(b"\x1f\x8b\x08")
    d = zlib.decompressobj(16 + zlib.MAX_WBITS)
    kernel = d.decompress(z[start:]) + d.flush()
    end = len(z) - len(d.unused_data)
    flags = z[start + 3]
    hdr_end = start + 10
    if flags & 0x04:  # FEXTRA
        hdr_end += 2 + struct.unpack_from("<H", z, hdr_end)[0]
    for bit in (0x08, 0x10):  # FNAME, FCOMMENT
        if flags & bit:
            hdr_end = z.index(b"\0", hdr_end) + 1
    if flags & 0x02:  # FHCRC
        hdr_end += 2
    return start, hdr_end, end, kernel


SEVEN_ZIP = r"C:\Program Files\7-Zip\7z.exe"


def deflate(data: bytes) -> bytes:
    """Raw deflate of `data`, the smaller of zlib -9 and 7-Zip's optimising deflate (zlib's
    output is ~5 KB larger than Tonium's original stream, so it alone does not fit)."""
    c = zlib.compressobj(9, zlib.DEFLATED, -zlib.MAX_WBITS, 9)
    best = c.compress(data) + c.flush()
    if os.path.exists(SEVEN_ZIP):
        with tempfile.TemporaryDirectory() as t:
            src, gz = os.path.join(t, "k"), os.path.join(t, "k.gz")
            open(src, "wb").write(data)
            subprocess.run([SEVEN_ZIP, "a", "-tgzip", "-mx=9", "-mpass=15", "-mfb=258", gz, src],
                           check=True, capture_output=True)
            g = open(gz, "rb").read()
        _, hdr_end, _, check = gzip_parts(g)
        assert check == data, "7-Zip output does not round-trip"
        body = g[hdr_end:-8]
        if len(body) < len(best):
            best = body
    if zlib.decompress(best, -zlib.MAX_WBITS) != data:
        raise SystemExit("deflate stream does not round-trip")
    return best


def main() -> None:
    from PIL import Image  # only the logo step needs Pillow; patch-kernel-name.py imports gzip_parts/deflate from here

    stock_path, logo_path, out_path = sys.argv[1:4]
    z = open(stock_path, "rb").read()
    start, hdr_end, end, kernel = gzip_parts(z)

    t, w, h, n, clut, data = struct.unpack_from("<IIIIII", kernel, LOGO_OFF)
    if (t, w, h, n) != (3, W, H, MAX_COLOURS) or clut - BASE != LOGO_OFF + 24 or data - clut != 3 * MAX_COLOURS:
        sys.exit(f"unexpected logo struct {(t, w, h, n, hex(clut), hex(data))}: not the 180096069 kernel?")

    img = Image.open(logo_path).convert("RGB")
    if img.size != (W, H):
        sys.exit(f"logo must be {W}x{H}, got {img.size}")
    q = img.quantize(colors=MAX_COLOURS, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    pal = q.getpalette()[: 3 * MAX_COLOURS]
    used = max(q.get_flattened_data()) + 1
    pixels = bytes(32 + v for v in q.get_flattened_data())

    new = bytearray(kernel)
    struct.pack_into("<I", new, LOGO_OFF + 12, used)  # clutsize
    new[clut - BASE: clut - BASE + 3 * MAX_COLOURS] = bytes(pal) + bytes(3 * MAX_COLOURS - len(pal))
    new[data - BASE: data - BASE + W * H] = pixels
    region = (LOGO_OFF + 12, data - BASE + W * H)
    diff = [i for i in range(len(kernel)) if kernel[i] != new[i]]
    if not diff or diff[0] < region[0] or diff[-1] >= region[1] or len(new) != len(kernel):
        sys.exit("kernel changed outside the logo")

    body = deflate(bytes(new))
    stream = z[start:hdr_end] + body + struct.pack("<II", zlib.crc32(new) & 0xFFFFFFFF, len(new) & 0xFFFFFFFF)
    span = end - start
    if len(stream) > span:
        sys.exit(f"recompressed kernel is {len(stream)} bytes, {len(stream) - span} more than the original {span}")
    out = z[:start] + stream + bytes(span - len(stream)) + z[end:]

    assert len(out) == len(z) and out[:start] == z[:start] and out[end:] == z[end:]
    _, _, _, check = gzip_parts(out)
    assert check == bytes(new), "payload does not decompress to the patched kernel"
    open(out_path, "wb").write(out)
    print(f"logo: {used} colours; kernel bytes changed {len(diff)} (all inside the logo); "
          f"payload {len(stream)} of {span} bytes ({span - len(stream)} zero-filled); zImage {len(out)} bytes, same as stock")

    if len(sys.argv) > 4:  # render the logo back out of the patched kernel
        p = bytes(new[clut - BASE: clut - BASE + 3 * used])
        px = new[data - BASE: data - BASE + W * H]
        im = Image.new("RGB", (W, H))
        im.putdata([tuple(p[3 * (v - 32): 3 * (v - 32) + 3]) for v in px])
        im.save(sys.argv[4])
        print("preview", sys.argv[4])


if __name__ == "__main__":
    main()
