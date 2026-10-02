//! Writes the shape sheet of the block catalogue (docs/blocks.md, « La planche des formes »): one
//! SVG per shape in `docs/blocks/`, the block in a three-quarter view over its grid cells, built
//! from the same frames as the game's meshes. The deck is drawn plain (tarp or dirt) with its
//! borders as coloured lines; what stands above the ground hangs a curtain down to it, with a
//! red mark every 8 m like the stilts, so heights read at a glance. Rows share a scale, so the
//! sizes of a family compare (a smaller block is drawn at most twice as big).
//!
//!     cargo run -p track --example shapes [-- OUT_DIR]
//!
//! Rerun it after changing a block's geometry or the catalogue. The ice planet's shapes
//! (docs/blocks-ice.md) go to their own sheet, `docs/blocks-ice/` (`OUT_DIR-ice`), drawn in ice
//! and snow.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};

use glam::{Vec2, Vec3};
use track::Surface;
use track::kit::{CELL, Connector, Edge, Gate, Heading, Kind, LEVEL, Layout, Placed};
use track::map::parse_block;

/// One tile: the pieces chained from `level`, heading north.
struct Tile {
    pieces: &'static [(&'static str, Option<&'static str>)],
    level: i32,
}

const fn tile(pieces: &'static [(&'static str, Option<&'static str>)], level: i32) -> Tile {
    Tile { pieces, level }
}

/// The sheet, row by row (a row shares a scale). Left versions only: `_right` is the mirror.
const ROWS: &[&[Tile]] = &[
    &[tile(&[("straight", None)], 0), tile(&[("straight2", None)], 0), tile(&[("straight3", None)], 0)],
    &[tile(&[("start", None)], 0), tile(&[("checkpoint", None)], 0), tile(&[("finish", None)], 0)],
    &[tile(&[("turn1_left", None)], 0), tile(&[("turn2_left", None)], 0), tile(&[("turn3_left", None)], 0)],
    &[tile(&[("banked1_left", None)], 1), tile(&[("banked2_left", None)], 1), tile(&[("banked3_left", None)], 1)],
    &[tile(&[("berm1_left", None)], 0), tile(&[("berm2_left", None)], 0), tile(&[("berm3_left", None)], 0)],
    &[tile(&[("uberm1_left", None)], 0), tile(&[("uberm2_left", None)], 0), tile(&[("uberm3_left", None)], 0)],
    &[
        tile(&[("slope1_up1", None)], 0),
        tile(&[("slope2_up1", None)], 0),
        tile(&[("slope3_up1", None)], 0),
        tile(&[("slope4_up1", None)], 0),
    ],
    &[
        tile(&[("slope1_up2", None)], 0),
        tile(&[("slope2_up2", None)], 0),
        tile(&[("slope3_up2", None)], 0),
        tile(&[("slope4_up2", None)], 0),
    ],
    &[tile(&[("whoops1", None)], 0), tile(&[("whoops2", None)], 0), tile(&[("whoops3", None)], 0)],
    &[tile(&[("to_dirt", None)], 0), tile(&[("to_road", None)], 0), tile(&[("berm2_left", Some("dirt"))], 0)],
    &[
        tile(&[("jump_ramp", None), ("landing4_down1", None)], 1),
        tile(&[("kicker", None), ("kicker_landing5_down2", None)], 2),
        tile(&[("kicker", None), ("kicker_landing5_down2_left", None)], 2),
    ],
];

/// The ice planet's sheet (docs/blocks-ice.md): its drift turns and S-bends on ice, its snow
/// track's shapes.
const ICE_ROWS: &[&[Tile]] = &[
    &[tile(&[("curve2_left", None)], 0), tile(&[("curve3_left", None)], 0), tile(&[("curve4_left", None)], 0)],
    &[tile(&[("curveberm2_left", None)], 0), tile(&[("curveberm3_left", None)], 0), tile(&[("curveberm4_left", None)], 0)],
    &[tile(&[("sbend2_left", None)], 0), tile(&[("sbend3_left", None)], 0), tile(&[("sbend4_left", None)], 0)],
    &[tile(&[("snake2", Some("snow"))], 0), tile(&[("snake3", Some("snow"))], 0), tile(&[("snake4", Some("snow"))], 0)],
    &[tile(&[("to_dirt", Some("snow"))], 0), tile(&[("sbend2_left", Some("snow"))], 0), tile(&[("uberm1_left", Some("snow"))], 0)],
    &[
        tile(&[("curve3_left_down2", None)], 2),
        tile(&[("curveberm3_left_up1", None)], 0),
        tile(&[("curve2_left_down1", Some("snow"))], 1),
    ],
    &[tile(&[("berm2_left_up1", Some("snow"))], 0), tile(&[("curveberm2_left_down1", Some("snow"))], 1), tile(&[("uberm2_left_up1", Some("snow"))], 0)],
];

