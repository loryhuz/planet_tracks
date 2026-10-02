//! Martian terrain under a map: a deterministic height field built from the map's
//! [`TerrainSettings`], flattened under the swept blocks, with the dirt corridors dug into it
//! (see [`crate::dirt`]), meshed with a crack-free adaptive quadtree.
//!
//! # Shape
//!
//! Every height is relative to [`TERRAIN_Y`], the level ground-level decks sit on, and depends on
//! `d`, the horizontal distance to the nearest block footprint (the ground a swept piece covers:
//! deck, lips, skirts and gate posts, see [`crate::kit`]; the floor of a dirt corridor):
//!
//! - `d` ≤ [`PAD`]: exactly [`TERRAIN_Y`]. Skirt feet, platform walls, end caps and gate posts all
//!   stand on it, and a car that leaves a ground-level road rolls down the shoulder onto it;
//! - sand ripples (`ripples`) and small bumps (`bumps`) fade in over the next [`NEAR_BLEND`] m;
//! - rolling ground (`hills`): rises and hollows about 100 m across start a few metres past
//!   the pad, in patches, so the track runs between low rises and over embankments rather than
//!   across a flat plain; their slopes stay mostly under 15° (every run-off drivable);
//! - the large undulation (`relief`) fades in over [`MID_BLEND`] m and grows with distance;
//! - dune fields (`dunes`) start about 20 m from the blocks: transverse dunes with a gentle
//!   windward side and a lee under 15°;
//! - mesas and buttes (`mesas`), craters (`craters`) and a ring of hills (`horizon`) only stand
//!   hundreds of metres away, as a backdrop;
//! - the map's own landforms ([`crate::landform`]) stand where its author put them, close to
//!   the track if they like: they are added last, cut around the blocks and filled up under
//!   elevated decks, so the pads stay flat at the level of the deck beside them.
//!
//! Every blend is a smoothstep, so the ground has no crease where it meets the pad.
//!
//! Near a dirt corridor the plain is then raised to the corridor's own level and dug through
//! ([`Corridors::plain`], [`Corridors::ground`]); the pads of swept pieces stay flat. The dirt
//! floor and the foot of its banks are [`Surface::Dirt`], the rest [`Surface::Ground`].
//!
//! # Mesh
//!
//! A quadtree over the square (`size` metres, centred on the map) on a 2 m lattice: leaves are
//! 4 m within 30 m of a block and grow to 256 m far away, are refined further where the height
//! field departs from the leaf's plane (mesa cliffs, crater rims, down to 2 m along the banks of
//! dirt corridors), and are balanced (neighbours differ by one level at most). A leaf whose neighbour is finer is fanned from its centre through
//! the neighbour's corner, so no vertex ever sits on another triangle's edge: the surface is
//! watertight for the renderer and the physics alike.

use std::collections::{HashMap, HashSet};

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::dirt::{Corridors, DIRT_BANK, REACH};
use crate::kit::{TERRAIN_Y, color, smoothstep};
use crate::landform::{self, Landform};
use crate::noise::{Rng, fbm, perlin};
use crate::{Surface, TrackMesh};

/// Flat margin around the block footprints, metres.
pub const PAD: f32 = 3.0;
/// Distance over which dunes and bumps fade in beyond the pad.
pub const NEAR_BLEND: f32 = 25.0;
/// Distance over which the large undulation fades in beyond the pad.
pub const MID_BLEND: f32 = 90.0;
/// Lattice step of the quadtree, metres.
const UNIT: f32 = 2.0;
/// Smallest leaf, in lattice units (2 m): only along the banks of dirt corridors; 4 m elsewhere.
const MIN_LEAF: i32 = 1;
/// Smallest leaf away from the banks, in lattice units (4 m).
const MIN_LEAF_PLAIN: i32 = 2;
/// Root tiles, in lattice units (256 m).
const ROOT: i32 = 128;
/// Resolution of the distance field, metres.
const FIELD_CELL: f32 = 2.0;
/// The distance field extends this far past the footprints; beyond it distances are estimated.
const FIELD_MARGIN: f32 = 320.0;
/// Within this distance of a footprint the field holds exact distances.
const EXACT_BAND: f32 = 12.0;

/// Terrain parameters stored in a map file. Missing fields take their default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerrainSettings {
    /// Seed of every random choice (shapes, feature placement, rock scatter).
    pub seed: u32,
    /// Side of the terrain square, metres, centred on the map (rounded up to 256 m).
    pub size: f32,
    /// Amplitude of the large-scale undulation, metres.
    pub relief: f32,
    /// Height of the dune fields from about 40 m off the track, metres.
    pub dunes: f32,
    /// Height of the sand ripples near the track, metres.
    pub ripples: f32,
    /// Height of the small bumps near the track, metres.
    pub bumps: f32,
    /// Height of the rolling ground (hummocks and hollows) near the track, metres.
    pub hills: f32,
    /// Mesas and buttes in the distance.
    pub mesas: u32,
    /// Craters in the distance.
    pub craters: u32,
    /// Height of the ring of hills that closes the horizon, metres.
    pub horizon: f32,
    /// Density of the automatic rock scatter (1 is the default look, 0 none).
    pub rocks: f32,
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self {
            seed: 1,
            size: 8192.0,
            relief: 14.0,
            dunes: 4.0,
            ripples: 0.35,
            bumps: 0.4,
            hills: 8.0,
            mesas: 10,
            craters: 6,
            horizon: 180.0,
            rocks: 1.0,
        }
    }
}

