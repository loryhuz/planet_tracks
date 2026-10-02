//! Stilts under raised roads. The colony builds its roads like camping gear: decks of laminated
//! tarp on lattices of red plastic tubes, weighed down with sandbags and held by orange straps
//! (art/roads/brief.md). It spares its material: the deck spans from pier to pier, and only some
//! places carry down to the ground.
//!
//! Under a road whose slab stands clear of the ground (see [`kit::SLAB_DEPTH`]):
//! - where there is room ([`TRUSS_NEED`] m under the slab), a truss runs under each side of the
//!   deck, [`TRUSS_DEPTH`] m deep, in panels of [`PANEL`] m: chords, verticals and diagonals,
//!   tied across by beams under the slab;
//! - piers carry the trusses down to the ground about every [`PIER_SPACING`] m, irregularly: a leg
//!   of two posts under each truss, braced, on a big stack of sandbags, braced across to the
//!   other leg;
//! - closer to the ground, stacks of sandbags under the slab (with a post on them where the gap
//!   is taller) hold it every 8 to 24 m, irregularly;
//! - orange straps run from the slab's edges down to stakes in the ground.
//!
//! The bents fall on multiples of [`PANEL`] along the route, the piers on a sequence along it, so
//! they keep their rhythm from one piece to the next. The tubes and sandbags are walls like any
//! other (a car that falls off the road can hit them); the straps, stakes and buckles are
//! decoration the physics ignores. Each tube vertex carries in `uv` its distance from the tube's
//! start and the tube's length, metres: the renderer paints the clamps at its ends and the
//! straps' webbing along it.

use glam::Vec3;

use crate::Surface;
use crate::kit::{self, Placed, color};
use crate::mesh::{MeshBuilder, add_box, add_sandbag, add_tube};
use crate::noise::{hash2, unit};

/// Length of a panel of the trusses along the route, metres.
pub const PANEL: f32 = 4.0;
/// Depth of the trusses under the slab, and the gap under the slab they need, metres.
pub const TRUSS_DEPTH: f32 = 1.8;
pub const TRUSS_NEED: f32 = TRUSS_DEPTH + 0.8;
/// Mean distance between two piers along the route, and how far each strays from its place in
/// the rhythm, metres.
pub const PIER_SPACING: f32 = 24.0;
const PIER_JITTER: f32 = 8.0;
/// Track coordinate across of the trusses and the piers' legs (either side), metres.
const TRUSS_U: f32 = 7.5;
/// A pier's two posts under each truss stand this far before and after it, metres.
const LEG_HALF: f32 = 0.9;
const POST_R: f32 = 0.2;
const BEAM_R: f32 = 0.16;
const BRACE_R: f32 = 0.1;
const STRAP_R: f32 = 0.04;
/// Under this gap the slab rests on the ground; up to [`TRUSS_NEED`] it stands on stacks of
/// sandbags, with a post on them from this gap.
const LOW_MIN: f32 = 0.15;
const LOW_POST: f32 = 1.3;
/// A sandbag: its length, height and width, metres; and the big ones the piers stand on.
const BAG: Vec3 = Vec3::new(0.95, 0.3, 0.55);
const BIG_BAG: Vec3 = Vec3::new(1.25, 0.4, 0.75);

/// One point of a truss: its top (under the slab) and bottom, and the ground under it.
#[derive(Clone, Copy)]
struct Node {
    top: Vec3,
    bottom: Vec3,
    ground: f32,
}

