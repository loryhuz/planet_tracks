"""The livery of buggy "B": the plans' paint (white, big black and orange diagonals, the 23 on the
doors and the hood, the swan on the cabin, the Aurora "A" marks) without its dirt, painted into one
texture, crates/app/assets/buggy_livery.png, that the game multiplies over the white of the
`livery` material.

The texture is an atlas of five parallel projections of the car at 512 texels per metre: the left
side (seen from +X), the right side, the top, the front and the back. A face takes its UVs from the
projection that sees it most squarely (`assign_uvs`), and each projection is painted as a flat
drawing in the car's own coordinates (metres): (z, height) on the sides, (z, x) on top, (x,
height) at the front and back.

The shapes are the plans' own, traced into clean polygons by tools/blender/trace_livery.py
(art/buggy/v2/livery/<view>.json): both sides use the left plan's (the right one mirrors it, its
number turned back to read the right way), the top the top plan's; the emblems (swan, Aurora
mark) were traced finer from their clearest instance, the numbers reuse the door's 23. Lettering
from the validated 3/4 view (AURORA CIRCUIT, Kestrel CIRCUIT) is set in a font. Polygons are
rasterised with 4x4 supersampling: crisp edges, no dirt.
"""

import json
import math
import os

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from meshkit import GROUND_Y

S = 512.0  # texels per metre
W, HT = 4096, 2048
SS = 4  # supersampling per axis
# Region: origin in the atlas (column, row from the top), size, the car-frame coordinates of the
# region's top-left corner and the car axes along the image's right and down (heights for y).
REGIONS = {
    "left": dict(at=(0, 0), size=(2048, 870), corner=(None, 1.70, 2.0), right=(0, 0, -1), down=(0, -1, 0)),
    "right": dict(at=(0, 880), size=(2048, 870), corner=(None, 1.70, -2.0), right=(0, 0, 1), down=(0, -1, 0)),
    "top": dict(at=(2048, 0), size=(2048, 1024), corner=(1.0, None, -2.0), right=(0, 0, 1), down=(-1, 0, 0)),
    "front": dict(at=(2048, 1034), size=(1024, 870), corner=(-1.0, 1.70, None), right=(1, 0, 0), down=(0, -1, 0)),
    "back": dict(at=(3072, 1034), size=(1024, 870), corner=(1.0, 1.70, None), right=(-1, 0, 0), down=(0, -1, 0)),
}
FONTS = ["/Library/Fonts/SF-Compact-Display-Black.otf", "/System/Library/Fonts/Supplemental/Arial Black.ttf"]
FONTS_TEXT = ["/Library/Fonts/SF-Compact-Display-Heavy.otf", "/System/Library/Fonts/Supplemental/Arial Bold.ttf"]


def region_of(n):
    """The projection that sees a face of normal `n` (car frame) most squarely. The sides win
    over the top for the pods' sloping shoulders."""
    ax, ay, az = abs(n.x), abs(n.y), abs(n.z)
    if ax >= 0.75 * ay and ax >= 0.9 * az:
        return "left" if n.x > 0 else "right"
    if ay >= az:
        return "top"
    return "front" if n.z > 0 else "back"


def texel(region, p):
    """Car-frame point (x, y, z) to (column, row) in the atlas."""
    r = REGIONS[region]
    q = (p[0], p[1] - GROUND_Y, p[2])
    d = [q[i] - (c if c is not None else 0.0) for i, c in enumerate(r["corner"])]
    col = sum(a * b for a, b in zip(r["right"], d)) * S
    row = sum(a * b for a, b in zip(r["down"], d)) * S
    return r["at"][0] + col, r["at"][1] + row


def assign_uvs(bm, uv_layer, to_car, mat_names):
    """UVs of the faces painted by the atlas (materials livery, livery_glass, livery_mesh): each
    from the projection that sees it best."""
    painted = {i for i, name in enumerate(mat_names) if name.startswith("livery")}
    if not painted:
        return
    for f in bm.faces:
        if f.material_index not in painted:
            continue
        pts = [to_car(lp.vert.co) for lp in f.loops]
        n = Vector()
        for a, b in zip(pts, pts[1:] + pts[:1]):
            n += a.cross(b)
        if n.length < 1e-12:
            n = Vector((0, 1, 0))
        reg = region_of(n.normalized())
        for lp, p in zip(f.loops, pts):
            c, r = texel(reg, p)
            lp[uv_layer].uv = (c / W, 1.0 - r / HT)


