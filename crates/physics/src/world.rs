//! Static collision world: the track's triangles in a bounding volume hierarchy.
//!
//! The tree is built deterministically (median splits on a total order of centroids, ties broken by
//! triangle index), and every query result is independent of the tree's shape: candidate lists are
//! sorted by triangle index and ray hits at equal distance keep the lowest index. The simulation
//! therefore gives bit-identical results whatever the tree looks like.

use glam::Vec3;
use track::{Surface, TrackMesh};

/// Where a ray hit the track.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub distance: f32,
    pub point: Vec3,
    /// Unit normal of the triangle, facing the ray's origin.
    pub normal: Vec3,
    pub surface: Surface,
}

/// One triangle, stored as its first vertex and two edges.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tri {
    pub a: Vec3,
    pub e1: Vec3,
    pub e2: Vec3,
    /// Unit normal from the winding (zero for a degenerate triangle).
    pub n: Vec3,
    pub surface: Surface,
}

impl Tri {
    fn bounds(&self) -> (Vec3, Vec3) {
        let b = self.a + self.e1;
        let c = self.a + self.e2;
        (self.a.min(b).min(c), self.a.max(b).max(c))
    }
}

#[derive(Clone, Copy, Debug)]
struct Node {
    min: Vec3,
    max: Vec3,
    /// Leaf: index of the first entry in `order`. Interior: index of the left child (right = +1).
    first: u32,
    /// Number of triangles for a leaf, 0 for an interior node.
    count: u32,
}

const LEAF_SIZE: usize = 4;
const STACK: usize = 64;

/// The static collision world built from a track mesh.
pub struct World {
    pub(crate) tris: Vec<Tri>,
    nodes: Vec<Node>,
    /// Triangle indices in leaf order.
    order: Vec<u32>,
}

