"""Blueprint calibration shared by the reference tools: for each orthographic view, the canvas size,
its scale (pixels per metre) and how a point of the car frame (metres, +X left, +Y up, +Z forward,
origin at the centre of gravity) lands on it. The wheel layout comes from the physics, so a
reference drawn on these canvases lines up with the game's car.

The views are the buggy "B" plans of art/buggy/v2/views, registered by tools/blender/warp_refs.py
into art/buggy/v2/registered/<view>.png (and <view>_grid.png, with a metric grid).
"""

import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", ".."))
PLANS = os.path.join(REPO, "art", "buggy", "v2", "views")
REGISTERED = os.path.join(REPO, "art", "buggy", "v2", "registered")

WHEEL_R = 0.45
TYRE_W = 0.48  # the plans' wide tyres (0.46 m from the front, 0.50 m from above and behind)
TRACK = 1.8
WHEELBASE = 2.6
CG_H = 0.05
TRAVEL = 0.5
REST = TRAVEL - 40.0 / (2.0 * math.pi * 2.0) ** 2
WHEEL_Y = -CG_H - REST
GROUND_Y = WHEEL_Y - WHEEL_R

# view: canvas (w, h), pixels per metre, pixel of the car-frame origin's projection (vertical views:
# its column, and the ground's row), and the car-frame axes along the image's right and down.
VIEWS = {
    # Left side (+X toward the viewer), nose to the left.
    "left": dict(size=(2688, 1520), scale=640.0, ground_px=1260, centre_px=1344, right=(0, 0, -1), down=(0, -1, 0)),
    # Right side, nose to the right.
    "right": dict(size=(2688, 1520), scale=640.0, ground_px=1260, centre_px=1344, right=(0, 0, 1), down=(0, -1, 0)),
    # From above, nose to the right: the car's left (+X) up.
    "top": dict(size=(2688, 1520), scale=600.0, ground_px=None, centre_px=(1344, 760), right=(0, 0, 1), down=(-1, 0, 0)),
    # From the front: the car's left (+X) on the image's right.
    "front": dict(size=(2336, 1744), scale=900.0, ground_px=1600, centre_px=1168, right=(1, 0, 0), down=(0, -1, 0)),
    # From behind: the car's right on the image's right.
    "back": dict(size=(2336, 1744), scale=900.0, ground_px=1600, centre_px=1168, right=(-1, 0, 0), down=(0, -1, 0)),
}
SIDES = ("left", "right")


def project(view, p):
    """Car-frame point to (x, y) pixels on the view's canvas."""
    v = VIEWS[view]
    s = v["scale"]
    r = sum(a * b for a, b in zip(v["right"], p))
    d = sum(a * b for a, b in zip(v["down"], p))
    if view == "top":
        cx, cy = v["centre_px"]
        return (cx + r * s, cy + d * s)
    # Vertical views: the ground line is GROUND_Y.
    return (v["centre_px"] + r * s, v["ground_px"] + (d + GROUND_Y) * s)


def wheel_centres():
    x, z = TRACK / 2, WHEELBASE / 2
    return [(x, WHEEL_Y, z), (-x, WHEEL_Y, z), (x, WHEEL_Y, -z), (-x, WHEEL_Y, -z)]
