//! Mesh assembly helpers.

use glam::Vec3;

use crate::{Surface, TrackMesh};

/// Accumulates triangles. A vertex normal is the normalised sum of the unit normals of the
/// triangles that use it: smooth along a strip whose vertices are shared, flat on a quad that
/// owns its four vertices.
#[derive(Default)]
pub(crate) struct MeshBuilder {
    mesh: TrackMesh,
    acc: Vec<Vec3>,
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
        i
    }

    /// A triangle wound so that `(b - a) × (c - a)` points to its visible side.
    pub fn tri(&mut self, a: u32, b: u32, c: u32, surface: Surface) {
        let [pa, pb, pc] = [a, b, c].map(|i| self.mesh.positions[i as usize]);
        let n = (pb - pa).cross(pc - pa).normalize_or_zero();
        for i in [a, b, c] {
            self.acc[i as usize] += n;
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
