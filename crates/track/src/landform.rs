//! Landforms a map places close to its track: mesas, buttes and escarpments that frame the
//! circuit, where the terrain's own features ([`crate::terrain`]) only stand hundreds of metres
//! away.
//!
//! A landform is a flat-topped rock with a cliff and a talus apron, round, or stretched along its
//! yaw into an escarpment. Around the blocks it is cut and filled like ground worked for the
//! circuit:
//!
//! - next to a deck it never stands above the deck: a shelf at the deck's level runs [`SHELF`] m
//!   past the block's flat pad, then the rock rises in a cut at [`CUT_SLOPE`], so a road through
//!   a landform runs in a cutting;
//! - under an elevated deck it comes up to the lower deck edge, so the block stands on the rock
//!   instead of on walls (a start on top of a butte, a road down its flank);
//! - under a jump ramp, its landing and the gap between them it never rises at all.
//!
//! ```json
//! {"landform":"butte","position":[440.0,-470.0],"radius":40.0,"height":26.0}
//! {"landform":"mesa","position":[-150.0,480.0],"radius":55.0,"height":60.0,"length":420.0,"yaw":90.0}
//! ```

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::kit::smoothstep;
use crate::noise::{fbm, hash2};
use crate::terrain::{Capsule, PAD};

/// Past the flat pad around a block, the ground stays at the deck's level this far before a
/// landform rises, metres.
pub const SHELF: f32 = 3.0;
/// Slope of the cut where a landform meets a block (dy/dx, 54°).
pub const CUT_SLOPE: f32 = 1.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandformKind {
    /// A table of rock: flat top, a cliff, a broad talus apron out to about twice the radius.
    Mesa,
    /// A tower of rock: steeper and narrower cliff, a short apron.
    Butte,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// A landform placed by the map author.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Landform {
    pub landform: LandformKind,
    /// Centre `[x, z]`, metres.
    pub position: [f32; 2],
    /// Radius of the top to the middle of the cliff, metres.
    pub radius: f32,
    /// Height of the top above the plain, metres.
    pub height: f32,
    /// An escarpment's top is a stadium this much longer than wide, metres.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub length: f32,
    /// Direction of the length, degrees (0 along +Z, positive turns left, as a car's yaw).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub yaw: f32,
}

/// A landform ready to sample: its shape and the blocks it is cut and filled around.
pub(crate) struct Placed {
    c: Vec2,
    axis: Vec2,
    half_length: f32,
    r: f32,
    h: f32,
    /// Half-width of the cliff band, in radii.
    band: f32,
    /// The talus apron ends this many radii out.
    apron: f32,
    /// Share of the height in the cliff, the ledge at its foot and the talus.
    cliff_share: f32,
    ledge_share: f32,
    seed: u32,
    /// Footprints near enough to cut it.
    caps: Vec<Capsule>,
}

impl Landform {
    pub(crate) fn place(&self, caps: &[Capsule]) -> Placed {
        let r = self.radius.max(4.0);
        let (band, apron, cliff_share, ledge_share) = match self.landform {
            LandformKind::Mesa => ((6.0 / r).clamp(0.07, 0.25), 1.9, 0.68, 0.08),
            LandformKind::Butte => ((4.0 / r).clamp(0.06, 0.2), 1.55, 0.8, 0.06),
        };
        let yaw = self.yaw.to_radians();
        let seed = hash2(0x1a4d, libm::roundf(self.position[0] * 10.0) as i32, libm::roundf(self.position[1] * 10.0) as i32);
        let mut p = Placed {
            c: Vec2::new(self.position[0], self.position[1]),
            axis: Vec2::new(libm::sinf(yaw), libm::cosf(yaw)),
            half_length: 0.5 * self.length.max(0.0),
            r,
            h: self.height.max(0.0),
            band,
            apron,
            cliff_share,
            ledge_share,
            seed,
            caps: Vec::new(),
        };
        // A footprint cuts the landform as far as its cut can climb to the top.
        let range = p.reach() + PAD + SHELF + p.h / CUT_SLOPE;
        p.caps = caps.iter().filter(|c| p.axis_distance(c.a.lerp(c.b, 0.5)) < range + c.r + 0.5 * c.a.distance(c.b)).copied().collect();
        p
    }
}

