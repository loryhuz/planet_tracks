"""The livery of the ice planet's car: glacier blue, white blades, graphite trim and the 07, as on
the validated concept (art/ice_car/concepts/monoplace), drawn as clean polygons into one texture,
crates/app/assets/skicar_livery.png, that the game multiplies over the white of the `livery`
material.

Same scheme as the Mars buggy's (tools/blender/buggy_livery.py, whose painting it reuses): an atlas
of five parallel projections of the car at 512 texels per metre, the left side, the right side,
the top, the front and the back; a face takes its UVs from the projection that sees it most
squarely. The regions are taller and reach further forward than the buggy's: the car is longer
(its nose at z 2.12) and its flag flies at 1.9 m.
"""

import math

import bpy
import numpy as np
from mathutils import Vector

import buggy_livery as bl
from meshkit import GROUND_Y

S = bl.S  # texels per metre
W, HT = 4096, 2048
REGIONS = {
    "left": dict(at=(0, 0), size=(2048, 1024), corner=(None, 2.0, 2.25), right=(0, 0, -1), down=(0, -1, 0)),
    "right": dict(at=(0, 1024), size=(2048, 1024), corner=(None, 2.0, -1.75), right=(0, 0, 1), down=(0, -1, 0)),
    "top": dict(at=(2048, 0), size=(2048, 1024), corner=(1.0, None, -1.75), right=(0, 0, 1), down=(-1, 0, 0)),
    "front": dict(at=(2048, 1024), size=(1024, 1024), corner=(-1.0, 2.0, None), right=(1, 0, 0), down=(0, -1, 0)),
    "back": dict(at=(3072, 1024), size=(1024, 1024), corner=(1.0, 2.0, None), right=(-1, 0, 0), down=(0, -1, 0)),
}

# Glacier blue, lighter than cobalt (the game's paint shading darkens it).
GLACIER = (0.14, 0.60, 0.94)
ICE = (0.55, 0.86, 1.0)
WHITE = (0.95, 0.96, 0.97)
GRAPHITE = (0.15, 0.17, 0.19)
NAVY = (0.04, 0.11, 0.24)
ORANGE = (1.0, 0.36, 0.07)


def texel(region, p):
    """Car-frame point (x, y, z) to (column, row) in the atlas."""
    r = REGIONS[region]
    q = (p[0], p[1] - GROUND_Y, p[2])
    d = [q[i] - (c if c is not None else 0.0) for i, c in enumerate(r["corner"])]
    col = sum(a * b for a, b in zip(r["right"], d)) * S
    row = sum(a * b for a, b in zip(r["down"], d)) * S
    return r["at"][0] + col, r["at"][1] + row


def assign_uvs(bm, uv_layer, to_car, mat_names):
    """UVs of the faces painted by the atlas: each from the projection that sees it best."""
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
        reg = bl.region_of(n.normalized())
        for lp, p in zip(f.loops, pts):
            c, r = texel(reg, p)
            lp[uv_layer].uv = (c / W, 1.0 - r / HT)


class Canvas(bl.Canvas):
    """The buggy's canvas on this car's regions."""

    def __init__(self, region, base=GLACIER):
        r = REGIONS[region]
        self.region = region
        self.w, self.h = r["size"]
        self.img = np.empty((self.h, self.w, 3), np.float32)
        self.img[:] = base
        self.r = r


# --- The 07: heavy squared digits after the concept's, as normalised rings (unit height, base
# centre at the origin, x in the reading direction, y up).
DIGIT_W, STROKE, GAP = 0.62, 0.21, 0.12


def zero(x0):
    w, t = DIGIT_W, STROKE
    outer = bl.rounded([(x0, 0), (x0 + w, 0), (x0 + w, 1), (x0, 1)], {0: 0.14, 1: 0.14, 2: 0.14, 3: 0.14})
    inner = bl.rounded([(x0 + t, t), (x0 + w - t, t), (x0 + w - t, 1 - t), (x0 + t, 1 - t)], {0: 0.04, 1: 0.04, 2: 0.04, 3: 0.04})
    return [outer, inner]


def seven(x0):
    w, t = DIGIT_W, STROKE
    return [[(x0, 1), (x0 + w, 1), (x0 + w, 1 - t * 1.05), (x0 + 0.42 * w, 0), (x0 + 0.06 * w, 0), (x0 + w - 0.38, 1 - t), (x0, 1 - t)]]


def number():
    rings = zero(0.0) + seven(DIGIT_W + GAP)
    width = 2 * DIGIT_W + GAP
    return [[(x - width / 2, y) for x, y in r] for r in rings]


