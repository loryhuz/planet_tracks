//! Surface textures of the track, the terrain and the rocks: two texture arrays with one layer per
//! material, baked by `tools/textures/bake.py` from generated photos (see its documentation).
//!
//! - colour: sRGB, and in A the height (0 low, 1 high), which the shader blends materials by;
//! - relief: R, G the tangent-space normal's x (along +u) and y (along +v), mapped from [-1, 1];
//!   the shader only reads it near the camera.
//!
//! Every layer tiles; `scene.wgsl` knows each layer's index and its size in metres.

use crate::car_model::{Image, decode_png};
use crate::gfx::Gpu;

/// (name, colour PNG, relief PNG), in the order of the `L_*` layers of `scene.wgsl`.
const LAYERS: [(&str, &[u8], &[u8]); 12] = [
    ("tarp", include_bytes!("../assets/textures/tarp_albedo.png"), include_bytes!("../assets/textures/tarp_normal.png")),
    ("dirt", include_bytes!("../assets/textures/dirt_albedo.png"), include_bytes!("../assets/textures/dirt_normal.png")),
    ("earth", include_bytes!("../assets/textures/earth_albedo.png"), include_bytes!("../assets/textures/earth_normal.png")),
    ("pebbles", include_bytes!("../assets/textures/pebbles_albedo.png"), include_bytes!("../assets/textures/pebbles_normal.png")),
    ("slabs", include_bytes!("../assets/textures/slabs_albedo.png"), include_bytes!("../assets/textures/slabs_normal.png")),
    ("sand", include_bytes!("../assets/textures/sand_albedo.png"), include_bytes!("../assets/textures/sand_normal.png")),
    ("rock", include_bytes!("../assets/textures/rock_albedo.png"), include_bytes!("../assets/textures/rock_normal.png")),
    ("concrete", include_bytes!("../assets/textures/concrete_albedo.png"), include_bytes!("../assets/textures/concrete_normal.png")),
    ("sandbag", include_bytes!("../assets/textures/sandbag_albedo.png"), include_bytes!("../assets/textures/sandbag_normal.png")),
    ("webbing", include_bytes!("../assets/textures/webbing_albedo.png"), include_bytes!("../assets/textures/webbing_normal.png")),
    ("galvanized", include_bytes!("../assets/textures/galvanized_albedo.png"), include_bytes!("../assets/textures/galvanized_normal.png")),
    ("rust", include_bytes!("../assets/textures/rust_albedo.png"), include_bytes!("../assets/textures/rust_normal.png")),
];

pub struct SurfaceTextures {
    pub colour: wgpu::TextureView,
    pub relief: wgpu::TextureView,
    /// Repeats, trilinear, anisotropic (the ground is mostly seen at grazing angles).
    pub sampler: wgpu::Sampler,
}

#[derive(Clone, Copy)]
enum Mips {
    /// sRGB colour averaged in linear, height averaged; RGBA8.
    Colour,
    /// Normal (x, y) averaged as unit vectors and renormalised; RG8.
    Relief,
}

pub fn load(gpu: &Gpu) -> SurfaceTextures {
    let (colour, relief): (Vec<Image>, Vec<Image>) = std::thread::scope(|s| {
        let jobs: Vec<_> = LAYERS
            .iter()
            .map(|&(name, c, r)| {
                s.spawn(move || {
                    let decode = |kind: &str, bytes: &[u8]| {
                        decode_png(bytes).unwrap_or_else(|e| panic!("assets/textures/{name}_{kind}.png: {e}"))
                    };
                    (decode("albedo", c), decode("normal", r))
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().expect("texture decoding panicked")).unzip()
    });
    SurfaceTextures {
        colour: create_array(gpu, "surface colour", &colour, wgpu::TextureFormat::Rgba8UnormSrgb, Mips::Colour),
        relief: create_array(gpu, "surface relief", &relief, wgpu::TextureFormat::Rg8Unorm, Mips::Relief),
        sampler: gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("surface sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 4,
            ..Default::default()
        }),
    }
}

fn create_array(gpu: &Gpu, label: &str, layers: &[Image], format: wgpu::TextureFormat, mips: Mips) -> wgpu::TextureView {
    let (width, height) = (layers[0].width, layers[0].height);
    assert!(layers.iter().all(|l| l.width == width && l.height == height), "{label}: layers differ in size");
    let levels = 32 - width.max(height).leading_zeros();
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: layers.len() as u32 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texel = match mips {
        Mips::Colour => 4,
        Mips::Relief => 2,
    };
    let chains: Vec<Vec<Vec<u8>>> = std::thread::scope(|s| {
        let jobs: Vec<_> = layers.iter().map(|l| s.spawn(move || mip_chain(l, levels, mips))).collect();
        jobs.into_iter().map(|j| j.join().expect("mip generation panicked")).collect()
    });
    for (layer, chain) in chains.iter().enumerate() {
        for (mip, bytes) in chain.iter().enumerate() {
            let (w, h) = ((width >> mip).max(1), (height >> mip).max(1));
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(texel * w), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// Every mip level of a layer, each level the 2 x 2 average of the one above (the layers tile,
/// so the averages never need a border rule).
fn mip_chain(image: &Image, levels: u32, mips: Mips) -> Vec<Vec<u8>> {
    let to_linear = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let to_srgb = |c: f32| {
        let c = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    let unorm = |c: f32| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    // Working values: linear colour and height, or the unit normal.
    let mut level: Vec<[f32; 4]> = image
        .rgba
        .chunks_exact(4)
        .map(|p| match mips {
            Mips::Colour => [to_linear(p[0]), to_linear(p[1]), to_linear(p[2]), p[3] as f32 / 255.0],
            Mips::Relief => {
                let x = p[0] as f32 / 127.5 - 1.0;
                let y = p[1] as f32 / 127.5 - 1.0;
                [x, y, (1.0 - x * x - y * y).max(0.0).sqrt(), 0.0]
            }
        })
        .collect();
    let (mut w, mut h) = (image.width, image.height);
    let mut chain = Vec::with_capacity(levels as usize);
    for _ in 0..levels {
        let mut bytes = Vec::with_capacity(level.len() * 4);
        for p in &level {
            match mips {
                Mips::Colour => bytes.extend_from_slice(&[to_srgb(p[0]), to_srgb(p[1]), to_srgb(p[2]), unorm(p[3])]),
                Mips::Relief => {
                    let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-6);
                    bytes.extend_from_slice(&[unorm(0.5 + 0.5 * p[0] / l), unorm(0.5 + 0.5 * p[1] / l)]);
                }
            }
        }
        chain.push(bytes);
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![[0.0f32; 4]; (nw * nh) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = level[(((2 * y + dy) % h) * w + (2 * x + dx) % w) as usize];
                    for c in 0..4 {
                        acc[c] += 0.25 * p[c];
                    }
                }
                next[(y * nw + x) as usize] = acc;
            }
        }
        level = next;
        w = nw;
        h = nh;
    }
    chain
}
