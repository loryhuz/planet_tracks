//! The buggy from Blender (`assets/buggy.glb`, built by `tools/blender/build_buggy.py`): the body,
//! and for each corner the parts of a double-wishbone suspension with their pivots. Every frame
//! the corners are posed from the physics' suspension lengths: the arms swing around their
//! hinges, the upright follows the lower ball joint and turns around its kingpin, the coilover
//! shortens and lengthens between its mounts, and the tie rod follows the steering.
//!
//! On top of the physics, [`Look`] adds what a massless raycast suspension lacks: a wheel that
//! leaves the ground drops with some inertia (the physics puts it at full travel at once), the
//! body dives under braking and squats under power (the presets have no pitch transfer), and the
//! tyres deflect under their load (the physics' wheel is a rigid disc touching the ground at one
//! point: the renderer sinks it into the ground and the vertex shader flattens the rubber there).
//! All are render-only and never feed back into the simulation.

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
use physics::{CarParams, CarState};
use serde_json::Value;

use crate::gfx::{MeshData, Vertex, kind};

pub const TYRE_WIDTH: f32 = 0.48;

const GLB: &[u8] = include_bytes!("../assets/buggy.glb");
/// The body's livery atlas, painted by tools/blender/buggy_livery.py.
const LIVERY: &[u8] = include_bytes!("../assets/buggy_livery.png");
const CORNERS: [&str; 4] = ["FL", "FR", "RL", "RR"];

/// Rest geometry of one corner, car frame: what the solver needs.
#[derive(Clone, Copy, Debug)]
pub struct CornerRig {
    /// Wheel centre (also the upright's and the wheel's origin).
    pub wheel: Vec3,
    pub pivot_lo: Vec3,
    pub joint_lo: Vec3,
    pub pivot_up: Vec3,
    pub joint_up: Vec3,
    /// Coilover mounts: on the body, and on the lower arm.
    pub top: Vec3,
    pub bottom: Vec3,
    /// Spring seats: the lower one rides on the rod, the upper one on the damper body.
    pub spring_lo: Vec3,
    pub spring_hi: Vec3,
    /// Tie rod ends: on the body, and on the upright's steering arm (front corners).
    pub tie: Option<(Vec3, Vec3)>,
}

/// One corner's meshes, each relative to its origin (the matching [`CornerRig`] point) in the
/// car's axes.
pub struct CornerParts {
    pub arm_lo: MeshData,
    pub arm_up: MeshData,
    pub upright: MeshData,
    pub wheel: MeshData,
    pub damper: MeshData,
    pub rod: MeshData,
    pub spring: MeshData,
    pub tierod: Option<MeshData>,
}

pub struct Buggy {
    /// Everything that moves with the chassis, in the car frame.
    pub body: MeshData,
    pub rigs: [CornerRig; 4],
    pub parts: Vec<CornerParts>,
    /// Wheel radius the model was built for, m.
    pub wheel_radius: f32,
    /// The livery texture (sRGB RGBA8), for kind::LIVERY vertices.
    pub livery: Image,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Transforms of one corner's parts in the body frame (each applies to its part's mesh).
#[derive(Clone, Copy, Debug)]
pub struct CornerPose {
    pub arm_lo: Mat4,
    pub arm_up: Mat4,
    pub upright: Mat4,
    pub wheel: Mat4,
    pub damper: Mat4,
    pub rod: Mat4,
    pub spring: Mat4,
    pub tierod: Option<Mat4>,
}

/// Rotation about Z (the hinge) that brings an arm's outer joint to height `joint_y`.
fn swing(pivot: Vec3, joint: Vec3, joint_y: f32) -> f32 {
    let r = joint - pivot;
    let len = Vec2::new(r.x, r.y).length().max(1e-4);
    let rest = (r.y / len).clamp(-1.0, 1.0).asin();
    let now = ((joint_y - pivot.y) / len).clamp(-0.99, 0.99).asin();
    r.x.signum() * (now - rest)
}

/// Scale by `k` along the unit vector `n`.
fn stretch(n: Vec3, k: f32) -> Mat4 {
    let outer = Mat3::from_cols(n * n.x, n * n.y, n * n.z);
    Mat4::from_mat3(Mat3::IDENTITY + outer * (k - 1.0))
}

