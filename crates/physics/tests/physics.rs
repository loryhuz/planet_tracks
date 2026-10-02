//! Behaviour tests of the vehicle physics, on the crate's own test meshes.

use glam::Vec3;
use physics::{Car, CarParams, CarState, DT, Input, World, presets, testing};
use track::{Pose, Surface, TrackMesh};

/// Small deterministic generator for input sequences.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Keyboard-like input: digital steering, gas mostly held, brake sometimes.
    fn input(&mut self) -> Input {
        let s = self.next();
        let steer = if s < 0.33 { -1.0 } else if s < 0.66 { 0.0 } else { 1.0 };
        let gas = if self.next() < 0.8 { 1.0 } else { 0.0 };
        let brake = if self.next() < 0.1 { 1.0 } else { 0.0 };
        Input { steer, gas, brake }
    }
}

fn pose(x: f32, y: f32, z: f32, yaw: f32) -> Pose {
    Pose { position: Vec3::new(x, y, z), yaw }
}

fn finite(s: &CarState) -> bool {
    let v = |v: Vec3| v.is_finite();
    v(s.position)
        && s.rotation.is_finite()
        && v(s.velocity)
        && v(s.angular_velocity)
        && v(s.acceleration)
        && s.steer.is_finite()
        && s.drift.is_finite()
        && s.engine.is_finite()
        && s.wheels.iter().all(|w| {
            w.suspension.is_finite() && w.steer.is_finite() && w.spin.is_finite() && w.slip.is_finite() && w.load.is_finite()
        })
}

/// Runs `ticks` of pseudo-random driving, holding the inputs a few ticks like a player would.
fn random_run(params: &CarParams, world: &World, mesh_start: Pose, seed: u64, ticks: usize) -> CarState {
    let mut car = Car::new(params.clone(), world, mesh_start);
    let mut rng = Rng(seed);
    let mut input = rng.input();
    for tick in 0..ticks {
        if tick % 17 == 0 {
            input = rng.input();
        }
        car.step(world, input);
    }
    car.state
}

#[test]
fn determinism_same_inputs_same_bits() {
    let world = World::new(&testing::playground());
    for p in presets() {
        let a = random_run(&p, &world, pose(0.0, 1.0, -100.0, 0.0), 42, 3000);
        let b = random_run(&p, &world, pose(0.0, 1.0, -100.0, 0.0), 42, 3000);
        assert_eq!(a, b, "{}: two identical runs differ", p.name);
        assert_eq!(a.hash(), b.hash());
        println!("determinism {:<14} hash {:016x}", p.name, a.hash());
    }
}

#[test]
fn restoring_a_cloned_state_replays_identically() {
    let world = World::new(&testing::playground());
    let p = presets().remove(0);
    let mut car = Car::new(p, &world, pose(0.0, 1.0, -100.0, 0.0));
    let mut rng = Rng(7);
    let inputs: Vec<Input> = (0..1500).map(|_| rng.input()).collect();
    for input in &inputs[..700] {
        car.step(&world, *input);
    }
    let saved = car.state.clone();
    for input in &inputs[700..] {
        car.step(&world, *input);
    }
    let first = car.state.clone();
    car.state = saved;
    for input in &inputs[700..] {
        car.step(&world, *input);
    }
    assert_eq!(first, car.state);
}

#[test]
fn no_nan_over_long_random_runs() {
    let world = World::new(&testing::playground());
    for (k, p) in presets().into_iter().enumerate() {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 1.0, -100.0, 0.0));
        let mut rng = Rng(1234 + k as u64);
        let mut input = rng.input();
        for tick in 0..20_000 {
            if tick % 13 == 0 {
                input = rng.input();
            }
            // Occasionally hand the physics garbage input: it must be sanitised.
            let fed = if tick % 997 == 0 { Input { steer: f32::NAN, gas: f32::INFINITY, brake: -3.0 } } else { input };
            car.step(&world, fed);
            assert!(finite(&car.state), "{}: non-finite state at tick {tick}", p.name);
            if car.state.position.y < -20.0 || car.state.position.length() > 1000.0 {
                car.respawn(&world, pose(0.0, 1.0, -100.0, 0.0));
            }
        }
    }
}

