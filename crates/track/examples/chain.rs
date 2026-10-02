//! Turns a chain of block ids into a validated map file.
//!
//! `cargo run -p track --example chain -- maps/olympus.chain` writes `maps/olympus.json`.
//!
//! Chain format, one directive per line, `#` starts a comment:
//!
//! ```text
//! name Olympus
//! seed 4242                 # terrain seed (optional)
//! start 0 0 0 0             # entry cell x z, level, rotation of the start block [variant]
//! straight
//! turn2_right
//! slope3_up1
//! turn2_left dirt           # optional deck variant
//! prop spire 470 0 -170 15 9   # scenery: kind x y z yaw scale
//! landform butte 430 -400 46 18.5 24 90   # kind x z radius height [length yaw]
//! terrain hills 10          # any other terrain setting
//! version 2                 # revision of the map (records are kept per version)
//! ```
//!
//! The first block must be `start` and the last `finish`. The tool refuses blocks that
//! overlap at the same height, warns about dirt blocks side by side (their corridors need a
//! free cell between them to widen and dig their banks), then prints the length, an estimated
//! time and the bounds.

use std::collections::BTreeMap;

use track::kit::{CELL, Connector, Heading, Layout};
use track::map::{Map, blocks_from_layout, parse_block};
use track::terrain::TerrainSettings;

fn main() {
    let path = std::env::args().nth(1).expect("usage: chain <file.chain>");
    let text = std::fs::read_to_string(&path).expect("read chain file");
    let mut name = String::from("Sans nom");
    let mut terrain = TerrainSettings::default();
    let mut layout: Option<Layout> = None;
    let mut props = Vec::new();
    let mut landforms = Vec::new();
    let mut version = 1;

    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let fail = |msg: &str| -> ! {
            eprintln!("{path}:{}: {msg}: {raw}", n + 1);
            std::process::exit(1)
        };
        match words[0] {
            "name" => name = words[1..].join(" "),
            "seed" => terrain.seed = words.get(1).and_then(|s| s.parse().ok()).unwrap_or_else(|| fail("bad seed")),
            "version" => version = words.get(1).and_then(|s| s.parse().ok()).unwrap_or_else(|| fail("bad version")),
            "terrain" => {
                let mut t = serde_json::to_value(&terrain).expect("terrain");
                let value: serde_json::Value = words.get(2).and_then(|v| serde_json::from_str(v).ok()).unwrap_or_else(|| fail("terrain needs: setting value"));
                t[words[1]] = value;
                terrain = serde_json::from_value(t).unwrap_or_else(|e| fail(&format!("{e}")));
            }
            "landform" => {
                let f: Vec<f32> = words[2..].iter().filter_map(|w| w.parse().ok()).collect();
                if f.len() != 4 && f.len() != 6 {
                    fail("landform needs: kind x z radius height [length yaw]");
                }
                let mut l = serde_json::json!({"landform": words[1], "position": [f[0], f[1]], "radius": f[2], "height": f[3]});
                if f.len() == 6 {
                    l["length"] = f[4].into();
                    l["yaw"] = f[5].into();
                }
                landforms.push(l);
            }
            "start" => {
                let v: Vec<i32> = words[1..].iter().filter_map(|w| w.parse().ok()).collect();
                if v.len() != 4 {
                    fail("start needs: cell_x cell_z level rotation [variant]");
                }
                let heading = Heading::from_rotation(v[3] as u8).unwrap_or_else(|| fail("rotation must be 0..3"));
                let mut l = Layout::new(&name, Connector::entering((v[0], v[1]), v[2], heading));
                l.push(parse_block("start", words.get(5).copied()).unwrap_or_else(|_| fail("start")));
                layout = Some(l);
            }
            "prop" => {
                let f: Vec<f32> = words[2..].iter().filter_map(|w| w.parse().ok()).collect();
                if f.len() != 5 {
                    fail("prop needs: kind x y z yaw scale");
                }
                props.push(serde_json::json!({"prop": words[1], "position": [f[0], f[1], f[2]], "yaw": f[3], "scale": f[4]}));
            }
            id => {
                let Some(l) = layout.as_mut() else { fail("blocks must come after `start`") };
                let piece = parse_block(id, words.get(1).copied()).unwrap_or_else(|e| fail(&format!("{e:?}")));
                l.push(piece);
            }
        }
    }
    let mut layout = layout.expect("no start");
    layout.name = name.clone();

    // Overlaps: two pieces on the same cell whose height ranges are closer than one level.
    let mut used: BTreeMap<(i32, i32), Vec<(usize, f32, f32)>> = BTreeMap::new();
    let mut overlaps = Vec::new();
    for (i, p) in layout.pieces.iter().enumerate() {
        let (lo, hi) = (p.entry.pos.y.min(p.exit.pos.y), p.entry.pos.y.max(p.exit.pos.y));
        for c in p.cells() {
            let list = used.entry(c).or_default();
            for &(j, l2, h2) in list.iter() {
                if j + 1 != i && lo < h2 + 6.0 && l2 < hi + 6.0 {
                    overlaps.push((j, i, c));
                }
            }
            list.push((i, lo, hi));
        }
    }
    // Dirt blocks side by side: their corridors merge instead of widening.
    for (i, p) in layout.pieces.iter().enumerate() {
        if p.piece.deck != track::Surface::Dirt {
            continue;
        }
        let y = p.entry.pos.y.min(p.exit.pos.y);
        for (j, q) in layout.pieces.iter().enumerate().skip(i + 2) {
            let near = p.cells().iter().any(|a| q.cells().iter().any(|b| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1));
            if near && (q.entry.pos.y.min(q.exit.pos.y) - y).abs() < 6.0 {
                eprintln!("tight: dirt piece {i} and piece {j} side by side");
            }
        }
    }
    if !overlaps.is_empty() {
        for (a, b, c) in &overlaps {
            eprintln!("overlap: piece {a} and piece {b} on cell {c:?}");
        }
        std::process::exit(1);
    }

    let blocks = blocks_from_layout(&layout).expect("blocks");
    let map = Map {
        format: track::map::FORMAT,
        name: name.clone(),
        author: "mars-racer".into(),
        version,
        terrain,
        blocks,
        landforms: serde_json::from_value(serde_json::Value::Array(landforms)).expect("landforms"),
        scenery: serde_json::from_value(serde_json::Value::Array(props)).expect("props"),
    };
    let json = map.to_json();
    let checked = Map::load(&json).unwrap_or_else(|e| {
        eprintln!("invalid map: {e}");
        std::process::exit(1)
    });
    let built = checked.build_detailed().expect("build");
    let length = layout.length();
    let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN));
    for p in &layout.pieces {
        for (x, z) in p.cells() {
            lo = (lo.0.min(x), lo.1.min(z));
            hi = (hi.0.max(x), hi.1.max(z));
        }
    }
    let out = std::path::Path::new(&path).with_extension("json");
    std::fs::write(&out, json).expect("write json");
    println!(
        "{name}: {} blocks, {:.0} m, ~{:.1} s at 200 km/h, {} checkpoints, cells x {}..{} z {}..{} ({:.0} x {:.0} m), {} triangles -> {}",
        layout.pieces.len(),
        length,
        length / (200.0 / 3.6),
        built.track.checkpoints.len(),
        lo.0,
        hi.0,
        lo.1,
        hi.1,
        (hi.0 - lo.0 + 1) as f32 * CELL,
        (hi.1 - lo.1 + 1) as f32 * CELL,
        built.track.mesh.triangle_count(),
        out.display()
    );
}
