//! The block kit: a grid of cells and height levels, pieces that join at the middle of cell
//! edges, and the sweep that turns a chain of pieces into a triangle mesh.
//!
//! # Grid
//!
//! A cell is [`CELL`] = 32 m square and a level [`LEVEL`] = 8 m high. Cell `(i, k)` covers
//! `x ∈ [32 i, 32 i + 32]`, `z ∈ [32 k, 32 k + 32]`. Pieces join at [`Connector`]s: the middle of
//! a cell edge, on a whole level, flat, heading along a grid axis. The only exception is the lip
//! of a jump ramp, which faces a gap rather than another piece.
//!
//! Why these sizes, for a car about 4 m long and 2 m wide driven at 150-300 km/h:
//! - the road is 20 m wide (62.5 % of a cell): 5 car lengths, 10 car widths, room for a racing
//!   line and ghosts side by side, while each side of the cell keeps 6 m for the lip, the
//!   shoulder, the gate posts and some terrain between neighbouring pieces. Dirt is 28 m wide and
//!   wider still on the outside of turns, to drift through them: it overflows into the
//!   neighbouring cells, which a map keeps free (see [`crate::dirt`]);
//! - at 200-250 km/h a car crosses a cell in about half a second, so turns of 1, 2 and 3 cells
//!   (centreline radius 16, 48 and 80 m) span a hairpin to flat-out sweepers: the reference car
//!   turns at 9.5 m under 80 km/h and 24 m at 228 km/h (docs/feel-targets);
//! - one level over one cell is a 14° average slope (26° at its steepest), over two cells 7°
//!   (14°), over three cells 5° (9.5°); 8 m is also a fall that clearly means "off the track".
//!
//! # Pieces
//!
//! Every piece is a horizontal centreline (straight or circular arc) with a height profile, an
//! optional bank angle and a deck surface (road or dirt). The cross-section, from the left:
//! terrain ← skirt ← lip ← edge line | deck | edge line → lip → skirt → terrain. Ground-level
//! decks sit at y = 0 over a terrain at [`TERRAIN_Y`], with a gentle shoulder down to it; as a
//! road rises the shoulder becomes the vertical side of a slab and the lip grows to
//! [`LIP_HEIGHT`]. Higher still the slab leaves the ground: it is [`SLAB_DEPTH`] thick, closed
//! underneath, and stands on stilts of plastic tubes ([`crate::stilts`]), the colony's roads
//! being built like camping gear (art/roads/brief.md). Slope changes
//! are parabolic vertical curves, bank changes smootherstep ramps, so the deck is continuous in
//! position, heading, grade and bank at every join.
//!
//! A dirt deck is [`DIRT_HALF_WIDTH`] × 2 m wide and not quite regular: it rises and falls a
//! little, its camber wanders and unbanked turns are dished (see [`Placed::frame`]). Its turns
//! are banked on the outside only, the inside staying level ([`Frame::level_inside`]), and roll
//! no faster than [`DIRT_ROLL_RATE`]: the floor stays smooth enough that the suspension soaks it
//! up, never knocking the car about. On Mars
//! terrain ([`crate::map::Map::build`]) dirt is not swept at all but dug into the ground as a
//! corridor ([`crate::dirt`]); only jump ramps and landings keep a swept deck, bedded in the
//! terrain. Swept whole (the flat-terrain preview, [`Layout::build`]), dirt has no lip and its
//! sides slope down like a mound.

use core::f32::consts::{FRAC_PI_2, PI};

use glam::{Vec2, Vec3};

use crate::jump::LandingProfile;
use crate::mesh::{MeshBuilder, add_box, add_post, add_sandbag, add_tube};
use crate::noise::{hash2, perlin, unit};
use crate::{Pose, Surface, Track, TrackMesh, Trigger};

/// Horizontal size of a grid cell, metres.
pub const CELL: f32 = 32.0;
/// Height of one level, metres.
pub const LEVEL: f32 = 8.0;
/// Half the width of the road, which is 20 m wide.
pub const HALF_WIDTH: f32 = 10.0;
/// Natural irregularity of dirt decks (see [`Placed::frame`]): rise and fall, metres; wandering
/// camber and dish of unbanked turns, degrees.
pub const DIRT_WANDER_Y: f32 = 0.6;
pub const DIRT_CAMBER_DEG: f32 = 3.0;
pub const DIRT_DISH_DEG: f32 = 4.0;
/// Half the nominal width of a dirt track, which is 28 m wide: room to drift through a turn
/// side by side. A dirt corridor dug into the terrain is wider still on the outside of its turns
/// and irregular along its edges (see [`crate::dirt`]); a swept dirt piece (jump ramp, landing)
/// is exactly this wide.
pub const DIRT_HALF_WIDTH: f32 = 14.0;
/// White edge line painted on the road, inside the driving surface.
pub const LINE_WIDTH: f32 = 0.5;
/// Nominal width of the border along a road's edges, outside the driving surface (gates and
/// triggers clear it; see [`Edge`] for the borders themselves).
pub const LIP_WIDTH: f32 = 0.5;
/// Height of the barrier along a road's edges (the bumpers, [`Edge::Bumpers`]).
pub const LIP_HEIGHT: f32 = 0.5;
/// Radius of the tube a bumper is drawn as (over its square hull, see [`border_profile`]).
const BUMPER_TUBE: f32 = 0.3;
/// Width and height of a sandbag border's hull (what the car meets, see [`border_profile`]); the
/// bags drawn over it are laid by hand, every one different ([`sandbag_row`]).
const BAG_WIDTH: f32 = 0.95;
const BAG_HEIGHT: f32 = 0.44;
/// A road's border tapers off over this many metres before a dirt track, and grows back after.
pub const BORDER_TAPER: f32 = 6.0;
/// Shoulder sloping from a ground-level deck (y = 0) down to the terrain.
pub const SHOULDER_WIDTH: f32 = 2.5;
/// Thickness of the slab of a raised road, from the deck down to its underside (the deck panels
/// on their frame). A road edge less than 1 m above the terrain comes down to it; from 2.5 m up
/// the slab is this thick and stands on stilts, the gap opening smoothly between.
pub const SLAB_DEPTH: f32 = 0.8;
/// The terrain plane, just below ground-level decks.
pub const TERRAIN_Y: f32 = -0.25;
/// Side of the square terrain under the map.
pub const TERRAIN_SIZE: f32 = 2048.0;
/// Where the car waits on the start block, metres from the block's entry.
pub const START_POSE_S: f32 = 8.0;
const START_GATE_S: f32 = 16.0;
const GATE_POST_U: f32 = HALF_WIDTH + LIP_WIDTH + 1.2;
const GATE_POST_HALF: f32 = 0.5;
const GATE_BEAM_BOTTOM: f32 = 7.5;
const GATE_BEAM_HEIGHT: f32 = 1.2;
/// Gate posts of a dirt piece stand this far out from its centreline, on the banks.
const DIRT_GATE_POST_U: f32 = DIRT_HALF_WIDTH + 4.0;
/// Half extents of checkpoint and finish triggers on the road: across (deck, lips and a margin),
/// height, along the road (4 m thick: more than a tick of travel at 1400 km/h).
pub const TRIGGER_HALF: Vec3 = Vec3::new(HALF_WIDTH + LIP_WIDTH + 0.5, 4.5, 2.0);
/// The same on dirt, across the whole corridor floor at the gate.
pub const DIRT_TRIGGER_HALF: Vec3 = Vec3::new(DIRT_HALF_WIDTH + 2.0, 4.5, 2.0);
/// Trigger centres sit this far above the deck.
pub const TRIGGER_LIFT: f32 = 2.0;
/// Below the terrain: only reached by leaving the terrain square.
pub const FALL_LIMIT_Y: f32 = TERRAIN_Y - 20.0;

/// Flat vertex colours (linear RGB).
pub mod color {
    pub const ROAD: [f32; 3] = [0.20, 0.20, 0.21];
    pub const LINE: [f32; 3] = [0.80, 0.80, 0.78];
    /// The deck and lines of a road with bumpers along it ([`super::Edge::Bumpers`]): its tarp is
    /// strapped down rather than staked.
    pub const ROAD_STRAPPED: [f32; 3] = [0.20, 0.20, 0.25];
    pub const LINE_STRAPPED: [f32; 3] = [0.80, 0.80, 0.84];
    /// The strip between a road's deck and its border, and the floor under the border: ground
    /// to the car, the deck's tarp carried on to the border to the eye (strapped or not, as the
    /// deck).
    pub const VERGE: [f32; 3] = [0.21, 0.20, 0.21];
    pub const VERGE_STRAPPED: [f32; 3] = [0.21, 0.20, 0.26];
    /// What the car collides with but the renderer does not draw: the plain shape of a border,
    /// drawn as bags or a tube instead (see [`super::Edge`]).
    pub const HULL: [f32; 3] = [0.01, 0.99, 0.01];
    /// Swept dirt only shows on the flat-terrain preview; the renderer darkens it as driven
    /// earth, like a corridor floor.
    pub const DIRT: [f32; 3] = GROUND;
    pub const GROUND: [f32; 3] = [0.55, 0.22, 0.10];
    pub const LIP: [f32; 3] = [0.68, 0.68, 0.66];
    pub const WALL: [f32; 3] = [0.32, 0.30, 0.29];
    /// The slab of a raised road: its sides, open ends and underside, wrapped in tarp.
    pub const SLAB: [f32; 3] = [0.62, 0.60, 0.56];
    /// The plastic tubes of the stilts, signal red.
    pub const TUBE: [f32; 3] = [0.62, 0.035, 0.025];
    /// Collars and base plates of the stilts.
    pub const COLLAR: [f32; 3] = [0.70, 0.70, 0.67];
    /// Sandbags of regolith: rows along roads, stacks under the stilts.
    pub const SANDBAG: [f32; 3] = [0.50, 0.27, 0.13];
    /// Orange ratchet straps holding raised roads down to the ground.
    pub const STRAP: [f32; 3] = [0.80, 0.17, 0.02];
    /// Steel: stakes, buckles; and rusty steel.
    pub const STEEL: [f32; 3] = [0.45, 0.45, 0.46];
    pub const RUST: [f32; 3] = [0.30, 0.13, 0.06];
    /// The galvanised steel of the stakes, dulled by the dust.
    pub const STAKE: [f32; 3] = [0.40, 0.40, 0.41];
    /// The face of a dirt kicker or landing over its trench.
    pub const EARTH_FACE: [f32; 3] = [0.40, 0.16, 0.075];
    pub const START: [f32; 3] = [0.10, 0.60, 0.15];
    pub const CHECKPOINT: [f32; 3] = [0.05, 0.35, 0.90];
    pub const FINISH: [f32; 3] = [0.90, 0.10, 0.05];
}

