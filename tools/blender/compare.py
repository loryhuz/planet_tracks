"""Renders the buggy (art/buggy/buggy.blend) with orthographic cameras calibrated exactly like the
registered plans (tools/blender/blueprint.py, art/buggy/v2/registered) and compares them. Per view
it writes, to the output directory:

- <view>_model.png: the model's render on the plan's canvas (on white);
- <view>_pair.png: the plan above, the model below, cropped to the car, for detail checks;
- <view>_overlay.png: the plan with the model's outline in cyan and the plan's in red (where they
  agree the outlines merge), on the metric grid;
- <view>_diff.png: silhouette difference: red = only in the plan, cyan = only in the model;

and prints the overlap (intersection over union) of the two silhouettes. The view `3q` renders
the validated 3/4 view (art/buggy/v2/3q_B_compact.jpg) from a matched perspective camera and pairs
it with the image.

    blender -b art/buggy/buggy.blend -P tools/blender/compare.py -- OUT_DIR [view ...]
"""

import json
import math
import os
import sys

import bpy
import numpy as np
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blueprint as bp  # noqa: E402
from warp_refs import grid_overlay, load, save  # noqa: E402

THREE_Q = os.path.join(bp.REPO, "art", "buggy", "v2", "3q_B_compact.jpg")
# Perspective camera of the 3/4 view, fitted (least squares, 25 px rms) on the tyres' ground
# contacts and hubs, the headlights, the front plate and the wing's endplate: position (x, height
# above the ground, z), forward and up directions in the car frame, lens for a 36 mm sensor.
THREE_Q_CAM = dict(pos=(5.3613, 1.7585, 11.7075), fwd=(-0.42175, -0.08409, -0.9028), up=(-0.06749, 0.99584, -0.06122), lens_mm=110.9)
# The front and back plans are photographs with perspective: the cameras sit on the car's axis
# this far from the near axle, at this height, with their scale right on the near axle's plane.
PERSPECTIVE = {"front": dict(distance=15.0, height=0.5), "back": dict(distance=10.0, height=0.6)}


def G(p):
    return Vector((p[0], -p[2], p[1]))


def camera_for(view):
    """Camera whose render lands exactly on the view's blueprint canvas: orthographic, or for the
    front and back a perspective one with the canvas' scale on the near axle's plane."""
    v = bp.VIEWS[view]
    w, h = v["size"]
    s = v["scale"]
    right = G(v["right"])
    down = G(v["down"])
    up = -down
    back = right.cross(up)
    if view == "top":
        cx, cy = v["centre_px"]
        centre = Vector(v["right"]) * ((w / 2 - cx) / s) + Vector(v["down"]) * ((h / 2 - cy) / s)
    else:
        centre = Vector(v["right"]) * ((w / 2 - v["centre_px"]) / s) + Vector(v["down"]) * ((h / 2 - v["ground_px"]) / s - bp.GROUND_Y)
    cam = bpy.data.cameras.new(f"cmp_{view}")
    cam.sensor_fit = "HORIZONTAL"
    cam.sensor_width = 36.0
    cam.clip_start = 0.1
    cam.clip_end = 60.0
    obj = bpy.data.objects.new(f"cmp_{view}", cam)
    bpy.context.scene.collection.objects.link(obj)
    rot = Matrix((right, up, back)).transposed().to_4x4()
    if view in PERSPECTIVE:
        p = PERSPECTIVE[view]
        cam.type = "PERSP"
        cam.lens = s * p["distance"] / w * 36.0
        axle = bp.WHEELBASE / 2
        eye = Vector((0.0, bp.GROUND_Y + p["height"], axle + p["distance"] if view == "front" else -axle - p["distance"]))
        # The optical axis lands on the canvas' centre column and on the row of the camera's height.
        row = v["ground_px"] - p["height"] * s
        cam.shift_y = (row - h / 2) / w
        obj.matrix_world = Matrix.Translation(G(eye)) @ rot
        return obj
    cam.type = "ORTHO"
    cam.ortho_scale = w / s
    obj.matrix_world = Matrix.Translation(G(centre) + back * 15.0) @ rot
    return obj


def camera_3q():
    c = THREE_Q_CAM
    cam = bpy.data.cameras.new("cmp_3q")
    cam.lens = c["lens_mm"]
    cam.sensor_fit = "HORIZONTAL"
    cam.sensor_width = 36.0
    cam.clip_end = 60.0
    obj = bpy.data.objects.new("cmp_3q", cam)
    bpy.context.scene.collection.objects.link(obj)
    fwd, up = G(c["fwd"]).normalized(), G(c["up"]).normalized()
    right = fwd.cross(up).normalized()
    up = right.cross(fwd)
    rot = Matrix((right, up, -fwd)).transposed().to_4x4()
    x, h, z = c["pos"]
    obj.matrix_world = Matrix.Translation(G((x, bp.GROUND_Y + h, z))) @ rot
    return obj


