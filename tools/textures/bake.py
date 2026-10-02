"""Bakes the surface textures the app embeds (crates/app/assets/textures/) from generated photos.

The sources are square images generated with Higgsfield (Nano Banana, 2048 px) from text
prompts, with the mood images as colour references; they live in art/textures/src/ (see
SOURCES below for what each one is). Per material this script:

1. keeps one period of the source: some generations come back as a 2 x 2 repetition of a tile,
   the top-left quarter is that tile;
2. removes the large lighting gradients (the tile is divided by its own very blurred luminance)
   and, with `flatten`, every variation larger than the grain;
3. hides the seams of the tile, one axis at a time: near the borders the tile is replaced by
   itself shifted by half a period along that axis, cut along the path where the two agree best
   (image quilting), so pebbles stay whole and strata stay level;
4. moves its mean colour toward a target for that surface (`target`, linear RGB);
5. derives a height map (high-passed luminance) and a tangent-space normal map from it;
6. writes, at 1024 x 1024:
   - <name>_albedo.png: sRGB colour, and in A the height, 0 low to 255 high (the shader blends
     materials by it);
   - <name>_normal.png: R, G the normal's x along +u (image right) and y along +v (image down),
     mapped from [-1, 1] to [0, 255]; B unused;
   - <name>.png in PREVIEW_DIR: 2 x 2 tiles lit from the top left, to check seams and relief.

    blender -b -P tools/textures/bake.py -- SRC_DIR OUT_DIR PREVIEW_DIR [name ...]

    e.g. -- art/textures/src crates/app/assets/textures art/textures/preview

The layer order of MATERIALS is the texture array's, and must match `scene.wgsl`.
"""

import os
import struct
import sys
import zlib

import bpy
import numpy as np

SIZE = 1024


def material(name, quarter=False, target=(0.5, 0.2, 0.1), pull=0.7, contrast=1.0, flatten=0.0, relief=0.05, bump=5.0, seam=0.25):
    """One layer of the texture arrays, baked from art/textures/src/<name>.png.

    - quarter: the source repeats 2 x 2, keep its top-left quarter;
    - target, pull: linear mean colour of the game's palette for the surface, and how far
      (0..1) the texture's own mean is moved toward it;
    - contrast: scales the variations around the mean;
    - flatten: removes the variations larger than this (fraction of the tile), keeping the grain;
    - relief: the height map keeps details smaller than this (fraction of the tile);
    - bump: strength of the normal map;
    - seam: how far in from the borders the seam cuts may run (fraction of the tile; 0 keeps the
      borders as they are).
    """
    return dict(name=name, file=f"{name}.png", quarter=quarter, target=target, pull=pull, contrast=contrast, flatten=flatten, relief=relief, bump=bump, seam=seam)


