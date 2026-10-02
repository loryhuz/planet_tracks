//! Game state: the track, the player's run, its ghost, the tuning session.

use glam::{Mat4, Quat, Vec3, Vec4};
use physics::{CarParams, CarState, Telemetry, World};
use track::Track;

use crate::camera::ChaseCamera;
use crate::car_model::{CornerRig, Look};
use crate::gfx::{DrawItem, MeshId};
use crate::input::{Action, Controls};
use crate::race::{Frame, RaceEvent, Run, format_time};
use crate::session::{Best, Session, map_key};

/// The last finish: its time, the record it was measured against (with that record's
/// checkpoints), and whether it beat it.
pub struct RaceResult {
    pub ticks: u32,
    pub previous: Option<u32>,
    pub previous_splits: Vec<u32>,
    pub record: bool,
}

/// One corner's suspension parts on the GPU.
pub struct CornerMeshes {
    pub arm_lo: MeshId,
    pub arm_up: MeshId,
    pub upright: MeshId,
    pub wheel: MeshId,
    pub damper: MeshId,
    pub rod: MeshId,
    pub spring: MeshId,
    pub tierod: Option<MeshId>,
}

/// The buggy on the GPU, with the rest geometry its corners are posed from.
pub struct CarMeshes {
    pub body: MeshId,
    pub corners: Vec<CornerMeshes>,
    pub rigs: [CornerRig; 4],
    /// Wheel radius the model was built for, m.
    pub wheel_radius: f32,
}

pub struct Game {
    /// Every playable map and the one being driven.
    pub maps: Vec<track::Map>,
    pub map_index: usize,
    /// The track changed (new map): the renderer must upload its mesh again.
    pub track_changed: bool,
    pub track: Track,
    pub world: World,
    pub session: Session,
    pub run: Run,
    pub ghost: Option<(Run, Vec<Frame>)>,
    pub controls: Controls,
    pub camera: ChaseCamera,
    pub panel_open: bool,
    pub result: Option<RaceResult>,
    pub fullscreen_requested: bool,
    pub marks: crate::marks::Marks,
    pub dust: crate::particles::Dust,
    pub mute_requested: bool,
    /// Impacts to play this frame (0..1 strength).
    pub impacts: Vec<f32>,
    last_impact: f32,
    /// Debug self-test: a route-following autopilot drives instead of the player.
    pub autodrive: Option<crate::debug::Autopilot>,
    pending_respawn: bool,
}

