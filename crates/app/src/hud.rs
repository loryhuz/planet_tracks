//! The race HUD, light like Trackmania's: the map at the top left, the record and the time to
//! beat at the top right, the chrono at the bottom with the gap to the record a little above it at
//! each checkpoint (blue ahead, red behind, grey equal), and the speed inside a ring that fills
//! with the revs in the gear's colour. The countdown and the finish card keep the menu's richer
//! style. Two layouts as in the menu: wide (1280 × 720 design space) and tall for phones
//! (390 × 844), where the buggy accelerates by itself: the bottom strip brakes, holding the left or
//! right half of the screen steers (the two halves blink at the start to show it), the speed is a
//! thin gauge on the right edge, and a settings button pauses the race (camera, sound, last
//! checkpoint, restart, menu). On a computer Escape (or the pad's Start) opens the same sheet, and
//! the arrows move through it. The wide layout takes the same touch controls once the screen has
//! been touched. Debug (FPS, profile, tuning) only shows with Tab.

use std::collections::BTreeMap;

use egui::epaint::{CornerRadius, Mesh, PathStroke, Vertex};
use egui::{Align2, Color32, Event, Id, LayerId, Order, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TouchPhase, Ui, pos2, vec2};
use physics::Input;

use crate::game::Game;
use crate::input::Nav;
use crate::menu::Layout;
use crate::menu::catalog::{MEDALS, TrackInfo};
use crate::menu::paint::{self, Font, Icon, MEDAL_COLOURS, col, fade};
use crate::race::{COUNTDOWN_TICKS, format_delta, format_time};
use crate::ui::Fps;
use crate::ui_sound::Cue;

/// Dark glass (the settings button, the finish card's keys, the debug chip) and its edge.
const GLASS: Color32 = Color32::from_rgba_premultiplied(6, 4, 5, 128);
const GLASS_LINE: Color32 = Color32::from_rgba_premultiplied(39, 37, 35, 41);
/// The empty part of the speed ring and gauge.
const RING_TRACK: Color32 = GLASS_LINE;
/// Slower than the record, and equal to it.
const SLOW: Color32 = Color32::from_rgb(255, 90, 95);
const EQUAL: Color32 = Color32::from_rgb(201, 188, 178);
const SLOW_SOFT: Color32 = Color32::from_rgba_premultiplied(41, 14, 15, 41);
const HUB_SOFT: Color32 = Color32::from_rgba_premultiplied(14, 35, 41, 41);
/// The speed ring's colour in each gear, from white to red.
const GEAR_COLOURS: [Color32; 6] = [
    Color32::from_rgb(239, 227, 214),
    Color32::from_rgb(158, 209, 242),
    Color32::from_rgb(85, 220, 255),
    Color32::from_rgb(243, 195, 79),
    Color32::from_rgb(255, 106, 31),
    Color32::from_rgb(255, 90, 95),
];
/// Darker top and bottom of the scene, so the text reads over bright sand.
const VIG_TOP: Color32 = Color32::from_rgba_premultiplied(2, 2, 2, 97);
const VIG_BOTTOM: Color32 = Color32::from_rgba_premultiplied(3, 2, 2, 107);
/// The touch zones' dark fill (the brake strip's foot, the start's steering tutorial).
const ZONE_DARK: Color32 = Color32::from_rgba_premultiplied(5, 3, 4, 107);
const ZONE_LIGHT: Color32 = Color32::from_rgba_premultiplied(1, 1, 1, 26);
/// Height of the brake strip, and where the steering halves start from the top.
const BRAKE_H: f32 = 108.0;
const STEER_TOP: f32 = 120.0;
/// How long a checkpoint gap stays, and the start's steering tutorial lasts, seconds.
const SPLIT_TIME: f32 = 2.6;
const TUTORIAL_TIME: f32 = 2.0;

/// What the HUD asks of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudRequest {
    Respawn,
    Restart,
    Menu,
}

/// A checkpoint just crossed: its index (1-based), time, gap to the record.
struct Split {
    index: usize,
    /// Checkpoints on the circuit.
    total: usize,
    tick: u32,
    delta: Option<i64>,
    at: f64,
}

pub struct Hud {
    tracks: Vec<TrackInfo>,
    seen_splits: usize,
    split: Option<Split>,
    /// The countdown's number on screen (3, 2, 1, 0 for GO) and since when.
    count: Option<(u32, f64)>,
    go_at: Option<f64>,
    finish_at: Option<f64>,
    /// When the run started, for the steering tutorial.
    start_at: Option<f64>,
    last_tick: u32,
    last_countdown: u32,
    /// Fingers on the screen, by touch id.
    touches: BTreeMap<u64, Pos2>,
    touch_seen: bool,
    /// The settings sheet, open since then; the race waits while it is.
    sheet: Option<f64>,
    /// Escape or Start: the sheet opens on the next frame.
    open_requested: bool,
    /// Keyboard and pad: the sheet's item with the focus (`SHEET_*`), none until a key is used.
    focus: Option<usize>,
    navs: Vec<Nav>,
    /// Self-test: open the sheet that far into the race (`MARS_HUD_SETTINGS=seconds`).
    sheet_test: Option<f64>,
    cues: Vec<Cue>,
    requests: Vec<HudRequest>,
}

/// The sheet's items for the keyboard and pad: the camera, the four rows, then "Reprendre".
const SHEET_CAMERA: usize = 0;
const SHEET_RESUME: usize = 5;

/// A vertical gradient band.
fn band(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    gradient(p, rect, [top, top, bottom, bottom]);
}

