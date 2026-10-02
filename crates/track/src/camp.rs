//! The colony's camps: scenery built the way its roads are (art/scenery/brief.md), from what the
//! colonists have on site — white fabric inflated into domes and tunnels and girdled with orange
//! straps, red plastic tubes, sandbags of regolith, steel stakes — and what the freight brought:
//! painted tanks and containers, solar panels, glass, beacons, a rocket.
//!
//! A map places them in its `structures` list:
//!
//! ```json
//! {"structure":"post","position":[480.0,-185.0],"yaw":200.0}
//! ```
//!
//! - `post`: an observation post, 13 to 17 m tall: a lattice tower of red tubes on stacks of
//!   sandbags, guyed with orange straps, a cabin with a band of windows on top, antennas, a dish,
//!   a beacon;
//! - `base_camp`: about 110 m across, its front toward its yaw: inflatable domes and tunnel
//!   modules with airlocks, tanks on cradles, containers, a solar field, sandbag walls, a lattice
//!   mast, masts with pennants, a windsock, a rover;
//! - `colony`: about 460 m long along its yaw, meant to be seen from afar: a giant dome held by a
//!   net of straps over stacks of sandbags, two domes, glass greenhouses, a tank farm, a 170 m
//!   lattice tower with beacons, a rocket on its landing pad.
//!
//! The terrain is levelled under them ([`Structure::pads`]). Posts and camps stand within reach
//! of a car and collide; the colony only draws. Everything placed by hand varies with the
//! structure's seed: sizes, angles, spacing, never one thing repeated at fixed intervals.

use core::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::kit::{color, stake};
use crate::mesh::{MeshBuilder, add_box, add_post, add_sandbag, add_tube};
use crate::noise::{hash2, unit};
use crate::stilts::bag_stack;
use crate::terrain::Terrain;
use crate::{Surface, TrackMesh};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureKind {
    /// An observation post: a lattice tower with a cabin on top.
    Post,
    /// A base camp beside the track.
    BaseCamp,
    /// The colony: giant domes, a tower, a rocket, seen from afar.
    Colony,
}

/// A structure placed by the map author.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub structure: StructureKind,
    /// Centre `[x, z]`, metres.
    pub position: [f32; 2],
    /// Degrees, positive turns left (as a car's yaw): where a camp's front faces, the colony's
    /// length.
    #[serde(default)]
    pub yaw: f32,
    /// Variant; derived from the position when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<u32>,
}

/// Ground levelled for a structure: a disc of radius `r` flattened at the mean height of the
/// ground there, blending back into it over `blend` metres.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pad {
    pub c: Vec2,
    pub r: f32,
    pub blend: f32,
}

/// One piece of the colony's layout, in its own frame (`x` to the left, `z` along its length).
#[derive(Clone, Copy)]
enum Piece {
    /// A dome: radius, height, gores (and as many meridian straps).
    Dome { r: f32, h: f32, gores: u32 },
    /// A glass greenhouse dome: radius, height.
    Greenhouse { r: f32, h: f32 },
    /// Tanks on cradles: count, radius, length.
    Tanks { n: u32, r: f32, len: f32 },
    /// Stacks of containers: count.
    Containers { n: u32 },
    /// A solar field: rows, tables per row.
    Solar { rows: u32, per_row: u32 },
    /// The lattice tower: height.
    Tower { h: f32 },
    /// The rocket on its landing pad: pad radius.
    Rocket { pad: f32 },
}

/// The colony: pieces at `[x, z]` in its frame, with the radius of ground each one levels.
const COLONY: [(Piece, [f32; 2], f32); 13] = [
    (Piece::Dome { r: 46.0, h: 36.0, gores: 20 }, [0.0, 0.0], 52.0),
    (Piece::Dome { r: 24.0, h: 18.0, gores: 12 }, [-6.0, 84.0], 28.0),
    (Piece::Dome { r: 19.0, h: 14.0, gores: 10 }, [10.0, -78.0], 23.0),
    (Piece::Greenhouse { r: 14.0, h: 11.0 }, [-16.0, 132.0], 17.0),
    (Piece::Greenhouse { r: 12.0, h: 9.5 }, [17.0, 128.0], 15.0),
    (Piece::Tanks { n: 5, r: 2.8, len: 15.0 }, [30.0, 36.0], 17.0),
    (Piece::Tanks { n: 4, r: 2.4, len: 12.0 }, [-33.0, -42.0], 14.0),
    (Piece::Containers { n: 9 }, [32.0, -40.0], 15.0),
    (Piece::Solar { rows: 5, per_row: 7 }, [-26.0, -120.0], 22.0),
    (Piece::Solar { rows: 4, per_row: 6 }, [22.0, -122.0], 18.0),
    (Piece::Tower { h: 170.0 }, [0.0, -186.0], 14.0),
    (Piece::Rocket { pad: 22.0 }, [2.0, 190.0], 27.0),
    (Piece::Containers { n: 6 }, [-30.0, 40.0], 12.0),
];

/// Radius of the base camp's levelled ground, metres.
const CAMP_PAD: f32 = 44.0;

impl Structure {
    fn seed(&self) -> u32 {
        self.variant.unwrap_or_else(|| hash2(0xca4b, libm::roundf(self.position[0] * 10.0) as i32, libm::roundf(self.position[1] * 10.0) as i32))
    }

    fn site(&self) -> Site {
        Site::new(self.position, self.yaw)
    }

    /// The ground the structure levels.
    pub(crate) fn pads(&self) -> Vec<Pad> {
        let site = self.site();
        match self.structure {
            StructureKind::Post => Vec::new(),
            StructureKind::BaseCamp => vec![Pad { c: site.o, r: CAMP_PAD, blend: 16.0 }],
            StructureKind::Colony => COLONY.iter().map(|&(_, [x, z], r)| Pad { c: site.at(x, z), r, blend: 8.0 }).collect(),
        }
    }

    /// Whether a rock of radius `r` at `p` would stand in the structure (the scattered rocks
    /// keep out of it).
    pub fn covers(&self, p: Vec2, r: f32) -> bool {
        let site = self.site();
        match self.structure {
            StructureKind::Post => p.distance(site.o) < 9.0 + r,
            StructureKind::BaseCamp => p.distance(site.o) < CAMP_PAD + 8.0 + r,
            StructureKind::Colony => COLONY.iter().any(|&(_, [x, z], pr)| p.distance(site.at(x, z)) < pr + 6.0 + r),
        }
    }
}

/// A structure's frame on the ground: `x` to the left of its yaw, `z` along it.
#[derive(Clone, Copy)]
struct Site {
    o: Vec2,
    f: Vec2,
    l: Vec2,
}

impl Site {
    fn new(position: [f32; 2], yaw_deg: f32) -> Self {
        let y = yaw_deg.to_radians();
        let f = Vec2::new(libm::sinf(y), libm::cosf(y));
        Self { o: Vec2::new(position[0], position[1]), f, l: Vec2::new(f.y, -f.x) }
    }

    fn at(&self, x: f32, z: f32) -> Vec2 {
        self.o + self.l * x + self.f * z
    }

    /// The same site turned half round.
    fn turned(&self) -> Self {
        Self { o: self.o, f: -self.f, l: -self.l }
    }

    /// Horizontal unit vector `deg` degrees to the left of the site's forward.
    fn dir(&self, deg: f32) -> Vec3 {
        let (s, c) = libm::sincosf(deg.to_radians());
        let d = self.f * c + self.l * s;
        Vec3::new(d.x, 0.0, d.y)
    }
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn ring_dir(theta: f32) -> Vec3 {
    let (s, c) = libm::sincosf(theta);
    Vec3::new(c, 0.0, s)
}

/// A random number in 0..1 from a seed and a key.
fn rnd(seed: u32, k: i32) -> f32 {
    unit(hash2(seed, k, 0x51))
}

/// A random number in -1..1.
fn jit(seed: u32, k: i32) -> f32 {
    2.0 * rnd(seed, k) - 1.0
}

/// A triangle wound to face away from `inside`.
fn tri_out(b: &mut MeshBuilder, i: u32, j: u32, k: u32, inside: Vec3, surface: Surface) {
    let [p, q, r] = [i, j, k].map(|v| b.position(v));
    let n = (q - p).cross(r - p);
    if n.length_squared() < 1e-12 {
        return;
    }
    if n.dot((p + q + r) / 3.0 - inside) >= 0.0 {
        b.tri(i, j, k, surface);
    } else {
        b.tri(i, k, j, surface);
    }
}

/// A solid of revolution about the axis from `base` along `axis` (unit length), `sides` faces
/// around. Each profile point is (distance along the axis, radius, colour); two points at the same
/// place change the colour there with a crisp edge, as does a corner sharper than 40°. A radius of
/// 0 closes the solid at that point. The profile is walked with the solid on its left (from the
/// axis, out, along and back): each face looks to the right of its step, (−Δradius, Δalong).
fn lathe(b: &mut MeshBuilder, base: Vec3, axis: Vec3, profile: &[(f32, f32, [f32; 3])], sides: u32, surface: Surface) {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = axis.cross(helper).normalize();
    let e2 = axis.cross(e1);
    let around = |k: usize| {
        let (s, co) = libm::sincosf(TAU * k as f32 / sides as f32);
        e1 * co + e2 * s
    };
    let ring = |b: &mut MeshBuilder, t: f32, r: f32, c: [f32; 3]| -> Vec<u32> {
        if r < 1e-4 {
            return vec![b.vertex(base + axis * t, c)];
        }
        (0..sides as usize).map(|k| b.vertex(base + axis * t + around(k) * r, c)).collect()
    };
    let step = |i: usize| Vec2::new(profile[i + 1].0 - profile[i].0, profile[i + 1].1 - profile[i].1);
    let mut prev: Option<Vec<u32>> = None;
    for i in 0..profile.len() - 1 {
        let (t0, r0, c0) = profile[i];
        let (t1, r1, _) = profile[i + 1];
        let d = step(i);
        if d.length() < 1e-5 {
            // A colour change: the next segment starts its own ring.
            prev = None;
            continue;
        }
        // A sharp corner with the previous segment: a crease.
        if i > 0 && step(i - 1).length() > 1e-5 && step(i - 1).normalize().dot(d.normalize()) < libm::cosf(40f32.to_radians()) {
            prev = None;
        }
        let a = prev.take().unwrap_or_else(|| ring(b, t0, r0, c0));
        let c = ring(b, t1, r1, c0);
        let n = sides as usize;
        for k in 0..n {
            let m = (k + 1) % n;
            // The face's outward direction at this angle.
            let out = axis * (-d.y) + around(k) * d.x + around(m) * d.x;
            let quad = [a[k.min(a.len() - 1)], a[m.min(a.len() - 1)], c[m.min(c.len() - 1)], c[k.min(c.len() - 1)]];
            for tri in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
                let [p, q, r] = tri.map(|v| b.position(v));
                let nrm = (q - p).cross(r - p);
                if nrm.length_squared() < 1e-12 {
                    continue;
                }
                if nrm.dot(out) >= 0.0 {
                    b.tri(tri[0], tri[1], tri[2], surface);
                } else {
                    b.tri(tri[0], tri[2], tri[1], surface);
                }
            }
        }
        prev = Some(c);
    }
}

