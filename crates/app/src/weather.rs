//! Weather on the circuit (render only, not part of the deterministic simulation: the wind never
//! pushes the car). The wind blows from the sandstorm toward the circuit and veers slowly; fine
//! sand drifts on it through the air around the camera, and every few seconds a gust sweeps a
//! low cloud of sand across the road, often just as the car gets there. weather.wgsl draws it.
//!
//! It belongs to the planet, not to the block kit: a `Climate` per planet, and every map gets it
//! as it is, since the gusts only need the track's route.

use bytemuck::{Pod, Zeroable};
use glam::{DVec2, Vec2, Vec3};

/// Gusts on screen at once, at most (the size of the renderer's instance buffer).
pub const MAX_GUSTS: usize = 8;
/// The air's drift is passed to the GPU modulo this many metres: a multiple of weather.wgsl's
/// box, so the grains do not jump when it wraps.
const DRIFT_PERIOD: f64 = 56.0 * 64.0;

/// A planet's weather.
#[derive(Clone, Copy, Debug)]
pub struct Climate {
    /// Mean wind speed, m/s.
    pub wind: f32,
    /// How far the wind veers each way around its mean direction, radians.
    pub veer: f32,
    /// Share of weather.wgsl's drifting grains shown (0 to 1).
    pub grains: f32,
    /// Seconds between two gusts: at least, at most.
    pub gust_every: (f32, f32),
}

impl Climate {
    /// Mars: a steady breeze from the storm, light sand in the air, a gust every few seconds.
    pub const MARS: Self = Self { wind: 7.0, veer: 0.35, grains: 1.0, gust_every: (2.5, 6.5) };
    /// Still air: nothing drawn.
    pub const STILL: Self = Self { wind: 0.0, veer: 0.0, grains: 0.0, gust_every: (f32::INFINITY, f32::INFINITY) };

    /// `MARS_WEATHER=off` stills the air, `MARS_WEATHER=gusty` brings a gust every second or so
    /// (checks); otherwise Mars.
    pub fn from_env() -> Self {
        match std::env::var("MARS_WEATHER").as_deref() {
            Ok("off") => Self::STILL,
            Ok("gusty") => Self { gust_every: (0.6, 1.4), ..Self::MARS },
            _ => Self::MARS,
        }
    }
}

/// One gust as weather.wgsl reads it (an instance of its puffs and of its grains).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct GustInstance {
    /// Where its centre crosses the road (x, y, z: on the road), and its seed.
    pub origin: [f32; 4],
    /// Direction it travels (x, z, unit), its speed (m/s), its age when it crosses (s).
    pub travel: [f32; 4],
    /// Its age and life (s), length along its travel and width across it (m).
    pub time: [f32; 4],
    /// Its height (m) and strength (0 to 1).
    pub shape: [f32; 4],
}

#[derive(Clone, Copy)]
struct Gust {
    origin: Vec3,
    dir: Vec2,
    speed: f32,
    cross: f32,
    age: f32,
    life: f32,
    length: f32,
    width: f32,
    height: f32,
    strength: f32,
    seed: f32,
}

impl Gust {
    fn centre(&self) -> Vec3 {
        self.origin + Vec3::new(self.dir.x, 0.0, self.dir.y) * self.speed * (self.age - self.cross)
    }

    /// How much of it there is (fading in and out), as weather.wgsl's `presence`.
    fn presence(&self) -> f32 {
        smoothstep(0.0, 1.0, self.age) * (1.0 - smoothstep(self.life - 1.8, self.life, self.age)) * self.strength
    }
}

/// The weather's state for the frame, as the frame uniform carries it.
pub struct WeatherUniforms {
    /// Wind direction (x, z), its speed (m/s), the time (s).
    pub wind: [f32; 4],
    /// The air's drift (x, z, metres, wrapped), share of the grains shown, veil over the view (0..1).
    pub drift: [f32; 4],
    /// The camera's velocity (m/s).
    pub eye_vel: [f32; 4],
}

pub struct Weather {
    climate: Climate,
    route: Vec<Vec3>,
    /// Metres along the route at each of its points.
    along: Vec<f32>,
    /// The storm's wind: unit direction (x, z) from the storm toward the circuit.
    from_storm: Vec2,
    /// The wind now, m/s (x, z).
    wind: Vec2,
    drift: DVec2,
    gusts: Vec<Gust>,
    next_gust: f32,
    seed: u32,
    time: Option<f32>,
    eye: Option<Vec3>,
    eye_vel: Vec3,
}

