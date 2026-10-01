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
GLASS = "livery_glass"  # painted panes (buggy_livery.py)


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
    [(0.0, 0.925, 1.58), (0.15, 0.925, 1.58), (0.24, 0.92, 1.58), (0.44, 0.93, 1.58), (0.445, 0.892, 1.58), (0.35, 0.88, 1.58)],
    [(0.0, 0.876, 1.665), (0.15, 0.876, 1.665), (0.23, 0.886, 1.665), (0.40, 0.897, 1.66), (0.405, 0.874, 1.66), (0.33, 0.868, 1.66)],
    [(0.0, 0.8755, 1.666), (0.15, 0.8755, 1.666), (0.22, 0.874, 1.70), (0.34, 0.873, 1.74), (0.345, 0.863, 1.74), (0.30, 0.859, 1.74)],
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
    [(0.0, 1.30, 0.40), (0.26, 1.298, 0.40), (0.33, 1.29, 0.40), (0.36, 1.28, 0.40), (0.47, 1.12, 0.40), (0.49, 1.095, 0.40)],
    [(0.0, 1.37, 0.24), (0.28, 1.37, 0.24), (0.345, 1.355, 0.24), (0.375, 1.32, 0.24), (0.47, 1.12, 0.30), (0.495, 1.095, 0.31)],
    [(0.0, 1.40, 0.0), (0.28, 1.40, 0.0), (0.35, 1.378, 0.0), (0.38, 1.335, 0.0), (0.47, 1.12, 0.0), (0.495, 1.095, 0.0)],
    [(0.0, 1.40, -0.38), (0.28, 1.40, -0.38), (0.35, 1.378, -0.38), (0.38, 1.335, -0.33), (0.47, 1.12, -0.47), (0.495, 1.095, -0.48)],
    [(0.0, 1.13, -0.66), (0.28, 1.13, -0.66), (0.35, 1.13, -0.665), (0.38, 1.13, -0.67), (0.47, 1.115, -0.67), (0.495, 1.095, -0.67)],
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
            return GLASS if i == 1 else LIV
        if j == 2:
            return FRAME if i == 1 else LIV
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
    top, foot = (0.875, 1.665), (0.70, 1.785)  # (h, z)
    pt, pf = (0.15, 0.115)  # half-widths at its top and foot
    m.poly([P(-pf, foot[0], foot[1]), P(pf, foot[0], foot[1]), P(pt, top[0], top[1]), P(-pt, top[0], top[1])], LIV)
    # Under the plate's foot, back to the skid plate.
    m.poly([P(pf, foot[0], foot[1]), P(-pf, foot[0], foot[1]), P(-0.15, 0.665, 1.72), P(0.15, 0.665, 1.72)], BLACK)
    side = Mesh()
    # Headlight housing: a black wedge under the blade's tip, its front face looking forward
    # and out, the LED slit in it.
    a, b = Vector((0.31, 1.735)), Vector((0.465, 1.60))  # (x, z) ends of the front face
    d = (b - a).normalized()
    n = Vector((-d.y, d.x))  # outward normal in (x, z): forward and out
    lo, hi = 0.80, 0.858
    back = -n * 0.10

    def q(v, h):
        return P(v.x, h, v.y)

    side.poly([q(a, lo), q(b, lo), q(b, hi), q(a, hi)], BLACK)
    side.poly([q(b, lo), q(b + back, lo), q(b + back, hi), q(b, hi)], BLACK)
    side.poly([q(a + back, lo), q(b + back, lo), q(b, lo), q(a, lo)], BLACK)
    e = n * 0.004
    a2, b2 = a + d * 0.015, b - d * 0.015
    side.poly([q(a2 + e, 0.814), q(b2 + e, 0.817), q(b2 + e, 0.845), q(a2 + e, 0.843)], "glow_white")
    # The blade tip's front, closing it down onto the headlight.
    tip = [P(*p) for p in HULL[-1][2:]]
    side.face([side.vert(p) for p in reversed(tip)] + [side.vert(q(a, hi))][::-1], LIV)
    # Cheek: a white facet from the plate's side edge back out to the headlight, down to the chin.
    cheek = [P(pf, foot[0], foot[1]), P(0.21, 0.70, 1.72), q(a, lo), q(a, hi), P(*HULL[-1][2]), P(pt, top[0], top[1])]
    side.poly(cheek, LIV)
    # Chin: black facets from the cheek and the housing down to the skid plate.
    side.poly([P(pf, foot[0], foot[1]), P(0.15, 0.665, 1.72), P(0.22, 0.66, 1.65), P(0.21, 0.70, 1.72)], BLACK)
    side.poly([P(0.21, 0.70, 1.72), P(0.22, 0.66, 1.65), P(0.34, 0.64, 1.50), q(b, lo), q(a, lo)], BLACK)
    # Wheel-well liner inboard of the front wheel: dark, from the doors' front face along the
    # blade's underside, its lower edge following the tyre.
    import math as _m

    arc = []
    for k in range(11):
        t = _m.radians(58 + 102 * k / 10)
        arc.append(P(0.40, 0.45 + 0.47 * _m.sin(t), 1.30 + 0.47 * _m.cos(t)))
    # (Its front stops behind the headlight housing.)
    top = [P(0.40, h, z) for z, h in ((0.75, 0.95), (0.90, 0.95), (1.20, 0.945), (1.50, 0.90))]
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
    side.box(P(0.43, 1.03, -1.36), (0.17, 0.045, 0.09), BLACK)
    side.poly([P(0.575, 1.01, -1.452), P(0.29, 1.01, -1.452), P(0.29, 1.04, -1.452), P(0.575, 1.04, -1.452)], "glow_red")
    side.tube(P(0.36, 1.09, -1.29), P(0.36, 1.09, -1.43), 0.018, "metal_chrome", seg=10)
    # Inner fender under the rear fender, over the tyre.
    side.box(P(0.52, 0.93, -1.12), (0.05, 0.065, 0.15), "metal_frame")
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
        [P(0.30, 1.38, -0.45), P(0.32, 1.29, -0.76), P(0.34, 1.29, -0.98), P(0.34, 1.15, -1.22)],
        [P(0.40, 1.22, -0.62), P(0.40, 1.20, -0.92)],
        [P(0.40, 1.08, -0.74), P(0.32, 1.29, -0.76)],
        [P(0.34, 1.29, -0.98), P(0.42, 1.00, -0.80)],
    ]
    for pts in left:
        m.polyline(pts, r, FRAME)
        m.polyline(mirror_x(pts), r, FRAME)
    for x, h, z in ((0.09, 0.44, -1.38), (0.30, 0.44, -1.00), (0.34, 1.29, -0.98), (0.16, 0.62, 1.60), (0.16, 0.40, 1.60)):
        m.tube(P(x, h, z), P(-x, h, z), r, FRAME, caps=False)
    # Spine under the hood, carrying the front arms' pivots.
    m.box(P(0, 0.52, 1.10), (0.10, 0.08, 0.42), FRAME)
    # Bulkheads: behind the front wheels, and behind the cabin.
    for z, h0, h1, w in ((0.72, 0.40, 0.86, 0.46), (-0.60, 0.45, 1.09, 0.47)):
        pl = [P(w, h0, z), P(w, h1, z), P(-w, h1, z), P(-w, h0, z)]
        m.prism(pl if z > 0 else list(reversed(pl)), 0.02, FRAME)
    side = Mesh()
    side.box(P(0.40, 1.07, -0.60), (0.10, 0.04, 0.14), FRAME)  # deck plate behind the cabin
    side.box(P(0.41, 0.925, 1.40), (0.045, 0.025, 0.045), FRAME)  # front coilover's top mount
    side.box(P(0.31, 1.02, -0.94), (0.045, 0.025, 0.045), FRAME)  # rear one's
    m.add(side.both())
    m.add(front_guard())
    # Belly pans.
    m.box(P(0, 0.34, 1.20), (0.12, 0.012, 0.42), "metal_graphite")
    m.box(P(0, 0.40, 0.10), (0.45, 0.012, 0.60), "metal_graphite")
    m.box(P(0, 0.40, -1.10), (0.20, 0.012, 0.30), "metal_graphite")
    return m


