//! Writes docs/track-plan.svg: a top view of the demo map (surface and height, block outlines,
//! start, checkpoints, finish), its elevation profile, and the jump's ballistic trajectories for
//! several speeds and gravities over the landing descent. Also prints a summary.
//!
//!     cargo run -p track --example plan

use std::fmt::Write as _;

use glam::Vec3;
use track::jump::{self, LandingProfile};
use track::kit::{self, CELL, Gate, HALF_WIDTH, Kind, Layout};
use track::{Surface, demo};

// Reference palette (light surface).
const SURFACE: &str = "#fcfcfb";
const TEXT: &str = "#0b0b0b";
const TEXT_2: &str = "#52514e";
const MUTED: &str = "#8a8984";
const GRID: &str = "#e9e8e4";
const TERRAIN: &str = "#f6ede6";
/// Sequential blue, light → dark, for road height.
const BLUE: [&str; 7] = ["#9ec5f4", "#6da7ec", "#3987e5", "#2a78d6", "#256abf", "#1c5cab", "#104281"];
const DIRT: &str = "#eb6834";
const DIRT_HIGH: &str = "#b8461c";
const START: &str = "#008300";
const CHECKPOINT: &str = "#4a3aa7";
const FINISH: &str = "#e34948";
/// Categorical slots 1-4 for the gravities.
const SERIES: [&str; 4] = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"];
const GRAVITIES: [(f32, &str); 4] = [(3.71, "Mars 3.71"), (9.81, "Earth 9.81"), (20.0, "20"), (40.0, "Reference 40")];
const SPEEDS: [(f32, &str); 3] = [(150.0, "6 4"), (200.0, ""), (250.0, "1.5 3")];

const WIDTH: f32 = 1240.0;
const MARGIN: f32 = 40.0;

fn road_color(h: f32) -> &'static str {
    let i = ((h / 18.0) * (BLUE.len() - 1) as f32).round().clamp(0.0, (BLUE.len() - 1) as f32) as usize;
    BLUE[i]
}

fn main() {
    let layout = demo::layout();
    let track = layout.build();
    let landing_piece = layout.pieces.iter().position(|p| p.landing().is_some()).expect("a jump");
    let landing = layout.pieces[landing_piece].landing().unwrap();
    let lip = layout.pieces[landing_piece].entry.pos;

    let mut svg = String::new();
    let mut y = MARGIN;
    let top_h = top_view(&mut svg, &layout, y);
    y += top_h + 50.0;
    y += profile(&mut svg, &layout, y) + 50.0;
    y += jump_view(&mut svg, &layout, landing_piece, &landing, y) + 40.0;
    y += envelope_table(&mut svg, &landing, lip.y, y) + MARGIN;

    let doc = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{y:.0}\" viewBox=\"0 0 {WIDTH} {y:.0}\" \
         font-family=\"system-ui, -apple-system, Helvetica, sans-serif\" font-size=\"12\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{SURFACE}\"/>\n{svg}</svg>\n"
    );
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/track-plan.svg");
    std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs")).unwrap();
    std::fs::write(path, doc).unwrap();

    // Summary.
    let length = layout.length();
    println!("{}: {} pieces, {:.0} m, {} triangles", layout.name, layout.pieces.len(), length, track.mesh.triangle_count());
    println!("run at 200 km/h average: {:.1} s", length / (200.0 / 3.6));
    for (i, p) in layout.pieces.iter().enumerate() {
        let extra = match p.piece.kind {
            Kind::Slope { cells, levels } => {
                let (sag, crest) = kit::slope_radii(cells, levels);
                format!("  sag R {sag:.0} m, crest R {crest:.0} m")
            }
            _ => String::new(),
        };
        println!(
            "{i:2} {:<70} y {:5.1} -> {:5.1}{extra}",
            format!("{:?}{}", p.piece.kind, p.piece.gate.map(|g| format!(" [{g:?}]")).unwrap_or_default()),
            p.entry.pos.y,
            p.exit.pos.y
        );
    }
    let (k_min, k_max) = landing.k_range();
    println!("landing: C {:.3e}, knee {:.0} m, k in [{k_min:.3e}, {k_max:.3e}]", landing.c, landing.knee);
    for g in [3.71, 9.81, 15.0, 20.0, 30.0, 40.0] {
        match jump::envelope_kmh(&landing, g, 5.0) {
            Some((lo, hi)) => println!("  g {g:5.2}: clean (<= 5 deg) from {lo:.0} to {hi:.0} km/h at the lip"),
            None => println!("  g {g:5.2}: never clean"),
        }
    }
    println!("wrote {path}");
}

