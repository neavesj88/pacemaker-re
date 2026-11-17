"""Photo helpers for tracing the help figures (needs Pillow: tools/venv/Scripts/python).

  grid <photo> <out.png> [--step 100]
      The photo with a labelled pixel grid, so coordinates can be read off it.
  mark <photo> <out.png> x,y [x,y ...]
      The photo with numbered crosshairs at those pixel coordinates, to check picked points.
  rectify <photo> <out.png> --quad x1,y1 x2,y2 x3,y3 x4,y4 --mm W H [--pxmm 12] [--pad MM]
      A top-down view: the four photo points (top-left, top-right, bottom-right, bottom-left of
      something whose real size is W x H mm) become a W x H mm rectangle at pxmm pixels per mm,
      with pad mm of margin around it. Prints the mm position of the output's top-left corner.
"""
import argparse
import sys

from PIL import Image, ImageDraw, ImageFont, ImageOps


def font(size):
    for name in ("arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default()


def grid(a):
    im = ImageOps.exif_transpose(Image.open(a.photo)).convert("RGB")
    d = ImageDraw.Draw(im)
    f = font(max(14, im.width // 90))
    for x in range(0, im.width, a.step):
        d.line([(x, 0), (x, im.height)], fill=(0, 255, 255) if x % (a.step * 5) else (255, 0, 255), width=1)
        d.text((x + 3, 3), str(x), fill=(255, 255, 0), font=f)
    for y in range(0, im.height, a.step):
        d.line([(0, y), (im.width, y)], fill=(0, 255, 255) if y % (a.step * 5) else (255, 0, 255), width=1)
        d.text((3, y + 3), str(y), fill=(255, 255, 0), font=f)
    im.save(a.out)
    print(a.out, im.size)


def mark(a):
    im = ImageOps.exif_transpose(Image.open(a.photo)).convert("RGB")
    d = ImageDraw.Draw(im)
    f = font(max(16, im.width // 70))
    r = max(10, im.width // 120)
    for i, p in enumerate(a.points, 1):
        x, y = map(float, p.split(","))
        d.line([(x - r, y), (x + r, y)], fill=(255, 0, 255), width=2)
        d.line([(x, y - r), (x, y + r)], fill=(255, 0, 255), width=2)
        d.text((x + r, y + r), str(i), fill=(255, 255, 0), font=f)
    im.save(a.out)
    print(a.out)


def solve(src, dst):
    """Perspective coefficients mapping output (dst) points to input (src) points, for Image.transform."""
    rows, rhs = [], []
    for (x, y), (u, v) in zip(dst, src):
        rows.append([x, y, 1, 0, 0, 0, -u * x, -u * y]); rhs.append(u)
        rows.append([0, 0, 0, x, y, 1, -v * x, -v * y]); rhs.append(v)
    n = 8
    m = [r + [b] for r, b in zip(rows, rhs)]
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(m[r][c]))
        m[c], m[p] = m[p], m[c]
        for r in range(n):
            if r != c:
                k = m[r][c] / m[c][c]
                m[r] = [a - k * b for a, b in zip(m[r], m[c])]
    return [m[i][n] / m[i][i] for i in range(n)]


def rectify(a):
    im = ImageOps.exif_transpose(Image.open(a.photo)).convert("RGB")
    src = [tuple(map(float, q.split(","))) for q in a.quad]
    w, h = a.mm
    s, pad = a.pxmm, a.pad
    dst = [(pad * s, pad * s), ((pad + w) * s, pad * s), ((pad + w) * s, (pad + h) * s), (pad * s, (pad + h) * s)]
    size = (round((w + 2 * pad) * s), round((h + 2 * pad) * s))
    out = im.transform(size, Image.PERSPECTIVE, solve(src, dst), Image.BICUBIC)
    out.save(a.out)
    print(a.out, size, f"{s} px/mm, the quad's top-left corner is at ({pad}, {pad}) mm")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("grid"); g.add_argument("photo"); g.add_argument("out"); g.add_argument("--step", type=int, default=100)
    m = sub.add_parser("mark"); m.add_argument("photo"); m.add_argument("out"); m.add_argument("points", nargs="+")
    r = sub.add_parser("rectify"); r.add_argument("photo"); r.add_argument("out"); r.add_argument("--quad", nargs=4, required=True)
    r.add_argument("--mm", nargs=2, type=float, required=True); r.add_argument("--pxmm", type=float, default=12); r.add_argument("--pad", type=float, default=3)
    a = ap.parse_args()
    {"grid": grid, "mark": mark, "rectify": rectify}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