/// A rectangle with a colour per corner: top left, top right, bottom right, bottom left.
fn gradient(p: &Painter, rect: Rect, colours: [Color32; 4]) {
    let mut mesh = Mesh::default();
    for (pos, c) in [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()].into_iter().zip(colours) {
        mesh.vertices.push(Vertex { pos, uv: egui::epaint::WHITE_UV, color: c });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(Shape::mesh(mesh));
}

/// Text laid straight on the scene, with a soft shadow so it reads over bright sand.
fn shadowed(p: &Painter, pos: Pos2, anchor: Align2, s: &str, font: Font, colour: Color32, a: f32) -> Rect {
    for d in [vec2(0.0, 2.0), vec2(1.2, 1.2), vec2(-1.2, 1.2), vec2(0.0, -0.6)] {
        paint::text(p, pos + d, anchor, s, font, fade(Color32::BLACK, 0.2 * a));
    }
    paint::text(p, pos, anchor, s, font, fade(colour, a))
}

/// The small uppercase labels (map tag, times, checkpoint).
fn tag_font() -> Font {
    Font::label(11.0, 0.18).weight(700.0)
}

fn delta_text(delta: i64) -> String {
    if delta == 0 { "0.00".into() } else { format_delta(delta).replace('-', "−") }
}

fn delta_colour(delta: i64) -> Color32 {
    match delta {
        d if d < 0 => col::HUB,
        0 => EQUAL,
        _ => SLOW,
    }
}

fn delta_pill(p: &Painter, center: Pos2, delta: i64, size: f32) -> Rect {
    let (c, soft) = if delta <= 0 { (col::HUB, HUB_SOFT) } else { (SLOW, SLOW_SOFT) };
    let s = format_delta(delta).replace('-', "−");
    let font = Font::data(size).weight(600.0);
    let ts = paint::text_size(p, &s, font);
    let rect = Rect::from_center_size(center, vec2(ts.x + size * 1.5, size * 1.9));
    p.rect_filled(rect, size * 0.55, soft);
    p.rect_stroke(rect, size * 0.55, Stroke::new(1.0, fade(c, 0.5)), StrokeKind::Inside);
    paint::text(p, rect.center(), Align2::CENTER_CENTER, &s, font, c);
    rect
}

/// An arc of a circle, clockwise from `from` (radians, 0 pointing right), with round ends when
/// `caps`.
#[allow(clippy::too_many_arguments)]
fn arc(p: &Painter, c: Pos2, r: f32, from: f32, sweep: f32, width: f32, colour: Color32, caps: bool) {
    let n = ((sweep.to_degrees() / 4.0).ceil() as usize).max(2);
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = from + sweep * i as f32 / n as f32;
            c + r * vec2(a.cos(), a.sin())
        })
        .collect();
    if caps {
        p.circle_filled(pts[0], width / 2.0, colour);
        p.circle_filled(pts[n], width / 2.0, colour);
    }
    p.add(Shape::line(pts, PathStroke::new(width, colour)));
}

/// A cog, for the settings button.
fn gear_icon(p: &Painter, c: Pos2, size: f32, colour: Color32) {
    let s = size / 24.0;
    let stroke = Stroke::new((2.0 * s).max(1.2), colour);
    let mut pts = Vec::new();
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        for (da, r) in [(-0.2, 9.6), (0.2, 9.6), (0.42, 6.9), (std::f32::consts::TAU / 8.0 - 0.42, 6.9)] {
            let t = a + da;
            pts.push(c + s * r * vec2(t.cos(), t.sin()));
        }
    }
    p.add(Shape::closed_line(pts, stroke));
    p.circle_stroke(c, 3.0 * s, stroke);
}

/// Opacity of the start's steering tutorial `t` seconds in: two blinks, then gone.
fn tutorial_alpha(t: f32) -> f32 {
    const KEYS: [(f32, f32); 7] = [(0.0, 0.0), (0.1, 1.0), (0.28, 1.0), (0.4, 0.0), (0.52, 1.0), (0.72, 1.0), (1.0, 0.0)];
    let u = t / TUTORIAL_TIME;
    if !(0.0..1.0).contains(&u) {
        return 0.0;
    }
    let i = KEYS.iter().position(|k| k.0 > u).unwrap_or(KEYS.len() - 1);
    let (a, b) = (KEYS[i - 1], KEYS[i]);
    a.1 + (b.1 - a.1) * (u - a.0) / (b.0 - a.0)
}

