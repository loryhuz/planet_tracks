//! Acceptance runs on « Ares Vallis », the all-dirt map whose corridors are dug into the terrain
//! (see the track crate's `dirt` module), driven by the keyboard-style autopilot.

use glam::Vec3;
use physics::{Car, World, presets, testing::Autopilot};
use track::Surface;

struct Run {
    finished: bool,
    seconds: f32,
    /// Lowest `up.y` of the body: 1 level, 0 on its side.
    worst_up: f32,
    /// Ticks with a wheel on off-track ground.
    ground_ticks: u32,
    /// Worst distance from the route centre, m.
    worst_offset: f32,
}

fn run(p: &physics::CarParams) -> Run {
    let map = track::builtin_maps().into_iter().find(|m| m.name == "Ares Vallis").expect("Ares Vallis");
    let track = map.build();
    let world = World::new(&track.mesh);
    let mut car = Car::new(p.clone(), &world, track.start);
    let mut pilot = Autopilot::new(&track.route);
    let mut out = Run { finished: false, seconds: 0.0, worst_up: 1.0, ground_ticks: 0, worst_offset: 0.0 };
    for tick in 0..9000 {
        let input = pilot.input(&car);
        car.step(&world, input);
        out.worst_up = out.worst_up.min((car.state.rotation * Vec3::Y).y);
        out.worst_offset = out.worst_offset.max(pilot.offset(car.state.position));
        if car.state.wheels.iter().any(|w| w.contact && w.surface == Some(Surface::Ground)) {
            out.ground_ticks += 1;
            if std::env::var("DIRT_DEBUG").is_ok() {
                println!("ground at tick {tick} pos {:?} route index {} offset {:.1}", car.state.position, pilot.index, pilot.offset(car.state.position));
            }
        }
        if pilot.finished() {
            out.finished = true;
            out.seconds = tick as f32 / 100.0;
            break;
        }
        if car.state.position.y < track.fall_limit_y {
            break;
        }
    }
    out
}

#[test]
fn every_profile_drives_the_dirt_map_cleanly() {
    for p in presets() {
        let r = run(&p);
        println!(
            "{}: finished {} in {:.1} s, worst up {:.2}, {} ticks on the ground, worst offset {:.1} m",
            p.name, r.finished, r.seconds, r.worst_up, r.ground_ticks, r.worst_offset
        );
        assert!(r.finished, "{}: did not finish", p.name);
        assert!(r.worst_up > 0.6, "{}: tipped over (up.y {})", p.name, r.worst_up);
        assert_eq!(r.ground_ticks, 0, "{}: wheels left the dirt", p.name);
    }
}
