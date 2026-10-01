//! Self-test mode driven by environment variables, used to check the game without a player:
//!
//! - `MARS_SHOTS=dir` and `MARS_SHOT_TIMES=1.5,4,8`: screenshots (BMP) at those seconds;
//! - `MARS_AUTODRIVE=1`: hold the throttle and steer gently (a technical check, not a driver);
//! - `MARS_PROFILE=n`: start with profile n (1-based);
//! - `MARS_EXIT_AFTER=seconds`: quit;
//! - `MARS_ORBIT=yaw_deg,distance,height`: look at the car from a fixed angle (model checks).

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec3;
use physics::Input;

pub struct Debug {
    start: Instant,
    shots_dir: Option<PathBuf>,
    shot_times: Vec<f32>,
    next_shot: usize,
    pub autodrive: bool,
    pub profile: Option<usize>,
    exit_after: Option<f32>,
    pub orbit: Option<(f32, f32, f32)>,
}

impl Debug {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let mut shot_times: Vec<f32> = var("MARS_SHOT_TIMES")
            .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        shot_times.sort_by(|a, b| a.total_cmp(b));
        Self {
            start: Instant::now(),
            shots_dir: var("MARS_SHOTS").map(PathBuf::from),
            shot_times,
            next_shot: 0,
            autodrive: var("MARS_AUTODRIVE").is_some(),
            profile: var("MARS_PROFILE").and_then(|p| p.parse::<usize>().ok()).map(|p| p.saturating_sub(1)),
            exit_after: var("MARS_EXIT_AFTER").and_then(|s| s.parse().ok()),
            orbit: var("MARS_ORBIT").and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 3).then(|| (v[0], v[1], v[2]))
            }),
        }
    }

    pub fn elapsed(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// Path of the screenshot to take this frame, if one is due.
    pub fn shot_due(&mut self) -> Option<PathBuf> {
        let dir = self.shots_dir.as_ref()?;
        let t = *self.shot_times.get(self.next_shot)?;
        if self.elapsed() < t {
            return None;
        }
        self.next_shot += 1;
        let _ = std::fs::create_dir_all(dir);
        Some(dir.join(format!("shot-{:02}-{:.1}s.bmp", self.next_shot, t)))
    }

    pub fn should_exit(&self) -> bool {
        self.exit_after.is_some_and(|t| self.elapsed() >= t)
    }

}

/// Follows the track's route for technical checks (nothing falls through, triggers fire).
/// It drives cautiously; its times say nothing about how the track plays.
#[derive(Default)]
pub struct Autopilot {
    index: usize,
}

impl Autopilot {
    pub fn reset(&mut self) {
        self.index = 0;
    }

    pub fn input(&mut self, route: &[Vec3], position: Vec3, forward: Vec3, speed: f32) -> Input {
        if route.len() < 2 {
            return Input { gas: 1.0, ..Default::default() };
        }
        // Nearest route point, searched a little ahead of the last one.
        let end = (self.index + 40).min(route.len());
        let (mut best, mut best_d) = (self.index, f32::MAX);
        for (i, p) in route.iter().enumerate().take(end).skip(self.index) {
            let d = p.distance_squared(position);
            if d < best_d {
                best = i;
                best_d = d;
            }
        }
        self.index = best;
        let look = ((10.0 + speed * 0.5) / 4.0) as usize;
        let target = route[(best + look).min(route.len() - 1)];
        let to = Vec3::new(target.x - position.x, 0.0, target.z - position.z).normalize_or(forward);
        let f = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::Z);
        let angle = (f.z * to.x - f.x * to.z).atan2(f.dot(to));
        let steer = (-angle * 2.5).clamp(-1.0, 1.0);
        let kmh = speed * 3.6;
        let (gas, brake) = if angle.abs() > 0.5 && kmh > 90.0 {
            (0.0, 1.0)
        } else if angle.abs() > 0.25 {
            (0.4, 0.0)
        } else {
            (1.0, 0.0)
        };
        Input { steer, gas, brake }
    }
}

/// Copies the frame to a buffer (call before submitting), then `save_bmp` after submitting.
pub fn copy_frame(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) -> (wgpu::Buffer, u32, u32, u32) {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    (buffer, w, h, row)
}

pub fn save_bmp(device: &wgpu::Device, buffer: &wgpu::Buffer, w: u32, h: u32, row: u32, bgra: bool, path: &PathBuf) {
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let Ok(data) = slice.get_mapped_range() else { return };
    let mut out = Vec::with_capacity(54 + (w * h * 4) as usize);
    let file_size = 54 + w * h * 4;
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes()); // top-down
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in 0..h {
        let line = &data[(y * row) as usize..(y * row + w * 4) as usize];
        for px in line.chunks_exact(4) {
            if bgra {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            } else {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
    }
    drop(data);
    buffer.unmap();
    let _ = std::fs::write(path, out);
}
