// Weather on the circuit (weather.rs). Fine sand drifts on the wind through a box of air that
// follows the camera (the grains stay put in the world: the box wraps around them as the camera
// moves), and gusts sweep low clouds of sand across the road. Grains are short streaks along
// their motion relative to the camera, so they rush past at speed; the clouds are camera-facing
// puffs of animated smoke that hug the road, carry their own stream of grains, and thin out close
// to the camera: driving through one only veils the view for a moment (`fs_veil`).

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
    storm_a: vec4<f32>,
    storm_b: vec4<f32>,
    far_light_view_proj: mat4x4<f32>,
    // xy: wind direction (x, z), z: wind speed (m/s), w: time (s)
    wind: vec4<f32>,
    // xy: the air's drift (x, z, metres, wrapped), z: share of the grains shown, w: veil (0..1)
    drift: vec4<f32>,
    // xyz: the camera's velocity (m/s)
    eye_vel: vec4<f32>,
    // xy: viewport (pixels), z: pixels per metre at a metre's depth
    viewport: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
// The lattice of the smoke's value noise (weather.rs's `noise_lattice`): a random value per
// texel, repeating.
@group(0) @binding(10) var noise_tex: texture_2d<f32>;
@group(0) @binding(11) var noise_sampler: sampler;
const NOISE_SIDE: f32 = 256.0;

// Grains drifting around the camera, in a box this big (x, y, z, metres) that starts this far
// below the camera.
const GRAINS: u32 = 2400u;
const BOX: vec3<f32> = vec3<f32>(56.0, 14.0, 56.0);
const BOX_BELOW: f32 = 4.0;
// Puffs and grains of each gust.
const PUFFS: u32 = 28u;
// A puff's width over its height.
const PUFF_WIDE: f32 = 2.0;
// How far out from its centre a puff can show (of its radius): past it fs_puff's density is
// zero whatever the noise (body * 1.6 <= 0.2), so its billboard stops there.
const PUFF_REACH: f32 = 0.78;
const GUST_GRAINS: u32 = 260u;
// Exposure of the streaks (s), and their longest on screen (pixels).
const SHUTTER: f32 = 0.012;
const MAX_STREAK: f32 = 40.0;
// Narrowest streak drawn, pixels (thinner ones fade instead).
const MIN_PX: f32 = 1.4;
// Albedo of the airborne sand (linear).
const SAND: vec3<f32> = vec3<f32>(0.48, 0.28, 0.16);
const DUST_AIR: vec3<f32> = vec3<f32>(0.37, 0.11, 0.04);
// By night (frame.misc.w), as in scene.wgsl.
const DUST_AIR_NIGHT: vec3<f32> = vec3<f32>(0.05, 0.028, 0.035);

fn dust_air() -> vec3<f32> {
    return mix(DUST_AIR, DUST_AIR_NIGHT, frame.misc.w);
}

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

// Four random numbers in [0, 1) for an index.
fn rand4(i: u32) -> vec4<f32> {
    let a = pcg(i);
    let b = pcg(a ^ 0x9e3779b9u);
    let c = pcg(b ^ 0x85ebca6bu);
    let d = pcg(c ^ 0xc2b2ae35u);
    return vec4<f32>(vec4<u32>(a, b, c, d) >> vec4<u32>(8u)) / 16777216.0;
}

// The lattice's four values around `p` blended with smoothstep weights, in one filtered fetch:
// it lands between their texels at the smoothed offset.
fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return textureSampleLevel(noise_tex, noise_sampler, (i + u + 0.5) / NOISE_SIDE, 0.0).r;
}

// Three octaves, each turning and drifting on its own so the smoke boils (each drifts faster,
// its direction turned 2.4 rad from the one before): about [0, 1].
fn fbm(p: vec2<f32>, t: f32) -> f32 {
    let turn = mat2x2<f32>(0.8, -0.6, 0.6, 0.8);
    let q1 = turn * p * 2.07;
    let q2 = turn * q1 * 2.07;
    let sum = 0.5 * value_noise(p + vec2<f32>(1.0, 0.0) * t)
        + 0.25 * value_noise(q1 + vec2<f32>(-0.7373937, 0.6754632) * t * 1.5 + 7.31)
        + 0.125 * value_noise(q2 + vec2<f32>(0.0874990, -0.9961646) * t * 2.0 + 14.62);
    return sum / 0.875;
}

