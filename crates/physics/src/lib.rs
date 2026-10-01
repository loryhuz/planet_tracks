//! Vehicle physics: deterministic, fixed 100 Hz tick, bit-identical on every platform.
//!
//! Determinism rules: f32 only, no `f32::sin`/`cos`/`atan2`/`exp`/`powf` from std (use `libm`),
//! no `mul_add`, no iteration over hash maps, no time or randomness, no threads.
//!
//! Wheel order everywhere: 0 front-left, 1 front-right, 2 rear-left, 3 rear-right.
//! Car frame: +Z forward, +Y up, +X left (see the conventions in the `track` crate). The car
//! frame's origin is the centre of gravity (`CarState::position`).
//!
//! The model, in short (details in `car.rs`):
//! - rigid chassis (mass, diagonal inertia), semi-implicit Euler at 100 Hz, renormalised quaternion;
//! - four raycast suspensions (spring, damper, bump stop, anti-roll bar) along the car's -Y;
//! - an arcade "path" model: the tyres keep the car on its path (the body heading turned back by
//!   the drift angle) and the car never translates sideways. Steering within the grip is on rails
//!   (tyre marks near the limit); steering beyond it bends the path at the grip limit and turns the
//!   excess into a drift angle that grows progressively and costs speed. Stepped engine curve,
//!   brakes, reverse, rolling resistance per surface;
//! - grip and steering authority smoothed over time so bumps move the body, not the heading;
//! - arcade assists: keep-flat torque, air control and auto-levelling;
//! - a body made of spheres swept in substeps against the track's BVH, with "modern" walls.

mod car;
mod params;
pub mod testing;
mod world;

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use track::{Pose, Surface};

pub use params::{
    CarParams, SurfaceGrip, Tunable, basse_gravite, buggy_lourd, combo, drift, equilibre, fidele, grip_arcade, presets,
};
pub use world::{Hit, World};

pub const TICK_HZ: u32 = 100;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

/// Driver input for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Input {
    /// -1 full left .. +1 full right.
    pub steer: f32,
    /// 0..1.
    pub gas: f32,
    /// 0..1. Brakes while moving forward, reverses once stopped.
    pub brake: f32,
}

/// Per-wheel state, used by the renderer (placement, spin) and the audio/particles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelState {
    /// Suspension anchor in the car frame (top of travel).
    pub anchor: Vec3,
    /// Current distance from the anchor down to the wheel centre, along the car's -Y.
    pub suspension: f32,
    /// Wheel yaw around the car's +Y axis, radians, positive turns the wheel's front toward +X (left).
    pub steer: f32,
    /// Rolling angle around the car's X axis, radians, increasing when rolling forward.
    pub spin: f32,
    pub contact: bool,
    pub surface: Option<Surface>,
    /// 0 = full grip .. 1 = fully sliding (for tyre marks, dust, sound).
    pub slip: f32,
    /// Wheel angle to draw, radians, positive toward +X (left): a clearly visible angle (up to
    /// 25° at full lock whatever the speed) plus countersteer while drifting. Render only; `steer`
    /// is the physical angle.
    pub steer_display: f32,
    /// Tyre-mark opacity 0..1: 0 while gripping, rising as the grip limit nears (tyres still
    /// aligned), 1 when drifting, spinning or locking.
    pub mark: f32,
    /// How far this tyre's ground velocity is from its own heading, |sin| of the angle: 0 = it rolls
    /// straight (sharp, parallel marks), 1 = fully sideways (blurred, diverging marks).
    pub smear: f32,
    /// Rolling speed, rad/s (spins up in the air with the throttle).
    pub spin_rate: f32,
    /// Normal load relative to the static load (1 at rest, 0 in the air).
    pub load: f32,
    /// Where the tyre touches the ground (world), valid when `contact`.
    pub contact_point: Vec3,
    /// Ground normal under the tyre (world), valid when `contact`.
    pub contact_normal: Vec3,
}

