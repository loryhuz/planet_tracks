"""Builds the ice planet's car in Blender: an open single-seater on skis (art/ice_car), its body
(tools/blender/skicar_body.py) with its livery (tools/blender/skicar_livery.py), two ski corners
in front (a snowmobile's spindle on short double wishbones, the ski on its saddle) and two
double-wishbone corners behind with studded winter tyres in snow chains, rigged like the Mars
buggy's (tools/blender/build_buggy.py), whose part names the game reads.

    blender -b -P tools/blender/build_skicar.py -- [--render DIR] [--no-livery] [--out DIR]

Writes art/ice_car/skicar.blend, crates/app/assets/skicar.glb and its livery
crates/app/assets/skicar_livery.png. The game reads the parts and their pivots from the .glb
(crates/app/src/car_model.rs): the front corners' `wheel` is the ski, carried by the upright
without spinning. The physics keeps its own track (1.8 m) for the skis' contact; the skis are
drawn closer together, under the nose's flanks as on the concept.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import meshkit  # noqa: E402

meshkit.MATERIALS.update(
    {
        "paint_blue": dict(color=meshkit.srgb(18, 104, 206), roughness=0.3),
        "paint_snow": dict(color=meshkit.srgb(232, 240, 248), roughness=0.85),
        "metal_chain": dict(color=meshkit.srgb(186, 190, 196), metallic=1.0, roughness=0.28),
        "rubber_snow": dict(color=meshkit.srgb(140, 150, 162), roughness=0.9),
        "glow_cyan": dict(color=meshkit.srgb(70, 225, 255), emission=4.0),
        # The headlights: white with a cyan tint (the game finds them as white glow at the front).
        "glow_white": dict(color=meshkit.srgb(205, 240, 255), emission=5.0),
    }
)

import skicar_body  # noqa: E402
import skicar_livery  # noqa: E402
from meshkit import (  # noqa: E402
    DOWN,
    GROUND_Y,
    REST,
    TRAVEL,
    UP,
    WHEEL_R,
    WHEEL_Y,
    WHEELBASE,
    WORLD,
    G,
    H,
    Mesh,
    axes_matrix,
    empty,
    make_materials,
    srgb,
    to_object,
    use_nodes,
)

REPO = os.path.normpath(os.path.join(HERE, "..", ".."))
BLEND_OUT = os.path.join(REPO, "art", "ice_car", "skicar.blend")
GLB_OUT = os.path.join(REPO, "crates", "app", "assets", "skicar.glb")
LIVERY_OUT = os.path.join(REPO, "crates", "app", "assets", "skicar_livery.png")
REGISTERED = os.path.join(REPO, "art", "ice_car", "registered")

ARGS = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []


def arg(name):
    return ARGS[ARGS.index(name) + 1] if name in ARGS and ARGS.index(name) + 1 < len(ARGS) else None


if arg("--out"):
    BLEND_OUT, GLB_OUT, LIVERY_OUT = (os.path.join(arg("--out"), f) for f in ("skicar.blend", "skicar.glb", "skicar_livery.png"))

TYRE_W = 0.46

# --- Suspension layout per axle (left side, mirrored for the right). Heights above the ground at
# rest; the arms' inner pivots on the frame, their outer ball joints on the upright; equal parallel
# arms sloping down toward the outside, so the upright stays upright through the travel.
AXLES = {
    "front": dict(
        # The ski's centre (the corner's "wheel"), the spindle's ball joints just inboard of it.
        wheel_x=0.65,
        joint_x=0.60,
        pivot_x=0.16,
        rest=math.radians(-12.0),
        lo_h=0.30,
        up_h=0.56,
        # Inner pivots along Z from the axle: (toward the car's middle, toward its end): the arms
        # trail back to the nose's spine, as on the side view.
        spread_lo=(0.30, 0.06),
        spread_up=(0.26, 0.05),
        # Coilover: its foot on the lower arm's front leg next to the spindle, its head under the
        # nose's shoulder, leaning in (front view).
        damper_x=0.54,
        damper_to_middle=False,
        top=(0.27, 0.70, -0.01),
        tie_dz=0.15,
    ),
    "rear": dict(
        wheel_x=0.90,
        joint_x=0.62,
        pivot_x=0.20,
        rest=math.radians(-10.0),
        lo_h=0.33,
        up_h=0.58,
        spread_lo=(0.42, 0.18),
        spread_up=(0.36, 0.16),
        # Coilover: its foot on the lower arm's long leg, its head under the deck ahead of the
        # axle (side and back views).
        damper_x=0.52,
        damper_to_middle=True,
        top=(0.38, 0.92, 0.32),
        tie_dz=None,
    ),
}

CORNERS = {"FL": (1.0, WHEELBASE / 2), "FR": (-1.0, WHEELBASE / 2), "RL": (1.0, -WHEELBASE / 2), "RR": (-1.0, -WHEELBASE / 2)}


def axle(front):
    return AXLES["front" if front else "rear"]


def arm_len(a):
    return (a["joint_x"] - a["pivot_x"]) / math.cos(a["rest"])


def arm_height(a, x):
    """Height of an arm at `x` along its rest slope, relative to its outer joint."""
    return (a["joint_x"] - x) * math.tan(-a["rest"])


def leg_point(a, zc, front, x, toward_middle=True):
    """Point at half-width `x` on the lower arm's leg toward the car's middle (or its end)."""
    inward = -1.0 if front else 1.0
    spread = a["spread_lo"][0] if toward_middle else a["spread_lo"][1]
    t = (x - a["pivot_x"]) / (a["joint_x"] - a["pivot_x"])
    z_pivot = zc + inward * spread * (1 if toward_middle else -1)
    return Vector((x, H(a["lo_h"] + arm_height(a, x)), z_pivot + (zc - z_pivot) * t))


def corner_points(sx, zc):
    front = zc > 0
    a = axle(front)
    jx, px = a["joint_x"], a["pivot_x"]
    c = Vector((sx * a["wheel_x"], WHEEL_Y, zc))
    j_lo = Vector((sx * jx, H(a["lo_h"]), zc))
    j_up = Vector((sx * jx, H(a["up_h"]), zc))
    p_lo = Vector((sx * px, H(a["lo_h"] + arm_height(a, px)), zc))
    p_up = Vector((sx * px, H(a["up_h"] + arm_height(a, px)), zc))
    b0 = leg_point(a, zc, front, a["damper_x"], a["damper_to_middle"]) + Vector((0, 0.04, 0))
    b0.x *= sx
    tx, th, tdz = a["top"]
    top = Vector((sx * tx, H(th), zc + (-1.0 if front else 1.0) * tdz))
    u = (top - b0).normalized()
    s0 = b0 + u * 0.10
    u0 = top - u * 0.07
    pt = dict(c=c, p_lo=p_lo, j_lo=j_lo, p_up=p_up, j_up=j_up, b0=b0, top=top, u=u, s0=s0, u0=u0)
    if a["tie_dz"] is not None:
        pt["tie_in"] = Vector((sx * px, WHEEL_Y + arm_height(a, px) * 0.5, zc + a["tie_dz"]))
        pt["tie_out"] = Vector((sx * (jx - 0.02), WHEEL_Y, zc + a["tie_dz"]))
    return pt


def damper_lengths(pt, a):
    """Damper body and rod lengths that cover the travel."""
    lengths = []
    L_arm = arm_len(a)
    for dy in (-DOWN, UP):
        r = pt["j_lo"] - pt["p_lo"]
        L = math.hypot(r.x, r.y)
        a0 = math.asin(r.y / L)
        an = math.asin(max(-0.99, min(0.99, (r.y + dy) / L)))
        q = Matrix.Rotation((an - a0) * (1 if r.x > 0 else -1), 3, "Z")
        b = pt["p_lo"] + q @ (pt["b0"] - pt["p_lo"])
        lengths.append((pt["top"] - b).length)
    del L_arm
    longest, shortest = lengths
    body = shortest - 0.03
    return body, longest + 0.02 - body


# --- Corner parts, built for the left side relative to each part's origin. ---
def arm_mesh(lower, front):
    """A-arm along its rest slope: two black tubular legs from the inner pivots (rod ends on
    chrome) to the ball joint, a brace, and on the lower arm the coilover's bracket. The rear lower
    arms carry the drive shafts above them (they swing with the arm, their inner joint on its
    hinge)."""
    a = axle(front)
    m = Mesh()
    L = arm_len(a)
    j = Vector((L * math.cos(a["rest"]), L * math.sin(a["rest"]), 0))
    r = 0.024 if lower else 0.020
    inward = -1.0 if front else 1.0
    spread = a["spread_lo"] if lower else a["spread_up"]
    legs = [Vector((0, 0, inward * spread[0])), Vector((0, 0, -inward * spread[1]))]
    for p in legs:
        m.tube(p, j, r, "metal_frame", seg=10, caps=False)
        m.tube(p - Vector((0, 0, 0.03)), p + Vector((0, 0, 0.03)), r * 1.5, "metal_steel", seg=10)
        m.sphere(p, r * 1.25, "metal_chrome", seg=10, rings=5)
    t = 0.62
    pa = legs[0] + (j - legs[0]) * t
    pb = legs[1] + (j - legs[1]) * t
    m.tube(pa, pb, r * 0.8, "metal_frame", seg=8, caps=False)
    m.sphere(j, r * 1.45, "metal_chrome", seg=12, rings=6)
    if lower:
        tt = (a["damper_x"] - a["pivot_x"]) / (a["joint_x"] - a["pivot_x"])
        leg = legs[0] if a["damper_to_middle"] else legs[1]
        q = leg + (j - leg) * tt
        m.box(q + Vector((0, 0.022, 0)), (0.012, 0.024, 0.03), "metal_graphite")
        if not front:
            # Drive shaft from the gearbox's side to the hub, 12 cm over the arm's hinge line:
            # inner joint boot, shaft, outer boot.
            y0 = (WHEEL_Y - H(a["lo_h"] + arm_height(a, a["pivot_x"])))
            inner = Vector((-0.04, y0, 0))
            outer = Vector((a["wheel_x"] - a["pivot_x"] - 0.12, y0, 0))
            m.tube(inner, inner + Vector((0.09, 0, 0)), 0.045, "rubber_black", seg=12, r2=0.028)
            m.tube(inner + Vector((0.09, 0, 0)), outer - Vector((0.09, 0, 0)), 0.018, "metal_steel", seg=10)
            m.tube(outer - Vector((0.09, 0, 0)), outer, 0.028, "rubber_black", seg=12, r2=0.045)
    return m


def upright_mesh(front):
    a = axle(front)
    m = Mesh()
    k = a["joint_x"] - a["wheel_x"]  # kingpin, relative to the wheel's (ski's) centre
    lo, up = a["lo_h"] - WHEEL_R, a["up_h"] - WHEEL_R
    if front:
        # The spindle: a graphite post through both ball joints, down to a fork over the ski's
        # pivot bolt (13 cm above the ground), the steering arm forward to the tie rod.
        bolt = 0.13 - WHEEL_R
        m.box((k, (lo + up) / 2 + 0.02, 0), (0.024, (up - lo) / 2 + 0.06, 0.03), "metal_graphite")
        for h in (lo, up):
            m.sphere((k, h, 0), 0.028, "metal_chrome", seg=10, rings=6)
        m.box((k * 0.5, lo - 0.05, 0), (0.05, 0.02, 0.03), "metal_graphite")
        for s in (1, -1):
            m.box((s * 0.045, (lo - 0.05 + bolt) / 2, 0), (0.008, (lo - 0.05 - bolt) / 2 + 0.02, 0.028), "metal_frame")
        m.tube((-0.06, bolt, 0), (0.06, bolt, 0), 0.012, "metal_chrome", seg=8)
        tie = a["joint_x"] - 0.02 - a["wheel_x"]
        m.tube((k, 0.0, 0.02), (tie, 0.0, a["tie_dz"]), 0.015, "metal_frame")
        m.sphere((tie, 0.0, a["tie_dz"]), 0.02, "metal_chrome", seg=10, rings=6)
    else:
        m.box((k + 0.01, (lo + up) / 2, 0), (0.026, (up - lo) / 2 + 0.02, 0.04), "metal_frame")
        for h in (lo, up):
            m.sphere((k, h, 0), 0.03, "metal_chrome", seg=10, rings=6)
        m.tube((k + 0.02, 0, 0), (0.0, 0, 0), 0.05, "metal_graphite", seg=14)
        m.tube((-0.12, 0, 0), (-0.10, 0, 0), 0.16, "metal_graphite", seg=28)  # brake disc
        m.box((-0.11, 0.13, -0.06), (0.03, 0.05, 0.06), "paint_blue")  # caliper
    return m


# --- The ski: a snowmobile's, 1.4 m long, its spindle a quarter from the tail; a steel keel under
# it, a raised rib along its top, the tip curled up, snow caked on it, the saddle over its pivot.
SKI_PATH = [(-0.37, 0.035), (-0.33, 0.012), (-0.25, 0.0), (0.78, 0.0), (0.86, 0.018), (0.93, 0.055), (0.985, 0.11), (1.02, 0.17), (1.04, 0.23), (1.05, 0.28)]  # z, y of the sole
SKI_HALF = 0.095


def ski_section(scale):
    """Left half of the ski's section (x across, y above the sole), from the top centre down to
    the keel's bottom: a raised rib, a broad top, rounded edges."""
    w = SKI_HALF * scale
    return [(0.0, 0.088), (0.032, 0.088), (0.048, 0.072), (w - 0.012, 0.066), (w, 0.055), (w, 0.022), (w - 0.012, 0.012), (0.012, 0.010), (0.010, -0.002), (0.0, -0.002)]


