//! The track editor's executable (`cargo run --bin mars-editor`): no menu, it opens on the
//! editor ([`editor`]); Enter (or "Jouer") races the draft from its start, Escape (or the race
//! settings' "Menu") comes back to it. It shares the game's modules; some of them it leaves
//! unused (the menu's screens and film).

#![allow(dead_code)]

mod audio;
mod blur;
mod camera;
mod car_model;
mod debug;
mod editor;
mod engine_sound;
mod game;
mod gfx;
mod hud;
mod input;
#[cfg(target_os = "ios")]
mod ios;
mod marks;
mod menu;
mod menu_gfx;
mod music;
mod particles;
mod race;
mod sample;
mod scene_data;
mod session;
mod surfaces;
mod ui;
mod ui_sound;
mod video;
mod weather;

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
#[cfg(not(target_os = "ios"))]
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::game::{CarMeshes, Game};
use crate::gfx::{Gpu, SceneRenderer, View};
use crate::hud::HudRequest;
use crate::input::Action;
use crate::menu::Layout;
use crate::scene_data::{track_render_data, upload_car};

/// Played on a touch screen: the race opens on the wide-angle camera.
const TOUCH_SCREEN: bool = cfg!(any(target_os = "ios", target_os = "android"));

struct Graphics {
    window: Arc<Window>,
    gpu: Gpu,
    scene: SceneRenderer,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    track_mesh: gfx::MeshId,
    car: CarMeshes,
    ski_car: CarMeshes,
    ski_livery: bool,
}

impl Graphics {
    fn car_for(&mut self, skis: bool) -> &CarMeshes {
        if skis != self.ski_livery {
            let livery = car_model::livery(skis);
            self.scene.set_livery(&self.gpu, livery.width, livery.height, &livery.rgba);
            self.ski_livery = skis;
        }
        if skis { &self.ski_car } else { &self.car }
    }
}

struct App {
    gfx: Option<Graphics>,
    game: Game,
    editor: editor::Editor,
    /// Racing the draft (otherwise editing it).
    racing: bool,
    hud: hud::Hud,
    egui_ctx: egui::Context,
    last_frame: Instant,
    accumulator: f32,
    fps: ui::Fps,
    debug: debug::Debug,
    audio: Option<audio::Audio>,
    audio_wanted: bool,
    frames_shown: u32,
    modifiers: ModifiersState,
}

impl App {
    fn back_to_editor(&mut self) {
        self.racing = false;
        self.hud.close_sheet();
        self.game.controls.clear();
    }

    fn frame(&mut self) {
        let Some(g) = self.gfx.as_mut() else { return };
        let game = &mut self.game;
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.25);
        self.last_frame = now;

        game.controls.poll();
        let navs = game.controls.take_nav();
        let mut leave = false;
        for action in game.controls.take_actions() {
            match action {
                _ if !self.racing => {}
                Action::Menu => leave = true,
                _ if self.hud.paused() => {
                    if matches!(action, Action::Fullscreen | Action::Mute | Action::TogglePanel) {
                        game.apply(action);
                    }
                }
                _ => game.apply(action),
            }
        }
        if self.racing && self.hud.paused() {
            for n in navs {
                self.hud.push_nav(n);
            }
        }
        if std::mem::take(&mut game.fullscreen_requested) {
            let full = g.window.fullscreen().is_some();
            g.window.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
        }

        let paused = !self.racing || self.hud.paused();
        self.accumulator = if paused { 0.0 } else { self.accumulator + dt };
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

        let shot_path = self.debug.shot_due(game.run.tick);
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

