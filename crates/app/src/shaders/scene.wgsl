// Lit scene (track, cars) and sky. Colours are linear; the output is tonemapped and
// sRGB-encoded here because the surface is a plain Unorm target (egui draws on it after us).

struct Frame {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_horizon: vec4<f32>,
    ground_bounce: vec4<f32>,
    // x: density per metre, y: start distance
    fog: vec4<f32>,
    // x: shadow map texel size in uv, y: 1 with surface textures, 0 the procedural look
    misc: vec4<f32>,
    // xy: circuit centre (x, z), zw: unit direction from the centre to the storm (x, z)
    storm_a: vec4<f32>,
    // x: distance from the centre to the front, y: time (s), z: ground height, w: approach 0..1
    storm_b: vec4<f32>,
};

struct Object {
    model: mat4x4<f32>,
    // rgb multiplies the vertex colour; a < 1 draws the object as a ghost
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var livery_tex: texture_2d<f32>;
@group(0) @binding(4) var livery_sampler: sampler;
// Surface textures (surfaces.rs): one layer per material, colour (sRGB) and relief (normal x, y
// and height).
@group(0) @binding(5) var surf_colour: texture_2d_array<f32>;
@group(0) @binding(6) var surf_relief: texture_2d_array<f32>;
@group(0) @binding(7) var surf_sampler: sampler;
@group(1) @binding(0) var<uniform> object: Object;

// Vertex kinds: 0 road, 1 dirt, 2 ground, 3 wall (painted gates), 10 car paint, 11 rubber,
// 12 metal, 13 glass, 14 lights (unlit), 15 woven wire tyre (lattice from `uv`, metres), 16 car
// paint coloured by the livery texture at `uv`, 20 concrete, 21 dug earth, 22 rock.
// The track's ground (kind 2) blends from natural ground to dug banks and driven dirt with
// `dirt` (0, ½, 1), and lays ruts along the track coordinates `uv` (metres along, across).
struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) kind: u32,
    @location(4) uv: vec2<f32>,
    @location(5) dirt: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) @interpolate(flat) kind: u32,
    @location(4) uv: vec2<f32>,
    @location(5) dirt: f32,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let world = object.model * vec4<f32>(v.pos, 1.0);
    var o: VsOut;
    o.clip = frame.view_proj * world;
    o.world = world.xyz;
    o.normal = (object.model * vec4<f32>(v.normal, 0.0)).xyz;
    o.color = v.color * object.tint.rgb;
    o.kind = v.kind;
    o.uv = v.uv;
    o.dirt = v.dirt;
    return o;
}

