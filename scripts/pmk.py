"""Decode/encode Pacemaker obfuscated mix documents (.pmk, prefab mixes).

Format (from pmplayer's MixDocumentObfuscatorFilter, verified on the Editor's bundled bin.pmk):
zlib-compressed boost XML, then XOR with the 18-byte table b"y7W6GSRycjV96qh3R\0". The table
index starts at 12 and advances by 2 before each byte, wrapping at 18.
Usage: pmk.py decode|encode <in> <out>
"""
import sys, zlib

KEY = b"y7W6GSRycjV96qh3R\x00"

def xor(data, start=12):
    s, out = start, bytearray(len(data))
    for i, b in enumerate(data):
        s += 2
        if s >= len(KEY):
            s -= len(KEY)
        out[i] = b ^ KEY[s]
    return bytes(out)

if __name__ == "__main__":
    mode, src, dst = sys.argv[1:4]
    data = open(src, "rb").read()
    out = zlib.decompress(xor(data)) if mode == "decode" else xor(zlib.compress(data, 9))
    open(dst, "wb").write(out)