#[test]
fn a_car_at_rest_stays_at_rest() {
    let world = World::new(&testing::flat(200.0, Surface::Road));
    for p in presets() {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.3));
        for _ in 0..100 {
            car.step(&world, Input::default());
        }
        let start = car.state.position;
        for _ in 0..1000 {
            car.step(&world, Input::default());
        }
        let moved = (car.state.position - start).length();
        assert!(moved < 1e-3, "{}: drifted {moved} m in 10 s at rest", p.name);
        assert!(car.state.velocity.length() < 1e-3, "{}: still moving at {}", p.name, car.state.velocity);
        assert!(car.state.wheels.iter().all(|w| w.contact));
    }
}

#[test]
fn dropping_from_ten_metres_never_goes_through_the_ground() {
    let world = World::new(&testing::flat(200.0, Surface::Road));
    for p in presets() {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 10.0, 0.0, 0.0));
        let mut lowest = f32::INFINITY;
        for _ in 0..400 {
            car.step(&world, Input::default());
            lowest = lowest.min(car.state.position.y);
        }
        // The body spheres reach at most ~0.2 m below the centre; the ground is at 0.
        assert!(lowest > 0.15, "{}: centre went down to {lowest}", p.name);
        let up = car.state.rotation * Vec3::Y;
        assert!(up.y > 0.99, "{}: not level after landing ({up})", p.name);
        assert!(car.state.wheels.iter().all(|w| w.contact), "{}: not on its four wheels", p.name);
        assert!(car.state.velocity.length() < 0.05, "{}: still moving after the drop", p.name);
    }
}

#[test]
fn no_tunnelling_through_a_thin_wall_at_400_kmh() {
    let world = World::new(&testing::thin_wall(0.0));
    for p in presets() {
        for yaw in [0.0f32, 0.4, -0.8] {
            let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, -40.0, yaw));
            let fwd = car.state.rotation * Vec3::Z;
            car.state.velocity = Vec3::new(0.0, 0.0, 400.0 / 3.6) + fwd * 0.0;
            for _ in 0..300 {
                car.step(&world, Input { gas: 1.0, ..Default::default() });
                assert!(car.state.position.z < 0.0, "{} (yaw {yaw}): went through the wall", p.name);
            }
        }
    }
}

#[test]
fn no_tunnelling_through_the_ground_when_falling_fast() {
    let world = World::new(&testing::flat(200.0, Surface::Road));
    for p in presets() {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 30.0, 0.0, 0.0));
        car.state.velocity = Vec3::new(20.0, -150.0, 60.0);
        for _ in 0..200 {
            car.step(&world, Input::default());
            assert!(car.state.position.y > 0.0, "{}: fell through the ground", p.name);
        }
    }
}

#[test]
fn every_profile_climbs_a_20_degree_slope_from_rest() {
    let length = 60.0;
    let world = World::new(&testing::slope(20.0, length, Surface::Road));
    let t = libm::tanf(20f32.to_radians());
    let top = length * t;
    for p in presets() {
        // Standing start on the slope itself, facing up.
        let mut car = Car::new(p.clone(), &world, pose(0.0, 5.0 * t, 5.0, 0.0));
        let mut reached = None;
        for tick in 0..1000 {
            car.step(&world, Input { gas: 1.0, ..Default::default() });
            if car.state.position.z > length {
                reached = Some(tick as f32 * DT);
                break;
            }
        }
        let t = reached.unwrap_or_else(|| panic!("{}: stuck at z {:.1} on a 20° slope", p.name, car.state.position.z));
        assert!(car.state.position.y > top, "{}: below the plateau", p.name);
        println!("climb 20° {:<14} {t:.2} s", p.name);
    }
}

/// At 100 km/h, a 0.5 s full-lock pulse then straight steering: the sideways speed dies out.
fn lateral_settle_time(p: &CarParams, surface: Surface) -> Option<f32> {
    let world = World::new(&testing::flat(400.0, surface));
    let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (100.0 / 3.6);
    let mut under = 0;
    for tick in 0..250 {
        let steer = if tick < 50 { 1.0 } else { 0.0 };
        car.step(&world, Input { steer, gas: 1.0, brake: 0.0 });
        if tick >= 50 {
            let lateral = car.state.velocity.dot(car.state.rotation * Vec3::X).abs();
            under = if lateral < 1.0 { under + 1 } else { 0 };
            if under == 20 {
                return Some((tick - 50 - 19) as f32 * DT);
            }
        }
    }
    None
}