# --- Rasterising. ---
WHITE = (0.93, 0.925, 0.91)
BLACK = (0.06, 0.06, 0.065)
ORANGE = (1.0, 0.42, 0.02)
TRACES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "art", "buggy", "v2", "livery")


def load_trace(name):
    with open(os.path.join(TRACES, f"{name}.json")) as f:
        return json.load(f)


class Canvas:
    """One projection's drawing: shapes in the region's car coordinates (a, b) — (z, h) on the
    sides, (z, x) on top, (x, h) at the front and back — painted at 512 texels per metre."""

    def __init__(self, region, base=WHITE):
        r = REGIONS[region]
        self.region = region
        self.w, self.h = r["size"]
        self.img = np.empty((self.h, self.w, 3), np.float32)
        self.img[:] = base
        self.r = r

    def px(self, a, b):
        """Drawing coordinates to pixel (column, row)."""
        r = self.r
        p = [0.0, 0.0, 0.0]
        if self.region in ("left", "right"):
            p[2], p[1] = a, b
        elif self.region == "top":
            p[2], p[0] = a, b
        else:
            p[0], p[1] = a, b
        d = [p[i] - (c if c is not None else 0.0) for i, c in enumerate(r["corner"])]
        col = sum(x * y for x, y in zip(r["right"], d)) * S
        row = sum(x * y for x, y in zip(r["down"], d)) * S
        return col, row

    def coords(self, cols, rows):
        """Pixel (column, row) arrays to drawing coordinates (a, b): px is affine."""
        p0 = np.array(self.px(0.0, 0.0))
        pa = np.array(self.px(1.0, 0.0)) - p0
        pb = np.array(self.px(0.0, 1.0)) - p0
        m = np.linalg.inv(np.array([[pa[0], pb[0]], [pa[1], pb[1]]]))
        dc, dr = cols - p0[0], rows - p0[1]
        return m[0, 0] * dc + m[0, 1] * dr, m[1, 0] * dc + m[1, 1] * dr

    def rings_px(self, rings, colour, alpha=1.0):
        """Rings (lists of pixel points) filled together with the even-odd rule. `colour` is an
        RGB triple or a function of the drawing coordinates (A, B arrays) returning RGB arrays."""
        rings = [np.asarray(r, float) for r in rings if len(r) >= 3]
        if not rings:
            return
        allp = np.concatenate(rings)
        x0, x1 = max(int(np.floor(allp[:, 0].min())) - 1, 0), min(int(np.ceil(allp[:, 0].max())) + 1, self.w)
        y0, y1 = max(int(np.floor(allp[:, 1].min())) - 1, 0), min(int(np.ceil(allp[:, 1].max())) + 1, self.h)
        if x1 <= x0 or y1 <= y0:
            return
        edges = np.concatenate([np.c_[r, np.roll(r, -1, axis=0)] for r in rings])  # ax ay bx by
        off = (np.arange(SS) + 0.5) / SS
        sx = (np.arange(x0, x1)[:, None] + off[None, :]).ravel()
        cov = np.zeros((y1 - y0, x1 - x0), np.float32)
        for k in range(SS):
            ys = np.arange(y0, y1) + off[k]
            inside = np.zeros((len(ys), len(sx)), bool)
            for ax, ay, bx, by in edges:
                if ay == by:
                    continue
                rows = (ys >= min(ay, by)) & (ys < max(ay, by))
                if not rows.any():
                    continue
                xi = ax + (ys[rows] - ay) * (bx - ax) / (by - ay)
                inside[rows] ^= sx[None, :] < xi[:, None]
            cov += inside.reshape(len(ys), x1 - x0, SS).mean(axis=2) / SS
        sub = self.img[y0:y1, x0:x1]
        if callable(colour):
            cc, rr = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
            col = np.asarray(colour(*self.coords(cc, rr)), np.float32)
        else:
            col = np.asarray(colour, np.float32)
        sub += (col - sub) * (cov * alpha)[..., None]

    def rings(self, rings, colour, alpha=1.0):
        """Rings in drawing coordinates, even-odd."""
        self.rings_px([[self.px(a, b) for a, b in r] for r in rings], colour, alpha)

    def poly(self, pts, colour, alpha=1.0):
        self.rings([pts], colour, alpha)

    def shape(self, rings, a, b, height, colour, mirror=False, angle=0.0):
        """Normalised rings (unit height, base centre at the origin, x to the image's right, y
        up), their base centre at (a, b), `height` metres tall, turned `angle` degrees
        counter-clockwise as the image is seen, mirrored if asked."""
        cx, cy = self.px(a, b)
        k = height * S
        t = math.radians(angle)
        ux, uy = math.cos(t), -math.sin(t)
        vx, vy = -math.sin(t), -math.cos(t)
        sgn = -1.0 if mirror else 1.0
        self.rings_px([[(cx + (sgn * x * ux + y * vx) * k, cy + (sgn * x * uy + y * vy) * k) for x, y in r] for r in rings], colour)

    def text(self, string, a, b, height, colour, fonts=FONTS_TEXT, spacing=1.0, angle=0.0):
        """Text centred on (a, b), its capitals `height` metres tall."""
        rings, (x0, x1, y0, y1) = glyphs(string, fonts, spacing)
        h = y1 - y0
        norm = [[((x - (x0 + x1) / 2) / h, (y - y0) / h) for x, y in r] for r in rings]
        t = math.radians(angle)
        # Base centre: half the height below (a, b), along the turned up direction.
        cx, cy = self.px(a, b)
        bx, by = cx - (-math.sin(t)) * height * S / 2, cy - (-math.cos(t)) * height * S / 2
        k = height * S
        ux, uy = math.cos(t), -math.sin(t)
        vx, vy = -math.sin(t), -math.cos(t)
        self.rings_px([[(bx + (x * ux + y * vx) * k, by + (x * uy + y * vy) * k) for x, y in r] for r in norm], colour)


