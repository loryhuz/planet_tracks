//! GPU setup and the scene renderer (wgpu, Metal on Apple platforms).

use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::marks::{CAPACITY as MARKS_CAPACITY, MarkVertex};
use crate::particles::{CAPACITY as DUST_CAPACITY, ParticleVertex};
use crate::surfaces::SurfaceTextures;
use crate::weather::{Climate, GustInstance, MAX_GUSTS, NOISE_SIDE, Weather};
use track::map::TimeOfDay;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const MSAA: u32 = 4;
const SHADOW_SIZE: u32 = 2048;
/// Half-width of the square the near shadow map covers, metres.
const SHADOW_EXTENT: f32 = 70.0;
/// The near map's centre stands this far ahead of the camera: it covers the ground the eye sees
/// closest (the car included), not the road behind it.
const SHADOW_AHEAD: f32 = 40.0;
/// The far shadow map, baked once per track over the whole circuit (`bake_shadows`): its side in
/// texels, and how far around the route it reaches, metres.
const FAR_SHADOW_SIZE: u32 = 4096;
const FAR_SHADOW_MARGIN: f32 = 250.0;
/// The headlights' shadow map (by night): its side in texels, and the beam it covers seen from
/// the lamps (tangents of the angles: to either side, above and below the beam's axis; the beam
/// itself is shaped in scene.wgsl, inside these), from `LAMP_NEAR` to `LAMP_REACH` metres.
const LAMP_SHADOW_SIZE: u32 = 1024;
const LAMP_FRUSTUM: (f32, f32, f32) = (0.92, 0.16, 0.9);
const LAMP_NEAR: f32 = 0.4;
/// How far the headlights light, metres (fading out from 40 % of it on, see scene.wgsl).
const LAMP_REACH: f32 = 150.0;
/// The beam's axis dips this far under the car's heading (radians), as low beams do.
const LAMP_DIP: f32 = 0.025;
const OBJECT_STRIDE: u64 = 256;
const MAX_OBJECTS: u64 = 128;
/// The sandstorm's front starts this far beyond the circuit and closes in to `STORM_STOP`, metres:
/// close enough to tower over the scenery from the start, never into the circuit's air
/// (scene.wgsl thickens the dust over the last 900 m before the wall).
const STORM_START: f32 = 2400.0;
const STORM_STOP: f32 = 1100.0;
/// Time constant of the approach, seconds (it covers 63 % of the way in that time).
const STORM_APPROACH: f32 = 200.0;
/// The storm stands ahead of the start, turned this far away from the sun (degrees), so the sun
/// hangs beside it rather than in front of it.
const STORM_SUN_OFFSET: f32 = 30.0;
/// storm.wgsl draws COLUMNS × ROWS quads per curtain, two curtains.
const STORM_VERTICES: u32 = 128 * 10 * 6;
const STORM_CURTAINS: u32 = 2;
/// weather.wgsl's grains around the camera, and each gust's puffs and grains (6 vertices each).
const WEATHER_GRAIN_VERTICES: u32 = 2400 * 6;
const PUFF_VERTICES: u32 = 28 * 6;
const GUST_GRAIN_VERTICES: u32 = 260 * 6;

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

/// The window's drawable size, physical pixels. On iOS the whole screen: winit's inner size there
/// is the safe area (without the notch and the home indicator), and its `Resized` events count
/// the screen's native pixels rather than the view's scale.
pub fn window_pixels(window: &Window) -> winit::dpi::PhysicalSize<u32> {
    if cfg!(target_os = "ios") { window.outer_size() } else { window.inner_size() }
}

impl Gpu {
    pub fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("device"),
            // What the GPU offers rather than wgpu's defaults: the iOS simulator falls short of
            // them (15 inter-stage shader variables, not 16).
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("no GPU device");

        let size = window_pixels(&window);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface not supported");
        // egui wants a gamma (non-sRGB) target; the scene shader encodes sRGB itself.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = f;
        }
        config.view_formats = vec![];
        // Screenshots (debug mode) copy the frame out of the surface.
        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&device, &config);
        Self { surface, device, queue, config }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub kind: u32,
    /// Track coordinates (metres along the route, across it) on the track's ground.
    pub uv: [f32; 2],
    /// Worked earth on the track's ground: 0 natural, ½ dug banks, 1 driven dirt.
    pub dirt: f32,
}

impl Vertex {
    /// A vertex of anything but the track's ground.
    pub fn plain(pos: [f32; 3], normal: [f32; 3], color: [f32; 3], kind: u32) -> Self {
        Self { pos, normal, color, kind, uv: [0.0, 0.0], dirt: 0.0 }
    }
}

const VERTEX_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Uint32, 4 => Float32x2, 5 => Float32
];
const SHADOW_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
const DUST_ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32];
const GUST_ATTRS: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
const MARK_ATTRS: [wgpu::VertexAttribute; 5] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32, 3 => Float32, 4 => Uint32];

