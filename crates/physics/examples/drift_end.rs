//! End of a drift with the steering held: does the body keep turning with its path?
//! Scenarios: (a) full lock held on dirt until the drift fades with speed; (b) the drifting car
//! runs from dirt onto road (grip jumps) with the lock still held.
use glam::Vec3;
use physics::{Car, Input, World, presets, testing};
use track::{Pose, Surface};

fn run(name: &str, world: &World, start: Pose, kmh: f32, ticks: usize) {
    let p = presets().into_iter().find(|p| p.name.to_lowercase().contains(name)).unwrap();
    let mut car = Car::new(p.clone(), world, start);
    for _ in 0..30 { car.step(world, Input::default()); }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    let (mut peak, mut decaying) = (0.0f32, false);
    let (mut min_ratio, mut max_drop, mut reversed) = (f32::INFINITY, 0.0f32, false);
    let mut path_at_peak = 0.0f32;
    let verbose = std::env::var_os("VERBOSE").is_some();
    for tick in 0..ticks {
        car.step(world, Input { steer: 1.0, gas: 1.0, brake: 0.0 });
        if verbose && tick % 2 == 0 && tick > 40 && tick < 140 {
            let t = car.telemetry();
            println!("  {tick:3} v {:5.1} cmd {:+.2} path {:+.2} body {:+.2} ref {:+5.1} ang {:+5.1} usage {:.2} grip {:.1} surf {:?}", t.speed_kmh, car.state.yaw_cmd, car.state.path_rate, t.yaw_rate, car.state.drift_ref.to_degrees(), t.drift_angle_deg, car.state.grip_usage, car.state.grip, t.surface);
        }
        let s = &car.state;
        let a = s.drift_ref.abs();
        let body = car.telemetry().yaw_rate;
        if a > peak { peak = a; path_at_peak = s.path_rate.abs(); }
        if peak > 8f32.to_radians() && a < peak - 1f32.to_radians() { decaying = true; }
        if decaying && a > 1f32.to_radians() {
            // Turning right: path and body rates negative.
            min_ratio = min_ratio.min(-body / (-s.path_rate).max(0.05));
            max_drop = max_drop.max(path_at_peak - s.path_rate.abs());
            reversed |= s.path_rate > 0.0 || body > 0.0;
        }
    }
    println!("{:<14} peak {:5.1}° | while it fades: body/path yaw ≥ {:.2}, path rate dropped by {:.2} rad/s, reversed {}",
        p.name, peak.to_degrees(), min_ratio, max_drop, reversed);
}

fn main() {
    let dirt = World::new(&testing::flat(500.0, Surface::Dirt));
    // Heading -Z, a right turn goes toward +X: road for x >= 0.
    let mixed = World::new(&testing::patches(500.0, Surface::Dirt, Surface::Road));
    let only = std::env::args().nth(1);
    for name in ["fid", "équi", "grip", "drift", "buggy", "basse"] {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        print!("faded  ");
        run(name, &dirt, Pose { position: Vec3::ZERO, yaw: 0.0 }, 140.0, 400);
        print!("onroad ");
        run(name, &mixed, Pose { position: Vec3::new(-12.0, 0.0, 0.0), yaw: std::f32::consts::PI }, 140.0, 300);
    }
}
