"""The two storage parts side by side, top view, one scale: the original Samsung drive in its rubber
holder, and the Sintech microSD adapter that replaces it.

Traced from the owner's photo of the opened unit (both parts lying in it, rectified so the adapter
board is square-on). Scale from parts of known size: the two microSD socket shells, taken as 15 mm
square (12.9 px/mm in the rectified photo). Three things agree with that scale: the 40 copper
fingers of the adapter ribbon span 20.6 mm (40 x 0.5 mm pitch is 20 mm), the drive body comes out
70.5 mm long, and the board 44.8 x 23.4 mm.

Drive size: 54 x 71 x 8 mm. Sources: the Samsung HS122JB brochure datasheet (directmacro.com,
HS122JB-brochure-datasheet.pdf, quoted by a search result as 71 x 54 x 8 mm; the file itself returns
403 to a script) and Wikipedia's "List of disk drive form factors" (1.8-inch: 54 mm wide, 71 mm long
for the ZIF version, 8 mm high; the Micro SATA version is 78.5 mm). The photo is only a cross-check:
the body measures about 70.5 mm long.

Layout: the two parts are stacked (drive on top, adapter below) so no callout leader has to cross the
other part. Set PHOTOLAYOUT=1 for the side-by-side layout the tracing was done in; only that layout
carries the photo underlay for edge checks (the stacked one is the same shapes moved, not re-measured).

Adapter facts checked against the seller's product photos (Sintech listing, owner's Amazon order) and the
owner's photo: the board is black and rectangular, the ZIF connector runs along one long edge with the
ribbon leaving perpendicular, two plated mounting holes sit near the corners on that edge (about 4.8 mm
from the left end and 6.5 mm from the right, read off the photos), the cards slide in from the two short
ends (TF2 from the left, TF1 from the right with the ZIF edge up) through a dark opening in each socket,
there is a small extra hole at the TF2 end, and the silkscreen TF2/TF1 sits just above each socket.

What the photo does not show, so is drawn from the form factor or assumed (and labelled "about"):
- the drive's left edge and the rubber on that side (cut off by the frame): straight, rubber about
  2 mm thick like the top; the notch in the right edge is traced, the left edge is not;
- the drive's connector: 1.8-inch ZIF/CE drives have a 40-pin ZIF connector on one short end, but which
  end is not visible in the photo, so it is not drawn or called out;
- the card in slot 1: the photo has both sockets empty, so the card is placed schematically (15 x 11
  mm, centred in the shell, pushed in from the right-hand short end, sticking out about 0.4 mm).
Drive points were also de-skewed by 3 degrees (the rubber holder leans that much against the board).
No serial numbers or label text are drawn; the caption under the drive is our own wording.
"""
import os

from figlib import Fig, GREY

FILE = "storage-parts.svg"

# rectified photo for --trace checks; local only (work/ is gitignored), set PM_TRACE_DIR to move it
RECT = os.path.join(os.environ.get("PM_TRACE_DIR", os.path.join(os.path.dirname(__file__), "..", "..", "work", "trace")), "storage/rect.png")
PXMM = 12.9           # rectified px per mm
# PHOTOLAYOUT=1: side by side, as measured on the photo (for --trace checks). Default: stacked.
PHOTO = bool(os.environ.get("PHOTOLAYOUT"))
OY = 5.0 if PHOTO else 0.8   # drive top; in the photo layout there is room above for the ribbon

# NOFILL=1 draws every shape unfilled so the photo shows through (for checking a tracing only)
NF = bool(os.environ.get("NOFILL"))


def c(col):
    return "none" if NF else col


RUBBER = c("#4d4d4d")
METAL = c("#d2d2d2")
BOARD = c("#3d3d3d")
SHELL = c("#bdbdbd")
COPPER = c("#d9a066")
CARD = c("#9ec1e6")

BX0, BY0 = 2.2, 1.7 + OY      # drive body top-left (mm)
BW, BL = 54.0, 71.0           # drive body size from the form factor
RR = BX0 + BW + 5.5           # rubber right edge (the right band is thick in the photo)
AW, AH = 44.8, 23.4
if PHOTO:
    AX, AY = 70.4, 1.9 + OY   # adapter board top-left
else:
    AX, AY = BX0 + BW / 2 - AW / 2, OY + 96.0


