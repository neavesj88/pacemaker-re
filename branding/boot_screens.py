"""Pacemaker Revival boot screens for the device's 240x296 portrait LCD.

The panel is a small, washed-out LCD (the originals use pure primaries on black), so the
designs use flat, fully saturated colour on black: matrix green for everything, magenta only
for the heartbeat peak. Every image is drawn from code so it can be tweaked and rebuilt.

Writes to branding/out/:
  <variant>.png            the image for the device (240x296 RGB, like Tonium's boot.png)
  <variant>_lcd.png        a simulation of the real panel: RGB565 colour, lifted blacks,
                           reduced contrast and saturation
  sheet.png                every variant next to its simulation, 2x, for review

Run with the project venv: tools/venv/Scripts/python branding/boot_screens.py
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageEnhance

W, H = 240, 296
BLACK = (0, 0, 0)
GREEN = (0, 255, 70)       # matrix green, slightly blue-shifted so it stays green when washed out
GREEN_DIM = (0, 90, 25)    # unlit dot-matrix cells / grid
MAGENTA = (255, 0, 200)

OUT = Path(__file__).with_name("out")

# 5x7 dot-matrix glyphs (rows top to bottom, '#' = lit)
FONT = {
    "A": [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "C": [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "I": ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####"],
    "K": ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
    "L": ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
    "M": ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"],
    "P": ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
    "R": ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
    "V": ["#...#", "#...#", "#...#", ".#.#.", ".#.#.", ".#.#.", "..#.."],
    " ": [".....", ".....", ".....", ".....", ".....", ".....", "....."],
}


def text_width(text: str, pitch: int, gap: int = 1) -> int:
    return len(text) * (5 + gap) * pitch - gap * pitch


def dot_text(d: ImageDraw.ImageDraw, text: str, x: int, y: int, pitch: int, dot: int,
             on=GREEN, off=None, gap: int = 1) -> None:
    """Draw text as round LED dots on a `pitch` grid; `off` draws the unlit cells too."""
    for i, ch in enumerate(text):
        rows = FONT[ch]
        for r, row in enumerate(rows):
            for c, cell in enumerate(row):
                colour = on if cell == "#" else off
                if colour is None:
                    continue
                cx = x + (i * (5 + gap) + c) * pitch + pitch // 2
                cy = y + r * pitch + pitch // 2
                d.ellipse([cx - dot // 2, cy - dot // 2, cx + dot // 2, cy + dot // 2], fill=colour)


def centred_text(d, text, y, pitch, dot, **kw):
    dot_text(d, text, (W - text_width(text, pitch)) // 2, y, pitch, dot, **kw)


def heartbeat(d: ImageDraw.ImageDraw, y: int, x0: int = 14, x1: int = W - 14, width: int = 4,
              peak_dot: int = 0) -> tuple[int, int]:
    """Flat line that kicks into one ECG complex (P, QRS, T) and settles. Returns the R peak."""
    mid = (x0 + x1) // 2
    pts = [
        (x0, y), (mid - 52, y),
        (mid - 44, y - 6), (mid - 36, y),          # P wave
        (mid - 22, y), (mid - 16, y + 10),         # Q
        (mid - 4, y - 58),                         # R
        (mid + 8, y + 22),                         # S
        (mid + 16, y), (mid + 28, y),
        (mid + 38, y - 12), (mid + 50, y),         # T wave
        (x1, y),
    ]
    d.line(pts, fill=GREEN, width=width, joint="curve")
    peak = (mid - 4, y - 58)
    if peak_dot:
        r = peak_dot // 2
        d.ellipse([peak[0] - r, peak[1] - r, peak[0] + r, peak[1] + r], fill=MAGENTA)
    return peak


def variant_heartbeat() -> Image.Image:
    """Revival as a flatline coming back to life, the name in dot matrix below."""
    im = Image.new("RGB", (W, H), BLACK)
    d = ImageDraw.Draw(im)
    heartbeat(d, 132, width=4, peak_dot=14)
    centred_text(d, "PACEMAKER", 196, 3, 2)
    centred_text(d, "REVIVAL", 226, 5, 4)
    return im


def variant_dotmark() -> Image.Image:
    """Tonium's four-dot mark rebuilt as a green dot 'P' with one magenta heartbeat dot."""
    im = Image.new("RGB", (W, H), BLACK)
    d = ImageDraw.Draw(im)
    # the original mark: three dots down the left, one to the right of the top dot
    for cx, cy, colour in [(104, 52, GREEN), (136, 52, MAGENTA), (104, 84, GREEN), (104, 116, GREEN)]:
        d.ellipse([cx - 11, cy - 11, cx + 11, cy + 11], fill=colour)
    heartbeat(d, 206, x0=40, x1=W - 40, width=3)
    centred_text(d, "REVIVAL", 244, 4, 3)
    return im


def variant_ring() -> Image.Image:
    """The startup ring in green LED dots, magenta at the top, a pulse across the middle."""
    import math
    im = Image.new("RGB", (W, H), BLACK)
    d = ImageDraw.Draw(im)
    cx, cy, rad, n = W // 2, H // 2, 104, 40
    for k in range(n):
        a = -math.pi / 2 + 2 * math.pi * k / n
        x, y = cx + rad * math.cos(a), cy + rad * math.sin(a)
        r = 6 if k == 0 else 4
        d.ellipse([x - r, y - r, x + r, y + r], fill=MAGENTA if k == 0 else GREEN)
    heartbeat(d, cy + 6, x0=cx - 86, x1=cx + 86, width=3, peak_dot=10)
    centred_text(d, "REVIVAL", cy + 44, 3, 2)
    return im


def lcd_sim(im: Image.Image) -> Image.Image:
    """Rough model of the device panel: RGB565 quantisation, lifted blacks, low contrast."""
    px = im.load()
    out = im.copy()
    q = out.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b = px[x, y]
            q[x, y] = (r & 0xF8, g & 0xFC, b & 0xF8)
    out = ImageEnhance.Color(out).enhance(0.75)
    out = ImageEnhance.Contrast(out).enhance(0.7)
    lift = Image.new("RGB", out.size, (28, 30, 34))
    return Image.blend(out, lift, 0.18)


def main() -> None:
    OUT.mkdir(exist_ok=True)
    variants = {"heartbeat": variant_heartbeat(), "dotmark": variant_dotmark(), "ring": variant_ring()}
    pairs = []
    for name, im in variants.items():
        im.save(OUT / f"{name}.png", optimize=True)
        sim = lcd_sim(im)
        sim.save(OUT / f"{name}_lcd.png", optimize=True)
        pairs.append((im, sim))
    pad, scale = 16, 2
    sheet = Image.new("RGB", (pad + len(pairs) * (2 * W * scale + 3 * pad), H * scale + 2 * pad), (40, 40, 40))
    for i, (im, sim) in enumerate(pairs):
        x = pad + i * (2 * W * scale + 3 * pad)
        sheet.paste(im.resize((W * scale, H * scale), Image.NEAREST), (x, pad))
        sheet.paste(sim.resize((W * scale, H * scale), Image.NEAREST), (x + W * scale + pad, pad))
    sheet.save(OUT / "sheet.png", optimize=True)
    print("wrote", ", ".join(sorted(p.name for p in OUT.iterdir())))


if __name__ == "__main__":
    main()