/// Top view. Returns its height.
fn top_view(svg: &mut String, layout: &Layout, y0: f32) -> f32 {
    let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
    for p in &layout.pieces {
        for (i, k) in p.cells() {
            lo = (lo.0.min(i), lo.1.min(k));
            hi = (hi.0.max(i), hi.1.max(k));
        }
    }
    let (x_min, x_max) = ((lo.0 - 1) as f32 * CELL, (hi.0 + 2) as f32 * CELL);
    let (z_min, z_max) = ((lo.1 - 1) as f32 * CELL, (hi.1 + 2) as f32 * CELL);
    let sc = (WIDTH - 2.0 * MARGIN) / (x_max - x_min);
    let height = (z_max - z_min) * sc;
    // East (−X) to the right, north (+Z) up: a true top view.
    let to = |p: Vec3| (MARGIN + (x_max - p.x) * sc, y0 + 24.0 + (z_max - p.z) * sc);

    let _ = writeln!(
        svg,
        "<text x=\"{MARGIN}\" y=\"{y0}\" font-size=\"16\" font-weight=\"600\" fill=\"{TEXT}\">{} — top view \
         <tspan font-size=\"12\" font-weight=\"400\" fill=\"{TEXT_2}\">(north up, grid 32 m, block outlines dashed, numbers = piece index)</tspan></text>",
        layout.name
    );
    let _ = writeln!(svg, "<rect x=\"{MARGIN}\" y=\"{}\" width=\"{}\" height=\"{height}\" fill=\"{TERRAIN}\"/>", y0 + 24.0, WIDTH - 2.0 * MARGIN);
    // Grid.
    let mut x = x_min;
    while x <= x_max + 1e-3 {
        let (sx, _) = to(Vec3::new(x, 0.0, 0.0));
        let _ = writeln!(svg, "<line x1=\"{sx:.1}\" y1=\"{:.1}\" x2=\"{sx:.1}\" y2=\"{:.1}\" stroke=\"{GRID}\" stroke-width=\"1\"/>", y0 + 24.0, y0 + 24.0 + height);
        x += CELL;
    }
    let mut z = z_min;
    while z <= z_max + 1e-3 {
        let (_, sy) = to(Vec3::new(0.0, 0.0, z));
        let _ = writeln!(svg, "<line x1=\"{MARGIN}\" y1=\"{sy:.1}\" x2=\"{:.1}\" y2=\"{sy:.1}\" stroke=\"{GRID}\" stroke-width=\"1\"/>", WIDTH - MARGIN);
        z += CELL;
    }
    // Block outlines.
    for (idx, p) in layout.pieces.iter().enumerate() {
        let _ = write!(svg, "<g><title>piece {idx}: {:?}</title>", p.piece.kind);
        for (i, k) in p.cells() {
            let (sx, sy) = to(Vec3::new((i + 1) as f32 * CELL, 0.0, (k + 1) as f32 * CELL));
            let _ = write!(
                svg,
                "<rect x=\"{sx:.1}\" y=\"{sy:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"{MUTED}\" stroke-width=\"0.8\" stroke-dasharray=\"3 3\"/>",
                CELL * sc,
                CELL * sc
            );
        }
        let _ = writeln!(svg, "</g>");
    }
    // Decks, one quad per sample step, coloured by surface and height.
    for p in &layout.pieces {
        let ss = p.samples();
        for w in ss.windows(2) {
            let (fa, fb) = (p.frame(w[0]), p.frame(w[1]));
            let fm = p.frame(0.5 * (w[0] + w[1]));
            let h = fm.centre().y;
            let color = match fm.deck {
                Surface::Dirt if h > 1.0 => DIRT_HIGH,
                Surface::Dirt => DIRT,
                _ => road_color(h),
            };
            let pts = [fa.deck_point(HALF_WIDTH), fb.deck_point(HALF_WIDTH), fb.deck_point(-HALF_WIDTH), fa.deck_point(-HALF_WIDTH)]
                .map(|q| {
                    let (sx, sy) = to(q);
                    format!("{sx:.1},{sy:.1}")
                })
                .join(" ");
            let _ = writeln!(svg, "<polygon points=\"{pts}\" fill=\"{color}\" stroke=\"{color}\" stroke-width=\"0.6\"/>");
        }
        // Platform edges: a dark outline where the deck is more than a metre up.
        for u in [HALF_WIDTH, -HALF_WIDTH] {
            let mut run: Vec<String> = Vec::new();
            let flush = |svg: &mut String, run: &mut Vec<String>| {
                if run.len() > 1 {
                    let _ = writeln!(svg, "<polyline points=\"{}\" fill=\"none\" stroke=\"{TEXT}\" stroke-width=\"1.2\"/>", run.join(" "));
                }
                run.clear();
            };
            for &s in &ss {
                let q = p.frame(s).deck_point(u);
                if q.y > 1.0 {
                    let (sx, sy) = to(q);
                    run.push(format!("{sx:.1},{sy:.1}"));
                } else {
                    flush(svg, &mut run);
                }
            }
            flush(svg, &mut run);
        }
    }
    // Gates.
    let mut cp = 0;
    for p in &layout.pieces {
        let Some(g) = p.piece.gate else { continue };
        let f = p.frame(p.gate_s());
        let (color, label) = match g {
            Gate::Start => (START, "START".to_string()),
            Gate::Checkpoint => {
                cp += 1;
                (CHECKPOINT, format!("CP{cp}"))
            }
            Gate::Finish => (FINISH, "FINISH".to_string()),
        };
        let (ax, ay) = to(f.deck_point(HALF_WIDTH + 2.0));
        let (bx, by) = to(f.deck_point(-HALF_WIDTH - 2.0));
        let _ = writeln!(svg, "<line x1=\"{ax:.1}\" y1=\"{ay:.1}\" x2=\"{bx:.1}\" y2=\"{by:.1}\" stroke=\"{color}\" stroke-width=\"3\" stroke-linecap=\"round\"/>");
        let (lx, ly) = to(f.deck_point(-HALF_WIDTH - 4.0));
        let anchor = if f.left.x > 0.5 { "start" } else if f.left.x < -0.5 { "end" } else { "middle" };
        let dy = if f.left.z > 0.5 { 14.0 } else if f.left.z < -0.5 { -6.0 } else { 4.0 };
        let _ = writeln!(svg, "<text x=\"{lx:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" font-weight=\"600\" fill=\"{TEXT}\">{label}</text>", ly + dy);
    }
    // Start arrow.
    let start = layout.start_pose();
    let fwd = Vec3::new(start.yaw.sin(), 0.0, start.yaw.cos());
    let (sx, sy) = to(start.position);
    let (tx, ty) = to(start.position + fwd * 14.0);
    let _ = writeln!(svg, "<line x1=\"{sx:.1}\" y1=\"{sy:.1}\" x2=\"{tx:.1}\" y2=\"{ty:.1}\" stroke=\"{START}\" stroke-width=\"2\"/><circle cx=\"{sx:.1}\" cy=\"{sy:.1}\" r=\"4\" fill=\"{START}\"/>");
    // Piece numbers.
    for (idx, p) in layout.pieces.iter().enumerate() {
        let f = p.frame(0.5 * (p.deck_range().0 + p.length));
        let (cx, cy) = to(f.centre());
        let _ = writeln!(
            svg,
            "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"8\" fill=\"{SURFACE}\" stroke=\"{TEXT_2}\" stroke-width=\"0.8\"/>\
             <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"{TEXT}\">{idx}</text>",
            cy + 3.3
        );
    }
    // Jump annotation.
    if let Some(i) = layout.pieces.iter().position(|p| matches!(p.piece.kind, Kind::JumpRamp { .. })) {
        let (jx, jy) = to(layout.pieces[i].exit.pos + layout.pieces[i].exit.heading.left() * (HALF_WIDTH + 8.0));
        let _ = writeln!(svg, "<text x=\"{jx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{TEXT}\">jump: lip, 8 m gap</text>", jy - 4.0);
    }
    // Scale bar, north arrow, legend (bottom-left corner of the map).
    let base_y = y0 + 24.0 + height - 16.0;
    let bar = 100.0 * sc;
    let _ = writeln!(
        svg,
        "<line x1=\"{0}\" y1=\"{base_y}\" x2=\"{1}\" y2=\"{base_y}\" stroke=\"{TEXT}\" stroke-width=\"2\"/>\
         <text x=\"{0}\" y=\"{2}\" fill=\"{TEXT_2}\">100 m</text>",
        MARGIN + 12.0,
        MARGIN + 12.0 + bar,
        base_y - 6.0
    );
    let nx = MARGIN + 30.0;
    let ny = y0 + 50.0;
    let _ = writeln!(
        svg,
        "<path d=\"M{nx} {} L{} {} L{nx} {} L{} {} Z\" fill=\"{TEXT}\"/><text x=\"{nx}\" y=\"{}\" text-anchor=\"middle\" font-weight=\"600\" fill=\"{TEXT}\">N</text>",
        ny - 12.0,
        nx + 6.0,
        ny + 6.0,
        ny + 2.0,
        nx - 6.0,
        ny + 6.0,
        ny + 20.0
    );
    let mut lx = MARGIN + 160.0;
    let legend: Vec<(String, &str)> = vec![
        ("road at 0 m".into(), road_color(0.0)),
        ("road at 8 m".into(), road_color(8.0)),
        ("road at 16 m".into(), road_color(16.0)),
        ("dirt".into(), DIRT),
        ("dirt berm (raised)".into(), DIRT_HIGH),
    ];
    for (label, color) in legend {
        let _ = writeln!(
            svg,
            "<rect x=\"{lx}\" y=\"{}\" width=\"14\" height=\"10\" rx=\"2\" fill=\"{color}\"/><text x=\"{}\" y=\"{}\" fill=\"{TEXT_2}\">{label}</text>",
            base_y - 9.0,
            lx + 19.0,
            base_y
        );
        lx += 30.0 + 6.2 * label.len() as f32;
    }
    let _ = writeln!(
        svg,
        "<line x1=\"{lx}\" y1=\"{0}\" x2=\"{1}\" y2=\"{0}\" stroke=\"{TEXT}\" stroke-width=\"1.2\"/><text x=\"{2}\" y=\"{3}\" fill=\"{TEXT_2}\">platform edge (walls down to the ground)</text>",
        base_y - 4.0,
        lx + 14.0,
        lx + 19.0,
        base_y
    );
    height + 24.0
}

