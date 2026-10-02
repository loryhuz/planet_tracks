//! The menu's screens, in both layouts: positions follow the validated mock-ups (1280 × 720 for
//! the wide layout, 390 × 844 for the tall one), anchored to the window's edges, over the game's
//! footage (darkened by shades where text sits on it).

use std::f32::consts::{PI, TAU};

use egui::{Align2, Color32, CornerRadius, Id, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use super::catalog::{MEDALS, PLANETS, SLOTS, Series, Stat, TIPS, TrackInfo};
use super::paint::{self, Font, Icon, col, fade};
use super::{Layout, Menu, Request, Shake};
use crate::menu_gfx::{PlanetDraw, PlanetKind};
use crate::race::format_time;
use crate::ui_sound::Cue;

const MARS_RIM: [f32; 3] = [212.0 / 255.0, 135.0 / 255.0, 86.0 / 255.0];
const ICE_RIM: [f32; 3] = [120.0 / 255.0, 170.0 / 255.0, 220.0 / 255.0];
const GAS_RIM: [f32; 3] = [210.0 / 255.0, 160.0 / 255.0, 100.0 / 255.0];

/// A record not set yet.
const NO_TIME: &str = "--:--.--";

/// The faint fill and edge of the planet's handling box, over the footage.
const STATS_FILL: Color32 = Color32::from_rgba_premultiplied(6, 4, 5, 140);
const STATS_EDGE: Color32 = Color32::from_rgba_premultiplied(34, 32, 31, 36);
/// The round arrows on either side of the planet screen.
const ARROW_FILL: Color32 = Color32::from_rgba_premultiplied(6, 4, 5, 140);
const ARROW_EDGE: Color32 = Color32::from_rgba_premultiplied(59, 56, 53, 61);
/// The chosen series in its switch, and a medal's box.
const SERIES_ON: Color32 = Color32::from_rgba_premultiplied(22, 21, 20, 23);
const MEDAL_EDGE: Color32 = Color32::from_rgba_premultiplied(29, 28, 26, 31);

enum BarLeft {
    None,
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

/// The colour of a planet's static while its images do not exist yet (sRGB 0..1).
fn static_tint(kind: PlanetKind) -> [f32; 3] {
    match kind {
        PlanetKind::Mars => [1.0, 0.55, 0.35],
        PlanetKind::Ice => [0.55, 0.75, 0.92],
        PlanetKind::Gas => [0.9, 0.69, 0.41],
    }
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

fn km(m: f32) -> String {
    format!("{:.2}", m / 1000.0).replace('.', ",")
}

fn pointer_moved(ui: &Ui) -> bool {
    ui.input(|i| i.pointer.delta() != Vec2::ZERO)
}

/// Small spaced capitals in the mono face: units, labels on the footage.
fn mono_caps(size: f32) -> Font {
    Font { spacing: 0.14, ..Font::data(size) }
}

impl Menu {
    /// The top bar: a back button on the left (or nothing), the sound switch on the right.
    /// Returns whether back was pressed.
    fn top_bar(&mut self, ui: &mut Ui, p: &Painter, r: Rect, layout: Layout, muted: bool, left: BarLeft) -> bool {
        let wide = layout == Layout::Wide;
        let (pad, btn, cy) = if wide { (36.0, 44.0, r.top() + 36.0) } else { (14.0, 40.0, r.top() + 34.0) };
        let mut back = false;
        if let BarLeft::Back(s) = left {
            let font = Font::label(if wide { 16.0 } else { 15.0 }, 0.1);
            let ts = paint::text_size(p, s, font);
            let rect = Rect::from_min_size(pos2(r.left() + pad - 8.0, cy - 22.0), vec2(ts.x + 46.0, 44.0));
            let resp = self.hit(ui, rect, Id::new(("menu back", s)), true);
            let c = if resp.hovered() { col::DUST } else { col::DUST_2 };
            if resp.hovered() {
                p.rect_filled(rect, 12.0, col::GLASS_DARK);
            }
            paint::icon_at(p, pos2(rect.left() + 18.0, cy), 22.0, Icon::ChevronLeft, c);
            paint::text_shadowed(p, pos2(rect.left() + 32.0, cy), Align2::LEFT_CENTER, s, font, c);
            back = resp.clicked();
        }
        let sr = Rect::from_min_size(pos2(r.right() - pad - btn, cy - btn / 2.0), vec2(btn, btn));
        let resp = self.hit(ui, sr, Id::new("menu sound"), true);
        paint::panel(p, sr, 12.0, col::GLASS_DARK, Some((1.0, col::EDGE)));
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
            p.rect_filled(rect, radius, fade(col::GLASS_DARK, alpha));
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

    /// A white flash over everything when the footage cuts to another shot.
    fn cut_flash(&self, p: &Painter, r: Rect, now: f64) {
        let t = (now - self.flash) as f32 / 0.24;
        if (0.0..1.0).contains(&t) {
            p.rect_filled(r, 0.0, Color32::from_white_alpha((0.4 * (1.0 - t) * 255.0) as u8));
        }
    }

    // ------------------------------------------------------------------ title

    /// The logo and its tagline over the footage, and the prompt to start.
    pub(super) fn title(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64) {
        let p = ui.painter().clone();
        if ui.interact(r, Id::new("menu title"), Sense::click()).clicked() {
            self.start(now);
        }
        self.sky.video = 1.0;
        self.sky.glow = [r.center().x, r.top() + 1.15 * r.height(), 1.1 * r.width(), 0.6 * r.height()];
        let show = (now as f32 / 1.2).clamp(0.0, 1.0);
        let wide = layout == Layout::Wide;
        let (w, h) = (r.width(), r.height());
        if wide {
            paint::gradient(&p, r, true, &[(0.0, 0.88), (0.45, 0.62), (0.62, 0.0)], col::VOID);
            paint::gradient(&p, r, true, &[(0.74, 0.0), (1.0, 0.8)], col::VOID);
        } else {
            paint::gradient(&p, r, true, &[(0.0, 0.92), (0.36, 0.72), (0.48, 0.0)], col::VOID);
            paint::gradient(&p, r, true, &[(0.64, 0.0), (0.78, 0.82), (1.0, 0.96)], col::VOID);
        }
        let (size, top) = if wide { (118.0, r.top() + h * 0.06) } else { ((w * 0.215).min(84.0), r.top() + h * 0.12) };
        let font = Font::display(size);
        let line = size * 0.86;
        let word = paint::text_size(&p, "PLANET", font).x;
        paint::letters(&p, pos2(r.center().x - word / 2.0, top), "PLANET", font, |i, n| fade(title_colour(i, n, now), show));
        let tracks = paint::text(&p, pos2(r.center().x, top + line), Align2::CENTER_TOP, "TRACKS", font, fade(col::DUST, show));
        let tag = "Chaque planète son propre style de conduite";
        if wide {
            paint::text_shadowed(&p, pos2(r.center().x, tracks.bottom() + 6.0), Align2::CENTER_TOP, tag, Font::race(32.0).weight(700.0), fade(col::DUST, show));
        } else {
            let f = Font::race(23.0).weight(700.0);
            let tw = (w - 80.0).min(300.0);
            let y = tracks.bottom() + 4.0;
            paint::para_centered(&p, pos2(r.center().x, y + 2.0), tw, tag, f, 1.15, fade(Color32::from_black_alpha(130), show));
            paint::para_centered(&p, pos2(r.center().x, y), tw, tag, f, 1.15, fade(col::DUST, show));
        }
        let pulse = 0.35 + 0.65 * (0.5 + 0.5 * (TAU * now as f32 / 1.6).cos());
        let (label, font, y) = if wide { ("APPUIE SUR UNE TOUCHE", Font::label(19.0, 0.32), r.bottom() - 62.0) } else { ("TOUCHE L'ÉCRAN", Font::label(16.0, 0.3), r.bottom() - 84.0) };
        paint::text_shadowed(&p, pos2(r.center().x, y), Align2::CENTER_CENTER, label, font, fade(col::DUST, show * pulse));
        self.cut_flash(&p, r, now);
    }

    // ------------------------------------------------------------------ planets

    /// One planet at a time: its footage behind (the static of one still to come), its card, the
    /// arrows on the edges of the screen and the dots of the three planets; a swipe changes it too.
    pub(super) fn planets(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64, muted: bool) {
        let p = ui.painter().clone();
        let wide = layout == Layout::Wide;
        let (w, h) = (r.width(), r.height());
        let pl = &PLANETS[self.planet];
        self.sky.glow = [r.center().x, r.top() + 1.15 * h, 1.1 * w, 0.6 * h];
        if pl.open {
            self.sky.video = 1.0;
        } else {
            let t = static_tint(pl.kind);
            self.sky.noise = [t[0], t[1], t[2], 0.6];
            let (c, radius) = match (wide, pl.kind) {
                (true, PlanetKind::Gas) => (pos2(r.right() - 345.0, r.center().y), ringed_radius(510.0)),
                (true, _) => (pos2(r.right() - 370.0, r.center().y), 220.0),
                (false, PlanetKind::Gas) => (pos2(r.center().x, r.top() + h * 0.59), ringed_radius(334.0)),
                (false, _) => (pos2(r.center().x, r.top() + h * 0.59), 135.0),
            };
            let rot = self.rot * 0.6 + self.planet as f32 * 1.7;
            self.sky.planets.push(planet(pl.kind, c, radius, rot, 0.35, 0.65, r, ([0.0; 3], 0.0, 0.0)));
        }

        // A horizontal swipe anywhere turns to the next or previous planet (the buttons, laid
        // over this area afterwards, keep their clicks).
        let swipe = ui.interact(r, Id::new("menu planet swipe"), Sense::click_and_drag());
        if swipe.dragged() {
            self.drag = Some(self.drag.unwrap_or(0.0) + swipe.drag_delta().x);
        }
        if swipe.drag_stopped()
            && let Some(d) = self.drag.take()
        {
            let v = ui.input(|i| i.pointer.velocity().x);
            if d < -60.0 || v < -500.0 {
                self.set_planet(self.planet as isize + 1, now);
            } else if d > 60.0 || v > 500.0 {
                self.set_planet(self.planet as isize - 1, now);
            }
        }

        if wide {
            paint::gradient(&p, r, false, &[(0.0, 0.92), (0.34, 0.75), (0.6, 0.0)], col::VOID);
            paint::gradient(&p, r, true, &[(0.75, 0.0), (1.0, 0.6)], col::VOID);
        } else {
            paint::gradient(&p, r, true, &[(0.0, 0.92), (0.38, 0.8), (0.52, 0.0)], col::VOID);
            paint::gradient(&p, r, true, &[(0.68, 0.0), (0.82, 0.6), (1.0, 0.92)], col::VOID);
        }
        self.top_bar(ui, &p, r, layout, muted, BarLeft::None);

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
        let (ca, cdy) = self.enter(1, now);
        let open = pl.open;
        let label = if open { "PILOTER SUR MARS" } else { "BIENTÔT DISPONIBLE" };
        let cta = if wide {
            let (x, width) = (r.left() + 110.0, 460.0);
            let block = self.info_height(&p, width, layout);
            let top = r.center().y - (block + 34.0 + 64.0) / 2.0;
            self.info_block(&p, pos2(x + dx, top + edy), width, layout, k, a * ea);
            Rect::from_min_size(pos2(x + self.shake_x(Shake::Cta, now), top + block + 34.0 + cdy), vec2(width, 64.0))
        } else {
            self.info_block(&p, pos2(r.left() + 22.0 + dx, r.top() + 96.0 + edy), w - 44.0, layout, k, a * ea);
            Rect::from_min_size(pos2(r.left() + 18.0 + self.shake_x(Shake::Cta, now), r.bottom() - 100.0 + cdy), vec2(w - 36.0, 62.0))
        };
        if self.cta(ui, &p, cta, "planet", label, !open, wide.then_some("Entrée"), ca, now) {
            self.choose_planet(now);
        }

        // Arrows on the edges and the dots.
        let last = PLANETS.len() - 1;
        let (ab, ay, xl, xr) = if wide { (52.0, r.center().y, r.left() + 50.0, r.right() - 50.0) } else { (44.0, r.top() + h * 0.56, r.left() + 32.0, r.right() - 32.0) };
        for (dir, x) in [(-1isize, xl), (1, xr)] {
            let rect = Rect::from_center_size(pos2(x, ay), vec2(ab, ab));
            let enabled = if dir < 0 { self.planet > 0 } else { self.planet < last };
            let a = ca * if enabled { 1.0 } else { 0.28 };
            let hovered = enabled && {
                let resp = self.hit(ui, rect, Id::new(("menu arrow", dir)), true);
                if resp.clicked() {
                    self.set_planet(self.planet as isize + dir, now);
                }
                resp.hovered()
            };
            p.circle_filled(rect.center(), ab / 2.0, fade(ARROW_FILL, a));
            p.circle_stroke(rect.center(), ab / 2.0 - 0.5, Stroke::new(1.0, fade(ARROW_EDGE, a)));
            let c = if hovered { col::LIVERY } else { col::DUST };
            paint::icon_at(&p, rect.center(), if wide { 24.0 } else { 21.0 }, if dir < 0 { Icon::ChevronLeft } else { Icon::ChevronRight }, fade(c, a));
        }
        let (dw, don, gap) = if wide { (26.0, 40.0, 9.0) } else { (22.0, 34.0, 8.0) };
        let total: f32 = (0..PLANETS.len()).map(|i| if i == self.planet { don } else { dw }).sum::<f32>() + gap * last as f32;
        let mut x = r.center().x - total / 2.0;
        let y = if wide { r.bottom() - 30.0 } else { r.bottom() - 128.0 };
        for (i, info) in PLANETS.iter().enumerate() {
            let wi = if i == self.planet { don } else { dw };
            let c = if i != self.planet { Color32::from_rgba_premultiplied(61, 58, 55, 64) } else if info.open { col::LIVERY } else { col::DUST_2 };
            p.rect_filled(Rect::from_min_size(pos2(x, y - 2.0), vec2(wi, 4.0)), 2.0, fade(c, ca));
            x += wi + gap;
        }
    }

    fn info_sizes(layout: Layout, width: f32) -> (f32, f32, f32, f32, f32) {
        // Name, lore size, lore line height, handling box height, gap.
        match layout {
            Layout::Wide => (150.0, 18.0, 1.5, 64.0, 14.0),
            Layout::Tall => ((width * 0.27).min(92.0), 15.0, 1.45, 52.0, 12.0),
        }
    }

    /// Height of the planet's card (the tallest of the planets', so the button stays put).
    fn info_height(&self, p: &Painter, width: f32, layout: Layout) -> f32 {
        let (name, lore_size, lore_line, stats, gap) = Self::info_sizes(layout, width);
        let lore = PLANETS
            .iter()
            .map(|pl| paint::para_height(p, width, pl.lore, Font::body(lore_size), lore_line))
            .fold(lore_size * lore_line * 2.0, f32::max);
        name * 0.92 + gap + lore + gap + stats
    }

    /// The planet's name and year, its story and how its vehicle handles.
    fn info_block(&self, p: &Painter, at: Pos2, width: f32, layout: Layout, k: usize, a: f32) {
        let wide = layout == Layout::Wide;
        let pl = &PLANETS[k];
        let (size, lore_size, lore_line, sh, gap) = Self::info_sizes(layout, width);
        let mut y = at.y;
        let font = Font::display(size);
        let name_c = if pl.open { col::DUST } else { col::DUST_3 };
        let nr = paint::text(p, pos2(at.x - size * 0.03, y - size * 0.3), Align2::LEFT_TOP, pl.name, font, fade(name_c, a));
        let year_c = if pl.open { col::LIVERY } else { col::DUST_3 };
        paint::text(p, pos2(nr.right() + 12.0, y + size * 0.86), Align2::LEFT_BOTTOM, pl.year, Font::race(if wide { 46.0 } else { 32.0 }), fade(year_c, a));
        y += size * 0.92 + gap;
        let lr = paint::para(p, pos2(at.x, y), width, pl.lore, Font::body(lore_size), lore_line, fade(col::TEXT, a));
        y += lr.height().max(lore_size * lore_line * 2.0) + gap;
        // Handling.
        let box_ = Rect::from_min_size(pos2(at.x, y), vec2(width, sh));
        p.rect_filled(box_, 0.0, fade(STATS_FILL, a));
        p.rect_stroke(box_, 0.0, Stroke::new(1.0, fade(STATS_EDGE, a)), StrokeKind::Inside);
        let cw = width / 3.0;
        for i in 1..3 {
            p.vline(box_.left() + i as f32 * cw, box_.y_range(), Stroke::new(1.0, fade(STATS_EDGE, a)));
        }
        for (i, (label, stat)) in pl.stats.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(box_.left() + i as f32 * cw, box_.top()), vec2(cw, sh));
            let lx = cell.left() + if wide { 14.0 } else { 10.0 };
            paint::text(p, pos2(lx, cell.top() + if wide { 12.0 } else { 9.0 }), Align2::LEFT_TOP, label, Font::label(if wide { 12.0 } else { 10.5 }, 0.14), fade(col::DUST_3, a));
            let vy = cell.top() + if wide { 43.0 } else { 35.0 };
            match stat {
                Stat::Text(s) => {
                    paint::text(p, pos2(lx, vy), Align2::LEFT_CENTER, s, Font::race(if wide { 25.0 } else { 20.0 }), fade(col::DUST, a));
                }
                Stat::Pips(n) => paint::pips(p, pos2(lx + 2.0, vy), *n, if wide { 13.0 } else { 11.0 }, if wide { 8.0 } else { 7.0 }, a),
            }
        }
    }

    // ------------------------------------------------------------------ circuits

    /// Mars's circuits as tiles, their outline first; the chosen one's record and medals beside
    /// them on a computer, in a sheet from the bottom on a phone.
    pub(super) fn solo(&mut self, ui: &mut Ui, r: Rect, layout: Layout, now: f64, muted: bool, bests: &[Option<u32>]) {
        let p = ui.painter().clone();
        let wide = layout == Layout::Wide;
        let w = r.width();
        self.sky.video = 1.0;
        self.sky.dim = 0.5;
        self.sky.glow = [r.center().x, r.top() + 1.15 * r.height(), 1.1 * w, 0.6 * r.height()];
        if wide {
            paint::gradient(&p, r, false, &[(0.0, 0.85), (0.46, 0.4), (0.64, 0.0)], col::VOID);
            paint::gradient(&p, r, true, &[(0.6, 0.0), (1.0, 0.75)], col::VOID);
        } else {
            paint::gradient(&p, r, true, &[(0.0, 0.85), (0.3, 0.4), (1.0, 0.3)], col::VOID);
        }
        let sheet_open = self.sheet.is_some();
        if self.top_bar(ui, &p, r, layout, muted, BarLeft::Back("PLANÈTES")) && !sheet_open {
            self.back(now);
        }
        let (ea, edy) = self.enter(0, now);
        let (x0, gw) = if wide { (r.left() + 64.0, 540.0) } else { (r.left() + 14.0, w - 28.0) };
        let head = pos2(if wide { x0 } else { r.left() + 20.0 }, r.top() + if wide { 64.0 } else { 74.0 } + edy);
        paint::text_shadowed(&p, head, Align2::LEFT_TOP, "CIRCUITS", Font::race(if wide { 58.0 } else { 44.0 }), fade(col::DUST, ea));
        let (sa, sdy) = self.enter(1, now);
        let switch = if wide {
            Rect::from_min_size(pos2(x0, r.top() + 132.0 + sdy), vec2(400.0, 48.0))
        } else {
            Rect::from_min_size(pos2(r.left() + 16.0, r.top() + 142.0 + sdy), vec2(w - 32.0, 46.0))
        };
        self.series_switch(ui, &p, switch, sa, sheet_open);
        let cols = layout.columns();
        let gap = 10.0;
        let tw = (gw - gap * (cols - 1) as f32) / cols as f32;
        let th = if wide { 190.0 } else { 196.0 };
        let gy = r.top() + if wide { 198.0 } else { 204.0 };
        for i in 0..SLOTS {
            let (a, dy) = self.enter(2 + i, now);
            let (row, c) = (i / cols, i % cols);
            let rect = Rect::from_min_size(pos2(x0 + c as f32 * (tw + gap) + self.shake_x(Shake::Tile(i), now), gy + row as f32 * (th + gap) + dy), vec2(tw, th));
            self.tile(ui, &p, rect, i, layout, bests, a, now, sheet_open);
        }
        if wide {
            self.detail(ui, &p, r, bests, now);
        } else if self.sheet.is_some() {
            self.sheet_ui(ui, r, bests, now);
        }
    }

    /// The two series side by side, the chosen one lit and underlined in its colour.
    fn series_switch(&mut self, ui: &mut Ui, p: &Painter, rect: Rect, a: f32, blocked: bool) {
        p.rect_filled(rect, 12.0, fade(col::GLASS_DARK, a));
        p.rect_stroke(rect, 12.0, Stroke::new(1.0, fade(col::EDGE, a)), StrokeKind::Inside);
        if self.on_series {
            p.rect_stroke(rect.expand(3.0), 15.0, Stroke::new(2.0, fade(col::LIVERY, a)), StrokeKind::Outside);
        }
        let half = (rect.width() - 8.0) / 2.0;
        let built = format!("{}/{SLOTS}", self.tracks.len().min(SLOTS));
        for (i, (series, label, count)) in [(Series::Easy, "FACILE", built.as_str()), (Series::Hard, "DUR", "0/5")].into_iter().enumerate() {
            let tr = Rect::from_min_size(rect.min + vec2(4.0 + i as f32 * half, 4.0), vec2(half, rect.height() - 8.0));
            let resp = if blocked { None } else { Some(self.hit(ui, tr, Id::new(("menu series", i)), true)) };
            if resp.as_ref().is_some_and(|r| r.clicked()) {
                self.on_series = false;
                self.set_series(series, ui.input(|inp| inp.time));
            }
            let on = self.series == series;
            if on {
                p.rect_filled(tr, 9.0, fade(SERIES_ON, a));
                let line = if series == Series::Easy { col::EASY } else { col::HARD_EDGE };
                p.rect_filled(Rect::from_min_max(pos2(tr.left() + tr.width() * 0.2, tr.bottom() - 6.0), pos2(tr.right() - tr.width() * 0.2, tr.bottom() - 3.0)), 1.5, fade(line, a));
            }
            let hovered = resp.is_some_and(|r| r.hovered());
            let font = Font::race(20.0);
            let lw = paint::text_size(p, label, font).x;
            let cw = paint::text_size(p, count, Font::data(11.0)).x;
            let x = tr.center().x - (lw + 8.0 + cw) / 2.0;
            let tc = if on || hovered { col::DUST } else { col::DUST_3 };
            paint::text(p, pos2(x, tr.center().y - 1.0), Align2::LEFT_CENTER, label, font, fade(tc, a));
            paint::text(p, pos2(x + lw + 8.0, tr.center().y), Align2::LEFT_CENTER, count, Font::data(11.0), fade(col::DUST_3, a));
        }
    }

    fn slot_index(&self, i: usize) -> Option<usize> {
        match self.series {
            Series::Easy => (i < self.tracks.len() && i < SLOTS).then_some(i),
            Series::Hard => None,
        }
    }

    /// A circuit's tile: its number and medals, its outline, its name and record.
    #[allow(clippy::too_many_arguments)]
    fn tile(&mut self, ui: &mut Ui, p: &Painter, rect: Rect, i: usize, layout: Layout, bests: &[Option<u32>], a: f32, now: f64, blocked: bool) {
        let wide = layout == Layout::Wide;
        if !blocked {
            let resp = self.hit(ui, rect, Id::new(("menu tile", i)), false);
            if wide && resp.hovered() && pointer_moved(ui) {
                self.on_series = false;
                self.set_sel(i, now, true);
            }
            if resp.clicked() {
                self.on_series = false;
                if wide {
                    self.set_sel(i, now, false);
                    self.launch(now);
                } else {
                    self.open_sheet(i, now);
                }
            }
        }
        let sel = self.sel == i && !self.on_series && (wide || self.sheet.is_none());
        let ti = self.slot_index(i);
        let a = if ti.is_some() { a } else { a * 0.5 };
        p.rect_filled(rect, 14.0, fade(if sel { col::GLASS_WARM } else { col::GLASS_DARK }, a));
        if sel {
            p.rect_stroke(rect, 14.0, Stroke::new(2.0, fade(col::LIVERY, a)), StrokeKind::Inside);
        } else {
            p.rect_stroke(rect, 14.0, Stroke::new(1.0, fade(col::EDGE, a)), StrokeKind::Inside);
        }
        let inner = rect.shrink2(vec2(14.0, 12.0));
        let ty = inner.top() + 7.0;
        paint::text(p, pos2(inner.left(), ty), Align2::LEFT_CENTER, &format!("{:02}", i + 1), Font::data(11.0), fade(if sel { col::LIVERY } else { col::DUST_3 }, a));
        let rec_y = inner.bottom() - 8.0;
        let name_y = rec_y - 24.0;
        match ti {
            Some(ti) => {
                let t = &self.tracks[ti];
                let best = bests.get(t.map).copied().flatten();
                let medals = t.medal_ticks();
                let ms = 9.0;
                for (k, mc) in paint::MEDAL_COLOURS.iter().enumerate() {
                    let c = pos2(inner.right() - ms / 2.0 - (2 - k) as f32 * (ms + 3.0), ty);
                    if best.is_some_and(|b| b <= medals[k]) {
                        p.circle_filled(c, ms / 2.0, fade(*mc, a));
                    } else {
                        p.circle_stroke(c, ms / 2.0 - 0.6, Stroke::new(1.2, fade(*mc, a * 0.8)));
                    }
                }
                let top = ty + 12.0;
                let bottom = name_y - 16.0;
                let s = (bottom - top).min(inner.width()).min(if wide { 100.0 } else { 104.0 });
                let plan = Rect::from_center_size(pos2(inner.center().x, (top + bottom) / 2.0), vec2(s, s));
                paint::route(p, plan, t, s * 0.05, 1.0, false, a);
                paint::text(p, pos2(inner.left(), name_y), Align2::LEFT_CENTER, &t.name.to_uppercase(), Font::race(24.0), fade(col::DUST, a));
                let rec = best.map_or(NO_TIME.to_string(), format_time);
                paint::text(p, pos2(inner.left(), rec_y), Align2::LEFT_CENTER, &rec, Font::data(13.0), fade(col::DUST_2, a));
            }
            None => {
                let hard = self.series == Series::Hard;
                paint::icon_at(p, pos2(inner.center().x, (ty + name_y) / 2.0), 24.0, Icon::Lock, fade(col::DUST_3, a));
                paint::text(p, pos2(inner.left(), name_y), Align2::LEFT_CENTER, if hard { "VERROUILLÉ" } else { "À VENIR" }, Font::race(24.0), fade(col::DUST_2, a));
                paint::text(p, pos2(inner.left(), rec_y), Align2::LEFT_CENTER, if hard { "Série Facile" } else { "En construction" }, Font::data(11.0), fade(col::DUST_3, a));
            }
        }
    }

    /// A circuit's record and its three medals, in a dark box: the time on the left, the medals
    /// stacked on the right.
    fn record_block(&self, p: &Painter, rect: Rect, t: &TrackInfo, best: Option<u32>, a: f32, wide: bool) {
        p.rect_filled(rect, 14.0, fade(col::GLASS_DARK, a));
        p.rect_stroke(rect, 14.0, Stroke::new(1.0, fade(col::EDGE, a)), StrokeKind::Inside);
        let inner = rect.shrink(16.0);
        let left_w = if wide { 190.0 } else { 128.0 };
        paint::text(p, inner.left_top(), Align2::LEFT_TOP, "TON RECORD", mono_caps(11.0), fade(col::DUST_2, a));
        let at = pos2(inner.left(), inner.top() + 20.0);
        match best {
            Some(b) => paint::text(p, at, Align2::LEFT_TOP, &format_time(b), Font::race(if wide { 58.0 } else { 42.0 }), fade(col::DUST, a)),
            None => paint::text(p, at + vec2(0.0, 10.0), Align2::LEFT_TOP, NO_TIME, Font::data(if wide { 26.0 } else { 20.0 }), fade(col::DUST_3, a)),
        };
        let mx = inner.left() + left_w;
        let mw = inner.right() - mx;
        let rh = (inner.height() - 12.0) / 3.0;
        for (k, ((name, _), ticks)) in MEDALS.iter().zip(t.medal_ticks()).enumerate() {
            let row = Rect::from_min_size(pos2(mx, inner.top() + k as f32 * (rh + 6.0)), vec2(mw, rh));
            let won = best.is_some_and(|b| b <= ticks);
            p.rect_stroke(row, 9.0, Stroke::new(1.0, fade(if won { paint::MEDAL_COLOURS[k] } else { MEDAL_EDGE }, a)), StrokeKind::Inside);
            let c = pos2(row.left() + 16.0, row.center().y);
            if won {
                p.circle_filled(c, 5.0, fade(paint::MEDAL_COLOURS[k], a));
            } else {
                p.circle_stroke(c, 4.4, Stroke::new(1.3, fade(paint::MEDAL_COLOURS[k], a)));
            }
            paint::text(p, pos2(row.left() + 30.0, row.center().y), Align2::LEFT_CENTER, name, Font::data(12.0), fade(col::DUST_2, a));
            paint::text(p, pos2(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, &format_time(ticks), Font::data(12.0), fade(if won { col::DUST } else { col::DUST_2 }, a));
        }
    }

    /// Wide layout: the chosen circuit over the footage, at the bottom right: its name, length
    /// and dirt, its record and medals, and the race button.
    fn detail(&mut self, ui: &mut Ui, p: &Painter, r: Rect, bests: &[Option<u32>], now: f64) {
        let (da, ddy) = self.enter(3, now);
        let since = (now - self.sel_at) as f32;
        let e = paint::ease_out(since / 0.3);
        let a = da * (0.25 + 0.75 * e);
        let w = 520.0;
        let x = r.right() - 60.0 - w;
        let cta = Rect::from_min_size(pos2(x, r.bottom() - 44.0 - 60.0 + ddy), vec2(w, 60.0));
        let slide = 30.0 * (1.0 - e);
        let Some(ti) = self.slot_index(self.sel) else {
            let hard = self.series == Series::Hard;
            let text = self.locked_text();
            paint::text_shadowed(p, pos2(x + slide, cta.top() - 30.0), Align2::LEFT_BOTTOM, text, Font::body(17.0), fade(col::TEXT, a));
            paint::text_shadowed(p, pos2(x - 4.0 + slide, cta.top() - 58.0), Align2::LEFT_BOTTOM, if hard { "VERROUILLÉ" } else { "À VENIR" }, Font::race(96.0).weight(900.0), fade(col::DUST, a));
            if self.cta(ui, p, cta, "run", "COURIR", true, Some("Entrée"), da, now) {
                self.launch(now);
            }
            return;
        };
        let t = &self.tracks[ti];
        let best = bests.get(t.map).copied().flatten();
        let rec = Rect::from_min_size(pos2(x, cta.top() - 14.0 - 148.0), vec2(w, 148.0));
        self.record_block(p, rec, t, best, da, true);
        let meta = format!("{} KM · {}\u{202F}% DIRT", km(t.length), (t.dirt * 100.0).round());
        let mr = paint::text_shadowed(p, pos2(x + slide, rec.top() - 14.0), Align2::LEFT_BOTTOM, &meta, mono_caps(11.0), fade(col::DUST_2, a));
        paint::text_shadowed(p, pos2(x - 4.0 + slide, mr.top() - 6.0), Align2::LEFT_BOTTOM, &t.name.to_uppercase(), Font::race(96.0).weight(900.0), fade(col::DUST, a));
        if self.cta(ui, p, cta, "run", "COURIR", false, Some("Entrée"), da, now) {
            self.launch(now);
        }
    }

    /// Tall layout: the circuit's sheet sliding up from the bottom.
    fn sheet_ui(&mut self, ui: &mut Ui, r: Rect, bests: &[Option<u32>], now: f64) {
        let Some((open, t0)) = self.sheet else { return };
        let e = paint::bezier(0.2, 0.9, 0.2, 1.0, ((now - t0) / 0.4) as f32);
        let k = if open { e } else { 1.0 - e };
        if !open && k <= 0.001 {
            self.sheet = None;
            return;
        }
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("menu sheet")));
        p.rect_filled(r, 0.0, fade(col::SCRIM, k));
        let h = 372.0;
        let rect = Rect::from_min_size(pos2(r.left(), r.bottom() - h * k), vec2(r.width(), h));
        // Clicks outside close the sheet; clicks on it stay on it.
        let scrim = ui.interact(r, Id::new("menu scrim"), Sense::click());
        let _sheet = ui.interact(rect, Id::new("menu sheet body"), Sense::click());
        if scrim.clicked() && !rect.contains(scrim.interact_pointer_pos().unwrap_or(r.center())) {
            self.cues.push(Cue::Back);
            self.close_sheet(now);
        }
        p.rect_filled(rect, CornerRadius { nw: 28, ne: 28, sw: 0, se: 0 }, Color32::from_rgba_premultiplied(19, 13, 14, 247));
        p.hline(rect.x_range().shrink(24.0), rect.top(), Stroke::new(1.0, col::EDGE));
        p.rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.top() + 12.0), vec2(40.0, 5.0)), 2.5, col::LINE);
        let Some(ti) = self.slot_index(self.sel) else { return };
        let t = &self.tracks[ti];
        let best = bests.get(t.map).copied().flatten();
        let x = rect.left() + 22.0;
        let w = rect.width() - 44.0;
        let name = paint::text(&p, pos2(x - 2.0, rect.top() + 28.0), Align2::LEFT_TOP, &t.name.to_uppercase(), Font::race(50.0).weight(900.0), col::DUST);
        let meta = format!("{} KM · {}\u{202F}% DIRT", km(t.length), (t.dirt * 100.0).round());
        let mr = paint::text(&p, pos2(x, name.bottom() + 2.0), Align2::LEFT_TOP, &meta, mono_caps(11.0), col::DUST_2);
        let rec = Rect::from_min_size(pos2(x, mr.bottom() + 14.0), vec2(w, 124.0));
        self.record_block(&p, rec, t, best, 1.0, false);
        let cta = Rect::from_min_size(pos2(x, rect.bottom() - 34.0 - 60.0), vec2(w, 60.0));
        let pc = p.clone();
        if self.cta(ui, &pc, cta, "sheet run", "COURIR", false, None, 1.0, now) {
            self.launch(now);
        }
    }

    // ------------------------------------------------------------------ overlays

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
