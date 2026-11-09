//! How the track editor shows the draft: a camera over the grid, straight down (the plan) or
//! tilted in perspective with its heights stretched (the 3D view), and the blocks drawn by it.
//!
//! Everything is drawn with egui's painter: the blocks' decks, the sides of raised ones, the red
//! tubes of their stilts and the gates are flat polygons, projected, sorted from the farthest to
//! the nearest and laid in one mesh in that order (egui draws a mesh's triangles in order, with no
//! depth test). Shadows fall on the ground first, away from the sun. A deck takes its height's
//! colour as it rises (the ramp of [`height_colour`]), road or dirt on the ground; the profile
//! under a selection draws the same heights along the route.

use egui::epaint::{Mesh, PathStroke};
use egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use glam::{Vec2 as W, Vec3};
use track::Surface;
use track::kit::{CELL, Gate, Kind, Placed};

use crate::menu::paint::{self, Font, col, fade};

/// The ground under the plan, the decks, lines and marks.
pub const GROUND: Color32 = Color32::from_rgb(38, 22, 17);
const ROAD_DECK: [f32; 3] = [70.0, 70.0, 78.0];
const DIRT_DECK: [f32; 3] = [168.0, 92.0, 52.0];
const SLAB: [f32; 3] = [154.0, 149.0, 139.0];
const SKIRT: [f32; 3] = [110.0, 98.0, 88.0];
const EDGE_LINE: Color32 = Color32::from_rgb(236, 232, 226);
const TUBE: Color32 = Color32::from_rgb(179, 32, 15);
pub const BOOST: Color32 = Color32::from_rgb(255, 136, 30);
pub const START: Color32 = Color32::from_rgb(61, 220, 132);
pub const CHECKPOINT: Color32 = Color32::from_rgb(85, 200, 255);
pub const SELECTED: Color32 = Color32::from_rgb(255, 212, 63);
pub const CLASH: Color32 = Color32::from_rgb(255, 64, 48);

/// Heights (metres) and their colours, turning from teal through blue and violet to magenta:
/// never the dirt's brown, the boosters' orange or the selection's yellow.
const RAMP: [(f32, [f32; 3]); 6] = [
    (0.0, [88.0, 86.0, 104.0]),
    (8.0, [56.0, 170.0, 170.0]),
    (16.0, [74.0, 128.0, 222.0]),
    (24.0, [148.0, 100.0, 232.0]),
    (32.0, [222.0, 88.0, 200.0]),
    (48.0, [250.0, 200.0, 240.0]),
];
/// Where the light comes from (world, unit), and how far a metre of height throws its shadow
/// on the ground (x, z): down and to the right on the plan.
const SUN: Vec3 = Vec3::new(0.443, 0.837, -0.296);
const SHADOW: W = W::new(-0.45, -0.45);
/// The 3D view: its tilt from straight down (radians), how much it stretches heights, and the
/// camera's distance from the point it looks at (metres; the perspective).
pub const TILT_3D: f32 = 0.9;
pub const RELIEF_3D: f32 = 2.0;
const DISTANCE: f32 = 900.0;

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn rgb(c: [f32; 3], k: f32) -> Color32 {
    let q = |v: f32| (v * k).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(q(c[0]), q(c[1]), q(c[2]))
}

/// The colour of a height on the ramp.
pub fn height_colour(h: f32) -> [f32; 3] {
    for w in RAMP.windows(2) {
        let ((h0, c0), (h1, c1)) = (w[0], w[1]);
        if h <= h1 {
            return mix(c0, c1, ((h - h0) / (h1 - h0)).clamp(0.0, 1.0));
        }
    }
    RAMP[RAMP.len() - 1].1
}

/// A deck's colour: road or dirt on the ground, taking its height's colour as it rises.
fn deck_colour(dirt: bool, h: f32) -> [f32; 3] {
    let base = if dirt { DIRT_DECK } else { ROAD_DECK };
    mix(base, height_colour(h), (h / 8.0).clamp(0.0, 1.0) * 0.75)
}

// ------------------------------------------------------------------------------------- camera