impl Weather {
    pub fn new(climate: Climate, route: &[Vec3], from_storm: Vec2) -> Self {
        let mut along = Vec::with_capacity(route.len());
        let mut s = 0.0;
        for (i, p) in route.iter().enumerate() {
            if i > 0 {
                s += route[i - 1].distance(*p);
            }
            along.push(s);
        }
        Self {
            climate,
            route: route.to_vec(),
            along,
            from_storm: from_storm.normalize_or(Vec2::Y),
            wind: Vec2::ZERO,
            drift: DVec2::ZERO,
            gusts: Vec::new(),
            // The first one soon after the start.
            next_gust: 1.2,
            seed: 0x2545_f491,
            time: None,
            eye: None,
            eye_vel: Vec3::ZERO,
        }
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Advances the weather to `time` (seconds since the track was set) with the camera at `eye`.
    pub fn update(&mut self, time: f32, eye: Vec3) {
        let dt = self.time.map_or(0.0, |t0| (time - t0).clamp(0.0, 0.1));
        self.time = Some(time);

        // The wind veers and swells slowly around the storm's.
        let c = self.climate;
        let veer = c.veer * (0.65 * (time * 0.051).sin() + 0.35 * (time * 0.137 + 1.3).sin());
        let swell = 0.85 + 0.15 * (time * 0.23 + 0.4).sin();
        self.wind = Vec2::from_angle(veer).rotate(self.from_storm) * c.wind * swell;
        self.drift = (self.drift + self.wind.as_dvec2() * dt as f64).rem_euclid(DVec2::splat(DRIFT_PERIOD));

        // The camera's velocity, for the grains' streaks; a jump (respawn, another camera) is
        // not a motion.
        if let Some(prev) = self.eye
            && dt > 0.0
        {
            let step = eye - prev;
            if step.length() > 25.0 {
                self.eye_vel = Vec3::ZERO;
            } else {
                let v = (step / dt).clamp_length_max(150.0);
                self.eye_vel += (v - self.eye_vel) * (1.0 - (-dt / 0.08).exp());
            }
        }
        self.eye = Some(eye);

        for g in &mut self.gusts {
            g.age += dt;
        }
        self.gusts.retain(|g| g.age < g.life);
        self.next_gust -= dt;
        if self.next_gust <= 0.0 {
            let (lo, hi) = c.gust_every;
            self.next_gust = lo + (hi - lo) * self.rand();
            if self.gusts.len() < MAX_GUSTS && c.wind > 0.0 {
                self.spawn(eye);
            }
        }
    }

    /// A gust over the route ahead of the camera: most cross the road just as the car gets there
    /// (at its speed along the route), the others farther on, for the scenery.
    fn spawn(&mut self, eye: Vec3) {
        if self.route.len() < 2 {
            return;
        }
        let near = (0..self.route.len())
            .min_by(|&a, &b| self.route[a].distance_squared(eye).total_cmp(&self.route[b].distance_squared(eye)))
            .unwrap_or(0);
        let i = near.min(self.route.len() - 2);
        let heading = (self.route[i + 1] - self.route[i]).normalize_or_zero();
        let speed = self.eye_vel.dot(heading).max(0.0);
        let (ahead, cross) = if self.rand() < 0.6 {
            let cross = 1.0 + 1.6 * self.rand();
            ((speed * cross).max(18.0 + 20.0 * self.rand()), cross)
        } else {
            (50.0 + 180.0 * self.rand(), 0.8 + 1.5 * self.rand())
        };
        let Some((point, road)) = self.route_at(self.along[near] + ahead) else { return };
        let side = Vec3::new(-road.z, 0.0, road.x).normalize_or_zero();
        let origin = point + side * (self.rand() - 0.5) * 6.0;
        let wind = self.wind.normalize_or(self.from_storm);
        let dir = Vec2::from_angle((self.rand() - 0.5) * 0.7).rotate(wind);
        let gust = Gust {
            origin,
            dir,
            speed: self.climate.wind * (1.6 + 0.8 * self.rand()),
            cross,
            age: 0.0,
            life: cross + 2.2 + 1.5 * self.rand(),
            length: 16.0 + 12.0 * self.rand(),
            width: 8.0 + 6.0 * self.rand(),
            height: 3.0 + 2.5 * self.rand(),
            strength: 0.55 + 0.45 * self.rand(),
            seed: (self.rand() * 4096.0).floor(),
        };
        self.gusts.push(gust);
    }

    /// The route's point `s` metres from the start, and its direction there.
    fn route_at(&self, s: f32) -> Option<(Vec3, Vec3)> {
        let k = self.along.partition_point(|&a| a < s);
        if k == 0 || k >= self.route.len() {
            return None;
        }
        let (a, b) = (self.route[k - 1], self.route[k]);
        let span = self.along[k] - self.along[k - 1];
        let t = if span > 0.0 { (s - self.along[k - 1]) / span } else { 0.0 };
        Some((a.lerp(b, t), (b - a).normalize_or_zero()))
    }

    /// How deep the camera stands in a gust: a light veil over the view (0 to 1).
    fn veil(&self) -> f32 {
        let Some(eye) = self.eye else { return 0.0 };
        self.gusts
            .iter()
            .map(|g| {
                let d = eye - g.centre();
                let dir = Vec3::new(g.dir.x, 0.0, g.dir.y);
                let side = Vec3::new(-g.dir.y, 0.0, g.dir.x);
                let local = Vec3::new(
                    d.dot(dir) / (0.5 * g.length),
                    (eye.y - g.origin.y) / (1.6 * g.height),
                    d.dot(side) / (0.5 * g.width),
                );
                g.presence() * (1.0 - smoothstep(0.4, 1.0, local.length()))
            })
            .fold(0.0, f32::max)
    }

    pub fn uniforms(&self) -> WeatherUniforms {
        let speed = self.wind.length();
        let dir = self.wind.normalize_or(self.from_storm);
        WeatherUniforms {
            wind: [dir.x, dir.y, speed, self.time.unwrap_or(0.0)],
            drift: [self.drift.x as f32, self.drift.y as f32, self.climate.grains, self.veil()],
            eye_vel: self.eye_vel.extend(0.0).to_array(),
        }
    }

    pub fn instances(&self) -> Vec<GustInstance> {
        self.gusts
            .iter()
            .map(|g| GustInstance {
                origin: [g.origin.x, g.origin.y, g.origin.z, g.seed],
                travel: [g.dir.x, g.dir.y, g.speed, g.cross],
                time: [g.age, g.life, g.length, g.width],
                shape: [g.height, g.strength, 0.0, 0.0],
            })
            .collect()
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> Vec<Vec3> {
        (0..200).map(|i| Vec3::new(0.0, 5.0, i as f32 * 4.0)).collect()
    }

    #[test]
    fn gusts_cross_the_road_ahead_of_a_moving_camera() {
        let mut w = Weather::new(Climate { gust_every: (0.5, 0.5), ..Climate::MARS }, &straight(), Vec2::new(1.0, 0.0));
        let mut seen = 0;
        for f in 0..600 {
            let t = f as f32 / 60.0;
            // 30 m/s up the route.
            w.update(t, Vec3::new(0.0, 7.0, 30.0 * t));
            for g in &w.gusts {
                assert!(g.origin.z > 30.0 * (t - g.age), "spawned ahead of the camera");
                assert!(g.origin.x.abs() <= 3.0 && (g.origin.y - 5.0).abs() < 1e-3, "on the road");
            }
            seen = seen.max(w.gusts.len());
        }
        assert!(seen >= 3, "several gusts at once ({seen})");
        assert!(w.gusts.len() <= MAX_GUSTS);
        assert!((w.eye_vel.z - 30.0).abs() < 1.0, "camera velocity {:?}", w.eye_vel);
    }

    #[test]
    fn still_air_draws_nothing() {
        let mut w = Weather::new(Climate::STILL, &straight(), Vec2::new(1.0, 0.0));
        for f in 0..600 {
            w.update(f as f32 / 60.0, Vec3::new(0.0, 7.0, 0.0));
        }
        assert!(w.gusts.is_empty());
        assert_eq!(w.uniforms().drift[2], 0.0);
    }
}
