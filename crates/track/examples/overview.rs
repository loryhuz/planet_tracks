//! Writes a shaded top view of a map's terrain with its route and props (PPM), for placing
//! scenery: `cargo run -p track --example overview -- [map name or file] [half extent m] [m per px] [out.ppm]`
//! (by default `target/overview.ppm`, Jezero, 900 m around the route at 3 m per pixel).

use glam::Vec2;
use track::kit::CELL;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or_else(|| "Jezero".into());
    let half: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(900.0);
    let step: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let out = args.get(4).cloned().unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/overview.ppm").into());
    let map = match track::builtin_maps().into_iter().find(|m| m.name.eq_ignore_ascii_case(&name)) {
        Some(m) => m,
        None => track::Map::load(&std::fs::read_to_string(&name).expect("map name or file")).expect("map"),
    };
    let built = map.build_detailed().expect("build");
    let route = &built.track.route;
    let (lo, hi) = route.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| {
        (lo.min(Vec2::new(p.x, p.z)), hi.max(Vec2::new(p.x, p.z)))
    });
    let mid = 0.5 * (lo + hi);
    let n = (2.0 * half / step) as usize;
    let t = &built.terrain;
    let mut h = vec![0.0f32; n * n];
    // Image x grows toward −X (east on the right, north up: +Z up).
    let world = |i: usize, k: usize| Vec2::new(mid.x + half - i as f32 * step, mid.y + half - k as f32 * step);
    for k in 0..n {
        for i in 0..n {
            let p = world(i, k);
            h[k * n + i] = t.height(p.x, p.y);
        }
    }
    let mut img = vec![[0u8; 3]; n * n];
    let light = glam::Vec3::new(-0.5, 0.7, 0.5).normalize();
    for k in 1..n - 1 {
        for i in 1..n - 1 {
            let dx = h[k * n + i + 1] - h[k * n + i - 1];
            let dz = h[(k + 1) * n + i] - h[(k - 1) * n + i];
            let nrm = glam::Vec3::new(dx, 2.0 * step, dz).normalize();
            let shade = nrm.dot(light).max(0.0);
            let e = ((h[k * n + i] + 20.0) / 160.0).clamp(0.0, 1.0);
            let c = [0.55 + 0.4 * e, 0.35 + 0.3 * e, 0.25 + 0.2 * e].map(|v| (255.0 * v * (0.35 + 0.75 * shade)).clamp(0.0, 255.0) as u8);
            img[k * n + i] = c;
        }
    }
    let px = |p: Vec2| -> Option<(usize, usize)> {
        let i = (mid.x + half - p.x) / step;
        let k = (mid.y + half - p.y) / step;
        (i >= 0.0 && k >= 0.0 && (i as usize) < n && (k as usize) < n).then_some((i as usize, k as usize))
    };
    let dot = |img: &mut Vec<[u8; 3]>, p: Vec2, r: f32, c: [u8; 3]| {
        let rp = (r / step).max(1.0) as i32;
        if let Some((i, k)) = px(p) {
            for dk in -rp..=rp {
                for di in -rp..=rp {
                    if di * di + dk * dk > rp * rp {
                        continue;
                    }
                    let (a, b) = (i as i32 + di, k as i32 + dk);
                    if a >= 0 && b >= 0 && (a as usize) < n && (b as usize) < n {
                        img[b as usize * n + a as usize] = c;
                    }
                }
            }
        }
    };
    // Grid every 4 cells.
    for k in 0..n {
        for i in 0..n {
            let p = world(i, k);
            let gx = (p.x / (4.0 * CELL)).rem_euclid(1.0);
            let gz = (p.y / (4.0 * CELL)).rem_euclid(1.0);
            if gx < step / (4.0 * CELL) || gz < step / (4.0 * CELL) {
                let c = &mut img[k * n + i];
                *c = c.map(|v| (v as f32 * 0.8) as u8);
            }
        }
    }
    for p in &built.props {
        dot(&mut img, Vec2::new(p.position.x, p.position.z), p.radius, [40, 40, 40]);
    }
    for (j, p) in route.iter().enumerate() {
        let c = if j < 10 { [0, 200, 0] } else { [30, 60, 220] };
        dot(&mut img, Vec2::new(p.x, p.z), 4.0, c);
    }
    let mut data = format!("P6 {n} {n} 255\n").into_bytes();
    for c in img {
        data.extend_from_slice(&c);
    }
    std::fs::write(&out, data).expect("write");
    let shown = std::fs::canonicalize(&out).map(|p| p.display().to_string()).unwrap_or(out);
    println!("wrote {shown} (open it in Preview)");
    println!("centre ({:.0}, {:.0}), {n} px, {step} m/px, image x = −X (east right), up = +Z (north)", mid.x, mid.y);
    let t = built.triangles;
    println!("triangles: blocks {} · terrain {} · scenery {} · structures {} + {} drawn", t.blocks, t.terrain, t.scenery, t.structures, t.structures_drawn);
}
