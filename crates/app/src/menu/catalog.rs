//! What the menu lists: the planets and the solo circuits, read from the maps' layouts (length,
//! checkpoints, jumps, how much of the route is dirt, and the route itself for the plans).

use glam::Vec2;
use track::Surface;
use track::kit::Kind;

use crate::menu_gfx::PlanetKind;

/// Circuits per series, as in the design: five slots, the ones not built yet shown "to come".
pub const SLOTS: usize = 5;

/// Medal targets, provisional: the route's length at these average speeds (km/h).
pub const MEDALS: [(&str, f32); 3] = [("Or", 200.0), ("Argent", 180.0), ("Bronze", 160.0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Series {
    Easy,
    Hard,
}

pub enum Stat {
    Text(&'static str),
    /// Out of five.
    Pips(u8),
}

pub struct PlanetInfo {
    pub open: bool,
    pub name: &'static str,
    pub year: &'static str,
    pub lore: &'static str,
    pub stats: [(&'static str, Stat); 3],
    pub kind: PlanetKind,
}

pub const PLANETS: [PlanetInfo; 3] = [
    PlanetInfo {
        open: true,
        name: "MARS",
        year: "2036",
        lore: "Les humains ont colonisé Mars et relié leurs bases par des routes. Depuis, ils y font la course.",
        stats: [("TRANSMISSION", Stat::Text("4×4")), ("ADHÉRENCE", Stat::Pips(4)), ("GLISSE", Stat::Pips(3))],
        kind: PlanetKind::Mars,
    },
    // The ice planet's prototype: one circuit to try the ski car.
    PlanetInfo {
        open: true,
        name: "GLACE",
        year: "PROTO",
        lore: "Prototype : une voiture à skis sur la neige et la glace. Le frein fait pivoter l'arrière.",
        stats: [("TRANSMISSION", Stat::Text("PROPULSION")), ("ADHÉRENCE", Stat::Pips(2)), ("GLISSE", Stat::Pips(5))],
        kind: PlanetKind::Ice,
    },
    PlanetInfo {
        open: false,
        name: "???",
        year: "20??",
        lore: "Plus loin dans le système, une troisième piste attend.",
        stats: [("VÉHICULE", Stat::Text("?")), ("ADHÉRENCE", Stat::Pips(0)), ("GLISSE", Stat::Pips(0))],
        kind: PlanetKind::Gas,
    },
];

pub const TIPS: [&str; 4] = [
    "Sur le dirt, lâche l'accélérateur avant le virage pour faire pivoter le buggy.",
    "Les quatre roues tirent le buggy hors des virages : remets les gaz tôt.",
    "Avant un saut, arrive droit sur la rampe, roues alignées.",
    "Le bitume des installations accroche plus que le dirt : freine plus tard.",
];

pub struct TrackInfo {
    /// Index in the game's maps.
    pub map: usize,
    pub name: String,
    /// Length of the route, metres.
    pub length: f32,
    /// Share of the route on dirt, 0..1.
    pub dirt: f32,
    /// The route seen from above in a unit square (x to the east, y to the south), and whether
    /// each point is on dirt.
    pub route: Vec<(Vec2, bool)>,
    /// Share of the medal speeds this circuit allows (tight ones are slower).
    pub pace: f32,
    pub series: Series,
    /// The planet it is on, an index in [`PLANETS`].
    pub planet: usize,
}

impl TrackInfo {
    /// Target time of each medal, in ticks (centiseconds).
    pub fn medal_ticks(&self) -> [u32; 3] {
        MEDALS.map(|(_, kmh)| (self.length / (kmh * self.pace / 3.6) * 100.0).round() as u32)
    }
}

/// Share of the medal speeds a circuit allows: Noctis, all hairpins, is driven well under 200 km/h
/// on average (a clean braking lap is about 28 s, 185 km/h), so its gold is 29.6 s. Marineris,
/// all drops and boosters, is driven well over: a player flat out did 44 s (290 km/h), so its
/// gold is 46.5 s, silver 51.7 s, bronze 58.1 s.
fn pace(name: &str) -> f32 {
    match name.to_lowercase().as_str() {
        "noctis" | "noctis neige" => 0.87,
        "marineris" => 1.357,
        _ => 1.0,
    }
}

/// The series a circuit belongs to: Marineris opens the hard one.
fn series(name: &str) -> Series {
    match name.to_lowercase().as_str() {
        "marineris" => Series::Hard,
        _ => Series::Easy,
    }
}

/// Every map's card, in the game's order (each series lists its circuits in that order).
pub fn tracks(maps: &[track::Map]) -> Vec<TrackInfo> {
    maps.iter().enumerate().filter_map(|(i, map)| info(i, map)).collect()
}

fn info(index: usize, map: &track::Map) -> Option<TrackInfo> {
    let layout = map.layout().ok()?;
    let last = layout.pieces.len().checked_sub(1)?;
    let points = layout.route_points(0, 0.0, last, 8.0);
    let on_dirt = |piece: usize, s: f32| {
        let p = &layout.pieces[piece];
        match p.piece.kind {
            Kind::Transition { to } if s >= 0.5 * p.length => to == Surface::Dirt,
            _ => p.piece.deck == Surface::Dirt,
        }
    };
    // Seen from above with north up: east is −X, south is −Z.
    let flat: Vec<(Vec2, bool)> = points.iter().map(|p| (Vec2::new(-p.pos.x, -p.pos.z), on_dirt(p.piece, p.s))).collect();
    let (lo, hi) = flat.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), (p, _)| (lo.min(*p), hi.max(*p)));
    let span = (hi - lo).max_element().max(1.0);
    let offset = (Vec2::splat(span) - (hi - lo)) * 0.5;
    let route = flat.iter().map(|(p, d)| ((*p - lo + offset) / span, *d)).collect();
    let dirt = flat.iter().filter(|(_, d)| *d).count() as f32 / flat.len().max(1) as f32;
    Some(TrackInfo {
        map: index,
        name: map.name.clone(),
        length: layout.length(),
        dirt,
        route,
        pace: pace(&map.name),
        series: series(&map.name),
        planet: match map.planet {
            track::Planet::Mars => 0,
            track::Planet::Ice => 1,
        },
    })
}