// ACES filmic fit (Narkowicz), as in scene.wgsl.
fn tonemap(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn ambient() -> vec3<f32> {
    return mix(frame.ground_bounce.rgb, frame.sky_top.rgb, 0.6) * 0.5;
}

// Dust in the sun: lit, and glowing when the sun is behind it (`view` from the camera).
fn sand_light(view: vec3<f32>, lit: f32, thin: f32) -> vec3<f32> {
    let toward = max(dot(view, frame.sun_dir.xyz), 0.0);
    let glow = toward * toward * toward * toward * toward * 1.4 * thin;
    return SAND * (frame.sun_color.rgb * (lit + glow) + ambient());
}

// As scene.wgsl's distance fog, toward the storm's dusty air.
fn fogged(col: vec3<f32>, to_point: vec3<f32>) -> vec3<f32> {
    let dist = length(to_point);
    // Nearer than the fog starts, as most of the smoke on screen is: clear.
    if dist <= frame.fog.y {
        return col;
    }
    let dir = to_point / max(dist, 1e-3);
    let h = dir.xz / max(length(dir.xz), 1e-4);
    let toward = smoothstep(0.2, 0.95, dot(h, frame.storm_a.zw));
    let low = 1.0 - smoothstep(-0.05, 0.4, dir.y);
    let dust = toward * low * (0.45 + 0.45 * frame.storm_b.w);
    let fog = 1.0 - exp(-max(dist - frame.fog.y, 0.0) * frame.fog.x);
    return mix(col, mix(frame.sky_horizon.rgb, dust_air(), dust), fog);
}

// Corner `c` (0..5) of a quad of two triangles: x across (-1, 1), y along (0, 1).
fn corner(c: u32) -> vec2<f32> {
    let x = select(-1.0, 1.0, c == 1u || c == 2u || c == 4u);
    let y = select(0.0, 1.0, c == 2u || c == 4u || c == 5u);
    return vec2<f32>(x, y);
}

struct GrainOut {
    @builtin(position) clip: vec4<f32>,
    // x across the streak (-1, 1), y along it (0 tail, 1 head)
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) alpha: f32,
};

fn hidden_grain() -> GrainOut {
    var o: GrainOut;
    o.clip = vec4<f32>(-2.0, -2.0, 0.5, 1.0);
    o.uv = vec2<f32>(0.0);
    o.color = vec3<f32>(0.0);
    o.alpha = 0.0;
    return o;
}

// A grain of `size` metres at `pos` moving at `vel`: a streak from where it was a shutter ago
// (relative to the camera) to where it is, at least MIN_PX wide.
fn streak(pos: vec3<f32>, vel: vec3<f32>, size: f32, alpha_in: f32, c: u32) -> GrainOut {
    let head = frame.view_proj * vec4<f32>(pos, 1.0);
    var tail = frame.view_proj * vec4<f32>(pos - (vel - frame.eye_vel.xyz) * SHUTTER, 1.0);
    if alpha_in < 0.004 || head.w < 0.2 || tail.w < 0.2 {
        return hidden_grain();
    }
    let half = frame.viewport.xy * 0.5;
    let hp = head.xy / head.w * half;
    var tp = tail.xy / tail.w * half;
    let full = length(hp - tp);
    if full > MAX_STREAK {
        let keep = MAX_STREAK / full;
        tp = mix(hp, tp, keep);
        tail = vec4<f32>(tail.xy, mix(head.zw, tail.zw, keep));
    }
    let d = hp - tp;
    let dl = length(d);
    let dir = select(vec2<f32>(1.0, 0.0), d / dl, dl > 1e-3);
    let nrm = vec2<f32>(-dir.y, dir.x);
    let px = size * frame.viewport.z / head.w;
    let w = max(px, MIN_PX);
    // Thinner than a pixel: fainter instead; long streaks spread the grain's light.
    let alpha = alpha_in * clamp(px / MIN_PX, 0.0, 1.0) * sqrt(w / (w + dl));

    let k = corner(c);
    let at = mix(tp, hp, k.y) + dir * (k.y * 2.0 - 1.0) * w * 0.5 + nrm * k.x * w * 0.5;
    let zw = mix(tail.zw, head.zw, k.y);
    var o: GrainOut;
    o.clip = vec4<f32>(at / half * zw.y, zw.x, zw.y);
    o.uv = k;
    let view = normalize(pos - frame.camera_pos.xyz);
    o.color = to_srgb(tonemap(sand_light(view, 0.8, 1.0)));
    o.alpha = alpha;
    return o;
}

