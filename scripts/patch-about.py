"""Rebrand the About screen of the stock pmplayer (firmware 180096069) as Pacemaker Revival
(notes/about-screen.md has the evidence behind every offset).

  patch-about.py [--notices-out FILE] [STOCK_PMPLAYER [OUT_PMPLAYER]]
  patch-about.py lang IN.lang OUT.lang
  defaults: work/fw-180096069/bin/pmplayer -> work/build/pmplayer-about

pmplayer mode, all against the stock binary (11 286 972 bytes, sha256 below), every old byte asserted first:
  * the Legal notice string (13152 bytes at file 0x844b90, VA 0x84cb90) is replaced in place by
    device/about/legal.txt, byte for byte, so the pool word that points at it stays valid. The old text, the
    4 NULs after it and the old URL are zeroed; "Pacemaker Revival" and the support URL go into the freed space.
  * the Support pane's pool word (0x394088) points at the new URL instead of www.pacemaker.net.
  * the six pool words that load the model strings P211 / 666 point at "Pacemaker Revival" instead.
  --notices-out writes the stock Legal text (extracted from the stock binary before it is zeroed) to FILE; the
  image ships it as /bin/resources/licenses/THIRD-PARTY-NOTICES.txt because legal.txt points there.

lang mode: string 49 ("(c) Tonium") becomes "(c) Revival contributors" (the (c) is the real sign, UTF-8). The line
is dead in this firmware (no row shows it) but nothing in the image should say Tonium. Exactly one match is required.

Legal text, model and URL go through mbstowcs in the C locale, so they must be pure ASCII; this is enforced.
"""
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
STOCK = os.path.join(ROOT, "work", "fw-180096069", "bin", "pmplayer")
OUT = os.path.join(ROOT, "work", "build", "pmplayer-about")
LEGAL = os.path.join(ROOT, "device", "about", "legal.txt")

STOCK_SIZE = 11286972
STOCK_SHA256 = "4c5d502c6d52e2bafab894cef43e7213e35aca73f497b68482dd7867c13797ab"
VA = 0x8000                       # Ghidra / pointer value = file offset + VA

LEGAL_OFF = 0x844B90              # stock Legal text, pointed to by the pool word at 0x39408c
LEGAL_LEN = 13152
LEGAL_SHA256 = "d2a84f373e2bbef096b968ffbfb5f7af05cd779450d6651ef206fa1d5c639211"
OLD_URL_OFF = 0x847EF4            # b"www.pacemaker.net\0", pointed to by the pool word at 0x394088
OLD_URL = b"www.pacemaker.net\0"
BLOCK_END = OLD_URL_OFF           # the Legal text, 4 NULs of padding and nothing else sit below this

MODEL = b"Pacemaker Revival\0"
URL = b"github.com/neavesj88/pacemaker-re\0"
MODEL_OFF = 0x847E00              # fixed slots at the end of the freed block, so the Legal text can change length freely
URL_OFF = 0x847E20

LEGAL_POOL = 0x39408C             # SettingString::paint, pool word of the Legal literal (unchanged)
URL_POOL = 0x394088
MODEL_DEFAULT = (0x350874, 0x350924)   # PwrSettings constructors, "P211"
MODEL_00 = (0x351650, 0x351C00)        # PwrMgr constructors, serial prefix 00 -> "P211"
MODEL_01 = (0x35164C, 0x351BFC)        # PwrMgr constructors, serial prefix 01 -> "666"
P211_VA, SIX66_VA = 0x8487C8, 0x84885C
P211_OFF, SIX66_OFF = P211_VA - VA, SIX66_VA - VA


def word(va):
    return struct.pack("<I", va)


