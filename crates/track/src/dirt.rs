//! Dirt tracks dug into the Martian ground.
//!
//! On Mars terrain a dirt piece is not a mesh laid on the ground: it is part of the terrain, a
//! corridor graded into the plain as if diggers had opened it for the race. The car drives the
//! same ground as the planet around it, worked: that is why it grips better than the rough ground
//! beyond the banks. This module describes the corridors; [`crate::terrain`] shapes and meshes
//! the ground with them.
//!
//! # Shape
//!
//! Across a corridor, from its centreline:
//!
//! - the floor, the driving surface: the piece's deck (height, grade and bank, see
//!   [`crate::kit`]) [`DIRT_HALF_WIDTH`] m to each side, then curving up a little like the bottom
//!   of a trough out to the floor edge. The floor is wider on the outside of turns, so a drift can
//!   swing wide, and its edges wander by a metre or so;
//! - the banks: from the floor edge the ground rises at most at [`CUT_SLOPE`] up to the natural
//!   ground, or falls at most at [`FILL_SLOPE`] down to it, with a rounded toe and crest;
//! - the natural ground near a corridor stands 0.4 to 2 m above its floor (lower on the inside of
//!   turns, so the apex stays in sight), with a ridge of spoil along the top of the banks, and
//!   blends into the plain's own relief within [`REACH`] m.
//!
//! The physics reads the floor and the first [`DIRT_BANK`] m of the banks as dirt, the rest as
//! off-track ground; the renderer blends from one look to the other with
//! [`crate::TrackMesh::dirt`].
//!
//! Where a corridor meets a swept deck (a road, a dirt jump ramp or landing), its floor carries
//! on [`APRON`] m under that deck, sinking slowly, so the wheels roll from one to the other
//! without a step. A swept dirt piece sits in a corridor of its own, its floor [`DECK_SINK`] m
//! under the deck except at the deck edges where the banks meet it, and the gap of a jump is a
//! trench [`PIT_DEPTH`] m below the lower of the lip and the landing.

use core::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::Vec2;

use crate::Surface;
use crate::kit::{DIRT_HALF_WIDTH, Frame, Kind, Placed, half_width, smootherstep, smoothstep, turn_radius};
use crate::noise::{fbm, hash2};
use crate::terrain::Capsule;

/// Beyond the deck edge the floor curves up by `BOWL · d²` (d metres past the edge).
const BOWL: f32 = 0.03;
/// On the low side of a banked deck the floor levels out over this distance past the edge.
const LEVEL_OUT: f32 = 8.0;
/// Steepest slope of the banks, dy/dx: cut into higher ground (37°), fill down to lower ground
/// (27°).
pub const CUT_SLOPE: f32 = 0.75;
pub const FILL_SLOPE: f32 = 0.5;
/// Widths of the rounded toe of a cut bank and crest of a fill bank, metres.
const TOE: f32 = 3.0;
const CREST: f32 = 3.0;
/// A corridor shapes the ground up to this far beyond its floor edge, metres.
pub const REACH: f32 = 60.0;
/// The first metres of the banks are still dirt for the physics.
pub const DIRT_BANK: f32 = 1.5;
/// Under a swept deck that joins a corridor, the corridor floor carries on this far, sinking by
/// 3 cm per metre.
pub const APRON: f32 = 6.0;
const APRON_SINK: f32 = 0.03;
/// The terrain under a swept dirt deck stays this far below it, metres.
pub const DECK_SINK: f32 = 0.3;
/// Depth of the trench under a jump's gap, below the lower of its lip and its landing, metres.
pub const PIT_DEPTH: f32 = 4.0;
/// Step of the floor width tables, metres.
const STEP: f32 = 2.0;
/// Size of the buckets of the spatial index, metres.
const BUCKET: f32 = 32.0;
/// Extra floor width on the outside of a turn at its middle, by turn size (cells); banked turns
/// get [`BANKED_WIDEN`] instead (their bank already carries the car round).
const OUTSIDE_WIDEN: [f32; 4] = [0.0, 4.0, 6.0, 7.0];
const BANKED_WIDEN: f32 = 2.0;
const INSIDE_WIDEN: f32 = 1.5;
/// How much lower the plain and its spoil ridge stand on the inside of a turn: enough to see
/// the apex, not so much that cutting across it is free.
const INSIDE_LOWER: f32 = 0.35;
/// How far the floor edges wander, metres.
const EDGE_WANDER: f32 = 1.5;
/// A widened floor keeps this far from any other piece (its banks need the room), metres.
const CLEARANCE: f32 = 6.0;

