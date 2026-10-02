// The speed blur while a booster pushes the car (blur.rs): every pixel averages the scene along
// the line toward the point the road runs to, the more the farther it is from it, so the road
// ahead stays sharp and the sides streak. The scene's image is already tonemapped and
// sRGB-encoded (scene.wgsl), so this pass only copies it, smeared.

struct Blur {
    // The point the road runs to (0..1 across and down the screen), how strong the blur is
    // (0..1), and the screen's width over its height.
    focus: vec2<f32>,
    strength: f32,
    aspect: f32,
};

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> blur: Blur;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blur(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

// Samples along each pixel's line, and the share of its distance to the focus they span at full
// strength, at the screen's edges.
const TAPS: i32 = 16;
const REACH: f32 = 0.07;

@fragment
fn fs_blur(in: Out) -> @location(0) vec4<f32> {
    let d = in.uv - blur.focus;
    let r = length(d * vec2<f32>(blur.aspect, 1.0));
    let reach = REACH * blur.strength * smoothstep(0.12, 0.75, r);
    // A different start along the line for neighbouring pixels hides the steps between taps.
    let jitter = fract(52.9829189 * fract(dot(in.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    var acc = vec3<f32>(0.0);
    for (var i = 0; i < TAPS; i++) {
        let t = (f32(i) + jitter) / f32(TAPS);
        acc += textureSampleLevel(scene, scene_sampler, in.uv - d * (reach * t), 0.0).rgb;
    }
    return vec4<f32>(acc / f32(TAPS), 1.0);
}
