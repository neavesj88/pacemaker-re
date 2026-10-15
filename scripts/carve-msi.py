"""Carve embedded OLE/CFB documents (MSI) out of an InstallShield setup.exe without executing it.

The CFB header gives sector size and FAT sector count, which bounds the file length well enough
for 7-Zip/msiextract to read it; trailing bytes past the real end are ignored by readers.
"""
import struct, sys
src, outdir = sys.argv[1], sys.argv[2]
data = open(src, "rb").read()
MAGIC = bytes.fromhex("d0cf11e0a1b11ae1")
i, n = data.find(MAGIC), 0
while i != -1:
    sec = 1 << struct.unpack_from("<H", data, i + 0x1e)[0]
    nfat = struct.unpack_from("<I", data, i + 0x2c)[0]
    # each FAT sector maps sec/4 sectors; this is an upper bound on document size
    est = min(len(data) - i, (nfat * (sec // 4) + 1) * sec) if sec in (512, 4096) else 0
    if est > 4096:
        path = f"{outdir}/carved_{n}_{i:x}.msi"
        open(path, "wb").write(data[i:i + est])
        print(f"0x{i:x} sector={sec} fat_sectors={nfat} -> {path} ({est} bytes)")
        n += 1
    i = data.find(MAGIC, i + 1)