/// Drawing the ice planet's sheet: ice for road decks, snow for dirt ones.
static ICE: AtomicBool = AtomicBool::new(false);
const ICE_DECK: [f32; 3] = [150.0, 196.0, 228.0];
const SNOW_DECK: [f32; 3] = [196.0, 205.0, 218.0];
const RIB_ICE: &str = "#4f7fa6";
const RIB_SNOW: &str = "#7b8aa0";

/// A deck's colour at height `y`.
fn deck_colour(deck: Surface, y: f32) -> [f32; 3] {
    match (ICE.load(Ordering::Relaxed), deck) {
        (true, Surface::Dirt) => SNOW_DECK,
        (true, _) => ICE_DECK,
        (false, Surface::Dirt) => DIRT,
        (false, _) => height_tint(y),
    }
}

// Tile size and drawing area, SVG pixels.
const W: f32 = 800.0;
const H: f32 = 540.0;
const PAD: f32 = 28.0;
const TOP: f32 = 92.0;

// Palette (the doc's: tarp, regolith, tube red, strap orange, sandbag cloth).
const PAPER: &str = "#fbf9f5";
const INK: &str = "#2a1d17";
const MUTED: &str = "#6f5a4e";
const GROUND: &str = "#ece4d8";
const CELL_FILL: &str = "#dfd1bd";
const GRID: &str = "#c9b7a1";
const SHADOW: &str = "#2a1d17";
const TARP: [f32; 3] = [252.0, 250.0, 245.0];
/// The tarp drawn ever more apricot the higher it is (see [`height_tint`]).
const HIGH: [f32; 3] = [240.0, 190.0, 150.0];
const DIRT: [f32; 3] = [190.0, 96.0, 52.0];
const SIDE: &str = "#b9a48f";
const CURTAIN: &str = "#e7c6b8";
/// The open end of a raised deck.
const END: &str = "#dbb5a5";
/// Ribs across the deck every 8 m, which show its bank and grade.
const RIB_ROAD: &str = "#b8aa96";
const RIB_DIRT: &str = "#8f4020";
const STILT: &str = "#c8231b";
const BUMPERS: &str = "#c8231b";
const SANDBAGS: &str = "#a38659";
const DIRT_EDGE: &str = "#8f4020";
const ORANGE: &str = "#e2690f";
const START: &str = "#2f8a3a";
const CHECKPOINT: &str = "#2a6fd6";
const FINISH: &str = "#c8231b";

/// Gate posts and beam, as `kit` builds them (its constants are private to the crate).
const GATE_POST_U_ROAD: f32 = 11.7;
const GATE_POST_U_DIRT: f32 = 18.0;
const GATE_BEAM: (f32, f32) = (7.5, 8.7);
/// Thickness of a raised slab.
const SLAB: f32 = 0.8;

/// A three-quarter view from behind and to the right of a block heading north: orthographic,
/// `r` to the right of the picture, `u` up it, `f` into it.
struct Camera {
    r: Vec3,
    u: Vec3,
    f: Vec3,
    light: Vec3,
}

impl Camera {
    fn new() -> Self {
        let (az, el) = (62f32.to_radians(), 30f32.to_radians());
        let h = Vec3::new(az.sin(), 0.0, az.cos());
        let f = (h * el.cos() - Vec3::Y * el.sin()).normalize();
        let r = f.cross(Vec3::Y).normalize();
        let u = r.cross(f);
        Self { r, u, f, light: Vec3::new(0.35, 1.0, -0.45).normalize() }
    }

    /// Picture-plane coordinates, y down.
    fn plane(&self, p: Vec3) -> Vec2 {
        Vec2::new(p.dot(self.r), -p.dot(self.u))
    }

    fn depth(&self, p: Vec3) -> f32 {
        p.dot(self.f)
    }
}

enum Prim {
    Poly(Vec<Vec3>),
    Line(Vec3, Vec3),
    /// Text at a point, offset in pixels.
    Text(Vec3, Vec2, String),
}

/// Something to draw: on the ground (in order, first), in the scene (back to front), or on top.
struct Item {
    layer: u8,
    depth: f32,
    prim: Prim,
    style: String,
}

#[derive(Default)]
struct Scene {
    items: Vec<Item>,
}

impl Scene {
    fn ground(&mut self, prim: Prim, style: String) {
        self.items.push(Item { layer: 0, depth: 0.0, prim, style });
    }

