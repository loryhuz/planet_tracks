//! The ice planet's car in a hairpin: what the brake pivot and the exit along the nose do.
//!
//!     cargo run -p physics --release --example skis
//!
//! On flat snow (the test meshes' dirt) and ice (their road), the car comes in at a speed with
//! the throttle held, steers full lock for a while, with or without a stab of brake at the turn-in,
//! then lets go of the steering. Printed: how far the line has turned (the velocity's heading)
//! when the steering lets go and once the car has settled, the speed then, and the largest drift
//! angle. The Mars car ("Combo") is driven the same way for comparison.
use glam::Vec3;
use physics::{Car, CarParams, Input, World, combo, neige, testing};
use track::{Pose, Surface};

/// Heading of the velocity on the ground, degrees, positive to the left.
fn heading(car: &Car) -> f32 {
    let v = car.state.velocity;
    libm::atan2f(v.x, v.z).to_degrees()
}

struct Run {
    at_release: f32,
    settled: f32,
    kmh: f32,
    max_angle: f32,
}

/// `kmh` in, full lock left for `steer_s` with the brake held for the first `brake_s`, then the
/// steering released for 1.5 s; the throttle held throughout (but while braking).
fn hairpin(p: &CarParams, world: &World, kmh: f32, steer_s: f32, brake_s: f32) -> Run {
    let mut car = Car::new(p.clone(), world, Pose { position: Vec3::new(0.0, 0.0, -400.0), yaw: 0.0 });
    for _ in 0..30 {
        car.step(world, Input::default());
    }
    let rot = car.state.rotation;
    car.state.velocity = rot * Vec3::Z * (kmh / 3.6);
    let h0 = heading(&car);
    let mut max_angle: f32 = 0.0;
    let steer_ticks = (steer_s * 100.0) as u32;
    let brake_ticks = (brake_s * 100.0) as u32;
    let mut at_release = 0.0;
    for t in 0..(steer_ticks + 150) {
        let braking = t < brake_ticks;
        let input = Input {
            steer: if t < steer_ticks { -1.0 } else { 0.0 },
            gas: if braking { 0.0 } else { 1.0 },
            brake: if braking { 1.0 } else { 0.0 },
        };
        car.step(world, input);
        max_angle = max_angle.max(car.state.drift_angle.abs().to_degrees());
        if t + 1 == steer_ticks {
            at_release = heading(&car) - h0;
        }
    }
    Run { at_release, settled: heading(&car) - h0, kmh: car.state.velocity.length() * 3.6, max_angle }
}

/// Drift angle every 0.1 s over the first second of a full-lock keyboard turn at `kmh`.
fn onset(p: &CarParams, world: &World, kmh: f32) -> Vec<f32> {
    let mut car = Car::new(p.clone(), world, Pose { position: Vec3::new(0.0, 0.0, -400.0), yaw: 0.0 });
    for _ in 0..30 {
        car.step(world, Input::default());
    }
    let rot = car.state.rotation;
    car.state.velocity = rot * Vec3::Z * (kmh / 3.6);
    let mut out = Vec::new();
    for t in 0..100 {
        car.step(world, Input { steer: -1.0, gas: 1.0, brake: 0.0 });
        if t % 10 == 9 {
            out.push(car.state.drift_angle.to_degrees());
        }
    }
    out
}

/// Speed (km/h) every 2 s over 10 s, full throttle straight on, from `kmh`.
fn straight_on(p: &CarParams, world: &World, kmh: f32) -> Vec<f32> {
    let mut car = Car::new(p.clone(), world, Pose { position: Vec3::new(0.0, 0.0, 0.0), yaw: 0.0 });
    for _ in 0..30 {
        car.step(world, Input::default());
    }
    let rot = car.state.rotation;
    car.state.velocity = rot * Vec3::Z * (kmh / 3.6);
    let mut out = Vec::new();
    for t in 1..=1000 {
        car.step(world, Input { steer: 0.0, gas: 1.0, brake: 0.0 });
        if t % 200 == 0 {
            out.push(car.state.velocity.length() * 3.6);
        }
    }
    out
}

fn main() {
    let snow = World::new(&testing::flat(800.0, Surface::Dirt));
    let ice = World::new(&testing::flat(800.0, Surface::Road));
    // The snow straight on: where the speed goes from below and from above its top speed (the
    // snow before its quadratic drag for comparison: rolling 1.5 m/s², linear drag 0.05/s).
    let long_snow = World::new(&testing::strip(3000.0, Surface::Dirt));
    let mut linear = neige();
    linear.name = "linéaire".into();
    (linear.dirt.rolling, linear.dirt.drag, linear.dirt.drag_quad) = (1.5, 0.05, 0.0);
    let long_powder = World::new(&testing::strip(3000.0, Surface::Ground));
    let long_ice = World::new(&testing::strip(3000.0, Surface::Road));
    println!("full throttle straight on: km/h at 2 / 4 / 6 / 8 / 10 s");
    for (name, p, world) in [("snow before", &linear, &long_snow), ("snow", &neige(), &long_snow), ("powder", &neige(), &long_powder), ("ice", &neige(), &long_ice)] {
        for kmh in [60.0, 100.0, 150.0, 190.0, 220.0] {
            let v: Vec<String> = straight_on(p, world, kmh).iter().map(|x| format!("{x:4.0}")).collect();
            println!("  {name:<11} from {kmh:3.0}: {}", v.join(" "));
        }
    }
    println!("drift angle every 0.1 s, full lock held from 120 km/h:");
    for p in [combo(), neige()] {
        for (name, world) in [("snow", &snow), ("ice", &ice)] {
            let a: Vec<String> = onset(&p, world, 120.0).iter().map(|x| format!("{x:4.0}")).collect();
            println!("  {:<6} {:<5} {}", p.name, name, a.join(" "));
        }
    }
    println!("car    surface  in km/h  steer  brake  | line turned at release / settled | km/h out | drift max");
    for p in [combo(), neige()] {
        for (name, world) in [("snow", &snow), ("ice", &ice)] {
            for kmh in [100.0, 140.0] {
                for (steer_s, brake_s) in [(0.6, 0.0), (0.6, 0.25), (1.0, 0.0), (1.0, 0.25)] {
                    let r = hairpin(&p, world, kmh, steer_s, brake_s);
                    println!(
                        "{:<6} {:<8} {:>7.0} {:>5.1}s {:>5.2}s  | {:>6.1}° / {:>6.1}°{:>19} | {:>8.0} | {:>6.1}°",
                        p.name, name, kmh, steer_s, brake_s, r.at_release, r.settled, "", r.kmh, r.max_angle
                    );
                }
            }
        }
    }
}
