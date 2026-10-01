//! Game state: the track, the player's run, its ghost, the tuning session.

use std::time::{Duration, Instant};

use glam::{Mat4, Quat, Vec3, Vec4};
use physics::{CarParams, CarState, Telemetry, World};
use track::Track;

use crate::camera::ChaseCamera;
use crate::car_model::{TYRE_WIDTH, arm_root};
use crate::gfx::{DrawItem, MeshId};
use crate::input::{Action, Controls};
use crate::race::{Frame, RaceEvent, Run, format_delta, format_time};
use crate::session::{Best, Session};

pub struct Popup {
    pub title: String,
    pub time: String,
    /// Ticks versus the record (negative = faster).
    pub delta: Option<i64>,
    pub until: Instant,
}

pub struct CarMeshes {
    pub body: MeshId,
    pub wheel: MeshId,
    pub arm: MeshId,
}

pub struct Game {
    pub track: Track,
    pub world: World,
    pub session: Session,
    pub run: Run,
    pub ghost: Option<(Run, Vec<Frame>)>,
    pub controls: Controls,
    pub camera: ChaseCamera,
    pub panel_open: bool,
    pub popup: Option<Popup>,
    pub fullscreen_requested: bool,
    pub marks: crate::marks::Marks,
    pub dust: crate::particles::Dust,
    pub mute_requested: bool,
    pub engine_requested: bool,
    /// Name of the engine sound in use, shown in the HUD.
    pub engine_name: &'static str,
    /// Impacts to play this frame (0..1 strength).
    pub impacts: Vec<f32>,
    last_impact: f32,
    /// Debug self-test: a route-following autopilot drives instead of the player.
    pub autodrive: Option<crate::debug::Autopilot>,
    pending_respawn: bool,
}

impl Game {
    pub fn new() -> Self {
        let track = track::demo_track();
        let world = World::new(&track.mesh);
        let session = Session::load();
        let run = Run::new(session.profile().params.clone(), &world, &track);
        let mut game = Self {
            track,
            world,
            session,
            run,
            ghost: None,
            controls: Controls::new(),
            camera: ChaseCamera::new(),
            panel_open: true,
            popup: None,
            fullscreen_requested: false,
            marks: crate::marks::Marks::new(),
            dust: crate::particles::Dust::new(),
            mute_requested: false,
            engine_requested: false,
            engine_name: crate::engine_sound::KINDS[0].name,
            impacts: Vec::new(),
            last_impact: 0.0,
            autodrive: None,
            pending_respawn: false,
        };
        game.restart();
        game
    }

    pub fn restart(&mut self) {
        // Finished runs were counted at the line.
        if self.run.tick > 0 && self.run.finished.is_none() {
            self.session.profile_mut().runs += 1;
            self.session.mark_dirty();
        }
        let profile = self.session.profile();
        self.run = Run::new(profile.params.clone(), &self.world, &self.track);
        self.ghost = profile
            .current_best()
            .map(|b| (Run::new(profile.params.clone(), &self.world, &self.track), b.frames.clone()));
        self.camera.snap(self.run.car.state.rotation);
        self.marks.clear();
        self.dust.clear();
        if let Some(pilot) = &mut self.autodrive {
            pilot.reset();
        }
        self.pending_respawn = false;
        self.popup = None;
    }

    pub fn select_profile(&mut self, index: usize) {
        if index < self.session.profiles.len() {
            self.session.current = index;
            self.session.mark_dirty();
            self.restart();
        }
    }

    fn cycle_profile(&mut self, step: isize) {
        let n = self.session.profiles.len() as isize;
        let mut i = self.session.current as isize;
        for _ in 0..n {
            i = (i + step).rem_euclid(n);
            if !self.session.profiles[i as usize].eliminated {
                break;
            }
        }
        self.select_profile(i as usize);
    }

    pub fn toggle_eliminated(&mut self, index: usize) {
        let p = &mut self.session.profiles[index];
        p.eliminated = !p.eliminated;
        self.session.mark_dirty();
    }

    /// The current profile's parameters changed (tuning panel): apply them to the running car.
    pub fn params_changed(&mut self) {
        self.run.car.params = self.session.profile().params.clone();
        if self.ghost.is_some() && self.session.profile().current_best().is_none() {
            self.ghost = None;
        }
        self.session.mark_dirty();
    }

    pub fn reset_params(&mut self) {
        let p = self.session.profile_mut();
        p.params = p.defaults.clone();
        self.params_changed();
        self.restart();
    }

    pub fn clear_best(&mut self) {
        self.session.profile_mut().best = None;
        self.ghost = None;
        self.session.mark_dirty();
    }