# --- Front guard under the nose: the chassis' nose box (a frame of square tubes holding two
# honeycomb grilles, set between two brackets whose eyes carry the front arms), the skid plate
# folding back under it, and the tubular bumper hoop around it.
def bar(m, a, b, w, d, mat, up=(0, 1, 0)):
    """Square-section bar from a to b, w wide (across, along `up` x axis) and d deep."""
    a, b = Vector(a), Vector(b)
    ax = (b - a).normalized()
    u = Vector(up)
    side = ax.cross(u).normalized()
    u = side.cross(ax).normalized()
    m.box((a + b) / 2, (w / 2, d / 2, (b - a).length / 2), mat, axes=(side, u, ax))


def front_guard():
    m = Mesh()
    # Nose box: its front face leans back 11° (z 1.665 at the foot, 1.725 at the top).
    def face_z(h):
        return 1.70 + (h - 0.33) / 0.31 * 0.06

    x0, h0, h1 = 0.118, 0.33, 0.64
    corners = [P(-x0, h0, face_z(h0)), P(x0, h0, face_z(h0)), P(x0, h1, face_z(h1)), P(-x0, h1, face_z(h1))]
    for a, b in zip(corners, corners[1:] + corners[:1]):
        bar(m, a, b, 0.022, 0.03, "metal_frame", up=(0, 0, 1))
    hm = 0.488
    bar(m, P(-x0, hm, face_z(hm)), P(x0, hm, face_z(hm)), 0.018, 0.026, "metal_frame", up=(0, 0, 1))
    # Honeycomb grilles, 1.5 cm behind the frame, and the box's dark inside behind them.
    for g0, g1 in ((h0 + 0.006, hm - 0.006), (hm + 0.006, h1 - 0.006)):
        q = [P(-0.11, g0, face_z(g0) - 0.015), P(0.11, g0, face_z(g0) - 0.015), P(0.11, g1, face_z(g1) - 0.015), P(-0.11, g1, face_z(g1) - 0.015)]
        m.poly(q, "livery_mesh")
    m.box(P(0, (h0 + h1) / 2, 1.62), (0.11, (h1 - h0) / 2, 0.07), "metal_graphite")
    # Brackets each side of the box: plates carrying the arms' eyes (orange bushings, chrome
    # bolts) at the top and at the foot.
    for s in (1, -1):
        outline = [(1.60, 0.30), (1.73, 0.32), (1.78, 0.40), (1.795, 0.665), (1.73, 0.70), (1.60, 0.68)]
        plate = [P(s * 0.135, h, z) for z, h in outline]
        m.prism(plate if s > 0 else list(reversed(plate)), 0.012, "metal_frame")
        for h, z in ((0.625, 1.77), (0.36, 1.75)):
            c = P(s * 0.142, h, z)
            m.tube(c, c + Vector((s * 0.012, 0, 0)), 0.024, "paint_orange", seg=14)
            m.tube(c, c + Vector((s * 0.02, 0, 0)), 0.011, "metal_chrome", seg=10)
        # Bolts along the bracket's edge.
        for z, h in ((1.64, 0.32), (1.64, 0.66), (1.775, 0.50)):
            c = P(s * 0.142, h, z)
            m.tube(c, c + Vector((s * 0.008, 0, 0)), 0.007, "metal_steel", seg=6)
    # Skid plate: from the box's foot folding back under the spine, with a raised lip and bolts.
    skid = [(1.73, 0.32), (1.64, 0.27), (1.05, 0.27)]
    m.grid([[P(x, h, z) for z, h in skid] for x in (-0.12, 0.12)], "metal_frame")
    m.grid([[P(x, h - 0.01, z) for z, h in skid] for x in (0.12, -0.12)], "metal_frame")
    for x in (-0.08, -0.027, 0.027, 0.08):
        c = P(x, 0.29, 1.69)
        m.tube(c, c + Vector((0, 0.007, 0.012)), 0.008, "metal_steel", seg=6)
    # Bumper tubes from under the headlights to the brackets.
    for s in (1, -1):
        m.polyline([P(s * 0.38, 0.70, 1.50), P(s * 0.30, 0.62, 1.68), P(s * 0.147, 0.60, 1.74)], 0.017, "metal_frame")
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

    rows = [sec(-0.40, 0.14, 1.40, 1.545), sec(-0.06, 0.14, 1.40, 1.55), sec(-0.0, 0.13, 1.42, 1.535), sec(0.035, 0.12, 1.45, 1.515)]
    rows = [list(reversed(r)) for r in rows]
    ids = m.grid(rows, BLACK)
    m.cap(ids[0], BLACK)
    m.cap(ids[-1], BLACK, flip=True)
    for z in (-0.32, -0.21, -0.10):
        c = P(0.0, 1.548, z)
        m.tube(c, c + Vector((0, 0.006, 0)), 0.046, "metal_graphite", seg=20)
        m.tube(c + Vector((0, 0.006, 0)), c + Vector((0, 0.009, 0)), 0.014, "metal_steel", seg=12)
        m.tube(c, c + Vector((0, 0.004, 0)), 0.054, "metal_copper", seg=20, caps=False)
    # The scoop's mouth: a black grille over an orange lip, amber lamps at its corners.
    m.poly([P(-0.098, 1.465, 0.037), P(0.098, 1.465, 0.037), P(0.098, 1.50, 0.037), P(-0.098, 1.50, 0.037)], "metal_grille")
    m.box(P(0, 1.459, 0.036), (0.105, 0.004, 0.003), "paint_orange")
    for x in (-0.112, 0.112):
        m.box(P(x, 1.482, 0.037), (0.006, 0.016, 0.003), "glow_amber")
    # Light bar across the scoop's back: four amber lamps facing back, one red in the middle.
    for x in (-0.105, -0.055, 0.055, 0.105):
        m.box(P(x, 1.505, -0.403), (0.02, 0.016, 0.004), "glow_amber")
    m.box(P(0, 1.505, -0.403), (0.018, 0.016, 0.004), "glow_red")
    # Small intake at the front of the roof.
    pts = [(0.10, 1.39, 0.21), (0.10, 1.40, -0.03), (0.11, 1.44, 0.04), (0.09, 1.445, 0.19)]
    a = [P(*p) for p in pts]
    b = mirror_x(a)
    m.poly([a[1], a[2], a[3], a[0]], BLACK)
    m.poly([b[0], b[3], b[2], b[1]], BLACK)
    m.poly([a[3], a[2], b[2], b[3]], BLACK)
    m.poly([a[0], a[3], b[3], b[0]], "metal_grille")
    m.poly([a[2], a[1], b[1], b[2]], BLACK)
    return m