/// Vertex kinds understood by the shader.
pub mod kind {
    pub const PAINT: u32 = 10;
    pub const RUBBER: u32 = 11;
    pub const METAL: u32 = 12;
    /// Tinted glass, drawn opaque with strong reflections.
    pub const GLASS: u32 = 13;
    /// Lights: their colour, unlit.
    pub const GLOW: u32 = 14;
    /// Woven wire tyre: a lattice drawn from the vertex uv (metres).
    pub const WIRE: u32 = 15;
    /// Painted body whose colour comes from the livery texture at the vertex uv.
    pub const LIVERY: u32 = 16;
    /// Concrete: the sides of dirt mounds; the vertex colour (over the kit's light barrier
    /// colour) darkens it.
    pub const CONCRETE: u32 = 20;
    /// Dug earth: the faces of dirt kickers and landings.
    pub const EARTH: u32 = 21;
    /// Rock: the scenery's boulders, slabs and spires.
    pub const ROCK: u32 = 22;
    /// Laminated tarp tinted by the vertex colour (over the kit's slab colour): the sides, ends
    /// and underside of raised slabs.
    pub const TARP: u32 = 23;
    /// Glossy plastic in the vertex colour: the stilts' tubes (clamps painted at their ends from
    /// the vertex uv, see track's stilts.rs) and base plates.
    pub const PLASTIC: u32 = 24;
    /// Inflatable bumpers along the edges of roads: red and white tarp tubes strapped down.
    pub const BUMPER: u32 = 25;
    /// Sandbags: the rows along roads (bags laid out from the vertex uv, see track's kit.rs)
    /// and the stacks under the stilts.
    pub const SANDBAG: u32 = 26;
    /// Orange ratchet straps (webbing along the vertex uv: metres along the strap).
    pub const STRAP: u32 = 27;
    /// Steel of the stakes and buckles, textured and mirroring like bare metal: galvanised, or
    /// rusty when the vertex colour is reddish.
    pub const STEEL: u32 = 28;
    /// The gates' banner: white fabric lettered from the signs texture, by the vertex uv (metres
    /// to the viewer's right of its middle, metres down from its top; see track's gates.rs).
    pub const BANNER: u32 = 29;
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Ranges of `indices` and the shader that draws each; empty: the whole mesh is
    /// `Shading::Other`.
    pub parts: Vec<(Range<u32>, Shading)>,
}

/// Which of the scene's fragment shaders draws a part of a mesh (see `scene.wgsl`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shading {
    /// Walls, rocks, cars: any kind.
    #[default]
    Other,
    /// The track's ground (kind 2) only.
    Ground,
    /// The track's roads (kind 0) only.
    Road,
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    parts: Vec<(Range<u32>, Shading)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshId(usize);

pub struct DrawItem {
    pub mesh: MeshId,
    pub model: Mat4,
    pub tint: Vec4,
    pub cast_shadow: bool,
    /// A wheel near the ground: its tyre flattens on it (scene.wgsl's `squash_tyre`) and shades
    /// it (`tyre_shade`).
    pub tyre: Option<Tyre>,
}

/// The ground under a wheel and the tyre's size: the vertex shader flattens the tyre's rubber
/// where the renderer sinks it into the ground, and the ground round it is shaded.
#[derive(Clone, Copy, Debug)]
pub struct Tyre {
    /// Ground plane, world: unit normal, and the normal's dot with a point of the plane (where
    /// the tyre last touched it).
    pub ground: Vec4,
    /// Unloaded radius and half width, m.
    pub radius: f32,
    pub half_width: f32,
    /// The tyre is on that ground now (its rubber flattens on it).
    pub pressed: bool,
    /// How dark the shade round it, 0..1 (eased with the contact).
    pub shade: f32,
}

/// Tyres whose shade the ground shows (the player's car's).
const MAX_CONTACTS: usize = 4;

/// Camera and lighting for one frame.
pub struct View {
    pub view: Mat4,
    pub proj: Mat4,
    pub eye: Vec3,
    /// The speed blur over the scene, 0..1 (a booster's push, see [`crate::blur`]).
    pub blur: f32,
    /// The player's headlights, lit by night.
    pub headlights: Option<Headlights>,
}

