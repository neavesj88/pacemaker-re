"""Statically unpack an old-format InstallShield setup.exe overlay (no execution of the installer).

Overlay: "InstallShield\0" + u16 count, 0x2e-byte header; then per file a 0x138-byte entry
(name[260], u32 flags, u32 ?, u32 size, ...) immediately followed by its data.
Flags 0x2/0x4 mean the data is obfuscated with a filename-derived key; 0x4 restarts the key every
1024 bytes. Correctness is checked by the caller: a decoded MSI must start with the CFB magic.
"""
import os, struct, sys

SEED = b"\x13\x35\x86\x07"

def overlay_offset(d):
    pe = struct.unpack_from("<I", d, 0x3c)[0]
    ns, so = struct.unpack_from("<H", d, pe + 6)[0], struct.unpack_from("<H", d, pe + 20)[0]
    return max(sum(struct.unpack_from("<II", d, pe + 24 + so + 40 * k + 16)) for k in range(ns))

def decode(buf, name, chunked):
    key = bytes(c ^ SEED[i % 4] for i, c in enumerate(name))
    out, kl = bytearray(len(buf)), len(key)
    for i, b in enumerate(buf):
        j = (i % 1024) if chunked else i
        out[i] = ~(key[j % kl] ^ (((b << 4) | (b >> 4)) & 0xff)) & 0xff
    return bytes(out)

def main(src, outdir):
    d = open(src, "rb").read()
    o = overlay_offset(d)
    assert d[o:o + 14] == b"InstallShield\0", "not an old-format InstallShield overlay"
    count, p = struct.unpack_from("<H", d, o + 14)[0], o + 0x2e
    os.makedirs(outdir, exist_ok=True)
    for _ in range(count):
        name = d[p:p + 260].split(b"\0", 1)[0]
        flags, _unk, size = struct.unpack_from("<III", d, p + 260)
        p += 0x138
        data = d[p:p + size]; p += size
        if flags & 6:
            data = decode(data, name, bool(flags & 4))
        safe = os.path.basename(name.decode("latin-1"))
        open(os.path.join(outdir, safe), "wb").write(data)
        print(f"{safe:40} flags=0x{flags:x} size={size} head={data[:8].hex()}")

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
