"""Builds the Mars buggy "B" in Blender: the body panels (tools/blender/buggy_body.py) with their
livery (tools/blender/buggy_livery.py), and four double-wishbone corners whose arms, upright,
wheel, coilover and tie rod are separate parts, rigged so that moving a corner's control moves
them like the real thing.

    blender -b -P tools/blender/build_buggy.py -- [--render DIR] [--video FILE] [--no-livery] [--out DIR]

Writes art/buggy/buggy.blend (with the registered plans of art/buggy/v2 as image empties),
crates/app/assets/buggy.glb and its livery crates/app/assets/buggy_livery.png. The game reads the
parts and their pivots from the .glb and solves the suspension itself (crates/app/src/car_model.rs):
the constraints here only drive the preview.

In the .blend, move `ctrl.FL` (and the others) up and down to compress or extend a corner, rotate
`hub.FL` / `hub.FR` around Z to steer, rotate `wheel.*` around X to roll. The timeline holds a
jump, a landing and a hard left turn.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blueprint as bp  # noqa: E402
import buggy_body  # noqa: E402
import buggy_livery  # noqa: E402
from meshkit import (  # noqa: E402
    DOWN,
    GROUND_Y,
    REST,
    TRACK,
    TRAVEL,
    TYRE_W,
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
BLEND_OUT = os.path.join(REPO, "art", "buggy", "buggy.blend")
GLB_OUT = os.path.join(REPO, "crates", "app", "assets", "buggy.glb")
LIVERY_OUT = os.path.join(REPO, "crates", "app", "assets", "buggy_livery.png")

ARGS = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []


def arg(name):
    return ARGS[ARGS.index(name) + 1] if name in ARGS and ARGS.index(name) + 1 < len(ARGS) else None


if arg("--out"):
    # Work in progress elsewhere: leave the game's assets alone.
    BLEND_OUT, GLB_OUT, LIVERY_OUT = (os.path.join(arg("--out"), f) for f in ("buggy.blend", "buggy.glb", "buggy_livery.png"))


# --- Suspension layout (left side, mirrored for the right), read off the plans' front and back
# views: equal parallel A-arms sloping down toward the wheel at rest, so the upright stays upright
# and the wheel moves up and down (and tucks in a few cm at full droop). The lower arms are wide
# and lopsided, their long leg reaching back (front) or forward (rear) to the side pods, which
# puts the coilovers' lower mounts where the side views show them.
JOINT_X = 0.62  # outer ball joints, just inboard of the tyre
PIVOT_X = 0.18  # inner pivots, on the frame
ARM_REST = math.radians(-11.5)  # arm slope at rest (outer end lower)
ARM_L = (JOINT_X - PIVOT_X) / math.cos(ARM_REST)
LO_H = 0.33  # ball joint heights above the ground (wheel centre at 0.45)
UP_H = 0.55
# Inner pivots of the lower and upper arms along Z from the axle: (toward the car's middle,
# toward its end).
SPREAD_LO = (0.55, 0.28)
SPREAD_UP = (0.40, 0.22)
# Coilovers: lower mount on a leg of the lower arm (at half-width x: the front one on the leg
# toward the nose, ahead of the axle as on the side and front views; the rear one on the long leg
# toward the middle), top mount on the body.
DAMPER_X = {"front": 0.56, "rear": 0.48}
DAMPER_LEG_TO_MIDDLE = {"front": False, "rear": True}
# Top mounts: x, h, z from the axle toward the middle. Seen from the front and the back the
# coilovers lean in at the top, as on the plans.
TOP = {"front": (0.41, 0.90, -0.10), "rear": (0.31, 1.0, 0.36)}
TIE_DZ = 0.17  # tie rod ahead of the front axle

CORNERS = {"FL": (1.0, WHEELBASE / 2), "FR": (-1.0, WHEELBASE / 2), "RL": (1.0, -WHEELBASE / 2), "RR": (-1.0, -WHEELBASE / 2)}


def arm_height(x):
    """Height (h) of an arm at `x` along its rest slope, relative to its outer joint."""
    return (JOINT_X - x) * math.tan(-ARM_REST)


def leg_point(zc, front, x, toward_middle=True):
    """Point at half-width `x` on the lower arm's leg toward the car's middle (or its end)."""
    inward = -1.0 if front else 1.0  # direction to the car's middle along Z
    spread = SPREAD_LO[0] if toward_middle else SPREAD_LO[1]
    t = (x - PIVOT_X) / (JOINT_X - PIVOT_X)
    z_pivot = zc + inward * spread * (1 if toward_middle else -1)
    return Vector((x, H(LO_H + arm_height(x)), z_pivot + (zc - z_pivot) * t))


def corner_points(sx, zc):
    front = zc > 0
    c = Vector((sx * TRACK / 2, WHEEL_Y, zc))
    j_lo = Vector((sx * JOINT_X, H(LO_H), zc))
    j_up = Vector((sx * JOINT_X, H(UP_H), zc))
    p_lo = Vector((sx * PIVOT_X, H(LO_H + arm_height(PIVOT_X)), zc))
    p_up = Vector((sx * PIVOT_X, H(UP_H + arm_height(PIVOT_X)), zc))
    key = "front" if front else "rear"
    b0 = leg_point(zc, front, DAMPER_X[key], DAMPER_LEG_TO_MIDDLE[key]) + Vector((0, 0.04, 0))
    b0.x *= sx
    tx, th, tdz = TOP[key]
    top = Vector((sx * tx, H(th), zc + (-1.0 if front else 1.0) * tdz))
    u = (top - b0).normalized()
    s0 = b0 + u * 0.10
    u0 = top - u * 0.07
    tie_in = Vector((sx * PIVOT_X, WHEEL_Y + arm_height(PIVOT_X) * 0.5, zc + TIE_DZ))
    tie_out = Vector((sx * (JOINT_X - 0.02), WHEEL_Y, zc + TIE_DZ))
    return dict(c=c, p_lo=p_lo, j_lo=j_lo, p_up=p_up, j_up=j_up, b0=b0, top=top, u=u, s0=s0, u0=u0, tie_in=tie_in, tie_out=tie_out)


def damper_lengths(pt):
    """Damper body and rod lengths that cover the travel: the body shorter than the coilover at
    full bump, body plus rod longer than it at full droop."""
    lengths = []
    for dy in (-DOWN, UP):
        r = pt["j_lo"] - pt["p_lo"]
        L = math.hypot(r.x, r.y)
        a0 = math.asin(r.y / L)
        a = math.asin(max(-0.99, min(0.99, (r.y + dy) / L)))
        q = Matrix.Rotation((a - a0) * (1 if r.x > 0 else -1), 3, "Z")
        b = pt["p_lo"] + q @ (pt["b0"] - pt["p_lo"])
        lengths.append((pt["top"] - b).length)
    longest, shortest = lengths
    body = shortest - 0.03
    return body, longest + 0.02 - body


# --- Corner parts, built for the left side relative to each part's origin. ---
def arm_mesh(lower, front):
    """A-arm along its rest slope: two black tubular legs from the inner pivots (rod ends on
    chrome) to the ball joint, a brace between them, and on the lower arm the coilover's
    bracket."""
    m = Mesh()
    j = Vector((ARM_L * math.cos(ARM_REST), ARM_L * math.sin(ARM_REST), 0))
    r = 0.026 if lower else 0.022
    inward = -1.0 if front else 1.0
    spread = SPREAD_LO if lower else SPREAD_UP
    legs = [Vector((0, 0, inward * spread[0])), Vector((0, 0, -inward * spread[1]))]
    for p in legs:
        m.tube(p, j, r, "metal_frame", seg=10, caps=False)
        m.tube(p - Vector((0, 0, 0.035)), p + Vector((0, 0, 0.035)), r * 1.5, "metal_steel", seg=10)
        m.sphere(p, r * 1.25, "metal_chrome", seg=10, rings=5)
    # Brace across the legs, a third of the way in.
    t = 0.62
    a = legs[0] + (j - legs[0]) * t
    b = legs[1] + (j - legs[1]) * t
    m.tube(a, b, r * 0.8, "metal_frame", seg=8, caps=False)
    m.sphere(j, r * 1.45, "metal_chrome", seg=12, rings=6)
    if lower:
        key = "front" if front else "rear"
        tt = (DAMPER_X[key] - PIVOT_X) / (JOINT_X - PIVOT_X)
        leg = legs[0] if DAMPER_LEG_TO_MIDDLE[key] else legs[1]
        q = leg + (j - leg) * tt
        m.box(q + Vector((0, 0.022, 0)), (0.012, 0.024, 0.03), "metal_graphite")
    return m


def upright_mesh(front):
    """Black knuckle between the ball joints, spindle, brake disc and orange caliper inside the
    rim, and the steering arm (front)."""
    m = Mesh()
    k = JOINT_X - TRACK / 2  # kingpin, relative to the wheel centre
    m.box((k + 0.01, (LO_H + UP_H) / 2 - WHEEL_R, 0), (0.026, (UP_H - LO_H) / 2 + 0.02, 0.04), "metal_frame")
    for h in (LO_H, UP_H):
        m.sphere((k, h - WHEEL_R, 0), 0.03, "metal_chrome", seg=10, rings=6)
    m.tube((k + 0.02, 0, 0), (0.06, 0, 0), 0.045, "metal_graphite", seg=14)
    m.tube((-0.10, 0, 0), (-0.08, 0, 0), 0.17, "metal_graphite", seg=28)  # brake disc
    m.box((-0.09, 0.14, -0.06), (0.03, 0.05, 0.06), "metal_copper")  # caliper
    if front:
        m.tube((k + 0.01, 0, 0.03), (k - 0.02, 0, TIE_DZ), 0.016, "metal_frame")
        m.sphere((k - 0.02, 0, TIE_DZ), 0.022, "metal_chrome", seg=10, rings=6)
    return m


# Tyre: a fat balloon with four circumferential grooves (the 3/4 view's tread) and blocks on the
# shoulders; the rim black with ten spokes and an orange beadlock ring on each face.
HW = TYRE_W / 2
RIM_R = 0.262


# Tyre: a balloon (after the validated 3/4 view): the sidewalls bulge out to the full width at a
# third of their height, a broad rounded shoulder runs into a slightly crowned tread with four
# deep circumferential grooves, fine angled sipes across the ribs; dirt packed in the grooves,
# dust in patches on the shoulders.
CROWN_X = 0.10  # half-width of the tread's crown
CROWN = 0.006  # its drop from the middle to its edges
SHOULDER = (0.14, 0.10)  # the shoulder's quarter-ellipse radii, across and down
GROOVES = (-0.125, -0.052, 0.052, 0.125)  # groove centres across the tyre
GROOVE_W, GROOVE_D = 0.015, 0.013
SIPES = 48  # sipes round each rib


def tyre_outline(x):
    """Outer radius of the tyre at axial position x (without the grooves), and the outward
    normal direction (dx, dr) of the contour there."""
    ax = abs(x)
    if ax <= CROWN_X:
        return WHEEL_R - CROWN * (ax / CROWN_X) ** 2, (0.0, 1.0)
    sa, sr = SHOULDER
    cx, cr = CROWN_X, WHEEL_R - CROWN - sr
    u = min(1.0, (ax - cx) / sa)
    r = cr + sr * math.sqrt(max(0.0, 1 - u * u))
    # Normal of the ellipse ((x-cx)/sa)² + ((r-cr)/sr)² = 1: ((x-cx)/sa², (r-cr)/sr²).
    nx, nr = (ax - cx) / sa ** 2, (r - cr) / sr ** 2
    k = math.hypot(nx, nr) or 1.0
    return r, (math.copysign(nx / k, x), nr / k)


def tyre_profile():
    """(x, r) of the tyre from the inner bead up the bulging sidewall, over the shoulder and
    the grooved tread, down to the outer bead; and the set of profile spans that are groove
    floors and walls."""
    sa, sr = SHOULDER
    full = CROWN_X + sa  # the widest point
    lower = [(-HW + 0.035, RIM_R), (-full + 0.02, 0.29), (-full + 0.004, 0.32)]
    xs = []
    n = 26
    for k in range(n + 1):
        # Dense over the shoulder and crown, from the widest point to the middle.
        t = k / n
        xs.append(-full + full * (1 - math.cos(t * math.pi / 2)))
    pts, grooves = [], set()
    half = []
    for x in xs:
        half.append(x)
    # Insert the grooves on the left half.
    for g in (g for g in GROOVES if g < 0):
        half += [g - GROOVE_W / 2, g - GROOVE_W / 2 + 0.002, g + GROOVE_W / 2 - 0.002, g + GROOVE_W / 2]
    half = sorted(set(round(x, 5) for x in half if x <= 0))
    for x in half:
        r, (nx, nr) = tyre_outline(x)
        in_groove = any(g - GROOVE_W / 2 + 0.001 < x < g + GROOVE_W / 2 - 0.001 for g in GROOVES)
        if in_groove:
            r_ = r - GROOVE_D * nr
            x_ = x - GROOVE_D * nx
            pts.append((x_, r_))
        else:
            pts.append((x, r))
    left = lower + pts
    prof = left + [(-x, r) for x, r in reversed(left[:-1])] if abs(left[-1][0]) < 1e-6 else left + [(-x, r) for x, r in reversed(left)]
    for i in range(len(prof) - 1):
        xm = (prof[i][0] + prof[i + 1][0]) / 2
        if any(abs(xm - g) < GROOVE_W / 2 for g in GROOVES):
            grooves.add(i)
    return prof, grooves


def dusty(i, j, salt):
    """Deterministic pseudo-random 0..1 for a face (no randomness module, reproducible)."""
    return ((i * 73856093) ^ (j * 19349663) ^ (salt * 83492791)) % 1000 / 1000.0


def tyre_mats(prof, grooves, i, j, seg=64):
    """The tread's top worn to a dusty brown, dirt packed dark in the grooves and the sipes, the
    dust running down onto the shoulders in soft, uneven tongues (a smooth function of the
    angle), the sidewalls black."""
    if i in grooves:
        return "rubber_mud"
    x = abs((prof[i][0] + prof[i + 1][0]) / 2)
    if x <= CROWN_X + 0.02:
        return "rubber_dust"
    # How far down the shoulder the dust reaches at this angle: 2 to 7 cm across.
    a = 2 * math.pi * (j + 0.5) / seg
    reach = 0.045 + 0.022 * math.sin(3 * a + 0.7) + 0.012 * math.sin(7 * a + 2.1) + 0.006 * math.sin(13 * a)
    if x <= CROWN_X + 0.02 + reach:
        return "rubber_dust"
    return "rubber_tyre"


def tread(m):
    """Sipes: fine angled cuts across each rib, staggered rib to rib (dark strips 1 mm proud)."""
    edges = [-(CROWN_X + 0.075)] + [e for g in GROOVES for e in (g - GROOVE_W / 2, g + GROOVE_W / 2)] + [CROWN_X + 0.075]
    ribs = [(edges[k], edges[k + 1]) for k in range(0, len(edges), 2)]
    pitch = 2 * math.pi / SIPES
    for k, (x0, x1) in enumerate(ribs):
        xm = (x0 + x1) / 2
        lean = 0.35 * (1 if xm > 0.01 else -1 if xm < -0.01 else 0)  # chevrons pointing forward
        for i in range(SIPES):
            a0 = pitch * (i + 0.5 * (k % 2))
            quad = []
            for x, da in ((x0 + 0.006, 0.0), (x1 - 0.006, 0.0)):
                aa = a0 + lean * (x - xm) / WHEEL_R
                r, (nx, nr) = tyre_outline(x)
                for w in (-0.0016, 0.0016):
                    ang = aa + w / r
                    quad.append(Vector((x + nx * 0.001, (r + nr * 0.001) * math.cos(ang), (r + nr * 0.001) * math.sin(ang))))
            v = [m.vert(p) for p in (quad[0], quad[2], quad[3], quad[1])]
            # Outward: (x across) x (angle forward) points along the radius.
            m.face(v, "rubber_mud")


def wheel_mesh():
    m = Mesh()
    prof, grooves = tyre_profile()
    m.lathe(prof, "rubber_tyre", seg=64, mats=lambda i, j: tyre_mats(prof, grooves, i, j))
    tread(m)
    # Rim: barrel, the outer face's dish and ten spokes, hub and lug nuts.
    face_x = HW - 0.05
    m.lathe([(face_x, RIM_R - 0.004), (-HW + 0.04, RIM_R - 0.004)], "metal_frame", seg=40)
    m.lathe([(face_x, RIM_R - 0.002), (face_x, 0.225)], "metal_frame", seg=40)
    m.lathe([(face_x, 0.225), (face_x - 0.06, 0.225)], "metal_frame", seg=40)
    # Back of the dish: closes the rim behind the spokes.
    m.lathe([(face_x - 0.10, 0.0), (face_x - 0.10, RIM_R - 0.004)], "rubber_black", seg=40)
    for i in range(10):
        a = 2 * math.pi * (i + 0.5) / 10
        er = Vector((0, math.cos(a), math.sin(a)))
        et = Vector((0, -math.sin(a), math.cos(a)))
        c = Vector((face_x - 0.028, 0, 0)) + er * 0.15
        m.box(c, (0.024, 0.078, 0.017), "metal_graphite", axes=(Vector((1, 0, 0)), er, et))
    m.tube((face_x - 0.06, 0, 0), (face_x - 0.005, 0, 0), 0.08, "metal_graphite", seg=20)
    m.tube((face_x - 0.005, 0, 0), (face_x + 0.012, 0, 0), 0.042, "metal_chrome", seg=16)
    for i in range(6):
        a = 2 * math.pi * i / 6
        p = Vector((face_x - 0.005, 0.058 * math.cos(a), 0.058 * math.sin(a)))
        m.tube(p, p + Vector((0.014, 0, 0)), 0.009, "metal_steel", seg=6)
    # Beadlock rings (orange) with their bolts, outer and inner face.
    for xr, sgn in ((HW - 0.034, 1), (-HW + 0.034, -1)):
        m.lathe([(xr, 0.264), (xr + sgn * 0.012, 0.262), (xr + sgn * 0.012, 0.230), (xr, 0.228)] if sgn > 0 else [(xr, 0.228), (xr + sgn * 0.012, 0.230), (xr + sgn * 0.012, 0.262), (xr, 0.264)], "paint_orange", seg=40)
        for i in range(16):
            a = 2 * math.pi * i / 16
            p = Vector((xr + sgn * 0.012, 0.246 * math.cos(a), 0.246 * math.sin(a)))
            m.tube(p, p + Vector((sgn * 0.007, 0, 0)), 0.0065, "metal_steel", seg=6)
    return m


def damper_parts(pt):
    """Damper body (origin at the top mount), rod (origin at the bottom mount) and spring (origin
    at its lower seat), each in the game frame relative to its origin."""
    u, s0, u0 = pt["u"], pt["s0"], pt["u0"]
    body_len, rod_len = damper_lengths(pt)
    body = Mesh()
    body.tube((0, 0, 0), -u * body_len, 0.034, "metal_graphite", seg=16)
    body.tube((0, 0, 0), -u * 0.05, 0.04, "metal_frame", seg=16)
    body.tube(-u * 0.055, -u * 0.07, 0.058, "metal_copper", seg=20)  # upper spring seat
    body.tube(-u * (body_len - 0.02), -u * body_len, 0.04, "metal_copper", seg=16)
    body.tube((0, 0, -0.025), (0, 0, 0.025), 0.022, "metal_steel", seg=12)
    rod = Mesh()
    rod.tube((0, 0, 0), u * rod_len, 0.014, "metal_chrome", seg=10)
    rod.tube((0, 0, -0.025), (0, 0, 0.025), 0.022, "metal_steel", seg=12)
    rod.tube(u * 0.085, u * 0.10, 0.058, "metal_copper", seg=20)  # lower spring seat
    spring = Mesh()
    spring.helix((0, 0, 0), u, (u0 - s0).length, 0.05, 0.010, 7, "metal_copper", steps_per_turn=16, ring=7)
    return body, rod, spring


def tierod_mesh(pt):
    m = Mesh()
    d = pt["tie_out"] - pt["tie_in"]
    m.tube((0, 0, 0), d, 0.014, "metal_chrome", seg=10)
    m.tube((0, 0, 0), d.normalized() * 0.08, 0.026, "rubber_black", seg=12)
    m.sphere(d, 0.02, "metal_frame", seg=10, rings=6)
    return m


def build_corner(name, sx, zc, coll, mats, root):
    pt = corner_points(sx, zc)
    front = zc > 0
    flip = (lambda m: m) if sx > 0 else (lambda m: m.mirrored())
    c = pt["c"]

    base = empty(f"ctrl_base.{name}", c, coll, root, size=0.02)
    ctrl = empty(f"ctrl.{name}", c, coll, base, kind="SINGLE_ARROW", size=0.35)
    ctrl.lock_location = (True, True, False)

    arm_lo = to_object(f"arm_lo.{name}", flip(arm_mesh(True, front)), pt["p_lo"], coll, mats, root, relative=True)
    a0 = ARM_REST
    a_down = a0 - math.asin(math.sin(a0) - DOWN / ARM_L)  # how far the arm swings down
    t = arm_lo.constraints.new("TRANSFORM")
    t.target = ctrl
    t.owner_space = "LOCAL"
    t.target_space = "LOCAL"
    t.map_from = "LOCATION"
    t.map_to = "ROTATION"
    t.use_motion_extrapolate = True
    # A linear map through zero at rest (so the exported rest pose is the modelled one): exact at
    # full droop, a little short of full bump. The game solves the exact angle.
    t.from_min_z = -DOWN
    t.from_max_z = DOWN
    t.map_to_x_from = "X"
    t.map_to_y_from = "Z"
    t.map_to_z_from = "X"
    # Rotating about Blender +Y lowers the +X side.
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
    to_object(f"wheel.{name}", flip(wheel_mesh()), c, coll, mats, upright, relative=True)

    top = empty(f"mount_top.{name}", pt["top"], coll, root)
    body, rod_m, spring_m = damper_parts(pt if sx > 0 else corner_points(1.0, zc))
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

    if front:
        out = empty(f"tierod_out.{name}", pt["tie_out"], coll, upright)
        d = pt["tie_out"] - pt["tie_in"]
        tm = tierod_mesh(pt if sx > 0 else corner_points(1.0, zc))
        if sx < 0:
            tm = tm.mirrored()
        tie = to_object(f"tierod.{name}", tm, pt["tie_in"], coll, mats, root, relative=True, rot=axes_matrix(y=d))
        st = tie.constraints.new("STRETCH_TO")
        st.target = out
        st.rest_length = d.length
        st.volume = "NO_VOLUME"
    return ctrl, hub


def add_references(coll):
    """The registered plans (with their metric grid and target tyres) as image empties behind the
    car, at the exact scale and place of the blueprint canvases (tools/blender/blueprint.py): in
    the orthographic Left, Right, Front, Back and Top views of Blender the model sits on them."""
    for view, v in bp.VIEWS.items():
        path = os.path.join(bp.REGISTERED, f"{view}_grid.png")
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
        back = right.cross(-down)  # toward the viewer
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
    coll = bpy.data.collections.new("Buggy")
    scene.collection.children.link(coll)
    refs = bpy.data.collections.new("References")
    scene.collection.children.link(refs)
    stage = bpy.data.collections.new("Stage")
    scene.collection.children.link(stage)

    root = bpy.data.objects.new("buggy", None)
    root.empty_display_type = "ARROWS"
    root.empty_display_size = 0.4
    coll.objects.link(root)
    WORLD[root.name] = Matrix.Identity(4)
    root["wheel_radius"] = WHEEL_R
    root["tyre_width"] = TYRE_W
    root["rest_suspension"] = REST
    root["travel"] = TRAVEL

    body = []
    for name, mesh in buggy_body.body_parts().items():
        chamfer = 0.006 if name in buggy_body.PANELS else None
        body.append(to_object(name, mesh, (0, 0, 0), coll, mats, root, uv_fn=buggy_livery.assign_uvs, chamfer=chamfer))

    rig = {}
    for name, (sx, zc) in CORNERS.items():
        rig[name] = build_corner(name, sx, zc, coll, mats, root)
    bpy.context.view_layer.update()
    if livery:
        buggy_livery.bake(body, mats, LIVERY_OUT)
    add_references(refs)
    return scene, coll, root, rig, stage, mats


# --- Preview animation: a jump, a landing, a hard left turn. ---
def animate(scene, root, rig):
    scene.render.fps = 24
    scene.frame_start, scene.frame_end = 1, 120
    root.rotation_mode = "XYZ"

    def key(frame, z=0.0, dy=None, roll=0.0, steer=0.0):
        root.location = (0, 0, z)
        root.rotation_euler = (0, -math.radians(roll), 0)
        root.keyframe_insert("location", frame=frame)
        root.keyframe_insert("rotation_euler", frame=frame)
        for name, (ctrl, hub) in rig.items():
            d = dy if not isinstance(dy, dict) else dy[name]
            ctrl.location = (0, 0, d if d is not None else 0.0)
            ctrl.keyframe_insert("location", index=2, frame=frame)
            if name.startswith("F"):
                hub.rotation_euler = (0, 0, steer)
                hub.keyframe_insert("rotation_euler", index=2, frame=frame)

    turn = {"FL": -0.125, "RL": -0.125, "FR": 0.125, "RR": 0.125}
    key(1)
    key(12)
    key(15, z=0.35, dy=-DOWN)
    key(24, z=0.95, dy=-DOWN)
    key(33, z=DOWN, dy=-DOWN)
    key(35, z=-0.22, dy=0.22)
    key(38, z=0.17, dy=-0.17)
    key(42, z=-0.09, dy=0.09)
    key(46, z=0.035, dy=-0.035)
    key(52)
    key(64)
    key(72, dy=turn, roll=8.0, steer=0.42)
    key(96, dy=turn, roll=8.0, steer=0.42)
    key(106)
    key(120)
    for name in rig:
        w = bpy.data.objects[f"wheel.{name}"]
        w.rotation_mode = "XYZ"
        w.rotation_euler = (0, 0, 0)
        w.keyframe_insert("rotation_euler", index=0, frame=1)
        w.rotation_euler = (12.0, 0, 0)
        w.keyframe_insert("rotation_euler", index=0, frame=120)
        for fc in w.animation_data.action.fcurves if hasattr(w.animation_data.action, "fcurves") else []:
            for kp in fc.keyframe_points:
                kp.interpolation = "LINEAR"
    scene.frame_set(1)


def stage_scene(scene, stage, mats):
    """Ground, sun, sky, and the preview cameras."""
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
    b.inputs["Base Color"].default_value = (*srgb(176, 92, 56), 1)
    b.inputs["Roughness"].default_value = 0.95
    me.materials.append(ground_mat)
    g = bpy.data.objects.new("ground", me)
    stage.objects.link(g)

    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 4.5
    sun.color = (1.0, 0.92, 0.82)
    sun.angle = math.radians(2.0)
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (math.radians(50), math.radians(-10), math.radians(-35))
    stage.objects.link(so)
    fill = bpy.data.lights.new("fill", "AREA")
    fill.energy = 600
    fill.size = 6
    fill.color = (0.85, 0.9, 1.0)
    fo = bpy.data.objects.new("fill", fill)
    fo.location = (-5, 4, 5)
    fo.rotation_euler = (math.radians(-45), math.radians(-40), 0)
    stage.objects.link(fo)

    world = bpy.data.worlds.new("mars")
    use_nodes(world)
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (*srgb(214, 160, 120), 1)
    bg.inputs["Strength"].default_value = 0.7
    scene.world = world

    cams = {}
    for name, loc, look, lens in [
        ("cam_hero", (4.4, -5.2, 0.6), (0, -0.2, -0.15), 42),
        ("cam_side", (7.4, 0.0, 0.1), (0, 0, 0.05), 40),
        ("cam_front", (0.0, -7.6, 0.05), (0, 0, -0.2), 55),
        ("cam_rear", (-4.2, 5.0, 1.5), (0, 0, -0.3), 45),
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


def render(scene, cams, out_dir, video):
    os.makedirs(out_dir, exist_ok=True)
    shots = [("1_repos", "cam_hero", 1), ("2_en_lair", "cam_side", 24), ("3_reception", "cam_hero", 35), ("4_virage", "cam_front", 84), ("5_arriere", "cam_rear", 1)]
    for label, cam, frame in shots:
        scene.frame_set(frame)
        scene.camera = cams[cam]
        scene.render.filepath = os.path.join(out_dir, f"{label}.png")
        bpy.ops.render.render(write_still=True)
    if video:
        scene.timeline_markers.new("hero", frame=1).camera = cams["cam_hero"]
        scene.timeline_markers.new("front", frame=58).camera = cams["cam_front"]
        frames = os.path.join(out_dir, "frames")
        os.makedirs(frames, exist_ok=True)
        scene.render.resolution_x, scene.render.resolution_y = 1280, 720
        scene.eevee.taa_render_samples = 16
        scene.render.filepath = os.path.join(frames, "f_")
        scene.render.image_settings.file_format = "PNG"
        bpy.ops.render.render(animation=True)
        os.system(f'ffmpeg -loglevel error -y -framerate 24 -i "{frames}/f_%04d.png" -c:v libx264 -pix_fmt yuv420p -crf 20 "{video}"')


def main():
    scene, coll, root, rig, stage, mats = build(livery="--no-livery" not in ARGS)
    animate(scene, root, rig)
    cams = stage_scene(scene, stage, mats)
    export(scene, coll, root)
    os.makedirs(os.path.dirname(BLEND_OUT), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=BLEND_OUT, relative_remap=True)
    out = arg("--render")
    if out:
        render(scene, cams, out, arg("--video"))
    tris = sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in coll.all_objects if o.type == "MESH")
    print(f"buggy: {len([o for o in coll.all_objects if o.type == 'MESH'])} meshes, {tris} triangles -> {GLB_OUT}")


main()
