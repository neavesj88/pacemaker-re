"""The Pacemaker's case outline, seen from the front, with its strap slots and screw bosses.

Traced from the owner's photos of the opened unit (whole length, and the end of the back cover, which
has the same outline as the front shell). The whole-length photo was rectified in two steps (a rotation
to square it up, then the four end screw bosses as a rectangle so the camera's perspective cancels).
The scale comes from parts of known size, not from the rulers: the 2 x 5 header on the board at the
strap end has a 2.0 mm pitch (19.6 px between pin holes in the rectified image = 9.8 px/mm), the
original battery pouch (48 x 30 mm in plan) gives about the same, and the second photo's Mini-B shell
(7.7 mm) gives the same case width within 3 percent. So the case is about 71 mm wide and 159 mm long,
each good to about 3 percent. The outline is the outer edge of the rim, measured at the bottom end of
the photo (clear of the strap and the hard drive lying beside the unit), averaged left and right, and
mirrored about the middle: the screw bosses at the two ends and the two middle bosses agree on the
middle to 3 mm, and the strap bands at the top end line up with the slots at the bottom. The top end
itself is hidden under the strap, so it is drawn as the mirror of the bottom end.
"""
import math
import os

from figlib import Fig, FILL, GREY

PUBLISH = False  # a working check for kb_figures.py, not shown in an article
FILE = "case-outline.svg"

PXMM = 9.85          # rectified-photo pixels per mm (header pitch, battery)
CX, MID = 443.0, 887.0   # case axis and middle in the rectified photo (px)

# right half of the outline from the middle to the end: (distance from the middle, half width), px
HALF = [(0, 352), (200, 351), (313, 347), (433, 341), (513, 329), (553, 318), (573, 304), (593, 287), (613, 272),
        (633, 262), (653, 256), (673, 254), (693, 255), (713, 245), (733, 220), (753, 181), (773, 127), (780, 100), (785, 0)]
L = 2 * HALF[-1][0] / PXMM
W = 2 * HALF[0][1] / PXMM
UNDERLAY = os.environ.get("CASE_PHOTO", "")   # the rectified photo (kept outside the repo), only used for --trace
RIM = 3.0            # the inner line of the rim sits this far inside the outer edge
BOSS_END = 673 / PXMM   # end screw bosses: distance from the middle
BOSS_MID = 321 / PXMM
BOSS_MID_X = 304 / PXMM
BOSS_X = (443 - 225) / PXMM  # half the spacing of a boss pair
SLOT = dict(cx=(555 - 330) / 2 / PXMM, dy=(1605 - 887) / PXMM, len=79 / PXMM, w=35 / PXMM, ang=24)


def catmull(pts, n=10, alpha=0.5):
    """A closed centripetal Catmull-Rom curve through pts, sampled."""
    out = []
    m = len(pts)
    for i in range(m):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[(i + 1) % m], pts[(i + 2) % m]
        d1 = max(1e-6, math.dist(p0, p1) ** alpha); d2 = max(1e-6, math.dist(p1, p2) ** alpha); d3 = max(1e-6, math.dist(p2, p3) ** alpha)
        for k in range(n):
            t = k / n
            def seg(a, b, ta, tb, tt):
                return tuple(((tb - tt) * x + (tt - ta) * y) / (tb - ta) for x, y in zip(a, b))
            t0, t1 = 0, d1; t2 = t1 + d2; t3 = t2 + d3
            tt = t1 + t * d2
            a1 = seg(p0, p1, t0, t1, tt); a2 = seg(p1, p2, t1, t2, tt); a3 = seg(p2, p3, t2, t3, tt)
            b1 = seg(a1, a2, t0, t2, tt); b2 = seg(a2, a3, t1, t3, tt)
            out.append(seg(b1, b2, t1, t2, tt))
    return out


def outline_points():
    """The outline as points, clockwise from the top tip, in mm relative to the case centre."""
    half = [(hw / PXMM, d / PXMM) for d, hw in HALF]          # (half width, distance from middle)
    top_to_mid = [(x, -d) for x, d in half[::-1]]               # right side, top tip to the middle
    mid_to_bottom = [(x, d) for x, d in half[1:]]               # right side, middle to the bottom tip
    right = [p for p in top_to_mid + mid_to_bottom if p[0] > 0]
    left = [(-x, y) for x, y in reversed(right)]
    return [(0, -L / 2)] + right + [(0, L / 2)] + left


