"""The display connection on the main board, component side, the display module at the left.

Traced from one photo of the opened unit (straight down; upright it is portrait). Nothing in the
photo has a known size except the connector itself, so the scale comes from counting: the long ZIF
has 40 contacts (pads 20.6 px apart in the 4000 px photo), and at a 0.5 mm pitch that is 41 px/mm,
which makes the contact row 19.5 mm, the ZIF body 22.2 mm long and the orange flex 21.8 mm wide, a
normal 40-way 0.5 mm FFC. The small ZIF lower down shows 15 contacts that come out at 0.50 mm too
after the rectification (below), and the two screw bosses come out the same size (4.4 mm copper
ring), both consistent with the 0.5 mm assumption. It is the same photo, so that is a consistency
check, not an independent measurement: the legend says the pitch is assumed, measure it.

The photo is rectified so the long ZIF body (corners picked on the photo) is a 3.9 x 22.2 mm
rectangle; the width is the one free number, chosen so the small ZIF reads 0.5 mm pitch. A thin
body fixes the perspective only roughly far from it: positions at the top and bottom of the drawing
(HDD_EN, the 05B flex) can be 0.3 to 0.5 mm off. Positions were first read off an earlier
rectification and are carried over with a homography (C below); the 05B flex and the LIGHT- label
were read off the photo and use N. Silkscreen that could not be read (the numbers along the ZIF,
the part markings) is left out; only pin 1 is drawn.

Second pass: edge scans along rows and columns of the rectified photo (rect3.png; figure millimetres
are u / 16 - 7.5 and v / 16 - 9.5) showed the display module 0.65 mm too far right at its left edge,
the flex and the two big chips 0.2 to 0.7 mm off and the bracket 0.3 to 0.55 mm too wide. Those are
now placed straight in figure millimetres from the scans (marked "scan"). Added: the board's lower
edge with its notch and the 05B flex tail, rounded bracket and flex corners, six more test pads
(TP4, TP19, TP5, P_VIO, P_VDIG and one unnamed), the gold tabs and card-socket housings, and the
larger chips as grey neighbours. The added pads and chip by P_VIO are read off the same photo with N
(P below) and were checked against an overlay of the rectified photo.
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
PAD = "#f0b060"
SMD = "#c9b8aa"       # chip resistors and capacitors
IC = "#9a9a9a"        # larger neighbouring chips
GOLD = "#e2c68a"      # unlabelled gold tabs on the board
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


def mr(f, x0, y0, x1, y1, **kw):
    """Rectangle from two corners in figure mm."""
    f.rect(x0, y0, x1 - x0, y1 - y0, **kw)


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
    # the board patch: dashed where the board carries on, solid along its real lower edge (scan: the notch
    # is 5.9 mm wide and 2.6 mm deep, the 05B flex tail goes through it)
    nl, nr, nt, r = 18.7, 24.6, H_MM - 2.6, 0.8
    f.path(f"M0 0 L{W_MM} 0 L{W_MM} {H_MM} L{nr} {H_MM} L{nr} {nt + r} Q{nr} {nt} {nr - r} {nt} L{nl + r} {nt} Q{nl} {nt} {nl} {nt + r} L{nl} {H_MM} L0 {H_MM} Z", sw=0, fill=FILL)
    f.path(f"M0 {H_MM} L0 0 L{W_MM} 0 L{W_MM} {H_MM}", sw=1.2, dash="4 3")
    f.path(f"M0 {H_MM} L{nl} {H_MM} L{nl} {nt + r} Q{nl} {nt} {nl + r} {nt} L{nr - r} {nt} Q{nr} {nt} {nr} {nt + r} L{nr} {H_MM} L{W_MM} {H_MM}", sw=1.2)
    # neighbours, grey and unlabelled
    disc(f, 347, 277, 1.9, sw=1, fill=IC)
    disc(f, 297, 405, 2.35, sw=1, fill=IC)
    mr(f, 29.6, 6.4, 40.1, 18.0, sw=1, fill=IC)                # scan
    mr(f, 29.55, 22.9, 40.7, 37.4, sw=1, fill=IC)              # scan
    for cy in (457, 525, 573, 621, 677, 726):
        rect(f, 551, cy - 17, 575, cy + 17, sw=0.9, fill=SMD)
    mr(f, 18.3, 7.2, 21.5, 11.5, sw=1, fill=IC)                # 8-pin chip by HDD_EN
    mr(f, 16.9, 14.9, 19.9, 17.9, sw=1, fill=IC)               # small chips beside the big inductor
    mr(f, 17.4, 18.7, 20.3, 20.4, sw=1, fill=IC)
    mr(f, 20.5, 44.5, 25.1, 48.8, sw=1, fill=IC)               # chip under the long ZIF
    mr(f, 8.5, 47.9, 14.2, 52.4, sw=1, fill=IC)                # chip left of the small ZIF
    rect(f, 1803, 3061, 1933, 3221, conv=P, sw=1, fill=IC)     # chip by P_VIO
    # more small parts read off the overlay of the rectified photo (resistor arrays below the long ZIF)
    for x0, y0, x1, y1 in ((26.8, 37.7, 28.6, 39.5), (26.8, 39.6, 28.6, 41.0), (26.8, 41.1, 28.6, 42.7),
                           (31.1, 38.9, 32.6, 40.1), (33.0, 38.9, 36.2, 40.1), (29.3, 39.1, 31.0, 40.9),
                           (30.8, 40.9, 32.4, 42.6), (34.2, 45.0, 37.0, 47.0), (25.9, 45.3, 27.9, 47.0),
                           (25.2, 47.7, 27.2, 48.6)):
        mr(f, x0, y0, x1, y1, sw=0.8, fill=SMD)
    f.circle(36.6, 30.0, 4.2, sw=0.8, fill="#f1e6e6")            # the round sticker on the chip
    # gold tabs along the top edge and bottom corners, the card-socket housings at the right edge
    mr(f, 19.9, 2.9, 24.2, 5.2, sw=0.9, fill="#cfd3d8")
    for x0, y0, x1, y1 in ((25.1, 3.5, 29.0, 6.7), (30.3, 3.7, 34.2, 6.3), (35.6, 3.8, 39.7, 6.5), (40.8, 4.0, W_MM, 7.0),
                           (7.1, 56.3, 11.3, 59.0), (30.5, 56.4, 34.7, 59.1)):
        mr(f, x0, y0, x1, y1, sw=0.9, fill=GOLD)
    mr(f, 41.1, 7.0, W_MM, 24.0, sw=1, fill="#b4b4b4")
    mr(f, 41.1, 37.4, W_MM, 53.5, sw=1, fill="#b4b4b4")
    # display module and the metal bracket beside it (scan)
    mr(f, 3.0, 19.1, 5.5, 47.9, sw=1, fill="#e6e6e6", r=1.1)
    mr(f, 5.6, 19.7, 16.1, 47.6, sw=1.4, fill="#1a1a1a", r=0.3)
    # the orange flex into the ZIF, the darker end is the stiffener (scan)
    mr(f, 16.15, 20.85, 19.5, 42.0, sw=1, fill=ORANGE)
    mr(f, 19.5, 21.25, 22.5, 42.15, sw=1, fill=BROWN)
    ytop = M(546, 498)[1]
    for i in range(40):
        f.line(16.15, ytop + 0.5 * i, 19.5, ytop + 0.5 * i, sw=0.5, colour="#c9923f")
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
    # test points, each with the name printed beside it
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
    # more pads read off the photo with P: TP4, TP19 and TP5 under the sticker chip, one unnamed beside TP4,
    # and P_VIO / P_VDIG at the lower right
    for (x, y), nm, tx, ty, rot in (((1576.7, 2335.5), "TP4", 1545, 2330, -90),
                                    ((1675.2, 2343.2), "TP19", 1675, 2382, 0),
                                    ((1762.9, 2339.4), "TP5", 1763, 2378, 0),
                                    ((2091, 3241), "P_VIO", 2050, 3250, -90),
                                    ((2168, 3184), "P_VDIG", 2171, 3280, 90)):
        disc(f, x, y, 0.45, conv=P, sw=1, fill=PAD)
        label(f, tx, ty, nm, rot, 8, conv=P)
    disc(f, 1603.6, 2365.5, 0.45, conv=P, sw=1, fill=PAD)
    # the two screw bosses
    for x, y in ((543, 385), (672, 934)):
        disc(f, x, y, 2.2, sw=1.2, fill=PAD)
        disc(f, x, y, 1.7, sw=1, fill="#fff")
    # the small ZIF with the 05B flex (probably keypad / touch, not the display); its tail goes through the board notch
    mr(f, 14.9, 55.7, 28.4, 56.3, sw=0.8, fill="#bdb7ac")
    mr(f, 20.3, 59.0, 22.6, 61.6, sw=1, fill="#d9a566")
    fx0, fy0, fx1, fy1 = box(P, 1045, 3110, 1395, 3312)
    rr = 0.9
    f.path(f"M{fx0:.2f} {fy0:.2f} L{fx1:.2f} {fy0:.2f} L{fx1:.2f} {fy1 - rr:.2f} Q{fx1:.2f} {fy1:.2f} {fx1 - rr:.2f} {fy1:.2f} L{fx0 + rr:.2f} {fy1:.2f} Q{fx0:.2f} {fy1:.2f} {fx0:.2f} {fy1 - rr:.2f} Z", sw=1, fill=ORANGE)
    rect(f, 361, 1012, 567, 1032, sw=1, fill="#8a7a6a")
    bar = box(M, 371, 964, 562, 1012)
    for i in range(15):
        x = M(255 / 4 + 350 + 30.5 * i / 4, 958)[0]
        f.rect(x - 0.12, bar[1] - 1.0, 0.24, 1.2, sw=0.6, fill="#d8d8d8")
    f.rect(bar[0], bar[1], bar[2] - bar[0], bar[3] - bar[1], sw=1.4, fill=CREAM)
    for x0, x1 in ((15.5, 17.4), (26.1, 27.9)):                 # the hinge ends of the latch
        mr(f, x0, 52.3, x1, 54.0, sw=0.7, fill="#e6dfd2")
    label(f, 1345, 3230, "05B", -90, 10, "#5a2a10", conv=P)
    # caption and dimension
    f.text(0.0, H_MM + 2.6, "Component side, main board (grey: neighbouring parts)", size=11, anchor="start")
    a = M(577, 498); b = (a[0], M(577, 810)[1])
    f.dim(a[0], a[1], b[0], b[1], "19.5 mm", offset=16)
    # callouts
    f.callout(1, *M(290, 560), "Display module", "black-covered block, glass under it", side="l")
    f.callout(2, *M(402, 640), "Display flex", "orange; the brown end is its stiffener", side="l")
    f.callout(3, 24.7, 20.95, "Long ZIF", "40 contacts counted, 0.5 mm assumed", side="r")
    f.callout(4, *M(495, 760), "ZIF latch", "white bar: release it before the flex", side="l")
    f.callout(5, *M(504, 440), "LIGHT-", "backlight pad", side="l")
    f.callout(6, *M(530, 857), "LIGHT+", "backlight pad, by the pin 1 end", side="l")
    f.callout(7, *M(495, 331), "Test points", "round pads, each named beside it", side="l")
    f.callout(8, *M(470, 988), "Small ZIF", "flex 05B: probably keypad, not display", side="l")
    f.callout(9, *M(672, 934), "Screw boss", "two in view, copper ring", side="r")
    f.note("Display controller: Himax HX8347-A, 240 x 320.")
    f.note("Backlight: from LIGHT+ / LIGHT-.")
    return f