/// One carved piece, or the bed of a swept dirt piece.
#[derive(Clone)]
struct Cut {
    placed: Placed,
    /// The carved part, metres along the piece.
    range: (f32, f32),
    /// How far the floor carries on before and after the carved part (an apron under a swept
    /// deck), metres.
    ext: (f32, f32),
    /// Distance along the route at the piece's entry (0 off the route).
    route_s: f32,
    /// Floor half-widths `[left, right]` every [`STEP`] m from `range.0 − ext.0`.
    widths: Vec<[f32; 2]>,
    /// Side of the turn (1 left, −1 right, 0 not a turn).
    turn: f32,
    /// A jump's trench: the floor is level at this height.
    pit: Option<f32>,
    /// How far the floor is below the deck (under a swept deck).
    sink: f32,
    seed: u32,
}

impl Cut {
    fn start(&self) -> f32 {
        self.range.0 - self.ext.0
    }

    fn end(&self) -> f32 {
        self.range.1 + self.ext.1
    }

    fn widths_at(&self, s: f32) -> [f32; 2] {
        let x = ((s - self.start()) / STEP).max(0.0);
        let i = (libm::floorf(x) as usize).min(self.widths.len() - 2);
        let t = (x - i as f32).min(1.0);
        let (a, b) = (self.widths[i], self.widths[i + 1]);
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    /// 1 in the middle of a turn on its inside, 0 elsewhere.
    fn inside(&self, s: f32, h: f32) -> f32 {
        if self.turn == 0.0 || h * self.turn <= 0.0 {
            return 0.0;
        }
        let t = libm::sinf(PI * (s / self.placed.length).clamp(0.0, 1.0));
        t * t
    }
}

/// Where a point stands relative to the nearest corridor.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Horizontal distance outside the floor edge, negative on the floor.
    pub edge: f32,
    /// Height of the floor under the point on the floor; outside, at the nearest floor edge.
    pub floor: f32,
    /// Outside the floor: slope of the floor going out across that edge (dy/dx).
    pub slope: f32,
    /// Height of the floor on the centreline there.
    pub centre: f32,
    /// Track coordinates: metres along the route and across the centreline (positive left).
    pub uv: [f32; 2],
    /// 1 on the inside of the middle of a turn, 0 elsewhere.
    pub inside: f32,
    seed: u32,
}

/// Height of the floor of frame `f` at `h` metres to the left of its centreline (horizontally,
/// negative to the right), and its slope going away from the centreline there.
fn floor_at(f: &Frame, h: f32) -> (f32, f32) {
    let t = libm::tanf(f.bank);
    let c = libm::cosf(f.bank);
    let hw = half_width(f.deck);
    let left = f.pivot_u + (hw - f.pivot_u) * c;
    let right = f.pivot_u + (-hw - f.pivot_u) * c;
    let plane = |h: f32| f.pivot_y + (h - f.pivot_u) * t;
    if h > left {
        let (y, g) = beyond(t, h - left);
        (plane(left) + y, g)
    } else if h < right {
        let (y, g) = beyond(-t, right - h);
        (plane(right) + y, g)
    } else {
        (plane(h), if h >= 0.0 { t } else { -t })
    }
}

/// Rise of the floor `d` metres past the deck edge, and its slope going out, for a deck that
/// rises going out by `g` at its edge: the high side of a bank carries on, the low side levels
/// out, both curve up like a trough.
fn beyond(g: f32, d: f32) -> (f32, f32) {
    let (bowl, bowl_slope) = (BOWL * d * d, 2.0 * BOWL * d);
    if g >= 0.0 {
        (g * d + bowl, g + bowl_slope)
    } else {
        let k = d.min(LEVEL_OUT);
        (g * (k - k * k / (2.0 * LEVEL_OUT)) + bowl, g * (1.0 - k / LEVEL_OUT) + bowl_slope)
    }
}

