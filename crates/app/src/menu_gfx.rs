//! The menu's background on the GPU (shaders/menu.wgsl): one full-screen pass drawing the
//! footage of the game in a loop (`video`), or the night sky where it cannot play or before its
//! first frame, then the static of a planet still to come and up to six procedural planets. The
//! menu decides what shows and where (in physical pixels); egui draws the interface over it.

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use crate::gfx::Gpu;
use crate::video::{FPS, Frame, VideoLoop};

pub const MAX_PLANETS: usize = 6;
const CRATERS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetKind {
    Mars = 0,
    Ice = 1,
    Gas = 2,
}

/// One planet to draw, in physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct PlanetDraw {
    pub kind: PlanetKind,
    pub center: Vec2,
    pub radius: f32,
    /// Spin about its axis, radians.
    pub rot: f32,
    /// 0 in full colour, 1 dark and desaturated (a planet still to come).
    pub dim: f32,
    /// Strength of its ring, 0 for none.
    pub ring: f32,
    pub alpha: f32,
    /// Drawn only inside this rectangle: x0, y0, x1, y1.
    pub clip: [f32; 4],
    /// Light catching the edge (sRGB 0..1).
    pub rim: [f32; 3],
    /// Soft glow around it: colour, strength, reach in radii (0 = none).
    pub halo: ([f32; 3], f32, f32),
}

/// The menu's footage, filmed in the game with `MARS_FILM` (`tools/video/menu_montage.sh`): one
/// for wide windows (16:9, 1920 × 1080), one for phones held upright (1080 × 2338).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footage {
    Wide,
    Tall,
}

impl Footage {
    fn clip(self) -> &'static str {
        match self {
            Footage::Wide => "menu-wide.mp4",
            Footage::Tall => "menu-tall.mp4",
        }
    }
}

/// What the background shows this frame.
#[derive(Clone, Debug, Default)]
pub struct SkyScene {
    /// Back to front.
    pub planets: Vec<PlanetDraw>,
    /// Horizon glow ellipse: centre and radii, px.
    pub glow: [f32; 4],
    /// Shifts the stars as the menu moves between screens.
    pub parallax: f32,
    /// Seconds, for the stars and dust.
    pub time: f32,
    /// How much the footage covers the sky, 0..1 (it fades in on its first frame).
    pub video: f32,
    /// Darkening of everything behind the planets, 0..1.
    pub dim: f32,
    /// Static over the background: colour (sRGB 0..1) and strength.
    pub noise: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PlanetUniform {
    a: [f32; 4],
    b: [f32; 4],
    clip: [f32; 4],
    rim: [f32; 4],
    halo: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniform {
    size: [f32; 4],
    glow: [f32; 4],
    misc: [f32; 4],
    video: [f32; 4],
    noise: [f32; 4],
    planets: [PlanetUniform; MAX_PLANETS],
    craters: [[f32; 4]; CRATERS],
}

pub struct MenuRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    group: wgpu::BindGroup,
    craters: [[f32; 4]; CRATERS],
    video: Option<Playing>,
    /// The footage last asked for: one that is missing is not looked for again every frame.
    asked: Option<Footage>,
}

/// The footage being played.
struct Playing {
    footage: Footage,
    clip: VideoLoop,
    /// The texture its frames go to, with their size; none before the first frame.
    texture: Option<(wgpu::Texture, u32, u32)>,
    /// Time not yet spent on frames, seconds.
    behind: f32,
    /// Seconds into the loop of the frame on screen.
    time: f32,
    /// Fade-in after the first frame, 0..1.
    shown: f32,
}

/// Craters scattered over the sphere: a unit vector and an angular radius each, the same every
/// run.
fn craters() -> [[f32; 4]; CRATERS] {
    let mut seed = 2036u32;
    let mut next = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let mut out = [[0.0; 4]; CRATERS];
    for c in &mut out {
        let lat = (next() * 2.0 - 1.0).asin();
        let lon = next() * std::f32::consts::TAU - std::f32::consts::PI;
        let r = 0.015 + next().powf(2.2) * 0.09;
        *c = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos(), r];
    }
    out
}