impl Game {
    pub fn new() -> Self {
        let maps = track::builtin_maps();
        let session = Session::load();
        let map_index = maps.iter().position(|m| map_key(m) == session.map).unwrap_or(0);
        let track = maps[map_index].build();
        let world = World::new(&track.mesh);
        let run = Run::new(session.profile().params.clone(), &world, &track);
        let mut game = Self {
            maps,
            map_index,
            track_changed: false,
            track,
            world,
            session,
            run,
            ghost: None,
            controls: Controls::new(),
            camera: ChaseCamera::new(),
            panel_open: false,
            result: None,
            fullscreen_requested: false,
            marks: crate::marks::Marks::new(),
            dust: crate::particles::Dust::new(),
            mute_requested: false,
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
        let key = self.map_key();
        let profile = self.session.profile();
        self.run = Run::new(profile.params.clone(), &self.world, &self.track);
        self.ghost = profile
            .current_best(&key)
            .map(|b| (Run::new(profile.params.clone(), &self.world, &self.track), b.frames.clone()));
        self.camera.snap(self.run.car.state.rotation);
        self.marks.clear();
        self.dust.clear();
        if let Some(pilot) = &mut self.autodrive {
            pilot.reset();
        }
        self.pending_respawn = false;
        self.result = None;
    }

    pub fn map_key(&self) -> String {
        map_key(&self.maps[self.map_index])
    }

    pub fn map_name(&self) -> &str {
        &self.maps[self.map_index].name
    }

    pub fn select_map(&mut self, index: usize) {
        if index >= self.maps.len() || index == self.map_index {
            return;
        }
        // An unfinished run on the old map still counts as a try.
        if self.run.tick > 0 && self.run.finished.is_none() {
            self.session.profile_mut().runs += 1;
        }
        self.run.tick = 0;
        self.map_index = index;
        self.track = self.maps[index].build();
        self.world = World::new(&self.track.mesh);
        self.track_changed = true;
        self.session.map = self.map_key();
        self.session.mark_dirty();
        self.restart();
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
        if self.ghost.is_some() && self.session.profile().current_best(&self.map_key()).is_none() {
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
        let key = self.map_key();
        self.session.profile_mut().bests.remove(&key);
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
            Action::NextMap => self.select_map((self.map_index + 1) % self.maps.len()),
            // The app opens the menu.
            Action::Menu => {}
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
                // The HUD shows the checkpoints from the run's splits.
                RaceEvent::Checkpoint { .. } => {}
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
        let key = self.map_key();
        let best = self.session.profile().current_best(&key);
        let previous = best.map(|b| b.ticks);
        let previous_splits = best.map(|b| b.splits.clone()).unwrap_or_default();
        let record = previous.is_none_or(|p| tick < p);
        let profile = self.session.profile_mut();
        profile.finishes += 1;
        profile.runs += 1;
        if record {
            profile.bests.insert(key, Best { ticks: tick, splits: self.run.splits.clone(), params_json, frames: self.run.frames.clone() });
        }
        self.session.mark_dirty();
        self.result = Some(RaceResult { ticks: tick, previous, previous_splits, record });
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
        let mut gear = 0u32;
        for &s in &p.accel_speeds {
            if kmh < s {
                hi = s;
                break;
            }
            lo = s;
            gear += 1;
        }
        let frac = ((kmh - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0);
        let input = self.controls.driving();
        let airborne = t.airborne;
        // In the air the wheels spin freely: the whine follows the throttle.
        let rpm = if airborne { (frac + 0.3 * input.gas).min(1.0) } else { frac };
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
            gear,
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

    /// Render state of the player's car and the ghost, interpolated between ticks.
    pub fn draw_items(&self, alpha: f32, meshes: &CarMeshes) -> Vec<DrawItem> {
        let mut items = Vec::new();
        let params = &self.run.car.params;
        let run = &self.run;
        car_items(&mut items, params, (&run.prev, &run.car.state), (&run.look_prev, &run.look), alpha, meshes, Vec4::ONE, true);
        if let Some((ghost, _)) = &self.ghost {
            let tint = Vec4::new(0.55, 0.8, 1.0, 0.5);
            car_items(&mut items, params, (&ghost.prev, &ghost.car.state), (&ghost.look_prev, &ghost.look), alpha, meshes, tint, false);
        }
        items
    }

    pub fn car_pose(&self, alpha: f32) -> (Vec3, Quat) {
        let (a, b) = (&self.run.prev, &self.run.car.state);
        (a.position.lerp(b.position, alpha), a.rotation.slerp(b.rotation, alpha))
    }
}

/// Wraps an angle step into (-π, π], so a lerp across the 2π wrap goes the short way.
fn angle_lerp(a: f32, b: f32, t: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let d = (b - a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    a + d * t
}

/// The body, then every corner's parts posed from the suspension lengths: the body carries the
/// render-only pitch, so the wheel targets are taken into its frame before solving the corners.
#[allow(clippy::too_many_arguments)]
fn car_items(
    items: &mut Vec<DrawItem>,
    params: &CarParams,
    (prev, cur): (&CarState, &CarState),
    (look_prev, look_cur): (&Look, &Look),
    alpha: f32,
    meshes: &CarMeshes,
    tint: Vec4,
    shadow: bool,
) {
    let pos = prev.position.lerp(cur.position, alpha);
    let rot = prev.rotation.slerp(cur.rotation, alpha);
    let look = look_prev.lerp(look_cur, alpha);
    let body = Mat4::from_rotation_translation(rot, pos) * Mat4::from_rotation_x(look.pitch);
    let into_body = Quat::from_rotation_x(-look.pitch);
    let mut push = |mesh, model| items.push(DrawItem { mesh, model, tint, cast_shadow: shadow });
    push(meshes.body, body);
    let anchors = params.wheel_anchors();
    let scale = params.wheel_radius / meshes.wheel_radius.max(0.05);
    for (i, (rig, parts)) in meshes.rigs.iter().zip(&meshes.corners).enumerate() {
        let (w0, w1) = (&prev.wheels[i], &cur.wheels[i]);
        let anchor = if w1.anchor == Vec3::ZERO { anchors[i] } else { w1.anchor };
        let centre = into_body * (anchor - Vec3::Y * look.travel[i]);
        let steer = w0.steer_display + (w1.steer_display - w0.steer_display) * alpha;
        let pose = rig.pose(centre.y, steer, angle_lerp(w0.spin, w1.spin, alpha), scale);
        for (mesh, m) in [
            (parts.arm_lo, pose.arm_lo),
            (parts.arm_up, pose.arm_up),
            (parts.upright, pose.upright),
            (parts.wheel, pose.wheel),
            (parts.damper, pose.damper),
            (parts.rod, pose.rod),
            (parts.spring, pose.spring),
        ] {
            push(mesh, body * m);
        }
        if let (Some(mesh), Some(m)) = (parts.tierod, pose.tierod) {
            push(mesh, body * m);
        }
    }
}