/// A flat disc lying on the ground at `c` (lifted `lift`), `sides` around, facing up.
fn disc(b: &mut MeshBuilder, c: Vec3, r0: f32, r1: f32, sides: u32, colour: [f32; 3]) {
    let at = |r: f32, k: u32| c + ring_dir(TAU * k as f32 / sides as f32) * r;
    for k in 0..sides {
        if r0 < 1e-3 {
            let v = [c, at(r1, k + 1), at(r1, k)].map(|p| b.vertex_facing(p, Vec3::Y, colour, [0.0, 0.0]));
            tri_out(b, v[0], v[1], v[2], c - Vec3::Y, Surface::Wall);
        } else {
            let v = [at(r0, k), at(r0, k + 1), at(r1, k + 1), at(r1, k)].map(|p| b.vertex_facing(p, Vec3::Y, colour, [0.0, 0.0]));
            tri_out(b, v[0], v[1], v[2], c - Vec3::Y, Surface::Wall);
            tri_out(b, v[0], v[2], v[3], c - Vec3::Y, Surface::Wall);
        }
    }
}

/// A red plastic tube.
fn tube(b: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32, sides: u32) {
    add_tube(b, a, c, r, sides, Surface::Wall, color::TUBE);
}

/// An orange strap, as a thin tube with the webbing along it.
fn strap(b: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32) {
    add_tube(b, a, c, r, 4, Surface::Wall, color::STRAP);
}

/// Builds the map's structures standing on `terrain`: what collides, and what only draws.
pub(crate) fn build(structures: &[Structure], terrain: &Terrain) -> (TrackMesh, TrackMesh) {
    let mut camp = Camp { solid: MeshBuilder::default(), decor: MeshBuilder::default(), terrain, collide: true };
    for s in structures {
        let site = s.site();
        let seed = s.seed();
        match s.structure {
            StructureKind::Post => {
                camp.collide = true;
                camp.post(site, seed);
            }
            StructureKind::BaseCamp => {
                camp.collide = true;
                camp.base_camp(site, seed);
            }
            StructureKind::Colony => {
                camp.collide = false;
                camp.colony(site, seed);
            }
        }
    }
    (camp.solid.finish(), camp.decor.finish())
}

struct Camp<'a> {
    solid: MeshBuilder,
    decor: MeshBuilder,
    terrain: &'a Terrain,
    /// Whether what is built now collides (goes into `solid`) or only draws.
    collide: bool,
}

