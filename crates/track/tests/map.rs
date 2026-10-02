//! Checks on the map file format, the Mars terrain and the scenery.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use track::kit::{self, Connector, Gate, HALF_WIDTH, Heading, Kind, Layout, Piece, Side, TERRAIN_Y};
use track::map::{self, BuiltMap, FORMAT, Map, MapError};
use track::scenery::ROUTE_CLEARANCE;
use track::{Surface, TrackMesh, demo, dirt};

/// Jezero as it was written before the map file existed, with the elevated start it has had
/// since: the chain of pieces the file must reproduce exactly.
fn legacy_jezero() -> Layout {
    use Side::{Left, Right};
    let road = Piece::road;
    let dirt = Piece::dirt;
    let mut l = Layout::new("Jezero", Connector::entering((13, -12), 2, Heading::North));
    l.push(road(Kind::Straight { cells: 1 }).gate(Gate::Start))
        .push(road(Kind::Slope { cells: 2, levels: -2 }))
        .push(road(Kind::turn(2, Right)))
        .push(road(Kind::turn(3, Left)))
        .push(road(Kind::Slope { cells: 4, levels: 2 }))
        .push(road(Kind::banked(2, Right, 18.0)))
        .push(road(Kind::turn(1, Left)))
        .push(road(Kind::turn(3, Right)))
        .push(road(Kind::Straight { cells: 1 }).gate(Gate::Checkpoint))
        .push(road(Kind::JumpRamp { lip_deg: 4.0 }))
        .push(road(Kind::Landing { cells: 7, levels: -2, gap: 8.0, epsilon: 0.07, outrun: 48.0, shift: 0 }))
        .push(road(Kind::Transition { to: Surface::Dirt }))
        .push(dirt(Kind::turn(2, Left)))
        .push(dirt(Kind::Whoops { cells: 3, bumps: 3, height: 0.6 }))
        .push(dirt(Kind::berm(2, Right, 2, 18.0)))
        .push(dirt(Kind::Straight { cells: 1 }).gate(Gate::Checkpoint))
        .push(dirt(Kind::turn(2, Left)))
        .push(dirt(Kind::Transition { to: Surface::Road }))
        .push(road(Kind::turn(3, Right)))
        .push(road(Kind::Slope { cells: 3, levels: 1 }))
        .push(road(Kind::turn(2, Right)))
        .push(road(Kind::Straight { cells: 1 }).gate(Gate::Checkpoint))
        .push(road(Kind::turn(2, Left)))
        .push(road(Kind::Slope { cells: 3, levels: -1 }))
        .push(road(Kind::turn(1, Right)))
        .push(road(Kind::turn(2, Left)))
        .push(road(Kind::turn(2, Right)))
        .push(road(Kind::Straight { cells: 1 }).gate(Gate::Finish));
    l
}

fn built() -> BuiltMap {
    demo::map().build_detailed().expect("jezero builds")
}

fn assert_close(a: Vec3, b: Vec3, what: &str) {
    assert!(a.distance(b) < 1e-4, "{what}: {a} vs {b}");
}

#[test]
fn jezero_file_matches_the_original_chain() {
    let legacy = legacy_jezero();
    let map = demo::map();
    assert_eq!(map.blocks, map::blocks_from_layout(&legacy).unwrap());
    let layout = map.layout().unwrap();
    assert_eq!(layout.pieces.len(), legacy.pieces.len());
    for (i, (a, b)) in layout.pieces.iter().zip(&legacy.pieces).enumerate() {
        assert_eq!(a.piece, b.piece, "piece {i}");
        assert_close(a.entry.pos, b.entry.pos, &format!("piece {i} entry"));
        assert_close(a.exit.pos, b.exit.pos, &format!("piece {i} exit"));
        assert_eq!(a.entry.heading, b.entry.heading);
        assert!((a.entry.grade - b.entry.grade).abs() < 1e-6);
    }
}

