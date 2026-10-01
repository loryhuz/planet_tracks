//! Placeholder buggy built from code: a bevelled tub, a roll cage, and wheels with rounded
//! tyres, tread ridges and spoked rims. The real model will come from Blender.

use glam::{Quat, Vec2, Vec3};
use physics::CarParams;

use crate::gfx::{MeshData, Vertex, kind, srgb};

pub const TYRE_WIDTH: f32 = 0.36;

struct Builder {
    mesh: MeshData,
}

impl Builder {
    fn new() -> Self {
        Self { mesh: MeshData::default() }
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, color: [f32; 3], kind: u32) -> u32 {
        self.mesh.vertices.push(Vertex { pos: p.to_array(), normal: n.to_array(), color, kind });
        self.mesh.vertices.len() as u32 - 1
    }

    /// Flat-shaded polygon (convex, planar), oriented to face away from `inside`.
    fn face(&mut self, pts: &[Vec3], inside: Vec3, color: [f32; 3], kind: u32) {
        let mut n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        if n.length_squared() < 1e-12 {
            return;
        }
        n = n.normalize();
        let centroid = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        let flip = n.dot(centroid - inside) < 0.0;
        if flip {
            n = -n;
        }
        let base: Vec<u32> = pts.iter().map(|&p| self.vertex(p, n, color, kind)).collect();
        for i in 1..pts.len() - 1 {
            let (a, b, c) = (base[0], base[i], base[i + 1]);
            if flip {
                self.mesh.indices.extend_from_slice(&[a, c, b]);
            } else {
                self.mesh.indices.extend_from_slice(&[a, b, c]);
            }
        }
    }

    /// Box between two corners, bevel-free.
    fn cuboid(&mut self, min: Vec3, max: Vec3, color: [f32; 3], kind: u32) {
        let c = (min + max) * 0.5;
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        let (a, b) = (min, max);
        let faces = [
            [p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(b.x, b.y, a.z), p(a.x, b.y, a.z)],
            [p(a.x, a.y, b.z), p(b.x, a.y, b.z), p(b.x, b.y, b.z), p(a.x, b.y, b.z)],
            [p(a.x, a.y, a.z), p(a.x, b.y, a.z), p(a.x, b.y, b.z), p(a.x, a.y, b.z)],
            [p(b.x, a.y, a.z), p(b.x, b.y, a.z), p(b.x, b.y, b.z), p(b.x, a.y, b.z)],
            [p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(b.x, a.y, b.z), p(a.x, a.y, b.z)],
            [p(a.x, b.y, a.z), p(b.x, b.y, a.z), p(b.x, b.y, b.z), p(a.x, b.y, b.z)],
        ];
        for f in faces {
            self.face(&f, c, color, kind);
        }
    }

    /// Smooth tube between two points.
    fn tube(&mut self, a: Vec3, b: Vec3, radius: f32, color: [f32; 3], kind: u32) {
        let axis = (b - a).normalize();
        let side = if axis.y.abs() < 0.9 { axis.cross(Vec3::Y).normalize() } else { axis.cross(Vec3::X).normalize() };
        let up = side.cross(axis);
        let n = 10;
        let start = self.mesh.vertices.len() as u32;
        for i in 0..=n {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let d = side * t.cos() + up * t.sin();
            self.vertex(a + d * radius, d, color, kind);
            self.vertex(b + d * radius, d, color, kind);
        }
        for i in 0..n as u32 {
            let (a0, b0, a1, b1) = (start + 2 * i, start + 2 * i + 1, start + 2 * i + 2, start + 2 * i + 3);
            // (b0 - a0) x (a1 - a0) = axis x tangent, which points outward.
            self.mesh.indices.extend_from_slice(&[a0, b0, a1, b0, b1, a1]);
        }
        // End caps.
        for (end, other) in [(a, b), (b, a)] {
            let ring: Vec<Vec3> = (0..n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    end + (side * t.cos() + up * t.sin()) * radius
                })
                .collect();
            self.face(&ring, other, color, kind);
        }
    }

    /// Prism from a convex side profile (z, y) extruded across x, with chamfered side edges.
    fn bevelled_prism(&mut self, profile: &[Vec2], half_width: f32, bevel: f32, color: [f32; 3], kind: u32) {
        let inset = inset_convex(profile, bevel);
        let inner_x = half_width - bevel;
        let centroid2 = profile.iter().copied().sum::<Vec2>() / profile.len() as f32;
        let inside = Vec3::new(0.0, centroid2.y, centroid2.x);
        let at = |p: Vec2, x: f32| Vec3::new(x, p.y, p.x);
        let n = profile.len();
        // Caps.
        for &x in &[half_width, -half_width] {
            let pts: Vec<Vec3> = inset.iter().map(|&p| at(p, x)).collect();
            self.face(&pts, inside, color, kind);
        }
        for i in 0..n {
            let j = (i + 1) % n;
            // Band around the profile.
            self.face(&[at(profile[i], inner_x), at(profile[j], inner_x), at(profile[j], -inner_x), at(profile[i], -inner_x)], inside, color, kind);
            // Chamfers.
            for &s in &[1.0f32, -1.0] {
                self.face(
                    &[at(profile[i], s * inner_x), at(profile[j], s * inner_x), at(inset[j], s * half_width), at(inset[i], s * half_width)],
                    inside,
                    color,
                    kind,
                );
            }
        }
    }
}

