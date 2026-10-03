"""The body of the ice planet's car, the open single-seater on skis (art/ice_car: the validated
concepts in concepts/monoplace, the plans in views/), in the game's car frame.

The plans are generated pictures that do not agree with each other: lengths and heights come
from the left side view (its rear axle at -1.3, its ski spindle stretched to +1.3), widths from the
top view scaled across so its rear wheels sit at the physics' track. Measurements (metres):
- side: nose tip (z 2.12, h 0.30..0.335), its top rising to h 0.90 at the windscreen's foot
  (z 0.95); the suspension opening between the nose and the pods (z 0.85..1.5, under h 0.75);
  the pods' raked front edge from (1.03, 0.86) down to (0.80, 0.32), their raked rear edge from
  (-0.45, 0.93) down to (-0.28, 0.31), their floor at h 0.31; the cockpit's sill h 0.945; the
  windscreen up to h 1.19 at z 0.48; the halo's crown h 1.31 at z 0; the roll hoop up to h 1.56
  at z -0.47; the rear deck from h 1.13 behind the seat down to 1.0 at the tail (z -1.32), its
  lower edge h 0.86; the flag pole from (-1.1, 1.05) to h 1.92;
- top: the nose's half-width 0.29 at the spindle, 0.44 at z 0.95; the pods 0.66 wide at
  z 0..0.3; the cockpit +-0.36 from z 0.62 back to -0.36; the deck 0.47 wide behind the seat,
  0.22 at the tail.
"""

import math

from mathutils import Vector

from meshkit import Mesh, P

LIV = "livery"
BLACK = "paint_black"
FRAME = "metal_frame"
GLASS = "glass"


def lerp(a, b, t):
    return a + (b - a) * t


def table_at(rows, z):
    """Row of a table (sorted by its first column, z) interpolated at z."""
    if z <= rows[0][0]:
        return rows[0][1:]
    for a, b in zip(rows, rows[1:]):
        if z <= b[0]:
            t = (z - a[0]) / (b[0] - a[0])
            return tuple(lerp(x, y, t) for x, y in zip(a[1:], b[1:]))
    return rows[-1][1:]


def mirror_x(pts):
    return [Vector((-p.x, p.y, p.z)) for p in pts]


def face_to(m, pts, d, mat, smooth=False):
    """A planar face from points, wound so it faces direction d."""
    pts = [Vector(p) for p in pts]
    n = Vector()
    for a, b in zip(pts, pts[1:] + pts[:1]):
        n += a.cross(b)
    if n.dot(Vector(d)) < 0:
        pts = list(reversed(pts))
    return m.poly(pts, mat, smooth)


# --- Nose: a narrow V hull from the tip back to the cockpit's front bulkhead, its top carrying the
# windscreen. Per station: half-width at the side's widest, top at the centre, keel.
NOSE = [
    # z, w, top h, keel h
    (0.62, 0.47, 0.930, 0.40),
    (0.75, 0.47, 0.925, 0.40),
    (0.85, 0.46, 0.915, 0.40),
    (0.95, 0.44, 0.900, 0.40),
    (1.05, 0.40, 0.880, 0.40),
    (1.15, 0.355, 0.855, 0.40),
    (1.25, 0.32, 0.830, 0.40),
    (1.35, 0.29, 0.805, 0.395),
    (1.45, 0.26, 0.770, 0.385),
    (1.55, 0.225, 0.730, 0.370),
    (1.65, 0.19, 0.675, 0.350),
    (1.75, 0.155, 0.615, 0.335),
    (1.85, 0.12, 0.545, 0.325),
    (1.95, 0.085, 0.470, 0.315),
    (2.05, 0.045, 0.390, 0.305),
    (2.12, 0.015, 0.335, 0.300),
]


def nose_section(z):
    """Left half of the nose at station z, from the top centre down the side to the keel."""
    w, ht, hb = table_at(NOSE, z)
    d = ht - hb
    return [
        P(0.0, ht + 0.006, z),
        P(0.55 * w, ht, z),
        P(0.88 * w, ht - 0.015 - 0.02 * w / 0.47, z),
        P(w, hb + 0.72 * d, z),
        P(0.78 * w, hb + 0.28 * d, z),
        P(0.38 * w, hb + 0.03, z),
        P(0.0, hb, z),
    ]