#[test]
fn sideways_slides_die_out_on_ground_and_dirt() {
    for p in presets() {
        for surface in [Surface::Ground, Surface::Dirt] {
            let t = lateral_settle_time(&p, surface);
            assert!(t.is_some_and(|t| t <= 1.0), "{} on {surface:?}: lateral speed still above 1 m/s ({t:?})", p.name);
        }
    }
}

#[test]
fn ground_costs_speed() {
    let road = World::new(&testing::strip(3000.0, Surface::Road));
    let ground = World::new(&testing::strip(3000.0, Surface::Ground));
    for p in presets() {
        let top = |world: &World| {
            let mut car = Car::new(p.clone(), world, pose(0.0, 0.0, 0.0, 0.0));
            for _ in 0..2000 {
                car.step(world, Input { gas: 1.0, ..Default::default() });
            }
            car.state.velocity.length()
        };
        assert!(top(&ground) < 0.7 * top(&road), "{}: off-track is not slow enough", p.name);
    }
}

#[test]
fn bvh_raycast_matches_brute_force() {
    let mesh: TrackMesh = testing::playground();
    let world = World::new(&mesh);
    let mut rng = Rng(99);
    let mut hits = 0;
    for _ in 0..2000 {
        let origin = Vec3::new(rng.next() * 300.0 - 150.0, rng.next() * 20.0 - 2.0, rng.next() * 300.0 - 150.0);
        let dir = Vec3::new(rng.next() - 0.5, rng.next() - 0.7, rng.next() - 0.5).normalize();
        let fast = world.raycast(origin, dir, 200.0);
        // Brute force over every triangle.
        let mut best: Option<(f32, Surface)> = None;
        for i in 0..mesh.triangle_count() {
            let [a, b, c] = mesh.triangle(i);
            let (e1, e2) = (b - a, c - a);
            let pv = dir.cross(e2);
            let det = e1.dot(pv);
            if det.abs() < 1e-12 {
                continue;
            }
            let s = origin - a;
            let u = s.dot(pv) / det;
            let q = s.cross(e1);
            let v = dir.dot(q) / det;
            let t = e2.dot(q) / det;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && (0.0..=200.0).contains(&t) && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, mesh.tri_surface[i]));
            }
        }
        match (fast, best) {
            (Some(h), Some((t, _))) => {
                hits += 1;
                assert!((h.distance - t).abs() < 1e-3, "distance {} vs {t}", h.distance);
            }
            (None, None) => {}
            (a, b) => panic!("BVH {a:?} vs brute force {b:?} from {origin} along {dir}"),
        }
    }
    assert!(hits > 200);
}

