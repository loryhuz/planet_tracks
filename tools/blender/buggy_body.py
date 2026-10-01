"""The body of buggy "B" (art/buggy/v2): hard-surface panels modelled after the registered plans
(art/buggy/v2/registered, see tools/blender/warp_refs.py), in the game's car frame.

Panels are built from character lines: each panel is a grid of rows (stations along the car) by
lines (creases running along it), every point read off the plans as (half-width x, height above
the ground h, station z). Neighbouring facets meet at sharp creases, as on the design. The left
half is modelled and mirrored.

Where the plans disagree (they are generated images), lengths and heights come from the left side
view, widths from the top view, then the front view; the back view only for what the others do not
show. Measurements (metres):
- side silhouette, top: fender ridge h 1.08 at z 1.0 down to 0.865 at the blade tip (z 1.735);
  windshield from (z 0.625, h 1.10) to (0.40, 1.335); roof 1.40 from z 0.2 to -0.45, down to 1.26
  at z -0.63; roof scoop z -0.40..0.08 up to h 1.55; rear fenders z -0.77..-1.27, h 0.99..1.14;
  tail lights to z -1.45; wing from z -1.08 to the endplates' rear at -1.73, top h 1.58;
- side, lower: sills h 0.355..0.46 from z -0.36 to 0.70; door crease h 0.84; the blade's lower edge
  h 0.975 behind z 1.2, down to 0.865 at the tip; headlights z 1.60..1.725, h 0.825..0.855;
- top: pods 0.657 wide from z -0.45 to 0.0, 0.62 at z 0.45, 0.555 at 0.70, fenders 0.52 at 0.90,
  0.47 at 1.50, 0.36 at 1.70; cabin roof 0.30, its sides 0.38..0.49; rear fenders x 0.30..0.60;
- front: headlight slits x 0.24..0.42; prow plate x ±0.15; skid plate x ±0.16, h 0.28..0.66.
"""

from mathutils import Vector

from meshkit import Mesh, P

LIV = "livery"
BLACK = "paint_black"
FRAME = "metal_frame"
GLASS = "glass"


def rows_of(table):
    """Rows of car-frame points from rows of (x, h, z)."""
    return [[P(*p) for p in row] for row in table]


def mirror_x(pts):
    return [Vector((-p.x, p.y, p.z)) for p in pts]