# --- Hood and fender vents, after the top plan and the 3/4 view: a raised black scoop on the hood's
# spine with two honeycomb grilles, and on each fender two slanted vents, each a honeycomb grille
# under a black lip raised along its rear edge.
def hull_height(x, z):
    """Height of the upper hull's surface at (x, z), between its lines 0 to 3 (hood and fenders)."""
    def row_h(row):
        pts = row[:4]
        if x <= pts[0][0]:
            return pts[0][1]
        for (x0, h0, _), (x1, h1, _) in zip(pts, pts[1:]):
            if x <= x1:
                return h0 + (h1 - h0) * (x - x0) / (x1 - x0)
        return pts[-1][1]

    zs = [row[0][2] for row in HULL]
    if z <= zs[0]:
        return row_h(HULL[0])
    for i in range(len(zs) - 1):
        if z <= zs[i + 1]:
            t = (z - zs[i]) / (zs[i + 1] - zs[i])
            return row_h(HULL[i]) * (1 - t) + row_h(HULL[i + 1]) * t
    return row_h(HULL[-1])


def on_hull(x, z, lift):
    return P(x, hull_height(x, z) + lift, z)


# Fender vents (left side): corners (x, z) — outer-rear, outer-front, inner-front, inner-rear.
FENDER_VENTS = [
    ((0.42, 0.67), (0.42, 0.94), (0.25, 1.10), (0.25, 0.83)),
    ((0.42, 1.03), (0.42, 1.30), (0.26, 1.44), (0.26, 1.17)),
]
HOOD_GRILLES = [((0.02, 0.86), (0.115, 1.10))]  # (x0, z0), (x1, z1), left side