/// The view: where it looks, how close, turned and tilted how much.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// The world point (x, z) drawn at `origin`, which the view turns about.
    pub target: W,
    /// Screen points per metre at the target.
    pub scale: f32,
    /// Turn about the vertical (radians; 0 has north up).
    pub yaw: f32,
    /// Tilt from straight down (radians) and how much heights are stretched.
    pub tilt: f32,
    pub relief: f32,
    pub origin: Pos2,
}

impl Camera {
    pub fn new() -> Self {
        Self { target: W::ZERO, scale: 1.0, yaw: 0.0, tilt: 0.0, relief: 0.0, origin: Pos2::ZERO }
    }

    fn basis(&self) -> (f32, f32, f32, f32) {
        (self.yaw.cos(), self.yaw.sin(), self.tilt.cos(), self.tilt.sin())
    }

    /// A world point on the screen, its depth (larger is farther: the order to draw in) and the
    /// perspective's scale there, negative for a point behind the eye (not to be drawn). Seen
    /// from above with north (+Z) up, +X (west) is on the left.
    pub fn project(&self, p: Vec3) -> (Pos2, f32, f32) {
        let (cy, sy, ct, st) = self.basis();
        let (du, dv, hh) = (-(p.x - self.target.x), p.z - self.target.y, p.y * self.relief);
        let (ru, rv) = (du * cy - dv * sy, du * sy + dv * cy);
        let yc = rv * ct + hh * st;
        let den = DISTANCE + rv * st - hh * ct;
        let f = DISTANCE / den.max(100.0);
        // Flat, the higher deck is drawn over the lower one.
        let depth = rv * st - if self.relief > 0.0 { hh } else { p.y } * ct;
        let pos = pos2(self.origin.x + ru * self.scale * f, self.origin.y - yc * self.scale * f);
        (pos, depth, if den > 100.0 { f } else { -f })
    }

    /// The world point (x, z) at `height` that shows at screen point `q`.
    pub fn unproject(&self, q: Pos2, height: f32) -> W {
        let (cy, sy, ct, st) = self.basis();
        let (a, b, hh) = ((q.x - self.origin.x) / self.scale, -(q.y - self.origin.y) / self.scale, height * self.relief);
        // Above the horizon a ray never meets the ground: take the farthest point it nearly does.
        let den = (ct - b * st / DISTANCE).max(0.05);
        let rv = (b * (1.0 - hh * ct / DISTANCE) - hh * st) / den;
        let ru = a * (DISTANCE + rv * st - hh * ct) / DISTANCE;
        let (du, dv) = (ru * cy + rv * sy, -ru * sy + rv * cy);
        W::new(self.target.x - du, self.target.y + dv)
    }

    /// Keeps the ground point under `at` there while `change` alters the view.
    fn keeping(&mut self, at: Pos2, change: impl FnOnce(&mut Self)) {
        let before = self.unproject(at, 0.0);
        change(self);
        let after = self.unproject(at, 0.0);
        self.target += before - after;
    }

    /// Moves the view with a drag of `d` points: the ground follows the fingers.
    pub fn pan(&mut self, d: egui::Vec2) {
        let (a, b) = (self.unproject(self.origin, 0.0), self.unproject(self.origin - d, 0.0));
        self.target += b - a;
    }

    pub fn zoom(&mut self, at: Pos2, factor: f32) {
        self.keeping(at, |c| c.scale = (c.scale * factor).clamp(0.08, 6.0));
    }

    pub fn rotate(&mut self, at: Pos2, angle: f32) {
        self.keeping(at, |c| c.yaw += angle);
    }

    /// Frames `points` in `area` (the part of the screen the bars leave).
    pub fn fit(&mut self, points: &[Vec3], area: Rect) {
        self.origin = area.center();
        if points.is_empty() {
            return;
        }
        for _ in 0..4 {
            let mut r = Rect::NOTHING;
            for &p in points {
                for q in [p, Vec3::new(p.x, 0.0, p.z)] {
                    let (at, _, f) = self.project(q);
                    if f > 0.0 {
                        r.extend_with(at);
                    }
                }
            }
            if !r.is_positive() {
                return;
            }
            let k = ((area.width() - 48.0) / r.width().max(1.0)).min((area.height() - 48.0) / r.height().max(1.0));
            self.pan(area.center() - r.center());
            self.scale = (self.scale * k).clamp(0.08, 6.0);
        }
    }
}

