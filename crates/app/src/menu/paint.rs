//! Drawing helpers for the menu: the palette (the buggy's livery over a Martian night), its
//! typefaces, text with letter spacing, panels, pills, keycaps, icons and the circuit plans.

use std::sync::Arc;

use egui::epaint::{Mesh, PathStroke, Vertex};
use egui::epaint::text::VariationCoords;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontFamily, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::catalog::TrackInfo;

/// The palette, sRGB.
pub mod col {
    use egui::Color32;
    pub const VOID: Color32 = Color32::from_rgb(11, 8, 9);
    pub const PANEL: Color32 = Color32::from_rgb(29, 20, 22);
    pub const PANEL_2: Color32 = Color32::from_rgb(40, 27, 28);
    pub const LINE: Color32 = Color32::from_rgb(59, 43, 42);
    pub const SCRIM: Color32 = Color32::from_rgba_premultiplied(5, 3, 4, 158);
    pub const DUST: Color32 = Color32::from_rgb(244, 232, 220);
    pub const DUST_2: Color32 = Color32::from_rgb(191, 169, 155);
    pub const DUST_3: Color32 = Color32::from_rgb(134, 113, 106);
    pub const LIVERY: Color32 = Color32::from_rgb(255, 106, 31);
    pub const LIVERY_INK: Color32 = Color32::from_rgb(27, 12, 5);
    pub const INK_STRIPE: Color32 = Color32::from_rgba_premultiplied(4, 2, 1, 41);
    pub const HUB: Color32 = Color32::from_rgb(85, 220, 255);
    pub const ROAD: Color32 = Color32::from_rgb(239, 227, 214);
    pub const DIRT: Color32 = Color32::from_rgb(194, 122, 72);
    pub const EASY: Color32 = Color32::from_rgb(37, 196, 109);
    pub const HARD_EDGE: Color32 = Color32::from_rgb(236, 226, 216);
    pub const GOLD: Color32 = Color32::from_rgb(243, 195, 79);
    pub const SILVER: Color32 = Color32::from_rgb(205, 213, 219);
    pub const BRONZE: Color32 = Color32::from_rgb(201, 129, 76);
    pub const PL_MARS: Color32 = Color32::from_rgb(216, 112, 63);
    pub const PL_GAS: Color32 = Color32::from_rgb(227, 171, 97);
    pub const PL_ICE: Color32 = Color32::from_rgb(158, 209, 242);
    /// Running text over the footage, a little brighter than `DUST_2`.
    pub const TEXT: Color32 = Color32::from_rgb(234, 220, 207);
    /// Dark glass over the footage, and its edge.
    pub const GLASS_DARK: Color32 = Color32::from_rgba_premultiplied(8, 6, 7, 194);
    pub const EDGE: Color32 = Color32::from_rgba_premultiplied(24, 23, 22, 26);
    /// A chosen tile: the dark glass warmed by the livery.
    pub const GLASS_WARM: Color32 = Color32::from_rgba_premultiplied(31, 15, 9, 219);
}

pub const MEDAL_COLOURS: [Color32; 3] = [col::GOLD, col::SILVER, col::BRONZE];

/// Colour faded towards transparent (`a` 0..1).
pub fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

pub fn lerp_colour(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// The menu's typefaces (Google Fonts, OFL, in `assets/fonts`): Saira Stencil One for the
/// title and planet names, Saira (its condensed width) for labels and headings, Saira for running
/// text, Saira Italic condensed for the racing headings (circuit names, the tagline), Martian Mono
/// for numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Display,
    Condensed,
    Text,
    Race,
    Mono,
}

const SAIRA: &[u8] = include_bytes!("../../assets/fonts/Saira.ttf");
const SAIRA_ITALIC: &[u8] = include_bytes!("../../assets/fonts/SairaItalic.ttf");
const STENCIL: &[u8] = include_bytes!("../../assets/fonts/SairaStencilOne-Regular.ttf");
const MARTIAN: &[u8] = include_bytes!("../../assets/fonts/MartianMono.ttf");

