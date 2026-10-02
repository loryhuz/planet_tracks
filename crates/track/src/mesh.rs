//! Mesh assembly helpers.

use glam::Vec3;

use crate::{Surface, TrackMesh};

/// Accumulates triangles. A vertex normal is the normalised sum of the unit normals of the
/// triangles that use it: smooth along a strip whose vertices are shared, flat on a quad that
/// owns its four vertices. A vertex can also be given its normal ([`MeshBuilder::vertex_facing`]).
#[derive(Default)]
pub(crate) struct MeshBuilder {
    mesh: TrackMesh,
    acc: Vec<Vec3>,
    /// The vertices whose normal was given, which the triangles leave alone.
    given: Vec<bool>,
}

impl MeshBuilder {
    pub fn vertex(&mut self, p: Vec3, color: [f32; 3]) -> u32 {
        self.vertex_on(p, color, [0.0, 0.0], 0.0)
    }

    /// A vertex with track coordinates and a dirt amount (see [`TrackMesh`]).
    pub fn vertex_on(&mut self, p: Vec3, color: [f32; 3], uv: [f32; 2], dirt: f32) -> u32 {
        let i = self.mesh.positions.len() as u32;
        self.mesh.positions.push(p);
        self.mesh.normals.push(Vec3::Y);
        self.mesh.colors.push(color);
        self.mesh.uv.push(uv);
        self.mesh.dirt.push(dirt);
        self.acc.push(Vec3::ZERO);
        self.given.push(false);
        i
    }

    /// A vertex with its own normal `n` (unit length), and `uv` as in [`MeshBuilder::vertex_on`].
    pub fn vertex_facing(&mut self, p: Vec3, n: Vec3, color: [f32; 3], uv: [f32; 2]) -> u32 {
        let i = self.vertex_on(p, color, uv, 0.0);
        self.acc[i as usize] = n;
        self.given[i as usize] = true;
        i
    }

    /// The position of vertex `i`.
    pub fn position(&self, i: u32) -> Vec3 {
        self.mesh.positions[i as usize]
    }

    /// A triangle wound so that `(b - a) × (c - a)` points to its visible side.
    pub fn tri(&mut self, a: u32, b: u32, c: u32, surface: Surface) {
        let [pa, pb, pc] = [a, b, c].map(|i| self.mesh.positions[i as usize]);
        let n = (pb - pa).cross(pc - pa).normalize_or_zero();
        for i in [a, b, c] {
            if !self.given[i as usize] {
                self.acc[i as usize] += n;
            }
        }
        self.mesh.indices.extend_from_slice(&[a, b, c]);
        self.mesh.tri_surface.push(surface);
    }

    /// A flat triangle with its own vertices.
    pub fn flat_tri(&mut self, p: [Vec3; 3], surface: Surface, color: [f32; 3]) {
        let v = p.map(|q| self.vertex(q, color));
        self.tri(v[0], v[1], v[2], surface);
    }

    /// A flat quad with its own vertices, `p[0..4]` counter-clockwise seen from the visible side.
    pub fn quad(&mut self, p: [Vec3; 4], surface: Surface, color: [f32; 3]) {
        let v = p.map(|q| self.vertex(q, color));
        self.tri(v[0], v[1], v[2], surface);
        self.tri(v[0], v[2], v[3], surface);
    }

    pub fn finish(mut self) -> TrackMesh {
        for (n, a) in self.mesh.normals.iter_mut().zip(&self.acc) {
            let l = a.length();
            if l > 1e-6 {
                *n = *a / l;
            }
        }
        self.mesh
    }
}