def vents():
    m = Mesh()
    # Hood scoop on the spine: a black box raised 3 cm, its sides tapering in.
    z0, z1, w = 0.80, 1.16, 0.135
    base = [(w, z0), (w, z1), (-w, z1), (-w, z0)]
    lo = [on_hull(x, z, -0.01) for x, z in base]
    hi = [on_hull(x * 0.90, z + (0.03 if z == z0 else -0.03), 0.022) for x, z in base]
    m.poly(hi[::-1], BLACK)  # top, facing up
    for k in range(4):
        n = (k + 1) % 4
        m.poly((lo[k], lo[n], hi[n], hi[k]), BLACK)
    side = Mesh()
    for (x0, za), (x1, zb) in HOOD_GRILLES:
        q = [on_hull(x0 * 0.90, za + 0.02, 0.024), on_hull(x1 * 0.90, za + 0.02, 0.024), on_hull(x1 * 0.90, zb - 0.02, 0.024), on_hull(x0 * 0.90, zb - 0.02, 0.024)]
        side.face([side.vert(p) for p in q[::-1]], "livery_mesh")
    for vent in FENDER_VENTS:
        # The grille, 3 mm over the paint.
        g = [on_hull(x, z, 0.003) for x, z in vent]
        side.face([side.vert(p) for p in g], "livery_mesh")
        # The lip: a black wedge along the rear edge (outer-rear to inner-rear), 2.5 cm tall at
        # the back, sloping down to the grille 6 cm ahead.
        (xa, za), (xd, zd) = vent[0], vent[3]
        a0, d0 = on_hull(xa, za - 0.005, -0.005), on_hull(xd, zd - 0.005, -0.005)
        a1, d1 = on_hull(xa, za, 0.028), on_hull(xd, zd, 0.028)
        a2, d2 = on_hull(xa, za + 0.06, 0.004), on_hull(xd, zd + 0.06, 0.004)
        side.poly([a0, a1, d1, d0][::-1], BLACK)  # its back
        side.poly([a1, a2, d2, d1][::-1], BLACK)  # its sloping top
        side.poly([a0, a2, a1][::-1], BLACK)
        side.poly([d0, d1, d2][::-1], BLACK)
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


# Panels whose creases get a chamfer (meshkit.to_object).
PANELS = ("body_hull", "body_doors", "body_sills", "body_cabin", "body_nose", "body_tail")


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