def number_07(c, a, b, height, angle=0.0, colour=WHITE, shadow=NAVY):
    """The 07 with a drop shadow (down and back in the reading direction)."""
    rings = number()
    off = 0.045 * height
    t = math.radians(angle)
    # Shadow offset in drawing coordinates: along the reading direction and down, as the image is
    # seen: the shape() frame's x and -y.
    if shadow is not None:
        sx, sy = c.px(a, b)
        dx, dy = (math.cos(t) + math.sin(t)) * off * S, (-math.sin(t) + math.cos(t)) * off * S
        cc = c.coords(np.array([sx + dx]), np.array([sy + dy]))
        c.shape(rings, float(cc[0][0]), float(cc[1][0]), height, shadow, angle=angle)
    c.shape(rings, a, b, height, colour, angle=angle)


def snowflake(c, a, b, r, colour):
    """A six-armed snowflake of radius r (drawing coordinates), each arm with two pairs of
    branches."""
    for k in range(6):
        t = math.pi / 2 + k * math.pi / 3
        d = np.array([math.cos(t), math.sin(t)])
        n = np.array([-d[1], d[0]])
        o = np.array([a, b])
        c.poly(bl.tube_band(tuple(o), tuple(o + d * r), r * 0.16), colour)
        for f, L in ((0.45, 0.34), (0.72, 0.24)):
            base = o + d * r * f
            for s in (1, -1):
                tip = base + (d * 0.55 + n * s * 0.85) * r * L
                c.poly(bl.tube_band(tuple(base), tuple(tip), r * 0.12), colour)
    c.poly([(a + r * 0.2 * math.cos(t), b + r * 0.2 * math.sin(t)) for t in np.linspace(0, 2 * math.pi, 6, endpoint=False)], colour)


def flag_side(c):
    """The pennant (z, h): orange, a white snowflake in its middle."""
    c.poly([(-1.16, 1.935), (-1.16, 1.765), (-1.50, 1.855)], ORANGE)
    snowflake(c, -1.265, 1.852, 0.045, WHITE)


def side(c, region):
    """The sides (z, h)."""
    # Nose: a white blade from the tip rising back along the upper side, a graphite lip under it.
    c.poly([(2.15, 0.28), (2.15, 0.345), (1.55, 0.42), (1.45, 0.38)], GRAPHITE)
    c.poly([(2.10, 0.36), (2.02, 0.43), (1.30, 0.80), (1.10, 0.88), (1.12, 0.80), (1.40, 0.62)], WHITE)
    c.poly([(1.95, 0.405), (1.70, 0.52), (1.62, 0.50)], ICE)
    # Pods: graphite along the floor and in a triangle at the lower front, a white blade sweeping
    # back under the number, a graphite wedge at the rear.
    c.poly([(1.10, 0.25), (-0.60, 0.25), (-0.60, 0.36), (1.10, 0.36)], GRAPHITE)
    c.poly([(0.84, 0.36), (0.40, 0.36), (0.86, 0.66)], GRAPHITE)
    c.poly([(0.90, 0.55), (0.98, 0.70), (0.30, 0.52), (-0.32, 0.36), (-0.05, 0.36), (0.40, 0.46)], WHITE)
    c.poly([(-0.10, 0.36), (-0.60, 0.36), (-0.60, 0.70), (-0.32, 0.70)], GRAPHITE)
    c.poly([(-0.20, 0.70), (-0.60, 0.70), (-0.60, 1.0), (-0.38, 1.0)], WHITE)
    # Ice-blue line along the shoulder.
    c.poly([(1.05, 0.875), (-0.45, 0.875), (-0.45, 0.89), (1.05, 0.89)], ICE)
    number_07(c, 0.36, 0.56, 0.26)
    # Rear deck: white over its front half with a small 07, a graphite lip along its foot.
    c.poly([(-0.30, 0.98), (-0.66, 0.98), (-0.62, 1.20), (-0.30, 1.20)], WHITE)
    c.poly([(-0.30, 0.84), (-1.40, 0.84), (-1.40, 0.90), (-0.30, 0.90)], GRAPHITE)
    number_07(c, -0.48, 1.005, 0.075, colour=GRAPHITE, shadow=None)
    # The pods' rear intakes' honeycomb (livery_mesh faces).
    bl.honeycomb_poly(c, [(-0.26, 0.76), (0.02, 0.76), (-0.02, 0.86), (-0.30, 0.86)])
    side_finish(c)
    flag_side(c)


