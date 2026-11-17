"""The three ways to give the USB board a USB-C port, and the wiring for option B.

The board outline, holes, switches and solder lands come from usb_board.py, checked here against the
component-side photo straightened with the 2 x 5 header (2.0 mm pitch, 10 x 4.5 mm silkscreen box) as
the scale (38 px/mm; the Mini-B shell then reads 7.3 mm in the photo, 7.7 mm is its real width). The
USB-C parts are drawn to the standard sizes (receptacle opening 8.34 x 2.56 mm, shell about 8.9 mm
wide), not traced; the adapter, breakout board and wire runs are a schematic, and so is the pad order
(pin 1 side not verified). The straightened photo goes under panel A when a trace folder is given.
"""
import os

from figlib import Fig, GREY, INK, MAGENTA
from figures.usb_board import L, C, TAB_H, SHELL_V1, outline, component, GOLD

H = TAB_H                  # straight edge to the tab top
UP = SHELL_V1 - TAB_H      # how far the Mini-B shell stands past the tab

FILE = "usb-c-options.svg"

# the component-side photo straightened onto the drawing's mm (24 px/mm); local only (work/ is gitignored)
UNDERLAY = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "usbc/rect.png")

OY = 15.0                  # board edge-to-top offset: room for the adapter above the case end
PX = (0.0, 48.0, 96.0)     # panel origins
ADD = "#d9e3f3"            # parts added for the option
WIRE = ("#c0392b", "#2a6fb0", "#2e8b57", INK)   # VBUS, D-, D+, GND


def board(f, ox, oy, socket, lands=True):
    """The USB board as in usb-board.svg, its tab top at oy."""
    outline(f, oy + H, ox)
    component(f, oy + H, ox, socket=socket, lands=lands)


def caption(f, ox, oy, head, l1, l2):
    y = oy + H + 4.0
    f.text(ox, y, head, size=12, colour=INK, anchor="start", weight=700)
    f.text(ox, y + 3.2, l1, size=10.5, anchor="start")
    f.text(ox, y + 6.0, l2, size=10.5, anchor="start")


def wiring(f, y0):
    f.text(0, y0, "Option B wiring", size=12, colour=INK, anchor="start", weight=700)
    f.text(2, y0 + 5.2, "Mini-B pads (old)", size=10.5, anchor="start")
    f.text(68, y0 + 5.2, "USB-C breakout pads", size=10.5, anchor="start")
    rows = (("1  VBUS", "A4  B4  A9  B9", WIRE[0]), ("2  D-", "A7  B7", WIRE[1]), ("3  D+", "A6  B6", WIRE[2]),
            ("4  ID", None, GREY), ("5  GND", "A1  B1  A12  B12", WIRE[3]))
    y = [y0 + 10 + i * 6.2 for i in range(5)]
    for (a, b, col), yy in zip(rows, y):
        f.rect(2, yy - 2.2, 22, 4.4, r=0.4, sw=1.0, fill="#fff")
        f.text(4, yy + 0.7, a, size=11, colour=INK, anchor="start")
        if b:
            f.line(24, yy, 68, yy, sw=1.8, colour=col)
            f.rect(68, yy - 2.2, 32, 4.4, r=0.4, sw=1.0, fill=ADD)
            f.text(70, yy + 0.7, b, size=11, colour=INK, anchor="start")
        else:
            f.line(24, yy, 44, yy, sw=1.8, colour=col)
            f.line(44.8, yy - 1.0, 46.8, yy + 1.0, sw=1.6, colour=INK)
            f.line(44.8, yy + 1.0, 46.8, yy - 1.0, sw=1.6, colour=INK)
            f.text(49, yy + 0.7, "not connected", size=11, anchor="start")
    rail = 118.0
    f.line(100, y[4], rail, y[4], sw=1.8, colour=WIRE[3])
    cc = (("A5  CC1", y0 + 10 + 5 * 6.2 + 4.0), ("B5  CC2", y0 + 10 + 5 * 6.2 + 11.6))
    for name, yy in cc:
        f.rect(68, yy - 2.2, 22, 4.4, r=0.4, sw=1.0, fill=ADD)
        f.text(70, yy + 0.7, name, size=11, colour=INK, anchor="start")
        f.line(90, yy, 96, yy, sw=1.4, colour=INK)
        f.rect(96, yy - 1.5, 12, 3.0, r=0.3, sw=1.2, fill="#fff")
        f.text(102, yy - 2.6, "5.1 kOhm", size=10.5, anchor="middle")
        f.line(108, yy, rail, yy, sw=1.4, colour=WIRE[3])
    f.line(rail, y[4], rail, cc[1][1], sw=1.8, colour=WIRE[3])
    f.circle(rail, y[4], 0.7, sw=0.8, fill=INK)
    f.text(rail + 2.5, y[4] + 0.7, "GND", size=11, colour=INK, anchor="start")


