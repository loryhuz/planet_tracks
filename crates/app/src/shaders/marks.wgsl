// Tyre marks: alpha-blended strips just above the ground. Sharp and dark while the tyres
// still roll along their heading, wider and blurred once they slide sideways.

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
};

@group(0) @binding(0) var<uniform> frame: Frame;

struct VsIn {
    @location(0) pos: vec3<f32>,
    // -1 .. 1 across the mark
    @location(1) across: f32,
    @location(2) alpha: f32,
    // 0 sharp .. 1 blurred
    @location(3) blur: f32,
    // track surface (0 road, 1 dirt, 2 ground)
    @location(4) surface: u32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) across: f32,
    @location(2) alpha: f32,
    @location(3) blur: f32,
    @location(4) @interpolate(flat) surface: u32,
};

@vertex
fn vs_marks(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = frame.view_proj * vec4<f32>(v.pos, 1.0);
    o.world = v.pos;
    o.across = v.across;
    o.alpha = v.alpha;
    o.blur = v.blur;
    o.surface = v.surface;
    return o;
}

fn hash1(x: f32) -> f32 {
    return fract(sin(x * 91.3458) * 47453.5453);
}

@fragment
fn fs_marks(in: VsOut) -> @location(0) vec4<f32> {
    let a = abs(in.across);
    // Sharp marks have crisp edges; blurred ones fade over most of their width.
    let soft = mix(0.12, 0.85, in.blur);
    let edge = 1.0 - smoothstep(1.0 - soft, 1.0, a);
    // Blurred marks get lengthwise streaks (several ribs of rubber or dirt sliding).
    let streak = mix(1.0, 0.55 + 0.45 * hash1(floor(in.across * 7.0)), in.blur);
    let dist = length(frame.camera_pos.xyz - in.world);
    let fade = 1.0 - smoothstep(120.0, 220.0, dist);
    var color = vec3<f32>(0.015, 0.014, 0.013);
    var strength = 0.75;
    if in.surface == 1u || in.surface == 2u {
        // Ruts in Martian dirt: darker, redder.
        color = vec3<f32>(0.10, 0.035, 0.015);
        strength = 0.6;
    }
    let alpha = clamp(in.alpha * edge * streak * strength * fade, 0.0, 1.0);
    // Premultiplied; colours are written as sRGB-ish directly (marks are near-black).
    return vec4<f32>(color * alpha, alpha);
}