def drive(f):
    xr = BX0 + BW                              # right edge of the body above the notch
    # rubber holder: outer outline, the thick band on the right with a chamfered top corner
    f.path(f"M5 {OY} L{RR - 6.5} {OY} L{RR} {OY + 6.5} L{RR} {OY + 33} Q{RR - 1.4} {OY + 37} {RR} {OY + 41} "
           f"L{RR} {OY + 68} Q{RR} {OY + 75.4} {RR - 6} {OY + 75.4} L5.5 {OY + 75.4} Q0 {OY + 75.4} 0 {OY + 70} "
           f"L0 {OY + 5} Q0 {OY} 5 {OY} Z", sw=1.6, fill=RUBBER)
    # body (the aluminium cover), right edge traced with its notch, left edge straight (not in the photo)
    pts = [(BX0 + 2.5, BY0), (xr - 2.7, BY0), (xr - 0.7, BY0 + 1.1), (xr, BY0 + 3.3), (xr, OY + 24.5),
           (xr - 0.5, OY + 26.1), (xr - 2.2, OY + 32.3), (xr - 3.5, OY + 37.8), (xr - 2.6, OY + 40.9), (xr - 0.5, OY + 43.7),
           (xr + 0.2, OY + 47.2), (xr + 0.2, OY + 67.5), (xr - 0.9, OY + 70.0), (xr - 3.6, OY + 72.4), (xr - 6.7, OY + 73.0),
           (BX0 + 2.5, OY + 73.0), (BX0, OY + 70.5), (BX0, BY0 + 2.5)]
    # (xr + 0.2 = the lower edge sits a hair beyond the upper one in the photo)
    f.poly(pts, sw=1.6, fill=METAL)
    # label area, inset from the body (no text from the label is copied)
    lab = [(BX0 + 2.4, BY0 + 3.0), (xr - 5.0, BY0 + 3.0), (xr - 2.9, BY0 + 4.2), (xr - 2.3, BY0 + 6.5), (xr - 2.3, OY + 25.2),
           (xr - 4.2, OY + 31.0), (xr - 5.6, OY + 36.6), (xr - 4.4, OY + 40.2), (xr - 2.4, OY + 43.0),
           (xr - 2.0, OY + 46.5), (xr - 2.0, OY + 66.2), (xr - 3.4, OY + 69.4), (xr - 6.8, OY + 70.4),
           (BX0 + 2.4, OY + 70.4)]
    f.poly(lab, sw=1, fill=c("#ffffff"))
    # screw head in the holder beside the notch
    f.circle(58.2, OY + 39.2, 1.0, sw=1, fill=c("#cfcfcf"))
    # model and capacity as printed on the label (reads bottom to top, like the photo)
    f.labels.append(lambda: f'<text x="{f._px(BX0 + 6.4):.1f}" y="{f._py(OY + 26):.1f}" font-size="12" fill="#1a1a1a" text-anchor="middle" '
                            f'transform="rotate(-90 {f._px(BX0 + 6.4):.1f} {f._py(OY + 26):.1f})">HS122JB, 120 GB</text>')
    f.text(BX0 + BW / 2, OY + 80.5, "Samsung HS122JB 1.8-inch 120 GB", size=11)
    f.text(BX0 + BW / 2, OY + 85.5, "about 54 x 71 mm, 8 mm thick, in its rubber holder", size=10.5)


