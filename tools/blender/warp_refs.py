"""Registers the buggy "B" plans (art/buggy/v2/views) on the blueprint canvases
(tools/blender/blueprint.py), at the game's proportions.

The plans are scaled so their wheels have the game's 0.45 m radius and sit exactly on the game's
wheel positions. Their wheelbase is longer than the physics' (about 3.25 wheel diameters on the
side views instead of 2.9), and their track a little wider: each view is warped piecewise along
the car's length and width so the wheels keep their exact shape, the overhangs keep the wheels'
scale, and only what lies between the tyres is squeezed. Each view also gets the car's mask (the
white studio background and the ground shadows removed), so the checks can compare silhouettes.

Writes, per view, art/buggy/v2/registered/<view>.png (RGBA, alpha = the car) and <view>_grid.png
(on white, with a metric grid and the target tyres).

    blender -b -P tools/blender/warp_refs.py

The landmarks below were measured on the plans: the side views' wheel centres are circle fits of
the orange beadlock rings and the radius their distance to the ground contact; the top view's are
the tyres' footprints; the front and back views', the near tyres' edges and ground contacts.
"""

import json
import math
import os
import sys

import bpy
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blueprint as bp  # noqa: E402

R = bp.WHEEL_R
HALF_IN = bp.TRACK / 2 - bp.TYRE_W / 2  # tyre inner edge, m from the centre line
HALF_OUT = bp.TRACK / 2 + bp.TYRE_W / 2

LANDMARKS = {
    # Wheel centres (px) and tyre radius (px) on the side views.
    "left": dict(front=(420.8, 975.7), rear=(2300.4, 971.5), r=287.0),
    "right": dict(front=(2263.8, 940.8), rear=(383.6, 926.3), r=284.0),
    # Top: tyre footprints. Columns of the front and rear axles, tyre length (2r); rows of the
    # left tyres' outer and inner edges, then the right tyres'.
    "top": dict(front=2051.5, rear=643.5, r=240.0, left=(119.0, 393.0), right=(1366.5, 1091.0)),
    # Front and back: the near tyres' edges from the image's left to its right (outer, inner,
    # inner, outer), the ground contact row and the tyres' scale (px per metre, from their height).
    "front": dict(edges=(70.0, 507.0, 1827.0, 2266.0), ground=1557.0, scale=948.0),
    "back": dict(edges=(68.0, 560.0, 1776.0, 2266.0), ground=1566.0, scale=984.0),
}


def load(path):
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1].copy()
    bpy.data.images.remove(img)
    return px


def save(path, px):
    """Saves an RGB or RGBA float image (row 0 at the top)."""
    h, w, c = px.shape
    img = bpy.data.images.new(os.path.basename(path), w, h, alpha=c == 4)
    rgba = np.ones((h, w, 4), dtype=np.float32)
    rgba[..., :c] = np.clip(px, 0, 1)
    img.pixels[:] = rgba[::-1].ravel()
    img.filepath_raw = path
    img.file_format = "PNG"
    if c == 4:
        img.alpha_mode = "STRAIGHT"
    img.save()
    bpy.data.images.remove(img)


def label(mask):
    """Connected components (4-neighbours) of a boolean image: per pixel, the smallest flat index
    of its component (-1 outside the mask). Min propagation with pointer jumping."""
    h, w = mask.shape
    big = h * w
    lab = np.where(mask, np.arange(big).reshape(h, w), big)
    while True:
        m = lab.copy()
        np.minimum(m[1:], lab[:-1], out=m[1:])
        np.minimum(m[:-1], lab[1:], out=m[:-1])
        np.minimum(m[:, 1:], lab[:, :-1], out=m[:, 1:])
        np.minimum(m[:, :-1], lab[:, 1:], out=m[:, :-1])
        m[~mask] = big
        flat = m.ravel()
        for _ in range(4):
            ok = flat < big
            flat[ok] = flat[flat[ok]]
        if np.array_equal(m, lab):
            break
        lab = m
    lab[~mask] = -1
    return lab


