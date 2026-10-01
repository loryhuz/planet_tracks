//! Map files: a JSON description of a map (metadata, blocks on the grid, terrain settings,
//! scenery) and the build that turns it into a playable [`Track`].
//!
//! ```json
//! {
//!   "format": 1,
//!   "name": "Jezero",
//!   "author": "mars-racer",
//!   "version": 1,
//!   "terrain": {"seed":1977,"size":8192.0,"relief":14.0,"dunes":4.0,"ripples":0.35,"bumps":0.4,"mesas":10,"craters":6,"horizon":180.0,"rocks":1.0},
//!   "blocks": [
//!     {"block":"start","cell":[13,-10],"level":0,"rotation":0},
//!     {"block":"turn2_right","cell":[13,-9],"level":0,"rotation":0},
//!     {"block":"checkpoint","cell":[-10,7],"level":0,"rotation":2,"variant":"dirt"}
//!   ],
//!   "scenery": [
//!     {"prop":"spire","position":[470.0,0.0,-170.0],"yaw":15.0,"scale":9.0}
//!   ]
//! }
//! ```
//!
//! # Blocks
//!
//! A block is placed by the cell a car enters it through (`cell`: `[x, z]`, see
//! [`crate::kit`] for the grid), the level of that entry (`level`, 8 m each) and the heading of
//! the entry (`rotation`: quarter turns to the left from north, 0 = +Z, 1 = +X, 2 = −Z, 3 = −X).
//! Blocks are directed: a car drives them from their entry to their exit. `variant` picks the
//! deck: `"road"` (the default) or `"dirt"`.
//!
//! The start, the checkpoints and the finish are blocks ([`Gate`]): the route is found by
//! following exits to entries from the start block to the finish block, and every checkpoint
//! must be on it. Blocks off the route are built too (scenery platforms, alternative lines).
//!
//! Block ids (`N` cells, `L` levels):
//!
//! | id | piece |
//! |---|---|
//! | `start`, `checkpoint`, `finish` | one straight cell with its gate |
//! | `straight`, `straightN` | straight, 1 or `N` cells |
//! | `turnN_left`, `turnN_right` | quarter turn filling `N × N` cells |
//! | `bankedN_left/right` | quarter turn banked [`BANK_DEG`] about its centreline (platforms) |
//! | `bermN_left/right` | ground-level quarter turn, outside raised by [`BANK_DEG`] |
//! | `ubermN_left/right` | ground-level U-turn (`2N × N` cells), outside raised |
//! | `slopeN_upL`, `slopeN_downL` | climb or descent of `L` levels over `N` cells |
//! | `whoopsN` | `N` cells of whoops, one [`WHOOPS_HEIGHT`] m bump per cell |
//! | `to_dirt`, `to_road` | one cell whose second half changes deck |
//! | `jump_ramp` | one cell rising to a [`LIP_DEG`]° lip, ending on a gap |
//! | `landingN_downL` | the descent that catches the jump ramp before it: placed on the cell after the ramp at the ramp's level, ending `L` levels lower |

use std::fmt;

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::dirt::Corridors;
use crate::kit::{self, CELL, Connector, FALL_LIMIT_Y, Gate, Heading, Kind, LEVEL, Layout, Piece, Pivot, Placed, Side};
use crate::mesh::MeshBuilder;
use crate::scenery::{self, PlacedProp, Prop};
use crate::terrain::{Capsule, Terrain, TerrainSettings};
use crate::{Surface, Track, TrackMesh};

/// Version of the file format this code reads and writes.
pub const FORMAT: u32 = 1;
/// Bank angle of the banked turns and berms of the catalogue, degrees.
pub const BANK_DEG: f32 = 18.0;
/// Height of a whoops bump, metres.
pub const WHOOPS_HEIGHT: f32 = 0.6;
/// Launch angle of the jump ramp, degrees.
pub const LIP_DEG: f32 = 4.0;
/// Landing shape: empty gap after the lip, touchdown mismatch, outrun length (see
/// [`crate::jump`]).
pub const LANDING_GAP: f32 = 8.0;
pub const LANDING_EPSILON: f32 = 0.07;
pub const LANDING_OUTRUN: f32 = 48.0;

fn one() -> u32 {
    1
}