/// Ground a block stands on: the points within `r` of the segment `a`–`b` (horizontal plane),
/// with the height of the deck's lower edge at `a` and `b` (0 for ground-level decks, see
/// [`crate::kit`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Capsule {
    pub a: Vec2,
    pub b: Vec2,
    pub r: f32,
    pub low: (f32, f32),
}

impl Capsule {
    /// Distance from `p` to the capsule (0 inside).
    fn distance(&self, p: Vec2) -> f32 {
        self.distance_and_low(p).0
    }

    /// Distance from `p` to the capsule (0 inside) and the deck's lower edge abreast of `p`.
    /// Past the capsule's ends the edge carries on along the deck's grade: the capsules of a ramp
    /// overlap, and the end of the one below must not hold the ground beside the one above down
    /// to its own height (the capsule abreast of `p` gives the exact edge, and the lowest wins).
    pub(crate) fn distance_and_low(&self, p: Vec2) -> (f32, f32) {
        let ab = self.b - self.a;
        let len2 = ab.length_squared();
        let along = if len2 > 0.0 { (p - self.a).dot(ab) / len2 } else { 0.0 };
        let t = along.clamp(0.0, 1.0);
        (((p - (self.a + ab * t)).length() - self.r).max(0.0), self.low.0 + (self.low.1 - self.low.0) * along)
    }
}

/// Distances to the footprints on a grid, interpolated bilinearly.
struct DistanceField {
    x0: f32,
    z0: f32,
    nx: usize,
    nz: usize,
    d: Vec<f32>,
}