def nose():
    m = Mesh()
    rows = [nose_section(r[0]) for r in NOSE]
    ids = m.grid(rows, LIV, mats=lambda i, j: LIV if j <= 3 else BLACK)
    m.cap(ids[-1], LIV, flip=True)  # the blunt tip, facing forward
    return m.both()


# --- Side pods: from their raked front edge back to their raked rear edge, wrapping from the
# cockpit's rim over the shoulder, down the side and in under the car.
POD_LINES = [
    # x, h: rim, deck, shoulder, upper side, widest, lower side, floor's edge, under the car
    (0.36, 0.945),
    (0.50, 0.935),
    (0.60, 0.905),
    (0.645, 0.82),
    (0.66, 0.62),
    (0.635, 0.42),
    (0.565, 0.31),
    (0.16, 0.31),
]
POD_FRONT = ((0.80, 0.31), (1.03, 0.905))  # (z, h) ends of the raked front edge
POD_REAR = ((-0.28, 0.31), (-0.45, 0.935))
# How wide the pods are along the car (the outer lines' x scale).
POD_WIDTH = [(-0.45, 0.92), (-0.30, 0.96), (-0.10, 1.0), (0.35, 1.0), (0.60, 0.975), (0.80, 0.90), (1.05, 0.74)]


def raked(edge, h):
    (z0, h0), (z1, h1) = edge
    return z0 + (z1 - z0) * min(1.0, max(0.0, (h - h0) / (h1 - h0)))


def pod_point(j, t):
    """Point on pod line j at fraction t from the rear edge (0) to the front edge (1)."""
    x, h = POD_LINES[j]
    z = lerp(raked(POD_REAR, h), raked(POD_FRONT, h), t)
    if 0 < j < len(POD_LINES) - 1:
        x *= table_at(POD_WIDTH, z)[0]
    return P(x, h, z)


POD_T = [0.0, 0.05, 0.13, 0.26, 0.40, 0.55, 0.70, 0.83, 0.93, 1.0]


def pods():
    m = Mesh()
    rows = [[pod_point(j, t) for j in range(len(POD_LINES))] for t in POD_T]
    ids = m.grid(rows, LIV, mats=lambda i, j: BLACK if j >= 6 else LIV)
    # The raked front face (graphite, facing the suspension) and the rear face.
    m.cap(ids[-1], LIV, flip=True)
    m.cap(ids[0], BLACK)
    return m.both()


# --- Cockpit tub between the pods: the floor, the walls under the rim, the bulkheads.
COCKPIT = (-0.36, 0.62)  # z of its rear and front
RIM_X, RIM_H, FLOOR_H = 0.36, 0.945, 0.36


def cockpit():
    m = Mesh()
    zr, zf = COCKPIT
    face_to(m, [P(RIM_X, FLOOR_H, zr), P(RIM_X, FLOOR_H, zf), P(-RIM_X, FLOOR_H, zf), P(-RIM_X, FLOOR_H, zr)], (0, 1, 0), BLACK)
    side = Mesh()
    # The wall under the left rim, facing in (-X).
    face_to(side, [P(RIM_X, FLOOR_H, zr), P(RIM_X, RIM_H, zr), P(RIM_X, RIM_H, zf), P(RIM_X, FLOOR_H, zf)], (-1, 0, 0), BLACK)
    m.add(side.both())
    # Front bulkhead (the dash's back) facing the driver, rear bulkhead behind the seat.
    face_to(m, [P(RIM_X, FLOOR_H, zf), P(RIM_X, RIM_H, zf), P(-RIM_X, RIM_H, zf), P(-RIM_X, FLOOR_H, zf)], (0, 0, -1), BLACK)
    face_to(m, [P(-RIM_X, FLOOR_H, zr), P(-RIM_X, 1.13, zr), P(RIM_X, 1.13, zr), P(RIM_X, FLOOR_H, zr)], (0, 0, 1), BLACK)
    # Padded rim along the sides and the back.
    for s in (1, -1):
        m.tube(P(s * RIM_X, RIM_H + 0.012, zr + 0.02), P(s * RIM_X, RIM_H + 0.012, zf - 0.03), 0.022, "rubber_black", seg=10)
    return m


