//! Full-throttle keyboard autopilot over the game's demo map: speeds, climb, dirt section.
//!
//!     cargo run -p physics --release --example demo_run -- [profile]
use glam::Vec3;
use physics::{Car, World, presets, testing::Autopilot};
use track::Surface;

fn main() {
    let filter = std::env::args().nth(1).map(|s| s.to_lowercase());
    let track = track::demo_track();
    let world = World::new(&track.mesh);
    for p in presets() {
        if filter.as_ref().is_some_and(|f| !p.name.to_lowercase().contains(f.as_str())) {
            continue;
        }
        let mut p = p.clone();
        if std::env::var_os("NO_DRIFT_TURN").is_some() {
            p.drift_turn = 0.0;
        }
        if std::env::var_os("OLD_BUILD").is_some() {
            // The previous build: snow-car engine, 0.75 slope gravity, no tighter drift turning,
            // dirt turning 10 % sharper than road and off-track 15 % less.
            p.accel_steps = [31.0, 18.85, 10.0, 6.0, 3.5, 2.0];
            p.accel_speeds = [55.0, 80.0, 115.0, 140.0, 240.0];
            p.top_speed_kmh = 280.0;
            p.slope_gravity = 0.75;
            p.drift_turn = 0.0;
            p.drift_turn_cost = 0.0;
            p.dirt.yaw = 1.1;
            p.ground.yaw = 0.85;
        }
        let mut car = Car::new(p.clone(), &world, track.start);
        let mut lock_ticks = 0;
        let mut dirt_ticks = 0;
        let mut pilot = Autopilot::new(&track.route);
        let (mut worst_dirt, mut worst_all, mut ground_ticks_dirt) = (0.0f32, 0.0f32, 0);
        let (mut climb_min, mut dirt_entry, mut max_angle) = (f32::INFINITY, None, 0.0f32);
        let mut tick = 0;
        let mut last_print = 0;
        while !pilot.finished() && tick < 9000 {
            let input = pilot.input(&car);
            car.step(&world, input);
            tick += 1;
            let t = car.telemetry();
            let off = pilot.offset(car.state.position);
            worst_all = worst_all.max(off);
            // Surface under the route point: is this the dirt section?
            let q = track.route[pilot.index];
            let on_dirt_section = world.raycast(q + Vec3::Y * 3.0, -Vec3::Y, 8.0).is_some_and(|h| h.surface == Surface::Dirt);
            if on_dirt_section {
                dirt_ticks += 1;
                if input.steer != 0.0 {
                    lock_ticks += 1;
                }
                dirt_entry.get_or_insert(t.speed_kmh);
                worst_dirt = worst_dirt.max(off);
                max_angle = max_angle.max(t.slip_angle_deg);
                if car.state.wheels.iter().any(|w| w.contact && w.surface == Some(Surface::Ground)) {
                    ground_ticks_dirt += 1;
                }
            }
            // The 2-level climb: route points 16 m (two levels) above the start, before the S-bends.
            if pilot.index > 10 && q.y > track.start.position.y + 2.0 && q.y < track.start.position.y + 15.0 && dirt_entry.is_none() && pilot.index < 120 {
                climb_min = climb_min.min(t.speed_kmh);
            }
            if std::env::var_os("DETAIL").is_some() && on_dirt_section && off > 5.0 {
                println!("  t {:5.2} idx {:4} v {:5.0} off {:4.1} steer {:+.0} cmd {:+.2} path {:+.2} ref {:+5.1} usage {:.2} grip {:.1} surf {:?}", tick as f32 * 0.01, pilot.index, t.speed_kmh, off, input.steer, car.state.yaw_cmd, car.state.path_rate, car.state.drift_ref.to_degrees(), car.state.grip_usage, car.state.grip, t.surface);
            }
            if std::env::var_os("VERBOSE").is_some() && tick - last_print >= 25 {
                last_print = tick;
                println!("  t {:5.2} idx {:4} v {:5.0} off {:4.1} y {:5.1} slip {:4.1} surf {:?}", tick as f32 * 0.01, pilot.index, t.speed_kmh, off, car.state.position.y, t.slip_angle_deg, t.surface);
            }
            if car.state.position.y < track.fall_limit_y {
                println!("  fell at route {}", pilot.index);
                break;
            }
        }
        println!(
            "{:<14} {} in {:.2} s | climb min {:.0} km/h | dirt: entry {:.0} km/h, worst offset {:.1} m, wheels on ground {} ticks, angle max {:.0}°, full lock {:.0} % of the time | worst offset overall {:.1} m",
            p.name,
            if pilot.finished() { "finished" } else { "NOT finished" },
            tick as f32 * 0.01,
            climb_min,
            dirt_entry.unwrap_or(f32::NAN),
            worst_dirt,
            ground_ticks_dirt,
            max_angle,
            100.0 * lock_ticks as f32 / dirt_ticks.max(1) as f32,
            worst_all
        );
    }
}
