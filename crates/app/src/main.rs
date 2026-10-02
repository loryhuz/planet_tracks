//! Planet Tracks (codename Mars Racer): a native window (winit + wgpu on Metal, on macOS and iOS)
//! opening on the menu, then running the deterministic physics at 100 Hz, rendering interpolated
//! between ticks, with an egui tuning panel.

mod audio;
mod camera;
mod car_model;
mod debug;
mod engine_sound;
mod game;
mod gfx;
mod input;
#[cfg(target_os = "ios")]
mod ios;
mod marks;
mod menu;
mod menu_gfx;
mod particles;
mod race;
mod sample;
mod session;
mod surfaces;
mod touch;
mod ui;
mod ui_sound;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
#[cfg(not(target_os = "ios"))]
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window, WindowId};

use crate::game::{CarMeshes, CornerMeshes, Game};
use crate::gfx::{Gpu, MeshData, SceneRenderer, Shading, Vertex, View};
use crate::input::Action;
use crate::menu::{Layout, Menu, MenuInput, Request};

struct Graphics {
    window: Arc<Window>,
    gpu: Gpu,
    scene: SceneRenderer,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    track_mesh: gfx::MeshId,
    car: CarMeshes,
    menu_gfx: menu_gfx::MenuRenderer,
}

struct App {
    gfx: Option<Graphics>,
    game: Game,
    menu: Menu,
    egui_ctx: egui::Context,
    last_frame: Instant,
    accumulator: f32,
    fps: ui::Fps,
    debug: debug::Debug,
    audio: Option<audio::Audio>,
    /// Sound starts once the game is running smoothly, not while the window, GPU and shaders
    /// are being set up (that busy start-up used to starve the audio thread once).
    audio_wanted: bool,
    frames_shown: u32,
}

/// Track mesh to GPU vertices. The vertex kind is the triangle's surface; the terrain shares its
/// vertices between dirt and ground triangles, so both draw as ground and the shader blends to
/// dirt with the vertex's dirt amount.
fn track_mesh_data(mesh: &track::TrackMesh) -> MeshData {
    let mut vertices: Vec<Vertex> = (0..mesh.positions.len())
        .map(|i| Vertex {
            pos: mesh.positions[i].to_array(),
            normal: mesh.normals.get(i).copied().unwrap_or(glam::Vec3::Y).to_array(),
            color: mesh.colors.get(i).copied().unwrap_or([0.5, 0.5, 0.5]),
            kind: 0,
            uv: mesh.uv.get(i).copied().unwrap_or([0.0, 0.0]),
            dirt: mesh.dirt.get(i).copied().unwrap_or(0.0),
        })
        .collect();
    for (t, surface) in mesh.tri_surface.iter().enumerate() {
        let kind = match surface {
            track::Surface::Dirt => track::Surface::Ground as u32,
            track::Surface::Wall => wall_kind(vertices[mesh.indices[3 * t] as usize].color),
            s => *s as u32,
        };
        for k in 0..3 {
            vertices[mesh.indices[3 * t + k] as usize].kind = kind;
        }
    }
    road_spill(&mut vertices);
    // Triangles grouped by the shader that draws them: roads, the rest, then the ground.
    let mut indices = Vec::with_capacity(mesh.indices.len());
    let mut parts = Vec::new();
    for shading in [Shading::Road, Shading::Other, Shading::Ground] {
        let first = indices.len() as u32;
        for t in mesh.indices.chunks_exact(3) {
            let s = match vertices[t[0] as usize].kind {
                k if k == track::Surface::Road as u32 => Shading::Road,
                k if k == track::Surface::Ground as u32 => Shading::Ground,
                _ => Shading::Other,
            };
            if s == shading {
                indices.extend_from_slice(t);
            }
        }
        parts.push((first..indices.len() as u32, shading));
    }
    MeshData { vertices, indices, parts }
}

/// Distance along the route over which a road takes on the earth of the dirt track it leads to,
/// metres.
const SPILL: f32 = 14.0;

