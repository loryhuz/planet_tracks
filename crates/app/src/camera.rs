//! Chase cameras.

use glam::{Mat4, Quat, Vec3};

pub const MODES: [&str; 3] = ["Poursuite", "Proche", "Capot"];

pub struct ChaseCamera {
    pub mode: usize,
    /// Smoothed viewing direction.
    dir: Vec3,
    fov: f32,
}

impl ChaseCamera {
    pub fn new() -> Self {
        Self { mode: 0, dir: Vec3::Z, fov: 60f32.to_radians() }
    }

    pub fn cycle(&mut self) {
        self.mode = (self.mode + 1) % MODES.len();
    }

    pub fn snap(&mut self, rotation: Quat) {
        self.dir = flatten(rotation * Vec3::Z);
    }

    /// Returns (view, proj, eye) for the interpolated car pose.
    pub fn update(&mut self, dt: f32, position: Vec3, rotation: Quat, speed_kmh: f32, airborne: bool, aspect: f32) -> (Mat4, Mat4, Vec3) {
        let forward = rotation * Vec3::Z;
        let target = flatten(forward);
        // Follow the heading quickly on the ground, lazily in the air so flips do not spin the view.
        let rate = if airborne { 1.5 } else { 7.0 };
        let k = 1.0 - (-rate * dt).exp();
        self.dir = (self.dir + (target - self.dir) * k).normalize_or(target);

        let fov_target = (60.0 + 12.0 * (speed_kmh / 350.0).clamp(0.0, 1.0)).to_radians();
        self.fov += (fov_target - self.fov) * (1.0 - (-3.0 * dt).exp());

        let (eye, look) = match self.mode {
            0 => (position - self.dir * 7.2 + Vec3::Y * 2.7, position + self.dir * 5.0 + Vec3::Y * 0.9),
            1 => (position - self.dir * 4.6 + Vec3::Y * 1.8, position + self.dir * 6.0 + Vec3::Y * 0.7),
            _ => {
                let up = rotation * Vec3::Y;
                let eye = position + up * 0.9 + forward * 0.2;
                (eye, eye + forward * 10.0)
            }
        };
        let up = if self.mode == 2 { rotation * Vec3::Y } else { Vec3::Y };
        let view = glam::camera::rh::view::look_at_mat4(eye, look, up);
        let proj = glam::camera::rh::proj::directx::perspective_infinite_reverse(self.fov, aspect, 0.1);
        (view, proj, eye)
    }
}

/// Heading with a softened pitch: the camera follows climbs a little but keeps the horizon calm.
fn flatten(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, forward.y * 0.4, forward.z).normalize_or(Vec3::Z)
}