// Hash without sine (after Dave Hoskins): a few multiply-adds, where sin is slow on GPUs.
fn hash2(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Anti-aliased grid lines: 1 on a line, 0 elsewhere, fading out where lines get denser than pixels.
fn grid(coord: vec2<f32>, spacing: f32) -> f32 {
    let c = coord / spacing;
    let w = max(fwidth(c), vec2<f32>(1e-5));
    let g = abs(fract(c - 0.5) - 0.5) / w;
    let line = 1.0 - min(min(g.x, g.y), 1.0);
    let fade = 1.0 - smoothstep(0.12, 0.45, max(w.x, w.y));
    return line * fade;
}

// Small round stones scattered on a plane, 0..1: at most one per cell of `1 / scale` metres, in
// `density` of the cells. `aa` is the screen footprint of a metre (anti-aliasing).
fn stones(p: vec2<f32>, scale: f32, density: f32, aa: f32) -> f32 {
    let q = p * scale;
    let c = floor(q);
    let pick = hash2(c + vec2<f32>(3.7, 9.1));
    let at = 0.25 + 0.5 * vec2<f32>(hash2(c + vec2<f32>(17.1, 2.3)), hash2(c + vec2<f32>(5.9, 31.7)));
    let r = 0.12 + 0.16 * hash2(c + vec2<f32>(11.3, 7.7));
    let w = max(aa * scale, 1e-4);
    let stone = 1.0 - smoothstep(r - w, r + w, length(fract(q) - at));
    // Stones smaller than a pixel fade out instead of shimmering.
    return select(0.0, stone, pick < density) * (1.0 - smoothstep(0.15, 0.4, w));
}

// The track's ground without the surface textures (the earlier look, kept to compare): the
// vertex colour shaded with noise, procedural pebbles on the natural ground, layered banks,
// driven dirt streaked along the track with ruts and clods.
fn procedural_ground(in: VsOut, xz: vec2<f32>, aa: f32, g_fine: f32, g_coarse: f32, n_low: f32, n_high: f32, dug: f32, wear: f32) -> vec3<f32> {
    var base = in.color * (0.88 + 0.24 * n_low + 0.06 * n_high);
    // The building grid shows on the natural ground only.
    base *= 1.0 - (0.14 * g_fine + 0.32 * g_coarse) * (1.0 - dug);
    // Natural ground: rough, strewn with pebbles, pale dust between.
    let pebbles = stones(xz, 2.2, 0.22, aa) + 0.6 * stones(xz + vec2<f32>(0.37, 0.71), 5.0, 0.18, aa);
    let dust = smoothstep(0.5, 0.8, value_noise(xz * 0.9 + vec2<f32>(7.3, 1.9)));
    let natural = base * (1.0 + 0.06 * dust) * (1.0 - 0.3 * min(pebbles, 1.0));
    // Dug banks: fresh, redder earth in faint layers.
    let layers = 1.0 + 0.08 * sin(in.world.y * 2.6 + 3.0 * n_high);
    let bank = base * vec3<f32>(0.8, 0.62, 0.55) * layers;
    return mix(mix(natural, bank, dug), procedural_driven(base, in.uv, xz, aa), wear);
}

// Driven dirt of the procedural look, over ground of colour `base`: compacted and darker,
// streaked along the track (`uv` the track coordinates), darker in the ruts, loose lighter earth
// between them, a few clods.
fn procedural_driven(base: vec3<f32>, uv: vec2<f32>, xz: vec2<f32>, aa: f32) -> vec3<f32> {
    let r = ruts(uv);
    let streak = value_noise(vec2<f32>(uv.x * 0.09, uv.y * 1.4));
    let fine = value_noise(vec2<f32>(uv.x * 0.6, uv.y * 5.0));
    var driven = base * vec3<f32>(0.68, 0.52, 0.45);
    driven *= 0.92 + 0.14 * streak + 0.06 * fine;
    driven *= 1.0 - 0.2 * r;
    driven *= 1.0 + 0.1 * (1.0 - r) * smoothstep(0.45, 0.7, streak);
    return mix(driven, driven * vec3<f32>(0.78, 0.7, 0.68), stones(xz, 3.1, 0.12, aa) * 0.6);
}

// How much of a road the earth of the dirt track it leads to covers, 0..1, from the road
// vertex's `dirt` (0 far from the track, 1 where it meets it, main.rs road_spill) and the track
// coordinates `uv`: patches first, then all of it, the front reaching further along some lines
// across the road (tongues of earth several metres long). `rub` lays it in the wheel paths first.
fn spill_cover(dirt: f32, xz: vec2<f32>, uv: vec2<f32>, rub: f32) -> f32 {
    let tongues = value_noise(vec2<f32>(uv.y * 0.45, 7.3)) - 0.5 + 0.5 * (value_noise(vec2<f32>(uv.y * 1.3, 2.9)) - 0.5);
    let patches = value_noise(xz * 0.17 + vec2<f32>(4.1, 0.7)) - 0.5;
    let d = dirt + (0.55 * tongues + 0.9 * patches) * dirt * (1.0 - dirt) * 2.0;
    return clamp(2.2 * (d - 0.5) + 0.5 + 0.35 * rub * dirt, 0.0, 1.0);
}

fn band(x: f32, width: f32) -> f32 {
    let t = x / width;
    return exp(-t * t);
}

// Ruts worn by the cars, 0..1: pairs of wheel tracks across the floor that wander slowly along
// it, the racing lines deepest.
fn ruts(uv: vec2<f32>) -> f32 {
    let s = uv.x;
    let a = (value_noise(vec2<f32>(s * 0.019, 3.1)) - 0.5) * 7.0;
    let b = (value_noise(vec2<f32>(s * 0.016, 7.7)) - 0.5) * 8.0;
    let c = 0.5 * (a + b) + (value_noise(vec2<f32>(s * 0.03, 11.3)) - 0.5) * 3.0;
    var r = 0.9 * (band(uv.y - (-5.0 + a), 0.5) + band(uv.y - (-3.1 + a), 0.5));
    r += 0.7 * (band(uv.y - (3.2 + b), 0.5) + band(uv.y - (5.1 + b), 0.5));
    r += 0.45 * (band(uv.y - (-0.9 + c), 0.6) + band(uv.y - (1.0 + c), 0.6));
    return clamp(r, 0.0, 1.0);
}

// Height of the dirt's small relief, metres: ruts pressed in, loose earth streaked along them.
fn dirt_relief(uv: vec2<f32>, wear: f32) -> f32 {
    let streak = value_noise(vec2<f32>(uv.x * 0.09, uv.y * 1.4));
    return wear * (-0.05 * ruts(uv) + 0.02 * streak);
}

// ---------------------------------------------------------------------------------------------
// Surface textures.

// Layers of the surface textures (surfaces.rs, tools/textures/bake.py) and the size of one tile
// of each, metres.
const L_ASPHALT: i32 = 0;
const L_DIRT: i32 = 1;
const L_EARTH: i32 = 2;
const L_PEBBLES: i32 = 3;
const L_SLABS: i32 = 4;
const L_SAND: i32 = 5;
const L_ROCK: i32 = 6;
const L_CONCRETE: i32 = 7;
const TILE_ASPHALT: f32 = 3.5;
const TILE_DIRT: f32 = 4.0;
const TILE_EARTH: f32 = 2.5;
const TILE_PEBBLES: f32 = 3.0;
const TILE_SLABS: f32 = 6.0;
const TILE_SAND: f32 = 7.0;
const TILE_ROCK: f32 = 9.0;
const TILE_CONCRETE: f32 = 2.8;

// The relief (normal maps) fades out between these distances, metres; beyond, it is not read.
const RELIEF_NEAR: f32 = 30.0;
const RELIEF_FAR: f32 = 50.0;

// Colours the vertex colours are measured against (track/src/kit.rs, terrain.rs, scenery.rs).
const KIT_GROUND: vec3<f32> = vec3<f32>(0.55, 0.22, 0.10);
// The colour terrain.rs blends the ground toward next to the swept blocks.
const KIT_GRADED: vec3<f32> = vec3<f32>(0.56, 0.26, 0.135);
const KIT_STEEP: vec3<f32> = vec3<f32>(0.27, 0.115, 0.065);
const KIT_LIP: vec3<f32> = vec3<f32>(0.68, 0.68, 0.66);
const KIT_EARTH_FACE: vec3<f32> = vec3<f32>(0.40, 0.16, 0.075);
const KIT_ROCK: vec3<f32> = vec3<f32>(0.30, 0.13, 0.075);
// The kit's road half-width and barrier width (kit.rs HALF_WIDTH, LIP_WIDTH), metres.
const KIT_HALF_WIDTH: f32 = 10.0;
const KIT_LIP_WIDTH: f32 = 0.5;
// The terrain's level next to the blocks (kit.rs TERRAIN_Y).
const KIT_TERRAIN_Y: f32 = -0.25;
// Fine Martian dust settled on the asphalt and the barriers.
const ROAD_DUST: vec3<f32> = vec3<f32>(0.36, 0.13, 0.055);

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// A material at a point: linear colour, the world-space tilt its relief gives the normal (add it
// to the unit normal, then normalise), height 0..1.
struct Surf {
    colour: vec3<f32>,
    bump: vec3<f32>,
    height: f32,
};

// One texel lookup: colour, slope along u and v (the relief normal divided by its z; zero
// unless `relief`, which reads the normal map), height. `gx`, `gy` are the screen derivatives of
// `uv` (sampling must not rely on implicit derivatives: the materials are picked per triangle).
struct Tex {
    colour: vec3<f32>,
    slope: vec2<f32>,
    height: f32,
};

fn tex_at(layer: i32, uv: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>, relief: bool) -> Tex {
    let c = textureSampleGrad(surf_colour, surf_sampler, uv, layer, gx, gy);
    var slope = vec2<f32>(0.0);
    if relief {
        let xy = textureSampleGrad(surf_relief, surf_sampler, uv, layer, gx, gy).rg * 2.0 - 1.0;
        slope = xy / sqrt(max(1.0 - dot(xy, xy), 0.04));
    }
    return Tex(c.rgb, slope, c.a);
}

// Stochastic tiling of the natural ground (after Mikkelsen's practical hex-tiling): each cell of a
// triangle grid laid on the ground shows its own copy of a material's tile, shifted, mirrored and
// turned by quarter turns at random, so the tile never repeats visibly. The grid is the same for
// every material at a point, set once per fragment by `hex_at`: the vertices of the cells around
// the point, the point's own cell first, and their weights. Every point reads its own cell's copy;
// near a border the neighbour's copy is read too, and the two blend by height (the higher surface
// shows).
var<private> hex_vertex: array<vec2<f32>, 3>;
var<private> hex_weight: vec3<f32>;
// Neighbouring copies weighing less than this are not read.
const HEX_KEEP: f32 = 0.02;

// Sets the grid at ground point `xz` (metres; the cells are about 2 m across).
fn hex_at(xz: vec2<f32>) {
    let skew = vec2<f32>(xz.x - 0.57735027 * xz.y, 1.15470054 * xz.y);
    let base = floor(skew);
    let f = fract(skew);
    let z = 1.0 - f.x - f.y;
    let s = step(0.0, -z);
    let s2 = 2.0 * s - 1.0;
    let w = vec3<f32>(-z * s2, s - f.y * s2, s - f.x * s2);
    let w2 = w * w;
    let w4 = w2 * w2;
    var k = w4 * w4 * w4;
    k /= k.x + k.y + k.z;
    let a = base + vec2<f32>(s, s);
    let b = base + vec2<f32>(s, 1.0 - s);
    let c = base + vec2<f32>(1.0 - s, s);
    if k.x >= k.y && k.x >= k.z {
        hex_vertex = array<vec2<f32>, 3>(a, b, c);
        hex_weight = k;
    } else if k.y >= k.z {
        hex_vertex = array<vec2<f32>, 3>(b, c, a);
        hex_weight = k.yzx;
    } else {
        hex_vertex = array<vec2<f32>, 3>(c, a, b);
        hex_weight = k.zxy;
    }
}

// One copy of `layer`'s tile for the cell around `vertex`: one of the eight symmetries of the
// square, shifted.
fn hex_copy(layer: i32, p: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>, vertex: vec2<f32>, relief: bool) -> Tex {
    let h = hash2(vertex + vec2<f32>(f32(layer) * 17.3, f32(layer) * 5.1));
    let off = fract(h * vec2<f32>(97.31, 31.77));
    let o = u32(fract(h * 13.13) * 8.0);
    let swap = (o & 1u) != 0u;
    let flip = vec2<f32>(select(1.0, -1.0, (o & 2u) != 0u), select(1.0, -1.0, (o & 4u) != 0u));
    let t = tex_at(layer, select(p, p.yx, swap) * flip + off, select(gx, gx.yx, swap) * flip, select(gy, gy.yx, swap) * flip, relief);
    // The slope is in the copy's frame: back to the tile's.
    let back = t.slope * flip;
    return Tex(t.colour, select(back, back.yx, swap), t.height);
}

// `layer` stochastically tiled on the grid `hex_at` set; `p` in tiles.
fn tex_hex(layer: i32, p: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>, relief: bool) -> Tex {
    // The point's own cell, read by every pixel on the same path, whichever cell it is.
    let own = hex_copy(layer, p, gx, gy, hex_vertex[0], relief);
    if hex_weight.y <= HEX_KEEP && hex_weight.z <= HEX_KEEP {
        return own;
    }
    var g = hex_weight.x * exp2(5.0 * own.height);
    var colour = own.colour * g;
    var slope = own.slope * g;
    var height = own.height * g;
    var total = g;
    if hex_weight.y > HEX_KEEP {
        let t = hex_copy(layer, p, gx, gy, hex_vertex[1], false);
        g = hex_weight.y * exp2(5.0 * t.height);
        colour += t.colour * g;
        slope += t.slope * g;
        height += t.height * g;
        total += g;
    }
    if hex_weight.z > HEX_KEEP {
        let t = hex_copy(layer, p, gx, gy, hex_vertex[2], false);
        g = hex_weight.z * exp2(5.0 * t.height);
        colour += t.colour * g;
        slope += t.slope * g;
        height += t.height * g;
        total += g;
    }
    return Tex(colour / total, slope / total, height / total);
}

// A material laid flat on the ground (projected along y), stochastically tiled (natural ground,
// whose stones would show the tile repeating). `bump` scales its relief (0 skips the normal map).
fn surf_ground(layer: i32, tile: f32, xz: vec2<f32>, dpx: vec3<f32>, dpy: vec3<f32>, bump: f32) -> Surf {
    let t = tex_hex(layer, xz / tile, dpx.xz / tile, dpy.xz / tile, bump > 0.0);
    return Surf(t.colour, vec3<f32>(t.slope.x, 0.0, t.slope.y) * bump, t.height);
}

// A material laid flat and plainly tiled: a fine, even grain (asphalt, driven dirt) whose repeats
// do not show, under variations the shader lays itself. One lookup.
fn surf_flat(layer: i32, tile: f32, xz: vec2<f32>, dpx: vec3<f32>, dpy: vec3<f32>, bump: f32) -> Surf {
    let t = tex_at(layer, xz / tile, dpx.xz / tile, dpy.xz / tile, bump > 0.0);
    return Surf(t.colour, vec3<f32>(t.slope.x, 0.0, t.slope.y) * bump, t.height);
}

// A material projected along the three axes and blended by the normal `n`, plainly tiled: on the
// sides the image's up is the world's up (rock strata stay level).
fn surf_triplanar(layer: i32, tile: f32, p: vec3<f32>, n: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>, bump: f32) -> Surf {
    // Sharp weights: one projection alone over most of a surface, two or three only where it turns.
    let a = abs(n) * abs(n);
    var w = a * a * a * a;
    w = select(w, vec3<f32>(0.0), w < vec3<f32>(0.05 * (w.x + w.y + w.z)));
    w /= w.x + w.y + w.z;
    let relief = bump > 0.0;
    var colour = vec3<f32>(0.0);
    var tilt = vec3<f32>(0.0);
    var height = 0.0;
    // x-facing: u along +z, v down; z-facing: u along +x, v down; y-facing: u along +x, v along +z.
    if w.x > 0.0 {
        let t = tex_at(layer, vec2<f32>(p.z, -p.y) / tile, vec2<f32>(dpx.z, -dpx.y) / tile, vec2<f32>(dpy.z, -dpy.y) / tile, relief);
        colour += t.colour * w.x;
        tilt += vec3<f32>(0.0, -t.slope.y, t.slope.x) * w.x;
        height += t.height * w.x;
    }
    if w.z > 0.0 {
        let t = tex_at(layer, vec2<f32>(p.x, -p.y) / tile, vec2<f32>(dpx.x, -dpx.y) / tile, vec2<f32>(dpy.x, -dpy.y) / tile, relief);
        colour += t.colour * w.z;
        tilt += vec3<f32>(t.slope.x, -t.slope.y, 0.0) * w.z;
        height += t.height * w.z;
    }
    if w.y > 0.0 {
        let t = tex_at(layer, p.xz / tile, dpx.xz / tile, dpy.xz / tile, relief);
        colour += t.colour * w.y;
        tilt += vec3<f32>(t.slope.x, 0.0, t.slope.y) * w.y;
        height += t.height * w.y;
    }
    return Surf(colour, tilt * bump, height);
}

// `b` laid over `a` with coverage `t`: where it starts to cover, the higher parts of either show
// (pebbles of the ground poke through thin dirt, clods of the banks over it), not a fade.
fn over(a: Surf, b: Surf, t: f32) -> Surf {
    let sa = (1.0 - t) + 0.5 * a.height;
    let sb = t + 0.5 * b.height;
    let m = max(sa, sb) - 0.18;
    let wa = max(sa - m, 0.0);
    let wb = max(sb - m, 0.0);
    let k = wb / (wa + wb);
    return Surf(mix(a.colour, b.colour, k), mix(a.bump, b.bump, k), mix(a.height, b.height, k));
}

// Driven dirt: compacted, darker and lower in the ruts, streaked along the track (`uv` the track
// coordinates). The same on the dirt floor and where it spills onto a road, so the two meet
// seamlessly.
fn driven_dirt(xz: vec2<f32>, uv: vec2<f32>, dpx: vec3<f32>, dpy: vec3<f32>, bump: f32) -> Surf {
    var d = surf_ground(L_DIRT, TILE_DIRT, xz, dpx, dpy, 0.8 * bump);
    let r = ruts(uv);
    let streak = value_noise(vec2<f32>(uv.x * 0.09, uv.y * 1.4));
    d.colour *= (0.94 + 0.12 * streak) * (1.0 - 0.22 * r);
    d.height *= 1.0 - 0.5 * r;
    return d;
}

// The normal `n` of a surface with geometric normal `ng` tilted by the ruts' relief, whose screen
// derivatives are `dhx`, `dhy` (surface gradient), by `amount` (0..1).
fn rut_relief(n: vec3<f32>, ng: vec3<f32>, dpx: vec3<f32>, dpy: vec3<f32>, dhx: f32, dhy: f32, amount: f32) -> vec3<f32> {
    let r1 = cross(dpy, ng);
    let r2 = cross(ng, dpx);
    let det = dot(dpx, r1);
    let grad = sign(det) * (dhx * r1 + dhy * r2);
    let bumped = normalize(abs(det) * n - grad * 1.5);
    return normalize(mix(n, bumped, amount));
}

// Rubber laid by the cars on the asphalt, 0..1: a few lines along the road that wander slowly.
fn rubber(uv: vec2<f32>) -> f32 {
    let s = uv.x;
    let a = (value_noise(vec2<f32>(s * 0.013, 5.3)) - 0.5) * 8.0;
    let b = (value_noise(vec2<f32>(s * 0.021, 9.1)) - 0.5) * 4.0;
    var r = band(uv.y - (-0.8 + a), 0.9) + band(uv.y - (0.8 + a), 0.9);
    r += 0.5 * (band(uv.y - (-0.8 + a + b), 0.7) + band(uv.y - (0.8 + a + b), 0.7));
    let streaks = value_noise(vec2<f32>(s * 0.35, uv.y * 6.0));
    return clamp(r * (0.6 + 0.6 * streaks), 0.0, 1.0);
}

fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let p = frame.light_view_proj * vec4<f32>(world + n * 0.15 + frame.sun_dir.xyz * 0.05, 1.0);
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let texel = frame.misc.x;
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(f32(x), f32(y)) * texel, ndc.z);
        }
    }
    let inside = all(uv > vec2<f32>(0.0)) && all(uv < vec2<f32>(1.0)) && ndc.z < 1.0;
    return select(1.0, sum / 9.0, inside);
}

