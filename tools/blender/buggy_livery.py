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
    """UVs of the livery faces: each from the projection that sees it best."""
    if "livery" not in mat_names:
        return
    k = mat_names.index("livery")
    for f in bm.faces:
        if f.material_index != k:
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

    def rings_px(self, rings, colour):
        """Rings (lists of pixel points) filled together with the even-odd rule."""
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
        sub += (np.asarray(colour, np.float32) - sub) * cov[..., None]

    def rings(self, rings, colour):
        """Rings in drawing coordinates, even-odd."""
        self.rings_px([[self.px(a, b) for a, b in r] for r in rings], colour)

    def poly(self, pts, colour):
        self.rings([pts], colour)

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
ENDPLATE_MARK_BOX = (-1.62, -1.43, 1.37, 1.55)


def number():
    """The door's 23 as normalised rings (unit height, base centre at the origin, x to the
    left plan's image right, i.e. toward the rear, y up)."""
    side = load_trace("left")
    rings = [r for r in side["black"] if within(r, NUMBER_BOX)]
    a0 = min(bbox(r)[0] for r in rings)
    a1 = max(bbox(r)[1] for r in rings)
    b0 = min(bbox(r)[2] for r in rings)
    b1 = max(bbox(r)[3] for r in rings)
    h = b1 - b0
    # On the left side the image's right is -z.
    return [[(-(a - (a0 + a1) / 2) / h, (b - b0) / h) for a, b in r] for r in rings], (a1 - a0) / h


def side(c, region):
    """The sides (z, h): the left plan's paint on both, its number and lettering readable on
    each."""
    trace = load_trace("left")
    emblems = load_trace("emblems")
    right = region == "right"
    nz = (NUMBER_BOX[0] + NUMBER_BOX[1]) / 2

    def keep(r):
        return not (within(r, SWAN_BOX) or within(r, DOOR_MARK_BOX) or within(r, ENDPLATE_MARK_BOX))

    def fix(r):
        # On the right side the drawing is mirrored: turn the number back.
        if right and within(r, NUMBER_BOX):
            return [(2 * nz - a, b) for a, b in r]
        return r

    c.rings([fix(r) for r in trace["black"] if keep(r)], BLACK)
    c.rings([r for r in trace["orange"] if bbox(r)[3] > 0.70], ORANGE)  # not the wheels' rings
    # Emblems, drawn the same way round on both sides; the swan faces forward on each.
    c.shape(emblems["swan"], 0.124, 1.123, 0.147, BLACK, mirror=right)
    c.shape(emblems["aurora"], 0.203, 0.893, 0.107, BLACK, mirror=False)
    c.shape(emblems["aurora"], -1.524, 1.392, 0.138, WHITE, mirror=False)
    # Lettering from the 3/4 view.
    c.text("AURORA", 0.575, 0.99, 0.032, BLACK)
    c.text("CIRCUIT", 0.575, 0.953, 0.018, BLACK, spacing=1.25)
    c.text("Kestrel", -0.13, 0.905, 0.032, BLACK)
    c.text("CIRCUIT", -0.13, 0.868, 0.018, BLACK, spacing=1.25)


def top(c):
    """From above (z, x): the top plan's paint. Its nose is redrawn after the 3/4 view and the
    front plan: the 23 on white on the hood's front slope, readable from the front."""
    trace = load_trace("top")
    c.rings(trace["black"], BLACK)
    c.rings(trace["orange"], ORANGE)
    # Behind the cabin the top plan is shorter than the side: its rear fenders are painted here
    # after the side plan instead.
    c.poly([(-2.0, -1.0), (-0.58, -1.0), (-0.58, 1.0), (-2.0, 1.0)], BLACK)
    for s in (1, -1):
        c.poly([(-1.28, s * 0.29), (-0.84, s * 0.29), (-0.76, s * 0.45), (-0.84, s * 0.60), (-1.28, s * 0.60)], WHITE)
        c.poly([(-1.24, s * 0.50), (-0.88, s * 0.50), (-0.84, s * 0.555), (-1.24, s * 0.555)], ORANGE)
        c.poly([(-0.86, s * 0.29), (-0.80, s * 0.29), (-0.76, s * 0.45), (-0.80, s * 0.60), (-0.86, s * 0.60)], BLACK)
    # The hood's front slope: white, the door's 23 on it, digits' tops toward the windshield.
    c.poly([(1.27, -0.16), (1.73, -0.16), (1.73, 0.16), (1.27, 0.16)], WHITE)
    rings, aspect = number()
    c.shape(rings, 1.708, 0.0, 0.19, BLACK, angle=90.0)


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
    c.poly([(-0.16, 0.69), (0.16, 0.69), (0.16, 0.856), (-0.16, 0.856)], BLACK)
    c.shape(emblems["aurora"], 0.0, 0.79, 0.055, WHITE)
    c.text("AURORA", 0.0, 0.768, 0.02, WHITE, spacing=1.05)
    c.text("CIRCUIT", 0.0, 0.740, 0.012, WHITE, spacing=1.35)


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
    mat = mats["livery"]
    nt = mat.node_tree
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    nt.links.new(tex.outputs["Color"], nt.nodes["Principled BSDF"].inputs["Base Color"])
    return img