// ------------------------------------------------------------------------------------- blocks

/// One row across a block's deck, in world space.
pub struct Row {
    pub left: Vec3,
    pub right: Vec3,
    pub centre: Vec3,
    pub dirt: bool,
    /// Distance along the route, metres.
    pub dist: f32,
}

/// A block of the draft as drawn.
pub struct Block {
    pub rows: Vec<Row>,
    pub boost: bool,
    /// A gate's line across the deck, just past its edges.
    pub gate: Option<(Gate, Vec3, Vec3)>,
    /// What a slope or a ramp says of its height.
    pub label: Option<String>,
}

/// The draft's blocks as drawn, their rows' distances along the route counted through.
pub fn blocks(pieces: &[Placed]) -> Vec<Block> {
    let mut dist = 0.0;
    let mut last: Option<Vec3> = None;
    pieces
        .iter()
        .map(|p| {
            let (s0, s1) = p.deck_range();
            let n = ((s1 - s0) / 3.0).ceil().max(2.0) as usize;
            let rows = (0..=n)
                .map(|k| {
                    let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
                    let centre = f.centre();
                    if let Some(l) = last {
                        dist += W::new(centre.x - l.x, centre.z - l.z).length();
                    }
                    last = Some(centre);
                    Row { left: f.deck_point(f.half_width), right: f.deck_point(-f.half_width), centre, dirt: f.deck == Surface::Dirt, dist }
                })
                .collect();
            let gate = p.piece.gate.map(|g| {
                let f = p.frame(p.gate_s());
                (g, f.deck_point(f.half_width + 1.0), f.deck_point(-f.half_width - 1.0))
            });
            let climbs = (p.exit.pos.y - p.entry.pos.y).abs() > 0.5;
            let label = match p.piece.kind {
                Kind::JumpRamp { .. } => Some("SAUT".to_string()),
                _ if climbs => Some(format!("{:.0} m", p.exit.pos.y)),
                _ => None,
            };
            Block { rows, boost: p.piece.boost, gate, label }
        })
        .collect()
}

/// The block shown at screen point `q` (the nearest to the eye where two cross), and the point
/// of its deck there.
pub fn pick(cam: &Camera, blocks: &[Block], q: Pos2) -> Option<(usize, Vec3)> {
    let mut best: Option<(f32, usize, Vec3)> = None;
    for (i, b) in blocks.iter().enumerate() {
        for r in &b.rows {
            let (c, depth, f) = cam.project(r.centre);
            if f < 0.0 {
                continue;
            }
            let reach = (cam.project(r.left).0.distance(cam.project(r.right).0) * 0.5 + 10.0).max(16.0);
            if c.distance(q) <= reach && best.is_none_or(|(d, _, _)| depth < d - 0.5) {
                best = Some((depth, i, r.centre));
            }
        }
    }
    best.map(|(_, i, p)| (i, p))
}

// ------------------------------------------------------------------------------------ drawing

enum Prim {
    Quad([Pos2; 4], Color32),
    Line(Pos2, Pos2, f32, Color32),
}

struct Item {
    depth: f32,
    prims: Vec<Prim>,
}

fn add_quad(m: &mut Mesh, q: [Pos2; 4], c: Color32) {
    let base = m.vertices.len() as u32;
    for p in q {
        m.colored_vertex(p, c);
    }
    m.add_triangle(base, base + 1, base + 2);
    m.add_triangle(base, base + 2, base + 3);
}

fn add_line(m: &mut Mesh, a: Pos2, b: Pos2, w: f32, c: Color32) {
    let d = b - a;
    let len = d.length();
    if len < 1e-3 {
        return;
    }
    let n = vec2(-d.y, d.x) / len * (w * 0.5);
    add_quad(m, [a + n, b + n, b - n, a - n], c);
}

/// What the scene marks besides the blocks: the selected ones and those that cross.
pub struct Marks<'a> {
    pub selection: Option<(usize, usize)>,
    pub clashes: &'a [usize],
}

