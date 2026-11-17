"""Battery fit: the battery end of the main assembly in plan (left) and the two cells in side view (right).

Traced from the owner's photo of the opened unit (straight down on the battery end), rectified with
trace.py rectify so the photographed cell outline is exactly 30 x 48 mm (the stated reference for the
original cell). Checked against parts of known size in the same photo: the 2 x 5 socket on the small
board has a 2.0 mm pitch and its housing measures about 10.2 mm long, which agrees with the cell-based
scale (about 21 px/mm in the photo; 21 px/mm in the rectified image). The ruler in the photos was not
used. The second photo (cell turned 90 degrees) was used only to confirm where the plug and wires run.

Plan coordinates below are millimetres from the cell's top-left corner (x right, y down, the terminal
end of the cell at the top); FX/FY move them into the figure's frame. The cell is drawn as a plain
30 x 48 rectangle: its right edge is a little ragged in the photo (the foil flap bulges), so the
photographed outline is within about 0.5 mm of it. The side view is to the same scale.
"""
import os

from figlib import Fig, FILL, GREY, INK

FILE = "battery-fit.svg"

FX, FY = 11.0, 11.0          # plan origin (cell corner) in figure millimetres
CW, CL = 29.8, 48.0          # cell outline
GOLD = "#e8d9a8"
PINK = "#f6c9e4"
RED = "#c8281e"

SX = 4.0                     # left edge of the side views (under the plan)
OLD_T, NEW_T, OLD_L, NEW_L = 7.2, 10.0, 48.0, 48.5


def P(f, x, y):
    return FX + x, FY + y