impl MenuRenderer {
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("menu.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/menu.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("menu layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let craters = craters();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("menu sky"),
            contents: bytemuck::bytes_of(&SkyUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("menu video"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Until the footage's first frame: one black pixel.
        let none = video_texture(gpu, 1, 1);
        gpu.queue.write_texture(none.as_image_copy(), &[0, 0, 0, 255], wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) }, wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 });
        let group = bind_group(device, &layout, &buffer, &none, &sampler);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("menu pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menu"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_menu"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_menu"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: gpu.config.format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, buffer, layout, sampler, group, craters, video: None, asked: None }
    }

    /// Plays `want` (or stops, with `None`), moving on by `dt` seconds: frames are taken from
    /// the decoder at the footage's rate and the latest goes to the texture.
    pub fn update_video(&mut self, gpu: &Gpu, want: Option<Footage>, dt: f32) {
        if self.video.as_ref().map(|v| v.footage) != want && self.asked != want {
            self.asked = want;
            self.video = want.and_then(|footage| {
                Some(Playing { footage, clip: VideoLoop::open(footage.clip())?, texture: None, behind: 0.0, time: 0.0, shown: 0.0 })
            });
        }
        let Some(v) = &mut self.video else { return };
        let period = 1.0 / FPS;
        v.behind = (v.behind + dt.min(0.25)).min(4.0 * period);
        let mut latest: Option<Frame> = None;
        while v.behind >= period || v.texture.is_none() {
            let Some(frame) = v.clip.next() else { break };
            v.behind = (v.behind - period).max(0.0);
            if let Some(old) = latest.replace(frame) {
                v.clip.recycle(old.data);
            }
        }
        if v.texture.is_some() {
            v.shown = (v.shown + dt / 0.6).min(1.0);
        }
        let Some(frame) = latest else { return };
        v.time = frame.time;
        if v.texture.as_ref().is_none_or(|t| t.1 != frame.width || t.2 != frame.height) {
            let texture = video_texture(gpu, frame.width, frame.height);
            self.group = bind_group(&gpu.device, &self.layout, &self.buffer, &texture, &self.sampler);
            v.texture = Some((texture, frame.width, frame.height));
        }
        if let Some((texture, w, h)) = &v.texture {
            gpu.queue.write_texture(
                texture.as_image_copy(),
                &frame.data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(*h) },
                wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
            );
        }
        v.clip.recycle(frame.data);
    }

    /// Seconds into the footage's loop, while it plays.
    pub fn video_time(&self) -> Option<f32> {
        self.video.as_ref().filter(|v| v.texture.is_some()).map(|v| v.time)
    }

    pub fn render(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, scene: &SkyScene, pixels_per_point: f32) {
        let mut u = SkyUniform::zeroed();
        u.size = [gpu.config.width as f32, gpu.config.height as f32, scene.time, pixels_per_point];
        u.glow = scene.glow;
        u.misc = [scene.parallax, scene.planets.len().min(MAX_PLANETS) as f32, 0.0, 0.0];
        if let Some(Playing { texture: Some((_, w, h)), shown, .. }) = &self.video {
            u.video = [scene.video * shown, scene.dim, *w as f32, *h as f32];
        } else {
            u.video = [0.0, scene.dim, 1.0, 1.0];
        }
        u.noise = scene.noise;
        for (slot, p) in u.planets.iter_mut().zip(&scene.planets) {
            *slot = PlanetUniform {
                a: [p.center.x, p.center.y, p.radius.max(1.0), p.rot],
                b: [p.kind as u32 as f32, p.dim, p.ring, p.alpha],
                clip: p.clip,
                rim: [p.rim[0], p.rim[1], p.rim[2], p.halo.1],
                halo: [p.halo.0[0], p.halo.0[1], p.halo.0[2], p.halo.2],
            };
        }
        u.craters = self.craters;
        gpu.queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&u));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("menu pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// The texture a frame of footage goes to: BGRA, its bytes sampled as they are (the menu pass
/// writes sRGB values straight to the gamma target).
fn video_texture(gpu: &Gpu, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("menu video"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer, texture: &wgpu::Texture, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
    let view = texture.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("menu group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}