# --- Rear deck: the cover over the motor from behind the seat to the tail.
DECK = [
    # z, w, top h, lower edge h
    (-1.32, 0.30, 1.000, 0.78),
    (-1.25, 0.32, 1.010, 0.80),
    (-1.10, 0.35, 1.025, 0.84),
    (-0.90, 0.36, 1.050, 0.86),
    (-0.70, 0.43, 1.085, 0.86),
    (-0.55, 0.47, 1.120, 0.87),
    (-0.45, 0.47, 1.130, 0.88),
    (-0.33, 0.44, 1.130, 0.90),
]


def deck_section(z):
    w, ht, hb = table_at(DECK, z)
    d = ht - hb
    return [
        P(0.0, ht + 0.005, z),
        P(0.42 * w, ht, z),
        P(0.80 * w, ht - 0.03, z),
        P(w, hb + 0.5 * d, z),
        P(0.94 * w, hb, z),
        P(0.0, hb, z),
    ]


def deck():
    m = Mesh()
    rows = [deck_section(r[0]) for r in DECK]
    ids = m.grid(rows, LIV, mats=lambda i, j: BLACK if j >= 4 else LIV)
    m.cap(ids[0], "metal_graphite")  # the tail's face, toward the back
    m.cap(ids[-1], BLACK, flip=True)
    return m.both()


def tail():
    """The tail's face: red light strips at its corners, a grille between them."""
    m = Mesh()
    w, ht, hb = table_at(DECK, -1.32)
    z = -1.323
    side = Mesh()
    # Light strip leaning in toward the top, along the face's outer edge.
    face_to(side, [P(0.21, hb + 0.03, z), P(0.17, hb + 0.03, z), P(0.235, ht - 0.035, z), P(0.275, ht - 0.035, z)], (0, 0, -1), "glow_red")
    m.add(side.both())
    face_to(m, [P(0.08, hb + 0.04, z), P(-0.08, hb + 0.04, z), P(-0.08, hb + 0.12, z), P(0.08, hb + 0.12, z)], (0, 0, -1), "metal_grille")
    return m


# --- Windscreen: a low wrap-around shield on the nose, the halo over the cockpit, the roll hoop
# behind the seat.
def screen_point(phi, v):
    """Point on the windscreen: phi across (-pi/2 right .. pi/2 left), v from foot (0) to top."""
    c, s = math.cos(phi), math.sin(phi)
    foot = Vector((0.37 * s, 0.925 + 0.02 * (1 - c), 0.38 + 0.57 * c))
    top = Vector((0.335 * s, 1.10 + 0.085 * c, 0.22 + 0.26 * c))
    p = foot.lerp(top, v)
    # A slight bulge forward in the middle of its height.
    p += Vector((0.0, 0.0, 0.025 * c * math.sin(math.pi * v)))
    return P(p.x, p.y, p.z)


def windscreen():
    m = Mesh()
    n = 14
    phis = [-math.pi / 2 + math.pi * k / n for k in range(n + 1)]
    vs = [0.0, 0.33, 0.66, 1.0]
    # Two skins a few millimetres apart, so it shows from the front and from the cockpit.
    rows = [[screen_point(phi, v) for v in vs] for phi in phis]  # right to left, foot to top
    m.grid(rows, GLASS)
    inner = [[p + Vector((0, -0.004, -0.004)) for p in row] for row in rows]
    m.grid([list(reversed(r)) for r in inner], GLASS)
    # Black frame along the top edge and the foot.
    top = [screen_point(phi, 1.0) for phi in phis]
    m.polyline(top, 0.012, BLACK, joints=False, seg=8)
    foot = [screen_point(phi, 0.0) + Vector((0, 0.004, 0)) for phi in phis]
    m.polyline(foot, 0.010, BLACK, joints=False, seg=6)
    return m


