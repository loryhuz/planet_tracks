//! The race HUD, in the menu's style: the circuit and the medal to aim for, the chrono with its
//! checkpoints and the gap to the record at each one, speed, revs, gear and what each wheel rolls
//! on, the start countdown and the finish card. Two layouts as in the menu: wide (1280 × 720
//! design space, keyboard hints) and tall for phones (390 × 844, with touch controls, which also
//! appear in the wide layout once the screen has been touched). Debug (FPS, telemetry, tuning)
//! only shows with Tab.

use std::collections::BTreeMap;

use egui::epaint::{Mesh, Vertex};
use egui::{Align2, Color32, Event, Id, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TouchPhase, Ui, pos2, vec2};
use physics::Input;

use crate::game::Game;
use crate::menu::Layout;
use crate::menu::catalog::{MEDALS, TrackInfo};
use crate::menu::paint::{self, Font, Icon, MEDAL_COLOURS, col, fade};
use crate::race::{COUNTDOWN_TICKS, format_delta, format_time};
use crate::ui::Fps;
use crate::ui_sound::Cue;

/// Dark glass over the scene, and its edge.
const GLASS: Color32 = Color32::from_rgba_premultiplied(7, 5, 5, 150);
const GLASS_LINE: Color32 = Color32::from_rgba_premultiplied(24, 23, 22, 26);
/// Slower than the record.
const SLOW: Color32 = Color32::from_rgb(255, 90, 95);
const SLOW_SOFT: Color32 = Color32::from_rgba_premultiplied(41, 14, 15, 41);
const HUB_SOFT: Color32 = Color32::from_rgba_premultiplied(14, 35, 41, 41);
/// Segments of the rev bar, the last ones red.
const REV_SEGMENTS: usize = 14;
const REV_RED: usize = 11;
/// Engine revs the rev bar starts from (idle is 0.45 of the redline).
const REV_FLOOR: f32 = 0.4;

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
    last_tick: u32,
    last_countdown: u32,
    /// Fingers on the screen, by touch id.
    touches: BTreeMap<u64, Pos2>,
    touch_seen: bool,
    cues: Vec<Cue>,
    requests: Vec<HudRequest>,
}

fn glass(p: &Painter, rect: Rect, radius: f32, a: f32) {
    p.rect_filled(rect, radius, fade(GLASS, a));
    p.rect_stroke(rect, radius, Stroke::new(1.0, fade(GLASS_LINE, a)), StrokeKind::Inside);
}

