"""Line-art figures for the help articles (docs/kb/images): the Pacemaker's controls and its screens.

Our own drawings, in the style of a printed manual: black outline on white, magenta numbered callouts.
SVG, so they stay sharp in the app's Help and on GitHub. Run with any Python 3:
    python branding/kb_figures.py
"""
import math
import os

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
        c, t = cx, top
        self.p = {
            "display": (c, t + 72), "rew": (c - 44, t + 140), "fwd": (c + 44, t + 140),
            "ch1": (c - 26, t + 155), "ch2": (c + 26, t + 155), "xfader": (c + 30, t + 175),
            "play": (c - 44, t + 196), "cue": (c + 44, t + 196), "touchpad": (c + 18, t + 250),
            "pswitch": (c - 64, t + 112),
        }

    def draw(self):
        c, t = self.cx, self.top
        o = []
        # the body: a long rounded slab drawn with a double line, like the case and its rim
        o.append(f'<rect x="{c - 64}" y="{t}" width="128" height="322" rx="56" {line_style(1.8)}/>')
        o.append(f'<rect x="{c - 59}" y="{t + 5}" width="118" height="312" rx="51" {line_style(1)}/>')
        # strap slots at both ends
        for y in (t + 13, t + 309):
            for dx in (-15, 15):
                o.append(f'<rect x="{c + dx - 8}" y="{y - 3}" width="16" height="6" rx="3" {line_style(1.2)}/>')
        # display
        o.append(f'<circle cx="{c}" cy="{t + 72}" r="50" {line_style(1.6)}/>')
        o.append(f'<circle cx="{c}" cy="{t + 72}" r="41" {line_style(1.1)}/>')
        # transport row, channel buttons, crossfader with its scale, play/cue
        o.append(round_button(c - 44, t + 140, "rew"))
        o.append(round_button(c + 44, t + 140, "fwd"))
        o.append(pill(c - 46, c - 6, t + 155, 8))
        o.append(pill(c + 6, c + 46, t + 155, 8))
        o.append(f'<path d="M{c - 26} {t + 155} H{c - 14} M{c + 14} {t + 155} H{c + 26}" stroke="{INK}" stroke-width="1"/>')
        for i in range(17):
            x = c - 48 + i * 6
            o.append(f'<circle cx="{x}" cy="{t + 166}" r="0.9" fill="{INK}"/>')
        o.append(pill(c - 52, c + 52, t + 175, 9))
        o.append(round_button(c - 44, t + 196, "play"))
        o.append(round_button(c + 44, t + 196, "cue"))
        # touchpad
        o.append(f'<circle cx="{c}" cy="{t + 252}" r="50" {line_style(1.6)}/>')
        o.append(f'<circle cx="{c}" cy="{t + 252}" r="41" {line_style(1.1)}/>')
        # P-switch on the left edge
        o.append(f'<rect x="{c - 67}" y="{t + 101}" width="4" height="22" rx="1.5" fill="{INK}"/>')
        return "\n".join(o)


def end_view(cx, y, kind):
    """The top or bottom edge seen end-on: three buttons (top) or three sockets (bottom). Returns svg and points."""
    o = [f'<rect x="{cx - 64}" y="{y - 13}" width="128" height="26" rx="13" {line_style(1.6)}/>',
         f'<rect x="{cx - 52}" y="{y - 9}" width="104" height="18" rx="9" {line_style(1)}/>']
    pts = {}
    if kind == "top":
        o.append(f'<rect x="{cx - 34}" y="{y - 4}" width="18" height="8" rx="4" {line_style(1.3)}/>')
        o.append(f'<rect x="{cx - 8}" y="{y - 3.5}" width="16" height="7" rx="1.5" {line_style(1.3)}/>')
        o.append(f'<rect x="{cx + 16}" y="{y - 4}" width="18" height="8" rx="4" {line_style(1.3)}/>')
        pts = {"settings": (cx - 25, y), "usb": (cx, y), "killall": (cx + 25, y)}
    else:
        for i, k in enumerate(("power", "lineout", "phones")):
            x = cx - 20 + i * 20
            o.append(f'<circle cx="{x}" cy="{y}" r="5" {line_style(1.3)}/><circle cx="{x}" cy="{y}" r="2" fill="{INK}"/>')
            pts[k] = (x, y)
    return "\n".join(o), pts


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
        o.append(f'<text x="{x}" y="{y}" font-size="12.5" fill="{INK}"><tspan font-weight="700" fill="{MAGENTA}">{n}</tspan>'
                 f'<tspan dx="8" font-weight="700">{esc(name)}</tspan><tspan dx="5">{esc(does)}</tspan></text>')
        y += 19
    return "\n".join(o)