def ski_mesh():
    m = Mesh()
    sole = -WHEEL_R
    path = [Vector((0.0, sole + y, z)) for z, y in SKI_PATH]
    # Resample the path densely.
    pts = []
    for a, b in zip(path, path[1:]):
        n = max(1, int((b - a).length / 0.05))
        for k in range(n):
            pts.append(a.lerp(b, k / n))
    pts.append(path[-1])
    rows = []
    for i, p in enumerate(pts):
        t = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized()
        up = t.cross(Vector((1, 0, 0))).normalized()  # perpendicular to the path, upward
        z = p.z
        scale = 1.0
        if z > 0.80:
            scale = 1.0 - 0.35 * min(1.0, (z - 0.80) / 0.25)
        if z < -0.25:
            scale = 0.85
        rows.append([p + Vector((x, 0, 0)) + up * y for x, y in ski_section(scale)])

    def mat(i, j):
        if j >= 7:
            return "metal_steel"
        # Snow caked over the top in irregular patches (deterministic), clear around the saddle.
        z = rows[i][0].z
        if j in (1, 2) and z < 0.8 and abs(z) > 0.12:
            v = math.sin(i * 1.9 + j) + 0.7 * math.sin(i * 0.53 + 2.0 * j)
            if v > 0.25:
                return "paint_snow"
        return "paint_black"

    ids = m.grid(rows, "paint_black", mats=mat)
    m.cap(ids[-1], "paint_black", flip=True)
    m.cap(ids[0], "paint_black")
    m.both()
    # The saddle over the pivot: two plates either side of the rib, a rubber block ahead.
    for s in (1, -1):
        m.box((s * 0.030, sole + 0.11, 0.0), (0.006, 0.035, 0.05), "metal_frame")
    m.box((0.0, sole + 0.10, 0.16), (0.03, 0.014, 0.05), "rubber_black")
    # Bolts along the top.
    for z in (-0.20, 0.25, 0.45, 0.65):
        for s in (1, -1):
            c = Vector((s * 0.07, sole + 0.068, z))
            m.tube(c, c + Vector((0, 0.005, 0)), 0.006, "metal_chrome", seg=6)
    return m


