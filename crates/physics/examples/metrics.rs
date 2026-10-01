//! Driving metrics of every profile, measured on the physics crate's own test meshes.
//!
//!     cargo run -p physics --release --example metrics [profile-name-filter]
//!
//! When docs/feel-targets/feel-targets.json exists, "Fidèle" is printed next to the measured
//! targets of the reference car.

use std::time::Instant;

use glam::Vec3;
use physics::{Car, CarParams, DT, Input, World, presets, testing};
use serde_json::Value;
use track::{Pose, Surface};

const KMH: f32 = 1.0 / 3.6;

struct Worlds {
    road_strip: World,
    dirt_strip: World,
    road_flat: World,
    dirt_flat: World,
    ground_flat: World,
    bumps: World,
    bump: World,
    bump_side: World,
    ramp: World,
    ramp_lip: f32,
    wall: World,
}

fn pose(x: f32, z: f32, yaw: f32) -> Pose {
    Pose { position: Vec3::new(x, 0.0, z), yaw }
}

/// A car at rest at `pose`, settled, then launched at `kmh` along its heading.
fn launch(p: &CarParams, world: &World, at: Pose, kmh: f32) -> Car {
    let mut car = Car::new(p.clone(), world, at);
    for _ in 0..30 {
        car.step(world, Input::default());
    }
    let fwd = car.state.rotation * Vec3::Z;
    car.state.velocity = fwd * (kmh * KMH);
    car
}

fn fwd_speed(car: &Car) -> f32 {
    car.state.velocity.dot(car.state.rotation * Vec3::Z)
}

/// Gas that holds a target speed (m/s).
fn hold(car: &Car, target: f32) -> f32 {
    ((target - car.state.velocity.length()) * 0.6 + 0.1).clamp(0.0, 1.0)
}

#[derive(Default)]
struct Accel {
    t100: Option<f32>,
    d100: Option<f32>,
    t200: Option<f32>,
    top: f32,
    at30s: f32,
}

fn accel(p: &CarParams, world: &World) -> Accel {
    let mut car = Car::new(p.clone(), world, pose(0.0, 0.0, 0.0));
    let z0 = car.state.position.z;
    let mut out = Accel::default();
    let mut last_gain = 0.0;
    let mut best = 0.0f32;
    for tick in 1..=6000 {
        car.step(world, Input { gas: 1.0, ..Default::default() });
        let t = tick as f32 * DT;
        let kmh = car.state.velocity.length() * 3.6;
        if out.t100.is_none() && kmh >= 100.0 {
            out.t100 = Some(t);
            out.d100 = Some(car.state.position.z - z0);
        }
        if out.t200.is_none() && kmh >= 200.0 {
            out.t200 = Some(t);
        }
        if tick == 3000 {
            out.at30s = kmh;
        }
        if kmh > best + 0.05 {
            best = kmh;
            last_gain = t;
        }
        if t - last_gain > 3.0 && t > 20.0 {
            break;
        }
    }
    out.top = best;
    if out.at30s == 0.0 {
        out.at30s = best;
    }
    out
}

/// (distance m, time s) to stop from `kmh` with the brake held.
fn brake(p: &CarParams, world: &World, kmh: f32) -> (f32, f32) {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let z0 = car.state.position.z;
    for tick in 1..3000 {
        car.step(world, Input { brake: 1.0, ..Default::default() });
        if fwd_speed(&car) <= 0.05 {
            return (car.state.position.z - z0, tick as f32 * DT);
        }
    }
    (f32::NAN, f32::NAN)
}

/// Deceleration (m/s²) coasting from `kmh` over one second.
fn coast(p: &CarParams, world: &World, kmh: f32) -> f32 {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let v0 = car.state.velocity.length();
    for _ in 0..100 {
        car.step(world, Input::default());
    }
    car.state.velocity.length() - v0
}

#[derive(Default, Clone, Copy)]
struct Turn {
    kmh: f32,
    yaw: f32,
    radius: f32,
    lat_g: f32,
    slip: f32,
    drift: f32,
    spun: bool,
}