#[test]
fn jezero_triggers_and_route_are_unchanged() {
    // The map file builds the same driving line as the original chain (the dirt is dug into the
    // terrain now, so the meshes differ there).
    let old = legacy_jezero().build();
    let new = built();
    let t = &new.track;
    assert_close(t.start.position, old.start.position, "start");
    assert_eq!(t.start.yaw, old.start.yaw);
    assert_eq!(t.checkpoints.len(), old.checkpoints.len());
    for (a, b) in t.checkpoints.iter().chain([&t.finish]).zip(old.checkpoints.iter().chain([&old.finish])) {
        assert_close(a.center, b.center, "trigger");
        assert_eq!(a.half_extents, b.half_extents);
        assert!((a.yaw - b.yaw).abs() < 1e-6);
    }
    assert_eq!(t.route.len(), old.route.len());
    for (a, b) in t.route.iter().zip(&old.route) {
        assert_close(*a, *b, "route");
    }
}

#[test]
fn json_round_trips() {
    let map = demo::map();
    let again = Map::load(&map.to_json()).unwrap();
    assert_eq!(map, again);
    assert_eq!(map.to_json(), demo::JSON, "maps/jezero.json is not in canonical form");
}

#[test]
fn catalogue_round_trips() {
    for id in map::catalogue() {
        for variant in [None, Some("dirt"), Some("sandbags"), Some("bumpers")] {
            let Ok(piece) = map::parse_block(&id, variant) else {
                assert!(id.starts_with("to_") && variant.is_some(), "{id} {variant:?}");
                continue;
            };
            let (back, v) = map::block_id(&piece).unwrap_or_else(|| panic!("{id}: no id for {piece:?}"));
            assert_eq!(back, id);
            assert_eq!(v.as_deref(), variant);
        }
    }
    assert_eq!(map::parse_block("turn0_left", None), Err(map::BlockError::UnknownId));
    assert_eq!(map::parse_block("loop", None), Err(map::BlockError::UnknownId));
    assert_eq!(map::parse_block("straight", Some("ice")), Err(map::BlockError::BadVariant));
}

fn edited(f: impl FnOnce(&mut Map)) -> Result<Map, MapError> {
    let mut m = demo::map();
    f(&mut m);
    Map::load(&m.to_json())
}

#[test]
fn invalid_maps_are_rejected() {
    assert!(matches!(Map::load("{"), Err(MapError::Json(_))));
    assert_eq!(edited(|m| m.format = 2).unwrap_err(), MapError::Format(2));
    assert!(matches!(edited(|m| m.blocks[3].block = "slope4_sideways".into()), Err(MapError::UnknownBlock { index: 3, .. })));
    assert!(matches!(edited(|m| m.blocks[1].rotation = 4), Err(MapError::BadRotation { index: 1, .. })));
    // Removing a block breaks the route before the finish.
    assert!(matches!(edited(|m| { m.blocks.remove(5); }), Err(MapError::RouteBroken { .. })));
    // A landing needs its ramp.
    let ramp = demo::map().blocks.iter().position(|b| b.block == "jump_ramp").unwrap();
    assert!(matches!(edited(|m| m.blocks[ramp].block = "straight".into()), Err(MapError::LandingWithoutRamp { .. })));
    assert_eq!(edited(|m| { m.blocks.remove(0); }).unwrap_err(), MapError::MissingGate("start"));
    // A checkpoint off the route makes the map unfinishable.
    let err = edited(|m| {
        m.blocks.push(map::BlockPlacement { block: "checkpoint".into(), cell: [40, 40], level: 0, rotation: 0, variant: None })
    });
    assert!(matches!(err, Err(MapError::Unreachable { .. })));
}

#[test]
fn blocks_off_the_route_are_built() {
    let base = built();
    let m = edited(|m| {
        m.blocks.push(map::BlockPlacement { block: "straight3".into(), cell: [40, 40], level: 1, rotation: 1, variant: None })
    })
    .unwrap();
    let b = m.build_detailed().unwrap();
    assert!(b.triangles.blocks > base.triangles.blocks);
    assert_eq!(b.track.route.len(), base.track.route.len());
}