_GLYPHS = {}


def glyphs(string, fonts, spacing):
    """Outline rings of a string (Blender's text, as curves) in font units, and their bounds."""
    key = (string, tuple(fonts), spacing)
    if key in _GLYPHS:
        return _GLYPHS[key]
    cu = bpy.data.curves.new("livery_text", "FONT")
    cu.body = string
    cu.size = 1.0
    cu.space_character = spacing
    for path in fonts:
        if os.path.exists(path):
            cu.font = bpy.data.fonts.load(path, check_existing=True)
            break
    cu.resolution_u = 4
    tmp = bpy.data.objects.new("livery_text", cu)
    bpy.context.scene.collection.objects.link(tmp)
    bpy.context.view_layer.update()
    ev = tmp.evaluated_get(bpy.context.evaluated_depsgraph_get())
    me = bpy.data.meshes.new_from_object(ev)
    bpy.data.objects.remove(tmp)
    bpy.data.curves.remove(cu)
    # The filled glyphs' boundary edges, chained into rings.
    bm = bmesh.new()
    bm.from_mesh(me)
    bpy.data.meshes.remove(me)
    boundary = [e for e in bm.edges if len(e.link_faces) == 1]
    nxt = {}
    for e in boundary:
        f = e.link_faces[0]
        for lp in f.loops:
            if lp.edge == e:
                nxt[lp.vert.index] = lp.link_loop_next.vert.index
    co = {v.index: (v.co.x, v.co.y) for v in bm.verts}
    bm.free()
    rings, seen = [], set()
    for start in nxt:
        if start in seen:
            continue
        ring, v = [], start
        while v not in seen and v in nxt:
            seen.add(v)
            ring.append(co[v])
            v = nxt[v]
        if len(ring) >= 3:
            rings.append(ring)
    xs = [p[0] for r in rings for p in r]
    ys = [p[1] for r in rings for p in r]
    out = (rings, (min(xs), max(xs), min(ys), max(ys)))
    _GLYPHS[key] = out
    return out


def bbox(ring):
    a = [p[0] for p in ring]
    b = [p[1] for p in ring]
    return min(a), max(a), min(b), max(b)


def within(ring, box):
    a0, a1, b0, b1 = bbox(ring)
    return a0 >= box[0] and a1 <= box[1] and b0 >= box[2] and b1 <= box[3]


# Boxes (a0, a1, b0, b1) on the left plan's drawing: the door's number, the emblems.
NUMBER_BOX = (-0.21, 0.17, 0.54, 0.82)
SWAN_BOX = (0.05, 0.20, 1.11, 1.28)
DOOR_MARK_BOX = (0.14, 0.27, 0.88, 1.01)
A_TRIANGLE_BOX = (0.05, 0.33, 0.82, 1.06)
DOOR_REAR_STRIPE_BOX = (-0.45, -0.08, 0.44, 0.86)
ENDPLATE_MARK_BOX = (-1.62, -1.43, 1.37, 1.55)


