//! Checks on the block kit and the demo map.

use glam::Vec3;
use track::jump::{self, LandingProfile};
use track::kit::{self, Gate, HALF_WIDTH, Kind, Layout};
use track::{Surface, Track, TrackMesh, demo, demo_track};

fn layout() -> Layout {
    demo::layout()
}

/// First hit of a ray on the mesh: (distance, triangle index).
fn raycast(mesh: &TrackMesh, origin: Vec3, dir: Vec3) -> Option<(f32, usize)> {
    let mut best: Option<(f32, usize)> = None;
    for i in 0..mesh.triangle_count() {
        let [a, b, c] = mesh.triangle(i);
        let (e1, e2) = (b - a, c - a);
        let p = dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-9 {
            continue;
        }
        let inv = 1.0 / det;
        let s = origin - a;
        let u = s.dot(p) * inv;
        let q = s.cross(e1);
        let v = dir.dot(q) * inv;
        if !(0.0..=1.0).contains(&u) || v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = e2.dot(q) * inv;
        if t >= 0.0 && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, i));
        }
    }
    best
}

#[test]
fn connectors_sit_on_the_grid() {
    let l = layout();
    assert!(l.start.is_on_grid());
    for (i, p) in l.pieces.iter().enumerate() {
        match p.piece.kind {
            Kind::JumpRamp { .. } => assert!(p.exit.grade > 0.0, "piece {i}"),
            _ => assert!(p.exit.is_on_grid(), "piece {i} exit {:?}", p.exit),
        }
    }
}

#[test]
fn pieces_never_share_a_cell() {
    let l = layout();
    let mut all: Vec<((i32, i32), usize)> = Vec::new();
    for (i, p) in l.pieces.iter().enumerate() {
        for c in p.cells() {
            if let Some((_, j)) = all.iter().find(|(d, _)| *d == c) {
                panic!("cell {c:?} used by pieces {j} and {i}");
            }
            all.push((c, i));
        }
    }
}

#[test]
fn mesh_is_valid() {
    let t = demo_track();
    let m = &t.mesh;
    assert_eq!(m.positions.len(), m.normals.len());
    assert_eq!(m.positions.len(), m.colors.len());
    assert_eq!(m.indices.len() % 3, 0);
    assert_eq!(m.tri_surface.len(), m.triangle_count());
    for (i, (p, n)) in m.positions.iter().zip(&m.normals).enumerate() {
        assert!(p.is_finite() && n.is_finite(), "vertex {i}");
        assert!((n.length() - 1.0).abs() < 1e-4, "normal {i} has length {}", n.length());
    }
    for c in &m.colors {
        assert!(c.iter().all(|v| (0.0..=1.0).contains(v)));
    }
    for &i in &m.indices {
        assert!((i as usize) < m.positions.len());
    }
    for i in 0..m.triangle_count() {
        let [a, b, c] = m.triangle(i);
        let cross = (b - a).cross(c - a);
        let longest = (b - a).length().max((c - b).length()).max((a - c).length());
        assert!(cross.length() > 1e-3 * longest, "triangle {i} is degenerate: {a} {b} {c}");
        // Vertex normals agree with the winding (the visible side).
        let face = cross.normalize();
        for k in 0..3 {
            let n = m.normals[m.indices[3 * i + k] as usize];
            assert!(n.dot(face) > 0.5, "triangle {i} ({:?}) winding disagrees with its normals", m.tri_surface[i]);
        }
    }
}

#[test]
fn driving_surfaces_face_up() {
    let t = demo_track();
    for i in 0..t.mesh.triangle_count() {
        if matches!(t.mesh.tri_surface[i], Surface::Road | Surface::Dirt) {
            let [a, b, c] = t.mesh.triangle(i);
            let n = (b - a).cross(c - a).normalize();
            assert!(n.y > 0.8, "driving triangle {i} faces {n}");
        }
    }
}

