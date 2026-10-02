//! Replays a recorded best run from the game's session file (`tuning/session.json`) with the
//! car parameters it was driven with, and logs it along the route every quarter second: speed,
//! height, flights, impacts, checkpoints and respawns. To see where a player's run went wrong.
//!
//!     cargo run -p physics --release --example replay -- Noctis@2 [from_s to_s]
//!
//! The race logic is the app's (`crates/app/src/race.rs`): a countdown with no input, then the
//! recorded frames; a respawn puts the car back as it was at the last checkpoint.
use glam::Vec3;
use physics::{Car, CarParams, Input, World};

const COUNTDOWN_TICKS: u32 = 150;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let key = args.get(1).cloned().unwrap_or_else(|| "Noctis@2".into());
    let from: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let to: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(f32::INFINITY);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tuning/session.json");
    let session: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("session file")).expect("session JSON");
    let profile = &session["profiles"][session["current"].as_u64().unwrap_or(0) as usize];
    let best = &profile["bests"][&key];
    assert!(best.is_object(), "no best run {key} in the current profile");
    let params: CarParams = serde_json::from_str(best["params"].as_str().expect("params")).expect("params JSON");
    let frames: Vec<(Input, bool)> = best["frames"]
        .as_array()
        .expect("frames")
        .iter()
        .map(|f| (serde_json::from_value(f["input"].clone()).expect("input"), f["respawn"].as_bool().unwrap_or(false)))
        .collect();
    let name = key.split('@').next().unwrap();
    let map = track::builtin_maps().into_iter().find(|m| m.name == name).expect("map");
    let track = map.build();
    let world = World::new(&track.mesh);
    let route = &track.route;
    // Distance along the route at each route point.
    let mut along = vec![0.0f32; route.len()];
    for i in 1..route.len() {
        along[i] = along[i - 1] + route[i].distance(route[i - 1]);
    }
    println!("{key}: {} ticks recorded, gravity {} (air ×{}), slope gravity {}", frames.len(), params.gravity, params.air_gravity, params.slope_gravity);

    let mut car = Car::new(params, &world, track.start);
    for _ in 0..COUNTDOWN_TICKS {
        car.step(&world, Input::default());
    }
    let mut crossed = vec![false; track.checkpoints.len()];
    let mut last_checkpoint = None;
    let (mut index, mut last_impact, mut was_air) = (0usize, 0.0f32, false);
    for (tick, &(input, respawn)) in frames.iter().enumerate() {
        let t = tick as f32 / 100.0;
        let show = t >= from && t <= to;
        if respawn {
            if let Some(state) = &last_checkpoint {
                car.state = Clone::clone(state);
            }
            println!("{t:6.2} s  RESPAWN at the last checkpoint");
        }
        let before = car.state.position;
        car.step(&world, input);
        let pos = car.state.position;
        // Nearest route point, searched around the last one (or everywhere after a respawn).
        let (lo, hi) = if respawn { (0, route.len()) } else { (index.saturating_sub(10), (index + 40).min(route.len())) };
        index = (lo..hi).min_by(|&a, &b| route[a].distance_squared(pos).total_cmp(&route[b].distance_squared(pos))).unwrap();
        let tel = car.telemetry();
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let off = flat(pos - route[index]).length();
        for (i, cp) in track.checkpoints.iter().enumerate() {
            let steps = (((pos - before).length() / 0.2).ceil() as usize).clamp(1, 64);
            if !crossed[i] && (0..=steps).any(|k| cp.contains(before.lerp(pos, k as f32 / steps as f32))) {
                crossed[i] = true;
                last_checkpoint = Some(car.state.clone());
                println!("{t:6.2} s  checkpoint {} at route {:.0} m, {:.0} km/h", i + 1, along[index], tel.speed_kmh);
            }
        }
        if show && tel.airborne != was_air {
            println!(
                "{t:6.2} s  {} at route {:.0} m, {:.0} km/h, y {:.1} (route {:.1}), offset {:.1} m",
                if tel.airborne { "TAKE-OFF" } else { "landing" },
                along[index],
                tel.speed_kmh,
                pos.y,
                route[index].y,
                off
            );
        }
        was_air = tel.airborne;
        if show && tel.impact > 2.0 && last_impact <= 2.0 {
            println!("{t:6.2} s  impact {:.1} m/s at route {:.0} m, {:?}{}", tel.impact, along[index], tel.surface, if car.state.wall_contact { ", wall" } else { "" });
        }
        last_impact = tel.impact;
        if show && tick % 25 == 0 {
            println!(
                "{t:6.2} s  route {:5.0} m  {:4.0} km/h  y {:5.1} (route {:5.1})  offset {:4.1}  steer {:+.1} gas {:.0} brake {:.0}  {:?}",
                along[index],
                tel.speed_kmh,
                pos.y,
                route[index].y,
                off,
                input.steer,
                input.gas,
                input.brake,
                tel.surface
            );
        }
        if crossed.iter().all(|&c| c) && track.finish.contains(pos) {
            println!("{t:6.2} s  finish");
            break;
        }
    }
}
