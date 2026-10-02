//! On-screen controls for touch screens (iPhone, iPad): throttle and brake under the left thumb,
//! steering under the right one, and small buttons for the menu, the casual mode, a restart, the
//! last checkpoint, the camera and the settings panel. They follow every finger themselves (egui
//! follows only one), so the thumbs can steer, hold the throttle and press a button at the same
//! time.
//!
//! The left half of the screen works the pedals (throttle or brake, either side of the line
//! between them) and the right half steers (left or right of the line between the two arrows),
//! wherever the thumb is now: it slides from one to the other without lifting. A finger landing on
//! a button presses it once and drives nothing.
//!
//! The casual mode draws no pads: the car accelerates by itself and a finger on the left half of
//! the screen turns left, on the right half right.

use std::time::Instant;

use egui::{Align2, Color32, FontFamily, FontId, LayerId, Order, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use physics::Input;
use winit::event::TouchPhase;

use crate::input::Action;

/// The buttons, from the corner inwards.
const BUTTONS: [Action; 6] = [Action::Menu, Action::Casual, Action::Restart, Action::Respawn, Action::Camera, Action::TogglePanel];

/// Gap to the edges of the safe area, points.
const MARGIN: f32 = 18.0;
/// Radius of a button, points (44 across, the smallest comfortable target).
const BUTTON: f32 = 22.0;
/// Half the width the race time takes at the top centre, points: the buttons keep clear of it.
const TIME_HALF: f32 = 95.0;
/// The casual button while the mode is on (the menu's orange).
const CASUAL_ON: Color32 = Color32::from_rgba_premultiplied(200, 96, 40, 200);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Steers or works a pedal, from where it is now.
    Drive,
    /// Landed on a button or on the settings panel: drives nothing.
    Off,
}

struct Finger {
    id: u64,
    /// Physical pixels.
    pos: Pos2,
    role: Role,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
    Gas,
    Brake,
    Left,
    Right,
}

/// Where the controls are, in points.
#[derive(Clone, Copy)]
struct Layout {
    /// Centre and radius of each driving control, in [`Zone`] order.
    pads: [(Pos2, f32); 4],
    buttons: [Pos2; BUTTONS.len()],
    /// Left of this the fingers work the pedals, right of it they steer.
    middle: f32,
    /// Between the throttle and the brake.
    pedal_split: f32,
    /// Between the two arrows.
    steer_split: f32,
    /// Physical pixels per point.
    ppp: f32,
}

impl Layout {
    fn new(r: Rect, ppp: f32) -> Self {
        let half = r.width() / 2.0;
        // Two pads side by side fill at most a half, with a margin on each side.
        let unit = (r.width().min(r.height()) * 0.13).min((half - 2.0 * MARGIN) / 4.4).clamp(28.0, 60.0);
        let gap = 0.4 * unit;
        let (gas_r, brake_r) = (1.12 * unit, 0.88 * unit);
        let gas = pos2(r.left() + MARGIN + gas_r, r.bottom() - MARGIN - gas_r);
        let brake = pos2(gas.x + gas_r + gap + brake_r, r.bottom() - MARGIN - brake_r);
        let y = r.bottom() - MARGIN - unit;
        let right = pos2(r.right() - MARGIN - unit, y);
        let left = pos2(right.x - 2.0 * unit - gap, y);
        // In landscape, rows along the top right of the race time (a second one under the first
        // when they do not fit); in portrait, a column down the right edge under the race time.
        let step = 2.0 * BUTTON + 10.0;
        let per_row = ((r.right() - MARGIN - r.center().x - TIME_HALF + 10.0) / step).max(1.0) as usize;
        let buttons = std::array::from_fn(|i| {
            if r.width() > r.height() {
                let (col, row) = ((i % per_row) as f32, (i / per_row) as f32);
                pos2(r.right() - MARGIN - BUTTON - col * step, r.top() + 10.0 + BUTTON + row * step)
            } else {
                pos2(r.right() - MARGIN - BUTTON, r.top() + 104.0 + BUTTON + i as f32 * step)
            }
        });
        Self {
            pads: [(gas, gas_r), (brake, brake_r), (left, unit), (right, unit)],
            buttons,
            middle: r.center().x,
            pedal_split: (gas.x + gas_r + brake.x - brake_r) / 2.0,
            steer_split: (left.x + right.x) / 2.0,
            ppp,
        }
    }

    fn zone(&self, p: Pos2, casual: bool) -> Zone {
        match (casual, p.x < self.middle, p.x < self.pedal_split, p.x < self.steer_split) {
            (true, true, _, _) => Zone::Left,
            (true, false, _, _) => Zone::Right,
            (false, true, true, _) => Zone::Gas,
            (false, true, false, _) => Zone::Brake,
            (false, false, _, true) => Zone::Left,
            (false, false, _, false) => Zone::Right,
        }
    }