/// The stilts under the part `range` of piece `p` (metres along it): tubes and sandbags into
/// `b`, straps and stakes into `decor`. `route_s` is the distance along the route at the piece's
/// entry, `ground(q)` the height of the ground under `q`.
pub(crate) fn stilts(
    b: &mut MeshBuilder,
    decor: &mut MeshBuilder,
    p: &Placed,
    range: (f32, f32),
    route_s: f32,
    ground: impl Fn(Vec3) -> f32,
) {
    let (s0, s1) = range;
    // Panel points: the multiples of PANEL along the route, and the ends of the range, where the
    // trusses meet those of the pieces before and after.
    let mut ss = vec![s0];
    let mut k = libm::ceilf((route_s + s0) / PANEL);
    loop {
        let s = k * PANEL - route_s;
        k += 1.0;
        if s >= s1 - 0.5 {
            break;
        }
        if s > s0 + 0.5 {
            ss.push(s);
        }
    }
    ss.push(s1);
    let index = |s: f32| libm::roundf((route_s + s) / PANEL) as i32;
    let sides = |s: f32| sides_at(p, s, &ground);

    // Trusses: along each side, between consecutive nodes that both have one.
    let nodes: Vec<[Option<Node>; 2]> = ss.iter().map(|&s| sides(s).0).collect();
    for i in 0..ss.len() {
        let idx = index(ss[i]);
        let last = i + 1 == ss.len();
        for side in 0..2 {
            let Some(n) = nodes[i][side] else { continue };
            if !last && idx % 2 == 0 {
                tube(b, n.top, n.bottom, BRACE_R, 6);
            }
            if let Some(m) = nodes.get(i + 1).and_then(|next| next[side]) {
                tube(b, n.top, m.top, BEAM_R, 8);
                tube(b, n.bottom, m.bottom, BEAM_R, 8);
                if idx % 2 == 0 {
                    tube(b, n.top, m.bottom, BRACE_R, 6);
                } else {
                    tube(b, n.bottom, m.top, BRACE_R, 6);
                }
            }
        }
        if let ([Some(l), Some(r)], false) = (nodes[i], last) {
            if idx % 2 == 0 {
                tube(b, l.top, r.top, BEAM_R, 8);
            }
            if idx % 4 == 0 {
                tube(b, l.bottom, r.bottom, BRACE_R, 6);
            }
        }
    }

    // Piers: on the sequence along the route, where a truss runs.
    let r0 = route_s + s0;
    let r1 = route_s + s1;
    let first = libm::floorf((r0 - PIER_JITTER) / PIER_SPACING) as i32;
    let last = libm::ceilf((r1 + PIER_JITTER) / PIER_SPACING) as i32;
    for m in first..=last {
        let at = m as f32 * PIER_SPACING + (unit(hash2(0x9e2, m, 0)) - 0.5) * 2.0 * PIER_JITTER;
        if (r0..r1).contains(&at) {
            pier(b, decor, p, at - route_s, range, &ground, hash2(0x9e2, m, 1));
        }
    }

    // Low supports where the slab is too close to the ground for a truss: one in every 16 m of
    // route, at its start or 8 m in at random (8 to 24 m apart).
    for &s in &ss {
        let idx = index(s);
        let block = idx.div_euclid(4);
        let shift = if unit(hash2(0x10e, block, 9)) < 0.5 { 0 } else { 2 };
        if idx != 4 * block + shift || s >= s1 {
            continue;
        }
        let (truss, low, edges) = sides(s);
        for side in 0..2 {
            if truss[side].is_some() {
                continue;
            }
            let Some((top, gap)) = low[side] else { continue };
            let seed = hash2(0x10e, idx, side as i32);
            let f = p.frame(s);
            let foot = top - Vec3::Y * gap;
            if gap < LOW_POST {
                bag_stack(b, decor, foot, gap, f.forward, 1.0, 0.55, BAG, seed);
            } else {
                bag_stack(b, decor, foot, 0.65, f.forward, 1.0, 0.55, BAG, seed);
                tube(b, foot + Vec3::Y * 0.5, top, POST_R, 10);
            }
            if gap >= 0.8 {
                if let Some(e) = edges {
                    strap(decor, p, s, side, e, &ground, 0.0, seed);
                }
            }
        }
    }
}

