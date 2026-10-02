//! Acceptance runs on « Marineris », the first circuit of the hard series: a roller coaster from
//! a start 80 m up, with boosters, a kicker and 90° turns on scaffolding up to 48 m high. A
//! driver who lifts and brakes for its turns finishes in 45 s to a minute, without falling,
//! leaving the track or being knocked about (every crest and sag fits the speed it is taken at);
//! its kicker gives real air; its boosters push the car past the engine's top speed. One who
//! never brakes does not get round: that is what makes it hard.

use glam::Vec3;
use physics::{Car, World, presets, testing::Autopilot};
use track::kit::Kind;
use track::{Surface, Track};

struct Run {
    /// Time to the finish, every checkpoint crossed, seconds.
    finished: Option<f32>,
    /// Whether the car dropped well below the route (off an elevated deck).
    fell: bool,
    /// Ticks with a wheel on off-track ground.
    ground_ticks: u32,
    /// Lowest `up.y` of the body: 1 level, 0 on its side.
    worst_up: f32,
    /// Body impacts as the game counts them for its sound (rising past 2 m/s), and the hardest.
    impacts: u32,
    worst_impact: f32,
    /// The flight off the kicker: how long (seconds) and the highest the car's centre rose above
    /// the lip (metres).
    flight: f32,
    above_lip: f32,
    top_kmh: f32,
    /// Ticks with a boost under way.
    boost_ticks: u32,
}

/// The track and its kicker's lip.
fn marineris() -> (Track, Vec3) {
    let map = track::builtin_maps().into_iter().find(|m| m.name == "Marineris").expect("Marineris");
    let layout = map.layout().expect("layout");
    let kicker = layout.pieces.iter().find(|p| matches!(p.piece.kind, Kind::JumpRamp { .. })).expect("a kicker");
    (map.build(), kicker.exit.pos)
}

/// `bends`: the lateral accelerations the driver allows itself on road and on dirt, or `None`
/// for one who never brakes.
fn run(p: &physics::CarParams, track: &Track, lip: Vec3, world: &World, bends: Option<(f32, f32)>) -> Run {
    let mut car = Car::new(p.clone(), world, track.start);
    let mut pilot = Autopilot::new(&track.route);
    if let Some((road, dirt)) = bends {
        pilot = pilot.braking(world, road, dirt);
    }
    let mut crossed = vec![false; track.checkpoints.len()];
    let mut out = Run {
        finished: None,
        fell: false,
        ground_ticks: 0,
        worst_up: 1.0,
        impacts: 0,
        worst_impact: 0.0,
        flight: 0.0,
        above_lip: f32::MIN,
        top_kmh: 0.0,
        boost_ticks: 0,
    };
    // Ticks in the air, the highest point, and whether the flight began at the lip.
    let (mut last_impact, mut airborne, mut peak, mut off_lip) = (0.0, 0u32, f32::MIN, false);
    for tick in 0..8000 {
        let input = pilot.input(&car);
        car.step(world, input);
        let pos = car.state.position;
        let t = car.telemetry();
        if t.impact > 2.0 && last_impact <= 2.0 {
            out.impacts += 1;
        }
        last_impact = t.impact;
        out.worst_impact = out.worst_impact.max(t.impact);
        out.worst_up = out.worst_up.min((car.state.rotation * Vec3::Y).y);
        out.top_kmh = out.top_kmh.max(t.speed_kmh);
        if t.boost > 0.0 {
            out.boost_ticks += 1;
        }
        if t.airborne {
            if airborne == 0 {
                off_lip = Vec3::new(pos.x - lip.x, 0.0, pos.z - lip.z).length() < 15.0;
            }
            airborne += 1;
            peak = peak.max(pos.y);
        } else {
            if off_lip && airborne as f32 / 100.0 > out.flight {
                out.flight = airborne as f32 / 100.0;
                out.above_lip = peak - lip.y;
            }
            airborne = 0;
            peak = f32::MIN;
        }
        if car.state.wheels.iter().any(|w| w.contact && w.surface == Some(Surface::Ground)) {
            out.ground_ticks += 1;
        }
        for (c, done) in track.checkpoints.iter().zip(crossed.iter_mut()) {
            *done |= c.contains(pos);
        }
        if track.finish.contains(pos) && crossed.iter().all(|&c| c) {
            out.finished = Some((tick + 1) as f32 / 100.0);
            break;
        }
        if pos.y < track.route[pilot.index].y - 7.0 {
            out.fell = true;
            break;
        }
    }
    out
}