def car_mask(px, ground=None, shadow=0, pockets=None):
    """The car: everything but the studio background (near-white and neutral, connected to the
    border or in large enclosed pockets) and, in side, front and back views, the grey ground
    shadow (light pixels in a band above the ground line)."""
    rgb = px[..., :3]
    lum = rgb.mean(axis=2)
    sat = rgb.max(axis=2) - rgb.min(axis=2)
    light = (lum > 0.955) & (sat < 0.03)
    if ground is not None:
        h = lum.shape[0]
        rows = np.arange(h)[:, None]
        light |= (rows > ground - shadow) & (lum > 0.62) & (sat < 0.05)
    lab = label(light)
    border = np.unique(np.r_[lab[0], lab[-1], lab[:, 0], lab[:, -1]])
    border = border[border >= 0]
    bg = np.isin(lab, border)
    # Enclosed pockets of background (between the wing's struts, under the car): thick, white and
    # perfectly neutral; the paint is warmer and spotted. On the front view the paint is as white
    # as the studio, so pockets only count above the roof and under the car (`pockets`: rows).
    rb = rgb[..., 0] - rgb[..., 2]
    ids, counts = np.unique(lab[(lab >= 0) & ~bg], return_counts=True)
    for i, n in zip(ids, counts):
        if n < 250:
            continue
        sel = lab == i
        thick = sel[3:-3, 3:-3] & sel[:-6, 3:-3] & sel[6:, 3:-3] & sel[3:-3, :-6] & sel[3:-3, 6:]
        if thick.sum() <= 120 or sat[sel].mean() > 0.0026 or abs(rb[sel].mean()) > 0.0012:
            continue
        if pockets is not None:
            rows = np.nonzero(sel.any(axis=1))[0]
            if not (rows.max() < pockets[0] or rows.min() > pockets[1]):
                continue
        bg |= sel
    return ~bg


def piecewise(src, dst):
    """Monotonic piecewise-linear map through breakpoints, extended linearly at both ends."""
    src, dst = np.asarray(src, float), np.asarray(dst, float)
    if src[0] > src[-1]:
        src, dst = src[::-1], dst[::-1]

    def f(x):
        x = np.asarray(x, float)
        y = np.interp(x, src, dst)
        lo, hi = x < src[0], x > src[-1]
        y = np.where(lo, dst[0] + (x - src[0]) * (dst[1] - dst[0]) / (src[1] - src[0]), y)
        y = np.where(hi, dst[-1] + (x - src[-1]) * (dst[-1] - dst[-2]) / (src[-1] - src[-2]), y)
        return y

    return f


def sample(px, sx, sy):
    """Bilinear lookup of `px` at float pixel coordinates (white and transparent outside)."""
    h, w = px.shape[:2]
    x0 = np.clip(np.floor(sx).astype(int), 0, w - 2)
    y0 = np.clip(np.floor(sy).astype(int), 0, h - 2)
    fx = np.clip(sx - x0, 0, 1)[..., None]
    fy = np.clip(sy - y0, 0, 1)[..., None]
    out = px[y0, x0] * (1 - fx) * (1 - fy) + px[y0, x0 + 1] * fx * (1 - fy) + px[y0 + 1, x0] * (1 - fx) * fy + px[y0 + 1, x0 + 1] * fx * fy
    outside = (sx < 0) | (sy < 0) | (sx > w - 1) | (sy > h - 1)
    out[outside] = [1.0, 1.0, 1.0, 0.0]
    return out.astype(np.float32)


def canvas_points(view):
    """Car-frame coordinates of every canvas pixel: (a, b) = (z, height above ground) for the
    side views, (z, x) from above, (x, height) from the front and back."""
    v = bp.VIEWS[view]
    w, h = v["size"]
    s = v["scale"]
    X, Y = np.meshgrid(np.arange(w, dtype=np.float64), np.arange(h, dtype=np.float64))
    if view == "top":
        cx, cy = v["centre_px"]
        return (X - cx) / s, (cy - Y) / s
    sign = sum(v["right"])  # +1 when the image's right is +Z (or +X), -1 otherwise
    return sign * (X - v["centre_px"]) / s, (v["ground_px"] - Y) / s


