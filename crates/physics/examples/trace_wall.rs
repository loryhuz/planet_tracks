//! Per-tick trace of a wall hit, for tuning.
//!
//!     cargo run -p physics --release --example trace_wall -- <profile> <incidence deg> <kmh> [gas]
use glam::Vec3;
use physics::{Car, Input, World, presets, testing};
use track::Pose;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or("Fidèle".into()).to_lowercase();
    let inc: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(45.0);
    let kmh: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(150.0);
    let gas: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let p = presets().into_iter().find(|p| p.name.to_lowercase().contains(&name)).expect("profile");
    let world = World::new(&testing::wall_lane(10.0));
    let mut car = Car::new(p, &world, Pose { position: Vec3::new(7.0, 0.0, 0.0), yaw: inc.to_radians() });
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    for tick in 0..60 {
        car.step(&world, Input { gas, ..Default::default() });
        let t = car.telemetry();
        let v = car.state.velocity;
        let head = (car.state.rotation * Vec3::Z).x.atan2((car.state.rotation * Vec3::Z).z).to_degrees();
        println!(
            "{tick:3} v {:6.1} vx {:6.2} vz {:6.2} head {:6.1} yaw {:6.2} slip {:5.1} wall {} imp {:5.2} x {:5.2} drift {:.2} up.y {:.3}",
            t.speed_kmh, v.x, v.z, head, t.yaw_rate, t.slip_angle_deg, car.state.wall_contact as u8, car.state.impact,
            car.state.position.x, t.drift, (car.state.rotation * Vec3::Y).y
        );
    }
}