MATERIALS = [
    # Off-white laminated woven tarpaulin of the roads: the weave and fine dust only, the shader
    # lays the panels, seams, markings, dust and rubber.
    material("tarp", target=(0.50, 0.47, 0.42), pull=0.85, flatten=0.02, relief=0.02, bump=3.0),
    # The compacted floor of a dirt track, lightly churned, small clods.
    material("dirt", quarter=True, target=(0.25, 0.09, 0.05), pull=0.9, contrast=1.2, relief=0.05, bump=5.0),
    # Freshly dug loose earth of the banks, full of clods.
    material("earth", quarter=True, target=(0.28, 0.10, 0.055), pull=0.9, contrast=1.25, relief=0.06, bump=7.0),
    # Natural ground: regolith with dark basalt pebbles and red stones.
    material("pebbles", quarter=True, target=(0.38, 0.13, 0.06), pull=0.9, contrast=1.25, relief=0.05, bump=6.0),
    # Natural ground: dusty orange regolith with pale half-buried slabs.
    material("slabs", target=(0.42, 0.15, 0.07), pull=0.9, contrast=1.25, relief=0.04, bump=5.0),
    # Fine sand with low wind ripples.
    material("sand", target=(0.48, 0.18, 0.085), pull=0.9, contrast=1.2, relief=0.05, bump=3.0),
    # Layered red sandstone (cliffs, mesas, rocks), image up = world up.
    material("rock", quarter=True, target=(0.28, 0.10, 0.055), pull=0.8, contrast=1.2, relief=0.08, bump=6.0),
    # Light poured concrete, one formwork panel per tile (its joints stay on the borders).
    material("concrete", quarter=True, target=(0.50, 0.48, 0.45), pull=0.5, relief=0.05, bump=2.0, seam=0.0),
    # Khaki woven polypropylene of the regolith sandbags, dust in the weave (tile about 0.6 m).
    material("sandbag", target=(0.45, 0.34, 0.20), pull=0.85, flatten=0.03, relief=0.03, bump=4.0),
    # Safety-orange polyester webbing of the ratchet straps, the strap's length along u (tile
    # about 0.2 m).
    material("webbing", target=(0.80, 0.17, 0.02), pull=0.85, flatten=0.06, relief=0.02, bump=3.0),
    # Hot-dip galvanised steel of the stakes and buckles, its zinc spangle and scuffs (tile about
    # 0.35 m); the shader mirrors the sky in it, this colour is its tint.
    material("galvanized", target=(0.52, 0.53, 0.55), pull=0.8, contrast=1.15, flatten=0.05, relief=0.03, bump=2.5),
    # Rusty steel: bare patches and flaking rust.
    material("rust", target=(0.20, 0.09, 0.045), pull=0.7, contrast=1.2, flatten=0.05, relief=0.04, bump=4.0),
]

SOURCES = """
tarp      off-white laminated woven plastic tarpaulin stretched taut, Martian dust in the weave
          (GPT Image 2.5, with art/roads/moodboard/04-chaussee.jpg as reference)
dirt      compacted floor of a Martian off-road track, lightly churned (mood: off-road)
earth     loose soil pushed aside by a bulldozer to bank a dirt track (mood: off-road)
pebbles   regolith with gravel, dark basalt pebbles and red stones (mood: off-road)
slabs     orange regolith, angular pebbles, half-buried flat stones (mood: off-road)
sand      fine orange-red sand with gentle wind ripples (mood: sandstorm)
rock      layered red sandstone like the Martian mesas (mood: off-road canyon walls)
concrete  light grey poured concrete of a racing barrier, dusty (mood: sandstorm)
sandbag   khaki woven polypropylene sandbag cloth, soft wrinkles, orange dust in the weave
          (GPT Image 2.5; cropped and stretched vertically to 68 tapes, 72 across, so the
          seam pass's half-tile shifts land on the weave)
galvanized hot-dip galvanised steel, zinc spangle, hammer scuffs, Martian dust (GPT Image 2.5)
rust      weathered steel, bare patches and flaking rust, pitting (GPT Image 2.5)
webbing   safety-orange polyester ratchet-strap webbing, ribs along x, a little dust and wear
          (GPT Image 2.5; cropped and stretched vertically to 36 ribs, as for sandbag)
"""


def load(path):
    img = bpy.data.images.load(path)
    img.colorspace_settings.name = "Non-Color"
    w, h = img.size
    px = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(px)
    bpy.data.images.remove(img)
    return px.reshape(h, w, 4)[::-1, :, :3].copy()


def to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    c = np.clip(c, 0.0, 1.0)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * c ** (1.0 / 2.4) - 0.055)


def luminance(c):
    return c[..., 0] * 0.2126 + c[..., 1] * 0.7152 + c[..., 2] * 0.0722


def blur(x, sigma):
    """Gaussian blur that wraps around the tile (through the FFT)."""
    h, w = x.shape[:2]
    fy = np.fft.fftfreq(h)[:, None]
    fx = np.fft.fftfreq(w)[None, :]
    g = np.exp(-2.0 * (np.pi * sigma) ** 2 * (fx * fx + fy * fy))
    if x.ndim == 2:
        return np.real(np.fft.ifft2(np.fft.fft2(x) * g))
    return np.stack([np.real(np.fft.ifft2(np.fft.fft2(x[..., k]) * g)) for k in range(x.shape[2])], axis=-1)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def height_of(lin, sigma):
    lum = luminance(lin)
    n = lum.shape[0]
    hp = blur(lum - blur(lum, sigma * n), 0.8)
    lo, hi = np.percentile(hp, 1.0), np.percentile(hp, 99.0)
    return np.clip((hp - lo) / max(hi - lo, 1e-6), 0.0, 1.0)