# --- Upper hull: the deck of the side pods, the fender blades and the hood, one grid. Lines from
# the centre line outward: 0 hood centre, 1 edge of the hood's spine, 2 the valley with the fender
# vents, 3 the ridge (the pods' shoulder, then the blades' top edge), 4 the crease (the doors'
# top, then the blades' lower edge), 5 tucked in under the blade. Rows from the pods' rear to the
# blades' tips; the prow is `nose()`.
HULL = [
    # x, h, z per line
    [(0.0, 1.090, -0.58), (0.13, 1.090, -0.58), (0.30, 1.090, -0.58), (0.55, 1.085, -0.58), (0.63, 0.84, -0.40), (0.63, 0.84, -0.40)],
    [(0.0, 1.090, -0.36), (0.13, 1.090, -0.36), (0.30, 1.090, -0.36), (0.555, 1.088, -0.36), (0.657, 0.84, -0.36), (0.657, 0.84, -0.36)],
    [(0.0, 1.095, 0.30), (0.13, 1.095, 0.30), (0.30, 1.095, 0.30), (0.55, 1.095, 0.30), (0.645, 0.84, 0.30), (0.645, 0.84, 0.30)],
    [(0.0, 1.10, 0.45), (0.13, 1.10, 0.45), (0.30, 1.10, 0.45), (0.54, 1.10, 0.45), (0.622, 0.84, 0.45), (0.622, 0.84, 0.45)],
    [(0.0, 1.10, 0.60), (0.13, 1.10, 0.60), (0.30, 1.10, 0.60), (0.53, 1.10, 0.60), (0.585, 0.85, 0.60), (0.585, 0.85, 0.60)],
    [(0.0, 1.10, 0.75), (0.13, 1.10, 0.75), (0.30, 1.095, 0.75), (0.515, 1.10, 0.75), (0.545, 0.97, 0.75), (0.43, 0.95, 0.75)],
    [(0.0, 1.085, 0.90), (0.13, 1.085, 0.90), (0.29, 1.07, 0.90), (0.505, 1.105, 0.90), (0.525, 0.975, 0.90), (0.40, 0.95, 0.90)],
    [(0.0, 1.055, 1.05), (0.13, 1.055, 1.05), (0.28, 1.04, 1.05), (0.495, 1.072, 1.05), (0.515, 0.975, 1.05), (0.39, 0.95, 1.05)],
    [(0.0, 1.025, 1.20), (0.13, 1.025, 1.20), (0.27, 1.012, 1.20), (0.485, 1.041, 1.20), (0.50, 0.965, 1.20), (0.38, 0.945, 1.20)],
    [(0.0, 0.99, 1.35), (0.14, 0.99, 1.35), (0.26, 0.98, 1.35), (0.47, 1.003, 1.35), (0.48, 0.945, 1.35), (0.37, 0.93, 1.35)],
    [(0.0, 0.955, 1.48), (0.145, 0.955, 1.48), (0.25, 0.948, 1.48), (0.45, 0.965, 1.48), (0.455, 0.918, 1.48), (0.36, 0.905, 1.48)],
    [(0.0, 0.925, 1.58), (0.15, 0.925, 1.58), (0.24, 0.92, 1.58), (0.42, 0.93, 1.58), (0.425, 0.892, 1.58), (0.35, 0.88, 1.58)],
    [(0.0, 0.888, 1.65), (0.155, 0.888, 1.65), (0.23, 0.888, 1.65), (0.38, 0.897, 1.65), (0.385, 0.876, 1.65), (0.32, 0.87, 1.65)],
    [(0.0, 0.855, 1.72), (0.15, 0.855, 1.72), (0.20, 0.857, 1.718), (0.27, 0.866, 1.712), (0.275, 0.856, 1.712), (0.24, 0.852, 1.712)],
]


def hull():
    m = Mesh()
    rows = rows_of(HULL)
    ids = m.grid(rows, LIV, mats=lambda i, j: BLACK if j == 4 else LIV)
    # Pods' rear: the deck's rear edge down to the crease.
    m.face([ids[0][2], ids[0][3], ids[0][4]], LIV)
    m.face([ids[0][0], ids[0][1], ids[0][2]], LIV)
    return m.both()


# --- Doors: the pods' sides from the crease down to the sills; the doors' front corner is raked
# in toward the front wheel.
DOOR = [
    # crease, lower edge, under the door
    [(0.63, 0.84, -0.40), (0.61, 0.47, -0.37), (0.46, 0.46, -0.37)],
    [(0.657, 0.84, -0.36), (0.63, 0.47, -0.33), (0.46, 0.46, -0.33)],
    [(0.645, 0.84, 0.30), (0.625, 0.47, 0.30), (0.46, 0.46, 0.30)],
    [(0.622, 0.84, 0.45), (0.605, 0.47, 0.45), (0.46, 0.46, 0.45)],
    [(0.585, 0.85, 0.60), (0.575, 0.47, 0.60), (0.46, 0.46, 0.60)],
    [(0.555, 0.86, 0.70), (0.55, 0.47, 0.70), (0.46, 0.46, 0.70)],
]


def doors():
    m = Mesh()
    m.grid(rows_of(DOOR), LIV, mats=lambda i, j: LIV if j == 0 else BLACK)
    side = Mesh()
    # The doors' front face, from the raked corner in to the frame, up to the blade's tuck.
    side.poly([P(0.55, 0.47, 0.70), P(0.555, 0.86, 0.70), P(0.545, 0.97, 0.75), P(0.43, 0.95, 0.75), P(0.46, 0.46, 0.70)], BLACK)
    return m.both().add(side.both())


def sills():
    """Black box beams under the doors."""
    m = Mesh()
    prof = [(0.45, 0.355), (0.625, 0.36), (0.645, 0.405), (0.63, 0.462), (0.45, 0.462)]
    rows = []
    for z in (-0.37, -0.34, 0.67, 0.70):
        e = 0.012 if z in (-0.37, 0.70) else 0.0
        rows.append([P(x - (e if x > 0.5 else 0), h + (e if h < 0.4 else -e if h > 0.45 else 0), z) for x, h in prof])
    rows = [list(reversed(r)) for r in rows]
    ids = m.grid(rows, FRAME)
    m.cap(ids[0], FRAME, flip=True)
    m.cap(ids[-1], FRAME)
    return m.both()