# --- Rear tyre: a studded winter tyre on a blue rim, wrapped in a ladder of snow chains.
HW = TYRE_W / 2
RIM_R = 0.245
TREAD_R = 0.430  # the tread's floor; its blocks stand 1.5 cm proud
BLOCK_H = 0.015
TYRE_PROFILE = [
    # x, r from the inner bead to the middle of the tread
    (-HW + 0.03, RIM_R),
    (-HW + 0.006, 0.285),
    (-HW, 0.33),
    (-HW + 0.004, 0.37),
    (-HW + 0.016, 0.398),
    (-HW + 0.038, 0.420),
    (-0.165, TREAD_R),
    (-0.10, TREAD_R + 0.001),
    (0.0, TREAD_R + 0.0015),
]


def profile_r(x):
    """The tyre's outer radius (without its blocks) at axial x."""
    ax = -abs(x)
    pts = TYRE_PROFILE
    for (x0, r0), (x1, r1) in zip(pts, pts[1:]):
        if x0 <= ax <= x1:
            return r0 + (r1 - r0) * (ax - x0) / (x1 - x0)
    return pts[-1][1]


def full_profile():
    left = TYRE_PROFILE
    return left + [(-x, r) for x, r in reversed(left[:-1])]


def tyre_mats(prof, i, j, seg):
    x = abs((prof[i][0] + prof[i + 1][0]) / 2)
    if x > 0.18:
        return "rubber_tyre"
    # Snow packed between the blocks in uneven patches round the tyre.
    a = 2 * math.pi * (j + 0.5) / seg
    v = math.sin(3 * a + 0.4) + 0.6 * math.sin(7 * a + 1.3) + 0.4 * math.sin(11 * a + x * 30)
    return "rubber_snow" if v > 1.0 else "rubber_tyre"


