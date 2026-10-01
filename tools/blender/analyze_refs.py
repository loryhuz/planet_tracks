"""Splits the registered reference views (art/buggy/refs/<view>.png) into material classes by colour
(white and orange paint, gold foil, dark structure, glass, lights) and extracts, in metres, the
outlines the model is lofted from:

- side: for each station along the car, the top and bottom of the painted body;
- top: for each station, the painted body's half-width;
- front / back: for each height, the painted body's half-width.

Writes art/buggy/refs/profiles.json and <view>_classes.png (one flat colour per class) to check.

    blender -b -P tools/blender/analyze_refs.py
"""

import json
import os
import sys

import bpy  # noqa: F401  (image IO through register_refs)
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blueprint as bp  # noqa: E402
from register_refs import load, save  # noqa: E402
from warp_refs import background  # noqa: E402

REFS = os.path.normpath(os.path.join(HERE, "..", "..", "art", "buggy", "refs"))

CLASSES = {
    "white": (0.92, 0.92, 0.9),
    "orange": (1.0, 0.42, 0.0),
    "gold": (0.85, 0.65, 0.1),
    "dark": (0.12, 0.12, 0.14),
    "grey": (0.5, 0.5, 0.52),
    "cyan": (0.1, 0.85, 1.0),
    "red": (0.9, 0.1, 0.1),
}


def classify(px, bg):
    r, g, b = px[..., 0], px[..., 1], px[..., 2]
    lum = px.mean(axis=2)
    sat = px.max(axis=2) - px.min(axis=2)
    cls = np.full(lum.shape, "", dtype=object)
    car = ~bg
    cls[car & (lum < 0.3)] = "dark"
    cls[car & (lum >= 0.3) & (sat < 0.18)] = "grey"
    cls[car & (lum > 0.66) & (sat < 0.16)] = "white"
    cls[car & (r > 0.7) & (g > 0.18) & (g < 0.62) & (b < 0.32) & (r - g > 0.3)] = "orange"
    cls[car & (r > 0.55) & (g > 0.42) & (b < 0.4) & (r - b > 0.3) & (r - g < 0.3)] = "gold"
    cls[car & (b > 0.55) & (g > 0.5) & (r < 0.5) & (b - r > 0.25)] = "cyan"
    cls[car & (r > 0.6) & (g < 0.25) & (b < 0.25)] = "red"
    return cls


def close(mask, radius):
    """Binary closing with a square of `radius` px (dilate then erode)."""
    m = mask.copy()
    for _ in range(radius):
        d = m.copy()
        d[1:] |= m[:-1]
        d[:-1] |= m[1:]
        d[:, 1:] |= m[:, :-1]
        d[:, :-1] |= m[:, 1:]
        m = d
    for _ in range(radius):
        e = m.copy()
        e[1:] &= m[:-1]
        e[:-1] &= m[1:]
        e[:, 1:] &= m[:, :-1]
        e[:, :-1] &= m[:, 1:]
        m = e
    return m


def fill_holes(mask):
    """Fill what the outside (connected to the border) does not reach."""
    h, w = mask.shape
    outside = np.zeros_like(mask)
    stack = [(y, x) for y in (0, h - 1) for x in range(w) if not mask[y, x]] + [(y, x) for x in (0, w - 1) for y in range(h) if not mask[y, x]]
    for y, x in stack:
        outside[y, x] = True
    while stack:
        y, x = stack.pop()
        for ny, nx in ((y + 1, x), (y - 1, x), (y, x + 1), (y, x - 1)):
            if 0 <= ny < h and 0 <= nx < w and not mask[ny, nx] and not outside[ny, nx]:
                outside[ny, nx] = True
                stack.append((ny, nx))
    return ~outside


def to_m(view, x_px=None, y_px=None):
    v = bp.VIEWS[view]
    s = v["scale"]
    if view == "top":
        cx, cy = v["centre_px"]
        return (None if x_px is None else (x_px - cx) / s, None if y_px is None else (cy - y_px) / s)
    return (None if x_px is None else (x_px - v["centre_px"]) / s, None if y_px is None else bp.GROUND_Y + (v["ground_px"] - y_px) / s)


def main():
    profiles = {}
    for view in ("side", "top", "front", "back"):
        px = load(os.path.join(REFS, f"{view}.png"))
        bg = background(px)
        cls = classify(px, bg)
        out = np.ones_like(px)
        for name, col in CLASSES.items():
            out[cls == name] = col
        save(os.path.join(REFS, f"{view}_classes.png"), out)
        paint = (cls == "white") | (cls == "orange")
        body = fill_holes(close(paint, 6))
        prof = []
        if view in ("side", "top"):
            for x in range(0, px.shape[1], 6):
                col = np.nonzero(body[:, x])[0]
                if col.size:
                    z, _ = to_m(view, x_px=x)
                    _, a = to_m(view, y_px=col.min())
                    _, b = to_m(view, y_px=col.max())
                    prof.append([round(z, 4), round(b, 4), round(a, 4)])  # z, low, high (side: y; top: x)
        else:
            for y in range(0, px.shape[0], 6):
                row = np.nonzero(body[y])[0]
                if row.size:
                    _, h = to_m(view, y_px=y)
                    a, _ = to_m(view, x_px=row.min())
                    b, _ = to_m(view, x_px=row.max())
                    prof.append([round(h, 4), round(a, 4), round(b, 4)])  # y, left, right (image order)
        profiles[view] = prof
        save(os.path.join(REFS, f"{view}_body.png"), np.where(body[..., None], px * 0.6 + 0.4 * np.array([0, 0.6, 1.0]), px))
        print(view, len(prof), "stations")
    json.dump(profiles, open(os.path.join(REFS, "profiles.json"), "w"))


if __name__ == "__main__":
    main()
