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
    // x: near shadow map texel size in uv; y: the far map's; z: how far lookups in the far map
    // are lifted off surfaces (metres, half one of its texels)
    misc: vec4<f32>,
    // xy: circuit centre (x, z), zw: unit direction from the centre to the storm (x, z)
    storm_a: vec4<f32>,
    // x: distance from the centre to the front, y: time (s), z: ground height, w: approach 0..1
    storm_b: vec4<f32>,
    // The far shadow map's sun: baked once over the whole circuit (see shadow_factor).
    far_light_view_proj: mat4x4<f32>,
    // The weather's and the viewport's (weather.wgsl).
    wind: vec4<f32>,
    drift: vec4<f32>,
    eye_vel: vec4<f32>,
    viewport: vec4<f32>,
    // The car's tyres on the ground (tyre_shade), three vectors each: the middle of the footprint
    // and the shade's strength (0: none); the axle and the footprint's half width; the heading
    // and its half length.
    contacts: array<vec4<f32>, 12>,
};

struct Object {
    model: mat4x4<f32>,
    // rgb multiplies the vertex colour; a < 1 draws the object as a ghost
    tint: vec4<f32>,
    // A wheel near the ground (squash_tyre): the ground plane (unit normal, its dot with a point
    // of the plane), and the tyre's unloaded radius, half width, and whether it is pressed on
    // that ground (x = 0: not a wheel).
    ground: vec4<f32>,
    tyre: vec4<f32>,
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
@group(0) @binding(8) var far_shadow_map: texture_depth_2d;
@group(1) @binding(0) var<uniform> object: Object;

// Vertex kinds: 0 road, 1 dirt, 2 ground, 3 wall (painted gates), 10 car paint, 11 rubber,
// 12 metal, 13 glass, 14 lights (unlit), 15 woven wire tyre (lattice from `uv`, metres), 16 car
// paint coloured by the livery texture at `uv`, 20 concrete, 21 dug earth, 22 rock, 23 tarp
// (slabs of raised roads), 24 plastic (stilts; `uv` metres along the tube, its length), 25
// inflatable bumpers, 26 sandbags, 27 straps (`uv` metres along), 28 steel (stakes, buckles:
// galvanised, or rusty by a reddish vertex colour), 29 the gates' banner (`uv` metres right of
// its middle, down from its top). A gate block's road deck carries in its colour what is painted
// on it (track's kit.rs gate_deck_colour), a booster block's where its arrows go
// (booster_deck_colour).
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

// Rubber (gfx::kind::RUBBER).
const K_RUBBER: u32 = 11u;
// Width of the rounded edge round a tyre's contact patch, m, and how far the sidewalls bulge out
// over the patch, per metre the tyre is pressed in.
const PATCH_EDGE: f32 = 0.012;
const BULGE: f32 = 0.6;

// A loaded tyre: the renderer sinks the wheel into the ground by the tyre's deflection
// (car_model::Look), and the rubber below the ground is pressed flat onto it here, the contact
// patch with a rounded edge, while the sidewalls bulge out over it.
fn squash_tyre(p: vec3<f32>) -> vec3<f32> {
    let n = object.ground.xyz;
    let centre = object.model[3].xyz;
    let r = object.tyre.x;
    let sink = r - (dot(n, centre) - object.ground.w);
    if sink <= 0.0 {
        return p;
    }
    let h = dot(n, p) - object.ground.w;
    // max(h, 0), rounded over PATCH_EDGE.
    let e = max(PATCH_EDGE - abs(h), 0.0) / PATCH_EDGE;
    let lifted = max(h, 0.0) + e * e * PATCH_EDGE * 0.25;
    let axle = normalize(object.model[0].xyz);
    let across = dot(p - centre, axle);
    let side = sign(across) * smoothstep(0.35, 0.95, abs(across) / object.tyre.y);
    let low = 1.0 - smoothstep(0.0, 0.5 * r, lifted);
    return p + n * (lifted - h) + axle * (side * low * sink * BULGE);
}

// How much ambient light reaches the ground at `p` past the car's tyres: each hides the sky
// from the ground round its footprint, darkest at its edge and fading over a few decimetres.
const TYRE_SHADE: f32 = 0.6;
const TYRE_SHADE_REACH: f32 = 0.16;

fn tyre_shade(p: vec3<f32>) -> f32 {
    var lit = 1.0;
    for (var i = 0u; i < 4u; i++) {
        let c = frame.contacts[3u * i];
        if c.w <= 0.0 {
            continue;
        }
        let axle = frame.contacts[3u * i + 1u];
        let heading = frame.contacts[3u * i + 2u];
        let v = p - c.xyz;
        let q = vec3<f32>(
            max(abs(dot(v, axle.xyz)) - axle.w, 0.0),
            max(abs(dot(v, heading.xyz)) - heading.w, 0.0),
            dot(v, cross(axle.xyz, heading.xyz)),
        );
        lit *= 1.0 - c.w * TYRE_SHADE * exp(-length(q) / TYRE_SHADE_REACH);
    }
    return lit;
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var world = object.model * vec4<f32>(v.pos, 1.0);
    if object.tyre.z > 0.5 && v.kind == K_RUBBER {
        world = vec4<f32>(squash_tyre(world.xyz), 1.0);
    }
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

// Value noise stretched along the track (`uv` the track coordinates, scaled by `along` and
// `across`), its lattice turned by 45°: the edges of its cells then run along the track like the
// streaks themselves, instead of across it like planks.
fn streak_noise(uv: vec2<f32>, along: f32, across: f32) -> f32 {
    let q = vec2<f32>(uv.x * along, uv.y * across);
    return value_noise(vec2<f32>(q.x + q.y, q.y - q.x) * 0.70710678);
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
    let streak = streak_noise(uv, 0.09, 1.4);
    return wear * (-0.05 * ruts(uv) + 0.02 * streak);
}

// ---------------------------------------------------------------------------------------------
// Surface textures.

// Layers of the surface textures (surfaces.rs, tools/textures/bake.py) and the size of one tile
// of each, metres.
const L_TARP: i32 = 0;
const L_DIRT: i32 = 1;
const L_EARTH: i32 = 2;
const L_PEBBLES: i32 = 3;
const L_SLABS: i32 = 4;
const L_SAND: i32 = 5;
const L_ROCK: i32 = 6;
const L_CONCRETE: i32 = 7;
const L_SANDBAG: i32 = 8;
const L_WEBBING: i32 = 9;
const L_GALVANIZED: i32 = 10;
const L_RUST: i32 = 11;
// The gates' lettering (tools/textures/signs.py): five rows, each five times as wide as tall,
// the ink in alpha: PLANET, TRACKS (side by side on the banner), DÉPART, ARRIVÉE, CHECKPOINT.
const L_SIGNS: i32 = 12;
// The booster arrow (tools/textures/booster.py): one chevron filling the layer, pointing up, the
// paint's colour in RGB and its coverage in alpha.
const L_BOOSTER: i32 = 13;
const TILE_TARP: f32 = 1.6;
const TILE_DIRT: f32 = 4.0;
const TILE_EARTH: f32 = 2.5;
const TILE_PEBBLES: f32 = 3.0;
const TILE_SLABS: f32 = 6.0;
const TILE_SAND: f32 = 7.0;
const TILE_ROCK: f32 = 9.0;
const TILE_CONCRETE: f32 = 2.8;
const TILE_SANDBAG: f32 = 0.6;
const TILE_WEBBING: f32 = 0.2;
const TILE_STEEL: f32 = 0.22;

// The relief (normal maps) fades out between these distances, metres; beyond, it is not read.
const RELIEF_NEAR: f32 = 30.0;
const RELIEF_FAR: f32 = 50.0;

// Colours the vertex colours are measured against (track/src/kit.rs, terrain.rs, scenery.rs).
const KIT_GROUND: vec3<f32> = vec3<f32>(0.55, 0.22, 0.10);
const KIT_STEEP: vec3<f32> = vec3<f32>(0.27, 0.115, 0.065);
const KIT_LIP: vec3<f32> = vec3<f32>(0.68, 0.68, 0.66);
const KIT_SLAB: vec3<f32> = vec3<f32>(0.62, 0.60, 0.56);
const KIT_COLLAR: vec3<f32> = vec3<f32>(0.70, 0.70, 0.67);
const KIT_SANDBAG: vec3<f32> = vec3<f32>(0.50, 0.27, 0.13);
const KIT_EARTH_FACE: vec3<f32> = vec3<f32>(0.40, 0.16, 0.075);
const KIT_ROCK: vec3<f32> = vec3<f32>(0.30, 0.13, 0.075);
// The kit's road half-width (kit.rs HALF_WIDTH), metres.
const KIT_HALF_WIDTH: f32 = 10.0;
// The terrain's level next to the blocks (kit.rs TERRAIN_Y).
const KIT_TERRAIN_Y: f32 = -0.25;
// Fine Martian dust settled on the roads and the barriers.
const ROAD_DUST: vec3<f32> = vec3<f32>(0.36, 0.13, 0.055);

// The tarp of the road decks (art/roads/brief.md): lengths of tarp DECK_STRIP m wide laid along
// the road and welded where they overlap, cut every DECK_PANEL m, the cuts staggered from one
// length to the next.
const DECK_STRIP: f32 = 5.0;
const DECK_PANEL: f32 = 12.0;
// Stencilled colours: the orange edge lines, the black dashes inside them.
const STENCIL_ORANGE: vec3<f32> = vec3<f32>(0.78, 0.2, 0.025);
const STENCIL_BLACK: vec3<f32> = vec3<f32>(0.025, 0.024, 0.023);
// Booster arrows on a booster block's deck: chevrons BOOST_WIDTH m wide and BOOST_ARROW m long,
// about every BOOST_SPACING m (as many as fit the block evenly), pointing the way to go. A band
// of light runs forward over their orange every 1 / BOOST_PULSE_RATE s, BOOST_PULSE_LEN m apart.
const BOOST_WIDTH: f32 = 14.0;
const BOOST_ARROW: f32 = 12.0;
const BOOST_SPACING: f32 = 16.0;
const BOOST_PULSE_RATE: f32 = 1.6;
const BOOST_PULSE_LEN: f32 = 32.0;
const BOOST_GLOW: f32 = 0.9;
// The bumpers' two colours.
const BUMPER_RED: vec3<f32> = vec3<f32>(0.55, 0.03, 0.022);
const BUMPER_WHITE: vec3<f32> = vec3<f32>(0.70, 0.68, 0.64);
// Orange webbing of the straps, and the steel of stakes and buckles.
const STRAP_ORANGE: vec3<f32> = vec3<f32>(0.62, 0.17, 0.015);
const STEEL: vec3<f32> = vec3<f32>(0.4, 0.4, 0.42);

// The booster arrow's paint at `t` (0..1 across, and down from its tip), with `gx`, `gy` the
// screen derivatives of `t`: its colour and coverage; none outside it.
fn booster_paint(t: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> vec4<f32> {
    if any(t < vec2<f32>(0.0)) || any(t > vec2<f32>(1.0)) {
        return vec4<f32>(0.0);
    }
    return textureSampleGrad(surf_colour, surf_sampler, t, L_BOOSTER, gx, gy);
}

// The ink of row `row` of the signs layer at `t` (0..1 across and down the row), with `gx`, `gy`
// the screen derivatives of `t`; none outside the row.
fn sign_ink(row: f32, t: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> f32 {
    if any(t < vec2<f32>(0.0)) || any(t > vec2<f32>(1.0)) {
        return 0.0;
    }
    let s = vec2<f32>(1.0, 0.2);
    return textureSampleGrad(surf_colour, surf_sampler, vec2<f32>(t.x, (row + t.y) * 0.2), L_SIGNS, gx * s, gy * s).a;
}

// A checkerboard of `size` squares at `p`, antialiased over `aa` (metres a pixel covers): 1 on
// the dark squares.
fn checkers(p: vec2<f32>, size: f32, aa: f32) -> f32 {
    let c = p / size;
    let f = abs(fract(c) - 0.5);
    let w = max(aa / size, 1e-4);
    let sx = smoothstep(0.25 - w, 0.25 + w, f.x) * 2.0 - 1.0;
    let sy = smoothstep(0.25 - w, 0.25 + w, f.y) * 2.0 - 1.0;
    let parity = select(1.0, -1.0, (i32(floor(c.x + 0.5)) + i32(floor(c.y + 0.5))) % 2 == 0);
    return clamp(0.5 + 0.5 * sx * sy * parity, 0.0, 1.0);
}

// The panel of deck tarp at track coordinates `uv` (metres along, across): its length across and
// its index along (xy), and the point's place in it, metres from its corner (zw).
fn deck_panel(uv: vec2<f32>) -> vec4<f32> {
    let j = floor((uv.y + 0.5 * DECK_STRIP) / DECK_STRIP);
    let along = uv.x + hash2(vec2<f32>(j, 3.7)) * DECK_PANEL;
    let i = floor(along / DECK_PANEL);
    return vec4<f32>(j, i, along - i * DECK_PANEL, uv.y + 0.5 * DECK_STRIP - j * DECK_STRIP);
}

// Distance from a point of a panel (`local`, from `deck_panel`) to its nearest edge, metres.
fn panel_edge(local: vec2<f32>) -> f32 {
    return min(min(local.x, DECK_PANEL - local.x), min(local.y, DECK_STRIP - local.y));
}

// Height of the welds between the deck's panels, metres: the overlapping edge stands a few
// millimetres proud.
fn deck_weld(uv: vec2<f32>) -> f32 {
    let d = panel_edge(deck_panel(uv).zw);
    return 0.004 * (1.0 - smoothstep(0.0, 0.07, d));
}

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
    let streak = streak_noise(uv, 0.09, 1.4);
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
    let streaks = streak_noise(uv, 0.35, 6.0);
    return clamp(r * (0.6 + 0.6 * streaks), 0.0, 1.0);
}

// Where `p` falls in a shadow map of sun `m`: xy, uv; z, depth; w, 0 outside the map rising to 1
// `band` (in uv) inside its edges.
fn shadow_coords(m: mat4x4<f32>, p: vec3<f32>, band: f32) -> vec4<f32> {
    let c = m * vec4<f32>(p, 1.0);
    let ndc = c.xyz / c.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let inside = select(0.0, smoothstep(0.0, band, edge), ndc.z < 1.0);
    return vec4<f32>(uv, ndc.z, inside);
}

// How much sun reaches `world`, on a surface of normal `n`, from two shadow maps: the near one,
// sharp, redrawn every frame over the ground just ahead of the camera (the car's shadow is
// there), and the far one, coarser, baked once over the whole circuit. Toward the near map's
// edges the far one takes over across a band, and far shadows only sharpen as they come closer.
// Each lookup is lifted off the surface against acne, by about a texel of the near map (7 cm),
// half one of the far map's; more, and the shadows of low things (sandbags, stakes) start well
// away from their feet, which then seem to float.
fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let near = shadow_coords(frame.light_view_proj, world + n * 0.06 + frame.sun_dir.xyz * 0.03, 0.12);
    var sun = 1.0;
    if near.w > 0.0 {
        let texel = frame.misc.x;
        var sum = 0.0;
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                sum += textureSampleCompareLevel(shadow_map, shadow_sampler, near.xy + vec2<f32>(f32(x), f32(y)) * texel, near.z);
            }
        }
        sun = sum / 9.0;
    }
    if near.w < 1.0 {
        let lift = frame.misc.z;
        let far = shadow_coords(frame.far_light_view_proj, world + n * lift + frame.sun_dir.xyz * (0.5 * lift), 0.03);
        var far_sun = 1.0;
        if far.w > 0.0 {
            let texel = frame.misc.y;
            var sum = 0.0;
            for (var y = -1; y <= 1; y++) {
                for (var x = -1; x <= 1; x++) {
                    sum += textureSampleCompareLevel(far_shadow_map, shadow_sampler, far.xy + vec2<f32>(f32(x), f32(y)) * texel, far.z);
                }
            }
            far_sun = mix(1.0, sum / 9.0, far.w);
        }
        sun = mix(far_sun, sun, near.w);
    }
    return sun;
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
    // The bumpers' stripes along the route (4 m period), antialiased.
    let kerb_aa = fwidth(in.uv.x) / 4.0;
    // How many metres of track a pixel covers, how the track coordinate along changes on screen
    // (the sandbags' bulges, the straps' webbing); the welds of the road decks' tarp, their
    // relief.
    let uv_aa = max(fwidth(in.uv.x), fwidth(in.uv.y));
    let along_dx = dpdx(in.uv.x);
    let along_dy = dpdy(in.uv.x);
    let uv_dx = dpdx(in.uv);
    let uv_dy = dpdy(in.uv);
    let weld = deck_weld(in.uv);
    let dwx = dpdx(weld);
    let dwy = dpdy(weld);