def seam_error(lin):
    """Mean step across the wrap-around edges over the mean step between neighbours."""
    lum = luminance(lin)
    inside = np.abs(np.diff(lum, axis=1)).mean() + np.abs(np.diff(lum, axis=0)).mean()
    edges = np.abs(lum[:, 0] - lum[:, -1]).mean() + np.abs(lum[0, :] - lum[-1, :]).mean()
    return edges / max(inside, 1e-6)


def blur_reflect(x, sigma):
    """Gaussian blur of an image that does not tile (mirrored at its borders)."""
    n = x.shape[0] // 2
    pad = ((n, n), (n, n)) + ((0, 0),) * (x.ndim - 2)
    return blur(np.pad(x, pad, mode="reflect"), sigma)[n:-n, n:-n]


def min_cut(err):
    """Path of least total error down the rows of `err`, one column per row, moving at most one
    column between rows (dynamic programming)."""
    rows, cols = err.shape
    acc = err.copy()
    step = np.zeros((rows, cols), dtype=np.int8)
    for r in range(1, rows):
        prev = acc[r - 1]
        left = np.concatenate([[np.inf], prev[:-1]])
        right = np.concatenate([prev[1:], [np.inf]])
        choice = np.argmin(np.stack([left, prev, right]), axis=0)
        acc[r] += np.choose(choice, [left, prev, right])
        step[r] = choice - 1
    path = np.empty(rows, dtype=np.int64)
    path[-1] = int(np.argmin(acc[-1]))
    for r in range(rows - 1, 0, -1):
        path[r - 1] = path[r] + step[r, path[r]]
    return path