# --- Cabin. Lines: 0 roof centre, 1 the roof's flat edge, 2 the roof's side edge, 3 the top of
# the side face (window line), 4 its bottom, 5 the base on the pods. Rows from the windshield's
# base back; the swan panel's front edge and the window's rear edge lean, so their rows are
# slanted (the side face's lower points sit further from the roof's).
CABIN = [
    [(0.0, 1.10, 0.625), (0.30, 1.10, 0.625), (0.40, 1.10, 0.62), (0.45, 1.10, 0.615), (0.48, 1.10, 0.61), (0.49, 1.095, 0.60)],
    [(0.0, 1.34, 0.40), (0.26, 1.338, 0.40), (0.33, 1.33, 0.40), (0.36, 1.318, 0.40), (0.47, 1.12, 0.40), (0.49, 1.095, 0.40)],
    [(0.0, 1.385, 0.22), (0.28, 1.385, 0.22), (0.345, 1.37, 0.22), (0.375, 1.335, 0.22), (0.47, 1.12, 0.30), (0.495, 1.095, 0.31)],
    [(0.0, 1.40, 0.0), (0.28, 1.40, 0.0), (0.35, 1.378, 0.0), (0.38, 1.335, 0.0), (0.47, 1.12, 0.0), (0.495, 1.095, 0.0)],
    [(0.0, 1.40, -0.38), (0.28, 1.40, -0.38), (0.35, 1.378, -0.38), (0.38, 1.335, -0.33), (0.47, 1.12, -0.47), (0.495, 1.095, -0.48)],
    [(0.0, 1.16, -0.66), (0.28, 1.16, -0.66), (0.35, 1.16, -0.665), (0.38, 1.155, -0.67), (0.47, 1.12, -0.67), (0.495, 1.095, -0.67)],
]


def cabin():
    m = Mesh()
    rows = rows_of(CABIN)[::-1]  # rear to front, so the faces point out
    n = len(rows) - 2

    def mat(i, j):
        i = n - i  # the table's row order
        if i == 0:
            return GLASS if j <= 1 else (FRAME if j == 2 else GLASS if j == 3 else BLACK)
        if j <= 1:
            return GLASS if i == 4 else LIV
        if j == 2:
            return LIV
        if j == 3:
            return {1: GLASS, 3: BLACK}.get(i, LIV)
        return BLACK

    ids = m.grid(rows, LIV, mats=mat)
    # The rear wall under the rear window, down to the deck (one side; mirrored below).
    m.face(ids[0] + [m.vert(P(0.49, 1.09, -0.675)), m.vert(P(0.0, 1.09, -0.675))], BLACK)
    # The side window: the glass a little proud of the black frame.
    a0, a1 = Vector(CABIN[3][3]), Vector(CABIN[3][4])  # front edge: top and bottom (x, h, z)
    b0, b1 = Vector(CABIN[4][3]), Vector(CABIN[4][4])  # rear edge
    out = Vector((a1.y - a0.y, a0.x - a1.x)).normalized()  # outward in (x, h)

    def on_band(u, v):
        p = a1.lerp(b1, u).lerp(a0.lerp(b0, u), v)
        return P(p.x + out.x * 0.004, p.y + out.y * 0.004, p.z)

    m.poly([on_band(0.07, 0.12), on_band(0.88, 0.12), on_band(0.88, 0.88), on_band(0.09, 0.88)], GLASS)
    return m.both()