/// Every terrain triangle of the mesh (they follow the blocks).
fn terrain_mesh(b: &BuiltMap) -> TrackMesh {
    let mut m = TrackMesh::default();
    let first = b.triangles.blocks;
    for i in first..first + b.triangles.terrain {
        let base = m.positions.len() as u32;
        for p in b.track.mesh.triangle(i) {
            m.positions.push(p);
        }
        m.indices.extend_from_slice(&[base, base + 1, base + 2]);
        m.tri_surface.push(b.track.mesh.tri_surface[i]);
    }
    m
}

#[test]
fn terrain_is_watertight() {
    let b = built();
    let m = terrain_mesh(&b);
    let key = |p: Vec3| ((p.x * 8.0).round() as i64, (p.z * 8.0).round() as i64);
    type Edge = ((i64, i64), (i64, i64));
    let mut edges: HashMap<Edge, i32> = HashMap::new();
    for i in 0..m.triangle_count() {
        let t = m.triangle(i);
        assert!(matches!(m.tri_surface[i], Surface::Ground | Surface::Dirt));
        for k in 0..3 {
            let (a, c) = (key(t[k]), key(t[(k + 1) % 3]));
            *edges.entry((a.min(c), a.max(c))).or_default() += 1;
        }
    }
    let half = 0.5 * b.terrain.size();
    let centre = kit::Layout::centre(&b.layout);
    let on_border = |k: (i64, i64)| {
        let (x, z) = (k.0 as f32 / 8.0 - centre.x, k.1 as f32 / 8.0 - centre.z);
        (x.abs() - half).abs() < 0.2 || (z.abs() - half).abs() < 0.2
    };
    for ((a, c), n) in &edges {
        assert!(*n == 2 || (*n == 1 && on_border(*a) && on_border(*c)), "edge {a:?}-{c:?} used by {n} triangles");
    }
}

/// Points around the footprint of every swept part of a block (not where a dirt corridor's apron
/// runs under it): deck edges, skirt feet, gate posts, with their offset from the centreline and
/// the height of the deck's lower edge there.
fn footprint_points(layout: &Layout) -> Vec<(Vec2, f32, f32)> {
    let mut out = Vec::new();
    for (i, p) in layout.pieces.iter().enumerate() {
        let Some((s0, s1)) = p.swept_range() else { continue };
        let carved = |q: &kit::Placed| q.carved_range().is_some();
        let (mut s0, mut s1) = (s0, s1);
        let (meets_before, meets_after) = match p.carved_range() {
            Some((c0, c1)) => ((c1 - s0).abs() < 1e-3, (c0 - s1).abs() < 1e-3),
            None => (i > 0 && carved(&layout.pieces[i - 1]), layout.pieces.get(i + 1).is_some_and(carved)),
        };
        if meets_before {
            s0 += dirt::APRON + 8.0;
        }
        if meets_after {
            s1 -= dirt::APRON + 8.0;
        }
        if s1 <= s0 {
            continue;
        }
        let n = ((s1 - s0) / 2.0).ceil() as usize;
        for k in 0..=n {
            let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
            let low = f.deck_point(-HALF_WIDTH).y.min(f.deck_point(HALF_WIDTH).y);
            for u in [-15.0, -13.0, -12.2, -HALF_WIDTH, 0.0, HALF_WIDTH, 12.2, 13.0, 15.0] {
                let q = f.horiz + f.left * u;
                out.push((Vec2::new(q.x, q.z), u, low));
            }
        }
    }
    out
}

#[test]
fn terrain_is_flat_under_the_swept_blocks() {
    // At the terrain plane, or where a landform comes up under an elevated deck, a level below
    // the deck's lower edge.
    let b = built();
    for (p, u, low) in footprint_points(&b.layout) {
        let h = b.terrain.height(p.x, p.y);
        let rock = b.terrain.landform(p.x, p.y);
        assert!(rock <= low + 1e-3, "landform {rock} m high under a deck {low} m up at {p}");
        let flat = TERRAIN_Y + rock;
        assert!(h == flat || u.abs() > 13.0, "terrain at {p} (u {u}) is {h}, not {flat}");
        // Just past the footprint the ground leaves the pad smoothly.
        assert!((h - flat).abs() < 0.05, "terrain at {p} (u {u}) is {h}, not {flat}");
    }
}