impl CornerRig {
    /// Poses the corner for a wheel centre at height `wheel_y` (body frame), a steering angle
    /// (radians, positive turns the front of the wheel toward +X), a rolling angle, and a wheel
    /// scale (a preset's wheel radius over the model's).
    pub fn pose(&self, wheel_y: f32, steer: f32, spin: f32, wheel_scale: f32) -> CornerPose {
        let dy = wheel_y - self.wheel.y;
        let r_lo = Quat::from_rotation_z(swing(self.pivot_lo, self.joint_lo, self.joint_lo.y + dy));
        let joint_lo = self.pivot_lo + r_lo * (self.joint_lo - self.pivot_lo);
        let lift = joint_lo.y - self.joint_lo.y;
        let r_up = Quat::from_rotation_z(swing(self.pivot_up, self.joint_up, self.joint_up.y + lift));

        // The upright rides on the lower joint and turns around the kingpin through it.
        let steer_q = Quat::from_rotation_y(steer);
        let upright = Mat4::from_translation(joint_lo)
            * Mat4::from_quat(steer_q)
            * Mat4::from_translation(self.wheel - self.joint_lo)
            * Mat4::from_scale(Vec3::splat(wheel_scale));
        let wheel = upright * Mat4::from_rotation_x(spin);

        // Coilover between its top mount and the mount on the lower arm.
        let bottom = self.pivot_lo + r_lo * (self.bottom - self.pivot_lo);
        let u0 = (self.top - self.bottom).normalize();
        let u = (self.top - bottom).normalize();
        let rd = Quat::from_rotation_arc(u0, u);
        let s_lo = bottom + rd * (self.spring_lo - self.bottom);
        let s_hi = self.top + rd * (self.spring_hi - self.top);
        let k = (s_hi - s_lo).length() / (self.spring_hi - self.spring_lo).length().max(1e-4);

        let tierod = self.tie.map(|(inner, outer)| {
            let out = upright.transform_point3(outer - self.wheel);
            let v0 = outer - inner;
            let v = out - inner;
            let n0 = v0.normalize();
            Mat4::from_rotation_translation(Quat::from_rotation_arc(n0, v.normalize()), inner) * stretch(n0, v.length() / v0.length())
        });

        CornerPose {
            arm_lo: Mat4::from_rotation_translation(r_lo, self.pivot_lo),
            arm_up: Mat4::from_rotation_translation(r_up, self.pivot_up),
            upright,
            wheel,
            damper: Mat4::from_rotation_translation(rd, self.top),
            rod: Mat4::from_rotation_translation(rd, bottom),
            spring: Mat4::from_rotation_translation(rd, s_lo) * stretch(u0, k),
            tierod,
        }
    }
}

// --- What the renderer adds on top of the physics. ---

/// Natural frequency (Hz) and damping ratio of a wheel dropping when it leaves the ground.
const DROOP_HZ: f32 = 5.0;
const DROOP_ZETA: f32 = 0.35;
/// Share of its speed a wheel keeps when it hits the end of its travel.
const DROOP_BOUNCE: f32 = 0.3;
/// Body pitch per m/s² of longitudinal acceleration (radians), its limits, and its spring.
const PITCH_PER_ACCEL: f32 = 0.0008;
const PITCH_DIVE_MAX: f32 = 0.045;
const PITCH_SQUAT_MAX: f32 = 0.03;
const PITCH_HZ: f32 = 1.4;
const PITCH_ZETA: f32 = 0.5;
/// Tyre deflection, as a share of the wheel radius, under the static load (2.2 cm on the
/// 0.45 m wheels) and its limits while the tyre touches the ground; how fast it follows the
/// load (s).
const SQUASH_REST: f32 = 0.05;
const SQUASH_MIN: f32 = 0.015;
const SQUASH_MAX: f32 = 0.11;
const SQUASH_TIME: f32 = 0.03;
/// How fast the shade a tyre casts round it on the ground comes and goes with its contact (s).
const TOUCH_TIME: f32 = 0.08;

/// Render-side suspension state of one car, advanced every physics tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// Suspension length drawn for each wheel, m.
    pub travel: [f32; 4],
    speed: [f32; 4],
    /// Body pitch drawn on top of the physics' (radians, positive = nose down).
    pub pitch: f32,
    pitch_rate: f32,
    /// How far each tyre is pressed into the ground, m: the wheel is drawn that much lower.
    pub squash: [f32; 4],
    /// Where each tyre last touched the ground (point, normal, world), and how much it touches
    /// it now, 0..1, eased (the shade it casts round it).
    pub ground: [(Vec3, Vec3); 4],
    pub touch: [f32; 4],
}