def rounded(pts, corners):
    """A polygon with some corners rounded: `corners` maps a point's index to its radius."""
    out = []
    n = len(pts)
    for i, p in enumerate(pts):
        r = corners.get(i)
        if not r:
            out.append(p)
            continue
        a, b = pts[i - 1], pts[(i + 1) % n]
        da = (a[0] - p[0], a[1] - p[1])
        db = (b[0] - p[0], b[1] - p[1])
        la, lb = math.hypot(*da), math.hypot(*db)
        ua, ub = (da[0] / la, da[1] / la), (db[0] / lb, db[1] / lb)
        p0 = (p[0] + ua[0] * r, p[1] + ua[1] * r)
        p1 = (p[0] + ub[0] * r, p[1] + ub[1] * r)
        for k in range(7):  # quadratic Bézier through the corner
            t = k / 6
            out.append(((1 - t) ** 2 * p0[0] + 2 * t * (1 - t) * p[0] + t * t * p1[0], (1 - t) ** 2 * p0[1] + 2 * t * (1 - t) * p[1] + t * t * p1[1]))
    return out


def chaikin(ring, iterations=2):
    """Corner-cutting smoothing of a closed ring (the swan's curves)."""
    for _ in range(iterations):
        out = []
        for (ax, ay), (bx, by) in zip(ring, ring[1:] + ring[:1]):
            out += [(0.75 * ax + 0.25 * bx, 0.75 * ay + 0.25 * by), (0.25 * ax + 0.75 * bx, 0.25 * ay + 0.75 * by)]
        ring = out
    return ring


def digits_23():
    """The 23 of the doors, redrawn as clean polygons after the left plan's (z, height): heavy
    squared digits with rounded outer corners, the steps of their bars and the 3's waist."""
    two = rounded([
        (0.160, 0.555), (0.003, 0.555), (0.003, 0.625), (0.050, 0.625), (0.050, 0.612), (0.106, 0.612),
        (0.003, 0.672), (0.003, 0.785), (0.028, 0.810), (0.128, 0.810), (0.152, 0.786), (0.152, 0.725),
        (0.100, 0.725), (0.100, 0.750), (0.050, 0.750), (0.050, 0.700), (0.155, 0.640),
    ], {7: 0.03, 8: 0.0, 10: 0.03, 16: 0.03, 0: 0.0, 1: 0.0})
    three = rounded([
        (-0.020, 0.555), (-0.178, 0.555), (-0.178, 0.668), (-0.164, 0.690), (-0.178, 0.712),
        (-0.178, 0.810), (-0.020, 0.810), (-0.020, 0.732), (-0.085, 0.732), (-0.085, 0.765),
        (-0.130, 0.765), (-0.130, 0.700), (-0.070, 0.700), (-0.070, 0.665), (-0.130, 0.665),
        (-0.130, 0.615), (-0.075, 0.615), (-0.075, 0.635), (-0.020, 0.635),
    ], {1: 0.04, 2: 0.012, 4: 0.012, 5: 0.04, 6: 0.006, 0: 0.006})
    return [two, three]


def number():
    """The 23 as normalised rings (unit height, base centre at the origin, x in the reading
    direction, y up), and its width over its height."""
    rings = digits_23()
    a0 = min(bbox(r)[0] for r in rings)
    a1 = max(bbox(r)[1] for r in rings)
    b0 = min(bbox(r)[2] for r in rings)
    b1 = max(bbox(r)[3] for r in rings)
    h = b1 - b0
    # On the left side the reading direction is -z.
    return [[(-(a - (a0 + a1) / 2) / h, (b - b0) / h) for a, b in r] for r in rings], (a1 - a0) / h


# --- Glass: smoked panes painted with what they show. The game draws the cabin's glass opaque, so
# each pane carries its look: a tint darkening toward its foot (the sky reflects in its top), a
# hint of the cabin behind it (seats, harness, roll cage, steering wheel), two soft reflection
# streaks; the specular highlight comes from the shading.
GLASS_TOP = (0.21, 0.24, 0.29)
GLASS_FOOT = (0.025, 0.03, 0.038)


def gradient(a0, b0, a1, b1, c0, c1, power=1.4):
    """Colour function: c0 at (a0, b0) to c1 at (a1, b1), along that direction."""
    d = np.array([a1 - a0, b1 - b0])
    n2 = d @ d
    c0, c1 = np.asarray(c0, np.float32), np.asarray(c1, np.float32)

    def f(A, B):
        t = np.clip(((A - a0) * d[0] + (B - b0) * d[1]) / n2, 0, 1) ** power
        return c0 + (c1 - c0) * t[..., None]

    return f