/// A map as stored in a file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Map {
    /// File format, [`FORMAT`].
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub author: String,
    /// Revision of this map, bumped when its blocks change (records are kept per version).
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub terrain: TerrainSettings,
    pub blocks: Vec<BlockPlacement>,
    #[serde(default)]
    pub scenery: Vec<Prop>,
}

/// One block on the grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPlacement {
    /// Catalogue id (see the module documentation).
    pub block: String,
    /// The cell the block is entered through, `[x, z]`.
    pub cell: [i32; 2],
    /// Level of the entry.
    pub level: i32,
    /// Entry heading, quarter turns to the left from north (+Z).
    pub rotation: u8,
    /// Deck: `"road"` (default) or `"dirt"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MapError {
    Json(String),
    Format(u32),
    UnknownBlock { index: usize, id: String },
    BadVariant { index: usize, variant: String },
    BadRotation { index: usize, rotation: u8 },
    /// A landing whose entry is not the lip of a jump ramp.
    LandingWithoutRamp { index: usize },
    /// No start or no finish block.
    MissingGate(&'static str),
    /// More than one start or finish block.
    DuplicateGate(&'static str),
    /// Two blocks start where this one ends.
    AmbiguousJoin { index: usize },
    /// The route from the start stops at this block before reaching the finish.
    RouteBroken { at: usize },
    /// A checkpoint the route from the start never crosses.
    Unreachable { index: usize },
    /// A layout piece with no catalogue block.
    NotInCatalogue(String),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::Json(e) => write!(f, "invalid map JSON: {e}"),
            MapError::Format(v) => write!(f, "map format {v} is not supported (expected {FORMAT})"),
            MapError::UnknownBlock { index, id } => write!(f, "block {index}: unknown block id {id:?}"),
            MapError::BadVariant { index, variant } => write!(f, "block {index}: variant {variant:?} does not apply"),
            MapError::BadRotation { index, rotation } => write!(f, "block {index}: rotation {rotation} (expected 0 to 3)"),
            MapError::LandingWithoutRamp { index } => write!(f, "block {index}: a landing must follow a jump_ramp at the same level"),
            MapError::MissingGate(g) => write!(f, "the map has no {g} block"),
            MapError::DuplicateGate(g) => write!(f, "the map has more than one {g} block"),
            MapError::AmbiguousJoin { index } => write!(f, "block {index}: several blocks start at its exit"),
            MapError::RouteBroken { at } => write!(f, "the route from the start stops at block {at} before the finish"),
            MapError::Unreachable { index } => write!(f, "block {index}: checkpoint not on the route from the start"),
            MapError::NotInCatalogue(p) => write!(f, "no catalogue block for {p}"),
        }
    }
}

impl std::error::Error for MapError {}

// ---------------------------------------------------------------------------------------------
// Catalogue.

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}

/// `name` followed by a number: `("turn", Some(2))` for `turn2`.
fn split_number(s: &str) -> (&str, Option<u32>) {
    let cut = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let (name, num) = s.split_at(cut);
    (name, if num.is_empty() { None } else { num.parse().ok().filter(|n| (1..=32).contains(n)) })
}

/// Why a block id and variant do not name a catalogue piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    UnknownId,
    /// The variant is neither road nor dirt, or the block's deck is fixed (transitions).
    BadVariant,
}

/// The shape of a catalogue id, for ids other than gates and transitions.
fn parse_shape(id: &str) -> Option<Kind> {
    if id == "jump_ramp" {
        return Some(Kind::JumpRamp { lip_deg: LIP_DEG });
    }
    let (head, tail) = id.split_once('_').unwrap_or((id, ""));
    let (name, n) = split_number(head);
    let side = match tail {
        "left" => Some(Side::Left),
        "right" => Some(Side::Right),
        _ => None,
    };
    let levels = |prefix: &str| tail.strip_prefix(prefix).and_then(|l| l.parse::<i32>().ok()).filter(|l| (1..=8).contains(l));
    Some(match (name, n, side) {
        ("straight", n, None) if tail.is_empty() => Kind::Straight { cells: n.unwrap_or(1) },
        ("whoops", Some(n), None) if tail.is_empty() => Kind::Whoops { cells: n, bumps: n, height: WHOOPS_HEIGHT },
        ("turn", Some(n), Some(s)) => Kind::turn(n, s),
        ("banked", Some(n), Some(s)) => Kind::banked(n, s, BANK_DEG),
        ("berm", Some(n), Some(s)) => Kind::berm(n, s, 1, BANK_DEG),
        ("uberm", Some(n), Some(s)) => Kind::berm(n, s, 2, BANK_DEG),
        ("slope", Some(n), None) => match (levels("up"), levels("down")) {
            (Some(l), _) => Kind::Slope { cells: n, levels: l },
            (_, Some(l)) => Kind::Slope { cells: n, levels: -l },
            _ => return None,
        },
        ("landing", Some(n), None) => Kind::Landing {
            cells: n,
            levels: -levels("down")?,
            gap: LANDING_GAP,
            epsilon: LANDING_EPSILON,
            outrun: LANDING_OUTRUN,
        },
        _ => return None,
    })
}

