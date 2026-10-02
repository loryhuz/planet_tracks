//! Acceptance runs on « Noctis », the technical map (hairpins on road, on a butte 16 m up and on
//! dirt), driven by the keyboard-style autopilot. A driver who lifts and brakes for its hairpins
//! finishes in under 30 s without falling or leaving the track; one who never brakes does not
//! get round: that is what makes it technical. Its dirt is smooth: wide of the centreline too,
//! nothing on it knocks the body or throws the car in the air.

use glam::Vec3;
use physics::{Car, World, presets, testing::Autopilot};
use track::{Surface, Track};

struct Run {
    /// Time to the finish, every checkpoint crossed, seconds.
    finished: Option<f32>,
    /// Lowest `up.y` of the body: 1 level, 0 on its side.
    worst_up: f32,
    /// Ticks with a wheel on off-track ground.
    ground_ticks: u32,
    /// Whether the car dropped well below the route (off an elevated deck).
    fell: bool,
    /// Body impacts as the game counts them for its sound (rising past 2 m/s) and airborne
    /// ticks, while the driver is on the dirt section.
    dirt_impacts: u32,
    dirt_air_ticks: u32,
}

fn noctis() -> Track {
    track::builtin_maps().into_iter().find(|m| m.name == "Noctis").expect("Noctis").build()
}

/// `bends`: the lateral accelerations the driver allows itself on road and on dirt, or `None`
/// for one who never brakes. `offset`: how far left of the centreline the line it follows runs
/// on the dirt, metres (it eases back to the centreline over 20 m on the roads either side,
/// whose borders it would otherwise ride along).
fn run(p: &physics::CarParams, track: &Track, world: &World, bends: Option<(f32, f32)>, offset: f32) -> Run {
    let r = &track.route;
    let dirt: Vec<bool> = r.iter().map(|&q| world.raycast(q + Vec3::Y * 3.0, -Vec3::Y, 8.0).is_some_and(|h| h.surface == Surface::Dirt)).collect();
    // Route points are about 4 m apart: within 5 of the dirt, the offset eases in.
    let line: Vec<Vec3> = (0..r.len())
        .map(|i| {
            let (a, b) = (r[i.saturating_sub(1)], r[(i + 1).min(r.len() - 1)]);
            let d = Vec3::new(b.x - a.x, 0.0, b.z - a.z).normalize_or_zero();
            let reach = (0..=5).find(|&k| dirt[i.saturating_sub(k)] || dirt[(i + k).min(r.len() - 1)]);
            let weight = reach.map_or(0.0, |k| 1.0 - k as f32 / 6.0);
            r[i] + Vec3::new(d.z, 0.0, -d.x) * (offset * weight)
        })
        .collect();
    let mut car = Car::new(p.clone(), world, track.start);
    let mut pilot = Autopilot::new(&line);
    if let Some((road, dirt)) = bends {
        pilot = pilot.braking(world, road, dirt);
    }
    let mut crossed = vec![false; track.checkpoints.len()];
    let mut out = Run { finished: None, worst_up: 1.0, ground_ticks: 0, fell: false, dirt_impacts: 0, dirt_air_ticks: 0 };
    let mut last_impact = 0.0;
    for tick in 0..4000 {
        let input = pilot.input(&car);
        car.step(world, input);
        let pos = car.state.position;
        let t = car.telemetry();
        if dirt[pilot.index] {
            if t.impact > 2.0 && last_impact <= 2.0 {
                out.dirt_impacts += 1;
            }
            if t.airborne {
                out.dirt_air_ticks += 1;
            }
        }
        last_impact = t.impact;
        out.worst_up = out.worst_up.min((car.state.rotation * Vec3::Y).y);
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
fn a_braking_driver_finishes_under_30_seconds() {
    let track = noctis();
    let world = World::new(&track.mesh);
    // Brisk and careful: the second brakes to a lower speed on dirt.
    for (bends, label) in [((150.0, 100.0), "brisk"), ((150.0, 60.0), "careful")] {
        for p in presets() {
            let r = run(&p, &track, &world, Some(bends), 0.0);
            println!("{} ({label}): {:?} s, worst up {:.2}, {} ticks on the ground, fell {}", p.name, r.finished, r.worst_up, r.ground_ticks, r.fell);
            assert!(!r.fell, "{} ({label}): fell off", p.name);
            let t = r.finished.unwrap_or_else(|| panic!("{} ({label}): did not finish", p.name));
            assert!(t < 30.0, "{} ({label}): {t} s", p.name);
            assert!(r.worst_up > 0.6, "{} ({label}): tipped over (up.y {})", p.name, r.worst_up);
            assert_eq!(r.ground_ticks, 0, "{} ({label}): left the track", p.name);
            assert_eq!((r.dirt_impacts, r.dirt_air_ticks), (0, 0), "{} ({label}): knocked about on the dirt", p.name);
        }
    }
}

#[test]
fn the_dirt_is_smooth_wide_of_the_centreline_too() {
    // A player drifts wide and cuts the apexes: 4 m either side of the centreline through the
    // dirt hairpins, the body never hits the ground and the wheels stay on it.
    let track = noctis();
    let world = World::new(&track.mesh);
    for p in presets() {
        for offset in [-4.0, 4.0] {
            let r = run(&p, &track, &world, Some((150.0, 100.0)), offset);
            println!("{} ({offset} m): {} impacts, {} ticks in the air on the dirt", p.name, r.dirt_impacts, r.dirt_air_ticks);
            assert!(r.finished.is_some(), "{} ({offset} m): did not finish", p.name);
            assert_eq!((r.dirt_impacts, r.dirt_air_ticks), (0, 0), "{} ({offset} m): knocked about on the dirt", p.name);
        }
    }
}

#[test]
fn flat_out_does_not_get_round() {
    let track = noctis();
    let world = World::new(&track.mesh);
    for p in presets() {
        let r = run(&p, &track, &world, None, 0.0);
        println!("{}: {:?} s, {} ticks on the ground, fell {}", p.name, r.finished, r.ground_ticks, r.fell);
        assert!(r.finished.is_none_or(|t| t > 30.0) || r.ground_ticks > 100, "{}: flat out gets round cleanly", p.name);
    }
}