/// Earth carried onto the roads next to dirt tracks: a road vertex's `dirt` grows from 0, `SPILL`
/// metres along the route from the nearest dirt floor, to 1 where it meets it, and the shader lays
/// that much dirt over the asphalt (where the road meets the dirt, both show the same surface).
fn road_spill(vertices: &mut [Vertex]) {
    let ground = track::Surface::Ground as u32;
    let road = track::Surface::Road as u32;
    let mut floor: Vec<f32> = vertices.iter().filter(|v| v.kind == ground && v.dirt >= 0.9).map(|v| v.uv[0]).collect();
    if floor.is_empty() {
        return;
    }
    floor.sort_by(f32::total_cmp);
    for v in vertices.iter_mut().filter(|v| v.kind == road && v.uv != [0.0, 0.0]) {
        let s = v.uv[0];
        let i = floor.partition_point(|&f| f < s);
        let d = [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|j| floor.get(j)).map(|&f| (f - s).abs()).fold(f32::MAX, f32::min);
        let t = (1.0 - d / SPILL).clamp(0.0, 1.0);
        v.dirt = t * t * (3.0 - 2.0 * t);
    }
}

/// What a wall is made of, told by the kit's colour it was given: concrete barriers and platform
/// sides, the dug earth of dirt jumps, painted gates (kept plain), and rocks (any other colour:
/// the scenery shades each rock its own way).
fn wall_kind(color: [f32; 3]) -> u32 {
    use track::kit::color as c;
    match color {
        x if x == c::LIP || x == c::WALL => gfx::kind::CONCRETE,
        x if x == c::EARTH_FACE => gfx::kind::EARTH,
        x if x == c::START || x == c::CHECKPOINT || x == c::FINISH => track::Surface::Wall as u32,
        _ => gfx::kind::ROCK,
    }
}

/// Uploads the buggy's body and every corner's parts.
fn upload_car(scene: &mut SceneRenderer, gpu: &Gpu) -> CarMeshes {
    let buggy = car_model::load();
    scene.set_livery(gpu, buggy.livery.width, buggy.livery.height, &buggy.livery.rgba);
    let device = &gpu.device;
    let mut up = |mesh: &MeshData| scene.upload(device, mesh);
    let body = up(&buggy.body);
    let corners = buggy
        .parts
        .iter()
        .map(|p| CornerMeshes {
            arm_lo: up(&p.arm_lo),
            arm_up: up(&p.arm_up),
            upright: up(&p.upright),
            wheel: up(&p.wheel),
            damper: up(&p.damper),
            rod: up(&p.rod),
            spring: up(&p.spring),
            tierod: p.tierod.as_ref().map(&mut up),
        })
        .collect();
    CarMeshes { body, corners, rigs: buggy.rigs, wheel_radius: buggy.wheel_radius }
}

impl App {
    fn frame(&mut self) {
        let Some(g) = self.gfx.as_mut() else { return };
        let game = &mut self.game;

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.25);
        self.last_frame = now;

        game.controls.poll();
        let navs = game.controls.take_nav();
        for action in game.controls.take_actions() {
            match action {
                // In the menu the keys move through it; only the window and the sound keys stay.
                _ if self.menu.active => {
                    if matches!(action, Action::Fullscreen | Action::Mute) {
                        game.apply(action);
                    }
                }
                Action::Menu => {
                    game.restart();
                    game.controls.touch.clear();
                    self.menu.open_from_race(game.map_index);
                }
                _ => game.apply(action),
            }
        }
        if self.menu.active {
            for n in navs {
                self.menu.push_nav(n);
            }
        }
        if std::mem::take(&mut game.fullscreen_requested) {
            let full = g.window.fullscreen().is_some();
            g.window.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
        }

