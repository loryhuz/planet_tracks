//! Chase cameras.

use glam::{Mat4, Quat, Vec3};

pub const MODES: [&str; 4] = ["Poursuite", "Grand angle", "Proche", "Capot"];
/// The wide-angle chase camera (the touch screens' first).
pub const WIDE: usize = 1;
const HOOD: usize = 3;

/// The wide-angle camera's field of view across the screen's diagonal, degrees: a lens rather
/// than the others' 60° high view, so a portrait screen sees about twice as wide as with them
/// (like a phone's ultra-wide camera), a landscape one a little wider.
const WIDE_DIAGONAL: f32 = 112.0;
/// How far the wide-angle view slides down on a portrait screen, in half screen heights: the
/// horizon rises and the road fills the screen down to the car.
const PORTRAIT_SHIFT: f32 = 0.1;

pub struct ChaseCamera {
    pub mode: usize,
    /// Smoothed viewing direction.
    dir: Vec3,
    fov: f32,
    /// The mode `fov` follows: a new mode takes its field of view at once (the wide angle's is far
    /// wider), only the speed's widening eases in.
    fov_mode: usize,
}

impl ChaseCamera {
    pub fn new() -> Self {
        Self { mode: 0, dir: Vec3::Z, fov: 60f32.to_radians(), fov_mode: 0 }
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

        let rush = (speed_kmh / 350.0).clamp(0.0, 1.0);
        let fov_target = if self.mode == WIDE {
            // The height of a view that spans the diagonal.
            let diagonal = (WIDE_DIAGONAL + 8.0 * rush).to_radians();
            2.0 * ((diagonal / 2.0).tan() / (1.0 + aspect * aspect).sqrt()).atan()
        } else {
            (60.0 + 12.0 * rush).to_radians()
        };
        if self.fov_mode != self.mode {
            self.fov_mode = self.mode;
            self.fov = fov_target;
        }
        self.fov += (fov_target - self.fov) * (1.0 - (-3.0 * dt).exp());

        let (eye, look) = match self.mode {
            0 => (position - self.dir * 7.2 + Vec3::Y * 2.7, position + self.dir * 5.0 + Vec3::Y * 0.9),
            // Closer and a little higher, so the car stays big in the wider view.
            WIDE => (position - self.dir * 4.4 + Vec3::Y * 2.3, position + self.dir * 7.0 + Vec3::Y * 0.6),
            2 => (position - self.dir * 4.6 + Vec3::Y * 1.8, position + self.dir * 6.0 + Vec3::Y * 0.7),
            _ => {
                let up = rotation * Vec3::Y;
                let eye = position + up * 0.9 + forward * 0.2;
                (eye, eye + forward * 10.0)
            }
        };
        let up = if self.mode == HOOD { rotation * Vec3::Y } else { Vec3::Y };
        let view = glam::camera::rh::view::look_at_mat4(eye, look, up);
        // On a portrait screen the field of view is widened so the road ahead keeps some width
        // (at least 58° across; the wide-angle view is wider still).
        let min_across = 58f32.to_radians();
        let fov = self.fov.max(2.0 * ((min_across * 0.5).tan() / aspect.max(0.1)).atan());
        let mut proj = glam::camera::rh::proj::directx::perspective_infinite_reverse(fov, aspect, 0.1);
        if self.mode == WIDE {
            // Lens shift: the frame slides down (verticals stay upright), from none on a square or
            // wide screen to `PORTRAIT_SHIFT` on a phone held upright (aspect 0.46).
            let shift = PORTRAIT_SHIFT * ((1.0 - aspect) / 0.54).clamp(0.0, 1.0);
            proj = Mat4::from_translation(Vec3::new(0.0, shift, 0.0)) * proj;
        }
        (view, proj, eye)
    }
}

/// Heading with a softened pitch: the camera follows climbs a little but keeps the horizon calm.
fn flatten(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, forward.y * 0.4, forward.z).normalize_or(Vec3::Z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_camera_takes_its_field_of_view_at_once() {
        let mut cam = ChaseCamera::new();
        let frame = |cam: &mut ChaseCamera| cam.update(1.0 / 60.0, Vec3::ZERO, Quat::IDENTITY, 0.0, false, 0.46);
        for _ in 0..120 {
            frame(&mut cam);
        }
        let chase = cam.fov;
        cam.mode = WIDE;
        frame(&mut cam);
        let wide = cam.fov;
        for _ in 0..120 {
            frame(&mut cam);
        }
        assert!(wide > chase * 1.5, "the wide angle is far wider ({chase} → {wide})");
        assert!((cam.fov - wide).abs() < 1e-4, "no zoom after the change ({wide} → {})", cam.fov);
    }
}