#[test]
fn a_braking_driver_finishes_in_45_seconds_to_a_minute() {
    let (track, lip) = marineris();
    let world = World::new(&track.mesh);
    // Brisk, and careful (it allows itself less in the bends).
    for (bends, label) in [((150.0, 100.0), "brisk"), ((110.0, 80.0), "careful")] {
        for p in presets() {
            let r = run(&p, &track, lip, &world, Some(bends));
            println!(
                "{} ({label}): {:?} s, top {:.0} km/h, {} impacts (worst {:.1} m/s), longest flight {:.2} s {:.1} m over the lip, {} ticks boosted, worst up {:.2}, {} ticks on the ground, fell {}",
                p.name, r.finished, r.top_kmh, r.impacts, r.worst_impact, r.flight, r.above_lip, r.boost_ticks, r.worst_up, r.ground_ticks, r.fell
            );
            assert!(!r.fell, "{} ({label}): fell off", p.name);
            let t = r.finished.unwrap_or_else(|| panic!("{} ({label}): did not finish", p.name));
            assert!((45.0..60.0).contains(&t), "{} ({label}): {t} s", p.name);
            assert!(r.worst_up > 0.6, "{} ({label}): tipped over (up.y {})", p.name, r.worst_up);
            assert_eq!(r.ground_ticks, 0, "{} ({label}): left the track", p.name);
            assert!(r.worst_impact < 12.0 && r.impacts <= 8, "{} ({label}): knocked about ({} impacts, worst {} m/s)", p.name, r.impacts, r.worst_impact);
        }
    }
}

#[test]
fn its_kicker_gives_real_air_and_its_boosters_push_past_top_speed() {
    let (track, lip) = marineris();
    let world = World::new(&track.mesh);
    for p in presets() {
        let r = run(&p, &track, lip, &world, Some((150.0, 100.0)));
        println!("{}: flight {:.2} s, {:.1} m over the lip, top {:.0} km/h, {} ticks boosted", p.name, r.flight, r.above_lip, r.top_kmh, r.boost_ticks);
        // The braking driver reaches the kicker at about 240 km/h, out of the chicane: a short
        // flight (a player flat out flies a second, see the next test).
        assert!(r.flight >= 0.3, "{}: the kicker's flight lasts {} s", p.name, r.flight);
        assert!(r.above_lip >= 5.0, "{}: the car rises {} m over the kicker's lip", p.name, r.above_lip);
        assert!(r.top_kmh > p.top_speed_kmh + 15.0, "{}: top speed {} km/h, the boosters hardly push", p.name, r.top_kmh);
        assert!(r.boost_ticks > 1000, "{}: boosted for {} ticks only", p.name, r.boost_ticks);
    }
}

#[test]
fn flat_out_does_not_get_round_cleanly() {
    // Without braking, the turns at height throw the car off or into their bumpers again and
    // again.
    let (track, lip) = marineris();
    let world = World::new(&track.mesh);
    for p in presets() {
        let r = run(&p, &track, lip, &world, None);
        println!("{}: {:?} s, {} ticks on the ground, fell {}, {} impacts", p.name, r.finished, r.ground_ticks, r.fell, r.impacts);
        assert!(r.finished.is_none() || r.fell || r.ground_ticks > 100 || r.impacts >= 12, "{}: flat out gets round cleanly", p.name);
    }
}