/// The trusses' nodes at `s` (left, right; none where the slab is too low or the ground too
/// high), the low supports' tops and gaps where there is no room for a truss, and the slab's
/// bottom edges (left, right) where it is raised.
#[allow(clippy::type_complexity)]
fn sides_at(p: &Placed, s: f32, ground: &impl Fn(Vec3) -> f32) -> ([Option<Node>; 2], [Option<(Vec3, f32)>; 2], Option<(Vec3, Vec3)>) {
    let f = p.frame(s);
    let mut truss = [None, None];
    let mut low = [None, None];
    if f.deck != Surface::Road {
        return (truss, low, None);
    }
    let Some((l, r)) = kit::underside(&f, LOW_MIN) else { return (truss, low, None) };
    let span = l.distance(r);
    for (side, u) in [TRUSS_U, -TRUSS_U].into_iter().enumerate() {
        // Along the underside from its left edge, which is `span / 2` left of the centreline.
        let top = l + (r - l) * ((0.5 * span - u) / span) - Vec3::Y * BEAM_R;
        let g = ground(top);
        let gap = top.y - g;
        if gap >= TRUSS_NEED {
            truss[side] = Some(Node { top, bottom: top - Vec3::Y * TRUSS_DEPTH, ground: g });
        } else if gap >= LOW_MIN {
            low[side] = Some((top, gap));
        }
    }
    (truss, low, Some((l, r)))
}

/// A pier at `s` carrying the trusses down to the ground: on each side with a truss, a leg of
/// two posts braced together on a stack of sandbags, the two legs braced across, and straps
/// from the slab's edges to stakes in the ground.
#[allow(clippy::too_many_arguments)]
fn pier(
    b: &mut MeshBuilder,
    decor: &mut MeshBuilder,
    p: &Placed,
    s: f32,
    range: (f32, f32),
    ground: &impl Fn(Vec3) -> f32,
    seed: u32,
) {
    let (truss, _, edges) = sides_at(p, s, ground);
    let f = p.frame(s);
    let mut legs: [Option<(Vec3, Vec3, f32)>; 2] = [None, None];
    for side in 0..2 {
        if truss[side].is_none() {
            continue;
        }
        // The two posts, each under the truss's bottom chord where it stands.
        let post = |ds: f32| {
            let t = (s + ds).clamp(range.0, range.1);
            sides_at(p, t, ground).0[side]
        };
        let (Some(a), Some(c)) = (post(-LEG_HALF), post(LEG_HALF)) else { continue };
        let foot = a.ground.min(c.ground);
        let height = a.bottom.y.min(c.bottom.y) - foot;
        let stack = (0.2 * height).clamp(0.7, 1.7);
        let centre = Vec3::new(0.5 * (a.bottom.x + c.bottom.x), foot, 0.5 * (a.bottom.z + c.bottom.z));
        bag_stack(b, decor, centre, stack, f.forward, 3.4, 2.0, BIG_BAG, hash2(seed, side as i32, 2));
        let base = |n: Node| Vec3::new(n.bottom.x, foot + stack - 0.1, n.bottom.z);
        tube(b, base(a), a.bottom, POST_R, 10);
        tube(b, base(c), c.bottom, POST_R, 10);
        braces(b, base(a), a.bottom, base(c), c.bottom, 2.6);
        legs[side] = Some((base(a), a.bottom, stack));
    }
    if let [Some((la, lt, _)), Some((ra, rt, _))] = legs {
        braces(b, la, lt, ra, rt, 6.0);
    }
    if let Some(e) = edges {
        for side in 0..2 {
            if legs[side].is_some() {
                for (n, along) in [-1.5f32, 1.5].into_iter().enumerate() {
                    strap(decor, p, s, side, e, ground, along, hash2(seed, side as i32, 5 + n as i32));
                }
            }
        }
    }
}

/// X braces between two posts (feet `a0`, `b0`, tops `a1`, `b1`), in storeys about `storey`
/// metres tall, with a ledger between storeys.
fn braces(b: &mut MeshBuilder, a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3, storey: f32) {
    let height = (a1.y - a0.y).min(b1.y - b0.y);
    if height < 1.2 {
        return;
    }
    let n = libm::roundf(height / storey).max(1.0) as u32;
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        tube(b, a0.lerp(a1, t0), b0.lerp(b1, t1), BRACE_R, 6);
        tube(b, a0.lerp(a1, t1), b0.lerp(b1, t0), BRACE_R, 6);
        if i + 1 < n {
            tube(b, a0.lerp(a1, t1), b0.lerp(b1, t1), BRACE_R, 6);
        }
    }
}