/// Ray/triangle test (Möller–Trumbore, double-sided, with a tiny tolerance on the edges so that a
/// ray through a shared edge never falls between two triangles). Returns the distance.
#[inline]
pub(crate) fn ray_tri(t: &Tri, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<f32> {
    const EDGE: f32 = 1e-5;
    let p = dir.cross(t.e2);
    let det = t.e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - t.a;
    let u = s.dot(p) * inv;
    if !(-EDGE..=1.0 + EDGE).contains(&u) {
        return None;
    }
    let q = s.cross(t.e1);
    let v = dir.dot(q) * inv;
    if v < -EDGE || u + v > 1.0 + EDGE {
        return None;
    }
    let d = t.e2.dot(q) * inv;
    if d < 0.0 || d > max_distance {
        return None;
    }
    Some(d)
}

/// Closest point of triangle `t` to `p` (Ericson, Real-Time Collision Detection 5.1.5).
#[inline]
pub(crate) fn closest_point(t: &Tri, p: Vec3) -> Vec3 {
    let a = t.a;
    let ab = t.e1;
    let ac = t.e2;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let b = a + ab;
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let c = a + ac;
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

#[inline]
fn ray_box(min: Vec3, max: Vec3, origin: Vec3, inv_dir: Vec3, max_distance: f32) -> bool {
    let t1 = (min - origin) * inv_dir;
    let t2 = (max - origin) * inv_dir;
    let tmin = t1.min(t2);
    let tmax = t1.max(t2);
    let enter = tmin.x.max(tmin.y).max(tmin.z).max(0.0);
    let exit = tmax.x.min(tmax.y).min(tmax.z).min(max_distance);
    enter <= exit
}

#[inline]
fn boxes_overlap(amin: Vec3, amax: Vec3, bmin: Vec3, bmax: Vec3) -> bool {
    amin.x <= bmax.x && amax.x >= bmin.x && amin.y <= bmax.y && amax.y >= bmin.y && amin.z <= bmax.z && amax.z >= bmin.z
}

impl World {
    pub fn new(mesh: &TrackMesh) -> Self {
        let count = mesh.triangle_count();
        let mut tris = Vec::with_capacity(count);
        for i in 0..count {
            let [a, b, c] = mesh.triangle(i);
            let e1 = b - a;
            let e2 = c - a;
            let cr = e1.cross(e2);
            let len = cr.length();
            let n = if len > 1e-12 { cr / len } else { Vec3::ZERO };
            let surface = mesh.tri_surface.get(i).copied().unwrap_or(Surface::Ground);
            tris.push(Tri { a, e1, e2, n, surface });
        }
        let mut world = World { tris, nodes: Vec::new(), order: (0..count as u32).collect() };
        world.build();
        world
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    fn build(&mut self) {
        let n = self.tris.len();
        if n == 0 {
            return;
        }
        let centroids: Vec<Vec3> = self.tris.iter().map(|t| t.a + (t.e1 + t.e2) * (1.0 / 3.0)).collect();
        self.nodes.reserve(2 * n / LEAF_SIZE + 2);
        self.nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
        // Explicit work list instead of recursion: (node, start, end).
        let mut work: Vec<(usize, usize, usize)> = vec![(0, 0, n)];
        while let Some((node, start, end)) = work.pop() {
            let mut bmin = Vec3::splat(f32::INFINITY);
            let mut bmax = Vec3::splat(f32::NEG_INFINITY);
            let mut cmin = Vec3::splat(f32::INFINITY);
            let mut cmax = Vec3::splat(f32::NEG_INFINITY);
            for &i in &self.order[start..end] {
                let (lo, hi) = self.tris[i as usize].bounds();
                bmin = bmin.min(lo);
                bmax = bmax.max(hi);
                let c = centroids[i as usize];
                cmin = cmin.min(c);
                cmax = cmax.max(c);
            }
            self.nodes[node].min = bmin;
            self.nodes[node].max = bmax;
            let ext = cmax - cmin;
            let axis = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            if end - start <= LEAF_SIZE || ext[axis] <= 0.0 {
                self.nodes[node].first = start as u32;
                self.nodes[node].count = (end - start) as u32;
                continue;
            }
            self.order[start..end].sort_unstable_by(|&a, &b| {
                centroids[a as usize][axis].total_cmp(&centroids[b as usize][axis]).then(a.cmp(&b))
            });
            let mid = (start + end) / 2;
            let left = self.nodes.len();
            self.nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
            self.nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
            self.nodes[node].first = left as u32;
            self.nodes[node].count = 0;
            work.push((left + 1, mid, end));
            work.push((left, start, mid));
        }
    }

    /// First hit along `dir` (unit length) within `max_distance`. Triangles are double-sided.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit> {
        self.raycast_filtered(origin, dir, max_distance, |_| true)
    }

    /// Like [`World::raycast`], ignoring the triangles for which `accept(hit)` is false.
    pub fn raycast_filtered(
        &self,
        origin: Vec3,
        dir: Vec3,
        max_distance: f32,
        accept: impl Fn(&Hit) -> bool,
    ) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv_dir = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
        let mut best_t = max_distance;
        let mut best: Option<(u32, Hit)> = None;
        let mut stack = [0u32; STACK];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let node = self.nodes[stack[sp] as usize];
            if !ray_box(node.min, node.max, origin, inv_dir, best_t) {
                continue;
            }
            if node.count == 0 {
                if sp + 2 <= STACK {
                    stack[sp] = node.first + 1;
                    stack[sp + 1] = node.first;
                    sp += 2;
                }
                continue;
            }
            for k in node.first..node.first + node.count {
                let i = self.order[k as usize];
                let tri = &self.tris[i as usize];
                let Some(t) = ray_tri(tri, origin, dir, best_t) else { continue };
                if t == best_t && best.is_some_and(|(bi, _)| bi < i) {
                    continue;
                }
                let hit = self.make_hit(i, origin, dir, t);
                if !accept(&hit) {
                    continue;
                }
                best_t = t;
                best = Some((i, hit));
            }
        }
        best.map(|(_, h)| h)
    }

    #[inline]
    pub(crate) fn make_hit(&self, i: u32, origin: Vec3, dir: Vec3, t: f32) -> Hit {
        let tri = &self.tris[i as usize];
        let mut normal = tri.n;
        if normal.dot(dir) > 0.0 {
            normal = -normal;
        }
        Hit { distance: t, point: origin + dir * t, normal, surface: tri.surface }
    }

    /// Indices of every triangle whose bounding box overlaps `[min, max]`, sorted ascending.
    pub fn query_box(&self, min: Vec3, max: Vec3, out: &mut Vec<u32>) {
        out.clear();
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = [0u32; STACK];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let node = self.nodes[stack[sp] as usize];
            if !boxes_overlap(node.min, node.max, min, max) {
                continue;
            }
            if node.count == 0 {
                if sp + 2 <= STACK {
                    stack[sp] = node.first + 1;
                    stack[sp + 1] = node.first;
                    sp += 2;
                }
                continue;
            }
            for k in node.first..node.first + node.count {
                let i = self.order[k as usize];
                let (lo, hi) = self.tris[i as usize].bounds();
                if boxes_overlap(lo, hi, min, max) {
                    out.push(i);
                }
            }
        }
        out.sort_unstable();
    }

    /// Ray test against a candidate list from [`World::query_box`]. Lowest index wins ties.
    pub(crate) fn raycast_in(
        &self,
        cands: &[u32],
        origin: Vec3,
        dir: Vec3,
        max_distance: f32,
        accept: impl Fn(&Hit) -> bool,
    ) -> Option<Hit> {
        let mut best_t = max_distance;
        let mut best: Option<Hit> = None;
        for &i in cands {
            let tri = &self.tris[i as usize];
            let Some(t) = ray_tri(tri, origin, dir, best_t) else { continue };
            if best.is_some() && t >= best_t {
                continue;
            }
            let hit = self.make_hit(i, origin, dir, t);
            if !accept(&hit) {
                continue;
            }
            best_t = t;
            best = Some(hit);
        }
        best
    }
}