/// `∫` of a slope ramping linearly from 0 to 1 over `r` metres, then 1.
fn fillet(e: f32, r: f32) -> f32 {
    if e < r { e * e / (2.0 * r) } else { e - 0.5 * r }
}

/// Polynomial smooth minimum: `min(a, b)` rounded over `k`.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - 0.25 * h * h * k
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
    -smin(-a, -b, k)
}

/// Distance along `p`'s centreline and horizontal offset to the left of it of the point `q`.
fn project(p: &Placed, q: Vec2) -> (f32, f32) {
    let e = Vec2::new(p.entry.pos.x, p.entry.pos.z);
    let (f3, l3) = (p.entry.heading.forward(), p.entry.heading.left());
    let (f, l) = (Vec2::new(f3.x, f3.z), Vec2::new(l3.x, l3.z));
    match p.piece.kind {
        Kind::Turn { size, side, quarters, .. } => {
            let r = turn_radius(size);
            let sg = side.sign();
            let v = q - (e + l * (sg * r));
            let mut th = libm::atan2f(v.dot(f), v.dot(-l * sg));
            // Angles behind the entry by more than half the gap belong past the exit.
            let sweep = quarters as f32 * FRAC_PI_2;
            if th < 0.5 * sweep - PI {
                th += TAU;
            }
            (r * th, sg * (r - v.length()))
        }
        _ => {
            let d = q - e;
            (d.dot(f), d.dot(l))
        }
    }
}

/// The dirt corridors of a map, with a spatial index.
pub struct Corridors {
    cuts: Vec<Cut>,
    origin: Vec2,
    nx: usize,
    nz: usize,
    /// Cuts near each bucket, in ascending order.
    buckets: Vec<Vec<u16>>,
}

impl Corridors {
    /// No corridor at all.
    pub fn none() -> Self {
        Self { cuts: Vec::new(), origin: Vec2::ZERO, nx: 0, nz: 0, buckets: Vec::new() }
    }

