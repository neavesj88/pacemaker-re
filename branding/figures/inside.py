"""The inside of the Pacemaker: main assembly (back cover off, seen from the back) on the right and
the back cover opened out like a book on the left, both with the JACK END up and the USB END down.

Traced from the owner's photos. The main assembly comes from the angled whole-unit photo, rectified
with a least-squares homography fitted to 15 points that were also measured in the straight-down
battery-end photo (microSD sockets, mounting screws, small-board corners, sticker): so the scale is
that photo's. Scale references: the 2 x 5 socket pitch (2.0 mm = 40 px, both axes), the original
cell (48 x 30 mm body, 52 x 30 mm with its seal strip) and the microSD sockets; all agree on
20 px/mm in the straight-down photo, which is 10 px/mm in the rectified view used here. The small
board is 41 mm wide in both photos; the 1 x 10 audio socket measures 20 mm, i.e. 2.0 mm pitch.
The case is symmetric about its long axis and end to end (measured within 1-2 mm), so its outline is
drawn symmetric from the clean right side and bottom lobe. The back cover has the same outline; its
USB board is placed so its 2 x 5 header lands on the small board's socket, and the audio board so
its pin row lands on the audio socket (the cover is mirrored left to right when opened like a book).
The audio board's jack-side edge is hidden under the strap in the photos, so it is drawn dashed.
"""
import os

from figlib import Fig, FILL, GREY, INK

FILE = "pm-inside.svg"

# rectified photo for --trace checks; local only (work/ is gitignored), set PM_TRACE_DIR to move it
RECT = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "inside/rect_p1.png")

AX = 398.5            # case axis in rectified px (10 px per mm)
YC = 1069.5           # case middle
# NOFILL=1 draws every shape unfilled so the photo shows through (for checking a tracing only)
NF = bool(os.environ.get("NOFILL"))


def c(col):
    return "none" if NF else col


CASE_BG = c("#e8e8e8")
PCB = c("#ffffff")
DARK = c("#2e2e2e")
COPPER = c("#d9a066")
GOLD = c("#e8d9a8")
CELL = c("#d4d4d4")
LIGHT = c("#f2f2f2")

YO = 7.0              # mm of space above the case for the JACK END label
MAIN_X = 80.0         # mm: the main assembly's left edge of the rectified frame (px 33.5)
COVER_X = 0.0


def nf(k):
    if NF and "fill" in k:
        k["fill"] = "none"


class View:
    """Maps rectified px (X to the right, Y down) to figure mm, optionally mirrored left to right."""

    def __init__(self, f, ox, mirror=False):
        self.f, self.ox, self.mirror = f, ox, mirror

    def p(self, X, Y):
        if self.mirror:
            X = 2 * AX - X
        return self.ox + (X - 33.5) / 10, (Y - 189) / 10 + YO

    def pts(self, pts):
        return [self.p(x, y) for x, y in pts]

    def box(self, x, y, w, h, **k):
        self.rect(x, y, x + w, y + h, **k)

    def rect(self, x0, y0, x1, y1, **k):
        nf(k)
        a, b = self.p(x0, y0), self.p(x1, y1)
        self.f.rect(min(a[0], b[0]), min(a[1], b[1]), abs(b[0] - a[0]), abs(b[1] - a[1]), **k)

    def circle(self, X, Y, r_mm, **k):
        nf(k)
        x, y = self.p(X, Y)
        self.f.circle(x, y, r_mm, **k)

    def poly(self, pts, **k):
        nf(k)
        self.f.poly(self.pts(pts), **k)

    def line(self, x0, y0, x1, y1, **k):
        a, b = self.p(x0, y0), self.p(x1, y1)
        self.f.line(a[0], a[1], b[0], b[1], **k)

    def smooth(self, pts, closed=True, **k):
        """Catmull-Rom spline through the points, as an SVG path."""
        q = self.pts(pts)
        n = len(q)
        d = f"M{q[0][0]:.2f} {q[0][1]:.2f}"
        rng = range(n) if closed else range(n - 1)
        for i in rng:
            p0, p1, p2, p3 = q[(i - 1) % n], q[i], q[(i + 1) % n], q[(i + 2) % n]
            if not closed:
                p0 = q[max(i - 1, 0)]
                p3 = q[min(i + 2, n - 1)]
            c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
            c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
            d += f" C{c1[0]:.2f} {c1[1]:.2f} {c2[0]:.2f} {c2[1]:.2f} {p2[0]:.2f} {p2[1]:.2f}"
        nf(k)
        self.f.path(d + (" Z" if closed else ""), **k)