    pub fn apply(&mut self, action: Action) {
        match action {
            Action::Respawn => {
                if self.run.racing() {
                    self.pending_respawn = true;
                } else if self.run.finished.is_some() {
                    self.restart();
                }
            }
            Action::Restart => self.restart(),
            Action::Profile(i) => self.select_profile(i),
            Action::NextProfile => self.cycle_profile(1),
            Action::PrevProfile => self.cycle_profile(-1),
            Action::Eliminate => self.toggle_eliminated(self.session.current),
            Action::TogglePanel => self.panel_open = !self.panel_open,
            Action::Camera => self.camera.cycle(),
            Action::Fullscreen => self.fullscreen_requested = true,
            Action::Mute => self.mute_requested = true,
            Action::NextEngine => self.engine_requested = true,
        }
    }

    /// One 10 ms simulation tick.
    pub fn tick(&mut self) {
        let input = match &mut self.autodrive {
            Some(pilot) => {
                let s = &self.run.car.state;
                pilot.input(&self.track.route, s.position, s.rotation * Vec3::Z, s.velocity.length())
            }
            None => self.controls.driving(),
        };
        let frame = Frame { input, respawn: std::mem::take(&mut self.pending_respawn) };
        let events = self.run.step(&self.world, &self.track, frame);
        if frame.respawn {
            self.marks.break_strips();
        }
        self.marks.update(&self.run.car.state);
        self.dust.update(&self.run.car.state, physics::DT);
        let impact = self.run.car.telemetry().impact;
        if impact > 2.0 && self.last_impact <= 2.0 {
            self.impacts.push((impact / 15.0).clamp(0.15, 1.0));
        }
        self.last_impact = impact;
        if self.autodrive.is_some() && std::env::var("MARS_VERBOSE").is_ok() {
            for e in &events {
                let p = self.run.car.state.position;
                println!("[autopilot] t={} {:?} at ({:.1}, {:.1}, {:.1}) {:.0} km/h", format_time(self.run.tick), e, p.x, p.y, p.z, self.run.car.telemetry().speed_kmh);
            }
            if self.run.tick % 200 == 0 && self.run.racing() {
                let p = self.run.car.state.position;
                println!("[autopilot] t={} pos ({:.1}, {:.1}, {:.1}) {:.0} km/h", format_time(self.run.tick), p.x, p.y, p.z, self.run.car.telemetry().speed_kmh);
            }
        }
        for event in events {
            match event {
                RaceEvent::Checkpoint { index: _, tick } => {
                    let n = self.run.splits.len();
                    let delta = self
                        .session
                        .profile()
                        .current_best()
                        .and_then(|b| b.splits.get(n - 1))
                        .map(|&best| tick as i64 - best as i64);
                    self.popup = Some(Popup {
                        title: format!("Checkpoint {}/{}", n, self.track.checkpoints.len()),
                        time: format_time(tick),
                        delta,
                        until: Instant::now() + Duration::from_millis(2500),
                    });
                }
                RaceEvent::Finish { tick } => self.finish(tick),
                RaceEvent::Fell => {
                    if self.run.racing() {
                        self.pending_respawn = true;
                    }
                }
                RaceEvent::RespawnAtStart => self.restart(),
            }
        }
        if let Some((ghost, frames)) = &mut self.ghost {
            let f = if ghost.countdown > 0 { Frame::default() } else { frames.get(ghost.tick as usize).copied().unwrap_or_default() };
            ghost.step(&self.world, &self.track, f);
        }
    }

    fn finish(&mut self, tick: u32) {
        let params_json = self.session.profile().params_json();
        let previous = self.session.profile().current_best().map(|b| b.ticks);
        let record = previous.is_none_or(|p| tick < p);
        let profile = self.session.profile_mut();
        profile.finishes += 1;
        profile.runs += 1;
        if record {
            profile.best = Some(Best { ticks: tick, splits: self.run.splits.clone(), params_json, frames: self.run.frames.clone() });
        }
        self.session.mark_dirty();
        self.popup = Some(Popup {
            title: if record { "Record !".into() } else { "Arrivée".into() },
            time: format_time(tick),
            delta: previous.map(|p| tick as i64 - p as i64),
            until: Instant::now() + Duration::from_secs(3600),
        });
    }