#[test]
fn jezero_starts_on_a_butte() {
    // The start block stands 16 m up on a butte: the rock comes up to its deck all around it.
    let b = built();
    let s = b.track.start.position;
    assert!(s.y > 15.0, "start at {s}");
    let f = b.layout.pieces[0].frame(kit::START_POSE_S);
    for u in [-15.0, -HALF_WIDTH, 0.0, HALF_WIDTH, 15.0] {
        let q = f.horiz + f.left * u;
        let h = b.terrain.height(q.x, q.z);
        assert!((h - (s.y + TERRAIN_Y)).abs() < 1e-3, "ground {u} m aside the start at {h}");
    }
}

#[test]
fn nothing_but_the_deck_under_the_route() {
    // Straight down from above the deck, across its whole width, the first thing hit is the deck.
    let b = built();
    let world = Raycaster::new(&b.track.mesh);
    for p in b.layout.pieces.iter() {
        let (s0, s1) = p.deck_range();
        let n = ((s1 - s0) / 6.0).ceil() as usize;
        for k in 0..=n {
            let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
            for u in [-HALF_WIDTH + 0.6, -5.0, 0.0, 5.0, HALF_WIDTH - 0.6] {
                let q = f.deck_point(u);
                let (dist, surface) = world.down(q + Vec3::Y * 3.0).expect("deck under the route");
                assert!(matches!(surface, Surface::Road | Surface::Dirt), "{surface:?} over the deck at {q}");
                // The deck is a polyline along the road: a few centimetres of chord on the whoops.
                assert!((dist - 3.0).abs() < 0.1, "deck at {q} is {} m off", dist - 3.0);
            }
        }
    }
}

#[test]
fn no_builtin_map_buries_a_deck() {
    // Across the whole road width of every block, the first thing below is a deck, never the
    // terrain over it (a turn banked about its centreline at level 0 sinks its inside half).
    for map in track::builtin_maps() {
        let b = map.build_detailed().unwrap();
        let world = Raycaster::new(&b.track.mesh);
        for (i, p) in b.layout.pieces.iter().enumerate() {
            let (s0, s1) = p.deck_range();
            let n = ((s1 - s0) / 6.0).ceil() as usize;
            for k in 0..=n {
                let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
                for u in [-HALF_WIDTH + 0.6, -5.0, 0.0, 5.0, HALF_WIDTH - 0.6] {
                    let q = f.deck_point(u);
                    let surface = world.down(q + Vec3::Y * 3.0).map(|(_, s)| s);
                    assert!(
                        matches!(surface, Some(Surface::Road | Surface::Dirt)),
                        "{}: {surface:?} over block {i} ({:?}) {u} m from its centreline at {q}",
                        map.name,
                        p.piece.kind
                    );
                }
            }
        }
    }
}