def streaks(c, pane, a_dir, width, gap, bright=(0.75, 0.8, 0.85), alpha=0.16):
    """Two parallel diagonal reflection bands across a pane (a polygon in drawing coordinates),
    `a_dir` their slope."""
    xs = [p[0] for p in pane]
    ys = [p[1] for p in pane]
    ca, cb = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    span = max(max(xs) - min(xs), max(ys) - min(ys))
    d = np.array(a_dir, float)
    d /= np.linalg.norm(d)
    nrm = np.array([-d[1], d[0]])
    for off, w in ((-gap / 2, width), (gap / 2, width * 0.5)):
        o = np.array([ca, cb]) + nrm * off
        band = [tuple(o + d * span - nrm * w / 2), tuple(o + d * span + nrm * w / 2), tuple(o - d * span + nrm * w / 2), tuple(o - d * span - nrm * w / 2)]
        clip_to(c, pane, band, bright, alpha)


def cross2(u, v):
    return u[0] * v[1] - u[1] * v[0]


def clip_to(c, pane, shape, colour, alpha):
    """Paints `shape` only inside `pane` (convex polygons in drawing coordinates;
    Sutherland-Hodgman clipping)."""
    poly = [np.array(p, float) for p in shape]
    n = len(pane)
    for i in range(n):
        a, b = np.array(pane[i], float), np.array(pane[(i + 1) % n], float)
        ref = np.sign(cross2(b - a, np.array(pane[(i + 2) % n], float) - a))
        out = []
        for j in range(len(poly)):
            p, q = poly[j], poly[(j + 1) % len(poly)]
            dp, dq = cross2(b - a, p - a) * ref, cross2(b - a, q - a) * ref
            if dp >= 0:
                out.append(p)
            if (dp >= 0) != (dq >= 0):
                out.append(p + (q - p) * (dp / (dp - dq)))
        poly = out
        if len(poly) < 3:
            return
    c.poly([tuple(p) for p in poly], colour, alpha)


def tube_band(p0, p1, w):
    """A straight band of width w from p0 to p1 (drawing coordinates)."""
    p0, p1 = np.array(p0, float), np.array(p1, float)
    d = p1 - p0
    n = np.array([-d[1], d[0]]) / np.linalg.norm(d) * w / 2
    return [tuple(p0 - n), tuple(p1 - n), tuple(p1 + n), tuple(p0 + n)]


def side_window(c, front, rear):
    """The side window seen from the side (z, h): front and rear are its (z, h) corners in order
    front-top, front-bottom, rear-bottom, rear-top."""
    pane = [front[0], front[1], rear[0], rear[1]]
    top_h = max(p[1] for p in pane)
    foot_h = min(p[1] for p in pane)
    c.poly(pane, gradient(0, top_h, 0, foot_h, GLASS_TOP, GLASS_FOOT))
    # The cabin behind: the seat's back and headrest, the harness, a roll-cage tube.
    seat = [(-0.08, foot_h), (-0.30, foot_h), (-0.30, 1.30), (-0.25, 1.345), (-0.13, 1.345), (-0.08, 1.30)]
    clip_to(c, pane, seat, (0.01, 0.01, 0.012), 0.55)
    for z0 in (-0.15, -0.23):
        clip_to(c, pane, tube_band((z0, foot_h), (z0 + 0.02, 1.33), 0.022), (0.85, 0.35, 0.05), 0.35)
    clip_to(c, pane, tube_band((0.05, 1.20), (-0.45, 1.36), 0.035), (0.0, 0.0, 0.0), 0.45)
    streaks(c, pane, (1.0, 0.55), 0.07, 0.11)


def windshield_top(c):
    """The windshield seen from above (z, x): its pane between z 0.24 and 0.625."""
    pane = [(0.24, -0.30), (0.24, 0.30), (0.625, 0.30), (0.625, -0.30)]
    c.poly(pane, gradient(0.24, 0, 0.625, 0, GLASS_TOP, GLASS_FOOT, power=1.0))
    # Dashboard along the foot, the steering wheel and the seats' tops behind.
    clip_to(c, pane, [(0.56, -0.30), (0.56, 0.30), (0.625, 0.30), (0.625, -0.30)], (0.0, 0.0, 0.0), 0.5)
    for x, r in ((0.17, 0.065),):
        ring = [(0.53 + r * np.cos(t), x + r * 1.4 * np.sin(t)) for t in np.linspace(0, 2 * np.pi, 24, endpoint=False)]
        hole = [(0.53 + (r - 0.012) * np.cos(t), x + (r - 0.012) * 1.4 * np.sin(t)) for t in np.linspace(0, 2 * np.pi, 24, endpoint=False)]
        c.rings([ring, hole], (0.0, 0.0, 0.0), 0.6)
    for x in (-0.17, 0.17):
        clip_to(c, pane, [(0.24, x - 0.10), (0.24, x + 0.10), (0.36, x + 0.08), (0.36, x - 0.08)], (0.01, 0.01, 0.012), 0.5)
        for dx in (-0.045, 0.045):
            clip_to(c, pane, tube_band((0.24, x + dx), (0.38, x + dx * 0.6), 0.016), (0.85, 0.35, 0.05), 0.35)
    streaks(c, pane, (0.35, 1.0), 0.06, 0.12)