def side_finish(c):
    """Seams between the panels and the bolts that hold them (z, h)."""
    # The pods' front panel, behind the raked edge; the seam over the floor's graphite band.
    bl.seam(c, (0.84, 0.36), (1.00, 0.84), bolts=0.12, inset=-0.014)
    bl.seam(c, (0.80, 0.362), (-0.26, 0.362), bolts=0.14, inset=0.014)
    # The door-like panel under the number, and its rear edge.
    bl.seam(c, (0.02, 0.37), (0.02, 0.50))
    bl.seam(c, (-0.24, 0.37), (-0.40, 0.88), bolts=0.13, inset=0.014)
    # The rear deck: its foot, the edge of its white panel.
    bl.seam(c, (-0.34, 0.905), (-1.28, 0.905), bolts=0.15, inset=0.012)
    bl.seam(c, (-0.66, 0.92), (-0.62, 1.13))
    # The nose: a seam behind its white blade, bolts along its lip.
    bl.seam(c, (1.98, 0.40), (1.60, 0.47))
    for z in np.linspace(1.62, 2.02, 4):
        bl.bolt(c, z, 0.335 + 0.06 * (2.05 - z) / 0.45)


def top(c):
    """From above (z, x)."""
    for s in (1, -1):
        # Nose: white blades along its edges, a graphite chevron pointing forward.
        c.poly([(2.15, s * 0.0), (1.98, s * 0.06), (1.20, s * 0.34), (1.05, s * 0.40), (1.30, s * 0.24), (1.95, s * 0.02)], WHITE)
        c.poly([(1.62, s * 0.0), (1.47, s * 0.13), (1.40, s * 0.13), (1.53, s * 0.0)], GRAPHITE)
        # The shoulders over the front suspension: a white facet along their outer edge, a
        # graphite wedge behind it.
        c.poly([(1.56, s * 0.34), (1.30, s * 0.52), (1.00, s * 0.60), (0.96, s * 0.55), (1.25, s * 0.47)], WHITE)
        c.poly([(1.10, s * 0.50), (0.96, s * 0.53), (0.96, s * 0.42)], GRAPHITE)
        # Pods' tops: white angular panels, an ice-blue line along the shoulder.
        c.poly([(0.95, s * 0.47), (0.35, s * 0.56), (-0.10, s * 0.62), (-0.30, s * 0.55), (0.30, s * 0.47)], WHITE)
        c.poly([(1.05, s * 0.585), (-0.45, s * 0.585), (-0.45, s * 0.60), (1.05, s * 0.60)], ICE)
        # Deck: white beside the seat, its edges graphite toward the tail.
        c.poly([(-0.30, s * 0.20), (-0.64, s * 0.24), (-0.64, s * 0.50), (-0.30, s * 0.50)], WHITE)
    number_07(c, 1.62, 0.0, 0.30, angle=90.0)
    number_07(c, -0.47, 0.0, 0.11, angle=-90.0, colour=GRAPHITE, shadow=None)
    # Seams and bolts: along the pods' shoulders, round the cockpit, across the deck.
    for s in (1, -1):
        bl.seam(c, (1.00, s * 0.52), (-0.42, s * 0.54), bolts=0.15, inset=-s * 0.014)
        bl.seam(c, (0.62, s * 0.39), (-0.36, s * 0.39), bolts=0.14, inset=s * 0.014)
    bl.seam(c, (-0.66, -0.40), (-0.66, 0.40))
    bl.seam(c, (1.05, -0.36), (1.05, 0.36), bolts=0.12, inset=0.014)


def front(c):
    """From the front (x, h)."""
    for s in (1, -1):
        c.poly([(s * 0.02, 0.30), (s * 0.08, 0.30), (s * 0.34, 0.86), (s * 0.26, 0.86)], WHITE)
        # The pods' raked front faces: graphite, a honeycomb intake under the shoulder, the
        # planet's snowflake (the flag's) below it.
        c.poly([(s * 0.36, 0.25), (s * 0.70, 0.25), (s * 0.70, 0.97), (s * 0.36, 0.97)], GRAPHITE)
        lo, hi = sorted((s * 0.41, s * 0.62))
        bl.honeycomb(c, (lo, hi, 0.43, 0.64))
        snowflake(c, s * 0.50, 0.355, 0.04, WHITE)


def back(c):
    """From behind (x, h): the deck's back edge."""
    c.poly([(-0.6, 0.80), (0.6, 0.80), (0.6, 0.92), (-0.6, 0.92)], GRAPHITE)


def bake(mats, out_png):
    """Paints the atlas and saves it; shows it on the livery material in Blender."""
    atlas = np.ones((HT, W, 3), np.float32)
    atlas[:] = GLACIER
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
        print(f"livery: {name} painted", flush=True)
    img = bpy.data.images.new("skicar_livery", W, HT, alpha=False)
    rgba = np.ones((HT, W, 4), np.float32)
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