impl Placed {
    /// Nothing of the landform lies farther than this from its axis, metres.
    pub(crate) fn reach(&self) -> f32 {
        (self.apron + 0.25) * self.r
    }

    /// Horizontal distance from `p` to the landform's axis (its centre, or the segment of its
    /// length).
    fn axis_distance(&self, p: Vec2) -> f32 {
        let d = p - self.c;
        let t = d.dot(self.axis).clamp(-self.half_length, self.half_length);
        (d - self.axis * t).length()
    }

    pub(crate) fn covers(&self, p: Vec2) -> bool {
        self.axis_distance(p) < self.reach()
    }

    /// Largest terrain leaf wanted over the landform, metres: enough for the error-driven
    /// refinement of the terrain mesh to find the cliff band.
    pub(crate) fn leaf_size(&self) -> f32 {
        (self.r / 6.0).clamp(4.0, 16.0)
    }

    /// Height above the plain and rockiness (0 dust, 1 bare cliff) of the uncut shape.
    fn shape(&self, p: Vec2) -> (f32, f32) {
        let s = 0.7 * self.r;
        let warp = Vec2::new(fbm(self.seed, p.x / s, p.y / s, 3), fbm(self.seed ^ 0x55, p.x / s, p.y / s, 3)) * (0.2 * self.r);
        // An escarpment's face also wanders on the scale of its length.
        let wide = if self.half_length > 0.0 { fbm(self.seed ^ 0x99, p.x / (3.0 * self.r), p.y / (3.0 * self.r), 2) * 0.35 * self.r } else { 0.0 };
        let q = (self.axis_distance(p + warp) + wide) / self.r;
        let band = self.band;
        let cliff = 1.0 - smoothstep(1.0 - band, 1.0 + band, q);
        let ledge = 1.0 - smoothstep(1.0 + band, 1.2 + 2.0 * band, q);
        let talus = 1.0 - smoothstep(0.85, self.apron, q);
        let top = 0.03 * fbm(self.seed ^ 0xaa, p.x / 35.0, p.y / 35.0, 3) * cliff;
        let talus_share = 1.0 - self.cliff_share - self.ledge_share;
        // An escarpment's skyline rises and dips along its length.
        let skyline = if self.half_length > 0.0 { 1.0 + 0.25 * fbm(self.seed ^ 0x33, (p - self.c).dot(self.axis) / (2.5 * self.r), 0.37, 2) } else { 1.0 };
        let h = self.h * skyline * (self.cliff_share * cliff + self.ledge_share * ledge + talus_share * talus * talus + top);
        (h.max(0.0), smoothstep(0.3, 0.9, cliff) * (1.0 - smoothstep(0.98, 1.0, cliff)))
    }

    /// Height the blocks let the landform reach at `p`, above the plain: the deck's lower edge on
    /// the pad and the shelf beyond it, then up the cut.
    fn allowed(&self, p: Vec2) -> f32 {
        self.caps
            .iter()
            .map(|c| {
                let (d, low) = c.distance_and_low(p);
                low + CUT_SLOPE * (d - PAD - SHELF).max(0.0)
            })
            .fold(f32::MAX, f32::min)
    }

    /// Height above the plain and rockiness at `p`, cut and filled around the blocks.
    pub(crate) fn sample(&self, p: Vec2) -> (f32, f32) {
        if !self.covers(p) {
            return (0.0, 0.0);
        }
        let (h, rock) = self.shape(p);
        if h <= 0.0 {
            return (0.0, 0.0);
        }
        let cut = h.min(self.allowed(p).max(0.0));
        // The cut face is coloured by its slope; a shelf levelled into the cliff is not cliff.
        (cut, rock * (1.0 - smoothstep(0.3, 1.5, h - cut)))
    }
}