/// Where a car's headlights shine from, and the car's axes there.
#[derive(Clone, Copy, Debug)]
pub struct Headlights {
    /// Between the two lamps.
    pub at: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

/// The light over a track: the sun's by day; by night a moon's, low over the horizon, under a
/// dark sky (scene.wgsl adds the stars), the headlights lighting the road.
#[derive(Clone, Copy, Debug)]
struct Lighting {
    /// Toward the sun (or the moon).
    sun_dir: Vec3,
    sun_color: [f32; 4],
    /// The sky at the zenith and at the horizon (the distance fog's colour), and the light
    /// coming back up from the ground.
    sky_top: [f32; 4],
    sky_horizon: [f32; 4],
    ground_bounce: [f32; 4],
    /// Distance fog: density per metre, distance it starts at.
    fog: [f32; 2],
    /// 0 by day, 1 by night (the shaders' night sky, darker dust, headlights).
    night: f32,
}

impl Lighting {
    fn of(time: TimeOfDay, planet: track::Planet) -> Self {
        let scaled = |c: [f32; 3], s: f32| [c[0] * s, c[1] * s, c[2] * s, 1.0];
        if planet == track::Planet::Ice && time == TimeOfDay::Day {
            // The ice planet's prototype: a cold sky, a weaker sun (it is farther from it).
            return Self {
                sun_dir: Vec3::new(-0.45, 0.62, 0.64).normalize(),
                sun_color: scaled(srgb(255, 248, 240), 2.0),
                sky_top: scaled(srgb(70, 104, 150), 0.8),
                sky_horizon: scaled(srgb(184, 204, 226), 1.0),
                ground_bounce: scaled(srgb(150, 165, 185), 0.5),
                fog: [1.0 / 1400.0, 150.0],
                night: 0.0,
            };
        }
        match time {
            TimeOfDay::Day => Self {
                sun_dir: Vec3::new(-0.45, 0.62, 0.64).normalize(),
                sun_color: scaled(srgb(255, 238, 214), 2.6),
                sky_top: scaled(srgb(176, 118, 92), 0.8),
                sky_horizon: scaled(srgb(226, 178, 140), 1.0),
                ground_bounce: scaled(srgb(170, 100, 70), 0.5),
                fog: [1.0 / 1400.0, 150.0],
                night: 0.0,
            },
            // Not a true Martian night (Phobos lights next to nothing): a moonlit night, dark
            // enough for the headlights to matter, light enough to read the circuit ahead.
            TimeOfDay::Night => Self {
                sun_dir: Vec3::new(-0.54, 0.34, 0.77).normalize(),
                sun_color: scaled(srgb(196, 208, 255), 0.8),
                sky_top: scaled(srgb(46, 60, 112), 0.5),
                sky_horizon: scaled(srgb(60, 66, 104), 0.7),
                ground_bounce: scaled(srgb(70, 52, 60), 0.25),
                fog: [1.0 / 1100.0, 120.0],
                night: 1.0,
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    sky_top: [f32; 4],
    sky_horizon: [f32; 4],
    ground_bounce: [f32; 4],
    fog: [f32; 4],
    misc: [f32; 4],
    storm_a: [f32; 4],
    storm_b: [f32; 4],
    far_light_view_proj: [[f32; 4]; 4],
    /// The weather (weather.rs): wind, drift of the air, camera velocity (w: 1 when the air
    /// carries snow).
    wind: [f32; 4],
    drift: [f32; 4],
    eye_vel: [f32; 4],
    /// Viewport in pixels, pixels per metre at a metre's depth, and how fast the snow falls.
    viewport: [f32; 4],
    /// Tyres on the ground, three vectors each: the middle of the footprint and the shade's
    /// strength (0: none); the axle and the footprint's half width; the heading and its half
    /// length (all in the ground's plane).
    contacts: [[f32; 4]; 3 * MAX_CONTACTS],
    /// The headlights (by night): where the beam starts and its strength (0: off), the beam's
    /// axis and its reach (metres), the car's up axis; the matrix of their shadow map.
    lamp_pos: [f32; 4],
    lamp_dir: [f32; 4],
    lamp_up: [f32; 4],
    lamp_view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniform {
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    /// A tyre's ground plane, and its radius and half width (0: not a tyre); see [`Tyre`].
    ground: [f32; 4],
    tyre: [f32; 4],
}

/// The footprints of the opaque cars' tyres on the ground (see [`FrameUniform::contacts`]): under
/// each wheel's centre, faded as the wheel rises off the ground.
fn tyre_contacts(items: &[DrawItem]) -> [[f32; 4]; 3 * MAX_CONTACTS] {
    let mut out = [[0.0; 4]; 3 * MAX_CONTACTS];
    let tyres = items.iter().filter(|item| item.tint.w >= 0.99).filter_map(|item| Some((item.model, item.tyre?)));
    for (k, (model, t)) in tyres.take(MAX_CONTACTS).enumerate() {
        let n = t.ground.truncate();
        let centre = model.w_axis.truncate();
        let above = n.dot(centre) - t.ground.w;
        let axle = model.x_axis.truncate();
        let axle = (axle - n * n.dot(axle)).normalize_or_zero();
        let heading = axle.cross(n);
        let lift = (above - t.radius).max(0.0) / (0.6 * t.radius);
        let shade = t.shade * (1.0 - lift.min(1.0));
        out[3 * k] = (centre - n * above).extend(shade).to_array();
        out[3 * k + 1] = axle.extend(t.half_width).to_array();
        out[3 * k + 2] = heading.extend(0.35 * t.radius).to_array();
    }
    out
}

pub fn srgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let f = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    [f(r), f(g), f(b)]
}

/// Where the sandstorm stands around the current track.
struct StormSite {
    /// Centre of the route's bounding box, at the start's height.
    centre: Vec3,
    /// Horizontal unit direction (x, z) from the centre to the storm.
    dir: Vec2,
    /// Distance from the centre to the farthest point of the route, metres.
    reach: f32,
    since: Instant,
}

pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    ghost_pipeline: wgpu::RenderPipeline,
    ground_pipeline: wgpu::RenderPipeline,
    road_pipeline: wgpu::RenderPipeline,
    marks_pipeline: wgpu::RenderPipeline,
    marks_buffer: wgpu::Buffer,
    dust_pipeline: wgpu::RenderPipeline,
    dust_buffer: wgpu::Buffer,
    dust_count: u32,
    sky_pipeline: wgpu::RenderPipeline,
    storm_pipeline: wgpu::RenderPipeline,
    storm: Option<StormSite>,
    /// The planet of the track: its sky and its materials (scene.wgsl's `frame.misc.w`).
    planet: track::Planet,
    /// Wind, drifting sand or falling snow, and gusts over the current track, since when.
    weather: Option<Weather>,
    weather_since: Instant,
    grain_pipeline: wgpu::RenderPipeline,
    gust_grain_pipeline: wgpu::RenderPipeline,
    puff_pipeline: wgpu::RenderPipeline,
    veil_pipeline: wgpu::RenderPipeline,
    gust_buffer: wgpu::Buffer,
    gust_count: u32,
    /// `MARS_STORM_TIME`: seconds of approach skipped, to see the storm at its closest.
    storm_skip: f32,
    /// Seconds since the track was set, when the clock is not the wall's (filming).
    pub clock: Option<f32>,
    shadow_pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    frame_layout: wgpu::BindGroupLayout,
    frame_group: wgpu::BindGroup,
    shadow_frame_group: wgpu::BindGroup,
    shadow_sampler: wgpu::Sampler,
    /// The far shadow map and what draws it: the sun's matrix in a frame of its own.
    far_shadow_view: wgpu::TextureView,
    far_shadow_buffer: wgpu::Buffer,
    far_shadow_group: wgpu::BindGroup,
    far_light_view_proj: Mat4,
    /// How far lookups in the far map are lifted off surfaces against acne, metres (half a texel:
    /// the slope bias of the shadow pass does the rest).
    far_lift: f32,
    livery_sampler: wgpu::Sampler,
    livery_view: wgpu::TextureView,
    surfaces: SurfaceTextures,
    noise_view: wgpu::TextureView,
    noise_sampler: wgpu::Sampler,
    object_buffer: wgpu::Buffer,
    object_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    meshes: Vec<GpuMesh>,
    lighting: Lighting,
    /// `MARS_TIME=day|night`: every map at that time (checks).
    time_override: Option<TimeOfDay>,
    /// The current track's mesh: what the headlights' shadow map draws.
    track_mesh: Option<MeshId>,
    /// The headlights' shadow map and what draws it (their matrix in a frame of its own).
    lamp_shadow_view: wgpu::TextureView,
    lamp_shadow_buffer: wgpu::Buffer,
    lamp_shadow_group: wgpu::BindGroup,
    speed_blur: crate::blur::SpeedBlur,
}

impl SceneRenderer {
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let format = gpu.config.format;

        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let object_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("objects"),
            size: OBJECT_STRIDE * MAX_OBJECTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shadow_view = create_shadow_map(device, "shadow map", SHADOW_SIZE);
        let far_shadow_view = create_shadow_map(device, "far shadow map", FAR_SHADOW_SIZE);
        let lamp_shadow_view = create_shadow_map(device, "headlight shadow map", LAMP_SHADOW_SIZE);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                // The car's livery texture and its sampler.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // The surface textures (colour, relief) and their sampler.
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // The far shadow map, and the headlights' (sampled with the shadow sampler).
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // The lattice of the weather's smoke noise, and its sampler (repeating).
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shadow_frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow frame layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let object_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("object layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<ObjectUniform>() as u64),
                },
                count: None,
            }],
        });

        let livery_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("livery sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let livery_view = create_livery(gpu, 1, 1, &[255, 255, 255, 255]);
        let surfaces = crate::surfaces::load(gpu);
        let noise_view = create_noise(gpu);
        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("noise sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let frame_group = create_frame_group(
            device,
            &frame_layout,
            &frame_buffer,
            [&shadow_view, &far_shadow_view, &lamp_shadow_view],
            &shadow_sampler,
            &livery_view,
            &livery_sampler,
            &surfaces,
            (&noise_view, &noise_sampler),
        );
        let shadow_frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow frame group"),
            layout: &shadow_frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buffer.as_entire_binding() }],
        });
        // The start of a frame (shadow.wgsl reads its first three matrices only).
        let far_shadow_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("far shadow frame"),
            size: 3 * std::mem::size_of::<[[f32; 4]; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let far_shadow_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("far shadow frame group"),
            layout: &shadow_frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: far_shadow_buffer.as_entire_binding() }],
        });
        let lamp_shadow_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headlight shadow frame"),
            size: 3 * std::mem::size_of::<[[f32; 4]; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lamp_shadow_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("headlight shadow frame group"),
            layout: &shadow_frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: lamp_shadow_buffer.as_entire_binding() }],
        });
        let object_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("object group"),
            layout: &object_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &object_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<ObjectUniform>() as u64),
                }),
            }],
        });

        let scene_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene.wgsl").into()),
        });
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shadow.wgsl").into()),
        });

        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene layout"),
            bind_group_layouts: &[Some(&frame_layout), Some(&object_layout)],
            immediate_size: 0,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow layout"),
            bind_group_layouts: &[Some(&shadow_frame_layout), Some(&object_layout)],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRS,
        };
        let shadow_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SHADOW_ATTRS,
        };
        let color_target = [Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        // Opaque objects, and ghosts in a pipeline of their own (its shader discards, which would
        // turn off the hidden surface removal of tile-based GPUs for everything else).
        let scene_pipeline = |label: &str, fragment: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&scene_layout),
                vertex: wgpu::VertexState {
                    module: &scene_module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    // Reverse-Z: nearer is greater.
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
                fragment: Some(wgpu::FragmentState {
                    module: &scene_module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &color_target,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = scene_pipeline("scene", "fs_main");
        let ghost_pipeline = scene_pipeline("scene ghosts", "fs_ghost");
        let ground_pipeline = scene_pipeline("scene ground", "fs_ground");
        let road_pipeline = scene_pipeline("scene roads", "fs_road");
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &scene_module,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &scene_module,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &color_target,
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &[Some(shadow_vertex_layout)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 4, slope_scale: 3.0, clamp: 0.0 },
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        let marks_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("marks.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/marks.wgsl").into()),
        });
        let marks_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MarkVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &MARK_ATTRS,
        };
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let marks_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("marks"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &marks_module,
                entry_point: Some("vs_marks"),
                compilation_options: Default::default(),
                buffers: &[Some(marks_layout)],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &marks_module,
                entry_point: Some("fs_marks"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let marks_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tyre marks"),
            size: (MARKS_CAPACITY * 6 * std::mem::size_of::<MarkVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dust_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dust.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/dust.wgsl").into()),
        });
        let dust_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ParticleVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &DUST_ATTRS,
        };
        let dust_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dust"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &dust_module,
                entry_point: Some("vs_dust"),
                compilation_options: Default::default(),
                buffers: &[Some(dust_layout)],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &dust_module,
                entry_point: Some("fs_dust"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let dust_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dust"),
            size: (DUST_CAPACITY * 6 * std::mem::size_of::<ParticleVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let storm_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("storm.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/storm.wgsl").into()),
        });
        // Over the opaque scene, under the tyre marks and dust (which are nearer).
        let storm_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("storm"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &storm_module,
                entry_point: Some("vs_storm"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &storm_module,
                entry_point: Some("fs_storm"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let weather_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weather.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/weather.wgsl").into()),
        });
        let gust_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GustInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &GUST_ATTRS,
        };
        let gust_buffers = [Some(gust_layout)];
        // Blended over the scene like the dust, tested against its depth (the veil over all of it).
        let weather_pipeline = |label: &str, vertex: &str, fragment: &str, gusts: bool, depth: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&scene_layout),
                vertex: wgpu::VertexState {
                    module: &weather_module,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: if gusts { &gust_buffers } else { &[] },
                },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(depth),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: MSAA, mask: !0, alpha_to_coverage_enabled: false },
                fragment: Some(wgpu::FragmentState {
                    module: &weather_module,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let nearer = wgpu::CompareFunction::GreaterEqual;
        let grain_pipeline = weather_pipeline("weather grains", "vs_grain", "fs_grain", false, nearer);
        let gust_grain_pipeline = weather_pipeline("gust grains", "vs_gust_grain", "fs_grain", true, nearer);
        let puff_pipeline = weather_pipeline("gust puffs", "vs_puff", "fs_puff", true, nearer);
        let veil_pipeline = weather_pipeline("gust veil", "vs_veil", "fs_veil", false, wgpu::CompareFunction::Always);
        let gust_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gusts"),
            size: (MAX_GUSTS * std::mem::size_of::<GustInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (msaa_view, depth_view) = create_targets(device, format, gpu.config.width, gpu.config.height);
        Self {
            pipeline,
            ghost_pipeline,
            ground_pipeline,
            road_pipeline,
            marks_pipeline,
            marks_buffer,
            dust_pipeline,
            dust_buffer,
            dust_count: 0,
            sky_pipeline,
            storm_pipeline,
            storm: None,
            planet: track::Planet::Mars,
            weather: None,
            weather_since: Instant::now(),
            grain_pipeline,
            gust_grain_pipeline,
            puff_pipeline,
            veil_pipeline,
            gust_buffer,
            gust_count: 0,
            storm_skip: std::env::var("MARS_STORM_TIME").ok().and_then(|t| t.parse().ok()).unwrap_or(0.0),
            clock: None,
            shadow_pipeline,
            frame_buffer,
            frame_layout,
            frame_group,
            shadow_frame_group,
            shadow_sampler,
            far_shadow_view,
            far_shadow_buffer,
            far_shadow_group,
            // Until a track is baked, every point falls outside the far map.
            far_light_view_proj: Mat4::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::new(3.0, 3.0, 0.5, 1.0)),
            far_lift: 0.0,
            livery_sampler,
            livery_view,
            surfaces,
            noise_view,
            noise_sampler,
            object_buffer,
            object_group,
            shadow_view,
            msaa_view,
            depth_view,
            size: (gpu.config.width, gpu.config.height),
            format,
            meshes: Vec::new(),
            lighting: Lighting::of(TimeOfDay::Day, track::Planet::Mars),
            time_override: std::env::var("MARS_TIME").ok().and_then(|t| serde_json::from_value(t.into()).ok()),
            track_mesh: None,
            lamp_shadow_view,
            lamp_shadow_buffer,
            lamp_shadow_group,
            speed_blur: crate::blur::SpeedBlur::new(device, format),
        }
    }

    /// Sets the scene up for a new track on `planet`, drawn with `mesh` and raced at `time`:
    /// lights it, bakes its far shadows, places the sandstorm and starts its approach over
    /// (kilometres beyond the route, ahead of the start), and starts the planet's weather over,
    /// the wind blowing from the storm. The ice planet has no storm: its wind starts out blowing
    /// at the car on the start line.
    pub fn set_track(&mut self, gpu: &Gpu, track: &track::Track, mesh: MeshId, time: TimeOfDay, planet: track::Planet) {
        self.lighting = Lighting::of(self.time_override.unwrap_or(time), planet);
        self.track_mesh = Some(mesh);
        self.bake_shadows(gpu, track, mesh);
        self.planet = planet;
        self.weather_since = Instant::now();
        let forward = Vec2::new(track.start.forward().x, track.start.forward().z);
        if !planet.is_mars() {
            self.weather = Some(Weather::new(Climate::of(planet), &track.route, -forward));
            self.storm = None;
            return;
        }
        let (lo, hi) = track.route.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| {
            (lo.min(Vec2::new(p.x, p.z)), hi.max(Vec2::new(p.x, p.z)))
        });
        let mid = if lo.x <= hi.x { 0.5 * (lo + hi) } else { Vec2::new(track.start.position.x, track.start.position.z) };
        let reach = track.route.iter().map(|p| Vec2::new(p.x, p.z).distance(mid)).fold(0.0, f32::max);
        let sun = Vec2::new(self.lighting.sun_dir.x, self.lighting.sun_dir.z);
        let away = if forward.perp_dot(sun) > 0.0 { -STORM_SUN_OFFSET } else { STORM_SUN_OFFSET };
        let dir = Vec2::from_angle(away.to_radians()).rotate(forward);
        self.weather = Some(Weather::new(Climate::of(planet), &track.route, -dir));
        self.storm = Some(StormSite {
            centre: Vec3::new(mid.x, track.start.position.y, mid.y),
            dir,
            reach,
            since: Instant::now(),
        });
    }

    /// The sun's view, shared by both shadow maps.
    fn light_view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.lighting.sun_dir * 400.0, Vec3::ZERO, Vec3::Y)
    }

    /// Draws the far shadow map once for the track: the track's shadows (the sun is fixed and the
    /// track does not move) over the route and `FAR_SHADOW_MARGIN` around it. The near map,
    /// redrawn every frame, only reaches a hundred metres or so ahead of the camera; beyond it,
    /// the shadows of rocks and raised roads used to appear as the car came closer.
    fn bake_shadows(&mut self, gpu: &Gpu, track: &track::Track, mesh: MeshId) {
        let (lo, hi) = track
            .route
            .iter()
            .fold((track.start.position, track.start.position), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        // The ground the map shades (rock faces up to a hundred metres above the road included),
        // and the top of whatever can cast a shadow on it.
        let lo = lo - Vec3::new(FAR_SHADOW_MARGIN, 30.0, FAR_SHADOW_MARGIN);
        let hi = hi + Vec3::new(FAR_SHADOW_MARGIN, 120.0, FAR_SHADOW_MARGIN);
        let top = track.mesh.positions.iter().chain(&track.decor.positions).fold(hi.y, |top, p| top.max(p.y));

        let light_view = self.light_view();
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let p = light_view.transform_point3(corner);
            min = min.min(p);
            max = max.max(p);
        }
        // Distances from the sun's plane (the light looks down -z). Casters stand up to `top`,
        // up the ray to the sun from the ground: that much nearer.
        let reach = (top - lo.y) / self.lighting.sun_dir.y.max(0.1);
        let light_proj =
            glam::camera::rh::proj::directx::orthographic(min.x, max.x, min.y, max.y, -max.z - reach - 1.0, -min.z + 1.0);
        self.far_light_view_proj = light_proj * light_view;
        self.far_lift = 0.5 * (max.x - min.x).max(max.y - min.y) / FAR_SHADOW_SIZE as f32;

        let object = ObjectUniform { model: Mat4::IDENTITY.to_cols_array_2d(), tint: [1.0; 4], ground: [0.0; 4], tyre: [0.0; 4] };
        gpu.queue.write_buffer(&self.object_buffer, 0, bytemuck::bytes_of(&object));
        let frame = [[[0.0f32; 4]; 4], [[0.0; 4]; 4], self.far_light_view_proj.to_cols_array_2d()];
        gpu.queue.write_buffer(&self.far_shadow_buffer, 0, bytemuck::bytes_of(&frame));
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("far shadows") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("far shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.far_shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.far_shadow_group, &[]);
            self.draw(&mut pass, 0, mesh, None);
        }
        gpu.queue.submit([encoder.finish()]);
    }

    pub fn write_dust(&mut self, queue: &wgpu::Queue, vertices: &[ParticleVertex]) {
        let n = vertices.len().min(DUST_CAPACITY * 6);
        if n > 0 {
            queue.write_buffer(&self.dust_buffer, 0, bytemuck::cast_slice(&vertices[..n]));
        }
        self.dust_count = n as u32;
    }

    /// Writes new tyre-mark quads (byte offset, vertices), or wipes them all.
    pub fn write_marks(&self, queue: &wgpu::Queue, clear: bool, updates: Vec<(u64, Vec<MarkVertex>)>) {
        if clear {
            let zeros = vec![0u8; self.marks_buffer.size() as usize];
            queue.write_buffer(&self.marks_buffer, 0, &zeros);
        }
        for (offset, vertices) in updates {
            queue.write_buffer(&self.marks_buffer, offset, bytemuck::cast_slice(&vertices));
        }
    }

    /// The car's livery: an sRGB RGBA8 image sampled by kind::LIVERY vertices at their uv.
    pub fn set_livery(&mut self, gpu: &Gpu, width: u32, height: u32, rgba: &[u8]) {
        self.livery_view = create_livery(gpu, width, height, rgba);
        self.frame_group = create_frame_group(
            &gpu.device,
            &self.frame_layout,
            &self.frame_buffer,
            [&self.shadow_view, &self.far_shadow_view, &self.lamp_shadow_view],
            &self.shadow_sampler,
            &self.livery_view,
            &self.livery_sampler,
            &self.surfaces,
            (&self.noise_view, &self.noise_sampler),
        );
    }

    pub fn upload(&mut self, device: &wgpu::Device, mesh: &MeshData) -> MeshId {
        self.meshes.push(create_mesh(device, mesh));
        MeshId(self.meshes.len() - 1)
    }

    pub fn replace(&mut self, device: &wgpu::Device, id: MeshId, mesh: &MeshData) {
        self.meshes[id.0] = create_mesh(device, mesh);
    }

    fn ensure_targets(&mut self, gpu: &Gpu) {
        let size = (gpu.config.width, gpu.config.height);
        if size != self.size {
            let (m, d) = create_targets(&gpu.device, self.format, size.0, size.1);
            self.msaa_view = m;
            self.depth_view = d;
            self.size = size;
        }
    }

    pub fn render(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        view: &View,
        items: &[DrawItem],
    ) {
        self.ensure_targets(gpu);

        // Near shadow map: the sun's camera over the ground just ahead of ours, snapped to shadow
        // texels so shadows do not shimmer as it moves.
        let light_view = self.light_view();
        let texel = 2.0 * SHADOW_EXTENT / SHADOW_SIZE as f32;
        let forward = -view.view.row(2).truncate();
        let focus = view.eye + Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero() * SHADOW_AHEAD;
        let focus_ls = light_view.transform_point3(focus);
        let snapped = Vec3::new((focus_ls.x / texel).round() * texel, (focus_ls.y / texel).round() * texel, focus_ls.z);
        let light_proj = glam::camera::rh::proj::directx::orthographic(
            snapped.x - SHADOW_EXTENT,
            snapped.x + SHADOW_EXTENT,
            snapped.y - SHADOW_EXTENT,
            snapped.y + SHADOW_EXTENT,
            -snapped.z - 400.0,
            -snapped.z + 400.0,
        );
        let light_view_proj = light_proj * light_view;
        let view_proj = view.proj * view.view;

        let light = self.lighting;
        let lamp = view.headlights.filter(|_| light.night > 0.0).map(|h| {
            // The beam's axis dips a little under the heading.
            let (sin, cos) = LAMP_DIP.sin_cos();
            let dir = (h.forward * cos - h.up * sin).normalize();
            let up = (h.up * cos + h.forward * sin).normalize();
            let (side, above, below) = LAMP_FRUSTUM;
            let proj = glam::camera::rh::proj::directx::frustum(
                -side * LAMP_NEAR,
                side * LAMP_NEAR,
                -below * LAMP_NEAR,
                above * LAMP_NEAR,
                LAMP_NEAR,
                LAMP_REACH,
            );
            (h.at, dir, up, proj * glam::camera::rh::view::look_to_mat4(h.at, dir, up))
        });
        let mut frame = FrameUniform {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            light_view_proj: light_view_proj.to_cols_array_2d(),
            camera_pos: view.eye.extend(1.0).to_array(),
            sun_dir: light.sun_dir.extend(0.0).to_array(),
            sun_color: light.sun_color,
            sky_top: light.sky_top,
            sky_horizon: light.sky_horizon,
            ground_bounce: light.ground_bounce,
            fog: [light.fog[0], light.fog[1], if self.planet.is_mars() { 0.0 } else { 1.0 }, 0.0],
            misc: [1.0 / SHADOW_SIZE as f32, 1.0 / FAR_SHADOW_SIZE as f32, self.far_lift, light.night],
            storm_a: [0.0; 4],
            storm_b: [0.0; 4],
            far_light_view_proj: self.far_light_view_proj.to_cols_array_2d(),
            wind: [0.0; 4],
            drift: [0.0; 4],
            eye_vel: [0.0; 4],
            viewport: [self.size.0 as f32, self.size.1 as f32, 0.5 * self.size.1 as f32 * view.proj.y_axis.y, 0.0],
            contacts: tyre_contacts(items),
            lamp_pos: lamp.map_or([0.0; 4], |(at, ..)| at.extend(1.0).to_array()),
            lamp_dir: lamp.map_or([0.0; 4], |(_, dir, ..)| dir.extend(LAMP_REACH).to_array()),
            lamp_up: lamp.map_or([0.0; 4], |(_, _, up, _)| up.extend(0.0).to_array()),
            lamp_view_proj: lamp.map_or(Mat4::IDENTITY, |(.., m)| m).to_cols_array_2d(),
        };
        if let Some(s) = &self.storm {
            let since = self.clock.unwrap_or_else(|| s.since.elapsed().as_secs_f32());
            let t = since + self.storm_skip;
            let left = (-t / STORM_APPROACH).exp();
            let front = s.reach + STORM_STOP + (STORM_START - STORM_STOP) * left;
            frame.storm_a = [s.centre.x, s.centre.z, s.dir.x, s.dir.y];
            frame.storm_b = [front, t, s.centre.y, 1.0 - left];
        }
        if let Some(w) = &mut self.weather {
            w.update(self.clock.unwrap_or_else(|| self.weather_since.elapsed().as_secs_f32()), view.eye);
            let u = w.uniforms();
            (frame.wind, frame.drift, frame.eye_vel) = (u.wind, u.drift, u.eye_vel);
            frame.viewport[3] = u.snow_fall;
            let gusts = w.instances();
            if !gusts.is_empty() {
                gpu.queue.write_buffer(&self.gust_buffer, 0, bytemuck::cast_slice(&gusts));
            }
            self.gust_count = gusts.len() as u32;
        }
        gpu.queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&frame));

        let mut objects = vec![0u8; (OBJECT_STRIDE as usize) * items.len().max(1)];
        for (i, item) in items.iter().enumerate().take(MAX_OBJECTS as usize) {
            let (ground, tyre) =
                item.tyre.map_or(([0.0; 4], [0.0; 4]), |t| (t.ground.to_array(), [t.radius, t.half_width, if t.pressed { 1.0 } else { 0.0 }, 0.0]));
            let o = ObjectUniform { model: item.model.to_cols_array_2d(), tint: item.tint.to_array(), ground, tyre };
            let at = i * OBJECT_STRIDE as usize;
            objects[at..at + std::mem::size_of::<ObjectUniform>()].copy_from_slice(bytemuck::bytes_of(&o));
        }
        gpu.queue.write_buffer(&self.object_buffer, 0, &objects);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_frame_group, &[]);
            for (i, item) in items.iter().enumerate().take(MAX_OBJECTS as usize) {
                if item.cast_shadow {
                    self.draw(&mut pass, i, item.mesh, None);
                }
            }
        }
        // The headlights' shadows: the track only (the car would hide its own lamps).
        let track_slot = items.iter().take(MAX_OBJECTS as usize).position(|item| Some(item.mesh) == self.track_mesh);
        if let (Some((.., lamp_view_proj)), Some(slot)) = (lamp, track_slot) {
            let frame = [[[0.0f32; 4]; 4], [[0.0; 4]; 4], lamp_view_proj.to_cols_array_2d()];
            gpu.queue.write_buffer(&self.lamp_shadow_buffer, 0, bytemuck::bytes_of(&frame));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headlight shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.lamp_shadow_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.lamp_shadow_group, &[]);
            self.draw(&mut pass, slot, items[slot].mesh, None);
        }
        // While a booster pushes the car the scene resolves into the speed blur's image, drawn
        // into the frame smeared once the pass is over.
        let blurring = view.blur > 0.01;
        if blurring {
            self.speed_blur.prepare(&gpu.device, self.size);
        }
        let resolve = if blurring { self.speed_blur.image().unwrap_or(target) } else { target };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_view,
                    resolve_target: Some(resolve),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Discard },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(1, &self.object_group, &[0]);
            pass.draw(0..3, 0..1);
            // Opaque objects, one pipeline at a time: roads, then walls, rocks and cars, then the
            // ground (mostly behind the others, whose depth then hides much of it).
            for (pipeline, shading) in
                [(&self.road_pipeline, Shading::Road), (&self.pipeline, Shading::Other), (&self.ground_pipeline, Shading::Ground)]
            {
                pass.set_pipeline(pipeline);
                for (i, item) in items.iter().enumerate().take(MAX_OBJECTS as usize) {
                    if item.tint.w >= 0.99 {
                        self.draw(&mut pass, i, item.mesh, Some(shading));
                    }
                }
            }
            if items.iter().any(|item| item.tint.w < 0.99) {
                pass.set_pipeline(&self.ghost_pipeline);
                for (i, item) in items.iter().enumerate().take(MAX_OBJECTS as usize) {
                    if item.tint.w < 0.99 {
                        self.draw(&mut pass, i, item.mesh, None);
                    }
                }
            }
            if self.storm.is_some() {
                pass.set_pipeline(&self.storm_pipeline);
                pass.set_bind_group(1, &self.object_group, &[0]);
                pass.draw(0..STORM_VERTICES, 0..STORM_CURTAINS);
            }
            // Tyre marks over the opaque scene (unused slots are zero-area quads).
            pass.set_pipeline(&self.marks_pipeline);
            pass.set_bind_group(1, &self.object_group, &[0]);
            pass.set_vertex_buffer(0, self.marks_buffer.slice(..));
            pass.draw(0..(MARKS_CAPACITY * 6) as u32, 0..1);
            // The weather: the gusts' clouds and sand, the grains in the air, then the wheels'
            // dust (nearer), and the veil when the camera stands in a gust.
            if self.weather.is_some() {
                if self.gust_count > 0 {
                    pass.set_vertex_buffer(0, self.gust_buffer.slice(..));
                    pass.set_pipeline(&self.puff_pipeline);
                    pass.draw(0..PUFF_VERTICES, 0..self.gust_count);
                    pass.set_pipeline(&self.gust_grain_pipeline);
                    pass.draw(0..GUST_GRAIN_VERTICES, 0..self.gust_count);
                }
                if frame.drift[2] > 0.0 {
                    pass.set_pipeline(&self.grain_pipeline);
                    pass.draw(0..WEATHER_GRAIN_VERTICES, 0..1);
                }
            }
            if self.dust_count > 0 {
                pass.set_pipeline(&self.dust_pipeline);
                pass.set_vertex_buffer(0, self.dust_buffer.slice(..));
                pass.draw(0..self.dust_count, 0..1);
            }
            if frame.drift[3] > 0.004 {
                pass.set_pipeline(&self.veil_pipeline);
                pass.draw(0..3, 0..1);
            }
        }
        if blurring {
            // The streaks run toward the point the road runs to: far ahead of the camera.
            let ahead = view.eye + Vec3::new(forward.x, 0.0, forward.z).normalize_or(forward) * 1000.0;
            let clip = view_proj * ahead.extend(1.0);
            let focus = if clip.w > 1e-3 { Vec2::new(0.5 + 0.5 * clip.x / clip.w, 0.5 - 0.5 * clip.y / clip.w) } else { Vec2::new(0.5, 0.45) };
            let aspect = self.size.0 as f32 / self.size.1.max(1) as f32;
            self.speed_blur.apply(&gpu.queue, encoder, target, view.blur, focus.clamp(Vec2::splat(0.2), Vec2::splat(0.8)), aspect);
        }
    }

    /// Draws the parts of `mesh` shaded `only` that way, or all of it.
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, slot: usize, mesh: MeshId, only: Option<Shading>) {
        let m = &self.meshes[mesh.0];
        if m.count == 0 || only.is_some_and(|s| !m.parts.iter().any(|(r, p)| *p == s && !r.is_empty())) {
            return;
        }
        pass.set_bind_group(1, &self.object_group, &[(slot as u64 * OBJECT_STRIDE) as u32]);
        pass.set_vertex_buffer(0, m.vertices.slice(..));
        pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
        match only {
            None => pass.draw_indexed(0..m.count, 0, 0..1),
            Some(s) => {
                for (range, _) in m.parts.iter().filter(|(r, p)| *p == s && !r.is_empty()) {
                    pass.draw_indexed(range.clone(), 0, 0..1);
                }
            }
        }
    }
}