def block(m, x0, x1, a0, a1, mat="rubber_tyre"):
    """A tread block between axial x0..x1 and angles a0..a1, standing on the tread's floor."""
    def at(x, a, r):
        return Vector((x, r * math.cos(a), r * math.sin(a)))

    inset_x, inset_a = 0.004, 0.004 / TREAD_R
    base = [at(x0, a0, profile_r(x0) - 0.002), at(x1, a0, profile_r(x1) - 0.002), at(x1, a1, profile_r(x1) - 0.002), at(x0, a1, profile_r(x0) - 0.002)]
    top = [
        at(x0 + inset_x, a0 + inset_a, profile_r(x0) + BLOCK_H),
        at(x1 - inset_x, a0 + inset_a, profile_r(x1) + BLOCK_H),
        at(x1 - inset_x, a1 - inset_a, profile_r(x1) + BLOCK_H),
        at(x0 + inset_x, a1 - inset_a, profile_r(x0) + BLOCK_H),
    ]
    tb = [m.vert(p) for p in base]
    tt = [m.vert(p) for p in top]
    # Top: outward is the radius; (x across) x (angle forward) points out.
    m.face([tt[0], tt[3], tt[2], tt[1]][::-1], mat)
    for k in range(4):
        n = (k + 1) % 4
        m.face([tb[k], tb[n], tt[n], tt[k]], mat)
    return top


def stud(m, x, a, r):
    d = Vector((0, math.cos(a), math.sin(a)))
    c = Vector((x, 0, 0))
    m.tube(c + d * (r - 0.004), c + d * (r + 0.0035), 0.0045, "metal_steel", seg=6)