impl Camp<'_> {
    /// Where the bulk of what is built now goes.
    fn body(&mut self) -> &mut MeshBuilder {
        if self.collide { &mut self.solid } else { &mut self.decor }
    }

    fn ground(&self, p: Vec2) -> f32 {
        self.terrain.height(p.x, p.y)
    }

    /// The lowest ground within `r` of `p`: what stands there never floats.
    fn foot(&self, p: Vec2, r: f32) -> f32 {
        let mut low = self.ground(p);
        for k in 0..8 {
            let d = ring_dir(TAU * k as f32 / 8.0);
            for f in [0.5, 1.0] {
                low = low.min(self.ground(p + Vec2::new(d.x, d.z) * (r * f)));
            }
        }
        low
    }

    /// The point on the ground at `p`, lifted `lift`.
    fn on_ground(&self, p: Vec2, lift: f32) -> Vec3 {
        Vec3::new(p.x, self.ground(p) + lift, p.y)
    }

    // -----------------------------------------------------------------------------------------
    // Fabric.

    /// An inflated dome on the ground at `c` (the middle of its base): `r` in radius, `h` tall,
    /// sunk a little into the ground. With `gores` > 0 its fabric pillows out between as many
    /// meridian straps, which meet a crown ring near the top, girdled by a strap at each height
    /// in `girdles` (fractions of `h`); their feet pinned by stakes and sandbags, or by stacks
    /// of bags on a giant one. The door, if any, faces `door`.
    #[allow(clippy::too_many_arguments)]
    fn dome(&mut self, c: Vec3, r: f32, h: f32, gores: u32, girdles: &[f32], door: Option<Vec3>, seed: u32) {
        let giant = r > 30.0;
        // Up close its fabric shows its weave; from afar a smooth white coat.
        let skin = if self.collide { color::FABRIC } else { color::PAINT_WHITE };
        let per = if giant { 3 } else { 4 };
        let gores_n = if gores == 0 { 8 } else { gores };
        let sides = gores_n * per;
        let rings = if giant { 12 } else { 9 };
        let sink = (0.03 * h).max(0.3);
        let phi0 = -libm::asinf((sink / h).min(0.4));
        let phase = TAU * rnd(seed, 1);
        let puff = if gores == 0 { 0.0 } else if giant { 0.022 } else { 0.035 };
        // The surface at angle `theta` (from the phase) and elevation `phi`.
        let surf = |theta: f32, phi: f32| -> Vec3 {
            let gore = if gores == 0 { 0.0 } else {
                let g = theta * gores_n as f32 / TAU;
                g - libm::floorf(g)
            };
            let bulge = 1.0 + puff * libm::sinf(PI * gore) * libm::cosf(phi);
            let (sp, cp) = libm::sincosf(phi);
            c + ring_dir(phase + theta) * (r * cp * bulge) + Vec3::Y * (h * sp)
        };
        let normal = |theta: f32, phi: f32| -> Vec3 {
            let (sp, cp) = libm::sincosf(phi);
            (ring_dir(phase + theta) * (cp / r) + Vec3::Y * (sp / h)).normalize()
        };
        let elevation = |j: u32| phi0 + (FRAC_PI_2 - phi0) * (j as f32 / rings as f32);
        let inside = c + Vec3::Y * (0.3 * h);
        {
            let b = self.body();
            let mut grid: Vec<Vec<u32>> = Vec::new();
            for j in 0..rings {
                let phi = elevation(j);
                grid.push((0..sides).map(|k| b.vertex(surf(TAU * k as f32 / sides as f32, phi), skin)).collect());
            }
            let apex = b.vertex(c + Vec3::Y * h, skin);
            let n = sides as usize;
            for j in 0..rings as usize {
                for k in 0..n {
                    let m = (k + 1) % n;
                    if j + 1 < rings as usize {
                        tri_out(b, grid[j][k], grid[j][m], grid[j + 1][k], inside, Surface::Wall);
                        tri_out(b, grid[j][m], grid[j + 1][m], grid[j + 1][k], inside, Surface::Wall);
                    } else {
                        tri_out(b, grid[j][k], grid[j][m], apex, inside, Surface::Wall);
                    }
                }
            }
        }
        if gores == 0 {
            return;
        }
        // The straps: meridians from the ground to the crown, the crown, the girdles.
        let w = if giant { 1.1 } else { 0.22 };
        let lift = if giant { 0.12 } else { 0.03 };
        let crown = elevation(rings - 1) - 0.02;
        let steps = 2 * rings;
        for g in 0..gores_n {
            let theta = TAU * g as f32 / gores_n as f32;
            let side = ring_dir(phase + theta + FRAC_PI_2);
            let pts: Vec<(Vec3, Vec3)> = (0..=steps)
                .map(|i| {
                    let phi = elevation(0).max(0.0) + (crown - elevation(0).max(0.0)) * (i as f32 / steps as f32);
                    let n = normal(theta, phi);
                    (surf(theta, phi) + n * lift, n)
                })
                .collect();
            ribbon(&mut self.decor, &pts, side, w, inside);
        }
        let girdle = |phi: f32, decor: &mut MeshBuilder| {
            let ring_steps = sides * 2;
            let pts: Vec<(Vec3, Vec3)> = (0..=ring_steps)
                .map(|k| {
                    let theta = TAU * k as f32 / ring_steps as f32;
                    let n = normal(theta, phi);
                    (surf(theta, phi) + n * (lift * 1.5), n)
                })
                .collect();
            // Across a girdle: up the dome's surface, along its meridian.
            let (sp, cp) = libm::sincosf(phi);
            let up_dir = |n: Vec3| (Vec3::Y * (h * cp) - Vec3::new(n.x, 0.0, n.z).normalize_or(Vec3::X) * (r * sp)).normalize_or(Vec3::Y);
            ribbon_across(decor, &pts, &up_dir, w, inside);
        };
        girdle(crown, &mut self.decor);
        for &f in girdles {
            girdle(libm::asinf(f.clamp(0.0, 0.95)), &mut self.decor);
        }
        // The straps' feet: stakes and a sandbag, or a stack of bags.
        for g in 0..gores_n {
            let theta = TAU * g as f32 / gores_n as f32;
            let out = ring_dir(phase + theta);
            let foot_p = flat(c + out * (r * 1.02));
            let s = hash2(seed, 40 + g as i32, 3);
            if giant {
                // A pile of big bags: three side by side, two across them on top.
                let bag = Vec3::new(4.2, 1.5, 2.3) * (0.9 + 0.2 * rnd(s, 1));
                let base = Vec3::new(foot_p.x, self.foot(foot_p, 4.0), foot_p.y) + out * 2.0;
                let across = ring_dir(phase + theta + FRAC_PI_2);
                let mut k = 0;
                for (layer, offsets) in [[-0.5f32, 0.5].as_slice(), [0.0f32].as_slice()].into_iter().enumerate() {
                    for &o in offsets {
                        k += 1;
                        let (lie, step) = if layer == 0 { (out, across * (0.92 * bag.z)) } else { (across, out * (0.92 * bag.z)) };
                        let yaw = 0.12 * jit(s, 20 + k);
                        let lie = (lie * libm::cosf(yaw) + Vec3::Y.cross(lie) * libm::sinf(yaw)).normalize();
                        let foot = base + step * o + Vec3::Y * (layer as f32 * 0.8 * bag.y - 0.1);
                        add_sandbag(&mut self.decor, foot, lie, Vec3::Y, bag * (1.0 + 0.1 * jit(s, 30 + k)), hash2(s, k, 4), false, Surface::Wall, color::SANDBAG, [0.24 + 0.24 * rnd(s, 40 + k), 0.3 + 0.7 * rnd(s, 50 + k)]);
                    }
                }
            } else if self.collide {
                let p = Vec3::new(foot_p.x, self.ground(foot_p), foot_p.y);
                let across = ring_dir(phase + theta + FRAC_PI_2);
                stake(&mut self.decor, p + out * 0.45, out, across, s);
                let size = Vec3::new(1.1 + 0.3 * rnd(s, 4), 0.38 + 0.08 * rnd(s, 5), 0.7 + 0.1 * rnd(s, 6));
                let lie = (across + out * (0.3 * jit(s, 7))).normalize();
                add_sandbag(&mut self.decor, p + out * 0.2 - Vec3::Y * 0.04, lie, Vec3::Y, size, s, false, Surface::Wall, color::SANDBAG, [0.24 + 0.24 * rnd(s, 8), 0.3 + 0.7 * rnd(s, 9)]);
            }
        }
        if let Some(d) = door {
            let at = flat(c + d * (r * 0.92));
            let ground = self.ground(at);
            self.airlock(Vec3::new(at.x, ground, at.y), d, (0.16 * h).clamp(1.2, 1.7), seed);
        }
    }

    /// An airlock out of a dome or a tunnel: a short tube of fabric from `at` (on the ground) out
    /// along `out`, its round door at the end: a grey ring, a white hatch with a porthole.
    fn airlock(&mut self, at: Vec3, out: Vec3, r: f32, seed: u32) {
        let len = 1.8 + 0.6 * rnd(seed, 60);
        let axis_y = 0.9 * r;
        let a = at + Vec3::Y * axis_y;
        let end = a + out * len;
        let body = self.body();
        lathe(body, a - out * 0.6, out, &[(0.0, r, color::FABRIC), (len + 0.6, r, color::FABRIC)], 14, Surface::Wall);
        // The door: a ring, the hatch, its porthole.
        lathe(body, end, out, &[(0.0, r * 1.04, color::PAINT_GREY), (0.18, r * 1.04, color::PAINT_GREY), (0.18, r * 0.8, color::PAINT_GREY), (0.12, r * 0.8, color::PAINT_WHITE), (0.12, 0.0, color::PAINT_WHITE)], 16, Surface::Wall);
        let up = Vec3::Y;
        lathe(body, end + out * 0.1 + up * (0.35 * r), out, &[(0.0, 0.22 * r, color::WINDOW), (0.05, 0.22 * r, color::WINDOW), (0.05, 0.0, color::WINDOW)], 12, Surface::Wall);
        let w = 0.18;
        let pts: Vec<(Vec3, Vec3)> = (0..=24)
            .map(|k| {
                let t = TAU * k as f32 / 24.0;
                let side = out.cross(Vec3::Y).normalize();
                let n = side * libm::cosf(t) + Vec3::Y * libm::sinf(t);
                (a + out * (0.55 * len) + n * (r + 0.03), n)
            })
            .collect();
        ribbon_across(&mut self.decor, &pts, &|_| out, w, a + out * (0.55 * len));
    }

    /// A tunnel module of fabric lying on the ground from `a` along `dir`, `len` long and `r` in
    /// radius, its ends rounded, girdled with straps every few metres, an airlock at its far
    /// end.
    fn tunnel(&mut self, a: Vec2, dir: Vec3, len: f32, r: f32, seed: u32) {
        let steps = libm::ceilf(len / 2.0).max(2.0) as u32;
        let side = Vec3::Y.cross(dir).normalize();
        let base = |t: f32, me: &Camp| -> Vec3 {
            let p = a + flat(dir) * t;
            Vec3::new(p.x, me.foot(p, 0.6 * r), p.y)
        };
        let axis_h = 0.62 * r;
        let cap = 0.75 * r;
        let around = 18u32;
        let mut rows: Vec<Vec<u32>> = Vec::new();
        let mut centres = Vec::new();
        // Along: the rounded start, the body, the rounded end.
        let mut ts: Vec<(f32, f32)> = Vec::new();
        for i in 0..4 {
            let q = i as f32 / 4.0;
            let ang = FRAC_PI_2 * (1.0 - q);
            ts.push((-cap * libm::sinf(ang), libm::cosf(ang)));
        }
        for i in 0..=steps {
            ts.push((len * i as f32 / steps as f32, 1.0));
        }
        for i in 1..=4 {
            let ang = FRAC_PI_2 * i as f32 / 4.0;
            ts.push((len + cap * libm::sinf(ang), libm::cosf(ang)));
        }
        let ground_at: Vec<Vec3> = ts.iter().map(|&(t, _)| base(t.clamp(0.0, len), self)).collect();
        let b = self.body();
        for (i, &(t, k)) in ts.iter().enumerate() {
            let g = ground_at[i];
            let centre = Vec3::new(a.x + dir.x * t, g.y + axis_h, a.y + dir.z * t);
            centres.push(centre);
            let rr = r * k.max(0.0);
            if rr < 1e-3 {
                rows.push(vec![b.vertex(centre, color::FABRIC)]);
                continue;
            }
            rows.push(
                (0..around)
                    .map(|j| {
                        let ang = TAU * j as f32 / around as f32;
                        b.vertex(centre + (side * libm::cosf(ang) + Vec3::Y * libm::sinf(ang)) * rr, color::FABRIC)
                    })
                    .collect(),
            );
        }
        for i in 0..rows.len() - 1 {
            let inside = 0.5 * (centres[i] + centres[i + 1]);
            let (p, q) = (&rows[i], &rows[i + 1]);
            for j in 0..around as usize {
                let m = (j + 1) % around as usize;
                let at = |row: &Vec<u32>, j: usize| row[j.min(row.len() - 1)];
                if p.len() > 1 {
                    tri_out(b, at(p, j), at(p, m), at(q, j), inside, Surface::Wall);
                }
                if q.len() > 1 {
                    tri_out(b, at(p, m), at(q, m), at(q, j), inside, Surface::Wall);
                }
            }
        }
        // Straps round it every 3 to 4 m.
        let count = libm::roundf(len / 3.5).max(2.0) as u32;
        for i in 0..count {
            let t = len * (i as f32 + 0.5) / count as f32 + 0.4 * jit(seed, 70 + i as i32);
            let g = base(t, self);
            let centre = Vec3::new(a.x + dir.x * t, g.y + axis_h, a.y + dir.z * t);
            let pts: Vec<(Vec3, Vec3)> = (0..=around)
                .map(|j| {
                    let ang = -0.35 + (PI + 0.7) * j as f32 / around as f32;
                    let n = side * libm::cosf(ang) + Vec3::Y * libm::sinf(ang);
                    (centre + n * (r + 0.03), n)
                })
                .collect();
            ribbon_across(&mut self.decor, &pts, &|_| dir, 0.24, centre);
        }
        let end = a + flat(dir) * (len + 0.6 * cap);
        let g = self.ground(end);
        self.airlock(Vec3::new(end.x, g, end.y), dir, 0.62 * r, seed);
    }

    // -----------------------------------------------------------------------------------------
    // Freight.

    /// A tank lying on red cradles, centred over `p` along `dir`: white, girdled with orange.
    fn tank(&mut self, p: Vec2, dir: Vec3, r: f32, len: f32, seed: u32) {
        let lift = 0.55 + 0.25 * r;
        let ground = self.foot(p, 0.5 * len);
        let centre = Vec3::new(p.x, ground + lift + r, p.y);
        let start = centre - dir * (0.5 * len);
        let cap = 0.8 * r;
        let mut prof = Vec::new();
        let cap_steps = if self.collide { 4 } else { 3 };
        for i in 0..=cap_steps {
            let a = FRAC_PI_2 * i as f32 / cap_steps as f32;
            prof.push((cap * (1.0 - libm::cosf(a)), r * libm::sinf(a), color::PAINT_WHITE));
        }
        let body_end = len - cap;
        let bands = [0.3, 0.7];
        for &f in &bands {
            let t = cap + (body_end - cap) * f;
            prof.push((t - 0.25, r, color::PAINT_WHITE));
            prof.push((t - 0.25, r, color::PAINT_ORANGE));
            prof.push((t + 0.25, r, color::PAINT_ORANGE));
            prof.push((t + 0.25, r, color::PAINT_WHITE));
        }
        for i in 1..=cap_steps {
            let a = FRAC_PI_2 * i as f32 / cap_steps as f32;
            prof.push((body_end + cap * libm::sinf(a), r * libm::cosf(a), color::PAINT_WHITE));
        }
        let sides = if self.collide { 14 } else { 12 };
        lathe(self.body(), start, dir, &prof, sides, Surface::Wall);
        // The cradles: a saddle of red tubes under each third.
        let side = Vec3::Y.cross(dir).normalize();
        for f in [0.22, 0.78] {
            let c = start + dir * (len * f);
            let floor = Vec3::new(c.x, ground, c.z);
            let tr = (0.06 + 0.02 * r).min(0.16);
            let ts = if self.collide { 6 } else { 4 };
            for s in [-1.0f32, 1.0] {
                let foot = floor + side * (s * 0.95 * r);
                let top = c + side * (s * 0.62 * r) - Vec3::Y * (0.78 * r);
                tube(self.body(), foot - Vec3::Y * 0.1, top, tr, ts);
                tube(self.body(), foot + dir * (0.5 * r), top, 0.7 * tr, ts);
            }
            tube(self.body(), floor + side * (0.95 * r) + Vec3::Y * 0.1, floor - side * (0.95 * r) + Vec3::Y * 0.1, tr, ts);
            let saddle = c - Vec3::Y * (0.98 * r);
            tube(self.body(), saddle + side * (0.62 * r) + Vec3::Y * (0.2 * r), saddle - side * (0.62 * r) + Vec3::Y * (0.2 * r), tr, ts);
        }
        let _ = seed;
    }

    /// A shipping container on the ground at `p` (its middle), along `dir`, `stack` levels up:
    /// painted steel, ribbed sides, its doors at one end.
    fn container(&mut self, p: Vec2, dir: Vec3, stack: u32, colour: [f32; 3], seed: u32) {
        let (hl, hw, h) = (3.03, 1.22, 2.59);
        let y = self.foot(p, 3.0) - 0.05 + stack as f32 * h;
        let c = Vec3::new(p.x, y, p.y);
        let side = Vec3::Y.cross(dir).normalize();
        // Ribs on the long sides (up close only), the frame at the corners, the doors' bars.
        let n = if self.collide { 14 } else { 0 };
        let b = self.body();
        add_box(b, c + Vec3::Y * (0.5 * h), Vec3::new(hw, 0.5 * h, hl), dir, Surface::Wall, colour, false);
        for s in [-1.0f32, 1.0] {
            for i in 0..n {
                let t = -hl + 0.25 + (2.0 * hl - 0.5) * (i as f32 + 0.5) / n as f32;
                add_box(b, c + side * (s * (hw + 0.02)) + dir * t + Vec3::Y * (0.5 * h), Vec3::new(0.025, 0.46 * h, 0.06), dir, Surface::Wall, colour, false);
            }
        }
        for (s, t) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            add_box(b, c + side * (s * (hw - 0.05)) + dir * (t * (hl - 0.05)) + Vec3::Y * (0.5 * h), Vec3::new(0.08, 0.5 * h + 0.01, 0.08), dir, Surface::Wall, color::PAINT_GREY, false);
        }
        for s in [-0.6f32, -0.2, 0.2, 0.6] {
            add_box(b, c + side * (s * hw) + dir * (hl + 0.03) + Vec3::Y * (0.5 * h), Vec3::new(0.025, 0.45 * h, 0.03), dir, Surface::Wall, color::PAINT_GREY, false);
        }
        let _ = seed;
    }

    /// A table of solar panels on red tubes at `p`, facing `dir` (tilted about 25° toward it):
    /// `n` panels side by side in a grey frame, each split into two halves of cells.
    fn solar_table(&mut self, p: Vec2, dir: Vec3, n: u32, seed: u32) {
        let side = Vec3::Y.cross(dir).normalize();
        let (pw, pl) = (1.05, 2.0);
        let half = 0.5 * pw * n as f32;
        let tilt = (22.0 + 6.0 * rnd(seed, 1)).to_radians();
        let ground = self.foot(p, half);
        let low = 0.5;
        let (st, ct) = libm::sincosf(tilt);
        // The panels' normal, and up their slope, away from `dir`.
        let up = Vec3::Y * ct + dir * st;
        let slope = Vec3::Y * st - dir * ct;
        let front = Vec3::new(p.x, ground + low, p.y) + dir * (0.5 * pl * ct);
        let corner = |u: f32, v: f32| front + side * u + slope * v;
        let below = front - up;
        let quad = |b: &mut MeshBuilder, u0: f32, u1: f32, v0: f32, v1: f32, lift: f32, colour: [f32; 3]| {
            let q = [corner(u0, v0), corner(u1, v0), corner(u1, v1), corner(u0, v1)].map(|x| b.vertex_facing(x + up * lift, up, colour, [0.0, 0.0]));
            tri_out(b, q[0], q[1], q[2], below, Surface::Wall);
            tri_out(b, q[0], q[2], q[3], below, Surface::Wall);
        };
        let near = self.collide;
        let b = self.body();
        quad(b, -half - 0.04, half + 0.04, -0.04, pl + 0.04, 0.03, color::PAINT_GREY);
        for i in 0..n {
            let u0 = -half + i as f32 * pw + 0.03;
            let u1 = u0 + pw - 0.06;
            if near {
                quad(b, u0, u1, 0.03, 0.5 * pl - 0.015, 0.05, color::SOLAR);
                quad(b, u0, u1, 0.5 * pl + 0.015, pl - 0.03, 0.05, color::SOLAR);
            } else {
                quad(b, u0, u1, 0.03, pl - 0.03, 0.05, color::SOLAR);
            }
        }
        // Legs: short in front, tall behind, braced.
        let legs = if near { (n / 2).max(2) } else { 1 };
        for i in 0..=legs {
            let u = -half + 0.15 + (2.0 * half - 0.3) * i as f32 / legs as f32;
            let f_top = corner(u, 0.15);
            let b_top = corner(u, pl - 0.15);
            let f_foot = Vec3::new(f_top.x, ground - 0.1, f_top.z);
            let b_foot = Vec3::new(b_top.x, ground - 0.1, b_top.z);
            let ts = if near { 6 } else { 4 };
            tube(b, f_foot, f_top, 0.04, ts);
            tube(b, b_foot, b_top, 0.045, ts);
            tube(b, f_foot + Vec3::Y * 0.2, b_top - Vec3::Y * 0.1, 0.03, ts);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Masts and towers.

    /// A lattice tower of red tubes standing on the ground at `c`: four legs from a square
    /// `base` metres across to `top` metres across at `h`, an X brace on each face of each
    /// storey and a ledger between storeys.
    #[allow(clippy::too_many_arguments)]
    fn lattice(&mut self, c: Vec3, yaw: Vec3, base: f32, top: f32, h: f32, storeys: u32, leg_r: f32, brace_r: f32, sides: u32) -> [Vec3; 4] {
        let side = Vec3::Y.cross(yaw).normalize();
        let corner = |k: usize, t: f32| {
            let half = 0.5 * (base + (top - base) * t);
            let (a, b) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][k];
            c + side * (a * half) + yaw * (b * half) + Vec3::Y * (h * t)
        };
        // Storeys a little shorter toward the top.
        let level = |i: u32| {
            let q = i as f32 / storeys as f32;
            q * (1.25 - 0.25 * q)
        };
        let b = self.body();
        for k in 0..4 {
            tube(b, corner(k, 0.0) - Vec3::Y * 0.3, corner(k, 1.0), leg_r, sides);
        }
        for i in 0..storeys {
            let (t0, t1) = (level(i), level(i + 1));
            for k in 0..4 {
                let m = (k + 1) % 4;
                tube(b, corner(k, t0), corner(m, t1), brace_r, sides);
                tube(b, corner(m, t0), corner(k, t1), brace_r, sides);
                tube(b, corner(k, t1), corner(m, t1), brace_r, sides);
            }
        }
        [0, 1, 2, 3].map(|k| corner(k, 1.0))
    }

    /// A flag pole on the ground at `p`: a steel pole `h` tall with an orange pennant, guyed.
    fn pennant(&mut self, p: Vec2, h: f32, wind: Vec3, seed: u32) {
        let foot = self.on_ground(p, -0.3);
        let top = foot + Vec3::Y * (h + 0.3);
        add_post(&mut self.decor, foot, top, 0.045, 6, 0.0, Surface::Wall, color::STEEL);
        // The pennant: a long triangle streaming downwind, a little twisted.
        let len = 1.6 + 0.8 * rnd(seed, 1);
        let drop = 0.25 + 0.2 * rnd(seed, 2);
        let tip = top - Vec3::Y * (0.45 + drop) + wind * len;
        let a = top - Vec3::Y * 0.08;
        let b = top - Vec3::Y * 0.9;
        flag(&mut self.decor, a, b, tip, wind);
        for k in 0..3 {
            let d = ring_dir(TAU * (k as f32 / 3.0 + rnd(seed, 3)));
            let anchor = flat(foot + d * (0.45 * h));
            let g = self.ground(anchor);
            strap(&mut self.decor, foot + Vec3::Y * (0.75 * h), Vec3::new(anchor.x, g + 0.05, anchor.y), 0.012);
            stake(&mut self.decor, Vec3::new(anchor.x, g, anchor.y), d, Vec3::Y.cross(d), hash2(seed, 10 + k, 1));
        }
    }

    /// A windsock on a pole at `p`, its sock streaming downwind: orange and white rings.
    fn windsock(&mut self, p: Vec2, wind: Vec3, seed: u32) {
        let h = 5.0 + rnd(seed, 1);
        let foot = self.on_ground(p, -0.3);
        let top = foot + Vec3::Y * (h + 0.3);
        add_post(&mut self.decor, foot, top, 0.05, 6, 0.0, Surface::Wall, color::STEEL);
        let droop = (8.0 + 10.0 * rnd(seed, 2)).to_radians();
        let axis = (wind * libm::cosf(droop) - Vec3::Y * libm::sinf(droop)).normalize();
        let start = top - Vec3::Y * 0.25 + wind * 0.15;
        let len = 2.6;
        let mut prof = Vec::new();
        for i in 0..5 {
            let (t0, t1) = (len * i as f32 / 5.0, len * (i + 1) as f32 / 5.0);
            let r = |t: f32| 0.32 - 0.14 * t / len;
            let c = if i % 2 == 0 { color::PAINT_ORANGE } else { color::PAINT_WHITE };
            prof.push((t0, r(t0), c));
            prof.push((t1, r(t1), c));
        }
        prof.push((len, 0.0, color::PAINT_WHITE));
        lathe(&mut self.decor, start, axis, &prof, 12, Surface::Wall);
    }

    /// A beacon: a lamp of `colour` on a grey base, at `p`.
    fn beacon(b: &mut MeshBuilder, p: Vec3, r: f32, colour: [f32; 3]) {
        lathe(b, p, Vec3::Y, &[(0.0, 1.1 * r, color::PAINT_GREY), (0.35 * r, 1.1 * r, color::PAINT_GREY), (0.35 * r, 0.9 * r, colour), (1.5 * r, 0.9 * r, colour), (1.9 * r, 0.0, colour)], 10, Surface::Wall);
    }

    /// A dish `r` in radius at `p`, looking along `look`, on a short mount.
    fn dish(b: &mut MeshBuilder, p: Vec3, look: Vec3, r: f32) {
        let depth = 0.28 * r;
        lathe(b, p, look, &[(0.0, 0.0, color::PAINT_WHITE), (0.25 * depth, 0.5 * r, color::PAINT_WHITE), (depth, r, color::PAINT_WHITE), (depth, 0.96 * r, color::PAINT_WHITE), (0.2 * depth, 0.0, color::PAINT_WHITE)], 16, Surface::Wall);
        add_tube(b, p + look * depth, p + look * (1.1 * r), 0.025 * r.max(1.0), 5, Surface::Wall, color::STEEL);
        add_tube(b, p - look * (0.3 * r), p, 0.06 * r.max(1.0), 6, Surface::Wall, color::PAINT_GREY);
    }

    // -----------------------------------------------------------------------------------------
    // Ground clutter.

    /// A low wall of sandbags from `a` to `b`, `layers` high, staggered like brickwork.
    fn bag_wall(&mut self, a: Vec2, b: Vec2, layers: u32, seed: u32) {
        let run = b - a;
        let len = run.length();
        let dir = Vec3::new(run.x, 0.0, run.y) / len;
        let side = Vec3::Y.cross(dir);
        let bag = Vec3::new(1.35, 0.4, 0.75);
        let mut n = 0;
        for layer in 0..layers {
            let count = libm::floorf((len - 0.5 * bag.x * layer as f32) / (0.95 * bag.x)).max(1.0) as i32;
            for i in 0..count {
                n += 1;
                let h = hash2(seed, n, layer as i32);
                let t = (0.5 + i as f32 + 0.5 * layer as f32) * 0.95 * bag.x + 0.05 * jit(h, 1);
                let p = a + flat(dir) * t + flat(side) * (0.06 * jit(h, 2));
                let g = self.foot(p, 0.5) + layer as f32 * 0.82 * bag.y - 0.04;
                let yaw = 0.12 * jit(h, 3);
                let d = (dir * libm::cosf(yaw) + side * libm::sinf(yaw)).normalize();
                let size = bag * (1.0 + 0.1 * jit(h, 4));
                add_sandbag(&mut self.decor, Vec3::new(p.x, g, p.y), d, Vec3::Y, size, h, false, Surface::Wall, color::SANDBAG, [0.24 + 0.24 * rnd(h, 5), 0.3 + 0.7 * rnd(h, 6)]);
            }
        }
        // What the car meets: a plain box, not drawn.
        let mid = a + run * 0.5;
        let g = self.ground(mid);
        let top = layers as f32 * 0.82 * bag.y;
        add_box(&mut self.solid, Vec3::new(mid.x, g + 0.5 * top, mid.y), Vec3::new(0.5 * bag.z, 0.5 * top, 0.5 * len), dir, Surface::Wall, color::HULL, false);
    }

    /// Crates and drums stacked at `p`, some under a strap.
    fn clutter(&mut self, p: Vec2, dir: Vec3, seed: u32) {
        let side = Vec3::Y.cross(dir);
        for i in 0..6 {
            let s = hash2(seed, i, 7);
            let at = p + flat(dir) * (1.3 * (i % 3) as f32 - 1.3 + 0.2 * jit(s, 1)) + flat(side) * (1.2 * (i / 3) as f32 + 0.2 * jit(s, 2));
            let g = self.foot(at, 0.6);
            if rnd(s, 3) < 0.5 {
                let size = Vec3::new(0.45 + 0.15 * rnd(s, 4), 0.35 + 0.15 * rnd(s, 5), 0.6 + 0.2 * rnd(s, 6));
                let yaw = 0.3 * jit(s, 7);
                let d = (dir * libm::cosf(yaw) + side * libm::sinf(yaw)).normalize();
                add_box(self.body(), Vec3::new(at.x, g + size.y, at.y), size, d, Surface::Wall, color::PAINT_ORANGE, false);
                if rnd(s, 8) < 0.5 {
                    add_box(self.body(), Vec3::new(at.x, g + 2.0 * size.y + 0.25, at.y), Vec3::new(0.32, 0.25, 0.42), d, Surface::Wall, color::PAINT_GREY, false);
                }
            } else {
                for k in 0..3 {
                    let q = at + Vec2::new(0.62 * (k as f32 - 1.0), 0.15 * jit(s, 9 + k));
                    let foot = Vec3::new(q.x, self.ground(q) - 0.02, q.y);
                    lathe(self.body(), foot, Vec3::Y, &[(0.0, 0.29, color::PAINT_ORANGE), (0.88, 0.29, color::PAINT_ORANGE), (0.88, 0.25, color::PAINT_GREY), (0.9, 0.0, color::PAINT_GREY)], 12, Surface::Wall);
                }
            }
        }
    }

    /// A six-wheeled rover parked at `p`, facing `dir`.
    fn rover(&mut self, p: Vec2, dir: Vec3, seed: u32) {
        let side = Vec3::Y.cross(dir);
        let g = self.foot(p, 2.5);
        let c = Vec3::new(p.x, g, p.y);
        let wr = 0.62;
        let b = self.body();
        for (i, t) in [-1.7f32, 0.0, 1.7].iter().enumerate() {
            for s in [-1.0f32, 1.0] {
                let hub = c + dir * *t + side * (s * 1.25) + Vec3::Y * wr;
                lathe(b, hub - side * 0.25, side, &[(0.0, 0.0, color::TYRE), (0.0, wr * 0.9, color::TYRE), (0.08, wr, color::TYRE), (0.42, wr, color::TYRE), (0.5, wr * 0.9, color::TYRE), (0.5, 0.0, color::TYRE)], 14, Surface::Wall);
                lathe(b, hub + side * (s * 0.27), side * s, &[(0.0, 0.33, color::PAINT_GREY), (0.03, 0.33, color::PAINT_GREY), (0.03, 0.0, color::PAINT_GREY)], 8, Surface::Wall);
                let _ = i;
            }
        }
        // The body: a white chassis, a cab with windows, a dark rack behind.
        add_box(b, c + Vec3::Y * 1.25, Vec3::new(1.05, 0.38, 2.5), dir, Surface::Wall, color::PAINT_WHITE, true);
        add_box(b, c + Vec3::Y * 1.95 + dir * 0.9, Vec3::new(0.98, 0.42, 1.05), dir, Surface::Wall, color::PAINT_WHITE, false);
        add_box(b, c + Vec3::Y * 2.02 + dir * 0.96, Vec3::new(1.0, 0.24, 1.0), dir, Surface::Wall, color::WINDOW, false);
        add_box(b, c + Vec3::Y * 1.72 - dir * 1.35, Vec3::new(0.95, 0.12, 1.0), dir, Surface::Wall, color::PAINT_BLACK, false);
        add_box(b, c + Vec3::Y * 1.32 + dir * 2.52, Vec3::new(0.9, 0.12, 0.05), dir, Surface::Wall, color::PAINT_ORANGE, false);
        Self::beacon(b, c + Vec3::Y * 2.37 + dir * 0.6 + side * 0.6, 0.09, color::BEACON_AMBER);
        add_post(&mut self.decor, c + Vec3::Y * 2.3 - dir * 0.2 - side * 0.7, c + Vec3::Y * 4.2 - dir * 0.2 - side * 0.7, 0.015, 4, 0.0, Surface::Wall, color::STEEL);
        let _ = seed;
    }

    // -----------------------------------------------------------------------------------------
    // The structures.

    /// The observation post.
    fn post(&mut self, site: Site, seed: u32) {
        let fwd = site.dir(0.0);
        let side = Vec3::Y.cross(fwd);
        let h = 13.0 + 4.0 * rnd(seed, 1);
        let ground = self.foot(site.o, 2.4);
        let base_c = Vec3::new(site.o.x, ground, site.o.y);
        let stack_h = 1.0;
        let leg_c = base_c + Vec3::Y * stack_h;
        // Sandbag stacks under the legs.
        for k in 0..4 {
            let (a, b) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][k];
            let p = flat(base_c + side * (a * 1.6) + fwd * (b * 1.6));
            let foot = Vec3::new(p.x, self.foot(p, 0.9), p.y);
            let s = hash2(seed, k as i32, 11);
            bag_stack(&mut self.solid, &mut self.decor, foot, stack_h + 0.25, fwd, 1.6, 1.5, Vec3::new(1.3, 0.4, 0.75), s);
        }
        let tops = self.lattice(leg_c, fwd, 3.2, 2.9, h - stack_h, ((h - stack_h) / 2.6) as u32, 0.16, 0.085, 8);
        let deck_y = leg_c.y + (h - stack_h);
        let deck_c = Vec3::new(site.o.x, deck_y, site.o.y);
        // The platform and its railing.
        let b = self.body();
        add_box(b, deck_c + Vec3::Y * 0.08, Vec3::new(2.4, 0.08, 2.4), fwd, Surface::Wall, color::SLAB, true);
        for k in 0..4 {
            let (a, bb) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][k];
            let (m, mb) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][(k + 1) % 4];
            let p0 = deck_c + side * (a * 2.35) + fwd * (bb * 2.35) + Vec3::Y * 0.16;
            let p1 = deck_c + side * (m * 2.35) + fwd * (mb * 2.35) + Vec3::Y * 0.16;
            tube(&mut self.decor, p0, p0 + Vec3::Y * 1.05, 0.045, 6);
            tube(&mut self.decor, p0 + Vec3::Y * 1.05, p1 + Vec3::Y * 1.05, 0.04, 6);
            tube(&mut self.decor, p0 + Vec3::Y * 0.55, p1 + Vec3::Y * 0.55, 0.03, 6);
            tube(&mut self.decor, p0.lerp(p1, 0.5), p0.lerp(p1, 0.5) + Vec3::Y * 1.05, 0.035, 6);
        }
        // The cabin: fabric walls, a band of windows round three sides, a white roof.
        let cab = deck_c + Vec3::Y * 0.16;
        let hw = 1.7;
        let b = self.body();
        add_box(b, cab + Vec3::Y * 0.5, Vec3::new(hw, 0.5, hw), fwd, Surface::Wall, color::FABRIC, false);
        add_box(b, cab + Vec3::Y * 1.5, Vec3::new(hw - 0.06, 0.5, hw - 0.06), fwd, Surface::Wall, color::WINDOW, false);
        add_box(b, cab + Vec3::Y * 2.25, Vec3::new(hw, 0.25, hw), fwd, Surface::Wall, color::FABRIC, false);
        add_box(b, cab + Vec3::Y * 2.58, Vec3::new(hw + 0.28, 0.08, hw + 0.28), fwd, Surface::Wall, color::PAINT_WHITE, true);
        // The back wall stays fabric, with its door; mullions on the windows.
        add_box(b, cab + Vec3::Y * 1.5 - fwd * (hw - 0.02), Vec3::new(hw, 0.5, 0.05), fwd, Surface::Wall, color::FABRIC, false);
        add_box(b, cab + Vec3::Y * 1.0 - fwd * (hw + 0.02) + side * 0.6, Vec3::new(0.42, 0.95, 0.03), fwd, Surface::Wall, color::PAINT_WHITE, false);
        for k in 0..4 {
            let (a, bb) = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)][k];
            add_box(b, cab + side * (a * (hw - 0.02)) + fwd * (bb * (hw - 0.02)) + Vec3::Y * 1.5, Vec3::new(0.07, 0.52, 0.07), fwd, Surface::Wall, color::PAINT_GREY, false);
        }
        for u in [-0.6f32, 0.6] {
            add_box(b, cab + side * u + fwd * (hw - 0.02) + Vec3::Y * 1.5, Vec3::new(0.035, 0.5, 0.035), fwd, Surface::Wall, color::PAINT_GREY, false);
            add_box(b, cab + fwd * u + side * (hw - 0.02) + Vec3::Y * 1.5, Vec3::new(0.035, 0.5, 0.035), fwd, Surface::Wall, color::PAINT_GREY, false);
            add_box(b, cab + fwd * u - side * (hw - 0.02) + Vec3::Y * 1.5, Vec3::new(0.035, 0.5, 0.035), fwd, Surface::Wall, color::PAINT_GREY, false);
        }
        // On the roof: two antennas, a dish, a beacon, a flag.
        let roof = cab + Vec3::Y * 2.66;
        add_post(&mut self.decor, roof + side * 1.2 - fwd * 1.1, roof + side * 1.2 - fwd * 1.1 + Vec3::Y * 3.4, 0.03, 5, 0.0, Surface::Wall, color::STEEL);
        add_post(&mut self.decor, roof + side * 0.8 - fwd * 1.3, roof + side * 0.8 - fwd * 1.3 + Vec3::Y * 2.3, 0.025, 5, 0.0, Surface::Wall, color::STEEL);
        let dish_at = roof - side * 1.25 - fwd * 1.0 + Vec3::Y * 0.8;
        add_post(&mut self.decor, roof - side * 1.25 - fwd * 1.0, dish_at, 0.05, 6, 0.0, Surface::Wall, color::PAINT_GREY);
        Self::dish(&mut self.decor, dish_at, (site.dir(150.0 + 40.0 * rnd(seed, 2)) + Vec3::Y * 0.5).normalize(), 0.6);
        Self::beacon(&mut self.decor, roof + Vec3::Y * 0.02, 0.17, color::BEACON_AMBER);
        let pole = roof - side * 1.5 + fwd * 1.5;
        add_post(&mut self.decor, pole, pole + Vec3::Y * 3.0, 0.03, 5, 0.0, Surface::Wall, color::STEEL);
        let wind = site.dir(100.0 + 30.0 * rnd(seed, 3));
        flag(&mut self.decor, pole + Vec3::Y * 2.95, pole + Vec3::Y * 2.2, pole + Vec3::Y * 2.6 + wind * 1.3, wind);
        // The ladder up one face, inside the lattice.
        let l0 = leg_c - fwd * 1.35;
        for s in [-0.25f32, 0.25] {
            add_tube(&mut self.decor, l0 + side * s - Vec3::Y * 1.0, l0 + side * s + Vec3::Y * (deck_y - leg_c.y), 0.025, 5, Surface::Wall, color::STEEL);
        }
        let rungs = ((deck_y - leg_c.y + 1.0) / 0.35) as i32;
        for i in 0..rungs {
            let y = -0.85 + 0.35 * i as f32;
            add_tube(&mut self.decor, l0 - side * 0.25 + Vec3::Y * y, l0 + side * 0.25 + Vec3::Y * y, 0.018, 4, Surface::Wall, color::STEEL);
        }
        // Guy straps from the legs out to stakes.
        for (k, top) in tops.iter().enumerate() {
            let from = leg_c + (*top - leg_c) * 0.82;
            let out = (Vec3::new(from.x - site.o.x, 0.0, from.z - site.o.y)).normalize();
            let reach = 0.5 * h + 1.5 * rnd(seed, 20 + k as i32);
            let at = flat(from + out * reach);
            let g = self.ground(at);
            let anchor = Vec3::new(at.x, g, at.y);
            strap(&mut self.decor, from, anchor + Vec3::Y * 0.08, 0.035);
            stake(&mut self.decor, anchor, out, Vec3::Y.cross(out), hash2(seed, 30 + k as i32, 2));
            let buckle = anchor.lerp(from, 0.18);
            add_box(&mut self.decor, buckle, Vec3::new(0.05, 0.04, 0.08), out, Surface::Wall, color::STEEL, true);
        }
    }

    /// The base camp: domes and tunnels at the back, tanks on one side, containers on the other,
    /// a solar field and sandbag walls in front, masts between them.
    fn base_camp(&mut self, site: Site, seed: u32) {
        // Laid out with its front toward -z: turned so that it faces the site's forward.
        let site = site.turned();
        // The layout below, squeezed to about 85 by 70 m.
        let at = |x: f32, z: f32| site.at(0.85 * x, 0.72 * z);
        let j = |k: i32| jit(seed, k);
        let back = site.dir(180.0);
        // Domes, their doors toward the front.
        let domes = [(-24.0, 26.0, 8.6, 6.6, 6u32), (1.0, 33.0, 7.2, 5.6, 6), (24.0, 25.0, 6.4, 5.0, 5), (-6.0, 8.0, 5.2, 4.0, 4)];
        for (i, &(x, z, r, h, g)) in domes.iter().enumerate() {
            let p = at(x + 1.2 * j(10 + i as i32), z + 1.2 * j(20 + i as i32));
            let g0 = self.foot(p, 0.7 * r);
            let door = site.dir(-15.0 + 30.0 * rnd(seed, 30 + i as i32));
            self.dome(Vec3::new(p.x, g0, p.y), r, h, g, &[0.32], Some(door), hash2(seed, i as i32, 40));
        }
        // Tunnel modules.
        self.tunnel(at(-10.0, 21.0), site.dir(-90.0), 14.0, 2.5, hash2(seed, 1, 50));
        self.tunnel(at(33.0, 14.0), back, 13.0, 2.3, hash2(seed, 2, 50));
        // Tanks on the left, side by side.
        for i in 0..4 {
            let p = at(38.0 + 3.6 * i as f32, -8.0 + 0.6 * j(60 + i));
            self.tank(p, site.dir(2.0 * j(70 + i)), 1.25 + 0.1 * rnd(seed, 80 + i), 7.0 + 0.8 * rnd(seed, 90 + i), hash2(seed, i, 60));
        }
        // Containers on the right, two stacked.
        let containers = [(-34.0, -2.0, 0u32, 90.0, true), (-34.3, -2.1, 1, 91.5, false), (-34.0, 3.5, 0, 89.0, false), (-27.0, -10.0, 0, 8.0, true), (-40.0, 10.0, 0, 92.0, true)];
        for (i, &(x, z, level, yaw, orange)) in containers.iter().enumerate() {
            let colour = if orange { color::PAINT_ORANGE } else { color::PAINT_WHITE };
            self.container(at(x, z), site.dir(yaw + 2.0 * j(100 + i as i32)), level, colour, hash2(seed, i as i32, 100));
        }
        // The solar field, in front on the right, facing the front.
        for row in 0..3 {
            for t in 0..3 {
                let p = at(-44.0 + 8.6 * t as f32 + 0.5 * j(110 + 3 * row + t), -22.0 - 5.5 * row as f32);
                self.solar_table(p, site.dir(180.0), 6, hash2(seed, row, 110 + t));
            }
        }
        // A lattice mast with dishes and a beacon, in the middle.
        let mast_p = at(8.0, -4.0);
        let mg = self.foot(mast_p, 1.0);
        let tops = self.lattice(Vec3::new(mast_p.x, mg, mast_p.y), site.dir(20.0), 1.4, 0.8, 15.0, 6, 0.07, 0.045, 6);
        let top = (tops[0] + tops[1] + tops[2] + tops[3]) / 4.0;
        Self::beacon(&mut self.decor, top, 0.2, color::BEACON_RED);
        Self::dish(&mut self.decor, tops[0] + Vec3::Y * -1.2 + site.dir(20.0) * 0.6, (site.dir(10.0) + Vec3::Y * 0.4).normalize(), 0.9);
        Self::dish(&mut self.decor, tops[2] + Vec3::Y * -3.0 - site.dir(20.0) * 0.5, (site.dir(200.0) + Vec3::Y * 0.6).normalize(), 0.7);
        add_post(&mut self.decor, top, top + Vec3::Y * 4.0, 0.04, 5, 0.0, Surface::Wall, color::STEEL);
        for k in 0..4 {
            let d = site.dir(65.0 + 90.0 * k as f32);
            let at = mast_p + flat(d) * 9.0;
            let g = self.ground(at);
            strap(&mut self.decor, Vec3::new(mast_p.x, mg + 11.0, mast_p.y), Vec3::new(at.x, g + 0.05, at.y), 0.02);
            stake(&mut self.decor, Vec3::new(at.x, g, at.y), d, Vec3::Y.cross(d), hash2(seed, k, 120));
        }
        // Pennants, a windsock, a rover, crates.
        let wind = site.dir(80.0 + 20.0 * j(130));
        for (i, &(x, z)) in [(-48.0, -40.0), (46.0, -38.0), (14.0, 20.0), (-14.0, 40.0)].iter().enumerate() {
            self.pennant(at(x, z), 6.0 + 2.0 * rnd(seed, 140 + i as i32), wind, hash2(seed, i as i32, 140));
        }
        self.windsock(at(-8.0, -36.0), wind, hash2(seed, 1, 150));
        self.rover(at(18.0, -24.0), site.dir(160.0 + 10.0 * j(160)), hash2(seed, 1, 160));
        self.clutter(at(-16.0, -12.0), site.dir(10.0), hash2(seed, 1, 170));
        self.clutter(at(30.0, -26.0), site.dir(-30.0), hash2(seed, 2, 170));
        // Sandbag walls along the front, with gaps.
        for (i, &(x0, x1)) in [(-46.0, -28.0), (-12.0, 4.0), (18.0, 38.0)].iter().enumerate() {
            let a = at(x0, -44.0 + 1.5 * j(180 + i as i32));
            let b = at(x1, -44.0 + 1.5 * j(190 + i as i32));
            self.bag_wall(a, b, 2, hash2(seed, i as i32, 180));
        }
        self.bag_wall(at(50.0, -38.0), at(51.0, -24.0), 2, hash2(seed, 5, 180));
    }

    /// The colony, along the site's forward.
    fn colony(&mut self, site: Site, seed: u32) {
        let fwd = site.dir(0.0);
        for (i, &(piece, [x, z], pad)) in COLONY.iter().enumerate() {
            let p = site.at(x, z);
            let s = hash2(seed, i as i32, 200);
            match piece {
                Piece::Dome { r, h, gores } => {
                    let g = self.foot(p, 0.6 * r);
                    let girdles: &[f32] = if r > 30.0 { &[0.3, 0.62] } else { &[0.35] };
                    let door = if r > 30.0 { None } else { Some(site.dir(90.0)) };
                    self.dome(Vec3::new(p.x, g, p.y), r, h, gores, girdles, door, s);
                }
                Piece::Greenhouse { r, h } => {
                    let g = self.foot(p, 0.6 * r);
                    self.greenhouse(Vec3::new(p.x, g, p.y), r, h, s);
                }
                Piece::Tanks { n, r, len } => {
                    for k in 0..n {
                        let q = p + flat(site.dir(90.0)) * ((k as f32 - 0.5 * (n - 1) as f32) * (2.0 * r + 1.6));
                        self.tank(q, fwd, r, len, hash2(s, k as i32, 1));
                    }
                }
                Piece::Containers { n } => {
                    for k in 0..n {
                        let (col, level) = (k / 2, k % 2);
                        let q = p + flat(site.dir(90.0)) * (2.7 * col as f32 - 3.0) + flat(fwd) * (0.3 * jit(s, k as i32));
                        let colour = if rnd(s, 10 + k as i32) < 0.55 { color::PAINT_ORANGE } else { color::PAINT_WHITE };
                        self.container(q, site.dir(3.0 * jit(s, 20 + k as i32)), level, colour, hash2(s, k as i32, 2));
                    }
                }
                Piece::Solar { rows, per_row } => {
                    for row in 0..rows {
                        for t in 0..per_row {
                            let q = p + flat(site.dir(90.0)) * ((t as f32 - 0.5 * (per_row - 1) as f32) * 6.6) + flat(fwd) * ((row as f32 - 0.5 * (rows - 1) as f32) * 5.0);
                            self.solar_table(q, site.dir(180.0), 6, hash2(s, row as i32, t as i32));
                        }
                    }
                }
                Piece::Tower { h } => self.tower(p, fwd, h, s),
                Piece::Rocket { pad: pr } => self.rocket(p, pr),
            }
            let _ = pad;
        }
        // Tunnels between the domes.
        self.tunnel(site.at(-3.0, 44.0), fwd, 14.0, 3.6, hash2(seed, 1, 210));
        self.tunnel(site.at(5.0, -44.0), site.dir(180.0), 14.0, 3.4, hash2(seed, 2, 210));
    }

    /// A glass greenhouse dome on the ground at `c`: faceted panes in a white frame, plants
    /// glowing green behind them.
    fn greenhouse(&mut self, c: Vec3, r: f32, h: f32, seed: u32) {
        let sides = 14u32;
        let rings = 5u32;
        let phase = TAU * rnd(seed, 1);
        let point = |j: u32, k: u32| -> Vec3 {
            if j == rings {
                return c + Vec3::Y * h;
            }
            let phi = FRAC_PI_2 * j as f32 / rings as f32;
            let theta = phase + TAU * (k as f32 + 0.5 * (j % 2) as f32) / sides as f32;
            let (sp, cp) = libm::sincosf(phi);
            c + ring_dir(theta) * (r * cp) + Vec3::Y * (h * sp)
        };
        let inside = c + Vec3::Y * (0.3 * h);
        let b = self.body();
        // The panes: flat faceted triangles.
        let mut edges: Vec<(Vec3, Vec3)> = Vec::new();
        for j in 0..rings {
            for k in 0..sides {
                let (a, bb, cc) = (point(j, k), point(j, k + 1), point(j + 1, k));
                let d = point(j + 1, k + 1);
                for tri in [[a, bb, cc], [bb, d, cc]] {
                    if tri[0].distance(tri[2]) < 1e-3 || tri[1].distance(tri[2]) < 1e-3 {
                        continue;
                    }
                    let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
                    let n = if n.dot((tri[0] + tri[1] + tri[2]) / 3.0 - inside) < 0.0 { -n } else { n };
                    let v = tri.map(|p| b.vertex_facing(p, n, color::GREENHOUSE, [0.0, 0.0]));
                    tri_out(b, v[0], v[1], v[2], inside, Surface::Wall);
                }
                edges.push((a, bb));
                edges.push((a, cc));
            }
        }
        // The frame along the panes' edges, a low wall of fabric round the foot.
        for (a, bb) in edges {
            if a.distance(bb) > 0.05 {
                add_tube(&mut self.decor, a, bb, 0.06 + 0.004 * r, 4, Surface::Wall, color::PAINT_WHITE);
            }
        }
        lathe(self.body(), c - Vec3::Y * 0.3, Vec3::Y, &[(0.0, r * 1.02, color::FABRIC), (1.2, r * 1.02, color::FABRIC), (1.2, 0.0, color::FABRIC)], sides * 2, Surface::Wall);
    }

    /// The colony's tower: a lattice of red tubes `h` tall on the ground at `p`, beacons at three
    /// heights, dishes, an antenna on top.
    fn tower(&mut self, p: Vec2, fwd: Vec3, h: f32, seed: u32) {
        let g = self.foot(p, 8.0);
        let c = Vec3::new(p.x, g, p.y);
        let tops = self.lattice(c, fwd, 14.0, 3.6, h, 22, 0.42, 0.2, 6);
        // Beacons at the corners of three levels, a platform and an antenna on top.
        let corner_at = |k: usize, t: f32| c + (tops[k] - c) * t;
        for t in [0.36f32, 0.68, 1.0] {
            for &corner in &tops {
                let q = c.lerp(corner, t);
                let q = Vec3::new(q.x, c.y + h * t, q.z);
                Self::beacon(&mut self.decor, q + Vec3::Y * 0.4, 0.6, color::BEACON_RED);
            }
        }
        let top = (tops[0] + tops[1] + tops[2] + tops[3]) / 4.0;
        add_box(&mut self.decor, top + Vec3::Y * 0.2, Vec3::new(2.2, 0.2, 2.2), fwd, Surface::Wall, color::PAINT_GREY, false);
        lathe(&mut self.decor, top, Vec3::Y, &[(0.0, 0.35, color::PAINT_WHITE), (18.0, 0.12, color::PAINT_WHITE), (18.0, 0.0, color::PAINT_WHITE)], 6, Surface::Wall);
        Self::beacon(&mut self.decor, top + Vec3::Y * 18.0, 0.7, color::BEACON_RED);
        for (k, t) in [(0usize, 0.55f32), (2, 0.6), (1, 0.82)] {
            let q = corner_at(k, t);
            let q = Vec3::new(q.x, c.y + h * t, q.z);
            let out = Vec3::new(q.x - c.x, 0.0, q.z - c.z).normalize_or(fwd);
            Self::dish(&mut self.decor, q + out * 1.0, (out + Vec3::Y * 0.15).normalize(), 2.6);
        }
        let _ = seed;
    }

    /// The rocket standing on its landing pad at `p`: a pad of white tiles with orange marks and a
    /// ring of sandbags, scorched under the rocket; the rocket white with orange bands, sooty at
    /// its base, on four legs.
    fn rocket(&mut self, p: Vec2, pad: f32) {
        let g = self.foot(p, 0.8 * pad);
        let c = Vec3::new(p.x, g, p.y);
        let b = &mut self.decor;
        disc(b, c + Vec3::Y * 0.2, 0.0, pad, 40, color::FABRIC);
        disc(b, c + Vec3::Y * 0.22, 0.68 * pad, 0.74 * pad, 48, color::FLAG);
        disc(b, c + Vec3::Y * 0.23, 0.0, 0.42 * pad, 32, color::TYRE);
        // Orange ticks round the ring.
        for k in 0..8 {
            let d = ring_dir(TAU * k as f32 / 8.0);
            let s = Vec3::Y.cross(d);
            let q = [0.78, 0.92].map(|f| c + d * (f * pad) + Vec3::Y * 0.22);
            let quad = [q[0] - s * 0.6, q[1] - s * 0.6, q[1] + s * 0.6, q[0] + s * 0.6].map(|x| b.vertex_facing(x, Vec3::Y, color::FLAG, [0.0, 0.0]));
            tri_out(b, quad[0], quad[1], quad[2], c - Vec3::Y, Surface::Wall);
            tri_out(b, quad[0], quad[2], quad[3], c - Vec3::Y, Surface::Wall);
        }
        // The ring of sandbags round the pad (seen from afar: one low rounded wall).
        lathe(&mut self.decor, c - Vec3::Y * 0.2, Vec3::Y, &[(0.0, pad + 1.6, color::SANDBAG), (0.6, pad + 1.55, color::SANDBAG), (0.95, pad + 0.9, color::SANDBAG), (0.6, pad + 0.25, color::SANDBAG), (0.0, pad + 0.2, color::SANDBAG)], 48, Surface::Wall);
        // The rocket.
        let (h, r) = (48.0, 4.2);
        let base_y = 4.2;
        let b = &mut self.decor;
        let foot = c + Vec3::Y * base_y;
        let mut prof = vec![(0.0, 0.0, color::PAINT_BLACK), (0.0, r, color::PAINT_BLACK), (2.6, r, color::PAINT_BLACK), (2.6, r, color::PAINT_GREY), (5.5, r, color::PAINT_GREY), (5.5, r, color::PAINT_WHITE)];
        let bands = [(12.5, 14.6), (25.0, 26.2)];
        for (a, bb) in bands {
            prof.push((a, r, color::PAINT_WHITE));
            prof.push((a, r, color::PAINT_ORANGE));
            prof.push((bb, r, color::PAINT_ORANGE));
            prof.push((bb, r, color::PAINT_WHITE));
        }
        let body_top = 33.0;
        prof.push((body_top, r, color::PAINT_WHITE));
        let nose = h - base_y - body_top;
        for i in 1..=8 {
            let q = i as f32 / 8.0;
            let rr = r * libm::sqrtf((1.0 - q * q).max(0.0)) * (1.0 - 0.15 * q);
            let colour = if q > 0.86 { color::PAINT_ORANGE } else { color::PAINT_WHITE };
            if q > 0.86 && q - 0.125 <= 0.86 {
                let prev_r = r * libm::sqrtf((1.0 - (q - 0.0625) * (q - 0.0625)).max(0.0)) * (1.0 - 0.15 * (q - 0.0625));
                prof.push((body_top + nose * (q - 0.0625), prev_r, color::PAINT_WHITE));
                prof.push((body_top + nose * (q - 0.0625), prev_r, color::PAINT_ORANGE));
            }
            prof.push((body_top + nose * q, rr.max(0.0), colour));
        }
        lathe(b, foot, Vec3::Y, &prof, 24, Surface::Wall);
        // Engines under it.
        for k in 0..3 {
            let d = ring_dir(TAU * k as f32 / 3.0 + 0.5);
            let e = foot + d * (0.45 * r);
            lathe(b, e, -Vec3::Y, &[(0.0, 0.35, color::STEEL), (0.6, 0.45, color::STEEL), (2.6, 1.05, color::STEEL), (2.6, 0.95, color::PAINT_BLACK), (0.8, 0.0, color::PAINT_BLACK)], 12, Surface::Wall);
        }
        // Four legs: a strut from the body to a foot pad, a brace.
        for k in 0..4 {
            let d = ring_dir(TAU * (k as f32 + 0.5) / 4.0);
            let pad_at = c + d * (2.1 * r) + Vec3::Y * 0.25;
            let hip = foot + d * (0.95 * r) + Vec3::Y * 7.5;
            let knee = foot + d * (0.95 * r) + Vec3::Y * 0.6;
            add_tube(b, hip, pad_at + Vec3::Y * 0.4, 0.42, 8, Surface::Wall, color::PAINT_GREY);
            add_tube(b, knee, pad_at + Vec3::Y * 0.4, 0.26, 8, Surface::Wall, color::PAINT_GREY);
            lathe(b, pad_at - Vec3::Y * 0.1, Vec3::Y, &[(0.0, 1.3, color::PAINT_GREY), (0.35, 1.2, color::PAINT_GREY), (0.5, 0.0, color::PAINT_GREY)], 12, Surface::Wall);
        }
        // Soot streaks up the lower body: dark fins between the legs.
        for k in 0..4 {
            let d = ring_dir(TAU * k as f32 / 4.0);
            let s = Vec3::Y.cross(d);
            let a = foot + d * (r + 0.02);
            let q = [a - s * 0.8, a + s * 0.8, a + s * 0.3 + Vec3::Y * 9.0, a - s * 0.4 + Vec3::Y * 7.0].map(|x| b.vertex_facing(x, d, color::PAINT_BLACK, [0.0, 0.0]));
            tri_out(b, q[0], q[1], q[2], foot, Surface::Wall);
            tri_out(b, q[0], q[2], q[3], foot, Surface::Wall);
        }
    }
}