#[test]
fn presets_are_distinct_named_serializable_and_in_range() {
    // The game offers Combo only for now.
    let all = presets();
    assert_eq!(all.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Combo"]);
    for (i, p) in all.iter().enumerate() {
        assert!(!p.name.is_empty() && !p.description.is_empty());
        for q in &all[i + 1..] {
            assert_ne!(p.name, q.name);
            assert_ne!(p, q);
        }
        let json = serde_json::to_string(p).unwrap();
        let back: CarParams = serde_json::from_str(&json).unwrap();
        assert_eq!(*p, back);
        let mut p = p.clone();
        for t in p.tunables() {
            assert!(
                *t.value >= t.min && *t.value <= t.max,
                "{} / {}: {} outside [{}, {}]",
                t.group,
                t.name,
                t.value,
                t.min,
                t.max
            );
        }
    }
    // Old or partial profiles still load, with the missing fields from "Fidèle".
    let partial: CarParams = serde_json::from_str(r#"{"name":"Vieux","gravity":12.0}"#).unwrap();
    assert_eq!(partial.gravity, 12.0);
    assert_eq!(partial.brake, physics::fidele().brake);
}

struct Step {
    /// Peak angle between the body and its motion, degrees, and when (s).
    peak: f32,
    peak_t: f32,
    /// Worst speed across the path, m/s.
    across: f32,
    /// Speed 1.25 s after the step, km/h.
    kmh_125: f32,
    /// Time after release until the angle is under 3°, s.
    realign: Option<f32>,
}

/// Step steer at `kmh`: full lock with the throttle held for 1.5 s, then straight.
fn step_steer(p: &CarParams, surface: Surface, kmh: f32) -> Step {
    let world = World::new(&testing::flat(400.0, surface));
    let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    let mut out = Step { peak: 0.0, peak_t: 0.0, across: 0.0, kmh_125: 0.0, realign: None };
    for tick in 0..260 {
        let steer = if tick < 150 { 1.0 } else { 0.0 };
        car.step(&world, Input { steer, gas: 1.0, brake: 0.0 });
        let across = car.state.velocity.dot(car.state.ground_normal.cross(car.path_direction())).abs();
        out.across = out.across.max(across);
        let t = car.telemetry();
        if tick < 150 && t.slip_angle_deg > out.peak {
            out.peak = t.slip_angle_deg;
            out.peak_t = (tick + 1) as f32 * DT;
        }
        if tick == 124 {
            out.kmh_125 = t.speed_kmh;
        }
        if tick >= 150 && out.realign.is_none() && t.slip_angle_deg < 3.0 {
            out.realign = Some((tick - 150) as f32 * DT);
        }
    }
    out
}

#[test]
fn on_dirt_the_rear_steps_out_but_the_car_follows_its_path() {
    for p in presets() {
        let dirt = step_steer(&p, Surface::Dirt, 120.0);
        let road = step_steer(&p, Surface::Road, 120.0);
        println!(
            "{:<14} dirt: peak {:4.1}° at {:.2} s, {:.2} m/s across, {:.0} km/h at 1.25 s, realigned {:?} | road: {:.1}°, {:.0} km/h",
            p.name, dirt.peak, dirt.peak_t, dirt.across, dirt.kmh_125, dirt.realign, road.peak, road.kmh_125
        );
        assert!(dirt.across < 1.0 && road.across < 1.0, "{}: the car slides across its path ({} m/s)", p.name, dirt.across);
        let realign = dirt.realign.unwrap_or(f32::INFINITY);
        assert!(realign <= 0.6, "{}: body realigned only after {realign} s", p.name);
        if p.dirt.drift_angle_deg >= 15.0 {
            assert!(dirt.peak > 12.0, "{}: no visible drift on dirt ({}°)", p.name, dirt.peak);
            assert!(dirt.kmh_125 < road.kmh_125 - 5.0, "{}: drifting costs no speed", p.name);
        } else {
            assert!(dirt.peak < p.dirt.drift_angle_deg + 1.0, "{}: drifts more than its maximum ({}°)", p.name, dirt.peak);
        }
        if p.road.grip > 15.0 {
            assert!(road.peak < 3.0, "{}: slides on road ({}°)", p.name, road.peak);
        }
    }
}

/// Dirt car launched at `kmh`, then `inputs(tick, car)` each tick; returns the car after `ticks`.
fn dirt_run(p: &CarParams, kmh: f32, ticks: usize, mut each: impl FnMut(usize, &mut Car, &World)) {
    let world = World::new(&testing::flat(500.0, Surface::Dirt));
    let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    for tick in 0..ticks {
        each(tick, &mut car, &world);
    }
}

#[test]
fn dirt_drift_is_progressive_and_controllable() {
    for p in [physics::fidele(), physics::equilibre()] {
        // On rails well below the limit.
        for kmh in [60.0, 70.0] {
            let mut worst: f32 = 0.0;
            dirt_run(&p, kmh, 150, |_, car, world| {
                let gas = if car.state.velocity.length() < kmh / 3.6 { 1.0 } else { 0.0 };
                car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
                worst = worst.max(car.telemetry().slip_angle_deg);
            });
            assert!(worst < 2.0, "{} at {kmh} km/h: {worst}° drift, should be on rails", p.name);
        }
        // Beyond it, more speed (more excess) gives more angle; the angle lasts while asked for,
        // and a mid-angle drift costs speed moderately.
        let mut at_1s = Vec::new();
        for kmh in [100.0, 120.0, 140.0] {
            let (mut a1, mut a15, mut v05, mut v15, mut sum, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            dirt_run(&p, kmh, 150, |tick, car, world| {
                car.step(world, Input { steer: 1.0, gas: 1.0, brake: 0.0 });
                let t = car.telemetry();
                if tick == 49 {
                    v05 = t.speed_kmh;
                }
                if tick == 99 {
                    a1 = t.slip_angle_deg;
                }
                if tick >= 50 {
                    sum += t.slip_angle_deg;
                    n += 1.0;
                }
                if tick == 149 {
                    a15 = t.slip_angle_deg;
                    v15 = t.speed_kmh;
                }
            });
            let mean = sum / n;
            println!("{} {kmh} km/h: {a1:.1}° at 1 s, {a15:.1}° at 1.5 s, mean {mean:.1}°, {:.1} km/h lost from 0.5 to 1.5 s", p.name, v05 - v15);
            at_1s.push(a1);
            assert!(a15 > 4.0, "{} at {kmh} km/h: the drift died while still steering ({a15}°)", p.name);
            if (10.0..22.0).contains(&mean) {
                assert!(v05 - v15 < 20.0, "{} at {kmh} km/h: a {mean:.0}° drift lost {} km/h in 1 s", p.name, v05 - v15);
            }
        }
        assert!(at_1s[0] < at_1s[1] && at_1s[1] < at_1s[2], "{}: angle not growing with the excess {at_1s:?}", p.name);
        // Keyboard taps: longer taps, bigger angles; a short tap stays small.
        let taps: Vec<f32> = [0.1f32, 0.2, 0.3]
            .iter()
            .map(|&secs| {
                let mut best: f32 = 0.0;
                dirt_run(&p, 130.0, 120, |tick, car, world| {
                    let steer = if (tick as f32) < secs / DT { 1.0 } else { 0.0 };
                    car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
                    best = best.max(car.telemetry().slip_angle_deg);
                });
                best
            })
            .collect();
        println!("{} taps 0.1/0.2/0.3 s at 130 km/h: {taps:?}", p.name);
        assert!(taps[0] < 3.0 && taps[0] < taps[1] && taps[1] < taps[2], "{}: taps {taps:?}", p.name);
    }
}

#[test]
fn tyre_marks_appear_near_the_limit_before_any_drift() {
    for p in [physics::fidele(), physics::equilibre(), physics::buggy_lourd()] {
        // Pure grip: no marks.
        let mut mark_low: f32 = 0.0;
        dirt_run(&p, 60.0, 200, |_, car, world| {
            let gas = if car.state.velocity.length() < 60.0 / 3.6 { 1.0 } else { 0.0 };
            car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
            mark_low = mark_low.max(car.state.wheels[2].mark);
        });
        assert_eq!(mark_low, 0.0, "{}: marks while far from the limit", p.name);
        // Near the limit: marks, aligned tyres, no drift angle.
        let mut found = false;
        for kmh in (35..50).map(|k| k as f32 * 2.0) {
            let (mut usage, mut mark, mut angle, mut smear) = (0.0, 0.0, 0.0, 0.0);
            dirt_run(&p, kmh, 300, |tick, car, world| {
                let gas = if car.state.velocity.length() < kmh / 3.6 { 1.0 } else { 0.0 };
                car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
                if tick == 299 {
                    usage = car.state.grip_usage;
                    mark = car.state.wheels[2].mark;
                    smear = car.state.wheels[2].smear;
                    angle = car.telemetry().slip_angle_deg;
                }
            });
            if (0.9..0.99).contains(&usage) {
                println!("{} at {kmh} km/h: usage {usage:.2}, mark {mark:.2}, smear {smear:.2}, angle {angle:.1}°", p.name);
                assert!(mark > 0.3, "{}: no marks at {usage} of the grip", p.name);
                assert!(angle < 2.0 && smear < 0.05, "{}: drifting ({angle}°) before the limit", p.name);
                found = true;
                break;
            }
        }
        assert!(found, "{}: no steady turn found near the limit", p.name);
    }
}

#[test]
fn braking_hard_at_speed_skids_the_tyres() {
    for surface in [Surface::Road, Surface::Dirt] {
        let world = World::new(&testing::strip(3000.0, surface));
        for p in presets() {
            let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
            for _ in 0..3000 {
                if car.state.velocity.length() > 80.0 / 3.6 {
                    break;
                }
                car.step(&world, Input { gas: 1.0, ..Default::default() });
            }
            car.step(&world, Input::default());
            assert_eq!(car.state.skid, 0.0, "{} on {surface:?}: skidding while coasting", p.name);
            for _ in 0..10 {
                car.step(&world, Input { brake: 1.0, ..Default::default() });
            }
            // Every braking tyre leaves a full mark (skis never brake).
            let braking = if p.front_skis { 2..4 } else { 0..4 };
            assert!(car.state.skid > 0.9, "{} on {surface:?}: skid {}", p.name, car.state.skid);
            assert!(
                car.state.wheels[braking].iter().all(|w| w.mark > 0.9 && w.slip > 0.9),
                "{} on {surface:?}: braking tyres without marks",
                p.name
            );
            car.step(&world, Input::default());
            assert_eq!(car.state.skid, 0.0, "{} on {surface:?}: still skidding off the brakes", p.name);
        }
    }
}

#[test]
fn lifting_off_or_braking_in_a_dirt_turn_does_not_rotate_the_car() {
    let p = physics::fidele();
    let world = World::new(&testing::flat(400.0, Surface::Dirt));
    for input in [Input { steer: 1.0, gas: 0.0, brake: 0.0 }, Input { steer: 1.0, gas: 0.0, brake: 1.0 }] {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
        for _ in 0..30 {
            car.step(&world, Input::default());
        }
        car.state.velocity = car.state.rotation * Vec3::Z * (85.0 / 3.6);
        for _ in 0..150 {
            let gas = if car.state.velocity.length() < 85.0 / 3.6 { 1.0 } else { 0.0 };
            car.step(&world, Input { steer: 1.0, gas, brake: 0.0 });
        }
        let before = car.telemetry().slip_angle_deg;
        let mut worst: f32 = 0.0;
        for _ in 0..60 {
            car.step(&world, input);
            worst = worst.max(car.telemetry().slip_angle_deg);
        }
        assert!(worst < before + 2.0 && worst < 5.0, "{input:?}: the car rotated to {worst}° (was {before}°)");
    }
}

#[test]
fn a_single_bump_does_not_steer_the_car() {
    for one_side in [false, true] {
        let world = World::new(&testing::single_bump(0.1, 1.0, 20.0, one_side));
        for p in presets() {
            let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
            for _ in 0..30 {
                car.step(&world, Input::default());
            }
            car.state.velocity = car.state.rotation * Vec3::Z * (150.0 / 3.6);
            let mut worst: f32 = 0.0;
            for _ in 0..150 {
                car.step(&world, Input { gas: 0.4, ..Default::default() });
                let f = car.state.rotation * Vec3::Z;
                let d = car.path_direction();
                worst = worst.max(libm::atan2f(f.x, f.z).abs()).max(libm::atan2f(d.x, d.z).abs());
            }
            let worst = worst.to_degrees();
            assert!(worst < 1.0, "{} (one side: {one_side}): heading moved {worst}° over a 10 cm bump", p.name);
        }
    }
}

#[test]
fn a_yaw_kick_turns_the_body_back_not_the_path() {
    let world = World::new(&testing::flat(400.0, Surface::Road));
    for p in presets() {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
        for _ in 0..30 {
            car.step(&world, Input::default());
        }
        car.state.velocity = car.state.rotation * Vec3::Z * (150.0 / 3.6);
        car.state.angular_velocity += Vec3::Y * 1.5;
        for _ in 0..100 {
            car.step(&world, Input { gas: 0.4, ..Default::default() });
        }
        let v = car.state.velocity;
        let course = libm::atan2f(v.x, v.z).to_degrees();
        let f = car.state.rotation * Vec3::Z;
        let heading = libm::atan2f(f.x, f.z).to_degrees();
        assert!(course.abs() < 1.0, "{}: the kick bent the trajectory by {course}°", p.name);
        assert!(heading.abs() < 1.0, "{}: the body did not come back ({heading}°)", p.name);
    }
}

#[test]
fn displayed_wheel_angle_is_visible_at_any_speed() {
    let world = World::new(&testing::flat(400.0, Surface::Road));
    let mut car = Car::new(physics::fidele(), &world, pose(0.0, 0.0, 0.0, 0.0));
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (220.0 / 3.6);
    for _ in 0..20 {
        car.step(&world, Input { steer: -1.0, gas: 1.0, brake: 0.0 });
    }
    let w = car.state.wheels[0];
    assert!(w.steer.to_degrees() < 8.0, "physical angle {}°", w.steer.to_degrees());
    assert!(w.steer_display.to_degrees() > 20.0, "displayed angle only {}°", w.steer_display.to_degrees());
    assert_eq!(car.state.wheels[2].steer_display, 0.0);
}

/// Holds full lock while a drift ends, either by fading with speed on dirt or by the car running
/// onto road (grip comes back): returns (peak angle, lowest body/path yaw-rate ratio while the
/// angle closes, largest drop of the path's turn rate, whether path or body turned outward).
fn drift_end(p: &CarParams, mesh: TrackMesh, start: Pose, ticks: usize) -> (f32, f32, f32, bool) {
    let world = World::new(&mesh);
    let mut car = Car::new(p.clone(), &world, start);
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (140.0 / 3.6);
    let (mut peak, mut closing, mut path_start, mut last) = (0.0f32, false, 0.0f32, 0.0f32);
    let (mut ratio, mut drop, mut outward, mut done) = (f32::INFINITY, 0.0f32, false, false);
    for _ in 0..ticks {
        car.step(&world, Input { steer: 1.0, gas: 1.0, brake: 0.0 });
        let s = &car.state;
        let angle = s.drift_ref.abs();
        // Turning right: path and body rates are negative.
        let path = -s.path_rate;
        let body = -car.telemetry().yaw_rate;
        if !done {
            peak = peak.max(angle);
            if !closing && peak > 8f32.to_radians() && angle < peak - 1f32.to_radians() {
                closing = true;
                path_start = path;
            }
            if closing {
                // The first drift's closing phase, until the angle is gone or grows again.
                if angle < 1f32.to_radians() || angle > last + 1e-4 {
                    done = true;
                } else {
                    ratio = ratio.min(body / path.max(0.05));
                    drop = drop.max(path_start - path);
                    outward |= path < 0.0 || body < 0.0;
                }
            }
        }
        last = angle;
    }
    (peak.to_degrees(), ratio, drop, outward)
}

#[test]
fn when_a_drift_ends_the_car_never_turns_outward_on_its_own() {
    for p in presets() {
        let faded = drift_end(&p, testing::flat(500.0, Surface::Dirt), pose(0.0, 0.0, 0.0, 0.0), 400);
        // Heading -Z, a right turn goes toward +X, where the road starts.
        let onto_road = drift_end(
            &p,
            testing::patches(500.0, Surface::Dirt, Surface::Road),
            pose(-12.0, 0.0, 0.0, core::f32::consts::PI),
            300,
        );
        for (case, (peak, ratio, drop, outward)) in [("fading", faded), ("onto road", onto_road)] {
            println!("{:<14} {case:<9} peak {peak:4.1}°, body ≥ {ratio:.2} × path, path drop {drop:.2} rad/s", p.name);
            if peak < 8.0 {
                continue;
            }
            assert!(!outward, "{} ({case}): turned outward on its own as the drift ended", p.name);
            assert!(drop < 0.05, "{} ({case}): the path loosened by {drop} rad/s as the drift ended", p.name);
            assert!(ratio > 0.25, "{} ({case}): the body nearly stopped turning ({ratio} of the path)", p.name);
        }
    }
}

#[test]
fn road_acceleration_has_punch() {
    let world = World::new(&testing::strip(3000.0, Surface::Road));
    for p in presets() {
        let mut car = Car::new(p.clone(), &world, pose(0.0, 0.0, 0.0, 0.0));
        let (mut t100, mut t200, mut top) = (None, None, 0.0f32);
        for tick in 1..=4000 {
            car.step(&world, Input { gas: 1.0, ..Default::default() });
            let kmh = car.telemetry().speed_kmh;
            let t = tick as f32 * DT;
            if t100.is_none() && kmh >= 100.0 {
                t100 = Some(t);
            }
            if t200.is_none() && kmh >= 200.0 {
                t200 = Some(t);
            }
            top = top.max(kmh);
        }
        let (t100, t200) = (t100.unwrap_or(f32::INFINITY), t200.unwrap_or(f32::INFINITY));
        println!("{}: 0→100 {t100:.2} s, 0→200 {t200:.2} s, top {top:.0} km/h", p.name);
        assert!(t100 <= 1.6, "{}: 0→100 in {t100} s", p.name);
        assert!(t200 <= 7.5, "{}: 0→200 in {t200} s", p.name);
        assert!((275.0..=310.0).contains(&top), "{}: top speed {top} km/h", p.name);
    }
}

/// A straight road from z = −100 to `length`, with a booster pad across it from `pad.0` to
/// `pad.1` (none if empty).
fn booster_strip(length: f32, pad: (f32, f32)) -> TrackMesh {
    let mut b = testing::MeshBuilder::new();
    b.grid((-30.0, -100.0), (30.0, length), 2.0, |_, _| 0.0, move |_, z| if (pad.0..pad.1).contains(&z) { Surface::Booster } else { Surface::Road });
    b.build()
}

#[test]
fn a_booster_pad_pushes_the_car_then_fades_out() {
    // From 150 km/h with the throttle held, the car crosses a pad 10 m long: the boost starts at
    // full on it and is gone `boost_time` after its end, leaving the car about half of
    // `boost_accel · boost_time` faster than without the pad.
    let plain = World::new(&booster_strip(2000.0, (0.0, 0.0)));
    let padded = World::new(&booster_strip(2000.0, (40.0, 50.0)));
    for p in presets() {
        let run = |world: &World| {
            let mut car = Car::new(p.clone(), world, pose(0.0, 0.0, 0.0, 0.0));
            car.state.velocity = Vec3::Z * (150.0 / 3.6);
            let (mut after_pad, mut boost_seen) = (None, 0.0f32);
            for tick in 0..600u32 {
                car.step(world, Input { gas: 1.0, ..Default::default() });
                boost_seen = boost_seen.max(car.telemetry().boost);
                if after_pad.is_none() && car.state.position.z > 50.0 {
                    after_pad = Some(tick);
                }
            }
            (car.state.velocity.length(), boost_seen, car.telemetry().boost, after_pad)
        };
        let (v_plain, seen_plain, _, _) = run(&plain);
        let (v_boost, seen, left, after) = run(&padded);
        let gain = v_boost - v_plain;
        println!("{}: +{:.1} m/s ({:.0} → {:.0} km/h), boost seen {seen:.2}, left {left:.2}, pad left at tick {after:?}", p.name, gain, v_plain * 3.6, v_boost * 3.6);
        assert_eq!(seen_plain, 0.0, "{}: a boost without a pad", p.name);
        assert!(seen > 0.99, "{}: the pad did not start the boost ({seen})", p.name);
        assert_eq!(left, 0.0, "{}: the boost never faded out", p.name);
        let full = 0.5 * p.boost_accel * p.boost_time;
        assert!((0.7 * full..=1.05 * full).contains(&gain), "{}: the boost gave {gain} m/s (about {full} expected)", p.name);
    }
}

#[test]
fn a_booster_pushes_past_the_engines_top_speed() {
    // At 310 km/h the engine no longer pushes; a pad still does.
    let plain = World::new(&booster_strip(3000.0, (0.0, 0.0)));
    let padded = World::new(&booster_strip(3000.0, (40.0, 50.0)));
    for p in presets() {
        let run = |world: &World| {
            let mut car = Car::new(p.clone(), world, pose(0.0, 0.0, 0.0, 0.0));
            car.state.velocity = Vec3::Z * (310.0 / 3.6);
            for _ in 0..300 {
                car.step(world, Input { gas: 1.0, ..Default::default() });
            }
            car.telemetry().speed_kmh
        };
        let (plain, boosted) = (run(&plain), run(&padded));
        println!("{}: {plain:.0} km/h without the pad, {boosted:.0} with it", p.name);
        assert!(plain <= 310.5, "{}: the engine pushed past its top speed ({plain} km/h)", p.name);
        assert!(boosted > plain + 30.0, "{}: the pad hardly pushed ({plain} → {boosted} km/h)", p.name);
    }
}
