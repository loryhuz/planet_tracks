// Sandstorm on the horizon: a wall of thick dust well over a kilometre high, kilometres away,
// rolling toward the circuit and stopping short of it. Two concentric curtains (a taller one
// behind the main wall) are drawn back to front over the opaque scene, so the far hills stand
// in front of the dust; both they and the storm's base fade into the same dusty air. Each curtain is flat; the smoke on it is
// domain-warped noise that rises, drifts and boils, lit through its density gradient.

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
    fog: vec4<f32>,
    misc: vec4<f32>,
    // xy: circuit centre (x, z), zw: unit direction from the centre to the storm (x, z)
    storm_a: vec4<f32>,
    // x: distance from the centre to the front, y: time (s), z: ground height, w: approach 0..1
    storm_b: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;

const COLUMNS: u32 = 128u;
const ROWS: u32 = 10u;
// The front is an arc of this radius, bulging toward the circuit (scene.wgsl shades the ground
// under it with the same radius, height and the main wall's half angle).
const ARC_RADIUS: f32 = 7000.0;
const WALL_HEIGHT: f32 = 1400.0;
// Plumes rise this far above the envelope at most (share of its height).
const PLUMES: f32 = 0.4;
// Height scale of the ground haze, metres: the higher a point, the clearer the air in front.
const HAZE_SCALE: f32 = 260.0;
// Share of the scene's fog density the storm is seen through.
const HAZE_SHARE: f32 = 0.35;
// The storm's foot is lost in the ground haze up to this height, then rises out of it over
// FOOT_FADE, metres: the far hills in front fade into the same haze, so they cut no silhouette.
const FOOT_HEIGHT: f32 = 150.0;
const FOOT_FADE: f32 = 500.0;

struct Layer {
    // Behind the front (+) or ahead of it (-), metres.
    offset: f32,
    height: f32,
    half_angle: f32,
    // Size of the largest swirls, metres.
    scale: f32,
    alpha: f32,
    seed: f32,
    // Speed of the smoke rising and of the wind along the wall, m/s.
    rise: f32,
    wind: f32,
};

fn layer(i: u32) -> Layer {
    if i == 0u {
        return Layer(1400.0, 1.35 * WALL_HEIGHT, 0.95, 650.0, 0.9, 17.0, 21.0, 6.0);
    }
    return Layer(0.0, WALL_HEIGHT, 0.85, 400.0, 1.0, 3.0, 35.0, 9.0);
}

fn pcg2d(v_in: vec2<u32>) -> vec2<u32> {
    var v = v_in * 1664525u + 1013904223u;
    v.x += v.y * 1664525u;
    v.y += v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    v.x += v.y * 1664525u;
    v.y += v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    return v;
}

// A gradient in [-1, 1]² per lattice point.
fn gradient(p: vec2<i32>) -> vec2<f32> {
    return vec2<f32>(pcg2d(bitcast<vec2<u32>>(p)) >> vec2<u32>(8u)) / 8388608.0 - 1.0;
}

// Gradient noise, about [-0.7, 0.7]: (value, d/dx, d/dy).
fn noised(p: vec2<f32>) -> vec3<f32> {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    let ga = gradient(i);
    let gb = gradient(i + vec2<i32>(1, 0));
    let gc = gradient(i + vec2<i32>(0, 1));
    let gd = gradient(i + vec2<i32>(1, 1));
    let va = dot(ga, f);
    let vb = dot(gb, f - vec2<f32>(1.0, 0.0));
    let vc = dot(gc, f - vec2<f32>(0.0, 1.0));
    let vd = dot(gd, f - vec2<f32>(1.0, 1.0));
    let k = va - vb - vc + vd;
    let value = va + u.x * (vb - va) + u.y * (vc - va) + u.x * u.y * k;
    let deriv = ga + u.x * (gb - ga) + u.y * (gc - ga) + u.x * u.y * (ga - gb - gc + gd)
        + du * (u.yx * k + vec2<f32>(vb, vc) - va);
    return vec3<f32>(value, deriv);
}