def chain_links(m, pts, normals, pitch=0.030, length=0.036):
    """Links along a polyline (with the surface normal at each point), alternately flat and
    upright, overlapping like a chain."""
    seglen = [(b - a).length for a, b in zip(pts, pts[1:])]
    total = sum(seglen)
    n = max(1, int(total / pitch))
    for k in range(n):
        s = (k + 0.5) * total / n
        acc = 0.0
        for i, L in enumerate(seglen):
            if acc + L >= s or i == len(seglen) - 1:
                t = (s - acc) / L if L > 0 else 0
                p = pts[i].lerp(pts[i + 1], t)
                nn = normals[i].lerp(normals[i + 1], t).normalized()
                tan = (pts[i + 1] - pts[i]).normalized()
                break
            acc += L
        side = tan.cross(nn).normalized()
        nn = side.cross(tan).normalized()
        if k % 2 == 0:
            axes, half = (side, nn, tan), (0.0075, 0.0026, length / 2)
        else:
            axes, half = (side, nn, tan), (0.0026, 0.0068, length / 2)
        m.box(p, half, "metal_chain", axes=axes)


CHAIN_TREAD_R = 0.449
CHAIN_CONTOUR = [(-0.222, 0.400), (-0.214, 0.424), (-0.198, 0.443), (-0.16, CHAIN_TREAD_R), (-0.08, CHAIN_TREAD_R), (0.0, CHAIN_TREAD_R)]


def chains(m):
    contour = CHAIN_CONTOUR + [(-x, r) for x, r in reversed(CHAIN_CONTOUR[:-1])]
    cross = 14
    for k in range(cross):
        a = 2 * math.pi * (k + 0.25) / cross
        d = Vector((0, math.cos(a), math.sin(a)))
        pts = [Vector((x, 0, 0)) + d * r for x, r in contour]
        normals = []
        for i in range(len(contour)):
            x0, r0 = contour[max(i - 1, 0)]
            x1, r1 = contour[min(i + 1, len(contour) - 1)]
            tx, tr = x1 - x0, r1 - r0
            nx, nr = -tr, tx  # outward of a contour running -x to +x over the top
            L = math.hypot(nx, nr)
            normals.append(Vector((nx / L, 0, 0)) + d * (nr / L))
        chain_links(m, pts, normals)
    # Rings round the tyre: the side chains on the shoulders, two on the tread.
    for x, r in ((-0.222, 0.400), (0.222, 0.400), (-0.085, CHAIN_TREAD_R), (0.085, CHAIN_TREAD_R)):
        seg = 96
        pts, normals = [], []
        for q in range(seg + 1):
            a = 2 * math.pi * q / seg
            d = Vector((0, math.cos(a), math.sin(a)))
            pts.append(Vector((x, 0, 0)) + d * r)
            normals.append((d + Vector((math.copysign(0.8, x), 0, 0))).normalized() if abs(x) > 0.2 else d)
        chain_links(m, pts, normals)


def wheel_mesh():
    m = Mesh()
    prof = full_profile()
    seg = 64
    m.lathe(prof, "rubber_tyre", seg=seg, mats=lambda i, j: tyre_mats(prof, i, j, seg))
    # Blocks: a centre row, two middle rows staggered half a pitch, shoulder lugs; a stud in each
    # block of the inner rows.
    n = 28
    pitch = 2 * math.pi / n
    rows = [((-0.045, 0.045), 0.0, True), ((0.06, 0.145), 0.5, True), ((-0.145, -0.06), 0.5, True), ((0.155, 0.205), 0.0, False), ((-0.205, -0.155), 0.0, False)]
    for (x0, x1), off, studded in rows:
        for j in range(n):
            a0 = (j + off) * pitch
            a1 = a0 + 0.64 * pitch
            block(m, x0, x1, a0, a1)
            if studded:
                xm = (x0 + x1) / 2
                stud(m, xm, (a0 + a1) / 2, profile_r(xm) + BLOCK_H)
    chains(m)
    # Rim: the blue barrel, its outer lip with a bolted ring, a black fourteen-spoke centre, the
    # hub, its chrome cap and lug nuts.
    face_x = HW - 0.05
    m.lathe([(face_x, RIM_R - 0.004), (-HW + 0.04, RIM_R - 0.004)], "paint_blue", seg=40)
    m.lathe([(face_x, RIM_R - 0.002), (face_x, 0.205)], "paint_blue", seg=40)
    m.lathe([(face_x, 0.205), (face_x - 0.05, 0.205)], "paint_blue", seg=40)
    m.lathe([(face_x - 0.10, 0.0), (face_x - 0.10, RIM_R - 0.004)], "rubber_black", seg=40)
    for xr, sgn in ((HW - 0.034, 1), (-HW + 0.034, -1)):
        prof_ring = [(xr, 0.250), (xr + sgn * 0.014, 0.248), (xr + sgn * 0.014, 0.212), (xr, 0.21)]
        m.lathe(prof_ring if sgn > 0 else list(reversed(prof_ring)), "paint_blue", seg=40)
        for i in range(16):
            a = 2 * math.pi * i / 16
            p = Vector((xr + sgn * 0.014, 0.23 * math.cos(a), 0.23 * math.sin(a)))
            m.tube(p, p + Vector((sgn * 0.007, 0, 0)), 0.0065, "metal_steel", seg=6)
    for i in range(14):
        a = 2 * math.pi * (i + 0.5) / 14
        er = Vector((0, math.cos(a), math.sin(a)))
        et = Vector((0, -math.sin(a), math.cos(a)))
        c = Vector((face_x - 0.028, 0, 0)) + er * 0.13
        m.box(c, (0.02, 0.075, 0.011), "metal_frame", axes=(Vector((1, 0, 0)), er, et))
    m.tube((face_x - 0.06, 0, 0), (face_x - 0.005, 0, 0), 0.07, "metal_frame", seg=20)
    m.tube((face_x - 0.005, 0, 0), (face_x + 0.012, 0, 0), 0.040, "metal_chrome", seg=16)
    for i in range(6):
        a = 2 * math.pi * i / 6
        p = Vector((face_x - 0.005, 0.054 * math.cos(a), 0.054 * math.sin(a)))
        m.tube(p, p + Vector((0.014, 0, 0)), 0.009, "metal_steel", seg=6)
    return m