/// A vertical gradient band (darkens the top and bottom of the scene under the HUD).
fn band(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    for (pos, c) in [(rect.left_top(), top), (rect.right_top(), top), (rect.right_bottom(), bottom), (rect.left_bottom(), bottom)] {
        mesh.vertices.push(Vertex { pos, uv: egui::epaint::WHITE_UV, color: c });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(Shape::mesh(mesh));
}

/// Skewed bars in a row, `on` of them lit (checkpoints, revs).
#[allow(clippy::too_many_arguments)]
fn bars(p: &Painter, left_center: Pos2, n: usize, on: usize, w: f32, h: f32, gap: f32, colour: impl Fn(usize) -> Color32) {
    for i in 0..n {
        let x = left_center.x + i as f32 * (w + gap);
        let k = h * 0.32;
        let pts = vec![
            pos2(x + k, left_center.y - h / 2.0),
            pos2(x + w + k, left_center.y - h / 2.0),
            pos2(x + w - k, left_center.y + h / 2.0),
            pos2(x - k, left_center.y + h / 2.0),
        ];
        let c = if i < on { colour(i) } else { col::LINE };
        p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
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

/// Pause (two bars) and back-to-checkpoint (a flag) icons for the touch layout.
fn pause_icon(p: &Painter, c: Pos2, s: f32, colour: Color32) {
    for dx in [-0.22, 0.22] {
        p.rect_filled(Rect::from_center_size(c + vec2(dx * s, 0.0), vec2(0.16 * s, 0.6 * s)), 1.5, colour);
    }
}

fn flag_icon(p: &Painter, c: Pos2, s: f32, colour: Color32) {
    let x = c.x - 0.25 * s;
    p.line_segment([pos2(x, c.y - 0.35 * s), pos2(x, c.y + 0.38 * s)], Stroke::new(2.0, colour));
    let pts = vec![pos2(x, c.y - 0.35 * s), pos2(x + 0.55 * s, c.y - 0.2 * s), pos2(x, c.y - 0.02 * s)];
    p.add(Shape::convex_polygon(pts, colour, Stroke::NONE));
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
            last_tick: 0,
            last_countdown: 0,
            touches: BTreeMap::new(),
            touch_seen: false,
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
            self.cues.push(if delta.is_some_and(|d| d > 0) { Cue::SplitSlower } else { Cue::SplitFaster });
            self.split = Some(Split { index: n, total: game.track.checkpoints.len(), tick, delta, at: now });
        }
        self.seen_splits = run.splits.len();
        if run.finished.is_some() && self.finish_at.is_none() {
            self.finish_at = Some(now);
            self.cues.push(if game.result.as_ref().is_some_and(|r| r.record) { Cue::Record } else { Cue::Finish });
        }
    }

    /// Fingers on the screen (and the mouse, held, to try the touch controls on a computer).
    fn touch_points(&mut self, ui: &Ui) -> Vec<Pos2> {
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
        let mut pts: Vec<Pos2> = self.touches.values().copied().collect();
        if let Some(pos) = ui.input(|i| i.pointer.primary_down().then(|| i.pointer.latest_pos()).flatten()) {
            pts.push(pos);
        }
        pts
    }

    /// Draws the HUD for this frame and sets the touch controls' driving input.
    pub fn ui(&mut self, ui: &mut Ui, game: &mut Game, fps: &Fps) {
        let now = ui.input(|i| i.time);
        let r = ui.ctx().content_rect();
        let layout = Layout::for_size(r.width(), r.height());
        let wide = layout == Layout::Wide;
        let p = ui.painter().clone();
        self.events(game, now);

        // Darker top and bottom, so the panels read over bright Martian sand.
        let shade = Color32::from_rgba_premultiplied(2, 1, 1, 107);
        band(&p, Rect::from_min_max(r.min, pos2(r.right(), r.top() + r.height() * 0.22)), shade, Color32::TRANSPARENT);
        band(&p, Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.74), r.max), Color32::TRANSPARENT, shade);

        let finished = game.run.finished.is_some();
        let finish_k = self.finish_at.map_or(0.0, |t| (((now - t) as f32 - 0.5) / 0.35).clamp(0.0, 1.0));
        let hud_a = 1.0 - finish_k;

        let touch = !wide || self.touch_seen;
        let input = if touch && !finished { self.touch_controls(ui, &p, r, wide, now) } else { Input::default() };
        game.controls.touch = input;

        if wide {
            self.track_card(&p, r.min + vec2(24.0, 22.0), game, hud_a, true);
            self.chrono(&p, pos2(r.center().x, r.top() + 18.0), game, hud_a, true);
            self.split_popup(&p, pos2(r.center().x, r.top() + 116.0), now, hud_a, true);
            self.cluster(&p, pos2(r.center().x, r.bottom() - 22.0), game, now, hud_a, true);
            if !touch {
                self.hints(&p, r, game);
            }
        } else {
            self.touch_buttons_top(ui, &p, r, hud_a);
            self.chrono(&p, pos2(r.center().x, r.top() + 14.0), game, hud_a, false);
            self.objective_chip(&p, pos2(r.center().x, r.top() + 104.0), game, hud_a);
            self.split_popup(&p, pos2(r.center().x, r.top() + 136.0), now, hud_a, false);
            self.cluster(&p, pos2(r.center().x, r.bottom() - 24.0 - 84.0 - 14.0), game, now, hud_a, false);
        }
        if game.panel_open {
            self.debug_chip(&p, r, wide, game, fps);
        }
        self.countdown(&p, r, wide, now);
        if finished && finish_k > 0.0 {
            self.finish_card(ui, &p, r, wide, game, now, finish_k);
        }
    }

    // ---------------------------------------------------------------- panels

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

    fn track_card(&self, p: &Painter, at: Pos2, game: &Game, a: f32, _wide: bool) {
        let rect = Rect::from_min_size(at, vec2(232.0, 112.0));
        glass(p, rect, 14.0, a);
        let x = rect.left() + 16.0;
        let slot = self.track(game).map_or(1, |(i, _)| i + 1);
        let flag = Rect::from_min_size(pos2(x, rect.top() + 14.0), vec2(12.0, 12.0));
        p.rect_filled(flag, 3.0, fade(col::EASY, a));
        paint::text(p, pos2(flag.right() + 8.0, flag.center().y), Align2::LEFT_CENTER, &format!("FACILE · {slot:02}"), Font::label(12.0, 0.16).weight(700.0), fade(col::DUST_2, a));
        paint::text(p, pos2(x - 1.0, rect.top() + 28.0), Align2::LEFT_TOP, &game.map_name().to_uppercase(), Font::heading(26.0), fade(col::DUST, a));
        let best = game.session.profile().best(&game.map_key()).map(|b| b.ticks);
        let row = |y: f32, label: &str, value: &str, dot: Option<Color32>| {
            let mut lx = x;
            if let Some(c) = dot {
                p.circle_filled(pos2(x + 5.0, y), 5.0, fade(c, a));
                lx += 16.0;
            }
            paint::text(p, pos2(lx, y), Align2::LEFT_CENTER, label, Font::label(11.0, 0.16), fade(col::DUST_3, a));
            paint::text(p, pos2(rect.right() - 16.0, y), Align2::RIGHT_CENTER, value, Font::data(13.0), fade(col::DUST_2, a));
        };
        row(rect.top() + 72.0, "TON RECORD", &best.map_or("--:--.--".into(), format_time), None);
        match self.objective(game) {
            Some((k, ticks)) => row(rect.top() + 93.0, &format!("OBJECTIF {}", MEDALS[k].0.to_uppercase()), &format_time(ticks), Some(MEDAL_COLOURS[k])),
            None => row(rect.top() + 93.0, "TOUTES LES MÉDAILLES", "✓", Some(MEDAL_COLOURS[0])),
        }
    }

    /// Tall layout: the medal to aim for, under the chrono.
    fn objective_chip(&self, p: &Painter, center: Pos2, game: &Game, a: f32) {
        let (label, c) = match self.objective(game) {
            Some((k, ticks)) => (format!("OBJECTIF {}  {}", MEDALS[k].0.to_uppercase(), format_time(ticks)), MEDAL_COLOURS[k]),
            None => ("TOUTES LES MÉDAILLES".to_string(), MEDAL_COLOURS[0]),
        };
        let font = Font::label(11.0, 0.12);
        let ts = paint::text_size(p, &label, font);
        let rect = Rect::from_center_size(center, vec2(ts.x + 40.0, 26.0));
        glass(p, rect, 13.0, a);
        p.circle_filled(pos2(rect.left() + 14.0, center.y), 4.5, fade(c, a));
        paint::text(p, pos2(rect.left() + 26.0, center.y), Align2::LEFT_CENTER, &label, font, fade(col::DUST_2, a));
    }

    fn chrono(&self, p: &Painter, top_center: Pos2, game: &Game, a: f32, wide: bool) {
        let run = &game.run;
        let ticks = run.finished.unwrap_or(run.tick);
        let time = format_time(ticks);
        let size = if wide { 42.0 } else { 34.0 };
        let font = Font::data(size).weight(600.0);
        let ts = paint::text_size(p, &time, font);
        let w = (ts.x + 48.0).max(if wide { 220.0 } else { 180.0 });
        let h = if wide { 88.0 } else { 74.0 };
        let rect = Rect::from_min_size(pos2(top_center.x - w / 2.0, top_center.y), vec2(w, h));
        glass(p, rect, 14.0, a);
        paint::text(p, pos2(rect.center().x, rect.top() + h * 0.4), Align2::CENTER_CENTER, &time, font, fade(col::DUST, a));
        let n = game.track.checkpoints.len();
        let label = format!("CP {} / {}", run.splits.len(), n);
        let lf = Font::data(11.0);
        let lw = paint::text_size(p, &label, lf).x;
        let (bw, gap) = (24.0, 5.0);
        let total = n as f32 * (bw + gap) - gap + 10.0 + lw;
        let y = rect.bottom() - if wide { 16.0 } else { 13.0 };
        let x = rect.center().x - total / 2.0;
        bars(p, pos2(x, y), n, run.splits.len(), bw, 6.0, gap, |_| fade(col::LIVERY, a));
        paint::text(p, pos2(x + total - lw, y), Align2::LEFT_CENTER, &label, lf, fade(col::DUST_2, a));
    }

    fn split_popup(&self, p: &Painter, top_center: Pos2, now: f64, a: f32, wide: bool) {
        let Some(s) = &self.split else { return };
        let t = (now - s.at) as f32;
        if t > 2.8 {
            return;
        }
        let (k, dy) = if t < 0.22 {
            let e = paint::ease_out(t / 0.22);
            (e, -10.0 * (1.0 - e))
        } else if t > 2.4 {
            (1.0 - (t - 2.4) / 0.4, -6.0 * (t - 2.4) / 0.4)
        } else {
            (1.0, 0.0)
        };
        let a = a * k;
        let label = format!("CHECKPOINT {} / {}", s.index, s.total);
        let time = format_time(s.tick);
        let tf = Font::data(if wide { 26.0 } else { 22.0 }).weight(600.0);
        let tw = paint::text_size(p, &time, tf).x;
        let dw = if s.delta.is_some() { 96.0 } else { 0.0 };
        let w = (tw + dw + 44.0).max(240.0);
        let h = if wide { 78.0 } else { 70.0 };
        let rect = Rect::from_min_size(pos2(top_center.x - w / 2.0, top_center.y + dy), vec2(w, h));
        glass(p, rect, 14.0, a);
        paint::text(p, pos2(rect.center().x, rect.top() + 18.0), Align2::CENTER_CENTER, &label, Font::label(13.0, 0.2).weight(700.0), fade(col::DUST_2, a));
        let row_y = rect.top() + h * 0.64;
        let start = rect.center().x - (tw + if dw > 0.0 { dw + 12.0 } else { 0.0 }) / 2.0;
        paint::text(p, pos2(start, row_y), Align2::LEFT_CENTER, &time, tf, fade(col::DUST, a));
        if let Some(d) = s.delta {
            let mut q = p.clone();
            q.set_opacity(a);
            delta_pill(&q, pos2(start + tw + 12.0 + dw / 2.0, row_y), d, 15.0);
        }
    }

    /// Speed, gear, revs and the four wheels' surfaces.
    fn cluster(&self, p: &Painter, bottom_center: Pos2, game: &Game, now: f64, a: f32, wide: bool) {
        let t = game.telemetry();
        let s = game.sound_frame();
        let revs = crate::engine_sound::revs(s.rpm, s.gear, s.load);
        let (w, h) = if wide { (436.0, 112.0) } else { (330.0, 96.0) };
        let rect = Rect::from_min_size(pos2(bottom_center.x - w / 2.0, bottom_center.y - h), vec2(w, h));
        glass(p, rect, 18.0, a);
        // Rev bar, flashing white at the redline.
        let lit = (((revs - REV_FLOOR) / (1.0 - REV_FLOOR)) * REV_SEGMENTS as f32).round().clamp(0.0, REV_SEGMENTS as f32) as usize;
        let flash = revs > 0.97 && (now * 14.0) as i64 % 2 == 0;
        let gap = 4.0;
        let bw = (w - 36.0 - gap * (REV_SEGMENTS - 1) as f32) / REV_SEGMENTS as f32;
        bars(p, pos2(rect.left() + 18.0, rect.top() + 16.0), REV_SEGMENTS, lit, bw, 7.0, gap, |i| {
            fade(if flash { col::DUST } else if i >= REV_RED { SLOW } else { col::LIVERY }, a)
        });
        let cy = rect.top() + h * 0.62;
        // Speed in the middle.
        let speed = format!("{:.0}", t.speed_kmh.max(0.0));
        let sf = Font::data(if wide { 62.0 } else { 50.0 }).weight(600.0);
        let sw = paint::text_size(p, "000", sf).x;
        let sx = rect.center().x - (sw + 44.0) / 2.0 + 12.0;
        paint::text(p, pos2(sx + sw, cy), Align2::RIGHT_CENTER, &speed, sf, fade(col::DUST, a));
        paint::text(p, pos2(sx + sw + 7.0, cy + if wide { 14.0 } else { 11.0 }), Align2::LEFT_CENTER, "KM/H", Font::label(if wide { 13.0 } else { 11.0 }, 0.14).weight(700.0), fade(col::DUST_3, a));
        // Gear on the right.
        let gw = if wide { 58.0 } else { 48.0 };
        let gr = Rect::from_center_size(pos2(rect.right() - 18.0 - gw / 2.0, cy), vec2(gw, if wide { 56.0 } else { 50.0 }));
        p.rect_stroke(gr, 11.0, Stroke::new(1.0, fade(GLASS_LINE, a)), StrokeKind::Inside);
        let gear = if game.run.countdown > 0 && t.speed_kmh < 1.0 { "N".to_string() } else { (s.gear + 1).to_string() };
        paint::text(p, pos2(gr.center().x, gr.top() + gr.height() * 0.42), Align2::CENTER_CENTER, &gear, Font::heading(if wide { 30.0 } else { 26.0 }), fade(col::DUST, a));
        paint::text(p, pos2(gr.center().x, gr.bottom() - 9.0), Align2::CENTER_CENTER, "RAPPORT", Font::label(if wide { 9.5 } else { 8.5 }, 0.14), fade(col::DUST_3, a));
        // The wheels, as seen from above, coloured by what they roll on.
        let wheels = &game.run.car.state.wheels;
        let wx = rect.left() + 18.0;
        let (ww, wh, wg) = if wide { (9.0, 14.0, 12.0) } else { (8.0, 12.0, 10.0) };
        let mut dirt = 0;
        let mut air = 0;
        for (i, wheel) in wheels.iter().enumerate() {
            let col_i = (i % 2) as f32;
            let row_i = (i / 2) as f32;
            let wr = Rect::from_min_size(pos2(wx + col_i * (ww + wg), cy - wh - wg / 2.0 + row_i * (wh + wg)), vec2(ww, wh));
            if !wheel.contact {
                air += 1;
                p.rect_stroke(wr, 3.0, Stroke::new(1.5, fade(col::DUST_3, a)), StrokeKind::Inside);
                continue;
            }
            let on_dirt = matches!(wheel.surface, Some(track::Surface::Dirt) | Some(track::Surface::Ground));
            dirt += on_dirt as usize;
            p.rect_filled(wr, 3.0, fade(if on_dirt { col::DIRT } else { col::ROAD }, a));
        }
        let (word, wc) = if air == 4 {
            ("EN L'AIR", col::DUST_2)
        } else if dirt >= 2 {
            ("DIRT", col::DIRT)
        } else {
            ("BITUME", col::DUST)
        };
        let tx = wx + 2.0 * ww + wg + 12.0;
        if wide {
            paint::text(p, pos2(tx, cy - 9.0), Align2::LEFT_CENTER, "SOUS LES ROUES", Font::label(10.5, 0.16), fade(col::DUST_3, a));
            paint::text(p, pos2(tx, cy + 9.0), Align2::LEFT_CENTER, word, Font::label(17.0, 0.1).weight(800.0), fade(wc, a));
        } else {
            paint::text(p, pos2(tx, cy), Align2::LEFT_CENTER, word, Font::label(14.0, 0.1).weight(800.0), fade(wc, a));
        }
    }

    /// Wide layout: the keys, stacked in the bottom right corner, shown at the start and fading
    /// once the race is under way.
    fn hints(&self, p: &Painter, r: Rect, game: &Game) {
        let run = &game.run;
        let a = if run.countdown > 0 { 1.0 } else { 1.0 - ((run.tick as f32 - 250.0) / 60.0).clamp(0.0, 1.0) };
        if a <= 0.0 || run.finished.is_some() {
            return;
        }
        let items = [("Entrée", "DERNIER CP"), ("Retour", "RECOMMENCER"), ("Échap", "MENU"), ("Tab", "DEBUG")];
        let font = Font::label(13.0, 0.14);
        let key_w = items.iter().map(|(k, _)| (paint::text_size(p, k, Font::data(11.0)).x + 14.0).max(28.0)).fold(0.0, f32::max);
        let label_w = items.iter().map(|(_, l)| paint::text_size(p, l, font).x).fold(0.0, f32::max);
        let x = r.right() - 26.0 - label_w - 10.0 - key_w;
        for (i, (key, label)) in items.iter().enumerate() {
            let y = r.bottom() - 30.0 - 13.0 - (items.len() - 1 - i) as f32 * 32.0;
            paint::keycap(p, pos2(x, y), key, fade(GLASS, a), Some(fade(GLASS_LINE, a)), fade(col::DUST_2, a));
            paint::text(p, pos2(x + key_w + 10.0, y), Align2::LEFT_CENTER, label, font, fade(col::DUST, a));
        }
    }

    fn debug_chip(&self, p: &Painter, r: Rect, wide: bool, game: &Game, fps: &Fps) {
        let top = if wide { r.top() + 146.0 } else { r.top() + 124.0 };
        let left = if wide { r.left() + 24.0 } else { r.left() + 14.0 };
        let line1 = format!("{:.0} FPS · {:.1} ms", fps.fps, fps.frame_ms).replace('.', ",");
        let p_name = &game.session.profile().params.name;
        let line2 = format!("Profil {} · {} · Caméra {}", game.session.current + 1, p_name, crate::camera::MODES[game.camera.mode].to_lowercase());
        let f = Font::data(12.0);
        let w = paint::text_size(p, &line2, f).x.max(paint::text_size(p, &line1, f).x) + 28.0;
        let rect = Rect::from_min_size(pos2(left, top), vec2(w, 50.0));
        glass(p, rect, 12.0, 1.0);
        paint::text(p, pos2(rect.left() + 14.0, rect.top() + 16.0), Align2::LEFT_CENTER, &line1, f, col::DUST);
        paint::text(p, pos2(rect.left() + 14.0, rect.top() + 35.0), Align2::LEFT_CENTER, &line2, f, col::DUST_2);
    }

    // ---------------------------------------------------------------- countdown and finish

    fn countdown(&self, p: &Painter, r: Rect, wide: bool, now: f64) {
        let fg = p.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("hud countdown")));
        if let Some(go) = self.go_at {
            // The livery band sweeping across at GO.
            let t = ((now - go) / 0.8) as f32;
            if t < 1.0 {
                let e = paint::bezier(0.7, 0.0, 0.3, 1.0, t);
                let h = if wide { 120.0 } else { 90.0 };
                let cy = r.center().y - if wide { 40.0 } else { 30.0 };
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
        let base = if wide { 230.0 } else { 160.0 };
        let c = pos2(r.center().x, r.center().y - if wide { 40.0 } else { 30.0 });
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
                let c = if d <= 0 { col::HUB } else { SLOW };
                paint::text(p, pos2(x + iw - 14.0, ry), Align2::RIGHT_CENTER, &format_delta(d).replace('-', "−"), Font::data(12.0), fade(c, k));
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

    /// Tall layout: pause (back to the menu) and back-to-checkpoint buttons at the top.
    fn touch_buttons_top(&mut self, ui: &mut Ui, p: &Painter, r: Rect, a: f32) {
        let s = 44.0;
        let menu = Rect::from_min_size(pos2(r.left() + 14.0, r.top() + 14.0), vec2(s, s));
        let cp = Rect::from_min_size(pos2(r.right() - 14.0 - s, r.top() + 14.0), vec2(s, s));
        for (rect, id, which) in [(menu, "hud pause", HudRequest::Menu), (cp, "hud respawn", HudRequest::Respawn)] {
            let resp = ui.interact(rect, Id::new(id), Sense::click());
            glass(p, rect, s / 2.0, a);
            let c = fade(if resp.hovered() { col::DUST } else { col::DUST_2 }, a);
            if which == HudRequest::Menu {
                pause_icon(p, rect.center(), 22.0, c);
            } else {
                flag_icon(p, rect.center(), 24.0, c);
            }
            if resp.clicked() {
                self.requests.push(which);
            }
        }
    }

    /// Steering on the left, brake and throttle on the right; returns the input they give.
    fn touch_controls(&mut self, ui: &mut Ui, p: &Painter, r: Rect, wide: bool, _now: f64) -> Input {
        let pts = self.touch_points(ui);
        let (bw, bh, margin, gap) = if wide { (104.0, 104.0, 28.0, 16.0) } else { (76.0, 84.0, 16.0, 10.0) };
        let y = r.bottom() - margin - bh;
        let left = Rect::from_min_size(pos2(r.left() + margin, y), vec2(bw, bh));
        let right = Rect::from_min_size(pos2(left.right() + gap, y), vec2(bw, bh));
        let gas = Rect::from_min_size(pos2(r.right() - margin - bw, y), vec2(bw, bh));
        let brake = Rect::from_min_size(pos2(gas.left() - gap - bw, y), vec2(bw, bh));
        let pressed = |rect: Rect| pts.iter().any(|q| rect.expand(6.0).contains(*q));
        // The buttons take the clicks, so nothing behind reacts to them.
        for (rect, id) in [(left, "l"), (right, "r"), (brake, "b"), (gas, "g")] {
            let _ = ui.interact(rect, Id::new(("hud touch", id)), Sense::click_and_drag());
        }
        let draw = |rect: Rect, on: bool, accent: Option<Color32>| {
            let fill = if on { Color32::from_rgba_premultiplied(40, 30, 28, 190) } else { GLASS };
            p.rect_filled(rect, 22.0, fill);
            let edge = match (accent, on) {
                (Some(c), true) => (2.0, c),
                (Some(c), false) => (1.5, fade(c, 0.6)),
                (None, true) => (2.0, col::DUST_2),
                (None, false) => (1.0, GLASS_LINE),
            };
            p.rect_stroke(rect, 22.0, Stroke::new(edge.0, edge.1), StrokeKind::Inside);
        };
        let (l, rt, b, g) = (pressed(left), pressed(right), pressed(brake), pressed(gas));
        draw(left, l, None);
        draw(right, rt, None);
        draw(brake, b, Some(SLOW));
        draw(gas, g, Some(col::LIVERY));
        let isz = if wide { 34.0 } else { 30.0 };
        paint::icon_at(p, left.center(), isz, Icon::ChevronLeft, if l { col::DUST } else { col::DUST_2 });
        paint::icon_at(p, right.center(), isz, Icon::ChevronRight, if rt { col::DUST } else { col::DUST_2 });
        let lf = Font::label(if wide { 16.0 } else { 14.0 }, 0.14).weight(800.0);
        paint::text(p, brake.center(), Align2::CENTER_CENTER, "FREIN", lf, if b { SLOW } else { fade(SLOW, 0.8) });
        paint::text(p, gas.center(), Align2::CENTER_CENTER, "GAZ", lf, if g { col::LIVERY } else { fade(col::LIVERY, 0.85) });
        Input { steer: (rt as i32 - l as i32) as f32, gas: g as i32 as f32, brake: b as i32 as f32 }
    }
}
