"""The USB board in the back cover, component side above, back below.

Traced from the owner's photos (component side straight on, back with a ruler), both rectified with
branding/trace.py. The back photo is the master for everything that lies in the board plane: its 2 x 5
joints sit on a 2.0 mm pitch, which gives the board about 39 mm along its length, a tab 11.9 mm wide
standing 14.0 mm above the straight edge, shoulders 10.0 mm above it and screw holes 33.2 mm apart.
The component-side photo is used for the parts that stand up (socket shell 7.7 mm wide, header,
switches), read near the tab so that its perspective stays small. The outline is made symmetric about
the tab; the photos' small asymmetries (up to about 0.4 mm) are camera angle.

Coordinates: w is across the board from the tab centre (right as drawn), v is the height above the
straight edge, so the same board numbers serve both views.
"""
import os

from figlib import Fig, FILL, GREY

FILE = "usb-board.svg"

L = 39.1          # board length
C = L / 2         # tab centre, mm from the left end
HALF = L / 2
TAB_H = 14.0      # straight edge to the top of the tab
SHOULDER = 10.0   # straight edge to the flat between chamfer and tab
TAB_HALF = 5.95   # half the tab width
SHELL_W, SHELL_V0, SHELL_V1 = 7.7, 6.5, 15.4   # Mini-B shell: width, v of its two ends (it sticks out of the tab)
HOLE_W, HOLE_V = 16.6, 3.4                      # screw hole centres
HDR_W = 0.2       # header centre, towards the 5/6 end in the component view
GOLD = "#e8d9a8"
COPPER = "#f2cdb5"
DARK = "#555"
PAD = "#fff"

# the chamfer from each end up to the shoulder is a curve, measured as (w, v) from the end inwards
CHAMFER = [(HALF, 4.5), (18.6, 5.0), (17.5, 6.1), (16.5, 7.1), (15.5, 7.9), (14.5, 8.6), (13.4, 9.25), (12.0, 9.8), (10.6, SHOULDER)]


def smooth(pts):
    """Cubic segments (Catmull-Rom) through pts, starting from pts[0] as the current point."""
    out = []
    for i in range(len(pts) - 1):
        p0, p1, p2, p3 = pts[max(i - 1, 0)], pts[i], pts[i + 1], pts[min(i + 2, len(pts) - 1)]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        out.append(f"C{c1[0]:.2f} {c1[1]:.2f} {c2[0]:.2f} {c2[1]:.2f} {p2[0]:.2f} {p2[1]:.2f}")
    return " ".join(out)


def outline(f, ye, ox=0.0):
    sx = lambda w: ox + C + w
    sy = lambda v: ye - v
    right = [(sx(w), sy(v)) for w, v in CHAMFER]
    left = [(sx(-w), sy(v)) for w, v in reversed(CHAMFER)]
    th, sh = TAB_HALF, TAB_HALF + 1.5     # tab side, and where the concave corner meets the shoulder
    d = (f"M{sx(-HALF):.2f} {sy(0):.2f} L{sx(HALF):.2f} {sy(0):.2f} L{right[0][0]:.2f} {right[0][1]:.2f} " + smooth(right)
         + f" L{sx(sh):.2f} {sy(SHOULDER):.2f} A1.5 1.5 0 0 1 {sx(th):.2f} {sy(SHOULDER + 1.5):.2f}"
         + f" L{sx(th):.2f} {sy(TAB_H - 0.5):.2f} A0.5 0.5 0 0 0 {sx(th - 0.5):.2f} {sy(TAB_H):.2f}"
         + f" L{sx(-th + 0.5):.2f} {sy(TAB_H):.2f} A0.5 0.5 0 0 0 {sx(-th):.2f} {sy(TAB_H - 0.5):.2f}"
         + f" L{sx(-th):.2f} {sy(SHOULDER + 1.5):.2f} A1.5 1.5 0 0 1 {sx(-sh):.2f} {sy(SHOULDER):.2f}"
         + f" L{left[0][0]:.2f} {left[0][1]:.2f} " + smooth(left) + f" L{sx(-HALF):.2f} {sy(0):.2f} Z")
    f.path(d, sw=1.6, fill=FILL)
    for s in (-1, 1):
        f.circle(sx(s * HOLE_W), sy(HOLE_V), 2.3, sw=1, fill=COPPER)
        f.circle(sx(s * HOLE_W), sy(HOLE_V), 1.2, sw=1.4, fill="#fff")
        f.circle(sx(s * 14.6), sy(1.1), 0.65, sw=0.9, fill="#fff")   # the small unplated hole beside each screw hole