/// The piece a catalogue id and variant stand for.
pub fn parse_block(id: &str, variant: Option<&str>) -> Result<Piece, BlockError> {
    let transition = match id {
        "to_dirt" => Some((Surface::Road, Surface::Dirt)),
        "to_road" => Some((Surface::Dirt, Surface::Road)),
        _ => None,
    };
    if let Some((deck, to)) = transition {
        return match variant {
            None => Ok(Piece { kind: Kind::Transition { to }, deck, gate: None }),
            Some(_) => Err(BlockError::BadVariant),
        };
    }
    let gate = match id {
        "start" => Some(Gate::Start),
        "checkpoint" => Some(Gate::Checkpoint),
        "finish" => Some(Gate::Finish),
        _ => None,
    };
    let kind = if gate.is_some() { Kind::Straight { cells: 1 } } else { parse_shape(id).ok_or(BlockError::UnknownId)? };
    let deck = match variant {
        None | Some("road") => Surface::Road,
        Some("dirt") => Surface::Dirt,
        Some(_) => return Err(BlockError::BadVariant),
    };
    Ok(Piece { kind, deck, gate })
}

/// The catalogue id and variant of a piece, or `None` when the catalogue has no such block.
pub fn block_id(piece: &Piece) -> Option<(String, Option<String>)> {
    let dirt = (piece.deck == Surface::Dirt).then(|| "dirt".to_string());
    if let Some(g) = piece.gate {
        if piece.kind != (Kind::Straight { cells: 1 }) {
            return None;
        }
        let id = match g {
            Gate::Start => "start",
            Gate::Checkpoint => "checkpoint",
            Gate::Finish => "finish",
        };
        return Some((id.into(), dirt));
    }
    let id = match piece.kind {
        Kind::Straight { cells: 1 } => "straight".into(),
        Kind::Straight { cells } => format!("straight{cells}"),
        Kind::Turn { size, side, quarters: 1, bank_deg: 0.0, pivot: Pivot::Centre } => format!("turn{size}_{}", side_name(side)),
        Kind::Turn { size, side, quarters: 1, bank_deg, pivot: Pivot::Centre } if bank_deg == BANK_DEG => format!("banked{size}_{}", side_name(side)),
        Kind::Turn { size, side, quarters: 1, bank_deg, pivot: Pivot::Inner } if bank_deg == BANK_DEG => format!("berm{size}_{}", side_name(side)),
        Kind::Turn { size, side, quarters: 2, bank_deg, pivot: Pivot::Inner } if bank_deg == BANK_DEG => format!("uberm{size}_{}", side_name(side)),
        Kind::Slope { cells, levels } if levels > 0 => format!("slope{cells}_up{levels}"),
        Kind::Slope { cells, levels } if levels < 0 => format!("slope{cells}_down{}", -levels),
        Kind::Whoops { cells, bumps, height } if bumps == cells && height == WHOOPS_HEIGHT => format!("whoops{cells}"),
        Kind::Transition { to: Surface::Dirt } if piece.deck == Surface::Road => return Some(("to_dirt".into(), None)),
        Kind::Transition { to: Surface::Road } if piece.deck == Surface::Dirt => return Some(("to_road".into(), None)),
        Kind::JumpRamp { lip_deg } if lip_deg == LIP_DEG => "jump_ramp".into(),
        Kind::Landing { cells, levels, gap, epsilon, outrun }
            if levels < 0 && gap == LANDING_GAP && epsilon == LANDING_EPSILON && outrun == LANDING_OUTRUN =>
        {
            format!("landing{cells}_down{}", -levels)
        }
        _ => return None,
    };
    Some((id, dirt))
}