/// The full simulation state. Cloning it is how a checkpoint respawn restores the car.
#[derive(Clone, Debug, PartialEq)]
pub struct CarState {
    /// Centre of gravity, world space.
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub wheels: [WheelState; 4],
    pub tick: u64,
    /// Smoothed steering, -1 full left .. +1 full right (same sign as [`Input::steer`]).
    pub steer: f32,
    /// Drift engagement, 0 gripping .. 1 fully drifting (|drift angle| / 20°).
    pub drift: f32,
    /// Angle of the body relative to its path, radians, positive when the body points left of where
    /// the car goes (a left-hand drift).
    pub drift_angle: f32,
    /// The drift angle the model holds the body at (the steering's rotation not yet followed by
    /// the path), radians. `drift_angle` tracks it; bumps only disturb the latter.
    pub drift_ref: f32,
    /// The body's commanded turn rate from the steering, rad/s (positive to the left).
    pub yaw_cmd: f32,
    /// How fast the path turns, rad/s (positive to the left).
    pub path_rate: f32,
    /// Smoothed lateral grip available, in g.
    pub grip: f32,
    /// Lateral grip the steering asks for / grip available (1 = at the limit, above = drifting).
    pub grip_usage: f32,
    /// Smoothed ground contact (steering authority), 0..1.
    pub contact: f32,
    /// Last ground normal under the wheels.
    pub ground_normal: Vec3,
    /// Engine revs 0..1, for the sound.
    pub engine: f32,
    /// Measured acceleration over the last tick, world, m/s².
    pub acceleration: Vec3,
    /// Hardest body impact during the last tick (speed into the wall or ground, m/s).
    pub impact: f32,
    /// Whether the body touched a wall during the last tick.
    pub wall_contact: bool,
    /// Ticks since the last wheel contact (0 while on the ground).
    pub air_ticks: u32,
}

