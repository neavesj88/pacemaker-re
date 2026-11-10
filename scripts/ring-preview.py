#!/usr/bin/env python3
"""Preview one frame of the power-on ring as PwrComponent::render draws it (angle 0): centre (120,148),
radius 109, stroke width 22, four dim quadrants (sweep pi/2) then four bright cursor ticks (sweep
0.001 rad) starting at 0, pi/2, pi, 3pi/2; y down, angles clockwise on screen (assumed). Left panel =
stock colours, right = patched. Swatch rows underneath show the 8 colours (dim, bright).
Output: work/build/ring-preview.png (default). Needs Pillow (tools/venv)."""
import math, os, sys
from PIL import Image, ImageDraw

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'work/build/ring-preview.png')
STOCK = dict(dim=['800000', '008000', '008080', '800080'], bright=['ff0000', '00ff00', '00ffff', 'ff00ff'])
NEW = dict(dim=['4a2554', '551a55', '551a45', '551838'], bright=['e070ff', 'ff50ff', 'ff50d0', 'ff48a8'])
SS, W, H, CX, CY, R, LW = 4, 240, 296, 120, 148, 109, 22


def rgb(h): return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def sector(d, a0, sweep, col):
    n = max(2, int(abs(sweep) / 0.02) + 2)
    ro, ri = (R + LW / 2) * SS, (R - LW / 2) * SS
    pts = [((CX * SS + ro * math.cos(a0 + sweep * i / (n - 1))), (CY * SS + ro * math.sin(a0 + sweep * i / (n - 1)))) for i in range(n)]
    pts += [((CX * SS + ri * math.cos(a0 + sweep * i / (n - 1))), (CY * SS + ri * math.sin(a0 + sweep * i / (n - 1)))) for i in reversed(range(n))]
    d.polygon(pts, fill=col)


def frame(cols, angle=0.0):
    im = Image.new('RGB', (W * SS, H * SS), (0, 0, 0))
    d = ImageDraw.Draw(im)
    for q in range(4):
        sector(d, angle + q * math.pi / 2, math.pi / 2, rgb(cols['dim'][q]))
    for q in range(4):
        sector(d, angle + q * math.pi / 2, 0.001, rgb(cols['bright'][q]))
    im = im.resize((W, H), Image.LANCZOS)
    return im


def panel(cols, label):
    f = frame(cols)
    p = Image.new('RGB', (W, H + 60), (24, 24, 24))
    p.paste(f, (0, 0))
    d = ImageDraw.Draw(p)
    for row, key in enumerate(('dim', 'bright')):
        for q in range(4):
            x = 10 + q * 56
            d.rectangle([x, H + 8 + row * 26, x + 50, H + 28 + row * 26], fill=rgb(cols[key][q]))
    d.text((6, 4), label, fill=(200, 200, 200))
    return p


a, b = panel(STOCK, 'stock'), panel(NEW, 'patched (magenta)')
out = Image.new('RGB', (W * 2 + 12, a.height), (60, 60, 60))
out.paste(a, (0, 0)); out.paste(b, (W + 12, 0))
out = out.resize((out.width * 2, out.height * 2), Image.NEAREST)
os.makedirs(os.path.dirname(os.path.abspath(OUT)), exist_ok=True)
out.save(OUT)
print('wrote', os.path.abspath(OUT), out.size)