// Turbulent smoke: each octave turns and drifts on its own, the small eddies faster than the
// large swirls, so the pattern boils instead of sliding as a whole. About [-0.6, 0.6]:
// (all octaves, the first two only).
fn smoke(p: vec2<f32>, octaves: i32, t: f32) -> vec2<f32> {
    let turn = mat2x2<f32>(0.8, -0.6, 0.6, 0.8);
    var q = p;
    var sum = 0.0;
    var low = 0.0;
    var amp = 0.5;
    var speed = 1.0;
    for (var k = 0; k < octaves; k++) {
        let a = f32(k) * 2.4;
        sum += amp * noised(q + vec2<f32>(cos(a), sin(a)) * t * speed + f32(k) * 7.31).x;
        if k == 1 {
            low = sum;
        }
        q = turn * q * 2.03;
        amp *= 0.5;
        speed *= 1.3;
    }
    return vec2<f32>(sum, select(low, sum, octaves < 2));
}

fn arc_radius(l: Layer) -> f32 {
    return ARC_RADIUS - l.offset;
}

// Height of the wall's top above the ground at arc length s.
fn envelope(s: f32, l: Layer) -> f32 {
    let u = clamp(s / (arc_radius(l) * l.half_angle), -1.0, 1.0);
    let taper = sqrt(1.0 - u * u);
    let towers = 0.85 + 0.5 * noised(vec2<f32>(s / 2600.0, l.seed)).x + 0.3 * noised(vec2<f32>(s / 900.0, l.seed + 3.7)).x;
    return l.height * (0.35 + 0.65 * taper) * towers;
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // (arc length, height above the ground), metres
    @location(1) wall: vec2<f32>,
    @location(2) u: f32,
    @location(3) tangent: vec3<f32>,
    @location(4) normal: vec3<f32>,
    @location(5) @interpolate(flat) layer: u32,
};