def warp(view, px):
    lm = LANDMARKS[view]
    a, b = canvas_points(view)
    if view in bp.SIDES:
        f, r_ = np.array(lm["front"]), np.array(lm["rear"])
        k = lm["r"] / R
        span = np.linalg.norm(r_ - f)
        e = (r_ - f) / span  # along the axle line, front to rear
        n = np.array([-e[1], e[0]])
        if n[1] < 0:
            n = -n  # perpendicular, pointing down the image
        # Metres behind the front axle to source pixels along the axle line.
        dz = bp.WHEELBASE / 2 - a
        u = piecewise([0.0, R, bp.WHEELBASE - R, bp.WHEELBASE], [0.0, lm["r"], span - lm["r"], span])(dz)
        v = -(b - R) * k  # below the axle line, source px
        sx = f[0] + e[0] * u + n[0] * v
        sy = f[1] + e[1] * u + n[1] * v
    elif view == "top":
        k = lm["r"] / R
        dz = bp.WHEELBASE / 2 - a
        sx = lm["front"] - piecewise([0.0, R, bp.WHEELBASE - R, bp.WHEELBASE], [0.0, lm["r"], lm["front"] - lm["rear"] - lm["r"], lm["front"] - lm["rear"]])(dz)
        (lo, li), (ro, ri) = lm["left"], lm["right"]
        sy = piecewise([-HALF_OUT, -HALF_IN, HALF_IN, HALF_OUT], [ro, ri, li, lo])(b)
    else:
        e0, e1, e2, e3 = lm["edges"]
        xs = [-HALF_OUT, -HALF_IN, HALF_IN, HALF_OUT] if view == "front" else [HALF_OUT, HALF_IN, -HALF_IN, -HALF_OUT]
        sx = piecewise(xs, [e0, e1, e2, e3])(a)
        sy = lm["ground"] - b * lm["scale"]
    return sample(px, sx, sy)


def grid_overlay(view, px):
    """The view on white with a metric grid (10 cm, 50 cm darker, the car's axes in blue) and the
    target tyres in red."""
    v = bp.VIEWS[view]
    w, h = v["size"]
    s = v["scale"]
    rgb = px[..., :3] * px[..., 3:4] + (1 - px[..., 3:4]) if px.shape[2] == 4 else px.copy()
    out = rgb.copy()
    origin = bp.project(view, (0, 0, 0))
    oy = v["ground_px"] if v["ground_px"] is not None else origin[1]
    for i in range(-40, 41):
        x = int(round(origin[0] + i * 0.1 * s))
        y = int(round(oy - i * 0.1 * s))
        strong = i % 5 == 0
        col = np.array([0.0, 0.25, 0.9]) if i == 0 else (np.array([0.35, 0.35, 0.4]) if strong else np.array([0.6, 0.62, 0.7]))
        a = 0.55 if strong or i == 0 else 0.28
        if 0 <= x < w:
            out[:, x] = out[:, x] * (1 - a) + col * a
        if 0 <= y < h:
            out[y, :] = out[y, :] * (1 - a) + col * a
    ys, xs = np.mgrid[0:h, 0:w]
    for c in bp.wheel_centres():
        cx, cy = bp.project(view, c)
        if view in bp.SIDES:
            ring = np.abs(np.hypot(xs - cx, ys - cy) - R * s) < 2.0
        else:
            hw = (R if view == "top" else bp.TYRE_W / 2) * s
            hh = (bp.TYRE_W / 2 if view == "top" else R) * s
            inside = (np.abs(xs - cx) <= hw) & (np.abs(ys - cy) <= hh)
            inner = (np.abs(xs - cx) <= hw - 3) & (np.abs(ys - cy) <= hh - 3)
            ring = inside & ~inner
        out[ring] = [1.0, 0.0, 0.0]
    return out


def main():
    os.makedirs(bp.REGISTERED, exist_ok=True)
    report = {}
    for view in bp.VIEWS:
        px = load(os.path.join(bp.PLANS, f"{view}.png"))
        ground = LANDMARKS[view].get("ground")
        if view in bp.SIDES:
            lm = LANDMARKS[view]
            ground = max(lm["front"][1], lm["rear"][1]) + lm["r"]
        k = LANDMARKS[view].get("scale") or LANDMARKS[view]["r"] / R
        mask = car_mask(px, None if view == "top" else ground, 0.24 * k, (340, 1150) if view == "front" else None)
        px[..., 3] = mask
        px[~mask, :3] = 1.0
        out = warp(view, px)
        out[..., 3] = (out[..., 3] > 0.5).astype(np.float32)
        save(os.path.join(bp.REGISTERED, f"{view}.png"), out)
        save(os.path.join(bp.REGISTERED, f"{view}_grid.png"), grid_overlay(view, out))
        report[view] = dict(car_px=int(out[..., 3].sum()))
        print(view, report[view], flush=True)
    json.dump(dict(landmarks=LANDMARKS, report=report), open(os.path.join(bp.REGISTERED, "warp.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
