//! Test meshes built inside the physics crate (plane, slopes, ramp, walls, bumps, surface patches),
//! so that the tests and the metrics do not depend on the game's maps.

use glam::Vec3;
use track::{Surface, TrackMesh};

/// Builds a triangle soup with a surface per triangle.
#[derive(Default)]
pub struct MeshBuilder {
    pub mesh: TrackMesh,
}

fn color(s: Surface) -> [f32; 3] {
    match s {
        Surface::Road => [0.3, 0.3, 0.32],
        Surface::Booster => [0.8, 0.4, 0.1],
        Surface::Dirt => [0.55, 0.28, 0.14],
        Surface::Ground => [0.45, 0.2, 0.1],
        Surface::Wall => [0.7, 0.7, 0.7],
    }
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// One triangle; its normal follows the right-hand rule on `a, b, c`.
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, surface: Surface) {
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
        let base = self.mesh.positions.len() as u32;
        for p in [a, b, c] {
            self.mesh.positions.push(p);
            self.mesh.normals.push(n);
            self.mesh.colors.push(color(surface));
        }
        self.mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        self.mesh.tri_surface.push(surface);
    }

    /// A quad `a, b, c, d` (in order around its edge).
    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, surface: Surface) {
        self.tri(a, b, c, surface);
        self.tri(a, c, d, surface);
    }

    /// A heightfield over `[x0, x1] × [z0, z1]` in square cells, normals facing up.
    pub fn grid(
        &mut self,
        (x0, z0): (f32, f32),
        (x1, z1): (f32, f32),
        cell: f32,
        height: impl Fn(f32, f32) -> f32,
        surface: impl Fn(f32, f32) -> Surface,
    ) {
        let nx = (((x1 - x0) / cell) as usize).max(1);
        let nz = (((z1 - z0) / cell) as usize).max(1);
        let dx = (x1 - x0) / nx as f32;
        let dz = (z1 - z0) / nz as f32;
        for i in 0..nx {
            for j in 0..nz {
                let xa = x0 + dx * i as f32;
                let xb = xa + dx;
                let za = z0 + dz * j as f32;
                let zb = za + dz;
                let s = surface((xa + xb) * 0.5, (za + zb) * 0.5);
                let p = |x: f32, z: f32| Vec3::new(x, height(x, z), z);
                self.quad(p(xa, za), p(xa, zb), p(xb, zb), p(xb, za), s);
            }
        }
    }

    /// A vertical wall panel from `a` to `b` (on the ground), `height` tall, both faces.
    pub fn wall(&mut self, a: Vec3, b: Vec3, bottom: f32, height: f32) {
        let a0 = Vec3::new(a.x, bottom, a.z);
        let b0 = Vec3::new(b.x, bottom, b.z);
        let a1 = Vec3::new(a.x, bottom + height, a.z);
        let b1 = Vec3::new(b.x, bottom + height, b.z);
        self.quad(a0, a1, b1, b0, Surface::Wall);
    }

    pub fn build(self) -> TrackMesh {
        self.mesh
    }
}

/// A flat square of one surface, `half` metres from the origin in each direction.
pub fn flat(half: f32, surface: Surface) -> TrackMesh {
    let mut b = MeshBuilder::new();
    b.grid((-half, -half), (half, half), 20.0, |_, _| 0.0, |_, _| surface);
    b.build()
}

/// A long straight strip of one surface along +Z, from z = -100 to `length`.
pub fn strip(length: f32, surface: Surface) -> TrackMesh {
    let mut b = MeshBuilder::new();
    b.grid((-60.0, -100.0), (60.0, length), 40.0, |_, _| 0.0, |_, _| surface);
    b.build()
}

/// Flat ground up to z = 0, then a constant slope of `deg` degrees for `length` metres, then a
/// plateau.
pub fn slope(deg: f32, length: f32, surface: Surface) -> TrackMesh {
    let t = libm::tanf(deg.to_radians());
    let mut b = MeshBuilder::new();
    let h = |_: f32, z: f32| {
        if z <= 0.0 {
            0.0
        } else if z <= length {
            z * t
        } else {
            length * t
        }
    };
    b.grid((-20.0, -100.0), (20.0, 0.0), 10.0, h, |_, _| surface);
    b.grid((-20.0, 0.0), (20.0, length), 2.0, h, |_, _| surface);
    b.grid((-20.0, length), (20.0, length + 100.0), 10.0, h, |_, _| surface);
    b.build()
}

