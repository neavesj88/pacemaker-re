"""Battery fit: the battery end of the main assembly in plan and the two cells in side view.

Traced from the owner's photo of the opened unit (straight down on the battery end), rectified with
trace.py rectify at 20.5 px/mm (quad = the cell body, 30 x 48 mm). Scale check from parts of known
size in the same photo: the 2 x 5 socket on the small board has a 2.0 mm pitch (about 20.5 to 21 px/mm
across its five rows), the original cell is 30 mm wide and about 51 mm long with its foil seal strip
(48 mm body), and the gold contact is about 3.6 x 5 mm. The ruler in the photos was not used (it lies
at a different height). Parts differ in height above the board by a few millimetres, so features on
the cell's top surface and on the board can disagree by up to about 0.5 mm; the trace is good to about
0.5 mm.

Plan coordinates are millimetres from the cell body's top-left corner (x right, y down, the terminal end
of the cell at the top); FX/FY move them into the figure's frame. The pouch runs from y = -2.9 (foil
edge, where the wires leave) to y = 48.4; the 48 mm body starts at y = 0.4 under the seal strip.
The side views are to the same scale.
"""
import os

from figlib import Fig, FILL, GREY, INK

FILE = "battery-fit.svg"

FX, FY = 28.5, 11.5          # plan origin (cell body's top-left corner) in figure millimetres
CW = 30.2                    # pouch width
GOLD = "#e8d9a8"
PINK = "#f6c9e4"
RED = "#c8281e"
DARK = "#2e2e2e"
FOAM = "#4d4d4d"

SX = 1.5                     # left edge of everything under the plan
OLD_T, NEW_T, OLD_L, NEW_L = 7.2, 10.0, 48.0, 48.5