@vertex
fn vs_storm(@builtin(vertex_index) vi: u32, @builtin(instance_index) li: u32) -> VsOut {
    let quad = vi / 6u;
    let corner = vi % 6u;
    let cx = select(0u, 1u, corner == 1u || corner == 4u || corner == 5u);
    let cy = select(0u, 1u, corner == 2u || corner == 3u || corner == 5u);
    let col = quad % COLUMNS + cx;
    let row = quad / COLUMNS + cy;
    let u = f32(col) / f32(COLUMNS) * 2.0 - 1.0;
    let v = f32(row) / f32(ROWS);

    let l = layer(li);
    let radius = arc_radius(l);
    let theta = u * l.half_angle;
    let s = radius * theta;
    // Up to the highest plume; below the ground, so no gap shows under the far hills.
    let y = mix(-500.0, envelope(s, l) * (1.0 + PLUMES), v);

    let d = frame.storm_a.zw;
    let e = vec2<f32>(-d.y, d.x);
    let centre = frame.storm_a.xy + d * (frame.storm_b.x + ARC_RADIUS);
    let out_dir = -d * cos(theta) + e * sin(theta);
    let xz = centre + out_dir * radius;
    let world = vec3<f32>(xz.x, frame.storm_b.z + y, xz.y);
    let t = d * sin(theta) + e * cos(theta);

    var o: VsOut;
    o.clip = frame.view_proj * vec4<f32>(world, 1.0);
    o.world = world;
    o.wall = vec2<f32>(s, y);
    o.u = u;
    o.tangent = vec3<f32>(t.x, 0.0, t.y);
    o.normal = vec3<f32>(out_dir.x, 0.0, out_dir.y);
    o.layer = li;
    return o;
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

// Same as in scene.wgsl: dust hanging in the air toward the storm, low over the horizon.
fn storm_dust(dir: vec3<f32>) -> f32 {
    let h = dir.xz / max(length(dir.xz), 1e-4);
    let toward = smoothstep(0.2, 0.95, dot(h, frame.storm_a.zw));
    let low = 1.0 - smoothstep(-0.05, 0.4, dir.y);
    return toward * low * (0.45 + 0.45 * frame.storm_b.w);
}

const DUST_AIR: vec3<f32> = vec3<f32>(0.37, 0.11, 0.04);

@fragment
fn fs_storm(in: VsOut) -> @location(0) vec4<f32> {
    let l = layer(in.layer);
    let time = frame.storm_b.y;
    let p = in.wall;
    let top = envelope(p.x, l);

    // The smoke field scrolls down past the curtain (so the smoke rises) and along it with the
    // wind, through a domain warp that itself churns: swirls form, roll and tear apart.
    // Swirls are wider than tall, as the front rolls along.
    let q = (p + vec2<f32>(-l.wind, -l.rise) * time) / (l.scale * vec2<f32>(1.4, 1.0)) + l.seed;
    let warp = vec2<f32>(smoke(q + vec2<f32>(0.0, 3.1), 3, time * 0.04).x, smoke(q + vec2<f32>(5.2, 1.3), 3, time * 0.04).x);
    let n = smoke(q + 1.1 * warp + vec2<f32>(1.7, 9.2), 5, time * 0.06);

    // Dense along the ground, thinning toward the envelope, torn into plumes above it. The
    // large swirls alone (`billows`) give the light its round masses; the small eddies only
    // fray the edges and darken the hollows.
    let below = (top - p.y) / top;
    let density = clamp(below * 1.6 + n.x * 1.5 - 0.05, 0.0, 1.0);
    // Not clamped: the light keeps its relief deep inside the dense body too.
    let billows = below * 1.6 + n.y * 1.5 - 0.05;

    // Gradient of the billows along the curtain, from screen-space derivatives (per metre).
    let px = dpdxFine(p);
    let py = dpdyFine(p);
    let dx = dpdxFine(billows);
    let dy = dpdyFine(billows);
    let det = px.x * py.y - px.y * py.x;
    let grad = select(vec2<f32>(dx * py.y - dy * px.y, dy * px.x - dx * py.x) / det, vec2<f32>(0.0), abs(det) < 1e-6);

    let fade = 1.0 - smoothstep(0.72, 1.0, abs(in.u));
    let alpha = smoothstep(0.0, 0.45, density) * l.alpha * fade;

    // Lighting: smoke facing the sun (its density falls off toward the sun) is lit; inside and
    // low down it is in its own shadow; thin wisps glow when the sun is behind them.
    let sun = frame.sun_dir.xyz;
    let sun_along = vec2<f32>(dot(sun, in.tangent), sun.y);
    let facing = clamp(0.25 - dot(grad * l.scale, sun_along) * 0.5, 0.0, 1.0);
    let front = mix(0.2, 1.0, clamp(dot(in.normal, sun) * 0.5 + 0.5, 0.0, 1.0));
    let deep = mix(0.22, 1.0, exp(-max(below, 0.0) * 2.0));
    let hollow = mix(0.55, 1.0, clamp(1.0 + (density - clamp(billows, 0.0, 1.0)) * 3.0, 0.0, 1.0));
    let lit = facing * front * deep * hollow;

    let to_point = in.world - frame.camera_pos.xyz;
    let dist = length(to_point);
    let view = to_point / dist;
    let glow = (1.0 - density) * pow(max(dot(view, sun), 0.0), 5.0) * 1.6;

    let albedo = vec3<f32>(0.76, 0.24, 0.055);
    let ambient = mix(frame.ground_bounce.rgb, frame.sky_top.rgb, 0.6) * 0.45;
    var col = albedo * (frame.sun_color.rgb * (lit + glow) + ambient * hollow);

    // Haze: the storm is dense enough to show through the distance fog the hills fade into;
    // thinner still for rays that climb toward the plumes. Its foot, though, sinks into the
    // same ground haze as the hills in front of it, so they meet without a seam.
    let rise_m = max(in.world.y - frame.camera_pos.y, 0.0) / HAZE_SCALE;
    let thinning = select((1.0 - exp(-rise_m)) / rise_m, 1.0, rise_m < 1e-3);
    let path = max(dist - frame.fog.y, 0.0) * frame.fog.x;
    let haze = 1.0 - exp(-path * HAZE_SHARE * thinning);
    let foot = 1.0 - smoothstep(FOOT_HEIGHT, FOOT_HEIGHT + FOOT_FADE, in.world.y - frame.storm_b.z);
    let air = mix(frame.sky_horizon.rgb, DUST_AIR, storm_dust(view));
    col = mix(col, air, max(haze, foot));

    let a = clamp(alpha, 0.0, 1.0);
    return vec4<f32>(to_srgb(tonemap(col)) * a, a);
}