HALO = [(-0.38, 1.25), (-0.2, 1.29), (0.0, 1.31), (0.2, 1.29), (0.45, 1.20), (0.62, 1.15)]  # z, h


def halo():
    m = Mesh()
    pts = []
    for k in range(32):
        u = 2 * math.pi * k / 32
        z = 0.12 + 0.50 * math.cos(u)
        x = 0.38 * math.sin(u)
        pts.append(P(x, table_at(HALO, z)[0], z))
    m.polyline(pts + pts[:1], 0.026, FRAME, joints=False, seg=10)
    for p in pts[::2]:
        m.sphere(p, 0.0262, FRAME, seg=10, rings=5)
    # Front posts from the ring down onto the cowl beside the windscreen.
    for s in (1, -1):
        m.polyline([P(s * 0.33, 1.17, 0.50), P(s * 0.36, 0.95, 0.47)], 0.022, FRAME, seg=10)
    return m


def roll_hoop():
    """Double hoop behind the seat (the plans show two tubes side by side), its stays back to the
    deck, the antenna on its top left, the dish on its right."""
    m = Mesh()
    r = 0.032
    for dz in (0.0, -0.07):
        z0 = -0.40 + dz
        pts = [P(0.27, 1.10, z0), P(0.255, 1.42, z0 - 0.04), P(0.20, 1.53, z0 - 0.06), P(0.10, 1.565, z0 - 0.07)]
        hoop = pts + [P(-p.x, p.y, p.z) for p in reversed(pts)]
        m.polyline(hoop, r, FRAME, joints=True, seg=12)
    side = Mesh()
    side.polyline([P(0.20, 1.52, -0.52), P(0.25, 1.10, -0.82)], 0.026, FRAME, seg=10)
    side.polyline([P(0.27, 1.25, -0.42), P(0.27, 1.25, -0.49)], 0.022, FRAME, seg=10)
    m.add(side.both())
    return m


def antenna():
    """A whip on the hoop's top left: a base, a spring, the whip, a ball at its tip."""
    m = Mesh()
    base = P(0.16, 1.585, -0.47)
    m.tube(base - Vector((0, 0.02, 0)), base + Vector((0, 0.03, 0)), 0.022, "metal_graphite", seg=12)
    m.helix(base + Vector((0, 0.03, 0)), (0, 1, 0), 0.07, 0.012, 0.004, 7, "metal_steel", steps_per_turn=10, ring=5)
    tip = base + Vector((0, 0.32, -0.01))
    m.tube(base + Vector((0, 0.10, 0)), tip, 0.006, "rubber_black", seg=6)
    m.sphere(tip, 0.011, "rubber_black", seg=8, rings=5)
    return m