impl Look {
    pub fn new(state: &CarState) -> Self {
        let mut look = Self::default();
        for (t, w) in look.travel.iter_mut().zip(&state.wheels) {
            *t = w.suspension;
        }
        look
    }

    pub fn step(&mut self, state: &CarState, params: &CarParams, dt: f32) {
        let travel = params.suspension_travel.max(0.01);
        let w = std::f32::consts::TAU * DROOP_HZ;
        for i in 0..4 {
            let wheel = &state.wheels[i];
            let squash = if wheel.contact { (SQUASH_REST * wheel.load).clamp(SQUASH_MIN, SQUASH_MAX) * params.wheel_radius } else { 0.0 };
            self.squash[i] += (squash - self.squash[i]) * (1.0 - (-dt / SQUASH_TIME).exp());
            self.touch[i] += (if wheel.contact { 1.0 } else { 0.0 } - self.touch[i]) * (1.0 - (-dt / TOUCH_TIME).exp());
            if wheel.contact {
                // On the ground the physics knows where the wheel is.
                self.speed[i] = (wheel.suspension - self.travel[i]) / dt;
                self.travel[i] = wheel.suspension;
                self.ground[i] = (wheel.contact_point, wheel.contact_normal);
            } else {
                let a = w * w * (travel - self.travel[i]) - 2.0 * DROOP_ZETA * w * self.speed[i];
                self.speed[i] += a * dt;
                self.travel[i] += self.speed[i] * dt;
                if self.travel[i] > travel {
                    self.travel[i] = travel;
                    self.speed[i] = -self.speed[i].max(0.0) * DROOP_BOUNCE;
                }
            }
        }
        let grounded = state.wheels.iter().filter(|w| w.contact).count() >= 3;
        let along = (state.rotation.inverse() * state.acceleration).z;
        let target = if grounded { (-along * PITCH_PER_ACCEL).clamp(-PITCH_SQUAT_MAX, PITCH_DIVE_MAX) } else { 0.0 };
        let w = std::f32::consts::TAU * PITCH_HZ;
        self.pitch_rate += (w * w * (target - self.pitch) - 2.0 * PITCH_ZETA * w * self.pitch_rate) * dt;
        self.pitch += self.pitch_rate * dt;
    }

    pub fn lerp(&self, next: &Self, t: f32) -> Self {
        let mut out = *next;
        for i in 0..4 {
            out.travel[i] = self.travel[i] + (next.travel[i] - self.travel[i]) * t;
            out.squash[i] = self.squash[i] + (next.squash[i] - self.squash[i]) * t;
            out.touch[i] = self.touch[i] + (next.touch[i] - self.touch[i]) * t;
        }
        out.pitch = self.pitch + (next.pitch - self.pitch) * t;
        out
    }
}

// --- Loading the .glb. ---

pub fn load() -> Buggy {
    let mut buggy = parse(GLB).unwrap_or_else(|e| panic!("assets/buggy.glb: {e}"));
    buggy.livery = decode_png(LIVERY).unwrap_or_else(|e| panic!("assets/buggy_livery.png: {e}"));
    buggy
}

pub(crate) fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let px = (info.width * info.height) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3].chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::Grayscale => buf[..px].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..px * 2].chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        t => return Err(format!("unsupported colour type {t:?}")),
    };
    Ok(Image { width: info.width, height: info.height, rgba })
}

struct Glb<'a> {
    json: Value,
    bin: &'a [u8],
}