def quarter_window(c, pane):
    c.poly(pane, gradient(0, max(p[1] for p in pane), 0, min(p[1] for p in pane), GLASS_TOP, GLASS_FOOT))
    streaks(c, pane, (1.0, 0.8), 0.03, 0.05)


def honeycomb(c, rect, **kw):
    """A hexagonal mesh in a rectangle (a0, a1, b0, b1) of drawing coordinates."""
    a0, a1, b0, b1 = rect
    honeycomb_poly(c, [(a0, b0), (a1, b0), (a1, b1), (a0, b1)], **kw)


def honeycomb_poly(c, pts, cell=0.011, frame=(0.20, 0.20, 0.21), hole=(0.025, 0.025, 0.028)):
    """A hexagonal mesh in a polygon of drawing coordinates."""
    s3 = np.sqrt(3.0)

    def f(A, B):
        # Axial coordinates of a pointy-top hex lattice of circumradius `cell`.
        q = (s3 / 3 * A - B / 3) / cell
        r = (2 / 3 * B) / cell
        x, z = q, r
        y = -x - z
        rx, ry, rz = np.round(x), np.round(y), np.round(z)
        dx, dy, dz = np.abs(rx - x), np.abs(ry - y), np.abs(rz - z)
        fix_x = (dx > dy) & (dx > dz)
        fix_y = ~fix_x & (dy > dz)
        rx = np.where(fix_x, -ry - rz, rx)
        ry = np.where(fix_y, -rx - rz, ry)
        rz = np.where(~fix_x & ~fix_y, -rx - ry, rz)
        d = np.maximum(np.maximum(np.abs(rx - x), np.abs(ry - y)), np.abs(rz - z))  # 0 centre, 0.5 edge
        inner = d < 0.36
        return np.where(inner[..., None], np.asarray(hole, np.float32), np.asarray(frame, np.float32))

    c.poly(pts, f)


# --- Panel finish: seams between the panels, and the bolts that hold them, as on the plans.
SEAM = (0.30, 0.30, 0.31)
BOLT = (0.62, 0.62, 0.64)
BOLT_RIM = (0.18, 0.18, 0.19)


def disc(c, a, b, r, colour):
    c.poly([(a + r * math.cos(t), b + r * math.sin(t)) for t in np.linspace(0, 2 * math.pi, 14, endpoint=False)], colour)


def bolt(c, a, b, r=0.0065):
    disc(c, a, b, r, BOLT_RIM)
    disc(c, a, b, r * 0.62, BOLT)


def seam(c, p0, p1, width=0.0035, bolts=0.0, inset=0.012):
    """A thin seam from p0 to p1, with bolts every `bolts` metres along it, `inset` beside it."""
    c.poly(tube_band(p0, p1, width), SEAM)
    if bolts:
        p0, p1 = np.array(p0, float), np.array(p1, float)
        d = p1 - p0
        length = np.linalg.norm(d)
        n = np.array([-d[1], d[0]]) / length
        k = max(1, int(length / bolts))
        for i in range(k + 1):
            q = p0 + d * (0.04 / length + (1 - 0.08 / length) * i / k) + n * inset
            bolt(c, *q)


def side_finish(c):
    """Seams and bolts on the sides (z, h)."""
    # Door: its front and rear edges, the crease along its top.
    seam(c, (0.60, 0.47), (0.60, 0.835), bolts=0.12, inset=-0.012)
    seam(c, (-0.33, 0.47), (-0.36, 0.835), bolts=0.12)
    seam(c, (-0.36, 0.836), (0.60, 0.836), bolts=0.16, inset=-0.014)
    # Swan panel: bolts at its corners, its seams.
    for a, b in ((0.03, 1.14), (0.27, 1.14), (0.03, 1.31), (0.22, 1.31)):
        bolt(c, a, b, 0.0075)
    seam(c, (0.005, 1.12), (0.005, 1.33))
    # Rear fender and front blade: a row of bolts along their lower edge.
    for z in np.linspace(-0.82, -1.22, 5):
        bolt(c, z, 1.005)
    for z in np.linspace(0.80, 1.40, 6):
        bolt(c, z, 0.99 - 0.07 * max(0.0, (z - 1.0) / 0.5))