/// An oriented box: `half.x` across (to the left of `forward`), `half.y` up, `half.z` along
/// `forward` (horizontal, unit length). Every face flat-shaded and facing out.
pub(crate) fn add_box(
    b: &mut MeshBuilder,
    centre: Vec3,
    half: Vec3,
    forward: Vec3,
    surface: Surface,
    color: [f32; 3],
    with_bottom: bool,
) {
    let ez = forward;
    let ey = Vec3::Y;
    let ex = ey.cross(ez);
    // (normal, a, b, half along normal, half along a, half along b) with a × b = normal.
    let faces = [
        (ex, ey, ez, half.x, half.y, half.z),
        (-ex, ez, ey, half.x, half.z, half.y),
        (ey, ez, ex, half.y, half.z, half.x),
        (-ey, ex, ez, half.y, half.x, half.z),
        (ez, ex, ey, half.z, half.x, half.y),
        (-ez, ey, ex, half.z, half.y, half.x),
    ];
    for (i, (n, a, bb, hn, ha, hb)) in faces.into_iter().enumerate() {
        if i == 3 && !with_bottom {
            continue;
        }
        let c = centre + n * hn;
        b.quad(
            [c - a * ha - bb * hb, c + a * ha - bb * hb, c + a * ha + bb * hb, c - a * ha + bb * hb],
            surface,
            color,
        );
    }
}

/// A tube of radius `r` from `a` to `c`, `sides` faces around it, shaded round (radial normals),
/// its ends open (they meet other tubes or the ground). Its vertices' `uv` are the distance from
/// `a` and the tube's length, metres.
pub(crate) fn add_tube(b: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32, sides: u32, surface: Surface, color: [f32; 3]) {
    let axis = (c - a).normalize_or_zero();
    if axis == Vec3::ZERO {
        return;
    }
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = axis.cross(helper).normalize();
    let e2 = axis.cross(e1);
    let length = a.distance(c);
    let ring = |centre: Vec3, along: f32, b: &mut MeshBuilder| -> Vec<u32> {
        (0..sides)
            .map(|k| {
                let (s, co) = libm::sincosf(core::f32::consts::TAU * k as f32 / sides as f32);
                let out = e1 * co + e2 * s;
                b.vertex_facing(centre + out * r, out, color, [along, length])
            })
            .collect()
    };
    let (ra, rc) = (ring(a, 0.0, b), ring(c, length, b));
    for k in 0..sides as usize {
        let n = (k + 1) % sides as usize;
        b.tri(ra[k], ra[n], rc[k], surface);
        b.tri(ra[n], rc[n], rc[k], surface);
    }
}