def seam_pass(lin, band):
    """Makes the left and right borders meet: near them the tile is replaced by itself shifted by
    half a period sideways, along the cuts where the two agree best (image quilting)."""
    n = lin.shape[1]
    shifted = np.roll(lin, n // 2, axis=1)
    err = blur_reflect(((lin - shifted) ** 2).sum(axis=2), 1.5)
    z0, z1 = int(0.03 * n), int(band * n)
    left = min_cut(err[:, z0:z1]) + z0
    right = min_cut(err[:, n - z1 : n - z0]) + n - z1
    cols = np.arange(n)[None, :]
    mask = ((cols < left[:, None]) | (cols >= right[:, None])).astype(np.float64)
    mask = blur(mask, 1.5)
    return lin * (1.0 - mask[..., None]) + shifted * mask[..., None]


def make_seamless(lin, band):
    lin = seam_pass(lin, band)
    return seam_pass(lin.transpose(1, 0, 2), band).transpose(1, 0, 2)


def downsample(x, size):
    while x.shape[0] > size:
        x = 0.25 * (x[0::2, 0::2] + x[1::2, 0::2] + x[0::2, 1::2] + x[1::2, 1::2])
    return x


def normal_of(h, strength):
    dx = 0.5 * (np.roll(h, -1, axis=1) - np.roll(h, 1, axis=1))
    dy = 0.5 * (np.roll(h, -1, axis=0) - np.roll(h, 1, axis=0))
    s = strength * h.shape[0] / 256.0
    n = np.stack([-dx * s, -dy * s, np.ones_like(h)], axis=-1)
    return n / np.linalg.norm(n, axis=-1, keepdims=True)


def write_png(path, px):
    """8-bit RGB or RGBA PNG, each row with the filter that compresses it best."""
    h, w, c = px.shape
    x = px.reshape(h, w * c).astype(np.int16)
    a = np.zeros_like(x)
    a[:, c:] = x[:, :-c]
    b = np.zeros_like(x)
    b[1:] = x[:-1]
    ab = np.zeros_like(x)
    ab[1:, c:] = x[:-1, :-c]
    p = a + b - ab
    pa, pb, pc = np.abs(p - a), np.abs(p - b), np.abs(p - ab)
    paeth = np.where((pa <= pb) & (pa <= pc), a, np.where(pb <= pc, b, ab))
    filtered = np.stack([x, x - a, x - b, x - (a + b) // 2, x - paeth]) % 256
    cost = np.abs(filtered.astype(np.uint8).view(np.int8).astype(np.int32)).sum(axis=2)
    choice = cost.argmin(axis=0)
    rows = np.empty((h, 1 + w * c), dtype=np.uint8)
    rows[:, 0] = choice
    rows[:, 1:] = filtered[choice, np.arange(h)]

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    kind = {3: 2, 4: 6}[c]
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, kind, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(rows.tobytes(), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def to_bytes(x):
    return (np.clip(x, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)


def bake(src_dir, out_dir, preview_dir, m):
    name = m["name"]
    src = to_linear(load(os.path.join(src_dir, m["file"])))
    if m["quarter"]:
        n = src.shape[0] // 2
        src = src[:n, :n]
    n = src.shape[0]
    # Delight: divide out the luminance seen through a very wide blur, so the tile has the same
    # brightness everywhere and its seams cut between parts that match.
    lum = luminance(src)
    low = blur_reflect(lum, n / 8.0)
    src = src * np.clip(lum.mean() / np.maximum(low, 1e-4), 0.6, 1.7)[..., None]
    if m["flatten"] > 0.0:
        # Keep the grain only: every channel divided by its own blur (the shader lays the large
        # variations, so they never repeat with the tile).
        mean = src.reshape(-1, 3).mean(axis=0)
        src = src * np.clip(mean / np.maximum(blur_reflect(src, m["flatten"] * n), 1e-4), 0.3, 3.0)
    before = seam_error(src)
    lin = make_seamless(src, m["seam"]) if m["seam"] > 0.0 else src
    after = seam_error(lin)
    lin = downsample(lin, SIZE)
    mean = lin.reshape(-1, 3).mean(axis=0)
    lin = mean + (lin - mean) * m["contrast"]
    gain = (np.array(m["target"]) / mean) ** m["pull"]
    lin = np.clip(lin * gain, 0.0, 1.0)
    h = height_of(lin, m["relief"])
    nrm = normal_of(h, m["bump"])

    albedo = np.concatenate([to_bytes(to_srgb(lin)), to_bytes(h)[..., None]], axis=-1)
    normal = np.concatenate([to_bytes(0.5 + 0.5 * nrm[..., :2]), np.zeros_like(albedo[..., :1])], axis=-1)
    write_png(os.path.join(out_dir, f"{name}_albedo.png"), albedo)
    write_png(os.path.join(out_dir, f"{name}_normal.png"), normal)

    # Preview: 2 x 2 tiles, lit from the top left.
    light = np.array([-0.5, -0.5, 0.7])
    light /= np.linalg.norm(light)
    lit = lin * (0.35 + 0.9 * np.clip(nrm @ light, 0.0, 1.0))[..., None]
    tiles = np.tile(downsample(lit, SIZE // 2), (2, 2, 1))
    write_png(os.path.join(preview_dir, f"{name}.png"), to_bytes(to_srgb(tiles)))
    srgb_mean = to_srgb(lin.reshape(-1, 3).mean(axis=0))
    print(f"{name:9s} seam {before:5.2f} -> {after:4.2f}  mean linear {np.round(mean, 3)} -> sRGB {np.round(srgb_mean * 255).astype(int)}")


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if len(argv) < 3:
        print(__doc__)
        sys.exit(1)
    src_dir, out_dir, preview_dir, only = argv[0], argv[1], argv[2], set(argv[3:])
    os.makedirs(out_dir, exist_ok=True)
    os.makedirs(preview_dir, exist_ok=True)
    for m in MATERIALS:
        if not only or m["name"] in only:
            bake(src_dir, out_dir, preview_dir, m)


main()
