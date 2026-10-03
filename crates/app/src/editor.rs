//! The track editor (`mars-editor`): the draft seen from above on the grid, drawn with a brush.
//!
//! A stroke of the brush ("Route" or "Terre") is snapped to blocks as it is drawn
//! ([`track::sketch`]): starting on the draft it redraws it from there, elsewhere it begins a new
//! one. The "Sélection" tool paints over a stretch of the draft (or taps a block) and offers the
//! changes a stretch can take: its surface, its height, a hump, whoops, a jump, banked turns, a
//! checkpoint, or the draft cut off there. "Jouer" (or Enter) races it.
//!
//! Pinch (trackpad or two fingers) zooms, two fingers (or the trackpad's scroll, or the right
//! mouse button) move the view. The draft is saved as a map file after every change
//! (`tuning/draft.json`, on iOS the app's Documents folder, or `MARS_DRAFT=path`): copied into
//! `crates/track/maps/` and added to `BUILTIN_MAPS`, it becomes a circuit of the game.

use std::path::PathBuf;

use egui::epaint::{Mesh, PathStroke};
use egui::{Align2, Color32, Id, Order, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::Vec2 as W;
use track::Surface;
use track::kit::{CELL, Gate, Kind, LEVEL};
use track::sketch::{Deck, Draft, Edit};

use crate::menu::Layout;
use crate::menu::paint::{self, Font, Icon, col, fade};

/// What the editor asks of the app.
pub enum Request {
    /// Race this map.
    Play(track::Map),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Brush(Deck),
    Select,
}

/// One row of a block's deck, across it, in world (x, z).
struct Row {
    left: W,
    right: W,
    centre: W,
    forward: W,
    height: f32,
    dirt: bool,
}

/// A block of the draft as drawn.
struct Block {
    rows: Vec<Row>,
    boost: bool,
    /// A gate line across the deck.
    gate: Option<(Gate, W, W)>,
    /// Where its height label goes and what it says (slopes, ramps).
    label: Option<(W, String)>,
    top: f32,
}

/// The ground under the plan, the decks, lines and marks.
const GROUND: Color32 = Color32::from_rgb(38, 22, 17);
const ROAD_DECK: Color32 = Color32::from_rgb(70, 70, 78);
const DIRT_DECK: Color32 = Color32::from_rgb(168, 92, 52);
const EDGE_LINE: Color32 = Color32::from_rgb(236, 232, 226);
const BOOST: Color32 = Color32::from_rgb(255, 136, 30);
const START: Color32 = Color32::from_rgb(61, 220, 132);
const CHECKPOINT: Color32 = Color32::from_rgb(85, 200, 255);
const SELECTED: Color32 = Color32::from_rgb(255, 212, 63);
const CLASH: Color32 = Color32::from_rgb(255, 64, 48);

/// The changes a selection offers, as laid out in its panel: a short label for the phone's
/// narrow buttons, a long one for the computer's.
const OPTIONS: [(&str, &str, Edit); 12] = [
    ("Route", "Route", Edit::Surface(Deck::Road)),
    ("Terre", "Terre", Edit::Surface(Deck::Dirt)),
    ("Booster", "Booster", Edit::Surface(Deck::Booster)),
    ("Checkpoint", "Checkpoint", Edit::Checkpoint),
    ("Monter", "Monter", Edit::Height(1)),
    ("Descendre", "Descendre", Edit::Height(-1)),
    ("Bosse", "Bosse", Edit::Hump),
    ("Saut", "Saut", Edit::Jump),
    ("Vagues", "Petites bosses", Edit::Whoops),
    ("Relever", "Virages relevés", Edit::Bank(true)),
    ("À plat", "Virages plats", Edit::Bank(false)),
    ("Couper", "Couper ici", Edit::Cut),
];

pub struct Editor {
    pub draft: Draft,
    undo: Vec<Draft>,
    tool: Tool,
    /// The world point (x, z) at the middle of the screen, and points per metre.
    centre: W,
    scale: f32,
    framed: bool,
    /// The stroke being drawn, world points, and whether it is still on.
    stroke: Vec<W>,
    drawing: bool,
    /// The draft as the stroke being drawn would leave it, and the stroke length it was made at.
    preview: Option<Draft>,
    preview_at: usize,
    /// What the selection was painted with: it is found again on the draft after each change.
    selection_stroke: Vec<W>,
    selection: Option<(usize, usize)>,
    toast: Option<(String, f64)>,
    requests: Vec<Request>,
    /// The drawn blocks of what is shown, and what they were made from.
    blocks: Vec<Block>,
    clashes: Vec<usize>,
    shown: u64,
    generation: u64,
    save_path: Option<PathBuf>,
}

impl Editor {
    pub fn new() -> Self {
        let save_path = draft_path();
        // A saved draft the game cannot load is put aside (`draft-invalid.json`), never written
        // over by the next change.
        let mut toast = None;
        let draft = save_path.as_ref().and_then(|p| {
            let json = std::fs::read_to_string(p).ok()?;
            match track::Map::load(&json).and_then(|map| Draft::from_map(&map)) {
                Ok(d) => Some(d),
                Err(e) => {
                    let aside = p.with_file_name("draft-invalid.json");
                    let _ = std::fs::rename(p, &aside);
                    toast = Some((format!("Brouillon illisible ({e}), mis de côté dans {}", aside.display()), 0.0));
                    None
                }
            }
        });
        let draft = draft.unwrap_or_else(|| Draft::new("Brouillon"));
        let mut editor = Self {
            draft,
            undo: Vec::new(),
            tool: Tool::Brush(Deck::Road),
            centre: W::new(0.0, 4.0 * CELL),
            scale: 1.0,
            framed: false,
            stroke: Vec::new(),
            drawing: false,
            preview: None,
            preview_at: 0,
            selection_stroke: Vec::new(),
            selection: None,
            toast,
            requests: Vec::new(),
            blocks: Vec::new(),
            clashes: Vec::new(),
            shown: u64::MAX,
            generation: 0,
            save_path,
        };
        if std::env::var("MARS_EDITOR_DEMO").is_ok() {
            editor.demo();
        }
        editor
    }

    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Races the draft (Enter, or the "Jouer" button).
    pub fn play(&mut self, now: f64) {
        // Only a map the game loads is raced (building another would stop the game).
        match self.draft.to_map().map(|m| track::Map::load(&m.to_json())) {
            Some(Ok(map)) => self.requests.push(Request::Play(map)),
            Some(Err(e)) => self.say(&format!("Tracé impossible à jouer : {e}"), now),
            None => self.say("Dessine d'abord un tracé", now),
        }
    }

    pub fn undo(&mut self) {
        if let Some(d) = self.undo.pop() {
            self.draft = d;
            self.changed();
        }
    }

    pub fn deselect(&mut self) {
        self.selection = None;
        self.selection_stroke.clear();
    }

    fn say(&mut self, s: &str, now: f64) {
        self.toast = Some((s.into(), now));
    }

    /// A new draft to keep: the last one goes on the undo stack.
    fn commit(&mut self, draft: Draft) {
        self.undo.push(std::mem::replace(&mut self.draft, draft));
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.changed();
    }

    fn changed(&mut self) {
        self.generation += 1;
        self.selection = (!self.selection_stroke.is_empty()).then(|| self.draft.touched(&self.selection_stroke)).flatten();
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.save_path else { return };
        match self.draft.to_map() {
            Some(map) => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(path, map.to_json());
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn apply(&mut self, edit: Edit, now: f64) {
        let Some(range) = self.selection else {
            self.say("Sélectionne d'abord une partie du tracé", now);
            return;
        };
        let mut d = self.draft.clone();
        match d.edit(range, edit) {
            Ok(()) => {
                if edit == Edit::Cut {
                    self.deselect();
                }
                self.commit(d);
            }
            Err(e) => self.say(&e, now),
        }
    }

    // ------------------------------------------------------------------------------ the view

    fn to_screen(&self, rect: Rect, p: W) -> Pos2 {
        let c = rect.center();
        // Seen from above with north (+Z) up, +X (west) is on the left.
        pos2(c.x - (p.x - self.centre.x) * self.scale, c.y - (p.y - self.centre.y) * self.scale)
    }

    fn to_world(&self, rect: Rect, q: Pos2) -> W {
        let c = rect.center();
        W::new(self.centre.x - (q.x - c.x) / self.scale, self.centre.y - (q.y - c.y) / self.scale)
    }

    /// Zooms by `factor` about the screen point `at`.
    fn zoom(&mut self, rect: Rect, at: Pos2, factor: f32) {
        let w = self.to_world(rect, at);
        self.scale = (self.scale * factor).clamp(0.08, 6.0);
        let c = rect.center();
        self.centre = W::new(w.x + (at.x - c.x) / self.scale, w.y + (at.y - c.y) / self.scale);
    }

    /// Moves the view with a drag of `d` points (the plan follows the fingers).
    fn pan(&mut self, d: egui::Vec2) {
        self.centre += W::new(d.x, d.y) / self.scale;
    }

    /// Shows the whole draft (or a 16 × 12 cell field) in `canvas`, part of the screen `full`.
    fn frame_view(&mut self, full: Rect, canvas: Rect) {
        let placed = self.draft.layout().pieces;
        let (mut lo, mut hi) = (W::splat(f32::MAX), W::splat(f32::MIN));
        for p in &placed {
            for (x, z) in p.cells() {
                lo = lo.min(W::new(x as f32, z as f32) * CELL);
                hi = hi.max(W::new(x as f32 + 1.0, z as f32 + 1.0) * CELL);
            }
        }
        if placed.is_empty() {
            (lo, hi) = (W::new(-8.0, -2.0) * CELL, W::new(8.0, 10.0) * CELL);
        }
        let size = (hi - lo) + W::splat(2.0 * CELL);
        self.scale = (canvas.width() / size.x).min(canvas.height() / size.y).clamp(0.08, 6.0);
        let off = canvas.center() - full.center();
        self.centre = (lo + hi) * 0.5 + W::new(off.x, off.y) / self.scale;
    }

    // ---------------------------------------------------------------------------- the frame

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let full = ctx.viewport_rect();
        let safe = ctx.content_rect();
        let tall = matches!(Layout::for_size(full.width(), full.height()), Layout::Tall);
        // The canvas: the part of the screen the bars leave.
        let bar_h = if tall { 76.0 } else { 80.0 };
        let canvas = Rect::from_min_max(full.min, pos2(full.max.x, safe.max.y - bar_h));
        if !self.framed {
            self.frame_view(full, canvas);
            self.framed = true;
        }

        self.gestures(ui, full, canvas, now);
        let mut painter = ui.painter().clone();
        painter.set_clip_rect(full);
        painter.rect_filled(full, 0.0, GROUND);
        self.paint_grid(&painter, full);
        self.refresh();
        self.paint_blocks(&painter, full);
        if self.drawing && self.stroke.len() > 1 {
            let pts: Vec<Pos2> = self.stroke.iter().map(|&p| self.to_screen(full, p)).collect();
            painter.add(Shape::line(pts, PathStroke::new(3.0, fade(Color32::WHITE, 0.75))));
        }
        self.paint_info(&painter, safe, tall);
        self.bars(ui, full, safe, tall, now);
        if let Some((s, t)) = &self.toast {
            // Long enough to be read: longer messages stay longer.
            let life = 2.3 + s.chars().count() as f64 / 25.0;
            let age = now - t;
            if age > life + 0.5 {
                self.toast = None;
            } else {
                let a = (1.0 - ((age - life) / 0.5).clamp(0.0, 1.0)) as f32;
                let p = ctx.layer_painter(egui::LayerId::new(Order::Tooltip, Id::new("toast")));
                let font = Font::label(15.0, 0.04);
                let width = (safe.width() - 64.0).min(560.0);
                let size = paint::galley(&p, s, font, col::TEXT, Some(width), Some(20.0)).size();
                let r = Rect::from_center_size(pos2(safe.center().x, safe.min.y + 64.0 + size.y / 2.0), size + vec2(32.0, 20.0));
                paint::panel(&p, r, 10.0, fade(col::GLASS_DARK, a), Some((1.0, fade(col::LIVERY, a))));
                paint::para_centered(&p, pos2(r.center().x, r.min.y + 10.0), width, s, font, 20.0 / 15.0, fade(col::TEXT, a));
            }
        }
        if std::env::var("MARS_EDITOR_PLAY").ok().and_then(|s| s.parse::<f64>().ok()).is_some_and(|t| now >= t) && !self.requested_once() {
            self.play(now);
        }
    }

    /// The self-test's automatic start (`MARS_EDITOR_PLAY=seconds`) asks once.
    fn requested_once(&mut self) -> bool {
        static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        ASKED.swap(true, std::sync::atomic::Ordering::Relaxed)
    }

    /// Strokes, taps, pinches and pans on the canvas.
    fn gestures(&mut self, ui: &mut Ui, full: Rect, canvas: Rect, now: f64) {
        let resp = ui.interact(canvas, Id::new("editor-canvas"), Sense::click_and_drag());
        let (multi, zoom, scroll, hover, origin) =
            ui.input(|i| (i.multi_touch(), i.zoom_delta(), i.smooth_scroll_delta(), i.pointer.hover_pos(), i.pointer.press_origin()));
        // Two fingers: the view moves and zooms; a stroke begun with the first finger is dropped.
        if let Some(touch) = multi {
            if self.drawing {
                self.drawing = false;
                self.stroke.clear();
                self.preview = None;
            }
            self.pan(touch.translation_delta);
            if (zoom - 1.0).abs() > 1e-4 {
                self.zoom(full, touch.center_pos, zoom);
            }
            return;
        }
        if (zoom - 1.0).abs() > 1e-4 {
            let at = hover.filter(|h| canvas.contains(*h)).unwrap_or(canvas.center());
            self.zoom(full, at, zoom);
        } else if scroll != egui::Vec2::ZERO && hover.is_some_and(|h| canvas.contains(h)) {
            self.pan(scroll);
        }
        if resp.dragged_by(PointerButton::Secondary) || resp.dragged_by(PointerButton::Middle) {
            self.pan(resp.drag_delta());
        }
        if resp.drag_started_by(PointerButton::Primary) {
            self.drawing = true;
            self.stroke.clear();
            self.preview = None;
            self.preview_at = 0;
            if let Some(o) = origin {
                self.stroke.push(self.to_world(full, o));
            }
        }
        if self.drawing && resp.dragged_by(PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                let w = self.to_world(full, p);
                if self.stroke.last().is_none_or(|l| l.distance(w) > 1.5) {
                    self.stroke.push(w);
                }
            }
            // The blocks follow the stroke as it is drawn.
            if self.stroke.len() >= self.preview_at + 3 {
                self.preview_at = self.stroke.len();
                match self.tool {
                    Tool::Brush(deck) => {
                        let mut d = self.draft.clone();
                        self.preview = d.draw(&self.stroke, deck).then_some(d);
                    }
                    Tool::Select => {
                        self.selection = self.draft.touched(&self.stroke);
                    }
                }
            }
        }
        if self.drawing && resp.drag_stopped() {
            self.drawing = false;
            let stroke = std::mem::take(&mut self.stroke);
            self.preview = None;
            match self.tool {
                Tool::Brush(deck) => {
                    let mut d = self.draft.clone();
                    if d.draw(&stroke, deck) {
                        self.commit(d);
                    } else {
                        self.say("Trait trop court ou trop tordu", now);
                    }
                }
                Tool::Select => {
                    self.selection = self.draft.touched(&stroke);
                    self.selection_stroke = if self.selection.is_some() { stroke } else { Vec::new() };
                }
            }
        }
        if resp.clicked() && self.tool == Tool::Select {
            if let Some(p) = resp.interact_pointer_pos() {
                let w = self.to_world(full, p);
                let tap = vec![w, w + W::new(0.5, 0.0)];
                self.selection = self.draft.touched(&tap);
                self.selection_stroke = if self.selection.is_some() { tap } else { Vec::new() };
            }
        }
    }

    /// The drawn blocks of the draft shown (the preview while a stroke is drawn).
    fn refresh(&mut self) {
        let key = if self.preview.is_some() { self.generation.wrapping_mul(1_000_003).wrapping_add(self.preview_at as u64 + 1) } else { self.generation };
        if key == self.shown {
            return;
        }
        self.shown = key;
        let draft = self.preview.as_ref().unwrap_or(&self.draft);
        self.clashes = draft.overlaps();
        self.blocks = draft.layout().pieces.iter().map(block_geometry).collect();
    }

    fn paint_grid(&self, p: &egui::Painter, rect: Rect) {
        let cell = CELL * self.scale;
        let a = self.to_world(rect, rect.min);
        let b = self.to_world(rect, rect.max);
        let (lo, hi) = (a.min(b), a.max(b));
        for major in [false, true] {
            let every = if major { 4 } else { 1 };
            if cell * (every as f32) < 6.0 {
                continue;
            }
            let colour = if major { fade(Color32::WHITE, 0.09) } else { fade(Color32::WHITE, 0.035) };
            let stroke = Stroke::new(if major { 1.0 } else { 1.0 }, colour);
            let (x0, x1) = ((lo.x / CELL).floor() as i32, (hi.x / CELL).ceil() as i32);
            for i in x0..=x1 {
                if i.rem_euclid(every) == 0 && (major || i.rem_euclid(4) != 0) {
                    let x = self.to_screen(rect, W::new(i as f32 * CELL, 0.0)).x;
                    p.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], stroke);
                }
            }
            let (z0, z1) = ((lo.y / CELL).floor() as i32, (hi.y / CELL).ceil() as i32);
            for k in z0..=z1 {
                if k.rem_euclid(every) == 0 && (major || k.rem_euclid(4) != 0) {
                    let y = self.to_screen(rect, W::new(0.0, k as f32 * CELL)).y;
                    p.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], stroke);
                }
            }
        }
    }

    fn paint_blocks(&self, p: &egui::Painter, rect: Rect) {
        let s = |w: W| self.to_screen(rect, w);
        // Lower blocks first: a bridge is drawn over the road it crosses.
        let mut order: Vec<usize> = (0..self.blocks.len()).collect();
        order.sort_by(|&a, &b| self.blocks[a].top.total_cmp(&self.blocks[b].top));
        let selected = |i: usize| self.selection.is_some_and(|(a, b)| (a..=b).contains(&i)) && self.preview.is_none();
        for &i in &order {
            let block = &self.blocks[i];
            // Its shadow, cast down and to the right by its height.
            let mut shadow = Mesh::default();
            for w in block.rows.windows(2) {
                let lift = |r: &Row| vec2(1.0, 1.0) * (r.height * self.scale * 0.45).min(18.0);
                if w[0].height < 0.5 && w[1].height < 0.5 {
                    continue;
                }
                quad(&mut shadow, [s(w[0].left) + lift(&w[0]), s(w[0].right) + lift(&w[0]), s(w[1].right) + lift(&w[1]), s(w[1].left) + lift(&w[1])], fade(Color32::BLACK, 0.4));
            }
            p.add(Shape::mesh(shadow));
            // The deck, lighter the higher it is.
            let mut deck = Mesh::default();
            for w in block.rows.windows(2) {
                let colour = |r: &Row| {
                    let base = if r.dirt { DIRT_DECK } else { ROAD_DECK };
                    paint::lerp_colour(base, Color32::WHITE, (r.height / 48.0).clamp(0.0, 1.0) * 0.35)
                };
                let (c0, c1) = (colour(&w[0]), colour(&w[1]));
                let base = deck.vertices.len() as u32;
                for (pos, c) in [(s(w[0].left), c0), (s(w[0].right), c0), (s(w[1].right), c1), (s(w[1].left), c1)] {
                    deck.colored_vertex(pos, c);
                }
                deck.add_triangle(base, base + 1, base + 2);
                deck.add_triangle(base, base + 2, base + 3);
            }
            p.add(Shape::mesh(deck));
            // Edge lines on the road.
            let road_rows: Vec<&Row> = block.rows.iter().filter(|r| !r.dirt).collect();
            if road_rows.len() > 1 && self.scale > 0.25 {
                for side in [true, false] {
                    let pts: Vec<Pos2> = road_rows.iter().map(|r| s(if side { r.left } else { r.right })).collect();
                    p.add(Shape::line(pts, PathStroke::new(1.0, fade(EDGE_LINE, 0.55))));
                }
            }
            // Arrows: the way the track goes, and the boosters.
            let every = if block.boost { 3 } else { 8 };
            for (k, r) in block.rows.iter().enumerate() {
                if k % every != every / 2 || self.scale < 0.2 {
                    continue;
                }
                let l = W::new(r.forward.y, -r.forward.x) * -1.0;
                let size = if block.boost { 6.0 } else { 3.5 };
                let pts = [s(r.centre - r.forward * size + l * size * 1.2), s(r.centre + r.forward * size * 0.6), s(r.centre - r.forward * size - l * size * 1.2)];
                let colour = if block.boost { BOOST } else { fade(Color32::WHITE, 0.25) };
                p.add(Shape::line(pts.to_vec(), PathStroke::new(if block.boost { 2.5 } else { 1.5 }, colour)));
            }
            if selected(i) || self.clashes.contains(&i) {
                let colour = if self.clashes.contains(&i) { CLASH } else { SELECTED };
                let mut over = Mesh::default();
                for w in block.rows.windows(2) {
                    quad(&mut over, [s(w[0].left), s(w[0].right), s(w[1].right), s(w[1].left)], fade(colour, 0.3));
                }
                p.add(Shape::mesh(over));
                for side in [true, false] {
                    let pts: Vec<Pos2> = block.rows.iter().map(|r| s(if side { r.left } else { r.right })).collect();
                    p.add(Shape::line(pts, PathStroke::new(2.0, colour)));
                }
            }
            if let Some((gate, a, b)) = block.gate {
                let (a, b) = (s(a), s(b));
                match gate {
                    Gate::Finish => {
                        // A chequered line.
                        let n = 8;
                        let across = b - a;
                        let normal = vec2(-across.y, across.x).normalized() * 3.0;
                        for k in 0..n {
                            let t0 = k as f32 / n as f32;
                            let t1 = (k + 1) as f32 / n as f32;
                            for (row, off) in [(0, -normal), (1, normal * 0.0)] {
                                let c = if (k + row) % 2 == 0 { Color32::WHITE } else { Color32::BLACK };
                                let q0 = a + across * t0 + off;
                                let q1 = a + across * t1 + off;
                                let mut m = Mesh::default();
                                quad(&mut m, [q0, q1, q1 + normal, q0 + normal], c);
                                p.add(Shape::mesh(m));
                            }
                        }
                    }
                    g => {
                        let colour = if g == Gate::Start { START } else { CHECKPOINT };
                        p.line_segment([a, b], Stroke::new(4.0, colour));
                    }
                }
            }
        }
        // Heights on the slopes, once the plan is close enough to read them.
        if self.scale > 0.45 {
            for block in &self.blocks {
                if let Some((at, s_)) = &block.label {
                    paint::text(p, s(*at), Align2::CENTER_CENTER, s_, Font::data(10.0), fade(col::TEXT, 0.9));
                }
            }
        }
    }

    fn paint_info(&self, p: &egui::Painter, safe: Rect, tall: bool) {
        let x = safe.min.x + 18.0;
        let y = safe.min.y + 16.0;
        paint::text(p, pos2(x, y), Align2::LEFT_TOP, "ÉDITEUR", Font::label(13.0, 0.18), col::LIVERY);
        let layout = self.draft.layout();
        let info = if self.draft.is_empty() {
            "Dessine un tracé avec le pinceau".to_string()
        } else {
            let m = layout.length();
            format!("{} blocs · {:.1} km · ~{:.0} s", self.draft.pieces.len(), m / 1000.0, m / (200.0 / 3.6))
        };
        paint::text(p, pos2(x, y + 20.0), Align2::LEFT_TOP, &info, Font::body(14.0), col::DUST_2);
        let mut line = y + 40.0;
        // What the selection holds: what its options can work with.
        if let Some(range) = self.selection.filter(|_| self.tool == Tool::Select && self.preview.is_none()) {
            let s = format!("Sélection : {}", self.draft.describe(range));
            paint::text(p, pos2(x, line), Align2::LEFT_TOP, &s, Font::body(13.0), SELECTED);
            line += 19.0;
        }
        if !self.clashes.is_empty() {
            paint::text(p, pos2(x, line), Align2::LEFT_TOP, "Le tracé se croise : surélève une partie", Font::body(13.0), CLASH);
        }
        if !tall {
            let hints = "Entrée : jouer   ·   Pincer : zoom   ·   Deux doigts : déplacer   ·   ⌘Z : annuler";
            paint::text(p, pos2(safe.max.x - 18.0, y + 2.0), Align2::RIGHT_TOP, hints, Font::body(12.5), col::DUST_3);
        }
    }

    /// The tool bar along the bottom and, over it, the selection's options.
    /// Drawn on a layer of their own over the plan; their buttons, registered after the canvas,
    /// take the pointer before it.
    fn bars(&mut self, ui: &mut Ui, full: Rect, safe: Rect, tall: bool, now: f64) {
        let ctx = ui.ctx().clone();
        let bar_h = if tall { 76.0 } else { 80.0 };
        let bar = Rect::from_min_max(pos2(full.min.x, safe.max.y - bar_h), full.max);
        {
            let mut p = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("editor-bar")));
            p.set_clip_rect(full);
            let _ = ui.interact(bar, Id::new("editor-bar-back"), Sense::click_and_drag());
            // Behind the home indicator too.
            p.rect_filled(bar, 0.0, col::GLASS_DARK);
            p.line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0, col::LINE));
            let tools: [(&str, Option<Tool>); 5] = [
                ("Route", Some(Tool::Brush(Deck::Road))),
                ("Terre", Some(Tool::Brush(Deck::Dirt))),
                ("Sélection", Some(Tool::Select)),
                ("Annuler", None),
                ("Jouer", None),
            ];
            let gap = 8.0;
            let w = if tall { (safe.width() - 24.0 - gap * 4.0) / 5.0 } else { 120.0 };
            let total = w * 5.0 + gap * 4.0;
            let x0 = safe.center().x - total / 2.0;
            let top = bar.min.y + 10.0;
            for (k, (label, tool)) in tools.iter().enumerate() {
                let r = Rect::from_min_size(pos2(x0 + k as f32 * (w + gap), top), vec2(w, 56.0));
                let active = tool.is_some_and(|t| t == self.tool);
                let play = k == 4;
                let resp = ui.interact(r, Id::new(("tool", k)), Sense::click());
                let fill = if play {
                    col::EASY
                } else if active {
                    col::LIVERY
                } else if resp.hovered() {
                    col::PANEL_2
                } else {
                    col::PANEL
                };
                let ink = if play || active { col::LIVERY_INK } else { col::TEXT };
                paint::panel(&p, r, 10.0, fill, (!play && !active).then_some((1.0, col::LINE)));
                let icon = pos2(r.center().x, r.min.y + 20.0);
                match k {
                    0 | 1 => {
                        let c = if k == 0 { ROAD_DECK } else { DIRT_DECK };
                        let sw = Rect::from_center_size(icon, vec2(30.0, 11.0));
                        p.rect_filled(sw, 3.0, c);
                        if k == 0 {
                            p.line_segment([sw.left_top() + vec2(2.0, 2.0), sw.right_top() + vec2(-2.0, 2.0)], Stroke::new(1.0, EDGE_LINE));
                            p.line_segment([sw.left_bottom() + vec2(2.0, -2.0), sw.right_bottom() + vec2(-2.0, -2.0)], Stroke::new(1.0, EDGE_LINE));
                        }
                    }
                    2 => {
                        let sw = Rect::from_center_size(icon, vec2(26.0, 16.0));
                        p.rect_stroke(sw, 3.0, Stroke::new(1.6, ink), StrokeKind::Inside);
                    }
                    3 => paint::icon_at(&p, icon, 20.0, Icon::Restart, ink),
                    _ => {
                        let t = [icon + vec2(-6.0, -8.0), icon + vec2(9.0, 0.0), icon + vec2(-6.0, 8.0)];
                        p.add(Shape::convex_polygon(t.to_vec(), ink, Stroke::NONE));
                    }
                }
                let font = Font::label(if tall { 12.0 } else { 13.0 }, 0.06);
                paint::text(&p, pos2(r.center().x, r.max.y - 13.0), Align2::CENTER_CENTER, &label.to_uppercase(), font, ink);
                if resp.clicked() {
                    match (k, tool) {
                        (_, Some(t)) => {
                            self.tool = *t;
                            if *t != Tool::Select {
                                self.deselect();
                            }
                        }
                        (3, None) => self.undo(),
                        _ => self.play(now),
                    }
                }
            }
        }
        // The selection's options, over the bar.
        if self.tool == Tool::Select && self.selection.is_some() && !self.drawing {
            let gap = 8.0;
            {
                let (cols, bw) = if tall { (4, (safe.width() - 24.0 - gap * 3.0) / 4.0) } else { (6, 128.0) };
                let rows = OPTIONS.len().div_ceil(cols);
                let bh = 56.0;
                let pw = cols as f32 * bw + (cols - 1) as f32 * gap + 24.0;
                let ph = rows as f32 * bh + (rows - 1) as f32 * gap + 24.0;
                let panel = Rect::from_min_size(pos2(safe.center().x - pw / 2.0, bar.min.y - ph - 10.0), vec2(pw, ph));
                let clicked = {
                let mut p = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("editor-options")));
                p.set_clip_rect(full);
                let _ = ui.interact(panel, Id::new("editor-options-back"), Sense::click_and_drag());
                paint::panel(&p, panel, 14.0, col::GLASS_DARK, Some((1.0, col::LINE)));
                let mut clicked = None;
                for (n, (short, long, edit)) in OPTIONS.iter().enumerate() {
                    let label = if tall { short } else { long };
                    let (row, colm) = (n / cols, n % cols);
                    let r = Rect::from_min_size(panel.min + vec2(12.0 + colm as f32 * (bw + gap), 12.0 + row as f32 * (bh + gap)), vec2(bw, bh));
                    let resp = ui.interact(r, Id::new(("option", n)), Sense::click());
                    let fill = if resp.hovered() { col::PANEL_2 } else { col::PANEL };
                    paint::panel(&p, r, 9.0, fill, Some((1.0, col::LINE)));
                    let swatch = match edit {
                        Edit::Surface(Deck::Road) => Some(ROAD_DECK),
                        Edit::Surface(Deck::Dirt) => Some(DIRT_DECK),
                        Edit::Surface(Deck::Booster) => Some(BOOST),
                        Edit::Checkpoint => Some(CHECKPOINT),
                        Edit::Cut => Some(CLASH),
                        _ => None,
                    };
                    if let Some(c) = swatch {
                        p.rect_filled(Rect::from_min_size(r.min + vec2(8.0, r.height() / 2.0 - 3.0), vec2(6.0, 6.0)), 2.0, c);
                    }
                    paint::text(&p, r.center() + vec2(if swatch.is_some() { 5.0 } else { 0.0 }, 0.0), Align2::CENTER_CENTER, label, Font::label(if tall { 13.0 } else { 14.0 }, 0.02), col::TEXT);
                    if resp.clicked() {
                        clicked = Some(*edit);
                    }
                }
                clicked
                };
                if let Some(edit) = clicked {
                    self.apply(edit, now);
                }
            }
        }
    }

    /// The self-test's draft (`MARS_EDITOR_DEMO=1`): a loop drawn in two strokes, then a raised
    /// stretch with a jump, a stretch of dirt and a checkpoint, the dirt left selected.
    fn demo(&mut self) {
        let c = |x: f32, z: f32| W::new((x + 0.5) * CELL, (z + 0.5) * CELL);
        let arc = |centre: W, r: f32, a0: f32, a1: f32| -> Vec<W> {
            (0..=24).map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 24.0;
                centre + W::new(a.cos(), a.sin()) * r
            })
            .collect()
        };
        let pi = std::f32::consts::PI;
        let mut d = Draft::new("Brouillon");
        // North along x = 0, a wide right turn, east, a hairpin right, back south.
        let mut s1 = vec![c(0.0, 0.0), c(0.0, 9.0)];
        s1.extend(arc(c(-2.5, 9.5), 80.0, 0.0, 0.5 * pi));
        s1.extend([c(-9.0, 12.0)]);
        d.draw(&s1, Deck::Road);
        let end = d.layout().pieces.last().map(|p| W::new(p.entry.pos.x, p.entry.pos.z)).unwrap();
        let s2 = vec![end, c(-10.0, 12.0), c(-12.0, 12.0), c(-12.0, 6.0), c(-12.0, 2.0)];
        d.draw(&s2, Deck::Road);
        // A raised stretch with a jump on the way north.
        if let Some(r) = d.touched(&[c(0.0, 1.0), c(0.0, 9.0)]) {
            let _ = d.edit(r, Edit::Jump);
        }
        let dirt = vec![c(-12.0, 9.0), c(-12.0, 4.0)];
        if let Some(r) = d.touched(&dirt) {
            let _ = d.edit(r, Edit::Surface(Deck::Dirt));
        }
        if let Some(r) = d.touched(&[c(-6.0, 12.0), c(-7.0, 12.0)]) {
            let _ = d.edit(r, Edit::Checkpoint);
        }
        self.draft = d;
        self.tool = Tool::Select;
        self.selection_stroke = dirt;
        self.save_path = None;
        self.changed();
    }
}