/// Draws the ground's grid, the shadows and the blocks.
pub fn paint(p: &Painter, cam: &Camera, blocks: &[Block], marks: Marks, screen: Rect) {
    p.rect_filled(screen, 0.0, GROUND);
    grid(p, cam, blocks, screen);
    let solid = cam.relief > 0.05;

    // Shadows on the ground, under everything.
    let mut shadow = Mesh::default();
    let ground = |v: Vec3| cam.project(Vec3::new(v.x + SHADOW.x * v.y, 0.0, v.z + SHADOW.y * v.y)).0;
    for b in blocks {
        for w in b.rows.windows(2) {
            if w[0].centre.y < 0.4 && w[1].centre.y < 0.4 {
                continue;
            }
            add_quad(&mut shadow, [ground(w[0].left), ground(w[0].right), ground(w[1].right), ground(w[1].left)], fade(Color32::BLACK, 0.36));
        }
    }
    p.add(Shape::mesh(shadow));

    let mut items: Vec<Item> = Vec::new();
    let lines = cam.scale > 0.25;
    for (i, b) in blocks.iter().enumerate() {
        let selected = marks.selection.is_some_and(|(a, z)| (a..=z).contains(&i));
        let clash = marks.clashes.contains(&i);
        for (k, w) in b.rows.windows(2).enumerate() {
            let (r0, r1) = (&w[0], &w[1]);
            let pr = |v: Vec3| cam.project(v);
            let q = [pr(r0.left), pr(r0.right), pr(r1.right), pr(r1.left)];
            if q.iter().any(|x| x.2 < 0.0) {
                continue;
            }
            let pts = [q[0].0, q[1].0, q[2].0, q[3].0];
            let depth = (q[0].1 + q[1].1 + q[2].1 + q[3].1) / 4.0;
            // The deck, lit by the sun (slopes and banks show).
            let mut n = (r0.right - r0.left).cross(r1.centre - r0.centre).normalize_or_zero();
            if n.y < 0.0 {
                n = -n;
            }
            let shade = 0.7 + 0.35 * n.dot(SUN).max(0.0);
            let h = 0.5 * (r0.centre.y + r1.centre.y);
            let mut prims = vec![Prim::Quad(pts, rgb(deck_colour(r0.dirt, h), shade))];
            if selected || clash {
                prims.push(Prim::Quad(pts, fade(if clash { CLASH } else { SELECTED }, 0.3)));
            }
            if !r0.dirt && lines {
                prims.push(Prim::Line(pts[0], pts[3], 1.0, fade(EDGE_LINE, 0.55)));
                prims.push(Prim::Line(pts[1], pts[2], 1.0, fade(EDGE_LINE, 0.55)));
            }
            if selected || clash {
                let c = if clash { CLASH } else { SELECTED };
                prims.push(Prim::Line(pts[0], pts[3], 2.0, c));
                prims.push(Prim::Line(pts[1], pts[2], 2.0, c));
            }
            // Arrows: the way the track goes, and the boosters.
            let every = if b.boost { 2 } else { 6 };
            if k % every == every / 2 && cam.scale > 0.2 {
                let f = (r1.centre - r0.centre).normalize_or_zero();
                let l = Vec3::new(f.z, 0.0, -f.x);
                let size = if b.boost { 6.0 } else { 3.5 };
                let c = r0.centre;
                let a = pr(c - f * size + l * size * 1.2).0;
                let m = pr(c + f * size * 0.6).0;
                let z = pr(c - f * size - l * size * 1.2).0;
                let (colour, width) = if b.boost { (BOOST, 2.5) } else { (fade(Color32::WHITE, 0.25), 1.5) };
                prims.push(Prim::Line(a, m, width, colour));
                prims.push(Prim::Line(m, z, width, colour));
            }
            items.push(Item { depth, prims });
            if solid {
                sides(&mut items, cam, r0, r1);
                if k % 4 == 0 && r0.centre.y > 2.5 {
                    stilts(&mut items, cam, r0);
                }
            }
        }
        if let Some((gate, a, z)) = b.gate {
            items.push(gate_item(cam, gate, a, z, solid));
        }
    }
    items.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    let mut mesh = Mesh::default();
    for it in &items {
        for prim in &it.prims {
            match *prim {
                Prim::Quad(q, c) => add_quad(&mut mesh, q, c),
                Prim::Line(a, b, w, c) => add_line(&mut mesh, a, b, w, c),
            }
        }
    }
    p.add(Shape::mesh(mesh));

    // Heights on the slopes, once the plan is close enough to read them.
    if cam.scale > 0.45 {
        for b in blocks {
            if let Some(s) = &b.label {
                let mid = &b.rows[b.rows.len() / 2];
                let (at, _, f) = cam.project(mid.centre);
                if f < 0.0 {
                    continue;
                }
                let r = Rect::from_center_size(at, paint::text_size(p, s, Font::data(10.0)));
                p.rect_filled(r.expand2(vec2(5.0, 2.0)), 4.0, fade(col::VOID, 0.7));
                paint::text(p, at, Align2::CENTER_CENTER, s, Font::data(10.0), fade(col::TEXT, 0.95));
            }
        }
    }
}