fn create_mesh(device: &wgpu::Device, mesh: &MeshData) -> GpuMesh {
    // Empty buffers are not allowed; keep a dummy element.
    let vertices: &[u8] = if mesh.vertices.is_empty() { &[0; 40] } else { bytemuck::cast_slice(&mesh.vertices) };
    let indices: &[u8] = if mesh.indices.is_empty() { &[0; 4] } else { bytemuck::cast_slice(&mesh.indices) };
    GpuMesh {
        vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: vertices,
            usage: wgpu::BufferUsages::VERTEX,
        }),
        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: indices,
            usage: wgpu::BufferUsages::INDEX,
        }),
        count: mesh.indices.len() as u32,
        parts: if mesh.parts.is_empty() { vec![(0..mesh.indices.len() as u32, Shading::Other)] } else { mesh.parts.clone() },
    }
}

fn create_targets(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let size = wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 };
    let msaa = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("msaa color"),
        size,
        mip_level_count: 1,
        sample_count: MSAA,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size,
        mip_level_count: 1,
        sample_count: MSAA,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    (msaa.create_view(&Default::default()), depth.create_view(&Default::default()))
}


fn create_shadow_map(device: &wgpu::Device, label: &str, size: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

#[allow(clippy::too_many_arguments)]
fn create_frame_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame_buffer: &wgpu::Buffer,
    [shadow_view, far_shadow_view, lamp_shadow_view]: [&wgpu::TextureView; 3],
    shadow_sampler: &wgpu::Sampler,
    livery_view: &wgpu::TextureView,
    livery_sampler: &wgpu::Sampler,
    surfaces: &SurfaceTextures,
    (noise_view, noise_sampler): (&wgpu::TextureView, &wgpu::Sampler),
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("frame group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: frame_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(shadow_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(livery_view) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(livery_sampler) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&surfaces.colour) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&surfaces.relief) },
            wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&surfaces.sampler) },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(far_shadow_view) },
            wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(lamp_shadow_view) },
            wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::TextureView(noise_view) },
            wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::Sampler(noise_sampler) },
        ],
    })
}