# --- Nose: the prow between the blades (its white top carries the hood's 23, the black plate
# below it the Aurora logo), the headlight housings under the blades' tips with their LED slits.
def nose():
    m = Mesh()
    # Black plate under the hood's front edge, leaning back: its foot sticks out ahead of the
    # headlights. The hood's front slope above it carries the 23.
    top, foot = (0.855, 1.72), (0.70, 1.785)  # (h, z)
    pt, pf = (0.15, 0.115)  # half-widths at its top and foot
    m.poly([P(-pf, foot[0], foot[1]), P(pf, foot[0], foot[1]), P(pt, top[0], top[1]), P(-pt, top[0], top[1])], LIV)
    # Under the plate's foot, back to the skid plate.
    m.poly([P(pf, foot[0], foot[1]), P(-pf, foot[0], foot[1]), P(-0.15, 0.665, 1.72), P(0.15, 0.665, 1.72)], BLACK)
    side = Mesh()
    # Blade tip's end: a small white facet between the hood's last row and the headlight.
    tip = [P(*p) for p in HULL[-1][1:]]
    side.face([side.vert(p) for p in reversed(tip)] + [side.vert(P(0.15, 0.845, 1.72))][::-1], LIV)
    # Headlight housing: a black wedge under the blade, its front face along the diagonal out to
    # the blade's side, the LED slit in it.
    a, b = Vector((0.25, 1.69)), Vector((0.43, 1.585))  # (x, z) ends of the front face
    d = (b - a).normalized()
    n = Vector((-d.y, d.x))  # outward normal in (x, z): forward and out
    lo, hi = 0.80, 0.856
    back = -n * 0.10

    def q(v, h):
        return P(v.x, h, v.y)

    side.poly([q(a, lo), q(b, lo), q(b, hi), q(a, hi)], BLACK)
    side.poly([q(b, lo), q(b + back, lo), q(b + back, hi), q(b, hi)], BLACK)
    side.poly([q(a + back, lo), q(b + back, lo), q(b, lo), q(a, lo)], BLACK)
    e = n * 0.004
    a2, b2 = a + d * 0.015, b - d * 0.015
    side.poly([q(a2 + e, 0.814), q(b2 + e, 0.817), q(b2 + e, 0.843), q(a2 + e, 0.841)], "glow_white")
    # Cheek: a white facet from the plate's side edge back out to the headlight, down to the chin.
    cheek = [P(pf, foot[0], foot[1]), P(0.21, 0.70, 1.67), q(a, lo), q(a, hi), P(0.20, 0.857, 1.718), P(pt, top[0], top[1])]
    side.poly(cheek, LIV)
    # Chin: black facets from the cheek and the housing down to the skid plate.
    side.poly([P(pf, foot[0], foot[1]), P(0.15, 0.665, 1.72), P(0.21, 0.66, 1.62), P(0.21, 0.70, 1.67)], BLACK)
    side.poly([P(0.21, 0.70, 1.67), P(0.21, 0.66, 1.62), P(0.32, 0.64, 1.48), q(b, lo), q(a, lo)], BLACK)
    # Wheel-well liner inboard of the front wheel: dark, from the doors' front face along the
    # blade's underside, its lower edge following the tyre.
    import math as _m

    arc = []
    for k in range(13):
        t = _m.radians(20 + 140 * k / 12)
        arc.append(P(0.40, 0.45 + 0.47 * _m.sin(t), 1.30 + 0.47 * _m.cos(t)))
    top = [P(0.40, h, z) for z, h in ((0.75, 0.95), (0.90, 0.95), (1.20, 0.945), (1.48, 0.905), (1.66, 0.868), (1.74, 0.80))]
    liner = [P(0.40, 0.47, 0.72)] + top + arc + [P(0.40, 0.47, 0.80)]  # around the arch, seen from +X
    side.poly(liner, BLACK)
    side.poly([Vector((0.399, p.y, p.z)) for p in reversed(liner)], BLACK)
    m.add(side.both())
    return m