/// Half the width of a deck of this surface.
pub fn half_width(deck: Surface) -> f32 {
    match deck {
        Surface::Dirt => DIRT_HALF_WIDTH,
        _ => HALF_WIDTH,
    }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn smootherstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * t * (t * (6.0 * t - 15.0) + 10.0)
}

/// `(sin, cos)`, exact at multiples of a quarter turn so pieces meet exactly on the grid.
fn sin_cos(angle: f32) -> (f32, f32) {
    let q = angle / FRAC_PI_2;
    let r = libm::roundf(q);
    if (q - r).abs() < 1e-5 {
        match (r as i32).rem_euclid(4) {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        }
    } else {
        (libm::sinf(angle), libm::cosf(angle))
    }
}

fn wrap_angle(a: f32) -> f32 {
    let mut a = a;
    while a > PI {
        a -= 2.0 * PI;
    }
    while a <= -PI {
        a += 2.0 * PI;
    }
    a
}

/// A grid direction. Yaw 0 faces +Z ("north"); +X is to the left of a car facing +Z, so seen
/// from above with north up, west (+X) is on the left and east (−X) on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heading {
    North,
    West,
    South,
    East,
}

impl Heading {
    fn index(self) -> i32 {
        match self {
            Heading::North => 0,
            Heading::West => 1,
            Heading::South => 2,
            Heading::East => 3,
        }
    }

    fn from_index(i: i32) -> Self {
        match i.rem_euclid(4) {
            0 => Heading::North,
            1 => Heading::West,
            2 => Heading::South,
            _ => Heading::East,
        }
    }

    /// Turned by a number of quarter turns, positive to the left.
    pub fn turned(self, quarters: i32) -> Self {
        Self::from_index(self.index() + quarters)
    }

    pub fn yaw(self) -> f32 {
        match self {
            Heading::North => 0.0,
            Heading::West => FRAC_PI_2,
            Heading::South => PI,
            Heading::East => -FRAC_PI_2,
        }
    }

    pub fn forward(self) -> Vec3 {
        match self {
            Heading::North => Vec3::Z,
            Heading::West => Vec3::X,
            Heading::South => Vec3::NEG_Z,
            Heading::East => Vec3::NEG_X,
        }
    }

    pub fn left(self) -> Vec3 {
        self.turned(1).forward()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn sign(self) -> f32 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }

    fn quarters(self) -> i32 {
        match self {
            Side::Left => 1,
            Side::Right => -1,
        }
    }
}

/// The line a banked deck rotates about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pivot {
    /// The centreline keeps its height: the inner edge dips, the outer edge rises (platforms).
    Centre,
    /// Berms at ground level: on a road the inner edge keeps its height and the outer edge rises;
    /// on dirt the deck pivots half way in, so the middle of the track rises half as much and the
    /// inside dips into the ground it is dug in (see [`berm_pivot`]).
    Inner,
}

/// Longest ramp of a turn's bank, in and out, metres.
pub const BANK_RAMP: f32 = 48.0;
/// A dirt deck never rolls faster than this, radians of bank per metre along it: as fast as a
/// two-cell banked road turn rolls into its 18°. A shorter dirt berm gets less bank (6° on one
/// cell), so its edges rise and fall smoothly instead of standing up under the wheels.
pub const DIRT_ROLL_RATE: f32 = 0.016;
/// Steepest slope of a smootherstep from 0 to 1 over a unit length.
const SMOOTHERSTEP_SLOPE: f32 = 1.875;
/// On a dirt turn the deck is only banked outside its pivot; across this width, centred on the
/// pivot, it bends from level to the bank (see [`Frame::level_inside`]), metres.
pub const DIRT_KINK: f32 = 6.0;

/// How far inside the centreline a ground-level berm ([`Pivot::Inner`]) pivots, metres.
pub fn berm_pivot(deck: Surface) -> f32 {
    match deck {
        Surface::Dirt => 0.5 * HALF_WIDTH,
        _ => HALF_WIDTH,
    }
}

/// Shape of a piece. Lengths are counted in cells, heights in levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Straight { cells: u32 },
    /// A circular arc of `quarters` × 90° whose centreline radius is `(size − ½)` cells: it fills
    /// `size × size` cells (a quarter) or `2·size × size` cells (a U-turn). `bank_deg` tilts the
    /// deck towards the inside, ramping in and out within the turn.
    Turn { size: u32, side: Side, quarters: u32, bank_deg: f32, pivot: Pivot },
    /// Climb (`levels` > 0) or descent over `cells`, flat at both ends. A parabolic sag on the
    /// low 35 % and a longer parabolic crest on the high 65 %, so crests stay gentle.
    Slope { cells: u32, levels: i32 },
    /// Cosine bumps along a straight, `height` metres tall.
    Whoops { cells: u32, bumps: u32, height: f32 },
    /// One cell whose second half has a different deck (road ↔ dirt), at ground level.
    Transition { to: Surface },
    /// One cell rising along a parabolic sag to a lip at `lip_deg`; ends on a gap.
    JumpRamp { lip_deg: f32 },
    /// The descent that catches a jump: `cells` from the lip of the ramp before it (the first
    /// `gap` metres are empty), ending flat `levels` (negative) from the ramp's level. See
    /// [`crate::jump`]. With a `shift` of ±1 the deck bends one cell to the left (+1) or the
    /// right (−1) over its length, an S that ends on the grid one column over: a car flying
    /// straight off the lip lands on it if it drifts that way in the air, a little more the
    /// farther it flies. Its profile is the same, measured along the ramp's axis.
    Landing { cells: u32, levels: i32, gap: f32, epsilon: f32, outrun: f32, shift: i32 },
}

impl Kind {
    pub const fn turn(size: u32, side: Side) -> Self {
        Kind::Turn { size, side, quarters: 1, bank_deg: 0.0, pivot: Pivot::Centre }
    }

    /// A quarter turn banked about its centreline (for elevated roads).
    pub const fn banked(size: u32, side: Side, bank_deg: f32) -> Self {
        Kind::Turn { size, side, quarters: 1, bank_deg, pivot: Pivot::Centre }
    }

    /// A ground-level banked turn: the outside rises, the inside stays on the ground.
    pub const fn berm(size: u32, side: Side, quarters: u32, bank_deg: f32) -> Self {
        Kind::Turn { size, side, quarters, bank_deg, pivot: Pivot::Inner }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    Start,
    Checkpoint,
    Finish,
}

/// How the edges of a road are finished: the colony builds its roads from what it has
/// (art/roads/brief.md). Dirt has no border.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    /// Bumpers on a piece that leaves the ground, sandbags on one that stays on it.
    #[default]
    Auto,
    /// A row of sandbags along each side, stakes pinning the tarp just inside it.
    Sandbags,
    /// Red and white inflatable tubes along each side, the tarp strapped down.
    Bumpers,
}

/// A piece as the map designer picks it: a shape, a deck, the finish of a road's edges and
/// maybe a gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    /// Road or dirt (at the entry, for a transition).
    pub deck: Surface,
    pub gate: Option<Gate>,
    pub edge: Edge,
}

impl Piece {
    pub fn road(kind: Kind) -> Self {
        Self { kind, deck: Surface::Road, gate: None, edge: Edge::Auto }
    }

    pub fn dirt(kind: Kind) -> Self {
        Self { kind, deck: Surface::Dirt, gate: None, edge: Edge::Auto }
    }

    pub fn gate(mut self, gate: Gate) -> Self {
        self.gate = Some(gate);
        self
    }
}

/// Where two pieces meet: the centre of the deck on the joining edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Connector {
    pub pos: Vec3,
    pub heading: Heading,
    /// dy/ds of the deck; 0 on every grid join, the lip grade at the end of a jump ramp.
    pub grade: f32,
}

impl Connector {
    /// The join a car crosses when it enters cell `(i, k)` at `level`, heading `heading`.
    pub fn entering(cell: (i32, i32), level: i32, heading: Heading) -> Self {
        let centre = Vec3::new((cell.0 as f32 + 0.5) * CELL, level as f32 * LEVEL, (cell.1 as f32 + 0.5) * CELL);
        Self { pos: centre - heading.forward() * (0.5 * CELL), heading, grade: 0.0 }
    }

    /// The level, when the connector sits on a whole one.
    pub fn level(&self) -> Option<i32> {
        let l = self.pos.y / LEVEL;
        let r = libm::roundf(l);
        ((l - r).abs() < 1e-4).then_some(r as i32)
    }

    /// Middle of a cell edge, on a whole level, flat.
    pub fn is_on_grid(&self) -> bool {
        let multiple = |v: f32| {
            let q = v / CELL;
            (q - libm::roundf(q)).abs() < 1e-5
        };
        let (along, across) = match self.heading {
            Heading::North | Heading::South => (self.pos.z, self.pos.x),
            Heading::West | Heading::East => (self.pos.x, self.pos.z),
        };
        multiple(along) && multiple(across - 0.5 * CELL) && self.level().is_some() && self.grade == 0.0
    }
}