/// Elevation profile along the driving line. Returns its height.
fn profile(svg: &mut String, layout: &Layout, y0: f32) -> f32 {
    let pts = layout.route_points(0, 0.0, layout.pieces.len() - 1, 2.0);
    let total = layout.length();
    let (plot_x, plot_w) = (MARGIN + 40.0, WIDTH - 2.0 * MARGIN - 50.0);
    let (plot_y, plot_h) = (y0 + 30.0, 180.0);
    let (h_min, h_max) = (-2.0, 20.0);
    let to = |d: f32, h: f32| (plot_x + d / total * plot_w, plot_y + (h_max - h) / (h_max - h_min) * plot_h);
    let _ = writeln!(
        svg,
        "<text x=\"{MARGIN}\" y=\"{y0}\" font-size=\"16\" font-weight=\"600\" fill=\"{TEXT}\">Elevation along the route \
         <tspan font-size=\"12\" font-weight=\"400\" fill=\"{TEXT_2}\">({total:.0} m; height of the deck centre, vertical scale ×{:.0})</tspan></text>",
        (plot_h / (h_max - h_min)) / (plot_w / total)
    );
    for h in [0.0, 8.0, 16.0] {
        let (_, sy) = to(0.0, h);
        let _ = writeln!(
            svg,
            "<line x1=\"{plot_x}\" y1=\"{sy:.1}\" x2=\"{:.1}\" y2=\"{sy:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{TEXT_2}\">{h:.0} m</text>",
            plot_x + plot_w,
            plot_x - 6.0,
            sy + 4.0
        );
    }
    let mut d = 0.0;
    while d <= total {
        let (sx, sy) = to(d, h_min);
        let _ = writeln!(svg, "<text x=\"{sx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{TEXT_2}\">{d:.0}</text>", sy + 30.0);
        d += 250.0;
    }
    // Piece boundaries.
    let mut acc = 0.0;
    for (i, p) in layout.pieces.iter().enumerate() {
        let (sx, sy) = to(acc, h_min);
        let _ = writeln!(svg, "<line x1=\"{sx:.1}\" y1=\"{sy:.1}\" x2=\"{sx:.1}\" y2=\"{:.1}\" stroke=\"{MUTED}\" stroke-width=\"0.8\"/>", sy + 5.0);
        let (mx, _) = to(acc + 0.5 * p.length, h_min);
        let _ = writeln!(svg, "<text x=\"{mx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"9.5\" fill=\"{MUTED}\">{i}</text>", sy + 14.0);
        acc += p.length;
    }
    // Gates.
    let mut acc = 0.0;
    let mut cp = 0;
    for p in &layout.pieces {
        if let Some(g) = p.piece.gate {
            let (color, label) = match g {
                Gate::Start => (START, "START".to_string()),
                Gate::Checkpoint => {
                    cp += 1;
                    (CHECKPOINT, format!("CP{cp}"))
                }
                Gate::Finish => (FINISH, "FINISH".to_string()),
            };
            let (sx, _) = to(acc + p.gate_s(), 0.0);
            let _ = writeln!(
                svg,
                "<line x1=\"{sx:.1}\" y1=\"{plot_y}\" x2=\"{sx:.1}\" y2=\"{:.1}\" stroke=\"{color}\" stroke-width=\"1.5\" stroke-dasharray=\"4 3\"/>\
                 <text x=\"{sx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-weight=\"600\" fill=\"{TEXT}\">{label}</text>",
                plot_y + plot_h,
                plot_y - 4.0
            );
        }
        acc += p.length;
    }
    // Profile, coloured by surface; the jump gap dashed.
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let deck = layout.pieces[b.piece].frame(b.s).deck;
        let (color, dash) = if !b.on_deck {
            (TEXT_2, " stroke-dasharray=\"2 3\"")
        } else if deck == Surface::Dirt {
            (DIRT, "")
        } else {
            (BLUE[4], "")
        };
        let (ax, ay) = to(a.dist, a.pos.y);
        let (bx, by) = to(b.dist, b.pos.y);
        let _ = writeln!(svg, "<line x1=\"{ax:.1}\" y1=\"{ay:.1}\" x2=\"{bx:.1}\" y2=\"{by:.1}\" stroke=\"{color}\" stroke-width=\"2.5\"{dash}/>");
    }
    let ly = plot_y + plot_h + 52.0;
    let _ = writeln!(
        svg,
        "<line x1=\"{plot_x}\" y1=\"{0}\" x2=\"{1}\" y2=\"{0}\" stroke=\"{2}\" stroke-width=\"2.5\"/><text x=\"{3}\" y=\"{4}\" fill=\"{TEXT_2}\">road</text>\
         <line x1=\"{5}\" y1=\"{0}\" x2=\"{6}\" y2=\"{0}\" stroke=\"{DIRT}\" stroke-width=\"2.5\"/><text x=\"{7}\" y=\"{4}\" fill=\"{TEXT_2}\">dirt</text>\
         <line x1=\"{8}\" y1=\"{0}\" x2=\"{9}\" y2=\"{0}\" stroke=\"{TEXT_2}\" stroke-width=\"2.5\" stroke-dasharray=\"2 3\"/><text x=\"{10}\" y=\"{4}\" fill=\"{TEXT_2}\">jump gap</text>\
         <text x=\"{11}\" y=\"{4}\" fill=\"{MUTED}\">small numbers: piece index · x axis: metres from the start block</text>",
        ly - 4.0,
        plot_x + 20.0,
        BLUE[4],
        plot_x + 25.0,
        ly,
        plot_x + 70.0,
        plot_x + 90.0,
        plot_x + 95.0,
        plot_x + 140.0,
        plot_x + 160.0,
        plot_x + 165.0,
        plot_x + 250.0
    );
    ly - y0 + 6.0
}

