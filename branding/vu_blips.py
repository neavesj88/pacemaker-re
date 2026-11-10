"""Recolour the VU meter sprite sheet (blips.png) from the stock firmware.

The meters are cut from 22x4 px tiles: column 0 unlit, column 1 lit; rows 0-2 are greys (kept as
Tonium drew them), rows 3-9 the bottom-to-top meter colours. Each tile has a small bevel (darker
edge and corners), kept by scaling the new colour by each pixel's brightness relative to the tile's
main colour. The input is Tonium's artwork, so it is read from the unpacked firmware at build time
and the output goes to work/, never into the repo.

  tools/venv/Scripts/python branding/vu_blips.py [stock blips.png] [output.png]
"""
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "work/fw-180096069/bin/resources/symbols/blips.png"
OUT = ROOT / "work/branding/blips.png"

# L2: bright magenta, lifted with green/white so it reads on the washed-out panel (D1's violet
# looked dark purple on the device); red clip on top (rows 3..9, lit colours)
RAMP = ["e070ff", "f060ff", "ff50ff", "ff50e8", "ff50d0", "ff48a8", "ff3030"]
UNLIT = 84 / 255  # Tonium's unlit tiles are the lit colour at 0x54/0xff


def hx(s: str) -> tuple[int, int, int]:
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


def main() -> None:
    src = Path(sys.argv[1]) if len(sys.argv) > 1 else SRC
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else OUT
    im = Image.open(src).convert("RGB")
    assert im.size == (44, 40), im.size
    px = im.load()
    for k, lit in enumerate(RAMP):
        row = 3 + k
        for col in (0, 1):
            box = (col * 22, row * 4, col * 22 + 22, row * 4 + 4)
            main = max(im.crop(box).getcolors(1 << 16))[1]
            ref = max(main)
            colour = [c * (UNLIT if col == 0 else 1) for c in hx(lit)]
            for x in range(box[0], box[2]):
                for y in range(box[1], box[3]):
                    f = max(px[x, y]) / ref
                    px[x, y] = tuple(round(c * f) for c in colour)
    out.parent.mkdir(parents=True, exist_ok=True)
    im.save(out)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