/// Centreline radius of a turn of `size` cells.
pub fn turn_radius(size: u32) -> f32 {
    (size as f32 - 0.5) * CELL
}

/// Height of a slope piece at `s` (0 at the entry): parabolic sag on the low 35 %, parabolic
/// crest on the high 65 %, flat at both ends.
fn slope_height(s: f32, length: f32, rise: f32) -> f32 {
    let h = rise.abs();
    let a = 0.35 * length;
    let b = length - a;
    let m = 2.0 * h / length;
    let climb = |s: f32| if s <= a { m * s * s / (2.0 * a) } else { h - m * (length - s) * (length - s) / (2.0 * b) };
    if rise >= 0.0 { climb(s) } else { climb(length - s) - h }
}

/// Radii of the vertical curves of a slope piece: `(sag, crest)`, metres.
pub fn slope_radii(cells: u32, levels: i32) -> (f32, f32) {
    let length = cells as f32 * CELL;
    let m = 2.0 * (levels.abs() as f32 * LEVEL) / length;
    (0.35 * length / m, 0.65 * length / m)
}

/// The deck at one point along a piece, in world space.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Centreline position projected to y = 0.
    pub horiz: Vec3,
    /// Horizontal unit vectors.
    pub forward: Vec3,
    pub left: Vec3,
    pub yaw: f32,
    /// Bank angle, positive raises the left edge.
    pub bank: f32,
    /// Lateral offset (positive left) and height of the line the deck is banked about.
    pub pivot_u: f32,
    pub pivot_y: f32,
    pub deck: Surface,
    /// A dirt turn's deck is banked only on the outside of its pivot: inside it stays level at
    /// the pivot's height, the bend rounded over [`DIRT_KINK`] m. Its inside never dips below the
    /// ground and the floors around it (on a one-cell turn, those of the pieces before and after
    /// it), and its centreline keeps the height a plane would give it.
    pub level_inside: bool,
    /// The finish of the road's edges (never `Auto`), and how much of the border stands here
    /// (0..1, see [`BORDER_TAPER`]).
    pub edge: Edge,
    pub border: f32,
}

impl Frame {
    /// Unit vector across the (banked) deck, pointing left.
    pub fn lateral(&self) -> Vec3 {
        self.left * libm::cosf(self.bank) + Vec3::Y * libm::sinf(self.bank)
    }

    /// Deck normal, ignoring the grade.
    pub fn up(&self) -> Vec3 {
        Vec3::Y * libm::cosf(self.bank) - self.left * libm::sinf(self.bank)
    }

    /// The deck surface `u` metres left of the centreline.
    pub fn deck_point(&self, u: f32) -> Vec3 {
        if !self.level_inside {
            return self.horiz + self.left * self.pivot_u + Vec3::Y * self.pivot_y + self.lateral() * (u - self.pivot_u);
        }
        let h = self.pivot_u + (u - self.pivot_u) * self.squeeze(u - self.pivot_u);
        self.horiz + self.left * h + Vec3::Y * self.deck_height(h).0
    }

    /// How much a length of deck at `w` metres left of the pivot shrinks seen from above: by the
    /// cosine of the bank where it is banked.
    fn squeeze(&self, w: f32) -> f32 {
        if self.level_inside && w * self.bank < 0.0 { 1.0 } else { libm::cosf(self.bank) }
    }

    /// Horizontal offsets of the deck's left and right edges, for a deck `hw` metres to each side.
    pub fn deck_edges(&self, hw: f32) -> (f32, f32) {
        let edge = |u: f32| self.pivot_u + (u - self.pivot_u) * self.squeeze(u - self.pivot_u);
        (edge(hw), edge(-hw))
    }

    /// Height of the deck surface `h` metres left of the centreline (horizontally, extended past
    /// its edges), and its slope going left (dy/dh).
    pub fn deck_height(&self, h: f32) -> (f32, f32) {
        let t = libm::tanf(self.bank);
        let w = h - self.pivot_u;
        if !self.level_inside {
            return (self.pivot_y + w * t, t);
        }
        // Distance towards the raised side, and the bank's rise along it, rounded at the pivot.
        let x = w * t.signum();
        let e = (x + 0.5 * DIRT_KINK).max(0.0);
        let (rise, slope) = if e < DIRT_KINK { (e * e / (2.0 * DIRT_KINK), e / DIRT_KINK) } else { (e - 0.5 * DIRT_KINK, 1.0) };
        (self.pivot_y + t.abs() * rise, t * slope)
    }