impl Glb<'_> {
    fn get(&self, kind: &str, i: usize) -> Result<&Value, String> {
        self.json[kind].get(i).ok_or_else(|| format!("missing {kind}[{i}]"))
    }

    /// Floats of an accessor, `n` per element.
    fn floats(&self, accessor: usize, n: usize) -> Result<Vec<f32>, String> {
        let (data, stride, count, ty) = self.view(accessor)?;
        if ty != 5126 {
            return Err(format!("accessor {accessor}: not float"));
        }
        let mut out = Vec::with_capacity(count * n);
        for e in 0..count {
            for c in 0..n {
                let o = e * stride + c * 4;
                out.push(f32::from_le_bytes(data[o..o + 4].try_into().unwrap()));
            }
        }
        Ok(out)
    }

    fn indices(&self, accessor: usize) -> Result<Vec<u32>, String> {
        let (data, stride, count, ty) = self.view(accessor)?;
        (0..count)
            .map(|e| {
                let o = e * stride;
                Ok(match ty {
                    5121 => data[o] as u32,
                    5123 => u16::from_le_bytes([data[o], data[o + 1]]) as u32,
                    5125 => u32::from_le_bytes(data[o..o + 4].try_into().unwrap()),
                    _ => return Err(format!("accessor {accessor}: index type {ty}")),
                })
            })
            .collect()
    }

    /// (bytes from the first element, stride, count, component type).
    fn view(&self, accessor: usize) -> Result<(&[u8], usize, usize, u64), String> {
        let a = self.get("accessors", accessor)?;
        let bv = self.get("bufferViews", a["bufferView"].as_u64().ok_or("sparse accessor")? as usize)?;
        let ty = a["componentType"].as_u64().unwrap_or(0);
        let comps = match a["type"].as_str().unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            t => return Err(format!("accessor type {t}")),
        };
        let size = match ty {
            5121 => 1,
            5123 => 2,
            _ => 4,
        };
        let stride = bv["byteStride"].as_u64().map_or(comps * size, |s| s as usize);
        let start = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = a["count"].as_u64().unwrap_or(0) as usize;
        let end = start + stride * count.saturating_sub(1) + comps * size;
        if count == 0 || end > self.bin.len() {
            return Err(format!("accessor {accessor} out of the buffer"));
        }
        Ok((&self.bin[start..], stride, count, ty))
    }
}

fn local_matrix(node: &Value) -> Mat4 {
    let f = |v: &Value, n: usize, d: &[f32]| -> Vec<f32> {
        v.as_array().map_or(d.to_vec(), |a| a.iter().take(n).map(|x| x.as_f64().unwrap_or(0.0) as f32).collect())
    };
    if node["matrix"].is_array() {
        let m = f(&node["matrix"], 16, &[0.0; 16]);
        return Mat4::from_cols_slice(&m);
    }
    let t = f(&node["translation"], 3, &[0.0; 3]);
    let r = f(&node["rotation"], 4, &[0.0, 0.0, 0.0, 1.0]);
    let s = f(&node["scale"], 3, &[1.0; 3]);
    Mat4::from_scale_rotation_translation(Vec3::from_slice(&s), Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(), Vec3::from_slice(&t))
}

/// Material name prefix to shader kind.
fn material_kind(name: &str) -> u32 {
    match name.split('_').next().unwrap_or("") {
        "paint" => kind::PAINT,
        "rubber" => kind::RUBBER,
        "glass" => kind::GLASS,
        "glow" => kind::GLOW,
        "wire" => kind::WIRE,
        "livery" => kind::LIVERY,
        _ => kind::METAL,
    }
}

/// The node's mesh with `matrix` applied (normals by its rotation).
fn node_mesh(glb: &Glb, mesh: usize, matrix: Mat4) -> Result<MeshData, String> {
    let normal_m = Mat3::from_mat4(matrix).inverse().transpose();
    let mut out = MeshData::default();
    let prims = glb.get("meshes", mesh)?["primitives"].as_array().cloned().unwrap_or_default();
    for p in prims {
        if p["mode"].as_u64().unwrap_or(4) != 4 {
            continue;
        }
        let attr = &p["attributes"];
        let pos = glb.floats(attr["POSITION"].as_u64().ok_or("no POSITION")? as usize, 3)?;
        let nrm = glb.floats(attr["NORMAL"].as_u64().ok_or("no NORMAL")? as usize, 3)?;
        let uv = match attr["TEXCOORD_0"].as_u64() {
            Some(a) => glb.floats(a as usize, 2)?,
            None => vec![0.0; pos.len() / 3 * 2],
        };
        let (color, k) = match p["material"].as_u64() {
            Some(m) => {
                let mat = glb.get("materials", m as usize)?;
                let name = mat["name"].as_str().unwrap_or("");
                let c = if mat["extras"]["game_color"].is_array() { &mat["extras"]["game_color"] } else { &mat["pbrMetallicRoughness"]["baseColorFactor"] };
                let c: Vec<f32> = c.as_array().map_or(vec![0.8; 3], |a| a.iter().take(3).map(|x| x.as_f64().unwrap_or(0.8) as f32).collect());
                ([c[0], c[1], c[2]], material_kind(name))
            }
            None => ([0.8; 3], kind::METAL),
        };
        let base = out.vertices.len() as u32;
        for i in 0..pos.len() / 3 {
            let p = matrix.transform_point3(Vec3::from_slice(&pos[3 * i..3 * i + 3]));
            let n = (normal_m * Vec3::from_slice(&nrm[3 * i..3 * i + 3])).normalize_or(Vec3::Y);
            let mut v = Vertex::plain(p.to_array(), n.to_array(), color, k);
            v.uv = [uv[2 * i], uv[2 * i + 1]];
            out.vertices.push(v);
        }
        match p["indices"].as_u64() {
            Some(a) => out.indices.extend(glb.indices(a as usize)?.into_iter().map(|i| base + i)),
            None => out.indices.extend(base..out.vertices.len() as u32),
        }
    }
    Ok(out)
}

