"""The display connection on the main board, component side, the display module at the left.

Traced from one photo of the opened unit (straight down; upright it is portrait). Nothing in the
photo has a known size except the connector itself, so the scale comes from counting: the long ZIF
has 40 contacts (pads 20.6 px apart in the 4000 px photo), and at a 0.5 mm pitch that is 41 px/mm,
which makes the contact row 19.5 mm, the ZIF body 22.2 mm long and the orange flex 21.8 mm wide, a
normal 40-way 0.5 mm FFC. The small ZIF lower down shows 15 contacts that come out at 0.50 mm too
after the rectification (below), and the two screw bosses come out the same size (4.4 mm copper
ring), both consistent with the 0.5 mm assumption. It is the same photo, so that is a consistency
check, not an independent measurement: the legend says "about 40 pins, 0.5 mm pitch: measure".

The photo is rectified so the long ZIF body (corners picked on the photo) is a 3.9 x 22.2 mm
rectangle; the width is the one free number, chosen so the small ZIF reads 0.5 mm pitch. A thin
body fixes the perspective only roughly far from it: positions at the top and bottom of the drawing
(HDD_EN, the 05B flex) can be 0.3 to 0.5 mm off. Positions were first read off an earlier
rectification and are carried over with a homography (C below); the 05B flex and the LIGHT- label
were read off the photo and use N. Silkscreen that could not be read (the numbers along the ZIF,
the part markings) is left out; only pin 1 is drawn.
"""
import os
from figlib import Fig, FILL, GREY, INK

FILE = "display-connector.svg"

# first working picture px -> second rectification px (16 px/mm, body top-left at 480, 480)
C = (0.71490863, -0.0639497, 90.1016195, -0.12290145, 0.71799953, 117.90633325, -0.00020849, -0.00012914)
# upright photo px -> second rectification px
N = (0.4152395001, 0.0273559762, -67.4663179941, -0.0063389646, 0.4469240157, -214.6747430849, -7.5898e-06, 4.36974e-05)
BODY_W, BODY_L = 3.9, 22.2                   # the long ZIF body, mm
WIN = (-22.5, -20.5, 21.7, 42.0)             # board patch shown, mm from the body's top-left
WX, WY = WIN[0], WIN[1]
W_MM, H_MM = WIN[2] - WIN[0], WIN[3] - WIN[1]

ORANGE = "#f3c98b"
BROWN = "#b9774f"
CREAM = "#f4efe6"
PART = "#d9d9d9"
PAD = "#f0b060"
# rectified photo for --trace checks; local only (work/ is gitignored), set PM_TRACE_DIR to move it
TRACE = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "display/rect3.png")


def hom(c, x, y):
    d = c[6] * x + c[7] * y + 1
    return (c[0] * x + c[1] * y + c[2]) / d, (c[3] * x + c[4] * y + c[5]) / d


def M(x, y):
    """First-rectification pixel -> figure mm."""
    u, v = hom(C, x, y)
    return (u - 480) / 16 - WX, (v - 480) / 16 - WY


def P(x, y):
    """Photo pixel -> figure mm."""
    u, v = hom(N, x, y)
    return (u - 480) / 16 - WX, (v - 480) / 16 - WY


def box(conv, x0, y0, x1, y1):
    a, b, c, d = conv(x0, y0), conv(x1, y0), conv(x1, y1), conv(x0, y1)
    return (a[0] + d[0]) / 2, (a[1] + b[1]) / 2, (b[0] + c[0]) / 2, (c[1] + d[1]) / 2


def rect(f, x0, y0, x1, y1, conv=M, **kw):
    a, b, c, d = box(conv, x0, y0, x1, y1)
    f.rect(a, b, c - a, d - b, **kw)


def disc(f, x, y, r, conv=M, **kw):
    a = conv(x, y)
    f.circle(a[0], a[1], r, **kw)


def label(f, x, y, s, rot=0, size=9, colour=GREY, conv=M):
    """Silkscreen name at a picture position; rot -90 reads upwards, 90 downwards."""
    a = conv(x, y)

    def lab():
        px, py = f._px(a[0]), f._py(a[1])
        tr = f' transform="rotate({rot} {px:.1f} {py:.1f})"' if rot else ""
        return f'<text x="{px:.1f}" y="{py + size * 0.35:.1f}" font-size="{size}" fill="{colour}" text-anchor="middle"{tr}>{s}</text>'
    f.labels.append(lab)