def outline(mask):
    m = mask
    e = np.zeros_like(m)
    e[1:-1, 1:-1] = m[1:-1, 1:-1] & ~(m[:-2, 1:-1] & m[2:, 1:-1] & m[1:-1, :-2] & m[1:-1, 2:])
    t = e.copy()
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            t |= np.roll(np.roll(e, dy, 0), dx, 1)
    return t


def render(camera, size, path):
    scene = bpy.context.scene
    scene.camera = camera
    scene.render.resolution_x, scene.render.resolution_y = size
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1].copy()
    bpy.data.images.remove(img)
    return px


def crop_box(mask, margin=30):
    ys, xs = np.nonzero(mask)
    h, w = mask.shape
    return max(0, ys.min() - margin), min(h, ys.max() + margin), max(0, xs.min() - margin), min(w, xs.max() + margin)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    out_dir = args[0]
    views = args[1:] or ["left", "right", "top", "front", "back", "3q"]
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.context.scene
    for name in ("Stage", "References"):
        if name in bpy.data.collections:
            for o in bpy.data.collections[name].all_objects:
                o.hide_render = o.type != "LIGHT"
    scene.frame_set(1)
    scene.render.engine = "BLENDER_EEVEE"
    scene.eevee.taa_render_samples = 24
    scene.view_settings.view_transform = "Standard"
    world = scene.world
    bg = world.node_tree.nodes.get("Background")
    if bg:
        bg.inputs["Color"].default_value = (0.85, 0.85, 0.85, 1.0)
        bg.inputs["Strength"].default_value = 0.9
    report = {}
    for view in views:
        if view == "3q":
            ref = load(THREE_Q)[..., :3]
            h, w = ref.shape[:2]
            rgba = render(camera_3q(), (w, h), os.path.join(out_dir, "3q_model.png"))
            comp = rgba[..., :3] * rgba[..., 3:4] + ref * (1 - rgba[..., 3:4]) * 0.5
            save(os.path.join(out_dir, "3q_overlay.png"), np.clip(comp * 0.6 + ref * 0.4, 0, 1))
            model = rgba[..., :3] * rgba[..., 3:4] + (1 - rgba[..., 3:4]) * np.array([0.82, 0.55, 0.40])
            save(os.path.join(out_dir, "3q_model.png"), model)
            save(os.path.join(out_dir, "3q_pair.png"), np.concatenate([ref, model], axis=1))
            continue
        v = bp.VIEWS[view]
        rgba = render(camera_for(view), v["size"], os.path.join(out_dir, f"{view}_model.png"))
        model = rgba[..., 3] > 0.5
        ref_rgba = load(os.path.join(bp.REGISTERED, f"{view}.png"))
        ref_mask = ref_rgba[..., 3] > 0.5
        ref = ref_rgba[..., :3] * ref_rgba[..., 3:4] + (1 - ref_rgba[..., 3:4])
        inter = (model & ref_mask).sum()
        union = (model | ref_mask).sum()
        iou = float(inter / max(union, 1))
        over = grid_overlay(view, ref * 0.85 + 0.15)
        over[outline(ref_mask)] = [1.0, 0.1, 0.1]
        over[outline(model)] = [0.0, 0.85, 1.0]
        save(os.path.join(out_dir, f"{view}_overlay.png"), over)
        diff = np.ones_like(ref)
        diff[model & ref_mask] = [0.75, 0.75, 0.75]
        diff[ref_mask & ~model] = [1.0, 0.2, 0.2]
        diff[model & ~ref_mask] = [0.0, 0.75, 1.0]
        save(os.path.join(out_dir, f"{view}_diff.png"), grid_overlay(view, diff))
        comp = rgba[..., :3] * rgba[..., 3:4] + (1 - rgba[..., 3:4])
        save(os.path.join(out_dir, f"{view}_model.png"), comp)
        save(os.path.join(out_dir, f"{view}_rgba.png"), np.concatenate([comp, model[..., None].astype(np.float32)], axis=2))
        y0, y1, x0, x1 = crop_box(ref_mask | model)
        save(os.path.join(out_dir, f"{view}_pair.png"), np.concatenate([ref[y0:y1, x0:x1], comp[y0:y1, x0:x1]], axis=0))
        report[view] = dict(iou=round(iou, 4), only_ref=int((ref_mask & ~model).sum()), only_model=int((model & ~ref_mask).sum()))
        print(view, json.dumps(report[view]), flush=True)
    json.dump(report, open(os.path.join(out_dir, "compare.json"), "w"), indent=1)


main()