    var base = in.color;
    var n = normalize(in.normal);
    let eye_dist = length(frame.camera_pos.xyz - in.world);
    // How much relief the textures give: full near the camera, none (and not read) far away.
    let bump = 1.0 - smoothstep(RELIEF_NEAR, RELIEF_FAR, eye_dist);
    // A soft highlight on plastics: strength and exponent (none on matt surfaces).
    var sheen = vec2<f32>(0.0, 1.0);
    // How much a surface is bare metal (0..1): it then mirrors the sky and the ground, tinted by
    // its colour, instead of scattering the light.
    var metal = 0.0;
    // Light a surface gives off itself (the booster arrows' pulse), added after the lighting.
    var emit = vec3<f32>(0.0);
    if k == 2u || k == 22u || (k == 0u && in.dirt > 0.01) {
        hex_at(xz);
    }
    // Patches of pale slabs and drifts of sand on the natural ground.
    let slabby = smoothstep(0.5, 0.6, value_noise(xz * 0.021 + vec2<f32>(9.1, 2.3)));
    let sandy = smoothstep(0.7, 0.8, value_noise(xz * 0.009 + vec2<f32>(3.7, 6.1)));
    if terrain && k == 2u {
        let ng = n;
        // The terrain's own colour variations (broad tints, graded pads, crater ejecta), measured
        // against the flat colour terrain.rs starts from.
        let steep_kit = smoothstep(0.93, 0.7, ng.y) * max(1.0 - 2.0 * in.dirt, 0.0);
        let tint = clamp(lum(in.color) / lum(mix(KIT_GROUND, KIT_STEEP, steep_kit)), 0.75, 1.3);
        // Natural ground: pebbles, with patches of pale slabs and drifts of fine sand. Their
        // edges are narrow (both sides are read only there), height blending makes them ragged.
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
        n = normalize(ng + g.bump);
        n = rut_relief(n, ng, dpx, dpy, dhx, dhy, (1.0 - smoothstep(30.0, 90.0, eye_dist)) * dug);
    } else if k == 1u {
        base *= 0.9 + 0.14 * n_high;
    } else if terrain && k == 0u {
        // Laminated tarp over the deck panels: the lengths and their welds, a panel of another
        // make now and then, patches, stencilled lines, eyelets along the edges.
        var a = surf_flat(L_TARP, TILE_TARP, xz, dpx, dpy, 0.6 * bump);
        let pn = deck_panel(in.uv);
        let h1 = hash2(pn.xy + vec2<f32>(0.37, 1.91));
        let h2 = hash2(pn.xy + vec2<f32>(5.3, 0.71));
        var tint = vec3<f32>(0.8 + 0.09 * h1);
        if h2 < 0.08 {
            tint *= vec3<f32>(0.84, 0.88, 0.9);
        } else if h2 > 0.93 {
            tint *= vec3<f32>(1.0, 0.93, 0.82);
        }
        // Detail finer than a few pixels fades to its average.
        let fine = 1.0 - smoothstep(0.02, 0.09, uv_aa);
        let aa = max(uv_aa, 0.004);
        // A patch on some panels: a rectangle of grey tarp welded on.
        let h3 = hash2(pn.xy + vec2<f32>(9.2, 4.4));
        if h3 < 0.2 {
            let centre = vec2<f32>(2.0 + 8.0 * hash2(pn.xy + vec2<f32>(1.1, 7.7)), 1.2 + 2.6 * hash2(pn.xy + vec2<f32>(6.6, 2.2)));
            let half = vec2<f32>(0.5 + 0.5 * h3 * 5.0, 0.35 + 0.25 * h1);
            let q = abs(pn.zw - centre) - half;
            let inside = 1.0 - smoothstep(-aa, aa, max(q.x, q.y));
            let rim = (1.0 - smoothstep(0.0, 0.03 + aa, abs(max(q.x, q.y)))) * fine;
            tint = mix(tint, vec3<f32>(0.62, 0.64, 0.6), inside) * (1.0 - 0.25 * rim);
        }
        // The welds: a dark line where an edge overlaps, a lighter band of melted laminate.
        let edge = panel_edge(pn.zw);
        let line = 1.0 - smoothstep(0.012, 0.012 + aa, edge);
        let band = 1.0 - smoothstep(0.06, 0.06 + aa, edge);
        tint *= mix(1.0, (1.0 + 0.05 * band) * (1.0 - 0.3 * line), fine);
        a.colour *= tint;
        // A gate block's deck says what is painted on it (track's kit.rs gate_deck_colour), a
        // booster block's where its arrows go (booster_deck_colour), with the same flags.
        let gate_deck = in.color.r < 0.005;
        let boost_deck = in.color.r > 1.0;
        let coded = gate_deck || boost_deck;
        let code = select(u32(floor(in.color.b * 16.0)), u32(floor((in.color.r - 1.0) * 16.0)), boost_deck);
        // Stencilled marks: the kit's edge lines painted orange, black dashes inside them.
        let grain = clamp(lum(a.colour) / 0.45, 0.7, 1.3);
        let paint = select(smoothstep(0.3, 0.5, lum(in.color)), select(0.0, 1.0, (code & 8u) != 0u), coded);
        a.colour = mix(a.colour, STENCIL_ORANGE * (0.8 + 0.2 * grain), paint);
        let u = abs(in.uv.y);
        let dash = (smoothstep(KIT_HALF_WIDTH - 1.25 - aa, KIT_HALF_WIDTH - 1.25 + aa, u) - smoothstep(KIT_HALF_WIDTH - 0.95 - aa, KIT_HALF_WIDTH - 0.95 + aa, u))
            * smoothstep(0.5 + aa * 0.25, 0.5 - aa * 0.25, fract(in.uv.x / 4.0));
        a.colour = mix(a.colour, STENCIL_BLACK, 0.9 * dash);
        // The tarp's edges (the kit tells them apart by the blue of the vertex colour): strapped
        // down under bumpers, or pinned by stakes in front of a row of sandbags.
        let strapped = select(in.color.b - in.color.r > 0.02, (code & 4u) != 0u, coded);
        var fixings = 0.0;
        if strapped {
            // Eyelets every 0.9 m, and every 4 m a strap across the edge to the bumper's own
            // strap (in the middle of a white length), its ratchet buckle on the tarp.
            let e = vec2<f32>((fract(in.uv.x / 0.9) - 0.5) * 0.9, u - (KIT_HALF_WIDTH - 0.22));
            let r = length(e);
            let ring = (1.0 - smoothstep(0.06, 0.06 + aa, r)) * fine;
            let hole = 1.0 - smoothstep(0.03, 0.03 + aa, r);
            a.colour = mix(a.colour, mix(vec3<f32>(0.42, 0.36, 0.25), vec3<f32>(0.03), hole), ring);
            let q = (in.uv.x - 2.0) / 4.0;
            let to_strap = abs(q - round(q)) * 4.0;
            let strap = (1.0 - smoothstep(0.075, 0.075 + aa, to_strap)) * smoothstep(KIT_HALF_WIDTH - 1.0 - aa, KIT_HALF_WIDTH - 1.0 + aa, u);
            a.colour = mix(a.colour, STRAP_ORANGE * (0.85 + 0.15 * grain), strap);
            let bq = abs(vec2<f32>(to_strap, u - (KIT_HALF_WIDTH - 0.7))) - vec2<f32>(0.1, 0.08);
            let buckle = (1.0 - smoothstep(-aa, aa, max(bq.x, bq.y))) * fine;
            a.colour = mix(a.colour, STEEL, buckle);
            fixings = max(max(strap, buckle), ring);
        } else {
            // Eyelets along the tarp's edge (the verge's, 0.5 m past the deck's, where the stakes
            // pin it against the bags), punched by hand: about every 0.9 m, a little off line,
            // now and then one missing.
            let cell = floor(in.uv.x / 0.9);
            let off = vec2<f32>(hash2(vec2<f32>(cell, 1.3)) - 0.5, hash2(vec2<f32>(cell, 4.1)) - 0.5);
            let e = vec2<f32>((fract(in.uv.x / 0.9) - 0.5) * 0.9 - 0.3 * off.x, u - (KIT_HALF_WIDTH + 0.38) - 0.03 * off.y);
            let r = length(e) + select(0.0, 1.0, hash2(vec2<f32>(cell, 7.7)) < 0.15);
            let ring = (1.0 - smoothstep(0.06, 0.06 + aa, r)) * fine;
            let hole = 1.0 - smoothstep(0.03, 0.03 + aa, r);
            a.colour = mix(a.colour, mix(vec3<f32>(0.42, 0.36, 0.25), vec3<f32>(0.03), hole), ring);
            fixings = ring;
        }
        if gate_deck {
            // The gate's line across the deck (`y` metres along from it), checkered at the start
            // and the finish, orange at a checkpoint; before it, its word in big stencilled
            // letters, read by a driver coming through: DÉPART, CHECKPOINT, ARRIVÉE.
            let kind = code & 3u;
            let y = in.uv.x - in.color.g * 8192.0;
            let x = in.uv.y;
            let worn = 0.85 + 0.15 * value_noise(xz * 3.7);
            if kind == 2u {
                let band = 1.0 - smoothstep(0.35 - aa, 0.35 + aa, abs(y));
                a.colour = mix(a.colour, STENCIL_ORANGE * (0.8 + 0.2 * grain), band * worn);
            } else {
                let band = 1.0 - smoothstep(0.6 - aa, 0.6 + aa, abs(y));
                let dark = checkers(vec2<f32>(x, y + 0.6), 0.4, aa);
                let white = vec3<f32>(0.86, 0.85, 0.82) * grain;
                a.colour = mix(a.colour, mix(white, STENCIL_BLACK, dark), band * worn);
            }
            // The word: 15 m by 3 m, 3 m before the line; its rows in the signs layer.
            let row = select(select(2.0, 3.0, kind == 3u), 4.0, kind == 2u);
            let t = vec2<f32>((7.5 - x) / 15.0, (-3.0 - y) / 3.0);
            let gx = vec2<f32>(-uv_dx.y / 15.0, -uv_dx.x / 3.0);
            let gy = vec2<f32>(-uv_dy.y / 15.0, -uv_dy.x / 3.0);
            let ink = sign_ink(row, t, gx, gy);
            a.colour = mix(a.colour, STENCIL_BLACK, 0.92 * ink * worn);
            fixings = max(fixings, ink);
        }
        if boost_deck {
            // The arrows, `y` metres along the block's deck from its start; in each slot of the
            // deck one chevron, its tip forward.
            let y = in.uv.x - in.color.g * 8192.0;
            let deck_len = in.color.b * 1024.0;
            let count = max(round(deck_len / BOOST_SPACING), 1.0);
            let spacing = deck_len / count;
            let slot = clamp(floor(y / spacing), 0.0, count - 1.0);
            let from_tail = y - slot * spacing - 0.5 * (spacing - BOOST_ARROW);
            let t = vec2<f32>((0.5 * BOOST_WIDTH - in.uv.y) / BOOST_WIDTH, 1.0 - from_tail / BOOST_ARROW);
            let gx = vec2<f32>(-uv_dx.y / BOOST_WIDTH, -uv_dx.x / BOOST_ARROW);
            let gy = vec2<f32>(-uv_dy.y / BOOST_WIDTH, -uv_dy.x / BOOST_ARROW);
            let arrow = booster_paint(t, gx, gy);
            let worn = 0.94 + 0.06 * value_noise(xz * 2.3);
            a.colour = mix(a.colour, arrow.rgb * (0.85 + 0.15 * grain), arrow.a * worn);
            fixings = max(fixings, arrow.a);
            // The pulse: a band of light running forward over the orange, the arrows glowing a
            // little between bands.
            let orange = arrow.a * smoothstep(0.08, 0.3, arrow.r - arrow.b);
            let phase = fract(frame.storm_b.y * BOOST_PULSE_RATE - y / BOOST_PULSE_LEN);
            emit = arrow.rgb * (BOOST_GLOW * orange * worn * (0.15 + pow(phase, 8.0)));
        }
        // Rubber laid along the racing lines.
        let rub = rubber(in.uv);
        a.colour = mix(a.colour, vec3<f32>(0.035, 0.033, 0.032), 0.55 * rub);
        // Martian dust: blown in from the edges, in drifting patches, caught in the welds and the
        // weave, and thicker toward a dirt track.
        let edge_dust = smoothstep(6.5, 10.0, u);
        let drift = smoothstep(0.4, 0.85, value_noise(xz * 0.06 + vec2<f32>(1.3, 8.2)) * (0.6 + 0.6 * n_high));
        let dust = clamp((0.55 * edge_dust + 0.45 * drift + 0.5 * in.dirt + 0.12 * band) * (1.3 - a.height), 0.0, 1.0);
        a.colour = mix(a.colour, ROAD_DUST * (0.95 + 0.3 * n_high), 0.75 * dust);
        a.bump *= 1.0 - 0.6 * dust;
        // Earth carried onto the road where it meets a dirt track (`dirt` grows to 1 toward it,
        // main.rs): in patches and in the wheel paths first, laid over the tarp by height; where
        // it covers everything it is the dirt floor's own surface, so the two meet without a line.
        var cover = 0.0;
        if in.dirt > 0.01 {
            cover = spill_cover(in.dirt, xz, in.uv, rub);
            if cover > 0.0 {
                a = over(a, driven_dirt(xz, in.uv, dpx, dpy, bump), cover);
            }
        }
        base = a.colour * mix(1.0, 0.94 + 0.12 * n_low, cover);
        let ng = n;
        n = normalize(n + a.bump);
        n = rut_relief(n, ng, dpx, dpy, dwx, dwy, bump * (1.0 - cover));
        sheen = vec2<f32>(0.1 * (1.0 - dust) * (1.0 - cover) * (1.0 - rub) * (1.0 - fixings), 24.0);
    } else if k == 20u {
        // Concrete: the sides of dirt mounds.
        let s = surf_triplanar(L_CONCRETE, TILE_CONCRETE, in.world, n, dpx, dpy, 0.8 * bump);
        let grain = clamp(s.colour / vec3<f32>(0.50, 0.48, 0.45), vec3<f32>(0.6), vec3<f32>(1.3));
        var c = s.colour * (in.color / KIT_LIP);
        // Weathering: streaks down the walls, dust settled at their foot and on their tops.
        let streaks = value_noise(vec2<f32>((in.world.x + in.world.z) * 1.3, in.world.y * 0.35));
        c *= 0.82 + 0.3 * streaks;
        let foot = 1.0 - smoothstep(KIT_TERRAIN_Y + 0.1, KIT_TERRAIN_Y + 1.8, in.world.y);
        let top = smoothstep(0.6, 0.9, n.y);
        let patches = value_noise(in.world.xz * 0.9 + vec2<f32>(in.world.y * 0.7));
        let dust = clamp(0.6 * foot * (0.6 + 0.6 * patches) + 0.5 * top * smoothstep(0.35, 0.75, patches), 0.0, 1.0);
        base = mix(c, ROAD_DUST * (0.9 + 0.2 * patches), 0.75 * dust);
        n = normalize(n + s.bump);
    } else if k == 25u {
        // Inflatable bumpers along a road: tubes of coated tarp (shaded round by their vertex
        // normals), red and white lengths of 2 m welded end to end, an orange strap across every
        // 4 m, dust on top and at the foot.
        let s = surf_triplanar(L_TARP, TILE_TARP, in.world, n, dpx, dpy, 0.5 * bump);
        let grain = clamp(lum(s.colour) / 0.45, 0.75, 1.25);
        let top = smoothstep(0.6, 0.9, n.y);
        let tri = abs(fract(in.uv.x / 4.0) - 0.5) * 2.0;
        let w = max(2.0 * kerb_aa, 0.004);
        let stripe = smoothstep(0.5 - w, 0.5 + w, tri);
        var c = mix(BUMPER_WHITE, BUMPER_RED, stripe) * grain;
        // The welds between lengths, and a strap across the middle of every white length (2 m,
        // 6 m...), its webbing darker at its edges.
        c *= 1.0 - 0.35 * (1.0 - smoothstep(0.0, 2.0 * w + 0.004, abs(tri - 0.5)));
        let q = (in.uv.x - 2.0) / 4.0;
        let to_strap = abs(q - round(q)) * 4.0;
        let strap = 1.0 - smoothstep(0.075, 0.075 + 4.0 * kerb_aa + 0.004, to_strap);
        let webbing = 0.75 + 0.25 * smoothstep(0.075, 0.03, to_strap);
        c = mix(c, STRAP_ORANGE * webbing * grain, strap);
        let patches = value_noise(in.world.xz * 0.9 + vec2<f32>(in.world.y * 0.7));
        let dust = clamp(0.4 * top * smoothstep(0.35, 0.85, patches), 0.0, 1.0);
        base = mix(c, ROAD_DUST * (0.9 + 0.2 * patches), 0.7 * dust);
        n = normalize(n + s.bump);
        sheen = vec2<f32>(0.22 * (1.0 - dust) * (1.0 - strap), 36.0);
    } else if k == 23u {
        // Tarp wrapped round the slabs of raised roads: the decks' tarp in the vertex colour (over
        // the kit's slab colour), streaked down the sides, dust settled on top. Underneath, in the
        // shade, it catches the light of the ground.
        let s = surf_triplanar(L_TARP, TILE_TARP, in.world, n, dpx, dpy, 0.8 * bump);
        var c = s.colour * (in.color / KIT_SLAB);
        let streaks = value_noise(vec2<f32>((in.world.x + in.world.z) * 1.3, in.world.y * 0.35));
        c *= (0.84 + 0.26 * streaks) * (1.0 + 0.6 * smoothstep(-0.3, -0.8, n.y));
        let top = smoothstep(0.6, 0.9, n.y);
        let patches = value_noise(in.world.xz * 0.9 + vec2<f32>(in.world.y * 0.7));
        let dust = clamp(0.55 * top * smoothstep(0.25, 0.75, patches) + 0.2 * smoothstep(0.6, 0.9, patches), 0.0, 1.0);
        base = mix(c, ROAD_DUST * (0.9 + 0.2 * patches), 0.75 * dust);
        n = normalize(n + s.bump);
        sheen = vec2<f32>(0.06 * (1.0 - dust), 16.0);
    } else if k == 24u {
        // Glossy plastic: the stilts' red tubes with grey clamps at both ends (`uv`: metres from
        // the tube's start, its length), the base plates, dust settled on top.
        var c = in.color;
        if in.uv.y > 0.5 {
            let end = min(in.uv.x, in.uv.y - in.uv.x);
            c = mix(c, KIT_COLLAR, 1.0 - smoothstep(0.3, 0.3 + max(uv_aa, 0.01), end));
        }
        let top = smoothstep(0.35, 0.9, n.y);
        let patches = value_noise(in.world.xz * 1.7 + vec2<f32>(in.world.y * 1.1));
        let dust = clamp(0.7 * top * smoothstep(0.25, 0.7, patches) + 0.2 * smoothstep(0.6, 0.85, patches), 0.0, 1.0);
        base = mix(c, ROAD_DUST * 1.1, dust);
        sheen = vec2<f32>(0.45 * (1.0 - dust), 60.0);
    } else if k == 26u {
        // Sandbags of regolith: woven sackcloth stained by the dust, each bag shaded round by its
        // normals, each its own shade (`uv`: its brightness, and how far it has gone rusty-brown).
        let s = surf_triplanar(L_SANDBAG, TILE_SANDBAG, in.world, n, dpx, dpy, bump);
        let tone = select(0.36, in.uv.x, in.uv.x > 0.01);
        var c = s.colour * (in.color / KIT_SANDBAG) * tone * mix(vec3<f32>(0.95, 0.85, 0.72), vec3<f32>(1.0, 0.66, 0.44), in.uv.y);
        c *= 0.85 + 0.25 * value_noise(in.world.xz * 2.3 + vec2<f32>(in.world.y * 1.7));
        let top = smoothstep(0.5, 0.9, n.y);
        let patches = value_noise(in.world.xz * 1.3 + vec2<f32>(in.world.y * 0.9));
        let dust = clamp(0.8 * top * smoothstep(0.15, 0.7, patches) + 0.35 * smoothstep(0.45, 0.85, patches), 0.0, 1.0);
        base = mix(c, ROAD_DUST * (0.8 + 0.25 * patches), 0.6 * dust);
        n = normalize(n + s.bump);
    } else if k == 27u {
        // Orange webbing: the strap's weave along it (`uv`: metres along the strap).
        let t = tex_at(L_WEBBING, vec2<f32>(in.uv.x / TILE_WEBBING, 0.37), vec2<f32>(along_dx / TILE_WEBBING, 0.0), vec2<f32>(along_dy / TILE_WEBBING, 0.0), false);
        let dust = 0.3 * smoothstep(0.4, 0.9, n.y);
        base = mix(t.colour, ROAD_DUST, dust);
        sheen = vec2<f32>(0.12, 20.0);
    } else if k == 29u {
        // The gates' banner: white fabric, checkered at both ends, PLANET TRACKS across its
        // middle over an orange stripe, hemmed top and bottom, dusty toward its lower edge. `uv`:
        // metres to the viewer's right of its middle, metres down from its top (1.8 m high).
        let x = in.uv.x;
        let y = in.uv.y;
        let aa = max(uv_aa, 0.002);
        let t = tex_at(L_TARP, in.uv / TILE_TARP, uv_dx / TILE_TARP, uv_dy / TILE_TARP, false);
        let grain = clamp(lum(t.colour) / 0.45, 0.8, 1.2);
        var c = vec3<f32>(0.80, 0.79, 0.76) * grain;
        // The words: 9 m by 1.8 m each, side by side.
        let tw = (x + 9.0) / 9.0;
        let row = select(0.0, 1.0, tw >= 1.0);
        let tt = vec2<f32>(tw - row, y / 1.8);
        let gx = vec2<f32>(uv_dx.x / 9.0, uv_dx.y / 1.8);
        let gy = vec2<f32>(uv_dy.x / 9.0, uv_dy.y / 1.8);
        let ink = select(0.0, sign_ink(row, tt, gx, gy), abs(x) < 9.0);
        let stripe = (smoothstep(1.43 - aa, 1.43 + aa, y) - smoothstep(1.53 - aa, 1.53 + aa, y)) * (1.0 - smoothstep(8.2 - aa, 8.2 + aa, abs(x)));
        let ends = smoothstep(9.3 - aa, 9.3 + aa, abs(x));
        let dark = checkers(vec2<f32>(x, y - 0.09), 0.45, aa) * ends;
        let hem = 1.0 - smoothstep(0.05, 0.05 + aa, min(y, 1.8 - y));
        c = mix(c, STENCIL_ORANGE, stripe);
        c = mix(c, STENCIL_BLACK * 1.4, max(ink, dark));
        c *= 1.0 - 0.25 * hem;
        let dust = 0.3 * smoothstep(0.9, 1.8, y) * (0.6 + 0.4 * value_noise(in.world.xz * 0.7 + vec2<f32>(x)));
        base = mix(c, ROAD_DUST * 1.3, dust);
        sheen = vec2<f32>(0.05, 12.0);
    } else if k == 28u {
        // Steel: galvanised (its zinc spangle, hammer scuffs, dust in the scratches) or rusty,
        // told by the vertex colour (the kit's rust is reddish).
        let rusty = in.color.r - in.color.b > 0.08;
        let s = surf_triplanar(select(L_GALVANIZED, L_RUST, rusty), TILE_STEEL, in.world, n, dpx, dpy, 0.6 * bump);
        n = normalize(n + s.bump);
        if rusty {
            // Rust scatters the light; its bare patches still shine a little.
            base = s.colour;
            metal = 0.12 + 0.25 * s.height;
        } else {
            // Zinc: a mid grey mirror, its spangle a patchwork of flakes that mirror more or less
            // (the texture's brightness), not a pattern painted on it.
            let spangle = clamp(lum(s.colour) / 0.27, 0.4, 1.8);
            base = vec3<f32>(0.56, 0.58, 0.61) * mix(1.0, spangle, 0.3);
            metal = 0.82 + 0.16 * smoothstep(0.6, 1.5, spangle);
        }
        // Dust settled on the cap.
        let top = smoothstep(0.75, 0.95, n.y);
        base = mix(base, ROAD_DUST * 1.2, 0.5 * top);
        metal *= 1.0 - 0.6 * top;
    } else if k == 21u {
        let s = surf_triplanar(L_EARTH, TILE_EARTH, in.world, n, dpx, dpy, bump);
        base = s.colour * clamp(lum(in.color) / lum(KIT_EARTH_FACE), 0.6, 1.4);
        n = normalize(n + s.bump);
    } else if k == 22u {
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
    // Cloth stuffed with regolith (the sandbags) lets the light wrap round it: its shaded flanks
    // stay readable instead of going black under a hard terminator.
    let cloth = k == 26u;
    let ndl = select(max(dot(n, l), 0.0), max((dot(n, l) + 0.45) / 1.45, 0.0), cloth);
    let sun = shadow_factor(in.world, n);
    let sh = select(sun, mix(sun, 1.0, 0.35), cloth);
    var hemi = mix(frame.ground_bounce.rgb, frame.sky_top.rgb, n.y * 0.5 + 0.5);
    if terrain {
        hemi *= tyre_shade(in.world);
    }
    let storm = storm_ground(in.world);
    var col = base * (frame.sun_color.rgb * ndl * sh * (1.0 - 0.9 * storm.x) + hemi) + emit;

    let to_eye = frame.camera_pos.xyz - in.world;
    let dist = length(to_eye);
    if sheen.x > 0.0 {
        let v = to_eye / max(dist, 1e-3);
        let h = normalize(l + v);
        col += frame.sun_color.rgb * pow(max(dot(n, h), 0.0), sheen.y) * sh * sheen.x;
        col += frame.sky_horizon.rgb * pow(1.0 - max(dot(n, v), 0.0), 4.0) * sheen.x * 0.3;
    }
    if metal > 0.0 {
        // Bare metal: the sky above the horizon and the darker Martian ground below it, mirrored
        // (metal reads by that contrast: a dark band under a bright one, each flat face showing a
        // different part of it), a little greyed (the zinc's own cool tint against the orange
        // world), tinted by the metal (Schlick's Fresnel toward white at grazing angles); a sharp
        // glint of the sun and a broader sheen round it.
        let v = to_eye / max(dist, 1e-3);
        let r = reflect(-v, n);
        let sky = sky_color(normalize(vec3<f32>(r.x, max(r.y, 0.02), r.z)));
        let ground = KIT_GROUND * (frame.sun_color.rgb * 0.12 + frame.sky_top.rgb * 0.3);
        let seen = mix(ground, sky, smoothstep(-0.04, 0.35, r.y));
        let env = mix(seen, vec3<f32>(lum(seen)), 0.35);
        let f = base + (vec3<f32>(1.0) - base) * pow(1.0 - max(dot(n, v), 0.0), 5.0);
        let h = normalize(l + v);
        let nh = max(dot(n, h), 0.0);
        let glint = (pow(nh, 160.0) * 4.0 + pow(nh, 24.0) * 0.35) * sh;
        col = mix(col, (env + frame.sun_color.rgb * glint) * f, metal);
    }
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
