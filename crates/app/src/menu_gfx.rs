//! The menu's background on the GPU (shaders/menu.wgsl): one full-screen pass drawing the night
//! sky and up to six procedural planets. The menu decides where they go (in physical pixels);
//! egui draws the interface over it.

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use crate::gfx::Gpu;

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
    planets: [PlanetUniform; MAX_PLANETS],
    craters: [[f32; 4]; CRATERS],
}

pub struct MenuRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    craters: [[f32; 4]; CRATERS],
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
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let craters = craters();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("menu sky"),
            contents: bytemuck::bytes_of(&SkyUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("menu group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
        });
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
        Self { pipeline, buffer, group, craters }
    }

    pub fn render(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, scene: &SkyScene, pixels_per_point: f32) {
        let mut u = SkyUniform::zeroed();
        u.size = [gpu.config.width as f32, gpu.config.height as f32, scene.time, pixels_per_point];
        u.glow = scene.glow;
        u.misc = [scene.parallax, scene.planets.len().min(MAX_PLANETS) as f32, 0.0, 0.0];
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