/// Offsets a convex CCW polygon inward by `d` (miter joins).
fn inset_convex(poly: &[Vec2], d: f32) -> Vec<Vec2> {
    let n = poly.len();
    let normal = |i: usize| {
        let e = (poly[(i + 1) % n] - poly[i]).normalize();
        Vec2::new(-e.y, e.x)
    };
    (0..n)
        .map(|i| {
            let n0 = normal((i + n - 1) % n);
            let n1 = normal(i);
            poly[i] + (n0 + n1) * d / (1.0 + n0.dot(n1))
        })
        .collect()
}

/// Chassis centre to wheel-centre height at rest, used to place the body.
fn rest_wheel_center_y(p: &CarParams) -> f32 {
    p.wheel_anchors()[0].y - p.rest_suspension()
}

/// The body, in the car frame (+Z forward, +Y up, +X left), origin at the chassis centre.
pub fn body(p: &CarParams) -> MeshData {
    let mut b = Builder::new();
    let paint = srgb(236, 232, 224);
    let accent = srgb(232, 96, 36);
    let dark = srgb(40, 42, 46);
    let metal = srgb(70, 74, 80);

    let wheel_y = rest_wheel_center_y(p);
    let ground = wheel_y - p.wheel_radius;
    let y0 = ground + 0.32;
    let half_len = p.wheelbase * 0.5 + 0.55;
    let half_w = (p.track_width * 0.5 - TYRE_WIDTH * 0.5 - 0.06).max(0.45);
    let (zf, zr) = (half_len, -half_len);

    // Main tub: low nose, raised cockpit sides, engine deck at the back. Convex, CCW in (z, y).
    let tub = [
        Vec2::new(zr + 0.15, y0),
        Vec2::new(zf - 0.45, y0),
        Vec2::new(zf, y0 + 0.22),
        Vec2::new(zf - 0.25, y0 + 0.42),
        Vec2::new(zf - 1.25, y0 + 0.62),
        Vec2::new(zr + 0.35, y0 + 0.70),
        Vec2::new(zr, y0 + 0.40),
    ];
    b.bevelled_prism(&tub, half_w, 0.08, paint, kind::PAINT);

    // Orange side stripe panels.
    let stripe = [
        Vec2::new(zr + 0.30, y0 + 0.30),
        Vec2::new(zf - 0.50, y0 + 0.30),
        Vec2::new(zf - 0.60, y0 + 0.44),
        Vec2::new(zr + 0.40, y0 + 0.50),
    ];
    for &s in &[1.0f32, -1.0] {
        let x = s * (half_w + 0.004);
        let pts: Vec<Vec3> = stripe.iter().map(|q| Vec3::new(x, q.y, q.x)).collect();
        b.face(&pts, Vec3::new(0.0, y0 + 0.4, 0.0), accent, kind::PAINT);
    }

    // Cockpit hole cover and seat (dark), then the roll cage.
    let cz = zr + 0.9;
    b.cuboid(Vec3::new(-0.32, y0 + 0.62, cz - 0.25), Vec3::new(0.32, y0 + 0.66, cz + 0.85), dark, kind::RUBBER);
    b.cuboid(Vec3::new(-0.26, y0 + 0.64, cz - 0.15), Vec3::new(0.26, y0 + 1.25, cz + 0.05), dark, kind::RUBBER);

    let top = y0 + 1.45;
    let cage_w = half_w - 0.08;
    let front_z = cz + 1.15;
    let rear_z = cz - 0.35;
    let r = 0.035;
    let pts = [
        (Vec3::new(cage_w, y0 + 0.6, rear_z), Vec3::new(cage_w * 0.85, top, rear_z + 0.1)),
        (Vec3::new(-cage_w, y0 + 0.6, rear_z), Vec3::new(-cage_w * 0.85, top, rear_z + 0.1)),
        (Vec3::new(cage_w, y0 + 0.55, front_z), Vec3::new(cage_w * 0.8, top - 0.05, cz + 0.55)),
        (Vec3::new(-cage_w, y0 + 0.55, front_z), Vec3::new(-cage_w * 0.8, top - 0.05, cz + 0.55)),
        (Vec3::new(cage_w * 0.85, top, rear_z + 0.1), Vec3::new(-cage_w * 0.85, top, rear_z + 0.1)),
        (Vec3::new(cage_w * 0.8, top - 0.05, cz + 0.55), Vec3::new(-cage_w * 0.8, top - 0.05, cz + 0.55)),
        (Vec3::new(cage_w * 0.85, top, rear_z + 0.1), Vec3::new(cage_w * 0.8, top - 0.05, cz + 0.55)),
        (Vec3::new(-cage_w * 0.85, top, rear_z + 0.1), Vec3::new(-cage_w * 0.8, top - 0.05, cz + 0.55)),
        (Vec3::new(cage_w * 0.85, top, rear_z + 0.1), Vec3::new(cage_w * 0.7, y0 + 0.72, zr + 0.2)),
        (Vec3::new(-cage_w * 0.85, top, rear_z + 0.1), Vec3::new(-cage_w * 0.7, y0 + 0.72, zr + 0.2)),
    ];
    for (a, c) in pts {
        b.tube(a, c, r, metal, kind::METAL);
    }

    // Front bumper bar and rear engine block.
    b.tube(Vec3::new(half_w * 0.9, y0 + 0.18, zf + 0.12), Vec3::new(-half_w * 0.9, y0 + 0.18, zf + 0.12), 0.045, metal, kind::METAL);
    b.cuboid(Vec3::new(-0.45, y0 + 0.55, zr + 0.05), Vec3::new(0.45, y0 + 0.9, zr + 0.75), dark, kind::METAL);

    b.mesh
}