# half width of the case at distance d from the middle (px), measured on the bottom lobe and checked on the top
PROFILE = [(0, 353), (130, 353), (330, 350), (430, 345), (505, 334), (580, 317), (640, 286), (682, 263),
           (727, 252), (759, 241), (790, 216), (817, 182), (842, 141), (858, 86)]
TIP = 866

# strap slots at the bottom (left one; the others are mirrored): centre, length, width, angle (deg, towards the axis)
SLOT = (287, 1829, 118, 50, 30)
# brass bosses for the cover screws (right side; mirrored), measured
BOSSES = [(626.5, 358), (714, 733), (710, 1427), (618, 1786)]


def stadium(cx, cy, length, width, ang):
    import math
    r = width / 2
    h = length / 2 - r
    a = math.radians(ang)
    ux, uy = math.cos(a), math.sin(a)
    pts = []
    for k in range(0, 13):  # cap at +u
        t = -math.pi / 2 + math.pi * k / 12 + a
        pts.append((cx + ux * h + r * math.cos(t), cy + uy * h + r * math.sin(t)))
    for k in range(0, 13):  # cap at -u
        t = math.pi / 2 + math.pi * k / 12 + a
        pts.append((cx - ux * h + r * math.cos(t), cy - uy * h + r * math.sin(t)))
    return pts


def case(v):
    # from the top tip down the right side, round the bottom tip and back up the left side
    pts = ([(AX, YC - TIP)] + [(AX + hw, YC - d) for d, hw in reversed(PROFILE[1:])]
           + [(AX + hw, YC + d) for d, hw in PROFILE] + [(AX, YC + TIP)]
           + [(AX - hw, YC + d) for d, hw in reversed(PROFILE)] + [(AX - hw, YC - d) for d, hw in PROFILE[1:]])
    v.smooth(pts, sw=2.0, fill=CASE_BG)
    cx, cy, ln, wd, an = SLOT
    for sy in (1, -1):
        for sx in (1, -1):
            X = AX + sx * (AX - cx)
            Y = YC + sy * (cy - YC)
            v.poly(stadium(X, Y, ln, wd, an * sx * sy), sw=1.4, fill=DARK)


def bosses(v):
    for bx, by in BOSSES:
        for X in (bx, 2 * AX - bx):
            v.circle(X, by, 2.5, sw=1.2, fill="#fff")
            v.circle(X, by, 1.15, sw=1.0, fill=GOLD)


def screw(v, X, Y):
    v.circle(X, Y, 2.1, sw=1.0, fill=GOLD)
    v.circle(X, Y, 1.2, sw=1.0, fill="#cfcfcf")


def zif(v, x0, y0, x1, y1):
    v.rect(x0, y0, x1, y1, sw=1.1, fill="#fff")
    if x1 - x0 > y1 - y0:
        v.box(x0 + 4, y0 + (y1 - y0) * 0.55, x1 - x0 - 8, 5, sw=0.8, fill=DARK)
    else:
        v.box(x0 + (x1 - x0) * 0.55, y0 + 4, 5, y1 - y0 - 8, sw=0.8, fill=DARK)


