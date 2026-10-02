//! The ice planet's shapes (docs/blocks-ice.md): progressive turns, S-bends and snakes join the
//! grid exactly, bend without kinks, and are no tighter than they are meant to be.

use glam::Vec3;
use track::Surface;
use track::kit::{Connector, Heading, Kind, Piece, Pivot, Placed, Side};

fn shapes() -> Vec<(String, Kind, f32)> {
    // (name, shape, tightest radius allowed along the centreline, metres)
    let mut out = Vec::new();
    for side in [Side::Left, Side::Right] {
        for (size, min) in [(2, 24.0), (3, 40.0), (4, 56.0)] {
            out.push((format!("curve{size} {side:?}"), Kind::Curve { size, side, bank_deg: 0.0, pivot: Pivot::Centre, levels: 0 }, min));
            out.push((format!("curveberm{size} {side:?}"), Kind::Curve { size, side, bank_deg: 18.0, pivot: Pivot::Inner, levels: 0 }, min));
        }
        let shift = if side == Side::Left { 1 } else { -1 };
        for (cells, min) in [(2, 20.0), (3, 45.0), (4, 80.0)] {
            out.push((format!("sbend{cells} {side:?}"), Kind::Shift { cells, shift }, min));
        }
    }
    for cells in [2, 3, 4] {
        out.push((format!("snake{cells}"), Kind::Snake { cells }, 10.0));
    }
    out
}

fn placed(kind: Kind, deck: Surface) -> Placed {
    let piece = if deck == Surface::Dirt { Piece::snow(kind) } else { Piece::road(kind) };
    Placed::new(piece, Connector::entering((0, 0), 0, Heading::North))
}

#[test]
fn ends_join_the_grid() {
    for (name, kind, _) in shapes() {
        for deck in [Surface::Road, Surface::Dirt] {
            let p = placed(kind, deck);
            assert!(p.exit.is_on_grid(), "{name}: exit off the grid");
            let (a, b) = (p.frame(0.0), p.frame(p.length));
            assert!(a.horiz.distance(Vec3::new(p.entry.pos.x, 0.0, p.entry.pos.z)) < 1e-4, "{name}: entry");
            assert!(b.horiz.distance(Vec3::new(p.exit.pos.x, 0.0, p.exit.pos.z)) < 1e-4, "{name}: exit {:?} vs {:?}", b.horiz, p.exit.pos);
            assert!(a.forward.distance(p.entry.heading.forward()) < 1e-4, "{name}: entry heading");
            assert!(b.forward.distance(p.exit.heading.forward()) < 1e-4, "{name}: exit heading");
            // Just inside the ends the deck is already almost there: no step at the joins.
            let near = p.frame(p.length - 0.01);
            assert!(near.horiz.distance(b.horiz) < 0.02, "{name}: step at the exit");
        }
    }
}

#[test]
fn bend_without_kinks_and_not_too_tight() {
    for (name, kind, min_radius) in shapes() {
        let p = placed(kind, Surface::Road);
        let h = 0.25;
        let mut tightest = f32::INFINITY;
        let mut s = h;
        while s <= p.length - h {
            let (a, b, c) = (p.frame(s - h).horiz, p.frame(s).horiz, p.frame(s + h).horiz);
            // Radius of the circle through three points of the centreline.
            let (ab, bc, ca) = (a.distance(b), b.distance(c), c.distance(a));
            let area2 = (b - a).cross(c - a).length();
            if area2 > 1e-6 {
                tightest = tightest.min(ab * bc * ca / (2.0 * area2));
            }
            // The heading turns smoothly: over half a metre, no more than the tightest radius
            // allowed would turn it (and a little more).
            let turn = p.frame(s - h).forward.angle_between(p.frame(s + h).forward);
            assert!(turn < 1.2 * 2.0 * h / min_radius, "{name} at {s}: kink of {}°", turn.to_degrees());
            s += h;
        }
        println!("{name}: length {:.1} m, tightest radius {tightest:.1} m", p.length);
        assert!(tightest >= min_radius, "{name}: radius {tightest} under {min_radius}");
    }
}

#[test]
fn snow_is_narrow() {
    let p = placed(Kind::Straight { cells: 1 }, Surface::Dirt);
    assert_eq!(p.frame(10.0).half_width, track::kit::SNOW_HALF_WIDTH);
    let p = Placed::new(Piece::dirt(Kind::Straight { cells: 1 }), Connector::entering((0, 0), 0, Heading::North));
    assert_eq!(p.frame(10.0).half_width, track::kit::DIRT_HALF_WIDTH);
}

#[test]
fn turns_climb_and_descend_smoothly() {
    use track::map::{catalogue, parse_block};
    let climbing: Vec<String> = catalogue().into_iter().filter(|id| id.contains("_up") || id.contains("_down")).filter(|id| !id.contains("slope") && !id.contains("landing")).collect();
    assert!(climbing.iter().any(|id| id == "curve3_left_down2"));
    assert!(climbing.iter().any(|id| id == "uberm2_right_up1"));
    assert!(climbing.iter().any(|id| id == "berm2_left_down1"));
    // Too short to climb a level with a rounded crest, or a bank and a climb that add up.
    assert!(parse_block("turn1_left_up1", None).is_err());
    assert!(parse_block("uberm1_left_up1", None).is_err());
    assert!(parse_block("berm2_left_up2", None).is_err());
    for id in &climbing {
        for variant in [None, Some("snow")] {
            let piece = parse_block(id, variant).unwrap();
            let p = Placed::new(piece, Connector::entering((0, 0), 2, Heading::North));
            assert!(p.exit.is_on_grid(), "{id}: exit off the grid");
            let levels = (p.exit.pos.y - p.entry.pos.y) / track::kit::LEVEL;
            assert!((levels - levels.round()).abs() < 1e-4 && levels.round() != 0.0, "{id}: climbs {levels} levels");
            // Flat at both ends, and no crest or sag tighter than the kit allows.
            let y = |s: f32| p.frame(s).centre().y;
            assert!((y(0.5) - y(0.0)).abs() < 0.01 && (y(p.length) - y(p.length - 0.5)).abs() < 0.01, "{id}: not flat at the joins");
            let h = 0.5;
            let (mut crest, mut sag) = (f32::INFINITY, f32::INFINITY);
            let mut s = h;
            while s <= p.length - h {
                let curv = (y(s + h) - 2.0 * y(s) + y(s - h)) / (h * h);
                if curv < 0.0 {
                    crest = crest.min(-1.0 / curv);
                } else if curv > 0.0 {
                    sag = sag.min(1.0 / curv);
                }
                s += h;
            }
            if variant.is_none() {
                println!("{id}: crest {crest:.0} m, sag {sag:.0} m");
            }
            assert!(crest >= 55.0 && sag >= 25.0, "{id} {variant:?}: crest {crest} m, sag {sag} m");
        }
    }
    println!("{} climbing turns: {}", climbing.len(), climbing.join(" "));
}
