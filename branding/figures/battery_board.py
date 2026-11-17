"""The battery socket and the chip that heats if the battery is reversed: the main assembly straight
down with the cell removed, USB end at the top.

Traced from the owner's photo of the opened unit (back cover off, cell out), rectified with
trace.py rectify at 8 px/mm. The rectifying quad is the pale foil sheet the cell sits on (its bounding
box, 1443,350 - 2079,738 in the photo), taken as the original cell's 48 x 30 mm footprint. Cross-checks
from parts of known size in the same photo: the small USB board comes out 39.5 mm wide against its known
39.1 mm, and the 2 x 5 header pins come out 1.95 mm apart against 2.0 mm, so the scale is good to about
2 %, and edges are good to about 0.5 mm near the cell and about 1 mm at the jack end (the photo is not
perfectly square on, and the screws, chips and header stand a few millimetres above the board).
The original cell in the photo of the cell fitted (01bb6346) lies on this footprint: its wire edge is
beside the battery socket, its far end under the small board, and one corner screw beside it.
The owner confirmed the USB port is at the battery end of the unit.

Coordinates are millimetres in the photo's plate frame (x right, y down), origin at the top-left corner
of the 48 x 30 mm cell footprint; X/Y shift them into the figure's frame. The symmetry axis of the case
and boards is x = 24.0. Everything below the adapter's top edge (y = 112) is hidden by fingers in the
photo, so the jack end is only a simplified box.
"""
import math
import os

from figlib import Fig, FILL, GREY, INK, MAGENTA

FILE = "battery-board.svg"

OX, OY = 13.0, 29.0          # frame shift: plate x,y -> figure mm
CX = 24.0                    # symmetry axis
GOLD = "#e8d9a8"
DARK = "#3a3a3a"
CASE = "#dcdcdc"
LIGHT = "#bdbdbd"
RIBBON = "#efe3b8"
BOTTOM = 127.0               # where the boards and case lines stop (simplified jack end)

# outer edge of the case rim, half width about CX against y, right-hand side from the top centre down
OUTER = [(0.0, -24.4), (10.0, -24.0), (14.2, -23.5), (18.3, -22.2), (24.3, -19.8), (26.0, -17.5),
         (27.7, -12.0), (30.7, -6.0), (33.4, 0.0), (34.6, 10.0), (35.4, 20.0), (35.6, 30.0)]
RIM = 1.8                    # rim thickness


def X(x):
    return x + OX


def Y(y):
    return y + OY


def smooth(pts):
    """Cubic segments (Catmull-Rom) through pts, from pts[0] as the current point."""
    out = []
    for i in range(len(pts) - 1):
        p0, p1, p2, p3 = pts[max(i - 1, 0)], pts[i], pts[i + 1], pts[min(i + 2, len(pts) - 1)]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        out.append(f"C{c1[0]:.2f} {c1[1]:.2f} {c2[0]:.2f} {c2[1]:.2f} {p2[0]:.2f} {p2[1]:.2f}")
    return " ".join(out)


def case_path(inset):
    """The case rim's edge: left side up, over the top, right side down, open at the jack end."""
    right = [(CX + hw - inset, y + (inset if y < -12 else 0)) for hw, y in OUTER]
    right[0] = (CX, OUTER[0][1] + inset)
    left = [(2 * CX - x, y) for x, y in reversed(right)]
    pts = left + right[1:]
    pts = [(X(x), Y(y)) for x, y in pts]
    d = f"M{pts[0][0]:.2f} {Y(BOTTOM):.2f} L{pts[0][0]:.2f} {pts[0][1]:.2f} " + smooth(pts)
    d += f" L{pts[-1][0]:.2f} {Y(BOTTOM):.2f}"
    return d


def raw(f, d, fill="none", stroke=INK, sw=1.5, dash=None):
    da = f' stroke-dasharray="{dash}"' if dash else ""
    f.items.append(f'<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{sw / f.s:.3f}" stroke-linejoin="round" stroke-linecap="round"{da}/>')


