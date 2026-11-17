"""Line-art figures for the help articles (docs/kb/images): the Pacemaker's controls and its screens.

Our own drawings, in the style of a printed manual: black outline on white, magenta numbered callouts.
SVG, so they stay sharp in the app's Help and on GitHub. Run with any Python 3:
    python branding/kb_figures.py
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from figures import case_outline  # noqa: E402  the traced case outline (see its docstring)

OUT = os.path.join(os.path.dirname(__file__), "..", "docs", "kb", "images")
MAGENTA = "#e0109a"
INK = "#1a1a1a"
FONT = "Helvetica, Arial, sans-serif"


def svg(w, h, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" font-family="{FONT}">\n'
            f'<rect width="{w}" height="{h}" rx="10" fill="#fff"/>\n{body}</svg>\n')


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def line_style(w=1.6):
    return f'fill="none" stroke="{INK}" stroke-width="{w}" stroke-linejoin="round" stroke-linecap="round"'


def tri(x, y, s, direction):
    """A small solid triangle pointing left or right, centred on x, y."""
    d = 1 if direction == "r" else -1
    return f'<path d="M{x - d * s * 0.6:.1f} {y - s * 0.7:.1f} L{x + d * s * 0.6:.1f} {y:.1f} L{x - d * s * 0.6:.1f} {y + s * 0.7:.1f} Z" fill="{INK}"/>'


def round_button(x, y, glyph):
    out = [f'<circle cx="{x}" cy="{y}" r="7.5" {line_style(1.4)}/>']
    if glyph == "rew":
        out += [tri(x - 2.2, y, 3, "l"), tri(x + 2.2, y, 3, "l")]
    elif glyph == "fwd":
        out += [tri(x - 2.2, y, 3, "r"), tri(x + 2.2, y, 3, "r")]
    elif glyph == "play":
        out += [tri(x - 1.8, y, 3, "r"), f'<path d="M{x + 1.6} {y - 2.1} V{y + 2.1} M{x + 3.2} {y - 2.1} V{y + 2.1}" stroke="{INK}" stroke-width="1"/>']
    elif glyph == "cue":
        out += [tri(x - 1.4, y, 3, "r"), f'<path d="M{x + 2.6} {y - 2.2} V{y + 2.2}" stroke="{INK}" stroke-width="1.2"/>']
    return "\n".join(out)


def pill(x0, x1, y, h):
    return f'<rect x="{x0}" y="{y - h / 2}" width="{x1 - x0}" height="{h}" rx="{h / 2}" {line_style(1.4)}/>'


class Device:
    """Front view of the unit, top edge at `top`, centred on `cx`. Points other drawings can point at."""

    def __init__(self, cx, top):
        self.cx, self.top = cx, top
        self.cy = top + CASE_H / 2
        at = self.at
        self.p = {
            "display": at(0, DISP_V), "rew": at(-BTN_U, BTN_V), "fwd": at(BTN_U, BTN_V),
            "ch1": at(-(PILL_X0 + PILL_X1) / 2, PILL_V), "ch2": at((PILL_X0 + PILL_X1) / 2, PILL_V),
            "xfader": at(XF_U * 0.75, XF_V), "play": at(-BTN_U, PLAY_V),
            "cue": at(BTN_U, PLAY_V), "touchpad": at(0, -DISP_V), "pswitch": at(-CASE_W / 2, PSW_V),
        }

    def at(self, u, v):
        """The point u mm right and v mm down from the centre of the case, in px."""
        return (self.cx + u * CASE_K, self.cy + v * CASE_K)

    def draw(self):
        c, t = self.cx, self.top
        at = self.at
        o = []
        # the body: the traced case outline (outer edge, the lip inside it, the four strap slots)
        o.extend(case_body(c, self.cy))
        # display and touchpad: an outer ring and the screen / pad inside it
        for v in (DISP_V, -DISP_V):
            x, y = at(0, v)
            o.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{RING_R * CASE_K:.1f}" {line_style(1.6)}/>')
            o.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{PAD_R * CASE_K:.1f}" {line_style(1.1)}/>')
        # transport row, channel buttons, crossfader with its scale, play/cue
        for u, v, g in ((-BTN_U, BTN_V, "rew"), (BTN_U, BTN_V, "fwd"), (-BTN_U, PLAY_V, "play"), (BTN_U, PLAY_V, "cue")):
            x, y = at(u, v)
            o.append(round_button(round(x, 1), round(y, 1), g))
        for sx in (-1, 1):
            x0, x1 = sorted((at(sx * PILL_X0, 0)[0], at(sx * PILL_X1, 0)[0]))
            y = at(0, PILL_V)[1]
            o.append(pill(round(x0, 1), round(x1, 1), round(y, 1), PILL_H * CASE_K))
            xa, xb = sorted((at(sx * (PILL_X0 + 2.4), 0)[0], at(sx * (PILL_X1 - 2.4), 0)[0]))
            o.append(f'<path d="M{xa:.1f} {y:.1f} H{xb:.1f}" stroke="{INK}" stroke-width="1"/>')
        for i in range(15):
            x, y = at((i - 7) * DOT_PITCH, DOT_V)
            o.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="0.9" fill="{INK}"/>')
        x0, y = at(-XF_U, XF_V)
        o.append(pill(round(x0, 1), round(at(XF_U, 0)[0], 1), round(y, 1), XF_H * CASE_K))
        # P-switch: the bump on the left edge
        x, y = at(-CASE_W / 2, PSW_V)
        o.append(f'<rect x="{x - 2.6:.1f}" y="{y - PSW_LEN / 2 * CASE_K:.1f}" width="3.2" height="{PSW_LEN * CASE_K:.1f}" rx="1.2" fill="{INK}"/>')
        return "\n".join(o)


CASE_H = 322                       # drawn length of the case, px
CASE_K = CASE_H / case_outline.L   # px per mm of the traced outline
CASE_HW = round(case_outline.W / 2 * CASE_K)   # half the width of the case
CASE_W = case_outline.W            # width of the case, mm
# The layout of the front is taken from the maker's line drawing in the manual (vendor/docs, section 3.1 and
# 3.2), measured on it in pixels and scaled so its case matches the traced outline (the drawing's case is
# 296 px long and 126.5 px wide; ours is 159 x 71.5 mm). Positions are mm from the centre of the case: u to
# the right, v downwards (the display end is up). Its slots, 11.1 mm out and 73 mm from the middle, agree with
# the ones traced from the photos, which is the check that the scaling holds. Good to about 0.5 mm.
DISP_V = -39.6        # display centre (the touchpad is its mirror image)
RING_R, PAD_R = 28.8, 22.4   # outer ring and inner circle of the display and the touchpad
BTN_U, BTN_V, PLAY_V = 25.9, -13.7, 14.0   # Rewind / Forward row and Play / Cue row; button radius 4.3 mm
PILL_X0, PILL_X1, PILL_V, PILL_H = 7.6, 29.5, -4.3, 4.9   # the two channel buttons
XF_U, XF_V, XF_H = 29.5, 4.3, 4.8          # crossfader strip, half length and centre
DOT_V, DOT_PITCH = 0.3, 3.88               # its scale: 15 dots
PSW_V, PSW_LEN = -26.1, 17.0               # P-switch bump on the left edge: centre and length
# End views. Top (USB) end: the two switches sit 10.3 mm either side of the Mini-B axis on the USB board
# (photo, scale from the 2.0 mm header pitch); the manual's end view puts the caps 11.0 mm out, so 10.7 is
# used. The caps are 8.7 x 4.0 mm and sit in a recess 36 mm wide. Bottom (audio) end: the three sockets are
# not evenly spaced, 9.6 and 11.3 mm apart (manual 9.4 and 11.3, photo of the audio board 9.8 and 11.3), and
# the group sits 1.4 mm off the case axis towards the Power end.
SW_DX = 10.7
JACK_X = (-11.4, -1.8, 9.5)
RECESS_W = 36.4


def case_body(cx, cy):
    """The case outline centred on cx, cy: outer edge, the lip inside it and the strap slots, as svg."""
    base = case_outline.catmull(case_outline.outline_points())

    def poly(pts):
        return "M" + " L".join(f"{cx + x * CASE_K:.1f} {cy + y * CASE_K:.1f}" for x, y in pts) + " Z"
    o = [f'<path d="{poly(base)}" {line_style(1.8)}/>',
         f'<path d="{poly(case_outline.inset(base, case_outline.RIM))}" {line_style(1)}/>']
    for x, y, ln, w, ang in case_outline.slots():
        d = case_outline.slot_d(cx + x * CASE_K, cy + y * CASE_K, ln * CASE_K, w * CASE_K, ang)
        o.append(f'<path d="{d}" {line_style(1.2)}/>')
    return o


def end_view(cx, y, kind):
    """The top or bottom edge seen end-on: three buttons (top) or three sockets (bottom). Returns svg and points."""
    rw = RECESS_W * CASE_K
    o = [f'<rect x="{cx - CASE_HW}" y="{y - 13}" width="{2 * CASE_HW}" height="26" rx="13" {line_style(1.6)}/>',
         f'<rect x="{cx - rw / 2:.1f}" y="{y - 10}" width="{rw:.1f}" height="20" rx="10" {line_style(1)}/>']
    pts = {}
    if kind == "top":
        dx, cw, ch = SW_DX * CASE_K, 8.7 * CASE_K, 4.0 * CASE_K    # cap centre offset, cap size
        for sx in (-1, 1):
            o.append(f'<rect x="{cx + sx * dx - cw / 2:.1f}" y="{y - ch / 2:.1f}" width="{cw:.1f}" height="{ch:.1f}" rx="{ch / 2:.1f}" {line_style(1.3)}/>')
        o.append(f'<rect x="{cx - 7.7 / 2 * CASE_K:.1f}" y="{y - 3.9 / 2 * CASE_K:.1f}" width="{7.7 * CASE_K:.1f}" height="{3.9 * CASE_K:.1f}" rx="1.5" {line_style(1.3)}/>')
        pts = {"settings": (cx - dx, y), "usb": (cx, y), "killall": (cx + dx, y)}
    else:
        for k, u in zip(("power", "lineout", "phones"), JACK_X):
            x = cx + u * CASE_K
            o.append(f'<circle cx="{x:.1f}" cy="{y}" r="5" {line_style(1.3)}/><circle cx="{x:.1f}" cy="{y}" r="2" fill="{INK}"/>')
            pts[k] = (round(x, 1), y)
    return "\n".join(o), pts


def caption(x, y, text):
    return f'<text x="{x}" y="{y}" font-size="11" fill="#6b6b6b">{esc(text)}</text>'


def switch_arrow(cx, top, up):
    """A small triangle inside the left rim showing which way the P-switch is held."""
    x, y, d = cx - CASE_HW + 12, top + CASE_H / 2 + PSW_V * CASE_K, (-1 if up else 1)
    return f'<path d="M{x} {y + d * 6} L{x - 4.5} {y - d * 3} L{x + 4.5} {y - d * 3} Z" fill="{MAGENTA}"/>'


def callout(n, at, label_xy):
    (x, y), (lx, ly) = at, label_xy
    return (f'<line x1="{lx}" y1="{ly}" x2="{x}" y2="{y}" stroke="{MAGENTA}" stroke-width="1.2"/>'
            f'<circle cx="{x}" cy="{y}" r="1.8" fill="{MAGENTA}"/>'
            f'<circle cx="{lx}" cy="{ly}" r="10" fill="{MAGENTA}"/>'
            f'<text x="{lx}" y="{ly + 4}" text-anchor="middle" font-size="11" font-weight="700" fill="#fff">{n}</text>')


def legend(x, y, rows, title=None):
    o = []
    if title:
        o.append(f'<text x="{x}" y="{y}" font-size="13" font-weight="700" fill="{INK}">{esc(title)}</text>')
        y += 22
    for n, name, does in rows:
        # number and name in fixed columns, as in the shared figure layout, so the names line up
        o.append(f'<text x="{x}" y="{y}" font-size="12.5" fill="{INK}"><tspan font-weight="700" fill="{MAGENTA}">{n}</tspan>'
                 f'<tspan x="{x + 22}" font-weight="700">{esc(name)}</tspan>'
                 f'<tspan dx="6" fill="#6b6b6b">{esc(does)}</tspan></text>')
        y += 19
    return "\n".join(o)


def controls():
    """Every control and socket (the manual's basic functions), front view plus both end views."""
    cx = 170
    top_svg, top = end_view(cx, 46, "top")
    dev = Device(cx, 92)
    bot_svg, bot = end_view(cx, 462, "bottom")
    p = dev.p
    L, R = cx - 112, cx + 112
    # Bubbles stand in two columns. Each leader runs straight to its target without crossing another
    # leader or passing over another control: the crossfader's bubble sits between Channel 2 and Cue, and the bottom-end bubbles stand clear of
    # that view's caption.
    xf = (p["xfader"][0] + 0, p["xfader"][1])
    calls = [
        (1, p["display"], (L, 120)), (2, p["rew"], (L, 205)), (3, p["fwd"], (R, 200)), (4, p["ch1"], (L, 248)),
        (5, p["ch2"], (R, 232)), (6, p["play"], (L, 298)), (7, p["cue"], (R, 298)), (8, xf, (R, xf[1])),
        (9, (cx - 20, p["touchpad"][1]), (L, 352)), (10, p["touchpad"], (R, 372)),
        (11, top["settings"], (cx - 40, 14)), (12, top["usb"], (cx, 14)), (13, top["killall"], (cx + 40, 14)),
        (14, bot["power"], (cx + 30, 512)), (15, bot["lineout"], (cx + 70, 512)), (16, bot["phones"], (cx + 110, 512)),
    ]
    rows = [(1, "Display", "time, meters, BPM, track"), (2, "Rewind", "search back, previous track"), (3, "Forward", "search on, next track"),
            (4, "Channel 1", "make active"), (5, "Channel 2", "make active"), (6, "Play", "play / pause"),
            (7, "Cue", "set cue, launch from it"), (8, "Crossfader", "touch strip, blends ch1 / ch2"), (9, "Menu", "library in / out"),
            (10, "Touchpad", "middle: tap, swipe; rim: stroke"), (11, "Settings", "settings menu, power on"), (12, "USB", "computer, charging"),
            (13, "Kill-all", "effects off, press again to restore"), (14, "Power", "adapter jack"), (15, "Line out", "to a mixer or speakers"),
            (16, "Phones", "headphones")]
    body = "\n".join([top_svg, dev.draw(), bot_svg, caption(cx - CASE_HW, 74, "Top end"), caption(cx - CASE_HW, 432, "Front"),
                      caption(cx - CASE_HW, 490, "Bottom end")]
                     + [callout(*c) for c in calls] + [legend(330, 70, rows, "Controls")])
    return svg(640, 538, body)


def pswitch(up):
    """The P-switch layers: held up (loops, reverse, view time) or held down (monitoring)."""
    cx = 170
    dev = Device(cx, 30)
    p = dev.p
    L, R = cx - 112, cx + 112
    if up:
        calls = [(1, p["pswitch"], (L, 120)), (2, p["rew"], (L, 172)), (3, p["fwd"], (R, 172)), (4, p["ch1"], (L, 215)),
                 (5, p["ch2"], (R, 215)), (6, p["play"], (L, 258)), (7, p["cue"], (R, 258)), (8, p["touchpad"], (R, 300))]
        rows = [(1, "P-switch up", "hold it up"), (2, "Rewind", "loop in point; with Forward, split / double"),
                (3, "Forward", "loop end point; with Rewind, split / double"),
                (4, "Channel 1", "time played / time left"), (5, "Channel 2", "time played / time left"), (6, "Play", "reverse play"),
                (7, "Cue", "exit / re-enter the loop"), (8, "Touchpad", "tempo, effects, filters")]
        title = "P-switch held up"
    else:
        calls = [(1, p["pswitch"], (L, 120)), (2, p["play"], (L, 258)), (3, p["xfader"], (R, p["xfader"][1])),
                 (4, p["touchpad"], (R, 300))]
        rows = [(1, "P-switch down", "hold it down"), (2, "Play", "sync BPM and phase to the other channel"),
                (3, "Crossfader", "headphone blend ch1 / ch2"), (4, "Touchpad", "rim stroke: headphone volume")]
        title = "P-switch held down"
    body = "\n".join([dev.draw(), switch_arrow(cx, 30, up), caption(cx - CASE_HW, 366, "Front")]
                     + [callout(*c) for c in calls] + [legend(330, 60, rows, title)])
    return svg(700, 384, body)


def play_screen():
    """The play screen as Revival firmware draws it, with the parts numbered (layout after the manual's display figure)."""
    cx, cy, r = 190, 200, 128
    o = [f'<defs><clipPath id="lcd"><circle cx="{cx}" cy="{cy}" r="{r - 6}"/></clipPath></defs>',
         f'<circle cx="{cx}" cy="{cy}" r="{r}" {line_style(1.8)}/>',
         f'<g clip-path="url(#lcd)"><rect x="{cx - r}" y="{cy - r}" width="{2 * r}" height="{2 * r}" fill="#000"/>']
    white, grey = "#f2f2f2", "#8a8a8a"
    # top row: time played, playback symbol, time left
    o.append(f'<text x="{cx - 34}" y="{cy - 82}" text-anchor="end" font-size="13" fill="{white}">-01:43</text>')
    o.append(f'<path d="M{cx - 5} {cy - 92} L{cx + 6} {cy - 86} L{cx - 5} {cy - 80} Z" fill="{white}"/>')
    o.append(f'<text x="{cx + 34}" y="{cy - 82}" font-size="13" fill="{grey}">02:31</text>')
    o.append(f'<line x1="{cx - 66}" y1="{cy - 74}" x2="{cx + 66}" y2="{cy - 74}" stroke="{grey}" stroke-width="1"/>')
    # engaged effects: one ring per effect family
    for i, col in enumerate(("#ff40ff", "#f2f2f2", "#40ff60")):
        o.append(f'<circle cx="{cx - 14 + i * 14}" cy="{cy - 61}" r="4.5" fill="none" stroke="{col}" stroke-width="2"/>')
    # VU meters, Revival colours: purple at the bottom to magenta to red at the top
    lit = ["#e070ff", "#f060ff", "#ff50ff", "#ff50e8", "#ff50d0", "#ff48a8", "#ff3030"]
    for side, xx, level in ((-1, cx - 96, 13), (1, cx + 82, 0)):
        for s in range(24):
            y = cy + 42 - s * 4.6
            row = min(6, s * 7 // 24)
            col = lit[row] if s < level else "#3a2a3e"
            o.append(f'<rect x="{xx}" y="{y:.1f}" width="14" height="3" fill="{col}"/>')
    # track text
    o.append(f'<text x="{cx}" y="{cy - 22}" text-anchor="middle" font-size="12.5" fill="{white}">Artist Name</text>')
    o.append(f'<text x="{cx}" y="{cy - 5}" text-anchor="middle" font-size="13.5" font-weight="700" fill="{white}">Track Title</text>')
    o.append(f'<text x="{cx}" y="{cy + 12}" text-anchor="middle" font-size="12.5" fill="{white}">(Album)</text>')
    # tempo change
    o.append(f'<text x="{cx}" y="{cy + 40}" text-anchor="middle" font-size="12.5" fill="{white}">+0.83%</text>')
    # beat graph: both channels' beats on one scale
    o.append(f'<line x1="{cx - 66}" y1="{cy + 50}" x2="{cx + 66}" y2="{cy + 50}" stroke="{grey}" stroke-width="1"/>')
    o.append(f'<rect x="{cx - 66}" y="{cy + 54}" width="132" height="14" fill="none" stroke="{grey}" stroke-width="1"/>')
    for i in range(5):
        x = cx - 52 + i * 26
        o.append(f'<line x1="{x}" y1="{cy + 55}" x2="{x}" y2="{cy + 61}" stroke="{white}" stroke-width="1.5"/>')
        o.append(f'<line x1="{x + 4}" y1="{cy + 61}" x2="{x + 4}" y2="{cy + 67}" stroke="{grey}" stroke-width="1.5"/>')
    # BPMs and the power status
    o.append(f'<text x="{cx - 14}" y="{cy + 87}" text-anchor="end" font-size="13" fill="{white}">127.1</text>')
    o.append(f'<circle cx="{cx}" cy="{cy + 83}" r="5" fill="none" stroke="{white}" stroke-width="1.4"/>'
             f'<path d="M{cx} {cy + 78} A5 5 0 0 1 {cx} {cy + 88} Z" fill="{white}"/>')
    o.append(f'<text x="{cx + 14}" y="{cy + 87}" font-size="13" fill="{grey}">127</text>')
    o.append("</g>")
    L, R = cx - 160, cx + 160
    calls = [(1, (cx - 60, cy - 86), (L, cy - 100)), (2, (cx - 90, cy - 20), (L, cy - 30)), (3, (cx - 20, cy + 36), (L, cy + 40)),
             (4, (cx - 30, cy + 83), (L, cy + 100)), (5, (cx, cy - 86), (R, cy - 100)), (6, (cx + 14, cy - 61), (R, cy - 62)),
             (7, (cx + 34, cy - 6), (R, cy - 10)), (8, (cx + 40, cy + 61), (R, cy + 62)), (9, (cx, cy + 83), (cx, cy + 156))]
    rows = [(1, "Time", "played and left"), (2, "Level meters", "ch1 / ch2"), (3, "Tempo change", ""), (4, "BPM", "this / other channel"),
            (5, "Playback", "play, pause, reverse, loop"), (6, "Effects on", ""), (7, "Track", "artist, title, album"),
            (8, "Beat graph", "ch1 and ch2"), (9, "Battery", "")]
    body = "\n".join(o + [callout(*c) for c in calls] + [legend(390, 90, rows, "The play screen")])
    return svg(620, 380, body)


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, make in (("pm-controls.svg", controls), ("pm-pswitch-up.svg", lambda: pswitch(True)),
                       ("pm-pswitch-down.svg", lambda: pswitch(False)), ("pm-play-screen.svg", play_screen)):
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(make())
        print("wrote", name)


if __name__ == "__main__":
    main()
