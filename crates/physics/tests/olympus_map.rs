//! Acceptance runs on « Olympus », the short map (a climb to a plateau, a dirt section, a climb
//! to a kicker whose big jump lands on a hill back down to the ground, a technical finish),
//! driven by the keyboard-style autopilot: full lock or nothing, throttle always held, never
//! braking.

use glam::Vec3;
use physics::{Car, World, presets, testing::Autopilot};
use track::Surface;
use track::jump::LandingProfile;

struct Run {
    /// Time to the finish, every checkpoint crossed, seconds.
    finished: Option<f32>,
    /// Whether the car dropped well below the route (off an elevated deck).
    fell: bool,
    /// The jump's landing descent, relative to its lip.
    landing: LandingProfile,
    /// The flight over the jump's gap: where it took off and touched down, metres past the lip.
    takeoff: Option<f32>,
    touchdown: Option<f32>,
    /// That flight's time in the air, s, and the highest the car rose above the lip, m.
    air: f32,
    apex: f32,
    /// Lowest speed from the foot of the climb to the end of the landing, km/h.
    jump_min_kmh: f32,
    /// Hardest body impact there, m/s.
    jump_impact: f32,
    /// Lowest `up.y` of the body: 1 level, 0 on its side.
    worst_up: f32,
    /// Ticks with a wheel on off-track ground, outside the hairpins.
    ground_ticks: u32,
}

fn run(p: &physics::CarParams) -> Run {
    let map = track::builtin_maps().into_iter().find(|m| m.name == "Olympus").expect("Olympus");
    let layout = map.layout().expect("layout");
    let track = map.build();
    let world = World::new(&track.mesh);
    let route = &track.route;
    // The jump: the climb, the ramp and the landing, from the lip's frame.
    let j = layout.pieces.iter().position(|q| q.landing().is_some()).expect("Olympus has a jump");
    let landing = &layout.pieces[j];
    let profile = landing.landing().unwrap();
    let lip = landing.entry.pos;
    let forward = landing.entry.heading.forward();
    let nearest = |q: Vec3| (0..route.len()).min_by(|&a, &b| route[a].distance(q).total_cmp(&route[b].distance(q))).unwrap();
    let window = nearest(layout.pieces[j - 2].entry.pos)..=nearest(landing.exit.pos);
    // The one-cell hairpins, which a driver who never brakes cuts at 270 km/h, a wheel brushing
    // the ground inside them.
    let hairpins: Vec<_> = layout
        .pieces
        .iter()
        .filter(|q| matches!(q.piece.kind, track::kit::Kind::Turn { size: 1, .. }))
        .map(|q| nearest(q.entry.pos)..=nearest(q.exit.pos))
        .collect();

    let mut car = Car::new(p.clone(), &world, track.start);
    let mut pilot = Autopilot::new(route);
    let mut crossed = vec![false; track.checkpoints.len()];
    let mut out = Run {
        finished: None,
        fell: false,
        landing: profile,
        takeoff: None,
        touchdown: None,
        air: 0.0,
        apex: 0.0,
        jump_min_kmh: f32::INFINITY,
        jump_impact: 0.0,
        worst_up: 1.0,
        ground_ticks: 0,
    };
    // The current flight: where and when it took off, and its highest point above the lip.
    let mut flight: Option<(f32, u32, f32)> = None;
    for tick in 0..4000 {
        let input = pilot.input(&car);
        car.step(&world, input);
        let t = car.telemetry();
        let pos = car.state.position;
        out.worst_up = out.worst_up.min((car.state.rotation * Vec3::Y).y);
        let off_track = car.state.wheels.iter().any(|w| w.contact && w.surface == Some(Surface::Ground));
        if off_track && !hairpins.iter().any(|h| h.contains(&pilot.index)) {
            out.ground_ticks += 1;
        }
        if window.contains(&pilot.index) {
            out.jump_min_kmh = out.jump_min_kmh.min(t.speed_kmh);
            out.jump_impact = out.jump_impact.max(t.impact);
            let along = (pos - lip).dot(forward);
            match (t.airborne, flight) {
                (true, None) => flight = Some((along, tick, pos.y - lip.y)),
                (true, Some((from, start, apex))) => flight = Some((from, start, apex.max(pos.y - lip.y))),
                (false, Some((from, start, apex))) => {
                    // The flight that crossed the gap, where there is nothing to land on.
                    if from < profile.gap && along >= profile.gap && out.touchdown.is_none() {
                        out.takeoff = Some(from);
                        out.touchdown = Some(along);
                        out.air = (tick - start) as f32 / 100.0;
                        out.apex = apex;
                    }
                    flight = None;
                }
                _ => {}
            }
        }
        for (c, done) in track.checkpoints.iter().zip(crossed.iter_mut()) {
            *done |= c.contains(pos);
        }
        if track.finish.contains(pos) && crossed.iter().all(|&c| c) {
            out.finished = Some((tick + 1) as f32 / 100.0);
            break;
        }
        if pos.y < track.fall_limit_y || pos.y < route[pilot.index].y - 7.0 {
            out.fell = true;
            break;
        }
    }
    out
}

#[test]
fn full_throttle_keyboard_run_finishes_under_30_seconds() {
    for p in presets() {
        let r = run(&p);
        println!(
            "{}: {:?} s, fell {}, worst up {:.2}, {} ticks on the ground outside the hairpins",
            p.name, r.finished, r.fell, r.worst_up, r.ground_ticks
        );
        assert!(!r.fell, "{}: fell off", p.name);
        let t = r.finished.unwrap_or_else(|| panic!("{}: did not finish", p.name));
        assert!(t < 30.0, "{}: {t} s", p.name);
        assert!(r.worst_up > 0.6, "{}: tipped over (up.y {})", p.name, r.worst_up);
        assert_eq!(r.ground_ticks, 0, "{}: wheels left the track", p.name);
    }
}

#[test]
fn the_kicker_throws_the_car_high_and_it_lands_on_the_hill() {
    for p in presets() {
        let r = run(&p);
        println!(
            "{}: takeoff {:?} m, touchdown {:?} m from the lip, {:.2} s in the air, {:.1} m above the lip, lowest {:.0} km/h, hardest impact {:.1} m/s",
            p.name, r.takeoff, r.touchdown, r.air, r.apex, r.jump_min_kmh, r.jump_impact
        );
        let (Some(from), Some(to)) = (r.takeoff, r.touchdown) else { panic!("{}: never flew over the gap", p.name) };
        // Off the lip, not off the crest of the climb before it: the landing is shaped for that.
        assert!(from.abs() < 5.0, "{}: took off {from} m from the lip", p.name);
        // On the parabolic part of the descent, which catches at the design angle.
        assert!((r.landing.gap..=r.landing.knee).contains(&to), "{}: touched down {to} m from the lip", p.name);
        assert!(r.jump_impact < 2.0, "{}: the body hit something at {} m/s", p.name, r.jump_impact);
        assert!(r.jump_min_kmh > 200.0, "{}: down to {} km/h over the jump", p.name, r.jump_min_kmh);
        // A big jump, the fun of the map: the car rises well above the lip and stays in the air
        // for most of a second (a jump ramp's 4° flight skims the landing for a third of one).
        assert!(r.apex > 5.0, "{}: rose only {} m above the lip", p.name, r.apex);
        assert!(r.air > 0.8, "{}: only {} s in the air", p.name, r.air);
    }
}