    /// The corridors of `pieces`. `route_s[i]` is the distance along the route at the entry of
    /// piece `i`, `next[i]` the piece whose deck joins the end of piece `i`.
    pub(crate) fn new(pieces: &[Placed], route_s: &[f32], next: &[Option<usize>], seed: u32) -> Self {
        let mut prev = vec![None; pieces.len()];
        for (i, n) in next.iter().enumerate() {
            if let Some(j) = *n {
                prev[j] = Some(i);
            }
        }
        // Ends of a carved range that meet a swept deck get an apron.
        let swept_end = |j: usize| pieces[j].carved_range().is_none_or(|(_, c1)| c1 < pieces[j].length - 1e-3);
        let swept_start = |j: usize| pieces[j].carved_range().is_none_or(|(c0, _)| c0 > 1e-3);
        // Everything a widened floor must keep away from: centreline samples of every piece and
        // how far it reaches out.
        let mut obstacles: Vec<(usize, Vec2, f32)> = Vec::new();
        for (j, p) in pieces.iter().enumerate() {
            let (d0, d1) = p.deck_range();
            let n = libm::ceilf((d1 - d0) / 4.0).max(1.0) as usize;
            for k in 0..=n {
                let s = d0 + (d1 - d0) * k as f32 / n as f32;
                let f = p.frame(s);
                let carved = p.carved_range().is_some_and(|(c0, c1)| s >= c0 && s <= c1);
                let reach = if carved { DIRT_HALF_WIDTH } else { half_width(f.deck) + 3.0 + (f.centre().y.max(0.0)) };
                obstacles.push((j, Vec2::new(f.horiz.x, f.horiz.z), reach));
            }
        }
        let mut cuts = Vec::new();
        for (i, p) in pieces.iter().enumerate() {
            let (d0, d1) = p.deck_range();
            let cell = |v: f32| libm::floorf(v / 4.0) as i32;
            let cut_seed = hash2(seed, cell(p.entry.pos.x), cell(p.entry.pos.z) ^ (cell(p.entry.pos.y) << 16));
            let Some(range) = p.carved_range() else {
                // A swept dirt piece (jump ramp, landing): a corridor of its exact width just under
                // its deck, and a trench under the gap before a landing.
                if p.piece.deck == Surface::Dirt && !matches!(p.piece.kind, Kind::Transition { .. }) {
                    let n = (libm::ceilf(d1 / STEP) as usize).max(1) + 2;
                    let widths = vec![[DIRT_HALF_WIDTH, DIRT_HALF_WIDTH]; n];
                    let base = Cut {
                        placed: p.clone(),
                        range: (d0, d1),
                        ext: (0.0, 0.0),
                        route_s: route_s[i],
                        widths: widths.clone(),
                        turn: 0.0,
                        pit: None,
                        sink: DECK_SINK,
                        seed: cut_seed,
                    };
                    if d0 > 0.0 && p.landing().is_some() {
                        let lip = p.entry.pos.y;
                        let land = p.frame(d0).centre().y;
                        cuts.push(Cut { range: (0.0, d0), pit: Some(lip.min(land) - PIT_DEPTH), ..base.clone() });
                    }
                    cuts.push(base);
                }
                continue;
            };
            let joined_before = prev[i].filter(|_| d0 == 0.0);
            let joined_after = next[i].filter(|&j| pieces[j].deck_range().0 == 0.0);
            // An apron runs under the swept deck before or after the carved part, if any.
            let ext0 = if range.0 > d0 + 1e-3 || joined_before.is_some_and(swept_end) { APRON } else { 0.0 };
            let ext1 = if range.1 < d1 - 1e-3 || joined_after.is_some_and(swept_start) { APRON } else { 0.0 };
            let turn = match p.piece.kind {
                Kind::Turn { side, .. } => side.sign(),
                _ => 0.0,
            };
            let mut cut = Cut {
                placed: p.clone(),
                range,
                ext: (ext0, ext1),
                route_s: route_s[i],
                widths: Vec::new(),
                turn,
                pit: None,
                sink: 0.0,
                seed: cut_seed,
            };
            let neighbours = [Some(i), prev[i], next[i]];
            let n = (libm::ceilf((cut.end() - cut.start()) / STEP) as usize).max(1);
            for k in 0..=n + 1 {
                let s = cut.start() + k as f32 * STEP;
                let base = Self::base_widths(p, range, s, turn, cut_seed);
                let mut w = base;
                // Widening yields to other pieces nearby.
                let f = p.frame(s.clamp(0.0, p.length));
                let c = Vec2::new(f.horiz.x, f.horiz.z);
                let left = Vec2::new(f.left.x, f.left.z);
                for &(j, q, reach) in &obstacles {
                    if neighbours.contains(&Some(j)) {
                        continue;
                    }
                    let v = q - c;
                    let side = if v.dot(left) >= 0.0 { 0 } else { 1 };
                    let limit = v.length() - reach - CLEARANCE;
                    let floor = DIRT_HALF_WIDTH.min(base[side]);
                    w[side] = w[side].min(limit.max(floor));
                }
                cut.widths.push(w);
            }
            cuts.push(cut);
        }
        Self::index(cuts)
    }

    /// Floor half-widths `[left, right]` of piece `p` at `s`, before other pieces are considered.
    fn base_widths(p: &Placed, range: (f32, f32), s: f32, turn: f32, seed: u32) -> [f32; 2] {
        let (c0, c1) = range;
        // A transition narrows to the road at its middle.
        let base = match p.piece.kind {
            Kind::Transition { to: Surface::Dirt } => {
                half_width(Surface::Road) + (DIRT_HALF_WIDTH - half_width(Surface::Road)) * smootherstep(c0, c1, s)
            }
            Kind::Transition { .. } => {
                half_width(Surface::Road) + (DIRT_HALF_WIDTH - half_width(Surface::Road)) * (1.0 - smootherstep(c0, c1, s))
            }
            _ => DIRT_HALF_WIDTH,
        };
        let mut w = [base, base];
        if let Kind::Turn { size, bank_deg, .. } = p.piece.kind {
            let t = libm::sinf(PI * (s / p.length).clamp(0.0, 1.0));
            let bump = t * t;
            let out = if bank_deg != 0.0 { BANKED_WIDEN } else { OUTSIDE_WIDEN[(size as usize).min(3)] };
            let r = turn_radius(size);
            let inside = INSIDE_WIDEN.min(r - 3.0 - DIRT_HALF_WIDTH).max(0.0);
            let (outer, inner) = if turn > 0.0 { (1, 0) } else { (0, 1) };
            w[outer] += out * bump;
            w[inner] += inside * bump;
        }
        // Wandering edges, still on gates and at both ends of the piece.
        let still = p.piece.gate.is_some() || matches!(p.piece.kind, Kind::Transition { .. });
        if !still {
            let fade = smoothstep(0.0, 12.0, s) * smoothstep(0.0, 12.0, p.length - s);
            for (k, wk) in w.iter_mut().enumerate() {
                let n = fbm(seed.wrapping_add(k as u32 * 7), s / 22.0, 0.37 + k as f32 * 5.1, 2);
                *wk += EDGE_WANDER * n * fade;
            }
        }
        w
    }

