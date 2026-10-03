//! How the ski car rides a run of gutters (docs/blocks-ice.md): the test autopilot drives it on
//! the middle, on lines up the outside of the turns (on the outer wall), steering at most half
//! or a third of the lock (it runs wide onto the walls), or not steering at all, and the example
//! reports for each the time, the speeds, the hardest knock (slowing down along the motion, m/s²),
//! how high it rode over the floor and how long it flew.
//!
//!     cargo run -p physics --release --example gutter_lines -- [map name or file.json]
//!
//! Without a map, `RUN=straight3,curve3_left,...` lays those blocks as gutters (`LEVEL=4` from a
//! level, for those that drop), entered at `KMH` (220 by default; a map starts from rest).
//! `ONLY=name` drives one line, `TRACE=1` prints it every 0.1 s.
use glam::Vec3;
use physics::{Car, Input, World, car_for, testing::Autopilot};
use track::kit::{Connector, Heading, Layout};
use track::map::parse_block;

/// The route pushed `wall` metres toward the outside of its turns, fully where their radius is
/// 80 m or less.
fn outer_line(route: &[Vec3], wall: f32) -> Vec<Vec3> {
    let n = route.len();
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let turn = |i: usize| {
        let (a, b, c) = (route[i.saturating_sub(3)], route[i], route[(i + 3).min(n - 1)]);
        let (d0, d1) = (flat(b - a), flat(c - b));
        if d0.length() < 1e-3 || d1.length() < 1e-3 {
            return 0.0;
        }
        // Turning left per metre.
        libm::atan2f(d0.x * d1.z - d0.z * d1.x, d0.dot(d1)) / (0.5 * (d0.length() + d1.length()))
    };
    let raw: Vec<f32> = (0..n).map(turn).collect();
    (0..n)
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(6), (i + 6).min(n - 1));
            let k = raw[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32;
            let (a, b) = (route[i.saturating_sub(1)], route[(i + 1).min(n - 1)]);
            let d = flat(b - a).normalize_or_zero();
            let left = Vec3::new(d.z, 0.0, -d.x);
            route[i] - left * (k.signum() * (k.abs() * 80.0).min(1.0) * wall)
        })
        .collect()
}

struct Out {
    time: Option<f32>,
    slowest: f32,
    end_speed: f32,
    knock: f32,
    highest: f32,
    air: f32,
}

fn drive(track: &track::Track, planet: track::Planet, kmh: f32, line: &[Vec3], max_steer: f32, trace: bool) -> Out {
    let world = World::new(&track.mesh);
    let mut car = Car::new(car_for(planet), &world, track.start);
    for _ in 0..30 {
        car.step(&world, Input::default());
    }
    car.state.velocity = car.state.rotation * Vec3::Z * (kmh / 3.6);
    let mut pilot = Autopilot::new(line);
    let mut out = Out { time: None, slowest: f32::INFINITY, end_speed: 0.0, knock: 0.0, highest: 0.0, air: 0.0 };
    let mut last_v = car.state.velocity;
    for tick in 0..6000u32 {
        let mut input = pilot.input(&car);
        input.steer = input.steer.clamp(-max_steer, max_steer);
        car.step(&world, input);
        let s = &car.state;
        let v = s.velocity;
        let knock = -(v - last_v).dot(v.normalize_or_zero()) / 0.01;
        last_v = v;
        if tick > 5 {
            out.knock = out.knock.max(knock);
        }
        out.slowest = out.slowest.min(v.length() * 3.6);
        let h = s.position.y - track.route[pilot.index].y;
        out.highest = out.highest.max(h);
        let flying = !s.wheels.iter().any(|w| w.contact);
        if flying {
            out.air += 0.01;
        }
        if trace && tick % 10 == 0 {
            let tilt = (s.rotation * Vec3::Y).y.clamp(-1.0, 1.0).acos().to_degrees();
            println!("    {:5.1} s  route {:4}  {h:4.1} m up  tilt {tilt:3.0}°  {:3.0} km/h  knock {knock:4.0}{}", tick as f32 / 100.0, pilot.index, v.length() * 3.6, if flying { "  in the air" } else { "" });
        }
        if pilot.index + 2 >= line.len() {
            out.time = Some(tick as f32 / 100.0);
            out.end_speed = v.length() * 3.6;
            break;
        }
        if s.position.y < track.fall_limit_y {
            break;
        }
    }
    out
}

fn main() {
    let env = |k: &str| std::env::var(k).ok();
    let (layout, planet, kmh) = match std::env::args().nth(1) {
        Some(arg) => {
            let map = match track::builtin_maps().into_iter().find(|m| m.name.eq_ignore_ascii_case(&arg)) {
                Some(m) => m,
                None => track::Map::load(&std::fs::read_to_string(&arg).expect("map name or file")).expect("map"),
            };
            (map.layout().expect("layout"), map.planet, 0.0)
        }
        None => {
            let run = env("RUN").unwrap_or_else(|| "straight3,curve3_left,straight1,curve4_right,straight2,curve4_left,straight3".into());
            let level = env("LEVEL").and_then(|s| s.parse().ok()).unwrap_or(0);
            let mut l = Layout::new("gutters", Connector::entering((0, 0), level, Heading::North));
            for id in run.split(',') {
                l.push(parse_block(id, Some("gutter")).unwrap_or_else(|e| panic!("{id}: {e:?}")));
            }
            (l, track::Planet::Ice, env("KMH").and_then(|s| s.parse().ok()).unwrap_or(220.0))
        }
    };
    let track = layout.build();
    let only = env("ONLY");
    for (name, line, steer) in [
        ("middle", track.route.clone(), 1.0),
        ("outside 8", outer_line(&track.route, 8.0), 1.0),
        ("outside 12", outer_line(&track.route, 12.0), 1.0),
        ("half lock", track.route.clone(), 0.5),
        ("third lock", track.route.clone(), 0.33),
        ("no steering", track.route.clone(), 0.0),
    ] {
        if only.as_ref().is_some_and(|o| o != name) {
            continue;
        }
        let o = drive(&track, planet, kmh, &line, steer, env("TRACE").is_some());
        println!(
            "  {name:<12} {}  slowest {:3.0}, end {:3.0} km/h  knock {:5.1} m/s²  {:.1} m up at most  {:.2} s in the air",
            o.time.map_or(" did not get through".into(), |t| format!("{t:6.2} s")),
            o.slowest,
            o.end_speed,
            o.knock,
            o.highest,
            o.air
        );
    }
}
