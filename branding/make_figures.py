"""Builds the traced help figures in docs/kb/images from branding/figures/*.py.

    python branding/make_figures.py [name ...] [--trace DIR]

Each module in branding/figures defines FILE (the svg name) and build() -> figlib.Fig. With --trace,
a copy with the rectified photo underneath is also written to DIR (for checking a tracing).
"""
import argparse
import importlib
import os
import pkgutil
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import figlib  # noqa: E402
import figures  # noqa: E402

OUT = os.path.join(here, "..", "docs", "kb", "images")

ap = argparse.ArgumentParser()
ap.add_argument("names", nargs="*")
ap.add_argument("--trace")
a = ap.parse_args()
for m in pkgutil.iter_modules(figures.__path__):
    if a.names and m.name not in a.names:
        continue
    mod = importlib.import_module("figures." + m.name)
    if not getattr(mod, "PUBLISH", True) and not a.names:
        continue
    t = figlib.write(mod.build(), os.path.join(OUT, mod.FILE), a.trace)
    print("wrote", mod.FILE, "| trace:", t or "-")