/// Squared Euclidean distance transform of one line (Felzenszwalb & Huttenlocher).
fn edt_1d(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f64;
        loop {
            let p = v[k];
            let s = (fq - (f[p] + (p * p) as f64)) / (2.0 * q as f64 - 2.0 * p as f64);
            // z[0] is -inf, so k never goes below 0.
            if s <= z[k] {
                k -= 1;
                continue;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        *o = dq * dq + f[p];
    }
}

impl DistanceField {
    fn new(caps: &[Capsule]) -> Self {
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for c in caps {
            lo = lo.min(c.a.min(c.b) - Vec2::splat(c.r));
            hi = hi.max(c.a.max(c.b) + Vec2::splat(c.r));
        }
        if caps.is_empty() {
            (lo, hi) = (Vec2::ZERO, Vec2::ZERO);
        }
        lo -= Vec2::splat(FIELD_MARGIN);
        hi += Vec2::splat(FIELD_MARGIN);
        let x0 = libm::floorf(lo.x / FIELD_CELL) * FIELD_CELL;
        let z0 = libm::floorf(lo.y / FIELD_CELL) * FIELD_CELL;
        let nx = libm::ceilf((hi.x - x0) / FIELD_CELL) as usize + 1;
        let nz = libm::ceilf((hi.y - z0) / FIELD_CELL) as usize + 1;
        const FAR: f64 = 1e20;
        let mut f = vec![FAR; nx * nz];
        let mut exact = vec![f32::MAX; nx * nz];
        let node = |i: usize, k: usize| Vec2::new(x0 + i as f32 * FIELD_CELL, z0 + k as f32 * FIELD_CELL);
        for c in caps {
            let reach = c.r + EXACT_BAND;
            let (a, b) = (c.a.min(c.b) - Vec2::splat(reach), c.a.max(c.b) + Vec2::splat(reach));
            let i0 = (libm::floorf((a.x - x0) / FIELD_CELL).max(0.0)) as usize;
            let k0 = (libm::floorf((a.y - z0) / FIELD_CELL).max(0.0)) as usize;
            let i1 = (libm::ceilf((b.x - x0) / FIELD_CELL) as usize).min(nx - 1);
            let k1 = (libm::ceilf((b.y - z0) / FIELD_CELL) as usize).min(nz - 1);
            for k in k0..=k1 {
                for i in i0..=i1 {
                    let d = c.distance(node(i, k));
                    let j = k * nx + i;
                    if d < exact[j] {
                        exact[j] = d;
                    }
                    if d <= 0.0 {
                        f[j] = 0.0;
                    }
                }
            }
        }
        // Rows, then columns.
        let n = nx.max(nz);
        let (mut line, mut out, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
        for k in 0..nz {
            line[..nx].copy_from_slice(&f[k * nx..(k + 1) * nx]);
            edt_1d(&line[..nx], &mut out[..nx], &mut v, &mut z);
            f[k * nx..(k + 1) * nx].copy_from_slice(&out[..nx]);
        }
        for i in 0..nx {
            for k in 0..nz {
                line[k] = f[k * nx + i];
            }
            edt_1d(&line[..nz], &mut out[..nz], &mut v, &mut z);
            for k in 0..nz {
                f[k * nx + i] = out[k];
            }
        }
        let d = (0..nx * nz)
            .map(|j| {
                if exact[j] <= EXACT_BAND {
                    exact[j]
                } else {
                    // Distance between grid nodes overestimates by up to a cell.
                    ((libm::sqrt(f[j]) as f32) * FIELD_CELL - 0.7 * FIELD_CELL).max(EXACT_BAND)
                }
            })
            .collect();
        Self { x0, z0, nx, nz, d }
    }

    fn at(&self, p: Vec2) -> f32 {
        let gx = (p.x - self.x0) / FIELD_CELL;
        let gz = (p.y - self.z0) / FIELD_CELL;
        let (mx, mz) = ((self.nx - 1) as f32, (self.nz - 1) as f32);
        let (cx, cz) = (gx.clamp(0.0, mx), gz.clamp(0.0, mz));
        let outside = libm::hypotf(gx - cx, gz - cz) * FIELD_CELL;
        let (i, k) = ((libm::floorf(cx) as usize).min(self.nx - 2), (libm::floorf(cz) as usize).min(self.nz - 2));
        let (u, w) = (cx - i as f32, cz - k as f32);
        let at = |i: usize, k: usize| self.d[k * self.nx + i];
        let a = at(i, k) + (at(i + 1, k) - at(i, k)) * u;
        let b = at(i, k + 1) + (at(i + 1, k + 1) - at(i, k + 1)) * u;
        a + (b - a) * w + outside
    }
}

/// A flat-topped mesa (or a narrower butte) with a cliff and a talus apron.
#[derive(Clone, Copy, Debug)]
struct Mesa {
    c: Vec2,
    r: f32,
    h: f32,
    /// Half-width of the cliff band, in radii.
    band: f32,
    seed: u32,
}

impl Mesa {
    fn reach(&self) -> f32 {
        2.3 * self.r
    }

    /// Height and rockiness (0 dust, 1 rock).
    fn sample(&self, p: Vec2) -> (f32, f32) {
        let d = p - self.c;
        if d.length_squared() > self.reach() * self.reach() {
            return (0.0, 0.0);
        }
        let s = 0.7 * self.r;
        let warp = Vec2::new(fbm(self.seed, p.x / s, p.y / s, 3), fbm(self.seed ^ 0x55, p.x / s, p.y / s, 3)) * (0.2 * self.r);
        let q = (d + warp).length() / self.r;
        let cliff = 1.0 - smoothstep(1.0 - self.band, 1.0 + self.band, q);
        let talus = 1.0 - smoothstep(0.8, 1.95, q);
        let ledge = 1.0 - smoothstep(1.0 + self.band, 1.2 + 2.0 * self.band, q);
        let top = 0.03 * fbm(self.seed ^ 0xaa, p.x / 45.0, p.y / 45.0, 3) * cliff;
        let h = self.h * (0.66 * cliff + 0.1 * ledge + 0.24 * talus * talus + top);
        (h, smoothstep(0.3, 0.9, cliff) * (1.0 - smoothstep(0.98, 1.0, cliff)))
    }
}

/// A bowl crater with a raised rim and an ejecta apron.
#[derive(Clone, Copy, Debug)]
struct Crater {
    c: Vec2,
    r: f32,
    depth: f32,
    rim: f32,
    seed: u32,
}

impl Crater {
    fn reach(&self) -> f32 {
        2.7 * self.r
    }

    /// Height and albedo factor.
    fn sample(&self, p: Vec2) -> (f32, f32) {
        let d = p - self.c;
        if d.length_squared() > self.reach() * self.reach() {
            return (0.0, 1.0);
        }
        let wobble = 1.0 + 0.06 * fbm(self.seed, p.x / (0.5 * self.r), p.y / (0.5 * self.r), 2);
        let q = d.length() / (self.r * wobble);
        if q < 1.0 {
            ((self.depth + self.rim) * q * q - self.depth, 0.86 + 0.12 * smoothstep(0.5, 1.0, q))
        } else {
            let t = 1.0 - smoothstep(1.0, 2.6, q);
            (self.rim * t * t, 1.0 + 0.1 * t)
        }
    }
}

/// One evaluation of the height field.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub height: f32,
    /// Distance to the nearest block footprint (swept piece or dirt corridor floor).
    pub distance: f32,
    /// Distance to the nearest swept piece (road, jump ramp, landing).
    pub pad: f32,
    /// Colour multiplier (crater floors darker, ejecta lighter).
    pub albedo: f32,
    /// Bare rock (mesa cliffs), 0 to 1.
    pub rock: f32,
    /// Distance outside the floor edge of the nearest dirt corridor (negative on its floor,
    /// `f32::MAX` far from any).
    pub edge: f32,
    /// Track coordinates and dirt amount (see [`TrackMesh`]).
    pub uv: [f32; 2],
    pub dirt: f32,
}

/// The terrain of one map.
pub struct Terrain {
    pub settings: TerrainSettings,
    /// Lower corner of the square.
    origin: Vec2,
    /// Root tiles per side.
    roots: i32,
    centre: Vec2,
    /// Footprints of the swept pieces and of the corridor floors.
    caps: Vec<Capsule>,
    /// Distances to every footprint, and to the swept pieces only (flat pads around them).
    field: DistanceField,
    pad: DistanceField,
    dirt: Corridors,
    /// The map's own landforms, close to the track.
    landforms: Vec<landform::Placed>,
    mesas: Vec<Mesa>,
    craters: Vec<Crater>,
}

impl Terrain {
    /// The terrain around swept blocks standing on `caps` and dirt corridors `dirt`, with the
    /// map's `landforms`, centred on `centre` (a grid line).
    pub(crate) fn new(settings: &TerrainSettings, centre: Vec3, caps: Vec<Capsule>, dirt: Corridors, landforms: &[Landform]) -> Self {
        let roots = (libm::ceilf(settings.size.max(256.0) / (ROOT as f32 * UNIT)) as i32).max(1);
        let half = 0.5 * roots as f32 * ROOT as f32 * UNIT;
        let centre = Vec2::new(centre.x, centre.z);
        let pad = DistanceField::new(&caps);
        let mut caps = caps;
        caps.extend(dirt.capsules());
        let field = DistanceField::new(&caps);
        let landforms = landforms.iter().map(|l| l.place(&caps)).collect();
        let mut t = Self {
            settings: settings.clone(),
            origin: centre - Vec2::splat(half),
            roots,
            centre,
            caps,
            field,
            pad,
            dirt,
            landforms,
            mesas: Vec::new(),
            craters: Vec::new(),
        };
        t.place_features();
        t
    }

    /// Side of the square, metres.
    pub fn size(&self) -> f32 {
        self.roots as f32 * ROOT as f32 * UNIT
    }

    fn place_features(&mut self) {
        let s = &self.settings;
        let half = 0.5 * self.size();
        let mut rng = Rng::new(u64::from(s.seed).wrapping_mul(7919).wrapping_add(17));
        let mut mesas: Vec<Mesa> = Vec::new();
        for i in 0..s.mesas {
            for _ in 0..120 {
                let butte = i % 3 == 2;
                let (r, h) = if butte { (rng.range(35.0, 70.0), rng.range(55.0, 110.0)) } else { (rng.range(90.0, 220.0), rng.range(50.0, 140.0)) };
                let a = rng.range(0.0, core::f32::consts::TAU);
                let dist = rng.range(400.0, 1800.0f32.min(0.8 * half).max(500.0));
                let c = self.centre + Vec2::new(libm::cosf(a), libm::sinf(a)) * dist;
                // Close enough to stand out of the haze, never near the track.
                let d = self.exact_distance(c);
                let fits = d >= 220.0 + 2.3 * r
                    && d <= 1600.0
                    && mesas.iter().all(|m| m.c.distance(c) >= 1.6 * (m.r + r) + 60.0)
                    && self.clear_of_landforms(c, 2.3 * r)
                    && (c - self.centre).abs().max_element() < half - 2.3 * r;
                if fits {
                    // The cliff band is at least 24 m wide so the mesh can follow it.
                    let band = (0.11f32).max(12.0 / r).min(0.3);
                    mesas.push(Mesa { c, r, h, band, seed: s.seed.wrapping_add(101 + 31 * i) });
                    break;
                }
            }
        }
        let mut craters: Vec<Crater> = Vec::new();
        for i in 0..s.craters {
            for _ in 0..80 {
                let u = rng.f32();
                let r = 22.0 + 90.0 * u * u;
                let a = rng.range(0.0, core::f32::consts::TAU);
                let dist = rng.range(250.0, (0.55 * half).max(300.0));
                let c = self.centre + Vec2::new(libm::cosf(a), libm::sinf(a)) * dist;
                let fits = self.exact_distance(c) >= 150.0 + 2.7 * r
                    && mesas.iter().all(|m| m.c.distance(c) >= m.reach() + 2.7 * r)
                    && self.clear_of_landforms(c, 2.7 * r)
                    && craters.iter().all(|k| k.c.distance(c) >= 2.0 * (k.r + r))
                    && (c - self.centre).abs().max_element() < half - 2.7 * r;
                if fits {
                    craters.push(Crater { c, r, depth: 0.19 * r, rim: 0.055 * r, seed: s.seed.wrapping_add(701 + 17 * i) });
                    break;
                }
            }
        }
        self.mesas = mesas;
        self.craters = craters;
    }

    /// Whether a feature reaching `r` metres around `c` keeps off the map's landforms.
    fn clear_of_landforms(&self, c: Vec2, r: f32) -> bool {
        (0..16).all(|i| {
            let a = i as f32 * core::f32::consts::TAU / 16.0;
            [0.0, 0.5, 1.0].iter().all(|&k| !self.landforms.iter().any(|l| l.covers(c + Vec2::new(libm::cosf(a), libm::sinf(a)) * (k * r))))
        })
    }

    /// Height the map's landforms add at a point, metres (0 off them).
    pub fn landform(&self, x: f32, z: f32) -> f32 {
        self.landforms_at(Vec2::new(x, z)).0
    }

    /// Height and rockiness of the map's landforms at `p`: the highest one wins.
    fn landforms_at(&self, p: Vec2) -> (f32, f32) {
        let (mut h, mut rock) = (0.0f32, 0.0f32);
        for l in &self.landforms {
            let (lh, lr) = l.sample(p);
            h = h.max(lh);
            rock = rock.max(lr);
        }
        (h, rock)
    }

    /// Exact horizontal distance to the nearest block footprint (slow: every capsule).
    pub fn exact_distance(&self, p: Vec2) -> f32 {
        self.caps.iter().map(|c| c.distance(p)).fold(f32::MAX, f32::min)
    }

    /// Distance to the nearest block footprint, from the precomputed field (exact within a few
    /// metres of the blocks, within a metre or so farther).
    pub fn distance(&self, x: f32, z: f32) -> f32 {
        self.field.at(Vec2::new(x, z))
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        self.sample(x, z).height
    }

    /// Upward unit normal of the height field, by central differences.
    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = 1.0;
        let dx = self.height(x + e, z) - self.height(x - e, z);
        let dz = self.height(x, z + e) - self.height(x, z - e);
        Vec3::new(-dx, 2.0 * e, -dz).normalize()
    }

    pub fn sample(&self, x: f32, z: f32) -> Sample {
        let p = Vec2::new(x, z);
        let d = self.field.at(p);
        let pad = self.pad.at(p);
        let (height, albedo, rock) = self.natural(p, d);
        let mut out = Sample { height, distance: d, pad, albedo, rock, edge: f32::MAX, uv: [0.0, 0.0], dirt: 0.0 };
        if d > REACH + 4.0 {
            return out;
        }
        if let Some(hit) = self.dirt.query(p) {
            // The plain the corridor was dug into, flat again on the pads of swept pieces: at the
            // terrain plane, or up at an elevated deck where a landform fills under it.
            let plain = self.dirt.plain(p, &hit);
            let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
            let flat = if pad < PAD + 10.0 { TERRAIN_Y + self.landforms_at(p).0 } else { TERRAIN_Y };
            let before = mix(plain, height, smoothstep(15.0, REACH, hit.edge));
            let before = mix(flat, before, smoothstep(PAD, PAD + 10.0, pad));
            let mut after = self.dirt.ground(before, &hit);
            // The banks give way to the flat pads of swept pieces (not the floor and its aprons).
            if hit.edge > 0.0 {
                after = mix(flat, after, smoothstep(PAD, 2.0 * PAD, pad));
            }
            let driven = 1.0 - smoothstep(-0.5, 2.0, hit.edge);
            let moved = smoothstep(0.1, 0.5, (after - before).abs()) * (1.0 - smoothstep(8.0, 14.0, hit.edge));
            out.height = after;
            out.edge = hit.edge;
            out.uv = hit.uv;
            out.dirt = driven.max(0.5 * moved);
        }
        out
    }

    /// The plain without dirt corridors, `d` metres from the nearest footprint, with the map's
    /// landforms: height, albedo and rock.
    fn natural(&self, p: Vec2, d: f32) -> (f32, f32, f32) {
        let (h, albedo, rock) = self.plain(p, d);
        if self.landforms.is_empty() {
            return (h, albedo, rock);
        }
        let (lh, lr) = self.landforms_at(p);
        (h + lh, albedo, rock.max(lr))
    }

    /// The plain without dirt corridors or landforms.
    fn plain(&self, p: Vec2, d: f32) -> (f32, f32, f32) {
        let s = &self.settings;
        let (x, z) = (p.x, p.y);
        if d <= PAD {
            return (TERRAIN_Y, 1.0, 0.0);
        }
        let seed = s.seed;
        let near = smoothstep(PAD, PAD + NEAR_BLEND, d);
        let mid = smoothstep(PAD, PAD + MID_BLEND, d);
        let far = smoothstep(250.0, 1500.0, d);
        let mut h = 0.0;
        // Large undulation, stronger far away, and a medium one.
        h += s.relief * fbm(seed, x / 1100.0, z / 1100.0, 4) * (0.6 + 0.9 * far) * mid;
        h += 0.5 * s.relief * fbm(seed.wrapping_add(1), x / 320.0, z / 320.0, 3) * mid;
        // Rough ground away from the track.
        let rough = smoothstep(60.0, 400.0, d);
        if rough > 0.0 {
            h += 2.5 * fbm(seed.wrapping_add(2), x / 70.0, z / 70.0, 3) * rough;
        }
        // Rolling ground from a few metres past the pad: rises and hollows about 100 m across,
        // stronger in some stretches than in others.
        if s.hills > 0.0 {
            let rise = smoothstep(PAD + 2.0, PAD + 30.0, d) * (1.0 - smoothstep(500.0, 800.0, d));
            if rise > 0.0 {
                let patches = 0.5 + 0.5 * smoothstep(-0.3, 0.3, fbm(seed.wrapping_add(14), x / 280.0, z / 280.0, 2));
                h += s.hills * fbm(seed.wrapping_add(13), x / 100.0, z / 100.0, 2) * patches * rise;
            }
        }
        // Dune fields from about 20 m off the blocks: transverse dunes with a gentle windward
        // side and a steeper lee (about 5° and 14° for 4 m dunes 100 m apart), in patches.
        let dune_t = smoothstep(18.0, 80.0, d) * (1.0 - smoothstep(900.0, 1300.0, d));
        if dune_t > 0.0 && s.dunes > 0.0 {
            let mask = smoothstep(-0.1, 0.45, fbm(seed.wrapping_add(8), x / 420.0, z / 420.0, 3));
            if mask > 0.0 {
                let wind = 0.9 + 0.35 * fbm(seed.wrapping_add(9), x / 1500.0, z / 1500.0, 2);
                let wavelength = 95.0 + 30.0 * fbm(seed.wrapping_add(10), x / 600.0, z / 600.0, 2);
                let along = x * libm::cosf(wind) + z * libm::sinf(wind) + 40.0 * fbm(seed.wrapping_add(11), x / 260.0, z / 260.0, 3);
                let phase = along / wavelength;
                let f = phase - libm::floorf(phase);
                let profile = if f < 0.75 { smoothstep(0.0, 0.75, f) } else { 1.0 - smoothstep(0.75, 1.0, f) };
                let height = 0.6 + 0.4 * fbm(seed.wrapping_add(12), x / 180.0, z / 180.0, 2);
                h += s.dunes * profile * height * mask * dune_t;
            }
        }
        // Sand ripples and small bumps near the track (faded out again far away).
        let fine = near * (1.0 - smoothstep(600.0, 900.0, d));
        if fine > 0.0 {
            if s.ripples > 0.0 {
                let a = 0.7 + 0.6 * fbm(seed.wrapping_add(3), x / 700.0, z / 700.0, 2);
                let along = x * libm::cosf(a) + z * libm::sinf(a) + 9.0 * fbm(seed.wrapping_add(4), x / 90.0, z / 90.0, 2);
                let phase = along / 17.0;
                let ripple = 0.5 - 0.5 * libm::cosf(core::f32::consts::TAU * phase);
                let field = smoothstep(-0.25, 0.35, fbm(seed.wrapping_add(5), x / 260.0, z / 260.0, 2));
                h += s.ripples * ripple * ripple * field * fine;
            }
            if s.bumps > 0.0 {
                h += s.bumps * fbm(seed.wrapping_add(6), x / 13.0, z / 13.0, 2) * fine;
            }
        }
        // Distant features.
        let (mut albedo, mut rock) = (1.0, 0.0f32);
        for m in &self.mesas {
            let (mh, mr) = m.sample(p);
            h += mh;
            rock = rock.max(mr);
        }
        for c in &self.craters {
            let (ch, ca) = c.sample(p);
            h += ch;
            albedo *= ca;
        }
        if s.horizon > 0.0 {
            let size = self.size();
            let ring = smoothstep(0.28 * size, 0.46 * size, p.distance(self.centre));
            if ring > 0.0 {
                let ridge = 1.0 - fbm(seed.wrapping_add(7), x / 900.0, z / 900.0, 4).abs();
                h += s.horizon * ring * (0.25 + 0.75 * ridge * ridge);
            }
        }
        (TERRAIN_Y + h, albedo, rock)
    }

    /// Ground colour (linear RGB) at a point with surface normal `n`.
    fn color(&self, p: Vec3, n: Vec3, s: &Sample) -> [f32; 3] {
        let seed = self.settings.seed.wrapping_add(20);
        let v = fbm(seed, p.x / 420.0, p.z / 420.0, 3);
        let f = perlin(seed.wrapping_add(1), p.x / 35.0, p.z / 35.0);
        let g = color::GROUND;
        let mut c = [g[0] * (1.0 + 0.10 * v + 0.05 * f), g[1] * (1.0 + 0.16 * v + 0.05 * f), g[2] * (1.0 + 0.10 * v + 0.04 * f)];
        // Graded ground next to the swept blocks: a little lighter and greyer.
        let graded = 1.0 - smoothstep(PAD, PAD + 10.0, s.pad);
        let mix = |c: [f32; 3], o: [f32; 3], t: f32| [c[0] + (o[0] - c[0]) * t, c[1] + (o[1] - c[1]) * t, c[2] + (o[2] - c[2]) * t];
        c = mix(c, [0.56, 0.26, 0.135], 0.5 * graded);
        // Darker, layered rock on steep slopes and cliffs (not on the earth of dug banks).
        let steep = (smoothstep(0.93, 0.7, n.y) * (1.0 - 2.0 * s.dirt).max(0.0)).max(s.rock);
        let strata = 1.0 + 0.13 * libm::sinf(p.y * 0.55 + 1.5 * f);
        c = mix(c, [0.27 * strata, 0.115 * strata, 0.065 * strata], steep);
        c.map(|k| (k * s.albedo).clamp(0.0, 1.0))
    }

    // -----------------------------------------------------------------------------------------
    // Mesh.

    fn lattice(&self, ux: i32, uz: i32) -> Vec2 {
        self.origin + Vec2::new(ux as f32 * UNIT, uz as f32 * UNIT)
    }

    /// Largest leaf wanted `d` metres from the blocks.
    fn target_size(d: f32) -> f32 {
        match d {
            d if d < 30.0 => 4.0,
            d if d < 100.0 => 8.0,
            d if d < 220.0 => 16.0,
            d if d < 480.0 => 32.0,
            d if d < 1000.0 => 64.0,
            d if d < 2000.0 => 128.0,
            _ => 256.0,
        }
    }

    /// Largest leaf near the distant features, so their shapes survive.
    fn feature_size(&self, c: Vec2, half_diag: f32) -> f32 {
        let mut best = f32::MAX;
        for m in &self.mesas {
            if c.distance(m.c) < m.reach() + half_diag {
                best = best.min((m.r / 7.0).clamp(8.0, 32.0));
            }
        }
        for k in &self.craters {
            if c.distance(k.c) < k.reach() + half_diag {
                best = best.min((k.r / 5.0).clamp(4.0, 16.0));
            }
        }
        for l in &self.landforms {
            if l.covers(c) || (0..4).any(|i| l.covers(c + Vec2::new([1.0, -1.0, 1.0, -1.0][i], [1.0, 1.0, -1.0, -1.0][i]) * (0.7072 * half_diag))) {
                best = best.min(l.leaf_size());
            }
        }
        best
    }

    fn wants_split(&self, cache: &mut HeightCache, x: i32, z: i32, s: i32) -> bool {
        if s <= MIN_LEAF {
            return false;
        }
        let size = s as f32 * UNIT;
        let c = self.lattice(x, z) + Vec2::splat(0.5 * size);
        let half_diag = 0.7072 * size;
        // The finest leaves only follow the banks of dirt corridors.
        if s <= MIN_LEAF_PLAIN && !self.dirt.query(c).is_some_and(|h| h.edge > -2.0 - half_diag && h.edge < 12.0 + half_diag) {
            return false;
        }
        let dc = self.field.at(c);
        let d_min = (dc - half_diag).max(0.0);
        // The pads are flat, except where a landform comes up under an elevated deck.
        if dc + half_diag < PAD && self.feature_size(c, half_diag) == f32::MAX {
            return false;
        }
        if size > Self::target_size(d_min) || size > self.feature_size(c, half_diag) {
            return true;
        }
        // Height error of the leaf's bilinear patch at its centre and edge midpoints.
        let h = |cache: &mut HeightCache, i: i32, k: i32| cache.get(self, x + i, z + k).height;
        let m = s / 2;
        let (h00, h10, h01, h11) = (h(cache, 0, 0), h(cache, s, 0), h(cache, 0, s), h(cache, s, s));
        let err = [
            (h(cache, m, m), 0.25 * (h00 + h10 + h01 + h11)),
            (h(cache, m, 0), 0.5 * (h00 + h10)),
            (h(cache, m, s), 0.5 * (h01 + h11)),
            (h(cache, 0, m), 0.5 * (h00 + h01)),
            (h(cache, s, m), 0.5 * (h10 + h11)),
        ]
        .iter()
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
        err > 0.06 + 0.009 * d_min
    }

    fn subdivide(&self, cache: &mut HeightCache, x: i32, z: i32, s: i32, out: &mut Vec<(i32, i32, i32)>) {
        if self.wants_split(cache, x, z, s) {
            let h = s / 2;
            for (dx, dz) in [(0, 0), (h, 0), (0, h), (h, h)] {
                self.subdivide(cache, x + dx, z + dz, h, out);
            }
        } else {
            out.push((x, z, s));
        }
    }

    /// The terrain mesh (surface [`Surface::Ground`]).
    pub fn mesh(&self) -> TrackMesh {
        let mut cache = HeightCache::default();
        let mut order = Vec::new();
        for rz in 0..self.roots {
            for rx in 0..self.roots {
                self.subdivide(&mut cache, rx * ROOT, rz * ROOT, ROOT, &mut order);
            }
        }
        let leaves = Leaves::balanced(order, self.roots * ROOT);

        // Vertices on the lattice, shared between leaves.
        let mut pool = VertexPool::default();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for &(x, z, s) in &leaves.list {
            let m = s / 2;
            let finer = |nx: i32, nz: i32| leaves.at(nx, nz).is_some_and(|l| l.2 < s);
            // Perimeter counter-clockwise seen from above (+Z then +X), with the midpoints of
            // edges whose neighbour is finer.
            let mut ring: Vec<(i32, i32)> = vec![(x, z)];
            if finer(x - 1, z) {
                ring.push((x, z + m));
            }
            ring.push((x, z + s));
            if finer(x, z + s) {
                ring.push((x + m, z + s));
            }
            ring.push((x + s, z + s));
            if finer(x + s, z) {
                ring.push((x + s, z + m));
            }
            ring.push((x + s, z));
            if finer(x, z - 1) {
                ring.push((x + m, z));
            }
            let v: Vec<u32> = ring.iter().map(|&(a, b)| pool.get(self, &mut cache, a, b)).collect();
            if v.len() == 4 {
                let hc = cache.get(self, x + m, z + m).height;
                let y = |i: usize| pool.verts[v[i] as usize].0.y;
                // The diagonal whose midpoint is closer to the true centre height.
                if (0.5 * (y(0) + y(2)) - hc).abs() <= (0.5 * (y(1) + y(3)) - hc).abs() {
                    tris.push([v[0], v[1], v[2]]);
                    tris.push([v[0], v[2], v[3]]);
                } else {
                    tris.push([v[0], v[1], v[3]]);
                    tris.push([v[1], v[2], v[3]]);
                }
            } else {
                let c = pool.get(self, &mut cache, x + m, z + m);
                for i in 0..v.len() {
                    tris.push([c, v[i], v[(i + 1) % v.len()]]);
                }
            }
        }
        let verts = pool.verts;
        self.assemble(&verts, &tris)
    }

    /// Normals (area-weighted, split where a triangle disagrees with its smooth normal) and
    /// colours.
    fn assemble(&self, verts: &[(Vec3, Sample)], tris: &[[u32; 3]]) -> TrackMesh {
        let mut acc = vec![Vec3::ZERO; verts.len()];
        let mut faces = Vec::with_capacity(tris.len());
        for t in tris {
            let [a, b, c] = t.map(|i| verts[i as usize].0);
            let n = (b - a).cross(c - a);
            for &i in t {
                acc[i as usize] += n;
            }
            faces.push(n.normalize_or_zero());
        }
        let normals: Vec<Vec3> = acc.iter().map(|n| n.normalize_or(Vec3::Y)).collect();
        let mut mesh = TrackMesh::default();
        let push = |mesh: &mut TrackMesh, p: Vec3, n: Vec3, s: &Sample| {
            mesh.positions.push(p);
            mesh.normals.push(n);
            mesh.colors.push(self.color(p, n, s));
            mesh.uv.push(s.uv);
            mesh.dirt.push(s.dirt);
        };
        for (i, (p, s)) in verts.iter().enumerate() {
            push(&mut mesh, *p, normals[i], s);
        }
        for (t, face) in tris.iter().zip(&faces) {
            for &i in t {
                let i = if normals[i as usize].dot(*face) >= 0.6 {
                    i
                } else {
                    // A crease (cliff edge): this corner gets its own flat vertex.
                    let (p, s) = &verts[i as usize];
                    push(&mut mesh, *p, *face, s);
                    (mesh.positions.len() - 1) as u32
                };
                mesh.indices.push(i);
            }
            // The floor of a dirt corridor and the foot of its banks are dirt.
            let edge = t.iter().map(|&i| verts[i as usize].1.edge.min(1e6)).sum::<f32>() / 3.0;
            mesh.tri_surface.push(if edge < DIRT_BANK { Surface::Dirt } else { Surface::Ground });
        }
        mesh
    }
}