def screw(f, x, y):
    f.circle(X(x), Y(y), 2.3, sw=1.0, fill=GOLD)
    f.circle(X(x), Y(y), 1.5, sw=1.0, fill="#d9d9d9")
    f.line(X(x) - 0.7, Y(y), X(x) + 0.7, Y(y), sw=0.8)


def boss(f, x, y):
    f.circle(X(x), Y(y), 2.1, sw=1.0, fill=GOLD)
    f.circle(X(x), Y(y), 0.9, sw=1.0, fill="#fff")


def chip(f, x0, y0, x1, y1, fill=DARK):
    f.rect(X(x0), Y(y0), x1 - x0, y1 - y0, r=0.5, sw=1.3, fill=fill)


def build():
    f = Fig("Battery socket and the chip to watch", 74, 163, scale=5.0, legend_title="Battery end of the main board")
    if os.path.exists(UNDERLAY):
        f.set_underlay(UNDERLAY, X(-150), Y(-150), 348, 330)

    # ---- case rim (outer edge, inner edge) and the boards ----------------------------------------
    d_out = case_path(0.0)
    raw(f, d_out + " Z", fill=CASE, stroke="none", sw=0)
    raw(f, d_out, sw=1.8)
    raw(f, case_path(RIM), sw=1.0, fill="none")

    hw = 31.0
    xl, xr = CX - hw, CX + hw
    f.path(f"M{X(xl)} {Y(BOTTOM)} L{X(xl)} {Y(6.6)} Q{X(xl)} {Y(5.4)} {X(xl + 1.2)} {Y(5.4)} L{X(xr - 1.2)} {Y(5.4)} Q{X(xr)} {Y(5.4)} {X(xr)} {Y(6.6)} L{X(xr)} {Y(BOTTOM)}",
           sw=1.6, fill=FILL)

    for x, y in ((2.2, -15.0), (45.8, -15.0), (-7.0, 18.6), (55.0, 18.6), (-8.2, 88.7), (56.2, 88.7)):
        boss(f, x, y)

    # small USB board (2 x 5 socket, gold contact) at the USB end, 39.1 mm wide, meeting the main board under the cell
    sx0, sx1 = CX - 19.55, CX + 19.55
    f.rect(X(sx0), Y(-11.6), sx1 - sx0, 17.0, sw=1.5, fill="#fff")
    f.rect(X(22.2), Y(-11.6), 4.0, 4.1, r=0.4, sw=1.3, fill=GOLD)          # gold contact
    f.rect(X(18.7), Y(-6.6), 11.2, 5.9, r=0.4, sw=1.4, fill=DARK)           # 2 x 5 socket
    for i in range(5):
        for yy in (-4.4, -2.4):
            f.rect(X(20.2 + i * 2.0) - 0.4, Y(yy) - 0.4, 0.8, 0.8, sw=0.5, fill="#fff")
        f.rect(X(20.2 + i * 2.0) - 0.35, Y(-0.7), 0.7, 1.5, sw=0.7, fill=GOLD)   # pin tips below the housing

    # battery socket on the left edge with the printed "+"
    f.rect(X(-3.7), Y(5.7), 3.2, 5.3, r=0.4, sw=1.4, fill="#fff")
    f.rect(X(-3.0), Y(6.6), 1.8, 3.5, r=0.2, sw=1.0, fill=DARK)
    f.text(X(-5.2), Y(9.9), "+", size=14, colour=INK, weight=700)

    # display flex and its tape (landmarks only)
    f.rect(X(6.7), Y(43.4), 30.1, 2.4, r=0.3, sw=1.0, fill="#fff")
    f.rect(X(8.3), Y(45.8), 27.3, 9.7, r=0.3, sw=1.0, fill=LIGHT)
    f.rect(X(13.3), Y(55.9), 21.5, 6.7, sw=1.0, fill=RIBBON)
    f.rect(X(12.8), Y(62.6), 23.2, 3.5, r=0.3, sw=1.0, fill="#fff")
    f.rect(X(12.6), Y(89.9), 22.8, 4.0, r=0.3, sw=1.0, fill="#fff")
    f.rect(X(13.0), Y(94.6), 22.0, 17.2, sw=1.0, fill=RIBBON)

    # the original cell's footprint, dashed
    f.rect(X(0), Y(0), 48, 30, r=1.0, sw=1.8, dash="6 4")
    f.text(X(24), Y(15.8), "original cell", size=11, colour=GREY)
    f.text(X(24), Y(19.3), "48 x 30 mm", size=11, colour=GREY)

    # chips: main processor with its dotted sticker, the two large chips beside it, and the hot chip
    chip(f, 18.6, 70.2, 33.7, 85.8)
    sc = (26.2, 78.3)
    f.circle(X(sc[0]), Y(sc[1]), 5.3, sw=1.0, fill="#fff")
    dots = [(0.0, 0.0)] + [(1.7 * math.cos(a * math.pi / 3), 1.7 * math.sin(a * math.pi / 3)) for a in range(6)] \
        + [(3.5 * math.cos((a + 0.5) * math.pi / 6), 3.5 * math.sin((a + 0.5) * math.pi / 6)) for a in range(12)]
    for dx, dy in dots:
        f.circle(X(sc[0] + dx), Y(sc[1] + dy), 0.4, sw=0, fill=INK)
    chip(f, 38.6, 70.0, 49.6, 80.8)
    chip(f, 38.6, 82.9, 49.6, 94.0)

    hx0, hy0, hx1, hy1 = -0.2, 82.9, 9.9, 94.0
    f.items.append('<defs><pattern id="hot" width="1.1" height="1.1" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">'
                   f'<line x1="0" y1="0" x2="0" y2="1.1" stroke="{MAGENTA}" stroke-width="0.32"/></pattern></defs>')
    chip(f, hx0, hy0, hx1, hy1)
    f.items.append(f'<rect x="{X(hx0)}" y="{Y(hy0)}" width="{hx1 - hx0}" height="{hy1 - hy0}" rx="0.5" fill="url(#hot)"/>')
    f.items.append(f'<rect x="{X(hx0) - 1.3}" y="{Y(hy0) - 1.3}" width="{hx1 - hx0 + 2.6}" height="{hy1 - hy0 + 2.6}" rx="1.6" fill="none" stroke="{MAGENTA}" stroke-width="{2.4 / f.s:.3f}"/>')

    # screws
    for x, y in ((0.1, 31.6), (47.7, 31.7), (40.7, 66.1), (5.0, 76.6), (51.1, 96.2)):
        screw(f, x, y)

    # simplified microSD adapter at the jack end (its lower part is hidden under fingers in the photo)
    f.rect(X(CX - 24.9), Y(111.8), 49.8, BOTTOM - 111.8, r=0.6, sw=1.6, fill=DARK)
    f.text(X(24.0), Y(120.0), "microSD adapter", size=11, colour="#fff", weight=700)
    screw(f, 43.4, 114.5)

    # ends
    f.text(X(CX), Y(-26.4), "USB END", size=12, colour=INK, weight=700)
    f.text(X(CX), Y(BOTTOM + 7.5), "JACK END", size=12, colour=INK, weight=700)

    f.callout(1, X(-2.1), Y(6.3), "Battery socket", "the \"+\" side is marked on the board", side="l")
    f.callout(2, X(40.0), Y(26.0), "Where the cell sits", "dashed, 48 x 30 mm", side="r")
    f.callout(3, X(21.5), Y(72.5), "Main processor", "dotted sticker on top", side="l")
    f.callout(4, X(1.5), Y(85.5), "Samsung chip", "heats if the battery is reversed", side="l")
    f.callout(5, X(CX + 3.0), Y(-1.7), "USB-board socket", "2 x 5, takes the USB board", side="r")
    f.callout(6, X(5.0), Y(76.6), "Screws", "beside the chips", side="l")
    f.note("Reversed battery: the Samsung chip (4)")
    f.note("heats within seconds. Unplug at once.")
    f.note("Check polarity before connecting:")
    f.note("red to the + side.")
    return f


# the photo rectified onto the drawing's plate frame (8 px/mm, plate corner at 150, 150 mm); local only (work/ is gitignored), set PM_TRACE_DIR to move it
UNDERLAY = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "batboard/rect.png")
