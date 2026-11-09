//! The track editor (`mars-editor`): the draft seen from above on the grid, drawn with a brush.
//!
//! A stroke of the brush ("Route" or "Terre") is snapped to blocks as it is drawn
//! ([`track::sketch`]): starting on the draft it redraws it from there, elsewhere it begins a new
//! one. The "Sélection" tool paints over a stretch of the draft (or taps a block) and offers the
//! changes a stretch can take: its surface, its height, a hump, whoops, a jump, banked turns, a
//! checkpoint, or the draft cut off there; the heights along the route show over the plan while a
//! stretch is selected. "Jouer" (or Enter) races it.
//!
//! The plan is drawn by [`crate::editor_view`]: decks take their height's colour as they rise.
//! Leaning the phone to the side glances at the relief: the view tilts in perspective, heights
//! stretched, and lays flat again as the phone comes back; the round "3D" button (or T) keeps it
//! tilted, to draw in 3D (the only way on a computer). Pinch (trackpad or two fingers) zooms, two fingers (or the
//! trackpad's scroll, or the right mouse button) move the view, and in 3D turning two fingers
//! (or the trackpad's rotation) turns it. Tilted, a stroke is drawn on the deck it starts on (or
//! the ground), and a selection is picked on what the screen shows. The draft is saved as a map
//! file after every change (`tuning/draft.json`, on iOS the app's Documents folder, or
//! `MARS_DRAFT=path`): copied into `crates/track/maps/` and added to `BUILTIN_MAPS`, it becomes
//! a circuit of the game.

use std::path::PathBuf;

