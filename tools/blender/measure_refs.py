"""Measures the reference views in art/buggy/refs: the car's silhouette (everything that is not the
white background), its bounding box, the ground line and, where they show, the wheel centres
(from the cyan hub rings) and tyre extents. Prints them in pixels and writes the silhouette masks
next to the views (`*_mask.png`), for checking.

    blender -b -P tools/blender/measure_refs.py
"""

import json
import os

import bpy
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
REFS = os.path.normpath(os.path.join(HERE, "..", "..", "art", "buggy", "refs"))


def load(name):
    img = bpy.data.images.load(os.path.join(REFS, name))
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1, :, :3]  # top row first
    return px


def save_mask(name, mask):
    h, w = mask.shape
    img = bpy.data.images.new(name, w, h)
    rgba = np.ones((h, w, 4), dtype=np.float32)
    rgba[..., :3] = np.where(mask[..., None], 0.0, 1.0)
    img.pixels[:] = rgba[::-1].ravel()
    img.filepath_raw = os.path.join(REFS, name)
    img.file_format = "PNG"
    img.save()


def silhouette(px):
    # Background: near white and unsaturated (the views have no cast shadow).
    lum = px.mean(axis=2)
    sat = px.max(axis=2) - px.min(axis=2)
    return ~((lum > 0.88) & (sat < 0.08))


def clusters(mask, min_px=200):
    """Connected blobs (4-neighbour flood fill on a downsampled grid): list of (cx, cy, x0, y0, x1, y1, n)."""
    step = 2
    m = mask[::step, ::step]
    h, w = m.shape
    seen = np.zeros_like(m, dtype=bool)
    out = []
    ys, xs = np.nonzero(m)
    for y, x in zip(ys, xs):
        if seen[y, x]:
            continue
        stack = [(y, x)]
        seen[y, x] = True
        pts = []
        while stack:
            cy, cx = stack.pop()
            pts.append((cy, cx))
            for ny, nx in ((cy + 1, cx), (cy - 1, cx), (cy, cx + 1), (cy, cx - 1)):
                if 0 <= ny < h and 0 <= nx < w and m[ny, nx] and not seen[ny, nx]:
                    seen[ny, nx] = True
                    stack.append((ny, nx))
        if len(pts) * step * step >= min_px:
            a = np.array(pts) * step
            out.append(dict(cx=float(a[:, 1].mean()), cy=float(a[:, 0].mean()), x0=int(a[:, 1].min()), y0=int(a[:, 0].min()), x1=int(a[:, 1].max()), y1=int(a[:, 0].max()), n=len(pts) * step * step))
    return sorted(out, key=lambda c: -c["n"])


def cyan(px):
    r, g, b = px[..., 0], px[..., 1], px[..., 2]
    return (b > 0.55) & (g > 0.45) & (r < 0.45) & (b - r > 0.3)


def main():
    report = {}
    for view in ("side", "front", "back", "top"):
        px = load(f"{view}.jpg")
        h, w, _ = px.shape
        sil = silhouette(px)
        save_mask(f"{view}_mask.png", sil)
        ys, xs = np.nonzero(sil)
        info = dict(size=[w, h], bbox=[int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())])
        info["ground_y"] = int(ys.max())
        rings = [c for c in clusters(cyan(px), 400)]
        info["cyan"] = rings[:6]
        # Lowest silhouette pixel per column: the tyres' bottoms near the ground.
        low = np.full(w, -1)
        for x in range(w):
            col = np.nonzero(sil[:, x])[0]
            if col.size:
                low[x] = col.max()
        near = low >= info["ground_y"] - max(4, h // 150)
        runs, start = [], None
        for x in range(w):
            if near[x] and start is None:
                start = x
            if (not near[x] or x == w - 1) and start is not None:
                runs.append([start, x - 1])
                start = None
        info["ground_contacts"] = runs
        report[view] = info
        print(view, json.dumps(info))
    with open(os.path.join(REFS, "measurements.json"), "w") as f:
        json.dump(report, f, indent=1)


main()
