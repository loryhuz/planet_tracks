// Soft camera-facing dust puffs, lit like Martian dust in the sun (or under the moon).

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

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_dust(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) alpha: f32) -> VsOut {
    var o: VsOut;
    o.clip = frame.view_proj * vec4<f32>(pos, 1.0);
    o.uv = uv;
    o.alpha = alpha;
    return o;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
}

@fragment
fn fs_dust(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    let soft = 1.0 - smoothstep(0.2, 1.0, d);
    let a = clamp(in.alpha * soft, 0.0, 1.0);
    // Martian dust, or snow spray on the ice planet (fog.z = 1); by night (misc.w) only lit by
    // the moon: dim, greyed toward blue.
    let day = select(vec3<f32>(0.85, 0.58, 0.40), vec3<f32>(0.9, 0.94, 1.0), frame.fog.z > 0.5);
    let color = to_srgb(mix(day, vec3<f32>(0.085, 0.075, 0.1), frame.misc.w));
    return vec4<f32>(color * a, a);
}