/// egui's fonts with the menu's added as named families (the HUD keeps egui's own).
pub fn fonts() -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, bytes) in [("Saira", SAIRA), ("SairaItalic", SAIRA_ITALIC), ("SairaStencilOne", STENCIL), ("MartianMono", MARTIAN)] {
        defs.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        defs.families.insert(FontFamily::Name(name.into()), list);
    }
    defs
}

/// A text style: size in points, extra spacing between letters (in ems), typeface and weight
/// (100 to 900, for the variable faces).
#[derive(Clone, Copy, Debug)]
pub struct Font {
    pub size: f32,
    pub spacing: f32,
    pub face: Face,
    pub weight: f32,
}

impl Font {
    /// Running text.
    pub const fn body(size: f32) -> Self {
        Self { size, spacing: 0.0, face: Face::Text, weight: 400.0 }
    }
    /// Uppercase labels and buttons.
    pub const fn label(size: f32, spacing: f32) -> Self {
        Self { size, spacing, face: Face::Condensed, weight: 600.0 }
    }
    /// Headings and names.
    pub const fn heading(size: f32) -> Self {
        Self { size, spacing: 0.03, face: Face::Condensed, weight: 800.0 }
    }
    /// The title and planet names, stencilled.
    pub const fn display(size: f32) -> Self {
        Self { size, spacing: 0.02, face: Face::Display, weight: 400.0 }
    }
    /// Racing headings: italic, condensed, heavy.
    pub const fn race(size: f32) -> Self {
        Self { size, spacing: 0.005, face: Face::Race, weight: 800.0 }
    }
    /// Numbers, times, counters.
    pub const fn data(size: f32) -> Self {
        Self { size, spacing: 0.0, face: Face::Mono, weight: 400.0 }
    }
    pub const fn weight(mut self, w: f32) -> Self {
        self.weight = w;
        self
    }

    fn id(&self) -> FontId {
        let family = match self.face {
            Face::Display => "SairaStencilOne",
            Face::Condensed | Face::Text => "Saira",
            Face::Race => "SairaItalic",
            Face::Mono => "MartianMono",
        };
        FontId::new(self.size, FontFamily::Name(family.into()))
    }

    fn format(&self, color: Color32) -> TextFormat {
        let coords = match self.face {
            Face::Display => VariationCoords::default(),
            Face::Condensed => VariationCoords::new([(b"wdth", 75.0), (b"wght", self.weight)]),
            Face::Race => VariationCoords::new([(b"wdth", 72.0), (b"wght", self.weight)]),
            Face::Text | Face::Mono => VariationCoords::new([(b"wdth", 100.0), (b"wght", self.weight)]),
        };
        TextFormat { font_id: self.id(), extra_letter_spacing: self.spacing * self.size, color, coords, ..Default::default() }
    }
}

pub fn galley(p: &Painter, text: &str, font: Font, color: Color32, wrap: Option<f32>, line_height: Option<f32>) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    let mut format = font.format(color);
    format.line_height = line_height;
    job.append(text, 0.0, format);
    if let Some(w) = wrap {
        job.wrap.max_width = w;
    }
    p.layout_job(job)
}

/// Draws a galley in one colour.
pub fn paint_galley(p: &Painter, pos: Pos2, g: &Arc<Galley>, color: Color32, _font: Font) {
    p.galley_with_override_text_color(pos, g.clone(), color);
}

/// One line of text anchored at `pos`; returns its rectangle.
pub fn text(p: &Painter, pos: Pos2, anchor: Align2, s: &str, font: Font, color: Color32) -> Rect {
    let g = galley(p, s, font, color, None, None);
    // The spacing after the last letter is not part of the word.
    let size = g.size() - vec2(font.spacing * font.size, 0.0);
    let rect = anchor.anchor_size(pos, size);
    paint_galley(p, rect.min, &g, color, font);
    rect
}

pub fn text_size(p: &Painter, s: &str, font: Font) -> Vec2 {
    galley(p, s, font, Color32::WHITE, None, None).size() - vec2(font.spacing * font.size, 0.0)
}