def controls():
    """Every control and socket (the manual's basic functions), front view plus both end views."""
    cx = 170
    top_svg, top = end_view(cx, 46, "top")
    dev = Device(cx, 92)
    bot_svg, bot = end_view(cx, 452, "bottom")
    p = dev.p
    L, R = cx - 112, cx + 112
    calls = [
        (1, p["display"], (L, 120)), (2, p["rew"], (L, 205)), (3, p["fwd"], (R, 205)), (4, p["ch1"], (L, 248)),
        (5, p["ch2"], (R, 248)), (6, p["play"], (L, 290)), (7, p["cue"], (R, 290)), (8, p["xfader"], (R, 330)),
        (9, (cx - 20, 352), (L, 352)), (10, p["touchpad"], (R, 372)),
        (11, top["settings"], (cx - 40, 14)), (12, top["usb"], (cx, 14)), (13, top["killall"], (cx + 40, 14)),
        (14, bot["power"], (cx - 40, 486)), (15, bot["lineout"], (cx, 486)), (16, bot["phones"], (cx + 40, 486)),
    ]
    rows = [(1, "Display", ""), (2, "Rewind", "search back, skip"), (3, "Forward", "search on, skip"),
            (4, "Channel 1", "select"), (5, "Channel 2", "select"), (6, "Play", "play / pause"),
            (7, "Cue", "set or go to cue"), (8, "Crossfader", "blend ch1 / ch2"), (9, "Menu", "library in / out"),
            (10, "Touchpad", "navigate, adjust"), (11, "Settings", "on, menu"), (12, "USB", "computer, charge"),
            (13, "Kill-all", "effects off / back"), (14, "Power", "adapter"), (15, "Line out", "to the mixer"),
            (16, "Phones", "headphones")]
    body = "\n".join([top_svg, dev.draw(), bot_svg] + [callout(*c) for c in calls] + [legend(330, 70, rows, "Controls")])
    return svg(560, 500, body)


def pswitch(up):
    """The P-switch layers: held up (loops, reverse, view time) or held down (monitoring)."""
    cx = 170
    dev = Device(cx, 30)
    p = dev.p
    L, R = cx - 112, cx + 112
    if up:
        calls = [(1, p["pswitch"], (L, 120)), (2, p["rew"], (L, 172)), (3, p["fwd"], (R, 172)), (4, p["ch1"], (L, 215)),
                 (5, p["ch2"], (R, 215)), (6, p["play"], (L, 258)), (7, p["cue"], (R, 258)), (8, p["touchpad"], (R, 300))]
        rows = [(1, "P-switch up", "hold"), (2, "Rewind", "loop in-point, split / double"), (3, "Forward", "loop end-point, split / double"),
                (4, "Channel 1", "time played / left"), (5, "Channel 2", "time played / left"), (6, "Play", "reverse play"),
                (7, "Cue", "exit / re-enter loop"), (8, "Touchpad", "effect settings")]
        title = "P-switch held up"
    else:
        calls = [(1, p["pswitch"], (L, 120)), (2, p["play"], (L, 258)), (3, p["xfader"], (R, 215)), (4, p["touchpad"], (R, 300))]
        rows = [(1, "P-switch down", "hold"), (2, "Play", "sync BPM and phase to the other channel"),
                (3, "Crossfader", "headphone blend ch1 / ch2"), (4, "Touchpad", "headphone volume")]
        title = "P-switch held down"
    body = "\n".join([dev.draw()] + [callout(*c) for c in calls] + [legend(330, 60, rows, title)])
    return svg(600, 380, body)


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