/// Every catalogue id with sizes up to 3 cells and 2 levels (an editor's palette).
pub fn catalogue() -> Vec<String> {
    let mut out: Vec<String> = ["start", "checkpoint", "finish", "straight", "to_dirt", "to_road", "jump_ramp"].map(String::from).to_vec();
    for n in 1..=3 {
        out.push(format!("whoops{n}"));
        for kind in ["turn", "banked", "berm", "uberm"] {
            for side in ["left", "right"] {
                out.push(format!("{kind}{n}_{side}"));
            }
        }
    }
    for n in 1..=4 {
        for l in 1..=2 {
            out.push(format!("slope{n}_up{l}"));
            out.push(format!("slope{n}_down{l}"));
        }
    }
    for n in 4..=8 {
        for l in 1..=2 {
            out.push(format!("landing{n}_down{l}"));
        }
    }
    out
}

impl Heading {
    /// Quarter turns to the left from north.
    pub fn rotation(self) -> u8 {
        match self {
            Heading::North => 0,
            Heading::West => 1,
            Heading::South => 2,
            Heading::East => 3,
        }
    }

    pub fn from_rotation(r: u8) -> Option<Self> {
        match r {
            0 => Some(Heading::North),
            1 => Some(Heading::West),
            2 => Some(Heading::South),
            3 => Some(Heading::East),
            _ => None,
        }
    }
}

/// Level a connector stands on (for a jump lip, the level of the ramp's base).
fn base_level(c: &Connector) -> i32 {
    libm::floorf(c.pos.y / LEVEL + 1e-4) as i32
}