def main(f):
    v = View(f, MAIN_X)
    case(v)
    # PCB: flat top with rounded lobe, straight sides, ends under the battery
    v.f.path(_pcb_path(v), sw=1.6, fill=PCB)
    bosses(v)
    # top flex and its connector, 1 x 10 audio socket
    v.poly([(388, 372), (428, 372), (428, 436), (498, 436), (498, 462), (388, 462)], sw=1.0, fill=COPPER)
    zif(v, 496, 404, 534, 492)
    v.rect(324, 503, 504, 527, sw=1.1, fill=DARK)
    # microSD adapter plate, storage ZIF and ribbon
    v.rect(154, 553, 630, 806, r=0.5, sw=1.3, fill=DARK)
    v.rect(282, 530, 504, 558, sw=1.0, fill=COPPER)
    zif(v, 267, 557, 520, 602)
    v.circle(212, 580, 2.0, sw=1.0, fill="#cfcfcf")
    v.circle(572, 584, 1.6, sw=1.0, fill="#cfcfcf")
    for x0 in (160, 470):
        v.rect(x0, 628, x0 + 154, 800, r=0.4, sw=1.4, fill="#d0d0d0")
        for dy in (50, 130):
            v.box(x0 + 52, 628 + dy, 40, 4, sw=0.9, fill=DARK)
    f.text(*v.p(176, 622), "TF2", size=9, anchor="start", colour="#bbb")
    f.text(*v.p(608, 622), "TF1", size=9, anchor="start", colour="#bbb")
    for y0, y1 in ((762, 798), (818, 858), (876, 914), (930, 966)):
        v.rect(126, y0, 144, y1, r=0.2, sw=0.9, fill=COPPER)
    # memory and main chip with its sticker
    v.rect(154, 822, 256, 922, r=0.3, sw=1.1, fill="#555")
    v.rect(304, 806, 452, 910, r=0.3, sw=1.1, fill=DARK)
    v.circle(380, 850, 5.0, sw=1.0, fill="#fff")
    # display ZIF, its ribbon and the display window
    zif(v, 288, 954, 508, 992)
    v.rect(294, 992, 506, 1052, sw=1.0, fill=COPPER)
    v.rect(292, 1056, 572, 1148, r=0.4, sw=1.3, fill=DARK)
    v.rect(276, 1162, 572, 1190, r=0.3, sw=1.0, fill="#cfcfcf")
    # third connector with its flex, chips, coils, crystal
    zif(v, 596, 938, 622, 1058)
    v.rect(622, 962, 682, 1040, sw=1.0, fill=COPPER)
    v.rect(524, 970, 572, 1018, r=0.3, sw=1.0, fill="#555")
    v.rect(564, 1092, 612, 1132, r=0.3, sw=1.0, fill="#555")
    v.rect(412, 1204, 476, 1272, r=0.3, sw=1.0, fill="#555")
    v.rect(510, 1244, 548, 1282, r=0.3, sw=1.0, fill=GOLD)
    v.circle(258, 1102, 3.2, sw=1.1, fill=DARK)
    v.circle(192, 1072, 2.0, sw=1.0, fill=DARK)
    # small board with the 2 x 5 socket, then the battery on top of it
    v.rect(196, 1560, 602, 1752, r=0.4, sw=1.4, fill=PCB)
    v.rect(340, 1634, 462, 1704, r=0.3, sw=1.2, fill=DARK)
    for i in range(5):
        for Y in (1664, 1684):
            v.box(401 + (i - 2) * 20 - 4, Y - 4, 8, 8, sw=0.6, fill=LIGHT)
    v.rect(383, 1708, 427, 1748, r=0.6, sw=1.0, fill=GOLD)
    v.rect(132, 1325, 652, 1628, r=1.2, sw=1.8, fill=CELL)
    v.line(585, 1327, 585, 1626, sw=0.9, dash="2 1.5")
    v.rect(270, 1408, 484, 1570, sw=1.0, fill=DARK)
    # battery plug and wires
    v.rect(661, 1504, 683, 1538, r=0.3, sw=1.2, fill="#fff")
    v.circle(672, 1514, 0.45, sw=0.8, fill=DARK)
    v.circle(672, 1528, 0.45, sw=0.8, fill=DARK)
    for off, col in ((0, "#c8102e"), (9, DARK)):
        x0, y0 = v.p(672 + off, 1538)
        x1, y1 = v.p(685 + off, 1590)
        x2, y2 = v.p(655 + off * 0.4, 1620)
        f.items.append(f'<path d="M{x0:.2f} {y0:.2f} C{x0:.2f} {y0 + 3:.2f} {x1:.2f} {y1 - 3:.2f} {x1:.2f} {y1:.2f} '
                       f'S{x2 + 3:.2f} {y2 - 1:.2f} {x2:.2f} {y2:.2f}" fill="none" stroke="{col}" '
                       f'stroke-width="{2.2 / f.s:.3f}" stroke-linecap="round"/>')
    for X, Y in ((208, 462), (586, 464), (134, 656), (585, 854), (232, 954), (158, 1300), (636, 1302)):
        screw(v, X, Y)
    return v


