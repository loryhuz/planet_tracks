//! Mars Racer demo: a native macOS window (winit + wgpu on Metal) running the deterministic
//! physics at 100 Hz, rendering interpolated between ticks, with an egui tuning panel.

mod audio;
mod camera;
mod car_model;
mod debug;
mod engine_sound;
mod game;
mod gfx;
mod input;
mod marks;
mod particles;
mod race;
mod session;
mod ui;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window, WindowId};

use crate::game::{CarMeshes, Game};
use crate::gfx::{Gpu, MeshData, SceneRenderer, Vertex, View};

struct Graphics {
    window: Arc<Window>,
    gpu: Gpu,
    scene: SceneRenderer,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    track_mesh: gfx::MeshId,
    car: CarMeshes,
    /// Car geometry the meshes were built for; rebuilt when the tuning changes it.
    car_key: [f32; 5],
}

struct App {
    gfx: Option<Graphics>,
    game: Game,
    egui_ctx: egui::Context,
    last_frame: Instant,
    accumulator: f32,
    fps: ui::Fps,
    debug: debug::Debug,
    audio: Option<audio::Audio>,
}

/// Track mesh to GPU vertices; the vertex kind is the triangle's surface (vertices are never
/// shared across surfaces).
fn track_mesh_data(mesh: &track::TrackMesh) -> MeshData {
    let mut vertices: Vec<Vertex> = (0..mesh.positions.len())
        .map(|i| Vertex {
            pos: mesh.positions[i].to_array(),
            normal: mesh.normals.get(i).copied().unwrap_or(glam::Vec3::Y).to_array(),
            color: mesh.colors.get(i).copied().unwrap_or([0.5, 0.5, 0.5]),
            kind: 0,
        })
        .collect();
    for (t, surface) in mesh.tri_surface.iter().enumerate() {
        for k in 0..3 {
            vertices[mesh.indices[3 * t + k] as usize].kind = *surface as u32;
        }
    }
    MeshData { vertices, indices: mesh.indices.clone() }
}

fn car_key(p: &physics::CarParams) -> [f32; 5] {
    [p.wheel_radius, p.wheelbase, p.track_width, p.rest_suspension(), p.wheel_anchors()[0].z]
}

impl App {
    fn frame(&mut self) {
        let Some(g) = self.gfx.as_mut() else { return };
        let game = &mut self.game;

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.25);
        self.last_frame = now;

        game.controls.poll();
        for action in game.controls.take_actions() {
            game.apply(action);
        }
        if std::mem::take(&mut game.fullscreen_requested) {
            let full = g.window.fullscreen().is_some();
            g.window.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
        }

        self.accumulator += dt;
        let mut ticks = 0;
        while self.accumulator >= physics::DT && ticks < 25 {
            game.tick();
            self.accumulator -= physics::DT;
            ticks += 1;
        }
        if ticks == 25 {
            self.accumulator = 0.0;
        }
        let alpha = (self.accumulator / physics::DT).clamp(0.0, 1.0);