def side(c, region):
    """The sides (z, h): the left plan's paint on both, its number and lettering readable on
    each."""
    trace = load_trace("left")
    emblems = load_trace("emblems")
    swan = [chaikin([tuple(p) for p in r]) for r in emblems["swan"]]
    right = region == "right"
    def keep(r):
        return not any(within(r, box) for box in (SWAN_BOX, DOOR_MARK_BOX, ENDPLATE_MARK_BOX, NUMBER_BOX))

    c.rings([r for r in trace["black"] if keep(r)], BLACK)
    # The 23, readable on each side (the right side's drawing is the left's, mirrored).
    rings, aspect = number()
    c.shape(rings, -0.009, 0.555, 0.255, BLACK)
    # Orange, but the wheels' rings and the two shapes redrawn by hand below (frayed in the plan).
    redrawn = (A_TRIANGLE_BOX, DOOR_REAR_STRIPE_BOX)
    c.rings([r for r in trace["orange"] if bbox(r)[3] > 0.70 and not any(within(r, b) for b in redrawn)], ORANGE)
    # The orange triangle under the door's Aurora mark, the stripe along the door's rear edge.
    c.poly([(0.07, 0.836), (0.31, 0.836), (0.198, 1.04)], ORANGE)
    c.poly([(-0.148, 0.455), (-0.198, 0.455), (-0.42, 0.845), (-0.35, 0.845)], ORANGE)
    # Emblems, drawn the same way round on both sides (as on the right plan).
    c.shape(swan, 0.124, 1.123, 0.147, BLACK)  # a decal: the same way round on both sides
    c.shape(emblems["aurora"], 0.195, 0.872, 0.128, BLACK, mirror=False)
    c.shape(emblems["aurora"], -1.524, 1.392, 0.138, WHITE, mirror=False)
    # Glass: the side window and the quarter glass ahead of the swan panel.
    side_window(c, [(0.0, 1.325), (0.0, 1.135)], [(-0.455, 1.135), (-0.385, 1.325)])
    quarter_window(c, [(0.60, 1.11), (0.40, 1.11), (0.40, 1.30), (0.42, 1.30)])
    side_finish(c)
    # Lettering from the 3/4 view.
    c.text("AURORA", 0.575, 0.99, 0.032, BLACK)
    c.text("CIRCUIT", 0.575, 0.953, 0.018, BLACK, spacing=1.25)
    c.text("Kestrel", -0.13, 0.905, 0.032, BLACK)
    c.text("CIRCUIT", -0.13, 0.868, 0.018, BLACK, spacing=1.25)


def top(c):
    """From above (z, x): the top plan's paint. Its nose is redrawn after the 3/4 view and the
    front plan: the 23 on white on the hood's front slope, readable from the front."""
    trace = load_trace("top")
    number_box = (1.15, 1.52, -0.22, 0.22)  # the plan's (dirty) 23, redrawn below
    c.rings([r for r in trace["black"] if not within(r, number_box)], BLACK)
    c.rings(trace["orange"], ORANGE)
    # Behind the cabin the top plan is shorter than the side: its rear fenders are painted here
    # after the side plan instead.
    c.poly([(-2.0, -1.0), (-0.58, -1.0), (-0.58, 1.0), (-2.0, 1.0)], BLACK)
    for s in (1, -1):
        c.poly([(-1.28, s * 0.29), (-0.84, s * 0.29), (-0.76, s * 0.45), (-0.84, s * 0.60), (-1.28, s * 0.60)], WHITE)
        c.poly([(-1.24, s * 0.50), (-0.88, s * 0.50), (-0.84, s * 0.555), (-1.24, s * 0.555)], ORANGE)
        c.poly([(-0.86, s * 0.29), (-0.80, s * 0.29), (-0.76, s * 0.45), (-0.80, s * 0.60), (-0.86, s * 0.60)], BLACK)
    # The cabin's roof: white over its rounded side edges (as on the 3/4 view; from above the
    # plan shows the side glass there, black).
    c.poly([(-0.40, -0.385), (0.245, -0.385), (0.245, 0.385), (-0.40, 0.385)], WHITE)
    windshield_top(c)
    # The hood's honeycomb grilles (on livery_mesh faces: hood scoop and fender vents).
    import buggy_body as bb

    for (x0, za), (x1, zb) in bb.HOOD_GRILLES:
        for s in (1, -1):
            honeycomb_poly(c, [(za + 0.02, s * x0 * 0.90), (za + 0.02, s * x1 * 0.90), (zb - 0.02, s * x1 * 0.90), (zb - 0.02, s * x0 * 0.90)])
    for vent in bb.FENDER_VENTS:
        for s in (1, -1):
            honeycomb_poly(c, [(z, s * x) for x, z in vent])
    # The hood's number panel ahead of the scoop: white, its front edge rounded, the door's 23 on
    # it, the digits' tops toward the windshield (it reads from the front).
    c.poly([(1.17, -0.19), (1.47, -0.19), (1.50, -0.12), (1.512, 0.0), (1.50, 0.12), (1.47, 0.19), (1.17, 0.19)], WHITE)
    rings, aspect = number()
    c.shape(rings, 1.47, 0.0, 0.25, BLACK, angle=90.0)