def build():
    f = Fig("Display connection (main board)", W_MM, H_MM + 4.0, scale=11, legend_title="Display connection")
    f.set_underlay(TRACE, -30 - WX, -30 - WY, 1022 / 16, 1315 / 16)
    # the board patch (dashed edge: the board carries on all round)
    f.rect(0, 0, W_MM, H_MM, sw=1.2, fill=FILL, dash="4 3")
    # neighbours, grey and unlabelled
    disc(f, 347, 277, 1.9, sw=1, fill=PART)
    disc(f, 297, 405, 2.35, sw=1, fill=PART)
    rect(f, 598, 247, 752, 442, sw=1, fill="#9a9a9a")
    rect(f, 600, 527, 752, 742, sw=1, fill="#9a9a9a")
    for cy in (457, 525, 573, 621, 677, 726):
        rect(f, 551, cy - 17, 575, cy + 17, sw=0.9, fill="#c9b8aa")
    # display module and the metal bracket beside it
    rect(f, 138, 445, 200, 945, sw=1, fill="#e6e6e6")
    rect(f, 203, 458, 370, 925, sw=1.4, fill="#1a1a1a")
    # the orange flex into the ZIF, the darker end is the stiffener
    rect(f, 372, 482, 432, 832, sw=1, fill=ORANGE)
    rect(f, 432, 482, 482, 832, sw=1, fill=BROWN)
    ytop = M(546, 498)[1]
    xl, xr = M(372, 600)[0], M(432, 600)[0]
    for i in range(40):
        f.line(xl, ytop + 0.5 * i, xr, ytop + 0.5 * i, sw=0.5, colour="#c9923f")
    # the long ZIF: body, lock bar, slot with the 40 contact fingers (pin 1 at the lower end)
    bx0, by0 = -WX, -WY
    f.rect(bx0, by0, BODY_W, BODY_L, sw=1.5, fill=CREAM)
    f.rect(bx0 + 2.0, by0 + 0.25, 1.55, BODY_L - 0.5, sw=0.8, fill="#2a2a2a")
    f.rect(bx0, by0, 2.0, BODY_L, sw=1.2, fill=CREAM)
    px = M(546, 498)[0]
    for i in range(40):
        y = ytop + 0.5 * i
        f.line(bx0 + 2.0, y, bx0 + 3.2, y, sw=0.9, colour="#d8d8d8")
        f.circle(px, y, 0.17, sw=0.7, fill="#fff")
    # LIGHT- / LIGHT+ pads
    for x, y in ((504, 440), (530, 857)):
        disc(f, x, y, 0.62, sw=1, fill="#fff")
        disc(f, x, y, 0.42, sw=0.8, fill=PAD)
    label(f, 1393, 1570, "LIGHT-", -90, conv=P)
    label(f, 567, 855, "LIGHT+", -90)
    label(f, 529, 842, "1", 0, 10, INK)
    # test points
    for x, y, nm, tx, ty, rot in (
            (377, 182, "HDD_EN", 352, 182, -90),
            (495, 331, "P_VIN", 490, 352, 0),
            (567, 330, "P_VBKUP", 546, 300, -90),
            (593, 816, "DVS_SWA", 593, 857, -90),
            (638, 825, "DVS_SWB", 655, 835, -90),
            (715, 809, "P_SW1", 695, 780, 90)):
        disc(f, x, y, 0.5, sw=1, fill=PAD)
        label(f, tx, ty, nm, rot)
    disc(f, 672, 800, 0.4, sw=1, fill="#fff")
    label(f, 671, 822, "TP3", 90)
    disc(f, 734, 836, 0.7, sw=1)
    label(f, 736, 800, "USB_EN", 90)
    # the two screw bosses
    for x, y in ((543, 385), (672, 934)):
        disc(f, x, y, 2.2, sw=1.2, fill=PAD)
        disc(f, x, y, 1.7, sw=1, fill="#fff")
    # the small ZIF with the 05B flex (keypad / touch, not the display)
    rect(f, 1045, 3110, 1395, 3312, conv=P, sw=1, fill=ORANGE)
    rect(f, 361, 1012, 567, 1032, sw=1, fill="#444")
    bar = box(M, 371, 964, 562, 1012)
    for i in range(15):
        x = M(255 / 4 + 350 + 30.5 * i / 4, 958)[0]
        f.rect(x - 0.12, bar[1] - 1.0, 0.24, 1.2, sw=0.6, fill="#d8d8d8")
    f.rect(bar[0], bar[1], bar[2] - bar[0], bar[3] - bar[1], sw=1.4, fill=CREAM)
    label(f, 1345, 3230, "05B", -90, 10, "#5a2a10", conv=P)
    # captions and dimension
    f.text(0.0, H_MM + 2.6, "Component side, main board (grey: neighbouring parts)", size=11, anchor="start")
    a = M(577, 498); b = (a[0], M(577, 810)[1])
    f.dim(a[0], a[1], b[0], b[1], "40 contacts, 0.5 mm pitch: 19.5 mm", offset=16)
    # callouts
    f.callout(1, *M(290, 560), "Display module", "the black block, glass under it", side="l")
    f.callout(2, *M(402, 640), "Display flex", "orange, into the long ZIF", side="l")
    f.callout(3, *M(523, 490), "Long ZIF", "about 40 pins, 0.5 mm pitch: measure", side="r")
    f.callout(4, *M(495, 760), "Lock bar", "white, flips up to free the flex", side="l")
    f.callout(5, *M(504, 440), "LIGHT-", "backlight pad", side="l")
    f.callout(6, *M(530, 857), "LIGHT+", "backlight pad, pin 1 end", side="l")
    f.callout(7, *M(377, 182), "Test points", "eight, each named beside it", side="r")
    f.callout(8, *M(470, 988), "Small ZIF", "flex 05B: keypad / touch, not display", side="l")
    f.callout(9, *M(672, 934), "Screw boss", "two, copper ring", side="r")
    f.note("Display controller: Himax HX8347-A, 240 x 320.")
    f.note("Backlight: from LIGHT+ / LIGHT-.")
    return f