    fn solid(&mut self, cam: &Camera, pts: Vec<Vec3>, style: String, bias: f32) {
        let c = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        self.items.push(Item { layer: 1, depth: cam.depth(c) - bias, prim: Prim::Poly(pts), style });
    }

    fn line(&mut self, depth: f32, a: Vec3, b: Vec3, style: String) {
        self.items.push(Item { layer: 1, depth, prim: Prim::Line(a, b), style });
    }

    fn label(&mut self, at: Vec3, offset: Vec2, text: String, style: String) {
        self.items.push(Item { layer: 2, depth: 0.0, prim: Prim::Text(at, offset, text), style });
    }

    /// Bounds in the picture plane (labels excluded).
    fn bounds(&self, cam: &Camera) -> (Vec2, Vec2) {
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        let mut add = |p: Vec3| {
            let q = cam.plane(p);
            lo = lo.min(q);
            hi = hi.max(q);
        };
        for it in &self.items {
            match &it.prim {
                Prim::Poly(pts) => pts.iter().for_each(|&p| add(p)),
                Prim::Line(a, b) => {
                    add(*a);
                    add(*b);
                }
                Prim::Text(..) => {}
            }
        }
        (lo, hi)
    }
}

/// The road's colour at height `y`: the tarp, tinted toward [`HIGH`] quickly over the first
/// metres and slowly higher up.
fn height_tint(y: f32) -> [f32; 3] {
    let t = 1.0 - (-y.max(0.0) / 10.0).exp();
    [0, 1, 2].map(|i| TARP[i] + (HIGH[i] - TARP[i]) * t)
}

fn rgb(c: [f32; 3], k: f32) -> String {
    let ch = |v: f32| (v * k).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", ch(c[0]), ch(c[1]), ch(c[2]))
}

/// A filled face with an outline of its own colour, so neighbouring faces leave no seam.
fn face(fill: &str) -> String {
    format!("fill=\"{fill}\" stroke=\"{fill}\" stroke-width=\"0.8\" stroke-linejoin=\"round\"")
}