@vertex
fn vs_grain(@builtin(vertex_index) vi: u32) -> GrainOut {
    let id = vi / 6u;
    if f32(id) >= f32(GRAINS) * frame.drift.z {
        return hidden_grain();
    }
    let r = rand4(id * 2u + 1u);
    let s = rand4(id * 2u + 2u);
    let t = frame.wind.w;
    let wind = vec3<f32>(frame.wind.x, 0.0, frame.wind.y);
    // Each grain rides its own share of the wind and wanders in small loops.
    let share = 0.75 + 0.5 * s.x;
    let phase = s.y * 6.2832;
    let f = vec3<f32>(0.7 + 0.6 * s.z, 1.1 + 0.8 * s.w, 0.9 + 0.5 * s.z);
    let wander = vec3<f32>(sin(t * f.x + phase), 0.6 * sin(t * f.y + phase * 1.7), cos(t * f.z + phase * 0.6)) * 0.5;
    let wander_vel = vec3<f32>(f.x * cos(t * f.x + phase), 0.6 * f.y * cos(t * f.y + phase * 1.7), -f.z * sin(t * f.z + phase * 0.6)) * 0.5;

    // The grain's place in a lattice of boxes that the wind carries along; the one around the
    // camera is drawn.
    let origin = frame.camera_pos.xyz - vec3<f32>(BOX.x * 0.5, BOX_BELOW, BOX.z * 0.5);
    let drifted = r.xyz * BOX + vec3<f32>(frame.drift.x, 0.0, frame.drift.y) * share + wander;
    let rel = drifted - origin;
    let inside = rel - floor(rel / BOX) * BOX;
    let pos = origin + inside;

    // Faded toward the box's sides (so the wrap never shows), high up, and right at the camera.
    let off = pos - frame.camera_pos.xyz;
    let edge = 1.0 - smoothstep(0.3 * BOX.x, 0.46 * BOX.x, length(off.xz));
    let ends = smoothstep(0.0, 1.5, inside.y) * (1.0 - smoothstep(BOX.y - 4.0, BOX.y, inside.y));
    let near = smoothstep(0.8, 2.5, length(off));
    let alpha = (0.35 + 0.4 * r.w) * edge * ends * near;
    let size = 0.02 + 0.035 * s.w * s.w;
    return streak(pos, wind * frame.wind.z * share + wander_vel, size, alpha, vi % 6u);
}

@fragment
fn fs_grain(in: GrainOut) -> @location(0) vec4<f32> {
    let across = 1.0 - smoothstep(0.25, 1.0, abs(in.uv.x));
    let a = clamp(in.alpha * across * mix(0.45, 1.0, in.uv.y), 0.0, 1.0);
    return vec4<f32>(in.color * a, a);
}

struct GustIn {
    // xyz: where its centre crosses the road (on it), w: seed
    @location(0) origin: vec4<f32>,
    // xy: direction it travels (x, z), z: speed (m/s), w: its age when it crosses (s)
    @location(1) travel: vec4<f32>,
    // x: age (s), y: life (s), z: length along its travel, w: width across it (m)
    @location(2) time: vec4<f32>,
    // x: height (m), y: strength (0..1)
    @location(3) shape: vec4<f32>,
};