/// Terrain vertices, one per lattice point.
#[derive(Default)]
struct VertexPool {
    index: HashMap<(i32, i32), u32>,
    verts: Vec<(Vec3, Sample)>,
}

impl VertexPool {
    fn get(&mut self, t: &Terrain, cache: &mut HeightCache, ux: i32, uz: i32) -> u32 {
        if let Some(&i) = self.index.get(&(ux, uz)) {
            return i;
        }
        let s = cache.get(t, ux, uz);
        let p = t.lattice(ux, uz);
        self.verts.push((Vec3::new(p.x, s.height, p.y), s));
        let i = (self.verts.len() - 1) as u32;
        self.index.insert((ux, uz), i);
        i
    }
}

/// Heights on the lattice, computed once each.
#[derive(Default)]
struct HeightCache(HashMap<(i32, i32), Sample>);

impl HeightCache {
    fn get(&mut self, t: &Terrain, ux: i32, uz: i32) -> Sample {
        *self.0.entry((ux, uz)).or_insert_with(|| {
            let p = t.lattice(ux, uz);
            t.sample(p.x, p.y)
        })
    }
}

/// Quadtree leaves `(x, z, size)` in lattice units.
struct Leaves {
    /// Leaves in a deterministic order.
    list: Vec<(i32, i32, i32)>,
    set: HashSet<(i32, i32, i32)>,
    extent: i32,
}