        // The race waits while the menu is up.
        self.accumulator = if self.menu.active { 0.0 } else { self.accumulator + dt };
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
        let benching = self.debug.benching();
        let surface_texture = match g.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let size = gfx::window_pixels(&g.window);
                g.gpu.resize(size.width, size.height);
                None
            }
            _ => None,
        };
        if surface_texture.is_none() && shot_path.is_none() && !benching {
            // Hidden or minimised: keep simulating and keep the camera following (so a later
            // screenshot shows the usual view), but do not spin the CPU.
            let (pos, rot) = game.car_pose(alpha);
            let t = game.telemetry();
            let aspect = g.gpu.config.width as f32 / g.gpu.config.height.max(1) as f32;
            let _ = game.camera.update(dt, pos, rot, t.speed_kmh, t.airborne, aspect);
            std::thread::sleep(std::time::Duration::from_millis(8));
            return;
        }
        let offscreen = (shot_path.is_some() || benching).then(|| {
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

        // The menu is laid out in a fixed design space that egui's zoom fits to the window (on iOS,
        // its inner size is the safe area, which the menu keeps to).
        let size = g.window.inner_size();
        let scale = g.window.scale_factor() as f32;
        let zoom = if self.menu.shows() { Layout::zoom(size.width as f32 / scale, size.height as f32 / scale) } else { 1.0 };
        if (self.egui_ctx.zoom_factor() - zoom).abs() > 1e-4 {
            // Straight into the options, so this frame is laid out with it (`set_zoom_factor`
            // waits for the next one).
            self.egui_ctx.options_mut(|o| o.zoom_factor = zoom);
        }

        // UI first: it can change the profile or the tuning.
        let mut raw = g.egui_state.take_egui_input(&g.window);
        #[cfg(target_os = "ios")]
        {
            raw.safe_area_insets = Some(ios::safe_area(&g.window, zoom));
        }
        // A hidden window (self-tests) reports no screen size: take the surface's.
        let screen = egui::vec2(g.gpu.config.width as f32, g.gpu.config.height as f32) / (scale * zoom);
        if raw.screen_rect.is_none_or(|r| (r.size() - screen).length() > 1.0) {
            raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen));
        }
        let fps = &self.fps;
        let menu = &mut self.menu;
        menu.touch = game.controls.touch.active;
        let bests: Vec<Option<u32>> =
            game.maps.iter().map(|m| game.session.profile().best(&session::map_key(m)).map(|b| b.ticks)).collect();
        let muted = self.audio.as_ref().is_some_and(|a| a.muted());
        let mut sky = None;
        let mut full = self.egui_ctx.run_ui(raw, |ui| {
            if menu.shows() {
                sky = Some(menu.ui(ui, MenuInput { bests: &bests, muted }).clone());
            }
            if !menu.shows() {
                ui::draw(ui, game, fps);
            }
        });
        g.egui_state.handle_platform_output(&g.window, full.platform_output);
        let prims = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
        for request in self.menu.take_requests() {
            match request {
                Request::Build(map) if map != game.map_index => game.select_map(map),
                Request::Build(_) => {}
                Request::Start(_) => game.restart(),
                Request::ToggleMute => game.mute_requested = true,
            }
        }
        game.session.autosave();

        if std::mem::take(&mut game.track_changed) {
            g.scene.replace(&g.gpu.device, g.track_mesh, &track_mesh_data(&game.track.mesh));
            g.scene.set_track(&game.track);
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
        if let Some((s, height, back)) = self.debug.view {
            let at = debug::route_point(&game.track.route, s);
            eye = debug::route_point(&game.track.route, s - back) + glam::Vec3::Y * height;
            view = glam::camera::rh::view::look_at_mat4(eye, at, glam::Vec3::Y);
        }
        let mut items = vec![gfx::DrawItem {
            mesh: g.track_mesh,
            model: glam::Mat4::IDENTITY,
            tint: glam::Vec4::ONE,
            cast_shadow: true,
        }];
        items.extend(game.draw_items(alpha, &g.car));
        if let Some(audio) = &self.audio {
            audio.set_scene(!self.menu.active, self.menu.ambience());
            for cue in self.menu.take_cues() {
                audio.cue(cue);
            }
            audio.update(&if self.menu.active { audio::SoundFrame::default() } else { game.sound_frame() });
            for strength in game.impacts.drain(..) {
                audio.impact(strength);
            }
            if std::mem::take(&mut game.mute_requested) {
                audio.toggle_mute();
            }
        } else {
            game.impacts.clear();
            self.menu.take_cues();
        }
        let right = view.row(0).truncate();
        let up = view.row(1).truncate();
        g.scene.write_dust(&g.gpu.queue, &game.dust.vertices(right, up));
        let clear = std::mem::take(&mut game.marks.cleared);
        g.scene.write_marks(&g.gpu.queue, clear, game.marks.take_pending());
        g.scene.textures = game.session.textures;
        match (&sky, self.menu.active) {
            (Some(sky), true) => g.menu_gfx.render(&g.gpu, &mut encoder, &target, sky, full.pixels_per_point),
            _ => g.scene.render(&g.gpu, &mut encoder, &target, &View { view, proj, eye, focus: car_pos }, &items),
        }

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
        let submitted = Instant::now();
        g.gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        if benching {
            let _ = g.gpu.device.poll(wgpu::PollType::wait_indefinitely());
            let size = (g.gpu.config.width, g.gpu.config.height);
            self.debug.bench_sample(submitted.elapsed().as_secs_f32() * 1000.0, size);
        }
        if let Some((path, (buffer, w, h, row))) = shot {
            let bgra = matches!(g.gpu.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            debug::save_bmp(&g.gpu.device, &buffer, w, h, row, bgra, &path);
        }
        // A frame spent on a screenshot is not shown; the next one is.
        if let (None, Some(st)) = (offscreen, surface_texture) {
            g.gpu.queue.present(st);
            self.fps.frame();
            self.frames_shown += 1;
            if self.audio.is_none() && self.audio_wanted && self.frames_shown >= 30 {
                self.audio = audio::Audio::new();
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            // Back from the background (iOS): the race carries on from here.
            self.last_frame = Instant::now();
            return;
        }
        let attrs = Window::default_attributes().with_title("Planet Tracks").with_visible(!self.debug.runs_hidden());
        // On iOS the window is the screen.
        #[cfg(not(target_os = "ios"))]
        let attrs = attrs.with_inner_size(window_size());
        // Full screen without the status bar; swipes from the edges reach the game first (a
        // second swipe opens the Control Centre).
        #[cfg(target_os = "ios")]
        let attrs = {
            use winit::platform::ios::{ScreenEdge, WindowAttributesExtIOS};
            attrs
                .with_prefers_status_bar_hidden(true)
                .with_prefers_home_indicator_hidden(true)
                .with_preferred_screen_edges_deferring_system_gestures(ScreenEdge::ALL)
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        // In the app's scene, then drawn at twice the points rather than the 3× of recent
        // iPhones: hardly visible, and far lighter (set once the view is shown: UIKit resets a
        // scale given before).
        #[cfg(target_os = "ios")]
        {
            use winit::platform::ios::{ScreenEdge, WindowExtIOS};
            ios::attach_to_scene(&window);
            window.set_scale_factor(2.0);
            // Asked again now that the window is in the scene: winit asked when it made the
            // window, outside any scene, and iOS ignored it (taps along the edges waited for a
            // system gesture, and quick ones were lost).
            window.set_preferred_screen_edges_deferring_system_gestures(ScreenEdge::ALL);
            window.set_prefers_home_indicator_hidden(true);
            window.set_prefers_status_bar_hidden(true);
        }
        let gpu = Gpu::new(window.clone());
        let mut scene = SceneRenderer::new(&gpu);
        let track_mesh = scene.upload(&gpu.device, &track_mesh_data(&self.game.track.mesh));
        scene.set_track(&self.game.track);
        let car = upload_car(&mut scene, &gpu);
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
        let menu_gfx = menu_gfx::MenuRenderer::new(&gpu);
        self.last_frame = Instant::now();
        self.gfx = Some(Graphics { window, gpu, scene, egui_state, egui_renderer, track_mesh, car, menu_gfx });
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
        // Every finger goes to the touch controls (egui follows only one, for the menu and the
        // settings panel).
        if let WindowEvent::Touch(t) = &event {
            let pos = egui::pos2(t.location.x as f32, t.location.y as f32);
            self.game.controls.touch.touch(t.id, t.phase, pos, !self.menu.shows());
        }
        let _ = g.egui_state.on_window_event(&g.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                self.game.session.save();
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                let size = gfx::window_pixels(&g.window);
                g.gpu.resize(size.width, size.height);
            }
            WindowEvent::Focused(false) => self.game.controls.clear(),
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    /// iOS: the app leaves the foreground, and may be closed from there without notice.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.game.controls.clear();
        self.game.session.save();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.debug.bench_report();
        if self.debug.should_exit() {
            self.game.session.save();
            event_loop.exit();
            return;
        }
        if self.debug.runs_hidden() {
            // A hidden window gets no redraw requests: the loop runs the frames itself.
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            if self.gfx.is_some() {
                self.frame();
            }
        } else if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

/// The window's logical size: 1600 × 900, or `MARS_WINDOW=WxH` (a portrait size shows the phone
/// layout of the menu).
#[cfg(not(target_os = "ios"))]
fn window_size() -> LogicalSize<f64> {
    std::env::var("MARS_WINDOW")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some(LogicalSize::new(w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(LogicalSize::new(1600.0, 900.0))
}

/// `MARS_HEADLESS=1`: the autopilot drives every profile without a window and prints the
/// race events (technical check: the track is completed, nothing falls through).
fn headless() {
    let mut game = Game::new();
    if let Ok(name) = std::env::var("MARS_MAP") {
        if let Some(i) = game.maps.iter().position(|m| m.name.eq_ignore_ascii_case(&name)) {
            game.select_map(i);
        }
    }
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

/// `MARS_ENGINE_DEMO=path.wav`: the car's sound over a scripted lap, at the game's level: idling
/// on the line, pulling away through the gears to 300 km/h, a lift, cruising on dirt, braking,
/// then full throttle again.
fn engine_demo(path: &str) {
    let params = physics::presets().remove(0);
    let gear_of = |kmh: f32| {
        let mut lo = 0.0;
        let mut hi = params.top_speed_kmh;
        let mut gear = 0u32;
        for &s in &params.accel_speeds {
            if kmh < s {
                hi = s;
                break;
            }
            lo = s;
            gear += 1;
        }
        (gear, ((kmh - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0))
    };
    // (seconds, speed km/h at the end, throttle, share of the wheels on dirt)
    let script = [(2.0, 0.0, 0.0, 0.0), (9.0, 300.0, 1.0, 0.0), (1.5, 280.0, 0.0, 0.0), (3.0, 150.0, 0.3, 1.0), (1.5, 90.0, 0.0, 1.0), (5.0, 260.0, 1.0, 0.0)];
    let mut frames = Vec::new();
    let mut kmh = 0.0f32;
    for (seconds, end, load, dirt) in script {
        let n = (seconds * 100.0) as usize;
        let start = kmh;
        for i in 0..n {
            // Accelerations ease off with speed, as in the car.
            let t = i as f32 / n as f32;
            let shape = if end > start { 1.0 - (1.0 - t).powf(1.6) } else { t };
            kmh = start + (end - start) * shape;
            let (gear, rpm) = gear_of(kmh);
            frames.push(audio::SoundFrame { rpm, gear, load, speed: kmh / 3.6, gravel: dirt, ..Default::default() });
        }
    }
    let rate = 44_100u32;
    write_wav(std::path::Path::new(path), &audio::render_offline(&frames, &[], rate), rate);
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
    let debug = debug::Debug::from_env();
    let mut builder = EventLoop::builder();
    // A screenshot or benchmark run leaves the screen and the keyboard to whatever the player is
    // doing meanwhile (its window stays hidden too).
    #[cfg(target_os = "macos")]
    if debug.runs_hidden() {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_activate_ignoring_other_apps(false);
    }
    let event_loop = builder.build().expect("event loop");
    let mut game = Game::new();
    // A self-test run leaves the player's session (profiles, records, settings) untouched.
    game.session.persist = !debug.runs_hidden();
    if let Ok(name) = std::env::var("MARS_MAP") {
        if let Some(i) = game.maps.iter().position(|m| m.name.eq_ignore_ascii_case(&name)) {
            game.select_map(i);
        }
    }
    if let Some(p) = debug.profile {
        game.select_profile(p);
    }
    // The tuning panel starts closed on a phone (its button opens it), and the wide-angle camera
    // shows the sides of the road on its narrow screen.
    if std::env::var("MARS_HIDE_UI").is_ok() || game.controls.touch.active {
        game.panel_open = false;
    }
    if game.controls.touch.active {
        game.camera.mode = camera::WIDE;
    }
    // Surface textures on (1) or off (0) for this run, whatever the session says.
    if let Ok(v) = std::env::var("MARS_TEXTURES") {
        game.session.textures = v != "0";
    }
    if debug.autodrive {
        game.autodrive = Some(debug::Autopilot::default());
    }
    // The game opens on the menu; self-tests of the race (a map, a profile, a view or the
    // autopilot asked for) start driving at once. `MARS_MENU=title|planets|modes|solo` opens the
    // menu on that screen, for its own checks.
    let mut menu = Menu::new(&game.maps);
    let race_test = ["MARS_MAP", "MARS_PROFILE", "MARS_VIEW", "MARS_ORBIT", "MARS_AUTODRIVE", "MARS_BENCH"]
        .iter()
        .any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()));
    match std::env::var("MARS_MENU").ok().filter(|v| !v.is_empty()) {
        Some(screen) => menu.open_on(&screen),
        None if race_test => menu.active = false,
        None => {}
    }
    let egui_ctx = egui::Context::default();
    egui_ctx.set_fonts(menu::fonts());
    let mut app = App {
        gfx: None,
        game,
        menu,
        egui_ctx,
        last_frame: Instant::now(),
        accumulator: 0.0,
        fps: ui::Fps::new(),
        audio: None,
        audio_wanted: std::env::var("MARS_NO_AUDIO").is_err(),
        frames_shown: 0,
        debug,
    };
    event_loop.run_app(&mut app).expect("run");
}