struct Gust {
    centre: vec3<f32>,
    dir: vec3<f32>,
    side: vec3<f32>,
    ground: f32,
    age: f32,
    speed: f32,
    length: f32,
    width: f32,
    height: f32,
    seed: u32,
    // How much of it there is: it builds up, then dies away.
    presence: f32,
};

fn gust(g: GustIn) -> Gust {
    var o: Gust;
    o.dir = vec3<f32>(g.travel.x, 0.0, g.travel.y);
    o.side = vec3<f32>(-g.travel.y, 0.0, g.travel.x);
    o.age = g.time.x;
    o.speed = g.travel.z;
    o.centre = g.origin.xyz + o.dir * o.speed * (o.age - g.travel.w);
    o.ground = g.origin.y;
    o.length = g.time.z;
    o.width = g.time.w;
    o.height = g.shape.x;
    o.seed = u32(g.origin.w);
    let life = g.time.y;
    o.presence = smoothstep(0.0, 1.0, o.age) * (1.0 - smoothstep(life - 1.8, life, o.age)) * g.shape.y;
    return o;
}

// The gust's own sand: grains streaming through the cloud faster than it moves, from its tail
// to its front, most of them low over the ground.
@vertex
fn vs_gust_grain(@builtin(vertex_index) vi: u32, g_in: GustIn) -> GrainOut {
    let g = gust(g_in);
    let i = vi / 6u;
    let r = rand4(g.seed * 7919u + i * 2u + 100003u);
    let s = rand4(g.seed * 7919u + i * 2u + 100004u);
    let stream = g.speed * (0.3 + 0.4 * s.x);
    let u = fract(r.x + g.age * stream / g.length);
    let along = (u - 0.65) * g.length;
    let across = (r.y - 0.5) * g.width * 0.85;
    let h = 0.05 + r.z * r.z * g.height * 0.8;
    let bob = 0.3 * sin(g.age * (2.0 + 2.0 * s.z) + s.w * 6.2832);
    let pos = g.centre + g.dir * along + g.side * across + vec3<f32>(0.0, h + bob * r.z, 0.0);
    let vel = g.dir * (g.speed + stream);
    let ends = sin(3.14159 * u);
    let near = smoothstep(0.8, 2.5, distance(pos, frame.camera_pos.xyz));
    let alpha = g.presence * ends * near * (0.4 + 0.4 * s.y);
    return streak(pos, vel, 0.025 + 0.03 * s.z, alpha, vi % 6u);
}

struct PuffOut {
    @builtin(position) clip: vec4<f32>,
    // On the billboard: x right, y up (-1, 1).
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    // The wind's direction on the billboard (right, up).
    @location(2) flow: vec2<f32>,
    // x: seed, y: opacity, z: the road's height under the gust, w: front (0 tail, 1 front)
    @location(3) @interpolate(flat) params: vec4<f32>,
};

// The gust's cloud: flat puffs strewn along it, wider than tall, thick and high at the front, low
// and thin at the tail, which lags behind and lifts as the cloud stretches out.
@vertex
fn vs_puff(@builtin(vertex_index) vi: u32, g_in: GustIn) -> PuffOut {
    let g = gust(g_in);
    let i = vi / 6u;
    let r = rand4(g.seed * 977u + i);
    let front = sqrt(r.x);
    let along = (front - 0.65) * g.length - (1.0 - front) * g.age * 1.5;
    let across = (r.y - 0.5) * g.width * mix(0.6, 1.0, front);
    // Half height; the half width is PUFF_WIDE times more.
    let radius = g.height * mix(0.35, 0.6, front) * (0.75 + 0.5 * r.z) * (1.0 + 0.1 * g.age);
    let lift = radius * 0.7 + (1.0 - front) * g.age * 0.2;
    let centre = g.centre + g.dir * along + g.side * across + vec3<f32>(0.0, lift, 0.0);

    let to_cam = frame.camera_pos.xyz - centre;
    let dist = length(to_cam);
    let fwd = to_cam / max(dist, 1e-3);
    let flat_right = cross(vec3<f32>(0.0, 1.0, 0.0), fwd);
    let right = select(vec3<f32>(1.0, 0.0, 0.0), normalize(flat_right), length(flat_right) > 1e-3);
    let up = cross(fwd, right);

    // Thinning out within a few metres of the camera.
    let near = smoothstep(radius, radius + 7.0, dist);
    let opacity = g.presence * near * (0.42 + 0.25 * r.w) * mix(0.6, 1.0, front);

    var o: PuffOut;
    let k = corner(vi % 6u);
    let c = vec2<f32>(k.x, k.y * 2.0 - 1.0) * PUFF_REACH;
    let world = centre + (right * c.x * PUFF_WIDE + up * c.y) * radius;
    o.clip = select(vec4<f32>(-2.0, -2.0, 0.5, 1.0), frame.view_proj * vec4<f32>(world, 1.0), opacity > 0.004);
    o.uv = c;
    o.world = world;
    o.flow = vec2<f32>(dot(g.dir, right), dot(g.dir, up));
    o.params = vec4<f32>(f32(g.seed % 256u) * 0.37 + f32(i) * 1.73, opacity, g.ground, front);
    return o;
}

