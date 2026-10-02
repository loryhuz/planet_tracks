//! The menu's screens, in both layouts: positions follow the validated mock-ups (1280 × 720 for
//! the wide layout, 390 × 844 for the tall one), anchored to the window's edges.

use std::f32::consts::PI;

use egui::{Align2, Color32, CornerRadius, Id, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use super::catalog::{MEDALS, PLANETS, SLOTS, Series, Stat, TIPS};
use super::paint::{self, Font, Icon, col, fade};
use super::{Layout, Menu, Request, Screen, Shake, pin};
use crate::menu_gfx::{PlanetDraw, PlanetKind};
use crate::race::format_time;
use crate::ui_sound::Cue;

/// Places shown on the Mars globe: the three circuits' namesakes (latitude, longitude, degrees).
const PINS: [(&str, f32, f32); 3] = [("JEZERO", 18.4, 77.5), ("OLYMPUS", 18.65, -133.8), ("ARES VALLIS", 10.3, -25.8)];

const MARS_RIM: [f32; 3] = [212.0 / 255.0, 135.0 / 255.0, 86.0 / 255.0];
const ICE_RIM: [f32; 3] = [120.0 / 255.0, 170.0 / 255.0, 220.0 / 255.0];
const GAS_RIM: [f32; 3] = [210.0 / 255.0, 160.0 / 255.0, 100.0 / 255.0];
const HALO_WARM: [f32; 3] = [1.0, 110.0 / 255.0, 50.0 / 255.0];
const HALO_COLD: [f32; 3] = [140.0 / 255.0, 170.0 / 255.0, 210.0 / 255.0];

enum BarLeft {
    Title(&'static str),
    Back(&'static str),
}

/// A planet for the background, in points (converted to pixels at the end of the frame).
#[allow(clippy::too_many_arguments)]
fn planet(kind: PlanetKind, center: Pos2, radius: f32, rot: f32, dim: f32, alpha: f32, clip: Rect, halo: ([f32; 3], f32, f32)) -> PlanetDraw {
    let rim = match (kind, dim > 0.5) {
        (PlanetKind::Mars, _) => MARS_RIM,
        (PlanetKind::Ice, false) => ICE_RIM,
        (PlanetKind::Gas, false) => GAS_RIM,
        (PlanetKind::Ice, true) => [95.0 / 255.0, 125.0 / 255.0, 165.0 / 255.0],
        (PlanetKind::Gas, true) => [140.0 / 255.0, 112.0 / 255.0, 74.0 / 255.0],
    };
    let ring = if kind == PlanetKind::Gas { 1.0 - 0.6 * dim } else { 0.0 };
    PlanetDraw {
        kind,
        center: glam::Vec2::new(center.x, center.y),
        radius,
        rot,
        dim,
        ring,
        alpha,
        clip: [clip.left(), clip.top(), clip.right(), clip.bottom()],
        rim,
        halo,
    }
}

/// The sphere radius of a ringed planet whose ring fits a box `size` across.
fn ringed_radius(size: f32) -> f32 {
    size / 2.25 / 2.0
}

/// Back and forth over `period` seconds, `amp` points.
fn bob(now: f64, period: f32, amp: f32) -> f32 {
    amp * (0.5 - 0.5 * (PI * now as f32 / period).cos())
}

/// The colours of "PLANET": Mars, the gas giant, the ice world, sliding slowly.
fn title_colour(i: usize, n: usize, now: f64) -> Color32 {
    let u = (i as f32 + 0.5) / n as f32;
    let shift = 0.5 - 0.5 * (PI * now as f32 / 9.0).cos();
    let g = (u * 0.5 + shift * 0.5) * 4.0;
    let stops = [col::PL_MARS, col::PL_GAS, col::PL_ICE, col::PL_GAS, col::PL_MARS];
    let k = (g.floor() as usize).min(3);
    paint::lerp_colour(stops[k], stops[k + 1], g - k as f32)
}

fn thousands(n: u32) -> String {
    if n >= 1000 { format!("{}\u{202F}{:03}", n / 1000, n % 1000) } else { n.to_string() }
}

fn km(m: f32) -> String {
    format!("{:.2}", m / 1000.0).replace('.', ",")
}

fn pointer_moved(ui: &Ui) -> bool {
    ui.input(|i| i.pointer.delta() != Vec2::ZERO)
}

impl Menu {
    #[allow(clippy::too_many_arguments)]
    fn mars_pins(&self, p: &Painter, center: Pos2, radius: f32, rot: f32, alpha: f32, scale: f32, now: f64) {
        for (i, (name, lat, lon)) in PINS.iter().enumerate() {
            let Some((at, vis)) = pin(center, radius, rot, *lat, *lon) else { continue };
            let a = vis * alpha;
            let ph = ((now / 1.7 + i as f64 * 0.37) % 1.0) as f32;
            p.circle_stroke(at, (3.0 + ph * 9.0) * scale, Stroke::new(1.2, fade(col::HUB, a * (1.0 - ph))));
            p.circle_filled(at, 2.4 * scale, fade(col::HUB, a));
            let left = at.x > center.x + 0.35 * radius;
            let (anchor, x) = if left { (Align2::RIGHT_CENTER, at.x - 8.0 * scale) } else { (Align2::LEFT_CENTER, at.x + 8.0 * scale) };
            paint::text(p, pos2(x, at.y), anchor, name, Font::data(9.0 * scale), fade(col::DUST, a * 0.9));
        }
    }

    /// The top bar: a title or a back button on the left, the sound switch on the right.
    /// Returns whether back was pressed.
    fn top_bar(&mut self, ui: &mut Ui, p: &Painter, r: Rect, layout: Layout, muted: bool, left: BarLeft) -> bool {
        let wide = layout == Layout::Wide;
        let (pad, btn, cy) = if wide { (36.0, 44.0, r.top() + 36.0) } else { (14.0, 40.0, r.top() + 34.0) };
        let mut back = false;
        match left {
            BarLeft::Title(s) => {
                let x = r.left() + pad + if wide { 0.0 } else { 6.0 };
                paint::text(p, pos2(x, cy), Align2::LEFT_CENTER, s, Font::label(if wide { 15.0 } else { 14.0 }, 0.2), col::DUST_2);
            }
            BarLeft::Back(s) => {
                let font = Font::label(if wide { 16.0 } else { 15.0 }, 0.1);
                let ts = paint::text_size(p, s, font);
                let rect = Rect::from_min_size(pos2(r.left() + pad - 8.0, cy - 22.0), vec2(ts.x + 46.0, 44.0));
                let resp = self.hit(ui, rect, Id::new(("menu back", s)), true);
                let c = if resp.hovered() { col::DUST } else { col::DUST_2 };
                if resp.hovered() {
                    p.rect_filled(rect, 12.0, col::GLASS);
                }
                paint::icon_at(p, pos2(rect.left() + 18.0, cy), 22.0, Icon::ChevronLeft, c);
                paint::text(p, pos2(rect.left() + 32.0, cy), Align2::LEFT_CENTER, s, font, c);
                back = resp.clicked();
            }
        }
        let sr = Rect::from_min_size(pos2(r.right() - pad - btn, cy - btn / 2.0), vec2(btn, btn));
        let resp = self.hit(ui, sr, Id::new("menu sound"), true);
        paint::panel(p, sr, 12.0, col::GLASS, Some((1.0, col::LINE)));
        let c = if resp.hovered() { col::DUST } else { col::DUST_2 };
        paint::icon_at(p, sr.center(), 20.0, if muted { Icon::SoundOff } else { Icon::SoundOn }, c);
        if resp.clicked() {
            self.requests.push(Request::ToggleMute);
        }
        back
    }

    /// The main button: livery orange with its stripes and a sweeping shine, or a dashed-out
    /// "coming soon" when locked.
    #[allow(clippy::too_many_arguments)]
    fn cta(&mut self, ui: &mut Ui, p: &Painter, rect: Rect, id: &str, label: &str, locked: bool, key: Option<&str>, alpha: f32, now: f64) -> bool {
        let resp = self.hit(ui, rect, Id::new(("menu cta", id)), true);
        let radius = if rect.height() > 58.0 { 15.0 } else { 14.0 };
        let font = Font::label(if rect.height() > 58.0 { 20.0 } else { 19.0 }, 0.14).weight(800.0);
        if locked {
            p.rect_stroke(rect, radius, Stroke::new(1.5, fade(col::LINE, alpha)), StrokeKind::Inside);
            let ts = paint::text_size(p, label, font);
            let x = rect.center().x - (ts.x + 26.0) / 2.0;
            paint::icon_at(p, pos2(x + 8.0, rect.center().y), 16.0, Icon::Lock, fade(col::DUST_3, alpha));
            paint::text(p, pos2(x + 26.0, rect.center().y), Align2::LEFT_CENTER, label, font, fade(col::DUST_3, alpha));
            return resp.clicked();
        }
        let pressed = resp.is_pointer_button_down_on();
        let rect = if pressed { rect.translate(vec2(0.0, 1.0)) } else { rect };
        paint::glow(p, rect, radius, col::LIVERY, alpha * 0.9);
        let fill = if resp.hovered() { Color32::from_rgb(255, 128, 64) } else { col::LIVERY };
        p.rect_filled(rect, radius, fade(fill, alpha));
        let clip = p.with_clip_rect(rect.shrink(0.5));
        paint::stripes(&clip, rect, fade(col::INK_STRIPE, alpha));
        paint::shine(&clip, rect, now);
        let ts = paint::text_size(p, label, font);
        let key = key.filter(|_| !self.touch);
        let key_w = key.map_or(0.0, |k| paint::text_size(p, k, Font::data(11.0)).x.max(14.0) + 14.0 + 12.0);
        let x = rect.center().x - (ts.x + key_w) / 2.0;
        paint::text(p, pos2(x, rect.center().y), Align2::LEFT_CENTER, label, font, fade(col::LIVERY_INK, alpha));
        if let Some(k) = key {
            paint::keycap(p, pos2(x + ts.x + 12.0, rect.center().y), k, fade(col::INK_STRIPE, alpha), None, fade(col::LIVERY_INK, alpha));
        } else {
            paint::icon_at(p, pos2(x + ts.x + 18.0, rect.center().y), 22.0, Icon::ChevronRight, fade(col::LIVERY_INK, alpha));
        }
        resp.clicked()
    }

    // ------------------------------------------------------------------ title

    pub(super) fn title(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64) {
        let p = ui.painter().clone();
        // The planets may run under the screen's notch and home indicator, the text may not.
        let screen = ui.ctx().viewport_rect();
        if ui.interact(r, Id::new("menu title"), Sense::click()).clicked() {
            self.start(now);
        }
        let show = (now as f32 / 1.4).clamp(0.0, 1.0);
        let rot = self.rot;
        let (w, h) = (r.width(), r.height());
        if layout == Layout::Wide {
            let mars_c = r.min + vec2(w - 270.0, h - 250.0);
            let gas_c = r.min + vec2(w - 510.0, 125.0 + bob(now, 9.0, 8.0));
            let ice_c = r.min + vec2(w - 680.0, h - 110.0 + bob(now, 6.5, -7.0));
            self.sky.glow = [r.left() + 0.7 * w, r.top() + 1.15 * h, 0.9 * w, 0.6 * h];
            self.sky.planets.push(planet(PlanetKind::Gas, gas_c, ringed_radius(330.0), rot * 0.9 + 2.0, 0.0, show, screen, ([0.0; 3], 0.0, 0.0)));
            self.sky.planets.push(planet(PlanetKind::Ice, ice_c, 34.0, rot * 1.4 + 0.4, 0.0, show, screen, ([0.0; 3], 0.0, 0.0)));
            self.sky.planets.push(planet(PlanetKind::Mars, mars_c, 310.0, rot * 0.6 - 0.9, 0.0, show, screen, (HALO_WARM, 0.22, 1.94)));
            self.mars_pins(&p, mars_c, 310.0, rot * 0.6 - 0.9, show, 1.35, now);

            let font = Font::display(124.0);
            let at = r.min + vec2(96.0, 140.0);
            let line = 124.0 * 0.86;
            paint::letters(&p, at, "PLANET", font, |i, n| fade(title_colour(i, n, now), show));
            paint::text(&p, at + vec2(0.0, line), Align2::LEFT_TOP, "TRACKS", font, fade(col::DUST, show));
            paint::para(&p, at + vec2(4.0, 2.0 * line + 58.0), 340.0, "À chaque planète son style de conduite particulier", Font::body(22.0), 1.4, fade(col::DUST_2, show));

            let pulse = 0.35 + 0.65 * (0.5 + 0.5 * (std::f32::consts::TAU * now as f32 / 1.8).cos());
            let (devices, press) = if self.touch {
                ("Son activé · casque conseillé", "TOUCHER POUR DÉMARRER")
            } else {
                ("Clavier, souris ou manette · son activé", "APPUIE SUR UNE TOUCHE")
            };
            let sub = paint::text(&p, pos2(r.left() + 96.0, r.bottom() - 88.0), Align2::LEFT_BOTTOM, devices, Font::data(12.0), fade(col::DUST_3, show));
            paint::text(&p, pos2(r.left() + 96.0, sub.top() - 10.0), Align2::LEFT_BOTTOM, press, Font::label(20.0, 0.3), fade(col::DUST, show * pulse));
        } else {
            let s = (w * 1.5).max(h * 0.62);
            let vis = (s * 0.4).min(h * 0.3);
            let top = r.top() + (h * 0.16).max(96.0);
            let mars_c = pos2(r.center().x, r.bottom() - vis + s / 2.0);
            self.sky.glow = [r.center().x, r.top() + 1.12 * h, 1.3 * w, 0.55 * h];
            let gb = (w * 0.62).min(250.0);
            let gas_c = pos2(r.left() + w * 0.9, r.top() + (top - r.top()) * 0.55 + bob(now, 8.5, 7.0));

            let size = (w * 0.17).clamp(52.0, 68.0);
            let font = Font::display(size);
            let line = size * 0.86;
            let word = paint::text_size(&p, "PLANET", font).x;
            paint::letters(&p, pos2(r.center().x - word / 2.0, top), "PLANET", font, |i, n| fade(title_colour(i, n, now), show));
            paint::text(&p, pos2(r.center().x, top + line), Align2::CENTER_TOP, "TRACKS", font, fade(col::DUST, show));
            let tag_w = 240.0f32.min(w - 60.0);
            let tag = "À chaque planète son style de conduite particulier";
            let tag_rect = paint::para_centered(&p, pos2(r.center().x, top + 2.0 * line + 30.0), tag_w, tag, Font::body(15.0), 1.4, fade(col::DUST_2, show));

            let pulse = 0.35 + 0.65 * (0.5 + 0.5 * (std::f32::consts::TAU * now as f32 / 1.8).cos());
            let sub = paint::text(&p, pos2(r.center().x, r.bottom() - vis - 34.0), Align2::CENTER_BOTTOM, "Son activé · casque conseillé", Font::data(10.5), fade(col::DUST_3, show));
            let press = paint::text(&p, pos2(r.center().x, sub.top() - 8.0), Align2::CENTER_BOTTOM, "TOUCHER POUR DÉMARRER", Font::label(17.0, 0.3), fade(col::DUST, show * pulse));

            let (y0, y1) = (tag_rect.bottom(), press.top() - 12.0);
            let is = ((y1 - y0) * 0.5).clamp(36.0, 84.0);
            let ice_c = pos2(r.left() + w * 0.15, (y0 + y1) / 2.0 + bob(now, 6.0, -6.0));
            self.sky.planets.push(planet(PlanetKind::Gas, gas_c, ringed_radius(gb), rot * 0.9 + 2.0, 0.0, show, screen, ([0.0; 3], 0.0, 0.0)));
            self.sky.planets.push(planet(PlanetKind::Ice, ice_c, is / 2.0, rot * 1.4 + 0.4, 0.0, show, screen, ([0.0; 3], 0.0, 0.0)));
            self.sky.planets.push(planet(PlanetKind::Mars, mars_c, s / 2.0, rot * 0.6 - 0.9, 0.0, show, screen, (HALO_WARM, 0.22, 1.3)));
            self.mars_pins(&p, mars_c, s / 2.0, rot * 0.6 - 0.9, show, 1.15, now);
        }
    }

    // ------------------------------------------------------------------ planets

    pub(super) fn planets(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64, muted: bool) {
        let p = ui.painter().clone();
        let wide = layout == Layout::Wide;
        self.top_bar(ui, &p, r, layout, muted, BarLeft::Title("CHOISIS TA PLANÈTE"));
        let (car, info) = if wide {
            (
                Rect::from_min_max(pos2(r.left() + 520.0, r.top() + 72.0), pos2(r.right(), r.bottom() - 56.0)),
                Rect::from_min_max(pos2(r.left() + 80.0, r.top() + 72.0), pos2(r.left() + 500.0, r.bottom() - 56.0)),
            )
        } else {
            let h = self.info_height(&p, r.width() - 40.0, layout) + 14.0 + 56.0;
            let top = r.bottom() - 20.0 - h;
            (
                Rect::from_min_max(pos2(r.left(), r.top() + 60.0), pos2(r.right(), top - 26.0)),
                Rect::from_min_max(pos2(r.left() + 20.0, top), pos2(r.right() - 20.0, r.bottom() - 20.0)),
            )
        };
        self.sky.glow = if wide { [r.left() + 0.7 * r.width(), r.top() + 1.15 * r.height(), 0.9 * r.width(), 0.6 * r.height()] } else { [r.center().x, r.top() + 1.12 * r.height(), 1.3 * r.width(), 0.55 * r.height()] };

        // Carousel: drag, or click a neighbour.
        let slide_w = car.width() * if wide { 0.62 } else { 0.76 };
        let last = PLANETS.len() - 1;
        let resp = ui.interact(car, Id::new("menu carousel"), Sense::click_and_drag());
        if resp.dragged() {
            self.drag = Some(self.drag.unwrap_or(0.0) + resp.drag_delta().x);
        }
        let drag_eff = |d: f32, planet: usize| if (planet == 0 && d > 0.0) || (planet == last && d < 0.0) { d * 0.35 } else { d };
        if resp.drag_stopped() {
            if let Some(d) = self.drag.take() {
                let d = drag_eff(d, self.planet);
                let v = ui.input(|i| i.pointer.velocity().x);
                let here = self.planet as f32 - d / slide_w;
                self.carousel = super::Tween { from: here, to: self.planet as f32, start: now, duration: 0.55 };
                if d < -slide_w * 0.2 || v < -500.0 {
                    self.set_planet(self.planet as isize + 1, now);
                } else if d > slide_w * 0.2 || v > 500.0 {
                    self.set_planet(self.planet as isize - 1, now);
                }
            }
        } else if resp.clicked()
            && let Some(x) = resp.interact_pointer_pos().map(|q| q.x)
        {
            if x < car.center().x - slide_w / 2.0 {
                self.set_planet(self.planet as isize - 1, now);
            } else if x > car.center().x + slide_w / 2.0 {
                self.set_planet(self.planet as isize + 1, now);
            }
        }
        let pos = match self.drag {
            Some(d) => self.planet as f32 - drag_eff(d, self.planet) / slide_w,
            None => self.carousel.at(now),
        };
        let size = (slide_w * if wide { 0.92 } else { 0.9 }).min(car.height() * if wide { 0.84 } else { 0.86 });
        let clip_p = p.with_clip_rect(car);
        for (k, info_k) in PLANETS.iter().enumerate() {
            let d = k as f32 - pos;
            if d.abs() > 1.6 {
                continue;
            }
            let scale = 1.0 - d.abs().min(1.2) * 0.36;
            let alpha = 1.0 - d.abs().min(1.0) * 0.45;
            let c = pos2(car.center().x + d * slide_w, car.center().y);
            let dim = if info_k.open { 0.0 } else { 1.0 };
            let rot = if info_k.open { self.rot } else { self.rot * 0.6 + k as f32 * 1.7 };
            let (radius, box_) = match info_k.kind {
                PlanetKind::Gas => (ringed_radius(size * 1.3) * scale, size * 1.3 * scale),
                _ => (size / 2.0 * scale, size * scale),
            };
            let halo = if info_k.open { (HALO_WARM, 0.22, 1.32) } else { (HALO_COLD, 0.08, 1.32) };
            self.sky.planets.push(planet(info_k.kind, c, radius, rot, dim, alpha, car, halo));
            if info_k.open {
                if d.abs() < 0.6 {
                    self.mars_pins(&clip_p, c, radius, rot, 1.0 - d.abs() / 0.6, if wide { 1.25 } else { 1.0 }, now);
                }
            } else {
                let cy = c.y + if info_k.kind == PlanetKind::Gas { radius * 1.25 } else { box_ * 0.26 } - 4.0;
                let font = Font::label(if wide { 13.0 } else { 12.0 }, 0.2);
                paint::pill(&clip_p, pos2(c.x, cy), "BIENTÔT", font, fade(col::DUST_2, alpha), Some(fade(col::VOID, alpha * 0.7)), fade(col::LINE, alpha), if wide { 32.0 } else { 28.0 }, Some(Icon::Lock));
            }
        }
        // Arrows and dots.
        let ab = if wide { 52.0 } else { 42.0 };
        for (dir, x) in [(-1isize, car.left() + if wide { 18.0 } else { 10.0 } + ab / 2.0), (1, car.right() - if wide { 28.0 } else { 10.0 } - ab / 2.0)] {
            let rect = Rect::from_center_size(pos2(x, car.center().y), vec2(ab, ab));
            let enabled = if dir < 0 { self.planet > 0 } else { self.planet < last };
            let a = if enabled { 1.0 } else { 0.2 };
            let hovered = enabled && {
                let resp = self.hit(ui, rect, Id::new(("menu arrow", dir)), true);
                if resp.clicked() {
                    self.set_planet(self.planet as isize + dir, now);
                }
                resp.hovered()
            };
            p.circle_filled(rect.center(), ab / 2.0, fade(col::GLASS, a));
            p.circle_stroke(rect.center(), ab / 2.0 - 0.5, Stroke::new(1.0, fade(col::LINE, a)));
            let c = if hovered { col::DUST } else { col::DUST_2 };
            paint::icon_at(&p, rect.center(), 22.0, if dir < 0 { Icon::ChevronLeft } else { Icon::ChevronRight }, fade(c, a));
        }
        let (dw, don, gap) = if wide { (26.0, 40.0, 9.0) } else { (22.0, 34.0, 8.0) };
        let total: f32 = (0..PLANETS.len()).map(|k| if k == self.planet { don } else { dw }).sum::<f32>() + gap * (PLANETS.len() - 1) as f32;
        let mut x = car.center().x - total / 2.0;
        let y = if wide { car.bottom() - 16.0 } else { car.bottom() + 13.0 };
        for (k, info_k) in PLANETS.iter().enumerate() {
            let wk = if k == self.planet { don } else { dw };
            let c = if k != self.planet { col::LINE } else if info_k.open { col::LIVERY } else { col::DUST_3 };
            p.rect_filled(Rect::from_min_size(pos2(x, y - 2.0), vec2(wk, 4.0)), 2.0, c);
            x += wk + gap;
        }

        // The planet's card, crossfading on a change.
        let (shown, prev, t0, dir) = self.info;
        let t = (now - t0) as f32;
        let (k, a, dx) = if t < 0.13 {
            (prev, 1.0 - t / 0.13, -dir * 22.0 * (t / 0.13))
        } else {
            let e = paint::ease_out((t - 0.13) / 0.28);
            (shown, e, dir * 22.0 * (1.0 - e))
        };
        let (ea, edy) = self.enter(0, now);
        let block_h = self.info_height(&p, info.width(), layout);
        let cta_h = if wide { 60.0 } else { 56.0 };
        let top = if wide { info.center().y - (block_h + 22.0 + cta_h) / 2.0 } else { info.top() };
        self.info_block(&p, pos2(info.left() + dx, top + edy), info.width(), layout, k, a * ea, now);
        let (ca, cdy) = self.enter(1, now);
        let cta_top = if wide { top + block_h + 22.0 } else { info.bottom() - cta_h };
        let open = PLANETS[self.planet].open;
        let cta = Rect::from_min_size(pos2(info.left() + self.shake_x(Shake::Cta, now), cta_top + cdy), vec2(info.width(), cta_h));
        let label = if open { "PILOTER SUR MARS" } else { "BIENTÔT DISPONIBLE" };
        if self.cta(ui, &p, cta, "planet", label, !open, wide.then_some("Entrée"), ca, now) {
            self.choose_planet(now);
        }
    }

    fn info_height(&self, p: &Painter, width: f32, layout: Layout) -> f32 {
        let wide = layout == Layout::Wide;
        let (name, lore_size, lore_line, stats, gap) = if wide { (104.0, 17.0, 1.5, 66.0, 14.0) } else { ((width * 0.17).clamp(52.0, 66.0), 15.0, 1.45, 52.0, 12.0) };
        let lore_w = if wide { width.min(400.0) } else { width };
        let lore = PLANETS
            .iter()
            .map(|pl| paint::para_height(p, lore_w, pl.lore, Font::body(lore_size), lore_line))
            .fold(lore_size * lore_line * 3.0, f32::max);
        26.0 + gap + name * 0.92 + gap + lore + gap + stats + gap + 20.0
    }

    #[allow(clippy::too_many_arguments)]
    fn info_block(&self, p: &Painter, at: Pos2, width: f32, layout: Layout, k: usize, a: f32, now: f64) {
        let wide = layout == Layout::Wide;
        let pl = &PLANETS[k];
        let gap = if wide { 14.0 } else { 12.0 };
        let mut y = at.y;
        // Eyebrow and status.
        paint::text(p, pos2(at.x, y + 13.0), Align2::LEFT_CENTER, &format!("PLANÈTE {} / {}", k + 1, PLANETS.len()), Font::label(if wide { 13.0 } else { 12.0 }, 0.22), fade(col::DUST_3, a));
        let status = if pl.open { "DISPONIBLE" } else { "BIENTÔT" };
        let font = Font::label(if wide { 12.0 } else { 11.0 }, 0.18);
        let ts = paint::text_size(p, status, font);
        let ph = if wide { 26.0 } else { 24.0 };
        let (tc, sc) = if pl.open { (col::HUB, Color32::from_rgba_premultiplied(38, 99, 115, 115)) } else { (col::DUST_3, col::LINE) };
        paint::pill(p, pos2(at.x + width - (ts.x + ph * 0.95) / 2.0, y + 13.0), status, font, fade(tc, a), None, fade(sc, a), ph, None);
        y += 26.0 + gap;
        // Name and year.
        let size = if wide { 104.0 } else { (width * 0.17).clamp(52.0, 66.0) };
        let font = Font::display(size);
        let name_c = if pl.open { col::DUST } else { col::DUST_3 };
        let nr = paint::text(p, pos2(at.x - size * 0.03, y - size * 0.3), Align2::LEFT_TOP, pl.name, font, fade(name_c, a));
        let year_c = if pl.open { col::LIVERY } else { col::DUST_3 };
        let ys = if wide { 24.0 } else { 18.0 };
        paint::text(p, pos2(nr.right() + 12.0, y + size * 0.86), Align2::LEFT_BOTTOM, pl.year, Font::data(ys).weight(600.0), fade(year_c, a));
        y += size * 0.92 + gap;
        // Story.
        let (ls, ll) = if wide { (17.0, 1.5) } else { (15.0, 1.45) };
        let lore_w = if wide { width.min(400.0) } else { width };
        let lr = paint::para(p, pos2(at.x, y), lore_w, pl.lore, Font::body(ls), ll, fade(col::DUST_2, a));
        y += lr.height().max(ls * ll * 3.0) + gap;
        // Handling.
        let sh = if wide { 66.0 } else { 52.0 };
        let box_ = Rect::from_min_size(pos2(at.x, y), vec2(width, sh));
        let radius = if wide { 14.0 } else { 12.0 };
        p.rect_filled(box_, radius, fade(col::PANEL, a));
        p.rect_stroke(box_, radius, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
        let cw = width / 3.0;
        for i in 1..3 {
            p.vline(box_.left() + i as f32 * cw, box_.y_range(), Stroke::new(1.0, fade(col::LINE, a)));
        }
        for (i, (label, stat)) in pl.stats.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(box_.left() + i as f32 * cw, box_.top()), vec2(cw, sh));
            let lx = cell.left() + if wide { 14.0 } else { 11.0 };
            paint::text(p, pos2(lx, cell.top() + if wide { 12.0 } else { 9.0 }), Align2::LEFT_TOP, label, Font::label(if wide { 12.0 } else { 10.5 }, 0.14), fade(col::DUST_3, a));
            let vy = cell.top() + if wide { 43.0 } else { 35.0 };
            match stat {
                Stat::Text(s) => {
                    paint::text(p, pos2(lx, vy), Align2::LEFT_CENTER, s, Font::heading(if wide { 22.0 } else { 18.0 }), fade(col::DUST, a));
                }
                Stat::Pips(n) => paint::pips(p, pos2(lx + 2.0, vy), *n, if wide { 13.0 } else { 11.0 }, if wide { 8.0 } else { 7.0 }, a),
            }
        }
        y += sh + gap;
        // Pilots online (made up for now).
        let dot = pos2(at.x + 5.0, y + 10.0);
        let fs = if wide { 13.0 } else { 12.0 };
        if pl.open {
            let ph = ((now / 2.0) % 1.0) as f32;
            p.circle_filled(dot, 4.5 + ph * 10.0, fade(col::HUB, a * 0.45 * (1.0 - ph)));
            p.circle_filled(dot, 4.5, fade(col::HUB, a));
            let n = paint::text(p, pos2(at.x + 19.0, y + 10.0), Align2::LEFT_CENTER, &thousands(self.online), Font::data(fs).weight(600.0), fade(col::DUST, a));
            paint::text(p, pos2(n.right() + 8.0, y + 10.0), Align2::LEFT_CENTER, "pilotes sur Mars", Font::data(fs), fade(col::DUST_2, a));
        } else {
            p.circle_filled(dot, 4.5, fade(col::DUST_3, a));
            paint::text(p, pos2(at.x + 19.0, y + 10.0), Align2::LEFT_CENTER, "Aucun pilote pour l'instant", Font::data(fs), fade(col::DUST_2, a));
        }
    }

    // ------------------------------------------------------------------ modes

    pub(super) fn modes(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64, muted: bool) {
        let p = ui.painter().clone();
        let wide = layout == Layout::Wide;
        if self.top_bar(ui, &p, r, layout, muted, BarLeft::Back("PLANÈTES")) {
            self.back(now);
        }
        self.sky.glow = [r.center().x, r.top() + 1.15 * r.height(), 1.1 * r.width(), 0.6 * r.height()];
        let (ea, edy) = self.enter(0, now);
        let head = if wide { r.min + vec2(80.0, 88.0 + edy) } else { r.min + vec2(22.0, 66.0 + edy) };
        paint::text(&p, head, Align2::LEFT_TOP, "MARS 2036", Font::label(if wide { 13.0 } else { 12.0 }, 0.22), fade(col::DUST_3, ea));
        let h2 = if wide { 50.0 } else { 34.0 };
        let hr = paint::text(&p, head + vec2(-2.0, if wide { 20.0 } else { 18.0 }), Align2::LEFT_TOP, "MODE DE JEU", Font::heading(h2), fade(col::DUST, ea));
        let rects: [Rect; 2] = if wide {
            let area = Rect::from_min_max(pos2(r.left() + 80.0, r.top() + 196.0), pos2(r.right() - 80.0, r.bottom() - 92.0));
            let w = (area.width() - 26.0) / 2.0;
            [Rect::from_min_size(area.min, vec2(w, area.height())), Rect::from_min_size(area.min + vec2(w + 26.0, 0.0), vec2(w, area.height()))]
        } else {
            let top = hr.bottom() + 18.0;
            let h = ((r.bottom() - 22.0 - top - 14.0) / 2.0).clamp(150.0, 300.0);
            let x = r.left() + 16.0;
            let w = r.width() - 32.0;
            [Rect::from_min_size(pos2(x, top), vec2(w, h)), Rect::from_min_size(pos2(x, top + h + 14.0), vec2(w, h))]
        };
        for (m, rect) in rects.into_iter().enumerate() {
            let (a, dy) = self.enter(1 + m, now);
            self.mode_card(ui, &p, rect.translate(vec2(0.0, dy)), m, layout, a, now);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mode_card(&mut self, ui: &mut Ui, p: &Painter, rect: Rect, m: usize, layout: Layout, a: f32, now: f64) {
        let wide = layout == Layout::Wide;
        let solo = m == 0;
        let resp = self.hit(ui, rect, Id::new(("menu mode", m)), false);
        if resp.hovered() && pointer_moved(ui) {
            self.set_mode(m);
        }
        if resp.clicked() {
            self.choose_mode(m, now);
        }
        let sel = self.mode == m;
        let lift = if wide && sel { -4.0 } else { 0.0 };
        let rect = rect.translate(vec2(if solo { 0.0 } else { self.shake_x(Shake::Multi, now) }, lift));
        let a = a * if sel { 1.0 } else { 0.72 };
        let radius = if wide { 26.0 } else { 22.0 };
        if sel && solo {
            paint::glow(p, rect, radius, col::LIVERY, a * 0.7);
        }
        p.rect_filled(rect, radius, fade(col::PANEL_2, a));
        // A darker lower half, for depth.
        p.rect_filled(Rect::from_min_max(pos2(rect.left(), rect.center().y), rect.max), CornerRadius { nw: 0, ne: 0, sw: radius as u8, se: radius as u8 }, fade(Color32::from_rgba_premultiplied(5, 3, 3, 40), a));
        // The circuit plan in the corner, a dash running along it (several for multiplayer).
        let deco = if wide { 400.0 } else { 230.0 };
        let (dx, dy) = if wide { (50.0, -56.0) } else { (30.0, -24.0) };
        let box_ = Rect::from_min_size(pos2(rect.right() - deco + dx, rect.top() + dy), vec2(deco, deco));
        let clip = p.with_clip_rect(rect.shrink(1.0).intersect(p.clip_rect()));
        let track = &self.tracks[if solo { 0 } else { 2.min(self.tracks.len() - 1) }];
        let pts: Vec<Pos2> = track.route.iter().map(|(q, _)| box_.min + vec2(q.x, q.y) * box_.size()).collect();
        let base_c = if solo { col::LIVERY } else { col::DUST_3 };
        clip.add(egui::Shape::line(pts.clone(), egui::epaint::PathStroke::new(deco * 0.024, fade(base_c, a * 0.32))));
        let runners: &[(Color32, f64, f64)] = if solo { &[(col::DUST, 6.0, 0.0)] } else { &[(col::DUST_2, 6.2, 0.0), (col::DUST_3, 6.8, 1.4), (col::DUST_3, 7.4, 2.9)] };
        for &(c, period, offset) in runners {
            let ph = (((now + offset) / period) % 1.0) as f32;
            let seg = sub_path(&pts, ph, (ph + 0.06).min(1.0));
            if seg.len() >= 2 {
                clip.add(egui::Shape::line(seg, egui::epaint::PathStroke::new(deco * 0.036, fade(c, a))));
            }
        }
        let stroke = match (sel, solo) {
            (true, true) => (2.0, col::LIVERY),
            (true, false) => (2.0, col::DUST_3),
            _ => (1.0, col::LINE),
        };
        p.rect_stroke(rect, radius, Stroke::new(stroke.0, fade(stroke.1, a)), StrokeKind::Inside);
        let pad = if wide { 34.0 } else { 20.0 };
        if solo {
            let gs = if wide { 60.0 } else { 46.0 };
            let inset = if wide { 28.0 } else { 18.0 };
            let c = pos2(rect.right() - inset - gs / 2.0, rect.top() + inset + gs / 2.0);
            p.circle_filled(c, gs / 2.0, fade(col::LIVERY, a));
            paint::icon_at(p, c, 24.0, Icon::ChevronRight, fade(col::LIVERY_INK, a));
        } else {
            let inset = if wide { 30.0 } else { 20.0 };
            let font = Font::label(if wide { 12.0 } else { 11.0 }, 0.18);
            let ts = paint::text_size(p, "BIENTÔT", font);
            let ph = if wide { 26.0 } else { 24.0 };
            paint::pill(p, pos2(rect.right() - inset - (ts.x + ph * 0.95) / 2.0, rect.top() + inset + ph / 2.0), "BIENTÔT", font, fade(col::DUST_3, a), None, fade(col::LINE, a), ph, None);
        }
        let (title, desc, foot): (&str, &str, &[&str]) = if solo {
            ("SOLO", "Contre la montre. Bats les médailles, circuit après circuit.", &["3 circuits jouables", "0 / 9 médailles"])
        } else {
            ("MULTIJOUEUR", "Course en ligne contre les pilotes de la colonie.", &["En préparation"])
        };
        let foot_font = Font::data(if wide { 12.0 } else { 11.0 });
        let mut y = rect.bottom() - pad;
        let mut x = rect.left() + pad;
        for f in foot {
            let fr = paint::text(p, pos2(x, y), Align2::LEFT_BOTTOM, f, foot_font, fade(col::DUST_3, a));
            x = fr.right() + 18.0;
        }
        y -= if wide { 26.0 } else { 22.0 };
        let ds = if wide { 18.0 } else { 15.0 };
        let dw = if wide { 360.0 } else { 260.0f32.min(rect.width() - 2.0 * pad) };
        let dh = paint::para_height(p, dw, desc, Font::body(ds), 1.45);
        paint::para(p, pos2(rect.left() + pad, y - dh), dw, desc, Font::body(ds), 1.45, fade(col::DUST_2, a));
        y -= dh + 8.0;
        let ts = if wide { 68.0 } else { 42.0 };
        let tc = if solo { col::DUST } else { col::DUST_2 };
        paint::text(p, pos2(rect.left() + pad - 3.0, y + ts * 0.28), Align2::LEFT_BOTTOM, title, Font::heading(ts), fade(tc, a));
    }

    // ------------------------------------------------------------------ solo

    pub(super) fn solo(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64, muted: bool, bests: &[Option<u32>]) {
        let p = ui.painter().clone();
        let wide = layout == Layout::Wide;
        let sheet_open = self.sheet.is_some();
        if self.top_bar(ui, &p, r, layout, muted, BarLeft::Back("MODES")) && !sheet_open {
            self.back(now);
        }
        self.sky.glow = [r.center().x, r.top() + 1.15 * r.height(), 1.1 * r.width(), 0.6 * r.height()];
        let (x0, w) = if wide { (r.left() + 64.0, 540.0) } else { (r.left() + 16.0, r.width() - 32.0) };
        // Heading.
        let (ea, edy) = self.enter(0, now);
        let head = pos2(if wide { x0 } else { r.left() + 22.0 }, r.top() + if wide { 82.0 } else { 66.0 } + edy);
        paint::text(&p, head, Align2::LEFT_TOP, "MARS 2036 · SOLO", Font::label(if wide { 13.0 } else { 12.0 }, 0.22), fade(col::DUST_3, ea));
        let hr = paint::text(&p, head + vec2(-2.0, if wide { 20.0 } else { 18.0 }), Align2::LEFT_TOP, "CIRCUITS", Font::heading(if wide { 44.0 } else { 34.0 }), fade(col::DUST, ea));
        // Series tabs.
        let (ta, tdy) = self.enter(1, now);
        let tabs = Rect::from_min_size(pos2(x0, hr.bottom() + 16.0 + tdy), vec2(w, if wide { 56.0 } else { 54.0 }));
        p.rect_filled(tabs, 15.0, fade(col::PANEL, ta));
        p.rect_stroke(tabs, 15.0, Stroke::new(1.0, fade(col::LINE, ta)), StrokeKind::Inside);
        let tw = (tabs.width() - 16.0) / 2.0;
        for (i, (series, label, count)) in [(Series::Easy, "FACILE", "3/5"), (Series::Hard, "DUR", "0/5")].into_iter().enumerate() {
            let tr = Rect::from_min_size(tabs.min + vec2(5.0 + i as f32 * (tw + 6.0), 5.0), vec2(tw, tabs.height() - 10.0));
            let resp = if sheet_open { None } else { Some(self.hit(ui, tr, Id::new(("menu tab", i)), true)) };
            if resp.as_ref().is_some_and(|r| r.clicked()) {
                self.set_series(series, now);
            }
            let on = self.series == series;
            let (c, edge) = if series == Series::Easy { (col::EASY, None) } else { (col::HARD, Some(col::HARD_EDGE)) };
            if on {
                p.rect_filled(tr, 11.0, fade(col::PANEL_2, ta));
                p.rect_filled(Rect::from_min_max(pos2(tr.left() + 8.0, tr.bottom() - 2.0), pos2(tr.right() - 8.0, tr.bottom())), 1.0, fade(edge.unwrap_or(c), ta));
            }
            let font = Font::label(if wide { 17.0 } else { 16.0 }, 0.14).weight(800.0);
            let lw = paint::text_size(&p, label, font).x;
            let cw = paint::text_size(&p, count, Font::data(11.0)).x;
            let total = 14.0 + 10.0 + lw + 9.0 + cw;
            let mut x = tr.center().x - total / 2.0;
            let flag = Rect::from_min_size(pos2(x, tr.center().y - 7.0), vec2(14.0, 14.0));
            p.rect_filled(flag, 3.0, fade(c, ta));
            if let Some(e) = edge {
                p.rect_stroke(flag, 3.0, Stroke::new(1.5, fade(e, ta)), StrokeKind::Inside);
            }
            x += 24.0;
            let hovered = resp.is_some_and(|r| r.hovered());
            let tc = if on || hovered { col::DUST } else { col::DUST_3 };
            paint::text(&p, pos2(x, tr.center().y), Align2::LEFT_CENTER, label, font, fade(tc, ta));
            paint::text(&p, pos2(x + lw + 9.0, tr.center().y + 1.0), Align2::LEFT_CENTER, count, Font::data(11.0), fade(col::DUST_3, ta));
        }
        // Series line: name and medals won.
        let (sa, sdy) = self.enter(2, now);
        let sy = tabs.bottom() + if wide { 24.0 } else { 26.0 } + sdy;
        let (won, total) = self.medal_count(bests);
        paint::text(&p, pos2(x0 + 4.0, sy), Align2::LEFT_CENTER, if self.series == Series::Easy { "SÉRIE FACILE" } else { "SÉRIE DUR" }, Font::label(13.0, 0.14), fade(col::DUST_3, sa));
        let medals_text = if total > 0 { format!("{won} / {total} médailles") } else { "aucun circuit pour l'instant".into() };
        paint::text(&p, pos2(x0 + w - 4.0, sy), Align2::RIGHT_CENTER, &medals_text, Font::data(11.0), fade(col::DUST_3, sa));
        // The circuits.
        let row_h = if wide { 70.0 } else { 74.0 };
        let mut y = sy + 18.0;
        for i in 0..SLOTS {
            let (a, dy) = self.enter(3 + i, now);
            let rect = Rect::from_min_size(pos2(x0 + self.shake_x(Shake::Row(i), now), y + dy), vec2(w, row_h));
            self.row(ui, &p, rect, i, layout, bests, a, now, sheet_open);
            y += row_h + 8.0;
        }
        if wide {
            let (da, ddy) = self.enter(2, now);
            let rect = Rect::from_min_max(pos2(r.left() + 640.0, r.top() + 80.0 + ddy), pos2(r.right() - 64.0, r.bottom() - 64.0 + ddy));
            self.detail(ui, &p, rect, bests, da, now);
        } else if self.sheet.is_some() {
            self.sheet_ui(ui, r, bests, now);
        }
    }

    /// Medals won in the current series, out of those that can be.
    fn medal_count(&self, bests: &[Option<u32>]) -> (usize, usize) {
        let mut won = 0;
        let mut total = 0;
        for i in 0..SLOTS {
            if let Some(t) = self.slot(i) {
                total += 3;
                if let Some(b) = bests.get(t.map).copied().flatten() {
                    won += t.medal_ticks().iter().filter(|&&m| b <= m).count();
                }
            }
        }
        (won, total)
    }

    fn series_colours(&self) -> (Color32, Option<Color32>, Color32) {
        match self.series {
            Series::Easy => (col::EASY, None, col::VOID),
            Series::Hard => (col::HARD, Some(col::HARD_EDGE), col::DUST),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, ui: &mut Ui, p: &Painter, rect: Rect, i: usize, layout: Layout, bests: &[Option<u32>], a: f32, now: f64, blocked: bool) {
        let wide = layout == Layout::Wide;
        if !blocked {
            let resp = self.hit(ui, rect, Id::new(("menu row", i)), false);
            if wide && resp.hovered() && pointer_moved(ui) {
                self.set_sel(i, now, true);
            }
            if resp.clicked() {
                if wide {
                    self.set_sel(i, now, false);
                    self.launch(now);
                } else {
                    self.open_sheet(i, now);
                }
            }
        }
        let (c, edge, ink) = self.series_colours();
        let sel = self.sel == i && (wide || self.sheet.is_none());
        let track = self.slot(i).map(|t| (t.name.clone(), t.length, t.checkpoints, t.dirt, t.map, t.medal_ticks()));
        let locked = track.is_none();
        if locked {
            if sel {
                p.rect_filled(rect, 16.0, fade(col::GLASS, a));
            }
            p.rect_stroke(rect, 16.0, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
        } else {
            p.rect_filled(rect, 16.0, fade(if sel { col::PANEL_2 } else { col::PANEL }, a));
            let s = if sel { (1.5, edge.unwrap_or(c)) } else { (1.0, col::LINE) };
            p.rect_stroke(rect, 16.0, Stroke::new(s.0, fade(s.1, a)), StrokeKind::Inside);
        }
        let ns = if wide { 40.0 } else { 34.0 };
        let num = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - ns / 2.0), vec2(ns, ns));
        let na = if locked { a * 0.4 } else { a };
        p.rect_filled(num, if wide { 10.0 } else { 9.0 }, fade(c, na));
        if let Some(e) = edge {
            p.rect_stroke(num, if wide { 10.0 } else { 9.0 }, Stroke::new(1.5, fade(e, na)), StrokeKind::Inside);
        }
        paint::text(p, num.center(), Align2::CENTER_CENTER, &format!("{:02}", i + 1), Font::heading(if wide { 18.0 } else { 16.0 }), fade(ink, na));
        let th = if wide { 56.0 } else { 52.0 };
        let gap = if wide { 14.0 } else { 11.0 };
        let thumb = Rect::from_min_size(pos2(num.right() + gap, rect.center().y - th / 2.0), vec2(th, th));
        let tx = thumb.right() + gap;
        match &track {
            Some((name, length, cps, dirt, map, medals)) => {
                p.rect_filled(thumb, 12.0, fade(col::VOID, a));
                p.rect_stroke(thumb, 12.0, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
                let ti = self.slot_index(i).unwrap_or(0);
                paint::route(p, thumb.shrink(th * 0.1), &self.tracks[ti], th * 0.058, 1.0, false, a);
                let top = rect.top() + if wide { 9.0 } else { 11.0 };
                paint::text(p, pos2(tx, top), Align2::LEFT_TOP, name, Font::heading(if wide { 22.0 } else { 20.0 }), fade(col::DUST, a));
                paint::text(p, pos2(tx, top + if wide { 29.0 } else { 27.0 }), Align2::LEFT_TOP, &format!("{} km · {} CP", km(*length), cps), Font::label(13.0, 0.04), fade(col::DUST_3, a));
                let my = top + if wide { 52.0 } else { 51.0 };
                let bw = if wide { 56.0 } else { 48.0 };
                let road_w = (bw - 2.0) * (1.0 - dirt);
                if road_w > 0.5 {
                    p.rect_filled(Rect::from_min_size(pos2(tx, my - 2.0), vec2(road_w, 4.0)), 2.0, fade(col::ROAD, a));
                }
                if *dirt > 0.0 {
                    p.rect_filled(Rect::from_min_max(pos2(tx + road_w + 2.0, my - 2.0), pos2(tx + bw, my + 2.0)), 2.0, fade(col::DIRT, a));
                }
                paint::text(p, pos2(tx + bw + 8.0, my), Align2::LEFT_CENTER, &format!("{}\u{202F}% dirt", (dirt * 100.0).round()), Font::label(11.0, 0.06), fade(col::DUST_3, a));
                // Medals won and the best time.
                let best = bests.get(*map).copied().flatten();
                let ms = if wide { 13.0 } else { 12.0 };
                let right = rect.right() - if wide { 18.0 } else { 12.0 };
                for (k, mc) in paint::MEDAL_COLOURS.iter().enumerate() {
                    let cx = right - ms / 2.0 - (2 - k) as f32 * (ms + 5.0);
                    let cy = rect.center().y - 9.0;
                    let won = best.is_some_and(|b| b <= medals[k]);
                    if won {
                        p.circle_filled(pos2(cx, cy), ms / 2.0, fade(*mc, a));
                    } else {
                        p.circle_stroke(pos2(cx, cy), ms / 2.0 - 0.75, Stroke::new(1.5, fade(*mc, a * 0.55)));
                    }
                }
                let bt = best.map_or("--:--.--".to_string(), format_time);
                paint::text(p, pos2(right, rect.center().y + 12.0), Align2::RIGHT_CENTER, &bt, Font::data(11.0), fade(if best.is_some() { col::DUST_2 } else { col::DUST_3 }, a));
            }
            None => {
                paint::dashed_rect(p, thumb, fade(col::LINE, a));
                paint::icon_at(p, thumb.center(), 15.0, Icon::Lock, fade(col::DUST_3, a));
                let hard = self.series == Series::Hard;
                let top = rect.center().y - if wide { 20.0 } else { 19.0 };
                paint::text(p, pos2(tx, top), Align2::LEFT_TOP, if hard { "Verrouillé" } else { "À venir" }, Font::heading(if wide { 22.0 } else { 20.0 }), fade(col::DUST_3, a));
                paint::text(p, pos2(tx, top + if wide { 29.0 } else { 27.0 }), Align2::LEFT_TOP, if hard { "Termine la série Facile" } else { "Circuit en construction" }, Font::label(13.0, 0.04), fade(col::DUST_3, a));
                paint::icon_at(p, pos2(rect.right() - 24.0, rect.center().y), 15.0, Icon::Lock, fade(col::DUST_3, a));
            }
        }
    }

    fn slot_index(&self, i: usize) -> Option<usize> {
        match self.series {
            Series::Easy => (i < self.tracks.len() && i < SLOTS).then_some(i),
            Series::Hard => None,
        }
    }

    /// Wide layout: the selected circuit's card, beside the list.
    fn detail(&mut self, ui: &mut Ui, p: &Painter, rect: Rect, bests: &[Option<u32>], a: f32, now: f64) {
        p.rect_filled(rect, 24.0, fade(col::PANEL, a));
        p.rect_stroke(rect, 24.0, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
        let inner = rect.shrink2(vec2(28.0, 24.0));
        let since = (now - self.sel_at) as f32;
        let a = a * (0.25 + 0.75 * paint::ease_out(since / 0.22));
        let (c, edge, _) = self.series_colours();
        let series = if self.series == Series::Easy { "FACILE" } else { "DUR" };
        let chip = format!("{series} · {:02}", self.sel + 1);
        let flag = Rect::from_min_size(pos2(inner.left(), inner.top() + 5.0), vec2(14.0, 14.0));
        p.rect_filled(flag, 3.0, fade(c, a));
        if let Some(e) = edge {
            p.rect_stroke(flag, 3.0, Stroke::new(1.5, fade(e, a)), StrokeKind::Inside);
        }
        paint::text(p, pos2(flag.right() + 8.0, flag.center().y), Align2::LEFT_CENTER, &chip, Font::label(13.0, 0.16), fade(col::DUST_2, a));
        let Some(ti) = self.slot_index(self.sel) else {
            let hard = self.series == Series::Hard;
            let cy = inner.center().y - 20.0;
            paint::icon_at(p, pos2(inner.center().x, cy - 70.0), 44.0, Icon::Lock, fade(col::DUST_3, a));
            paint::text(p, pos2(inner.center().x, cy), Align2::CENTER_CENTER, if hard { "VERROUILLÉ" } else { "À VENIR" }, Font::heading(52.0), fade(col::DUST_3, a));
            paint::text(p, pos2(inner.center().x, cy + 50.0), Align2::CENTER_CENTER, self.locked_text(), Font::body(16.0), fade(col::DUST_2, a));
            return;
        };
        let t = &self.tracks[ti];
        let best = bests.get(t.map).copied().flatten();
        let rec = paint::text(p, pos2(inner.right(), flag.center().y), Align2::RIGHT_CENTER, &best.map_or("--:--.--".into(), format_time), Font::data(13.0), fade(col::DUST_2, a));
        paint::text(p, pos2(rec.left() - 10.0, flag.center().y), Align2::RIGHT_CENTER, "TON RECORD", Font::label(13.0, 0.14), fade(col::DUST_3, a));
        let name = paint::text(p, pos2(inner.left() - 2.0, inner.top() + 34.0), Align2::LEFT_TOP, &t.name.to_uppercase(), Font::heading(52.0), fade(col::DUST, a));
        let desc = paint::para(p, pos2(inner.left(), name.bottom() - 6.0), inner.width().min(470.0), t.desc, Font::body(16.0), 1.45, fade(col::DUST_2, a));
        let mid = desc.bottom() + 16.0;
        let map = Rect::from_min_size(pos2(inner.left(), mid), vec2(250.0, 250.0));
        p.rect_filled(map, 18.0, fade(col::VOID, a));
        paint::grid(p, map.shrink(1.0), a);
        p.rect_stroke(map, 18.0, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
        let progress = ((since - 0.12) / 1.1).clamp(0.0, 1.0);
        paint::route(p, map.shrink(16.0), t, 8.6, progress, true, a);
        // Numbers and medals beside the plan.
        let sx = map.right() + 22.0;
        let sw = inner.right() - sx;
        let cw = (sw - 8.0) / 2.0;
        let stats = [
            (km(t.length), "km", "LONGUEUR"),
            (t.checkpoints.to_string(), "", "CHECKPOINTS"),
            (format!("{}", (t.dirt * 100.0).round()), "%", "DIRT"),
            (t.jumps.to_string(), "", if t.jumps > 1 { "SAUTS" } else { "SAUT" }),
        ];
        for (k, (v, unit, label)) in stats.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(sx + (k % 2) as f32 * (cw + 8.0), mid + (k / 2) as f32 * 58.0), vec2(cw, 50.0));
            p.rect_filled(cell, 12.0, fade(col::VOID, a));
            p.rect_stroke(cell, 12.0, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
            let vr = paint::text(p, pos2(cell.left() + 12.0, cell.top() + 17.0), Align2::LEFT_CENTER, v, Font::data(16.0).weight(600.0), fade(col::DUST, a));
            if !unit.is_empty() {
                paint::text(p, pos2(vr.right() + 3.0, cell.top() + 19.0), Align2::LEFT_CENTER, unit, Font::data(10.0), fade(col::DUST_3, a));
            }
            paint::text(p, pos2(cell.left() + 12.0, cell.top() + 37.0), Align2::LEFT_CENTER, label, Font::label(11.0, 0.1), fade(col::DUST_3, a));
        }
        let my = mid + 2.0 * 58.0 + 8.0;
        paint::text(p, pos2(sx, my + 8.0), Align2::LEFT_CENTER, "MÉDAILLES", Font::label(13.0, 0.16), fade(col::DUST_2, a));
        paint::text(p, pos2(inner.right(), my + 8.0), Align2::RIGHT_CENTER, "temps provisoires", Font::data(10.0), fade(col::DUST_3, a));
        for (k, ((label, _), ticks)) in MEDALS.iter().zip(t.medal_ticks()).enumerate() {
            let row = Rect::from_min_size(pos2(sx, my + 22.0 + k as f32 * 40.0), vec2(sw, 34.0));
            let won = best.is_some_and(|b| b <= ticks);
            p.rect_filled(row, 10.0, fade(col::VOID, a));
            p.rect_stroke(row, 10.0, Stroke::new(1.0, fade(if won { paint::MEDAL_COLOURS[k] } else { col::LINE }, a)), StrokeKind::Inside);
            let cc = pos2(row.left() + 18.0, row.center().y);
            p.circle_filled(cc, 10.0, fade(paint::MEDAL_COLOURS[k], a));
            p.circle_stroke(cc, 8.5, Stroke::new(2.0, fade(col::INK_STRIPE, a)));
            paint::text(p, pos2(row.left() + 38.0, row.center().y), Align2::LEFT_CENTER, &label.to_uppercase(), Font::label(12.0, 0.12), fade(col::DUST_3, a));
            paint::text(p, pos2(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, &format_time(ticks), Font::data(13.0), fade(if won { col::DUST } else { col::DUST_2 }, a));
        }
        let cta = Rect::from_min_max(pos2(inner.left(), inner.bottom() - 60.0), inner.max);
        if self.cta(ui, p, cta, "run", "COURIR", false, Some("Entrée"), a, now) {
            self.launch(now);
        }
    }

    /// Tall layout: the circuit's sheet sliding up from the bottom.
    fn sheet_ui(&mut self, ui: &mut Ui, r: Rect, bests: &[Option<u32>], now: f64) {
        let Some((open, t0)) = self.sheet else { return };
        let e = paint::bezier(0.2, 0.9, 0.2, 1.0, ((now - t0) / 0.45) as f32);
        let k = if open { e } else { 1.0 - e };
        if !open && k <= 0.001 {
            self.sheet = None;
            return;
        }
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("menu sheet")));
        p.rect_filled(r, 0.0, fade(col::SCRIM, k));
        let h = r.height() * 0.88;
        let rect = Rect::from_min_size(pos2(r.left(), r.bottom() - h * k), vec2(r.width(), h));
        // Clicks outside close the sheet; clicks on it stay on it.
        let scrim = ui.interact(r, Id::new("menu scrim"), Sense::click());
        let _sheet = ui.interact(rect, Id::new("menu sheet body"), Sense::click());
        if scrim.clicked() && !rect.contains(scrim.interact_pointer_pos().unwrap_or(r.center())) {
            self.close_sheet(now);
        }
        let top = CornerRadius { nw: 26, ne: 26, sw: 0, se: 0 };
        p.rect_filled(rect, top, col::PANEL);
        p.hline(rect.x_range().shrink(20.0), rect.top(), Stroke::new(1.0, col::LINE));
        p.rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.top() + 12.0), vec2(40.0, 4.0)), 2.0, col::LINE);
        let Some(ti) = self.slot_index(self.sel) else { return };
        let pad = 20.0;
        let x = rect.left() + pad;
        let w = rect.width() - 2.0 * pad;
        let mut y = rect.top() + 30.0;
        // Chip and close.
        let flag = Rect::from_min_size(pos2(x, y + 13.0), vec2(14.0, 14.0));
        p.rect_filled(flag, 3.0, col::EASY);
        paint::text(&p, pos2(flag.right() + 8.0, flag.center().y), Align2::LEFT_CENTER, &format!("FACILE · {:02}", self.sel + 1), Font::label(13.0, 0.16), col::DUST_2);
        let close = Rect::from_min_size(pos2(rect.right() - pad - 40.0, y), vec2(40.0, 40.0));
        let resp = self.hit(ui, close, Id::new("menu sheet close"), true);
        paint::panel(&p, close, 12.0, col::GLASS, Some((1.0, col::LINE)));
        paint::icon_at(&p, close.center(), 18.0, Icon::Close, if resp.hovered() { col::DUST } else { col::DUST_2 });
        if resp.clicked() {
            self.cues.push(Cue::Back);
            self.close_sheet(now);
        }
        y += 52.0;
        let t = &self.tracks[ti];
        let name = paint::text(&p, pos2(x - 2.0, y), Align2::LEFT_TOP, &t.name.to_uppercase(), Font::heading(42.0), col::DUST);
        let desc = paint::para(&p, pos2(x, name.bottom() - 4.0), w, t.desc, Font::body(15.0), 1.45, col::DUST_2);
        y = desc.bottom() + 14.0;
        // The plan takes what is left above the numbers, medals and record (218 points) and the
        // footer.
        let foot = rect.bottom() - 96.0;
        let ms = (w * 0.72).min(230.0).min(foot - 12.0 - 218.0 - y).max(120.0);
        let map = Rect::from_min_size(pos2(rect.center().x - ms / 2.0, y), vec2(ms, ms));
        p.rect_filled(map, 18.0, col::VOID);
        paint::grid(&p, map.shrink(1.0), 1.0);
        p.rect_stroke(map, 18.0, Stroke::new(1.0, col::LINE), StrokeKind::Inside);
        let progress = (((now - t0) as f32 - 0.25) / 1.3).clamp(0.0, 1.0);
        paint::route(&p, map.shrink(ms * 0.07), t, ms * 0.036, progress, true, 1.0);
        y = map.bottom() + 14.0;
        // Legend.
        let legend = [(col::ROAD, "BITUME", false), (col::DIRT, "DIRT", false), (col::HUB, "DÉPART", true)];
        let font = Font::label(12.0, 0.14);
        let widths: Vec<f32> = legend.iter().map(|(_, s, _)| 22.0 + paint::text_size(&p, s, font).x).collect();
        let mut lx = rect.center().x - (widths.iter().sum::<f32>() + 16.0 * 2.0) / 2.0;
        for ((c, s, dot), lw) in legend.iter().zip(&widths) {
            if *dot {
                p.circle_filled(pos2(lx + 4.0, y + 6.0), 4.0, *c);
            } else {
                p.rect_filled(Rect::from_min_size(pos2(lx, y + 4.0), vec2(16.0, 4.0)), 2.0, *c);
            }
            paint::text(&p, pos2(lx + 22.0, y + 6.0), Align2::LEFT_CENTER, s, font, col::DUST_3);
            lx += lw + 16.0;
        }
        y += 28.0;
        // Numbers.
        let cw = (w - 24.0) / 4.0;
        let stats = [(km(t.length), "km", "LONGUEUR"), (t.checkpoints.to_string(), "", "CHECKPOINTS"), (format!("{}", (t.dirt * 100.0).round()), "%", "DIRT"), (t.jumps.to_string(), "", "SAUT")];
        for (k, (v, unit, label)) in stats.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(x + k as f32 * (cw + 8.0), y), vec2(cw, 52.0));
            p.rect_filled(cell, 12.0, col::VOID);
            p.rect_stroke(cell, 12.0, Stroke::new(1.0, col::LINE), StrokeKind::Inside);
            let vr = paint::text(&p, pos2(cell.left() + 8.0, cell.top() + 18.0), Align2::LEFT_CENTER, v, Font::data(15.0).weight(600.0), col::DUST);
            if !unit.is_empty() {
                paint::text(&p, pos2(vr.right() + 2.0, cell.top() + 20.0), Align2::LEFT_CENTER, unit, Font::data(10.0), col::DUST_3);
            }
            paint::text(&p, pos2(cell.left() + 8.0, cell.top() + 38.0), Align2::LEFT_CENTER, label, Font::label(10.0, 0.06), col::DUST_3);
        }
        y += 66.0;
        // Medals.
        paint::text(&p, pos2(x, y + 6.0), Align2::LEFT_CENTER, "MÉDAILLES", Font::label(13.0, 0.16), col::DUST_2);
        paint::text(&p, pos2(x + w, y + 6.0), Align2::RIGHT_CENTER, "temps provisoires", Font::data(10.0), col::DUST_3);
        y += 22.0;
        let mw = (w - 16.0) / 3.0;
        for (k, ((label, _), ticks)) in MEDALS.iter().zip(t.medal_ticks()).enumerate() {
            let cx = x + k as f32 * (mw + 8.0);
            let cc = pos2(cx + 11.0, y + 14.0);
            p.circle_filled(cc, 11.0, paint::MEDAL_COLOURS[k]);
            p.circle_stroke(cc, 9.5, Stroke::new(2.0, col::INK_STRIPE));
            paint::text(&p, pos2(cx + 30.0, y + 6.0), Align2::LEFT_CENTER, &label.to_uppercase(), Font::label(11.0, 0.12), col::DUST_3);
            paint::text(&p, pos2(cx + 30.0, y + 22.0), Align2::LEFT_CENTER, &format_time(ticks), Font::data(13.0), col::DUST);
        }
        y += 44.0;
        let rec = Rect::from_min_size(pos2(x, y), vec2(w, 44.0));
        p.rect_filled(rec, 12.0, col::VOID);
        p.rect_stroke(rec, 12.0, Stroke::new(1.0, col::LINE), StrokeKind::Inside);
        paint::text(&p, pos2(rec.left() + 14.0, rec.center().y), Align2::LEFT_CENTER, "TON RECORD", Font::label(13.0, 0.14), col::DUST_3);
        let best = bests.get(t.map).copied().flatten();
        paint::text(&p, pos2(rec.right() - 14.0, rec.center().y), Align2::RIGHT_CENTER, &best.map_or("--:--.--".into(), format_time), Font::data(13.0), col::DUST_2);
        // Footer with the race button.
        p.hline(rect.x_range(), foot, Stroke::new(1.0, col::LINE));
        let cta = Rect::from_min_size(pos2(x, foot + 20.0), vec2(w, 56.0));
        let pc = p.clone();
        if self.cta(ui, &pc, cta, "sheet run", "COURIR", false, None, 1.0, now) {
            self.launch(now);
        }
    }

    // ------------------------------------------------------------------ overlays

    pub(super) fn hints(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        let items: &[(&[&str], &str)] = match self.screen {
            Screen::Planets => &[(&["←", "→"], "PLANÈTE"), (&["Entrée"], "VALIDER")],
            Screen::Modes => &[(&["←", "→"], "MODE"), (&["Entrée"], "VALIDER"), (&["Échap"], "RETOUR")],
            Screen::Solo => &[(&["↑", "↓"], "CIRCUIT"), (&["←", "→"], "SÉRIE"), (&["Entrée"], "COURIR"), (&["Échap"], "RETOUR")],
            Screen::Title => &[],
        };
        let (a, _) = self.enter(4, now);
        let font = Font::label(14.0, 0.14);
        let y = r.bottom() - 28.0;
        let mut x = r.right() - 40.0;
        for (keys, label) in items.iter().rev() {
            let lw = paint::text_size(&p, label, font).x;
            x -= lw;
            paint::text(&p, pos2(x, y), Align2::LEFT_CENTER, label, font, fade(col::DUST_3, a));
            x -= 10.0;
            for k in keys.iter().rev() {
                let kw = (paint::text_size(&p, k, Font::data(11.0)).x + 14.0).max(28.0);
                x -= kw;
                paint::keycap(&p, pos2(x, y), k, fade(col::PANEL_2, a), Some(fade(col::LINE, a)), fade(col::DUST_2, a));
                x -= 5.0;
            }
            x -= 23.0;
        }
    }

    pub(super) fn toast_ui(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64) {
        let Some((text, t0)) = &self.toast else { return };
        let t = (now - t0) as f32;
        let e = paint::bezier(0.2, 0.9, 0.2, 1.0, t / 0.3);
        let a = if t < 2.4 { (t / 0.25).min(1.0) } else { 1.0 - ((t - 2.4) / 0.25).min(1.0) };
        let wide = layout == Layout::Wide;
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("menu toast")));
        let font = Font::label(if wide { 17.0 } else { 15.0 }, 0.03);
        let ts = paint::text_size(&p, text, font);
        let pad = if wide { vec2(20.0, 13.0) } else { vec2(16.0, 11.0) };
        let bottom = r.bottom() - if wide { 80.0 } else { 24.0 } + (1.0 - e) * 16.0;
        let rect = Rect::from_min_max(pos2(r.center().x - ts.x / 2.0 - pad.x, bottom - ts.y - 2.0 * pad.y), pos2(r.center().x + ts.x / 2.0 + pad.x, bottom));
        p.rect_filled(rect.translate(vec2(0.0, 6.0)), 14.0, fade(Color32::from_black_alpha(90), a));
        p.rect_filled(rect, 14.0, fade(col::PANEL_2, a));
        p.rect_stroke(rect, 14.0, Stroke::new(1.0, fade(col::LINE, a)), StrokeKind::Inside);
        paint::text(&p, rect.center(), Align2::CENTER_CENTER, text, font, fade(col::DUST, a));
    }

    /// The loading screen: the plan drawing itself, a progress bar, a tip, then "PRÊT".
    #[allow(clippy::too_many_arguments)]
    pub(super) fn loading_overlay(&self, ui: &mut Ui, r: Rect, layout: Layout, track: usize, t: f32, alpha: f32, now: f64) {
        let _ = now;
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("menu loading")));
        p.rect_filled(r, 0.0, fade(col::VOID, alpha));
        let wide = layout == Layout::Wide;
        let Some(info) = self.tracks.get(track) else { return };
        let ready = t >= 2.0;
        let ca = alpha * if ready { 0.15 } else { 1.0 };
        let (name_s, map_s, bar_w, tip_s, tip_w, gap) = if wide { (64.0, 280.0, 340.0, 17.0, 520.0, 20.0) } else { (40.0, 190.0, 220.0, 14.0, 250.0, 16.0) };
        let tip = TIPS[track % TIPS.len()];
        let tip_h = paint::para_height(&p, tip_w, tip, Font::body(tip_s), 1.45).max(tip_s * 1.45 * 2.0);
        let total = 16.0 + gap + name_s + gap + map_s + gap + 5.0 + gap + tip_h;
        let mut y = r.center().y - total / 2.0;
        paint::text(&p, pos2(r.center().x, y + 8.0), Align2::CENTER_CENTER, "CHARGEMENT", Font::label(13.0, 0.22), fade(col::DUST_3, ca));
        y += 16.0 + gap;
        paint::text(&p, pos2(r.center().x, y + name_s * 0.5), Align2::CENTER_CENTER, &info.name.to_uppercase(), Font::heading(name_s), fade(col::DUST, ca));
        y += name_s + gap;
        let map = Rect::from_min_size(pos2(r.center().x - map_s / 2.0, y), vec2(map_s, map_s));
        paint::route(&p, map.shrink(map_s * 0.04), info, map_s * 0.035, (t / 1.8).clamp(0.0, 1.0), false, ca);
        y += map_s + gap;
        let bar = Rect::from_min_size(pos2(r.center().x - bar_w / 2.0, y), vec2(bar_w, if wide { 5.0 } else { 4.0 }));
        p.rect_filled(bar, 3.0, fade(col::LINE, ca));
        let k = paint::bezier(0.3, 0.1, 0.3, 1.0, (t - 0.05) / 1.9);
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar_w * k, bar.height())), 3.0, fade(col::LIVERY, ca));
        y += 5.0 + gap;
        paint::para_centered(&p, pos2(r.center().x, y), tip_w, tip, Font::body(tip_s), 1.45, fade(col::DUST_2, ca));
        if ready {
            let k = ((t - 2.0) / 0.8).clamp(0.0, 1.0);
            let (scale, op) = if k < 0.35 { (1.6 - 0.6 * paint::ease_out(k / 0.35), k / 0.35) } else { (1.0 - 0.04 * (k - 0.35) / 0.65, 1.0) };
            let size = if wide { 150.0 } else { 84.0 } * scale;
            paint::text(&p, r.center(), Align2::CENTER_CENTER, "PRÊT", Font::display(size), fade(col::LIVERY, alpha * op));
        }
    }
}

/// The part of a polyline between fractions `a` and `b` of its length.
fn sub_path(pts: &[Pos2], a: f32, b: f32) -> Vec<Pos2> {
    let mut lengths = vec![0.0f32];
    for w in pts.windows(2) {
        lengths.push(lengths.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *lengths.last().unwrap_or(&0.0);
    let (sa, sb) = (a * total, b * total);
    let at = |s: f32| {
        let i = lengths.partition_point(|&l| l < s).clamp(1, pts.len() - 1);
        let k = (s - lengths[i - 1]) / (lengths[i] - lengths[i - 1]).max(1e-3);
        pts[i - 1] + (pts[i] - pts[i - 1]) * k
    };
    let mut out = vec![at(sa)];
    for (i, &l) in lengths.iter().enumerate() {
        if l > sa && l < sb {
            out.push(pts[i]);
        }
    }
    out.push(at(sb));
    out
}