def dish():
    """A small satellite dish on a bracket off the hoop's right upright, facing up and back."""
    m = Mesh()
    hub = P(-0.37, 1.43, -0.58)
    axis = Vector((-0.45, 0.55, -0.70)).normalized()
    # Its frame: u, v across the dish, w along the axis.
    u = axis.cross(Vector((0, 1, 0))).normalized()
    v = axis.cross(u)
    R, depth = 0.11, 0.035
    rings, seg = 5, 20
    front, back = [], []
    for i in range(rings + 1):
        rr = R * i / rings
        d = depth * (rr / R) ** 2
        front.append([hub + axis * d + (u * math.cos(t) + v * math.sin(t)) * rr for t in (2 * math.pi * k / seg for k in range(seg))])
        back.append([p - axis * 0.006 for p in front[-1]])
    # The concave face toward +axis: rings outward with angle counter-clockwise about the axis.
    for i in range(rings):
        for k in range(seg):
            n = (k + 1) % seg
            m.face([m.vert(front[i][k]), m.vert(front[i + 1][k]), m.vert(front[i + 1][n]), m.vert(front[i][n])], "paint_white", True)
            m.face([m.vert(back[i][k]), m.vert(back[i + 1][k]), m.vert(back[i + 1][n]), m.vert(back[i][n])][::-1], "paint_white", True)
    for k in range(seg):
        n = (k + 1) % seg
        a, b = front[-1][k], front[-1][n]
        c, d = back[-1][n], back[-1][k]
        m.face([m.vert(a), m.vert(d), m.vert(c), m.vert(b)], "paint_white")
    # The feed on three struts, its head facing the dish.
    feed = hub + axis * 0.12
    for k in range(3):
        t = 2 * math.pi * k / 3 + 0.4
        m.tube(hub + axis * (depth + 0.002) + (u * math.cos(t) + v * math.sin(t)) * (R - 0.004), feed, 0.0035, "metal_steel", seg=5)
    m.tube(feed - axis * 0.012, feed + axis * 0.03, 0.014, "metal_graphite", seg=10)
    # The bracket back to the hoop.
    m.tube(hub - axis * 0.006, hub - axis * 0.05, 0.016, "metal_graphite", seg=10)
    m.polyline([hub - axis * 0.05, P(-0.27, 1.40, -0.47)], 0.014, FRAME, seg=8)
    return m


def flag():
    """The whip pole at the rear right corner with its spring base, and the pennant: an orange
    triangle (painted by the livery, both faces) with a snowflake."""
    m = Mesh()
    base = P(-0.33, 1.03, -1.12)
    top = P(-0.36, 1.93, -1.17)
    m.box(base + Vector((0, -0.01, 0)), (0.035, 0.012, 0.035), "metal_graphite")
    m.helix(base, (top - base).normalized(), 0.09, 0.016, 0.005, 8, "metal_steel", steps_per_turn=10, ring=5)
    m.tube(base + (top - base).normalized() * 0.09, top, 0.009, "rubber_black", seg=6)
    m.sphere(top + (top - base).normalized() * 0.012, 0.016, "rubber_black", seg=8, rings=5)
    # Pennant: from the pole between h 1.78 and 1.92, its point back at z -1.47, rippling a little.
    a = P(-0.36, 1.92, -1.172)
    b = P(-0.355, 1.78, -1.162)
    tip = P(-0.40, 1.855, -1.47)
    cols = 6
    left, right = [], []
    for k in range(cols + 1):
        t = k / cols
        hi = a.lerp(tip, t)
        lo = b.lerp(tip, t)
        bulge = Vector((0.012 * math.sin(math.pi * t * 1.5), 0, 0))
        left.append((hi + bulge, lo + bulge))
    for k in range(cols):
        (h0, l0), (h1, l1) = left[k], left[k + 1]
        # Face toward +X (seen from the left side), then the same toward -X.
        face_to(m, [h0, l0, l1, h1], (1, 0, 0), LIV)
        face_to(m, [h0, l0, l1, h1], (-1, 0, 0), LIV)
    return m