impl CarState {
    /// FNV-1a hash of every bit of the state, to compare runs across platforms.
    pub fn hash(&self) -> u64 {
        let mut words: Vec<u32> = Vec::with_capacity(128);
        let v3 = |w: &mut Vec<u32>, v: Vec3| w.extend_from_slice(&[v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]);
        v3(&mut words, self.position);
        words.extend_from_slice(&[
            self.rotation.x.to_bits(),
            self.rotation.y.to_bits(),
            self.rotation.z.to_bits(),
            self.rotation.w.to_bits(),
        ]);
        v3(&mut words, self.velocity);
        v3(&mut words, self.angular_velocity);
        for wh in &self.wheels {
            v3(&mut words, wh.anchor);
            words.extend_from_slice(&[
                wh.suspension.to_bits(),
                wh.steer.to_bits(),
                wh.spin.to_bits(),
                wh.contact as u32,
                wh.surface.map_or(255, |s| s as u32),
                wh.slip.to_bits(),
                wh.steer_display.to_bits(),
                wh.mark.to_bits(),
                wh.smear.to_bits(),
                wh.spin_rate.to_bits(),
                wh.load.to_bits(),
            ]);
            v3(&mut words, wh.contact_point);
            v3(&mut words, wh.contact_normal);
        }
        words.extend_from_slice(&[
            self.tick as u32,
            (self.tick >> 32) as u32,
            self.steer.to_bits(),
            self.drift.to_bits(),
            self.drift_angle.to_bits(),
            self.drift_ref.to_bits(),
            self.yaw_cmd.to_bits(),
            self.path_rate.to_bits(),
            self.grip.to_bits(),
            self.grip_usage.to_bits(),
            self.contact.to_bits(),
            self.engine.to_bits(),
        ]);
        v3(&mut words, self.ground_normal);
        v3(&mut words, self.acceleration);
        words.extend_from_slice(&[self.impact.to_bits(), self.wall_contact as u32, self.air_ticks]);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for word in words {
            for b in word.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Readouts for the HUD and the tuning panel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Telemetry {
    pub speed_kmh: f32,
    /// Signed speed along the car's forward axis, m/s.
    pub forward_speed: f32,
    /// Angle between heading and velocity, degrees (0 when gripping, large when drifting).
    pub slip_angle_deg: f32,
    /// Lateral acceleration in g (absolute value).
    pub lateral_g: f32,
    pub airborne: bool,
    pub sliding: bool,
    /// 0..1, for the engine sound.
    pub engine: f32,
    /// Drift engagement 0..1.
    pub drift: f32,
    /// Signed drift angle (body vs path), degrees, positive in a left-hand drift.
    pub drift_angle_deg: f32,
    /// Lateral grip asked for / available (1 = at the limit, above = drifting).
    pub grip_usage: f32,
    /// Longitudinal acceleration in g (positive when speeding up).
    pub longitudinal_g: f32,
    /// Yaw rate, rad/s (positive turning left).
    pub yaw_rate: f32,
    /// Number of wheels touching the ground.
    pub wheels_on_ground: u8,
    /// Surface under most of the wheels touching the ground.
    pub surface: Option<Surface>,
    /// Hardest body impact during the last tick, m/s (for sound and camera shake).
    pub impact: f32,
    /// Whether the body is scraping a wall.
    pub wall_contact: bool,
}

pub struct Car {
    pub params: CarParams,
    pub state: CarState,
}

impl Car {
    pub fn new(params: CarParams, world: &World, spawn: Pose) -> Self {
        let mut car = Self { params, state: rest_state(spawn) };
        car.respawn(world, spawn);
        car
    }

    /// Standing start at `spawn`, the chassis resting on its springs.
    pub fn respawn(&mut self, world: &World, spawn: Pose) {
        let p = &self.params;
        self.state = rest_state(spawn);
        let rest = p.rest_suspension();
        let anchors = p.wheel_anchors();
        self.state.position = spawn.position + Vec3::Y * (p.cg_height + rest + p.wheel_radius);
        // Start with the grip of the surface under the car (smoothed afterwards).
        let under = world.raycast(spawn.position + Vec3::Y * 1.0, -Vec3::Y, 3.0);
        let surface = under.map_or(Surface::Road, |h| h.surface);
        self.state.grip = p.surface(surface).grip * p.grip_scale;
        for (w, a) in self.state.wheels.iter_mut().zip(anchors) {
            w.anchor = a;
            w.suspension = rest;
            w.load = 1.0;
            w.contact = true;
            w.contact_normal = Vec3::Y;
            w.contact_point = self.state.position + self.state.rotation * (a - Vec3::Y * (rest + p.wheel_radius));
        }
    }

    /// Advances one tick.
    pub fn step(&mut self, world: &World, input: Input) {
        car::step(&self.params, &mut self.state, world, input);
    }

    /// Unit direction the tyres make the car travel (the heading turned back by the drift angle).
    pub fn path_direction(&self) -> Vec3 {
        car::path_axes(self.state.rotation, self.state.drift_angle, self.state.ground_normal).0
    }

    pub fn telemetry(&self) -> Telemetry {
        let s = &self.state;
        let fwd = s.rotation * Vec3::Z;
        let left = s.rotation * Vec3::X;
        let up = s.rotation * Vec3::Y;
        let v = s.velocity;
        let on_ground = s.wheels.iter().filter(|w| w.contact).count() as u8;
        let vf = v.dot(fwd);
        let normal = if on_ground > 0 { s.ground_normal } else { Vec3::Y };
        let slip = libm::fabsf(car::body_vs_motion(s.rotation, v, normal)).to_degrees();
        let mut counts = [0u8; 4];
        for w in &s.wheels {
            if let (true, Some(surf)) = (w.contact, w.surface) {
                counts[surf as usize] += 1;
            }
        }
        let mut surface = None;
        let mut best = 0;
        for (i, &c) in counts.iter().enumerate() {
            if c > best {
                best = c;
                surface = Some(match i {
                    0 => Surface::Road,
                    1 => Surface::Dirt,
                    2 => Surface::Ground,
                    _ => Surface::Wall,
                });
            }
        }
        Telemetry {
            speed_kmh: v.length() * 3.6,
            forward_speed: vf,
            slip_angle_deg: slip,
            lateral_g: (s.acceleration.dot(left) / 9.81).abs(),
            airborne: on_ground == 0,
            sliding: on_ground > 0 && s.drift_angle.abs() > 8f32.to_radians(),
            engine: s.engine,
            drift: s.drift,
            drift_angle_deg: s.drift_angle.to_degrees(),
            grip_usage: s.grip_usage,
            longitudinal_g: s.acceleration.dot(fwd) / 9.81,
            yaw_rate: s.angular_velocity.dot(up),
            wheels_on_ground: on_ground,
            surface,
            impact: s.impact,
            wall_contact: s.wall_contact,
        }
    }
}

fn rest_state(spawn: Pose) -> CarState {
    CarState {
        position: spawn.position,
        rotation: Quat::from_rotation_y(spawn.yaw),
        velocity: Vec3::ZERO,
        angular_velocity: Vec3::ZERO,
        wheels: [WheelState::default(); 4],
        tick: 0,
        steer: 0.0,
        drift: 0.0,
        drift_angle: 0.0,
        drift_ref: 0.0,
        yaw_cmd: 0.0,
        path_rate: 0.0,
        grip: 0.0,
        grip_usage: 0.0,
        contact: 1.0,
        ground_normal: Vec3::Y,
        engine: 0.0,
        acceleration: Vec3::ZERO,
        impact: 0.0,
        wall_contact: false,
        air_ticks: 0,
    }
}
