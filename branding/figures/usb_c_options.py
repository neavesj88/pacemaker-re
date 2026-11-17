"""The three ways to give the USB board a USB-C port, and the wiring for option B.

No new photo tracing: every board outline here is the one traced for usb-board.svg (usb_board.py),
imported rather than redrawn, so it keeps that figure's references (2 x 5 header pitch 2.0 mm and
Mini-B shell 7.7 mm wide, 38 px/mm in the component photo, board about 38 mm long). The USB-C parts
are drawn to the standard sizes (receptacle 8.9 mm wide, opening 8.9 x 3.2 mm), not traced; the
breakout board size and wire runs are a schematic, and so is the pad order (pin 1 side not verified).
"""
from figlib import Fig, GREY, INK, MAGENTA
from figures.usb_board import L, H, C, outline, holes

FILE = "usb-c-options.svg"

OY = 15.0                  # board edge-to-top offset: room for the adapter above the case end
PX = (0.0, 48.0, 96.0)     # panel origins
ADD = "#d9e3f3"            # parts added for the option
WIRE = ("#c0392b", "#2a6fb0", "#2e8b57", INK)   # VBUS, D-, D+, GND


def board(f, ox, oy, socket):
    outline(f, ox, oy)
    holes(f, ox, oy)
    for x in (8.7, L - 8.7):          # the two switches, as in usb-board.svg
        f.rect(ox + x - 2.1, oy + 4.0, 4.2, 2.7, r=0.3, sw=1.0, fill="#fff")
        f.rect(ox + x - 0.7, oy + 3.2, 1.4, 0.8, r=0.3, sw=0.8)
    f.rect(ox + C - 5, oy + H - 4.8, 10, 4.5, sw=1.0)     # 2 x 5 header
    for i in range(5):
        for y in (H - 3.8, H - 1.8):
            f.rect(ox + C - 4 + i * 2 - 0.45, oy + y - 0.45, 0.9, 0.9, sw=0.7, fill="#1a1a1a")
    if socket:                         # Mini-B shell, through the tab to the case end
        f.rect(ox + C - 3.85, oy - 2.7, 7.7, 9.2, r=0.5, sw=1.4, fill="#fff")
        f.rect(ox + C - 2.2, oy - 1.9, 4.4, 1.2, r=0.4, sw=0.9)
        for i in range(5):
            f.rect(ox + C - 1.8 + i * 0.8, oy + 6.6, 0.4, 1.0, sw=0.7, fill="#1a1a1a")
        for dx in (-4.9, 4.1):
            for y in (0.3, 4.6):
                f.rect(ox + C + dx, oy + y, 0.8, 1.4, sw=0.7)


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
    cc = (("A5  CC1", y0 + 10 + 5 * 6.2 + 4.0), ("B5  CC2", y0 + 10 + 6 * 6.2 + 4.0))
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
    f = Fig("USB-C options for the USB board", 134, 100, scale=6, legend_title="USB-C options")
    oy = OY
    # ---- A: adapter ----
    ox = PX[0]
    board(f, ox, oy, socket=True)
    f.rect(ox + C - 3.0, oy - 2.7, 6.0, 3.6, r=0.4, sw=1.2, fill=ADD)           # plug end inside the socket
    f.rect(ox + C - 5.25, oy - 12.7, 10.5, 10.0, r=0.8, sw=1.6, fill=ADD)       # adapter body
    f.rect(ox + C - 4.45, oy - 11.7, 8.9, 3.2, r=1.6, sw=1.2, fill="#fff")      # USB-C opening 8.9 x 3.2
    caption(f, ox, oy, "A   Adapter", "Mini-USB to USB-C adapter plugs", "into the socket. No soldering.")
    # ---- B: breakout over the tab ----
    ox = PX[1]
    board(f, ox, oy, socket=False)
    pads = [ox + C - 1.6 + i * 0.8 for i in range(5)]
    for dx in (-4.9, 4.1):
        for y in (0.3, 4.6):
            f.rect(ox + C + dx, oy + y, 0.8, 1.4, sw=0.7, fill="#e8d9a8")
    for p in pads:
        f.rect(p - 0.25, oy + 6.4, 0.5, 1.4, sw=0.7, fill="#e8d9a8")
    bx = [ox + C - 3.0, ox + C - 1.0, ox + C + 1.0, ox + C + 3.0]
    for x, p, col in zip(bx, (pads[0], pads[1], pads[2], pads[4]), WIRE):
        f.line(x, oy + 4.1, p, oy + 6.6, sw=1.6, colour=col)
    f.rect(ox + C - 6.2, oy - 1.0, 12.4, 5.6, r=0.4, sw=1.4, fill=ADD)         # breakout board
    for x in bx:
        f.rect(x - 0.4, oy + 3.6, 0.8, 0.8, sw=0.7, fill="#e8d9a8")
    f.rect(ox + C - 4.45, oy - 2.7, 8.9, 5.5, r=0.5, sw=1.4, fill="#fff")      # USB-C receptacle
    caption(f, ox, oy, "B   Re-solder", "Mini-B removed, USB-C breakout over", "the tab, wired to the old pads.")
    # ---- C: receptacle on the board ----
    ox = PX[2]
    board(f, ox, oy, socket=False)
    f.rect(ox + C - 4.45, oy - 2.7, 8.9, 7.7, r=0.5, sw=1.4, fill="#fff")
    for dx in (-5.25, 4.45):
        for y in (0.3, 4.0):
            f.rect(ox + C + dx, oy + y, 0.8, 1.4, sw=0.7)
    for i in range(12):
        f.rect(ox + C - 2.75 - 0.15 + i * 0.5, oy + 5.3, 0.3, 1.0, sw=0.6, fill="#1a1a1a")
    caption(f, ox, oy, "C   Replace the board", "Same outline, USB-C receptacle", "in place of the Mini-B.")

    f.note("A: the adapter's USB-C opening is 8.9 x 3.2 mm.")
    f.note("B: a USB-C breakout wired to the old pads.")
    f.note("C: a USB-C receptacle, 8.9 mm wide.")
    f.note("Confirm pin 1 with a meter before soldering.")
    wiring(f, oy + H + 16.0)
    return f
