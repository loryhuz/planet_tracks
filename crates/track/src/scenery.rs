//! Scenery: procedural low-poly rocks, placed by the map's scenery list and scattered
//! automatically away from the driving line.
//!
//! Placeholders until the art direction is chosen: a few deterministic shapes (an icosahedron
//! for small rocks, a subdivided one for boulders, a flattened one for slabs, a stacked column for
//! spires), jittered per instance, flat-shaded with vertex colours. Rocks wider than
//! [`WALL_SIZE`] are [`Surface::Wall`] (the car bumps into them), smaller ones
//! [`Surface::Ground`].

use std::collections::{BTreeMap, HashMap};

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::kit::{color, smoothstep};
use crate::mesh::MeshBuilder;
use crate::noise::{Rng, fbm, hash2, unit};
use crate::terrain::Terrain;
use crate::{Surface, TrackMesh};

/// No automatic rock closer than this to the driving line (edge of the rock to the road
/// centre), metres.
pub const ROUTE_CLEARANCE: f32 = 25.0;
/// Rocks at least this wide collide as walls.
pub const WALL_SIZE: f32 = 1.5;
/// The automatic scatter covers ground up to this far from the blocks.
const SCATTER_RANGE: f32 = 1100.0;
const SCATTER_CELL: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropKind {
    /// A rounded boulder, about as deep as wide and two thirds as tall.
    Boulder,
    /// A small angular rock.
    Rock,
    /// A flat, tilted slab.
    Slab,
    /// A rock column (hoodoo), about twice as tall as wide.
    Spire,
}

fn one() -> f32 {
    1.0
}

/// A prop placed by the map author.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prop {
    pub prop: PropKind,
    /// `[x, lift, z]`, metres: `lift` raises the prop above the terrain under it (0 rests it on
    /// the ground, partly buried like a real rock).
    pub position: [f32; 3],
    /// Degrees, positive turns left (as a car's yaw).
    #[serde(default)]
    pub yaw: f32,
    /// Width of the prop, metres.
    #[serde(default = "one")]
    pub scale: f32,
    /// Shape variant; derived from the position when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<u32>,
}

impl Prop {
    fn variant_seed(&self) -> u32 {
        self.variant.unwrap_or_else(|| hash2(0x5eed, libm::roundf(self.position[0] * 10.0) as i32, libm::roundf(self.position[2] * 10.0) as i32))
    }

    /// Horizontal radius, metres.
    pub fn radius(&self) -> f32 {
        0.5 * self.scale
    }
}

/// Unit icosahedron: vertices and faces wound outwards.
fn icosahedron() -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let t = (1.0 + libm::sqrtf(5.0)) / 2.0;
    let v = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ]
    .map(|(x, y, z)| Vec3::new(x, y, z).normalize())
    .to_vec();
    let f = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    (v, f)
}

/// Splits every face in four, new vertices pushed onto the unit sphere.
fn subdivide(v: &mut Vec<Vec3>, faces: &[[usize; 3]]) -> Vec<[usize; 3]> {
    let mut mid: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut get = |a: usize, b: usize, v: &mut Vec<Vec3>| -> usize {
        let key = (a.min(b), a.max(b));
        *mid.entry(key).or_insert_with(|| {
            v.push((v[a] + v[b]).normalize());
            v.len() - 1
        })
    };
    let mut out = Vec::with_capacity(faces.len() * 4);
    for &[a, b, c] in faces {
        let (ab, bc, ca) = (get(a, b, v), get(b, c, v), get(c, a, v));
        out.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
    }
    out
}