# --- Interior: the seat with its harness, the steering wheel, the dash.
def interior():
    m = Mesh()
    # Seat shell: a base and a reclined back with side bolsters and a headrest.
    m.box(P(0, 0.44, -0.05), (0.21, 0.05, 0.20), BLACK)
    back = Vector((0, 0.95, -0.28))
    up = Vector((0, 0.94, -0.33)).normalized()
    fwd = Vector((0, 0.33, 0.94)).normalized()
    ax = (Vector((1, 0, 0)), up, fwd)
    m.box(P(0, 0.80, -0.27), (0.20, 0.36, 0.035), BLACK, axes=ax)
    for s in (1, -1):
        m.box(P(s * 0.20, 0.78, -0.24), (0.035, 0.32, 0.08), BLACK, axes=ax)
        m.box(P(s * 0.19, 0.52, 0.0), (0.035, 0.05, 0.18), BLACK)
    m.box(P(0, 1.20, -0.35), (0.12, 0.07, 0.05), BLACK, axes=ax)
    # Harness: two shoulder straps down to the buckle, two lap straps (blue).
    buckle = P(0, 0.56, 0.08)
    for s in (1, -1):
        m.polyline([P(s * 0.08, 1.12, -0.33), P(s * 0.09, 1.06, -0.25), P(s * 0.06, 0.82, -0.13), buckle], 0.012, "paint_blue", joints=False, seg=4)
        m.polyline([P(s * 0.19, 0.52, -0.08), buckle], 0.011, "paint_blue", joints=False, seg=4)
    m.box(buckle, (0.03, 0.03, 0.01), "metal_chrome")
    # Steering wheel on its column from the dash, facing the driver.
    centre = P(0, 0.99, 0.30)
    n = Vector((0, 0.45, -0.89)).normalized()  # toward the driver
    u = Vector((1, 0, 0))
    v = n.cross(u)
    ring = [centre + (u * math.cos(t) + v * math.sin(t)) * 0.14 for t in (2 * math.pi * k / 20 for k in range(20))]
    m.polyline(ring + ring[:1], 0.016, "rubber_black", joints=False, seg=8)
    for t in (0.0, math.pi, -math.pi / 2):
        m.tube(centre, centre + (u * math.cos(t) + v * math.sin(t)) * 0.13, 0.012, "metal_graphite", seg=6)
    m.tube(centre - n * 0.01, centre + n * 0.02, 0.04, "metal_graphite", seg=12)
    m.tube(centre - n * 0.01, P(0, 0.90, 0.58), 0.022, "metal_graphite", seg=8)
    # Dash under the cowl, a small screen in it.
    m.box(P(0, 0.86, 0.58), (0.30, 0.07, 0.05), BLACK)
    face_to(m, [P(0.07, 0.85, 0.529), P(-0.07, 0.85, 0.529), P(-0.07, 0.91, 0.529), P(0.07, 0.91, 0.529)], (0, 0, -1), "glow_cyan")
    return m


# --- Chassis: the frames that carry the suspension where the body does not cover them.
def chassis():
    m = Mesh()
    r = 0.022
    left = [
        # Rear subframe: lower and upper rails from the pods back to the tail, posts and
        # diagonals, the cross tubes.
        [P(0.20, 0.33, -0.30), P(0.18, 0.38, -0.90), P(0.17, 0.42, -1.18)],
        [P(0.24, 0.86, -0.42), P(0.22, 0.80, -0.95), P(0.18, 0.76, -1.18)],
        [P(0.18, 0.38, -0.90), P(0.22, 0.80, -0.95)],
        [P(0.20, 0.33, -0.30), P(0.22, 0.80, -0.95)],
        [P(0.17, 0.42, -1.18), P(0.18, 0.76, -1.18)],
        # Front: the lower rails under the nose to its bracket, the pickup posts.
        [P(0.16, 0.33, 0.80), P(0.13, 0.36, 1.60)],
        [P(0.16, 0.36, 1.00), P(0.16, 0.66, 1.05)],
        [P(0.16, 0.36, 1.45), P(0.16, 0.66, 1.40)],
    ]
    for pts in left:
        m.polyline(pts, r, FRAME)
        m.polyline(mirror_x(pts), r, FRAME)
    for x, h, z in ((0.18, 0.38, -0.90), (0.22, 0.80, -0.95), (0.13, 0.36, 1.60)):
        m.tube(P(x, h, z), P(-x, h, z), r, FRAME, caps=False)
    # Front spine carrying the arms' pivots, and its skid plate.
    m.box(P(0, 0.52, 1.25), (0.15, 0.13, 0.30), "metal_graphite")
    m.box(P(0, 0.335, 1.25), (0.13, 0.01, 0.42), "metal_frame")
    # The front coilovers' top mounts: brackets out of the nose's sides.
    side = Mesh()
    side.box(P(0.25, 0.70, 1.31), (0.04, 0.02, 0.04), "metal_frame")
    # The rear ones', under the deck.
    side.box(P(0.38, 0.92, -0.98), (0.05, 0.02, 0.04), "metal_frame")
    side.polyline([P(0.38, 0.92, -0.98), P(0.22, 0.80, -0.95)], 0.018, FRAME, seg=8)
    m.add(side.both())
    return m