/// Steady full lock (right) at a held speed, averaged over the 4th second.
fn turn(p: &CarParams, world: &World, kmh: f32) -> Turn {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let target = kmh * KMH;
    let mut acc = Turn::default();
    let mut n = 0.0;
    for tick in 0..400 {
        let gas = hold(&car, target);
        car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
        let t = car.telemetry();
        if t.slip_angle_deg > 100.0 {
            acc.spun = true;
        }
        if tick >= 300 {
            let v = car.state.velocity;
            let speed = v.length().max(0.1);
            let up = car.state.rotation * Vec3::Y;
            let a_perp = car.state.acceleration - v * (car.state.acceleration.dot(v) / (speed * speed));
            let a_perp = a_perp - up * a_perp.dot(up);
            let w_path = a_perp.length() / speed;
            acc.kmh += speed * 3.6;
            acc.yaw += t.yaw_rate.abs();
            acc.radius += w_path;
            acc.lat_g += a_perp.length() / 9.81;
            acc.slip += t.slip_angle_deg;
            acc.drift += t.drift;
            n += 1.0;
        }
    }
    Turn {
        kmh: acc.kmh / n,
        yaw: acc.yaw / n,
        radius: (acc.kmh / 3.6) / acc.radius.max(1e-3),
        lat_g: acc.lat_g / n,
        slip: acc.slip / n,
        drift: acc.drift / n,
        spun: acc.spun,
    }
}

/// Acceleration lost at full lock vs straight, full throttle from `kmh` (m/s²).
fn turn_penalty(p: &CarParams, world: &World, kmh: f32) -> f32 {
    let run = |steer: f32| {
        let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
        for _ in 0..40 {
            car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
        }
        let v0 = car.state.velocity.length();
        for _ in 0..20 {
            car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
        }
        (car.state.velocity.length() - v0) / 0.2
    };
    run(1.0) - run(0.0)
}

/// (yaw t63 s, final yaw rad/s, yaw ratio per tick after release) for a step at `kmh`.
fn steer_response(p: &CarParams, world: &World, kmh: f32) -> (f32, f32, f32) {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let target = kmh * KMH;
    let mut yaws = Vec::new();
    for _ in 0..100 {
        let gas = hold(&car, target);
        car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
        yaws.push(car.telemetry().yaw_rate.abs());
    }
    let fin = yaws[80..].iter().sum::<f32>() / 20.0;
    let t63 = yaws.iter().position(|&y| y >= 0.63 * fin).map_or(f32::NAN, |i| (i + 1) as f32 * DT);
    let mut after = Vec::new();
    for _ in 0..20 {
        let gas = hold(&car, target);
        car.step(world, Input { steer: 0.0, gas, brake: 0.0 });
        after.push(car.telemetry().yaw_rate.abs());
    }
    let ratio = if after[5] > 1e-3 { libm::powf(after[15] / after[5], 0.1) } else { 0.0 };
    (t63, fin, ratio)
}

struct WallHit {
    kept_50ms: f32,
    kept_300ms: f32,
    upright: bool,
}

/// Hits the wall at `incidence` degrees at `kmh`, throttle held, no steering.
fn wall(p: &CarParams, world: &World, incidence: f32, kmh: f32) -> WallHit {
    let yaw = incidence.to_radians();
    let dist = 3.0;
    let mut car = launch(p, world, pose(10.0 - dist, 0.0, yaw), kmh);
    let mut before = car.state.velocity.length();
    let mut hit_tick = None;
    let mut kept_50 = f32::NAN;
    let mut upright = true;
    for tick in 0..400 {
        let prev = car.state.velocity.length();
        car.step(world, Input { gas: 1.0, ..Default::default() });
        if hit_tick.is_none() && car.state.wall_contact {
            hit_tick = Some(tick);
            before = prev;
        }
        if let Some(h) = hit_tick {
            if tick == h + 5 {
                kept_50 = car.state.velocity.length() / before;
            }
            if (car.state.rotation * Vec3::Y).y < 0.5 {
                upright = false;
            }
            if tick == h + 30 {
                return WallHit { kept_50ms: kept_50, kept_300ms: car.state.velocity.length() / before, upright };
            }
        }
    }
    WallHit { kept_50ms: f32::NAN, kept_300ms: f32::NAN, upright }
}

struct Jump {
    air: f32,
    dist: f32,
    gravity: f32,
    on_wheels: bool,
    takeoff_kmh: f32,
}