use egui::{Align2, Color32, Id, Order, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::{Vec2 as W, Vec3};
use track::kit::CELL;
use track::sketch::{Deck, Draft, Edit};

use crate::editor_view::{self as view, BOOST, CHECKPOINT, CLASH, Camera, SELECTED};
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

/// The tool bar's swatches: the road, the dirt.
const ROAD_SWATCH: Color32 = Color32::from_rgb(70, 70, 78);
const DIRT_SWATCH: Color32 = Color32::from_rgb(168, 92, 52);
const EDGE_LINE: Color32 = Color32::from_rgb(236, 232, 226);

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
    cam: Camera,
    /// The 3D view asked for (the camera eases toward it).
    tilted: bool,
    /// The phone's lean to the side (radians) and the attitude it is measured from, which
    /// follows slowly how the phone is held; and how far the view glances in 3D (0..1), to which
    /// side.
    lean: Option<f32>,
    lean_neutral: Option<f32>,
    glance: f32,
    glance_side: f32,
    /// When the last frame was drawn (egui's clock, seconds).
    last_time: f64,
    framed: bool,
    /// The stroke being drawn (world points on the plane at `stroke_height`), and whether it is
    /// still on.
    stroke: Vec<W>,
    stroke_height: f32,
    drawing: bool,
    /// Where a selection stroke last was on the screen.
    last_pick: Option<Pos2>,
    /// The draft as the stroke being drawn would leave it, and the stroke length it was made at.
    preview: Option<Draft>,
    preview_at: usize,
    /// Points on the selected blocks: the selection is found again on the draft after each change.
    selection_stroke: Vec<W>,
    selection: Option<(usize, usize)>,
    toast: Option<(String, f64)>,
    requests: Vec<Request>,
    /// The drawn blocks of what is shown, and what they were made from.
    blocks: Vec<view::Block>,
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
            cam: Camera::new(),
            tilted: std::env::var("MARS_EDITOR_3D").is_ok(),
            lean: None,
            lean_neutral: None,
            glance: 0.0,
            glance_side: 1.0,
            last_time: 0.0,
            framed: false,
            stroke: Vec::new(),
            stroke_height: 0.0,
            drawing: false,
            last_pick: None,
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
        if editor.tilted {
            (editor.cam.tilt, editor.cam.relief, editor.cam.yaw) = (view::TILT_3D, view::RELIEF_3D, 0.35);
        }
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

    /// Tilts the view to see the relief, or lays it flat again (the "3D" button, or T).
    pub fn toggle_3d(&mut self) {
        self.tilted = !self.tilted;
    }

    /// The phone's lean to the side this frame (radians, positive to the right; `None` without
    /// motion sensors).
    pub fn sense(&mut self, roll: Option<f32>, now: f64) {
        // Said once, the first time the phone's lean is known.
        if self.lean.is_none() && roll.is_some() && self.toast.is_none() {
            self.say("Penche le téléphone sur le côté pour voir le relief en 3D", now.max(1.5));
        }
        self.lean = roll;
    }

    /// The glance in 3D: leaning the phone to the side tilts the view and turns it a little that
    /// way, laid flat again as the phone comes back. Nothing moves while a finger is on the
    /// screen (a stroke stays under it), and a lean held still fades as the attitude it is
    /// measured from catches up with the phone.
    fn follow_lean(&mut self, dt: f32, touching: bool) {
        let Some(roll) = self.lean else {
            self.glance = 0.0;
            return;
        };
        let neutral = self.lean_neutral.get_or_insert(roll);
        if touching {
            return;
        }
        // How the phone is held is followed within a few seconds; a deliberate lean, slowly.
        let lean = roll - *neutral;
        let follow = if lean.abs() < 0.1 { 3.0 } else { 25.0 };
        *neutral += lean * (1.0 - (-dt / follow).exp());
        // Fully tilted from 6° to 15°, well before iOS would turn the screen to landscape.
        let t = ((lean.abs() - 0.1) / 0.16).clamp(0.0, 1.0);
        let goal = t * t * (3.0 - 2.0 * t);
        self.glance += (goal - self.glance) * (1.0 - (-dt * 8.0).exp());
        if goal > 0.0 {
            self.glance_side = lean.signum();
        }
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

    /// Shows the whole draft (or a 16 × 12 cell field) in `canvas`.
    fn frame_view(&mut self, canvas: Rect) {
        let mut points: Vec<Vec3> = self.blocks.iter().flat_map(|b| &b.rows).flat_map(|r| [r.left, r.right]).collect();
        if points.is_empty() {
            points = vec![Vec3::new(-8.0 * CELL, 0.0, -2.0 * CELL), Vec3::new(8.0 * CELL, 0.0, 10.0 * CELL)];
        }
        // Clear of the text along the top.
        let area = Rect::from_min_max(canvas.min + vec2(0.0, 70.0), canvas.max);
        self.cam.fit(&points, area);
    }

    /// Eases the camera toward the view asked for: tilted with its relief (locked by the button,
    /// or as far as the phone leans), or flat with north up.
    fn ease(&mut self, dt: f32) {
        let amount = if self.tilted { 1.0 } else { self.glance };
        let (tilt, relief) = (view::TILT_3D * amount, view::RELIEF_3D * amount);
        let k = 1.0 - (-dt * 9.0).exp();
        let c = &mut self.cam;
        let at = c.origin;
        let goal_yaw = if self.tilted {
            c.yaw
        } else {
            // Glancing, the view turns a little toward the side the phone leans to.
            (c.yaw / std::f32::consts::TAU).round() * std::f32::consts::TAU + self.glance_side * self.glance * 0.5
        };
        if (c.tilt - tilt).abs() + (c.relief - relief).abs() + (c.yaw - goal_yaw).abs() < 1e-3 {
            (c.tilt, c.relief, c.yaw) = (tilt, relief, goal_yaw);
            return;
        }
        // About the middle of the screen, which stays where it is.
        let before = c.unproject(at, 0.0);
        c.tilt += (tilt - c.tilt) * k;
        c.relief += (relief - c.relief) * k;
        c.yaw += (goal_yaw - c.yaw) * k;
        let after = c.unproject(at, 0.0);
        c.target += before - after;
    }

    // ---------------------------------------------------------------------------- the frame

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // The time since the last frame, from the clock (frames come faster than 60 a second on a
        // 120 Hz screen, and in self-tests).
        let now = ctx.input(|i| i.time);
        let dt = (now - self.last_time).clamp(0.0, 0.1) as f32;
        self.last_time = now;
        let full = ctx.viewport_rect();
        let safe = ctx.content_rect();
        let tall = matches!(Layout::for_size(full.width(), full.height()), Layout::Tall);
        // The canvas: the part of the screen the bars leave.
        let bar_h = if tall { 76.0 } else { 80.0 };
        let canvas = Rect::from_min_max(full.min, pos2(full.max.x, safe.max.y - bar_h));
        self.refresh();
        if !self.framed {
            self.frame_view(canvas);
            self.framed = true;
        }
        // Kept at the middle of the canvas as the window changes.
        let shift = canvas.center() - self.cam.origin;
        if shift.length() > 0.5 {
            self.cam.origin = canvas.center();
            self.cam.pan(-shift);
        }
        // `MARS_EDITOR_ROLL=degrees`: a phone leaning that much from a second on (self-tests).
        if let Some(r) = std::env::var("MARS_EDITOR_ROLL").ok().and_then(|s| s.parse::<f32>().ok()) {
            self.lean = Some(if now > 1.0 { r.to_radians() } else { 0.0 });
        }
        let touching = ctx.input(|i| i.any_touches() || i.pointer.any_down());
        self.follow_lean(dt, touching);
        self.ease(dt);

        self.gestures(ui, canvas, now);
        self.refresh();
        let mut painter = ui.painter().clone();
        painter.set_clip_rect(full);
        let marks = view::Marks { selection: self.selection.filter(|_| self.preview.is_none()), clashes: &self.clashes };
        view::paint(&painter, &self.cam, &self.blocks, marks, full);
        if self.drawing && matches!(self.tool, Tool::Brush(_)) {
            view::paint_stroke(&painter, &self.cam, &self.stroke, self.stroke_height);
        }
        let button = Rect::from_min_size(pos2(safe.max.x - 16.0 - 56.0, safe.min.y + 12.0), vec2(56.0, 56.0));
        let below = self.paint_info(&painter, safe, tall, button);
        self.paint_profile(&ctx, safe, tall, below);
        self.relief_button(ui, &ctx, full, button);
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

    /// Strokes, taps, pinches, turns and pans on the canvas.
    fn gestures(&mut self, ui: &mut Ui, canvas: Rect, now: f64) {
        let resp = ui.interact(canvas, Id::new("editor-canvas"), Sense::click_and_drag());
        let (multi, zoom, turn, scroll, hover, origin) = ui.input(|i| {
            (i.multi_touch(), i.zoom_delta(), i.rotation_delta(), i.smooth_scroll_delta(), i.pointer.hover_pos(), i.pointer.press_origin())
        });
        // Two fingers: the view moves, zooms and (tilted) turns; a stroke begun with the first
        // finger is dropped.
        if let Some(touch) = multi {
            if self.drawing {
                self.drawing = false;
                self.stroke.clear();
                self.preview = None;
            }
            self.cam.pan(touch.translation_delta);
            if (zoom - 1.0).abs() > 1e-4 {
                self.cam.zoom(touch.center_pos, zoom);
            }
            if self.tilted && touch.rotation_delta.abs() > 1e-5 {
                self.cam.rotate(touch.center_pos, -touch.rotation_delta);
            }
            return;
        }
        let at = hover.filter(|h| canvas.contains(*h)).unwrap_or(canvas.center());
        if (zoom - 1.0).abs() > 1e-4 {
            self.cam.zoom(at, zoom);
        } else if scroll != egui::Vec2::ZERO && hover.is_some_and(|h| canvas.contains(h)) {
            self.cam.pan(scroll);
        }
        if self.tilted && turn.abs() > 1e-5 {
            self.cam.rotate(at, -turn);
        }
        if resp.dragged_by(PointerButton::Secondary) || resp.dragged_by(PointerButton::Middle) {
            self.cam.pan(resp.drag_delta());
        }
        if resp.drag_started_by(PointerButton::Primary) {
            self.drawing = true;
            self.stroke.clear();
            self.preview = None;
            self.preview_at = 0;
            let o = origin.or(resp.interact_pointer_pos()).unwrap_or(canvas.center());
            match self.tool {
                Tool::Brush(_) => {
                    // Drawn on the deck it starts on (a raised road too), or else on the ground.
                    self.stroke_height = view::pick(&self.cam, &self.blocks, o).map_or(0.0, |(_, p)| p.y);
                    self.stroke.push(self.cam.unproject(o, self.stroke_height));
                }
                Tool::Select => {
                    self.selection = None;
                    self.selection_stroke.clear();
                    self.last_pick = None;
                    self.pick_along(o);
                }
            }
        }
        if self.drawing && resp.dragged_by(PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                match self.tool {
                    Tool::Brush(deck) => {
                        let w = self.cam.unproject(p, self.stroke_height);
                        if self.stroke.last().is_none_or(|l| l.distance(w) > 1.5) {
                            self.stroke.push(w);
                        }
                        // The blocks follow the stroke as it is drawn.
                        if self.stroke.len() >= self.preview_at + 3 {
                            self.preview_at = self.stroke.len();
                            let mut d = self.draft.clone();
                            self.preview = d.draw(&self.stroke, deck).then_some(d);
                        }
                    }
                    Tool::Select => self.pick_along(p),
                }
            }
        }
        if self.drawing && resp.drag_stopped() {
            self.drawing = false;
            let stroke = std::mem::take(&mut self.stroke);
            self.preview = None;
            if let Tool::Brush(deck) = self.tool {
                let mut d = self.draft.clone();
                if d.draw(&stroke, deck) {
                    self.commit(d);
                } else {
                    self.say("Trait trop court ou trop tordu", now);
                }
            }
        }
        if resp.clicked() && self.tool == Tool::Select {
            if let Some(p) = resp.interact_pointer_pos() {
                self.selection = None;
                self.selection_stroke.clear();
                self.last_pick = None;
                self.pick_along(p);
            }
        }
    }

    /// Adds to the selection the blocks shown under the pointer since its last position.
    fn pick_along(&mut self, p: Pos2) {
        let from = self.last_pick.unwrap_or(p);
        let steps = (from.distance(p) / 8.0).ceil().max(1.0) as usize;
        for k in 1..=steps {
            let q = from.lerp(p, k as f32 / steps as f32);
            if let Some((i, at)) = view::pick(&self.cam, &self.blocks, q) {
                self.selection = Some(self.selection.map_or((i, i), |(a, b)| (a.min(i), b.max(i))));
                self.selection_stroke.push(W::new(at.x, at.z));
            }
        }
        self.last_pick = Some(p);
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
        self.blocks = view::blocks(&draft.layout().pieces);
    }

    /// The editor's name, the draft's figures, the selection, a warning, the heights' legend.
    /// Returns where the text ends.
    fn paint_info(&self, p: &egui::Painter, safe: Rect, tall: bool, button: Rect) -> f32 {
        let x = safe.min.x + 18.0;
        let y = safe.min.y + 16.0;
        // On a phone the lines stop short of the round button.
        let width = if tall { button.min.x - x - 10.0 } else { 520.0 };
        paint::text(p, pos2(x, y), Align2::LEFT_TOP, "ÉDITEUR", Font::label(13.0, 0.18), col::LIVERY);
        let layout = self.draft.layout();
        let info = if self.draft.is_empty() {
            "Dessine un tracé avec le pinceau".to_string()
        } else {
            let m = layout.length();
            format!("{} blocs · {:.1} km · ~{:.0} s", self.draft.pieces.len(), m / 1000.0, m / (200.0 / 3.6))
        };
        let mut line = y + 20.0;
        line = paint::para(p, pos2(x, line), width, &info, Font::body(14.0), 1.3, col::DUST_2).max.y + 2.0;
        // What the selection holds: what its options can work with.
        if let Some(range) = self.selection.filter(|_| self.tool == Tool::Select && self.preview.is_none()) {
            let s = format!("Sélection : {}", self.draft.describe(range));
            line = paint::para(p, pos2(x, line), width, &s, Font::body(13.0), 1.3, SELECTED).max.y + 2.0;
        }
        if !self.clashes.is_empty() {
            line = paint::para(p, pos2(x, line), width, "Le tracé se croise : surélève une partie", Font::body(13.0), 1.3, CLASH).max.y + 2.0;
        }
        // The heights' colours, once the draft leaves the ground.
        let top = self.blocks.iter().flat_map(|b| &b.rows).map(|r| r.centre.y).fold(0.0, f32::max);
        if top > 0.5 {
            view::paint_legend(p, pos2(x + 14.0, line + 6.0), ((top - 1.0) / 8.0).ceil().max(1.0) * 8.0);
            line += 20.0;
        }
        // Under the selection's options there is no room for them.
        if !tall && !(self.tool == Tool::Select && self.selection.is_some()) {
            let hints = if self.tilted {
                "Entrée : jouer   ·   Pincer : zoom   ·   Deux doigts : déplacer   ·   Tourner deux doigts : pivoter   ·   ⌘Z : annuler"
            } else {
                "Entrée : jouer   ·   Pincer : zoom   ·   Deux doigts : déplacer   ·   3D ou T : relief   ·   ⌘Z : annuler"
            };
            paint::text(p, pos2(x, safe.max.y - 80.0 - 14.0), Align2::LEFT_BOTTOM, hints, Font::body(12.5), col::DUST_3);
        }
        line
    }

    /// The heights along the route, while a stretch is selected: under the text on a phone, at
    /// the top in the middle on a computer.
    fn paint_profile(&self, ctx: &egui::Context, safe: Rect, tall: bool, below: f32) {
        if self.tool != Tool::Select || self.selection.is_none() || self.drawing || self.blocks.is_empty() {
            return;
        }
        let rect = if tall {
            Rect::from_min_size(pos2(safe.min.x + 12.0, below + 8.0), vec2(safe.width() - 24.0, 84.0))
        } else {
            Rect::from_center_size(pos2(safe.center().x, safe.min.y + 14.0 + 48.0), vec2(560.0, 96.0))
        };
        let mut p = ctx.layer_painter(egui::LayerId::new(Order::Middle, Id::new("editor-profile")));
        p.set_clip_rect(ctx.viewport_rect());
        view::paint_profile(&p, rect, &self.blocks, self.selection);
    }

    /// The round button that tilts the view or lays it flat.
    fn relief_button(&mut self, ui: &mut Ui, ctx: &egui::Context, full: Rect, r: Rect) {
        let resp = ui.interact(r, Id::new("editor-3d"), Sense::click());
        let mut p = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("editor-3d-button")));
        p.set_clip_rect(full);
        let fill = if self.tilted {
            col::LIVERY
        } else if resp.hovered() {
            col::PANEL_2
        } else {
            col::GLASS_DARK
        };
        p.circle_filled(r.center(), r.width() / 2.0, fill);
        if !self.tilted {
            p.circle_stroke(r.center(), r.width() / 2.0 - 0.5, Stroke::new(1.0, col::LINE));
        }
        let ink = if self.tilted { col::LIVERY_INK } else { col::TEXT };
        paint::text(&p, r.center(), Align2::CENTER_CENTER, if self.tilted { "2D" } else { "3D" }, Font::display(19.0), ink);
        if resp.clicked() {
            self.toggle_3d();
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
                        let c = if k == 0 { ROAD_SWATCH } else { DIRT_SWATCH };
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
            let (cols, bw) = if tall { (4, (safe.width() - 24.0 - gap * 3.0) / 4.0) } else { (6, 128.0) };
            let rows = OPTIONS.len().div_ceil(cols);
            let bh = 56.0;
            let pw = cols as f32 * bw + (cols - 1) as f32 * gap + 24.0;
            let ph = rows as f32 * bh + (rows - 1) as f32 * gap + 24.0;
            let panel = Rect::from_min_size(pos2(safe.center().x - pw / 2.0, bar.min.y - ph - 10.0), vec2(pw, ph));
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
                    Edit::Surface(Deck::Road) => Some(ROAD_SWATCH),
                    Edit::Surface(Deck::Dirt) => Some(DIRT_SWATCH),
                    Edit::Surface(Deck::Booster) => Some(BOOST),
                    Edit::Checkpoint => Some(CHECKPOINT),
                    Edit::Cut => Some(CLASH),
                    _ => None,
                };
                if let Some(c) = swatch {
                    p.rect_filled(Rect::from_min_size(r.min + vec2(8.0, r.height() / 2.0 - 3.0), vec2(6.0, 6.0)), 2.0, c);
                }
                let font = Font::label(if tall { 13.0 } else { 14.0 }, 0.02);
                paint::text(&p, r.center() + vec2(if swatch.is_some() { 5.0 } else { 0.0 }, 0.0), Align2::CENTER_CENTER, label, font, col::TEXT);
                if resp.clicked() {
                    clicked = Some(*edit);
                }
            }
            if let Some(edit) = clicked {
                self.apply(edit, now);
            }
        }
    }

    /// The self-test's draft (`MARS_EDITOR_DEMO=1`): a loop drawn in two strokes, then a raised
    /// stretch with a jump, a stretch of dirt and a checkpoint, the dirt left selected.
    fn demo(&mut self) {
        let c = |x: f32, z: f32| W::new((x + 0.5) * CELL, (z + 0.5) * CELL);
        let arc = |centre: W, r: f32, a0: f32, a1: f32| -> Vec<W> {
            (0..=24)
                .map(|i| {
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