def colour_line(f, d, w, colour, outline=True):
    """A wire: a dark outline under a coloured stroke (w in px)."""
    if outline:
        f.items.append(f'<path d="{d}" fill="none" stroke="{INK}" stroke-width="{(w + 2.4) / f.s:.3f}" stroke-linecap="round" stroke-linejoin="round"/>')
    f.items.append(f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="{w / f.s:.3f}" stroke-linecap="round" stroke-linejoin="round"/>')


def rot_text(f, x, y, s, colour):
    """Small text centred on a mm position, reading downward."""
    def draw():
        px, py = f._px(x) - 3, f._py(y)
        return f'<text x="{px:.1f}" y="{py:.1f}" font-size="9" fill="{colour}" text-anchor="middle" transform="rotate(90 {px:.1f} {py:.1f})">{s}</text>'
    f.labels.append(draw)


def build():
    f = Fig("Battery fit", 78, 106, scale=7.5, legend_title="Battery end")
    if os.path.exists(UNDERLAY):
        f.set_underlay(UNDERLAY, FX - 28, FY - 28, 86.0, 104.0)

    def X(x): return FX + x
    def Y(y): return FY + y

    # ---- plan -------------------------------------------------------------------------------------
    # main board: top edge with the tongue that carries the plug, ends under the cell
    f.path(f"M{X(-27)} {Y(-8.6)} L{X(23.4)} {Y(-8.6)} Q{X(25)} {Y(-8.6)} {X(25)} {Y(-7)} L{X(25)} {Y(48.7)} L{X(-27)} {Y(48.7)}", sw=1.8, fill="#fff")
    # brass boss in the case, in a cut-out of the board tongue
    f.circle(X(10.0), Y(-8.5), 2.6, sw=1.4, fill=DARK)
    f.circle(X(10.0), Y(-8.5), 1.25, sw=1.2, fill=GOLD)
    f.circle(X(10.0), Y(-8.5), 0.65, sw=0.8, fill=DARK)

    # slot in the board with the black tape over it and the silver bar beside it
    f.rect(X(-26.0), Y(6.1), 9.8, 26.5, sw=1.3, fill=DARK)
    f.rect(X(-16.2), Y(5.6), 2.9, 28.4, r=1.3, sw=1.3, fill="#d9d9d9")
    # chips, crystal, gold part, coil beside the cell
    f.rect(X(-23.2), Y(0.9), 4.7, 4.6, r=0.3, sw=1.2, fill=DARK)
    f.rect(X(-21.2), Y(-5.0), 4.2, 2.7, r=0.3, sw=1.2, fill=GOLD)
    f.rect(X(-10.9), Y(14.1), 5.3, 5.8, r=0.3, sw=1.2, fill=DARK)
    f.rect(X(-7.8), Y(8.0), 4.9, 3.0, r=0.3, sw=1.2, fill="#d9c27a")
    f.circle(X(-22.6), Y(35.5), 2.6, sw=1.2, fill=DARK)

    # board connector beside the plug: two rows of pins
    f.rect(X(0.1), Y(-7.8), 5.9, 4.3, r=0.3, sw=1.3, fill=DARK)
    for i in range(5):
        for py in (-7.0, -5.0):
            f.rect(X(1.0 + i * 1.0) - 0.3, Y(py) - 0.3, 0.6, 0.6, sw=0.6, fill=GOLD)

    # small board right of the cell, with its 2 x 5 socket and gold contact
    f.rect(X(29.0), Y(2.3), 12.85, 40.4, r=1.0, sw=1.8, fill=FILL)
    f.rect(X(30.3), Y(15.7), 5.7, 11.2, sw=1.3, fill=DARK)
    for i in range(5):
        for cx in (31.65, 33.65):
            f.rect(X(cx) - 0.45, Y(17.8 + i * 2.0) - 0.45, 0.9, 0.9, sw=0.7, fill="#0d0d0d")
        f.rect(X(35.1), Y(18.4 + i * 1.95) - 0.5, 1.2, 1.0, sw=0.6, fill=GOLD)
    # white-outlined SMD pads beside the socket
    for sx0, sy0, sw0 in ((30.5, 14.8, 1.8), (30.5, 15.95, 1.8), (33.0, 15.95, 1.8), (30.65, 27.85, 1.65)):
        f.rect(X(sx0), Y(sy0), sw0, 0.9, sw=0.7, fill="#fff")
    # pin numbers are printed along the right edge, reading downward
    for lab, ly, col in (("6", 16.8, "#e0e0e0"), ("10", 27.85, GREY)):
        rot_text(f, X(35.8), Y(ly), lab, col)
    f.rect(X(37.2), Y(19.9), 3.7, 5.0, r=0.3, sw=1.4, fill=GOLD)
    f.rect(X(37.9), Y(20.8), 2.9, 3.0, r=0.2, sw=0.8, fill="#d8b866")
    # case screw bosses beside the small board
    for bx, by in ((45.0, 0.4), (45.0, 44.5)):
        f.circle(X(bx), Y(by), 1.9, sw=1.3, fill=DARK)
        f.circle(X(bx), Y(by), 1.15, sw=1.0, fill=GOLD)
        f.circle(X(bx), Y(by), 0.55, sw=0.8, fill=DARK)

    # plug on the board tongue: white housing, open to the right where the wires go in
    f.rect(X(16.9), Y(-6.2), 4.3, 3.0, r=0.2, sw=1.4, fill="#fff")
    f.rect(X(17.9), Y(-5.4), 2.4, 1.5, sw=0.7, fill="#1a1a1a")

    # wires first: they run in under the cell's foil at the terminal end
    colour_line(f, f"M{X(19.6)} {Y(-4.2)} L{X(24.3)} {Y(-4.2)}", 4.0, "#2a2a2a", outline=False)
    colour_line(f, f"M{X(19.6)} {Y(-5.1)} C{X(22.0)} {Y(-5.8)} {X(25.0)} {Y(-5.4)} {X(26.8)} {Y(-5.8)} Q{X(28.2)} {Y(-7.2)} {X(28.6)} {Y(-4.8)} L{X(28.6)} {Y(-3.1)}", 4.0, RED)

    # the cell's pouch with its seal strip (above the fold line) and foam pad
    f.rect(X(-0.4), Y(-2.9), CW, 51.3, r=0.9, sw=1.8, fill=FILL)
    f.line(X(-0.4), Y(0.4), X(-0.4 + CW), Y(0.4), sw=1.0, colour=GREY, dash="2 1.2")
    f.rect(X(6.2), Y(13.9), 15.95, 21.5, sw=1.3, fill=FOAM)

    # screws and open holes left of the cell
    for sx, sy in ((-2.2, -1.2), (-2.7, 45.6)):
        f.circle(X(sx), Y(sy), 2.05, sw=1.2, fill=GOLD)
        f.circle(X(sx), Y(sy), 1.35, sw=1.2, fill="#fff")
        f.line(X(sx) - 0.8, Y(sy), X(sx) + 0.8, Y(sy), sw=1.0)
        f.line(X(sx), Y(sy) - 0.8, X(sx), Y(sy) + 0.8, sw=1.0)
    for hx, hy, r in ((-7.2, -0.9, 0.9), (-7.9, 45.0, 1.1)):
        f.circle(X(hx), Y(hy), r, sw=1.2, fill="#555")

    # dimensions
    f.dim(X(-0.4), Y(51.6), X(-0.4 + CW), Y(51.6), "30 mm", offset=18)
    f.text(SX, Y(56.3), "Plan: the battery end, back cover off", size=11, anchor="start")
    f.text(SX, Y(58.5), "(terminal end of the cell at the top)", size=11, anchor="start")

    # ---- side view, same scale ------------------------------------------------------------------
    b1 = 82.0
    f.rect(SX + 3, b1 - OLD_T, OLD_L, OLD_T, r=1.0, sw=1.6, fill=FILL)
    f.dim(SX + 0.5, b1 - OLD_T, SX + 0.5, b1, "7.2 mm", offset=-1)
    f.text(SX, b1 + 4.6, "Original cell: about 48 x 30 x 7.2 mm, 1100 mAh", size=11, anchor="start")

    b2 = 100.0
    f.rect(SX + 3, b2 - NEW_T, NEW_L, NEW_T, r=1.0, sw=1.6, fill=FILL)
    f.rect(SX + 3.4, b2 - NEW_T + 0.4, NEW_L - 0.8, NEW_T - OLD_T - 0.4, r=0.6, sw=0, fill=PINK)
    f.line(SX + 1.5, b2 - OLD_T, SX + 3 + NEW_L + 1.5, b2 - OLD_T, sw=1.4, colour=GREY, dash="2 1.2")
    f.rect(SX + 3, b2 - NEW_T, NEW_L, NEW_T, r=1.0, sw=1.6, fill="none")
    f.dim(SX + 0.5, b2 - NEW_T, SX + 0.5, b2, "10 mm", offset=-1)
    f.dim(SX + 3 + NEW_L + 2.5, b2 - NEW_T, SX + 3 + NEW_L + 2.5, b2 - OLD_T, "", offset=0)
    f.text(SX + 3 + NEW_L + 3.6, b2 - OLD_T - 0.6, "+2.8 mm", size=11, colour="#b00b7c", anchor="start", weight=700)
    f.text(SX + 3 + NEW_L + 3.6, b2 - OLD_T + 1.9, "original top", size=10, anchor="start")
    f.text(SX, b2 + 4.6, "New cell, HXJNLDC 103048: 48.5 x 30 x 10 mm, 1600 mAh", size=11, anchor="start")

    # ---- callouts ---------------------------------------------------------------------------------
    f.callout(1, X(26.5), Y(6.0), "Cell", "original, 1100 mAh, about 48 x 30 mm", side="r")
    f.callout(2, X(21.0), Y(31.0), "Foam pad", "between the cell and the back", side="r")
    f.callout(3, X(17.5), Y(-5.9), "2-pin plug", "white, at the cell's terminal end", side="r")
    f.callout(4, X(28.6), Y(-3.8), "Cell wires", "red and black, run in under the foil", side="r")
    f.callout(5, X(36.0), Y(11.0), "Small board", "right of the cell", side="r")
    f.callout(6, X(32.6), Y(22.0), "2 x 5 socket", "2.0 mm pitch, pins 6 and 10 marked", side="r")
    f.callout(7, X(40.4), Y(22.4), "Gold contact", "on the small board", side="r")
    f.note("")
    f.note("Check the back closes without")
    f.note("pressing on the cell.")
    f.note("")
    f.note("The new cell's plug is too big for")
    f.note("the socket: move the original plug")
    f.note("onto it, red to the + side.")
    return f


# rectified photo for --trace checks; local only (work/ is gitignored), set PM_TRACE_DIR to move it
UNDERLAY = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "battery/rect.png")
