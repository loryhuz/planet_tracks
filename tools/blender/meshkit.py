"""Mesh building blocks for the buggy scripts, in the game's car frame (metres, +X left, +Y up,
+Z forward, origin at the centre of gravity), converted to Blender's frame (+X left, -Y forward,
+Z up) only when objects are made, so the .glb comes out in the game's frame.

Materials are named for the game's shaders (crates/app/src/car_model.rs, `material_kind`): the
prefix (paint, metal, rubber, glass, glow, wire, livery) picks the shader, the base colour is the
material's `game_color`.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

# --- Geometry shared with the physics (crates/physics/src/params.rs). ---
WHEEL_R = 0.45
TYRE_W = 0.48  # crates/app/src/car_model.rs TYRE_WIDTH
TRACK = 1.8
WHEELBASE = 2.6
CG_H = 0.05
TRAVEL = 0.5
GRAVITY = 40.0
SPRING_HZ = 2.0
REST = TRAVEL - GRAVITY / (2.0 * math.pi * SPRING_HZ) ** 2  # CarParams::rest_suspension
WHEEL_Y = -CG_H - REST  # wheel centre height at rest
GROUND_Y = WHEEL_Y - WHEEL_R
UP = REST  # bump travel from rest
DOWN = TRAVEL - REST  # droop travel from rest


def H(h):
    """Car-frame y of a height above the ground at rest."""
    return GROUND_Y + h


def P(x, h, z):
    """Car-frame point from a half-width, a height above the ground and a station."""
    return Vector((x, H(h), z))


def G(p):
    """Game car frame to Blender."""
    return Vector((p[0], -p[2], p[1]))


def srgb(r, g, b):
    def f(c):
        c /= 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    return (f(r), f(g), f(b))


MATERIALS = {
    "paint_white": dict(color=srgb(238, 236, 230), roughness=0.3),
    "paint_orange": dict(color=srgb(255, 112, 10), roughness=0.3),
    "paint_black": dict(color=srgb(16, 16, 18), roughness=0.35),
    "metal_graphite": dict(color=srgb(36, 37, 40), metallic=0.8, roughness=0.38),
    "metal_frame": dict(color=srgb(22, 22, 24), metallic=0.6, roughness=0.42),
    "metal_carbon": dict(color=srgb(20, 20, 22), metallic=0.3, roughness=0.3),
    "metal_steel": dict(color=srgb(140, 142, 146), metallic=1.0, roughness=0.3),
    "metal_chrome": dict(color=srgb(214, 216, 220), metallic=1.0, roughness=0.12),
    "metal_copper": dict(color=srgb(232, 110, 24), metallic=1.0, roughness=0.25),
    "metal_grille": dict(color=srgb(12, 12, 14), metallic=0.5, roughness=0.55),
    "rubber_tyre": dict(color=srgb(30, 28, 27), roughness=0.85),
    "rubber_black": dict(color=srgb(20, 20, 21), roughness=0.7),
    "glass": dict(color=srgb(22, 26, 32), roughness=0.03, transmission=1.0),
    "glow_white": dict(color=srgb(235, 245, 255), emission=5.0),
    "glow_red": dict(color=srgb(255, 30, 22), emission=4.0),
    "glow_amber": dict(color=srgb(255, 140, 20), emission=3.0),
    # White, multiplied in the game by the livery texture (crates/app/assets/buggy_livery.png).
    "livery": dict(color=(1.0, 1.0, 1.0), roughness=0.3),
}


def use_nodes(datablock):
    if bpy.app.version < (5, 0, 0):
        datablock.use_nodes = True


def make_materials():
    mats = {}
    for name, spec in MATERIALS.items():
        m = bpy.data.materials.new(name)
        use_nodes(m)
        b = m.node_tree.nodes["Principled BSDF"]
        col = (*spec["color"], 1.0)
        b.inputs["Base Color"].default_value = col
        b.inputs["Metallic"].default_value = spec.get("metallic", 0.0)
        b.inputs["Roughness"].default_value = spec.get("roughness", 0.5)
        if spec.get("emission"):
            b.inputs["Emission Color"].default_value = col
            b.inputs["Emission Strength"].default_value = spec["emission"]
        if spec.get("transmission"):
            # Smoked glass, opaque as the game draws it: dark, glossy, reflecting.
            b.inputs["Base Color"].default_value = (0.012, 0.014, 0.018, 1.0)
            b.inputs["Specular IOR Level"].default_value = 0.9
        m.diffuse_color = col
        m["game_color"] = list(spec["color"])
        mats[name] = m
    return mats


class Mesh:
    """Vertices and faces in the car frame; faces carry a material, a smooth flag and optional
    per-corner UVs."""

    def __init__(self):
        self.v = []
        self.f = []  # (indices, material, smooth, uvs)

    def vert(self, p):
        self.v.append(Vector(p))
        return len(self.v) - 1

    def face(self, idx, mat, smooth=False, uv=None):
        self.f.append((tuple(idx), mat, smooth, uv))
        return self

    def poly(self, pts, mat, smooth=False):
        """A face from points (counter-clockwise seen from its front)."""
        return self.face([self.vert(p) for p in pts], mat, smooth)

    def add(self, other):
        base = len(self.v)
        self.v.extend(v.copy() for v in other.v)
        for idx, mat, smooth, uv in other.f:
            self.f.append((tuple(i + base for i in idx), mat, smooth, uv))
        return self

    def mirrored(self):
        """Mirror in X (left part to right part): flips the winding back to outward."""
        m = Mesh()
        m.v = [Vector((-p.x, p.y, p.z)) for p in self.v]
        m.f = [(tuple(reversed(idx)), mat, smooth, list(reversed(uv)) if uv else None) for idx, mat, smooth, uv in self.f]
        return m

    def both(self):
        """This (left) part and its mirror image."""
        return self.add(self.mirrored())

    def moved(self, d):
        m = Mesh()
        m.v = [p + Vector(d) for p in self.v]
        m.f = list(self.f)
        return m

    def transformed(self, mat3, origin=(0, 0, 0)):
        m = Mesh()
        o = Vector(origin)
        m.v = [o + mat3 @ (p - o) for p in self.v]
        m.f = list(self.f)
        return m

    # -- surfaces --
    def grid(self, rows, mat, smooth=False, mats=None, close=False):
        """Quads between consecutive rows of points (each a list of the same length). Faces point
        to (row direction) x (column direction): rows along +Z with columns running down the left
        side (counter-clockwise seen from +X) face out. `mats(i, j)` may pick per-quad
        materials."""
        ids = [[self.vert(p) for p in row] for row in rows]
        n = len(rows[0])
        cols = n if close else n - 1
        for i in range(len(rows) - 1):
            for j in range(cols):
                k = (j + 1) % n
                q = (ids[i][j], ids[i + 1][j], ids[i + 1][k], ids[i][k])
                self.face(q, mats(i, j) if mats else mat, smooth)
        return ids

    def cap(self, ids, mat, flip=False, smooth=False):
        self.face(list(reversed(ids)) if flip else ids, mat, smooth)

    def prism(self, outline, depth, mat, side_mat=None, back_mat=None, smooth=False):
        """A plate: a planar outline (counter-clockwise seen from its front) extruded `depth` back
        along its normal."""
        pts = [Vector(p) for p in outline]
        n = Vector()
        for a, b in zip(pts, pts[1:] + pts[:1]):
            n += a.cross(b)
        n.normalize()
        front = [self.vert(p) for p in pts]
        back = [self.vert(p - n * depth) for p in pts]
        self.face(front, mat, smooth)
        self.face(list(reversed(back)), back_mat or mat, smooth)
        k = len(pts)
        for i in range(k):
            j = (i + 1) % k
            self.face((front[j], front[i], back[i], back[j]), side_mat or mat, smooth)
        return self

    def tube(self, a, b, r, mat, seg=12, caps=True, smooth=True, r2=None):
        a, b = Vector(a), Vector(b)
        axis = b - a
        if axis.length < 1e-6:
            return self
        axis.normalize()
        side = axis.cross(Vector((0, 1, 0))) if abs(axis.y) < 0.9 else axis.cross(Vector((1, 0, 0)))
        side.normalize()
        up = axis.cross(side)
        r2 = r if r2 is None else r2
        ra, rb = [], []
        for i in range(seg):
            t = 2 * math.pi * i / seg
            d = side * math.cos(t) + up * math.sin(t)
            ra.append(self.vert(a + d * r))
            rb.append(self.vert(b + d * r2))
        for i in range(seg):
            j = (i + 1) % seg
            self.face((ra[i], ra[j], rb[j], rb[i]), mat, smooth)
        if caps:
            self.face(rb, mat, False)
            self.face(list(reversed(ra)), mat, False)
        return self

    def box(self, center, half, mat, axes=None, smooth=False):
        c = Vector(center)
        ax = axes or (Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1)))
        h = half
        idx = {}
        for sx in (-1, 1):
            for sy in (-1, 1):
                for sz in (-1, 1):
                    idx[(sx, sy, sz)] = self.vert(c + ax[0] * (sx * h[0]) + ax[1] * (sy * h[1]) + ax[2] * (sz * h[2]))
        q = lambda *k: [idx[x] for x in k]  # noqa: E731
        self.face(q((1, -1, -1), (1, 1, -1), (1, 1, 1), (1, -1, 1)), mat, smooth)
        self.face(q((-1, -1, -1), (-1, -1, 1), (-1, 1, 1), (-1, 1, -1)), mat, smooth)
        self.face(q((-1, 1, -1), (-1, 1, 1), (1, 1, 1), (1, 1, -1)), mat, smooth)
        self.face(q((-1, -1, -1), (1, -1, -1), (1, -1, 1), (-1, -1, 1)), mat, smooth)
        self.face(q((-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1)), mat, smooth)
        self.face(q((-1, -1, -1), (-1, 1, -1), (1, 1, -1), (1, -1, -1)), mat, smooth)
        return self

    def sphere(self, c, r, mat, seg=12, rings=7):
        c = Vector(c)
        rows = []
        for k in range(1, rings):
            phi = math.pi * k / rings - math.pi / 2
            rows.append([self.vert(c + Vector((r * math.cos(phi) * math.cos(t), r * math.sin(phi), r * math.cos(phi) * math.sin(t)))) for t in (2 * math.pi * i / seg for i in range(seg))])
        bottom = self.vert(c + Vector((0, -r, 0)))
        top = self.vert(c + Vector((0, r, 0)))
        for k in range(len(rows) - 1):
            lo, hi = rows[k], rows[k + 1]
            for i in range(seg):
                j = (i + 1) % seg
                self.face((lo[i], hi[i], hi[j], lo[j]), mat, True)
        for i in range(seg):
            j = (i + 1) % seg
            self.face((bottom, rows[0][i], rows[0][j]), mat, True)
            self.face((top, rows[-1][j], rows[-1][i]), mat, True)
        return self

    def lathe(self, profile, mat, seg=48, smooth=True, mats=None):
        """Surface of revolution around the X axis. `profile` is a list of (x, r); faces point to
        dx·radial − dr·X, i.e. outward for a profile that climbs on the -X side, runs along the
        top and comes down on the +X side. `mats(i)` may pick a material per profile span."""
        grid = []
        for x, r in profile:
            grid.append([self.vert((x, r * math.cos(2 * math.pi * j / seg), r * math.sin(2 * math.pi * j / seg))) for j in range(seg)])
        for i in range(len(profile) - 1):
            for j in range(seg):
                k = (j + 1) % seg
                self.face((grid[i][j], grid[i][k], grid[i + 1][k], grid[i + 1][j]), mats(i) if mats else mat, smooth)
        return self

    def helix(self, a, axis, length, radius, wire, turns, mat, steps_per_turn=20, ring=8):
        axis = Vector(axis).normalized()
        e1 = axis.cross(Vector((0, 0, 1)))
        if e1.length < 1e-3:
            e1 = axis.cross(Vector((1, 0, 0)))
        e1.normalize()
        e2 = axis.cross(e1)
        n = int(turns * steps_per_turn)
        rings = []
        for k in range(n + 1):
            s = k / n
            phi = 2 * math.pi * turns * s
            c = Vector(a) + axis * (length * s) + (e1 * math.cos(phi) + e2 * math.sin(phi)) * radius
            tangent = (axis * length + (-e1 * math.sin(phi) + e2 * math.cos(phi)) * (2 * math.pi * turns * radius)).normalized()
            side = e1 * math.cos(phi) + e2 * math.sin(phi)
            side = (side - tangent * side.dot(tangent)).normalized()
            up = tangent.cross(side)
            rings.append([self.vert(c + (side * math.cos(t) + up * math.sin(t)) * wire) for t in (2 * math.pi * i / ring for i in range(ring))])
        for k in range(n):
            for i in range(ring):
                j = (i + 1) % ring
                self.face((rings[k][i], rings[k][j], rings[k + 1][j], rings[k + 1][i]), mat, True)
        self.face(rings[-1], mat, False)
        self.face(list(reversed(rings[0])), mat, False)
        return self

    def polyline(self, pts, r, mat, joints=True, seg=10):
        """A bent tube through points, with a ball at each bend."""
        for a, b in zip(pts, pts[1:]):
            self.tube(a, b, r, mat, seg=seg, caps=False)
        if joints:
            for p in pts:
                self.sphere(p, r * 1.05, mat, seg=seg, rings=5)
        return self


# --- Scene objects. ---
WORLD = {}
SHARP = math.radians(30)


def place(obj, world, parent=None):
    obj.parent = parent
    obj.matrix_parent_inverse = Matrix.Identity(4)
    obj.matrix_basis = (WORLD[parent.name].inverted() @ world) if parent else world
    WORLD[obj.name] = world.copy()


def axes_matrix(x=None, y=None, z=None):
    """Blender-space rotation whose given local axes point along the given game directions."""
    if z is not None:
        zz = G(z).normalized()
        ref = Vector((0, 0, 1)) if abs(zz.z) < 0.9 else Vector((1, 0, 0))
        xx = ref.cross(zz).normalized()
        yy = zz.cross(xx)
    elif y is not None:
        yy = G(y).normalized()
        ref = Vector((0, 0, 1)) if abs(yy.z) < 0.9 else Vector((1, 0, 0))
        xx = yy.cross(ref).normalized()
        zz = xx.cross(yy)
    else:
        return Matrix.Identity(3)
    return Matrix((xx, yy, zz)).transposed()


def to_object(name, mesh, origin, coll, mats, parent=None, rot=None, relative=False, bevel=None, uv_fn=None):
    """`mesh` is in the car frame, or relative to `origin` (still in car axes) if `relative`.
    `uv_fn(bm, uv_layer, to_car)` may fill UVs (e.g. the livery's projections)."""
    rot = rot or Matrix.Identity(3)
    o = G(origin)
    inv = rot.transposed()
    bm = bmesh.new()
    verts = [bm.verts.new(inv @ (G(p) if relative else G(p) - o)) for p in mesh.v]
    bm.verts.ensure_lookup_table()
    used = []
    has_uv = any(f[3] for f in mesh.f) or uv_fn is not None
    uv_layer = bm.loops.layers.uv.new("UVMap") if has_uv else None
    for idx, mat, smooth, uv in mesh.f:
        if len(set(idx)) < 3:
            continue
        try:
            face = bm.faces.new([verts[i] for i in idx])
        except ValueError:
            continue
        if mat not in used:
            used.append(mat)
        face.material_index = used.index(mat)
        face.smooth = smooth
        if uv_layer is not None and uv:
            for loop, (u, v) in zip(face.loops, uv):
                loop[uv_layer].uv = (u, v)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    for e in bm.edges:
        if len(e.link_faces) == 2 and e.calc_face_angle(0.0) > SHARP:
            e.smooth = False
    if uv_fn is not None:
        # Car-frame position of a vertex of this object.
        world = Matrix.Translation(o) @ rot.to_4x4()

        def to_car(co):
            w = world @ co
            return Vector((w.x, w.z, -w.y))

        uv_fn(bm, uv_layer, to_car, [mats[m].name for m in used])
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for m in used:
        me.materials.append(mats[m])
    obj = bpy.data.objects.new(name, me)
    coll.objects.link(obj)
    place(obj, Matrix.Translation(o) @ rot.to_4x4(), parent)
    if bevel:
        mod = obj.modifiers.new("bevel", "BEVEL")
        mod.width = bevel
        mod.segments = 2
        mod.limit_method = "ANGLE"
        mod.angle_limit = math.radians(35)
        mod.harden_normals = True
    return obj


def empty(name, origin, coll, parent=None, kind="PLAIN_AXES", size=0.05):
    e = bpy.data.objects.new(name, None)
    e.empty_display_type = kind
    e.empty_display_size = size
    coll.objects.link(e)
    place(e, Matrix.Translation(G(origin)), parent)
    return e