def damper_parts(pt, a):
    """Damper body (origin at the top mount), rod (origin at the bottom mount) and spring (origin
    at its lower seat): graphite body, chrome rod, blue spring."""
    u, s0, u0 = pt["u"], pt["s0"], pt["u0"]
    body_len, rod_len = damper_lengths(pt, a)
    body = Mesh()
    body.tube((0, 0, 0), -u * body_len, 0.032, "metal_graphite", seg=16)
    body.tube((0, 0, 0), -u * 0.05, 0.038, "metal_frame", seg=16)
    body.tube(-u * 0.055, -u * 0.07, 0.054, "metal_chrome", seg=20)
    body.tube(-u * (body_len - 0.02), -u * body_len, 0.038, "metal_chrome", seg=16)
    body.tube((0, 0, -0.025), (0, 0, 0.025), 0.022, "metal_steel", seg=12)
    rod = Mesh()
    rod.tube((0, 0, 0), u * rod_len, 0.013, "metal_chrome", seg=10)
    rod.tube((0, 0, -0.025), (0, 0, 0.025), 0.022, "metal_steel", seg=12)
    rod.tube(u * 0.085, u * 0.10, 0.054, "metal_chrome", seg=20)
    spring = Mesh()
    spring.helix((0, 0, 0), u, (u0 - s0).length, 0.046, 0.009, 7, "paint_blue", steps_per_turn=16, ring=7)
    return body, rod, spring


def tierod_mesh(pt):
    m = Mesh()
    d = pt["tie_out"] - pt["tie_in"]
    m.tube((0, 0, 0), d, 0.013, "metal_chrome", seg=10)
    m.tube((0, 0, 0), d.normalized() * 0.07, 0.024, "rubber_black", seg=12)
    m.sphere(d, 0.019, "metal_frame", seg=10, rings=6)
    return m


def build_corner(name, sx, zc, coll, mats, root):
    pt = corner_points(sx, zc)
    front = zc > 0
    a = axle(front)
    flip = (lambda m: m) if sx > 0 else (lambda m: m.mirrored())
    c = pt["c"]

    base = empty(f"ctrl_base.{name}", c, coll, root, size=0.02)
    ctrl = empty(f"ctrl.{name}", c, coll, base, kind="SINGLE_ARROW", size=0.35)
    ctrl.lock_location = (True, True, False)

    arm_lo = to_object(f"arm_lo.{name}", flip(arm_mesh(True, front)), pt["p_lo"], coll, mats, root, relative=True)
    a0 = a["rest"]
    a_down = a0 - math.asin(math.sin(a0) - DOWN / arm_len(a))
    t = arm_lo.constraints.new("TRANSFORM")
    t.target = ctrl
    t.owner_space = "LOCAL"
    t.target_space = "LOCAL"
    t.map_from = "LOCATION"
    t.map_to = "ROTATION"
    t.use_motion_extrapolate = True
    t.from_min_z = -DOWN
    t.from_max_z = DOWN
    t.map_to_x_from = "X"
    t.map_to_y_from = "Z"
    t.map_to_z_from = "X"
    t.to_min_y_rot = sx * a_down
    t.to_max_y_rot = -sx * a_down
    j_lo = empty(f"joint_lo.{name}", pt["j_lo"], coll, arm_lo)
    mount_bot = empty(f"mount_bot.{name}", pt["b0"], coll, arm_lo)

    arm_up = to_object(f"arm_up.{name}", flip(arm_mesh(False, front)), pt["p_up"], coll, mats, root, relative=True)
    cr = arm_up.constraints.new("COPY_ROTATION")
    cr.target = arm_lo
    cr.owner_space = "LOCAL"
    cr.target_space = "LOCAL"
    empty(f"joint_up.{name}", pt["j_up"], coll, arm_up)

    hub = empty(f"hub.{name}", pt["j_lo"], coll, root, kind="CIRCLE", size=0.12)
    cl = hub.constraints.new("COPY_LOCATION")
    cl.target = j_lo
    upright = to_object(f"upright.{name}", flip(upright_mesh(front)), c, coll, mats, hub, relative=True)
    to_object(f"wheel.{name}", flip(ski_mesh() if front else wheel_mesh()), c, coll, mats, upright, relative=True)

    top = empty(f"mount_top.{name}", pt["top"], coll, root)
    left = pt if sx > 0 else corner_points(1.0, zc)
    body, rod_m, spring_m = damper_parts(left, a)
    if sx < 0:
        body, rod_m, spring_m = body.mirrored(), rod_m.mirrored(), spring_m.mirrored()
    damper = to_object(f"damper.{name}", body, pt["top"], coll, mats, root, relative=True, rot=axes_matrix(z=pt["u"]))
    dt = damper.constraints.new("DAMPED_TRACK")
    dt.target = mount_bot
    dt.track_axis = "TRACK_NEGATIVE_Z"
    spring_top = empty(f"spring_top.{name}", pt["u0"], coll, damper)
    rod = to_object(f"rod.{name}", rod_m, pt["b0"], coll, mats, root, relative=True, rot=axes_matrix(z=pt["u"]))
    cl = rod.constraints.new("COPY_LOCATION")
    cl.target = mount_bot
    dt = rod.constraints.new("DAMPED_TRACK")
    dt.target = top
    dt.track_axis = "TRACK_Z"
    spring = to_object(f"spring.{name}", spring_m, pt["s0"], coll, mats, rod, relative=True, rot=axes_matrix(y=pt["u"]))
    st = spring.constraints.new("STRETCH_TO")
    st.target = spring_top
    st.rest_length = (pt["u0"] - pt["s0"]).length
    st.volume = "NO_VOLUME"

    if "tie_in" in pt:
        empty(f"tierod_out.{name}", pt["tie_out"], coll, upright)
        d = pt["tie_out"] - pt["tie_in"]
        tm = tierod_mesh(left)
        if sx < 0:
            tm = tm.mirrored()
        tie = to_object(f"tierod.{name}", tm, pt["tie_in"], coll, mats, root, relative=True, rot=axes_matrix(y=d))
        st = tie.constraints.new("STRETCH_TO")
        st.target = bpy.data.objects[f"tierod_out.{name}"]
        st.rest_length = d.length
        st.volume = "NO_VOLUME"
    return ctrl, hub