#[test]
fn ground_near_the_road_is_gentle() {
    // Within 40 m of the driving line along roads: no cliff (slopes under 20°), and the ground
    // rolls away from the road's level by no more than a metre plus 15 % of the distance (the
    // rises and hollows of `hills`, 7 m at 40 m). (Dirt corridors have banks, see
    // `dirt_corridors_are_dug_into_the_plain`; the map's own landforms stand close on purpose,
    // cut around the road, see `terrain_is_flat_under_the_swept_blocks`.)
    let b = built();
    let route = &b.track.route;
    let on_landform = |q: Vec3| [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)].iter().any(|(dx, dz)| b.terrain.landform(q.x + dx, q.z + dz) > 0.0);
    for (i, w) in route.windows(2).enumerate().step_by(3) {
        let near_dirt = |q: Vec3| b.terrain.sample(q.x, q.z).edge < dirt::REACH;
        if [-40.0, 0.0, 40.0].iter().any(|&u| near_dirt(w[0] + Vec3::new((w[1] - w[0]).z, 0.0, -(w[1] - w[0]).x).normalize() * u)) {
            continue;
        }
        let d = (w[1] - w[0]).normalize();
        let left = Vec3::new(d.z, 0.0, -d.x).normalize();
        for u in [-40.0, -30.0, -20.0, 20.0, 30.0, 40.0] {
            let q = w[0] + left * u;
            if on_landform(q) {
                continue;
            }
            let n = b.terrain.normal(q.x, q.z);
            assert!(n.y > 20f32.to_radians().cos(), "route point {i}, {u} m aside: slope {:.0}°", n.y.acos().to_degrees());
            let h = b.terrain.height(q.x, q.z);
            assert!((h - TERRAIN_Y).abs() < 1.0 + 0.15 * u.abs(), "route point {i}, {u} m aside: ground at {h}");
        }
    }
}

/// Route pieces carved as dirt corridors, with the part of them away from any other surface.
fn corridor_spans(b: &BuiltMap) -> Vec<(&kit::Placed, f32, f32)> {
    b.layout
        .pieces
        .iter()
        .filter(|p| !matches!(p.piece.kind, Kind::Transition { .. }))
        .filter_map(|p| p.carved_range().map(|(c0, c1)| (p, c0, c1)))
        .collect()
}

#[test]
fn dirt_floor_is_the_deck_across_its_width() {
    // Straight down onto a corridor floor, across its whole nominal width: dirt, at the deck's
    // height (the banked deck plane, then its gentle trough beyond 10 m).
    let b = built();
    let world = Raycaster::new(&b.track.mesh);
    let spans = corridor_spans(&b);
    assert!(!spans.is_empty());
    for (p, c0, c1) in spans {
        let n = ((c1 - c0) / 5.0).ceil() as usize;
        for k in 0..=n {
            let f = p.frame(c0 + (c1 - c0) * k as f32 / n as f32);
            for u in [-13.0, -9.0, -4.0, 0.0, 4.0, 9.0, 13.0] {
                let q = f.deck_point(u);
                let (dist, surface) = world.down(q + Vec3::Y * 3.0).expect("floor under the route");
                assert_eq!(surface, Surface::Dirt, "{surface:?} on the dirt floor at {q} (u {u})");
                let off = 3.0 - dist;
                let allowed = if u.abs() <= HALF_WIDTH { 0.1 } else { 0.1 + 0.03 * (u.abs() - HALF_WIDTH).powi(2) + 0.35 * f.bank.abs() * (u.abs() - HALF_WIDTH) };
                assert!(off.abs() < allowed, "floor at {q} (u {u}) is {off} m off the deck");
            }
        }
    }
}