impl Hud {
    pub fn new(maps: &[track::Map]) -> Self {
        Self {
            tracks: crate::menu::catalog::tracks(maps),
            seen_splits: 0,
            split: None,
            count: None,
            go_at: None,
            finish_at: None,
            start_at: None,
            last_tick: 0,
            last_countdown: 0,
            touches: BTreeMap::new(),
            touch_seen: false,
            sheet: None,
            open_requested: false,
            focus: None,
            navs: Vec::new(),
            sheet_test: std::env::var("MARS_HUD_SETTINGS").ok().and_then(|v| v.parse().ok()),
            cues: Vec::new(),
            requests: Vec::new(),
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn take_requests(&mut self) -> Vec<HudRequest> {
        std::mem::take(&mut self.requests)
    }

    /// The settings sheet is open: the race waits.
    pub fn paused(&self) -> bool {
        self.sheet.is_some() || self.open_requested
    }

    /// Escape or the pad's Start in a race: the sheet opens with the focus on "Reprendre".
    pub fn open_sheet(&mut self) {
        self.open_requested = true;
    }

    /// Escape or Start again: the race goes on.
    pub fn close_sheet(&mut self) {
        self.open_requested = false;
        if self.sheet.take().is_some() {
            self.cues.push(Cue::SheetClose);
        }
    }

    /// A keyboard or pad move in the open sheet.
    pub fn push_nav(&mut self, nav: Nav) {
        self.navs.push(nav);
    }

    /// Countdown steps, checkpoints and the finish, from the run's state.
    fn events(&mut self, game: &Game, now: f64) {
        let run = &game.run;
        // A restart (or a new map) starts the HUD over.
        if run.countdown > self.last_countdown || run.tick < self.last_tick {
            self.seen_splits = 0;
            self.split = None;
            self.count = None;
            self.go_at = None;
            self.finish_at = None;
            self.start_at = Some(now);
            self.sheet = None;
        }
        self.last_countdown = run.countdown;
        self.last_tick = run.tick;
        let shown = if run.countdown > 0 {
            Some((1 + run.countdown * 3 / COUNTDOWN_TICKS).min(3))
        } else if run.finished.is_none() && run.tick < 60 && self.count.is_some() {
            Some(0)
        } else {
            None
        };
        match shown {
            Some(n) if self.count.map(|c| c.0) != Some(n) => {
                self.count = Some((n, now));
                if n == 0 {
                    self.go_at = Some(now);
                    self.cues.push(Cue::CountGo);
                } else {
                    self.cues.push(Cue::CountBeep);
                }
            }
            None => self.count = None,
            _ => {}
        }
        if run.splits.len() > self.seen_splits {
            let n = run.splits.len();
            let tick = run.splits[n - 1];
            let best = game.session.profile().current_best(&game.map_key()).and_then(|b| b.splits.get(n - 1).copied());
            let delta = best.map(|b| tick as i64 - b as i64);
            self.cues.push(match delta {
                Some(d) if d > 0 => Cue::SplitSlower,
                Some(0) => Cue::SplitEqual,
                _ => Cue::SplitFaster,
            });
            self.split = Some(Split { index: n, total: game.track.checkpoints.len(), tick, delta, at: now });
        }
        self.seen_splits = run.splits.len();
        if run.finished.is_some() && self.finish_at.is_none() {
            self.finish_at = Some(now);
            self.cues.push(if game.result.as_ref().is_some_and(|r| r.record) { Cue::Record } else { Cue::Finish });
        }
    }

    /// Follows the fingers on the screen.
    fn note_touches(&mut self, ui: &Ui) {
        ui.input(|i| {
            for e in &i.events {
                if let Event::Touch { id, phase, pos, .. } = e {
                    self.touch_seen = true;
                    match phase {
                        TouchPhase::Start | TouchPhase::Move => {
                            self.touches.insert(id.0, *pos);
                        }
                        TouchPhase::End | TouchPhase::Cancel => {
                            self.touches.remove(&id.0);
                        }
                    }
                }
            }
        });
    }

    /// Fingers on the screen, and the mouse held (to try the touch controls on a computer).
    fn touch_points(&self, ui: &Ui) -> Vec<Pos2> {
        let mut pts: Vec<Pos2> = self.touches.values().copied().collect();
        if let Some(pos) = ui.input(|i| i.pointer.primary_down().then(|| i.pointer.latest_pos()).flatten()) {
            pts.push(pos);
        }
        pts
    }

    /// Draws the HUD for this frame and sets the touch controls' driving input.
    pub fn ui(&mut self, ui: &mut Ui, game: &mut Game, fps: &Fps, muted: bool) {
        let now = ui.input(|i| i.time);
        let r = ui.ctx().content_rect();
        let wide = Layout::for_size(r.width(), r.height()) == Layout::Wide;
        let p = ui.painter().clone();
        self.events(game, now);
        if std::mem::take(&mut self.open_requested) && self.sheet.is_none() {
            self.sheet = Some(now);
            self.focus = Some(SHEET_RESUME);
            self.cues.push(Cue::SheetOpen);
        }
        if self.sheet.is_none() {
            self.navs.clear();
        }
        self.note_touches(ui);
        let touch = !wide || self.touch_seen;
        if self.sheet_test.is_some_and(|t| game.run.countdown == 0 && game.run.tick as f64 >= t * 100.0) {
            self.sheet_test = None;
            self.sheet = Some(now);
        }

        band(&p, Rect::from_min_max(r.min, pos2(r.right(), r.top() + r.height() * 0.18)), VIG_TOP, Color32::TRANSPARENT);
        band(&p, Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.78), r.max), Color32::TRANSPARENT, VIG_BOTTOM);

        let finished = game.run.finished.is_some();
        let finish_k = self.finish_at.map_or(0.0, |t| (((now - t) as f32 - 0.5) / 0.35).clamp(0.0, 1.0));
        let a = 1.0 - finish_k;

        // Touch: the buggy accelerates by itself, the bottom strip brakes, the halves steer.
        let controls = touch && !finished;
        game.controls.auto_gas = controls;
        game.controls.touch = if controls { self.touch_controls(ui, &p, r, now) } else { Input::default() };

        if wide {
            // Above the brake strip when the touch controls are on.
            let lift = if touch { 96.0 } else { 0.0 };
            self.map_label(&p, pos2(r.left() + 30.0, r.top() + 24.0), game, a, 28.0);
            self.times(&p, pos2(r.right() - if touch { 88.0 } else { 30.0 }, r.top() + 24.0), game, a, true);
            self.split_line(&p, pos2(r.center().x, r.bottom() - 94.0 - lift), now, a, 22.0);
            self.chrono(&p, pos2(r.center().x, r.bottom() - 30.0 - lift), game, a, 46.0);
            let ring = Rect::from_min_size(pos2(r.right() - 30.0 - 132.0, r.bottom() - 24.0 - lift - 132.0), vec2(132.0, 132.0));
            self.speed_ring(&p, ring, game, now, a);
        } else {
            self.map_label(&p, pos2(r.left() + 18.0, r.top() + 20.0), game, a, 22.0);
            self.times(&p, pos2(r.left() + 18.0, r.top() + 80.0), game, a, false);
            self.split_line(&p, pos2(r.center().x, r.bottom() - 174.0), now, a, 17.0);
            self.chrono(&p, pos2(r.center().x, r.bottom() - 126.0), game, a, 34.0);
            self.speed_gauge(&p, pos2(r.right() - 14.0, r.top() + 226.0), game, now, a);
        }
        if controls && self.sheet.is_none() {
            self.settings_button(ui, &p, r, a, now);
        }
        if game.panel_open {
            let at = if wide { pos2(r.left() + 30.0, r.top() + 96.0) } else { pos2(r.left() + 18.0, r.top() + 144.0) };
            self.debug_chip(&p, at, game, fps);
        }
        self.countdown(&p, r, wide, now);
        if finished && finish_k > 0.0 {
            self.finish_card(ui, &p, r, wide, game, now, finish_k);
        }
        if let Some(since) = self.sheet {
            self.settings_sheet(ui, r, wide, game, muted, now, since);
        }
    }

    // ---------------------------------------------------------------- race screen

    fn track(&self, game: &Game) -> Option<(usize, &TrackInfo)> {
        self.tracks.iter().enumerate().find(|(_, t)| t.map == game.map_index)
    }

    /// The easiest medal not yet won with this record: (index in MEDALS, target ticks).
    fn objective(&self, game: &Game) -> Option<(usize, u32)> {
        let (_, t) = self.track(game)?;
        let best = game.session.profile().best(&game.map_key()).map(|b| b.ticks);
        let targets = t.medal_ticks();
        (0..3).rev().map(|k| (k, targets[k])).find(|(_, m)| best.is_none_or(|b| b > *m))
    }

