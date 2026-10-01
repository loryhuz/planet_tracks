//! Acceptance runs on the game's demo map (« Jezero », from the track crate), driven by a
//! keyboard-style autopilot: full lock or nothing, throttle always held, never braking.

use glam::Vec3;
use physics::{Car, World, presets, testing::Autopilot};
use track::Surface;

struct Run {
    finished: bool,
    /// Lowest speed on the two-level climb after the start, km/h.
    climb_min_kmh: f32,
    /// Speed when the route enters the dirt section, km/h.
    dirt_entry_kmh: f32,
    /// Worst distance from the route centre on the dirt section, m (the road is 20 m wide).
    dirt_worst_offset: f32,
    /// Ticks with a wheel on the off-track ground while on the dirt section.
    dirt_ground_ticks: u32,
    /// Largest drift angle on the dirt section, degrees.
    dirt_max_angle: f32,
}

fn run(p: &physics::CarParams) -> Run {
    let track = track::demo_track();
    let world = World::new(&track.mesh);
    let route = &track.route;
    // The first climb: from where the route starts rising to where it is two levels (16 m) up.
    let base = track.start.position.y;
    let climb_from = route.iter().position(|q| q.y > base + 1.0).expect("climb start");
    let climb_to = climb_from + route[climb_from..].iter().position(|q| q.y > base + 15.0).expect("climb top");
    let dirt: Vec<bool> = route
        .iter()
        .map(|q| world.raycast(*q + Vec3::Y * 3.0, -Vec3::Y, 8.0).is_some_and(|h| h.surface == Surface::Dirt))
        .collect();
    let mut car = Car::new(p.clone(), &world, track.start);
    let mut pilot = Autopilot::new(route);
    let mut out = Run {
        finished: false,
        climb_min_kmh: f32::INFINITY,
        dirt_entry_kmh: f32::NAN,
        dirt_worst_offset: 0.0,
        dirt_ground_ticks: 0,
        dirt_max_angle: 0.0,
    };
    for _ in 0..9000 {
        let input = pilot.input(&car);
        car.step(&world, input);
        let t = car.telemetry();
        let i = pilot.index;
        if (climb_from..=climb_to).contains(&i) {
            out.climb_min_kmh = out.climb_min_kmh.min(t.speed_kmh);
        }
        if dirt[i] {
            if out.dirt_entry_kmh.is_nan() {
                out.dirt_entry_kmh = t.speed_kmh;
            }
            out.dirt_worst_offset = out.dirt_worst_offset.max(pilot.offset(car.state.position));
            out.dirt_max_angle = out.dirt_max_angle.max(t.slip_angle_deg);
            if car.state.wheels.iter().any(|w| w.contact && w.surface == Some(Surface::Ground)) {
                out.dirt_ground_ticks += 1;
            }
        }
        if pilot.finished() {
            out.finished = true;
            break;
        }
        if car.state.position.y < track.fall_limit_y {
            break;
        }
    }
    out
}

#[test]
fn full_throttle_keyboard_run_stays_on_the_dirt_road() {
    for p in [physics::fidele(), physics::equilibre()] {
        let r = run(&p);
        println!(
            "{}: finished {}, dirt entered at {:.0} km/h, worst offset {:.1} m, {} ticks on the ground, drift up to {:.0}°",
            p.name, r.finished, r.dirt_entry_kmh, r.dirt_worst_offset, r.dirt_ground_ticks, r.dirt_max_angle
        );
        assert!(r.finished, "{}: did not finish the map", p.name);
        assert!(r.dirt_entry_kmh > 180.0, "{}: reached the dirt at only {} km/h", p.name, r.dirt_entry_kmh);
        assert!(r.dirt_worst_offset < 8.5, "{}: {} m from the centre of the 20 m dirt road", p.name, r.dirt_worst_offset);
        assert_eq!(r.dirt_ground_ticks, 0, "{}: wheels left the dirt road", p.name);
        assert!(r.dirt_max_angle > 10.0, "{}: no drift at all through the dirt section", p.name);
    }
}

#[test]
fn the_first_climb_is_taken_fast() {
    for p in presets() {
        let r = run(&p);
        println!("{}: lowest speed on the climb {:.0} km/h", p.name, r.climb_min_kmh);
        assert!(r.climb_min_kmh >= 150.0, "{}: the climb drops to {} km/h", p.name, r.climb_min_kmh);
    }
}