/// The sides of a raised deck: a skirt down to the ground when low, the slab's edge higher up.
fn sides(items: &mut Vec<Item>, cam: &Camera, r0: &Row, r1: &Row) {
    for (e0, e1) in [(r0.left, r1.left), (r0.right, r1.right)] {
        if e0.y < 0.3 && e1.y < 0.3 {
            continue;
        }
        let high = e0.y.min(e1.y) > 2.5;
        let drop = |e: Vec3| if high { e - Vec3::Y * 0.8 } else { Vec3::new(e.x, 0.0, e.z) };
        let q = [cam.project(e0), cam.project(e1), cam.project(drop(e1)), cam.project(drop(e0))];
        let out = Vec3::new(e0.x - r0.centre.x, 0.0, e0.z - r0.centre.z).normalize_or_zero();
        let shade = 0.55 + 0.4 * out.dot(SUN).max(0.0);
        let depth = (q[0].1 + q[1].1 + q[2].1 + q[3].1) / 4.0 + 0.01;
        items.push(Item { depth, prims: vec![Prim::Quad([q[0].0, q[1].0, q[2].0, q[3].0], rgb(if high { SLAB } else { SKIRT }, shade))] });
    }
}

/// Red tubes under a raised slab, down to the ground.
fn stilts(items: &mut Vec<Item>, cam: &Camera, r: &Row) {
    for e in [r.left, r.right] {
        let q = e + (r.centre - e) * 0.05 - Vec3::Y * 0.8;
        let (top, d0, f) = cam.project(q);
        let (bottom, d1, _) = cam.project(Vec3::new(q.x, 0.0, q.z));
        let w = (0.7 * cam.scale * f).max(1.2);
        items.push(Item { depth: 0.5 * (d0 + d1) + 0.02, prims: vec![Prim::Line(top, bottom, w, TUBE)] });
    }
}

/// A gate: a line across the deck on the plan, an arch over it in 3D.
fn gate_item(cam: &Camera, gate: Gate, a: Vec3, b: Vec3, solid: bool) -> Item {
    let colour = match gate {
        Gate::Start => START,
        Gate::Checkpoint => CHECKPOINT,
        Gate::Finish => Color32::WHITE,
    };
    let chequers = |prims: &mut Vec<Prim>, p0: Pos2, p1: Pos2, q0: Pos2, q1: Pos2| {
        let n = 10;
        for k in 0..n {
            let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            let c = if k % 2 == 0 { Color32::from_gray(240) } else { Color32::from_gray(20) };
            prims.push(Prim::Quad([p0.lerp(p1, t0), p0.lerp(p1, t1), q0.lerp(q1, t1), q0.lerp(q1, t0)], c));
        }
    };
    let mut prims = Vec::new();
    if !solid {
        let (pa, da, _) = cam.project(a);
        let (pb, db, _) = cam.project(b);
        if gate == Gate::Finish {
            let d = pb - pa;
            let n = vec2(-d.y, d.x).normalized() * 3.0;
            chequers(&mut prims, pa - n, pb - n, pa + n, pb + n);
        } else {
            prims.push(Prim::Line(pa, pb, 4.0, colour));
        }
        return Item { depth: 0.5 * (da + db) - 0.2, prims };
    }
    let up = |v: Vec3, h: f32| cam.project(v + Vec3::Y * h);
    let (a0, b0) = (up(a, 0.0), up(b, 0.0));
    let (a1, b1) = (up(a, 8.7), up(b, 8.7));
    let (a2, b2) = (up(a, 7.3), up(b, 7.3));
    let w = (0.9 * cam.scale * a1.2).max(1.5);
    let post = if gate == Gate::Finish { Color32::from_gray(232) } else { colour };
    prims.push(Prim::Line(a0.0, a1.0, w, post));
    prims.push(Prim::Line(b0.0, b1.0, w, post));
    if gate == Gate::Finish {
        chequers(&mut prims, a1.0, b1.0, a2.0, b2.0);
    } else {
        prims.push(Prim::Quad([a1.0, b1.0, b2.0, a2.0], colour));
    }
    Item { depth: 0.5 * (a1.1 + b1.1) - 0.5, prims }
}