/// A long run-up on road ending in a `deg`-degree ramp (`length` metres long, horizontally, the
/// first 3 m curving up from flat), whose lip at z = 0 drops to flat road. Returns the mesh and
/// the lip height.
pub fn ramp(deg: f32, length: f32) -> (TrackMesh, f32) {
    let t = libm::tanf(deg.to_radians());
    let mut b = MeshBuilder::new();
    let w = 12.0;
    // Run-up.
    b.grid((-w, -1500.0), (w, -length), 20.0, |_, _| 0.0, |_, _| Surface::Road);
    // Ramp surface, with a short curved transition from the run-up (like a real ramp).
    let blend = 3.0f32.min(length * 0.5);
    let h = move |z: f32| {
        let d = z + length;
        if d <= blend { d * d * t / (2.0 * blend) } else { (d - blend * 0.5) * t }
    };
    b.grid((-w, -length), (w, -length + blend), 0.5, move |_, z| h(z), |_, _| Surface::Road);
    b.grid((-w, -length + blend), (w, 0.0), 1.0, move |_, z| h(z), |_, _| Surface::Road);
    let lip = h(0.0);
    // Vertical face under the lip.
    b.quad(Vec3::new(w, 0.0, 0.0), Vec3::new(w, lip, 0.0), Vec3::new(-w, lip, 0.0), Vec3::new(-w, 0.0, 0.0), Surface::Wall);
    // Landing.
    b.grid((-40.0, 0.0), (40.0, 400.0), 20.0, |_, _| 0.0, |_, _| Surface::Road);
    (b.build(), lip)
}

/// Flat road with a long thin wall along +Z at `x = wall_x`.
pub fn wall_lane(wall_x: f32) -> TrackMesh {
    let mut b = MeshBuilder::new();
    b.grid((-60.0, -200.0), (60.0, 600.0), 20.0, |_, _| 0.0, |_, _| Surface::Road);
    b.wall(Vec3::new(wall_x, 0.0, -200.0), Vec3::new(wall_x, 0.0, 600.0), -1.0, 3.0);
    b.build()
}

/// Flat road with a thin single-quad wall across the road at `z`.
pub fn thin_wall(z: f32) -> TrackMesh {
    let mut b = MeshBuilder::new();
    b.grid((-40.0, -1500.0), (40.0, z + 200.0), 20.0, |_, _| 0.0, |_, _| Surface::Road);
    b.wall(Vec3::new(-40.0, 0.0, z), Vec3::new(40.0, 0.0, z), -1.0, 4.0);
    b.build()
}

/// Road with a field of sine bumps of amplitude `amp` and wavelength `wavelength`.
pub fn bumps(amp: f32, wavelength: f32) -> TrackMesh {
    let mut b = MeshBuilder::new();
    let k = core::f32::consts::TAU / wavelength;
    b.grid(
        (-80.0, -100.0),
        (80.0, 600.0),
        1.0,
        move |x, z| if z < 0.0 { 0.0 } else { amp * libm::sinf(k * z) * libm::cosf(0.5 * k * x) },
        |_, _| Surface::Dirt,
    );
    b.build()
}

/// Flat road with one smooth bump `height` high and `length` long starting at z = `at`, across the
/// whole width, or only under the left wheels (x > 0) when `one_side`.
pub fn single_bump(height: f32, length: f32, at: f32, one_side: bool) -> TrackMesh {
    let mut b = MeshBuilder::new();
    let h = move |x: f32, z: f32| {
        let t = (z - at) / length;
        if !(0.0..=1.0).contains(&t) {
            return 0.0;
        }
        let side = if one_side { crate::testing::ramp01(x / 0.5) } else { 1.0 };
        height * side * 0.5 * (1.0 - libm::cosf(core::f32::consts::TAU * t))
    };
    b.grid((-30.0, -600.0), (30.0, at - 1.0), 20.0, |_, _| 0.0, |_, _| Surface::Road);
    b.grid((-30.0, at - 1.0), (30.0, at + length + 1.0), 0.25, h, |_, _| Surface::Road);
    b.grid((-30.0, at + length + 1.0), (30.0, 800.0), 20.0, |_, _| 0.0, |_, _| Surface::Road);
    b.build()
}

