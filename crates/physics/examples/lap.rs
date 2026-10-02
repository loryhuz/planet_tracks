//! Times the keyboard-style autopilot over a map and logs its speed along the route every half
//! second: a quick check of a new map's length in seconds and of the places where the scripted
//! driver runs wide or falls.
//!
//!     cargo run -p physics --release --example lap -- [map name or file.json] [road dirt]
//!
//! With two numbers the driver lifts and brakes before the bends, allowing itself that lateral
//! acceleration (m/s²) on road and on dirt (see `Autopilot::braking`); without, it never brakes.
//! `QUIET=1` prints the summary only; `PARAMS=session` drives with the current profile of the
//! game's session file (`tuning/session.json`) instead of the presets.
use physics::{Car, World, presets, testing::Autopilot};
use track::Surface;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = args.get(1).cloned().unwrap_or_else(|| "Jezero".into());
    let lateral: Option<(f32, f32)> = match (args.get(2).and_then(|s| s.parse().ok()), args.get(3).and_then(|s| s.parse().ok())) {
        (Some(r), Some(d)) => Some((r, d)),
        _ => None,
    };
    let quiet = std::env::var_os("QUIET").is_some();
    let map = match track::builtin_maps().into_iter().find(|m| m.name.eq_ignore_ascii_case(&arg)) {
        Some(m) => m,
        None => track::Map::load(&std::fs::read_to_string(&arg).expect("map name or file")).expect("map"),
    };
    let track = map.build();
    let world = World::new(&track.mesh);
    // `OFFSET=u` drives a line `u` metres left of the centreline (negative: right), at the height
    // of the deck there, to try the edges of the track.
    let offset: f32 = std::env::var("OFFSET").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let shifted: Vec<glam::Vec3> = (0..track.route.len())
        .map(|i| {
            let r = &track.route;
            let (a, b) = (r[i.saturating_sub(1)], r[(i + 1).min(r.len() - 1)]);
            let d = glam::Vec3::new(b.x - a.x, 0.0, b.z - a.z).normalize_or_zero();
            r[i] + glam::Vec3::new(d.z, 0.0, -d.x) * offset
        })
        .collect();
    let route = if offset == 0.0 { &track.route } else { &shifted };
    // `PARAMS=session`: drive with the current profile of the game's session file instead.
    let cars = if std::env::var("PARAMS").is_ok_and(|v| v == "session") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tuning/session.json");
        let session: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("session file")).expect("session JSON");
        let profile = &session["profiles"][session["current"].as_u64().unwrap_or(0) as usize];
        vec![serde_json::from_value::<physics::CarParams>(profile["params"].clone()).expect("params")]
    } else {
        presets()
    };
    for p in cars {
        let mut car = Car::new(p.clone(), &world, track.start);
        let mut pilot = Autopilot::new(route);
        if let Some((road, dirt)) = lateral {
            pilot = pilot.braking(&world, road, dirt);
        }
        let (mut ground, mut worst, mut top, mut air, mut braking) = (0u32, 0.0f32, 0.0f32, 0u32, 0u32);
        // Body impacts as the game counts them for its sound: rising past 2 m/s.
        let (mut impacts, mut last_impact) = (0u32, 0.0f32);
        let mut crossed = vec![false; track.checkpoints.len()];
        let mut finished = None;
        for tick in 0..9000u32 {
            let input = pilot.input(&car);
            if input.brake > 0.0 {
                braking += 1;
            }
            car.step(&world, input);
            let t = car.telemetry();
            let pos = car.state.position;
            top = top.max(t.speed_kmh);
            if t.airborne {
                air += 1;
            }
            if t.impact > 2.0 && last_impact <= 2.0 {
                impacts += 1;
                if !quiet {
                    println!(
                        "  impact {:4.1} m/s at {:5.2} s, route {:4.0} m, {:3.0} km/h, {:?}{}",
                        t.impact,
                        tick as f32 / 100.0,
                        pilot.index as f32 * 4.0,
                        t.speed_kmh,
                        t.surface,
                        if car.state.wall_contact { ", wall" } else { "" }
                    );
                }
            }
            last_impact = t.impact;
            let off = pilot.offset(pos);
            worst = worst.max(off);
            if car.state.wheels.iter().any(|w| w.contact && w.surface == Some(Surface::Ground)) {
                ground += 1;
            }
            for (c, done) in track.checkpoints.iter().zip(crossed.iter_mut()) {
                *done |= c.contains(pos);
            }
            if tick % 50 == 0 && !quiet {
                println!(
                    "{:5.1} s  route {:5.0} m  {:5.0} km/h  y {:5.1}  offset {:4.1}  {:?}{}",
                    tick as f32 / 100.0,
                    pilot.index as f32 * 4.0,
                    t.speed_kmh,
                    pos.y,
                    off,
                    t.surface,
                    if t.airborne { "  air" } else { "" }
                );
            }
            if track.finish.contains(pos) && crossed.iter().all(|&c| c) {
                finished = Some(tick + 1);
                break;
            }
            // Well below the route: the car has fallen off an elevated deck.
            if pos.y < track.fall_limit_y || pos.y < route[pilot.index].y - 7.0 {
                println!("fell at {:.1} s near route {:.0} m ({pos})", tick as f32 / 100.0, pilot.index as f32 * 4.0);
                break;
            }
        }
        let length: f32 = route.windows(2).map(|w| w[0].distance(w[1])).sum();
        match finished {
            Some(t) => println!(
                "{} on {}: {:.2} s for {:.0} m ({:.0} km/h average), top {:.0} km/h, {} ticks on off-track ground, {:.1} s airborne, {:.1} s braking, {} impacts, worst offset {:.1} m",
                p.name,
                map.name,
                t as f32 / 100.0,
                length,
                length / (t as f32 / 100.0) * 3.6,
                top,
                ground,
                air as f32 / 100.0,
                braking as f32 / 100.0,
                impacts,
                worst
            ),
            None => println!("{} on {}: did not finish", p.name, map.name),
        }
    }
}