/// Side view of the jump with trajectories. Returns its height.
fn jump_view(svg: &mut String, layout: &Layout, landing_piece: usize, p: &LandingProfile, y0: f32) -> f32 {
    let ramp = &layout.pieces[landing_piece - 1];
    let lip_y = ramp.exit.pos.y;
    let (x_min, x_max) = (-40.0, p.length + 64.0);
    let (h_min, h_max) = (kit::TERRAIN_Y - lip_y - 1.0, 9.0);
    let exaggeration = 3.0;
    let (plot_x, plot_w) = (MARGIN + 40.0, WIDTH - 2.0 * MARGIN - 50.0);
    let sx_scale = plot_w / (x_max - x_min);
    let plot_h = (h_max - h_min) * sx_scale * exaggeration;
    let plot_y = y0 + 56.0;
    let to = |x: f32, h: f32| (plot_x + (x - x_min) * sx_scale, plot_y + (h_max - h) * sx_scale * exaggeration);
    let _ = writeln!(
        svg,
        "<text x=\"{MARGIN}\" y=\"{y0}\" font-size=\"16\" font-weight=\"600\" fill=\"{TEXT}\">Jump: flights from the lip over the landing descent \
         <tspan font-size=\"12\" font-weight=\"400\" fill=\"{TEXT_2}\">(point mass, no drag; heights from the lip, vertical scale ×{exaggeration:.0})</tspan></text>\
         <text x=\"{MARGIN}\" y=\"{}\" fill=\"{TEXT_2}\">Lip {:.1}° at {lip_y:.1} m, gap {:.0} m, landing {:.0} m long down to the ground. \
         On its parabolic part (up to the knee) every car touches down {:.1}° steeper than the slope, whatever its speed and gravity.</text>\
         <text x=\"{MARGIN}\" y=\"{}\" fill=\"{TEXT_2}\">Dots mark touchdowns, crosses cars that fall short and hit the front of the landing. Hover a curve for its numbers.</text>",
        y0 + 18.0,
        p.lip_grade.atan().to_degrees(),
        p.gap,
        p.length,
        (p.epsilon).atan().to_degrees(),
        y0 + 34.0
    );
    for h in [0.0, -8.0, -16.0] {
        let (_, sy) = to(x_min, h);
        let _ = writeln!(
            svg,
            "<line x1=\"{plot_x}\" y1=\"{sy:.1}\" x2=\"{:.1}\" y2=\"{sy:.1}\" stroke=\"{GRID}\"/><text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"{TEXT_2}\">{h:.0} m</text>",
            plot_x + plot_w,
            plot_x - 6.0,
            sy + 4.0
        );
    }
    let mut x = 0.0;
    while x <= x_max {
        let (sx, _) = to(x, h_min);
        let _ = writeln!(
            svg,
            "<line x1=\"{sx:.1}\" y1=\"{plot_y}\" x2=\"{sx:.1}\" y2=\"{:.1}\" stroke=\"{GRID}\"/><text x=\"{sx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"{TEXT_2}\">{x:.0} m</text>",
            plot_y + plot_h,
            plot_y + plot_h + 16.0
        );
        x += 32.0;
    }
    // Terrain, ramp, landing and the flat after it.
    let (gx0, gy) = to(x_min, kit::TERRAIN_Y - lip_y);
    let (gx1, _) = to(x_max, 0.0);
    let _ = writeln!(svg, "<line x1=\"{gx0:.1}\" y1=\"{gy:.1}\" x2=\"{gx1:.1}\" y2=\"{gy:.1}\" stroke=\"{DIRT}\" stroke-width=\"2\"/>");
    let mut deck = vec![to(x_min, ramp.entry.pos.y - lip_y)];
    let mut s = (ramp.length + x_min).max(0.0);
    while s <= ramp.length {
        deck.push(to(s - ramp.length, ramp.frame(s).centre().y - lip_y));
        s += 1.0;
    }
    let deck_pts = |v: &[(f32, f32)]| v.iter().map(|(a, b)| format!("{a:.1},{b:.1}")).collect::<Vec<_>>().join(" ");
    let mut outline = deck.clone();
    outline.push(to(0.0, kit::TERRAIN_Y - lip_y));
    outline.push(to(x_min, kit::TERRAIN_Y - lip_y));
    let _ = writeln!(svg, "<polygon points=\"{}\" fill=\"{GRID}\" stroke=\"none\"/>", deck_pts(&outline));
    let _ = writeln!(svg, "<polyline points=\"{}\" fill=\"none\" stroke=\"{TEXT}\" stroke-width=\"2.5\"/>", deck_pts(&deck));
    let mut land = Vec::new();
    let mut x = p.gap;
    while x <= x_max {
        land.push(to(x, p.height(x)));
        x += 1.0;
    }
    let mut outline = land.clone();
    outline.push(to(x_max, kit::TERRAIN_Y - lip_y));
    outline.push(to(p.gap, kit::TERRAIN_Y - lip_y));
    let _ = writeln!(svg, "<polygon points=\"{}\" fill=\"{GRID}\" stroke=\"none\"/>", deck_pts(&outline));
    let _ = writeln!(svg, "<polyline points=\"{}\" fill=\"none\" stroke=\"{TEXT}\" stroke-width=\"2.5\"/>", deck_pts(&land));
    let (kx, ky) = to(p.knee, p.height(p.knee));
    let _ = writeln!(
        svg,
        "<line x1=\"{kx:.1}\" y1=\"{:.1}\" x2=\"{kx:.1}\" y2=\"{:.1}\" stroke=\"{TEXT_2}\"/><text x=\"{:.1}\" y=\"{:.1}\" fill=\"{TEXT_2}\">knee: the outrun starts</text>",
        ky - 6.0,
        ky - 30.0,
        kx + 4.0,
        ky - 22.0
    );
    // Trajectories.
    for (gi, (g, name)) in GRAVITIES.iter().enumerate() {
        for (kmh, dash) in SPEEDS {
            let v = kmh / 3.6;
            let t = p.lip_grade;
            let vx = v / (1.0 + t * t).sqrt();
            let td = jump::fly(p, v, *g);
            let x_end = td.map_or(p.gap, |d| d.x.min(x_max));
            let mut pts = Vec::new();
            let mut x = 0.0;
            loop {
                let h = t * x - g * x * x / (2.0 * vx * vx);
                if h < h_min || x > x_end {
                    break;
                }
                pts.push(to(x, h));
                x += 0.5;
            }
            if let Some(d) = td.filter(|d| d.x <= x_max) {
                pts.push(to(d.x, d.y));
            }
            if td.is_none() {
                // Hits the front face of the landing.
                let h = t * p.gap - g * p.gap * p.gap / (2.0 * vx * vx);
                pts.push(to(p.gap, h));
            }
            let dash_attr = if dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{dash}\"") };
            let tip = match td {
                Some(d) => format!(
                    "g {g} m/s², {kmh:.0} km/h: lands {:.0} m from the lip after {:.2} s, {:.1}° steeper than the slope, {:.1} m/s into it",
                    d.x, d.air_time, d.mismatch_deg, d.normal_speed
                ),
                None => format!("g {g} m/s², {kmh:.0} km/h: falls into the gap"),
            };
            let _ = writeln!(
                svg,
                "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2\"{dash_attr}><title>{tip}</title></polyline>",
                deck_pts(&pts),
                SERIES[gi]
            );
            if td.is_none() {
                let h = t * p.gap - g * p.gap * p.gap / (2.0 * vx * vx);
                let (cx, cy) = to(p.gap, h);
                let _ = writeln!(
                    svg,
                    "<path d=\"M{:.1} {:.1} l8 8 m0 -8 l-8 8\" stroke=\"{}\" stroke-width=\"2.5\"><title>{tip}</title></path>",
                    cx - 4.0,
                    cy - 4.0,
                    SERIES[gi]
                );
            }
            if let Some(d) = td.filter(|d| d.x <= x_max) {
                let (cx, cy) = to(d.x, d.y);
                let _ = writeln!(
                    svg,
                    "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"4\" fill=\"{}\" stroke=\"{SURFACE}\" stroke-width=\"2\"><title>{tip}</title></circle>",
                    SERIES[gi]
                );
            }
        }
        let _ = name;
    }
    // Legend: gravity by colour, speed by dash.
    let ly = plot_y + plot_h + 40.0;
    let mut lx = plot_x;
    for (gi, (_, name)) in GRAVITIES.iter().enumerate() {
        let _ = writeln!(
            svg,
            "<line x1=\"{lx}\" y1=\"{0}\" x2=\"{1}\" y2=\"{0}\" stroke=\"{2}\" stroke-width=\"2.5\"/><text x=\"{3}\" y=\"{ly}\" fill=\"{TEXT_2}\">g {name} m/s²</text>",
            ly - 4.0,
            lx + 22.0,
            SERIES[gi],
            lx + 27.0
        );
        lx += 90.0 + 6.8 * name.len() as f32;
    }
    lx += 20.0;
    for (kmh, dash) in SPEEDS {
        let dash_attr = if dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{dash}\"") };
        let _ = writeln!(
            svg,
            "<line x1=\"{lx}\" y1=\"{0}\" x2=\"{1}\" y2=\"{0}\" stroke=\"{TEXT_2}\" stroke-width=\"2\"{dash_attr}/><text x=\"{2}\" y=\"{ly}\" fill=\"{TEXT_2}\">{kmh:.0} km/h</text>",
            ly - 4.0,
            lx + 26.0,
            lx + 31.0
        );
        lx += 100.0;
    }
    ly - y0 + 6.0
}