    /// The series and slot, then the map's name.
    fn map_label(&self, p: &Painter, at: Pos2, game: &Game, a: f32, size: f32) {
        let slot = self.track(game).map_or(1, |(i, _)| i + 1);
        let y = at.y + 7.0;
        let flag = Rect::from_center_size(pos2(at.x + 5.0, y), vec2(10.0, 10.0));
        p.rect_filled(flag.translate(vec2(0.0, 1.5)), 2.0, fade(Color32::BLACK, 0.3 * a));
        p.rect_filled(flag, 2.0, fade(col::EASY, a));
        shadowed(p, pos2(flag.right() + 7.0, y), Align2::LEFT_CENTER, &format!("FACILE · {slot:02}"), tag_font(), col::DUST_2, a);
        let name = game.map_name().to_uppercase();
        shadowed(p, pos2(at.x - 1.0, at.y + 16.0), Align2::LEFT_TOP, &name, Font::heading(size).weight(800.0), col::DUST, a);
    }

    /// The record and the time to beat (the easiest medal not won yet): right-aligned rows in the
    /// wide layout from the top right corner `at`, a table from the top left corner in the tall one.
    fn times(&self, p: &Painter, at: Pos2, game: &Game, a: f32, wide: bool) {
        let best = game.session.profile().best(&game.map_key()).map(|b| b.ticks);
        let mut rows: Vec<(&str, Option<Color32>, u32)> = Vec::new();
        if let Some(b) = best {
            rows.push(("RECORD", None, b));
        }
        if let Some((k, ticks)) = self.objective(game) {
            rows.push(("À BATTRE", Some(MEDAL_COLOURS[k]), ticks));
        }
        let label = tag_font();
        let value = Font::data(if wide { 18.0 } else { 14.0 }).weight(600.0);
        let step = if wide { 28.0 } else { 24.0 };
        let dot = |c: Pos2, colour: Color32| {
            p.circle_filled(c + vec2(0.0, 1.0), 4.5, fade(Color32::BLACK, 0.3 * a));
            p.circle_filled(c, 4.5, fade(colour, a));
        };
        if wide {
            for (i, (name, medal, ticks)) in rows.iter().enumerate() {
                let y = at.y + 11.0 + i as f32 * step;
                let v = shadowed(p, pos2(at.x, y), Align2::RIGHT_CENTER, &format_time(*ticks), value, col::DUST, a);
                let mut x = v.left() - 10.0;
                if let Some(c) = medal {
                    dot(pos2(x - 4.5, y), *c);
                    x -= 19.0;
                }
                shadowed(p, pos2(x, y + 1.0), Align2::RIGHT_CENTER, name, label, col::DUST_2, a);
            }
        } else {
            // Labels, medal dots and times in three columns.
            let label_w = rows.iter().map(|(n, _, _)| paint::text_size(p, n, label).x).fold(0.0, f32::max);
            let value_w = rows.iter().map(|(_, _, t)| paint::text_size(p, &format_time(*t), value).x).fold(0.0, f32::max);
            let dot_x = at.x + label_w + 10.0 + 4.5;
            let value_right = dot_x + 4.5 + 10.0 + value_w;
            for (i, (name, medal, ticks)) in rows.iter().enumerate() {
                let y = at.y + 9.0 + i as f32 * step;
                shadowed(p, pos2(at.x, y + 1.0), Align2::LEFT_CENTER, name, label, col::DUST_2, a);
                if let Some(c) = medal {
                    dot(pos2(dot_x, y), *c);
                }
                shadowed(p, pos2(value_right, y), Align2::RIGHT_CENTER, &format_time(*ticks), value, col::DUST, a);
            }
        }
    }

    /// The race time, its bottom centre at `bottom_center`.
    fn chrono(&self, p: &Painter, bottom_center: Pos2, game: &Game, a: f32, size: f32) {
        let run = &game.run;
        let ticks = run.finished.unwrap_or(run.tick);
        shadowed(p, bottom_center, Align2::CENTER_BOTTOM, &format_time(ticks), Font::data(size).weight(600.0), col::DUST, a);
    }

    /// The last checkpoint, for a moment above the chrono: its number and the gap to the record
    /// (its time when there is no record yet).
    fn split_line(&self, p: &Painter, bottom_center: Pos2, now: f64, a: f32, size: f32) {
        let Some(s) = &self.split else { return };
        let u = (now - s.at) as f32 / SPLIT_TIME;
        let k = if u < 0.06 {
            u / 0.06
        } else if u < 0.84 {
            1.0
        } else {
            ((1.0 - u) / 0.16).max(0.0)
        };
        if k <= 0.0 {
            return;
        }
        let a = a * k;
        let label = format!("CP {} / {}", s.index, s.total);
        let (text, colour) = match s.delta {
            Some(d) => (delta_text(d), delta_colour(d)),
            None => (format_time(s.tick), col::DUST),
        };
        let df = Font::data(size).weight(600.0);
        let lf = tag_font();
        let (dw, lw) = (paint::text_size(p, &text, df).x, paint::text_size(p, &label, lf).x);
        let left = bottom_center.x - (lw + 10.0 + dw) / 2.0;
        let d = shadowed(p, pos2(left + lw + 10.0, bottom_center.y), Align2::LEFT_BOTTOM, &text, df, colour, a);
        // On the gap's baseline.
        let y = d.center().y + (size - 11.0) * 0.3;
        shadowed(p, pos2(left, y), Align2::LEFT_CENTER, &label, lf, col::DUST_2, a);
    }

    /// The ring's fill (the revs, starting over in each gear) and colour (the gear's, white at
    /// the redline).
    fn revs(game: &Game, now: f64) -> (f32, Color32) {
        let s = game.sound_frame();
        let fill = if s.gear == 0 { s.rpm.max(0.12 * s.load) } else { 0.3 + 0.7 * s.rpm };
        let flash = fill > 0.96 && (now / 0.07) as i64 % 2 == 0;
        let colour = if flash { Color32::WHITE } else { GEAR_COLOURS[(s.gear as usize).min(GEAR_COLOURS.len() - 1)] };
        (fill.clamp(0.0, 1.0), colour)
    }

