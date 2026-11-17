"""The USB board in the back cover, component side above, back below.

Traced from the owner's photos (component side straight on, back with a ruler). Scale from parts of
known size: the 2 x 5 header has a 2.0 mm pitch and the Mini-B shell is 7.7 mm wide, which agree
(38 px/mm in the component photo) and give a board about 38 mm long. Outline made symmetric about
the socket; the photos' small asymmetries are camera angle.
"""
from figlib import Fig, FILL, GREY

FILE = "usb-board.svg"

L = 38.0          # board length
H = 13.3          # straight edge to the top of the socket tab
SH = 3.8          # tab height above the shoulders
END = 4.5         # straight part of each end
CH = 6.5          # chamfer run along the length
TAB = 13.0        # tab width
C = L / 2


def outline(f, ox, oy):
    t0, t1 = C - TAB / 2, C + TAB / 2
    pts = [(0, H), (0, H - END), (CH, SH), (t0, SH), (t0, 0.6), (t0 + 0.6, 0), (t1 - 0.6, 0), (t1, 0.6), (t1, SH), (L - CH, SH), (L, H - END), (L, H)]
    f.poly([(ox + x, oy + y) for x, y in pts], sw=1.6, fill=FILL)


def holes(f, ox, oy):
    for x in (2.7, L - 2.7):
        f.circle(ox + x, oy + H - 3.0, 2.2, sw=1)
        f.circle(ox + x, oy + H - 3.0, 1.45, sw=1.4, fill="#fff")


def build():
    f = Fig("USB board (in the back cover)", L, 2 * H + 2.7 * 2 + 16, scale=11, legend_title="USB board")
    # ---- component side, case end up ----
    ox, oy = 0, 2.7
    outline(f, ox, oy)
    holes(f, ox, oy)
    # Mini-B socket shell, through the tab to the case end, with its five pins and four shell tabs
    f.rect(ox + C - 3.85, oy - 2.7, 7.7, 2.7 + 6.5, r=0.5, sw=1.6, fill="#fff")
    f.rect(ox + C - 2.2, oy - 1.9, 4.4, 1.2, r=0.4, sw=1)
    for i in range(5):
        f.rect(ox + C - 1.8 + i * 0.8, oy + 6.6, 0.4, 1.0, sw=0.8, fill="#1a1a1a")
    for dx in (-4.9, 4.1):
        for y in (0.3, 4.6):
            f.rect(ox + C + dx, oy + y, 0.8, 1.4, sw=0.8)
    # the two switches, plunger towards the case end, three pads each towards the straight edge
    for x in (8.7, L - 8.7):
        f.rect(ox + x - 2.1, oy + 4.0, 4.2, 2.7, r=0.3, sw=1.3, fill="#fff")
        f.rect(ox + x - 0.7, oy + 3.2, 1.4, 0.8, r=0.3, sw=1)
        for k in (-1, 0, 1):
            f.rect(ox + x + k * 1.3 - 0.35, oy + 6.95, 0.7, 0.7, sw=0.8)
    # 2 x 5 header, 2.0 mm pitch: top row 10..6, bottom row 1..5 (as silkscreened)
    f.rect(ox + C - 5, oy + H - 4.8, 10, 4.5, sw=1.2)
    for i in range(5):
        for y in (H - 3.8, H - 1.8):
            f.rect(ox + C - 4 + i * 2 - 0.45, oy + y - 0.45, 0.9, 0.9, sw=0.8, fill="#1a1a1a")
    f.text(ox + C - 6.0, oy + H - 3.3, "10", size=10)
    f.text(ox + C - 6.0, oy + H - 1.2, "1", size=10)
    f.text(ox + C + 6.0, oy + H - 3.3, "6", size=10)
    f.text(ox + C + 6.0, oy + H - 1.2, "5", size=10)
    f.text(ox + 4.0, oy + H + 4.2, "Component side (case end up)", size=11, anchor="start")
    f.dim(ox, oy + H + 1.6, ox + L, oy + H + 1.6, "about 38 mm", offset=14)

    # ---- back, the same edge up (seen from behind, so left and right swap) ----
    oy2 = oy + H + 10 + 2.7
    outline(f, ox, oy2)
    holes(f, ox, oy2)
    f.rect(ox + C - 3.85, oy2 - 2.7, 7.7, 2.7 + 6.5, r=0.5, sw=1, fill="none", dash="1 0.8")
    for x in (8.7, L - 8.7):
        f.rect(ox + x - 2.1, oy2 + 4.0, 4.2, 2.7, r=0.3, sw=0.9, dash="1 0.8")
        f.rect(ox + x - 2.4, oy2 + H - 4.0, 4.8, 3.1, r=0.4, sw=1.4, fill="#e8d9a8")
    for i in range(5):
        for y in (H - 3.8, H - 1.8):
            x = ox + C + 4 - i * 2
            if i == 0 and y == H - 1.8:
                f.rect(x - 0.6, oy2 + y - 0.6, 1.2, 1.2, sw=1.1)
            else:
                f.circle(x, oy2 + y, 0.65, sw=1.1)
    for x, y, w, h in ((C - 5.2, 4.6, 2.2, 1.0), (C - 1.0, 3.0, 1.4, 1.6), (C + 2.8, 4.6, 2.2, 1.0), (C - 0.6, 5.6, 1.8, 1.2)):
        f.rect(ox + x, oy2 + y, w, h, sw=0.9, fill="#fff")
    f.text(ox + 4.0, oy2 + H + 4.2, "Back (dashed: parts on the other side)", size=11, anchor="start")

    f.callout(1, ox + C + 3.85, oy - 1.5, "Mini-B socket", "faces out through the case end", side="r")
    f.callout(2, ox + L - 8.7 + 2.1, oy + 5.3, "Switch", "one each side: Settings, Kill-all", side="r")
    f.callout(3, ox + C - 5, oy + H - 3.0, "2 x 5 header", "2.0 mm pitch, pins 1, 5, 6, 10 marked", side="l")
    f.callout(4, ox + 2.7 - 1.45, oy + H - 3.0, "Screw hole", "two, into the back cover", side="l")
    f.callout(5, ox + 8.7 - 2.4, oy2 + H - 2.4, "Spring contact", "two, on the back", side="l")
    f.callout(6, ox + C + 5.0, oy2 + 5.1, "Small parts", "", side="r")
    f.callout(7, ox + C + 4.6, oy2 + H - 1.8, "Header joints", "square pad = pin 1", side="r")
    return f
