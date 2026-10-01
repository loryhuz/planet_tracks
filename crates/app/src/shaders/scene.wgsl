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
    // x: shadow map texel size in uv
    misc: vec4<f32>,
};

struct Object {
    model: mat4x4<f32>,
    // rgb multiplies the vertex colour; a < 1 draws the object as a ghost
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(1) @binding(0) var<uniform> object: Object;

// Vertex kinds: 0 road, 1 dirt, 2 ground, 3 wall, 10 car paint, 11 rubber, 12 metal.
struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) kind: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) @interpolate(flat) kind: u32,
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
    return o;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
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

fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let t = clamp(dir.y, 0.0, 1.0);
    var col = mix(frame.sky_horizon.rgb, frame.sky_top.rgb, pow(t, 0.55));
    let s = max(dot(dir, frame.sun_dir.xyz), 0.0);
    // Martian skies turn bluish around the sun.
    col += vec3<f32>(0.45, 0.6, 0.85) * pow(s, 48.0) * 0.9;
    col += vec3<f32>(1.0, 0.97, 0.92) * smoothstep(0.99955, 0.9998, s) * 10.0;
    return col;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Derivatives first, while control flow is still uniform.
    let xz = in.world.xz;
    let g_fine = grid(xz, 8.0);
    let g_coarse = grid(xz, 32.0);
    let n_low = value_noise(xz * 0.015);
    let n_high = value_noise(xz * 0.4);

    if object.tint.a < 0.99 {
        let p = vec2<u32>(in.clip.xy);
        if ((p.x + p.y) & 1u) == 0u {
            discard;
        }
    }

    var base = in.color;
    let k = in.kind;
    if k == 2u {
        base *= 0.88 + 0.24 * n_low + 0.06 * n_high;
        base *= 1.0 - 0.14 * g_fine - 0.32 * g_coarse;
    } else if k == 1u {
        base *= 0.9 + 0.14 * n_high;
        base *= 1.0 - 0.08 * g_fine - 0.14 * g_coarse;
    } else if k == 0u {
        base *= 1.0 - 0.12 * g_fine;
    }

    let n = normalize(in.normal);
    let l = frame.sun_dir.xyz;
    let ndl = max(dot(n, l), 0.0);
    let sh = shadow_factor(in.world, n);
    let hemi = mix(frame.ground_bounce.rgb, frame.sky_top.rgb, n.y * 0.5 + 0.5);
    var col = base * (frame.sun_color.rgb * ndl * sh + hemi);

    let to_eye = frame.camera_pos.xyz - in.world;
    let dist = length(to_eye);
    if k == 10u || k == 12u {
        let v = to_eye / max(dist, 1e-3);
        let h = normalize(l + v);
        let paint = k == 10u;
        let spec = pow(max(dot(n, h), 0.0), select(40.0, 90.0, paint)) * sh;
        col += frame.sun_color.rgb * spec * select(0.35, 0.5, paint);
        let fres = pow(1.0 - max(dot(n, v), 0.0), 4.0);
        col += frame.sky_horizon.rgb * fres * 0.3;
    }

    let fog = 1.0 - exp(-max(dist - frame.fog.y, 0.0) * frame.fog.x);
    col = mix(col, frame.sky_horizon.rgb, fog);
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
