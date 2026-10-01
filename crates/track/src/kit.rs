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
//!   shoulder, the gate posts and some terrain between neighbouring pieces;
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
//! road rises the shoulder becomes a vertical platform side and the lip grows to
//! [`LIP_HEIGHT`], so elevated roads are platforms with walls down to the ground. Dirt has no
//! lip and its sides slope down like a mound. Slope changes are parabolic vertical curves, bank
//! changes smootherstep ramps, so the deck is continuous in position, heading, grade and bank at
//! every join.

use core::f32::consts::{FRAC_PI_2, PI};

use glam::Vec3;

use crate::jump::LandingProfile;
use crate::mesh::{MeshBuilder, add_box};
use crate::{Pose, Surface, Track, Trigger};

/// Horizontal size of a grid cell, metres.
pub const CELL: f32 = 32.0;
/// Height of one level, metres.
pub const LEVEL: f32 = 8.0;
/// Half the width of the driving surface (road or dirt), which is 20 m wide.
pub const HALF_WIDTH: f32 = 10.0;
/// White edge line painted on the road, inside the driving surface.
pub const LINE_WIDTH: f32 = 0.5;
/// Raised lip along the edges of elevated roads, outside the driving surface.
pub const LIP_WIDTH: f32 = 0.5;
pub const LIP_HEIGHT: f32 = 0.5;
/// Shoulder sloping from a ground-level deck (y = 0) down to the terrain.
pub const SHOULDER_WIDTH: f32 = 2.5;
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
/// Half extents of checkpoint and finish triggers: across (deck, lips and a margin), height,
/// along the road (4 m thick: more than a tick of travel at 1400 km/h).
pub const TRIGGER_HALF: Vec3 = Vec3::new(HALF_WIDTH + LIP_WIDTH + 0.5, 4.5, 2.0);
/// Trigger centres sit this far above the deck.
pub const TRIGGER_LIFT: f32 = 2.0;
/// Below the terrain: only reached by leaving the terrain square.
pub const FALL_LIMIT_Y: f32 = TERRAIN_Y - 20.0;

/// Flat vertex colours (linear RGB).
pub mod color {
    pub const ROAD: [f32; 3] = [0.20, 0.20, 0.21];
    pub const LINE: [f32; 3] = [0.80, 0.80, 0.78];
    pub const DIRT: [f32; 3] = [0.24, 0.08, 0.03];
    pub const GROUND: [f32; 3] = [0.55, 0.22, 0.10];
    pub const LIP: [f32; 3] = [0.68, 0.68, 0.66];
    pub const WALL: [f32; 3] = [0.32, 0.30, 0.29];
    pub const START: [f32; 3] = [0.10, 0.60, 0.15];
    pub const CHECKPOINT: [f32; 3] = [0.05, 0.35, 0.90];
    pub const FINISH: [f32; 3] = [0.90, 0.10, 0.05];
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smootherstep(e0: f32, e1: f32, x: f32) -> f32 {
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
    /// The inner edge keeps its height and the outer edge rises (berms at ground level).
    Inner,
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
    /// [`crate::jump`].
    Landing { cells: u32, levels: i32, gap: f32, epsilon: f32, outrun: f32 },
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

/// A piece as the map designer picks it: a shape, a deck and maybe a gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub kind: Kind,
    /// Road or dirt (at the entry, for a transition).
    pub deck: Surface,
    pub gate: Option<Gate>,
}

impl Piece {
    pub fn road(kind: Kind) -> Self {
        Self { kind, deck: Surface::Road, gate: None }
    }

    pub fn dirt(kind: Kind) -> Self {
        Self { kind, deck: Surface::Dirt, gate: None }
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
        self.horiz + self.left * self.pivot_u + Vec3::Y * self.pivot_y + self.lateral() * (u - self.pivot_u)
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
            Kind::Landing { cells, levels, gap, epsilon, outrun } => {
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
            Kind::Landing { .. } => {
                let p = self.landing.unwrap();
                l.y = if s >= self.length { -p.drop } else { p.height(s) };
            }
            Kind::Turn { size, side, bank_deg, pivot, .. } => {
                let r = turn_radius(size);
                let sg = side.sign();
                let (sn, cs) = sin_cos(s / r);
                l.x = sg * r * (1.0 - cs);
                l.z = r * sn;
                l.turn = sg * s / r;
                if bank_deg != 0.0 {
                    let ramp = (0.35 * self.length).min(40.0);
                    let k = smootherstep(0.0, ramp, s) * smootherstep(0.0, ramp, self.length - s);
                    l.bank = -sg * bank_deg.to_radians() * k;
                    if pivot == Pivot::Inner {
                        l.pivot_u = sg * HALF_WIDTH;
                    }
                }
            }
        }
        l
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
        Trigger { center: f.centre() + Vec3::Y * TRIGGER_LIFT, half_extents: TRIGGER_HALF, yaw: f.yaw }
    }