        // The editor and the HUD are laid out in the menu's design space.
        let size = g.window.inner_size();
        let scale = g.window.scale_factor() as f32;
        let zoom = Layout::zoom(size.width as f32 / scale, size.height as f32 / scale);
        if (self.egui_ctx.zoom_factor() - zoom).abs() > 1e-4 {
            self.egui_ctx.options_mut(|o| o.zoom_factor = zoom);
        }
        let mut raw = g.egui_state.take_egui_input(&g.window);
        #[cfg(target_os = "ios")]
        {
            raw.safe_area_insets = Some(ios::safe_area(&g.window, zoom));
        }
        if let Some([top, right, bottom, left]) = self.debug.safe_area {
            let k = 1.0 / zoom;
            raw.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 { left: left * k, right: right * k, top: top * k, bottom: bottom * k }));
        }
        let screen = egui::vec2(g.gpu.config.width as f32, g.gpu.config.height as f32) / (scale * zoom);
        if raw.screen_rect.is_none_or(|r| (r.size() - screen).length() > 1.0) {
            raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen));
        }
        let racing = self.racing;
        let editor = &mut self.editor;
        let hud = &mut self.hud;
        let fps = &self.fps;
        let muted = self.audio.as_ref().is_some_and(|a| a.muted());
        let mut full = self.egui_ctx.run_ui(raw, |ui| {
            if racing {
                hud.ui(ui, game, fps, muted);
                if game.panel_open {
                    let t = game.telemetry();
                    ui::panel(ui.ctx(), game, t);
                }
            } else {
                editor.ui(ui);
            }
        });
        g.egui_state.handle_platform_output(&g.window, full.platform_output);
        let prims = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);

        for request in self.editor.take_requests() {
            match request {
                editor::Request::Play(map) => {
                    game.race_map(map);
                    self.hud = hud::Hud::new(&game.maps);
                    self.racing = true;
                    self.accumulator = 0.0;
                }
            }
        }
        for request in self.hud.take_requests() {
            match request {
                HudRequest::Respawn => game.apply(Action::Respawn),
                HudRequest::Restart => game.restart(),
                HudRequest::Menu => leave = true,
            }
        }
        if std::mem::take(&mut game.track_changed) {
            g.scene.replace(&g.gpu.device, g.track_mesh, &track_render_data(&game.track));
            g.scene.set_track(&g.gpu, &game.track, g.track_mesh, game.time_of_day(), game.maps[game.map_index].planet);
        }

        let target = target_texture.create_view(&Default::default());
        let mut encoder = g.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let skis = game.run.car.params.front_skis;
        if self.racing {
            let (car_pos, car_rot) = game.car_pose(alpha);
            let t = game.telemetry();
            let aspect = g.gpu.config.width as f32 / g.gpu.config.height.max(1) as f32;
            let (view, proj, eye) = game.camera.update(dt, car_pos, car_rot, t.speed_kmh, t.boost, t.airborne, aspect);
            // The settings sheet slides the scene up, as in the game.
            let shift = self.hud.car_focus().map_or(0.0, |(k, y)| {
                let c = proj * view * (car_pos + glam::Vec3::Y * 0.6).extend(1.0);
                k * ((1.0 - 2.0 * y) - c.y / c.w.max(1e-3))
            });
            let proj = glam::Mat4::from_translation(glam::Vec3::new(0.0, shift, 0.0)) * proj;
            let mut items = vec![gfx::DrawItem { mesh: g.track_mesh, model: glam::Mat4::IDENTITY, tint: glam::Vec4::ONE, cast_shadow: true, tyre: None, coat: None }];
            items.extend(game.draw_items(alpha, g.car_for(skis)));
            let right = view.row(0).truncate();
            let up = view.row(1).truncate();
            g.scene.write_dust(&g.gpu.queue, &game.dust.vertices(right, up));
            let clear = std::mem::take(&mut game.marks.cleared);
            g.scene.write_marks(&g.gpu.queue, clear, game.marks.take_pending());
            let blur = if self.hud.paused() { 0.0 } else { game.camera.blur };
            let headlights = Some(game.headlights(alpha, g.car_for(skis)));
            g.scene.render(&g.gpu, &mut encoder, &target, &View { view, proj, eye, blur, headlights }, &items);
        }
        if let Some(audio) = &self.audio {
            audio.set_scene(self.racing, game.time_of_day() == track::map::TimeOfDay::Night);
            for cue in self.hud.take_cues() {
                audio.cue(cue);
            }
            audio.update(&if !self.racing || self.hud.paused() { audio::SoundFrame::default() } else { game.sound_frame() });
            for strength in game.impacts.drain(..) {
                audio.impact(strength);
            }
            for _ in 0..std::mem::take(&mut game.boosts) {
                audio.boost();
            }
            if std::mem::take(&mut game.mute_requested) {
                audio.toggle_mute();
            }
        } else {
            game.impacts.clear();
            game.boosts = 0;
            self.hud.take_cues();
        }

        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [g.gpu.config.width, g.gpu.config.height], pixels_per_point: full.pixels_per_point };
        for (id, deltas) in &full.textures_delta.set {
            for delta in deltas {
                g.egui_renderer.update_texture(&g.gpu.device, &g.gpu.queue, *id, delta);
            }
        }
        let extra = g.egui_renderer.update_buffers(&g.gpu.device, &g.gpu.queue, &mut encoder, &prims, &screen);
        {
            // The editor paints its whole screen; the race draws the HUD over the scene.
            let load = if self.racing { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) };
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
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
        if let (None, Some(st)) = (offscreen, surface_texture) {
            g.gpu.queue.present(st);
            self.fps.frame();
            self.frames_shown += 1;
            if self.audio.is_none() && self.audio_wanted && self.frames_shown >= 30 {
                self.audio = audio::Audio::new();
            }
        }
        if leave {
            self.back_to_editor();
        }
    }

    /// The editor's keys: Enter races, ⌘Z / Ctrl+Z undoes, Escape drops the selection.
    fn editor_key(&mut self, code: KeyCode) {
        let now = self.egui_ctx.input(|i| i.time);
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => self.editor.play(now),
            KeyCode::KeyZ if self.modifiers.super_key() || self.modifiers.control_key() => self.editor.undo(),
            KeyCode::Escape => self.editor.deselect(),
            KeyCode::KeyF => self.game.fullscreen_requested = true,
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            self.last_frame = Instant::now();
            return;
        }
        let attrs = Window::default_attributes().with_title("Planet Tracks — Éditeur").with_visible(!self.debug.runs_hidden());
        #[cfg(not(target_os = "ios"))]
        let attrs = attrs.with_inner_size(window_size());
        #[cfg(target_os = "ios")]
        let attrs = {
            use winit::platform::ios::{ScreenEdge, WindowAttributesExtIOS};
            attrs
                .with_prefers_status_bar_hidden(true)
                .with_prefers_home_indicator_hidden(true)
                .with_preferred_screen_edges_deferring_system_gestures(ScreenEdge::ALL)
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        #[cfg(target_os = "ios")]
        {
            use winit::platform::ios::{ScreenEdge, WindowExtIOS};
            ios::attach_to_scene(&window);
            window.set_scale_factor(2.0);
            window.set_preferred_screen_edges_deferring_system_gestures(ScreenEdge::ALL);
            window.set_prefers_home_indicator_hidden(true);
            window.set_prefers_status_bar_hidden(true);
        }
        let gpu = Gpu::new(window.clone());
        let mut scene = SceneRenderer::new(&gpu);
        let track_mesh = scene.upload(&gpu.device, &track_render_data(&self.game.track));
        scene.set_track(&gpu, &self.game.track, track_mesh, self.game.time_of_day(), self.game.maps[self.game.map_index].planet);
        let car = upload_car(&mut scene, &gpu, &car_model::load());
        let ski_car = upload_car(&mut scene, &gpu, &car_model::load_skicar());
        let ski_livery = self.game.run.car.params.front_skis;
        let livery = car_model::livery(ski_livery);
        scene.set_livery(&gpu, livery.width, livery.height, &livery.rgba);
        let max_texture = gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(self.egui_ctx.clone(), egui::ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), None, Some(max_texture));
        let egui_renderer = egui_wgpu::Renderer::new(&gpu.device, gpu.config.format, Default::default());
        self.last_frame = Instant::now();
        self.gfx = Some(Graphics { window, gpu, scene, egui_state, egui_renderer, track_mesh, car, ski_car, ski_livery });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gfx.as_mut() else { return };
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.modifiers = m.state();
        }
        // Keys drive the race, or work the editor; never egui's widgets.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            if let PhysicalKey::Code(code) = key.physical_key {
                if self.racing {
                    self.game.controls.key(code, key.state.is_pressed(), key.repeat);
                } else if key.state.is_pressed() && !key.repeat {
                    self.editor_key(code);
                }
            }
            return;
        }
        let _ = g.egui_state.on_window_event(&g.window, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                let size = gfx::window_pixels(&g.window);
                g.gpu.resize(size.width, size.height);
            }
            WindowEvent::Focused(false) => self.game.controls.clear(),
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.game.controls.clear();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.debug.should_exit() {
            event_loop.exit();
            return;
        }
        if self.debug.runs_hidden() {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            if self.gfx.is_some() {
                self.frame();
            }
        } else if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