# --- Tail: the white rear fenders over the rear coilovers (pointed at the front), the black
# tail-light pods behind them with their red LED bars, the hexagonal panel between them.
def tail():
    m = Mesh()
    side = Mesh()
    # Rear fender: lines inner top, outer top, outer bottom; rows from the rear to the point.
    rows = [
        [(0.30, 1.14, -1.27), (0.585, 1.14, -1.27), (0.60, 0.99, -1.25)],
        [(0.30, 1.14, -1.00), (0.585, 1.14, -1.00), (0.60, 0.995, -1.00)],
        [(0.32, 1.14, -0.86), (0.575, 1.14, -0.86), (0.59, 1.045, -0.86)],
        [(0.44, 1.10, -0.77), (0.46, 1.10, -0.77), (0.46, 1.075, -0.77)],
    ]
    ids = side.grid(rows_of(rows), LIV)
    side.face(ids[0] + [side.vert(P(0.30, 0.99, -1.25))], LIV)
    # Tail-light pod.
    side.box(P(0.43, 1.075, -1.36), (0.17, 0.045, 0.09), BLACK)
    side.poly([P(0.575, 1.055, -1.452), P(0.29, 1.055, -1.452), P(0.29, 1.085, -1.452), P(0.575, 1.085, -1.452)], "glow_red")
    side.tube(P(0.36, 1.135, -1.29), P(0.36, 1.135, -1.43), 0.018, "metal_chrome", seg=10)
    m.add(side.both())
    # Hexagonal rear panel between the pods.
    hexa = [P(0.26, 1.12, -1.40), P(-0.26, 1.12, -1.40), P(-0.28, 1.00, -1.40), P(-0.13, 0.76, -1.38), P(0.13, 0.76, -1.38), P(0.28, 1.00, -1.40)]
    m.prism(list(reversed(hexa)), 0.03, LIV, side_mat=BLACK, back_mat=BLACK)
    m.box(P(0, 0.86, -1.403), (0.12, 0.022, 0.004), "metal_grille")
    for s in (1, -1):
        m.tube(P(s * 0.17, 1.03, -1.37), P(s * 0.17, 1.03, -1.425), 0.035, "metal_graphite", seg=14)
    return m