    /// Sample positions along the deck: every half cell, then halved until the heading, grade,
    /// bank and height change little between samples.
    pub fn samples(&self) -> Vec<f32> {
        let (s0, s1) = self.deck_range();
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
        [-HALF_WIDTH, 0.0, HALF_WIDTH].into_iter().any(|u| {
            let mid = 0.5 * (fa.deck_point(u).y + fb.deck_point(u).y);
            (fm.deck_point(u).y - mid).abs() > 0.01
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Cross-section and sweep.

/// Role of each strip between consecutive section points, from the left skirt to the right.
/// The deck is split in four so a bank that changes between samples barely twists it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Skirt,
    LipTop,
    LipFace,
    Line,
    Deck,
}

const ROLES: [Role; 12] = [
    Role::Skirt,
    Role::LipTop,
    Role::LipFace,
    Role::Line,
    Role::Deck,
    Role::Deck,
    Role::Deck,
    Role::Deck,
    Role::Line,
    Role::LipFace,
    Role::LipTop,
    Role::Skirt,
];
const STRIPS: usize = ROLES.len();
/// Below this an edge counts as collapsed.
const EPS: f32 = 0.02;

#[derive(Clone, Copy)]
struct Section {
    /// Left skirt foot, left lip outer top, left lip inner top, left deck edge, left line inner
    /// edge, three deck points (quarter, centre, quarter), right line inner edge, right deck
    /// edge, right lip inner top, right lip outer top, right skirt foot.
    pts: [Vec3; STRIPS + 1],
    lip: [f32; 2],
}

/// Lip height and skirt width for a deck edge `e` metres above the terrain.
fn side_params(deck: Surface, e: f32) -> (f32, f32) {
    match deck {
        Surface::Dirt => (0.0, SHOULDER_WIDTH.max(1.2 * e)),
        _ => (LIP_HEIGHT * smoothstep(0.6, 2.0, e), SHOULDER_WIDTH * (1.0 - smoothstep(0.3, 1.5, e))),
    }
}

impl Section {
    fn new(f: &Frame) -> Self {
        let lat = f.lateral();
        let up = f.up();
        let el = f.deck_point(HALF_WIDTH);
        let er = f.deck_point(-HALF_WIDTH);
        let (lip_l, skirt_l) = side_params(f.deck, el.y - TERRAIN_Y);
        let (lip_r, skirt_r) = side_params(f.deck, er.y - TERRAIN_Y);
        let out_l = el + lat * LIP_WIDTH + up * lip_l;
        let out_r = er - lat * LIP_WIDTH + up * lip_r;
        let foot = |p: Vec3, dir: Vec3, w: f32| Vec3::new(p.x, TERRAIN_Y, p.z) + dir * w;
        let inner = HALF_WIDTH - LINE_WIDTH;
        Section {
            pts: [
                foot(out_l, f.left, skirt_l),
                out_l,
                el + up * lip_l,
                el,
                f.deck_point(inner),
                f.deck_point(0.5 * inner),
                f.deck_point(0.0),
                f.deck_point(-0.5 * inner),
                f.deck_point(-inner),
                er,
                er + up * lip_r,
                out_r,
                foot(out_r, -f.left, skirt_r),
            ],
            lip: [lip_l, lip_r],
        }
    }
}

fn classify(role: Role, deck: Surface, lip: f32, normal: Vec3) -> (Surface, [f32; 3]) {
    match role {
        Role::Skirt => {
            if normal.y >= 0.7 {
                (Surface::Ground, color::GROUND)
            } else {
                (Surface::Wall, color::WALL)
            }
        }
        Role::LipTop => {
            if lip > 0.05 {
                (Surface::Wall, color::LIP)
            } else {
                (Surface::Ground, color::GROUND)
            }
        }
        Role::LipFace => (Surface::Wall, color::LIP),
        Role::Line => match deck {
            Surface::Dirt => (Surface::Dirt, color::DIRT),
            _ => (Surface::Road, color::LINE),
        },
        Role::Deck => match deck {
            Surface::Dirt => (Surface::Dirt, color::DIRT),
            _ => (Surface::Road, color::ROAD),
        },
    }
}

fn run_vertex(b: &mut MeshBuilder, verts: &mut [[(u32, u32); 2]], run: u32, k: usize, w: usize, p: Vec3, color: [f32; 3]) -> u32 {
    let (r, i) = verts[k][w];
    if r == run {
        return i;
    }
    let i = b.vertex(p, color);
    verts[k][w] = (run, i);
    i
}

fn sweep(b: &mut MeshBuilder, p: &Placed, open_start: bool, open_end: bool) {
    let ss = p.samples();
    let secs: Vec<Section> = ss.iter().map(|&s| Section::new(&p.frame(s))).collect();
    let decks: Vec<Surface> = ss.windows(2).map(|w| p.frame(0.5 * (w[0] + w[1])).deck).collect();
    let n = ss.len();
    let mut run = 0u32;
    for (j, &role) in ROLES.iter().enumerate() {
        let mut current: Option<(Surface, [f32; 3])> = None;
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
            let side = if j < STRIPS / 2 { 0 } else { 1 };
            let class = classify(role, decks[k], s0.lip[side].max(s1.lip[side]), normal);
            if current != Some(class) {
                current = Some(class);
                run += 1;
            }
            let (surface, color) = class;
            if e0 && e1 {
                let ia = run_vertex(b, &mut verts, run, k, 0, pa, color);
                let ib = run_vertex(b, &mut verts, run, k, 1, pb, color);
                let ic = run_vertex(b, &mut verts, run, k + 1, 0, pc, color);
                let id = run_vertex(b, &mut verts, run, k + 1, 1, pd, color);
                b.tri(ia, ib, ic, surface);
                b.tri(ib, id, ic, surface);
            } else if e0 {
                let ia = run_vertex(b, &mut verts, run, k, 0, pa, color);
                let ib = run_vertex(b, &mut verts, run, k, 1, pb, color);
                let ic = run_vertex(b, &mut verts, run, k + 1, 0, pc, color);
                b.tri(ia, ib, ic, surface);
            } else {
                let ia = run_vertex(b, &mut verts, run, k, 0, pa, color);
                let ic = run_vertex(b, &mut verts, run, k + 1, 0, pc, color);
                let id = run_vertex(b, &mut verts, run, k + 1, 1, pd, color);
                b.tri(ia, id, ic, surface);
            }
        }
    }
    if open_start {
        cap(b, &secs[0], false);
    }
    if open_end {
        cap(b, &secs[n - 1], true);
    }
}

/// Closes an open end of a piece: the area under the section outline, down to the terrain.
fn cap(b: &mut MeshBuilder, sec: &Section, facing_forward: bool) {
    let ground = |p: Vec3| Vec3::new(p.x, TERRAIN_Y, p.z);
    for j in 0..STRIPS {
        let (p, q) = (sec.pts[j], sec.pts[j + 1]);
        if libm::hypotf(p.x - q.x, p.z - q.z) < EPS {
            continue;
        }
        for t in [[p, q, ground(p)], [q, ground(q), ground(p)]] {
            if (t[1] - t[0]).cross(t[2] - t[0]).length() < 1e-3 {
                continue;
            }
            let t = if facing_forward { t } else { [t[0], t[2], t[1]] };
            b.flat_tri(t, Surface::Wall, color::WALL);
        }
    }
}

fn gate(b: &mut MeshBuilder, f: &Frame, kind: Gate) {
    let color = match kind {
        Gate::Start => color::START,
        Gate::Checkpoint => color::CHECKPOINT,
        Gate::Finish => color::FINISH,
    };
    let deck_y = f.centre().y;
    let top = deck_y + GATE_BEAM_BOTTOM + GATE_BEAM_HEIGHT;
    for side in [1.0, -1.0] {
        let base = f.horiz + f.left * (side * GATE_POST_U);
        let h = 0.5 * (top - TERRAIN_Y);
        let centre = Vec3::new(base.x, TERRAIN_Y + h, base.z);
        add_box(b, centre, Vec3::new(GATE_POST_HALF, h, GATE_POST_HALF), f.forward, Surface::Wall, color, false);
    }
    let beam = f.horiz + Vec3::Y * (deck_y + GATE_BEAM_BOTTOM + 0.5 * GATE_BEAM_HEIGHT);
    let half = Vec3::new(GATE_POST_U + GATE_POST_HALF, 0.5 * GATE_BEAM_HEIGHT, 0.6);
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
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &self.pieces {
            for (i, k) in p.cells() {
                lo = lo.min(Vec3::new(i as f32 * CELL, 0.0, k as f32 * CELL));
                hi = hi.max(Vec3::new((i + 1) as f32 * CELL, 0.0, (k + 1) as f32 * CELL));
            }
        }
        let c = 0.5 * (lo + hi) / CELL;
        Vec3::new(libm::roundf(c.x) * CELL, 0.0, libm::roundf(c.z) * CELL)
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

    /// Builds the playable track: terrain, swept pieces, end caps, gates and triggers.
    pub fn build(&self) -> Track {
        let mut b = MeshBuilder::default();
        terrain(&mut b, self.centre());
        let n = self.pieces.len();
        for (i, p) in self.pieces.iter().enumerate() {
            let open_start = i == 0 || p.deck_range().0 > 0.0;
            let open_end = i + 1 == n || self.pieces[i + 1].deck_range().0 > 0.0;
            sweep(&mut b, p, open_start, open_end);
            if let Some(g) = p.piece.gate {
                gate(&mut b, &p.frame(p.gate_s()), g);
            }
        }
        let start_piece = self.piece_with(Gate::Start).unwrap_or(0);
        let finish_piece = self.piece_with(Gate::Finish).unwrap_or(n - 1);
        let route = self
            .route_points(start_piece, START_POSE_S, finish_piece, 4.0)
            .into_iter()
            .map(|r| r.pos)
            .collect();
        Track {
            name: self.name.clone(),
            mesh: b.finish(),
            start: self.start_pose(),
            checkpoints: self.checkpoint_pieces().into_iter().map(|i| self.pieces[i].trigger()).collect(),
            finish: self.pieces[finish_piece].trigger(),
            fall_limit_y: FALL_LIMIT_Y,
            route,
        }
    }
}
