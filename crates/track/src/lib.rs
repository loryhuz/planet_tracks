//! Track geometry: the block kit and the maps built from it.
//!
//! Conventions shared by every crate:
//! - metres, y up, right-handed;
//! - a yaw of 0 faces +Z; `forward(yaw) = (sin yaw, 0, cos yaw)`, so a positive yaw turns left;
//! - for a car facing +Z, +X is its left and -X its right.

use glam::Vec3;
use serde::{Deserialize, Serialize};

pub mod demo;
pub mod dirt;
pub mod jump;
pub mod kit;
pub mod landform;
pub mod map;
mod mesh;
mod noise;
pub mod scenery;
pub mod terrain;

pub use map::{Map, MapError};

/// What a triangle is made of. The physics picks grip and drag from it, the renderer its look.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Surface {
    /// Paved roads of the installations.
    Road = 0,
    /// Martian dirt tracks.
    Dirt = 1,
    /// Off-track terrain.
    Ground = 2,
    /// Barriers, curbs and the sides of platforms.
    Wall = 3,
}

/// Triangle soup, one triangle per three indices.
///
/// Vertices are never shared between triangles of different surfaces, so a renderer can use
/// per-vertex data without seams in the wrong places.
#[derive(Clone, Debug, Default)]
pub struct TrackMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// Linear RGB, one per vertex.
    pub colors: Vec<[f32; 3]>,
    /// Track coordinates, one per vertex: metres along the route and across it (positive to the
    /// left), on decks and dirt corridors; zero elsewhere. The renderer lays tyre ruts and road
    /// markings along them.
    pub uv: Vec<[f32; 2]>,
    /// How much a vertex is worked earth, one per vertex: 0 natural ground, about ½ earth moved
    /// by the diggers (banks of a dirt corridor), 1 driven dirt.
    pub dirt: Vec<f32>,
    pub indices: Vec<u32>,
    /// One per triangle (`indices.len() / 3`).
    pub tri_surface: Vec<Surface>,
}

impl TrackMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn triangle(&self, i: usize) -> [Vec3; 3] {
        let k = 3 * i;
        [
            self.positions[self.indices[k] as usize],
            self.positions[self.indices[k + 1] as usize],
            self.positions[self.indices[k + 2] as usize],
        ]
    }

    /// Appends another mesh.
    pub fn append(&mut self, other: &TrackMesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.colors.extend_from_slice(&other.colors);
        self.uv.extend_from_slice(&other.uv);
        self.dirt.extend_from_slice(&other.dirt);
        self.indices.extend(other.indices.iter().map(|i| i + base));
        self.tri_surface.extend_from_slice(&other.tri_surface);
    }
}

/// A place on the track: the point on the driving surface under the car's centre, and a heading.
/// The physics lifts the chassis to its resting height above that point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub position: Vec3,
    pub yaw: f32,
}

impl Pose {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(libm::sinf(self.yaw), 0.0, libm::cosf(self.yaw))
    }
}

/// An oriented box the car's centre passes through (checkpoint gate, finish line).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub yaw: f32,
}

impl Trigger {
    pub fn contains(&self, p: Vec3) -> bool {
        let d = p - self.center;
        let (s, c) = (libm::sinf(self.yaw), libm::cosf(self.yaw));
        // Into the trigger's frame: rotate by -yaw around +Y.
        let x = c * d.x - s * d.z;
        let z = s * d.x + c * d.z;
        x.abs() <= self.half_extents.x && d.y.abs() <= self.half_extents.y && z.abs() <= self.half_extents.z
    }
}

/// A playable map.
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub mesh: TrackMesh,
    pub start: Pose,
    /// Every checkpoint must be crossed (in any order) before the finish counts.
    pub checkpoints: Vec<Trigger>,
    pub finish: Trigger,
    /// Below this height the car has fallen off and is respawned.
    pub fall_limit_y: f32,
    /// Driving line: the road centre from the start pose to the end of the finish block, about
    /// every 4 m, at road-surface height. It passes through every checkpoint in order (across a
    /// jump gap it is the straight line from the lip to the top of the landing).
    pub route: Vec<Vec3>,
}

/// The first playable map: « Jezero », loaded from `maps/jezero.json` (see [`demo`]).
/// Maps shipped with the game, embedded so it needs no data path: (name, JSON).
pub const BUILTIN_MAPS: [(&str, &str); 3] = [
    (demo::NAME, demo::JSON),
    ("Olympus", include_str!("../maps/olympus.json")),
    ("Ares Vallis", include_str!("../maps/ares.json")),
];

/// Every shipped map, parsed. Panics on a broken file (they are checked by the tests).
pub fn builtin_maps() -> Vec<Map> {
    BUILTIN_MAPS
        .iter()
        .map(|(name, json)| Map::load(json).unwrap_or_else(|e| panic!("map {name}: {e}")))
        .collect()
}

pub fn demo_track() -> Track {
    demo::map().build()
}