# --- The chassis: the spine carrying the front arms, the bulkheads closing the pods, the
# tubular frames front and rear, the roll bars behind the cabin, the skid plate and the belly.
def frame():
    m = Mesh()
    r = 0.021
    left = [
        # Front truss behind the front wheels and up to the headlight housings.
        [P(0.46, 0.47, 0.70), P(0.17, 0.40, 1.15), P(0.16, 0.40, 1.60)],
        [P(0.45, 0.85, 0.72), P(0.36, 0.86, 1.10), P(0.30, 0.80, 1.52)],
        [P(0.17, 0.62, 0.95), P(0.45, 0.85, 0.72)],
        [P(0.17, 0.40, 1.15), P(0.36, 0.86, 1.10)],
        [P(0.16, 0.62, 1.60), P(0.30, 0.80, 1.52)],
        [P(0.17, 0.62, 0.95), P(0.16, 0.62, 1.60)],
        # Bumper: a bar from under the headlight down to the chin, a round stop at its end.
        [P(0.36, 0.78, 1.58), P(0.22, 0.64, 1.74), P(0.15, 0.62, 1.77)],
        # Rear: lower and upper rails, the posts and diagonals of the side truss, the V under
        # the tail.
        [P(0.45, 0.47, -0.37), P(0.30, 0.44, -1.00), P(0.22, 0.44, -1.42)],
        [P(0.47, 0.97, -0.48), P(0.42, 1.00, -0.80), P(0.32, 1.00, -1.30)],
        [P(0.45, 0.47, -0.37), P(0.47, 0.97, -0.48)],
        [P(0.45, 0.47, -0.37), P(0.42, 1.00, -0.80)],
        [P(0.30, 0.44, -1.00), P(0.42, 1.00, -0.80)],
        [P(0.30, 0.44, -1.00), P(0.32, 1.00, -1.30)],
        [P(0.26, 0.96, -1.38), P(0.09, 0.44, -1.38)],
        # Roll bars from the roof's rear corners down to the tail, the rail under the wing's
        # struts, a post and a stay to the deck.
        [P(0.30, 1.42, -0.38), P(0.33, 1.30, -0.72), P(0.36, 1.20, -0.95), P(0.34, 1.15, -1.22)],
        [P(0.40, 1.22, -0.62), P(0.40, 1.20, -0.92)],
        [P(0.40, 1.08, -0.74), P(0.33, 1.30, -0.72)],
        [P(0.36, 1.20, -0.95), P(0.42, 1.00, -0.80)],
    ]
    for pts in left:
        m.polyline(pts, r, FRAME)
        m.polyline(mirror_x(pts), r, FRAME)
    for x, h, z in ((0.09, 0.44, -1.38), (0.30, 0.44, -1.00), (0.36, 1.20, -0.95), (0.16, 0.62, 1.60), (0.16, 0.40, 1.60)):
        m.tube(P(x, h, z), P(-x, h, z), r, FRAME, caps=False)
    for s in (1, -1):
        m.tube(P(s * 0.15, 0.62, 1.79), P(s * 0.15, 0.62, 1.755), 0.026, "metal_chrome", seg=14)
        m.tube(P(s * 0.15, 0.62, 1.792), P(s * 0.15, 0.62, 1.78), 0.014, "metal_frame", seg=10)
    # Spine under the hood, carrying the front arms' pivots.
    m.box(P(0, 0.52, 1.17), (0.15, 0.12, 0.47), FRAME)
    # Bulkheads: behind the front wheels, and behind the cabin.
    for z, h0, h1, w in ((0.72, 0.40, 0.86, 0.46), (-0.60, 0.45, 1.09, 0.47)):
        pl = [P(w, h0, z), P(w, h1, z), P(-w, h1, z), P(-w, h0, z)]
        m.prism(pl if z > 0 else list(reversed(pl)), 0.02, FRAME)
    side = Mesh()
    side.box(P(0.47, 0.905, 1.17), (0.045, 0.025, 0.045), FRAME)  # front coilover's top mount
    side.box(P(0.36, 1.02, -0.90), (0.045, 0.025, 0.045), FRAME)  # rear one's
    m.add(side.both())
    # Front skid plate with its two honeycomb grilles and the orange-ringed pivots at its top.
    plate = [P(0.165, 0.28, 1.63), P(-0.165, 0.28, 1.63), P(-0.165, 0.66, 1.72), P(0.165, 0.66, 1.72)]
    m.prism(list(reversed(plate)), 0.02, "metal_graphite")
    for h0, h1 in ((0.39, 0.47), (0.50, 0.60)):
        z0 = 1.63 + (h0 - 0.28) / 0.38 * 0.09 + 0.004
        z1 = 1.63 + (h1 - 0.28) / 0.38 * 0.09 + 0.004
        m.poly([P(-0.11, h0, z0), P(0.11, h0, z0), P(0.11, h1, z1), P(-0.11, h1, z1)], "metal_grille")
    for s in (1, -1):
        m.tube(P(s * 0.13, 0.62, 1.71), P(s * 0.13, 0.62, 1.725), 0.03, "paint_orange", seg=14)
        m.tube(P(s * 0.13, 0.62, 1.71), P(s * 0.13, 0.62, 1.73), 0.014, "metal_chrome", seg=10)
    # Belly pans.
    m.box(P(0, 0.30, 1.20), (0.15, 0.012, 0.42), "metal_graphite")
    m.box(P(0, 0.40, 0.10), (0.45, 0.012, 0.60), "metal_graphite")
    m.box(P(0, 0.40, -1.10), (0.20, 0.012, 0.30), "metal_graphite")
    return m


# --- Engine bay behind the cabin: the motor and its gearbox low between the rear arms, the
# inverter under the deck with the radiator on top, the coolant tanks.
def engine_bay():
    m = Mesh()
    side = Mesh()
    # Inverter and battery boxes behind the pods, the motor housing over the rear axle.
    side.box(P(0.34, 0.70, -0.55), (0.10, 0.23, 0.17), "metal_graphite")
    side.box(P(0.36, 0.95, -0.55), (0.08, 0.025, 0.12), "metal_grille")
    side.box(P(0.24, 0.72, -0.86), (0.10, 0.22, 0.14), "metal_graphite")
    m.add(side.both())
    m.tube(P(-0.20, 0.58, -1.02), P(0.20, 0.58, -1.02), 0.15, "metal_graphite", seg=20)
    for s in (1, -1):
        m.tube(P(s * 0.20, 0.58, -1.02), P(s * 0.23, 0.58, -1.02), 0.11, "metal_copper", seg=16)
    m.box(P(0, 0.55, -1.25), (0.12, 0.10, 0.10), "metal_graphite")
    m.box(P(0, 0.86, -0.82), (0.22, 0.10, 0.18), "metal_graphite")
    m.box(P(0, 1.00, -1.08), (0.17, 0.025, 0.15), "metal_grille")
    m.box(P(0, 0.97, -0.68), (0.08, 0.035, 0.05), "metal_copper")
    for s in (1, -1):
        m.tube(P(s * 0.28, 0.62, -0.62), P(s * 0.28, 0.62, -0.90), 0.065, "metal_steel", seg=16)
    return m