/// A rock in its own frame: base at the origin, width about 1, y up. Faces wound outwards.
fn shape(kind: PropKind, seed: u32) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let mut rng = Rng::new(u64::from(seed) | (1u64 << 40));
    match kind {
        PropKind::Spire => {
            const SIDES: usize = 6;
            let rings = [(0.0, 0.5), (0.3, 0.44), (0.58, 0.34), (0.8, 0.42), (1.0, 0.36)];
            let height = 1.9 * rng.range(0.85, 1.15);
            let mut v = Vec::new();
            for (k, &(h, r)) in rings.iter().enumerate() {
                let twist = rng.range(0.0, 0.5);
                for i in 0..SIDES {
                    let a = (i as f32 + twist + 0.5 * (k % 2) as f32) * core::f32::consts::TAU / SIDES as f32;
                    let rr = r * rng.range(0.82, 1.15);
                    v.push(Vec3::new(rr * libm::cosf(a), h * height, rr * libm::sinf(a)));
                }
            }
            let top = v.len();
            v.push(Vec3::new(0.0, height * 1.03, 0.0));
            let mut f = Vec::new();
            for k in 0..rings.len() - 1 {
                for i in 0..SIDES {
                    let (a, b) = (k * SIDES + i, k * SIDES + (i + 1) % SIDES);
                    let (c, d) = (a + SIDES, b + SIDES);
                    f.push([a, c, b]);
                    f.push([b, c, d]);
                }
            }
            let last = (rings.len() - 1) * SIDES;
            for i in 0..SIDES {
                f.push([last + i, top, last + (i + 1) % SIDES]);
            }
            (v, f)
        }
        _ => {
            let (mut v, mut f) = icosahedron();
            let (jag, dims) = match kind {
                PropKind::Boulder => {
                    f = subdivide(&mut v, &f);
                    (0.16, Vec3::new(1.0, rng.range(0.55, 0.75), rng.range(0.75, 0.95)))
                }
                PropKind::Rock => (0.3, Vec3::new(1.0, rng.range(0.6, 0.85), rng.range(0.65, 0.9))),
                _ => (0.18, Vec3::new(1.0, rng.range(0.22, 0.32), rng.range(0.6, 0.8))),
            };
            let lean = Vec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            for p in &mut v {
                let r = 1.0 + jag * rng.range(-1.0, 1.0) + 0.12 * p.dot(lean);
                *p = *p * r * 0.5 * dims;
            }
            // Sit the widest part a little above the ground.
            let lift = 0.5 * dims.y * 0.55;
            for p in &mut v {
                p.y += lift;
            }
            if kind == PropKind::Slab {
                let tilt = Quat::from_rotation_x(rng.range(0.12, 0.35));
                for p in &mut v {
                    *p = tilt * *p;
                }
            }
            (v, f)
        }
    }
}

/// Builds the props into a mesh, standing on the terrain.
pub(crate) fn props_mesh(props: &[Prop], terrain: &Terrain) -> TrackMesh {
    let mut b = MeshBuilder::default();
    for prop in props {
        add_prop(&mut b, prop, terrain);
    }
    b.finish()
}

fn add_prop(b: &mut MeshBuilder, prop: &Prop, terrain: &Terrain) {
    let seed = prop.variant_seed();
    let (local, faces) = shape(prop.prop, seed);
    let [x, lift, z] = prop.position;
    let r = prop.radius();
    // Lowest ground under the rock, so no edge floats on a slope.
    let mut ground = terrain.height(x, z);
    for i in 0..6 {
        let a = i as f32 * core::f32::consts::TAU / 6.0;
        ground = ground.min(terrain.height(x + 0.8 * r * libm::cosf(a), z + 0.8 * r * libm::sinf(a)));
    }
    let sink = match prop.prop {
        PropKind::Spire => 0.4 + 0.1 * prop.scale,
        _ => 0.12 * prop.scale,
    };
    let base = Vec3::new(x, ground - sink + lift, z);
    let rot = Quat::from_rotation_y(prop.yaw.to_radians());
    let world: Vec<Vec3> = local.iter().map(|p| base + rot * (*p * prop.scale)).collect();
    let surface = if prop.scale >= WALL_SIZE { Surface::Wall } else { Surface::Ground };
    let mut rng = Rng::new(u64::from(seed).wrapping_mul(31).wrapping_add(7));
    let shade = rng.range(0.78, 1.12);
    let warm = rng.range(-0.06, 0.06);
    let rock = [0.30 * shade * (1.0 + warm), 0.13 * shade, 0.075 * shade * (1.0 - warm)];
    let dust = color::GROUND.map(|c| c * 1.04);
    let centre = local.iter().fold(Vec3::ZERO, |a, p| a + *p) / local.len() as f32;
    for f in faces {
        let mut t = f.map(|i| world[i]);
        if t.iter().all(|p| p.y < ground - 0.05) {
            continue;
        }
        // Wind outwards (away from the rock's centre).
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let mid = (local[f[0]] + local[f[1]] + local[f[2]]) / 3.0;
        if n.dot(rot * (mid - centre)) < 0.0 {
            t.swap(1, 2);
        }
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let longest = (t[1] - t[0]).length().max((t[2] - t[1]).length()).max((t[0] - t[2]).length());
        if n.length() <= 1e-3 * longest {
            continue;
        }
        let up = n.normalize().y;
        let k = 0.65 * smoothstep(0.55, 0.95, up);
        let c = [0, 1, 2].map(|i| (rock[i] + (dust[i] - rock[i]) * k).clamp(0.0, 1.0));
        b.flat_tri(t, surface, c);
    }
}