    pub fn centre(&self) -> Vec3 {
        self.deck_point(0.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct Local {
    x: f32,
    z: f32,
    turn: f32,
    y: f32,
    bank: f32,
    pivot_u: f32,
    deck: Surface,
}

/// A piece placed on the grid.
#[derive(Clone, Debug)]
pub struct Placed {
    pub piece: Piece,
    pub entry: Connector,
    pub exit: Connector,
    /// Horizontal length of the centreline, metres.
    pub length: f32,
    landing: Option<LandingProfile>,
}

impl Placed {
    pub fn new(piece: Piece, entry: Connector) -> Self {
        let cells_len = |cells: u32| cells as f32 * CELL;
        let (length, landing) = match piece.kind {
            Kind::Straight { cells } | Kind::Slope { cells, .. } | Kind::Whoops { cells, .. } => (cells_len(cells), None),
            Kind::Transition { .. } | Kind::JumpRamp { .. } => (CELL, None),
            Kind::Turn { size, quarters, .. } => (turn_radius(size) * quarters as f32 * FRAC_PI_2, None),
            Kind::Landing { cells, levels, gap, epsilon, outrun, .. } => {
                let length = cells_len(cells);
                let base = libm::floorf(entry.pos.y / LEVEL + 1e-4) as i32;
                let end_y = (base + levels) as f32 * LEVEL;
                let drop = entry.pos.y - end_y;
                (length, Some(LandingProfile::new(entry.grade, gap, epsilon, outrun, length, drop)))
            }
        };
        let mut placed = Self { piece, entry, exit: entry, length, landing };
        placed.exit = placed.compute_exit();
        placed
    }

    fn compute_exit(&self) -> Connector {
        let f = self.entry.heading.forward();
        let l = self.entry.heading.left();
        let (pos_h, heading) = match self.piece.kind {
            Kind::Turn { size, side, quarters, .. } => {
                let r = turn_radius(size);
                let sg = side.sign();
                if quarters >= 2 {
                    (self.entry.pos + l * (2.0 * sg * r), self.entry.heading.turned(2 * side.quarters()))
                } else {
                    (self.entry.pos + l * (sg * r) + f * r, self.entry.heading.turned(side.quarters()))
                }
            }
            Kind::Landing { shift, .. } => (self.entry.pos + f * self.length + l * (shift as f32 * CELL), self.entry.heading),
            _ => (self.entry.pos + f * self.length, self.entry.heading),
        };
        let y = match self.piece.kind {
            Kind::Landing { .. } => self.entry.pos.y - self.landing.unwrap().drop,
            _ => self.entry.pos.y + self.local(self.length).y,
        };
        let grade = match self.piece.kind {
            Kind::JumpRamp { lip_deg } => libm::tanf(lip_deg.to_radians()),
            _ => 0.0,
        };
        Connector { pos: Vec3::new(pos_h.x, y, pos_h.z), heading, grade }
    }

    /// The jump profile, for a landing piece.
    pub fn landing(&self) -> Option<LandingProfile> {
        self.landing
    }

    /// Part of `[0, length]` with a deck (a landing starts after its gap).
    pub fn deck_range(&self) -> (f32, f32) {
        match self.piece.kind {
            Kind::Landing { gap, .. } => (gap, self.length),
            _ => (0.0, self.length),
        }
    }

    /// Part of the deck dug into the terrain as a dirt corridor (see [`crate::dirt`]) rather
    /// than swept as a mesh, when the map is built on Mars terrain: dirt straights, turns,
    /// slopes and whoops, and the dirt half of a transition. Jump ramps and landings stay swept
    /// (a lip and a landing edge need sharp, exact geometry) and sit in the terrain; roads stay
    /// swept.
    pub fn carved_range(&self) -> Option<(f32, f32)> {
        let half = 0.5 * self.length;
        match (self.piece.kind, self.piece.deck) {
            (Kind::Transition { to: Surface::Dirt }, Surface::Road) => Some((half, self.length)),
            (Kind::Transition { to: Surface::Road }, Surface::Dirt) => Some((0.0, half)),
            (Kind::Straight { .. } | Kind::Turn { .. } | Kind::Slope { .. } | Kind::Whoops { .. }, Surface::Dirt) => {
                Some((0.0, self.length))
            }
            _ => None,
        }
    }

    /// Part of the deck swept as a mesh on Mars terrain (the rest is carved), if any.
    pub fn swept_range(&self) -> Option<(f32, f32)> {
        let (d0, d1) = self.deck_range();
        match self.carved_range() {
            None => Some((d0, d1)),
            Some((c0, _)) if c0 > d0 + 1e-3 => Some((d0, c0)),
            Some((_, c1)) if c1 < d1 - 1e-3 => Some((c1, d1)),
            Some(_) => None,
        }
    }

    fn local(&self, s: f32) -> Local {
        let mut l = Local { x: 0.0, z: s, turn: 0.0, y: 0.0, bank: 0.0, pivot_u: 0.0, deck: self.piece.deck };
        match self.piece.kind {
            Kind::Straight { .. } => {}
            Kind::Slope { levels, .. } => l.y = slope_height(s, self.length, levels as f32 * LEVEL),
            Kind::Whoops { bumps, height, .. } => {
                l.y = 0.5 * height * (1.0 - libm::cosf(2.0 * PI * bumps as f32 * s / self.length));
            }
            Kind::Transition { to } => {
                if s >= 0.5 * self.length {
                    l.deck = to;
                }
            }
            Kind::JumpRamp { lip_deg } => {
                let t = libm::tanf(lip_deg.to_radians());
                l.y = t * s * s / (2.0 * self.length);
            }
            Kind::Landing { shift, gap, .. } => {
                let p = self.landing.unwrap();
                l.y = if s >= self.length { -p.drop } else { p.height(s) };
                if shift != 0 {
                    // The S: a smootherstep across one cell over the deck (past the gap, so the
                    // deck starts square to the ramp), `s` measured along the ramp's axis.
                    let (w, span) = (shift as f32 * CELL, self.length - gap);
                    let t = ((s - gap) / span).clamp(0.0, 1.0);
                    l.x = w * smootherstep(0.0, 1.0, t);
                    l.turn = libm::atanf(w * 30.0 * t * t * (1.0 - t) * (1.0 - t) / span);
                }
            }
            Kind::Turn { size, side, bank_deg, pivot, .. } => {
                let r = turn_radius(size);
                let sg = side.sign();
                let (sn, cs) = sin_cos(s / r);
                l.x = sg * r * (1.0 - cs);
                l.z = r * sn;
                l.turn = sg * s / r;
                if bank_deg != 0.0 {
                    // The bank builds up over half the turn (at most BANK_RAMP), so the edges,
                    // and the middle of a berm, rise and fall gently.
                    let ramp = (0.5 * self.length).min(BANK_RAMP);
                    let k = smootherstep(0.0, ramp, s) * smootherstep(0.0, ramp, self.length - s);
                    let mut bank = bank_deg.to_radians();
                    if self.piece.deck == Surface::Dirt {
                        bank = bank.min(DIRT_ROLL_RATE * ramp / SMOOTHERSTEP_SLOPE);
                    }
                    l.bank = -sg * bank * k;
                    if pivot == Pivot::Inner {
                        l.pivot_u = sg * berm_pivot(self.piece.deck);
                    }
                }
            }
        }
        let (dy, dbank) = self.wander(s);
        l.y += dy;
        l.bank += dbank;
        l
    }

    /// The natural irregularity of a dirt deck `s` metres along: a slow rise and fall of up to
    /// [`DIRT_WANDER_Y`] m, a camber wandering up to [`DIRT_CAMBER_DEG`]° on straights, a
    /// [`DIRT_DISH_DEG`]° dish towards the inside of unbanked turns (their outside rises). The rise and the camber fade
    /// out within 16 m of the ends (joins stay level, flat and straight); nothing on gates,
    /// transitions, banked turns, slopes and whoops (their shape is the design). Returns (rise,
    /// bank).
    fn wander(&self, s: f32) -> (f32, f32) {
        let wanders = match self.piece.kind {
            Kind::Straight { .. } => true,
            Kind::Turn { bank_deg, .. } => bank_deg == 0.0,
            _ => false,
        };
        if self.piece.deck != Surface::Dirt || self.piece.gate.is_some() || !wanders {
            return (0.0, 0.0);
        }
        let key = |v: f32| libm::roundf(v * 4.0) as i32;
        let seed = hash2(0x6469_7274 ^ self.entry.heading.index() as u32, key(self.entry.pos.x), key(self.entry.pos.z) ^ key(self.entry.pos.y));
        let w = smoothstep(0.0, 16.0, s) * smoothstep(0.0, 16.0, self.length - s);
        let dy = DIRT_WANDER_Y * w * perlin(seed, s / 70.0, 0.5);
        let bank = match self.piece.kind {
            Kind::Turn { side, .. } => {
                let t = libm::sinf(PI * s / self.length);
                -side.sign() * DIRT_DISH_DEG.to_radians() * t * t
            }
            _ => DIRT_CAMBER_DEG.to_radians() * w * perlin(seed.wrapping_add(1), s / 55.0, 0.5),
        };
        (dy, bank)
    }

    /// The deck `s` metres along the centreline (horizontal distance from the entry).
    pub fn frame(&self, s: f32) -> Frame {
        let l = self.local(s);
        let f0 = self.entry.heading.forward();
        let l0 = self.entry.heading.left();
        let (st, ct) = sin_cos(l.turn);
        Frame {
            horiz: Vec3::new(self.entry.pos.x, 0.0, self.entry.pos.z) + l0 * l.x + f0 * l.z,
            forward: f0 * ct + l0 * st,
            left: l0 * ct - f0 * st,
            yaw: wrap_angle(self.entry.heading.yaw() + l.turn),
            bank: l.bank,
            pivot_u: l.pivot_u,
            pivot_y: self.entry.pos.y + l.y,
            deck: l.deck,
            level_inside: self.piece.deck == Surface::Dirt && matches!(self.piece.kind, Kind::Turn { .. }),
            edge: self.edge(),
            border: self.border_at(s),
        }
    }

    /// The finish of the piece's road edges, `Auto` resolved: bumpers on a piece that leaves the
    /// ground (raised at either end, or a jump ramp or landing), sandbags on one that stays on it.
    pub fn edge(&self) -> Edge {
        match self.piece.edge {
            Edge::Auto => {
                let raised = self.entry.level().unwrap_or(1) > 0
                    || self.exit.level().unwrap_or(1) > 0
                    || matches!(self.piece.kind, Kind::JumpRamp { .. } | Kind::Landing { .. });
                if raised { Edge::Bumpers } else { Edge::Sandbags }
            }
            e => e,
        }
    }

    /// How much of the road's border stands at `s` (0..1): it tapers off over the last
    /// [`BORDER_TAPER`] m of road before a dirt track, and grows back after one.
    fn border_at(&self, s: f32) -> f32 {
        let half = 0.5 * self.length;
        match (self.piece.kind, self.piece.deck) {
            (Kind::Transition { to: Surface::Dirt }, Surface::Road) => 1.0 - smoothstep(half - BORDER_TAPER, half, s),
            (Kind::Transition { to: Surface::Road }, Surface::Dirt) => smoothstep(half, half + BORDER_TAPER, s),
            _ => 1.0,
        }
    }

    /// Grade (dy/ds) of the centreline, by finite differences.
    pub fn grade(&self, s: f32) -> f32 {
        let d = 0.02;
        let (a, b) = if s - d < 0.0 {
            (s, s + d)
        } else if s + d > self.length {
            (s - d, s)
        } else {
            (s - d, s + d)
        };
        (self.frame(b).centre().y - self.frame(a).centre().y) / (b - a)
    }

    fn cell_count(&self) -> u32 {
        match self.piece.kind {
            Kind::Straight { cells } | Kind::Slope { cells, .. } | Kind::Whoops { cells, .. } | Kind::Landing { cells, .. } => cells,
            Kind::Transition { .. } | Kind::JumpRamp { .. } => 1,
            Kind::Turn { size, quarters, .. } => size * size * quarters.min(2),
        }
    }

    /// The grid cells the piece occupies.
    pub fn cells(&self) -> Vec<(i32, i32)> {
        let f = self.entry.heading.forward();
        let l = self.entry.heading.left();
        let (lateral, along): (Vec<i32>, i32) = match self.piece.kind {
            Kind::Turn { size, side, quarters, .. } => {
                let n = size as i32;
                let w = if quarters >= 2 { 2 * n } else { n };
                ((0..w).map(|j| j * side.quarters()).collect(), n)
            }
            Kind::Landing { cells, shift, .. } if shift != 0 => (vec![0, shift.signum()], cells as i32),
            _ => (vec![0], self.cell_count() as i32),
        };
        let mut out = Vec::new();
        for i in 0..along {
            for &j in &lateral {
                let c = self.entry.pos + l * (j as f32 * CELL) + f * ((i as f32 + 0.5) * CELL);
                out.push((libm::floorf(c.x / CELL) as i32, libm::floorf(c.z / CELL) as i32));
            }
        }
        out
    }

    /// Where the gate stands, metres from the entry.
    pub fn gate_s(&self) -> f32 {
        match self.piece.gate {
            Some(Gate::Start) => START_GATE_S,
            _ => 0.5 * self.length,
        }
    }

    /// The trigger of this piece's gate.
    pub fn trigger(&self) -> Trigger {
        let f = self.frame(self.gate_s());
        let half_extents = if f.deck == Surface::Dirt { DIRT_TRIGGER_HALF } else { TRIGGER_HALF };
        Trigger { center: f.centre() + Vec3::Y * TRIGGER_LIFT, half_extents, yaw: f.yaw }
    }

    /// Sample positions along the deck: every half cell, then halved until the heading, grade,
    /// bank and height change little between samples.
    pub fn samples(&self) -> Vec<f32> {
        let (s0, s1) = self.deck_range();
        self.samples_in(s0, s1)
    }

    /// [`Placed::samples`] over part of the deck.
    pub fn samples_in(&self, s0: f32, s1: f32) -> Vec<f32> {
        let mut cuts = vec![s0];
        let mut s = 0.5 * CELL;
        while s < s1 - 1e-3 {
            if s > s0 + 1e-3 {
                cuts.push(s);
            }
            s += 0.5 * CELL;
        }
        match self.piece.kind {
            Kind::Landing { .. } => cuts.push(self.landing.unwrap().knee),
            Kind::Slope { .. } => cuts.push(if self.exit.pos.y >= self.entry.pos.y { 0.35 } else { 0.65 } * self.length),
            _ => {}
        }
        cuts.push(s1);
        cuts.retain(|&c| (s0..=s1).contains(&c));
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        let mut out = vec![cuts[0]];
        for w in cuts.windows(2) {
            self.refine(w[0], w[1], &mut out);
        }
        out
    }

    fn refine(&self, a: f32, b: f32, out: &mut Vec<f32>) {
        if self.needs_split(a, b) {
            let m = 0.5 * (a + b);
            self.refine(a, m, out);
            self.refine(m, b, out);
        } else {
            out.push(b);
        }
    }

    fn needs_split(&self, a: f32, b: f32) -> bool {
        if b - a < 0.5 {
            return false;
        }
        let (fa, fb, fm) = (self.frame(a), self.frame(b), self.frame(0.5 * (a + b)));
        if fa.forward.dot(fb.forward) < libm::cosf(3f32.to_radians()) {
            return true;
        }
        if (fa.bank - fb.bank).abs() > 0.01 || (self.grade(a) - self.grade(b)).abs() > 0.012 {
            return true;
        }
        let hw = half_width(fa.deck);
        [-hw, 0.0, hw].into_iter().any(|u| {
            let mid = 0.5 * (fa.deck_point(u).y + fb.deck_point(u).y);
            (fm.deck_point(u).y - mid).abs() > 0.01
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Cross-section and sweep.

/// Role of each strip between consecutive section points, from the left skirt to the right,
/// then back under the slab. The deck is split in four so a bank that changes between samples
/// barely twists it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Skirt,
    /// The border along a deck edge ([`Edge`]), a strip of its profile.
    Border,
    /// A flat strip of ground between the deck edge and its border, [`LIP_WIDTH`] wide where the
    /// road lies on the ground (cars hugging the edge never touch the border) and closing as the
    /// road rises (the border stands at the deck edge, the barrier raised roads always had); on
    /// dirt, the flat strip along the deck.
    Verge,
    Line,
    Deck,
    /// The underside of a raised slab, from the right skirt's foot to the left one's (collapsed
    /// where the slab sits on the ground).
    Under,
}

/// Points of a border's profile, from its inner foot out to its outer foot.
const BORDER_POINTS: usize = 7;
/// Index of the left deck edge among a section's points (after the left foot, border and verge).
const LEFT_EDGE: usize = BORDER_POINTS + 1;
/// Index of the right deck edge.
const RIGHT_EDGE: usize = LEFT_EDGE + 6;

const ROLES: [Role; 2 * BORDER_POINTS + 9] = {
    let mut r = [Role::Border; 2 * BORDER_POINTS + 9];
    r[0] = Role::Skirt;
    r[LEFT_EDGE - 1] = Role::Verge;
    r[LEFT_EDGE] = Role::Line;
    r[LEFT_EDGE + 1] = Role::Deck;
    r[LEFT_EDGE + 2] = Role::Deck;
    r[LEFT_EDGE + 3] = Role::Deck;
    r[LEFT_EDGE + 4] = Role::Deck;
    r[LEFT_EDGE + 5] = Role::Line;
    r[RIGHT_EDGE] = Role::Verge;
    r[2 * BORDER_POINTS + 7] = Role::Skirt;
    r[2 * BORDER_POINTS + 8] = Role::Under;
    r
};
const STRIPS: usize = ROLES.len();
/// The right skirt's foot (the left one's is the first point, and the last one closes the
/// section under the slab).
const RIGHT_FOOT: usize = STRIPS - 1;
/// The strips of the driving surface: lines and deck.
const DECK_STRIPS: core::ops::Range<usize> = LEFT_EDGE..LEFT_EDGE + 6;
/// A slab whose underside is less than this above the terrain sits on it: no underside.
const UNDER_MIN_GAP: f32 = 0.05;
/// Below this an edge counts as collapsed.
const EPS: f32 = 0.02;

#[derive(Clone, Copy)]
struct Section {
    /// Left skirt foot; the left border from its outer foot in to its inner foot; the left deck
    /// edge, the left line's inner edge, three deck points (quarter, centre, quarter), the right
    /// line's inner edge, the right deck edge; the right border from its inner foot out to its
    /// outer foot; the right skirt foot; and the left skirt foot again under the slab (the right
    /// one where the slab sits on the ground).
    pts: [Vec3; STRIPS + 1],
    /// Track coordinate across of each point (positive left), metres; along a border, the deck
    /// edge's plus the distance around the border's profile.
    u: [f32; STRIPS + 1],
    /// The normal each point's vertices get, or zero where the triangles give it: borders are
    /// shaded round.
    nrm: [Vec3; STRIPS + 1],
    /// The deck's normal (ignoring the grade), the finish of its edges (none on dirt) and how
    /// much of the border stands ([`Frame::border`]).
    up: Vec3,
    edge: Option<Edge>,
    border: f32,
}

/// A border's profile, from its inner foot out to its outer foot: points (across, up) from the
/// inner foot, metres, and their normals (across, up; zero: computed from the triangles).
/// `edge` is `None` on dirt (no border); `scale` shrinks the border toward its inner foot.
fn border_profile(edge: Option<Edge>, scale: f32) -> [([f32; 2], [f32; 2]); BORDER_POINTS] {
    let mut out = [([0.0; 2], [0.0; 2]); BORDER_POINTS];
    match edge {
        None => return out,
        Some(Edge::Bumpers) | Some(Edge::Auto) => {
            // For a car, exactly the barrier the roads always had, [`LIP_WIDTH`] by
            // [`LIP_HEIGHT`] with sharp edges: a rounded edge is a step a wheel climbs, and a car
            // rubbing the barrier on a raised road would ride over it. It is not drawn: the
            // renderer shows a round tube over it ([`border_visuals`]).
            let (w, h) = (LIP_WIDTH, LIP_HEIGHT);
            out = [([0.0, 0.0], [0.0; 2]), ([0.0, h], [0.0; 2]), ([0.0, h], [0.0; 2]), ([0.5 * w, h], [0.0; 2]), ([w, h], [0.0; 2]), ([w, h], [0.0; 2]), ([w, 0.0], [0.0; 2])];
        }
        Some(Edge::Sandbags) => {
            // The row of bags as a box, upright on the road side (a car slides along it rather
            // than climbing it); not drawn: the renderer shows the bags ([`border_visuals`]).
            let (w, h) = (BAG_WIDTH, BAG_HEIGHT);
            out = [([0.0, 0.0], [0.0; 2]), ([0.0, h], [0.0; 2]), ([0.0, h], [0.0; 2]), ([0.5 * w, h], [0.0; 2]), ([w, h], [0.0; 2]), ([w, h], [0.0; 2]), ([w, 0.0], [0.0; 2])];
        }
    }
    for q in out.iter_mut() {
        q.0 = [q.0[0] * scale, q.0[1] * scale];
    }
    out
}

/// Width of the verge between the deck edge and its border ([`Role::Verge`]), skirt width, and
/// height of the skirt's foot above the terrain (the underside of a raised slab, 0 where the side
/// comes down to the ground) for a deck edge `e` metres above the terrain.
fn side_params(deck: Surface, e: f32) -> (f32, f32, f32) {
    match deck {
        Surface::Dirt => (LIP_WIDTH, SHOULDER_WIDTH.max(1.2 * e), 0.0),
        _ => (
            LIP_WIDTH * (1.0 - smoothstep(0.3, 1.2, e)),
            SHOULDER_WIDTH * (1.0 - smoothstep(0.3, 1.5, e)),
            (e - SLAB_DEPTH).max(0.0) * smoothstep(1.0, 2.5, e),
        ),
    }
}

/// The feet of the slab's sides at frame `f`, left then right, where its underside stands at
/// least `min_gap` m above the terrain plane: the edges of the underside, which runs straight
/// between them. `None` where the slab sits on the ground (and on dirt).
pub(crate) fn underside(f: &Frame, min_gap: f32) -> Option<(Vec3, Vec3)> {
    let sec = Section::new(f);
    let (l, r) = (sec.pts[0], sec.pts[RIGHT_FOOT]);
    (l.y.min(r.y) >= TERRAIN_Y + min_gap).then_some((l, r))
}

impl Section {
    fn new(f: &Frame) -> Self {
        let lat = f.lateral();
        let up = f.up();
        let hw = half_width(f.deck);
        let el = f.deck_point(hw);
        let er = f.deck_point(-hw);
        let edge = (f.deck != Surface::Dirt).then_some(f.edge);
        let profile = border_profile(edge, f.border);
        // Distance around the profile from the deck edge, for the track coordinate across.
        let mut around = [0.0f32; BORDER_POINTS];
        for i in 1..BORDER_POINTS {
            let (a, b) = (profile[i - 1].0, profile[i].0);
            around[i] = around[i - 1] + libm::hypotf(b[0] - a[0], b[1] - a[1]);
        }
        let width = around[BORDER_POINTS - 1];
        let (verge_l, skirt_l, gap_l) = side_params(f.deck, el.y - TERRAIN_Y);
        let (verge_r, skirt_r, gap_r) = side_params(f.deck, er.y - TERRAIN_Y);
        let at = |edge: Vec3, out: Vec3, verge: f32, q: [f32; 2]| edge + out * (verge + q[0]) + up * q[1];
        let facing = |out: Vec3, n: [f32; 2]| if n == [0.0, 0.0] { Vec3::ZERO } else { (out * n[0] + up * n[1]).normalize() };
        let last = BORDER_POINTS - 1;
        let base_l = at(el, lat, verge_l, profile[last].0);
        let base_r = at(er, -lat, verge_r, profile[last].0);
        let foot = |p: Vec3, dir: Vec3, w: f32, gap: f32| Vec3::new(p.x, TERRAIN_Y + gap, p.z) + dir * w;
        let (foot_l, foot_r) = (foot(base_l, f.left, skirt_l, gap_l), foot(base_r, -f.left, skirt_r, gap_r));
        let raised = gap_l.min(gap_r) >= UNDER_MIN_GAP;
        let inner = hw - LINE_WIDTH;
        let mut pts = [Vec3::ZERO; STRIPS + 1];
        let mut u = [0.0; STRIPS + 1];
        let mut nrm = [Vec3::ZERO; STRIPS + 1];
        pts[0] = foot_l;
        u[0] = hw + verge_l + width + skirt_l;
        // Across the verge the track coordinate goes on from the deck edge's (the tarp ends at the
        // border's inner foot); along a border it is the inner foot's plus the distance around
        // the border.
        for i in 0..BORDER_POINTS {
            // Left: from the outer foot in; right: from the inner foot out.
            let (ql, qr) = (last - i, i);
            pts[1 + i] = at(el, lat, verge_l, profile[ql].0);
            u[1 + i] = hw + verge_l + around[ql];
            nrm[1 + i] = facing(lat, profile[ql].1);
            pts[RIGHT_EDGE + 1 + i] = at(er, -lat, verge_r, profile[qr].0);
            u[RIGHT_EDGE + 1 + i] = -hw - verge_r - around[qr];
            nrm[RIGHT_EDGE + 1 + i] = facing(-lat, profile[qr].1);
        }
        pts[LEFT_EDGE] = el;
        u[LEFT_EDGE] = hw;
        for (k, w) in [inner, 0.5 * inner, 0.0, -0.5 * inner, -inner].into_iter().enumerate() {
            pts[LEFT_EDGE + 1 + k] = f.deck_point(w);
            u[LEFT_EDGE + 1 + k] = w;
        }
        pts[RIGHT_EDGE] = er;
        u[RIGHT_EDGE] = -hw;
        pts[RIGHT_FOOT] = foot_r;
        u[RIGHT_FOOT] = -hw - verge_r - width - skirt_r;
        pts[STRIPS] = if raised { foot_l } else { foot_r };
        u[STRIPS] = u[0];
        Section { pts, u, nrm, up, edge, border: f.border }
    }
}

/// Surface, colour and dirt amount (see [`TrackMesh::dirt`]) of a strip of a road whose edges
/// are finished with `edge`.
fn classify(role: Role, deck: Surface, edge: Edge, normal: Vec3) -> (Surface, [f32; 3], u8) {
    let worked = if deck == Surface::Dirt { 1 } else { 0 };
    let side = if deck == Surface::Dirt { color::WALL } else { color::SLAB };
    let strapped = edge == Edge::Bumpers;
    match role {
        Role::Skirt => {
            if normal.y >= 0.7 {
                (Surface::Ground, color::GROUND, worked)
            } else {
                (Surface::Wall, side, worked)
            }
        }
        Role::Under => (Surface::Wall, side, 0),
        Role::Verge => match deck {
            Surface::Dirt => (Surface::Ground, color::GROUND, worked),
            _ => (Surface::Ground, if strapped { color::VERGE_STRAPPED } else { color::VERGE }, 0),
        },
        Role::Border => (Surface::Wall, color::HULL, 0),
        Role::Line => match deck {
            Surface::Dirt => (Surface::Dirt, color::DIRT, 2),
            _ => (Surface::Road, if strapped { color::LINE_STRAPPED } else { color::LINE }, 0),
        },
        Role::Deck => match deck {
            Surface::Dirt => (Surface::Dirt, color::DIRT, 2),
            _ => (Surface::Road, if strapped { color::ROAD_STRAPPED } else { color::ROAD }, 0),
        },
    }
}

/// A vertex of strip run `run` at section `k`, side `w` (0 left, 1 right), shared along the run.
#[allow(clippy::too_many_arguments)]
fn run_vertex(
    b: &mut MeshBuilder,
    verts: &mut [[(u32, u32); 2]],
    run: u32,
    k: usize,
    w: usize,
    p: Vec3,
    normal: Vec3,
    color: [f32; 3],
    uv: [f32; 2],
    dirt: f32,
) -> u32 {
    let (r, i) = verts[k][w];
    if r == run {
        return i;
    }
    let i = if normal == Vec3::ZERO { b.vertex_on(p, color, uv, dirt) } else { b.vertex_facing(p, normal, color, uv) };
    verts[k][w] = (run, i);
    i
}

/// Sweeps the deck of `p` between `range.0` and `range.1` (metres along the piece) into a mesh,
/// capping the open ends. `route_s` is the distance along the route at the piece's entry (track
/// coordinates of the vertices). With `deck_only` the lips and skirts are left out: the terrain
/// meets the deck edges (a swept dirt piece bedded in the terrain, see [`crate::dirt`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn sweep(
    b: &mut MeshBuilder,
    decor: &mut MeshBuilder,
    p: &Placed,
    range: (f32, f32),
    route_s: f32,
    open_start: bool,
    open_end: bool,
    deck_only: bool,
) {
    let ss = p.samples_in(range.0, range.1);
    let secs: Vec<Section> = ss.iter().map(|&s| Section::new(&p.frame(s))).collect();
    let decks: Vec<Surface> = ss.windows(2).map(|w| p.frame(0.5 * (w[0] + w[1])).deck).collect();
    let edge = p.edge();
    let n = ss.len();
    let mut run = 0u32;
    for (j, &role) in ROLES.iter().enumerate() {
        if deck_only && !matches!(role, Role::Deck | Role::Line) {
            continue;
        }
        let mut current: Option<(Surface, [f32; 3], u8)> = None;
        let mut verts = vec![[(u32::MAX, 0u32); 2]; n];
        for k in 0..n - 1 {
            let (s0, s1) = (&secs[k], &secs[k + 1]);
            let (pa, pb, pc, pd) = (s0.pts[j], s0.pts[j + 1], s1.pts[j], s1.pts[j + 1]);
            let e0 = pa.distance(pb) >= EPS;
            let e1 = pc.distance(pd) >= EPS;
            let normal = (pd - pa).cross(pc - pb).normalize_or_zero();
            if (!e0 && !e1) || normal == Vec3::ZERO {
                current = None;
                continue;
            }
            let class = classify(role, decks[k], edge, normal);
            // A border's own normals where they agree with the strip (shaded round), the
            // triangles' elsewhere (its crease against the deck).
            let round = role == Role::Border
                && [s0.nrm[j], s0.nrm[j + 1], s1.nrm[j], s1.nrm[j + 1]].iter().all(|n| n.dot(normal) > 0.55);
            if current != Some(class) {
                current = Some(class);
                run += 1;
            }
            let (surface, color, worked) = class;
            let dirt = 0.5 * worked as f32;
            let uv = |kk: usize, jj: usize| [route_s + ss[kk], secs[kk].u[jj]];
            let mut v = |kk: usize, w: usize, q: Vec3| {
                let n = if round { secs[kk].nrm[j + w] } else { Vec3::ZERO };
                run_vertex(b, &mut verts, run, kk, w, q, n, color, uv(kk, j + w), dirt)
            };
            if e0 && e1 {
                let ia = v(k, 0, pa);
                let ib = v(k, 1, pb);
                let ic = v(k + 1, 0, pc);
                let id = v(k + 1, 1, pd);
                b.tri(ia, ib, ic, surface);
                b.tri(ib, id, ic, surface);
            } else if e0 {
                let ia = v(k, 0, pa);
                let ib = v(k, 1, pb);
                let ic = v(k + 1, 0, pc);
                b.tri(ia, ib, ic, surface);
            } else {
                let ia = v(k, 0, pa);
                let ic = v(k + 1, 0, pc);
                let id = v(k + 1, 1, pd);
                b.tri(ia, id, ic, surface);
            }
        }
    }
    if !deck_only {
        border_visuals(decor, p, range, &secs, &ss, route_s);
    }
    let strips = if deck_only { DECK_STRIPS } else { 0..STRIPS };
    let earth = p.piece.deck == Surface::Dirt;
    if open_start {
        cap(b, &secs[0], false, strips.clone(), earth);
    }
    if open_end {
        cap(b, &secs[n - 1], true, strips, earth);
    }
}

/// What the eye sees of a road's borders along a swept part (`range` of piece `p`, its sections
/// `secs` at `ss`, `route_s` at the piece's entry), into `decor` (the car meets their plain hulls,
/// see [`border_profile`]): the deck's tarp carried on under the border; on a road with bumpers, a
/// red and white tube; with sandbags, a row of filled bags and big stakes in front of them.
fn border_visuals(decor: &mut MeshBuilder, p: &Placed, range: (f32, f32), secs: &[Section], ss: &[f32], route_s: f32) {
    // A border's inner and outer feet among a section's points, left then right.
    let feet = [(LEFT_EDGE - 1, 1), (RIGHT_EDGE + 1, RIGHT_EDGE + BORDER_POINTS)];
    for (inner, outer) in feet {
        let mut floor: Option<[u32; 2]> = None;
        let mut tube: Option<Vec<u32>> = None;
        for (k, sec) in secs.iter().enumerate() {
            let Some(edge) = sec.edge.filter(|_| sec.border > 0.02) else {
                (floor, tube) = (None, None);
                continue;
            };
            let (i, o) = (sec.pts[inner], sec.pts[outer]);
            let along = route_s + ss[k];
            // The floor from the border's inner foot to its outer one: under bumpers the tarp, strapped
            // down to them; under sandbags the Martian ground, the tarp stopping at the bags.
            let colour = if edge == Edge::Bumpers { color::VERGE_STRAPPED } else { color::GROUND };
            let f = [decor.vertex_on(i, colour, [along, sec.u[inner]], 0.0), decor.vertex_on(o, colour, [along, sec.u[outer]], 0.0)];
            if let Some(g) = floor {
                facing_quad(decor, [g[0], g[1], f[1], f[0]], sec.up, Surface::Ground);
            }
            floor = Some(f);
            if edge != Edge::Bumpers {
                continue;
            }
            // The tube, lying on the floor against the deck's edge, nearly all the way round.
            let across = (o - i).normalize_or_zero();
            let r = BUMPER_TUBE * sec.border;
            let centre = i + across * r + sec.up * (0.95 * r);
            const AROUND: usize = 12;
            let ring: Vec<u32> = (0..AROUND)
                .map(|j| {
                    let a = (250.0 - 320.0 * j as f32 / (AROUND - 1) as f32).to_radians();
                    let out = across * libm::cosf(a) + sec.up * libm::sinf(a);
                    decor.vertex_facing(centre + out * r, out, color::LIP, [along, sec.u[inner]])
                })
                .collect();
            if let Some(last) = &tube {
                for j in 0..AROUND - 1 {
                    let mid = 0.5 * (decor.position(last[j]) + decor.position(ring[j + 1]));
                    facing_quad(decor, [last[j], last[j + 1], ring[j + 1], ring[j]], mid - centre, Surface::Wall);
                }
            }
            tube = Some(ring);
        }
    }
    if p.edge() == Edge::Sandbags {
        sandbag_row(decor, p, range, route_s);
    }
}

/// Two triangles over the quad `q` (in order around it), wound to face along `toward`.
fn facing_quad(b: &mut MeshBuilder, q: [u32; 4], toward: Vec3, surface: Surface) {
    let [p0, p1, p2] = [q[0], q[1], q[2]].map(|i| b.position(i));
    if (p1 - p0).cross(p2 - p0).dot(toward) >= 0.0 {
        b.tri(q[0], q[1], q[2], surface);
        b.tri(q[0], q[2], q[3], surface);
    } else {
        b.tri(q[0], q[2], q[1], surface);
        b.tri(q[0], q[3], q[2], surface);
    }
}

/// The sandbags along a road with sandbags (part `range` of `p`, `route_s` at its entry), laid
/// by hand on each side (art/roads/moodboard/05-au-sol.jpg): from the road out, the tarp, its
/// very edge pinned by a heavy stake about every bag, each leaning and turned its own way; then
/// big filled bags lying flat on the ground outside the tarp, end to end, touching it, every one
/// its own length, width and fill, a little askew. Drawn only: the car meets the border's plain
/// hull.
fn sandbag_row(decor: &mut MeshBuilder, p: &Placed, range: (f32, f32), route_s: f32) {
    // Where the border stands at `s`: its inner foot (the tarp's edge), the direction across it
    // (outward, in the deck's plane) and the deck's normal. `None` off the road.
    let at = |s: f32, side: usize| -> Option<(Frame, Vec3, Vec3, Vec3)> {
        let f = p.frame(s);
        if f.deck != Surface::Road || f.border < 0.3 {
            return None;
        }
        let sec = Section::new(&f);
        let (inner, outer) = if side == 0 { (LEFT_EDGE - 1, 1) } else { (RIGHT_EDGE + 1, RIGHT_EDGE + BORDER_POINTS) };
        let (i, o) = (sec.pts[inner], sec.pts[outer]);
        Some((f, i, (o - i).normalize_or_zero(), sec.up))
    };
    // Random numbers for a spot on the route (quantised to 5 cm) and a purpose.
    let rnd = |s: f32, side: usize, what: i32| unit(hash2(0x5a4d + side as u32, libm::floorf((route_s + s) * 20.0) as i32, what));
    for side in 0..2 {
        // A walk along the border: each bag starts where the last one ends, pressed into it by a
        // random few centimetres; a stake goes in near where two bags meet.
        let mut s = range.0 - 0.6 * rnd(range.0, side, 0);
        while s < range.1 {
            let long = 1.4 + 0.5 * rnd(s, side, 1);
            let mid = s + 0.5 * long;
            let next = s + long * (0.84 + 0.1 * rnd(s, side, 2));
            let seam = s;
            s = next;
            let (s0, s1) = ((mid - 0.5 * long).max(range.0), (mid + 0.5 * long).min(range.1 - 1e-3));
            let (Some((f, i, across, up)), Some((_, i0, _, _)), Some((_, i1, _, _))) = (at(mid.clamp(range.0, range.1 - 1e-3), side), at(s0, side), at(s1, side)) else { continue };
            let seed = hash2(0xba95, libm::floorf((route_s + mid) * 20.0) as i32, side as i32);
            let r = |k: i32| unit(hash2(seed, k, 1));
            // Along the chord of the border under it (following the road's grade and its bends),
            // a little askew in the deck's plane.
            let chord = (i1 - i0).normalize_or(f.forward);
            let yaw = 0.26 * (r(0) - 0.5);
            let along = chord * libm::cosf(yaw) + up.cross(chord) * libm::sinf(yaw);
            // Every bag its own fill: some a few centimetres taller than the others.
            let size = Vec3::new(long, 0.44 + 0.1 * r(1) * r(1), 0.86 + 0.14 * r(2)) * f.border;
            // On the ground outside the tarp (the border's inner foot is the tarp's edge), its
            // bulge touching the tarp or over it by up to 5 cm; sunk a little under its weight.
            let foot = 0.5 * (i0 + i1) + (i - 0.5 * (i0 + i1)).dot(up) * up + across * (0.5 * size.z - 0.05 * r(3)) - up * 0.04;
            let tint = [0.24 + 0.24 * r(4), 0.3 + 0.7 * r(5)];
            let lean = 0.04 * (r(6) - 0.5);
            let tilted = up * libm::cosf(lean) + across * libm::sinf(lean);
            add_sandbag(decor, foot, along, tilted, size, seed, true, Surface::Wall, color::SANDBAG, tint);
            // The stake: through the tarp's very edge, against the bags, near the seam with the bag
            // before
            // (a few left out).
            if r(16) < 0.88 {
                let ds = (seam + 0.9 * (r(17) - 0.5)).clamp(range.0, range.1 - 1e-3);
                if let Some((g, gi, gacross, _)) = at(ds, side) {
                    let ground = gi - gacross * (0.075 + 0.02 * r(18));
                    stake(decor, ground, gacross, g.forward, hash2(seed, 3, 3));
                }
            }
        }
    }
}

/// A heavy stake pinning the tarp's edge, driven in at `ground` (moodboard 05): a hexagonal steel
/// bar about 9 cm across, standing 35 to 55 cm out of the ground, under a round cap with a
/// handle to pull it out by; leaning 3 to 16° in its own direction and turned its own way,
/// mostly galvanised, some rusty (`seed`). `across` and `forward` are the border's directions.
fn stake(b: &mut MeshBuilder, ground: Vec3, across: Vec3, forward: Vec3, seed: u32) {
    let r = |k: i32| unit(hash2(seed, k, 5));
    let colour = if r(0) < 0.2 { color::RUST } else { color::STAKE };
    let tilt = (3.0 + 13.0 * r(1)).to_radians();
    let azimuth = core::f32::consts::TAU * r(2);
    let axis = (Vec3::Y * libm::cosf(tilt) + (across * libm::cosf(azimuth) + forward * libm::sinf(azimuth)) * libm::sinf(tilt)).normalize();
    let radius = 0.042 + 0.012 * r(3);
    let out = 0.35 + 0.2 * r(4);
    let turn = core::f32::consts::TAU * r(5);
    let top = ground + axis * out;
    // The bar, from below the ground; the eyelet's washer it goes through, on the tarp.
    add_post(b, ground - axis * 0.2, top, radius, 6, turn, Surface::Wall, colour);
    add_post(b, ground - Vec3::Y * 0.01, ground + Vec3::Y * 0.015, 1.9 * radius, 6, turn + 0.5, Surface::Wall, color::STEEL);
    // The cap, wider than the bar.
    let cap = top + axis * 0.05;
    add_post(b, top - axis * 0.01, cap, 1.75 * radius, 6, turn + 0.5, Surface::Wall, colour);
    // The handle: a loop over the cap, across it in its own direction.
    let side = (axis.cross(Vec3::Y).normalize_or(forward) * libm::cosf(turn) + axis.cross(forward).normalize_or(across) * libm::sinf(turn)).normalize();
    let side = (side - axis * side.dot(axis)).normalize_or(forward);
    let (w, h) = (1.3 * radius, 0.075);
    let pts = [cap + side * w, cap + side * w + axis * h, cap - side * w + axis * h, cap - side * w];
    for k in 0..pts.len() - 1 {
        add_tube(b, pts[k], pts[k + 1], 0.012, 3, Surface::Wall, colour);
    }
}

/// Closes an open end of a piece: the area under the section outline, down to the line between
/// the skirts' feet (the terrain, or the underside of a raised slab). The strips `strips` of the
/// section only; `earth` gives it the colour of dug earth.
fn cap(b: &mut MeshBuilder, sec: &Section, facing_forward: bool, strips: core::ops::Range<usize>, earth: bool) {
    let (l, r) = (sec.pts[0], sec.pts[RIGHT_FOOT]);
    let span = Vec2::new(r.x - l.x, r.z - l.z);
    let below = |p: Vec3| {
        let t = (Vec2::new(p.x - l.x, p.z - l.z).dot(span) / span.length_squared().max(1e-6)).clamp(0.0, 1.0);
        Vec3::new(p.x, l.y + (r.y - l.y) * t, p.z)
    };
    let side = if earth { color::EARTH_FACE } else { color::SLAB };
    for j in strips {
        let color = if ROLES[j] == Role::Border { color::HULL } else { side };
        let (p, q) = (sec.pts[j], sec.pts[j + 1]);
        if libm::hypotf(p.x - q.x, p.z - q.z) < EPS {
            continue;
        }
        for t in [[p, q, below(p)], [q, below(q), below(p)]] {
            if (t[1] - t[0]).cross(t[2] - t[0]).length() < 1e-3 {
                continue;
            }
            let t = if facing_forward { t } else { [t[0], t[2], t[1]] };
            b.flat_tri(t, Surface::Wall, color);
        }
    }
}

/// Ground the swept part `range` of a piece stands on, as discs along its centreline: `(centre,
/// radius, low)` in the horizontal plane, at most 4 m apart, with `low` the height of the lower
/// deck edge there. A disc reaches the farthest skirt foot or gate post of its section, so
/// everything the piece puts on the terrain lies inside the union of the capsules joining
/// consecutive discs (a jump gap included: the route flies over it and the caps of the ramp and
/// landing come down to the terrain there). A jump ramp and its landing report a `low` of 0:
/// nothing may rise under them, or into their gap.
pub(crate) fn footprint(p: &Placed, range: (f32, f32)) -> Vec<(Vec2, f32, f32)> {
    let (s0, s1) = range;
    let n = libm::ceilf((s1 - s0) / 4.0).max(1.0) as usize;
    let gate = if p.piece.gate.is_some() { gate_post_u(p.piece.deck) + GATE_POST_HALF } else { 0.0 };
    let jump = matches!(p.piece.kind, Kind::JumpRamp { .. } | Kind::Landing { .. });
    let hw = half_width(p.piece.deck);
    (0..=n)
        .map(|k| {
            let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
            let sec = Section::new(&f);
            let c = Vec2::new(f.horiz.x, f.horiz.z);
            let reach = |q: Vec3| Vec2::new(q.x, q.z).distance(c);
            let r = reach(sec.pts[0]).max(reach(sec.pts[RIGHT_FOOT])).max(gate).max(GATE_POST_U + GATE_POST_HALF);
            let low = if jump { 0.0 } else { f.deck_point(-hw).y.min(f.deck_point(hw).y).max(0.0) };
            (c, r, low)
        })
        .collect()
}

/// How far from the centreline the posts of a gate stand.
pub(crate) fn gate_post_u(deck: Surface) -> f32 {
    match deck {
        Surface::Dirt => DIRT_GATE_POST_U,
        _ => GATE_POST_U,
    }
}

/// A gate across the deck at frame `f`. `ground(p)` is the height the post at horizontal
/// position `p` stands on.
pub(crate) fn gate(b: &mut MeshBuilder, f: &Frame, kind: Gate, ground: impl Fn(Vec3) -> f32) {
    let color = match kind {
        Gate::Start => color::START,
        Gate::Checkpoint => color::CHECKPOINT,
        Gate::Finish => color::FINISH,
    };
    let post_u = gate_post_u(f.deck);
    let deck_y = f.centre().y;
    let top = deck_y + GATE_BEAM_BOTTOM + GATE_BEAM_HEIGHT;
    for side in [1.0, -1.0] {
        let base = f.horiz + f.left * (side * post_u);
        let foot = ground(base);
        let h = 0.5 * (top - foot);
        let centre = Vec3::new(base.x, foot + h, base.z);
        add_box(b, centre, Vec3::new(GATE_POST_HALF, h, GATE_POST_HALF), f.forward, Surface::Wall, color, false);
    }
    let beam = f.horiz + Vec3::Y * (deck_y + GATE_BEAM_BOTTOM + 0.5 * GATE_BEAM_HEIGHT);
    let half = Vec3::new(post_u + GATE_POST_HALF, 0.5 * GATE_BEAM_HEIGHT, 0.6);
    add_box(b, beam, half, f.forward, Surface::Wall, color, true);
}

fn terrain(b: &mut MeshBuilder, centre: Vec3) {
    let n = 16;
    let step = TERRAIN_SIZE / n as f32;
    let x0 = centre.x - 0.5 * TERRAIN_SIZE;
    let z0 = centre.z - 0.5 * TERRAIN_SIZE;
    for i in 0..n {
        for k in 0..n {
            let (xa, xb) = (x0 + i as f32 * step, x0 + (i + 1) as f32 * step);
            let (za, zb) = (z0 + k as f32 * step, z0 + (k + 1) as f32 * step);
            b.quad(
                [
                    Vec3::new(xa, TERRAIN_Y, za),
                    Vec3::new(xa, TERRAIN_Y, zb),
                    Vec3::new(xb, TERRAIN_Y, zb),
                    Vec3::new(xb, TERRAIN_Y, za),
                ],
                Surface::Ground,
                color::GROUND,
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Layout.

/// A map as a chain of pieces, from the start block to the finish.
#[derive(Clone, Debug)]
pub struct Layout {
    pub name: String,
    pub start: Connector,
    pub pieces: Vec<Placed>,
}

/// The middle of the cells the pieces occupy, on a grid line.
pub(crate) fn centre_of(pieces: &[Placed]) -> Vec3 {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in pieces {
        for (i, k) in p.cells() {
            lo = lo.min(Vec3::new(i as f32 * CELL, 0.0, k as f32 * CELL));
            hi = hi.max(Vec3::new((i + 1) as f32 * CELL, 0.0, (k + 1) as f32 * CELL));
        }
    }
    let c = 0.5 * (lo + hi) / CELL;
    Vec3::new(libm::roundf(c.x) * CELL, 0.0, libm::roundf(c.z) * CELL)
}

/// A point of the driving line.
#[derive(Clone, Copy, Debug)]
pub struct RoutePoint {
    /// Horizontal distance from the entry of the first piece.
    pub dist: f32,
    pub piece: usize,
    /// Distance along the piece.
    pub s: f32,
    /// Deck centre (interpolated across a jump gap).
    pub pos: Vec3,
    /// False across a jump gap.
    pub on_deck: bool,
}

impl Layout {
    pub fn new(name: &str, start: Connector) -> Self {
        Self { name: name.into(), start, pieces: Vec::new() }
    }

    /// Appends a piece at the end of the chain.
    pub fn push(&mut self, piece: Piece) -> &mut Self {
        let entry = self.pieces.last().map_or(self.start, |p| p.exit);
        self.pieces.push(Placed::new(piece, entry));
        self
    }

    /// Horizontal length of the whole chain, metres.
    pub fn length(&self) -> f32 {
        self.pieces.iter().map(|p| p.length).sum()
    }

    /// Indices `i` where piece `i` and piece `i + 1` share a deck edge.
    pub fn joins(&self) -> Vec<usize> {
        (0..self.pieces.len().saturating_sub(1)).filter(|&i| self.pieces[i + 1].deck_range().0 == 0.0).collect()
    }

    fn piece_with(&self, gate: Gate) -> Option<usize> {
        self.pieces.iter().position(|p| p.piece.gate == Some(gate))
    }

    pub fn start_pose(&self) -> Pose {
        let i = self.piece_with(Gate::Start).unwrap_or(0);
        let f = self.pieces[i].frame(START_POSE_S);
        Pose { position: f.centre(), yaw: f.yaw }
    }

    /// Checkpoint pieces in driving order.
    pub fn checkpoint_pieces(&self) -> Vec<usize> {
        (0..self.pieces.len()).filter(|&i| self.pieces[i].piece.gate == Some(Gate::Checkpoint)).collect()
    }

    /// Centre of the terrain square: the middle of the map, on the grid.
    pub fn centre(&self) -> Vec3 {
        centre_of(&self.pieces)
    }

    /// The driving line from the entry of piece `first` to the end of piece `last`, at most
    /// `step` metres between points.
    pub fn route_points(&self, first: usize, first_s: f32, last: usize, step: f32) -> Vec<RoutePoint> {
        let mut out = Vec::new();
        let mut dist = 0.0;
        for (i, p) in self.pieces.iter().enumerate() {
            if i < first || i > last {
                dist += p.length;
                continue;
            }
            let begin = if i == first { first_s } else { 0.0 };
            let n = libm::ceilf((p.length - begin) / step).max(1.0) as usize;
            let (s0, _) = p.deck_range();
            let top = p.frame(s0).centre();
            for k in 0..=n {
                if k == 0 && i != first {
                    continue;
                }
                let s = begin + (p.length - begin) * k as f32 / n as f32;
                let (pos, on_deck) = if s < s0 {
                    (p.entry.pos + (top - p.entry.pos) * (s / s0), false)
                } else {
                    (p.frame(s).centre(), true)
                };
                out.push(RoutePoint { dist: dist + s, piece: i, s, pos, on_deck });
            }
            dist += p.length;
        }
        out
    }

    /// Builds the playable track on the flat terrain square of the first maps: swept pieces, end
    /// caps, gates and triggers (the game builds maps with [`crate::map::Map::build`], on Mars
    /// terrain).
    pub fn build(&self) -> Track {
        let mut b = MeshBuilder::default();
        terrain(&mut b, self.centre());
        let mut mesh = b.finish();
        let (pieces, decor) = self.pieces_and_decor();
        mesh.append(&pieces);
        let mut track = self.track(mesh, FALL_LIMIT_Y);
        track.decor = decor;
        track
    }

    /// The pieces swept whole (dirt included), end caps, stilts and gates, without terrain: the
    /// flat-terrain preview.
    pub fn pieces_mesh(&self) -> TrackMesh {
        self.pieces_and_decor().0
    }

    /// The pieces' mesh (see [`Layout::pieces_mesh`]) and their decoration (see [`Track::decor`]).
    fn pieces_and_decor(&self) -> (TrackMesh, TrackMesh) {
        let mut b = MeshBuilder::default();
        let mut decor = MeshBuilder::default();
        let n = self.pieces.len();
        let mut route_s = 0.0;
        for (i, p) in self.pieces.iter().enumerate() {
            let open_start = i == 0 || p.deck_range().0 > 0.0;
            let open_end = i + 1 == n || self.pieces[i + 1].deck_range().0 > 0.0;
            sweep(&mut b, &mut decor, p, p.deck_range(), route_s, open_start, open_end, false);
            crate::stilts::stilts(&mut b, &mut decor, p, p.deck_range(), route_s, |_| TERRAIN_Y);
            if let Some(g) = p.piece.gate {
                gate(&mut b, &p.frame(p.gate_s()), g, |_| TERRAIN_Y);
            }
            route_s += p.length;
        }
        (b.finish(), decor.finish())
    }

    /// The track around a finished mesh: start, triggers and driving line from the chain.
    pub(crate) fn track(&self, mesh: TrackMesh, fall_limit_y: f32) -> Track {
        let n = self.pieces.len();
        let start_piece = self.piece_with(Gate::Start).unwrap_or(0);
        let finish_piece = self.piece_with(Gate::Finish).unwrap_or(n - 1);
        let route = self
            .route_points(start_piece, START_POSE_S, finish_piece, 4.0)
            .into_iter()
            .map(|r| r.pos)
            .collect();
        Track {
            name: self.name.clone(),
            mesh,
            decor: TrackMesh::default(),
            start: self.start_pose(),
            checkpoints: self.checkpoint_pieces().into_iter().map(|i| self.pieces[i].trigger()).collect(),
            finish: self.pieces[finish_piece].trigger(),
            fall_limit_y,
            route,
        }
    }
}