    /// Wide layout: the speed inside a three-quarter ring.
    fn speed_ring(&self, p: &Painter, rect: Rect, game: &Game, now: f64, a: f32) {
        let k = rect.width() / 100.0;
        let (c, radius, width) = (rect.center(), 44.0 * k, 8.0 * k);
        let from = 135f32.to_radians();
        let sweep = 270f32.to_radians();
        arc(p, c, radius, from, sweep, width, fade(RING_TRACK, a), false);
        let (fill, colour) = Self::revs(game, now);
        if fill > 0.01 {
            arc(p, c, radius, from, sweep * fill, width, fade(colour, a), true);
        }
        let speed = format!("{:.0}", game.telemetry().speed_kmh.abs());
        shadowed(p, c, Align2::CENTER_CENTER, &speed, Font::data(40.0).weight(600.0), col::DUST, a);
    }

    /// Tall layout: the speed on top of a thin vertical gauge along the right edge.
    fn speed_gauge(&self, p: &Painter, top_right: Pos2, game: &Game, now: f64, a: f32) {
        let speed = format!("{:.0}", game.telemetry().speed_kmh.abs());
        let n = shadowed(p, top_right, Align2::RIGHT_TOP, &speed, Font::data(22.0).weight(600.0), col::DUST, a);
        let track = Rect::from_min_size(pos2(top_right.x - 5.0, n.bottom() + 10.0), vec2(5.0, 300.0));
        p.rect_filled(track, 2.5, fade(RING_TRACK, a));
        let (fill, colour) = Self::revs(game, now);
        if fill > 0.01 {
            let h = (track.height() * fill).max(5.0);
            p.rect_filled(Rect::from_min_max(pos2(track.left(), track.bottom() - h), track.max), 2.5, fade(colour, a));
        }
    }

    fn debug_chip(&self, p: &Painter, at: Pos2, game: &Game, fps: &Fps) {
        let line1 = format!("{:.0} FPS · {:.1} ms", fps.fps, fps.frame_ms).replace('.', ",");
        let p_name = &game.session.profile().params.name;
        let line2 = format!("Profil {} · {} · Caméra {}", game.session.current + 1, p_name, crate::camera::MODES[game.camera.mode].to_lowercase());
        let f = Font::data(12.0);
        let w = paint::text_size(p, &line2, f).x.max(paint::text_size(p, &line1, f).x) + 28.0;
        let rect = Rect::from_min_size(at, vec2(w, 50.0));
        p.rect_filled(rect, 12.0, GLASS);
        p.rect_stroke(rect, 12.0, Stroke::new(1.0, GLASS_LINE), StrokeKind::Inside);
        paint::text(p, pos2(rect.left() + 14.0, rect.top() + 16.0), Align2::LEFT_CENTER, &line1, f, col::DUST);
        paint::text(p, pos2(rect.left() + 14.0, rect.top() + 35.0), Align2::LEFT_CENTER, &line2, f, col::DUST_2);
    }

    // ---------------------------------------------------------------- countdown and finish