/// Off a 15° ramp at 150 km/h (speed held on the run-up, throttle held in the air).
fn ramp(p: &CarParams, world: &World, lip: f32) -> Jump {
    let _ = lip;
    let mut car = launch(p, world, pose(0.0, -90.0, 0.0), 150.0);
    let target = 150.0 * KMH;
    let mut takeoff: Option<(usize, Vec3, f32)> = None;
    let mut landing: Option<(usize, Vec3)> = None;
    let mut vys = Vec::new();
    let mut min_up: f32 = 1.0;
    let mut settled = 0;
    for tick in 0..1500 {
        let on_ramp = car.state.position.z > -12.0;
        let gas = if on_ramp { 1.0 } else { hold(&car, target) };
        car.step(world, Input { gas, ..Default::default() });
        let wheels = car.state.wheels.iter().filter(|w| w.contact).count();
        match (takeoff, landing) {
            (None, _) if on_ramp && wheels == 0 => {
                takeoff = Some((tick, car.state.position, car.state.velocity.length() * 3.6));
            }
            (Some(_), None) => {
                if wheels > 0 || car.state.impact > 0.0 {
                    landing = Some((tick, car.state.position));
                } else {
                    vys.push(car.state.velocity.y);
                }
            }
            (Some(_), Some((lt, _))) => {
                min_up = min_up.min((car.state.rotation * Vec3::Y).y);
                settled = if wheels == 4 { settled + 1 } else { 0 };
                if settled >= 30 || tick > lt + 500 {
                    break;
                }
            }
            _ => {}
        }
    }
    let (Some((t0, p0, kmh)), Some((t1, p1))) = (takeoff, landing) else {
        return Jump { air: f32::NAN, dist: f32::NAN, gravity: f32::NAN, on_wheels: false, takeoff_kmh: f32::NAN };
    };
    let gravity = if vys.len() > 4 { -(vys[vys.len() - 2] - vys[1]) / ((vys.len() - 3) as f32 * DT) } else { f32::NAN };
    Jump {
        air: (t1 - t0) as f32 * DT,
        dist: Vec3::new(p1.x - p0.x, 0.0, p1.z - p0.z).length(),
        gravity,
        on_wheels: min_up > 0.5 && settled >= 30,
        takeoff_kmh: kmh,
    }
}

/// Dirt at 100 km/h, keyboard taps (full lock 0.15 s on, 0.15 s off): (max body angle to its
/// motion, max yaw acceleration rad/s², max speed across the path m/s).
fn taps(p: &CarParams, world: &World) -> (f32, f32, f32) {
    let target = 100.0 * KMH;
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), 100.0);
    let (mut slip, mut jerk, mut lat, mut prev_yaw) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for tick in 0..400 {
        let steer = if (tick / 15) % 2 == 0 { 1.0 } else { 0.0 };
        let gas = hold(&car, target);
        car.step(world, Input { steer, gas, brake: 0.0 });
        let t = car.telemetry();
        slip = slip.max(t.slip_angle_deg);
        if tick > 0 {
            jerk = jerk.max((t.yaw_rate - prev_yaw).abs() / DT);
        }
        prev_yaw = t.yaw_rate;
        let path_left = car.state.ground_normal.cross(car.path_direction());
        lat = lat.max(car.state.velocity.dot(path_left).abs());
    }
    (slip, jerk, lat)
}

/// At `kmh`, a 0.5 s full-lock pulse then straight: (max speed across the path m/s, max body
/// angle to its motion, time after release until the body is back within 3° of its motion).
fn lateral_decay(p: &CarParams, world: &World, kmh: f32) -> (f32, f32, Option<f32>) {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let (mut max_lat, mut max_slip) = (0.0f32, 0.0f32);
    let mut settled = None;
    for tick in 0..300 {
        let steer = if tick < 50 { 1.0 } else { 0.0 };
        car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
        let path_left = car.state.ground_normal.cross(car.path_direction());
        max_lat = max_lat.max(car.state.velocity.dot(path_left).abs());
        let slip = car.telemetry().slip_angle_deg;
        max_slip = max_slip.max(slip);
        if tick >= 50 && settled.is_none() && slip < 3.0 {
            settled = Some((tick - 50) as f32 * DT);
        }
    }
    (max_lat, max_slip, settled)
}

struct DirtDrift {
    /// Peak angle between the body and its motion, degrees, and when (s after the steering step).
    peak: f32,
    peak_t: f32,
    max_lat: f32,
    /// Speed at 0.5, 1.0 and 1.25 s after the step, km/h.
    kmh: [f32; 3],
    min_kmh: f32,
    /// Time after release until the angle is under 2°.
    realign: Option<f32>,
}

/// Step steer: speed held at `kmh`, then full lock with full throttle for 1.5 s, then straight.
fn drift_run(p: &CarParams, world: &World, kmh: f32) -> DirtDrift {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let mut out = DirtDrift { peak: 0.0, peak_t: 0.0, max_lat: 0.0, kmh: [0.0; 3], min_kmh: kmh, realign: None };
    for tick in 0..260 {
        let steer = if tick < 150 { 1.0 } else { 0.0 };
        car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
        let path_left = car.state.ground_normal.cross(car.path_direction());
        out.max_lat = out.max_lat.max(car.state.velocity.dot(path_left).abs());
        let t = car.telemetry();
        let time = (tick + 1) as f32 * DT;
        if tick < 150 {
            if t.slip_angle_deg > out.peak {
                out.peak = t.slip_angle_deg;
                out.peak_t = time;
            }
            out.min_kmh = out.min_kmh.min(t.speed_kmh);
        }
        for (k, at) in [0.5f32, 1.0, 1.25].iter().enumerate() {
            if (time - at).abs() < 0.005 {
                out.kmh[k] = t.speed_kmh;
            }
        }
        if tick >= 150 && out.realign.is_none() && t.slip_angle_deg < 2.0 {
            out.realign = Some((tick - 150) as f32 * DT);
        }
    }
    out
}

