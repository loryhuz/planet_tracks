//! The demo map, « Jezero »: about 2.1 km, three checkpoints, built only from kit pieces and
//! stored in `maps/jezero.json` (blocks, terrain settings, landforms, scenery). Buttes, mesas and
//! an escarpment stand along the route (see [`crate::landform`]), the start on top of one of
//! them.
//!
//! Route (compass as seen from above with north = +Z, east = −X):
//! 1. start block 16 m up on a butte, a 2-cell plunge to the ground (26° at its steepest), then
//!    a 2-cell right and a 3-cell left sweeper;
//! 2. a 2-level climb over 4 cells onto a platform 16 m up;
//! 3. elevated S-bends: 2-cell right banked 18°, 1-cell left hairpin, 3-cell right sweeper;
//! 4. checkpoint 1, which is also the run-up, a 4° jump ramp, an 8 m gap and a 7-cell landing
//!    descent back to the ground (see [`crate::jump`] for which speeds and gravities it catches);
//! 5. road → dirt, a 2-cell dirt left, 3 cells of whoops, a U-turn berm banked 18°, checkpoint 2,
//!    another 2-cell dirt left, dirt → road;
//! 6. a 3-cell right sweeper, a 1-level climb, 2-cell right, checkpoint 3 on the 8 m platform,
//!    2-cell left, 1-level descent, then a 1-cell right, 2-cell left, 2-cell right S to the finish.
//!
//! 2141 m of centreline: 38.5 s at a 200 km/h average. Crests: the 4-cell climb has a 333 m crest
//! radius and the 3-cell slopes 374 m, so the car stays on the ground over them up to
//! √(g·R): 206/218 km/h at 9.81 m/s², 412/437 km/h at 40 m/s² (the reference). The opening
//! plunge (83 m) is taken from a standing start: up to 207 km/h at 40 m/s², and the car only
//! hops off its crest above 103 km/h at 9.81. The whoops
//! (0.6 m, every 32 m, 86 m crest radius) make it hop above 105 km/h at 9.81 and 212 km/h at 40,
//! which is their purpose.
//!
//! # Jump envelope
//!
//! Lip speeds that clear the 8 m gap and touch down within 5° of the landing slope (the design
//! angle is 4.0° on the whole parabolic part; `cargo run -p track --example plan` prints and
//! draws it):
//!
//! | gravity (m/s²) | clean lip speeds (km/h) | at 200 km/h |
//! |---|---|---|
//! | 3.71 (Mars) | 52 – 171 | flies 236 m, lands on the flat at 12° |
//! | 9.81 | 84 – 279 | 60 m, 1.1 s in the air |
//! | 20 | 120 – 398 | 25 m, 0.45 s |
//! | 30 | 146 – 488 | 16 m, 0.28 s |
//! | 40 (reference) | 169 – 564 | 12 m, 0.21 s |
//!
//! A fast player reaches the lip at roughly 200-240 km/h, so every gravity from Earth to the
//! reference lands cleanly; Mars gravity needs to lift off below ~170 km/h. The price of that
//! range is a short hop under heavy gravity: the air time for a given touchdown angle only grows
//! when the landing follows one gravity's parabola, which would throw the others off.

use crate::kit::Layout;
use crate::map::Map;

pub const NAME: &str = "Jezero";

/// The map file, embedded so the game needs no data path.
pub const JSON: &str = include_str!("../maps/jezero.json");

/// The demo map. The start block sits in cell (13, −12), two levels up, so the map is centred
/// on the origin.
pub fn map() -> Map {
    Map::load(JSON).unwrap_or_else(|e| panic!("maps/jezero.json: {e}"))
}

/// The demo map's route as a chain of pieces.
pub fn layout() -> Layout {
    map().layout().unwrap_or_else(|e| panic!("maps/jezero.json: {e}"))
}