// ACES filmic fit (Narkowicz).
fn tonemap(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Dust hanging in the air toward the sandstorm (storm.wgsl), low over the horizon: how much of
// it a view direction looks through. The storm's own haze uses the same.
fn storm_dust(dir: vec3<f32>) -> f32 {
    let h = dir.xz / max(length(dir.xz), 1e-4);
    let toward = smoothstep(0.2, 0.95, dot(h, frame.storm_a.zw));
    let low = 1.0 - smoothstep(-0.05, 0.4, dir.y);
    return toward * low * (0.45 + 0.45 * frame.storm_b.w);
}

// Dusty orange: the far hills toward the storm fade into it, like the storm's own base.
const DUST_AIR: vec3<f32> = vec3<f32>(0.37, 0.11, 0.04);

// The storm's main wall, as in storm.wgsl: an arc of this radius, this wide, about this high.
const STORM_ARC_RADIUS: f32 = 7000.0;
const STORM_HALF_ANGLE: f32 = 0.45;
const STORM_HEIGHT: f32 = 1400.0;

// The sandstorm over the ground in front of it: x, how much of the sun its wall hides there
// (the sun stands behind it); y, how thick its dust lies there (it thickens over the last
// 900 m before the wall, short of the circuit where the storm stops).
fn storm_ground(p: vec3<f32>) -> vec2<f32> {
    let d = frame.storm_a.zw;
    let centre = frame.storm_a.xy + d * (frame.storm_b.x + STORM_ARC_RADIUS);
    let v = p.xz - centre;
    let r = max(length(v), 1.0);
    let away = v / r;
    let angle = acos(clamp(dot(away, -d), -1.0, 1.0));
    let side = 1.0 - smoothstep(0.72, 1.0, angle / STORM_HALF_ANGLE);
    // Metres in front of the wall (negative behind it).
    let gap = r - STORM_ARC_RADIUS;

    // Follow the ray to the sun back to the wall: shadowed if it meets the wall below its top.
    let sun = frame.sun_dir.xyz;
    let sun_h = max(length(sun.xz), 1e-4);
    let closing = -dot(sun.xz / sun_h, away);
    let climb = p.y - frame.storm_b.z + max(gap, 0.0) / max(closing, 1e-3) * sun.y / sun_h;
    let hidden = select(0.0, 1.0 - smoothstep(0.5, 1.0, climb / STORM_HEIGHT), closing > 0.0 || gap < 0.0);
    let dust = 1.0 - smoothstep(0.0, 900.0, gap);
    let present = select(0.0, 1.0, dot(d, d) > 0.5);
    return vec2<f32>(hidden, dust) * side * present;
}

fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let t = clamp(dir.y, 0.0, 1.0);
    var col = mix(frame.sky_horizon.rgb, frame.sky_top.rgb, pow(t, 0.55));
    col = mix(col, DUST_AIR, storm_dust(dir));
    let s = max(dot(dir, frame.sun_dir.xyz), 0.0);
    // Martian skies turn bluish around the sun.
    col += vec3<f32>(0.45, 0.6, 0.85) * pow(s, 48.0) * 0.9;
    col += vec3<f32>(1.0, 0.97, 0.92) * smoothstep(0.99955, 0.9998, s) * 10.0;
    return col;
}

