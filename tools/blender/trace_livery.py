"""Traces the paint of the registered plans (art/buggy/v2/registered) into clean vector shapes for
the livery (tools/blender/buggy_livery.py): each view is posterised into white, black and orange,
cleared of the dirt (spots and scratches smaller than a few square centimetres take the colour
around them), and the borders of the black and orange areas are traced and simplified into
polygons, in the car's metres. Writes art/buggy/v2/livery/<view>.json: {"black": [ring, ...],
"orange": [ring, ...]}, each ring a list of (a, b) points, filled with the even-odd rule; (a, b)
are (z, height) on the side views, (z, x) from above, (x, height) from the front and back.

Runs with a plain Python 3 that has numpy, scipy, scikit-image and Pillow (not Blender's):

    python3 tools/blender/trace_livery.py [view ...]
"""

import json
import os
import sys

import numpy as np
from PIL import Image
from scipy import ndimage
from skimage import measure

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import blueprint as bp  # noqa: E402

OUT = os.path.join(bp.REPO, "art", "buggy", "v2", "livery")
BLACK, WHITE, ORANGE, NONE = 0, 1, 2, 3
SPOT = 0.0025  # m²: dirt smaller than this goes
TOLERANCE = 0.008  # m: polygon simplification
CLOSE = {"black": 0.010, "orange": 0.010}  # m: closing radius bridging the scratches
OPEN = {"black": 0.008, "orange": 0.006}  # m: opening radius shaving the frayed edges
SHAPE_LOSS = 0.07  # a stripe's outline is reduced to the fewest corners losing at most this area
# Emblems traced finer from the clearest instance on the left plan, in a box (a0, a1, b0, b1),
# with the colour of the emblem; normalised to a unit height, base centre at the origin.
EMBLEMS = {
    "aurora": dict(box=(-1.66, -1.44, 1.385, 1.535), colour="white", blur=2.0, tolerance=1.6),
    "swan": dict(box=(0.045, 0.255, 1.115, 1.31), colour="black"),
}


def classify(px, alpha):
    rgb = ndimage.median_filter(px, size=(5, 5, 1))
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    lum = rgb.mean(-1)
    mx = rgb.max(-1)
    sat = (mx - rgb.min(-1)) / np.maximum(mx, 1e-3)
    orange = (r > 0.45) & (r - b > 0.28) & (sat > 0.45) & (g < r * 0.85)
    white = ~orange & (lum > 0.5)
    cls = np.where(orange, ORANGE, np.where(white, WHITE, BLACK))
    cls[~alpha] = NONE
    return cls


def clean(cls, area):
    """Small islands of a colour take the colour most present around them."""
    out = cls.copy()
    for _ in range(4):
        changed = 0
        for c in (BLACK, WHITE, ORANGE):
            lab, n = ndimage.label(out == c)
            if n == 0:
                continue
            sizes = ndimage.sum(np.ones_like(lab), lab, range(1, n + 1))
            small = np.isin(lab, 1 + np.nonzero(sizes < area)[0])
            if not small.any():
                continue
            votes = np.stack([ndimage.uniform_filter(((out == k) & ~small).astype(np.float32), 25) for k in range(4)], -1)
            votes[..., c] = -1
            out[small] = votes[small].argmax(-1)
            changed += int(small.sum())
        if not changed:
            break
    soft = np.stack([ndimage.gaussian_filter((out == k).astype(np.float32), 2.5) for k in range(4)], -1)
    return soft.argmax(-1)


def to_metres(view, rows, cols):
    v = bp.VIEWS[view]
    s = v["scale"]
    if view == "top":
        cx, cy = v["centre_px"]
        return (cols - cx) / s, (cy - rows) / s
    sign = sum(v["right"])
    return sign * (cols - v["centre_px"]) / s, (v["ground_px"] - rows) / s


def area(p):
    return 0.5 * abs(np.dot(p[:, 0], np.roll(p[:, 1], -1)) - np.dot(np.roll(p[:, 0], -1), p[:, 1]))


def clean_shape(pts):
    """A stripe's clean outline: when the ring is nearly convex (the paint's stripes, chevrons and
    wedges, frayed by dirt), its convex hull reduced to its main corners: corners are dropped,
    the least significant first, while the area lost stays under SHAPE_LOSS. Other rings (the
    body's outline, the panels' holes) are kept as traced."""
    from scipy.spatial import ConvexHull

    p = np.asarray(pts)
    hull = p[ConvexHull(p).vertices]
    a0 = area(p)
    if area(hull) > 1.25 * a0 or len(hull) < 4:
        return p
    q = hull
    while len(q) > 3:
        best = None
        for i in range(len(q)):
            r = np.delete(q, i, axis=0)
            loss = (area(hull) - area(r)) / area(hull)
            if best is None or loss < best[0]:
                best = (loss, r)
        if best[0] > SHAPE_LOSS:
            break
        q = best[1]
    return q