/// Wrapped text from its top-left corner; returns its rectangle.
pub fn para(p: &Painter, pos: Pos2, width: f32, s: &str, font: Font, line: f32, color: Color32) -> Rect {
    let g = galley(p, s, font, color, Some(width), Some(font.size * line));
    let rect = Rect::from_min_size(pos, g.size());
    paint_galley(p, pos, &g, color, font);
    rect
}

/// Wrapped text with each line centred on `top_center.x`; returns its rectangle.
pub fn para_centered(p: &Painter, top_center: Pos2, width: f32, s: &str, font: Font, line: f32, color: Color32) -> Rect {
    let mut job = LayoutJob::default();
    let mut format = font.format(color);
    format.line_height = Some(font.size * line);
    job.append(s, 0.0, format);
    job.wrap.max_width = width;
    job.halign = egui::Align::Center;
    let g = p.layout_job(job);
    let size = g.size();
    paint_galley(p, top_center, &g, color, font);
    Rect::from_min_size(pos2(top_center.x - size.x / 2.0, top_center.y), size)
}

pub fn para_height(p: &Painter, width: f32, s: &str, font: Font, line: f32) -> f32 {
    galley(p, s, font, Color32::WHITE, Some(width), Some(font.size * line)).size().y
}

/// A word whose letters take their colours from `colour(i, n)`.
pub fn letters(p: &Painter, pos: Pos2, s: &str, font: Font, colour: impl Fn(usize, usize) -> Color32) -> Rect {
    let chars: Vec<char> = s.chars().collect();
    let mut job = LayoutJob::default();
    for (i, c) in chars.iter().enumerate() {
        job.append(&c.to_string(), 0.0, font.format(colour(i, chars.len())));
    }
    let g = p.layout_job(job);
    let rect = Rect::from_min_size(pos, g.size() - vec2(font.spacing * font.size, 0.0));
    p.galley(pos, g, Color32::WHITE);
    rect
}

pub fn panel(p: &Painter, rect: Rect, radius: f32, fill: Color32, stroke: Option<(f32, Color32)>) {
    p.rect_filled(rect, radius, fill);
    if let Some((w, c)) = stroke {
        p.rect_stroke(rect, radius, Stroke::new(w, c), StrokeKind::Inside);
    }
}

/// A soft glow under a panel: a few widening translucent copies of its outline.
pub fn glow(p: &Painter, rect: Rect, radius: f32, colour: Color32, strength: f32) {
    for i in 1..=6 {
        let k = i as f32;
        p.rect_filled(rect.expand(k * 3.0).translate(vec2(0.0, k * 2.5)), radius + k * 3.0, fade(colour, strength * 0.05 * (1.0 - k / 7.0)));
    }
}

/// A key as drawn in the hints: `Entrée`, `←`.
pub fn keycap(p: &Painter, left_center: Pos2, s: &str, fill: Color32, border: Option<Color32>, colour: Color32) -> Rect {
    let font = Font::data(11.0);
    let ts = text_size(p, s, font);
    let rect = Rect::from_min_size(pos2(left_center.x, left_center.y - 13.0), vec2((ts.x + 14.0).max(28.0), 26.0));
    p.rect_filled(rect, 7.0, fill);
    if let Some(b) = border {
        p.rect_stroke(rect, 7.0, Stroke::new(1.0, b), StrokeKind::Inside);
        p.line_segment([pos2(rect.left() + 6.0, rect.bottom() - 1.0), pos2(rect.right() - 6.0, rect.bottom() - 1.0)], Stroke::new(2.0, b));
    }
    text(p, rect.center(), Align2::CENTER_CENTER, s, font, colour);
    rect
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    Lock,
    SoundOn,
    SoundOff,
    /// The last checkpoint.
    Flag,
    /// Restart: an arrow going round.
    Restart,
}