/// French number: decimal comma, no trailing ",0".
fn fr(x: f32, decimals: usize) -> String {
    let s = format!("{x:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

fn plural(n: u32, word: &str) -> String {
    format!("{n} {word}{}", if n > 1 { "s" } else { "" })
}

fn build_layout(t: &Tile) -> Layout {
    let mut layout = Layout::new("shape", Connector::entering((0, 0), t.level, Heading::North));
    for &(id, variant) in t.pieces {
        let piece = parse_block(id, variant).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        layout.push(piece);
    }
    layout
}

fn title(t: &Tile) -> String {
    t.pieces
        .iter()
        .map(|&(id, v)| v.map_or(id.to_string(), |v| format!("{id} {v}")))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn file_name(t: &Tile) -> String {
    let &(id, v) = t.pieces.last().unwrap();
    v.map_or(id.to_string(), |v| format!("{id}_{v}"))
}

/// How a turn climbs or descends, for its caption.
fn climb(p: &Placed) -> String {
    let rise = p.exit.pos.y - p.entry.pos.y;
    if rise.abs() < 0.1 {
        String::new()
    } else {
        format!(" · {} de {} m", if rise > 0.0 { "monte" } else { "descend" }, fr(rise.abs(), 0))
    }
}

/// Tightest radius along a piece's centreline, metres.
fn tightest(p: &Placed) -> f32 {
    let h = 0.25;
    let mut best = f32::INFINITY;
    let mut s = h;
    while s <= p.length - h {
        let (a, b, c) = (p.frame(s - h).horiz, p.frame(s).horiz, p.frame(s + h).horiz);
        let area2 = (b - a).cross(c - a).length();
        if area2 > 1e-6 {
            best = best.min(a.distance(b) * b.distance(c) * c.distance(a) / (2.0 * area2));
        }
        s += h;
    }
    best
}

/// Largest bank along a piece, degrees.
fn max_bank(p: &Placed) -> f32 {
    p.samples().into_iter().map(|s| p.frame(s).bank.abs()).fold(0.0, f32::max).to_degrees()
}

/// How much the deck's highest edge rises above its entry, metres.
fn edge_rise(p: &Placed) -> f32 {
    let base = p.entry.pos.y;
    p.samples()
        .into_iter()
        .map(|s| {
            let f = p.frame(s);
            let hw = f.half_width;
            f.deck_point(hw).y.max(f.deck_point(-hw).y) - base
        })
        .fold(0.0, f32::max)
}

/// The tile's caption: sizes and heights, from the pieces themselves.
fn spec(layout: &Layout) -> String {
    let p = &layout.pieces[0];
    match (p.piece.kind, p.piece.gate) {
        (Kind::Straight { .. }, Some(g)) => {
            let at = match g {
                Gate::Start => format!("portique à {} m de l'entrée", fr(p.gate_s(), 0)),
                _ => "portique au milieu".into(),
            };
            format!("1 cellule · {at}")
        }
        (Kind::Straight { cells }, None) => format!("{} · {} m", plural(cells, "cellule"), fr(p.length, 0)),
        (Kind::Turn { size, quarters, bank_deg, pivot, .. }, _) => {
            let r = fr((size as f32 - 0.5) * CELL, 0);
            let cells = if quarters >= 2 { format!("{} × {size} cellules", 2 * size) } else { format!("{size} × {size} {}", if size > 1 { "cellules" } else { "cellule" }) };
            if bank_deg == 0.0 {
                format!("rayon {r} m · {cells}{}", climb(p))
            } else if pivot == track::kit::Pivot::Centre {
                format!("rayon {r} m · dévers {}° autour de l'axe · ici au niveau {}", fr(max_bank(p), 0), p.entry.level().unwrap_or(0))
            } else {
                let deck = match (p.piece.deck, p.piece.narrow) {
                    (Surface::Dirt, true) => "neige 16 m · ",
                    (Surface::Dirt, false) => "terre 28 m · ",
                    _ => "",
                };
                if p.exit.pos.y != p.entry.pos.y {
                    format!("{deck}rayon {r} m · {cells} · relevé de {}°{}", fr(max_bank(p), 0), climb(p))
                } else {
                    format!("{deck}rayon {r} m · {cells} · l'extérieur monte de {} m ({}°)", fr(edge_rise(p), 1), fr(max_bank(p), 0))
                }
            }
        }
        (Kind::Slope { cells, levels }, _) => {
            let rise = levels as f32 * LEVEL;
            let avg = (rise.abs() / p.length).atan().to_degrees();
            let steepest = p.samples().into_iter().map(|s| p.grade(s).abs()).fold(0.0, f32::max).atan().to_degrees();
            format!(
                "+{} m sur {} m ({}) · {}° en moyenne, {}° au plus raide",
                fr(rise, 0),
                fr(p.length, 0),
                plural(cells, "cellule"),
                fr(avg, 0),
                fr(steepest, 0)
            )
        }
        (Kind::Whoops { cells, bumps, height }, _) => {
            format!("{} · {} de {} m", plural(cells, "cellule"), plural(bumps, "bosse"), fr(height, 1))
        }
        (Kind::Transition { to }, _) => match (to, p.piece.narrow) {
            (Surface::Dirt, true) => "1 cellule · glace 20 m, puis neige 16 m à mi-cellule".into(),
            (Surface::Dirt, false) => "1 cellule · route 20 m, puis terre 28 m à mi-cellule".into(),
            (_, true) => "1 cellule · neige 16 m, puis glace 20 m à mi-cellule".into(),
            (_, false) => "1 cellule · terre 28 m, puis route 20 m à mi-cellule".into(),
        },
        (Kind::Curve { size, bank_deg, .. }, _) => {
            let apex = tightest(p);
            let climbs = p.exit.pos.y != p.entry.pos.y;
            let rise = match (bank_deg == 0.0, climbs) {
                (true, _) => String::new(),
                (false, true) => format!(" · relevé de {}°", fr(max_bank(p), 0)),
                (false, false) => format!(" · l'extérieur monte de {} m ({}°)", fr(edge_rise(p), 1), fr(max_bank(p), 0)),
            };
            let deck = if p.piece.narrow { "neige 16 m · " } else { "" };
            format!("{deck}{size} × {size} cellules · {} m · rayon {} m à l'apex{rise}{}", fr(p.length, 0), fr(apex, 0), climb(p))
        }
        (Kind::Shift { cells, .. }, _) => {
            let deck = if p.piece.narrow { "neige 16 m · " } else { "" };
            format!("{deck}une cellule de côté sur {} · rayon {} m au plus serré", plural(cells, "cellule"), fr(tightest(p), 0))
        }
        (Kind::Snake { cells }, _) => {
            format!(
                "neige 16 m · {} · {} de {} m · rayon {} m",
                plural(cells, "cellule"),
                plural(cells, "virage"),
                fr(track::kit::SNAKE_SWERVE, 0),
                fr(tightest(p), 0)
            )
        }
        (Kind::JumpRamp { lip_deg }, _) => {
            let landing = &layout.pieces[1];
            let prof = landing.landing().unwrap();
            let cells = (landing.length / CELL).round() as u32;
            let shift = match landing.piece.kind {
                Kind::Landing { shift: 1, .. } => " · finit une cellule à gauche",
                Kind::Landing { shift: -1, .. } => " · finit une cellule à droite",
                _ => "",
            };
            format!(
                "lèvre à {}° · vide de {} m · réception de {}, qui finit {} m sous la lèvre{shift}",
                fr(lip_deg, 0),
                fr(prof.gap, 0),
                plural(cells, "cellule"),
                fr(landing.entry.pos.y - landing.exit.pos.y, 0)
            )
        }
        _ => String::new(),
    }
}

/// Builds a tile's scene.
fn scene(cam: &Camera, layout: &Layout) -> Scene {
    let mut sc = Scene::default();

    // The ground: the occupied cells, the grid half a cell beyond them.
    let cells: Vec<(i32, i32)> = layout.pieces.iter().flat_map(|p| p.cells()).collect();
    let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
    for &(i, k) in &cells {
        lo = (lo.0.min(i), lo.1.min(k));
        hi = (hi.0.max(i), hi.1.max(k));
    }
    let (x0, x1) = ((lo.0 as f32 - 0.5) * CELL, (hi.0 as f32 + 1.5) * CELL);
    let (z0, z1) = ((lo.1 as f32 - 0.5) * CELL, (hi.1 as f32 + 1.5) * CELL);
    let g = |x: f32, z: f32| Vec3::new(x, 0.0, z);
    sc.ground(Prim::Poly(vec![g(x0, z0), g(x1, z0), g(x1, z1), g(x0, z1)]), format!("fill=\"{GROUND}\""));
    for &(i, k) in &cells {
        let (a, b) = (i as f32 * CELL, k as f32 * CELL);
        sc.ground(Prim::Poly(vec![g(a, b), g(a + CELL, b), g(a + CELL, b + CELL), g(a, b + CELL)]), format!("fill=\"{CELL_FILL}\""));
    }
    let grid = format!("stroke=\"{GRID}\" stroke-width=\"1.2\"");
    for i in lo.0..=hi.0 + 1 {
        let x = i as f32 * CELL;
        sc.ground(Prim::Line(g(x, z0), g(x, z1)), grid.clone());
    }
    for k in lo.1..=hi.1 + 1 {
        let z = k as f32 * CELL;
        sc.ground(Prim::Line(g(x0, z), g(x1, z)), grid.clone());
    }

    let mut route_s = 0.0;
    let n = layout.pieces.len();
    for (i, p) in layout.pieces.iter().enumerate() {
        piece(cam, &mut sc, p, route_s);
        // Close the raised ends that face a gap or the end of the tile.
        let (s0, s1) = p.deck_range();
        if i == 0 || s0 > 0.0 {
            end_cap(cam, &mut sc, &p.frame(s0));
        }
        if i + 1 == n || layout.pieces[i + 1].deck_range().0 > 0.0 {
            end_cap(cam, &mut sc, &p.frame(s1));
        }
        route_s += p.length;
    }

    // Heights at the joins, when the tile leaves the ground.
    let raised = layout.pieces.iter().any(|p| p.entry.pos.y.abs() > 0.1 || p.exit.pos.y.abs() > 0.1);
    if raised {
        let label_style = format!("fill=\"{INK}\" font-size=\"19\" font-weight=\"600\" text-anchor=\"middle\"");
        let first = &layout.pieces[0];
        let last = layout.pieces.last().unwrap();
        for (c, s) in [(first.entry, 0.0), (last.exit, last.length)] {
            let piece = if s == 0.0 { first } else { last };
            let f = piece.frame(s);
            let side = f.horiz - f.left * (f.half_width + 9.0);
            let at = Vec3::new(side.x, c.pos.y, side.z);
            sc.label(at, Vec2::new(0.0, 6.0), format!("{} m", fr(c.pos.y, 0)), label_style.clone());
        }
    }

    // How high a berm's outside rises, at the middle of the turn.
    for p in &layout.pieces {
        if let Kind::Turn { bank_deg, pivot: track::kit::Pivot::Inner, .. } = p.piece.kind {
            if bank_deg > 0.0 {
                let f = p.frame(0.5 * p.length);
                let hw = f.half_width;
                let (l, r) = (f.deck_point(hw), f.deck_point(-hw));
                let (top, out) = if l.y > r.y { (l, f.left) } else { (r, -f.left) };
                let style = format!("fill=\"{INK}\" font-size=\"19\" font-weight=\"600\" text-anchor=\"middle\"");
                sc.label(top + out * 9.0 + Vec3::Y * 2.0, Vec2::ZERO, format!("{} m", fr(top.y - p.entry.pos.y, 0)), style);
            }
        }
    }

    // The flight off a jump: the middle of the speeds the landing catches cleanly.
    if let [ramp, landing] = layout.pieces.as_slice() {
        if let Some(prof) = landing.landing() {
            // A flight that touches down half way along the part of the landing shaped to
            // catch it.
            let x_end = 0.5 * (prof.gap + prof.knee);
            let k = prof.c + prof.epsilon / x_end;
            let t = prof.lip_grade;
            let lip = ramp.exit.pos;
            let fwd = ramp.exit.heading.forward();
            let touch = landing.frame(x_end).horiz;
            let drift = Vec3::new(touch.x, 0.0, touch.z) - Vec3::new(lip.x, 0.0, lip.z) - fwd * x_end;
            let at = |x: f32| {
                let q = x / x_end;
                lip + fwd * x + drift * (q * q) + Vec3::Y * (t * x - k * x * x + 0.9)
            };
            let style = format!("stroke=\"{INK}\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-dasharray=\"2 7\" fill=\"none\"");
            let n = 40;
            for i in 0..n {
                let (a, b) = (at(x_end * i as f32 / n as f32), at(x_end * (i + 1) as f32 / n as f32));
                sc.line(cam.depth(0.5 * (a + b)) - 30.0, a, b, style.clone());
            }
        }
    }
    sc
}

/// The open end of a raised deck: its cross-section down to the ground.
fn end_cap(cam: &Camera, sc: &mut Scene, f: &track::kit::Frame) {
    let hw = f.half_width;
    let top: Vec<Vec3> = [hw, 0.5 * hw, 0.0, -0.5 * hw, -hw].iter().map(|&u| f.deck_point(u)).collect();
    if top.iter().all(|q| q.y < 0.05) {
        return;
    }
    let mut pts = top.clone();
    pts.extend(top.iter().rev().map(|q| Vec3::new(q.x, 0.0, q.z)));
    sc.solid(cam, pts, face(END), 0.0);
}

/// One piece: the deck in four strips, its slab sides, the curtain down to the ground, its
/// borders, centre line, entry arrow and gate.
fn piece(cam: &Camera, sc: &mut Scene, p: &Placed, route_s: f32) {
    let ss = p.samples();
    let n = ss.len();
    let frames: Vec<_> = ss.iter().map(|&s| p.frame(s)).collect();

    // Shadow on the ground: the deck's footprint.
    let mut outline = Vec::new();
    let mut back = Vec::new();
    for i in 0..n {
        let hw = frames[i.min(n - 2)].half_width;
        let (l, r) = (frames[i].deck_point(hw), frames[i].deck_point(-hw));
        outline.push(Vec3::new(l.x, 0.0, l.z));
        back.push(Vec3::new(r.x, 0.0, r.z));
    }
    back.reverse();
    outline.extend(back);
    sc.ground(Prim::Poly(outline), format!("fill=\"{SHADOW}\" fill-opacity=\"0.10\""));

    let edge = p.edge();
    for i in 0..n - 1 {
        let (fa, fb) = (&frames[i], &frames[i + 1]);
        let mid = p.frame(0.5 * (ss[i] + ss[i + 1]));
        let deck = mid.deck;
        let hw = mid.half_width;
        let us = [hw, 0.5 * hw, 0.0, -0.5 * hw, -hw];
        let pa: Vec<Vec3> = us.iter().map(|&u| fa.deck_point(u)).collect();
        let pb: Vec<Vec3> = us.iter().map(|&u| fb.deck_point(u)).collect();

        // Deck strips, shaded by their normal.
        let mut strip_depth = [0.0f32; 4];
        for j in 0..4 {
            let quad = vec![pa[j], pb[j], pb[j + 1], pa[j + 1]];
            let mut nrm = (quad[1] - quad[0]).cross(quad[3] - quad[0]).normalize_or_zero();
            if nrm.y < 0.0 {
                nrm = -nrm;
            }
            let k = 0.8 + 0.2 * nrm.dot(cam.light).max(0.0);
            let c = quad.iter().copied().sum::<Vec3>() / 4.0;
            let base = deck_colour(deck, c.y);
            strip_depth[j] = cam.depth(c);
            sc.solid(cam, quad, face(&rgb(base, k)), 0.0);
        }

        // Each edge: the slab's side, then a curtain to the ground with the stilts' marks.
        for (j, e) in [(0usize, 0usize), (4, 3)] {
            let (a, b) = (pa[j], pb[j]);
            let drop = |q: Vec3, d: f32| Vec3::new(q.x, (q.y - d).max(0.0), q.z);
            if a.y > 0.05 || b.y > 0.05 {
                let (a1, b1) = (drop(a, SLAB), drop(b, SLAB));
                sc.solid(cam, vec![a, b, b1, a1], face(SIDE), 0.0);
                if a1.y > 0.05 || b1.y > 0.05 {
                    let (a2, b2) = (drop(a1, 1e9), drop(b1, 1e9));
                    let curtain = vec![a1, b1, b2, a2];
                    let c = curtain.iter().copied().sum::<Vec3>() / 4.0;
                    let d = cam.depth(c);
                    sc.solid(cam, curtain, face(CURTAIN), 0.0);
                    // A mark where the route passes a multiple of 8 m.
                    let (ra, rb) = (route_s + ss[i], route_s + ss[i + 1]);
                    let mark = (ra / 8.0).ceil() * 8.0;
                    if mark < rb {
                        let q = a1 + (b1 - a1) * ((mark - ra) / (rb - ra));
                        sc.line(d - 0.01, q, drop(q, 1e9), format!("stroke=\"{STILT}\" stroke-opacity=\"0.6\" stroke-width=\"1.6\""));
                    }
                }
            }
            // The border along the edge, on top of its strip.
            let style = if deck == Surface::Dirt {
                Some(format!("stroke=\"{DIRT_EDGE}\" stroke-width=\"1.4\""))
            } else if mid.border > 0.5 {
                Some(match edge {
                    Edge::Sandbags => format!("stroke=\"{SANDBAGS}\" stroke-width=\"4.5\" stroke-linecap=\"round\""),
                    _ => format!("stroke=\"{BUMPERS}\" stroke-width=\"3.5\" stroke-linecap=\"round\""),
                })
            } else {
                Some(format!("stroke=\"{SIDE}\" stroke-width=\"1.4\""))
            };
            if let Some(style) = style {
                sc.line(strip_depth[e] - 0.02, a, b, style);
            }
        }

        let (ra, rb) = (route_s + ss[i], route_s + ss[i + 1]);
        // Ribs across the deck where the route passes a multiple of 8 m, strip by strip.
        let rib_colour = match (ICE.load(Ordering::Relaxed), deck == Surface::Dirt) {
            (true, true) => RIB_SNOW,
            (true, false) => RIB_ICE,
            (false, true) => RIB_DIRT,
            (false, false) => RIB_ROAD,
        };
        let rib = format!("stroke=\"{rib_colour}\" stroke-width=\"1.1\"");
        let mut mark = (ra / 8.0).ceil() * 8.0;
        while mark < rb {
            let t = (mark - ra) / (rb - ra);
            let q: Vec<Vec3> = (0..5).map(|j| pa[j] + (pb[j] - pa[j]) * t).collect();
            for j in 0..4 {
                sc.line(strip_depth[j] - 0.01, q[j], q[j + 1], rib.clone());
            }
            mark += 8.0;
        }

        // Centre dashes on the road, every other 4 m.
        if deck == Surface::Road {
            let mut s = (ra / 8.0).floor() * 8.0;
            while s < rb {
                let (d0, d1) = (s.max(ra), (s + 4.0).min(rb));
                if d1 > d0 {
                    let lerp = |d: f32| pa[2] + (pb[2] - pa[2]) * ((d - ra) / (rb - ra));
                    let depth = strip_depth[1].min(strip_depth[2]) - 0.02;
                    sc.line(depth, lerp(d0), lerp(d1), format!("stroke=\"{ORANGE}\" stroke-width=\"1.6\""));
                }
                s += 8.0;
            }
        }
    }

    // An arrow at the entry of the first piece of the tile.
    if route_s == 0.0 {
        let (s0, _) = p.deck_range();
        // Two chevrons, or one on a short piece (a hairpin).
        let count = if p.length < 40.0 { 1 } else { 2 };
        for k in 0..count {
            let s = s0 + 4.0 + 7.0 * k as f32;
            let (fa, fb) = (p.frame(s), p.frame(s + 5.0));
            let lift = |q: Vec3| q + Vec3::Y * 0.05;
            let tri = vec![lift(fa.deck_point(3.5)), lift(fb.deck_point(0.0)), lift(fa.deck_point(-3.5))];
            let fill = if fa.deck == Surface::Dirt { PAPER } else { ORANGE };
            sc.solid(cam, tri, format!("fill=\"{fill}\""), 3.0);
        }
    }

    // The gate: two posts and the beam, in the gate's colour.
    if let Some(gate) = p.piece.gate {
        let colour = match gate {
            Gate::Start => START,
            Gate::Checkpoint => CHECKPOINT,
            Gate::Finish => FINISH,
        };
        let f = p.frame(p.gate_s());
        let post_u = if f.deck == Surface::Dirt { GATE_POST_U_DIRT } else { GATE_POST_U_ROAD };
        let deck_y = f.centre().y;
        let at = |u: f32, y: f32| Vec3::new(f.horiz.x, y, f.horiz.z) + f.left * u;
        let post = format!("stroke=\"{colour}\" stroke-width=\"5\" stroke-linecap=\"round\"");
        for u in [post_u, -post_u] {
            let (a, b) = (at(u, 0.0), at(u, deck_y + GATE_BEAM.1));
            sc.line(cam.depth(0.5 * (a + b)), a, b, post.clone());
        }
        let beam = vec![at(post_u, deck_y + GATE_BEAM.1), at(-post_u, deck_y + GATE_BEAM.1), at(-post_u, deck_y + GATE_BEAM.0), at(post_u, deck_y + GATE_BEAM.0)];
        sc.solid(cam, beam, face(colour), 1.0);
    }
}

fn write_tile(path: &str, cam: &Camera, sc: &Scene, scale: f32, title: &str, spec: &str) {
    let (lo, hi) = sc.bounds(cam);
    let centre = 0.5 * (lo + hi);
    let area = Vec2::new(0.5 * W, TOP + 0.5 * (H - TOP - PAD));
    let to = |p: Vec3| (cam.plane(p) - centre) * scale + area;

    let mut order: Vec<&Item> = sc.items.iter().collect();
    // Ground in the order given, then the scene back to front, then labels.
    order.sort_by(|a, b| a.layer.cmp(&b.layer).then(if a.layer == 1 { b.depth.total_cmp(&a.depth) } else { core::cmp::Ordering::Equal }));

    let mut body = String::new();
    for it in order {
        match &it.prim {
            Prim::Poly(pts) => {
                let d: Vec<String> = pts.iter().map(|&p| {
                    let q = to(p);
                    format!("{:.1},{:.1}", q.x, q.y)
                }).collect();
                let _ = writeln!(body, "<polygon points=\"{}\" {}/>", d.join(" "), it.style);
            }
            Prim::Line(a, b) => {
                let (a, b) = (to(*a), to(*b));
                let _ = writeln!(body, "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {}/>", a.x, a.y, b.x, b.y, it.style);
            }
            Prim::Text(at, off, text) => {
                let q = to(*at) + *off;
                let _ = writeln!(body, "<text x=\"{:.1}\" y=\"{:.1}\" {}>{text}</text>", q.x, q.y, it.style);
            }
        }
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{W}\" height=\"{H}\" viewBox=\"0 0 {W} {H}\" \
         font-family=\"'Martian Mono', 'SF Mono', Menlo, ui-monospace, monospace\">\n\
         <rect width=\"{W}\" height=\"{H}\" fill=\"{PAPER}\"/>\n\
         <text x=\"{PAD}\" y=\"46\" font-size=\"27\" font-weight=\"600\" fill=\"{INK}\">{title}</text>\n\
         <text x=\"{PAD}\" y=\"76\" font-size=\"16\" fill=\"{MUTED}\" font-family=\"Saira, system-ui, -apple-system, Helvetica, sans-serif\">{spec}</text>\n\
         {body}</svg>\n"
    );
    std::fs::write(path, svg).unwrap_or_else(|e| panic!("{path}: {e}"));
}

/// The scale that fits a scene in the drawing area, pixels per metre.
fn fit(cam: &Camera, sc: &Scene) -> f32 {
    let (lo, hi) = sc.bounds(cam);
    let size = (hi - lo).max(Vec2::splat(1.0));
    ((W - 2.0 * PAD) / size.x).min((H - TOP - 2.0 * PAD) / size.y)
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/blocks").to_string());
    std::fs::create_dir_all(&out).expect("create the output directory");
    let cam = Camera::new();
    let mut count = 0;
    let ice_out = format!("{out}-ice");
    std::fs::create_dir_all(&ice_out).expect("create the ice output directory");
    let sheets = ROWS.iter().map(|r| (r, &out, false)).chain(ICE_ROWS.iter().map(|r| (r, &ice_out, true)));
    for (row, out, ice) in sheets {
        ICE.store(ice, Ordering::Relaxed);
        let built: Vec<(Layout, Scene)> = row.iter().map(|t| {
            let layout = build_layout(t);
            let sc = scene(&cam, &layout);
            (layout, sc)
        }).collect();
        // The row's scale fits its largest tile; a smaller one is drawn bigger, at most twice
        // as big, so the sizes of a family still compare.
        let scale = built.iter().map(|(_, sc)| fit(&cam, sc)).fold(f32::MAX, f32::min);
        for (t, (layout, sc)) in row.iter().zip(&built) {
            let path = format!("{out}/{}.svg", file_name(t));
            let spec = spec(layout);
            write_tile(&path, &cam, sc, fit(&cam, sc).min(2.0 * scale), &title(t), &spec);
            println!("{:<40} {spec}", file_name(t));
            count += 1;
        }
    }
    println!("wrote {count} shapes to {out} and {ice_out}");
}
