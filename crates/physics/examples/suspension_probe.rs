//! Prints what the suspension and the body do on a landing and in a hard turn, to size the
//! renderer's suspension animation.

use glam::Vec3;
use physics::{Car, Input, World, presets, testing};
use track::{Pose, Surface};

fn roll_pitch(car: &Car) -> (f32, f32) {
    let r = car.state.rotation;
    let left = r * Vec3::X;
    let fwd = r * Vec3::Z;
    (left.y.asin().to_degrees(), fwd.y.asin().to_degrees())
}

fn main() {
    let params = presets().remove(0);
    let world = World::new(&testing::flat(2000.0, Surface::Road));
    println!("preset {}: rest suspension {:.3} m, travel {:.3} m", params.name, params.rest_suspension(), params.suspension_travel);

    println!("\n-- drop from 3 m");
    let mut car = Car::new(params.clone(), &world, Pose { position: Vec3::new(0.0, 3.0, 0.0), yaw: 0.0 });
    for t in 0..150 {
        car.step(&world, Input::default());
        if t % 3 == 0 {
            let w = &car.state.wheels;
            let (roll, pitch) = roll_pitch(&car);
            println!(
                "t={:.2}s susp fl={:.3} rl={:.3} contact={} vy={:.2} roll={roll:.2} pitch={pitch:.2}",
                t as f32 * 0.01,
                w[0].suspension,
                w[2].suspension,
                w[0].contact,
                car.state.velocity.y
            );
        }
    }

    for (label, speed_ticks) in [("120 km/h-ish", 250), ("top speed", 900)] {
        println!("\n-- {label}: straight, then full left lock for 3 s");
        let mut car = Car::new(params.clone(), &world, Pose { position: Vec3::new(0.0, 0.6, 0.0), yaw: 0.0 });
        for _ in 0..speed_ticks {
            car.step(&world, Input { steer: 0.0, gas: 1.0, brake: 0.0 });
        }
        for t in 0..300 {
            car.step(&world, Input { steer: -1.0, gas: 1.0, brake: 0.0 });
            if t % 25 == 0 {
                let w = &car.state.wheels;
                let a = car.state.rotation.inverse() * car.state.acceleration;
                let (roll, pitch) = roll_pitch(&car);
                println!(
                    "t={:.2}s v={:.0} km/h lat={:.1} m/s² roll={roll:.2}° pitch={pitch:.2}° susp L={:.3} R={:.3} drift={:.2}",
                    t as f32 * 0.01,
                    car.state.velocity.length() * 3.6,
                    a.x,
                    w[0].suspension,
                    w[1].suspension,
                    car.state.drift
                );
            }
        }
        println!("-- then full brake");
        for t in 0..120 {
            car.step(&world, Input { steer: 0.0, gas: 0.0, brake: 1.0 });
            if t % 20 == 0 {
                let a = car.state.rotation.inverse() * car.state.acceleration;
                let (roll, pitch) = roll_pitch(&car);
                println!(
                    "t={:.2}s v={:.0} km/h long={:.1} m/s² roll={roll:.2}° pitch={pitch:.2}° susp F={:.3} R={:.3}",
                    t as f32 * 0.01,
                    car.state.velocity.length() * 3.6,
                    a.z,
                    car.state.wheels[0].suspension,
                    car.state.wheels[2].suspension
                );
            }
        }
    }
}