#[test]
fn triangle_budget() {
    // Blocks (with their borders and the stilts of the raised roads), Mars terrain and rocks: a
    // guard against a runaway mesh. The decoration (sandbags, the bumpers' tubes, stakes,
    // straps) is drawn only, and costs the GPU little next to the pixels (2 October 2026, Jezero
    // with about 485 000 triangles drawn: 6.3 ms median at 3200 x 1800 on a Mac along the
    // sandbags, 8.2 ms over the raised road, as without most of them). The camps and the colony
    // add about 65 000 (2 October 2026: 9.0 ms median at the start, where they all show, with
    // and without them alike).
    let t = demo_track();
    assert!(t.mesh.triangle_count() < 300_000, "{} triangles", t.mesh.triangle_count());
    let n = t.mesh.triangle_count() + t.decor.triangle_count();
    assert!(n < 600_000, "{n} triangles drawn");
    let blocks = layout().pieces_mesh().triangle_count();
    assert!(blocks < 100_000, "{blocks} block triangles");
}

#[test]
fn joins_are_continuous() {
    let l = layout();
    for i in l.joins() {
        let (a, b) = (&l.pieces[i], &l.pieces[i + 1]);
        let (fa, fb) = (a.frame(a.length), b.frame(0.0));
        for u in [-HALF_WIDTH, -5.0, 0.0, 5.0, HALF_WIDTH] {
            let d = fa.deck_point(u).distance(fb.deck_point(u));
            assert!(d < 1e-3, "join {i}/{}: deck point {u} off by {d}", i + 1);
        }
        assert!(fa.forward.distance(fb.forward) < 1e-5, "join {i}: heading");
        assert!((fa.bank - fb.bank).abs() < 1e-5, "join {i}: bank");
        let (ga, gb) = (a.grade(a.length), b.grade(0.0));
        assert!((ga - gb).abs() < 3e-3, "join {i}: grade {ga} vs {gb}");
        assert_eq!(fa.deck, fb.deck, "join {i}: deck surface");
    }
}

#[test]
fn slopes_are_rounded() {
    // No kinks inside pieces and no crest tighter than 80 m of radius along the centreline
    // (the jump lip is an open end, not a crest).
    let l = layout();
    let h = 0.5;
    for (i, p) in l.pieces.iter().enumerate() {
        let (s0, s1) = p.deck_range();
        let y = |s: f32| p.frame(s).centre().y;
        let mut s = s0 + h;
        while s <= s1 - h {
            let curv = (y(s + h) - 2.0 * y(s) + y(s - h)) / (h * h);
            assert!(curv > -1.0 / 80.0, "piece {i} at {s}: crest radius {}", -1.0 / curv);
            assert!(curv < 1.0 / 40.0, "piece {i} at {s}: sag radius {}", 1.0 / curv);
            s += h;
        }
    }
}

#[test]
fn berms_have_no_hump() {
    // Along the centreline of every catalogue berm (quarter and U-turn, 2 and 3 cells, road and
    // dirt) the bank ramps in and out gently: no crest tighter than these radii, so a car on the
    // racing line is not thrown at speed.
    use track::kit::{Connector, Heading, Piece, Placed, Side};
    for size in [2, 3] {
        for quarters in [1, 2] {
            for (deck, min_radius) in [(Surface::Dirt, 120.0), (Surface::Road, 70.0)] {
                let kind = Kind::berm(size, Side::Left, quarters, 18.0);
                let p = Placed::new(Piece { kind, deck, gate: None, edge: Default::default(), boost: false }, Connector::entering((0, 0), 0, Heading::North));
                let h = 0.5;
                let y = |s: f32| p.frame(s).centre().y;
                let mut s = h;
                while s <= p.length - h {
                    let curv = (y(s + h) - 2.0 * y(s) + y(s - h)) / (h * h);
                    assert!(curv > -1.0 / min_radius, "{deck:?} berm {size}x{quarters} at {s}: crest radius {}", -1.0 / curv);
                    s += h;
                }
            }
        }
    }
}

