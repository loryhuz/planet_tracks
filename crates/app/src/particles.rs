//! Martian dust kicked up by the wheels on dirt and off-track ground, and snow thrown aside by
//! the wheels ploughing through it (render only, not part of the deterministic simulation).

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use physics::CarState;
use track::Surface;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct ParticleVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: f32,
}

pub const CAPACITY: usize = 900;

#[derive(Clone, Copy)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    alpha: f32,
    /// Upward acceleration, m/s²: fine dust drifts up a little, lumps of snow fall back.
    lift: f32,
    /// Growth of the size, m/s.
    grow: f32,
}

pub struct Dust {
    particles: Vec<Particle>,
    seed: u32,
    carry: f32,
}

impl Dust {
    pub fn new() -> Self {
        Self { particles: Vec::with_capacity(CAPACITY), seed: 0x9e37_79b9, carry: 0.0 }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }

    /// Call once per physics tick (dt = 10 ms).
    pub fn update(&mut self, state: &CarState, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            p.vel *= 1.0 - 2.2 * dt;
            p.vel.y += p.lift * dt;
            p.pos += p.vel * dt;
            p.size += p.grow * dt;
        }
        self.particles.retain(|p| p.age < p.life);

        let speed = state.velocity.length();
        let left = state.rotation * Vec3::X;
        for (i, w) in state.wheels.iter().enumerate() {
            let loose = matches!(w.surface, Some(Surface::Dirt) | Some(Surface::Ground));
            if !w.contact || !loose || speed < 6.0 {
                continue;
            }
            if w.sink > 0.02 {
                // Ploughing through snow (every wheel, the skis too): lumps thrown aside and up
                // from the wheel's own side, falling back; a wing of them in a slide.
                let side = if i % 2 == 0 { left } else { -left };
                let rate = (speed / 30.0).min(2.0) * (1.0 + 4.0 * w.smear) * 32.0;
                self.carry += rate * dt;
                while self.carry >= 1.0 && self.particles.len() < CAPACITY {
                    self.carry -= 1.0;
                    let jitter = Vec3::new(self.rand() - 0.5, self.rand() - 0.5, self.rand() - 0.5);
                    let out = 3.0 + 6.0 * w.smear + 3.0 * self.rand();
                    let up = 2.0 + 2.5 * self.rand();
                    let life = 0.45 + 0.4 * self.rand();
                    let size = 0.25 + 0.3 * self.rand();
                    self.particles.push(Particle {
                        pos: w.contact_point + Vec3::Y * 0.1,
                        vel: state.velocity * 0.6 + side * out + Vec3::Y * up + jitter * 1.5,
                        age: 0.0,
                        life,
                        size,
                        alpha: 0.55 + 0.3 * w.smear,
                        lift: -14.0,
                        grow: 0.9,
                    });
                }
                self.carry = self.carry.min(1.0);
                continue;
            }
            if i < 2 {
                continue;
            }
            let rate = (speed / 40.0).min(1.5) * (0.8 + 3.0 * w.smear + 1.0 * w.mark) * 60.0;
            self.carry += rate * dt;
            while self.carry >= 1.0 && self.particles.len() < CAPACITY {
                self.carry -= 1.0;
                let jitter = Vec3::new(self.rand() - 0.5, self.rand(), self.rand() - 0.5);
                let alpha = 0.32 + 0.4 * w.smear.max(w.mark * 0.5);
                let life = 0.8 + 1.0 * self.rand();
                let size = 0.8 + 0.7 * self.rand();
                self.particles.push(Particle {
                    pos: w.contact_point + Vec3::Y * 0.2,
                    // Dust is dragged along in the car's wake, then slows down: it forms a
                    // plume behind the car instead of vanishing behind the camera.
                    vel: state.velocity * 0.7 + jitter * 3.0 + Vec3::Y * 1.2,
                    age: 0.0,
                    life,
                    size,
                    alpha,
                    // Fine dust drifts up a little before settling.
                    lift: 0.35,
                    grow: 2.6,
                });
            }
            self.carry = self.carry.min(1.0);
        }
    }

    /// Camera-facing quads.
    pub fn vertices(&self, right: Vec3, up: Vec3) -> Vec<ParticleVertex> {
        let mut out = Vec::with_capacity(self.particles.len() * 6);
        for p in &self.particles {
            let t = p.age / p.life;
            let alpha = p.alpha * (1.0 - t) * (t * 8.0).min(1.0);
            let (r, u) = (right * p.size, up * p.size);
            let corners = [
                (p.pos - r - u, [-1.0, -1.0]),
                (p.pos + r - u, [1.0, -1.0]),
                (p.pos + r + u, [1.0, 1.0]),
                (p.pos - r + u, [-1.0, 1.0]),
            ];
            for i in [0, 1, 2, 0, 2, 3] {
                out.push(ParticleVertex { pos: corners[i].0.to_array(), uv: corners[i].1, alpha });
            }
        }
        out
    }
}