/// 1 on a wire, 0 in the gaps between them (call in uniform control flow: it uses derivatives).
fn wire_lattice(uv: vec2<f32>) -> f32 {
    let period = 0.045;
    let a = abs(fract((uv.x + uv.y) / period) - 0.5);
    let b = abs(fract((uv.x - uv.y) / period) - 0.5);
    let wa = max(fwidth(a), 1e-4);
    let wb = max(fwidth(b), 1e-4);
    let on = max(smoothstep(0.36 - wa, 0.36 + wa, a), smoothstep(0.36 - wb, 0.36 + wb, b));
    // Far away the lattice would shimmer: fade to its average coverage.
    let far = smoothstep(0.08, 0.3, max(wa, wb));
    return mix(on, 0.5, far);
}

// Entry points, one per pipeline. The track's ground and roads have pipelines of their own, each
// compiled with that surface's code only: one shader for every surface would need, for every
// pixel, the registers of the heaviest one, and the GPU would run fewer pixels at once. No
// discard in the opaque ones: on tile-based GPUs (Apple) a shader that may discard turns off
// hidden surface removal for everything drawn with it.

// The track's ground (kind 2).
@fragment
fn fs_ground(in: VsOut) -> @location(0) vec4<f32> {
    return shade(in, 2u, true);
}