def component(f, ye, ox=0.0, socket=True, lands=True):
    """The component-side parts: the socket's four solder pads, the Mini-B shell and its five leads (socket),
    the two switches and the 2 x 5 header. lands=False leaves the shell pads off (socket=False keeps them, empty)."""
    X = lambda w: ox + C + w
    Y = lambda v: ye - v
    # the four solder pads that hold the socket shell, then the shell with its window, spring tongue and five leads
    for s in (-1, 1) if lands else ():
        for v0, v1 in ((10.8, 13.6), (5.5, 8.3)):   # tinned pads, 1.6 x 2.8; their inner ends run under the shell
            f.rect(X(-5.3 if s < 0 else 3.7), Y(v1), 1.6, v1 - v0, r=0.2, sw=0.9, fill=PAD)
    if not socket:
        _rest(f, X, Y)
        return
    f.rect(X(-SHELL_W / 2), Y(SHELL_V1), SHELL_W, SHELL_V1 - SHELL_V0, r=0.5, sw=1.6, fill=GOLD)
    f.rect(X(-1.4), Y(13.0), 2.8, 2.6, r=0.3, sw=1, fill=PAD)
    for s in (-1, 1):
        f.rect(X(s * 2.2 - 0.45), Y(9.5), 0.9, 1.1, sw=0.8, fill=GOLD)   # gold lugs on the shell, level with the two peg holes on the back
    for i in range(5):
        f.rect(X(-1.6 + i * 0.8 - 0.25), Y(6.55), 0.5, 1.15, sw=0.8, fill=PAD)
    _rest(f, X, Y)


def _rest(f, X, Y):
    """The switches and the header."""
    # the two switches: plunger towards the case end, three solder joints towards the straight edge
    for s in (-1, 1):
        cx = s * 10.4
        f.rect(X(cx - 2.2), Y(9.3), 4.4, 2.9, r=0.3, sw=1.3, fill="#cfcfcf")
        f.rect(X(cx - 0.7), Y(10.0), 1.4, 0.7, r=0.3, sw=1, fill=PAD)
        for k in (-1, 0, 1):
            f.rect(X(cx + k * 1.15 - 0.375), Y(6.55), 0.75, 0.8, sw=0.8, fill=PAD)
    # 2 x 5 header, 2.0 mm pitch: top row 10..6, bottom row 1..5 (as silkscreened)
    f.rect(X(HDR_W - 4.9), Y(4.65), 9.8, 4.1, sw=1.2, fill=DARK)
    for i in range(5):
        for v in (3.55, 1.55):
            f.rect(X(HDR_W - 4 + i * 2 - 0.45), Y(v + 0.45), 0.9, 0.9, sw=0.8, fill=GOLD)


