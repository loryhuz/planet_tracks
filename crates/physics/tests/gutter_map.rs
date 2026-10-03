//! The ice planet's bobsleigh gutters (docs/blocks-ice.md) with the ski car: « Canalis », the run
//! made of them, is driven in under 30 s; a line up the outside of the turns, on the wall, is
//! smooth and no slower than the middle; and a car that does not steer at all is kept in: it
//! rides up the walls and back down, never over the lip, never on its roof.

use glam::Vec3;
use physics::{Car, Input, World, neige, presets, testing::Autopilot};
use track::kit::{Connector, Heading, Layout};
use track::map::parse_block;

struct Run {
    /// Seconds to the end of the route, if it got there.
    through: Option<f32>,
    /// Highest the car rose over the deck under it, metres.
    highest: f32,
    /// Most the body tilted from upright, degrees.
    tilt: f32,
    /// Hardest slowing down along the motion (a knock against a wall), m/s², and seconds with
    /// no wheel on the ground.
    knock: f32,
    airborne: f32,
}

/// The route pushed `wall` metres toward the outside of its turns, fully where its radius is 80 m
/// or less: a line up the outer wall.
fn outer_line(route: &[Vec3], wall: f32) -> Vec<Vec3> {
    let n = route.len();
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let turn = |i: usize| {
        let (a, b, c) = (route[i.saturating_sub(3)], route[i], route[(i + 3).min(n - 1)]);
        let (d0, d1) = (flat(b - a), flat(c - b));
        if d0.length() < 1e-3 || d1.length() < 1e-3 {
            return 0.0;
        }
        libm::atan2f(d0.x * d1.z - d0.z * d1.x, d0.dot(d1)) / (0.5 * (d0.length() + d1.length()))
    };
    let raw: Vec<f32> = (0..n).map(turn).collect();
    (0..n)
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(6), (i + 6).min(n - 1));
            let k = raw[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32;
            let (a, b) = (route[i.saturating_sub(1)], route[(i + 1).min(n - 1)]);
            let d = flat(b - a).normalize_or_zero();
            let left = Vec3::new(d.z, 0.0, -d.x);
            route[i] - left * (k.signum() * (k.abs() * 80.0).min(1.0) * wall)
        })
        .collect()
}

fn drive(track: &track::Track, kmh: f32, steering: bool, bends: Option<(f32, f32)>) -> Run {
    drive_line(track, &track.route, kmh, steering, bends)
}

fn drive_line(track: &track::Track, line: &[Vec3], kmh: f32, steering: bool, bends: Option<(f32, f32)>) -> Run {
    let world = World::new(&track.mesh);
    let mut car = Car::new(neige(), &world, track.start);
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    let mut pilot = Autopilot::new(line);
    if let Some((road, dirt)) = bends {
        pilot = pilot.braking(&world, road, dirt);
    }
    let mut out = Run { through: None, highest: 0.0, tilt: 0.0, knock: 0.0, airborne: 0.0 };
    let mut last_v = car.state.velocity;
    for tick in 0..4000u32 {
        let mut input = pilot.input(&car);
        if !steering {
            input.steer = 0.0;
        }
        car.step(&world, input);
        let s = &car.state;
        out.highest = out.highest.max(s.position.y - track.route[pilot.index].y);
        out.tilt = out.tilt.max((s.rotation * Vec3::Y).y.clamp(-1.0, 1.0).acos().to_degrees());
        if tick > 5 {
            out.knock = out.knock.max(-(s.velocity - last_v).dot(s.velocity.normalize_or_zero()) / 0.01);
        }
        last_v = s.velocity;
        if !s.wheels.iter().any(|w| w.contact) {
            out.airborne += 0.01;
        }
        if pilot.index + 2 >= line.len() {
            out.through = Some(tick as f32 / 100.0);
            break;
        }
        if s.position.y < track.fall_limit_y {
            break;
        }
    }
    out
}

#[test]
fn canalis_is_driven_in_under_30_s() {
    let track = track::builtin_maps().into_iter().find(|m| m.name == "Canalis").expect("Canalis").build();
    let run = drive(&track, 0.0, true, Some((100.0, 60.0)));
    let t = run.through.expect("the driver gets down the run");
    println!("Canalis: {t:.2} s, highest {:.1} m over the deck, tilt up to {:.0}°", run.highest, run.tilt);
    assert!(t < 30.0, "{t} s");
    assert!(run.tilt < 80.0, "tilted {}°", run.tilt);
}

#[test]
fn the_outer_wall_is_a_line() {
    // Long progressive turns at full speed: a line up the outer wall (40° to 50° of slope) rides
    // it without a knock or leaving it, and is no slower than the middle (the wall turns the car
    // that would otherwise drift).
    let mut l = Layout::new("gutters", Connector::entering((0, 0), 0, Heading::North));
    for id in ["straight3", "curve3_left", "straight1", "curve4_right", "straight2", "curve4_left", "straight3"] {
        l.push(parse_block(id, Some("gutter")).unwrap());
    }
    let track = l.build();
    let middle = drive(&track, 250.0, true, None);
    let wall = drive_line(&track, &outer_line(&track.route, 12.0), 250.0, true, None);
    let (tm, tw) = (middle.through.expect("middle"), wall.through.expect("wall line"));
    println!("middle {tm:.2} s; on the wall {tw:.2} s, {:.1} m high, knock {:.0} m/s², {:.2} s airborne", wall.highest, wall.knock, wall.airborne);
    assert!(wall.highest > 2.5, "the line stays low: {} m", wall.highest);
    assert!(wall.knock < 60.0 && wall.airborne < 0.05, "knock {} m/s², {} s airborne", wall.knock, wall.airborne);
    assert!(tw <= tm, "the wall is slower: {tw} s against {tm} s");
}

#[test]
fn a_car_not_steering_stays_in_the_gutter() {
    // Straights and turns of every kind, the tight ones too: a car going straight meets their
    // outer wall nearly head on.
    let mut l = Layout::new("gutters", Connector::entering((0, 0), 0, Heading::North));
    for id in ["straight3", "curve3_left", "straight1", "curve2_right", "straight2", "turn2_left", "sbend3_right", "turn1_right", "straight3"] {
        l.push(parse_block(id, Some("gutter")).unwrap());
    }
    let track = l.build();
    for kmh in [150.0, 250.0] {
        let run = drive(&track, kmh, false, None);
        println!("{kmh} km/h, no steering: {:?} s, highest {:.1} m over the deck, tilt up to {:.0}°", run.through, run.highest, run.tilt);
        assert!(run.through.is_some(), "{kmh} km/h: stuck or out");
        assert!(run.highest < track::kit::GUTTER_DEPTH + 0.5, "{kmh} km/h: rose {} m, over the lip", run.highest);
        assert!(run.tilt < 130.0, "{kmh} km/h: tilted {}°, on its roof", run.tilt);
    }
}

#[test]
fn mars_cars_feel_no_walls() {
    for p in presets() {
        assert_eq!((p.wall_gravity_deg, p.wall_climb_damp, p.wall_stick), (0.0, 0.0, 0.0), "{}", p.name);
    }
}
