//! Prints a per-tick trace of one manoeuvre, for tuning.
//!
//!     cargo run -p physics --release --example trace -- <profile> <kmh> <surface road|dirt> <steer> [ticks]
//!
//! The speed is held with the throttle unless GAS=<0..1> is set; RELEASE=<tick> centres the
//! steering from that tick on.
use glam::Vec3;
use physics::{Car, Input, World, presets, testing};
use track::{Pose, Surface};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or("Fidèle".into()).to_lowercase();
    let kmh: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200.0);
    let surface = if args.get(3).map(|s| s.as_str()) == Some("dirt") { Surface::Dirt } else { Surface::Road };
    let steer: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let ticks: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(150);
    let p = presets().into_iter().find(|p| p.name.to_lowercase().contains(&name)).expect("profile");
    let world = World::new(&testing::flat(420.0, surface));
    let mut car = Car::new(p, &world, Pose { position: Vec3::ZERO, yaw: 0.0 });
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    for tick in 0..ticks {
        let speed = car.state.velocity.length();
        let gas = match std::env::var("GAS").ok().and_then(|s| s.parse::<f32>().ok()) {
            Some(g) => g,
            None => ((kmh / 3.6 - speed) * 0.6 + 0.1).clamp(0.0, 1.0),
        };
        let release: usize = std::env::var("RELEASE").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
        let steer = if tick >= release { 0.0 } else { steer };
        car.step(&world, Input { steer, gas, brake: 0.0 });
        let t = car.telemetry();
        let w = &car.state.wheels;
        println!(
            "{tick:4} cmd {:5.2} path {:5.2} ref {:5.1} ang {:5.1} grip {:4.1} v {:6.1} yaw {:6.2} slip {:5.1} latg {:5.1} drift {:.2} susp {:.3} {:.3} {:.3} {:.3} load {:.2} {:.2} {:.2} {:.2} gas {:.2} up.y {:.3} pitch {:5.1} roll {:5.1}",
            car.state.yaw_cmd, car.state.path_rate, car.state.drift_ref.to_degrees(), car.state.drift_angle.to_degrees(), car.state.grip,
            t.speed_kmh, t.yaw_rate, t.slip_angle_deg, t.lateral_g, t.drift,
            w[0].suspension, w[1].suspension, w[2].suspension, w[3].suspension,
            w[0].load, w[1].load, w[2].load, w[3].load, gas, (car.state.rotation * Vec3::Y).y,
            (car.state.rotation * Vec3::Z).y.asin().to_degrees(), (car.state.rotation * Vec3::X).y.asin().to_degrees()
        );
    }
}