/// On dirt: (time for the angle to fall under 2° after releasing a 0.6 s full lock at 140 km/h;
/// countersteer at 100 km/h after 1 s of full lock: time for the yaw rate to reverse, time for the
/// angle to reach 0, largest angle the other way within 1 s).
fn release_and_countersteer(p: &CarParams, world: &World) -> (Option<f32>, Option<f32>, Option<f32>, f32) {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), 140.0);
    let mut release = None;
    for tick in 0..200 {
        let steer = if tick < 60 { 1.0 } else { 0.0 };
        car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
        if tick >= 60 && release.is_none() && car.telemetry().slip_angle_deg < 2.0 {
            release = Some((tick - 60) as f32 * DT);
        }
    }
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), 100.0);
    let (mut yaw_rev, mut zero, mut swing) = (None, None, 0.0f32);
    for tick in 0..200 {
        let steer = if tick < 100 { 1.0 } else { -1.0 };
        car.step(world, Input { steer, gas: 1.0, brake: 0.0 });
        if tick >= 100 {
            let t = car.telemetry();
            let k = (tick - 100) as f32 * DT;
            // Turning right: yaw negative, drift angle negative; countersteering reverses both.
            if yaw_rev.is_none() && t.yaw_rate > 0.0 {
                yaw_rev = Some(k);
            }
            if zero.is_none() && t.drift_angle_deg > -0.5 {
                zero = Some(k);
            }
            if zero.is_some() && k < 1.0 {
                swing = swing.max(t.drift_angle_deg);
            }
        }
    }
    (release, yaw_rev, zero, swing)
}

/// Full lock with the throttle held from `kmh`: (drift angle, speed change km/h) at 0.5, 1 and 2 s.
fn full_lock(p: &CarParams, world: &World, kmh: f32) -> [(f32, f32); 3] {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let mut out = [(0.0, 0.0); 3];
    for tick in 1..=200 {
        car.step(world, Input { steer: 1.0, gas: 1.0, brake: 0.0 });
        let t = car.telemetry();
        for (k, at) in [50, 100, 200].iter().enumerate() {
            if tick == *at {
                out[k] = (t.slip_angle_deg, t.speed_kmh - kmh);
            }
        }
    }
    out
}

/// Largest drift angle after a keyboard tap (full lock for `secs`) at `kmh`, throttle held.
fn tap(p: &CarParams, world: &World, kmh: f32, secs: f32) -> f32 {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    let on = (secs / DT) as usize;
    let mut best: f32 = 0.0;
    for tick in 0..150 {
        let steer = if tick < on { 1.0 } else { 0.0 };
        let gas = hold(&car, kmh * KMH);
        car.step(world, Input { steer, gas, brake: 0.0 });
        best = best.max(car.telemetry().slip_angle_deg);
    }
    best
}

/// Steady full lock at a held speed: (grip usage, tyre mark, drift angle) after 3 s.
fn steady(p: &CarParams, world: &World, kmh: f32) -> (f32, f32, f32) {
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), kmh);
    for _ in 0..300 {
        let gas = hold(&car, kmh * KMH);
        car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
    }
    let t = car.telemetry();
    (t.grip_usage, car.state.wheels[2].mark, t.slip_angle_deg)
}

/// Heading change (degrees) after a 10 cm bump at 150 km/h, straight, full width and one side.
fn bump_heading(p: &CarParams, full: &World, side: &World) -> (f32, f32) {
    let run = |world: &World| {
        let mut car = launch(p, world, pose(0.0, 0.0, 0.0), 150.0);
        let heading = |car: &Car| {
            let f = car.state.rotation * Vec3::Z;
            libm::atan2f(f.x, f.z).to_degrees()
        };
        let h0 = heading(&car);
        let mut worst: f32 = 0.0;
        for _ in 0..150 {
            let gas = hold(&car, 150.0 * KMH);
            car.step(world, Input { gas, ..Default::default() });
            worst = worst.max((heading(&car) - h0).abs());
        }
        worst
    };
    (run(full), run(side))
}