// The track's roads (kind 0).
@fragment
fn fs_road(in: VsOut) -> @location(0) vec4<f32> {
    return shade(in, 0u, true);
}

// Everything else: walls, rocks, cars.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return shade(in, in.kind, false);
}

// Ghosts (object tint alpha < 1): every other pixel.
@fragment
fn fs_ghost(in: VsOut) -> @location(0) vec4<f32> {
    let colour = shade(in, in.kind, false);
    let p = vec2<u32>(in.clip.xy);
    if ((p.x + p.y) & 1u) == 0u {
        discard;
    }
    return colour;
}

// The colour of a fragment of kind `k`; `terrain` compiles the ground's and the roads' code.
fn shade(in: VsOut, k: u32, terrain: bool) -> vec4<f32> {
    // Derivatives first, while control flow is still uniform.
    let xz = in.world.xz;
    let g_fine = grid(xz, 8.0);
    let g_coarse = grid(xz, 32.0);
    let n_low = value_noise(xz * 0.015);
    let n_high = value_noise(xz * 0.4);
    // Worked earth: an irregular edge between dirt, dug banks and natural ground.
    // (`dirt` is 1 on the floor, ½ on the banks, 0 beyond; `ragged` stays under ±0.12 so the
    // floor and the banks keep their look whole and only the edges between them wander.)
    let ragged = (value_noise(xz * 0.31) - 0.5) * 0.17 + (value_noise(xz * 1.7) - 0.5) * 0.07;
    let wear = smoothstep(0.62, 0.86, in.dirt + ragged);
    let dug = smoothstep(0.12, 0.36, in.dirt + ragged);
    // Bump from the dirt's relief (surface gradient from screen-space derivatives).
    let relief = dirt_relief(in.uv, wear) * dug;
    let dpx = dpdx(in.world);
    let dpy = dpdy(in.world);
    let dhx = dpdx(relief);
    let dhy = dpdy(relief);
    // Woven wire of the tyres: two diagonal families of wires every 4.5 cm, antialiased.
    let wire = wire_lattice(in.uv);
    let livery = textureSample(livery_tex, livery_sampler, in.uv).rgb;
    // Kerb stripes along the route (4 m period), antialiased; how the track coordinate across
    // changes on screen (the barriers' rounded edges).
    let kerb_aa = fwidth(in.uv.x) / 4.0;
    let across_dx = dpdx(in.uv.y);
    let across_dy = dpdy(in.uv.y);
    // The screen footprint of a metre (the procedural look's pebbles).
    let aa = length(fwidth(xz));

    var base = in.color;
    var n = normalize(in.normal);
    let eye_dist = length(frame.camera_pos.xyz - in.world);
    // Surface textures, or the earlier procedural look (a setting, to compare them).
    let textured = frame.misc.y > 0.5;
    // How much relief the textures give: full near the camera, none (and not read) far away.
    let bump = 1.0 - smoothstep(RELIEF_NEAR, RELIEF_FAR, eye_dist);
    if textured && (k == 2u || k == 22u || (k == 0u && in.dirt > 0.01)) {
        hex_at(xz);
    }
    if terrain && k == 2u && !textured {
        base = procedural_ground(in, xz, aa, g_fine, g_coarse, n_low, n_high, dug, wear);
        n = rut_relief(n, n, dpx, dpy, dhx, dhy, (1.0 - smoothstep(30.0, 90.0, eye_dist)) * dug);
    } else if terrain && k == 0u && !textured {
        base *= 1.0 - 0.12 * g_fine;
        // Earth carried onto the road before a dirt track: a dusting first, then clumps with a
        // ragged edge, then the dirt floor's own surface (the ground graded next to the road,
        // terrain.rs), so the two meet without a line.
        if in.dirt > 0.01 {
            let cover = spill_cover(in.dirt, xz, in.uv, 0.0);
            let ground = mix(KIT_GROUND, KIT_GRADED, 0.5) * (0.88 + 0.24 * n_low + 0.06 * n_high);
            let earth = procedural_driven(ground, in.uv, xz, aa);
            let clumps = smoothstep(0.42, 0.58, cover + (value_noise(xz * 1.9) - 0.5) * 0.35 + (n_high - 0.5) * 0.15);
            base = mix(mix(base, earth * 1.1, 0.45 * smoothstep(0.0, 0.5, cover)), earth, clumps);
        }
    } else if terrain && k == 2u {
        let ng = n;
        // The terrain's own colour variations (broad tints, graded pads, crater ejecta), measured
        // against the flat colour terrain.rs starts from.
        let steep_kit = smoothstep(0.93, 0.7, ng.y) * max(1.0 - 2.0 * in.dirt, 0.0);
        let tint = clamp(lum(in.color) / lum(mix(KIT_GROUND, KIT_STEEP, steep_kit)), 0.75, 1.3);
        // Natural ground: pebbles, with patches of pale slabs and drifts of fine sand. Their
        // edges are narrow (both sides are read only there), height blending makes them ragged.
        let slabby = smoothstep(0.5, 0.6, value_noise(xz * 0.021 + vec2<f32>(9.1, 2.3)));
        let sandy = smoothstep(0.7, 0.8, value_noise(xz * 0.009 + vec2<f32>(3.7, 6.1)));
        var g = Surf(KIT_GROUND, vec3<f32>(0.0), 0.5);
        if wear < 0.99 {
            if dug < 0.99 {
                if sandy < 0.99 {
                    if slabby < 0.99 {
                        g = surf_ground(L_PEBBLES, TILE_PEBBLES, xz, dpx, dpy, bump);
                    }
                    if slabby > 0.01 {
                        g = over(g, surf_ground(L_SLABS, TILE_SLABS, xz, dpx, dpy, bump), slabby);
                    }
                }
                if sandy > 0.01 {
                    g = over(g, surf_ground(L_SAND, TILE_SAND, xz, dpx, dpy, bump), sandy);
                }
            }
            // Banks: fresh dug earth full of clods.
            if dug > 0.01 {
                g = over(g, surf_ground(L_EARTH, TILE_EARTH, xz, dpx, dpy, bump), dug);
            }
        }
        if wear > 0.01 {
            g = over(g, driven_dirt(xz, in.uv, dpx, dpy, bump), wear);
        }
        // Rock on steep slopes and cliffs (not on the dug banks), in level strata, a larger tile
        // of them taking over with distance, and in broad patches a larger one still, so a long
        // cliff close to the track does not show one tile repeating along it.
        let rocky = smoothstep(0.9, 0.72, ng.y + (n_high - 0.5) * 0.1) * max(1.0 - 2.0 * in.dirt, 0.0);
        if rocky > 0.01 {
            let far_t = smoothstep(30.0, 120.0, eye_dist);
            var rock = Surf(KIT_STEEP, vec3<f32>(0.0), 0.5);
            if far_t < 0.99 {
                rock = surf_triplanar(L_ROCK, TILE_ROCK, in.world, ng, dpx, dpy, bump);
            }
            if far_t > 0.01 {
                var far = surf_triplanar(L_ROCK, TILE_ROCK * 5.3, in.world + vec3<f32>(37.0, 11.0, 53.0), ng, dpx, dpy, 0.0);
                let w = in.world;
                let broad = smoothstep(0.38, 0.62, value_noise(vec2<f32>(w.x * 0.006 + w.z * 0.004, w.y * 0.014 - w.x * 0.002 + w.z * 0.005)));
                if broad > 0.01 {
                    let wide = surf_triplanar(L_ROCK, TILE_ROCK * 12.7, w + vec3<f32>(-91.0, 23.0, 17.0), ng, dpx, dpy, 0.0);
                    far = Surf(mix(far.colour, wide.colour, broad), far.bump, mix(far.height, wide.height, broad));
                }
                rock = Surf(mix(rock.colour, far.colour, far_t), rock.bump * (1.0 - far_t), mix(rock.height, far.height, far_t));
            }
            g = over(g, rock, rocky);
        }
        // (The driven dirt is the same everywhere, on a road it spills onto too.)
        base = g.colour * mix(tint, 1.0, wear) * (0.94 + 0.12 * n_low);
        // The building grid shows on the natural ground only.
        base *= 1.0 - (0.10 * g_fine + 0.24 * g_coarse) * (1.0 - dug);
        n = normalize(ng + g.bump);
        n = rut_relief(n, ng, dpx, dpy, dhx, dhy, (1.0 - smoothstep(30.0, 90.0, eye_dist)) * dug);
    } else if k == 1u {
        base *= 0.9 + 0.14 * n_high;
        base *= 1.0 - 0.08 * g_fine - 0.14 * g_coarse;
    } else if terrain && k == 0u {
        var a = surf_flat(L_ASPHALT, TILE_ASPHALT, xz, dpx, dpy, 0.7 * bump);
        // Rubber laid along the racing lines.
        let rub = rubber(in.uv);
        a.colour *= 1.0 - 0.3 * rub;
        // Painted lines (the kit's line colour) keep the grain of the asphalt under them.
        let paint = smoothstep(0.3, 0.5, lum(in.color));
        let grain = clamp(lum(a.colour) / 0.1, 0.6, 1.4);
        a.colour = mix(a.colour, in.color * (0.62 + 0.25 * grain), paint);
        // Martian dust: blown in from the edges, in drifting patches, settled in the pores, and
        // thicker toward a dirt track.
        let edge = smoothstep(6.5, 10.0, abs(in.uv.y));
        let drift = smoothstep(0.4, 0.85, value_noise(xz * 0.06 + vec2<f32>(1.3, 8.2)) * (0.6 + 0.6 * n_high));
        let dust = clamp((0.6 * edge + 0.5 * drift + 0.5 * in.dirt) * (1.35 - a.height), 0.0, 1.0);
        a.colour = mix(a.colour, ROAD_DUST * (0.85 + 0.3 * n_high), 0.8 * dust);
        a.bump *= 1.0 - 0.6 * dust;
        // Earth carried onto the road where it meets a dirt track (`dirt` grows to 1 toward it,
        // main.rs): in patches and in the wheel paths first, laid over the asphalt by height; where
        // it covers everything it is the dirt floor's own surface, so the two meet without a line.
        var cover = 0.0;
        if in.dirt > 0.01 {
            cover = spill_cover(in.dirt, xz, in.uv, rub);
            if cover > 0.0 {
                a = over(a, driven_dirt(xz, in.uv, dpx, dpy, bump), cover);
            }
        }
        base = a.colour * mix(1.0 - 0.12 * g_fine, 0.94 + 0.12 * n_low, cover);
        // (No rut relief here: it needs screen derivatives on every road pixel.)
        n = normalize(n + a.bump);
    } else if k == 20u && textured {
        // Concrete: barriers along the route, and the sides of the platforms.
        let s = surf_triplanar(L_CONCRETE, TILE_CONCRETE, in.world, n, dpx, dpy, 0.8 * bump);
        let grain = clamp(s.colour / vec3<f32>(0.50, 0.48, 0.45), vec3<f32>(0.6), vec3<f32>(1.3));
        var c = s.colour * (in.color / KIT_LIP);
        // Weathering: streaks down the walls, dust settled at their foot and on their tops.
        let streaks = value_noise(vec2<f32>((in.world.x + in.world.z) * 1.3, in.world.y * 0.35));
        c *= 0.82 + 0.3 * streaks;
        let foot = 1.0 - smoothstep(KIT_TERRAIN_Y + 0.1, KIT_TERRAIN_Y + 1.8, in.world.y);
        let top = smoothstep(0.6, 0.9, n.y);
        let patches = value_noise(in.world.xz * 0.9 + vec2<f32>(in.world.y * 0.7));
        var dust = clamp(0.6 * foot * (0.6 + 0.6 * patches) + 0.5 * top * smoothstep(0.35, 0.75, patches), 0.0, 1.0);
        if abs(in.uv.x) + abs(in.uv.y) > 0.01 && lum(in.color) > 0.4 {
            // A barrier along the route, painted as a kerb: orange and white stripes, 2 m each,
            // with uneven ends, worn through to the concrete on its high spots and along its top
            // edges, smudged with tyre rubber on its sides.
            let wobble = (value_noise(vec2<f32>(in.uv.x * 2.5, (in.world.y + in.uv.y) * 6.0)) - 0.5) * 0.06;
            let tri = abs(fract(in.uv.x / 4.0 + wobble) - 0.5) * 2.0;
            let w = max(2.0 * kerb_aa, 0.01);
            let stripe = smoothstep(0.5 - w, 0.5 + w, tri);
            let paint = mix(vec3<f32>(0.66, 0.64, 0.60), vec3<f32>(0.62, 0.12, 0.03), stripe) * grain;
            // Across the barrier's top: 0 on its road side, 1 on its outer side.
            let across = clamp((abs(in.uv.y) - KIT_HALF_WIDTH) / KIT_LIP_WIDTH, 0.0, 1.0);
            let rim = max(smoothstep(0.25, 0.0, across), smoothstep(0.75, 1.0, across)) * top;
            let worn = 0.6 * s.height + 0.35 * value_noise(in.world.xz * 3.1 + vec2<f32>(in.world.y * 2.3)) + 0.35 * rim;
            c = mix(paint, s.colour * 0.9, 0.85 * smoothstep(0.85, 1.1, worn));
            let marks = smoothstep(0.55, 0.8, value_noise(vec2<f32>(in.uv.x * 0.6, in.world.y * 3.0))) * (1.0 - top);
            c *= 1.0 - 0.45 * marks;
            dust = max(dust, 0.45 * top * smoothstep(0.3, 0.8, patches));
            // Rounded edges: across the top the normal leans out toward both sides.
            let r1 = cross(dpy, n);
            let r2 = cross(n, dpx);
            let det = dot(dpx, r1);
            let toward = across_dx * r1 + across_dy * r2;
            if top > 0.5 && dot(toward, toward) > 1e-12 {
                let outward = normalize(toward) * sign(det) * sign(in.uv.y);
                let lean = smoothstep(0.84, 1.0, across) - smoothstep(0.16, 0.0, across);
                n = normalize(n + outward * lean * 1.2);
            }
        }
        base = mix(c, ROAD_DUST * (0.9 + 0.2 * patches), 0.75 * dust);
        n = normalize(n + s.bump);
    } else if k == 21u && textured {
        let s = surf_triplanar(L_EARTH, TILE_EARTH, in.world, n, dpx, dpy, bump);
        base = s.colour * clamp(lum(in.color) / lum(KIT_EARTH_FACE), 0.6, 1.4);
        n = normalize(n + s.bump);
    } else if k == 22u && textured {
        let s = surf_triplanar(L_ROCK, TILE_ROCK * 0.5, in.world, n, dpx, dpy, bump);
        // Dust settled on the upward faces (as scenery.rs shades them).
        let up = 0.65 * smoothstep(0.55, 0.95, n.y);
        var r = s;
        if up > 0.01 {
            r = over(s, surf_ground(L_SAND, TILE_SAND, xz, dpx, dpy, 0.5 * bump), up);
        }
        let shade = clamp(lum(in.color) / lum(mix(KIT_ROCK, KIT_GROUND * 1.04, up)), 0.7, 1.3);
        base = r.colour * shade;
        n = normalize(n + r.bump);
    } else if k == 15u {
        // Bright wires over the dark inside of the tyre.
        base = mix(base * 0.1, base * 1.5, wire);
    } else if k == 16u {
        base *= livery;
    }

    let l = frame.sun_dir.xyz;
    let ndl = max(dot(n, l), 0.0);
    let sh = shadow_factor(in.world, n);
    let hemi = mix(frame.ground_bounce.rgb, frame.sky_top.rgb, n.y * 0.5 + 0.5);
    let storm = storm_ground(in.world);
    var col = base * (frame.sun_color.rgb * ndl * sh * (1.0 - 0.9 * storm.x) + hemi);

    let to_eye = frame.camera_pos.xyz - in.world;
    let dist = length(to_eye);
    if k == 10u || k == 12u || k == 15u || k == 16u {
        let v = to_eye / max(dist, 1e-3);
        let h = normalize(l + v);
        let paint = k == 10u || k == 16u;
        let shine = select(1.0, wire, k == 15u);
        let spec = pow(max(dot(n, h), 0.0), select(40.0, 90.0, paint)) * sh * shine;
        col += frame.sun_color.rgb * spec * select(0.35, 0.5, paint);
        let fres = pow(1.0 - max(dot(n, v), 0.0), 4.0);
        col += frame.sky_horizon.rgb * fres * 0.3 * shine;
    } else if k == 13u {
        // Glass: a sharp sun glint and the sky in it, stronger at grazing angles.
        let v = to_eye / max(dist, 1e-3);
        let h = normalize(l + v);
        col += frame.sun_color.rgb * pow(max(dot(n, h), 0.0), 220.0) * sh * 1.6;
        let fres = 0.08 + 0.92 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
        let r = reflect(-v, n);
        col = mix(col, sky_color(normalize(vec3<f32>(r.x, abs(r.y), r.z))) * 0.9, fres * 0.8);
    } else if k == 14u {
        col = base * 2.4;
    }

    // Near the storm the ground sinks into its dust.
    col = mix(col, DUST_AIR, 0.9 * storm.y);
    let fog = 1.0 - exp(-max(dist - frame.fog.y, 0.0) * frame.fog.x);
    let air = mix(frame.sky_horizon.rgb, DUST_AIR, storm_dust(-to_eye / max(dist, 1e-3)));
    col = mix(col, air, fog);
    return vec4<f32>(to_srgb(tonemap(col)), 1.0);
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    let p = uv * 2.0 - 1.0;
    var o: SkyOut;
    o.clip = vec4<f32>(p, 0.0, 1.0);
    o.ndc = p;
    return o;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    // Reverse-Z: depth 1 is the near plane.
    let near = frame.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(near.xyz / near.w - frame.camera_pos.xyz);
    return vec4<f32>(to_srgb(tonemap(sky_color(dir))), 1.0);
}