def adapter(f):
    def X(x): return AX + x
    def Y(y): return AY + y
    # ribbon, leaving the top edge towards the unit's board (only the visible stub is drawn)
    f.rect(X(12.2), Y(-4.0), 20.6, 4.6, sw=1.1, fill=COPPER)
    for i in range(41):
        f.line(X(12.4 + i * 0.5), Y(-3.6), X(12.4 + i * 0.5), Y(-0.2), sw=0.6, colour="#9a6a38")
    f.rect(AX, AY, AW, AH, r=0.3, sw=1.6, fill=BOARD)
    for x in (4.8, 38.3):
        f.circle(X(x), Y(2.7), 1.55, sw=1, fill=c("#cfcfcf"))
        f.circle(X(x), Y(2.7), 0.75, sw=1, fill=BOARD)
    # 40-pin ZIF connector: white latch over the contact row, ribbon coming out of it
    f.rect(X(10.9), Y(0.3), 22.6, 2.7, r=0.3, sw=1.2, fill=c("#efefef"))
    f.rect(X(11.4), Y(3.0), 21.6, 1.6, sw=1, fill=c("#1a1a1a"))
    f.line(X(11.6), Y(3.8), X(32.8), Y(3.8), sw=1, colour="#ffffff", dash="0.25 0.25")
    # silkscreen
    f.text(X(0.4), Y(6.6), "TF2", size=10, colour="#ffffff", anchor="start", weight=700)
    f.text(X(AW - 0.2), Y(6.4), "TF1", size=10, colour="#ffffff", anchor="end", weight=700)
    f.circle(X(1.8), Y(2.7), 0.45, sw=0.8, fill=c("#cfcfcf"))   # small extra hole at the TF2 end
    # the two sockets: TF2 left, TF1 right (empty slot windows as in the photo)
    for sx, wins in ((0.5, ((7.4, 2.7), (7.4, 9.7))), (30.0, ((4.2, 3.7), (4.2, 11.1)))):
        f.rect(X(sx), Y(6.85), 14.8, 15.1, r=0.4, sw=1.4, fill=SHELL)
        for wx, wy in wins:
            f.rect(X(sx + wx), Y(6.85 + wy), 3.4, 0.9, r=0.4, sw=0.9, fill=c("#6b6b6b"))
    # card openings face the board's short edges: the card slides in from the end
    for ox in (0.5, 43.8):
        f.rect(X(ox), Y(8.2), 1.0, 12.4, sw=0.8, fill=c("#1a1a1a"))
    for sx in (15.45, 29.55):
        for k in range(7):
            f.rect(X(sx) - 0.2, Y(8.2 + k * 1.75), 0.45, 0.8, sw=0.5, fill=c("#e8e8e8"))
    # a microSD card in slot 1 (TF1): 15 x 11 mm, pushed in from the right-hand short end, chamfered end first
    cx, cy = 30.2, 6.85 + (15.1 - 11) / 2
    f.poly([(X(cx + 2.0), Y(cy)), (X(cx + 15), Y(cy)), (X(cx + 15), Y(cy + 11)), (X(cx), Y(cy + 11)), (X(cx), Y(cy + 2.0))], sw=1.2, fill=CARD)
    f.rect(X(30.0), Y(6.85), 14.8, 15.1, r=0.4, sw=1.4, fill="none")
    for wx, wy in ((4.2, 3.7), (4.2, 11.1)):
        f.rect(X(30.0 + wx), Y(6.85 + wy), 3.4, 0.9, r=0.4, sw=0.9, fill=c("#6b6b6b"))
    f.text(AX + AW / 2, AY + AH + 8.2, "Sintech dual microSD to 1.8-inch ZIF adapter", size=11)
    f.text(AX + AW / 2, AY + AH + 13.2, "about 45 x 23 mm, card in slot 1 (TF1)", size=10.5)


def build():
    f = Fig("The original drive and its microSD replacement", max(BX0 + BW + 5.5, AX + AW) + 0.8, AY + AH + 16.0, scale=6.0, legend_title="Storage parts")
    if PHOTO and os.path.exists(RECT):
        # photo px 0,0 sits at fig mm (X0, Y0): the drive's upper right body edge (photo px x 372) is mm 56.2 and the body top (y 870) is mm BY0
        f.set_underlay(RECT, BX0 + BW - 372 / PXMM, BY0 - 870 / PXMM, 2098 / PXMM, 2098 / PXMM)
    drive(f)
    adapter(f)
    f.callout(1, 20, OY + 50, "Drive", "Samsung Spinpoint HS122JB, 1.8-inch, 120 GB", side="l")
    f.callout(2, 1.1, OY + 55, "Rubber holder", "black shock mount round the drive", side="l")
    f.callout(3, AX + 0.8, AY + 22.7, "Adapter", "Sintech dual microSD to 1.8-inch ZIF", side="l")
    f.callout(4, AX + 41, AY + 14.4, "Slot 1 (TF1)", "the card goes here", side="r")
    f.callout(5, AX + 3.0, AY + 9.5, "Slot 2 (TF2)", "leave empty", side="l")
    f.callout(6, AX + 30, AY - 2.0, "Adapter ZIF connector", "its ribbon leaves the top edge", side="r")
    f.note("Top view, one scale. Sizes are about; the drive's")
    f.note("far (left) edge is not in the photo, drawn straight.")
    return f