/// Over dirt rollers (±0.3 m, 25 m wavelength) at 100 km/h: (suspension travel used m, body pitch
/// amplitude deg, share of ticks with 4 wheels down).
fn bumps(p: &CarParams, world: &World) -> (f32, f32, f32) {
    let mut car = launch(p, world, pose(0.0, 0.0, -20.0), 100.0);
    let (mut smin, mut smax) = ([f32::MAX; 4], [f32::MIN; 4]);
    let (mut pmin, mut pmax, mut down) = (f32::MAX, f32::MIN, 0);
    for tick in 0..300 {
        let gas = hold(&car, 100.0 * KMH);
        car.step(world, Input { gas, ..Default::default() });
        if tick >= 100 {
            let pitch = (car.state.rotation * Vec3::Z).y.asin().to_degrees();
            pmin = pmin.min(pitch);
            pmax = pmax.max(pitch);
            for (i, w) in car.state.wheels.iter().enumerate() {
                smin[i] = smin[i].min(w.suspension);
                smax[i] = smax[i].max(w.suspension);
            }
            if car.state.wheels.iter().all(|w| w.contact) {
                down += 1;
            }
        }
    }
    let travel = (0..4).map(|i| smax[i] - smin[i]).sum::<f32>() / 4.0;
    (travel, (pmax - pmin) * 0.5, down as f32 / 200.0)
}

/// Body attitude: (nose dive under full braking from 150 km/h, squat under full throttle from
/// standstill, roll at full lock at 120 km/h), degrees.
fn attitude(p: &CarParams, world: &World) -> (f32, f32, f32) {
    let pitch = |car: &Car| (car.state.rotation * Vec3::Z).y.asin().to_degrees();
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), 150.0);
    let mut dive: f32 = 0.0;
    for _ in 0..60 {
        car.step(world, Input { brake: 1.0, ..Default::default() });
        dive = dive.max(-pitch(&car));
    }
    let mut car = Car::new(p.clone(), world, pose(0.0, 0.0, 0.0));
    let mut squat: f32 = 0.0;
    for _ in 0..60 {
        car.step(world, Input { gas: 1.0, ..Default::default() });
        squat = squat.max(pitch(&car));
    }
    let mut car = launch(p, world, pose(0.0, 0.0, 0.0), 120.0);
    let mut roll: f32 = 0.0;
    for _ in 0..150 {
        let gas = hold(&car, 120.0 * KMH);
        car.step(world, Input { steer: 1.0, gas, brake: 0.0 });
        roll = roll.max((car.state.rotation * Vec3::X).y.asin().to_degrees().abs());
    }
    (dive, squat, roll)
}

fn opt(v: Option<f32>, unit: &str) -> String {
    v.map_or("—".into(), |x| format!("{x:.2} {unit}"))
}

fn curve_at(v: &Value, x: f32) -> Option<f32> {
    let pts: Vec<(f32, f32)> = v
        .as_array()?
        .iter()
        .filter_map(|p| Some((p.get(0)?.as_f64()? as f32, p.get(1)?.as_f64()? as f32)))
        .collect();
    if pts.is_empty() {
        return None;
    }
    let mut best = pts[0];
    for &p in &pts {
        if (p.0 - x).abs() < (best.0 - x).abs() {
            best = p;
        }
    }
    Some(best.1)
}

fn load_json(name: &str) -> Option<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/feel-targets").join(name);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_targets() -> Option<Value> {
    load_json("feel-targets.json")
}