def colour_line(f, d, w, colour, outline=True):
    """A wire: a dark outline under a coloured stroke (w in px)."""
    if outline:
        f.items.append(f'<path d="{d}" fill="none" stroke="{INK}" stroke-width="{(w + 2.4) / f.s:.3f}" stroke-linecap="round" stroke-linejoin="round"/>')
    f.items.append(f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="{w / f.s:.3f}" stroke-linecap="round" stroke-linejoin="round"/>')


def build():
    f = Fig("Battery fit", 60, 114, scale=8.5, legend_title="Battery end")
    if os.path.exists(UNDERLAY):
        f.set_underlay(UNDERLAY, FX - 16, FY - 16, 62, 80)

    def X(x): return FX + x
    def Y(y): return FY + y

    # ---- plan -------------------------------------------------------------------------------------
    # main board edge along the cell's terminal end (the board runs on under the cell)
    f.path(f"M{X(-9.2)} {Y(-5.0)} L{X(23.2)} {Y(-5.0)} Q{X(24.6)} {Y(-5.0)} {X(24.6)} {Y(-3.6)} L{X(24.6)} {Y(0.5)}", sw=1.8)
    # brass boss in the case beside the board tongue
    f.circle(X(10.1), Y(-5.4), 3.0, sw=1.4, fill="#fff")
    f.circle(X(10.1), Y(-5.4), 0.95, sw=1.2, fill=GOLD)
    f.circle(X(10.1), Y(-5.4), 0.4, sw=0.8, fill="#fff")

    # small board right of the cell, with its 2 x 5 socket and gold contact
    f.rect(X(29.2), Y(5.0), 11.6, 37.0, r=1.4, sw=1.8, fill=FILL)
    f.rect(X(30.2), Y(16.9), 4.8, 10.2, sw=1.3, fill="#fff")
    for i in range(5):
        for cx in (31.6, 33.6):
            f.rect(X(cx) - 0.5, Y(18.0 + i * 2.0) - 0.5, 1.0, 1.0, sw=0.8, fill="#1a1a1a")
    f.text(X(35.5), Y(17.7), "6", size=10, anchor="start")
    f.text(X(35.5), Y(27.6), "10", size=10, anchor="start")
    f.rect(X(37.1), Y(20.4), 3.4, 4.8, r=0.3, sw=1.4, fill=GOLD)
    f.rect(X(37.7), Y(21.0), 2.2, 3.6, r=0.2, sw=0.8, fill="#f6edcb")
    # case screw bosses beside the small board
    for by in (2.3, 44.4):
        f.circle(X(44.5), Y(by), 1.7, sw=1.3, fill="#fff")
        f.circle(X(44.5), Y(by), 0.7, sw=1.0, fill=GOLD)

    # wires first: they run in under the cell's foil at the terminal end
    colour_line(f, f"M{X(18.5)} {Y(-2.2)} Q{X(18.6)} {Y(-1.0)} {X(20.6)} {Y(-0.9)}", 4.5, "#2a2a2a", outline=False)
    colour_line(f, f"M{X(19.9)} {Y(-2.2)} L{X(22.4)} {Y(-1.8)} Q{X(23.4)} {Y(-2.4)} {X(24.0)} {Y(-3.0)} Q{X(25.5)} {Y(-3.7)} {X(27.1)} {Y(-3.2)} Q{X(28.5)} {Y(-2.6)} {X(28.5)} {Y(-0.6)}", 4.5, RED)

    # the cell with its foam pad
    f.rect(X(0.1), Y(0.1), CW, CL - 0.1, r=0.9, sw=1.8, fill=FILL)
    f.rect(X(6.7), Y(15.5), 15.6, 20.0, sw=1.3, fill="#d6d6d6")

    # plug on the board tongue: white housing, two pin openings
    f.rect(X(17.0), Y(-3.1), 4.2, 2.2, r=0.2, sw=1.4, fill="#fff")
    for px in (18.5, 19.9):
        f.rect(X(px) - 0.3, Y(-2.5), 0.6, 0.9, sw=0.8, fill="#1a1a1a")
    colour_line(f, f"M{X(18.5)} {Y(-1.7)} L{X(18.5)} {Y(-1.0)}", 4.5, "#2a2a2a", outline=False)

    # screws and open holes left of the cell
    for sy, sx in ((1.3, -1.75), (45.3, -1.75)):
        f.circle(X(sx), Y(sy), 2.1, sw=1.2, fill=GOLD)
        f.circle(X(sx), Y(sy), 1.3, sw=1.2, fill="#fff")
        f.line(X(sx) - 0.8, Y(sy), X(sx) + 0.8, Y(sy), sw=1.0)
        f.line(X(sx), Y(sy) - 0.8, X(sx), Y(sy) + 0.8, sw=1.0)
    for hy, r in ((1.8, 1.0), (44.4, 1.0)):
        f.circle(X(-6.6), Y(hy), r, sw=1.2, fill="#fff")

    # dimensions
    f.dim(X(0.1), Y(50.2), X(0.1 + 29.8), Y(50.2), "30 mm", offset=14)
    f.text(X(-9.0), Y(55.2), "Plan: the battery end, back cover off", size=11, anchor="start")
    f.text(X(-9.0), Y(57.4), "(terminal end of the cell at the top)", size=11, anchor="start")

    # ---- side view, same scale ------------------------------------------------------------------
    b1 = 84.0
    f.rect(SX, b1 - OLD_T, OLD_L, OLD_T, r=1.0, sw=1.6, fill=FILL)
    f.dim(SX - 2.5, b1 - OLD_T, SX - 2.5, b1, "7.2 mm", offset=-1)
    f.text(SX, b1 + 4.6, "Original: about 48 x 30 x 7.2 mm, 1100 mAh", size=11, anchor="start")

    b2 = 103.0
    f.rect(SX, b2 - NEW_T, NEW_L, NEW_T, r=1.0, sw=1.6, fill=FILL)
    f.rect(SX + 0.4, b2 - NEW_T + 0.4, NEW_L - 0.8, NEW_T - OLD_T - 0.4, r=0.6, sw=0, fill=PINK)
    f.line(SX - 1.5, b2 - OLD_T, SX + NEW_L + 1.5, b2 - OLD_T, sw=1.4, colour=GREY, dash="2 1.2")
    f.rect(SX, b2 - NEW_T, NEW_L, NEW_T, r=1.0, sw=1.6, fill="none")
    f.dim(SX - 2.5, b2 - NEW_T, SX - 2.5, b2, "10 mm", offset=-1)
    f.dim(SX + NEW_L + 2.5, b2 - NEW_T, SX + NEW_L + 2.5, b2 - OLD_T, "", offset=0)
    f.text(SX + NEW_L + 3.6, b2 - OLD_T - 0.6, "+2.8 mm", size=11, colour="#b00b7c", anchor="start", weight=700)
    f.text(SX + NEW_L + 3.6, b2 - OLD_T + 1.9, "original top", size=10, anchor="start")
    f.text(SX, b2 + 4.6, "HXJNLDC 103048: 48.5 x 30 x 10 mm, 1600 mAh", size=11, anchor="start")

    # ---- callouts ---------------------------------------------------------------------------------
    f.callout(1, X(3.0), Y(10.0), "Cell", "original, 1100 mAh, about 48 x 30 mm", side="l")
    f.callout(2, X(8.0), Y(22.0), "Foam pad", "between the cell and the back", side="l")
    f.callout(3, X(17.0), Y(-1.9), "2-pin plug", "white, at the cell's terminal end", side="l")
    f.callout(4, X(28.5), Y(-1.6), "Cell wires", "red and black, run in under the foil", side="r")
    f.callout(5, X(38.2), Y(9.0), "Small board", "right of the cell", side="r")
    f.callout(6, X(34.8), Y(18.6), "2 x 5 socket", "2.0 mm pitch, pins 6 and 10 marked", side="r")
    f.callout(7, X(40.5), Y(23.0), "Gold contact", "on the small board", side="r")
    f.note("")
    f.note("Check the back closes without")
    f.note("pressing on the cell.")
    f.note("")
    f.note("2-pin plug: measure the pitch")
    f.note("(PH 2.0 / ZH 1.5 / SH 1.0 mm)")
    f.note("and the polarity")
    return f


# rectified photo for --trace checks; local only (work/ is gitignored), set PM_TRACE_DIR to move it
UNDERLAY = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "battery/rect.png")