/// The ground's grid of cells, every fourth line brighter.
fn grid(p: &Painter, cam: &Camera, blocks: &[Block], screen: Rect) {
    let cell = CELL * cam.scale;
    if cell < 5.0 {
        return;
    }
    // Over what the screen shows of the ground, and around the draft.
    let mut lo = W::splat(f32::MAX);
    let mut hi = W::splat(f32::MIN);
    for q in [screen.left_top(), screen.right_top(), screen.left_bottom(), screen.right_bottom(), screen.center()] {
        let w = cam.unproject(q, 0.0);
        lo = lo.min(w);
        hi = hi.max(w);
    }
    let reach = W::splat(80.0 * CELL);
    lo = lo.max(cam.target - reach);
    hi = hi.min(cam.target + reach);
    if blocks.is_empty() {
        lo = lo.min(cam.target - W::splat(12.0 * CELL));
        hi = hi.max(cam.target + W::splat(12.0 * CELL));
    }
    let (x0, x1) = ((lo.x / CELL).floor() as i32, (hi.x / CELL).ceil() as i32);
    let (z0, z1) = ((lo.y / CELL).floor() as i32, (hi.y / CELL).ceil() as i32);
    for major in [false, true] {
        let every = if major { 4 } else { 1 };
        if cell * (every as f32) < 6.0 {
            continue;
        }
        let stroke = Stroke::new(1.0, fade(Color32::WHITE, if major { 0.09 } else { 0.035 }));
        let seg = |a: Vec3, b: Vec3| {
            let ((pa, _, fa), (pb, _, fb)) = (cam.project(a), cam.project(b));
            if fa > 0.0 && fb > 0.0 {
                p.line_segment([pa, pb], stroke);
            }
        };
        for i in x0..=x1 {
            if i.rem_euclid(every) == 0 && (major || i.rem_euclid(4) != 0) {
                let x = i as f32 * CELL;
                seg(Vec3::new(x, 0.0, z0 as f32 * CELL), Vec3::new(x, 0.0, z1 as f32 * CELL));
            }
        }
        for k in z0..=z1 {
            if k.rem_euclid(every) == 0 && (major || k.rem_euclid(4) != 0) {
                let z = k as f32 * CELL;
                seg(Vec3::new(x0 as f32 * CELL, 0.0, z), Vec3::new(x1 as f32 * CELL, 0.0, z));
            }
        }
    }
}

/// The stroke being drawn, on the plane it is drawn on.
pub fn paint_stroke(p: &Painter, cam: &Camera, stroke: &[W], height: f32) {
    if stroke.len() < 2 {
        return;
    }
    let pts: Vec<Pos2> = stroke.iter().map(|w| cam.project(Vec3::new(w.x, height, w.y)).0).collect();
    p.add(Shape::line(pts, PathStroke::new(3.0, fade(Color32::WHITE, 0.75))));
}