    fn button_at(&self, p: Pos2) -> Option<usize> {
        self.buttons.iter().position(|b| b.distance(p) < BUTTON * 1.35)
    }
}

pub struct TouchControls {
    /// A touch screen is in use: the race shows the controls and leaves out the keyboard help.
    pub active: bool,
    /// Casual mode: automatic throttle, steering by halves of the screen, no pads drawn.
    pub casual: bool,
    fingers: Vec<Finger>,
    /// Laid out by the last frame drawn.
    layout: Option<Layout>,
    actions: Vec<Action>,
    pressed: [Option<Instant>; BUTTONS.len()],
    /// The settings panel, in points: fingers landing on it are egui's.
    pub panel: Option<Rect>,
}

impl TouchControls {
    pub fn new() -> Self {
        Self {
            active: cfg!(any(target_os = "ios", target_os = "android")),
            casual: false,
            fingers: Vec::new(),
            layout: None,
            actions: Vec::new(),
            pressed: [None; BUTTONS.len()],
            panel: None,
        }
    }

    /// A finger landed, moved or lifted, in physical pixels. Fingers landing while the race is
    /// not on screen are left to the menu.
    pub fn touch(&mut self, id: u64, phase: TouchPhase, pos: Pos2, racing: bool) {
        self.active = true;
        match phase {
            TouchPhase::Started => {
                let Some(l) = self.layout.filter(|_| racing) else { return };
                let p = pos / l.ppp;
                let role = if self.panel.is_some_and(|r| r.contains(p)) {
                    Role::Off
                } else if let Some(i) = l.button_at(p) {
                    self.actions.push(BUTTONS[i]);
                    self.pressed[i] = Some(Instant::now());
                    Role::Off
                } else {
                    Role::Drive
                };
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger { id, pos, role });
            }
            TouchPhase::Moved => {
                if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
                    f.pos = pos;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => self.fingers.retain(|f| f.id != id),
        }
    }

    /// Forget every finger (the app went to the background, the menu opened).
    pub fn clear(&mut self) {
        self.fingers.clear();
    }

    /// Height the touch controls take up from the bottom of the safe area under its middle
    /// (a band 150 points wide), points: none in landscape or casual mode, the pads' height in
    /// portrait.
    pub fn middle_clearance(&self) -> f32 {
        let Some(l) = self.layout.filter(|_| !self.casual) else { return 0.0 };
        let bottom = l.pads.iter().map(|(c, r)| c.y + r + MARGIN).fold(0.0, f32::max);
        l.pads
            .iter()
            .filter(|(c, r)| (c.x - l.middle).abs() < r + 75.0)
            .map(|(c, r)| bottom - (c.y - r) + 6.0 - 18.0)
            .fold(0.0, f32::max)
    }

    /// The part of the safe area `r` the buttons leave free (the settings panel goes there, so
    /// its button stays in reach): below their rows in landscape, left of their column in portrait.
    pub fn room(&self, r: Rect) -> Rect {
        let Some(l) = self.layout else { return r };
        if r.width() > r.height() {
            let below = l.buttons.iter().map(|b| b.y).fold(f32::MIN, f32::max) + BUTTON + 6.0;
            Rect::from_min_max(pos2(r.left(), below), r.max)
        } else {
            let left = l.buttons.iter().map(|b| b.x).fold(f32::MAX, f32::min) - BUTTON - 6.0;
            Rect::from_min_max(r.min, pos2(left, r.bottom()))
        }
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    fn zones(&self) -> impl Iterator<Item = Zone> + '_ {
        let (layout, casual) = (self.layout, self.casual);
        self.fingers.iter().filter(|f| f.role == Role::Drive).filter_map(move |f| layout.map(|l| l.zone(f.pos / l.ppp, casual)))
    }

    /// What the fingers ask of the car (both arrows held cancel out, as on a keyboard); full
    /// throttle all along in casual mode.
    pub fn driving(&self) -> Input {
        let mut input = Input { gas: if self.casual && self.active { 1.0 } else { 0.0 }, ..Input::default() };
        let (mut left, mut right) = (false, false);
        for zone in self.zones() {
            match zone {
                Zone::Gas => input.gas = 1.0,
                Zone::Brake => input.brake = 1.0,
                Zone::Left => left = true,
                Zone::Right => right = true,
            }
        }
        input.steer = (right as i32 - left as i32) as f32;
        input
    }

