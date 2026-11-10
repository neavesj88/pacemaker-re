"""The Pacemaker Revival mark: a dot "R" in the style of Tonium's four-dot P, magenta ramp.

Twelve dots on a 4x5 grid (P2: the bowl pulled in, the leg tip dropped slightly below the
baseline so it reads level), coloured top to bottom with the L2 ramp the VU meters use. Drawn at
4x and downsampled for smooth dot edges.

Writes to branding/out/:
  r-logo-240x296.png  the player's loading screen (bin/resources/symbols/boot.png in the image)
  r-logo-240x320.png  the kernel boot logo (scripts/patch-kernel-logo.py quantises it)
  r-icon-1024.png     the app icon source (cd app && npx tauri icon ../branding/out/r-icon-1024.png)

Run with the project venv: tools/venv/Scripts/python branding/r_logo.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).with_name("out")
RAMP = ["e070ff", "f060ff", "ff50ff", "ff50e8", "ff50d0", "ff48a8"]  # L2 without the red clip step
# (column, row) in grid units; the bowl's right dot and the leg are nudged for optical balance
DOTS = [(0, 0), (1, 0), (2, 0), (0, 1), (2.55, 1), (0, 2), (1, 2), (2, 2), (0, 3), (1.8, 3), (0, 4), (2.75, 4.15)]
PITCH, RADIUS, SCALE = 40, 16, 4


def hx(s: str) -> tuple[int, int, int]:
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


def colour(t: float) -> tuple[int, int, int]:
    t = min(max(t, 0.0), 1.0) * (len(RAMP) - 1)
    i = min(int(t), len(RAMP) - 2)
    f = t - i
    a, b = hx(RAMP[i]), hx(RAMP[i + 1])
    return tuple(round(a[k] + (b[k] - a[k]) * f) for k in range(3))


def logo(width: int, height: int) -> Image.Image:
    im = Image.new("RGB", (width * SCALE, height * SCALE), (0, 0, 0))
    d = ImageDraw.Draw(im)
    xs = [x for x, _ in DOTS]
    mid = (min(xs) + max(xs)) / 2
    for gx, gy in DOTS:
        x = (width / 2 + (gx - mid) * PITCH) * SCALE
        y = (height / 2 + (gy - 2) * PITCH) * SCALE
        r = RADIUS * SCALE
        d.ellipse([x - r, y - r, x + r, y + r], fill=colour(min(gy, 4) / 4))
    return im.resize((width, height), Image.LANCZOS)


def icon(size: int = 1024) -> Image.Image:
    """App icon: the mark on a black rounded square (source for `npx tauri icon`)."""
    s = SCALE
    mark = logo(240, 260).resize((round(size * 0.86 * 240 / 260), round(size * 0.86)), Image.LANCZOS)
    mask = Image.new("L", (size * s, size * s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size * s - 1, size * s - 1], radius=size * s // 5, fill=255)
    mask = mask.resize((size, size), Image.LANCZOS)
    tile = Image.new("RGB", (size, size), (0, 0, 0))
    tile.paste(mark, ((size - mark.width) // 2, (size - mark.height) // 2))
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(tile, (0, 0), mask)
    return out


def main() -> None:
    OUT.mkdir(exist_ok=True)
    for w, h in [(240, 296), (240, 320)]:
        p = OUT / f"r-logo-{w}x{h}.png"
        logo(w, h).save(p, optimize=True)
        print("wrote", p)
    p = OUT / "r-icon-1024.png"
    icon().save(p, optimize=True)
    print("wrote", p)


if __name__ == "__main__":
    main()