    fn index(cuts: Vec<Cut>) -> Self {
        if cuts.is_empty() {
            return Self::none();
        }
        let margin = REACH + 2.0 * DIRT_HALF_WIDTH;
        let bounds = |c: &Cut| {
            let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
            let n = (libm::ceilf((c.end() - c.start()) / 4.0) as usize).max(1);
            for k in 0..=n {
                let s = c.start() + (c.end() - c.start()) * k as f32 / n as f32;
                let f = c.placed.frame(s.clamp(0.0, c.placed.length));
                let q = Vec2::new(f.horiz.x, f.horiz.z) + Vec2::new(f.forward.x, f.forward.z) * (s - s.clamp(0.0, c.placed.length));
                lo = lo.min(q);
                hi = hi.max(q);
            }
            (lo - Vec2::splat(margin), hi + Vec2::splat(margin))
        };
        let all: Vec<(Vec2, Vec2)> = cuts.iter().map(bounds).collect();
        let lo = all.iter().fold(Vec2::splat(f32::MAX), |a, b| a.min(b.0));
        let hi = all.iter().fold(Vec2::splat(f32::MIN), |a, b| a.max(b.1));
        let origin = Vec2::new(libm::floorf(lo.x / BUCKET), libm::floorf(lo.y / BUCKET)) * BUCKET;
        let nx = libm::ceilf((hi.x - origin.x) / BUCKET) as usize + 1;
        let nz = libm::ceilf((hi.y - origin.y) / BUCKET) as usize + 1;
        let mut buckets = vec![Vec::new(); nx * nz];
        for (i, (a, b)) in all.iter().enumerate() {
            let (i0, k0) = (((a.x - origin.x) / BUCKET) as usize, ((a.y - origin.y) / BUCKET) as usize);
            let (i1, k1) = (((b.x - origin.x) / BUCKET) as usize, ((b.y - origin.y) / BUCKET) as usize);
            for k in k0..=k1.min(nz - 1) {
                for x in i0..=i1.min(nx - 1) {
                    buckets[k * nx + x].push(i as u16);
                }
            }
        }
        Self { cuts, origin, nx, nz, buckets }
    }

    pub fn is_empty(&self) -> bool {
        self.cuts.is_empty()
    }

    /// Capsules covering the floors, from discs every 4 m along them.
    pub(crate) fn capsules(&self) -> Vec<Capsule> {
        let mut out = Vec::new();
        for c in &self.cuts {
            let n = (libm::ceilf((c.range.1 - c.range.0) / 4.0) as usize).max(1);
            let discs: Vec<(Vec2, f32)> = (0..=n)
                .map(|k| {
                    let s = c.range.0 + (c.range.1 - c.range.0) * k as f32 / n as f32;
                    let f = c.placed.frame(s);
                    let [wl, wr] = c.widths_at(s);
                    let centre = Vec2::new(f.horiz.x, f.horiz.z) + Vec2::new(f.left.x, f.left.z) * (0.5 * (wl - wr));
                    (centre, 0.5 * (wl + wr))
                })
                .collect();
            for w in discs.windows(2) {
                out.push(Capsule { a: w[0].0, b: w[1].0, r: w[0].1.max(w[1].1) });
            }
        }
        out
    }