fn append(into: &mut MeshData, mesh: MeshData) {
    let base = into.vertices.len() as u32;
    into.vertices.extend(mesh.vertices);
    into.indices.extend(mesh.indices.into_iter().map(|i| i + base));
}

fn parse(bytes: &[u8]) -> Result<Buggy, String> {
    let word = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or("truncated");
    if word(0)? != 0x4654_6C67 {
        return Err("not a GLB file".into());
    }
    let json_len = word(12)? as usize;
    let json: Value = serde_json::from_slice(bytes.get(20..20 + json_len).ok_or("truncated")?).map_err(|e| e.to_string())?;
    let bin_at = 20 + json_len;
    let bin_len = word(bin_at)? as usize;
    let bin = bytes.get(bin_at + 8..bin_at + 8 + bin_len).ok_or("truncated")?;
    let glb = Glb { json, bin };

    // World matrices of every node.
    let nodes = glb.json["nodes"].as_array().cloned().unwrap_or_default();
    let mut world = vec![None; nodes.len()];
    let scene = glb.json["scene"].as_u64().unwrap_or(0) as usize;
    let mut stack: Vec<(usize, Mat4)> =
        glb.json["scenes"][scene]["nodes"].as_array().ok_or("no scene")?.iter().filter_map(|n| n.as_u64()).map(|n| (n as usize, Mat4::IDENTITY)).collect();
    while let Some((i, parent)) = stack.pop() {
        let m = parent * local_matrix(&nodes[i]);
        world[i] = Some(m);
        for c in nodes[i]["children"].as_array().into_iter().flatten().filter_map(|c| c.as_u64()) {
            stack.push((c as usize, m));
        }
    }

    let mut body = MeshData::default();
    let mut found: std::collections::BTreeMap<(String, String), (Option<usize>, Mat4)> = Default::default();
    let mut wheel_radius = 0.45;
    for (i, node) in nodes.iter().enumerate() {
        let Some(m) = world[i] else { continue };
        let name = node["name"].as_str().unwrap_or("");
        if let Some(r) = node["extras"]["wheel_radius"].as_f64() {
            wheel_radius = r as f32;
        }
        match name.rsplit_once('.') {
            Some((part, corner)) if CORNERS.contains(&corner) => {
                found.insert((part.to_string(), corner.to_string()), (node["mesh"].as_u64().map(|x| x as usize), m));
            }
            _ => {
                if let Some(mesh) = node["mesh"].as_u64() {
                    append(&mut body, node_mesh(&glb, mesh as usize, m)?);
                }
            }
        }
    }

    let mut rigs = Vec::new();
    let mut parts = Vec::new();
    for corner in CORNERS {
        let at = |part: &str| -> Result<Vec3, String> {
            found.get(&(part.to_string(), corner.to_string())).map(|(_, m)| m.w_axis.truncate()).ok_or_else(|| format!("missing {part}.{corner}"))
        };
        let part = |name: &str| -> Result<MeshData, String> {
            let (mesh, m) = found.get(&(name.to_string(), corner.to_string())).ok_or_else(|| format!("missing {name}.{corner}"))?;
            let mesh = mesh.ok_or_else(|| format!("{name}.{corner} has no mesh"))?;
            node_mesh(&glb, mesh, Mat4::from_mat3(Mat3::from_mat4(*m)))
        };
        let front = corner.starts_with('F');
        rigs.push(CornerRig {
            wheel: at("upright")?,
            pivot_lo: at("arm_lo")?,
            joint_lo: at("joint_lo")?,
            pivot_up: at("arm_up")?,
            joint_up: at("joint_up")?,
            top: at("damper")?,
            bottom: at("rod")?,
            spring_lo: at("spring")?,
            spring_hi: at("spring_top")?,
            tie: if front { Some((at("tierod")?, at("tierod_out")?)) } else { None },
        });
        parts.push(CornerParts {
            arm_lo: part("arm_lo")?,
            arm_up: part("arm_up")?,
            upright: part("upright")?,
            wheel: part("wheel")?,
            damper: part("damper")?,
            rod: part("rod")?,
            spring: part("spring")?,
            tierod: if front { Some(part("tierod")?) } else { None },
        });
    }
    Ok(Buggy { body, rigs: rigs.try_into().map_err(|_| "corners")?, parts, wheel_radius, livery: Image { width: 1, height: 1, rgba: vec![255; 4] } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn the_model_has_a_body_and_four_rigged_corners() {
        let b = load();
        assert!(b.body.indices.len() > 3000, "body: {} indices", b.body.indices.len());
        assert_eq!(b.parts.len(), 4);
        assert!((b.wheel_radius - 0.45).abs() < 1e-3);
        // Wheels where the physics puts them: ±0.9 m across, ±1.3 m along.
        for (rig, (sx, sz)) in b.rigs.iter().zip([(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]) {
            assert!(close(Vec3::new(rig.wheel.x, 0.0, rig.wheel.z), Vec3::new(0.9 * sx, 0.0, 1.3 * sz)), "{:?}", rig.wheel);
        }
        assert!(b.rigs[0].tie.is_some() && b.rigs[2].tie.is_none());
    }

    #[test]
    fn at_rest_every_part_sits_at_its_origin() {
        let b = load();
        for (rig, parts) in b.rigs.iter().zip(&b.parts) {
            let pose = rig.pose(rig.wheel.y, 0.0, 0.0, 1.0);
            for (m, origin) in [
                (pose.arm_lo, rig.pivot_lo),
                (pose.arm_up, rig.pivot_up),
                (pose.upright, rig.wheel),
                (pose.wheel, rig.wheel),
                (pose.damper, rig.top),
                (pose.rod, rig.bottom),
                (pose.spring, rig.spring_lo),
            ] {
                assert!(m.abs_diff_eq(Mat4::from_translation(origin), 1e-4), "{m:?} vs {origin:?}");
            }
            if let (Some(m), Some((inner, _))) = (pose.tierod, rig.tie) {
                assert!(m.abs_diff_eq(Mat4::from_translation(inner), 1e-4));
            }
            // Wheels are round and centred on their origin.
            let r = parts.wheel.vertices.iter().map(|v| Vec2::new(v.pos[1], v.pos[2]).length()).fold(0.0, f32::max);
            assert!((r - b.wheel_radius).abs() < 0.01, "wheel radius {r}");
        }
    }

    #[test]
    fn the_wheel_follows_the_suspension_and_the_coilover_stays_between_its_mounts() {
        let b = load();
        for rig in &b.rigs {
            let mut last_len = f32::MAX;
            for k in 0..=10 {
                // From full droop to full bump.
                let y = rig.wheel.y - 0.25 + 0.05 * k as f32;
                let pose = rig.pose(y, 0.0, 0.0, 1.0);
                let centre = pose.wheel.transform_point3(Vec3::ZERO);
                assert!((centre.y - y).abs() < 1e-4, "wheel at {} for {y}", centre.y);
                // The arms slope down at rest: at full droop they swing in and tuck the wheel
                // under by about 14 cm.
                assert!((centre.x - rig.wheel.x).abs() < 0.16, "wheel moved {} m across", centre.x - rig.wheel.x);
                // Damper body on the top mount, rod on the lower arm, both on one axis.
                let bottom = pose.rod.transform_point3(Vec3::ZERO);
                let lower = pose.arm_lo.transform_point3(rig.bottom - rig.pivot_lo);
                assert!(close(bottom, lower));
                let axis = pose.damper.transform_vector3((rig.bottom - rig.top).normalize());
                assert!(close(axis, (bottom - rig.top).normalize()));
                // The spring spans its seats and gets shorter as the wheel rises.
                let seats = pose.spring.transform_vector3(rig.spring_hi - rig.spring_lo);
                let s_lo = pose.spring.transform_point3(Vec3::ZERO);
                let s_hi = rig.top + pose.damper.transform_vector3(rig.spring_hi - rig.top);
                assert!(close(s_lo + seats, s_hi));
                assert!(seats.length() < last_len);
                last_len = seats.length();
                // Arms stay on their joints: the upper joint moves with the upright.
                let up_joint = pose.arm_up.transform_point3(rig.joint_up - rig.pivot_up);
                let on_upright = pose.upright.transform_point3(rig.joint_up - rig.wheel);
                assert!(close(up_joint, on_upright), "{up_joint} vs {on_upright}");
            }
        }
    }

    #[test]
    fn the_tie_rod_follows_the_steering() {
        let b = load();
        let rig = &b.rigs[0];
        let (inner, outer) = rig.tie.unwrap();
        for steer in [-0.6f32, -0.3, 0.0, 0.3, 0.6] {
            let pose = rig.pose(rig.wheel.y + 0.1, steer, 0.0, 1.0);
            let m = pose.tierod.unwrap();
            assert!(close(m.transform_point3(Vec3::ZERO), inner));
            let end = m.transform_point3(outer - inner);
            let arm = pose.upright.transform_point3(outer - rig.wheel);
            assert!(close(end, arm), "{end} vs {arm}");
        }
    }

    #[test]
    fn a_wheel_leaving_the_ground_drops_smoothly_then_lands_where_the_physics_says() {
        let params = physics::presets().remove(0);
        let world = physics::World::new(&physics::testing::flat(50.0, track::Surface::Road));
        let car = physics::Car::new(params.clone(), &world, track::Pose { position: Vec3::new(0.0, 0.5, 0.0), yaw: 0.0 });
        let mut state = car.state.clone();
        for w in &mut state.wheels {
            w.contact = true;
            w.suspension = params.rest_suspension();
        }
        let mut look = Look::new(&state);
        look.step(&state, &params, 0.01);
        for w in &mut state.wheels {
            w.contact = false;
            w.suspension = params.suspension_travel;
        }
        look.step(&state, &params, 0.01);
        assert!(look.travel[0] < params.rest_suspension() + 0.05, "jumped to {}", look.travel[0]);
        for _ in 0..40 {
            look.step(&state, &params, 0.01);
            assert!(look.travel[0] <= params.suspension_travel + 1e-6);
        }
        assert!(look.travel[0] > params.suspension_travel - 0.03, "still at {}", look.travel[0]);
        for w in &mut state.wheels {
            w.contact = true;
            w.suspension = 0.05;
        }
        look.step(&state, &params, 0.01);
        assert_eq!(look.travel[0], 0.05);
    }

    #[test]
    fn a_tyre_is_pressed_in_by_its_load_and_comes_back_round_in_the_air() {
        let params = physics::presets().remove(0);
        let world = physics::World::new(&physics::testing::flat(50.0, track::Surface::Road));
        let car = physics::Car::new(params.clone(), &world, track::Pose { position: Vec3::new(0.0, 0.5, 0.0), yaw: 0.0 });
        let mut state = car.state.clone();
        let mut look = Look::new(&state);
        let settle = |look: &mut Look, state: &CarState| {
            for _ in 0..50 {
                look.step(state, &params, 0.01);
            }
        };
        for (w, load) in state.wheels.iter_mut().zip([1.0, 2.0, 0.0, 5.0]) {
            w.contact = true;
            w.load = load;
            w.contact_point = Vec3::new(0.3, -0.2, 0.1);
            w.contact_normal = Vec3::Y;
        }
        settle(&mut look, &state);
        let r = params.wheel_radius;
        // 2.2 cm under the static load, more under more, a little at least, never more than 5 cm.
        for (squash, want) in look.squash.iter().zip([SQUASH_REST, 2.0 * SQUASH_REST, SQUASH_MIN, SQUASH_MAX]) {
            assert!((squash - want * r).abs() < 1e-4, "{squash} vs {}", want * r);
        }
        assert!(look.touch.iter().all(|&t| t > 0.99));
        for w in &mut state.wheels {
            w.contact = false;
            w.load = 0.0;
        }
        settle(&mut look, &state);
        assert!(look.squash.iter().all(|&s| s < 1e-4), "{:?}", look.squash);
        assert!(look.touch.iter().all(|&t| t < 0.01), "{:?}", look.touch);
        // The shade still knows where the ground was.
        assert_eq!(look.ground[0], (Vec3::new(0.3, -0.2, 0.1), Vec3::Y));
    }
}