/// 1280 × 800, or `MARS_WINDOW=WxH` (a portrait size shows the phone layout).
#[cfg(not(target_os = "ios"))]
fn window_size() -> LogicalSize<f64> {
    std::env::var("MARS_WINDOW")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some(LogicalSize::new(w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(LogicalSize::new(1280.0, 800.0))
}

fn main() {
    let debug = debug::Debug::from_env();
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    if debug.runs_hidden() {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_activate_ignoring_other_apps(false);
    }
    let event_loop = builder.build().expect("event loop");
    let mut game = Game::new();
    // Drafts are raced for trying them out: no records or settings saved.
    game.session.persist = false;
    if TOUCH_SCREEN {
        game.camera.mode = camera::WIDE;
    }
    if debug.autodrive {
        game.autodrive = Some(debug::Autopilot::default());
    }
    let egui_ctx = egui::Context::default();
    egui_ctx.set_fonts(menu::fonts());
    let hud = hud::Hud::new(&game.maps);
    let mut app = App {
        gfx: None,
        game,
        editor: editor::Editor::new(),
        racing: false,
        hud,
        egui_ctx,
        last_frame: Instant::now(),
        accumulator: 0.0,
        fps: ui::Fps::new(),
        audio: None,
        audio_wanted: std::env::var("MARS_NO_AUDIO").is_err(),
        frames_shown: 0,
        debug,
        modifiers: ModifiersState::empty(),
    };
    event_loop.run_app(&mut app).expect("run");
}