/// A filled sandbag lying on the ground at `foot` (the middle of its underside): a fat pillow
/// `size.x` long along `forward`, `size.z` wide and `size.y` high over the ground it lies on,
/// whose normal is about `up`. Its section bulges widest a third of the way up and tucks under
/// where it meets the ground; it is full to its ends, which round down to the ground; its cloth
/// is lumpy (`seed`) and shaded smooth from its own shape. Its vertices carry `tint` as their uv
/// (the renderer's per-bag brightness and warmth). `fine` for bags seen up close (along the
/// roads), coarser otherwise (the stacks under the stilts). No underside.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_sandbag(b: &mut MeshBuilder, foot: Vec3, forward: Vec3, up: Vec3, size: Vec3, seed: u32, fine: bool, surface: Surface, color: [f32; 3], tint: [f32; 2]) {
    // Along the bag (−1..1), and its section, across (−1..1) and up (0..1), from one foot over
    // the top to the other.
    const ALONG_FINE: [f32; 7] = [-1.0, -0.88, -0.6, 0.0, 0.6, 0.88, 1.0];
    const SECTION_FINE: [[f32; 2]; 9] = [[0.84, 0.0], [1.0, 0.3], [0.88, 0.66], [0.52, 0.92], [0.0, 1.0], [-0.52, 0.92], [-0.88, 0.66], [-1.0, 0.3], [-0.84, 0.0]];
    const ALONG_COARSE: [f32; 5] = [-1.0, -0.8, 0.0, 0.8, 1.0];
    const SECTION_COARSE: [[f32; 2]; 7] = [[0.84, 0.0], [1.0, 0.35], [0.7, 0.85], [0.0, 1.0], [-0.7, 0.85], [-1.0, 0.35], [-0.84, 0.0]];
    let (along_t, section): (&[f32], &[[f32; 2]]) = if fine { (&ALONG_FINE, &SECTION_FINE) } else { (&ALONG_COARSE, &SECTION_COARSE) };
    let forward = forward.normalize_or(Vec3::Z);
    let side = up.cross(forward).normalize_or(Vec3::X);
    let up = forward.cross(side);
    let (a, w, h) = (0.5 * size.x, 0.5 * size.z, size.y);
    let lump = |i: usize, j: usize| (crate::noise::unit(crate::noise::hash2(seed, i as i32, j as i32)) - 0.5) * 0.12;
    let (ni, nj) = (along_t.len(), section.len());
    // Positions first, then normals from the shape itself, then the vertices.
    let mut pos = vec![Vec3::ZERO; ni * nj];
    for (i, &t) in along_t.iter().enumerate() {
        // Full along its length; at the ends it narrows and its height falls to the ground
        // (the cloth gathered at the seams).
        let e = libm::powf(libm::fabsf(t), 4.0);
        let (rw, rh) = (1.0 - 0.3 * e, libm::powf((1.0 - e).max(0.0), 0.45));
        for (j, q) in section.iter().enumerate() {
            let bump = if q[1] > 0.0 { lump(i, j) * rh } else { 0.0 };
            pos[i * nj + j] = foot + forward * (t * a) + side * (q[0] * w * rw * (1.0 + 0.5 * bump)) + up * ((q[1] + bump) * h * rh);
        }
    }
    let centre = foot + up * (0.4 * h);
    let at = |i: usize, j: usize| pos[i.min(ni - 1) * nj + j.min(nj - 1)];
    let ids: Vec<u32> = (0..ni * nj)
        .map(|k| {
            let (i, j) = (k / nj, k % nj);
            let around = at(i, j + 1) - at(i, j.saturating_sub(1));
            let along = at(i + 1, j) - at(i.saturating_sub(1), j);
            let mut n = around.cross(along).normalize_or(up);
            if n.dot(pos[k] - centre) < 0.0 {
                n = -n;
            }
            // Where the cloth meets the ground, it turns under.
            if section[j][1] == 0.0 {
                n = (n - up * 0.35).normalize_or(n);
            }
            b.vertex_facing(pos[k], n, color, tint)
        })
        .collect();
    let tri = |b: &mut MeshBuilder, i: u32, j: u32, k: u32| {
        // Wound to face away from the bag's middle.
        let [p, q, r] = [i, j, k].map(|v| b.position(v));
        let n = (q - p).cross(r - p);
        if n.length_squared() < 1e-12 {
            return;
        }
        if n.dot((p + q + r) / 3.0 - centre) >= 0.0 {
            b.tri(i, j, k, surface);
        } else {
            b.tri(i, k, j, surface);
        }
    };
    for i in 0..ni - 1 {
        for j in 0..nj - 1 {
            let (p, q) = (i * nj + j, (i + 1) * nj + j);
            tri(b, ids[p], ids[p + 1], ids[q]);
            tri(b, ids[p + 1], ids[q + 1], ids[q]);
        }
    }
}

/// A closed cylinder of radius `r` from `a` to `c`, `sides` faces around it (6: a hexagonal bar),
/// flat-shaded around when `sides` is small, its end at `c` closed by a flat lid (its end at `a`
/// stays open: it stands in the ground or on something). `turn` rotates it about its axis.
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_post(b: &mut MeshBuilder, a: Vec3, c: Vec3, r: f32, sides: u32, turn: f32, surface: Surface, color: [f32; 3]) {
    let axis = (c - a).normalize_or_zero();
    if axis == Vec3::ZERO {
        return;
    }
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = axis.cross(helper).normalize();
    let e2 = axis.cross(e1);
    let corner = |k: u32| {
        let (s, co) = libm::sincosf(turn + core::f32::consts::TAU * k as f32 / sides as f32);
        e1 * co + e2 * s
    };
    // Each face flat: its own four vertices.
    for k in 0..sides {
        let (p, q) = (corner(k), corner(k + 1));
        let n = (p + q).normalize();
        let v = [a + p * r, a + q * r, c + q * r, c + p * r].map(|x| b.vertex_facing(x, n, color, [0.0, 0.0]));
        b.tri(v[0], v[1], v[2], surface);
        b.tri(v[0], v[2], v[3], surface);
    }
    let centre = b.vertex_facing(c, axis, color, [0.0, 0.0]);
    let rim: Vec<u32> = (0..sides).map(|k| b.vertex_facing(c + corner(k) * r, axis, color, [0.0, 0.0])).collect();
    for k in 0..sides as usize {
        b.tri(centre, rim[k], rim[(k + 1) % sides as usize], surface);
    }
}