#[test]
fn triggers_span_the_road() {
    let l = layout();
    let t = l.build();
    let gates: Vec<usize> =
        (0..l.pieces.len()).filter(|&i| matches!(l.pieces[i].piece.gate, Some(Gate::Checkpoint | Gate::Finish))).collect();
    assert_eq!(gates.len(), t.checkpoints.len() + 1);
    for (g, trig) in gates.iter().zip(t.checkpoints.iter().chain([&t.finish])) {
        let p = &l.pieces[*g];
        let f = p.frame(p.gate_s());
        for k in 0..=20 {
            let u = -HALF_WIDTH + 2.0 * HALF_WIDTH * k as f32 / 20.0;
            for lift in [0.3, 1.0, 2.0] {
                let q = f.deck_point(u) + f.up() * lift;
                assert!(trig.contains(q), "piece {g}: {q} outside its trigger");
            }
        }
        // And nothing far behind or ahead.
        assert!(!trig.contains(f.centre() + f.forward * 5.0 + glam::Vec3::Y));
    }
    assert!((3..=4).contains(&t.checkpoints.len()));
}

#[test]
fn start_pose_is_on_the_road() {
    let t: Track = demo_track();
    let hit = raycast(&t.mesh, t.start.position + Vec3::Y * 2.0, Vec3::NEG_Y).expect("something under the start");
    assert_eq!(t.mesh.tri_surface[hit.1], Surface::Road);
    assert!((hit.0 - 2.0).abs() < 1e-3);
}

#[test]
fn route_follows_the_road_through_every_checkpoint() {
    let t = demo_track();
    assert!(t.route[0].distance(t.start.position) < 1e-3);
    for w in t.route.windows(2) {
        let d = w[0].distance(w[1]);
        assert!(d > 0.5 && d <= 4.5, "route step {d}");
    }
    let mut last = 0;
    for (c, trig) in t.checkpoints.iter().chain([&t.finish]).enumerate() {
        let i = t.route.iter().position(|p| trig.contains(*p)).unwrap_or_else(|| panic!("checkpoint {c} not on the route"));
        assert!(i >= last, "checkpoint {c} out of order");
        last = i;
    }
}

#[test]
fn route_points_are_on_the_deck() {
    let t = demo_track();
    let l = layout();
    let pts = l.route_points(0, kit::START_POSE_S, l.pieces.len() - 1, 8.0);
    for r in pts.iter().filter(|r| r.on_deck) {
        let hit = raycast(&t.mesh, r.pos + Vec3::Y * 1.0, Vec3::NEG_Y).expect("deck under the route");
        assert!(matches!(t.mesh.tri_surface[hit.1], Surface::Road | Surface::Dirt), "route at {}", r.dist);
        assert!((hit.0 - 1.0).abs() < 0.02, "route at {} (piece {} s {}): deck {} off", r.dist, r.piece, r.s, hit.0 - 1.0);
    }
}

#[test]
fn run_takes_30_to_45_s_at_200_kmh() {
    let secs = layout().length() / (200.0 / 3.6);
    assert!((30.0..=45.0).contains(&secs), "{secs} s");
}

fn demo_landing() -> LandingProfile {
    layout().pieces.iter().find_map(|p| p.landing()).expect("the demo has a jump")
}

#[test]
fn jump_catches_heavy_to_earth_gravity() {
    let p = demo_landing();
    // (gravity, slowest lip speed that must land cleanly, fastest), km/h, touchdown within 5°.
    for (g, lo, hi) in [(40.0, 175.0, 400.0), (30.0, 160.0, 380.0), (20.0, 130.0, 330.0), (9.81, 100.0, 260.0), (3.71, 60.0, 150.0)] {
        let (a, b) = jump::envelope_kmh(&p, g, 5.0).unwrap_or_else(|| panic!("nothing lands at g = {g}"));
        assert!(a <= lo && b >= hi, "g = {g}: clean from {a} to {b} km/h");
    }
}

#[test]
fn jump_lands_in_a_descent() {
    let p = demo_landing();
    let mut x = p.gap;
    while x < p.length {
        assert!(p.grade(x) <= 0.0, "landing climbs at {x}");
        x += 1.0;
    }
    assert!(p.height(p.gap) < 0.0, "landing top above the lip");
}