/// Grid placements for a chain of pieces (the inverse of [`Map::layout`]).
pub fn blocks_from_layout(layout: &Layout) -> Result<Vec<BlockPlacement>, MapError> {
    layout
        .pieces
        .iter()
        .map(|p| {
            let (block, variant) = block_id(&p.piece).ok_or_else(|| MapError::NotInCatalogue(format!("{:?}", p.piece)))?;
            let c = p.entry.pos + p.entry.heading.forward() * (0.5 * CELL);
            Ok(BlockPlacement {
                block,
                cell: [libm::floorf(c.x / CELL) as i32, libm::floorf(c.z / CELL) as i32],
                level: base_level(&p.entry),
                rotation: p.entry.heading.rotation(),
                variant,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Resolution and build.

/// Blocks placed in the world, with the route through them.
struct Resolved {
    /// In file order.
    pieces: Vec<Placed>,
    /// Indices of the route's blocks, start to finish.
    chain: Vec<usize>,
    /// Whether each block's start and end are open (no deck joins them).
    open: Vec<(bool, bool)>,
    /// The block whose entry each block's exit joins, if any.
    next: Vec<Option<usize>>,
}

/// Counts of the parts of a built map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TriangleCounts {
    pub blocks: usize,
    pub terrain: usize,
    pub scenery: usize,
}

/// A built map with the parts tools and checks need.
pub struct BuiltMap {
    pub track: Track,
    /// The route's blocks as a chain.
    pub layout: Layout,
    pub terrain: Terrain,
    /// Every prop (the map's, then the automatic scatter).
    pub props: Vec<PlacedProp>,
    pub triangles: TriangleCounts,
}

impl Map {
    /// Parses and validates a map.
    pub fn load(json: &str) -> Result<Map, MapError> {
        let map: Map = serde_json::from_str(json).map_err(|e| MapError::Json(e.to_string()))?;
        if map.format != FORMAT {
            return Err(MapError::Format(map.format));
        }
        map.resolve()?;
        Ok(map)
    }

    /// The map as JSON: one line per block and per prop, so diffs stay readable.
    pub fn to_json(&self) -> String {
        let list = |items: Vec<String>| {
            if items.is_empty() { "[]".to_string() } else { format!("[\n    {}\n  ]", items.join(",\n    ")) }
        };
        format!(
            "{{\n  \"format\": {},\n  \"name\": {},\n  \"author\": {},\n  \"version\": {},\n  \"terrain\": {},\n  \"blocks\": {},\n  \"scenery\": {}\n}}\n",
            self.format,
            compact(&self.name),
            compact(&self.author),
            self.version,
            compact(&self.terrain),
            list(self.blocks.iter().map(compact).collect()),
            list(self.scenery.iter().map(compact).collect()),
        )
    }

    fn resolve(&self) -> Result<Resolved, MapError> {
        let mut pieces = Vec::with_capacity(self.blocks.len());
        for (index, b) in self.blocks.iter().enumerate() {
            let piece = parse_block(&b.block, b.variant.as_deref()).map_err(|e| match e {
                BlockError::UnknownId => MapError::UnknownBlock { index, id: b.block.clone() },
                BlockError::BadVariant => MapError::BadVariant { index, variant: b.variant.clone().unwrap_or_default() },
            })?;
            let heading = Heading::from_rotation(b.rotation).ok_or(MapError::BadRotation { index, rotation: b.rotation })?;
            pieces.push(Placed::new(piece, Connector::entering((b.cell[0], b.cell[1]), b.level, heading)));
        }
        // A landing starts at the lip of the ramp that feeds it.
        for i in 0..pieces.len() {
            if !matches!(pieces[i].piece.kind, Kind::Landing { .. }) {
                continue;
            }
            let nominal = pieces[i].entry;
            let ramp = pieces.iter().find(|r| {
                matches!(r.piece.kind, Kind::JumpRamp { .. })
                    && r.exit.heading == nominal.heading
                    && Vec2::new(r.exit.pos.x - nominal.pos.x, r.exit.pos.z - nominal.pos.z).length() < 1e-3
                    && base_level(&r.entry) == base_level(&nominal)
            });
            let entry = ramp.ok_or(MapError::LandingWithoutRamp { index: i })?.exit;
            pieces[i] = Placed::new(pieces[i].piece, entry);
        }
        let joins = |a: &Connector, b: &Connector| a.heading == b.heading && a.pos.distance(b.pos) < 1e-3;
        let mut next: Vec<Option<usize>> = Vec::with_capacity(pieces.len());
        for (i, p) in pieces.iter().enumerate() {
            let mut found = (0..pieces.len()).filter(|&j| j != i && joins(&p.exit, &pieces[j].entry));
            let first = found.next();
            if found.next().is_some() {
                return Err(MapError::AmbiguousJoin { index: i });
            }
            next.push(first);
        }
        let unique = |gate: Gate, name: &'static str| -> Result<usize, MapError> {
            let mut it = (0..pieces.len()).filter(|&i| pieces[i].piece.gate == Some(gate));
            let first = it.next().ok_or(MapError::MissingGate(name))?;
            if it.next().is_some() {
                return Err(MapError::DuplicateGate(name));
            }
            Ok(first)
        };
        let (start, finish) = (unique(Gate::Start, "start")?, unique(Gate::Finish, "finish")?);
        let mut chain = vec![start];
        let mut cur = start;
        while cur != finish {
            match next[cur] {
                Some(j) if !chain.contains(&j) => {
                    chain.push(j);
                    cur = j;
                }
                _ => return Err(MapError::RouteBroken { at: cur }),
            }
        }
        if let Some(index) = (0..pieces.len()).find(|&i| pieces[i].piece.gate == Some(Gate::Checkpoint) && !chain.contains(&i)) {
            return Err(MapError::Unreachable { index });
        }
        let gapless = |j: usize| pieces[j].deck_range().0 == 0.0;
        let open = (0..pieces.len())
            .map(|i| {
                let joined_in = gapless(i) && next.contains(&Some(i));
                let joined_out = next[i].is_some_and(gapless);
                (!joined_in, !joined_out)
            })
            .collect();
        Ok(Resolved { pieces, chain, open, next })
    }

    /// The route's blocks as a chain, start to finish.
    pub fn layout(&self) -> Result<Layout, MapError> {
        let r = self.resolve()?;
        Ok(Self::chain_layout(&self.name, &r))
    }

    fn chain_layout(name: &str, r: &Resolved) -> Layout {
        let mut layout = Layout::new(name, r.pieces[r.chain[0]].entry);
        layout.pieces = r.chain.iter().map(|&i| r.pieces[i].clone()).collect();
        layout
    }

    /// Builds the playable track. Panics on an invalid map (maps from [`Map::load`] are valid);
    /// see [`Map::build_detailed`].
    pub fn build(&self) -> Track {
        match self.build_detailed() {
            Ok(b) => b.track,
            Err(e) => panic!("map {:?}: {e}", self.name),
        }
    }

    /// Builds the track: swept blocks, terrain flattened under them with the dirt corridors dug
    /// into it (see [`crate::dirt`]), gates, scenery.
    pub fn build_detailed(&self) -> Result<BuiltMap, MapError> {
        let r = self.resolve()?;
        // Distance along the route at each block's entry (0 off the route).
        let mut route_s = vec![0.0; r.pieces.len()];
        let mut dist = 0.0;
        for &i in &r.chain {
            route_s[i] = dist;
            dist += r.pieces[i].length;
        }
        // Swept blocks and their footprints.
        let mut b = MeshBuilder::default();
        let mut caps = Vec::new();
        for (i, (p, &(open_start, open_end))) in r.pieces.iter().zip(&r.open).enumerate() {
            let Some(range) = p.swept_range() else { continue };
            let (d0, d1) = p.deck_range();
            // The side where a transition meets its own corridor is never capped.
            let open_start = open_start && range.0 <= d0 + 1e-3;
            let open_end = open_end && range.1 >= d1 - 1e-3;
            // A swept dirt piece sits in a corridor of its own instead of a flat pad: only its
            // deck is swept, the terrain comes up to its edges.
            let bedded = p.piece.deck == Surface::Dirt && !matches!(p.piece.kind, Kind::Transition { .. });
            kit::sweep(&mut b, p, range, route_s[i], open_start, open_end, bedded);
            if bedded {
                continue;
            }
            let discs = kit::footprint(p, range);
            for w in discs.windows(2) {
                caps.push(Capsule { a: w[0].0, b: w[1].0, r: w[0].1.max(w[1].1) });
            }
        }
        // Terrain, with the dirt corridors.
        let dirt = Corridors::new(&r.pieces, &route_s, &r.next, self.terrain.seed);
        let terrain = Terrain::new(&self.terrain, kit::centre_of(&r.pieces), caps, dirt);
        // Gates, standing on the terrain.
        for p in &r.pieces {
            if let Some(g) = p.piece.gate {
                let f = p.frame(p.gate_s());
                let carved = p.carved_range().is_some_and(|(c0, c1)| (c0..=c1).contains(&p.gate_s()));
                kit::gate(&mut b, &f, g, |q| if carved { post_ground(&terrain, q) } else { kit::TERRAIN_Y });
            }
        }
        let mut mesh = b.finish();
        let blocks = mesh.triangle_count();
        let ground = terrain.mesh();
        let lowest = ground.positions.iter().map(|p| p.y).fold(kit::TERRAIN_Y, f32::min);
        mesh.append(&ground);
        // Route, triggers, then scenery away from the route.
        let layout = Self::chain_layout(&self.name, &r);
        let mut track = layout.track(TrackMesh::default(), FALL_LIMIT_Y.min(lowest - 20.0));
        let mut props = self.scenery.clone();
        let scattered = scenery::scatter(&terrain, &track.route, &self.scenery);
        props.extend(scattered);
        let rocks = scenery::props_mesh(&props, &terrain);
        mesh.append(&rocks);
        track.mesh = mesh;
        Ok(BuiltMap {
            props: scenery::placed(&props, &terrain),
            triangles: TriangleCounts { blocks, terrain: ground.triangle_count(), scenery: rocks.triangle_count() },
            track,
            layout,
            terrain,
        })
    }
}

/// Foot of a gate post standing at `q` on sloping ground: a little below the lowest ground
/// around it, so it never floats.
fn post_ground(terrain: &Terrain, q: Vec3) -> f32 {
    let r = 0.8;
    [(0.0, 0.0), (r, r), (r, -r), (-r, r), (-r, -r)]
        .iter()
        .map(|(dx, dz)| terrain.height(q.x + dx, q.z + dz))
        .fold(f32::MAX, f32::min)
        - 0.3
}

/// Compact JSON for the parts of [`Map::to_json`].
fn compact<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).expect("map values serialise")
}