def front(c):
    """From the front (x, h): the front plan's paint around the black plate with the Aurora
    mark."""
    trace = load_trace("front")
    plate = (-0.18, 0.18, 0.66, 0.99)
    c.rings([r for r in trace["black"] if not within(r, plate)], BLACK)
    c.rings([r for r in trace["orange"] if not within(r, plate)], ORANGE)
    emblems = load_trace("emblems")
    # Cheeks beside the plate: white, an orange slash rising outward.
    for s in (1, -1):
        c.poly([(s * 0.15, 0.68), (s * 0.27, 0.68), (s * 0.27, 0.88), (s * 0.15, 0.88)], WHITE)
        c.poly([(s * 0.165, 0.70), (s * 0.20, 0.70), (s * 0.245, 0.86), (s * 0.21, 0.86)], ORANGE)
    c.poly([(-0.16, 0.69), (0.16, 0.69), (0.16, 0.878), (-0.16, 0.878)], BLACK)
    c.shape(emblems["aurora"], 0.0, 0.79, 0.055, WHITE)
    c.text("AURORA", 0.0, 0.768, 0.02, WHITE, spacing=1.05)
    c.text("CIRCUIT", 0.0, 0.740, 0.012, WHITE, spacing=1.35)
    # The skid box's two honeycomb grilles.
    honeycomb(c, (-0.11, 0.11, 0.335, 0.48))
    honeycomb(c, (-0.11, 0.11, 0.50, 0.635))


def back(c):
    """From behind (x, h): the hexagonal panel black with the Aurora mark in white, the rear
    fenders' ends white with an orange band."""
    emblems = load_trace("emblems")
    c.poly([(-0.30, 0.70), (0.30, 0.70), (0.30, 1.13), (-0.30, 1.13)], BLACK)
    c.shape(emblems["aurora"], 0.0, 1.005, 0.085, WHITE)
    for s in (1, -1):
        c.poly([(s * 0.44, 1.095), (s * 0.60, 1.095), (s * 0.60, 1.125), (s * 0.44, 1.125)], ORANGE)
    # Endplates' inner faces (seen from behind past the wing): black.
    for s in (1, -1):
        c.poly([(s * 0.75, 1.30), (s * 0.82, 1.30), (s * 0.82, 1.60), (s * 0.75, 1.60)], BLACK)


def bake(body_objects, mats, out_png):
    """Paints the atlas and saves it; shows it on the livery material in Blender."""
    atlas = np.ones((HT, W, 3), np.float32)
    atlas[:] = WHITE
    for name, r in REGIONS.items():
        c = Canvas(name)
        if name in ("left", "right"):
            side(c, name)
        elif name == "top":
            top(c)
        elif name == "front":
            front(c)
        else:
            back(c)
        x0, y0 = r["at"]
        atlas[y0 : y0 + c.h, x0 : x0 + c.w] = c.img
        # Bleed the region's border into the gutter so mipmaps don't mix regions.
        atlas[y0 + c.h : y0 + c.h + 5, x0 : x0 + c.w] = c.img[-1:]
        print(f"livery: {name} painted", flush=True)
    img = bpy.data.images.new("buggy_livery", W, HT, alpha=False)
    rgba = np.ones((HT, W, 4), np.float32)
    # A byte image: its pixels are the sRGB values themselves.
    rgba[..., :3] = np.clip(atlas, 0, 1)
    img.pixels[:] = rgba[::-1].ravel()
    img.filepath_raw = out_png
    img.file_format = "PNG"
    img.save()
    for name, mat in mats.items():
        if not name.startswith("livery"):
            continue
        nt = mat.node_tree
        tex = nt.nodes.new("ShaderNodeTexImage")
        tex.image = img
        nt.links.new(tex.outputs["Color"], nt.nodes["Principled BSDF"].inputs["Base Color"])
    return img
