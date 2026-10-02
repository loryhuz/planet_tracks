"""Bakes the booster arrow the app paints on booster decks (crates/app/assets/textures/
booster_albedo.png and booster_normal.png, one layer of the surface texture arrays, see
scene.wgsl's L_BOOSTER) from art/textures/src/booster.png.

The source is a top-down picture generated with Higgsfield (GPT Image 2.5): one big chevron
pointing up, stencilled in safety orange with a thick black border on the road's tarp, its paint
worn by the tyres. Only the paint is kept, the tarp under it comes from the road's own texture:

1. each pixel is told orange paint, black paint or bare tarp, on a copy blurred past the weave;
2. the paint's colour is averaged over the weave within each paint (orange never bleeds into
   black), keeping the wear, larger than the weave;
3. the chevron is made exactly symmetric about its axis;
4. it is cropped to its outline with a clear margin (no mipmap bleeds over the layer's edges),
   stretched to the square layer: the shader lays it out at its own size on the deck.

The layer holds the paint's colour in RGB (sRGB) and its coverage in A (1 = paint); its relief
is flat.

    /usr/bin/python3 tools/textures/booster.py [SRC] [OUT_DIR]

    (defaults: art/textures/src/booster.png, crates/app/assets/textures)
"""

import os
import sys

import numpy as np
from PIL import Image, ImageDraw

SIZE = 1024
# Transparent margin around the chevron in the layer, pixels.
MARGIN = 12
# The weave of the source is about this many pixels across.
WEAVE = 11


def blur(a, sigma):
    """Gaussian blur of a float image (H x W or H x W x C), edges clamped: separable, one shifted
    copy per tap."""
    r = int(np.ceil(3.0 * sigma))
    k = np.exp(-0.5 * (np.arange(-r, r + 1) / sigma) ** 2)
    k /= k.sum()
    out = a.astype(np.float32)
    for axis in (0, 1):
        pad = [(0, 0)] * out.ndim
        pad[axis] = (r, r)
        p = np.pad(out, pad, mode="edge")
        n = out.shape[axis]
        out = sum(w * np.take(p, np.arange(i, i + n), axis=axis) for i, w in enumerate(k)).astype(np.float32)
    return out


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1 / 2.4) - 0.055)


def mirrored(a, axis):
    """`a` mirrored about the column `axis` (edges clamped)."""
    x = np.clip(np.round(2.0 * axis - np.arange(a.shape[1])).astype(int), 0, a.shape[1] - 1)
    return a[:, x]


def line_fit(u, v):
    """Least-squares `v = a u + b`."""
    a, b = np.polyfit(np.asarray(u, dtype=np.float64), np.asarray(v, dtype=np.float64), 1)
    return a, b


def chevron(mask, axis):
    """The outline of the chevron `mask` covers (pixels, x right, y down), symmetric about the
    column `axis`, clockwise on screen from its tip: the slanted outer edges down to the
    shoulders, the upright sides, the flat bottoms of the arms, the inner slanted edges up to the
    notch. Fitted on its left half: the outer slanted edge (leftmost pixel of each row above the
    upright side), the upright side, the bottom, and the inner slanted edge (lowest pixel of each
    column between the arm's bottom and the axis); the two slanted edges are made parallel."""
    ys, xs = np.nonzero(mask)
    top, bottom = ys.min(), ys.max()
    rows = np.arange(top, bottom + 1)
    left = np.array([np.nonzero(mask[y, : int(axis)])[0].min() if mask[y, : int(axis)].any() else axis for y in rows], dtype=np.float64)
    side = left.min()
    upright = rows[left <= side + 2.0]
    first = upright.min()
    h = bottom - top
    sl = (rows > top + 0.04 * h) & (rows < first - 0.04 * h)
    a_out, b_out = line_fit(rows[sl], left[sl])
    l = np.median(left[(rows >= first) & (rows <= bottom - 0.04 * h)])
    cols = np.arange(int(l + 0.3 * (axis - l)), int(axis - 0.1 * (axis - l)))
    low = np.array([np.nonzero(mask[:, x])[0].max() for x in cols], dtype=np.float64)
    a_in, b_in = line_fit(low, cols)
    # Parallel slanted edges: both at the mean slope, each through its own points.
    a = 0.5 * (a_out + a_in)
    b_out = np.mean(left[sl] - a * rows[sl])
    b_in = np.mean(cols - a * low)
    y_tip = (axis - b_out) / a
    y_shoulder = (l - b_out) / a
    y_notch = (axis - b_in) / a
    x_foot = a * bottom + b_in
    half = [(axis, y_tip), (l, y_shoulder), (l, bottom), (x_foot, bottom), (axis, y_notch)]
    m = lambda p: (2.0 * axis - p[0], p[1])
    # Clockwise on screen (y down): tip, right side, notch, left side.
    return [half[0], m(half[1]), m(half[2]), m(half[3]), half[4], half[3], half[2], half[1]]


