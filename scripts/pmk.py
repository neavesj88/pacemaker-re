"""Decode/encode Pacemaker mix documents: .pmk prefab files and the mixes.structured_ct blob.

Both are boost-XML MixDocuments, zlib-compressed and then XORed with the 18-byte table
b"y7W6GSRycjV96qh3R\\0" (pmplayer's MixDocumentObfuscatorFilter). The table index starts at
`start` and advances by 2 before each byte, wrapping at 18.
- DB blob (mixes.structured_ct): start = len(blob) % 18 (MixDocument::loadDocumentFromBlob).
  Verified byte-exact against a mix the Windows Editor 2.0.2.14170 wrote to its music.db.
- .pmk file: start 12 (a streamed file write cannot know its length up front, unlike the blob).
  Verified byte-exact against the Editor's bundled bin.pmk. Only one sample, so decode_pmk
  still falls back to every phase, validating the zlib header.
Usage: pmk.py decode|encode <in> <out>   (.pmk)
       pmk.py decode-blob|encode-blob <in> <out>
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

def decode_blob(blob):
    return zlib.decompress(xor(blob, len(blob) % len(KEY)))

def encode_blob(xml):
    z = zlib.compress(xml)
    return xor(z, len(z) % len(KEY))

def decode_pmk(data):
    for start in [12] + [s for s in range(len(KEY)) if s != 12]:
        try:
            return zlib.decompress(xor(data, start))
        except zlib.error:
            continue
    raise ValueError("no XOR phase yields a zlib stream")

def encode_pmk(xml):
    return xor(zlib.compress(xml))

if __name__ == "__main__":
    mode, src, dst = sys.argv[1:4]
    data = open(src, "rb").read()
    fn = {"decode": decode_pmk, "encode": encode_pmk, "decode-blob": decode_blob, "encode-blob": encode_blob}[mode]
    open(dst, "wb").write(fn(data))