/// Where a wheel's suspension arm leaves the body, in the car frame.
pub fn arm_root(p: &CarParams, anchor: Vec3) -> Vec3 {
    let half_w = (p.track_width * 0.5 - TYRE_WIDTH * 0.5 - 0.06).max(0.45);
    let y0 = rest_wheel_center_y(p) - p.wheel_radius + 0.32;
    Vec3::new(anchor.x.signum() * (half_w - 0.05), y0 + 0.2, anchor.z)
}

/// A unit tube along +X (from x = 0 to x = 1), stretched at draw time into a suspension arm.
pub fn arm() -> MeshData {
    let mut b = Builder::new();
    b.tube(Vec3::ZERO, Vec3::X, 0.04, srgb(70, 74, 80), kind::METAL);
    b.mesh
}

/// One wheel in its own frame: axle along X, centre at the origin, rim facing +X.
pub fn wheel(radius: f32) -> MeshData {
    let mut b = Builder::new();
    let rubber = srgb(34, 32, 30);
    let rim_col = srgb(196, 200, 206);
    let barrel = srgb(80, 82, 86);
    let w = TYRE_WIDTH;
    let rim_r = radius * 0.62;
    let shoulder = 0.07;
    let segments = 48;

    // Tyre cross-section (x, r), from the inner bead round the tread to the outer bead.
    let mut profile: Vec<(f32, f32, Vec2)> = Vec::new(); // (x, r, normal in (x, r))
    profile.push((-w * 0.5, rim_r, Vec2::new(-1.0, 0.0)));
    let arc = 5;
    for i in 0..=arc {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / arc as f32;
        let c = Vec2::new(-w * 0.5 + shoulder, radius - shoulder);
        let d = Vec2::new(-a.cos(), a.sin());
        profile.push((c.x + d.x * shoulder, c.y + d.y * shoulder, d));
    }
    for i in 0..=arc {
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / arc as f32;
        let c = Vec2::new(w * 0.5 - shoulder, radius - shoulder);
        let d = Vec2::new(a.sin(), a.cos());
        profile.push((c.x + d.x * shoulder, c.y + d.y * shoulder, d));
    }
    profile.push((w * 0.5, rim_r, Vec2::new(1.0, 0.0)));

    let rows = profile.len() as u32;
    let start = b.mesh.vertices.len() as u32;
    for s in 0..=segments {
        let t = s as f32 / segments as f32 * std::f32::consts::TAU;
        let (sin, cos) = t.sin_cos();
        // Tread ridges: every other segment of the running surface sits a little lower.
        let groove = if (s / 2) % 2 == 0 { 0.0 } else { 0.012 };
        for (i, &(x, r, n)) in profile.iter().enumerate() {
            let tread = i > 2 && i < rows as usize - 3;
            let rr = if tread { r - groove } else { r };
            let radial = Vec3::new(0.0, sin, cos);
            let p = Vec3::new(x, 0.0, 0.0) + radial * rr;
            let normal = (Vec3::X * n.x + radial * n.y).normalize();
            b.vertex(p, normal, rubber, kind::RUBBER);
        }
    }
    for s in 0..segments as u32 {
        for i in 0..rows - 1 {
            let a = start + s * rows + i;
            let c = a + rows;
            // (a, a+1) runs along the profile, (a, c) around the axle.
            b.mesh.indices.extend_from_slice(&[a, a + 1, c, c, a + 1, c + 1]);
        }
    }

    // Rim barrel (inside of the tyre), rim face with spokes, and the hub.
    let n = 24;
    for i in 0..n {
        let t0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let t1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let d0 = Vec3::new(0.0, t0.sin(), t0.cos());
        let d1 = Vec3::new(0.0, t1.sin(), t1.cos());
        let x0 = -w * 0.5;
        let x1 = w * 0.5 - 0.05;
        b.face(
            &[Vec3::X * x0 + d0 * rim_r, Vec3::X * x1 + d0 * rim_r, Vec3::X * x1 + d1 * rim_r, Vec3::X * x0 + d1 * rim_r],
            Vec3::new(0.0, 0.0, 0.0) + (d0 + d1) * rim_r * 2.0,
            barrel,
            kind::METAL,
        );
        // Recessed dish behind the spokes.
        let xd = w * 0.5 - 0.12;
        b.face(&[Vec3::X * xd, Vec3::X * xd + d0 * rim_r, Vec3::X * xd + d1 * rim_r], Vec3::new(-1.0, 0.0, 0.0), barrel, kind::METAL);
        // Rim lip.
        let lip = [Vec3::X * x1 + d0 * rim_r, Vec3::X * x1 + d1 * rim_r, Vec3::X * x1 + d1 * (rim_r - 0.04), Vec3::X * x1 + d0 * (rim_r - 0.04)];
        b.face(&lip, Vec3::new(-1.0, 0.0, 0.0), rim_col, kind::METAL);
    }
    let spokes = 5;
    for k in 0..spokes {
        let t = k as f32 / spokes as f32 * std::f32::consts::TAU;
        let rot = Quat::from_rotation_x(t);
        let (x0, x1) = (w * 0.5 - 0.12, w * 0.5 - 0.06);
        let corners = [
            Vec3::new(x0, 0.08, -0.035),
            Vec3::new(x1, rim_r - 0.03, 0.035),
        ];
        let (lo, hi) = (corners[0], corners[1]);
        let mut sub = Builder::new();
        sub.cuboid(lo, hi, rim_col, kind::METAL);
        for v in &mut sub.mesh.vertices {
            v.pos = (rot * Vec3::from_array(v.pos)).to_array();
            v.normal = (rot * Vec3::from_array(v.normal)).to_array();
        }
        let base = b.mesh.vertices.len() as u32;
        b.mesh.vertices.extend(sub.mesh.vertices);
        b.mesh.indices.extend(sub.mesh.indices.iter().map(|i| i + base));
    }
    b.tube(Vec3::X * (w * 0.5 - 0.13), Vec3::X * (w * 0.5 - 0.02), 0.09, rim_col, kind::METAL);
    b.mesh
}