# --- Roof: the small intake at the front, the scoop with three fans in a row, its light bar at
# the front and amber lamps at the back, the duct down to the engine bay.
def roof():
    m = Mesh()

    def sec(z, w, bot, top):
        return [P(w, bot, z), P(w, top - 0.02, z), P(w - 0.02, top, z), P(-w + 0.02, top, z), P(-w, top - 0.02, z), P(-w, bot, z)]

    rows = [sec(-0.40, 0.14, 1.40, 1.545), sec(-0.05, 0.14, 1.40, 1.55), sec(0.03, 0.13, 1.42, 1.535), sec(0.08, 0.12, 1.45, 1.52)]
    rows = [list(reversed(r)) for r in rows]
    ids = m.grid(rows, BLACK)
    m.cap(ids[0], BLACK)
    m.cap(ids[-1], BLACK, flip=True)
    for z in (-0.31, -0.20, -0.09):
        c = P(0.0, 1.548, z)
        m.tube(c, c + Vector((0, 0.006, 0)), 0.046, "metal_graphite", seg=20)
        m.tube(c + Vector((0, 0.006, 0)), c + Vector((0, 0.009, 0)), 0.014, "metal_steel", seg=12)
        m.tube(c, c + Vector((0, 0.004, 0)), 0.054, "metal_copper", seg=20, caps=False)
    for s in (1, -1):
        for k in range(2):
            for j in range(2):
                c = P(s * (0.05 + 0.028 * k), 1.47 + 0.026 * j, 0.081)
                m.tube(c, c + Vector((0, 0, 0.005)), 0.01, "glow_white", seg=8)
    m.box(P(0, 1.483, 0.083), (0.022, 0.016, 0.004), "glow_amber")
    for x in (-0.10, -0.05, 0.05, 0.10):
        m.box(P(x, 1.49, -0.403), (0.02, 0.016, 0.004), "glow_amber")
    m.box(P(0, 1.49, -0.403), (0.02, 0.016, 0.004), "glow_red")
    # Small intake at the front of the roof.
    pts = [(0.10, 1.37, 0.37), (0.10, 1.395, 0.16), (0.11, 1.445, 0.18), (0.09, 1.44, 0.33)]
    a = [P(*p) for p in pts]
    b = mirror_x(a)
    m.poly([a[1], a[2], a[3], a[0]], BLACK)
    m.poly([b[0], b[3], b[2], b[1]], BLACK)
    m.poly([a[3], a[2], b[2], b[3]], BLACK)
    m.poly([a[0], a[3], b[3], b[0]], "metal_grille")
    m.poly([a[2], a[1], b[1], b[2]], BLACK)
    # Duct from the scoop's back down to the engine bay.
    m.polyline([P(0, 1.47, -0.40), P(0, 1.40, -0.55), P(0, 1.22, -0.70), P(0, 1.05, -0.78)], 0.045, BLACK, seg=14)
    return m