fn ramp01(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A flat square split into one surface for x < 0 and another for x >= 0.
pub fn patches(half: f32, a: Surface, b_surf: Surface) -> TrackMesh {
    let mut b = MeshBuilder::new();
    b.grid((-half, -half), (half, half), 10.0, |_, _| 0.0, move |x, _| if x < 0.0 { a } else { b_surf });
    b.build()
}

/// A bit of everything, for long random runs: bumps, surfaces, a ramp, walls, a pit.
pub fn playground() -> TrackMesh {
    let mut b = MeshBuilder::new();
    let h = |x: f32, z: f32| {
        let bump = 0.4 * libm::sinf(0.15 * x) * libm::cosf(0.11 * z);
        let hill = if (x - 60.0).abs() < 30.0 && (z - 40.0).abs() < 30.0 { 6.0 } else { 0.0 };
        bump + hill * 0.5
    };
    let surf = |x: f32, z: f32| {
        if x.abs() < 8.0 {
            Surface::Road
        } else if z > 0.0 {
            Surface::Dirt
        } else {
            Surface::Ground
        }
    };
    b.grid((-150.0, -150.0), (150.0, 150.0), 3.0, h, surf);
    // Enclosure.
    for (a, c) in [
        (Vec3::new(-150.0, 0.0, -150.0), Vec3::new(150.0, 0.0, -150.0)),
        (Vec3::new(150.0, 0.0, -150.0), Vec3::new(150.0, 0.0, 150.0)),
        (Vec3::new(150.0, 0.0, 150.0), Vec3::new(-150.0, 0.0, 150.0)),
        (Vec3::new(-150.0, 0.0, 150.0), Vec3::new(-150.0, 0.0, -150.0)),
    ] {
        b.wall(a, c, -3.0, 8.0);
    }
    // Some inner walls and a ramp.
    b.wall(Vec3::new(-40.0, 0.0, -60.0), Vec3::new(-40.0, 0.0, 60.0), -2.0, 3.0);
    b.wall(Vec3::new(20.0, 0.0, -80.0), Vec3::new(60.0, 0.0, -40.0), -2.0, 3.0);
    let t = libm::tanf(18f32.to_radians());
    b.grid((-6.0, -40.0), (6.0, -30.0), 1.0, move |_, z| 0.5 + (z + 40.0) * t, |_, _| Surface::Road);
    b.build()
}

/// A large heightfield with about `triangles` triangles, for timing.
pub fn big_terrain(triangles: usize) -> TrackMesh {
    let cells = ((triangles / 2) as f32).sqrt() as usize;
    let size = cells as f32 * 2.0;
    let mut b = MeshBuilder::new();
    b.grid(
        (-size * 0.5, -size * 0.5),
        (size * 0.5, size * 0.5),
        2.0,
        |x, z| 0.6 * libm::sinf(0.05 * x) * libm::cosf(0.07 * z) + 0.2 * libm::sinf(0.3 * x + 0.2 * z),
        |x, _| if x.abs() < 10.0 { Surface::Road } else { Surface::Dirt },
    );
    b.build()
}

/// A keyboard-style driver for tests: follows a route polyline (pure pursuit), steering only at
/// full lock or not at all, throttle always held, never braking (see [`Autopilot::braking`] for
/// one that lifts and brakes before the bends).
pub struct Autopilot {
    route: Vec<Vec3>,
    /// Index of the route point nearest to the car.
    pub index: usize,
    steer: f32,
    /// Fastest speed each route point allows and the deceleration of the brakes there (m/s,
    /// m/s²); empty for a driver who never brakes.
    bends: Vec<(f32, f32)>,
}

/// Deceleration the braking driver counts on, on road and on dirt, m/s².
const BRAKE_ROAD: f32 = 40.0;
const BRAKE_DIRT: f32 = 25.0;
/// How far ahead the braking driver looks for bends, metres.
const BRAKE_LOOKAHEAD: f32 = 150.0;

impl Autopilot {
    pub fn new(route: &[Vec3]) -> Self {
        Self { route: route.to_vec(), index: 0, steer: 0.0, bends: Vec::new() }
    }

    /// The same driver, lifting and braking so that it reaches each point of the route at no
    /// more than `√(a · R)`: `R` is the radius of the route's centreline there, `a` the lateral
    /// acceleration it allows itself, `road` or `dirt` (m/s²) by the surface under the point.
    pub fn braking(mut self, world: &crate::World, road: f32, dirt: f32) -> Self {
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let r = &self.route;
        self.bends = (0..r.len())
            .map(|i| {
                let on_dirt = world.raycast(r[i] + Vec3::Y * 3.0, -Vec3::Y, 8.0).is_some_and(|h| h.surface == Surface::Dirt);
                let (a, brake) = if on_dirt { (dirt, BRAKE_DIRT) } else { (road, BRAKE_ROAD) };
                // Radius from the change of heading over three segments on each side.
                let (lo, hi) = (i.saturating_sub(3), (i + 3).min(r.len() - 1));
                if hi <= lo + 1 {
                    return (f32::INFINITY, brake);
                }
                let (d0, d1) = (flat(r[lo + 1] - r[lo]), flat(r[hi] - r[hi - 1]));
                let turn = libm::atan2f(d0.x * d1.z - d0.z * d1.x, d0.dot(d1)).abs();
                let length: f32 = (lo..hi).map(|k| flat(r[k + 1] - r[k]).length()).sum();
                let speed = if turn < 1e-3 { f32::INFINITY } else { libm::sqrtf(a * length / turn) };
                (speed, brake)
            })
            .collect();
        self
    }

    /// Throttle and brake for the braking driver at `speed` (m/s): the slowest speed any bend
    /// ahead allows from here, given the room to brake for it.
    fn pedals(&self, speed: f32) -> (f32, f32) {
        if self.bends.is_empty() {
            return (1.0, 0.0);
        }
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let (mut run, mut limit) = (0.0, f32::INFINITY);
        for k in self.index..self.route.len() {
            if k > self.index {
                run += flat(self.route[k] - self.route[k - 1]).length();
            }
            if run > BRAKE_LOOKAHEAD {
                break;
            }
            let (v, brake) = self.bends[k];
            limit = limit.min(libm::sqrtf(v * v + 2.0 * brake * run));
        }
        if speed > limit + 2.0 {
            (0.0, 1.0)
        } else if speed > limit {
            (0.0, 0.0)
        } else {
            (1.0, 0.0)
        }
    }

    /// Distance of `p` from the route around the current index, horizontally, metres.
    pub fn offset(&self, p: Vec3) -> f32 {
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let lo = self.index.saturating_sub(3);
        let hi = (self.index + 3).min(self.route.len() - 1);
        let mut best = f32::INFINITY;
        for k in lo..hi {
            let (a, b) = (flat(self.route[k]), flat(self.route[k + 1]));
            let ab = b - a;
            let t = ((flat(p) - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            best = best.min((flat(p) - (a + ab * t)).length());
        }
        best
    }

    /// The input for this tick.
    pub fn input(&mut self, car: &crate::Car) -> crate::Input {
        let pos = car.state.position;
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let mut best_d = f32::INFINITY;
        let start = self.index;
        for k in start..(start + 40).min(self.route.len()) {
            let d = (flat(self.route[k]) - flat(pos)).length();
            if d < best_d {
                best_d = d;
                self.index = k;
            }
        }
        let speed = car.state.velocity.length();
        let look = (speed * 0.55).clamp(10.0, 40.0);
        let (mut k, mut run) = (self.index, 0.0);
        while k + 1 < self.route.len() && run < look {
            run += (flat(self.route[k + 1]) - flat(self.route[k])).length();
            k += 1;
        }
        let to = flat(self.route[k]) - flat(pos);
        // Aim with the direction the car is actually travelling (its path), like a driver does.
        let v = flat(car.state.velocity);
        let f = if v.length() > 2.0 { v.normalize() } else { car.state.rotation * Vec3::Z };
        // Angle from the course to the target about +Y (positive: target on the left), the
        // curvature that reaches it (pure pursuit) and the curvature the car already follows.
        let ang = libm::atan2f(f.z * to.x - f.x * to.z, f.x * to.x + f.z * to.z);
        let wanted = 2.0 * libm::sinf(ang) / to.length().max(1.0);
        let now = car.state.path_rate / speed.max(1.0);
        let err = wanted - now;
        // Keyboard: full lock or nothing, with some hysteresis.
        let (on, off) = (0.004, 0.0015);
        self.steer = if err < -on || (self.steer > 0.0 && err < -off) {
            1.0
        } else if err > on || (self.steer < 0.0 && err > off) {
            -1.0
        } else {
            0.0
        };
        let (gas, brake) = self.pedals(speed);
        crate::Input { steer: self.steer, gas, brake }
    }

    pub fn finished(&self) -> bool {
        self.index + 2 >= self.route.len()
    }
}