/// Distance queries to the driving line, bucketed.
struct RouteIndex<'a> {
    route: &'a [Vec3],
    buckets: HashMap<(i32, i32), Vec<u32>>,
}

const BUCKET: f32 = 64.0;

impl<'a> RouteIndex<'a> {
    fn new(route: &'a [Vec3]) -> Self {
        let mut buckets: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for i in 0..route.len().saturating_sub(1) {
            let (a, b) = (route[i], route[i + 1]);
            let mut keys = vec![Self::key(a.x, a.z), Self::key(b.x, b.z)];
            keys.dedup();
            for k in keys {
                buckets.entry(k).or_default().push(i as u32);
            }
        }
        Self { route, buckets }
    }

    fn key(x: f32, z: f32) -> (i32, i32) {
        (libm::floorf(x / BUCKET) as i32, libm::floorf(z / BUCKET) as i32)
    }

    /// Horizontal distance to the route, capped at `BUCKET` (anything farther is "far").
    fn distance(&self, p: Vec2) -> f32 {
        let (kx, kz) = Self::key(p.x, p.y);
        let mut best = BUCKET;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let Some(list) = self.buckets.get(&(kx + dx, kz + dz)) else { continue };
                for &i in list {
                    let (a, b) = (self.route[i as usize], self.route[i as usize + 1]);
                    let (a, b) = (Vec2::new(a.x, a.z), Vec2::new(b.x, b.z));
                    let ab = b - a;
                    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                    best = best.min(p.distance(a + ab * t));
                }
            }
        }
        best
    }
}

/// Clearance check for a rock of radius `r` at `p`: away from the driving line and every block.
fn clear_of_track(route: &RouteIndex, terrain: &Terrain, p: Vec2, r: f32) -> bool {
    route.distance(p) >= ROUTE_CLEARANCE + r && terrain.distance(p.x, p.y) >= 8.0 + r
}