    /// Lays the controls out in the safe area and draws them under the HUD (only the buttons in
    /// casual mode).
    pub fn draw(&mut self, ctx: &egui::Context) {
        let layout = Layout::new(ctx.content_rect(), ctx.pixels_per_point());
        self.layout = Some(layout);
        let p = ctx.layer_painter(LayerId::new(Order::Background, egui::Id::new("touch controls")));
        let ink = Color32::from_white_alpha(235);
        if !self.casual {
            let held: Vec<Zone> = self.zones().collect();
            for (i, &(c, r)) in layout.pads.iter().enumerate() {
                let zone = [Zone::Gas, Zone::Brake, Zone::Left, Zone::Right][i];
                disc(&p, c, r, held.contains(&zone));
                match zone {
                    Zone::Gas => label(&p, c, "GAZ", 0.36 * r, ink),
                    Zone::Brake => label(&p, c, "FREIN", 0.3 * r, ink),
                    Zone::Left => chevron(&p, c, -0.42 * r, ink),
                    Zone::Right => chevron(&p, c, 0.42 * r, ink),
                }
            }
        }
        for (i, &c) in layout.buttons.iter().enumerate() {
            let lit = self.pressed[i].is_some_and(|t| t.elapsed().as_secs_f32() < 0.18);
            disc(&p, c, BUTTON, lit);
            if BUTTONS[i] == Action::Casual && self.casual {
                p.circle_filled(c, BUTTON - 1.0, CASUAL_ON);
            }
            icon(&p, c, BUTTONS[i], ink);
        }
    }
}

fn disc(p: &Painter, c: Pos2, r: f32, held: bool) {
    let fill = if held { Color32::from_white_alpha(70) } else { Color32::from_black_alpha(80) };
    p.circle(c, r, fill, Stroke::new(1.5, Color32::from_white_alpha(if held { 200 } else { 90 })));
}

/// A filled arrowhead pointing left (`w` < 0) or right.
fn chevron(p: &Painter, c: Pos2, w: f32, colour: Color32) {
    let h = w.abs() * 1.1;
    let tip = c + vec2(w * 0.6, 0.0);
    let back = c - vec2(w * 0.5, 0.0);
    p.add(Shape::convex_polygon(vec![tip, back + vec2(0.0, h), back - vec2(0.0, h)], colour, Stroke::NONE));
}

fn label(p: &Painter, c: Pos2, text: &str, size: f32, colour: Color32) {
    p.text(c, Align2::CENTER_CENTER, text, FontId::new(size, FontFamily::Name("Saira".into())), colour);
}

/// The button's line icon, about 20 points across.
fn icon(p: &Painter, c: Pos2, action: Action, colour: Color32) {
    let stroke = Stroke::new(2.2, colour);
    let at = |x: f32, y: f32| c + vec2(x, y);
    match action {
        // Pause bars.
        Action::Menu => {
            for x in [-4.0, 4.0] {
                p.line_segment([at(x, -7.0), at(x, 7.0)], Stroke::new(3.2, colour));
            }
        }
        // A steering wheel: only steering to do.
        Action::Casual => {
            p.circle_stroke(c, 8.5, Stroke::new(2.0, colour));
            p.circle_filled(c, 2.6, colour);
            for (x, y) in [(-8.0, 0.0), (8.0, 0.0), (0.0, 8.0)] {
                p.line_segment([c, at(x, y)], Stroke::new(1.8, colour));
            }
        }
        // A circular arrow.
        Action::Restart => {
            let points: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = (-60.0 + 290.0 * i as f32 / 20.0).to_radians();
                    at(8.0 * a.cos(), 8.0 * a.sin())
                })
                .collect();
            let end = points[0];
            p.add(Shape::line(points, stroke));
            p.add(Shape::convex_polygon(vec![end + vec2(-5.0, -1.0), end + vec2(4.0, -5.0), end + vec2(2.0, 4.0)], colour, Stroke::NONE));
        }
        // A flag: back to the last checkpoint.
        Action::Respawn => {
            p.line_segment([at(-6.0, -8.0), at(-6.0, 9.0)], stroke);
            p.add(Shape::convex_polygon(vec![at(-6.0, -8.0), at(8.0, -4.0), at(-6.0, 1.0)], colour, Stroke::NONE));
        }
        // A film camera.
        Action::Camera => {
            p.rect_stroke(Rect::from_min_size(at(-9.0, -5.5), vec2(12.0, 11.0)), 2.0, stroke, egui::StrokeKind::Middle);
            p.add(Shape::convex_polygon(vec![at(4.5, -1.0), at(9.5, -5.0), at(9.5, 5.0), at(4.5, 1.0)], colour, Stroke::NONE));
        }
        // Sliders: the tuning panel.
        _ => {
            for (y, x) in [(-5.0, 3.0), (0.0, -3.0), (5.0, 1.5)] {
                p.line_segment([at(-8.0, y), at(8.0, y)], Stroke::new(1.6, colour));
                p.circle_filled(at(x, y), 2.6, colour);
            }
        }
    }
}
