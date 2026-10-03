//! Tyre marks, TrackMania style: nothing while the tyres grip, smooth parallel marks near
//! the limit, wider blurred marks that stop being parallel once the car slides.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use physics::{CarParams, CarState, WheelState};
use track::Surface;

use crate::car_model::{SKI_WIDTH, SKI_X, TYRE_WIDTH};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct MarkVertex {
    pub pos: [f32; 3],
    pub across: f32,
    pub alpha: f32,
    pub blur: f32,
    pub surface: u32,
}

/// Quads kept in the ring buffer (6 vertices each).
pub const CAPACITY: usize = 12_000;
/// Distance between two mark segments.
const STEP: f32 = 0.35;
/// Lift above the ground so marks never z-fight with it.
const LIFT: f32 = 0.025;

#[derive(Clone, Copy)]
struct Head {
    left: Vec3,
    right: Vec3,
    point: Vec3,
    alpha: f32,
    blur: f32,
}

pub struct Marks {
    heads: [Option<Head>; 4],
    /// Next quad to overwrite.
    cursor: usize,
    /// Quads written since the last upload: (first quad, vertices).
    pending: Vec<(usize, Vec<MarkVertex>)>,
    pub cleared: bool,
}

/// How visible a wheel's mark is (0..1) and how blurred (0..1).
fn mark_of(w: &WheelState) -> (f32, f32) {
    (w.mark, w.smear)
}

impl Marks {
    pub fn new() -> Self {
        Self { heads: [None; 4], cursor: 0, pending: Vec::new(), cleared: true }
    }

    pub fn clear(&mut self) {
        self.heads = [None; 4];
        self.cursor = 0;
        self.pending.clear();
        self.cleared = true;
    }

    /// Breaks every strip (respawn): the next marks start fresh where the car is.
    pub fn break_strips(&mut self) {
        self.heads = [None; 4];
    }

    /// Call once per physics tick with the car state after the tick. A car on skis leaves its
    /// front marks where its skis are drawn (closer together than the physics' contacts), as
    /// narrow as the skis.
    pub fn update(&mut self, state: &CarState, params: &CarParams) {
        for (i, w) in state.wheels.iter().enumerate() {
            let (intensity, smear) = mark_of(w);
            let surface = w.surface.unwrap_or(Surface::Road);
            if !w.contact || intensity < 0.03 || surface == Surface::Wall {
                self.heads[i] = None;
                continue;
            }
            let n = w.contact_normal.normalize_or(Vec3::Y);
            let ski = params.front_skis && i < 2;
            let mut point = w.contact_point + n * LIFT;
            if ski {
                let side = if i == 0 { 1.0 } else { -1.0 };
                point -= state.rotation * Vec3::X * (side * (params.track_width * 0.5 - SKI_X));
            }
            let width = if ski { SKI_WIDTH } else { TYRE_WIDTH };
            let Some(head) = self.heads[i] else {
                self.heads[i] = Some(self.head_at(point, n, state, intensity, smear, width, None));
                continue;
            };
            let travel = point - head.point;
            if travel.length() < STEP {
                continue;
            }
            if travel.length() > 4.0 {
                // Teleport (respawn) or a jump: start a new strip.
                self.heads[i] = Some(self.head_at(point, n, state, intensity, smear, width, None));
                continue;
            }
            let next = self.head_at(point, n, state, intensity, smear, width, Some(travel));
            let kind = surface as u32;
            let v = |p: Vec3, across: f32, alpha: f32, blur: f32| MarkVertex { pos: p.to_array(), across, alpha, blur, surface: kind };
            let quad = vec![
                v(head.left, -1.0, head.alpha, head.blur),
                v(head.right, 1.0, head.alpha, head.blur),
                v(next.right, 1.0, next.alpha, next.blur),
                v(head.left, -1.0, head.alpha, head.blur),
                v(next.right, 1.0, next.alpha, next.blur),
                v(next.left, -1.0, next.alpha, next.blur),
            ];
            self.pending.push((self.cursor, quad));
            self.cursor = (self.cursor + 1) % CAPACITY;
            self.heads[i] = Some(next);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn head_at(&self, point: Vec3, n: Vec3, state: &CarState, intensity: f32, smear: f32, width: f32, travel: Option<Vec3>) -> Head {
        // Marks run along the direction the tyre moves; when it slides they get wider, as
        // the tyre's footprint is dragged sideways.
        let forward = state.rotation * Vec3::Z;
        let dir = travel.unwrap_or(forward).normalize_or(forward);
        let across = n.cross(dir).normalize_or(state.rotation * Vec3::X);
        let half = width * 0.45 * (1.0 + 1.2 * smear);
        Head { left: point + across * half, right: point - across * half, point, alpha: intensity, blur: smear }
    }

    /// New geometry to upload: (byte offset, vertices).
    pub fn take_pending(&mut self) -> Vec<(u64, Vec<MarkVertex>)> {
        std::mem::take(&mut self.pending)
            .into_iter()
            .map(|(quad, verts)| ((quad * 6 * std::mem::size_of::<MarkVertex>()) as u64, verts))
            .collect()
    }
}