/// An orange strap from the slab's bottom edge on `side` (`edges`: left, right), `along` metres
/// from `s`, down and out to a stake in the ground, with its ratchet buckle.
#[allow(clippy::too_many_arguments)]
fn strap(
    decor: &mut MeshBuilder,
    p: &Placed,
    s: f32,
    side: usize,
    edges: (Vec3, Vec3),
    ground: &impl Fn(Vec3) -> f32,
    along: f32,
    seed: u32,
) {
    let f = p.frame(s);
    let top = if side == 0 { edges.0 } else { edges.1 } + f.forward * (0.3 * along);
    let out = if side == 0 { f.left } else { -f.left };
    let height = top.y - ground(top);
    let reach = (0.45 * height).clamp(1.6, 4.0) * (0.85 + 0.3 * unit(seed));
    let spot = top + out * reach + f.forward * along;
    let anchor = Vec3::new(spot.x, ground(spot), spot.z);
    add_tube(decor, top, anchor + Vec3::Y * 0.08, STRAP_R, 4, Surface::Wall, color::STRAP);
    // The stake, leaning back against the pull, and the buckle a third of the way up.
    add_tube(decor, anchor - Vec3::Y * 0.15 - out * 0.04, anchor + Vec3::Y * 0.22 + out * 0.06, 0.025, 4, Surface::Wall, color::STEEL);
    let buckle = anchor.lerp(top, 0.3);
    add_box(decor, buckle, Vec3::new(0.06, 0.045, 0.09), f.forward, Surface::Wall, color::STEEL, true);
}

/// A red plastic tube of the stilts.
fn tube(b: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32, sides: u32) {
    add_tube(b, a, c, r, sides, Surface::Wall, color::TUBE);
}

/// A stack of sandbags of size `bag` (length, height, width), `height` m tall (whole layers of
/// bags), standing on the ground at `foot`, about `long` m along `forward` and `wide` across, each
/// layer laid crosswise to the one under it and a little smaller, every bag a little askew
/// (`seed`). The bags are drawn into `decor`; the car meets a plain box around them, not drawn.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bag_stack(b: &mut MeshBuilder, decor: &mut MeshBuilder, foot: Vec3, height: f32, forward: Vec3, long: f32, wide: f32, bag: Vec3, seed: u32) {
    let left = Vec3::Y.cross(forward);
    let layers = libm::roundf(height / (0.85 * bag.y)).max(1.0) as u32;
    let mut n = 0;
    for layer in 0..layers {
        let shrink = 1.0 - 0.07 * layer as f32;
        let crosswise = layer % 2 == 1;
        // Bags along `dir`, in rows across it.
        let (dir, side, run, rows) = if crosswise { (left, forward, wide, long) } else { (forward, left, long, wide) };
        let per_row = libm::roundf(run * shrink / bag.x).max(1.0) as i32;
        let row_count = libm::roundf(rows * shrink / bag.z).max(1.0) as i32;
        for i in 0..per_row {
            for j in 0..row_count {
                n += 1;
                let h = hash2(seed, n, layer as i32);
                let jitter = |k: u32| unit(hash2(h, k as i32, 3)) - 0.5;
                let a = 0.14 * jitter(0);
                let d = dir * libm::cosf(a) + side * libm::sinf(a);
                let at = foot
                    + dir * ((i as f32 - 0.5 * (per_row - 1) as f32) * 0.92 * bag.x + 0.06 * jitter(1))
                    + side * ((j as f32 - 0.5 * (row_count - 1) as f32) * 0.92 * bag.z + 0.05 * jitter(2))
                    + Vec3::Y * (layer as f32 * 0.85 * bag.y - 0.02);
                let lean = 0.2 * jitter(5);
                let up = Vec3::Y * libm::cosf(lean) + Vec3::Y.cross(d) * libm::sinf(lean);
                add_sandbag(decor, at, d, up, bag * (1.0 + 0.12 * jitter(4)), h, false, Surface::Wall, color::SANDBAG, [0.26 + 0.24 * (jitter(6) + 0.5), 0.3 + 0.7 * (jitter(7) + 0.5)]);
            }
        }
    }
    let top = layers as f32 * 0.85 * bag.y;
    add_box(b, foot + Vec3::Y * (0.5 * top), Vec3::new(0.5 * wide, 0.5 * top, 0.5 * long), forward, Surface::Wall, color::HULL, false);
}