fn main() {
    let filter = std::env::args().nth(1).map(|s| s.to_lowercase());
    let (ramp_mesh, lip) = testing::ramp(15.0, 10.0);
    let worlds = Worlds {
        road_strip: World::new(&testing::strip(9000.0, Surface::Road)),
        dirt_strip: World::new(&testing::strip(9000.0, Surface::Dirt)),
        road_flat: World::new(&testing::flat(420.0, Surface::Road)),
        dirt_flat: World::new(&testing::flat(420.0, Surface::Dirt)),
        ground_flat: World::new(&testing::flat(420.0, Surface::Ground)),
        bumps: World::new(&testing::bumps(0.3, 25.0)),
        bump: World::new(&testing::single_bump(0.1, 1.0, 20.0, false)),
        bump_side: World::new(&testing::single_bump(0.1, 1.0, 20.0, true)),
        ramp: World::new(&ramp_mesh),
        ramp_lip: lip,
        wall: World::new(&testing::wall_lane(10.0)),
    };
    let targets = load_targets();
    let rally_doc = load_json("rally-dirt.json");
    let rally = rally_doc.as_ref().map(|d| &d["steps"]);
    let kt = targets.as_ref().map(|t| &t["key_targets"]);
    let tv = |path: &[&str]| -> Option<f32> {
        let mut v = kt?;
        for k in path {
            v = v.get(*k)?;
        }
        v.as_f64().map(|x| x as f32)
    };
    let tc = |key: &str, x: f32| -> Option<f32> { curve_at(&kt?.get(key)?["curve"], x) };
    let fmt_t = |x: Option<f32>, unit: &str| x.map_or(String::new(), |x| format!("   cible {x:.2} {unit}"));

    let mut summary: Vec<String> = Vec::new();
    for p in presets() {
        if let Some(f) = &filter
            && !p.name.to_lowercase().contains(f.as_str())
        {
            continue;
        }
        let is_ref = p.name == "Fidèle" && kt.is_some();
        let show = |label: &str, value: String, target: String| {
            if is_ref {
                println!("  {label:<34} {value:<22}{target}");
            } else {
                println!("  {label:<34} {value}");
            }
        };
        println!("\n=== {} — {}", p.name, p.description);
        println!("  gravité {:.2} m/s² (air ×{:.2}, pentes ×{:.2})", p.gravity, p.air_gravity, p.slope_gravity);

        let a = accel(&p, &worlds.road_strip);
        show("0→100 km/h (route)", format!("{} / {}", opt(a.t100, "s"), opt(a.d100, "m")), fmt_t(tv(&["time_0_100_s"]), "s"));
        show("0→200 km/h (route)", opt(a.t200, "s"), fmt_t(tv(&["time_0_200_s_derived"]), "s"));
        show("vitesse à 30 s / max (route)", format!("{:.1} / {:.1} km/h", a.at30s, a.top), fmt_t(tv(&["top_speed_kmh", "max_observed"]), "km/h (non atteinte)"));
        let ad = accel(&p, &worlds.dirt_strip);
        show("0→100 km/h (terre)", opt(ad.t100, "s"), String::new());
        show("vitesse max (terre)", format!("{:.1} km/h", ad.top), String::new());
        let (bd, bt) = brake(&p, &worlds.road_strip, 200.0);
        show("freinage 200→0 (route)", format!("{bd:.1} m, {bt:.2} s"), String::new());
        let (bd96, bt96) = brake(&p, &worlds.road_strip, 96.0);
        show(
            "freinage 96→0 (route)",
            format!("{bd96:.1} m, {bt96:.2} s"),
            fmt_t(tv(&["brake_road", "stop_from_96kmh", "m"]), "m").to_string()
                + &fmt_t(tv(&["brake_road", "stop_from_96kmh", "t_s"]), "s"),
        );
        let (bdd, _) = brake(&p, &worlds.dirt_strip, 96.0);
        show("freinage 96→0 (terre)", format!("{bdd:.1} m"), String::new());
        show("roue libre à 90 km/h", format!("{:.2} m/s²", coast(&p, &worlds.road_strip, 90.0)), fmt_t(tv(&["coast_decel_ms2"]), "m/s²"));
        let (t63, yfin, rel) = steer_response(&p, &worlds.road_flat, 120.0);
        show(
            "lacet à 120 km/h : t63 / relâché",
            format!("{t63:.2} s / ×{rel:.3} par tick"),
            fmt_t(tv(&["steering", "yaw_t63_s"]), "s") + &fmt_t(tv(&["steering", "yaw_release_ratio_per_tick"]), ""),
        );
        let _ = yfin;
        let mut turns: Vec<Turn> = Vec::new();
        println!("  {:<34} {:>6} {:>7} {:>8} {:>7} {:>7} {:>6}", "plein braquage", "km/h", "lacet", "rayon", "a lat", "glisse", "dérive");
        for (surface, world) in [("route", &worlds.road_flat), ("terre", &worlds.dirt_flat)] {
            for kmh in [60.0, 120.0, 200.0] {
                let t = turn(&p, world, kmh);
                let target = if is_ref && surface == "route" {
                    let k = if kmh > 100.0 { kmh } else { 78.0 };
                    format!(
                        "   cible ~{:.0} km/h : {:.2} rad/s, {:.1} m, {:.1} g",
                        k,
                        tc("full_lock_yaw_rate_rad_s", k).unwrap_or(f32::NAN),
                        tc("full_lock_radius_m", k).unwrap_or(f32::NAN),
                        tc("full_lock_lateral_g", k).unwrap_or(f32::NAN)
                    )
                } else if is_ref && kmh < 100.0 {
                    format!("   neige : {:.1} g max, trajectoire {:.2} rad/s", tv(&["snow", "lateral_g_max_p50"]).unwrap_or(f32::NAN), tv(&["snow", "path_turn_rate_rad_s"]).unwrap_or(f32::NAN))
                } else {
                    String::new()
                };
                println!(
                    "  {:<34} {:>6.0} {:>7.2} {:>6.1} m {:>5.1} g {:>6.1}° {:>6.2}{}{}",
                    format!("  {surface} {kmh:.0} km/h"),
                    t.kmh,
                    t.yaw,
                    t.radius,
                    t.lat_g,
                    t.slip,
                    t.drift,
                    if t.spun { "  TÊTE-À-QUEUE" } else { "" },
                    target
                );
                turns.push(t);
            }
        }
        for kmh in [84.0, 120.0, 228.0] {
            let pen = turn_penalty(&p, &worlds.road_flat, kmh);
            show(&format!("perte en virage à {kmh:.0} km/h"), format!("{pen:.1} m/s²"), fmt_t(tc("full_lock_accel_penalty_ms2", kmh), "m/s²"));
        }
        println!("  {:<34} {:>6} {:>27} {:>30}", "terre, plein braquage + gaz", "départ", "angle à 0,5 / 1 / 2 s", "vitesse à 0,5 / 1 / 2 s (km/h)");
        for v in [100.0, 120.0, 140.0] {
            let r = full_lock(&p, &worlds.dirt_flat, v);
            let target = if is_ref {
                rally
                    .and_then(|r| r.get(format!("r_field_step{v:.0}R").as_str()))
                    .map(|t| {
                        format!(
                            "   (Rally : pic {:.0}°, {:+.0} km/h à 1 s)",
                            t["drift_peak_deg"].as_f64().unwrap_or(f64::NAN),
                            t["at_t"]["1.0"]["kmh"].as_f64().unwrap_or(f64::NAN) - v as f64
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            println!(
                "  {:<34} {:>6.0} {:>8.1}° {:>6.1}° {:>6.1}° {:>9.0} {:>6.0} {:>6.0}{}",
                "", v, r[0].0, r[1].0, r[2].0, r[0].1, r[1].1, r[2].1, target
            );
        }
        let tap_angles: Vec<f32> = [0.1, 0.2, 0.3, 0.5].iter().map(|&d| tap(&p, &worlds.dirt_flat, 130.0, d)).collect();
        show(
            "terre 130 km/h, coup de 0,1/0,2/0,3/0,5 s",
            format!("angle max {:.1}° / {:.1}° / {:.1}° / {:.1}°", tap_angles[0], tap_angles[1], tap_angles[2], tap_angles[3]),
            String::new(),
        );
        for v in [80.0, 90.0] {
            let (u, mark, angle) = steady(&p, &worlds.dirt_flat, v);
            show(&format!("terre, plein braquage tenu à {v:.0} km/h"), format!("adhérence utilisée {:.0} %, traces {:.2}, angle {:.1}°", u * 100.0, mark, angle), String::new());
        }
        for v in [180.0, 230.0, 270.0] {
            let (u, mark, angle) = steady(&p, &worlds.road_flat, v);
            show(&format!("route, plein braquage tenu à {v:.0} km/h"), format!("adhérence utilisée {:.0} %, traces {:.2}, angle {:.1}°", u * 100.0, mark, angle), String::new());
        }
        let steps: Vec<(f32, DirtDrift)> = [120.0, 140.0].iter().map(|&v| (v, drift_run(&p, &worlds.dirt_flat, v))).collect();
        let (rel, rev, zero, swing) = release_and_countersteer(&p, &worlds.dirt_flat);
        show(
            "terre : relâché / contre-braquage",
            format!(
                "angle < 2° {} après relâché ; lacet inversé en {}, angle nul en {}, balancier {swing:.1}°",
                opt(rel, "s"),
                opt(rev, "s"),
                opt(zero, "s")
            ),
            if is_ref { "   Rally : 0,25 s ; 0,11 s, 0,15 s, 2,5–3°".to_string() } else { String::new() },
        );
        let rr = drift_run(&p, &worlds.road_flat, 120.0);
        show(
            "route 120 km/h, même échelon",
            format!("pic {:.1}°, {:.0} km/h à 1,25 s", rr.peak, rr.kmh[2]),
            String::new(),
        );
        let dr120 = &steps[0].1;
        let dr140 = &steps[1].1;
        for (surface, world) in [("hors-piste", &worlds.ground_flat), ("terre", &worlds.dirt_flat)] {
            let (lat, slip, settle) = lateral_decay(&p, world, 100.0);
            show(
                &format!("{surface} 100 km/h, braqué 0,5 s"),
                format!("v en travers {lat:.2} m/s max, angle max {slip:.0}°, réalignée en {}", opt(settle, "s")),
                String::new(),
            );
        }
        let (bf, bs) = bump_heading(&p, &worlds.bump, &worlds.bump_side);
        show("bosse de 10 cm à 150 km/h", format!("cap dévié de {bf:.2}° (pleine largeur), {bs:.2}° (un côté)"), String::new());
        let (travel, bp, bd4) = bumps(&p, &worlds.bumps);
        show(
            "bosses ±30 cm / 25 m à 100 km/h",
            format!("débattement utilisé {travel:.2} m, tangage ±{bp:.1}°, 4 roues au sol {:.0} %", bd4 * 100.0),
            String::new(),
        );
        let (dive, squat, roll) = attitude(&p, &worlds.road_flat);
        show("caisse : plongée / cabrage / roulis", format!("{dive:.1}° au freinage, {squat:.1}° en accélération, {roll:.1}° en virage"), String::new());
        let (ts, tj, tl) = taps(&p, &worlds.dirt_flat);
        show("terre, tapotements clavier", format!("angle max {ts:.0}°, à-coup de lacet max {tj:.0} rad/s², v en travers {tl:.2} m/s max"), String::new());
        for (inc, speed) in [(20.0, 150.0), (45.0, 150.0), (60.0, 150.0), (88.0, 120.0)] {
            let w = wall(&p, &worlds.wall, inc, speed);
            let target = tc("walls_speed_retained_pct_vs_incidence", inc).map_or(String::new(), |x| format!("   cible ~{x:.0} %"));
            show(
                &format!("mur à {inc:.0}° ({speed:.0} km/h)"),
                format!("{:.0} % à 50 ms, {:.0} % à 0,3 s{}", w.kept_50ms * 100.0, w.kept_300ms * 100.0, if w.upright { "" } else { ", renversée" }),
                target,
            );
        }
        let w20 = wall(&p, &worlds.wall, 20.0, 150.0);
        let w60 = wall(&p, &worlds.wall, 60.0, 150.0);
        let j = ramp(&p, &worlds.ramp, worlds.ramp_lip);
        let rt = |t: &Turn| format!("{:.0} m {:.1} g", t.radius, t.lat_g);
        let dd = |d: &DirtDrift| format!("{:.0}° à {:.2} s, {:.0} km/h à 1,25 s", d.peak, d.peak_t, d.kmh[2]);
        summary.push(format!(
            "| {} | {} | {} | {:.0} | {:.1} | {} | {} | {} | {} | {} | {} | {:.0} / {:.0} % | {:.2} s, {:.0} m, {} |",
            p.name,
            opt(a.t100, "s"),
            opt(a.t200, "s"),
            a.top,
            bd,
            rt(&turns[0]),
            rt(&turns[1]),
            rt(&turns[2]),
            rt(&turns[4]),
            dd(dr120),
            dd(dr140),
            w20.kept_50ms * 100.0,
            w60.kept_50ms * 100.0,
            j.air,
            j.dist,
            if j.on_wheels { "roues" } else { "MAL" }
        ));
        show(
            "tremplin 15° à 150 km/h",
            format!(
                "{:.2} s en l'air, {:.1} m, g {:.1}, {}",
                j.air,
                j.dist,
                j.gravity,
                if j.on_wheels { "sur les roues" } else { "MAL reçu" }
            ),
            fmt_t(tv(&["air", "gravity_ms2"]), "m/s² en l'air"),
        );
        let _ = j.takeoff_kmh;
    }

    println!("\n=== Résumé (plein braquage : rayon et accélération latérale ; murs : vitesse gardée à 50 ms)");
    println!("| profil | 0→100 | 0→200 | v max km/h | frein 200→0 m | route 60 | route 120 | route 200 | terre 120 | terre, échelon 120 km/h | terre, échelon 140 km/h | mur 20° / 60° | tremplin 15° 150 km/h |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for line in &summary {
        println!("{line}");
    }

    // Timing on a 50k-triangle terrain.
    let big = testing::big_terrain(50_000);
    let t0 = Instant::now();
    let world = World::new(&big);
    let build = t0.elapsed();
    let mut car = Car::new(presets()[0].clone(), &world, Pose { position: Vec3::new(0.0, 1.0, 0.0), yaw: 0.0 });
    let n = 20_000;
    let t0 = Instant::now();
    for i in 0..n {
        let steer = if (i / 150) % 3 == 0 { 1.0 } else if (i / 150) % 3 == 1 { -1.0 } else { 0.0 };
        car.step(&world, Input { steer, gas: 1.0, brake: 0.0 });
    }
    let per = t0.elapsed().as_secs_f64() / n as f64 * 1e6;
    println!(
        "\n  Performance : {} triangles, BVH construit en {:.1} ms, Car::step {:.1} µs en moyenne",
        world.triangle_count(),
        build.as_secs_f64() * 1e3,
        per
    );
}