#[test]
fn dirt_corridors_are_dug_into_the_plain() {
    // Beside a corridor the plain stands above the floor (it was dug), the banks are no steeper
    // than about 40°, and past the first metres of the banks the ground is off-track.
    let b = built();
    let (mut above, mut total) = (0, 0);
    for (p, c0, c1) in corridor_spans(&b) {
        let n = ((c1 - c0) / 4.0).ceil() as usize;
        for k in 1..n {
            let f = p.frame(c0 + (c1 - c0) * k as f32 / n as f32);
            for side in [1.0, -1.0] {
                // Walk out from the floor until 12 m past its edge.
                let mut edge_floor = None;
                let mut u = 0.0;
                while u < 45.0 {
                    let q = f.horiz + f.left * (side * u);
                    let smp = b.terrain.sample(q.x, q.z);
                    if smp.edge >= 0.0 && edge_floor.is_none() {
                        edge_floor = Some(smp.height);
                    }
                    if (0.0..10.0).contains(&smp.edge) {
                        let slope = b.terrain.normal(q.x, q.z).y.acos().to_degrees();
                        assert!(slope < 42.0 + f.bank.abs().to_degrees(), "bank at {q}: {slope:.0}°");
                    }
                    if smp.edge >= 12.0 {
                        total += 1;
                        if smp.height > edge_floor.unwrap() + 0.2 {
                            above += 1;
                        }
                        break;
                    }
                    u += 0.5;
                }
            }
        }
    }
    assert!(total > 50);
    assert!(above * 10 >= total * 8, "the plain stands above the floor at only {above} of {total} points");
    // Surfaces: dirt on the floor, off-track ground well up the banks.
    let world = Raycaster::new(&b.track.mesh);
    for (p, c0, c1) in corridor_spans(&b) {
        let f = p.frame(0.5 * (c0 + c1));
        for side in [1.0f32, -1.0] {
            let mut u = 0.0;
            while u < 45.0 {
                let q = f.horiz + f.left * (side * u);
                let smp = b.terrain.sample(q.x, q.z);
                if smp.edge > 6.0 {
                    let (_, surface) = world.down(Vec3::new(q.x, smp.height + 5.0, q.z)).unwrap();
                    assert_eq!(surface, Surface::Ground, "{} m past the floor edge at {q}", smp.edge);
                    break;
                }
                u += 0.5;
            }
        }
    }
}

#[test]
fn no_step_where_dirt_meets_road() {
    // Along the centreline across every transition, the surface under the wheels never jumps.
    let b = built();
    let world = Raycaster::new(&b.track.mesh);
    let mut seen = 0;
    for p in &b.layout.pieces {
        if !matches!(p.piece.kind, Kind::Transition { .. }) {
            continue;
        }
        seen += 1;
        let mid = 0.5 * p.length;
        let mut last: Option<f32> = None;
        let mut surfaces = Vec::new();
        let mut s = mid - 12.0;
        while s <= mid + 12.0 {
            for u in [-8.0, 0.0, 8.0] {
                let q = p.frame(s).deck_point(u);
                let (dist, surface) = world.down(q + Vec3::Y * 3.0).expect("deck");
                let y = q.y + 3.0 - dist;
                assert!((y - q.y).abs() < 0.06, "transition at s {s}, u {u}: surface {} m off the deck", y - q.y);
                if u == 0.0 {
                    if let Some(l) = last {
                        assert!((y - l).abs() < 0.03, "step of {} m at s {s}", y - l);
                    }
                    last = Some(y);
                    surfaces.push(surface);
                }
            }
            s += 0.25;
        }
        assert!(surfaces.contains(&Surface::Road) && surfaces.contains(&Surface::Dirt));
    }
    assert!(seen >= 2);
}

#[test]
fn terrain_far_away_has_relief() {
    let b = built();
    let size = b.terrain.size();
    let c = b.layout.centre();
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for i in 0..64 {
        for k in 0..64 {
            let (x, z) = (c.x + (i as f32 / 63.0 - 0.5) * size, c.z + (k as f32 / 63.0 - 0.5) * size);
            let h = b.terrain.height(x, z);
            lo = lo.min(h);
            hi = hi.max(h);
        }
    }
    assert!(hi - lo > 60.0, "terrain spans only {lo}..{hi}");
    assert!(b.track.fall_limit_y < lo, "fall limit {} above the lowest ground {lo}", b.track.fall_limit_y);
}

#[test]
fn rocks_keep_clear_of_the_route() {
    let b = built();
    assert!(b.props.len() > 100, "{} props", b.props.len());
    assert!(!demo::map().scenery.is_empty());
    let route = &b.track.route;
    for (i, p) in b.props.iter().enumerate() {
        let q = Vec2::new(p.position.x, p.position.z);
        let d = route
            .windows(2)
            .map(|w| {
                let (a, c) = (Vec2::new(w[0].x, w[0].z), Vec2::new(w[1].x, w[1].z));
                let t = ((q - a).dot(c - a) / (c - a).length_squared().max(1e-6)).clamp(0.0, 1.0);
                q.distance(a + (c - a) * t)
            })
            .fold(f32::MAX, f32::min);
        assert!(d - p.radius >= ROUTE_CLEARANCE, "prop {i} ({:?}) {d} m from the route", p.kind);
    }
}