    /// The nearest corridor to the horizontal point `q`, if one is within [`REACH`] of it.
    pub fn query(&self, q: Vec2) -> Option<Hit> {
        if self.cuts.is_empty() {
            return None;
        }
        let (bx, bz) = ((q.x - self.origin.x) / BUCKET, (q.y - self.origin.y) / BUCKET);
        if bx < 0.0 || bz < 0.0 || bx >= self.nx as f32 || bz >= self.nz as f32 {
            return None;
        }
        let list = &self.buckets[bz as usize * self.nx + bx as usize];
        let mut best: Option<(f32, usize, f32, f32, f32)> = None;
        for &i in list {
            let c = &self.cuts[i as usize];
            let (s, h) = project(&c.placed, q);
            let sc = s.clamp(c.start(), c.end());
            let [wl, wr] = c.widths_at(sc);
            let hw = if h >= 0.0 { wl } else { wr };
            let ex = h.abs() - hw;
            let ax = (c.start() - s).max(s - c.end()).max(0.0);
            let e = if ax > 0.0 { libm::hypotf(ex.max(0.0), ax) } else { ex };
            if e <= REACH && best.is_none_or(|b| e < b.0) {
                best = Some((e, i as usize, sc, h, hw));
            }
        }
        let (e, i, sc, h, hw) = best?;
        let c = &self.cuts[i];
        let sf = sc.clamp(0.0, c.placed.length);
        let f = c.placed.frame(sf);
        // The apron sinks under the swept deck.
        // Under a swept deck the floor sinks, except at its very edges where the banks meet it.
        let bed = c.sink * (1.0 - smoothstep(hw - 3.0, hw - 0.5, h.abs()));
        let sink = bed + APRON_SINK * ((c.range.0 - sc).max(0.0) + (sc - c.range.1).max(0.0));
        let hc = h.clamp(-hw, hw);
        let ((floor, slope), centre) = match c.pit {
            Some(y) => ((y, 0.0), y),
            None => (floor_at(&f, hc), floor_at(&f, 0.0).0),
        };
        // Past an end of the corridor the floor is level along it.
        let slope = if e <= 0.0 {
            slope
        } else if h.abs() > hw {
            slope * (h.abs() - hw) / e
        } else {
            0.0
        };
        Some(Hit {
            edge: e,
            floor: floor - sink,
            slope,
            centre: centre - c.sink,
            uv: [c.route_s + sc, h],
            inside: c.inside(sc, h),
            seed: c.seed,
        })
    }

    /// The ground before a corridor was dug, near it: its floor level plus a depth that wanders
    /// between 0.4 and 2 m, a little lower on the inside of turns, with a ridge of spoil along the
    /// top of the banks.
    pub fn plain(&self, q: Vec2, hit: &Hit) -> f32 {
        let lower = 1.0 - INSIDE_LOWER * hit.inside;
        let depth = (1.2 + 1.1 * fbm(hit.seed, q.x / 40.0, q.y / 40.0, 3)).max(0.3) * lower;
        let ridge = 1.0 - smoothstep(0.0, 3.5, (hit.edge - 4.5).abs());
        let spoil = (0.7 + 0.5 * fbm(hit.seed.wrapping_add(1), hit.uv[0] / 18.0, 0.31, 2)).max(0.1) * ridge * lower;
        // From the higher of the floor edge and the centreline, so a dished inside edge does not
        // drag the plain down to the track.
        hit.floor.max(hit.centre) + depth + spoil
    }

    /// The ground at `hit` once the corridor is dug through `natural`.
    pub fn ground(&self, natural: f32, hit: &Hit) -> f32 {
        if hit.edge <= 0.0 {
            return hit.floor;
        }
        let (e, g) = (hit.edge, hit.slope);
        let rise = hit.floor + g * e;
        let hi = rise + (CUT_SLOPE - g).max(0.0) * fillet(e, TOE);
        let lo = rise - (g + FILL_SLOPE).max(0.0) * fillet(e, CREST);
        let k = 0.6 * ((hi - lo) / 1.5).min(1.0);
        smax(lo, smin(natural, hi, k), k)
    }
}