def inset(poly, dist):
    """Move every vertex of a closed polygon inwards by dist along its local normal."""
    out = []
    n = len(poly)
    for i in range(n):
        ax, ay = poly[i - 1]; bx, by = poly[(i + 1) % n]
        tx, ty = bx - ax, by - ay
        k = math.hypot(tx, ty) or 1
        nx, ny = ty / k, -tx / k       # clockwise polygon in y-down space: the right-hand normal points outwards
        out.append((poly[i][0] - nx * dist, poly[i][1] - ny * dist))
    return out


def slot_d(cx, cy, length, w, ang):
    """The path of a stadium-shaped slot: length between the end-circle centres, width w, tilted ang degrees."""
    a = math.radians(ang)
    ux, uy = math.cos(a), math.sin(a)
    nx, ny = -uy, ux
    h, r = length / 2, w / 2
    p1 = (cx - ux * h, cy - uy * h); p2 = (cx + ux * h, cy + uy * h)
    return (f"M{p1[0] + nx * r:.2f} {p1[1] + ny * r:.2f} L{p2[0] + nx * r:.2f} {p2[1] + ny * r:.2f} "
            f"A{r:.2f} {r:.2f} 0 0 0 {p2[0] - nx * r:.2f} {p2[1] - ny * r:.2f} L{p1[0] - nx * r:.2f} {p1[1] - ny * r:.2f} "
            f"A{r:.2f} {r:.2f} 0 0 0 {p1[0] + nx * r:.2f} {p1[1] + ny * r:.2f} Z")


def slots():
    """The four strap slots as (centre x, centre y, length, width, angle), mm from the case centre."""
    return [(sx * SLOT["cx"], sy * SLOT["dy"], SLOT["len"], SLOT["w"], -sx * SLOT["ang"] * sy) for sy in (-1, 1) for sx in (-1, 1)]


def draw_case(f, ox, oy, detail=True):
    """Draw the case with its centre at ox, oy (mm). Returns the key points other drawings can use."""
    base = catmull(outline_points())
    f.poly([(ox + x, oy + y) for x, y in base], sw=1.8, fill=FILL)
    f.poly([(ox + x, oy + y) for x, y in inset(base, RIM)], sw=1, fill="none")
    for cx, cy, ln, w, ang in slots():
        f.path(slot_d(ox + cx, oy + cy, ln, w, ang), sw=1.3, fill="#fff")
    return base


def build():
    pad = 10
    f = Fig("Case outline (front, seen from the inside of the shell)", W + 2 * pad, L + 2 * pad + 8, scale=4.4, legend_title="Case")
    ox, oy = pad + W / 2, pad + L / 2
    draw_case(f, ox, oy)
    # the rectified photo under the drawing (only drawn into the .trace.svg)
    if os.path.exists(UNDERLAY):
        f.set_underlay(UNDERLAY, ox - CX / PXMM, oy - MID / PXMM, 877 / PXMM, 1778 / PXMM)
    for sx in (-1, 1):
        for sy in (-1, 1):
            f.circle(ox + sx * BOSS_X, oy + sy * BOSS_END, 1.5, sw=1.2, fill="#fff")
        f.circle(ox + sx * BOSS_MID_X, oy + BOSS_MID, 1.5, sw=1.2, fill="#fff")
        f.circle(ox + sx * BOSS_MID_X, oy - BOSS_MID, 1.5, sw=1.2, fill="#fff")
    # dimensions
    f.dim(ox - W / 2, oy + L / 2 + 5.5, ox + W / 2, oy + L / 2 + 5.5, f"about {round(W)} mm", offset=0)
    f.dim(ox - W / 2 - 5, oy - L / 2, ox - W / 2 - 5, oy + L / 2, f"about {round(L)} mm")
    f.text(pad, pad + L + 11, "Symmetric about its long axis and its middle", size=11, anchor="start")
    f.callout(1, ox + SLOT["cx"] + 4, oy - SLOT["dy"] + 1.5, "Strap slot", "two at each end, in a V", side="r")
    f.callout(2, ox + BOSS_MID_X, oy - BOSS_MID, "Screw boss", "four at the ends, two in the middle", side="r")
    f.callout(3, ox + W / 2 - 0.3, oy + 24, "Rim", "the outer edge and the lip inside it", side="r")
    f.note("Sizes are estimates, good to about 3 percent,")
    f.note("scaled from the 2.0 mm pin pitch of the 2 x 5 header.")
    return f