/// Rocks scattered over the terrain by its settings: clusters of small rocks and boulders, a
/// few slabs and spires farther out, bigger with distance; none within [`ROUTE_CLEARANCE`] of
/// the driving line, near a block, on a steep slope or on one of the map's own props.
pub(crate) fn scatter(terrain: &Terrain, route: &[Vec3], placed: &[Prop]) -> Vec<Prop> {
    let s = &terrain.settings;
    if s.rocks <= 0.0 || route.is_empty() {
        return Vec::new();
    }
    let index = RouteIndex::new(route);
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in route {
        lo = lo.min(Vec2::new(p.x, p.z));
        hi = hi.max(Vec2::new(p.x, p.z));
    }
    lo -= Vec2::splat(SCATTER_RANGE);
    hi += Vec2::splat(SCATTER_RANGE);
    let seed = s.seed ^ 0x0bad_5eed;
    let (i0, i1) = (libm::floorf(lo.x / SCATTER_CELL) as i32, libm::ceilf(hi.x / SCATTER_CELL) as i32);
    let (k0, k1) = (libm::floorf(lo.y / SCATTER_CELL) as i32, libm::ceilf(hi.y / SCATTER_CELL) as i32);
    let mut out = Vec::new();
    for k in k0..k1 {
        for i in i0..i1 {
            let h = hash2(seed, i, k);
            let u = |n: u32| unit(hash2(h, n as i32, 0));
            let p = Vec2::new((i as f32 + u(1)) * SCATTER_CELL, (k as f32 + u(2)) * SCATTER_CELL);
            let d = terrain.distance(p.x, p.y);
            if d > SCATTER_RANGE {
                continue;
            }
            let cluster = 0.1 + 1.6 * smoothstep(-0.15, 0.45, fbm(seed.wrapping_add(40), p.x / 220.0, p.y / 220.0, 2));
            let chance = 0.09 * s.rocks * cluster * (1.0 - 0.6 * smoothstep(300.0, 1000.0, d));
            if u(3) >= chance {
                continue;
            }
            let roll = u(4);
            let grow = 1.0 + 1.2 * smoothstep(100.0, 800.0, d);
            let (kind, size) = if roll < 0.55 {
                (PropKind::Rock, 0.6 + 1.4 * u(5))
            } else if roll < 0.92 {
                (PropKind::Boulder, 1.5 + 3.5 * u(5) * u(5))
            } else if roll < 0.99 || d < 150.0 {
                (PropKind::Slab, 3.0 + 5.0 * u(5))
            } else {
                (PropKind::Spire, 5.0 + 4.0 * u(5))
            };
            let main = Prop { prop: kind, position: [p.x, 0.0, p.y], yaw: 360.0 * u(6), scale: size * grow, variant: Some(h) };
            // A few smaller rocks lie around most boulders and slabs.
            let satellites = if kind == PropKind::Rock { 0 } else { (u(7) * 3.0) as u32 };
            let mut group = vec![main];
            for n in 0..satellites {
                let a = core::f32::consts::TAU * u(10 + n);
                let r = group[0].radius() * (1.3 + 1.5 * u(20 + n));
                let q = p + Vec2::new(libm::cosf(a), libm::sinf(a)) * r;
                let scale = group[0].scale * (0.15 + 0.3 * u(30 + n));
                let kind = if u(40 + n) < 0.85 { PropKind::Rock } else { PropKind::Boulder };
                group.push(Prop { prop: kind, position: [q.x, 0.0, q.y], yaw: 360.0 * u(50 + n), scale, variant: Some(hash2(h, n as i32, 1)) });
            }
            for prop in group {
                let q = Vec2::new(prop.position[0], prop.position[2]);
                // Small rocks far out would only be specks in the haze.
                if prop.scale < 2.0 && d > 450.0 {
                    continue;
                }
                if !clear_of_track(&index, terrain, q, prop.radius()) {
                    continue;
                }
                if terrain.normal(q.x, q.y).y < 0.8 || terrain.sample(q.x, q.y).rock > 0.1 {
                    continue;
                }
                let on_a_prop = |o: &Prop| q.distance(Vec2::new(o.position[0], o.position[2])) < o.radius() + prop.radius() + 1.0;
                if placed.iter().any(on_a_prop) {
                    continue;
                }
                out.push(prop);
            }
        }
    }
    out
}

/// Every prop of a map in world space, for checks and tools: position on the ground and radius.
#[derive(Clone, Copy, Debug)]
pub struct PlacedProp {
    pub kind: PropKind,
    pub position: Vec3,
    pub radius: f32,
}

pub(crate) fn placed(props: &[Prop], terrain: &Terrain) -> Vec<PlacedProp> {
    props
        .iter()
        .map(|p| PlacedProp {
            kind: p.prop,
            position: Vec3::new(p.position[0], terrain.height(p.position[0], p.position[2]), p.position[2]),
            radius: p.radius(),
        })
        .collect()
}
