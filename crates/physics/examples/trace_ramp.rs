//! Per-tick trace of the 15° ramp jump at 150 km/h, for tuning.
//!
//!     cargo run -p physics --release --example trace_ramp -- <profile> [every]
use glam::Vec3;
use physics::{Car, Input, World, presets, testing};
use track::Pose;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or("Fidèle".into()).to_lowercase();
    let every: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let p = presets().into_iter().find(|p| p.name.to_lowercase().contains(&name)).expect("profile");
    let (mesh, _) = testing::ramp(15.0, 10.0);
    let world = World::new(&mesh);
    let mut car = Car::new(p, &world, Pose { position: Vec3::new(0.0, 0.0, -90.0), yaw: 0.0 });
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (150.0 / 3.6);
    for tick in 0..900 {
        let on_ramp = car.state.position.z > -12.0;
        let speed = car.state.velocity.length();
        let gas = if on_ramp { 1.0 } else { ((150.0 / 3.6 - speed) * 0.6 + 0.1).clamp(0.0, 1.0) };
        car.step(&world, Input { gas, ..Default::default() });
        let wheels = car.state.wheels.iter().filter(|w| w.contact).count();
        let up = car.state.rotation * Vec3::Y;
        let fwd = car.state.rotation * Vec3::Z;
        if tick % every == 0 || car.state.impact > 0.5 {
            println!(
                "{tick:4} z {:7.1} y {:6.2} vy {:6.2} v {:6.1} wheels {wheels} up.y {:.3} pitch {:6.1} w {:5.2} {:5.2} {:5.2} imp {:5.2}",
                car.state.position.z, car.state.position.y, car.state.velocity.y, speed * 3.6, up.y,
                fwd.y.asin().to_degrees(), car.state.angular_velocity.x, car.state.angular_velocity.y, car.state.angular_velocity.z,
                car.state.impact
            );
        }
    }
}