/// The lattice of weather.wgsl's value noise (weather.rs), read at its full size only.
fn create_noise(gpu: &Gpu) -> wgpu::TextureView {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("noise lattice"),
        size: wgpu::Extent3d { width: NOISE_SIDE, height: NOISE_SIDE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &crate::weather::noise_lattice(),
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(NOISE_SIDE), rows_per_image: Some(NOISE_SIDE) },
        wgpu::Extent3d { width: NOISE_SIDE, height: NOISE_SIDE, depth_or_array_layers: 1 },
    );
    texture.create_view(&Default::default())
}

/// An sRGB texture with its whole mip chain (each level averaged from the one above, in linear).
fn create_livery(gpu: &Gpu, width: u32, height: u32, rgba: &[u8]) -> wgpu::TextureView {
    let levels = 32 - width.max(height).leading_zeros();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("livery"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let to_linear = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let to_srgb = |c: f32| {
        let c = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    let mut level: Vec<[f32; 4]> = rgba
        .chunks_exact(4)
        .map(|p| [to_linear(p[0]), to_linear(p[1]), to_linear(p[2]), p[3] as f32 / 255.0])
        .collect();
    let (mut w, mut h) = (width, height);
    for mip in 0..levels {
        let bytes: Vec<u8> =
            level.iter().flat_map(|p| [to_srgb(p[0]), to_srgb(p[1]), to_srgb(p[2]), (p[3] * 255.0 + 0.5) as u8]).collect();
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: mip, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![[0.0f32; 4]; (nw * nh) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (2 * x + dx).min(w - 1);
                    let sy = (2 * y + dy).min(h - 1);
                    let p = level[(sy * w + sx) as usize];
                    for c in 0..4 {
                        acc[c] += p[c] * 0.25;
                    }
                }
                next[(y * nw + x) as usize] = acc;
            }
        }
        level = next;
        w = nw;
        h = nh;
    }
    texture.create_view(&Default::default())
}