@fragment
fn fs_puff(in: PuffOut) -> @location(0) vec4<f32> {
    let e = length(in.uv);
    // Soft where it meets the road (it would cut a hard line across the billboard).
    let ground = smoothstep(in.params.z - 0.1, in.params.z + 0.7, in.world.y);
    if e >= PUFF_REACH || ground <= 0.0 {
        return vec4<f32>(0.0);
    }
    let t = frame.wind.w;
    // Smoke drawn out along the wind into streaks that slide downwind across the puff (when the
    // wind crosses the view) through a churning warp: it streams and boils.
    let q = vec2<f32>(in.uv.x * PUFF_WIDE * 1.3 - in.flow.x * t * 1.6, in.uv.y * 3.0 - t * 0.25) + in.params.x;
    let warp = vec2<f32>(value_noise(q * 0.9 + vec2<f32>(0.0, t * 0.5)), value_noise(q * 0.9 + vec2<f32>(4.1, -t * 0.6))) - 0.5;
    let n = fbm(q + warp * vec2<f32>(1.8, 0.7), t * 0.7);
    // Billows with crisp edges that fray into wisps, holes opening in the body; thicker low down.
    let body = 1.0 - smoothstep(0.0, 1.0, e);
    let density = smoothstep(0.2, 0.7, body * (0.3 + 1.3 * n));
    let top = in.uv.y * 0.5 + 0.5;
    let a = clamp(in.params.y * density * mix(1.0, 0.6, top) * ground, 0.0, 1.0);

    // Lit from above, darker in its thick lower body and in the hollows between billows.
    let lit = mix(0.45, 0.95, top) * mix(0.6, 1.1, n);
    let to_point = in.world - frame.camera_pos.xyz;
    let col = fogged(sand_light(normalize(to_point), lit, 0.4 * (1.0 - density)), to_point);
    return vec4<f32>(to_srgb(tonemap(col)) * a, a);
}

struct VeilOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_veil(@builtin(vertex_index) i: u32) -> VeilOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    let p = uv * 2.0 - 1.0;
    var o: VeilOut;
    // Reverse-Z: in front of everything.
    o.clip = vec4<f32>(p, 1.0, 1.0);
    o.ndc = p;
    return o;
}

// Inside a gust: a thin drifting haze of sand over the whole view.
@fragment
fn fs_veil(in: VeilOut) -> @location(0) vec4<f32> {
    let t = frame.wind.w;
    let aspect = frame.viewport.x / max(frame.viewport.y, 1.0);
    let q = in.ndc * vec2<f32>(aspect, 1.0) * 1.6 + vec2<f32>(t * 0.9, t * 0.15);
    let n = fbm(q, t * 0.6);
    let a = clamp(frame.drift.w * (0.55 + 0.6 * n), 0.0, 1.0);
    let near = frame.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let view = normalize(near.xyz / near.w - frame.camera_pos.xyz);
    let col = to_srgb(tonemap(sand_light(view, 0.55, 0.5)));
    return vec4<f32>(col * a, a);
}