def build():
    f = Fig("USB-C options for the USB board", 134, 102, scale=6, legend_title="USB-C options")
    oy = OY
    # ---- A: adapter ----
    ox = PX[0]
    f.set_underlay(UNDERLAY, ox - 2, oy - 7.5, 1008 / 24, 878 / 24)
    board(f, ox, oy, socket=True)
    f.rect(ox + C - 3.4, oy - UP, 6.8, 5.7, r=0.4, sw=1.2, fill=ADD)           # Mini-B plug end inside the socket (6.8 mm wide)
    f.rect(ox + C - 5.25, oy - UP - 10.0, 10.5, 10.0, r=0.8, sw=1.6, fill=ADD)       # adapter body (typical, not measured)
    f.rect(ox + C - 4.17, oy - UP - 7.7, 8.34, 2.56, r=1.28, sw=1.2, fill="#fff")   # USB-C opening 8.34 x 2.56
    caption(f, ox, oy, "A   Adapter", "Mini-USB to USB-C adapter plugs", "into the socket. No soldering.")
    # ---- B: breakout over the tab ----
    ox = PX[1]
    board(f, ox, oy, socket=False)                                              # shell-leg lands left empty
    pads = [ox + C - 1.6 + i * 0.8 for i in range(5)]
    lead_y = oy + H - 6.55                                                      # the five lead pads, as in usb-board.svg
    for p in pads:
        f.rect(p - 0.25, lead_y, 0.5, 1.15, sw=0.7, fill=GOLD)
    bx = [ox + C - 3.0, ox + C - 1.0, ox + C + 1.0, ox + C + 3.0]
    for x, p, col in zip(bx, (pads[0], pads[1], pads[2], pads[4]), WIRE):
        f.line(x, oy + 4.1, p, lead_y + 0.2, sw=1.6, colour=col)
    f.rect(ox + C - 5.2, oy - 1.0, 10.4, 5.6, r=0.4, sw=1.4, fill=ADD)         # breakout board, narrower than the 11.3 mm tab
    for x in bx:
        f.rect(x - 0.4, oy + 3.6, 0.8, 0.8, sw=0.7, fill="#e8d9a8")
    f.rect(ox + C - 4.45, oy - 2.7, 8.9, 5.5, r=0.5, sw=1.4, fill="#fff")      # USB-C receptacle
    caption(f, ox, oy, "B   Re-solder", "Mini-B removed, USB-C breakout over", "the tab, wired to the old pads.")
    # ---- C: receptacle on the board ----
    ox = PX[2]
    board(f, ox, oy, socket=False, lands=False)
    f.rect(ox + C - 4.45, oy - 2.7, 8.9, 7.7, r=0.5, sw=1.4, fill="#fff")
    for dx in (-5.25, 4.45):                                                     # the receptacle's shield legs
        for y in (0.3, 4.0):
            f.rect(ox + C + dx, oy + y, 0.8, 1.4, sw=0.7)
    for i in range(12):
        f.rect(ox + C - 2.75 - 0.15 + i * 0.5, oy + 5.3, 0.3, 1.0, sw=0.6, fill="#1a1a1a")
    caption(f, ox, oy, "C   Replace the board", "New board, same outline, holes,", "switches and header, with USB-C.")

    f.note("A: the USB-C opening is 8.3 x 2.6 mm.")
    f.note("B: a USB-C breakout wired to the old pads.")
    f.note("C: a USB-C receptacle, about 8.9 mm wide.")
    f.note("Adapter and breakout sizes are typical, not")
    f.note("measured. Confirm pin 1 with a meter first.")
    wiring(f, oy + H + 16.0)
    return f