def build():
    ye = 1.7 + TAB_H                  # straight edge of the component view
    ye2 = ye + 21.5                   # straight edge of the back view
    X = lambda w: C + w
    f = Fig("USB board (in the back cover)", L, ye2 + 5.0, scale=11, legend_title="USB board")
    f.set_underlay(UNDERLAY, -2, 0, 504 / 12, 528 / 12)

    # ---- component side, case end up ----
    Y = lambda v: ye - v
    outline(f, ye)
    component(f, ye)
    f.text(X(HDR_W - 6.1), Y(3.1), "10", size=9)
    f.text(X(HDR_W - 6.1), Y(1.1), "1", size=9)
    f.text(X(HDR_W + 5.4), Y(3.1), "6", size=9)
    f.text(X(HDR_W + 5.4), Y(1.1), "5", size=9)
    f.text(X(9.45), Y(2.55), "USB", size=9)
    f.text(X(9.45), Y(1.1), "03C", size=9)
    f.dim(0, ye + 1.5, L, ye + 1.5, "about 39 mm", offset=18)
    f.text(0, ye + 4.0, "Component side (case end up)", size=11, anchor="start")

    # ---- back, the same edge down and the same end up (seen from behind, so left and right swap) ----
    Y2 = lambda v: ye2 - v
    outline(f, ye2)
    # the shell's tail sticks out of the tab; the rest of the shell and the switches are on the other side
    f.rect(X(-2.7), Y2(TAB_H + 1.4), 5.4, 1.4, r=0.3, sw=1.2, fill=GOLD)
    f.path(f"M{X(-SHELL_W / 2):.2f} {Y2(TAB_H):.2f} L{X(-SHELL_W / 2):.2f} {Y2(SHELL_V0):.2f} L{X(SHELL_W / 2):.2f} {Y2(SHELL_V0):.2f} L{X(SHELL_W / 2):.2f} {Y2(TAB_H):.2f}", sw=1, dash="1 0.8")
    for w in (-2.26, 1.99):   # the two plated peg holes for the socket shell
        f.circle(X(w), Y2(9.7), 0.5, sw=1, fill=DARK)
    for s in (-1, 1):
        f.rect(X(s * 10.4 - 2.2), Y2(9.3), 4.4, 2.9, r=0.3, sw=0.9, dash="1 0.8")
        f.rect(X(s * 10.1 - 2.5), Y2(3.7), 5.0, 3.5, r=0.4, sw=1.4, fill=GOLD)
    # 2 x 5 joints: bottom row 5..1 left to right (square pad = pin 1), top row 6..10
    for i in range(5):
        for v in (3.55, 1.55):
            x = X(-HDR_W + 4 - i * 2)
            if i == 0 and v == 1.55:
                f.rect(x - 0.75, Y2(v) - 0.75, 1.5, 1.5, sw=1.1, fill=GOLD)
            else:
                f.circle(x, Y2(v), 0.75, sw=1.1, fill=GOLD)
    # the six tiny surface-mount parts between the joints and the tab
    for w, v, pw, ph in ((-3.85, 8.6, 2.7, 1.45), (3.4, 8.5, 2.9, 1.35)):
        f.rect(X(w - pw / 2), Y2(v + ph / 2), pw, ph, sw=0.9, fill=PAD)
        f.rect(X(w - 0.8), Y2(v + 0.45), 1.6, 0.9, sw=0.8, fill=DARK)
    f.rect(X(-0.1 - 0.95), Y2(9.85 + 0.875), 1.9, 1.75, sw=0.9, fill=PAD)
    f.line(X(-1.05), Y2(9.85), X(0.85), Y2(9.85), sw=0.8)
    f.rect(X(-1.45 - 0.8), Y2(6.9 + 0.55), 1.6, 1.1, sw=0.9, fill=DARK)
    f.rect(X(1.15 - 0.625), Y2(6.8 + 0.65), 1.25, 1.3, sw=0.9, fill=PAD)
    f.rect(X(1.15 - 0.3), Y2(6.8 + 0.5), 0.6, 0.4, sw=0.6, fill=DARK)
    f.rect(X(1.15 - 0.3), Y2(6.8 - 0.1), 0.6, 0.4, sw=0.6, fill=DARK)
    f.text(0, ye2 + 4.0, "Back (dashed: parts on the other side)", size=11, anchor="start")

    f.callout(1, X(3.6), Y(13.6), "Mini-B socket", "faces out through the case end", side="r")
    f.callout(2, X(10.4 + 1.5), Y(7.9), "Switch", "one each side: Settings, Kill-all", side="r")
    f.callout(3, X(HDR_W + 4.9), Y(4.45), "2 x 5 header", "2.0 mm pitch, pins 1, 5, 6, 10 marked", side="r")
    f.callout(4, X(-HOLE_W), Y(HOLE_V), "Screw hole", "two, into the back cover", side="l")
    f.callout(5, X(-11.6), Y2(2.6), "Spring contact", "two, on the back", side="l")
    f.callout(6, X(3.4 + 1.0), Y2(8.5), "Small parts", "tiny surface-mount parts, back only", side="r")
    f.callout(7, X(-HDR_W + 3.9), Y2(4.1), "Header joints", "square pad = pin 1", side="r")
    f.note("Printed on the board: USB 03C")
    return f


# both photos rectified onto the drawing's coordinates (12 px/mm); local only (work/ is gitignored), set PM_TRACE_DIR to move it
UNDERLAY = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "usb/rect.png")