    /// Sound parameters for this frame. The engine fakes gear changes at the speeds where
    /// the car's acceleration steps down, so the pitch rises and drops like a gearbox.
    pub fn sound_frame(&self) -> crate::audio::SoundFrame {
        let car = &self.run.car;
        let t = car.telemetry();
        let kmh = t.speed_kmh;
        let p = &car.params;
        let mut lo = 0.0;
        let mut hi = p.top_speed_kmh.max(1.0);
        for &s in &p.accel_speeds {
            if kmh < s {
                hi = s;
                break;
            }
            lo = s;
        }
        let frac = ((kmh - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0);
        let input = self.controls.driving();
        let airborne = t.airborne;
        let rpm = if airborne { 0.35 + 0.75 * input.gas } else { 0.25 + 0.75 * frac };
        let (mut squeal, mut scrub, mut loose, mut on) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for w in &car.state.wheels {
            if !w.contact {
                continue;
            }
            on += 1.0;
            match w.surface {
                Some(track::Surface::Road) => squeal = squeal.max(((w.mark - 0.3) / 0.7).clamp(0.0, 1.0) * (0.4 + 0.6 * w.smear)),
                Some(track::Surface::Dirt) | Some(track::Surface::Ground) => {
                    loose += 1.0;
                    scrub = scrub.max(w.smear.max(w.mark * 0.3));
                }
                _ => {}
            }
        }
        crate::audio::SoundFrame {
            rpm,
            load: if self.run.racing() { input.gas.max(if self.autodrive.is_some() { 1.0 } else { 0.0 }) } else { 0.0 },
            speed: car.state.velocity.length(),
            squeal,
            gravel: if on > 0.0 { loose / on } else { 0.0 },
            scrub,
            airborne,
        }
    }

    pub fn telemetry(&self) -> Telemetry {
        self.run.car.telemetry()
    }

    pub fn delta_text(delta: i64) -> String {
        format_delta(delta)
    }

    /// Render state of the player's car and the ghost, interpolated between ticks.
    pub fn draw_items(&self, alpha: f32, meshes: &CarMeshes) -> Vec<DrawItem> {
        let mut items = Vec::new();
        let params = &self.run.car.params;
        car_items(&mut items, params, &self.run.prev, &self.run.car.state, alpha, meshes, Vec4::ONE, true);
        if let Some((ghost, _)) = &self.ghost {
            car_items(&mut items, params, &ghost.prev, &ghost.car.state, alpha, meshes, Vec4::new(0.55, 0.8, 1.0, 0.5), false);
        }
        items
    }

    pub fn car_pose(&self, alpha: f32) -> (Vec3, Quat) {
        let (a, b) = (&self.run.prev, &self.run.car.state);
        (a.position.lerp(b.position, alpha), a.rotation.slerp(b.rotation, alpha))
    }
}

#[allow(clippy::too_many_arguments)]
fn car_items(
    items: &mut Vec<DrawItem>,
    params: &CarParams,
    prev: &CarState,
    cur: &CarState,
    alpha: f32,
    meshes: &CarMeshes,
    tint: Vec4,
    shadow: bool,
) {
    let pos = prev.position.lerp(cur.position, alpha);
    let rot = prev.rotation.slerp(cur.rotation, alpha);
    let car = Mat4::from_rotation_translation(rot, pos);
    items.push(DrawItem { mesh: meshes.body, model: car, tint, cast_shadow: shadow });
    let anchors = params.wheel_anchors();
    for i in 0..4 {
        let (w0, w1) = (&prev.wheels[i], &cur.wheels[i]);
        let anchor = if w1.anchor == Vec3::ZERO { anchors[i] } else { w1.anchor };
        let lerp = |a: f32, b: f32| a + (b - a) * alpha;
        let center = anchor - Vec3::Y * lerp(w0.suspension, w1.suspension);
        let steer = lerp(w0.steer_display, w1.steer_display);
        let spin = lerp(w0.spin, w1.spin);
        let left = anchor.x > 0.0;
        let wheel_rot = if left {
            Quat::from_rotation_y(steer) * Quat::from_rotation_x(spin)
        } else {
            Quat::from_rotation_y(steer) * Quat::from_rotation_y(std::f32::consts::PI) * Quat::from_rotation_x(-spin)
        };
        let model = car * Mat4::from_rotation_translation(wheel_rot, center);
        items.push(DrawItem { mesh: meshes.wheel, model, tint, cast_shadow: shadow });

        let root = arm_root(params, anchor);
        let hub = center - Vec3::X * anchor.x.signum() * (TYRE_WIDTH * 0.5);
        let span = hub - root;
        let len = span.length();
        if len > 1e-3 {
            let arm = Mat4::from_scale_rotation_translation(
                Vec3::new(len, 1.0, 1.0),
                Quat::from_rotation_arc(Vec3::X, span / len),
                root,
            );
            items.push(DrawItem { mesh: meshes.arm, model: car * arm, tint, cast_shadow: shadow });
        }
    }
}