VIEWS = {
    # view: canvas (w, h), pixels per metre, the car-frame axes along the image's right and down,
    # and the pixel of the car-frame origin's projection (vertical views: its column and the
    # ground's row). Same layout as tools/blender/blueprint.py's.
    "left": dict(size=(2688, 1520), scale=520.0, ground_px=1300, centre_px=1344, right=(0, 0, -1), down=(0, -1, 0)),
    "top": dict(size=(2688, 1520), scale=520.0, ground_px=None, centre_px=(1344, 760), right=(0, 0, 1), down=(-1, 0, 0)),
}


def add_references(coll):
    """The registered plans (tools/blender/skicar_refs.py) as image empties behind the car, at the
    scale and place of their canvases: in Blender's orthographic Left and Top views the model sits
    on them."""
    for view, v in VIEWS.items():
        path = os.path.join(REGISTERED, f"{view}_grid.png")
        if not os.path.exists(path):
            continue
        w, h = v["size"]
        s = v["scale"]
        right, down = Vector(v["right"]), Vector(v["down"])
        if view == "top":
            cx, cy = v["centre_px"]
            centre = right * ((w / 2 - cx) / s) + down * ((h / 2 - cy) / s)
        else:
            centre = right * ((w / 2 - v["centre_px"]) / s) + down * ((h / 2 - v["ground_px"]) / s - GROUND_Y)
        back = right.cross(-down)
        img = bpy.data.images.load(path, check_existing=True)
        e = bpy.data.objects.new(f"ref_{view}", None)
        e.empty_display_type = "IMAGE"
        e.data = img
        e.empty_display_size = w / s
        e.empty_image_offset = (-0.5, -0.5)
        e.empty_image_side = "FRONT"
        e.show_empty_image_only_axis_aligned = True
        e.show_empty_image_perspective = False
        e.color[3] = 0.5
        e.use_empty_image_alpha = True
        rot = Matrix((G(right), G(-down), G(back))).transposed().to_4x4()
        e.matrix_world = Matrix.Translation(G(centre - back * 3.0)) @ rot
        coll.objects.link(e)


def build(livery=True):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    mats = make_materials()
    coll = bpy.data.collections.new("Skicar")
    scene.collection.children.link(coll)
    refs = bpy.data.collections.new("References")
    scene.collection.children.link(refs)
    stage = bpy.data.collections.new("Stage")
    scene.collection.children.link(stage)

    root = bpy.data.objects.new("skicar", None)
    root.empty_display_type = "ARROWS"
    root.empty_display_size = 0.4
    coll.objects.link(root)
    WORLD[root.name] = Matrix.Identity(4)
    root["wheel_radius"] = WHEEL_R
    root["tyre_width"] = TYRE_W
    root["rest_suspension"] = REST
    root["travel"] = TRAVEL

    body = []
    for name, mesh in skicar_body.body_parts().items():
        chamfer = 0.005 if name in skicar_body.PANELS else None
        body.append(to_object(name, mesh, (0, 0, 0), coll, mats, root, uv_fn=skicar_livery.assign_uvs, chamfer=chamfer))

    rig = {}
    for name, (sx, zc) in CORNERS.items():
        rig[name] = build_corner(name, sx, zc, coll, mats, root)
    bpy.context.view_layer.update()
    if livery:
        skicar_livery.bake(mats, LIVERY_OUT)
    add_references(refs)
    return scene, coll, root, rig, stage, mats