/// A line icon about `size` points across, centred on `c`.
pub fn icon_at(p: &Painter, c: Pos2, size: f32, icon: Icon, colour: Color32) {
    let s = size / 24.0;
    let pt = |x: f32, y: f32| pos2(c.x + (x - 12.0) * s, c.y + (y - 12.0) * s);
    let stroke = Stroke::new((2.2 * s).max(1.2), colour);
    match icon {
        Icon::ChevronLeft => {
            p.add(Shape::line(vec![pt(15.0, 5.0), pt(8.0, 12.0), pt(15.0, 19.0)], PathStroke::new(stroke.width * 1.1, colour)));
        }
        Icon::ChevronRight => {
            p.add(Shape::line(vec![pt(9.0, 5.0), pt(16.0, 12.0), pt(9.0, 19.0)], PathStroke::new(stroke.width * 1.1, colour)));
        }
        Icon::Lock => {
            p.rect_stroke(Rect::from_min_max(pt(5.0, 11.0), pt(19.0, 21.0)), 2.0 * s, stroke, StrokeKind::Middle);
            let arc: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 12.0;
                    pt(12.0 - 4.0 * a.cos(), 8.0 - 4.0 * a.sin())
                })
                .collect();
            let mut path = vec![pt(8.0, 11.0)];
            path.extend(arc);
            path.push(pt(16.0, 11.0));
            p.add(Shape::line(path, PathStroke::new(stroke.width, colour)));
        }
        Icon::SoundOn | Icon::SoundOff => {
            let body = vec![pt(4.0, 9.0), pt(8.0, 9.0), pt(13.0, 5.0), pt(13.0, 19.0), pt(8.0, 15.0), pt(4.0, 15.0)];
            p.add(Shape::closed_line(body, Stroke::new(stroke.width * 0.9, colour)));
            if icon == Icon::SoundOn {
                for (r, a) in [(4.6, 0.75), (8.0, 0.85)] {
                    let pts: Vec<Pos2> = (0..=10)
                        .map(|i| {
                            let t = -a + 2.0 * a * i as f32 / 10.0;
                            pt(13.0 + r * t.cos(), 12.0 + r * t.sin())
                        })
                        .collect();
                    p.add(Shape::line(pts, PathStroke::new(stroke.width * 0.9, colour)));
                }
            } else {
                p.line_segment([pt(17.0, 9.5), pt(22.0, 14.5)], stroke);
                p.line_segment([pt(22.0, 9.5), pt(17.0, 14.5)], stroke);
            }
        }
        Icon::Flag => {
            p.line_segment([pt(6.0, 21.0), pt(6.0, 3.5)], stroke);
            let flag = vec![pt(6.0, 4.0), pt(17.0, 4.0), pt(14.4, 8.2), pt(17.0, 12.4), pt(6.0, 12.4)];
            p.add(Shape::line(flag, PathStroke::new(stroke.width, colour)));
        }
        Icon::Restart => {
            // From the left, down and round to the top left, the arrow's head on the left.
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = (180.0 - 313.5 * i as f32 / 24.0).to_radians();
                    pt(12.0 + 8.0 * a.cos(), 12.0 + 8.0 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, PathStroke::new(stroke.width, colour)));
            p.add(Shape::line(vec![pt(4.0, 3.8), pt(4.0, 8.5), pt(8.7, 8.5)], PathStroke::new(stroke.width, colour)));
        }
    }
}

/// Five skewed bars, `on` of them lit.
pub fn pips(p: &Painter, left_center: Pos2, on: u8, w: f32, h: f32, alpha: f32) {
    for i in 0..5u8 {
        let x = left_center.x + i as f32 * (w + 3.5);
        let k = h * 0.32;
        let pts = vec![
            pos2(x + k, left_center.y - h / 2.0),
            pos2(x + w + k, left_center.y - h / 2.0),
            pos2(x + w - k, left_center.y + h / 2.0),
            pos2(x - k, left_center.y + h / 2.0),
        ];
        let c = if i < on { col::LIVERY } else { col::LINE };
        p.add(Shape::convex_polygon(pts, fade(c, alpha), Stroke::NONE));
    }
}

