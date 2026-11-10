"""Make a DFU restore set (kernel + rootfs) from an unpacked firmware's boot/ directory.

Same layout as Tonium's ltibCreateDfuFiles + dfuSuffixCreator: each payload is padded with 0xFF
to its flash partition size minus one byte, then a 16-byte DFU suffix is appended (bcdDevice
0x0200, idProduct 0xa7a4, idVendor 0x5e1d, bcdDFU 0x0100, "UFD", length 16, CRC-32 without the
final inversion, as dfu-util computes it). Flash with dfu-util (notes/FINDINGS.md section 5):

  dfu-util -d 0x5e1d:0xa7a4 -a kernel -t 4088 -D zImage_<v>.dfu
  dfu-util -d 0x5e1d:0xa7a4 -a rootfs -t 4088 -D cramfs_<v>.dfu
  dfu-util -d 0x5e1d:0xa7a4 -a status -t 4088 -R -D start.dfu

  make-restore-dfu.py BOOT_DIR VERSION OUT_DIR [TONIUM_START.dfu]

Checked: regenerating 14142 from its .pfw gives byte-identical zImage_14142.dfu and
cramfs_14142.dfu to Tonium's restore package.
"""
import struct
import sys
import zlib
from pathlib import Path

PARTITIONS = {"zImage": 2 * 1024 * 1024, "cramfs.little": 12 * 1024 * 1024 + 512 * 1024}
NAMES = {"zImage": "zImage_{}.dfu", "cramfs.little": "cramfs_{}.dfu"}
START = b"A 0 1\n"  # status payload of Tonium's start.dfu


def suffixed(payload: bytes) -> bytes:
    body = payload + struct.pack("<HHHH", 0x0200, 0xA7A4, 0x5E1D, 0x0100) + b"UFD" + bytes([16])
    return body + struct.pack("<I", ~zlib.crc32(body) & 0xFFFFFFFF)


def main() -> None:
    boot, version, out = Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3])
    out.mkdir(parents=True, exist_ok=True)
    for name, size in PARTITIONS.items():
        data = (boot / name).read_bytes()
        if len(data) > size - 1:
            sys.exit(f"{name} is {len(data)} bytes, more than its {size}-byte partition")
        dst = out / NAMES[name].format(version)
        dst.write_bytes(suffixed(data + b"\xff" * (size - 1 - len(data))))
        print(f"wrote {dst}")
    # start.dfu is not generated: Tonium's (38 bytes, "A 0 1\n" plus two suffixes, the inner CRC
    # computed some unknown way) is written to the status block, whose bytes 4-7 the device reads
    # as the installed bootloader version. Use their file verbatim from the restore package.
    if len(sys.argv) > 4:
        start = Path(sys.argv[4]).read_bytes()
        if len(start) != 38 or not start.startswith(START):
            sys.exit("that is not Tonium's start.dfu")
        (out / "start.dfu").write_bytes(start)
        print(f"copied {out / 'start.dfu'}")


if __name__ == "__main__":
    main()
