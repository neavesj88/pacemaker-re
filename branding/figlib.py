"""The layout every help figure shares: a drawing in millimetres, numbered callouts in two tidy
columns beside it, and a legend on the right. Figures are built by the modules in branding/figures/.

Coordinates are millimetres, origin top-left of the drawing, y down. Fig.svg() turns them into
pixels at `scale` px per mm. With an underlay (a rectified photo, see trace.py) the photo is drawn
under the shapes at reduced opacity so a tracing can be checked edge by edge; the published figure
never contains the photo.
"""
import base64
import os

MAGENTA = "#e0109a"
INK = "#1a1a1a"
GREY = "#6b6b6b"
FILL = "#f2f2f2"
FONT = "Helvetica, Arial, sans-serif"

BUBBLE_R = 10
ROW = 24          # minimum distance between two callout bubbles, px
GUTTER = 64       # space between the drawing and a callout column, px
LEGEND_W = 290    # legend column width, px


def esc(s):
    return str(s).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


class Fig:
    def __init__(self, title, w_mm, h_mm, scale=8.0, legend_title=None):
        self.title, self.w, self.h, self.s = title, w_mm, h_mm, scale
        self.legend_title = legend_title or title
        self.items = []       # svg snippets in mm (inside a scaled group)
        self.labels = []      # svg snippets in px (text stays a fixed size)
        self.calls = []       # (n, x_mm, y_mm, name, note, side)
        self.notes = []       # lines under the legend
        self.underlay = None  # (png path, x_mm, y_mm, w_mm, h_mm)

    # ---- shapes (all in mm) -------------------------------------------------------------------
    def _st(self, w, fill, dash):
        d = f' stroke-dasharray="{dash}"' if dash else ""
        return f'fill="{fill}" stroke="{INK}" stroke-width="{w / self.s:.3f}" stroke-linejoin="round" stroke-linecap="round"{d}'

    def rect(self, x, y, w, h, r=0, sw=1.5, fill="none", dash=None):
        self.items.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" {self._st(sw, fill, dash)}/>')

    def circle(self, x, y, r, sw=1.5, fill="none", dash=None):
        self.items.append(f'<circle cx="{x}" cy="{y}" r="{r}" {self._st(sw, fill, dash)}/>')

    def path(self, d, sw=1.5, fill="none", dash=None):
        """An SVG path in mm, e.g. 'M0 0 L10 0 Q12 0 12 2 Z'."""
        self.items.append(f'<path d="{d}" {self._st(sw, fill, dash)}/>')

    def poly(self, pts, sw=1.5, fill="none", closed=True, dash=None):
        d = "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + (" Z" if closed else "")
        self.path(d, sw, fill, dash)

    def line(self, x1, y1, x2, y2, sw=1.0, colour=INK, dash=None):
        d = f' stroke-dasharray="{dash}"' if dash else ""
        self.items.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{colour}" stroke-width="{sw / self.s:.3f}"{d}/>')

    def text(self, x, y, s, size=11, colour=GREY, anchor="middle", weight=400):
        """Text at a mm position, in a fixed px size."""
        self.labels.append(lambda: f'<text x="{self._px(x):.1f}" y="{self._py(y):.1f}" font-size="{size}" fill="{colour}" text-anchor="{anchor}" font-weight="{weight}">{esc(s)}</text>')

    def dim(self, x1, y1, x2, y2, label, offset=0):
        """A dimension line between two mm points with its label beside the middle."""
        self.line(x1, y1, x2, y2, sw=1, colour=GREY)
        for (ax, ay), (bx, by) in (((x1, y1), (x2, y2)), ((x2, y2), (x1, y1))):
            vx, vy = bx - ax, by - ay
            n = max(1e-6, (vx * vx + vy * vy) ** 0.5)
            ux, uy = vx / n, vy / n
            a = 9 / self.s
            self.items.append(f'<path d="M{ax} {ay} L{ax + ux * a - uy * a * 0.45:.2f} {ay + uy * a + ux * a * 0.45:.2f} L{ax + ux * a + uy * a * 0.45:.2f} {ay + uy * a - ux * a * 0.45:.2f} Z" fill="{GREY}"/>')
        mx, my = (x1 + x2) / 2, (y1 + y2) / 2
        if abs(x2 - x1) >= abs(y2 - y1):
            self.labels.append(lambda: f'<text x="{self._px(mx):.1f}" y="{self._py(my) - 5 + offset:.1f}" font-size="11" fill="{GREY}" text-anchor="middle">{esc(label)}</text>')
        else:
            def vlabel():
                px, py = self._px(mx) - 6 + offset, self._py(my)
                return f'<text x="{px:.1f}" y="{py:.1f}" font-size="11" fill="{GREY}" text-anchor="middle" transform="rotate(-90 {px:.1f} {py:.1f})">{esc(label)}</text>'
            self.labels.append(vlabel)

    def callout(self, n, x, y, name, note="", side=None):
        """Point number n at the mm position x, y. The bubble goes in the left or right column (nearest side unless given)."""
        self.calls.append((n, x, y, name, note, side or ("l" if x < self.w / 2 else "r")))

    def note(self, s):
        self.notes.append(s)

    def set_underlay(self, png, x_mm, y_mm, w_mm, h_mm):
        self.underlay = (png, x_mm, y_mm, w_mm, h_mm)

    # ---- layout ---------------------------------------------------------------------------------
    def _px(self, x):
        return self.ox + x * self.s

    def _py(self, y):
        return self.oy + y * self.s

    def _columns(self):
        """Bubble y positions: each column sorted by target y, pushed apart to ROW, kept inside the drawing height."""
        out = {}
        for side in ("l", "r"):
            cs = sorted([c for c in self.calls if c[5] == side], key=lambda c: c[2])
            ys = [self._py(c[2]) for c in cs]
            for _ in range(200):
                moved = False
                for i in range(1, len(ys)):
                    if ys[i] - ys[i - 1] < ROW:
                        push = (ROW - (ys[i] - ys[i - 1])) / 2
                        ys[i - 1] -= push; ys[i] += push; moved = True
                lo, hi = self.oy + BUBBLE_R, self.oy + self.h * self.s - BUBBLE_R
                if ys and ys[0] < lo:
                    ys = [y + (lo - ys[0]) for y in ys]; moved = True
                if ys and ys[-1] > hi and ys[-1] - ys[0] <= hi - lo:
                    ys = [y - (ys[-1] - hi) for y in ys]; moved = True
                if not moved:
                    break
            for c, y in zip(cs, ys):
                out[c[0]] = y
        return out

    def svg(self, underlay_opacity=None):
        has_l = any(c[5] == "l" for c in self.calls)
        has_r = any(c[5] == "r" for c in self.calls)
        left = (BUBBLE_R * 2 + GUTTER) if has_l else 20
        right = (BUBBLE_R * 2 + GUTTER) if has_r else 20
        self.ox, self.oy = left + 10, 56
        dw, dh = self.w * self.s, self.h * self.s
        lx = self.ox + dw + right + 16
        rows = sorted(self.calls, key=lambda c: c[0])
        legend_h = 34 + sum(22 + (18 if c[4] else 0) for c in rows) + 18 * len(self.notes) + 10
        W = int(lx + LEGEND_W + 16)
        H = int(max(self.oy + dh + 30, 56 + legend_h + 10))
        o = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}">',
             f'<rect width="{W}" height="{H}" rx="10" fill="#fff"/>',
             f'<text x="{self.ox}" y="30" font-size="15" font-weight="700" fill="{INK}">{esc(self.title)}</text>']
        if self.underlay and underlay_opacity:
            png, x, y, w, h = self.underlay
            data = base64.b64encode(open(png, "rb").read()).decode()
            o.append(f'<image href="data:image/png;base64,{data}" x="{self._px(x):.1f}" y="{self._py(y):.1f}" width="{w * self.s:.1f}" height="{h * self.s:.1f}" opacity="{underlay_opacity}" preserveAspectRatio="none"/>')
        o.append(f'<g transform="translate({self.ox} {self.oy}) scale({self.s})">')
        o += self.items
        o.append("</g>")
        o += [lab() for lab in self.labels]
        ys = self._columns()
        colx = {"l": self.ox - GUTTER, "r": self.ox + dw + GUTTER}
        for n, x, y, name, note, side in self.calls:
            bx, by, tx, ty = colx[side], ys[n], self._px(x), self._py(y)
            o.append(f'<path d="M{bx + (BUBBLE_R if side == "l" else -BUBBLE_R)} {by:.1f} L{tx:.1f} {ty:.1f}" stroke="{MAGENTA}" stroke-width="1.2" fill="none"/>')
            o.append(f'<circle cx="{tx:.1f}" cy="{ty:.1f}" r="2" fill="{MAGENTA}"/>')
            o.append(f'<circle cx="{bx}" cy="{by:.1f}" r="{BUBBLE_R}" fill="{MAGENTA}"/>')
            o.append(f'<text x="{bx}" y="{by + 4:.1f}" font-size="11" font-weight="700" fill="#fff" text-anchor="middle">{n}</text>')
        y = 56 + 4
        o.append(f'<text x="{lx}" y="{y}" font-size="13" font-weight="700" fill="{INK}">{esc(self.legend_title)}</text>')
        y += 26
        for n, _, _, name, note, _ in rows:
            o.append(f'<text x="{lx}" y="{y}" font-size="12.5" fill="{INK}"><tspan font-weight="700" fill="{MAGENTA}">{n}</tspan><tspan x="{lx + 22}" font-weight="700">{esc(name)}</tspan></text>')
            if note:
                y += 17
                o.append(f'<text x="{lx + 22}" y="{y}" font-size="12" fill="{GREY}">{esc(note)}</text>')
            y += 22
        for s in self.notes:
            o.append(f'<text x="{lx}" y="{y}" font-size="12" fill="{GREY}">{esc(s)}</text>')
            y += 18
        o.append("</svg>")
        return "\n".join(o) + "\n"


def write(fig, path, trace_dir=None, opacity=0.45):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(fig.svg())
    if trace_dir and fig.underlay:
        os.makedirs(trace_dir, exist_ok=True)
        t = os.path.join(trace_dir, os.path.basename(path).replace(".svg", ".trace.svg"))
        with open(t, "w", encoding="utf-8", newline="\n") as f:
            f.write(fig.svg(underlay_opacity=opacity))
        return t
    return None