/// A strap lying along a surface: `pts` (point, surface normal) along it, `width` across it in
/// `side` (horizontal), wound to face away from `inside`. The webbing runs along it.
fn ribbon(b: &mut MeshBuilder, pts: &[(Vec3, Vec3)], side: Vec3, width: f32, inside: Vec3) {
    let mut along = 0.0;
    let mut prev: Option<(u32, u32, Vec3)> = None;
    for &(p, n) in pts {
        if let Some((_, _, q)) = prev {
            along += p.distance(q);
        }
        let s = (side - n * side.dot(n)).normalize_or(side) * (0.5 * width);
        let l = b.vertex_facing(p + s, n, color::STRAP, [along, 1.0]);
        let r = b.vertex_facing(p - s, n, color::STRAP, [along, 1.0]);
        if let Some((pl, pr, _)) = prev {
            tri_out(b, pl, pr, r, inside, Surface::Wall);
            tri_out(b, pl, r, l, inside, Surface::Wall);
        }
        prev = Some((l, r, p));
    }
}

/// A strap round a body: `pts` (point, outward normal) round it, `width` along `across(n)`.
fn ribbon_across(b: &mut MeshBuilder, pts: &[(Vec3, Vec3)], across: &dyn Fn(Vec3) -> Vec3, width: f32, inside: Vec3) {
    let mut along = 0.0;
    let mut prev: Option<(u32, u32, Vec3)> = None;
    for &(p, n) in pts {
        if let Some((_, _, q)) = prev {
            along += p.distance(q);
        }
        let a = across(n);
        let s = (a - n * a.dot(n)).normalize_or(a) * (0.5 * width);
        let l = b.vertex_facing(p + s, n, color::STRAP, [along, 1.0]);
        let r = b.vertex_facing(p - s, n, color::STRAP, [along, 1.0]);
        if let Some((pl, pr, _)) = prev {
            tri_out(b, pl, pr, r, inside, Surface::Wall);
            tri_out(b, pl, r, l, inside, Surface::Wall);
        }
        prev = Some((l, r, p));
    }
}