#[test]
fn every_builtin_map_keeps_its_rocks_clear_of_the_route() {
    for map in track::builtin_maps() {
        let b = map.build_detailed().unwrap();
        let route = &b.track.route;
        for (i, p) in b.props.iter().enumerate() {
            let q = Vec2::new(p.position.x, p.position.z);
            let d = route
                .windows(2)
                .map(|w| {
                    let (a, c) = (Vec2::new(w[0].x, w[0].z), Vec2::new(w[1].x, w[1].z));
                    let t = ((q - a).dot(c - a) / (c - a).length_squared().max(1e-6)).clamp(0.0, 1.0);
                    q.distance(a + (c - a) * t)
                })
                .fold(f32::MAX, f32::min);
            assert!(d - p.radius >= ROUTE_CLEARANCE, "{}: prop {i} ({:?}) {d} m from the route", map.name, p.kind);
        }
    }
}

#[test]
fn dirt_decks_roll_gently() {
    // On every map, a dirt deck's bank never changes faster than kit::DIRT_ROLL_RATE along it,
    // and the inside of a dirt turn stays level: nothing stands up under the wheels or dips
    // beside them.
    for map in track::builtin_maps() {
        let layout = map.layout().unwrap();
        for p in layout.pieces.iter().filter(|p| p.piece.deck == Surface::Dirt) {
            let (s0, s1) = p.deck_range();
            let n = ((s1 - s0) / 0.5).ceil() as usize;
            let mut last: Option<f32> = None;
            for k in 0..=n {
                let s = s0 + (s1 - s0) * k as f32 / n as f32;
                let f = p.frame(s);
                if let Some(b) = last {
                    let rate = (f.bank - b).abs() / ((s1 - s0) / n as f32);
                    assert!(rate <= kit::DIRT_ROLL_RATE * 1.02, "{}: {:?} rolls {rate} rad/m at s {s}", map.name, p.piece.kind);
                }
                last = Some(f.bank);
                if matches!(p.piece.kind, Kind::Turn { .. }) && f.bank != 0.0 {
                    // The low side of the bank is the inside; past the bend it is level.
                    let inside = -f.bank.signum();
                    let edge = f.deck_point(inside * kit::DIRT_HALF_WIDTH).y;
                    assert!((edge - f.pivot_y).abs() < 1e-3, "{}: {:?} dips {} m inside at s {s}", map.name, p.piece.kind, f.pivot_y - edge);
                }
            }
        }
    }
}

#[test]
fn noctis_roads_up_high_stand_on_rock() {
    // Noctis runs through buttes: its elevated roads (the start 24 m up, the U-turn 16 m up, the
    // ramps between) stand on the rock rather than on platform walls. Just past the deck edges
    // the ground comes up to the deck: everywhere on the level parts, and along the ramps except
    // near their foot, where the rock is cut down to the road below, and the top of the last
    // climb, where it gives way to the kicker. The kicker and its landing are built structures
    // over open ground (rock never rises under a jump).
    let map = track::builtin_maps().into_iter().find(|m| m.name == "Noctis").unwrap();
    let b = map.build_detailed().unwrap();
    let (mut total, mut on_rock) = (0, 0);
    for p in b.layout.pieces.iter().filter(|p| !matches!(p.piece.kind, Kind::JumpRamp { .. } | Kind::Landing { .. })) {
        let level = !matches!(p.piece.kind, Kind::Slope { .. });
        let (s0, s1) = p.deck_range();
        let n = ((s1 - s0) / 4.0).ceil() as usize;
        for k in 0..=n {
            let f = p.frame(s0 + (s1 - s0) * k as f32 / n as f32);
            for u in [-(HALF_WIDTH + 1.0), HALF_WIDTH + 1.0] {
                let q = f.deck_point(u);
                if q.y < 2.0 {
                    continue;
                }
                let gap = q.y - b.terrain.height(q.x, q.z);
                assert!(!level || gap < 1.5, "{:?}: the ground is {gap} m below the deck at {q}", p.piece.kind);
                total += 1;
                if gap < 1.5 {
                    on_rock += 1;
                }
            }
        }
    }
    assert!(total > 200);
    assert!(on_rock * 100 >= total * 85, "only {on_rock} of {total} points beside the elevated decks on rock");
    // The start stands on the mesa.
    assert!(b.track.start.position.y > 23.0);
}