def emblem(name, spec):
    view = "left"
    im = np.asarray(Image.open(os.path.join(bp.REGISTERED, f"{view}.png")).convert("RGB")).astype(np.float32) / 255
    s = bp.VIEWS[view]["scale"]
    a0, a1, b0, b1 = spec["box"]
    v = bp.VIEWS[view]
    cols = sorted([v["centre_px"] - a0 * s, v["centre_px"] - a1 * s])
    rows = sorted([v["ground_px"] - b0 * s, v["ground_px"] - b1 * s])
    c0, c1, r0, r1 = int(cols[0]), int(cols[1]), int(rows[0]), int(rows[1])
    lum = ndimage.gaussian_filter(im[r0:r1, c0:c1].mean(-1), spec.get("blur", 1.0))
    fg = lum > 0.5 if spec["colour"] == "white" else lum < 0.5
    lab, n = ndimage.label(fg)
    sizes = ndimage.sum(np.ones_like(lab), lab, range(1, n + 1))
    border = set(np.unique(np.r_[lab[0], lab[-1], lab[:, 0], lab[:, -1]])) - {0}
    fg = np.isin(lab, [i for i in 1 + np.nonzero(sizes > 150)[0] if i not in border])
    lab, n = ndimage.label(~fg)
    sizes = ndimage.sum(np.ones_like(lab), lab, range(1, n + 1))
    fg |= np.isin(lab, 1 + np.nonzero(sizes < 30)[0])
    rings = []
    for ring in measure.find_contours(np.pad(fg.astype(np.float32), 1), 0.5):
        ring = measure.approximate_polygon(ring, spec.get("tolerance", 0.7))
        if len(ring) >= 4:
            rings.append(np.c_[ring[:, 1] - 1 + c0, ring[:, 0] - 1 + r0][:-1])
    allp = np.concatenate(rings)
    top, base = allp[:, 1].min(), allp[:, 1].max()
    mid = (allp[:, 0].min() + allp[:, 0].max()) / 2
    h = base - top
    # Unit height, x to the image's right (the car's rear on the left side), y up.
    return [[[round(float((x - mid) / h), 4), round(float((base - y) / h), 4)] for x, y in r] for r in rings]


def trace(view):
    im = np.asarray(Image.open(os.path.join(bp.REGISTERED, f"{view}.png")).convert("RGBA")).astype(np.float32) / 255
    s = bp.VIEWS[view]["scale"]
    cls = clean(classify(im[..., :3], im[..., 3] > 0.5), int(SPOT * s * s))
    out = {}
    disk = lambda r: np.hypot(*np.mgrid[-r : r + 1, -r : r + 1]) <= r  # noqa: E731
    for name, c in (("black", BLACK), ("orange", ORANGE)):
        mask = cls == c
        # Bridge the scratches across the shapes (closing), then shave the spurs and the frayed
        # edges (opening).
        mask = ndimage.binary_closing(mask, disk(int(round(CLOSE[name] * s))))
        mask = ndimage.binary_opening(mask, disk(int(round(OPEN[name] * s))))
        mask = np.pad(mask.astype(np.float32), 1)
        rings = []
        for ring in measure.find_contours(ndimage.gaussian_filter(mask, 0.7), 0.5):
            ring = measure.approximate_polygon(ring, TOLERANCE * s)
            if len(ring) < 4:
                continue
            a, b = to_metres(view, ring[:, 0] - 1, ring[:, 1] - 1)
            pts = np.c_[a, b]
            area = 0.5 * abs(np.dot(pts[:-1, 0], pts[1:, 1]) - np.dot(pts[1:, 0], pts[:-1, 1]))
            if area < SPOT:
                continue
            pts = clean_shape(pts[:-1])
            rings.append([[round(float(x), 4), round(float(y), 4)] for x, y in pts])
        out[name] = rings
    os.makedirs(OUT, exist_ok=True)
    json.dump(out, open(os.path.join(OUT, f"{view}.json"), "w"))
    print(view, {k: len(v) for k, v in out.items()}, sum(len(r) for v in out.values() for r in v), "points")


if __name__ == "__main__":
    for view in sys.argv[1:] or ["left", "top", "front", "back"]:
        trace(view)
    emblems = {name: emblem(name, spec) for name, spec in EMBLEMS.items()}
    json.dump(emblems, open(os.path.join(OUT, "emblems.json"), "w"))
    print("emblems", {k: len(v) for k, v in emblems.items()})