def stage_scene(scene, stage, mats):
    """Snow, a low cold sun, a pale sky, and the preview cameras."""
    import bmesh

    bm = bmesh.new()
    s = 30.0
    for x, y in ((-s, -s), (s, -s), (s, s), (-s, s)):
        bm.verts.new((x, y, GROUND_Y))
    bm.faces.new(bm.verts)
    me = bpy.data.meshes.new("ground")
    bm.to_mesh(me)
    bm.free()
    ground_mat = bpy.data.materials.new("ground")
    use_nodes(ground_mat)
    b = ground_mat.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*srgb(214, 226, 238), 1)
    b.inputs["Roughness"].default_value = 0.9
    me.materials.append(ground_mat)
    g = bpy.data.objects.new("ground", me)
    stage.objects.link(g)

    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 3.5
    sun.color = (1.0, 0.97, 0.92)
    sun.angle = math.radians(2.0)
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (math.radians(55), math.radians(-10), math.radians(-35))
    stage.objects.link(so)
    fill = bpy.data.lights.new("fill", "AREA")
    fill.energy = 500
    fill.size = 6
    fill.color = (0.85, 0.92, 1.0)
    fo = bpy.data.objects.new("fill", fill)
    fo.location = (-5, 4, 5)
    fo.rotation_euler = (math.radians(-45), math.radians(-40), 0)
    stage.objects.link(fo)

    world = bpy.data.worlds.new("ice")
    use_nodes(world)
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (*srgb(170, 196, 224), 1)
    bg.inputs["Strength"].default_value = 0.8
    scene.world = world

    cams = {}
    for name, loc, look, lens in [
        # Blender frame: +X the car's left, -Y forward, +Z up.
        ("cam_hero", (4.4, -5.4, 0.7), (0, -0.4, -0.15), 40),
        ("cam_side", (8.0, -0.25, 0.1), (0, -0.25, 0.05), 40),
        ("cam_front", (0.0, -8.0, 0.1), (0, 0, -0.2), 55),
        ("cam_rear", (-3.8, 5.6, 1.4), (0, 0, -0.3), 42),
        ("cam_chase", (0.0, 6.2, 1.6), (0, -1.0, -0.1), 45),
    ]:
        cd = bpy.data.cameras.new(name)
        cd.lens = lens
        co = bpy.data.objects.new(name, cd)
        co.location = loc
        d = Vector(look) - Vector(loc)
        co.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
        stage.objects.link(co)
        cams[name] = co
    scene.camera = cams["cam_hero"]

    scene.render.engine = "BLENDER_EEVEE"
    try:
        scene.eevee.taa_render_samples = 48
        scene.eevee.use_raytracing = True
        scene.eevee.use_shadows = True
    except AttributeError:
        pass
    scene.render.resolution_x, scene.render.resolution_y = 1600, 900
    scene.render.film_transparent = False
    return cams


def export(scene, coll, root):
    scene.frame_set(1)
    bpy.ops.object.select_all(action="DESELECT")
    for o in coll.all_objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = root
    os.makedirs(os.path.dirname(GLB_OUT), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=GLB_OUT,
        export_format="GLB",
        use_selection=True,
        export_apply=True,
        export_yup=True,
        export_animations=False,
        export_extras=True,
        export_cameras=False,
        export_lights=False,
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_image_format="NONE",
    )


def render(scene, cams, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    # Back faces culled, as the game draws them: a face wound the wrong way shows as a hole.
    for m in bpy.data.materials:
        m.use_backface_culling = True
    for label, cam in (("1_hero", "cam_hero"), ("2_side", "cam_side"), ("3_front", "cam_front"), ("4_rear", "cam_rear"), ("5_chase", "cam_chase")):
        scene.camera = cams[cam]
        scene.render.filepath = os.path.join(out_dir, f"{label}.png")
        bpy.ops.render.render(write_still=True)


def main():
    scene, coll, root, rig, stage, mats = build(livery="--no-livery" not in ARGS)
    cams = stage_scene(scene, stage, mats)
    export(scene, coll, root)
    os.makedirs(os.path.dirname(BLEND_OUT), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=BLEND_OUT, relative_remap=True)
    out = arg("--render")
    if out:
        render(scene, cams, out)
    tris = sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in coll.all_objects if o.type == "MESH")
    print(f"skicar: {len([o for o in coll.all_objects if o.type == 'MESH'])} meshes, {tris} triangles -> {GLB_OUT}")


main()