def _pcb_path(v):
    p = v.p
    def q(x, y):
        a = p(x, y)
        return f"{a[0]:.2f} {a[1]:.2f}"
    return (f"M{q(252, 370)} L{q(562, 370)} C{q(640, 372)} {q(705, 470)} {q(710, 640)} L{q(710, 1440)} L{q(705, 1600)} "
            f"L{q(104, 1600)} L{q(100, 640)} C{q(105, 470)} {q(172, 372)} {q(252, 370)} Z")


def usb_board(v, ox_px=AX, yh=1674):
    """The USB board (see usb_board.py), component side, rotated so the Mini-B points to the USB end."""
    L, H, SH, END, CH, TAB = 38.0, 13.3, 3.8, 4.5, 6.5, 13.0
    C = L / 2

    def q(x, y):
        return ox_px + (x - C) * 10, yh - (y - 10.5) * 10

    t0, t1 = C - TAB / 2, C + TAB / 2
    pts = [(0, H), (0, H - END), (CH, SH), (t0, SH), (t0, 0.6), (t0 + 0.6, 0), (t1 - 0.6, 0), (t1, 0.6), (t1, SH),
           (L - CH, SH), (L, H - END), (L, H)]
    v.poly([q(*p) for p in pts], sw=1.5, fill=LIGHT)
    for x in (2.7, L - 2.7):
        X, Y = q(x, H - 3.0)
        v.circle(X, Y, 2.2, sw=1.0, fill="#fff")
        v.circle(X, Y, 1.45, sw=1.3, fill=GOLD)
    a, b = q(C - 3.85, -2.7), q(C + 3.85, 6.5 - 2.7)
    v.rect(a[0], a[1], b[0], b[1], r=0.5, sw=1.5, fill="#fff")
    a, b = q(C - 2.2, -1.9), q(C + 2.2, -0.7)
    v.rect(a[0], a[1], b[0], b[1], r=0.4, sw=1.0, fill=DARK)
    for x in (8.7, L - 8.7):
        a, b = q(x - 2.1, 4.0), q(x + 2.1, 6.7)
        v.rect(a[0], a[1], b[0], b[1], r=0.3, sw=1.2, fill="#fff")
    a, b = q(C - 5, H - 4.8), q(C + 5, H - 0.3)
    v.rect(a[0], a[1], b[0], b[1], sw=1.2, fill=DARK)
    for i in range(5):
        for y in (H - 3.8, H - 1.8):
            X, Y = q(C - 4 + i * 2, y)
            v.circle(X, Y, 0.45, sw=0.0, fill=LIGHT)


