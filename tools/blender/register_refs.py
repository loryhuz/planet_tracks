"""Registers generated reference views on the blueprint canvases (tools/blender/blueprint.py): finds
the tyres where they touch the ground (or, from above, the tyres' outer edges), scales and shifts
the image so they land exactly on the game's wheel positions, and writes, per view:

- art/buggy/refs/<view>.png: the registered view (the reference Blender and the checks use);
- art/buggy/refs/<view>_grid.png: the same with a metric grid (10 cm, 50 cm in darker lines, the
  car's axes in blue) and the target tyre outlines in red, to read coordinates off it.

    blender -b -P tools/blender/register_refs.py -- side=path.png front=path.png ...
"""

import json
import os
import sys

import bpy
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blueprint as bp  # noqa: E402

REFS = os.environ.get("REFS_OUT") or os.path.normpath(os.path.join(HERE, "..", "..", "art", "buggy", "refs"))


def load(path):
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1, :, :3]
    bpy.data.images.remove(img)
    return px


def save(path, px):
    h, w, _ = px.shape
    img = bpy.data.images.new(os.path.basename(path), w, h)
    rgba = np.ones((h, w, 4), dtype=np.float32)
    rgba[..., :3] = np.clip(px, 0, 1)
    img.pixels[:] = rgba[::-1].ravel()
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    bpy.data.images.remove(img)


def silhouette(px):
    lum = px.mean(axis=2)
    sat = px.max(axis=2) - px.min(axis=2)
    return ~((lum > 0.88) & (sat < 0.08))


def runs(line, min_len=6):
    out, start = [], None
    for i, v in enumerate(line):
        if v and start is None:
            start = i
        if (not v or i == len(line) - 1) and start is not None:
            if i - start >= min_len:
                out.append((start, i - 1))
            start = None
    return out


def contacts(sil):
    """Ground row and the centres of the two widest runs of silhouette just above it."""
    ys = np.nonzero(sil.any(axis=1))[0]
    ground = int(ys.max())
    band = sil[ground - 12 : ground - 2].any(axis=0)
    rs = sorted(runs(band), key=lambda r: r[0] - r[1])[:2]
    rs.sort()
    return ground, [(a + b) / 2 for a, b in rs], rs


def warp(px, s, tx, ty, size):
    """Output pixel (u, v) samples the source at ((u - tx) / s, (v - ty) / s), bilinear."""
    w, h = size
    us, vs = np.meshgrid(np.arange(w, dtype=np.float32), np.arange(h, dtype=np.float32))
    x = (us - tx) / s
    y = (vs - ty) / s
    x0 = np.clip(np.floor(x).astype(int), 0, px.shape[1] - 2)
    y0 = np.clip(np.floor(y).astype(int), 0, px.shape[0] - 2)
    fx = np.clip(x - x0, 0, 1)[..., None]
    fy = np.clip(y - y0, 0, 1)[..., None]
    out = (px[y0, x0] * (1 - fx) * (1 - fy) + px[y0, x0 + 1] * fx * (1 - fy) + px[y0 + 1, x0] * (1 - fx) * fy + px[y0 + 1, x0 + 1] * fx * fy)
    outside = (x < 0) | (y < 0) | (x > px.shape[1] - 1) | (y > px.shape[0] - 1)
    out[outside] = 1.0
    return out


def register(view, px):
    v = bp.VIEWS[view]
    w, h = v["size"]
    sil = silhouette(px)
    centres = sorted({tuple(round(q) for q in bp.project(view, c)) for c in bp.wheel_centres()})
    if view == "top":
        ys = np.nonzero(sil.any(axis=1))[0]
        top, bottom = int(ys.min()), int(ys.max())
        # Outer edges of the tyres; their x centres from the rows just inside them.
        row = sil[bottom - 10]
        rs = sorted(runs(row), key=lambda r: r[0] - r[1])[:2]
        rs.sort()
        got_x = [(a + b) / 2 for a, b in rs]
        want_x = sorted({c[0] for c in centres})
        want_top = min(c[1] for c in centres) - bp.TYRE_W / 2 * v["scale"]
        want_bottom = max(c[1] for c in centres) + bp.TYRE_W / 2 * v["scale"]
        sx = (want_x[1] - want_x[0]) / (got_x[1] - got_x[0])
        sy = (want_bottom - want_top) / (bottom - top)
        s = sx
        tx = want_x[0] - got_x[0] * s
        ty = (want_top + want_bottom) / 2 - (top + bottom) / 2 * s
        found = dict(tyre_x=got_x, edges=[top, bottom])
    else:
        ground, got_x, rs = contacts(sil)
        want_x = sorted({c[0] for c in centres})
        sx = (want_x[1] - want_x[0]) / (got_x[1] - got_x[0])
        s = sx
        tx = want_x[0] - got_x[0] * s
        ty = v["ground_px"] - ground * s
        # Vertical check: the tyre's height where it is the outermost part of the silhouette.
        sy = None
        found = dict(ground=ground, tyre_x=got_x, contact_runs=rs)
    out = warp(px, s, tx, ty, (w, h))
    return out, dict(scale=s, tx=tx, ty=ty, sx=sx, sy=sy, found=found)


def grid_overlay(view, px):
    v = bp.VIEWS[view]
    w, h = v["size"]
    s = v["scale"]
    out = px.copy()
    origin = bp.project(view, (0, 0, 0))
    # Grid lines every 10 cm from the car's origin (vertical views: from the ground line).
    oy = v["ground_px"] if v["ground_px"] is not None else origin[1]
    for i in range(-40, 41):
        x = int(round(origin[0] + i * 0.1 * s))
        y = int(round(oy + i * 0.1 * s))
        strong = i % 5 == 0
        col = np.array([0.0, 0.25, 0.9]) if i == 0 else (np.array([0.35, 0.35, 0.4]) if strong else np.array([0.6, 0.62, 0.7]))
        a = 0.55 if strong or i == 0 else 0.28
        if 0 <= x < w:
            out[:, x] = out[:, x] * (1 - a) + col * a
        if 0 <= y < h:
            out[y, :] = out[y, :] * (1 - a) + col * a
    # Target tyre outlines.
    ys, xs = np.mgrid[0:h, 0:w]
    for c in bp.wheel_centres():
        cx, cy = bp.project(view, c)
        if view == "side":
            d = np.hypot(xs - cx, ys - cy)
            ring = np.abs(d - bp.WHEEL_R * s) < 2.0
        else:
            hw = (bp.WHEEL_R if view == "top" else bp.TYRE_W / 2) * s
            hh = (bp.TYRE_W / 2 if view == "top" else bp.WHEEL_R) * s
            inside = (np.abs(xs - cx) <= hw) & (np.abs(ys - cy) <= hh)
            inner = (np.abs(xs - cx) <= hw - 3) & (np.abs(ys - cy) <= hh - 3)
            ring = inside & ~inner
        out[ring] = [1.0, 0.0, 0.0]
    return out


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    report = {}
    for a in args:
        view, path = a.split("=", 1)
        px = load(path)
        reg, info = register(view, px)
        save(os.path.join(REFS, f"{view}.png"), reg)
        save(os.path.join(REFS, f"{view}_grid.png"), grid_overlay(view, reg))
        report[view] = info
        print(view, json.dumps(info))
    path = os.path.join(REFS, "registration.json")
    old = json.load(open(path)) if os.path.exists(path) else {}
    old.update(report)
    json.dump(old, open(path, "w"), indent=1)


if __name__ == "__main__":
    main()