/// The diagonal stripes at the end of a button, as on the buggy's flanks.
pub fn stripes(p: &Painter, rect: Rect, colour: Color32) {
    let clip = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let w = 90.0f32.min(rect.width() * 0.3);
    let left = rect.right() - w;
    let slant = rect.height() / 60f32.to_radians().tan();
    let mut x = left - slant;
    while x < rect.right() + 10.0 {
        let pts = vec![pos2(x + slant, rect.top()), pos2(x + slant + 6.5, rect.top()), pos2(x + 6.5, rect.bottom()), pos2(x, rect.bottom())];
        clip.add(Shape::convex_polygon(pts, colour, Stroke::NONE));
        x += 16.5;
    }
}

/// A light band sweeping across a button every few seconds.
pub fn shine(p: &Painter, rect: Rect, now: f64) {
    let period = 3.6;
    let t = ((now - 1.0).rem_euclid(period) / period) as f32;
    if t < 0.7 {
        return;
    }
    let k = (t - 0.7) / 0.3;
    let w = rect.width() * 0.3;
    let x = rect.left() - rect.width() * 0.4 + k * rect.width() * 1.7;
    let skew = rect.height() * 0.36;
    let clip = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let mut mesh = Mesh::default();
    let clear = Color32::TRANSPARENT;
    let white = Color32::from_white_alpha(70);
    for (i, (dx, c)) in [(0.0, clear), (0.5, white), (1.0, clear)].into_iter().enumerate() {
        mesh.vertices.push(Vertex { pos: pos2(x + dx * w + skew, rect.top()), uv: egui::epaint::WHITE_UV, color: c });
        mesh.vertices.push(Vertex { pos: pos2(x + dx * w - skew, rect.bottom()), uv: egui::epaint::WHITE_UV, color: c });
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.indices.extend_from_slice(&[b - 2, b - 1, b, b - 1, b + 1, b]);
        }
    }
    clip.add(Shape::mesh(mesh));
}

/// A circuit's plan inside `rect`: the route in road and dirt colours, drawn up to `progress`
/// (0..1) of its length, with the start and the chequered finish.
pub fn route(p: &Painter, rect: Rect, info: &TrackInfo, width: f32, progress: f32, shadow: bool, alpha: f32) {
    let pts: Vec<(Pos2, bool)> = info.route.iter().map(|(q, d)| (rect.min + vec2(q.x, q.y) * rect.size(), *d)).collect();
    if pts.len() < 2 {
        return;
    }
    let mut lengths = vec![0.0f32];
    for w in pts.windows(2) {
        lengths.push(lengths.last().unwrap() + w[0].0.distance(w[1].0));
    }
    let total = *lengths.last().unwrap();
    let upto = total * progress.clamp(0.0, 1.0);
    if shadow {
        let all: Vec<Pos2> = pts.iter().map(|(q, _)| *q).collect();
        p.add(Shape::line(all, PathStroke::new(width + 4.0, fade(col::VOID, alpha))));
    }
    let mut run: Vec<Pos2> = Vec::new();
    let mut dirt = pts[0].1;
    let flush = |run: &mut Vec<Pos2>, dirt: bool| {
        if run.len() >= 2 {
            let c = if dirt { col::DIRT } else { col::ROAD };
            p.add(Shape::line(std::mem::take(run), PathStroke::new(width, fade(c, alpha))));
        }
        run.clear();
    };
    for i in 0..pts.len() {
        if lengths[i] > upto {
            // The last, partial segment.
            if i > 0 {
                let k = (upto - lengths[i - 1]) / (lengths[i] - lengths[i - 1]).max(1e-3);
                run.push(pts[i - 1].0 + (pts[i].0 - pts[i - 1].0) * k);
            }
            break;
        }
        let (q, d) = pts[i];
        if d != dirt && !run.is_empty() {
            run.push(q);
            flush(&mut run, dirt);
            dirt = d;
        }
        run.push(q);
    }
    flush(&mut run, dirt);
    if progress >= 0.999 {
        let z = pts.last().unwrap().0;
        let f = width * 1.1;
        p.rect_filled(Rect::from_center_size(z, vec2(2.0 * f, 2.0 * f)), f * 0.3, fade(col::DUST, alpha));
        p.rect_filled(Rect::from_min_size(z - vec2(f, f), vec2(f, f)), 0.0, fade(col::VOID, alpha));
        p.rect_filled(Rect::from_min_size(z, vec2(f, f)), 0.0, fade(col::VOID, alpha));
    }
    p.circle_filled(pts[0].0, width * 0.95, fade(col::HUB, alpha));
}