def drivetrain():
    """The motor over the rear axle: a housing with vertical cooling fins on its back (seen from
    behind under the tail), the gearbox under it between the drive shafts, the diffuser."""
    m = Mesh()
    m.box(P(0, 0.64, -1.06), (0.15, 0.16, 0.22), "metal_graphite")
    for k in range(9):
        x = -0.12 + k * 0.03
        m.box(P(x, 0.64, -1.29), (0.005, 0.14, 0.02), "metal_steel")
    m.box(P(0, 0.46, -1.30), (0.12, 0.07, 0.08), "metal_graphite")
    for s in (1, -1):
        m.tube(P(s * 0.12, 0.45, -1.30), P(s * 0.16, 0.45, -1.30), 0.05, "metal_steel", seg=12)
    # Diffuser: a carbon plate rising toward the tail, vertical fins under it.
    face_to(m, [P(0.24, 0.31, -1.0), P(-0.24, 0.31, -1.0), P(-0.26, 0.40, -1.42), P(0.26, 0.40, -1.42)], (0, -1, 0), "metal_carbon")
    face_to(m, [P(0.24, 0.315, -1.0), P(-0.24, 0.315, -1.0), P(-0.26, 0.405, -1.42), P(0.26, 0.405, -1.42)], (0, 1, 0), "metal_carbon")
    for x in (-0.20, -0.10, 0.0, 0.10, 0.20):
        m.prism([P(x, 0.315, -1.02), P(x, 0.40, -1.42), P(x, 0.30, -1.42)], 0.01, "metal_carbon")
    return m


def lamps():
    """The cyan light strips: headlights along the nose's upper side facets near its tip (white
    with a cyan tint, the night beam starts between them), strips down the pods' front edges."""
    m = Mesh()
    side = Mesh()
    # Along the nose's upper side facet (between section points 2 and 3), 2 mm proud.
    strip = []
    for z in (1.66, 1.74, 1.82, 1.90, 1.97):
        s = nose_section(z)
        a, b = s[2], s[3]
        n = Vector((b.y - a.y, a.x - b.x, 0)).normalized()  # outward in (x, y)
        strip.append((a.lerp(b, 0.30) + n * 0.002, a.lerp(b, 0.62) + n * 0.002))
    for (a0, b0), (a1, b1) in zip(strip, strip[1:]):
        face_to(side, [a0, b0, b1, a1], (1, 0.3, 0), "glow_white")
    # Down the pods' raked front edge: a strip on the front face's outer border.
    rake = Vector((0, 0.905 - 0.31, 1.03 - 0.80)).normalized()  # up the raked edge
    n = Vector((0, -rake.z, rake.y))  # the front face's normal (forward, down)
    for h0, h1 in ((0.42, 0.80),):
        pts = []
        for h in (h0, h1):
            z = raked(POD_FRONT, h)
            x = 0.60 * table_at(POD_WIDTH, z)[0]
            pts.append(P(x, h, z) + n * 0.003)
        a, b = pts
        d = Vector((0.02, 0, 0))
        face_to(side, [a - d, a + d, b + d, b - d], n, "glow_cyan")
    m.add(side.both())
    return m


# Panels whose creases get a chamfer (meshkit.to_object).
PANELS = ("body_nose", "body_pods", "body_deck")


def body_parts():
    """Name -> mesh of every body part group (car frame)."""
    return {
        "body_nose": nose(),
        "body_pods": pods(),
        "body_cockpit": cockpit(),
        "body_deck": deck(),
        "body_tail": tail(),
        "body_screen": windscreen(),
        "body_halo": halo(),
        "body_hoop": roll_hoop(),
        "body_antenna": antenna(),
        "body_dish": dish(),
        "body_flag": flag(),
        "body_interior": interior(),
        "body_chassis": chassis(),
        "body_drivetrain": drivetrain(),
        "body_lamps": lamps(),
    }