def audio_board(v, cx=420):
    """The audio board. Seen from inside the cover with the jack end up, which is the photo turned half a
    turn (the cover swings open on its jack-end strap), so it is drawn without the left-right mirror the
    rest of the cover gets from the book-style opening. Board outline and screws are symmetric about the
    case axis (hx); the 1 x 10 pin row is centred 15.5 px left of the axis, mirroring the socket on the
    main assembly. Jack-side edge hidden under the strap (dashed)."""
    v.mirror = False
    hx = AX
    top, bot = 352, 470   # jack-side edge (hidden), pin-side edge
    hw = 190
    v.poly([(hx - hw, bot), (hx + hw, bot), (hx + hw, bot - 50), (hx + hw - 22, bot - 66)], closed=False, sw=1.5, fill="none")
    v.poly([(hx - hw, bot), (hx - hw, bot - 50), (hx - hw + 22, bot - 66)], closed=False, sw=1.5, fill="none")
    v.line(hx - hw + 22, bot - 66, hx - hw + 22, top, sw=1.0, dash="2 1.5")
    v.line(hx + hw - 22, bot - 66, hx + hw - 22, top, sw=1.0, dash="2 1.5")
    v.line(hx - hw + 22, top, hx + hw - 22, top, sw=1.0, dash="2 1.5")
    for x in (-hw + 27, hw - 27):
        v.circle(hx + x, bot - 35, 2.2, sw=1.0, fill="#fff")
        v.circle(hx + x, bot - 35, 1.3, sw=1.3, fill=GOLD)
    v.box(cx + 61, bot - 98, 50, 80, r=0.3, sw=1.3, fill=DARK)
    v.box(cx - 35, bot - 85, 80, 60, r=0.3, sw=1.3, fill="#fff")
    # pin row: 10 pins, 2.0 mm pitch, pointing to the USB end
    for i in range(10):
        X = cx - 37 + (i - 4.5) * 20
        v.box(X - 2.5, bot, 5, 45, sw=0.8, fill=DARK)
    v.mirror = True


def cover(f):
    v = View(f, COVER_X, mirror=True)
    case(v)
    bosses(v)
    usb_board(v)
    audio_board(v)
    return v


def build():
    f = Fig("Inside the Pacemaker: back cover opened out (left) and main assembly (right)", 152, 198, scale=5.0,
            legend_title="Inside the unit")
    f.set_underlay(RECT, MAIN_X - 3.35, -18.9 + YO, 90, 215)
    vc = cover(f)
    vm = main(f)
    mid_c = vc.p(AX, 0)[0]
    mid_m = vm.p(AX, 0)[0]
    for v, mid in ((vc, mid_c), (vm, mid_m)):
        f.text(mid, v.p(0, 203.5)[1] - 3.0, "JACK END", size=11, weight=700, colour=INK)
        f.text(mid, v.p(0, 1935)[1] + 5.0, "USB END", size=11, weight=700, colour=INK)
    f.text(1.2, 193.5, "Back cover, opened out (inside view)", size=11, anchor="start")
    f.text(MAIN_X + 1.2, 193.5, "Main assembly, back cover off (seen from the back)", size=11, anchor="start")
    # callouts: 1-6 and 9 on the main assembly (right column), 7-8 in the cover (left column)
    f.callout(1, *vm.p(547, 700), "Storage", "microSD adapter, slots TF1/TF2", side="r")
    f.callout(2, *vm.p(470, 580), "Storage ribbon (ZIF)", "ZIF, ribbon to the storage", side="r")
    f.callout(3, *vm.p(480, 972), "Display ribbon (ZIF)", "ZIF, ribbon to the display", side="r")
    f.callout(4, *vm.p(560, 1480), "Battery", "original about 48 x 30 mm", side="r")
    f.callout(5, *vm.p(672, 1521), "Battery plug", "white 2-pin, red and black wires", side="r")
    f.callout(6, *vm.p(455, 1684), "USB-board socket", "2 x 5, on the small board", side="r")
    f.callout(7, *vc.p(AX + 66, 1700), "USB board", "in the back cover, plugs into 6", side="l")
    f.callout(8, *vc.p(AX - 60, 420), "Audio board", "in the cover: power, line out, phones", side="l")
    f.callout(9, *vm.p(586, 464), "Screw positions", "PCB screws, cover bosses", side="r")
    return f