def apply(d, legal):
    """Return the patched binary (and the stock Legal bytes). Exits on any mismatch."""
    def refuse(msg):
        sys.exit("refusing: " + msg)

    if len(d) != STOCK_SIZE or hashlib.sha256(d).hexdigest() != STOCK_SHA256:
        refuse("input is not the stock 180096069 pmplayer (size/sha256 differ; already patched?)")
    if any(b > 0x7E for b in legal) or b"\0" in legal:
        refuse("legal.txt must be pure ASCII without NULs (the player decodes it with mbstowcs in the C locale)")
    if any(b > 0x7E for b in MODEL + URL):
        refuse("model/url must be pure ASCII")
    stock_legal = d[LEGAL_OFF:LEGAL_OFF + LEGAL_LEN]
    if hashlib.sha256(stock_legal).hexdigest() != LEGAL_SHA256:
        refuse("stock Legal text differs from the analysed one")
    if d[LEGAL_OFF + LEGAL_LEN:BLOCK_END] != b"\0\0\0\0":
        refuse("expected exactly 4 NULs between the Legal text and the URL")
    if d[OLD_URL_OFF:OLD_URL_OFF + len(OLD_URL)] != OLD_URL:
        refuse("old URL string not found")
    if d[P211_OFF:P211_OFF + 5] != b"P211\0" or d[SIX66_OFF:SIX66_OFF + 4] != b"666\0":
        refuse("model literals not found")
    pools = [(LEGAL_POOL, word(LEGAL_OFF + VA)), (URL_POOL, word(OLD_URL_OFF + VA))]
    pools += [(o, word(P211_VA)) for o in MODEL_DEFAULT + MODEL_00]
    pools += [(o, word(SIX66_VA)) for o in MODEL_01]
    for off, exp in pools:
        if d[off:off + 4] != exp:
            refuse(f"pool word at {off:#x} is {d[off:off + 4].hex()}, expected {exp.hex()}")

    text = legal + b"\0"
    if LEGAL_OFF + len(text) > MODEL_OFF or MODEL_OFF + len(MODEL) > URL_OFF or URL_OFF + len(URL) > BLOCK_END:
        refuse("new strings do not fit in the freed block")

    out = bytearray(d)
    out[LEGAL_OFF:BLOCK_END + len(OLD_URL)] = bytes(BLOCK_END + len(OLD_URL) - LEGAL_OFF)
    out[LEGAL_OFF:LEGAL_OFF + len(text)] = text
    out[MODEL_OFF:MODEL_OFF + len(MODEL)] = MODEL
    out[URL_OFF:URL_OFF + len(URL)] = URL
    out[URL_POOL:URL_POOL + 4] = word(URL_OFF + VA)
    for o in MODEL_DEFAULT + MODEL_00 + MODEL_01:
        out[o:o + 4] = word(MODEL_OFF + VA)
    assert len(out) == len(d)
    return bytes(out), stock_legal


def patch_lang(a):
    old, new = b'49 "\xc2\xa9 Tonium"\n', b'49 "\xc2\xa9 Revival contributors"\n'
    if a.count(old) != 1:
        sys.exit('refusing: lang file does not contain exactly one `49 "(c) Tonium"` line')
    b = a.replace(old, new)
    assert b.count(b"Tonium") == 0 and len(b) == len(a) + len(new) - len(old)
    return b


def main():
    args = sys.argv[1:]
    if args and args[0] == "lang":
        src, dst = args[1:3]
        open(dst, "wb").write(patch_lang(open(src, "rb").read()))
        print("ok:", dst)
        return
    notices = None
    if args and args[0] == "--notices-out":
        notices, args = args[1], args[2:]
    src = args[0] if args else STOCK
    dst = args[1] if len(args) > 1 else OUT
    d = open(src, "rb").read()
    out, stock_legal = apply(d, open(LEGAL, "rb").read())
    os.makedirs(os.path.dirname(os.path.abspath(dst)), exist_ok=True)
    open(dst, "wb").write(out)
    if notices:
        open(notices, "wb").write(stock_legal)
    assert open(dst, "rb").read() == out
    changed = sum(1 for x, y in zip(d, out) if x != y)
    print(f"ok: {dst}  {len(out)} bytes, {changed} bytes differ from stock")
    print("sha256 stock", hashlib.sha256(d).hexdigest())
    print("sha256 patch", hashlib.sha256(out).hexdigest())


if __name__ == "__main__":
    main()