/// Each flight from the booster before the kicker to the end of its landing hill, for a car
/// thrown at the booster at `kmh` with the throttle held: where it took off (metres before the
/// lip, along the ramp's axis), how long it flew, how high it rose over the lip, and the hardest
/// impact after it.
fn kicker_flights(p: &physics::CarParams, kmh: f32) -> Vec<(f32, f32, f32, f32)> {
    let map = track::builtin_maps().into_iter().find(|m| m.name == "Marineris").expect("Marineris");
    let layout = map.layout().expect("layout");
    let k = layout.pieces.iter().position(|p| matches!(p.piece.kind, Kind::JumpRamp { .. })).expect("a kicker");
    let (booster, kicker, landing) = (&layout.pieces[k - 1], &layout.pieces[k], &layout.pieces[k + 1]);
    assert!(booster.piece.boost, "a booster just before the kicker");
    let lip = kicker.exit.pos;
    let axis = kicker.exit.heading.forward();
    let end = landing.exit.pos;
    let track = map.build();
    let world = World::new(&track.mesh);
    let at = booster.frame(2.0);
    let mut car = Car::new(p.clone(), &world, track::Pose { position: at.centre(), yaw: at.yaw });
    car.state.velocity = at.forward * (kmh / 3.6);
    let mut pilot = Autopilot::new(&track.route);
    pilot.index = (0..track.route.len()).min_by(|&a, &b| track.route[a].distance(at.centre()).total_cmp(&track.route[b].distance(at.centre()))).unwrap();
    let mut flights: Vec<(f32, f32, f32, f32)> = Vec::new();
    let (mut airborne, mut from, mut peak) = (0u32, 0.0f32, f32::MIN);
    for _ in 0..600 {
        let input = pilot.input(&car);
        car.step(&world, input);
        let pos = car.state.position;
        let t = car.telemetry();
        if t.airborne {
            if airborne == 0 {
                from = (lip - pos).dot(axis);
            }
            airborne += 1;
            peak = peak.max(pos.y);
        } else if airborne > 0 {
            flights.push((from, airborne as f32 / 100.0, peak - lip.y, 0.0f32));
            (airborne, peak) = (0, f32::MIN);
        }
        if let Some(last) = flights.last_mut().filter(|_| airborne == 0) {
            last.3 = last.3.max(t.impact);
        }
        if (pos - end).dot(axis) > 0.0 {
            break;
        }
    }
    flights
}

#[test]
fn the_kicker_takes_one_clean_flight_from_260_to_340_kmh() {
    // A player takes the chicane flat out and the booster after it: up to about 330 km/h at the
    // kicker. Nothing before the lip throws the car (no climb before it), and the 8-cell landing
    // hill catches every flight: the faster, the longer and higher. After the longest the car
    // floats a moment over the hill's far side, with no impact.
    for p in presets() {
        for kmh in [260.0, 280.0, 300.0, 320.0, 340.0] {
            let flights = kicker_flights(&p, kmh);
            println!("{} at {kmh} km/h: {:?}", p.name, flights.iter().map(|f| format!("off {:.1} m before the lip, {:.2} s, {:.1} m up, impact {:.1} m/s", f.0, f.1, f.2, f.3)).collect::<Vec<_>>());
            assert!(flights.iter().all(|f| f.0 < 6.0 || f.1 < 0.05), "{} at {kmh} km/h: thrown before the lip", p.name);
            let jump = flights.iter().find(|f| f.0.abs() < 6.0).unwrap_or_else(|| panic!("{} at {kmh} km/h: no flight off the lip", p.name));
            let (_, time, up, _) = *jump;
            for f in &flights {
                assert!(f.3 < 10.0, "{} at {kmh} km/h: landed at {} m/s", p.name, f.3);
                if f.0 < -6.0 {
                    assert!(f.1 < 0.5, "{} at {kmh} km/h: thrown again {} m past the lip for {} s", p.name, -f.0, f.1);
                }
            }
            assert!(time >= 0.6, "{} at {kmh} km/h: {time} s in the air", p.name);
            if kmh >= 300.0 {
                assert!(time >= 0.9 && up >= 12.0, "{} at {kmh} km/h: {time} s, {up} m over the lip", p.name);
            }
        }
    }
}