/// Table of clean landing speeds per gravity. Returns its height.
fn envelope_table(svg: &mut String, p: &LandingProfile, lip_y: f32, y0: f32) -> f32 {
    let _ = writeln!(
        svg,
        "<text x=\"{MARGIN}\" y=\"{y0}\" font-size=\"16\" font-weight=\"600\" fill=\"{TEXT}\">Landing envelope \
         <tspan font-size=\"12\" font-weight=\"400\" fill=\"{TEXT_2}\">(lip speed range that clears the gap and touches down within 5° of the slope; lip at {lip_y:.1} m)</tspan></text>"
    );
    let cols = [MARGIN, MARGIN + 150.0, MARGIN + 340.0, MARGIN + 560.0, MARGIN + 780.0];
    let head = ["gravity (m/s²)", "clean lip speeds", "at 200 km/h", "at 250 km/h", "at 150 km/h"];
    let mut y = y0 + 28.0;
    for (c, h) in cols.iter().zip(head) {
        let _ = writeln!(svg, "<text x=\"{c}\" y=\"{y}\" font-weight=\"600\" fill=\"{TEXT}\">{h}</text>");
    }
    let _ = writeln!(svg, "<line x1=\"{MARGIN}\" y1=\"{0}\" x2=\"{1}\" y2=\"{0}\" stroke=\"{GRID}\"/>", y + 6.0, WIDTH - MARGIN);
    let describe = |g: f32, kmh: f32| match jump::fly(p, kmh / 3.6, g) {
        Some(d) => format!("{:.0} m, {:.2} s air, {:.1}°, {:.1} m/s", d.x, d.air_time, d.mismatch_deg, d.normal_speed),
        None => "falls into the gap".to_string(),
    };
    for g in [3.71, 9.81, 15.0, 20.0, 30.0, 40.0] {
        y += 22.0;
        let range = match jump::envelope_kmh(p, g, 5.0) {
            Some((lo, hi)) => format!("{lo:.0} – {hi:.0} km/h"),
            None => "none".to_string(),
        };
        let cells = [format!("{g}"), range, describe(g, 200.0), describe(g, 250.0), describe(g, 150.0)];
        for (c, t) in cols.iter().zip(cells) {
            let _ = writeln!(svg, "<text x=\"{c}\" y=\"{y}\" fill=\"{TEXT_2}\">{t}</text>");
        }
    }
    y += 26.0;
    let _ = writeln!(
        svg,
        "<text x=\"{MARGIN}\" y=\"{y}\" fill=\"{MUTED}\">Columns “at …”: touchdown distance from the lip, air time, how much steeper than the slope, speed into the surface.</text>"
    );
    y - y0 + 8.0
}