/// The heights along the route, the selection picked out on them.
pub fn paint_profile(p: &Painter, rect: Rect, blocks: &[Block], selection: Option<(usize, usize)>) {
    let Some(length) = blocks.last().and_then(|b| b.rows.last()).map(|r| r.dist).filter(|&l| l > 1.0) else { return };
    let top_h = blocks.iter().flat_map(|b| &b.rows).map(|r| r.centre.y).fold(0.0, f32::max);
    let top = ((top_h + 2.0) / 8.0).ceil().max(2.0) * 8.0;
    paint::panel(p, rect, 10.0, col::GLASS_DARK, Some((1.0, col::LINE)));
    let inner = Rect::from_min_max(rect.min + vec2(40.0, 10.0), rect.max - vec2(12.0, 12.0));
    let x = |d: f32| inner.min.x + d / length * inner.width();
    let y = |h: f32| inner.max.y - h / top * inner.height();
    // The levels: every 8 m.
    let mut h = 0.0;
    while h <= top + 0.1 {
        let yy = y(h);
        p.line_segment([pos2(inner.min.x, yy), pos2(inner.max.x, yy)], Stroke::new(1.0, fade(Color32::WHITE, if h == 0.0 { 0.18 } else { 0.06 })));
        paint::text(p, pos2(inner.min.x - 6.0, yy), Align2::RIGHT_CENTER, &format!("{h:.0} m"), Font::data(9.0), col::DUST_3);
        h += if top > 32.0 { 16.0 } else { 8.0 };
    }
    // The selection's stretch.
    if let Some((a, z)) = selection.filter(|&(_, z)| z < blocks.len()) {
        let (d0, d1) = (blocks[a].rows[0].dist, blocks[z].rows.last().map_or(0.0, |r| r.dist));
        p.rect_filled(Rect::from_x_y_ranges(x(d0)..=x(d1).max(x(d0) + 2.0), inner.y_range()), 3.0, fade(SELECTED, 0.16));
    }
    // The heights, filled with their colours.
    let mut fill = Mesh::default();
    let rows: Vec<&Row> = blocks.iter().flat_map(|b| &b.rows).collect();
    for w in rows.windows(2) {
        let (r0, r1) = (w[0], w[1]);
        let (x0, x1) = (x(r0.dist), x(r1.dist));
        if x1 - x0 < 0.01 {
            continue;
        }
        let c = |h: f32| fade(rgb(height_colour(h), 1.0), 0.55);
        let base = fill.vertices.len() as u32;
        fill.colored_vertex(pos2(x0, y(0.0)), c(0.0));
        fill.colored_vertex(pos2(x0, y(r0.centre.y)), c(r0.centre.y));
        fill.colored_vertex(pos2(x1, y(r1.centre.y)), c(r1.centre.y));
        fill.colored_vertex(pos2(x1, y(0.0)), c(0.0));
        fill.add_triangle(base, base + 1, base + 2);
        fill.add_triangle(base, base + 2, base + 3);
    }
    p.add(Shape::mesh(fill));
    let line: Vec<Pos2> = rows.iter().map(|r| pos2(x(r.dist), y(r.centre.y))).collect();
    p.add(Shape::line(line, PathStroke::new(1.5, col::TEXT)));
    // The gates.
    for b in blocks {
        if let Some((g, _, _)) = b.gate {
            let xx = x(0.5 * (b.rows[0].dist + b.rows.last().map_or(0.0, |r| r.dist)));
            let c = match g {
                Gate::Start => START,
                Gate::Checkpoint => CHECKPOINT,
                Gate::Finish => Color32::WHITE,
            };
            p.line_segment([pos2(xx, inner.min.y), pos2(xx, inner.max.y)], Stroke::new(1.5, c));
        }
    }
}

/// A small legend of the height colours, `top` metres at its right end.
pub fn paint_legend(p: &Painter, at: Pos2, top: f32) {
    let w = 96.0;
    let mut m = Mesh::default();
    let n = 24;
    for k in 0..n {
        let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
        let c = |t: f32| rgb(deck_colour(false, t * top), 1.0);
        let base = m.vertices.len() as u32;
        m.colored_vertex(pos2(at.x + t0 * w, at.y), c(t0));
        m.colored_vertex(pos2(at.x + t1 * w, at.y), c(t1));
        m.colored_vertex(pos2(at.x + t1 * w, at.y + 6.0), c(t1));
        m.colored_vertex(pos2(at.x + t0 * w, at.y + 6.0), c(t0));
        m.add_triangle(base, base + 1, base + 2);
        m.add_triangle(base, base + 2, base + 3);
    }
    p.add(Shape::mesh(m));
    paint::text(p, pos2(at.x - 6.0, at.y + 3.0), Align2::RIGHT_CENTER, "0", Font::data(9.0), col::DUST_3);
    paint::text(p, pos2(at.x + w + 6.0, at.y + 3.0), Align2::LEFT_CENTER, &format!("{top:.0} m"), Font::data(9.0), col::DUST_3);
}