/// A shade of `colour` across `rect`, its opacity going through `stops` (position 0..1 along
/// the rectangle, opacity 0..1): top to bottom if `vertical`, else left to right.
pub fn gradient(p: &Painter, rect: Rect, vertical: bool, stops: &[(f32, f32)], colour: Color32) {
    let mut mesh = Mesh::default();
    for (i, &(at, a)) in stops.iter().enumerate() {
        let c = fade(colour, a);
        let (p0, p1) = if vertical {
            let y = rect.top() + at * rect.height();
            (pos2(rect.left(), y), pos2(rect.right(), y))
        } else {
            let x = rect.left() + at * rect.width();
            (pos2(x, rect.top()), pos2(x, rect.bottom()))
        };
        mesh.vertices.push(Vertex { pos: p0, uv: egui::epaint::WHITE_UV, color: c });
        mesh.vertices.push(Vertex { pos: p1, uv: egui::epaint::WHITE_UV, color: c });
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.indices.extend_from_slice(&[b - 2, b - 1, b, b - 1, b + 1, b]);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// One line of text with a soft dark shadow under it, for text straight over the footage.
pub fn text_shadowed(p: &Painter, pos: Pos2, anchor: Align2, s: &str, font: Font, color: Color32) -> Rect {
    let a = color.a() as f32 / 255.0;
    for (dx, dy, k) in [(0.0, 2.0, 0.35), (0.0, 4.0, 0.2), (1.5, 3.0, 0.15), (-1.5, 3.0, 0.15)] {
        text(p, pos + vec2(dx, dy) * (font.size / 40.0).max(0.6), anchor, s, font, Color32::from_black_alpha((255.0 * k * a) as u8));
    }
    text(p, pos, anchor, s, font, color)
}

/// CSS-style cubic Bézier easing (control points `(x1, y1)` and `(x2, y2)`), at `x` in 0..1.
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let curve = |a: f32, b: f32, t: f32| 3.0 * a * t * (1.0 - t) * (1.0 - t) + 3.0 * b * t * t * (1.0 - t) + t * t * t;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if curve(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    curve(y1, y2, 0.5 * (lo + hi))
}

/// The usual "out" easing of the menu's movements.
pub fn ease_out(x: f32) -> f32 {
    bezier(0.2, 0.8, 0.2, 1.0, x)
}

/// The screen-change band in the buggy's colours (orange, a white line, then black), sweeping
/// across in `dir`; `t` 0..1. It covers the whole screen in the middle of its run.
pub fn wipe(p: &Painter, screen: Rect, t: f32, dir: f32) {
    let w = screen.width();
    let h = screen.height();
    let e = bezier(0.7, 0.0, 0.3, 1.0, t);
    let x0 = if dir > 0.0 { -3.4 * w + 4.8 * w * e } else { 1.4 * w - 4.8 * w * e };
    let band = 3.0 * w;
    let skew = h * 0.5 * 10f32.to_radians().tan();
    let gap = 14.0;
    // Stripes as fractions of the band, left to right for a forward wipe.
    let mut parts = vec![(0.0, 0.02, col::LIVERY), (0.02, 0.878, col::VOID), (0.878, 0.89, col::DUST), (0.89, 1.0, col::LIVERY)];
    if dir < 0.0 {
        parts = parts.into_iter().rev().map(|(a, b, c)| (1.0 - b, 1.0 - a, c)).collect();
    }
    for (a, b, c) in parts {
        let l = screen.left() + x0 + a * band + gap * 0.5;
        let r = screen.left() + x0 + b * band - gap * 0.5;
        if r < screen.left() - skew || l > screen.right() + skew {
            continue;
        }
        let pts = vec![pos2(l + skew, screen.top()), pos2(r + skew, screen.top()), pos2(r - skew, screen.bottom()), pos2(l - skew, screen.bottom())];
        p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
    }
}