#[test]
fn triangle_budget_by_part() {
    let b = built();
    let t = b.triangles;
    println!(
        "blocks {} · terrain {} · scenery {} · structures {} + {} drawn · total {}",
        t.blocks,
        t.terrain,
        t.scenery,
        t.structures,
        t.structures_drawn,
        b.track.mesh.triangle_count()
    );
    assert_eq!(t.blocks + t.terrain + t.scenery + t.structures, b.track.mesh.triangle_count());
    assert!(b.track.mesh.triangle_count() < 300_000);
    assert!(t.scenery < 60_000);
    assert!(t.structures + t.structures_drawn < 70_000);
}

#[test]
fn build_is_deterministic() {
    let (a, b) = (built().track.mesh, built().track.mesh);
    assert_eq!(a.positions, b.positions);
    assert_eq!(a.indices, b.indices);
}

/// Brute-force downward raycasts over a uniform grid of triangles.
struct Raycaster<'a> {
    mesh: &'a TrackMesh,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl<'a> Raycaster<'a> {
    const CELL: f32 = 16.0;

    fn new(mesh: &'a TrackMesh) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for i in 0..mesh.triangle_count() {
            let t = mesh.triangle(i);
            let lo = t[0].min(t[1]).min(t[2]);
            let hi = t[0].max(t[1]).max(t[2]);
            if hi.x - lo.x > 400.0 || hi.z - lo.z > 400.0 {
                continue; // far terrain, never under the route
            }
            for x in (lo.x / Self::CELL).floor() as i32..=(hi.x / Self::CELL).floor() as i32 {
                for z in (lo.z / Self::CELL).floor() as i32..=(hi.z / Self::CELL).floor() as i32 {
                    cells.entry((x, z)).or_default().push(i);
                }
            }
        }
        Self { mesh, cells }
    }

    /// Distance and surface of the first triangle straight below `o`.
    fn down(&self, o: Vec3) -> Option<(f32, Surface)> {
        let key = ((o.x / Self::CELL).floor() as i32, (o.z / Self::CELL).floor() as i32);
        let mut best: Option<(f32, Surface)> = None;
        for &i in self.cells.get(&key)? {
            let [a, b, c] = self.mesh.triangle(i);
            let dir = Vec3::NEG_Y;
            let (e1, e2) = (b - a, c - a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let s = o - a;
            let u = s.dot(p) / det;
            let q = s.cross(e1);
            let v = dir.dot(q) / det;
            if !(-1e-5..=1.0 + 1e-5).contains(&u) || v < -1e-5 || u + v > 1.0 + 1e-5 {
                continue;
            }
            let t = e2.dot(q) / det;
            if t >= 0.0 && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, self.mesh.tri_surface[i]));
            }
        }
        best
    }
}

#[test]
fn format_constant() {
    assert_eq!(demo::map().format, FORMAT);
}

#[test]
fn builtin_maps_load_and_build() {
    for map in track::builtin_maps() {
        let built = map.build_detailed().unwrap_or_else(|e| panic!("{}: {e}", map.name));
        assert!(!built.track.checkpoints.is_empty(), "{} has no checkpoint", map.name);
        assert!(built.track.mesh.triangle_count() < 300_000, "{} is over budget", map.name);
        assert!(built.track.mesh.triangle_count() + built.track.decor.triangle_count() < 550_000, "{} draws too much", map.name);
    }
}
