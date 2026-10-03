//! What the scene renderer draws, made from the game's data: the track's mesh as GPU vertices
//! (its surfaces, the walls' materials, the earth carried onto the roads) and the cars' parts.
//! Shared by the game and the track editor.

use crate::car_model;
use crate::game::{CarMeshes, CornerMeshes};
use crate::gfx::{self, Gpu, MeshData, SceneRenderer, Shading, Vertex};

/// The track's mesh and its decoration (see `track::Track::decor`), drawn together.
pub fn track_render_data(track: &track::Track) -> MeshData {
    let mut mesh = track.mesh.clone();
    mesh.append(&track.decor);
    track_mesh_data(&mesh)
}

/// Track mesh to GPU vertices. The vertex kind is the triangle's surface; the terrain shares its
/// vertices between dirt and ground triangles, so both draw as ground and the shader blends to
/// dirt with the vertex's dirt amount.
fn track_mesh_data(mesh: &track::TrackMesh) -> MeshData {
    let mut vertices: Vec<Vertex> = (0..mesh.positions.len())
        .map(|i| Vertex {
            pos: mesh.positions[i].to_array(),
            normal: mesh.normals.get(i).copied().unwrap_or(glam::Vec3::Y).to_array(),
            color: mesh.colors.get(i).copied().unwrap_or([0.5, 0.5, 0.5]),
            kind: 0,
            uv: mesh.uv.get(i).copied().unwrap_or([0.0, 0.0]),
            dirt: mesh.dirt.get(i).copied().unwrap_or(0.0),
        })
        .collect();
    for (t, surface) in mesh.tri_surface.iter().enumerate() {
        let color = vertices[mesh.indices[3 * t] as usize].color;
        let kind = match surface {
            track::Surface::Dirt => track::Surface::Ground as u32,
            // A booster deck is a road with arrows painted on it (its colour says where).
            track::Surface::Booster => track::Surface::Road as u32,
            // The strip between a road and its border is ground to the car, the road's tarp to
            // the eye.
            track::Surface::Ground if color == track::kit::color::VERGE || color == track::kit::color::VERGE_STRAPPED => {
                track::Surface::Road as u32
            }
            track::Surface::Wall => wall_kind(vertices[mesh.indices[3 * t] as usize].color),
            s => *s as u32,
        };
        for k in 0..3 {
            vertices[mesh.indices[3 * t + k] as usize].kind = kind;
        }
    }
    road_spill(&mut vertices);
    // Triangles grouped by the shader that draws them: roads, the rest, then the ground; the
    // borders' plain hulls (the car's, not the eye's) left out.
    let mut indices = Vec::with_capacity(mesh.indices.len());
    let mut parts = Vec::new();
    for shading in [Shading::Road, Shading::Other, Shading::Ground] {
        let first = indices.len() as u32;
        for t in mesh.indices.chunks_exact(3) {
            if vertices[t[0] as usize].color == track::kit::color::HULL {
                continue;
            }
            let s = match vertices[t[0] as usize].kind {
                k if k == track::Surface::Road as u32 => Shading::Road,
                k if k == track::Surface::Ground as u32 => Shading::Ground,
                _ => Shading::Other,
            };
            if s == shading {
                indices.extend_from_slice(t);
            }
        }
        parts.push((first..indices.len() as u32, shading));
    }
    MeshData { vertices, indices, parts }
}

/// Distance along the route over which a road takes on the earth of the dirt track it leads to,
/// metres.
const SPILL: f32 = 14.0;

/// Earth carried onto the roads next to dirt tracks: a road vertex's `dirt` grows from 0, `SPILL`
/// metres along the route from the nearest dirt floor, to 1 where it meets it, and the shader lays
/// that much dirt over the asphalt (where the road meets the dirt, both show the same surface).
fn road_spill(vertices: &mut [Vertex]) {
    let ground = track::Surface::Ground as u32;
    let road = track::Surface::Road as u32;
    let mut floor: Vec<f32> = vertices.iter().filter(|v| v.kind == ground && v.dirt >= 0.9).map(|v| v.uv[0]).collect();
    if floor.is_empty() {
        return;
    }
    floor.sort_by(f32::total_cmp);
    for v in vertices.iter_mut().filter(|v| v.kind == road && v.uv != [0.0, 0.0]) {
        let s = v.uv[0];
        let i = floor.partition_point(|&f| f < s);
        let d = [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|j| floor.get(j)).map(|&f| (f - s).abs()).fold(f32::MAX, f32::min);
        let t = (1.0 - d / SPILL).clamp(0.0, 1.0);
        v.dirt = t * t * (3.0 - 2.0 * t);
    }
}

/// What a wall is made of, told by the kit's colour it was given: inflatable bumpers, sandbags,
/// tarp-wrapped slabs and the gates' fabric sleeves, the gates' banner, plastic tubes, straps,
/// steel stakes and buckles, concrete sides of dirt mounds, the dug earth of dirt jumps, the
/// camps' fabric, paint, glass, lamps and tyres, and rocks (any other colour: the scenery shades
/// each rock its own way).
fn wall_kind(color: [f32; 3]) -> u32 {
    use track::kit::color as c;
    match color {
        x if x == c::LIP => gfx::kind::BUMPER,
        x if x == c::SANDBAG => gfx::kind::SANDBAG,
        x if x == c::SLAB || x == c::SLEEVE => gfx::kind::TARP,
        // A gutter's lip, top and sides: the ice of its walls, drawn as the deck.
        x if x == c::GUTTER => track::Surface::Road as u32,
        x if x == c::BANNER => gfx::kind::BANNER,
        x if x == c::TUBE || x == c::COLLAR => gfx::kind::PLASTIC,
        x if x == c::STRAP => gfx::kind::STRAP,
        x if x == c::STEEL || x == c::RUST || x == c::STAKE => gfx::kind::STEEL,
        x if x == c::WALL => gfx::kind::CONCRETE,
        x if x == c::EARTH_FACE => gfx::kind::EARTH,
        // The camps (track's camp.rs).
        x if x == c::FABRIC || x == c::FLAG => gfx::kind::TARP,
        x if x == c::PAINT_WHITE || x == c::PAINT_ORANGE || x == c::PAINT_GREY || x == c::PAINT_BLACK => gfx::kind::PAINT,
        x if x == c::WINDOW || x == c::SOLAR || x == c::GREENHOUSE => gfx::kind::GLASS,
        x if x == c::BEACON_RED || x == c::BEACON_AMBER => gfx::kind::GLOW,
        x if x == c::TYRE => gfx::kind::RUBBER,
        _ => gfx::kind::ROCK,
    }
}

/// Uploads a car's body and every corner's parts.
pub fn upload_car(scene: &mut SceneRenderer, gpu: &Gpu, buggy: &car_model::Buggy) -> CarMeshes {
    let device = &gpu.device;
    let mut up = |mesh: &MeshData| scene.upload(device, mesh);
    let body = up(&buggy.body);
    let corners = buggy
        .parts
        .iter()
        .map(|p| CornerMeshes {
            arm_lo: up(&p.arm_lo),
            arm_up: up(&p.arm_up),
            upright: up(&p.upright),
            wheel: up(&p.wheel),
            damper: up(&p.damper),
            rod: up(&p.rod),
            spring: up(&p.spring),
            tierod: p.tierod.as_ref().map(&mut up),
        })
        .collect();
    CarMeshes {
        body,
        corners,
        rigs: buggy.rigs,
        wheel_radius: buggy.wheel_radius,
        tyre_width: buggy.tyre_width,
        skis: buggy.skis,
        headlight: buggy.headlight,
    }
}
