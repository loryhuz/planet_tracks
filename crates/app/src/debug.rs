//! Self-test mode driven by environment variables, used to check the game without a player:
//!
//! - `MARS_SHOTS=dir` and `MARS_SHOT_TIMES=1.5,4,8`: screenshots (BMP) at those seconds;
//! - `MARS_SHOT_TICKS=600,1500`: screenshots at those physics ticks instead (with `MARS_SHOTS`):
//!   the simulation waits there while the camera settles, so runs with other settings or builds
//!   show exactly the same moments; the run quits after the last one;
//! - `MARS_NO_HUD=1`: no HUD or panel (clean screenshots);
//! - `MARS_AUTODRIVE=1`: hold the throttle and steer gently (a technical check, not a driver);
//! - `MARS_PROFILE=n`: start with profile n (1-based);
//! - `MARS_EXIT_AFTER=seconds`: quit;
//! - `MARS_ORBIT=yaw_deg,distance,height`: look at the car from a fixed angle (model checks);
//! - `MARS_VIEW=s,height,back`: look along the route at `s` metres from the start, from `back`
//!   metres before it and `height` metres above it (surface checks at a given place);
//! - `MARS_EYE=x,y,z,tx,ty,tz`: look from a fixed point at another (scenery checks);
//! - `MARS_STORM_TIME=seconds`: start the sandstorm that far into its approach (read by the
//!   renderer);
//! - `MARS_MAP=name`: start on that map; `MARS_DEBUG_PANEL=1`: the debug panel (Tab) open;
//!   `MARS_HUD_SETTINGS=seconds`: the settings sheet opened that far into the race (read by
//!   the HUD);
//! - `MARS_SAFE_AREA=top,right,bottom,left`: a phone's safe-area insets in points, on the computer
//!   (an iPhone 15 Pro held upright: `59,0,34,0`);
//! - `MARS_BENCH=from,to`: between those seconds every frame renders off screen (so a hidden
//!   window is measured too, without the display's frame cap) and waits for the GPU; the GPU time
//!   of those frames is printed at the end;
//! - `MARS_FILM=dir`: films the race (footage for the menu): every frame advances the game by
//!   exactly 1/`MARS_FILM_FPS` s (30 by default), whatever the time it takes to render, and
//!   frames from `MARS_FILM_FROM` to `MARS_FILM_FROM` + `MARS_FILM_SECONDS` seconds of race
//!   (0 and 10 by default) are saved as `frame-00001.bmp`…; the run quits after the last one.

use std::path::PathBuf;
use std::time::Instant;

use glam::Vec3;
use physics::Input;

pub struct Debug {
    start: Instant,
    shots_dir: Option<PathBuf>,
    shot_times: Vec<f32>,
    shot_ticks: Vec<u32>,
    next_shot: usize,
    /// When the simulation reached the tick of the next screenshot (seconds).
    held_since: Option<f32>,
    pub no_hud: bool,
    pub autodrive: bool,
    pub profile: Option<usize>,
    exit_after: Option<f32>,
    pub orbit: Option<(f32, f32, f32)>,
    pub view: Option<(f32, f32, f32)>,
    pub eye: Option<(Vec3, Vec3)>,
    pub safe_area: Option<[f32; 4]>,
    bench: Option<Bench>,
    pub film: Option<Film>,
}

pub struct Film {
    dir: PathBuf,
    pub fps: f32,
    /// Race seconds of the first and the last frame filmed.
    from: f32,
    to: f32,
    frames: u32,
}

struct Bench {
    from: f32,
    to: f32,
    /// Milliseconds from submission to completion, per frame.
    gpu_ms: Vec<f32>,
    size: (u32, u32),
    reported: bool,
}