impl Leaves {
    /// The leaf containing lattice cell `(x, z)`.
    fn at(&self, x: i32, z: i32) -> Option<(i32, i32, i32)> {
        if x < 0 || z < 0 || x >= self.extent || z >= self.extent {
            return None;
        }
        let mut s = MIN_LEAF;
        while s <= ROOT {
            let key = (x - x.rem_euclid(s), z - z.rem_euclid(s), s);
            if self.set.contains(&key) {
                return Some(key);
            }
            s *= 2;
        }
        None
    }

    /// Splits leaves until no two edge neighbours differ by more than one level.
    fn balanced(list: Vec<(i32, i32, i32)>, extent: i32) -> Self {
        let set: HashSet<_> = list.iter().copied().collect();
        let mut leaves = Self { list, set, extent };
        let mut stack: Vec<(i32, i32, i32)> = leaves.list.iter().rev().copied().collect();
        while let Some(l) = stack.pop() {
            if !leaves.set.contains(&l) {
                continue;
            }
            let (x, z, s) = l;
            for (nx, nz) in [(x + s, z), (x - 1, z), (x, z + s), (x, z - 1)] {
                while let Some(m) = leaves.at(nx, nz) {
                    if m.2 <= 2 * s {
                        break;
                    }
                    leaves.set.remove(&m);
                    let h = m.2 / 2;
                    for child in [(m.0, m.1, h), (m.0 + h, m.1, h), (m.0, m.1 + h, h), (m.0 + h, m.1 + h, h)] {
                        leaves.set.insert(child);
                        leaves.list.push(child);
                        stack.push(child);
                    }
                }
            }
        }
        let set = &leaves.set;
        let list: Vec<_> = leaves.list.iter().copied().filter(|l| set.contains(l)).collect();
        leaves.list = list;
        leaves
    }
}