def inset(polygon, w):
    """`polygon` (clockwise on screen) with every edge moved `w` inward: the corners where the
    moved edges meet. An edge too short to survive the move (the flat bottom of an arm, between
    two sharp corners) is dropped: the neighbouring edges then meet in a point."""
    cross = lambda u, v: u[0] * v[1] - u[1] * v[0]
    lines = []
    for i in range(len(polygon)):
        p, q = np.array(polygon[i]), np.array(polygon[(i + 1) % len(polygon)])
        d = (q - p) / np.linalg.norm(q - p)
        # Clockwise with y down: the inside is to the right of the direction of travel.
        lines.append((p + np.array([-d[1], d[0]]) * w, d))
    while True:
        n = len(lines)
        corners = []
        for i in range(n):
            (p1, d1), (p2, d2) = lines[i - 1], lines[i]
            corners.append(p1 + d1 * (cross(p2 - p1, d2) / cross(d1, d2)))
        # Edge i runs from corner i to corner i + 1: reversed, it is gone.
        gone = [i for i in range(n) if np.dot(corners[(i + 1) % n] - corners[i], lines[i][1]) <= 0.0]
        if not gone:
            return [tuple(c) for c in corners]
        lines = [l for i, l in enumerate(lines) if i not in gone]


def border(cover, orange, axis):
    """Width of the black border along the upright sides: from the paint's edge to the orange's,
    pixels (median over the rows of the sides)."""
    rows = [y for y in range(cover.shape[0]) if cover[y, : int(axis)].any() and orange[y, : int(axis)].any()]
    gaps = [np.nonzero(orange[y, : int(axis)])[0].min() - np.nonzero(cover[y, : int(axis)])[0].min() for y in rows]
    return float(np.median(gaps))


def raster(polygon):
    """Coverage (L image, SIZE x SIZE) of `polygon` (layer pixels), drawn four times larger and
    reduced: exact straight edges, antialiased."""
    big = Image.new("L", (4 * SIZE, 4 * SIZE), 0)
    ImageDraw.Draw(big).polygon([(4.0 * x, 4.0 * y) for x, y in polygon], fill=255)
    return big.resize((SIZE, SIZE), Image.BOX)


def main():
    root = os.path.join(os.path.dirname(__file__), "..", "..")
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "art", "textures", "src", "booster.png")
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(root, "crates", "app", "assets", "textures")
    rgb = to_linear(np.asarray(Image.open(src).convert("RGB"), dtype=np.float32) / 255.0)

    # 1. Orange, black or tarp, past the weave.
    soft = blur(rgb, WEAVE * 0.6)
    lum = soft @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    orange = smoothstep(0.18, 0.32, soft[..., 0] - soft[..., 2]) * smoothstep(0.12, 0.2, soft[..., 0])
    black = smoothstep(0.075, 0.035, lum) * (1.0 - orange)

    # 2. Each paint's colour, averaged over the weave within it (farther out where a paint is
    # thin, so the colour runs on past its edge).
    def paint(mask):
        def at(radius):
            w = blur(mask, radius)
            return blur(rgb * mask[..., None], radius) / np.maximum(w, 1e-6)[..., None], w

        near, w = at(WEAVE)
        far, _ = at(6 * WEAVE)
        return far + (near - far) * smoothstep(0.1, 0.4, w)[..., None]

    colour_orange = paint(orange)
    colour_black = paint(black)

    # 3. The outline: the paint (orange and black) and the orange inside it, each a symmetric
    # six-sided chevron fitted to the picture, about the axis through the middle of the paint.
    cover = (orange + black) > 0.5
    ys, xs = np.nonzero(cover)
    axis = 0.5 * (xs.min() + xs.max())
    outer = chevron(cover, axis)
    inner = inset(outer, border(cover, orange > 0.5, axis))
    colour_orange = 0.5 * (colour_orange + mirrored(colour_orange, axis))
    colour_black = 0.5 * (colour_black + mirrored(colour_black, axis))

    # 4. Drawn clean over the outline's box, a clear margin around it, stretched to the layer.
    x0, x1 = min(p[0] for p in outer), max(p[0] for p in outer)
    y0, y1 = min(p[1] for p in outer), max(p[1] for p in outer)
    box = (int(np.floor(x0)), int(np.floor(y0)), int(np.ceil(x1)), int(np.ceil(y1)))
    side = SIZE - 2 * MARGIN
    to_layer = lambda p: (MARGIN + (p[0] - box[0]) / (box[2] - box[0]) * side, MARGIN + (p[1] - box[1]) / (box[3] - box[1]) * side)
    alpha = raster([to_layer(p) for p in outer])
    share = np.asarray(raster([to_layer(p) for p in inner]), dtype=np.float32) / 255.0

    def layer(colour):
        c = Image.fromarray(np.uint8(np.round(to_srgb(colour[box[1] : box[3], box[0] : box[2]]) * 255.0))).resize((side, side), Image.LANCZOS)
        full = Image.new("RGB", (SIZE, SIZE))
        full.paste(c.resize((SIZE, SIZE), Image.LANCZOS))
        full.paste(c, (MARGIN, MARGIN))
        return to_linear(np.asarray(full, dtype=np.float32) / 255.0)

    colour = layer(colour_black) + (layer(colour_orange) - layer(colour_black)) * share[..., None]
    full = Image.fromarray(np.uint8(np.round(to_srgb(colour) * 255.0)))
    r, g, b = full.split()
    Image.merge("RGBA", (r, g, b, alpha)).save(os.path.join(out, "booster_albedo.png"))
    flat = Image.new("L", (SIZE, SIZE), 128)
    Image.merge("RGB", (flat, flat, Image.new("L", (SIZE, SIZE), 0))).save(os.path.join(out, "booster_normal.png"))
    print(f"booster: chevron {box[2] - box[0]} x {box[3] - box[1]} px of the source, axis at x = {axis:.1f} ->", out)


main()