/// A quadrilateral of one colour.
fn quad(m: &mut Mesh, q: [Pos2; 4], c: Color32) {
    let base = m.vertices.len() as u32;
    for pos in q {
        m.colored_vertex(pos, c);
    }
    m.add_triangle(base, base + 1, base + 2);
    m.add_triangle(base, base + 2, base + 3);
}

fn block_geometry(p: &track::kit::Placed) -> Block {
    let (s0, s1) = p.deck_range();
    let n = ((s1 - s0) / 3.0).ceil().max(2.0) as usize;
    let rows: Vec<Row> = (0..=n)
        .map(|k| {
            let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
            let c = W::new(f.horiz.x, f.horiz.z);
            let l = W::new(f.left.x, f.left.z);
            Row {
                left: c + l * f.half_width,
                right: c - l * f.half_width,
                centre: c,
                forward: W::new(f.forward.x, f.forward.z),
                height: f.centre().y,
                dirt: f.deck == Surface::Dirt,
            }
        })
        .collect();
    let gate = p.piece.gate.map(|g| {
        let f = p.frame(p.gate_s());
        let c = W::new(f.horiz.x, f.horiz.z);
        let l = W::new(f.left.x, f.left.z) * (f.half_width + 1.0);
        (g, c + l, c - l)
    });
    let climbs = (p.exit.pos.y - p.entry.pos.y).abs() > 0.5 || matches!(p.piece.kind, Kind::JumpRamp { .. });
    let label = climbs.then(|| {
        let mid = &rows[rows.len() / 2];
        let text = match p.piece.kind {
            Kind::JumpRamp { .. } => "SAUT".to_string(),
            _ => format!("{} m", (p.exit.pos.y / LEVEL).round() as i32 * LEVEL as i32),
        };
        (mid.centre, text)
    });
    let top = rows.iter().map(|r| r.height).fold(0.0, f32::max);
    Block { rows, boost: p.piece.boost, gate, label, top }
}

/// Where the draft is kept: `MARS_DRAFT`, the app's Documents folder on iOS, `tuning/draft.json`
/// in the source tree elsewhere.
fn draft_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MARS_DRAFT") {
        return Some(PathBuf::from(p));
    }
    if cfg!(target_os = "ios") {
        return std::env::var("HOME").ok().map(|h| PathBuf::from(h).join("Documents/draft.json"));
    }
    Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tuning/draft.json")))
}