    fn countdown(&self, p: &Painter, r: Rect, wide: bool, now: f64) {
        let fg = p.ctx().layer_painter(LayerId::new(Order::Foreground, Id::new("hud countdown")));
        if let Some(go) = self.go_at {
            // The livery band sweeping across at GO.
            let t = ((now - go) / 0.8) as f32;
            if t < 1.0 {
                let e = paint::bezier(0.7, 0.0, 0.3, 1.0, t);
                let h = if wide { 120.0 } else { 90.0 };
                let cy = r.center().y - 30.0;
                let x0 = r.left() - r.width() * 1.2 + e * r.width() * 2.4;
                let skew = h * 0.5 * 10f32.to_radians().tan();
                let quad = |l: f32, w: f32, c: Color32| {
                    let pts = vec![pos2(l + skew, cy - h / 2.0), pos2(l + w + skew, cy - h / 2.0), pos2(l + w - skew, cy + h / 2.0), pos2(l - skew, cy + h / 2.0)];
                    fg.add(Shape::convex_polygon(pts, c, Stroke::NONE));
                };
                let w = r.width();
                quad(x0, w * 0.03, col::LIVERY);
                quad(x0 + w * 0.04, w * 0.6, Color32::from_rgba_premultiplied(56, 23, 7, 56));
                quad(x0 + w * 0.65, w * 0.01, col::DUST);
                quad(x0 + w * 0.67, w * 0.03, col::LIVERY);
            }
        }
        let Some((n, since)) = self.count else { return };
        let t = (now - since) as f32;
        let (scale, alpha) = if t < 0.27 { (1.6 - 0.6 * paint::ease_out(t / 0.27), t / 0.27) } else { (1.0 - 0.06 * ((t - 0.27) / 0.6).min(1.0), 1.0) };
        let alpha = if n == 0 { alpha * (1.0 - ((t - 0.35) / 0.25).clamp(0.0, 1.0)) } else { alpha };
        let base = if wide { 220.0 } else { 150.0 };
        let c = pos2(r.center().x, r.center().y - 30.0);
        let (s, colour) = if n == 0 { ("GO".to_string(), col::LIVERY) } else { (n.to_string(), col::DUST) };
        paint::text(&fg, c + vec2(0.0, 4.0), Align2::CENTER_CENTER, &s, Font::display(base * scale), fade(Color32::from_black_alpha(110), alpha));
        paint::text(&fg, c, Align2::CENTER_CENTER, &s, Font::display(base * scale), fade(colour, alpha));
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_card(&mut self, ui: &mut Ui, p: &Painter, r: Rect, wide: bool, game: &Game, now: f64, k: f32) {
        let Some(result) = &game.result else { return };
        p.rect_filled(r, 0.0, fade(Color32::from_rgba_premultiplied(5, 3, 4, 150), k));
        let w = if wide { 600.0 } else { r.width() - 32.0 };
        let pad = if wide { 30.0 } else { 18.0 };
        let splits = &game.run.splits;
        let rows = splits.len() + 1;
        let row_h = 33.0;
        let h = 26.0 + 18.0 + 14.0 + 58.0 + 14.0 + 30.0 + 14.0 + 52.0 + 14.0 + rows as f32 * row_h + 14.0 + 56.0 + 26.0;
        let rect = Rect::from_center_size(r.center() + vec2(0.0, (1.0 - k) * 18.0), vec2(w, h));
        p.rect_filled(rect, 24.0, fade(Color32::from_rgba_premultiplied(26, 18, 20, 230), k));
        p.rect_stroke(rect, 24.0, Stroke::new(1.0, fade(col::LINE, k)), StrokeKind::Inside);
        let x = rect.left() + pad;
        let iw = w - 2.0 * pad;
        let mut y = rect.top() + 26.0;
        // Circuit and profile.
        let slot = self.track(game).map_or(1, |(i, _)| i + 1);
        let flag = Rect::from_min_size(pos2(x, y + 3.0), vec2(12.0, 12.0));
        p.rect_filled(flag, 3.0, fade(col::EASY, k));
        paint::text(p, pos2(flag.right() + 8.0, flag.center().y), Align2::LEFT_CENTER, &format!("FACILE · {slot:02} · {}", game.map_name().to_uppercase()), Font::label(13.0, 0.16).weight(700.0), fade(col::DUST_2, k));
        if wide {
            paint::text(p, pos2(x + iw, flag.center().y), Align2::RIGHT_CENTER, &format!("PROFIL {}", game.session.profile().params.name.to_uppercase()), Font::label(11.0, 0.16), fade(col::DUST_3, k));
        }
        y += 18.0 + 14.0;
        // Title and time.
        let title = if result.record { "RECORD !" } else { "ARRIVÉE" };
        let tc = if result.record { col::LIVERY } else { col::DUST };
        let title_size = if wide { 58.0 } else { 40.0 };
        paint::text(p, pos2(x, y + 58.0), Align2::LEFT_BOTTOM, title, Font::display(title_size), fade(tc, k));
        paint::text(p, pos2(x + iw, y + 54.0), Align2::RIGHT_BOTTOM, &format_time(result.ticks), Font::data(if wide { 46.0 } else { 32.0 }).weight(600.0), fade(col::DUST, k));
        y += 58.0 + 14.0;
        // Gap to the previous record.
        let mut q = p.clone();
        q.set_opacity(k);
        match result.previous {
            Some(prev) => {
                let pill = delta_pill(&q, pos2(x + 40.0, y + 15.0), result.ticks as i64 - prev as i64, 15.0);
                let what = if wide { format!("sur ton ancien record ({})", format_time(prev)) } else { format!("record : {}", format_time(prev)) };
                paint::text(p, pos2(pill.right() + 10.0, y + 15.0), Align2::LEFT_CENTER, &what, Font::body(15.0), fade(col::DUST_2, k));
            }
            None => {
                paint::text(p, pos2(x, y + 15.0), Align2::LEFT_CENTER, "Premier temps sur ce circuit", Font::body(15.0), fade(col::DUST_2, k));
            }
        }
        y += 30.0 + 14.0;
        // Medals, the ones won lighting up one after the other.
        if let Some((_, t)) = self.track(game) {
            let targets = t.medal_ticks();
            let mw = (iw - 16.0) / 3.0;
            for (m, (name, _)) in MEDALS.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(x + m as f32 * (mw + 8.0), y), vec2(mw, 52.0));
                let won = result.ticks <= targets[m];
                let mc = MEDAL_COLOURS[m];
                p.rect_filled(cell, 12.0, fade(col::VOID, k));
                p.rect_stroke(cell, 12.0, Stroke::new(if won { 1.5 } else { 1.0 }, fade(if won { mc } else { col::LINE }, k)), StrokeKind::Inside);
                let cc = pos2(cell.left() + 24.0, cell.center().y);
                if won {
                    let at = self.finish_at.unwrap_or(now) + 0.8 + (2 - m) as f64 * 0.25;
                    let tm = ((now - at) / 0.6) as f32;
                    let s = if tm < 0.0 { 0.0 } else if tm < 0.6 { paint::ease_out(tm / 0.6) * 1.25 } else { 1.25 - 0.25 * ((tm - 0.6) / 0.4).min(1.0) };
                    if s > 0.0 {
                        p.circle_filled(cc, 13.0 * s, fade(mc, k));
                        p.circle_stroke(cc, 11.5 * s, Stroke::new(2.0, fade(col::INK_STRIPE, k)));
                    }
                } else {
                    p.circle_stroke(cc, 12.0, Stroke::new(2.0, fade(mc, k * 0.45)));
                }
                if wide || mw > 100.0 {
                    paint::text(p, pos2(cc.x + 22.0, cell.center().y - 8.0), Align2::LEFT_CENTER, &name.to_uppercase(), Font::label(12.0, 0.12).weight(700.0), fade(col::DUST_3, k));
                    paint::text(p, pos2(cc.x + 22.0, cell.center().y + 9.0), Align2::LEFT_CENTER, &format_time(targets[m]), Font::data(13.0), fade(col::DUST, k));
                } else {
                    paint::text(p, pos2(cc.x + 20.0, cell.center().y), Align2::LEFT_CENTER, &format_time(targets[m]), Font::data(11.0), fade(col::DUST, k));
                }
            }
        }
        y += 52.0 + 14.0;
        // Checkpoints and the finish, with their gaps to the previous record.
        let table = Rect::from_min_size(pos2(x, y), vec2(iw, rows as f32 * row_h));
        p.rect_filled(table, 12.0, fade(col::VOID, k));
        p.rect_stroke(table, 12.0, Stroke::new(1.0, fade(col::LINE, k)), StrokeKind::Inside);
        for i in 0..rows {
            let ry = y + i as f32 * row_h + row_h / 2.0;
            if i > 0 {
                p.hline(table.x_range(), y + i as f32 * row_h, Stroke::new(1.0, fade(col::LINE, k)));
            }
            let (label, tick, prev) = if i < splits.len() {
                (format!("CHECKPOINT {}", i + 1), splits[i], result.previous_splits.get(i).copied())
            } else {
                ("ARRIVÉE".to_string(), result.ticks, result.previous)
            };
            paint::text(p, pos2(x + 14.0, ry), Align2::LEFT_CENTER, &label, Font::label(12.0, 0.16), fade(col::DUST_3, k));
            paint::text(p, pos2(x + iw - 96.0, ry), Align2::RIGHT_CENTER, &format_time(tick), Font::data(13.0), fade(col::DUST, k));
            if let Some(pv) = prev {
                let d = tick as i64 - pv as i64;
                paint::text(p, pos2(x + iw - 14.0, ry), Align2::RIGHT_CENTER, &delta_text(d), Font::data(12.0), fade(delta_colour(d), k));
            }
        }
        y += rows as f32 * row_h + 14.0;
        // Buttons.
        let bw = (iw - 10.0) * 0.58;
        let again = Rect::from_min_size(pos2(x, y), vec2(bw, 56.0));
        let menu = Rect::from_min_max(pos2(again.right() + 10.0, y), pos2(x + iw, y + 56.0));
        let ra = ui.interact(again, Id::new("hud again"), Sense::click());
        let rm = ui.interact(menu, Id::new("hud menu"), Sense::click());
        p.rect_filled(again, 14.0, fade(if ra.hovered() { Color32::from_rgb(255, 128, 64) } else { col::LIVERY }, k));
        paint::stripes(&p.with_clip_rect(again.shrink(0.5)), again, fade(col::INK_STRIPE, k));
        let lf = Font::label(if wide { 19.0 } else { 17.0 }, 0.14).weight(800.0);
        let key_w = if wide { 52.0 } else { 0.0 };
        let lw = paint::text_size(p, "RECOMMENCER", lf).x;
        let lx = again.center().x - (lw + key_w) / 2.0;
        paint::text(p, pos2(lx, again.center().y), Align2::LEFT_CENTER, "RECOMMENCER", lf, fade(col::LIVERY_INK, k));
        if wide {
            paint::keycap(p, pos2(lx + lw + 12.0, again.center().y), "Entrée", fade(col::INK_STRIPE, k), None, fade(col::LIVERY_INK, k));
        }
        p.rect_stroke(menu, 14.0, Stroke::new(1.5, fade(if rm.hovered() { col::DUST_3 } else { col::LINE }, k)), StrokeKind::Inside);
        let mw = paint::text_size(p, "MENU", lf).x;
        let mx = menu.center().x - (mw + key_w) / 2.0;
        paint::text(p, pos2(mx, menu.center().y), Align2::LEFT_CENTER, "MENU", lf, fade(col::DUST_2, k));
        if wide {
            paint::keycap(p, pos2(mx + mw + 12.0, menu.center().y), "Échap", fade(GLASS, k), Some(fade(GLASS_LINE, k)), fade(col::DUST_2, k));
        }
        if ra.clicked() {
            self.requests.push(HudRequest::Restart);
        }
        if rm.clicked() {
            self.requests.push(HudRequest::Menu);
        }
    }

    // ---------------------------------------------------------------- touch

    /// The brake strip along the bottom and the two steering halves above it (invisible, lit
    /// while held, shown at the start by two blinks); returns the input they give.
    fn touch_controls(&mut self, ui: &Ui, p: &Painter, r: Rect, now: f64) -> Input {
        let brake = Rect::from_min_max(pos2(r.left(), r.bottom() - BRAKE_H), r.max);
        let steer = Rect::from_min_max(pos2(r.left(), r.top() + STEER_TOP), pos2(r.right(), brake.top()));
        let left = Rect::from_min_max(steer.min, pos2(steer.center().x, steer.bottom()));
        let right = Rect::from_min_max(pos2(steer.center().x, steer.top()), steer.max);
        let pts = if self.sheet.is_none() { self.touch_points(ui) } else { Vec::new() };
        let held = |rect: Rect| pts.iter().any(|q| rect.contains(*q));
        let (l, rt, b) = (held(left), held(right), held(brake));

        let glow = fade(col::DUST, 0.12);
        if l {
            let w = left.width() * 0.4;
            gradient(p, Rect::from_min_size(left.min, vec2(w, left.height())), [glow, Color32::TRANSPARENT, Color32::TRANSPARENT, glow]);
        }
        if rt {
            let w = right.width() * 0.4;
            gradient(p, Rect::from_min_max(pos2(right.right() - w, right.top()), right.max), [Color32::TRANSPARENT, glow, glow, Color32::TRANSPARENT]);
        }
        if b {
            band(p, brake, fade(SLOW, 0.1), fade(SLOW, 0.3));
        } else {
            band(p, brake, ZONE_LIGHT, ZONE_DARK);
        }
        p.hline(brake.x_range(), brake.top() + 0.5, Stroke::new(1.0, fade(SLOW, 0.35)));
        // Faint: the player knows where the brake is.
        let label = Font::label(15.0, 0.34).weight(800.0);
        paint::text(p, brake.center(), Align2::CENTER_CENTER, "FREIN", label, fade(SLOW, if b { 0.75 } else { 0.3 }));

        // At the start, the two halves blink twice, dark like the brake, with their names.
        let tutorial = self.start_at.map_or(0.0, |t| tutorial_alpha((now - t) as f32));
        if tutorial > 0.0 {
            let font = Font::label(18.0, 0.3).weight(800.0);
            for (half, name) in [(left.with_max_x(left.right() - 1.0), "GAUCHE"), (right.with_min_x(right.left() + 1.0), "DROITE")] {
                p.rect_filled(half, 0.0, fade(ZONE_DARK, tutorial));
                paint::text(p, half.center(), Align2::CENTER_CENTER, name, font, fade(col::DUST, 0.9 * tutorial));
            }
        }
        Input { steer: (rt as i32 - l as i32) as f32, gas: 0.0, brake: b as i32 as f32 }
    }

    /// The settings button in the top right corner: it opens the sheet and pauses the race.
    fn settings_button(&mut self, ui: &Ui, p: &Painter, r: Rect, a: f32, now: f64) {
        let rect = Rect::from_min_size(pos2(r.right() - 16.0 - 44.0, r.top() + 16.0), vec2(44.0, 44.0));
        let resp = ui.interact(rect, Id::new("hud settings"), Sense::click());
        p.circle_filled(rect.center(), 22.0, fade(GLASS, a));
        p.circle_stroke(rect.center(), 21.5, Stroke::new(1.0, fade(GLASS_LINE, a)));
        gear_icon(p, rect.center(), 22.0, fade(if resp.hovered() { Color32::WHITE } else { col::DUST }, a));
        if resp.clicked() {
            self.sheet = Some(now);
            self.focus = None;
            self.cues.push(Cue::SheetOpen);
        }
    }

    /// The settings sheet, from the bottom: camera, sound, last checkpoint, restart, menu, resume.
    #[allow(clippy::too_many_arguments)]
    fn settings_sheet(&mut self, ui: &Ui, r: Rect, wide: bool, game: &mut Game, muted: bool, now: f64, since: f64) {
        let p = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::new("hud sheet")));
        let k = paint::ease_out(((now - since) as f32 / 0.25).min(1.0));
        let h = 427.0;
        let w = if wide { 420.0 } else { r.width() };
        let card = Rect::from_min_size(pos2(r.center().x - w / 2.0, r.bottom() - h + (1.0 - k) * h), vec2(w, h));
        let scrim = ui.interact(r, Id::new("hud sheet scrim"), Sense::click());
        p.rect_filled(r, 0.0, fade(col::SCRIM, k));
        let top = CornerRadius { nw: 24, ne: 24, sw: 0, se: 0 };
        p.rect_filled(card, top, fade(col::PANEL, 0.97));
        p.hline(card.shrink2(vec2(20.0, 0.0)).x_range(), card.top(), Stroke::new(1.0, col::LINE));

        let pad = 18.0;
        let (x, iw) = (card.left() + pad, w - 2.0 * pad);
        let mut y = card.top() + 12.0;
        p.rect_filled(Rect::from_center_size(pos2(card.center().x, y + 2.0), vec2(40.0, 4.0)), 2.0, col::LINE);
        y += 4.0 + 14.0;
        let head = paint::text(&p, pos2(x, y), Align2::LEFT_TOP, "RÉGLAGES", Font::heading(26.0).weight(800.0), col::DUST);
        paint::text(&p, pos2(x + iw, head.bottom() - 6.0), Align2::RIGHT_BOTTOM, "COURSE EN PAUSE", tag_font(), col::DUST_2);
        y += 30.0 + 14.0;
        paint::text(&p, pos2(x, y), Align2::LEFT_TOP, "CAMÉRA", Font::label(12.0, 0.2).weight(700.0), col::DUST_3);
        y += 15.0 + 14.0;

        // Camera: three segments.
        let seg = Rect::from_min_size(pos2(x, y), vec2(iw, 44.0));
        p.rect_filled(seg, 12.0, col::VOID);
        p.rect_stroke(seg, 12.0, Stroke::new(1.0, col::LINE), StrokeKind::Inside);
        let cw = (iw - 8.0 - 8.0) / 3.0;
        for (i, name) in crate::camera::MODES.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(seg.left() + 4.0 + i as f32 * (cw + 4.0), seg.top() + 4.0), vec2(cw, 36.0));
            let resp = ui.interact(cell, Id::new(("hud camera", i)), Sense::click());
            let on = game.camera.mode == i;
            if on {
                p.rect_filled(cell, 9.0, col::PANEL_2);
                p.hline(cell.shrink2(vec2(7.0, 0.0)).x_range(), cell.bottom() - 1.0, Stroke::new(2.0, col::LIVERY));
            }
            let c = if on || resp.hovered() { col::DUST } else { col::DUST_3 };
            paint::text(&p, cell.center(), Align2::CENTER_CENTER, &name.to_uppercase(), Font::label(13.0, 0.1).weight(700.0), c);
            if resp.clicked() && !on {
                game.camera.mode = i;
                self.cues.push(Cue::Select);
            }
        }
        y += 44.0 + 14.0;

        // Sound and the actions.
        let acts = Rect::from_min_size(pos2(x, y), vec2(iw, 4.0 * 44.0));
        p.rect_filled(acts, 14.0, col::VOID);
        p.rect_stroke(acts, 14.0, Stroke::new(1.0, col::LINE), StrokeKind::Inside);
        let rows: [(&str, Option<HudRequest>); 4] = [
            ("Son", None),
            ("Dernier checkpoint", Some(HudRequest::Respawn)),
            ("Recommencer", Some(HudRequest::Restart)),
            ("Quitter vers le menu", Some(HudRequest::Menu)),
        ];
        let lf = Font::label(15.0, 0.08).weight(600.0);
        let mut close = false;
        for (i, (name, request)) in rows.iter().enumerate() {
            let row = Rect::from_min_size(pos2(acts.left() + 12.0, acts.top() + i as f32 * 44.0), vec2(iw - 24.0, 44.0));
            if i > 0 {
                p.hline(row.x_range(), row.top(), Stroke::new(1.0, col::LINE));
            }
            let resp = ui.interact(row, Id::new(("hud setting", i)), Sense::click());
            let colour = if *request == Some(HudRequest::Menu) { SLOW } else if resp.hovered() { Color32::WHITE } else { col::DUST };
            paint::text(&p, pos2(row.left() + 4.0, row.center().y), Align2::LEFT_CENTER, name, lf, colour);
            match request {
                None => {
                    // The sound switch.
                    let sw = Rect::from_center_size(pos2(row.right() - 4.0 - 23.0, row.center().y), vec2(46.0, 26.0));
                    let on = !muted;
                    p.rect_filled(sw, 13.0, if on { col::LIVERY } else { col::LINE });
                    let knob = if on { sw.right() - 13.0 } else { sw.left() + 13.0 };
                    p.circle_filled(pos2(knob, sw.center().y), 10.0, if on { col::LIVERY_INK } else { col::DUST_3 });
                    if resp.clicked() {
                        game.mute_requested = true;
                        self.cues.push(Cue::Select);
                    }
                }
                Some(req) => {
                    paint::icon_at(&p, pos2(row.right() - 10.0, row.center().y), 16.0, Icon::ChevronRight, col::DUST_3);
                    if resp.clicked() {
                        self.requests.push(*req);
                        self.cues.push(if *req == HudRequest::Menu { Cue::Back } else { Cue::Confirm });
                        close = true;
                    }
                }
            }
        }
        y += 4.0 * 44.0 + 14.0;

        // Resume.
        let cta = Rect::from_min_size(pos2(x, y), vec2(iw, 54.0));
        let resp = ui.interact(cta, Id::new("hud resume"), Sense::click());
        p.rect_filled(cta, 14.0, if resp.hovered() { Color32::from_rgb(255, 128, 64) } else { col::LIVERY });
        paint::stripes(&p.with_clip_rect(cta.shrink(0.5)), Rect::from_min_max(pos2(cta.right() - 70.0, cta.top()), cta.max), col::INK_STRIPE);
        paint::text(&p, cta.center(), Align2::CENTER_CENTER, "REPRENDRE", Font::label(18.0, 0.14).weight(800.0), col::LIVERY_INK);
        let outside = scrim.clicked() && scrim.interact_pointer_pos().is_some_and(|q| !card.contains(q));
        if resp.clicked() || outside {
            self.cues.push(Cue::SheetClose);
            close = true;
        }
        if close {
            self.sheet = None;
        }
    }
}