# --- Hood and fender vents: black recessed panels with louvres.
def vents():
    m = Mesh()
    # Hood spine: a black box raised 2 cm along the hood, louvres on its top.
    spine = [(0.70, 1.10), (0.90, 1.085), (1.05, 1.055), (1.20, 1.025), (1.28, 1.01)]
    for (z0, h0), (z1, h1) in zip(spine, spine[1:]):
        rows = [[P(0.12, h0 - 0.01, z0), P(0.12, h0 + 0.02, z0), P(-0.12, h0 + 0.02, z0), P(-0.12, h0 - 0.01, z0)],
                [P(0.12, h1 - 0.01, z1), P(0.12, h1 + 0.02, z1), P(-0.12, h1 + 0.02, z1), P(-0.12, h1 - 0.01, z1)]]
        m.grid(rows, BLACK)
    for k in range(8):
        z = 0.76 + k * 0.065
        h = 1.10 - 0.09 * max(0.0, (z - 0.80) / 0.48) + 0.023
        m.box(P(0, h, z), (0.10, 0.003, 0.012), "metal_grille")
    m.poly([P(0.12, 1.11, 0.70), P(-0.12, 1.11, 0.70), P(-0.12, 1.09, 0.70), P(0.12, 1.09, 0.70)][::-1], BLACK)
    side = Mesh()
    # Fender vents: grilles lying on the black paint of the valleys.
    for (x0, x1, z0, z1) in ((0.30, 0.46, 0.72, 0.97), (0.29, 0.44, 1.05, 1.32)):
        def h(x, z):
            return 1.098 - 0.105 * max(0.0, (z - 0.85) / 0.6) + 0.004
        pts = [(x1, z0), (x1, z1), (x0, z1 - 0.04), (x0, z0 - 0.04)]
        side.poly([P(x, h(x, z), z) for x, z in pts][::-1], "metal_grille")
    m.add(side.both())
    return m


# --- Wing: the main plane between two big endplates, on two lattice struts leaning back.
WING_X = 0.77
WING_LE = (-1.08, 1.50)  # leading edge z, h
WING_TE = (-1.42, 1.555)
ENDPLATE = ((-1.385, 1.555), (-1.41, 1.338), (-1.64, 1.37), (-1.705, 1.41), (-1.73, 1.58))  # z, h


def wing():
    m = Mesh()
    (zl, hl), (zt, ht) = WING_LE, WING_TE
    prof = [(zl, hl), (zl - 0.04, hl + 0.026), (zt + 0.06, ht + 0.014), (zt, ht + 0.005), (zt, ht - 0.016), (zt + 0.06, ht - 0.016), (zl - 0.05, hl - 0.014)]
    rows = [[P(x, h, z) for z, h in prof] for x in (-WING_X, WING_X)]
    ids = m.grid(rows, "metal_carbon", close=True)
    m.cap(ids[0], "metal_carbon")
    m.cap(ids[-1], "metal_carbon", flip=True)
    m.box(P(0, ht + 0.022, zt + 0.006), (WING_X, 0.018, 0.006), "metal_carbon")
    side = Mesh()
    # Endplate: black, the Aurora mark on its outer face.
    side.prism([P(WING_X + 0.016, h, z) for z, h in ENDPLATE], 0.016, LIV, side_mat="metal_carbon", back_mat="metal_carbon")

    # Strut: a lattice plate leaning back, its outer frame, cross bars and diagonals.
    def at(z, h):
        return P(0.50 + 0.05 * (h - 1.17) / 0.33, h, z)

    c = [at(-0.97, 1.17), at(-1.24, 1.17), at(-1.38, 1.49), at(-1.11, 1.49)]
    bars = [(c[i], c[(i + 1) % 4]) for i in range(4)]
    for t in (0.25, 0.5, 0.75):
        bars.append((c[0].lerp(c[3], t), c[1].lerp(c[2], t)))
    for t in (0.0, 0.25, 0.5, 0.75):
        bars.append((c[0].lerp(c[3], t), c[1].lerp(c[2], t + 0.25)))
    for a, b in bars:
        d = (b - a).normalized()
        n = Vector((1, 0, 0))
        w = n.cross(d).normalized()
        side.box((a + b) / 2, (0.024, 0.011, (b - a).length / 2 + 0.012), "metal_carbon", axes=(w, n, d))
    # Base plate on the deck, mount under the wing.
    side.box(at(-1.105, 1.165), (0.03, 0.012, 0.17), "metal_carbon")
    side.box(at(-1.245, 1.495), (0.03, 0.01, 0.15), "metal_carbon")
    m.add(side.both())
    return m


def body_parts():
    """Name -> mesh of every body panel group (car frame)."""
    return {
        "body_hull": hull(),
        "body_doors": doors(),
        "body_sills": sills(),
        "body_cabin": cabin(),
        "body_nose": nose(),
        "body_tail": tail(),
        "body_frame": frame(),
        "body_engine": engine_bay(),
        "body_roof": roof(),
        "body_vents": vents(),
        "body_wing": wing(),
    }