/// A fabric flag: from `a` to `b` along its pole, out to `tip`, drawn on both faces.
fn flag(b: &mut MeshBuilder, a: Vec3, c: Vec3, tip: Vec3, wind: Vec3) {
    let n = (c - a).cross(tip - a).normalize_or(Vec3::Y.cross(wind));
    for side in [1.0f32, -1.0] {
        let off = n * (0.004 * side);
        let v = [a + off, c + off, tip + off].map(|p| b.vertex_facing(p, n * side, color::FLAG, [0.0, 0.0]));
        tri_out(b, v[0], v[1], v[2], (a + c + tip) / 3.0 - n * side, Surface::Wall);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle of every structure on Jezero, solid and drawn only: finite, not degenerate,
    /// its vertex normals on its visible side.
    #[test]
    fn camps_are_valid() {
        let map = crate::builtin_maps().into_iter().find(|m| m.name == "Jezero").expect("Jezero");
        assert!(!map.structures.is_empty(), "Jezero has structures");
        let built = map.build_detailed().expect("build");
        let (solid, decor) = build(&map.structures, &built.terrain);
        for (name, m) in [("solid", &solid), ("decor", &decor)] {
            for (i, (p, n)) in m.positions.iter().zip(&m.normals).enumerate() {
                assert!(p.is_finite() && n.is_finite(), "{name} vertex {i}");
                assert!((n.length() - 1.0).abs() < 1e-3, "{name} normal {i} has length {}", n.length());
            }
            for i in 0..m.triangle_count() {
                // The sandbags are the roads' own (mesh.rs), their ends gathered to a seam.
                if m.colors[m.indices[3 * i] as usize] == color::SANDBAG {
                    continue;
                }
                let [a, b, c] = m.triangle(i);
                let cross = (b - a).cross(c - a);
                let longest = (b - a).length().max((c - b).length()).max((a - c).length());
                assert!(cross.length() > 1e-3 * longest, "{name} triangle {i} is degenerate: {a} {b} {c}");
                // What collides as strictly as the track (tests/demo.rs); what only draws may bend
                // its normals (sandbags tuck theirs under) but never shows its back.
                let face = cross.normalize();
                let least = if name == "solid" { 0.5 } else { 0.0 };
                for k in 0..3 {
                    let n = m.normals[m.indices[3 * i + k] as usize];
                    assert!(n.dot(face) > least, "{name} triangle {i} at {a} ({:?}) winding disagrees with its normals", m.colors[m.indices[3 * i] as usize]);
                }
            }
        }
    }
}