impl Debug {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let mut shot_times: Vec<f32> = var("MARS_SHOT_TIMES")
            .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        shot_times.sort_by(|a, b| a.total_cmp(b));
        let mut shot_ticks: Vec<u32> = var("MARS_SHOT_TICKS")
            .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        shot_ticks.sort();
        Self {
            start: Instant::now(),
            shots_dir: var("MARS_SHOTS").map(PathBuf::from),
            shot_times,
            shot_ticks,
            next_shot: 0,
            held_since: None,
            no_hud: var("MARS_NO_HUD").is_some(),
            autodrive: var("MARS_AUTODRIVE").is_some(),
            profile: var("MARS_PROFILE").and_then(|p| p.parse::<usize>().ok()).map(|p| p.saturating_sub(1)),
            exit_after: var("MARS_EXIT_AFTER").and_then(|s| s.parse().ok()),
            orbit: var("MARS_ORBIT").and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 3).then(|| (v[0], v[1], v[2]))
            }),
            view: var("MARS_VIEW").and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 3).then(|| (v[0], v[1], v[2]))
            }),
            eye: var("MARS_EYE").and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 6).then(|| (Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5])))
            }),
            safe_area: var("MARS_SAFE_AREA").and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
            }),
            bench: var("MARS_BENCH").and_then(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 2).then(|| Bench { from: v[0], to: v[1], gpu_ms: Vec::new(), size: (0, 0), reported: false })
            }),
            film: var("MARS_FILM").map(|dir| {
                let num = |k: &str, default: f32| var(k).and_then(|s| s.parse().ok()).unwrap_or(default);
                let from = num("MARS_FILM_FROM", 0.0);
                Film { dir: PathBuf::from(dir), fps: num("MARS_FILM_FPS", 30.0), from, to: from + num("MARS_FILM_SECONDS", 10.0), frames: 0 }
            }),
        }
    }

    /// Filming: the fixed time step of every frame.
    pub fn film_dt(&self) -> Option<f32> {
        self.film.as_ref().map(|f| 1.0 / f.fps)
    }

    /// Whether this frame is measured.
    pub fn benching(&self) -> bool {
        let t = self.elapsed();
        self.bench.as_ref().is_some_and(|b| t >= b.from && t < b.to)
    }

    pub fn bench_sample(&mut self, gpu_ms: f32, size: (u32, u32)) {
        if let Some(b) = &mut self.bench {
            b.gpu_ms.push(gpu_ms);
            b.size = size;
        }
    }

    /// Prints the measured frames once their window is over.
    pub fn bench_report(&mut self) {
        let t = self.elapsed();
        let Some(b) = &mut self.bench else { return };
        if b.reported || t < b.to {
            return;
        }
        b.reported = true;
        let mut ms = b.gpu_ms.clone();
        if ms.is_empty() {
            println!("bench: no frame measured");
            return;
        }
        ms.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f32| ms[((ms.len() - 1) as f32 * q) as usize];
        let mean = ms.iter().sum::<f32>() / ms.len() as f32;
        println!(
            "bench: {} frames at {}x{}, GPU ms mean {:.2}, median {:.2}, p90 {:.2}, max {:.2}",
            ms.len(),
            b.size.0,
            b.size.1,
            mean,
            at(0.5),
            at(0.9),
            at(1.0)
        );
    }

    pub fn elapsed(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// A screenshot or benchmark run: everything it needs renders off screen, so its window stays
    /// hidden and never takes the focus from whatever the player is doing meanwhile.
    pub fn runs_hidden(&self) -> bool {
        self.shots_dir.is_some() || self.bench.is_some() || self.film.is_some()
    }

    /// Whether the simulation stays at `tick` (the next screenshot is taken there).
    pub fn holds(&self, tick: u32) -> bool {
        self.shots_dir.is_some() && self.shot_ticks.get(self.next_shot).is_some_and(|&t| tick >= t)
    }

    /// Path of the screenshot to take this frame, if one is due (`tick`: the simulation's).
    pub fn shot_due(&mut self, tick: u32) -> Option<PathBuf> {
        if let Some(f) = &mut self.film {
            let t = tick as f32 * physics::DT;
            if t < f.from || t > f.to {
                return None;
            }
            f.frames += 1;
            let _ = std::fs::create_dir_all(&f.dir);
            return Some(f.dir.join(format!("frame-{:05}.bmp", f.frames)));
        }
        let dir = self.shots_dir.as_ref()?;
        let name = if self.shot_ticks.is_empty() {
            let t = *self.shot_times.get(self.next_shot)?;
            if self.elapsed() < t {
                return None;
            }
            format!("shot-{:02}-{:.1}s.bmp", self.next_shot + 1, t)
        } else {
            let t = *self.shot_ticks.get(self.next_shot)?;
            if tick < t {
                return None;
            }
            // The chase camera eases toward the stopped car; it has settled after a second and a half.
            let now = self.elapsed();
            if now - *self.held_since.get_or_insert(now) < 1.5 {
                return None;
            }
            self.held_since = None;
            format!("shot-{:02}-tick{t}.bmp", self.next_shot + 1)
        };
        self.next_shot += 1;
        let _ = std::fs::create_dir_all(dir);
        Some(dir.join(name))
    }

    pub fn should_exit(&self) -> bool {
        let shots_done = self.shots_dir.is_some() && !self.shot_ticks.is_empty() && self.next_shot >= self.shot_ticks.len();
        let filmed = self.film.as_ref().is_some_and(|f| f.frames > 0 && f.frames as f32 >= (f.to - f.from) * f.fps);
        shots_done || filmed || self.exit_after.is_some_and(|t| self.elapsed() >= t)
    }

}

/// The point of the route `s` metres from its start (clamped to its ends).
pub fn route_point(route: &[Vec3], s: f32) -> Vec3 {
    let mut left = s.max(0.0);
    for w in route.windows(2) {
        let d = w[0].distance(w[1]);
        if left <= d {
            return w[0].lerp(w[1], if d > 0.0 { left / d } else { 0.0 });
        }
        left -= d;
    }
    route.last().copied().unwrap_or(Vec3::ZERO)
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