        // Acquire the frame before running the UI, so egui's texture updates are never dropped.
        // A due screenshot renders off screen, so it works even when the window is hidden.
        let shot_path = self.debug.shot_due();
        let surface_texture = match g.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let size = g.window.inner_size();
                g.gpu.resize(size.width, size.height);
                None
            }
            _ => None,
        };
        if surface_texture.is_none() && shot_path.is_none() {
            // Hidden or minimised: keep simulating, but do not spin the CPU.
            std::thread::sleep(std::time::Duration::from_millis(8));
            return;
        }
        let offscreen = shot_path.as_ref().map(|_| {
            g.gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("screenshot target"),
                size: wgpu::Extent3d { width: g.gpu.config.width, height: g.gpu.config.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: g.gpu.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let target_texture = match (&offscreen, &surface_texture) {
            (Some(t), _) => t,
            (None, Some(st)) => &st.texture,
            (None, None) => return,
        };

        // UI first: it can change the profile or the tuning.
        let raw = g.egui_state.take_egui_input(&g.window);
        let fps = &self.fps;
        let mut full = self.egui_ctx.run_ui(raw, |ui| ui::draw(ui, game, fps));
        g.egui_state.handle_platform_output(&g.window, full.platform_output);
        let prims = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
        game.session.autosave();

        let key = car_key(&game.run.car.params);
        if key != g.car_key {
            let p = &game.run.car.params;
            g.scene.replace(&g.gpu.device, g.car.body, &car_model::body(p));
            g.scene.replace(&g.gpu.device, g.car.wheel, &car_model::wheel(p.wheel_radius));
            g.car_key = key;
        }

        let target = target_texture.create_view(&Default::default());
        let mut encoder = g.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        let (car_pos, car_rot) = game.car_pose(alpha);
        let t = game.telemetry();
        let aspect = g.gpu.config.width as f32 / g.gpu.config.height.max(1) as f32;
        let (mut view, proj, mut eye) = game.camera.update(dt, car_pos, car_rot, t.speed_kmh, t.airborne, aspect);
        if let Some((yaw, dist, height)) = self.debug.orbit {
            let dir = car_rot * glam::Quat::from_rotation_y(yaw.to_radians()) * glam::Vec3::Z;
            eye = car_pos + dir * dist + glam::Vec3::Y * height;
            view = glam::camera::rh::view::look_at_mat4(eye, car_pos + glam::Vec3::Y * 0.3, glam::Vec3::Y);
        }
        let mut items = vec![gfx::DrawItem {
            mesh: g.track_mesh,
            model: glam::Mat4::IDENTITY,
            tint: glam::Vec4::ONE,
            cast_shadow: true,
        }];
        items.extend(game.draw_items(alpha, &g.car));
        if let Some(audio) = &self.audio {
            audio.update(&game.sound_frame());
            for strength in game.impacts.drain(..) {
                audio.impact(strength);
            }
            if std::mem::take(&mut game.mute_requested) {
                audio.toggle_mute();
            }
            if std::mem::take(&mut game.engine_requested) {
                game.engine_name = audio.next_engine();
            }
        } else {
            game.impacts.clear();
        }
        let right = view.row(0).truncate();
        let up = view.row(1).truncate();
        g.scene.write_dust(&g.gpu.queue, &game.dust.vertices(right, up));
        let clear = std::mem::take(&mut game.marks.cleared);
        g.scene.write_marks(&g.gpu.queue, clear, game.marks.take_pending());
        g.scene.render(&g.gpu, &mut encoder, &target, &View { view, proj, eye, focus: car_pos }, &items);

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [g.gpu.config.width, g.gpu.config.height],
            pixels_per_point: full.pixels_per_point,
        };
        for (id, deltas) in &full.textures_delta.set {
            for delta in deltas {
                g.egui_renderer.update_texture(&g.gpu.device, &g.gpu.queue, *id, delta);
            }
        }
        let extra = g.egui_renderer.update_buffers(&g.gpu.device, &g.gpu.queue, &mut encoder, &prims, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            g.egui_renderer.render(&mut pass.forget_lifetime(), &prims, &screen);
        }
        for id in &full.textures_delta.free {
            g.egui_renderer.free_texture(id);
        }
        full.textures_delta.clear();
        let shot = shot_path.map(|path| (path, debug::copy_frame(&g.gpu.device, &mut encoder, target_texture)));
        g.gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        if let Some((path, (buffer, w, h, row))) = shot {
            let bgra = matches!(g.gpu.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            debug::save_bmp(&g.gpu.device, &buffer, w, h, row, bgra, &path);
        }
        // A frame spent on a screenshot is not shown; the next one is.
        if let (None, Some(st)) = (offscreen, surface_texture) {
            g.gpu.queue.present(st);
            self.fps.frame();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Mars Racer · démo")
            .with_inner_size(LogicalSize::new(1600.0, 900.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let gpu = Gpu::new(window.clone());
        let mut scene = SceneRenderer::new(&gpu);
        let track_mesh = scene.upload(&gpu.device, &track_mesh_data(&self.game.track.mesh));
        let p = &self.game.run.car.params;
        let car = CarMeshes {
            body: scene.upload(&gpu.device, &car_model::body(p)),
            wheel: scene.upload(&gpu.device, &car_model::wheel(p.wheel_radius)),
            arm: scene.upload(&gpu.device, &car_model::arm()),
        };
        let car_key = car_key(p);
        let max_texture = gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            None,
            Some(max_texture),
        );
        let egui_renderer = egui_wgpu::Renderer::new(&gpu.device, gpu.config.format, Default::default());
        self.last_frame = Instant::now();
        self.gfx = Some(Graphics { window, gpu, scene, egui_state, egui_renderer, track_mesh, car, car_key });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gfx.as_mut() else { return };
        // Keyboard goes to the game only, so sliders never steal the arrow keys.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            if let PhysicalKey::Code(code) = key.physical_key {
                self.game.controls.key(code, key.state.is_pressed(), key.repeat);
            }
            return;
        }
        let _ = g.egui_state.on_window_event(&g.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                self.game.session.save();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => g.gpu.resize(size.width, size.height),
            WindowEvent::Focused(false) => self.game.controls.clear(),
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.debug.should_exit() {
            self.game.session.save();
            event_loop.exit();
            return;
        }
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

/// `MARS_HEADLESS=1`: the autopilot drives every profile without a window and prints the
/// race events (technical check: the track is completed, nothing falls through).
fn headless() {
    let mut game = Game::new();
    game.autodrive = Some(debug::Autopilot::default());
    for i in 0..game.session.profiles.len() {
        game.select_profile(i);
        let name = game.session.profile().params.name.clone();
        let mut ticks = 0;
        while game.run.finished.is_none() && ticks < 150 + 100 * 150 {
            game.tick();
            ticks += 1;
            if std::env::var("MARS_VERBOSE").is_ok() && ticks % 200 == 0 {
                let w = &game.run.car.state.wheels[2];
                println!("  t={} dust={} rear-left surface={:?} contact={} mark={:.2} smear={:.2}", ticks, game.dust.len(), w.surface, w.contact, w.mark, w.smear);
            }
        }
        if std::env::var("MARS_VERBOSE").is_ok() {
            println!("  dust particles at the end: {}", game.dust.len());
        }
        let splits: Vec<String> = game.run.splits.iter().map(|&t| race::format_time(t)).collect();
        match game.run.finished {
            Some(t) => println!("{name}: arrivée en {} · CP {:?} · respawns {}", race::format_time(t), splits, game.run.respawns),
            None => println!("{name}: PAS D'ARRIVÉE · CP {:?} · respawns {} · position {:?}", splits, game.run.respawns, game.run.car.state.position),
        }
    }
}

/// `MARS_AUDIO_WAV=path`: drives profile 1 with the autopilot for 30 s and writes the
/// synthesized sound to a WAV file (to check the audio without playing it).
fn audio_wav(path: &str) {
    let mut game = Game::new();
    game.autodrive = Some(debug::Autopilot::default());
    game.select_profile(0);
    let (mut frames, mut impacts) = (Vec::new(), Vec::new());
    for i in 0..3000 {
        game.tick();
        frames.push(game.sound_frame());
        for s in game.impacts.drain(..) {
            impacts.push((i, s));
        }
    }
    let rate = 44_100u32;
    let samples = audio::render_offline(&frames, &impacts, rate);
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
    println!("{} impacts", impacts.len());
}

fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) {
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

/// `MARS_ENGINE_DEMO=dir`: one WAV per engine character (idle, three full-throttle gears,
/// lift-off with pops, cruise), to choose an engine by ear.
fn engine_demo(dir: &str) {
    let rate = 44_100u32;
    let _ = std::fs::create_dir_all(dir);
    for kind in engine_sound::KINDS {
        let mut voice = engine_sound::EngineVoice::new(kind, rate as f32);
        let mut out = Vec::new();
        let mut push = |seconds: f32, f: &dyn Fn(f32) -> (f32, f32), voice: &mut engine_sound::EngineVoice| {
            let n = (seconds * rate as f32) as usize;
            for i in 0..n {
                let (rpm, load) = f(i as f32 / n as f32);
                out.push(voice.next(rpm, load));
            }
        };
        push(1.5, &|_| (0.0, 0.08), &mut voice);
        push(2.0, &|t| (0.15 + 0.8 * t, 1.0), &mut voice);
        push(2.2, &|t| (0.5 + 0.45 * t, 1.0), &mut voice);
        push(2.6, &|t| (0.55 + 0.42 * t, 1.0), &mut voice);
        push(1.8, &|t| (0.97 - 0.5 * t, 0.0), &mut voice);
        push(2.0, &|_| (0.5, 0.45), &mut voice);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
        for s in &mut out {
            *s *= 0.8 / peak;
        }
        write_wav(&std::path::Path::new(dir).join(format!("moteur-{}.wav", kind.name)), &out, rate);
    }
}

fn main() {
    if let Ok(dir) = std::env::var("MARS_ENGINE_DEMO") {
        engine_demo(&dir);
        return;
    }
    if let Ok(path) = std::env::var("MARS_AUDIO_WAV") {
        audio_wav(&path);
        return;
    }
    if std::env::var("MARS_HEADLESS").is_ok() {
        headless();
        return;
    }
    let event_loop = EventLoop::new().expect("event loop");
    let debug = debug::Debug::from_env();
    let mut game = Game::new();
    if let Some(p) = debug.profile {
        game.select_profile(p);
    }
    if debug.autodrive {
        game.autodrive = Some(debug::Autopilot::default());
    }
    let mut app = App {
        gfx: None,
        game,
        egui_ctx: egui::Context::default(),
        last_frame: Instant::now(),
        accumulator: 0.0,
        fps: ui::Fps::new(),
        audio: if std::env::var("MARS_NO_AUDIO").is_ok() { None } else { audio::Audio::new() },
        debug,
    };
    event_loop.run_app(&mut app).expect("run");
}
