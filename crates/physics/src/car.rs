//! The car simulation step.
//!
//! The model is an arcade "path" model with a progressive drift:
//! - the steering asks for a turn rate (the kinematic yaw rate of the wheel angle, capped by a
//!   speed-dependent yaw cap, first-order response). Grip usage u = the lateral acceleration it
//!   asks for / the available grip;
//! - the *path* is the direction the tyres make the car travel: the body heading turned back by
//!   the drift angle θ. The tyres act at car level: they cancel any velocity across the path
//!   (dead-beat, within the grip), so the car never translates sideways;
//! - u below `mark_start`: pure grip. From there to 1: still on rails, the tyres stay aligned but
//!   leave marks. Above 1: the path bends at the grip limit and the excess becomes a drift angle
//!   target (gentle knee, then proportional, up to the surface's maximum) that θ follows with a
//!   build-up time; it lasts as long as the steering asks for it and comes back quickly (no swing)
//!   when the steering eases off. Drifting costs speed in proportion to the angle;
//! - a stiff yaw controller holds the body at θ around its path; bumps or scrapes that yaw the
//!   body are pulled back without touching the path;
//! - any mismatch between the path and the real motion beyond a degree (wall hits, landings) is
//!   absorbed into θ, which then comes back: the car "catches" instead of gliding;
//! - grip and steering authority are smoothed over time (fast rise, slow fall), suspension forces
//!   never yaw the car and body scrapes barely do, so bumps move the body without steering it.
//!
//! One tick: steering smoothing; one BVH query for every test of the tick; suspension rays and
//! forces; grip and contact smoothing; tyre forces along/across the path; booster push; drift
//! bleed, turning drag, aero; yaw controller (or air control); semi-implicit integration; the body
//! (spheres) moves in substeps and collides; drift angle spring and catch; bookkeeping for the
//! renderer.

use glam::{Quat, Vec3};
use libm::{atan2f, cosf, sinf, sqrtf, tanf};
use track::Surface;

use crate::params::CarParams;
use crate::world::{Hit, World, closest_point};
use crate::{CarState, DT, Input};

const G: f32 = 9.81;
const REF_SPEED_DOWNFORCE: f32 = 200.0 / 3.6;
const TWO_PI: f32 = core::f32::consts::PI * 2.0;
/// Maximum angular speed, rad/s (safety net).
const MAX_SPIN: f32 = 40.0;
/// Largest drift angle, radians.
const MAX_DRIFT: f32 = 1.4;
/// Share of the yaw kept from body scrapes on the ground.
const SCRAPE_YAW: f32 = 0.2;
/// Gain of the body's yaw-rate loop, 1/s, and of its angle loop around the path, 1/s: the body
/// holds its angle to the path firmly (critically damped), whatever bumps or scrapes do to it.
const BODY_RATE_GAIN: f32 = 20.0;
const BODY_ANGLE_GAIN: f32 = 9.0;
/// Time for a wheel to settle into a soft surface (or climb out of it), s.
const SINK_TIME: f32 = 0.12;
/// Share of a surface's `sink` a ski sinks by: it spreads the load and glides on the snow.
const SKI_SINK: f32 = 0.3;
/// How much the depth a wheel sinks to varies over the ruts of soft snow (share of it), and the
/// size of those ruts, m: the car bobs gently as it rolls through snow.
const SINK_RIPPLE: f32 = 0.35;
const RIPPLE_CELL: f32 = 9.0;

/// Smooth value noise in -1..1 over a grid of `RIPPLE_CELL` metres (integer hashing and plain
/// arithmetic only, so it is the same on every platform).
fn ripple(x: f32, z: f32) -> f32 {
    let (gx, gz) = (x / RIPPLE_CELL, z / RIPPLE_CELL);
    let (ix, iz) = (libm::floorf(gx), libm::floorf(gz));
    let (fx, fz) = (gx - ix, gz - iz);
    let corner = |a: f32, b: f32| -> f32 {
        let n = ((a as i32).wrapping_mul(73_856_093) ^ (b as i32).wrapping_mul(19_349_663)) as u32;
        let n = n.wrapping_mul(0x9E37_79B1) >> 8;
        n as f32 / 8_388_608.0 - 1.0
    };
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let lo = corner(ix, iz) + (corner(ix + 1.0, iz) - corner(ix, iz)) * sx;
    let hi = corner(ix, iz + 1.0) + (corner(ix + 1.0, iz + 1.0) - corner(ix, iz + 1.0)) * sx;
    lo + (hi - lo) * sz
}
/// Wheel angle shown at full lock, radians (render only).
const DISPLAY_LOCK: f32 = 25.0 * core::f32::consts::PI / 180.0;

/// Diagonal inertia (pitch around X, yaw around Y, roll around Z), from a box around the car.
pub(crate) fn inertia(p: &CarParams) -> Vec3 {
    let m = p.mass.max(1.0);
    let l = p.wheelbase + 2.0 * p.wheel_radius + 0.4;
    let w = p.track_width + 0.3;
    let h = 1.2;
    Vec3::new(
        m * (h * h + l * l) / 12.0 * p.inertia_pitch.max(0.05),
        m * (w * w + l * l) / 12.0 * p.inertia_yaw.max(0.05),
        m * (w * w + h * h) / 12.0 * p.inertia_roll.max(0.05),
    )
}

fn omega(p: &CarParams) -> f32 {
    TWO_PI * p.spring_hz.max(0.1)
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-3)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// First-order smoothing, fast toward higher values and slower toward lower ones.
#[inline]
fn follow(current: f32, target: f32, rise: f32, fall: f32, dt: f32) -> f32 {
    let rate = if target > current { rise } else { fall };
    current + (target - current) * (rate * dt).min(1.0)
}

/// Drift angle (0..1 of the maximum) for a normalised grip excess `e` (1 = full angle): starts
/// with a gentle knee (no step), then grows linearly.
fn drift_curve(e: f32) -> f32 {
    const KNEE: f32 = 0.12;
    let soft = |x: f32| x - KNEE * (1.0 - libm::expf(-x / KNEE));
    (soft(e.max(0.0)) / soft(1.0)).min(1.0)
}

/// Load reference for the grip: 1 for a loaded wheel, falling to 0 for a wheel barely touching.
#[inline]
fn rho_ref(rho: f32) -> f32 {
    (2.0 * rho).min(1.0)
}

/// The body's forward and left axes flattened into the plane of normal `n`.
fn plane_axes(rot: Quat, n: Vec3) -> (Vec3, Vec3) {
    let fwd = rot * Vec3::Z;
    let f = (fwd - n * fwd.dot(n)).normalize_or(fwd);
    (f, n.cross(f))
}

/// The path frame (forward, left) in the plane of `n`: the body heading turned back by the drift
/// angle (positive drift = the body points left of its path).
pub(crate) fn path_axes(rot: Quat, drift_angle: f32, n: Vec3) -> (Vec3, Vec3) {
    let (f, l) = plane_axes(rot, n);
    let (s, c) = (sinf(drift_angle), cosf(drift_angle));
    let pf = f * c - l * s;
    (pf, n.cross(pf))
}

/// Angle (radians) of the body relative to its line of motion in the plane of `n`, positive when
/// the body points left of where it goes. Reversing counts as moving along the body axis.
pub(crate) fn body_vs_motion(rot: Quat, v: Vec3, n: Vec3) -> f32 {
    let v = v - n * v.dot(n);
    if v.length() < 1.0 {
        return 0.0;
    }
    let (f, l) = plane_axes(rot, n);
    let (x, y) = (v.dot(f), v.dot(l));
    let sgn = if x < 0.0 { -1.0 } else { 1.0 };
    atan2f(-y * sgn, x * sgn)
}

#[derive(Clone, Copy)]
struct Body {
    m: f32,
    inv_i: Vec3,
    rot: Quat,
}

impl Body {
    /// World inverse inertia applied to a vector.
    #[inline]
    fn inv_inertia(&self, v: Vec3) -> Vec3 {
        let local = self.rot.inverse() * v;
        self.rot * (local * self.inv_i)
    }
    #[inline]
    fn inertia(&self, v: Vec3) -> Vec3 {
        let local = self.rot.inverse() * v;
        self.rot * (local / self.inv_i)
    }
    /// Torque of an impulse `j` at lever arm `r`, scaled by `ang`, its yaw part (about the body's
    /// up axis) further scaled by `yaw`.
    #[inline]
    fn torque(&self, r: Vec3, j: Vec3, ang: f32, yaw: f32) -> Vec3 {
        let up = self.rot * Vec3::Y;
        let mut t = r.cross(j) * ang;
        t -= up * (t.dot(up) * (1.0 - yaw));
        t
    }
    /// Effective mass at lever arm `r` along unit direction `d` with the same torque scaling.
    #[inline]
    fn eff_mass(&self, r: Vec3, d: Vec3, ang: f32, yaw: f32) -> f32 {
        let k = 1.0 / self.m + d.dot(self.inv_inertia(self.torque(r, d, ang, yaw)).cross(r));
        1.0 / k.max(1e-9)
    }
}

#[derive(Clone, Copy)]
struct WheelContact {
    hit: Hit,
    /// Suspension length, unclamped (negative when the bump stop is hit hard).
    len: f32,
    force: f32,
}

#[derive(Clone, Copy, Default)]
struct BodyContact {
    point: Vec3,
    normal: Vec3,
    depth: f32,
    wall: bool,
}

fn sanitize(x: f32, lo: f32, hi: f32) -> f32 {
    if x.is_finite() { x.clamp(lo, hi) } else { 0.0 }
}

fn integrate_rotation(q: Quat, w: Vec3, dt: f32) -> Quat {
    let wq = Quat::from_xyzw(w.x, w.y, w.z, 0.0);
    let dq = wq * q;
    let h = 0.5 * dt;
    let r = Quat::from_xyzw(q.x + dq.x * h, q.y + dq.y * h, q.z + dq.z * h, q.w + dq.w * h);
    let len = sqrtf(r.x * r.x + r.y * r.y + r.z * r.z + r.w * r.w);
    if len > 1e-6 {
        let inv = 1.0 / len;
        Quat::from_xyzw(r.x * inv, r.y * inv, r.z * inv, r.w * inv)
    } else {
        Quat::IDENTITY
    }
}

pub(crate) fn step(p: &CarParams, s: &mut CarState, world: &World, input: Input) {
    let dt = DT;
    let steer_in = sanitize(input.steer, -1.0, 1.0);
    let gas = sanitize(input.gas, 0.0, 1.0);
    let brake = sanitize(input.brake, 0.0, 1.0);

    let g = p.gravity.max(0.0);
    let m = p.mass.max(1.0);
    let inertia_local = inertia(p);
    let travel = p.suspension_travel.max(0.01);
    let radius = p.wheel_radius.max(0.05);
    let wheelbase = p.wheelbase.max(0.5);
    let v0 = s.velocity;
    let w0 = s.angular_velocity;

    let rot = s.rotation;
    let fwd = rot * Vec3::Z;
    let up = rot * Vec3::Y;
    let left = rot * Vec3::X;
    let body = Body { m, inv_i: Vec3::ONE / inertia_local, rot };

    // --- 1. Steering.
    {
        let diff = steer_in - s.steer;
        let back = s.steer != 0.0 && (diff > 0.0) != (s.steer > 0.0);
        let rate = if back { p.steer_return } else { p.steer_rate }.max(0.1) * dt;
        s.steer = (s.steer + diff.clamp(-rate, rate)).clamp(-1.0, 1.0);
    }
    let vf = v0.dot(fwd);
    let steer_angle = -s.steer * p.max_steer(vf);

    // --- 2. Broadphase: everything this tick can touch.
    let anchors = p.wheel_anchors();
    let spheres = p.body_spheres();
    let mut reach: f32 = 0.0;
    for a in anchors {
        reach = reach.max(a.length() + travel + 2.0 * radius);
    }
    for (c, r) in spheres {
        reach = reach.max(c.length() + r);
    }
    let motion = v0 * dt;
    let margin = Vec3::splat(reach + 0.5 + 0.02 * v0.length());
    let lo = s.position.min(s.position + motion) - margin;
    let hi = s.position.max(s.position + motion) + margin;
    let mut cands: Vec<u32> = Vec::with_capacity(128);
    world.query_box(lo, hi, &mut cands);

    // --- 3. Suspension.
    let w_n = omega(p);
    let rear_share = p.weight_rear.clamp(0.1, 0.9);
    let corner_mass = |i: usize| if i < 2 { m * (1.0 - rear_share) * 0.5 } else { m * rear_share * 0.5 };
    let mut contacts: [Option<WheelContact>; 4] = [None; 4];
    let lift = radius;
    let bump_start = travel * p.bump_zone.clamp(0.0, 0.9);
    for i in 0..4 {
        let anchor_w = s.position + rot * anchors[i];
        let origin = anchor_w + up * lift;
        // A wheel sunk into snow rides that much below its surface, deeper or shallower over the
        // ruts.
        let mut sink = s.wheels[i].sink;
        if sink > 0.0 {
            sink *= 1.0 + SINK_RIPPLE * ripple(anchor_w.x, anchor_w.z);
        }
        let hit = world.raycast_in(&cands, origin, -up, lift + travel + radius, |h| {
            let c = h.normal.dot(up);
            if h.surface == Surface::Wall { c > 0.7 } else { c > 0.3 }
        });
        if let Some(hit) = hit {
            let len = hit.distance - lift - radius + sink;
            if sink > 0.0 && len > travel {
                continue;
            }
            let mc = corner_mass(i);
            let k = mc * w_n * w_n;
            let c = 2.0 * p.damping.max(0.0) * mc * w_n;
            // Compression speed from the anchor's motion toward the ground plane under the wheel.
            let v_anchor = v0 + w0.cross(anchor_w - s.position);
            let cos_n = hit.normal.dot(up).max(0.2);
            let v_comp = (-(v_anchor.dot(hit.normal)) / cos_n).clamp(-15.0, 15.0);
            let mut f = k * (travel - len) + c * v_comp;
            if len < bump_start {
                f += k * p.bump_stop.max(1.0) * (bump_start - len) + c * v_comp.max(0.0);
            }
            contacts[i] = Some(WheelContact { hit, len, force: f.max(0.0) });
        }
    }
    // Anti-roll bars.
    for (a, b) in [(0usize, 1usize), (2, 3)] {
        let comp = |c: &Option<WheelContact>| c.map_or(0.0, |c| (travel - c.len).max(0.0));
        let k = corner_mass(a) * w_n * w_n * p.anti_roll.max(0.0);
        let f = k * (comp(&contacts[a]) - comp(&contacts[b]));
        if let Some(c) = contacts[a].as_mut() {
            c.force = (c.force + f).max(0.0);
        }
        if let Some(c) = contacts[b].as_mut() {
            c.force = (c.force - f).max(0.0);
        }
    }
    let n_contact = contacts.iter().filter(|c| c.is_some()).count();
    let grounded = n_contact > 0;

    // Ground normal, surface mix and grip capacity (in g).
    let mut n_sum = Vec3::ZERO;
    let (mut top_mult, mut traction_mult, mut yaw_mult, mut drift_max, mut rolling, mut drag) =
        (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (mut lat_raw, mut long_raw) = (0.0f32, 0.0f32);
    let mut weights = [0.0f32; 4];
    let sens = p.load_sensitivity.clamp(0.0, 1.0);
    // The wheels that drive and brake (skis never do), and how much of the drift's tighter turn
    // the surfaces allow.
    let mut n_drive = 0usize;
    let mut drift_turn_mult = 0.0f32;
    let mut response = 0.0f32;
    let mut slide_cost = 0.0f32;
    for i in 0..4 {
        let Some(c) = contacts[i] else { continue };
        let sg = p.surface(c.hit.surface);
        let ski = p.front_skis && i < 2;
        n_sum += c.hit.normal;
        top_mult += sg.top_speed;
        yaw_mult += sg.yaw;
        drift_max += sg.drift_angle_deg;
        drift_turn_mult += sg.drift_turn;
        response += sg.response;
        slide_cost += sg.slide_cost;
        rolling += sg.rolling;
        drag += sg.drag;
        let rho = c.force * c.hit.normal.dot(up).max(0.0) / (corner_mass(i) * g.max(0.1));
        let rho_eff = (rho_ref(rho) + (rho - rho_ref(rho)) * sens).clamp(0.0, 2.5);
        let share = corner_mass(i) / m * rho_eff;
        lat_raw += sg.grip * share;
        if !ski {
            traction_mult += sg.traction;
            long_raw += sg.long_grip * share;
            n_drive += 1;
        }
        weights[i] = sg.grip * share;
        s.wheels[i].load = rho;
    }
    if p.front_skis {
        // The rear wheels alone carry the car's traction and braking grip.
        long_raw /= rear_share;
    }
    let n_avg = if grounded {
        let k = 1.0 / n_contact as f32;
        top_mult *= k;
        traction_mult *= if n_drive > 0 { 1.0 / n_drive as f32 } else { 0.0 };
        yaw_mult *= k;
        drift_max *= k;
        drift_turn_mult *= k;
        response = (response * k).max(0.05);
        slide_cost *= k;
        rolling *= k;
        drag *= k;
        n_sum.normalize_or(up)
    } else {
        top_mult = 1.0;
        traction_mult = 1.0;
        yaw_mult = 1.0;
        drift_turn_mult = 1.0;
        response = 1.0;
        slide_cost = 1.0;
        up
    };
    let scale = p.grip_scale.max(0.0);
    lat_raw *= scale;
    long_raw *= scale;
    // Brief contact losses (bumps, crests, seams) keep the grip and the steering authority.
    let memory = p.grip_memory.max(0.01);
    s.grip = follow(s.grip, lat_raw, 25.0, 1.0 / memory, dt);
    s.contact = follow(s.contact, (n_contact as f32 / 2.0).min(1.0), 25.0, 1.0 / memory, dt);
    if grounded {
        s.ground_normal = n_avg;
    }

    // Gravity: stronger in the air if wanted, weaker along slopes while on the wheels.
    let mut gravity = Vec3::new(0.0, -g, 0.0);
    if grounded {
        let tangential = gravity - n_avg * gravity.dot(n_avg);
        gravity -= tangential * (1.0 - p.slope_gravity.max(0.0));
    } else {
        gravity *= p.air_gravity.max(0.0);
    }
    let mut force = gravity * m;
    let mut torque = Vec3::ZERO;

    let v_plane = v0 - n_avg * v0.dot(n_avg);
    let speed_plane = v_plane.length();
    let (path_fwd, path_left) = path_axes(rot, s.drift_angle, n_avg);
    let v_long = v0.dot(path_fwd);
    let v_lat = v0.dot(path_left);

    // --- 4. Steering and grip usage. The steering asks for a turn rate (the kinematic yaw rate of
    // the wheel angle, capped). Grip usage u = the lateral acceleration that asks for / the grip.
    // Up to u = 1 the path turns as asked (on rails; from `mark_start` the tyres leave marks while
    // still aligned). Beyond, the path bends at the grip limit and the excess becomes a drift angle
    // (the body turned further into the turn than its path), growing smoothly with the excess, up to
    // the surface's maximum, for as long as the steering asks for it.
    let path_before = s.path_rate;
    let usage;
    let drift_ref_rate;
    let mut exit_pull = 0.0;
    {
        let cap = p.yaw_cap_at(vf) * yaw_mult;
        let command = (vf * tanf(steer_angle) / wheelbase * yaw_mult).clamp(-cap, cap) * s.contact;
        let releasing = command.abs() < s.yaw_cmd.abs() && command * s.yaw_cmd >= 0.0;
        let k = if releasing { p.yaw_release } else { p.yaw_response }.max(0.0);
        s.yaw_cmd += (command - s.yaw_cmd) * (k * dt).min(1.0);
        let grip_acc = (s.grip * G).max(0.1);
        usage = if speed_plane > 2.0 { s.yaw_cmd.abs() * speed_plane / grip_acc } else { 0.0 };
        let mut target = if grounded && vf > 0.0 && usage > 1.0 {
            let e = (usage - 1.0) / p.drift_excess_full.max(0.05);
            s.yaw_cmd.signum() * drift_max.max(0.0).to_radians() * drift_curve(e)
        } else if grounded {
            0.0
        } else {
            s.drift_ref
        };
        if grounded && vf > 2.0 && p.brake_pivot_deg > 0.0 {
            // Braking in a turn locks the rear wheels: the rear swings out toward the outside, the
            // nose in (the steering's side), up to the surface's largest drift angle.
            let pivot = (-s.steer * brake).clamp(-1.0, 1.0) * p.brake_pivot_deg.min(drift_max).max(0.0).to_radians();
            if pivot.abs() > target.abs() && pivot * target >= 0.0 {
                target = pivot;
            }
        }
        let building = target.abs() > s.drift_ref.abs() && target * s.drift_ref >= 0.0;
        let tau = (if building { p.drift_build_s } else { p.drift_release_s } / response).max(dt);
        let mut rate = (target - s.drift_ref) / tau;
        if building {
            // The body swings out no faster than this relative to its path (no whip).
            let most = p.drift_swing_rate.max(0.1) * response;
            rate = rate.clamp(-most, most);
        } else if s.drift_ref != 0.0 {
            // The angle closes by the body turning less than its path, never against it: while
            // the steering still asks for the turn, the body keeps turning inward (at least
            // `1 - drift_hold_turn` of the path's rate). Released or countersteered, it also
            // settles at `drift_swing_rate`.
            let dir = s.drift_ref.signum();
            let into = (-s.steer * dir).clamp(0.0, 1.0);
            let path_turn = (s.path_rate * dir).max(0.0);
            let most = p.drift_hold_turn.clamp(0.0, 1.0) * path_turn + (1.0 - into) * p.drift_swing_rate.max(0.1) * response;
            if rate * dir < -most {
                rate = -most * dir;
            }
        }
        let before = s.drift_ref;
        s.drift_ref = (s.drift_ref + rate * dt).clamp(-MAX_DRIFT, MAX_DRIFT);
        if (before - target) * (s.drift_ref - target) < 0.0 {
            s.drift_ref = target;
        }
        drift_ref_rate = (s.drift_ref - before) / dt;
        // The end of a drift under throttle: the path swings onto the body by `drift_exit` of the
        // angle closing, so the car leaves along its nose instead of the nose swinging back.
        if grounded && p.drift_exit > 0.0 && !building && drift_ref_rate * before < 0.0 {
            exit_pull = -drift_ref_rate * p.drift_exit.clamp(0.0, 1.0) * gas;
        }

        // The path: as asked within the grip. Beyond it, drifting lets the car turn tighter: as
        // the drift angle develops toward what the steering asks, the path's achievable rate grows
        // from the grip limit toward the full command (`drift_turn` of the way), paid in speed.
        let wanted = if !grounded {
            0.0
        } else if speed_plane > 2.0 {
            let grip_rate = grip_acc / speed_plane;
            // Full once the body is 10° out (or at its target if smaller); stays full while the
            // angle decays, so the end of a drift never loosens the line.
            let full_at = target.abs().min(10f32.to_radians());
            let progress = if full_at > 1e-3 { (s.drift_ref.abs() / full_at).min(1.0) } else { 1.0 };
            let extra = (s.yaw_cmd.abs() - grip_rate).max(0.0) * (p.drift_turn * drift_turn_mult).clamp(0.0, 1.0) * progress;
            let most = grip_rate + extra;
            s.yaw_cmd.clamp(-most, most)
        } else {
            s.yaw_cmd
        };
        // The path can always turn less at once, but turns tighter only at the steering's own
        // response, so grip coming back (dirt to road) never jerks the car into the turn.
        s.path_rate = if wanted.abs() > s.path_rate.abs() && wanted * s.path_rate > 0.0 {
            s.path_rate + (wanted - s.path_rate) * (p.yaw_response.max(0.0) * dt).min(1.0)
        } else {
            wanted
        };
        if exit_pull != 0.0 {
            s.path_rate += exit_pull;
        }
    }
    let path_accel = (s.path_rate - path_before) / dt;

    // --- 5. Tyres: along the path (engine, brakes, rolling) and across it (keep on the path).
    let top_scale = top_mult.max(0.05);
    let mut drive = 0.0;
    let mut brake_total = 0.0;
    if gas > 0.0 && v_long > -0.5 {
        drive = m * p.engine_push(v_long / top_scale) * gas * traction_mult;
    }
    if brake > 0.0 && v_long < p.reverse_engage_kmh / 3.6 && gas == 0.0 {
        let vrev = (p.reverse_speed_kmh / 3.6 * top_scale).max(0.5);
        let back_speed = (-v_long).max(0.0);
        drive -= m * p.reverse_accel * brake * (vrev - back_speed).clamp(0.0, 1.0);
    }
    if brake > 0.0 && v_long >= 0.8 {
        brake_total = m * p.brake * brake;
    } else if gas > 0.0 && v_long <= -0.5 {
        brake_total = m * p.brake * gas;
    }
    let hold = gas == 0.0 && brake == 0.0 && v0.length() < 0.8;

    let mut tyre = Vec3::ZERO;
    let mut wheelspin = 0.0f32;
    let mut long_usage = 0.0f32;
    if grounded {
        let cap_long = long_raw * G * m;
        let mut resist = brake_total + rolling * m + drag * m * v_long.abs();
        if hold {
            resist += cap_long;
        }
        let stop = -(m * v_long / dt + gravity.dot(path_fwd) * m);
        let demand = drive + stop.clamp(-resist, resist);
        let f_long = demand.clamp(-cap_long, cap_long);
        if cap_long > 0.0 {
            wheelspin = ((demand.abs() - cap_long) / cap_long).clamp(0.0, 1.0);
        }
        // Cancel the velocity across the path as the path will be at the end of the tick.
        let v_lat_end = v_lat - v_long * s.path_rate * dt;
        let need = -(m * v_lat_end / dt + gravity.dot(path_left) * m);
        let u = if cap_long > 0.0 { (f_long.abs() / cap_long).min(1.0) } else { 0.0 };
        long_usage = if cap_long > 0.0 { f_long.abs() / cap_long } else { 0.0 };
        let grip_cap = s.grip * G * m * sqrtf((1.0 - p.combined_grip.clamp(0.0, 1.0) * u * u).max(0.0));
        // A drift turns tighter than the grip alone: the tyres hold whatever path it allows.
        let cap_lat = grip_cap.max(s.path_rate.abs() * speed_plane * m * 1.02);
        let f_lat = need.clamp(-cap_lat, cap_lat);
        tyre = path_fwd * f_long + path_left * f_lat;
    }

    // Wheels: the tyre force shared by grip (for body roll and pitch, never yaw), and the
    // suspension's ground reaction (its tilt from bumps pushes the car but never turns it).
    let weight_sum: f32 = weights.iter().sum();
    let inv_rot = rot.inverse();
    let roll_h = p.roll_transfer.clamp(0.0, 1.0);
    let pitch_h = p.pitch_transfer.clamp(0.0, 1.0);
    let mut tyre_torque = Vec3::ZERO;
    let mut susp_total = 0.0;
    for i in 0..4 {
        let Some(c) = contacts[i] else { continue };
        let w = if weight_sum > 0.0 { weights[i] / weight_sum } else { 1.0 / n_contact as f32 };
        let local = inv_rot * (c.hit.point - s.position);
        let f_local = inv_rot * (tyre * w);
        let y_lat = local.y * roll_h;
        let y_long = local.y * pitch_h;
        tyre_torque += rot * Vec3::new(y_long * f_local.z, 0.0, -y_lat * f_local.x);

        let vertical = c.force * c.hit.normal.dot(up).max(0.0);
        force += c.hit.normal * c.force;
        torque += (rot * anchors[i]).cross(up * vertical);
        susp_total += c.force;
    }
    // Anti-rollover: the tyres' roll and pitch torques never exceed a share of what the wheel
    // loads can hold, so cornering or braking alone never tips the car over.
    let limit = p.anti_rollover.clamp(0.0, 2.0);
    for (axis, arm) in [(fwd, p.track_width * 0.5), (left, p.wheelbase * 0.5)] {
        let t = tyre_torque.dot(axis);
        let max = limit * susp_total * arm;
        if t.abs() > max {
            tyre_torque -= axis * (t - t.signum() * max);
        }
    }
    force += tyre;
    torque += tyre_torque;

    // Boosters: a wheel on a pad starts the boost over; it pushes along the path, past the
    // engine's top speed, while the wheels are on the ground, fading out.
    let boost_time = p.boost_time.max(dt);
    if contacts.iter().any(|c| c.is_some_and(|c| c.hit.surface == Surface::Booster)) {
        s.boost = boost_time;
    }
    if s.boost > 0.0 {
        if grounded {
            force += path_fwd * (m * p.boost_accel.max(0.0) * (s.boost / boost_time));
        }
        s.boost = (s.boost - dt).max(0.0);
    }

    // Drifting and turning cost speed.
    if grounded && speed_plane > 1.0 {
        let beyond = (s.path_rate.abs() * speed_plane - s.grip * G).max(0.0);
        let bleed = p.drift_bleed.max(0.0) * slide_cost.max(0.0) * (s.drift_angle.abs() / 30f32.to_radians()).min(3.0)
            + p.drift_turn_cost.max(0.0) * beyond;
        let w = s.path_rate.abs().min(p.yaw_cap_at(vf));
        let decel = (bleed + p.turn_drag.max(0.0) * w * w).min(speed_plane / dt * 0.5);
        force -= v_plane * (m * decel / speed_plane);
    }

    // Aero.
    let speed = v0.length();
    force -= v0 * (speed * m * p.air_drag.max(0.0) / 10000.0);
    if grounded {
        let x = vf / REF_SPEED_DOWNFORCE;
        force -= up * (m * G * p.downforce.max(0.0) * x * x);
    }

    // --- 6. Yaw: the body turns with its path and is held at the drift angle around it, so bumps
    // or scrapes that yaw it never bend the line.
    let i_yaw = inertia_local.y;
    let w_yaw = w0.dot(up);
    if s.contact > 0.0 {
        let w_target = s.path_rate + drift_ref_rate + BODY_ANGLE_GAIN * (s.drift_ref - s.drift_angle);
        let acc = ((path_accel + BODY_RATE_GAIN * (w_target - w_yaw)) * s.contact).clamp(-80.0, 80.0);
        torque += up * (acc * i_yaw);

        // Keep the body parallel to the ground.
        let k = p.ground_level.max(0.0);
        if k > 0.0 && n_contact >= 2 {
            let tilt = up.cross(n_avg);
            let w_tilt = w0 - up * w_yaw;
            let acc = tilt * k - w_tilt * sqrtf(k);
            torque += body.inertia(acc);
        }
    }
    if !grounded {
        let k = p.air_control.max(0.0);
        if k > 0.0 {
            torque += up * (-s.steer * k * i_yaw);
            // The brake pulls the nose up (the throttle is always held on a keyboard).
            torque += left * (-brake * k * inertia_local.x);
        }
        let lvl = p.air_level.max(0.0);
        if lvl > 0.0 {
            let tilt = up.cross(Vec3::Y);
            let w_tilt = w0 - up * w_yaw;
            let acc = tilt * lvl - w_tilt * (sqrtf(lvl) * 0.8);
            torque += body.inertia(acc);
        }
    }

    // --- 7. Integrate velocities.
    s.velocity = v0 + force * (dt / m);
    s.angular_velocity = w0 + body.inv_inertia(torque) * dt;
    if !grounded {
        let w = s.angular_velocity.length();
        let damp = (p.air_damping.max(0.0) + p.air_damping_quad.max(0.0) * w) * dt;
        s.angular_velocity *= (1.0 - damp).max(0.0);
    }
    let spin = s.angular_velocity.length();
    if spin > MAX_SPIN {
        s.angular_velocity *= MAX_SPIN / spin;
    }

    // --- 8. Move the body in substeps and collide.
    let r_min = spheres.iter().map(|&(_, r)| r).fold(f32::INFINITY, f32::min);
    let lever = spheres.iter().map(|&(c, r)| c.length() + r).fold(0.0f32, f32::max);
    let path = s.velocity.length() * dt + s.angular_velocity.length() * lever * dt;
    let substeps = ((path / (0.4 * r_min)) as u32 + 1).clamp(1, 64);
    let h = dt / substeps as f32;
    let mut impact: f32 = 0.0;
    let mut wall_contact = false;
    for _ in 0..substeps {
        s.position += s.velocity * h;
        s.rotation = integrate_rotation(s.rotation, s.angular_velocity, h);
        let (imp, wall) = collide_body(p, s, world, &cands, &spheres, m, Vec3::ONE / inertia_local);
        impact = impact.max(imp);
        wall_contact |= wall;
    }
    if wall_contact {
        let keep = 1.0 - p.wall_drag.clamp(0.0, 1.0);
        s.velocity.x *= keep;
        s.velocity.z *= keep;
    }

    // --- 9. Drift angle: the body's rotation relative to its path.
    s.drift_angle += (s.angular_velocity.dot(s.rotation * Vec3::Y) - s.path_rate) * dt;
    // Catch: a mismatch between where the path points and where the car really goes, beyond a
    // degree or so (walls, landings, grip losses), moves the path onto the motion: the body is
    // then at an angle to it (the rear stepped out) and the controller swings it back.
    let n_now = if grounded { n_avg } else { Vec3::Y };
    if (s.velocity - n_now * s.velocity.dot(n_now)).length() > 3.0 {
        let actual = body_vs_motion(s.rotation, s.velocity, n_now);
        let tol = if grounded { p.drift_catch_deg.max(0.0).to_radians() } else { 0.0 };
        let diff = actual - s.drift_angle;
        if diff.abs() > tol {
            s.drift_angle += diff - diff.signum() * tol;
        }
    }
    s.drift_angle = s.drift_angle.clamp(-MAX_DRIFT, MAX_DRIFT);
    if !grounded {
        // In the air the wanted angle follows the body, so it lands without a jolt.
        s.drift_ref = s.drift_angle;
    }
    s.drift = (s.drift_angle.abs() / 20f32.to_radians()).min(1.0);

    // --- 10. Bookkeeping.
    s.acceleration = (s.velocity - v0) * (1.0 / dt);
    s.impact = impact;
    s.wall_contact = wall_contact;
    s.air_ticks = if grounded { 0 } else { s.air_ticks.saturating_add(1) };

    // Engine revs for the sound.
    let vmax = (p.top_speed_kmh / 3.6 * top_scale).max(1.0);
    let target = if grounded { (vf.abs() / vmax + wheelspin * gas * 0.3).min(1.0) } else { gas.max(s.engine * 0.98) };
    s.engine += (target - s.engine).clamp(-3.0 * dt, 5.0 * dt);

    // Hard braking at speed skids the braking tyres, for the eye and the ear only.
    s.skid = if grounded { smoothstep(0.3, 1.0, brake) * smoothstep(3.0, 12.0, vf) } else { 0.0 };

    // Wheels for the renderer. Tyre marks: none while gripping, rising near the limit (still
    // aligned), full once drifting, spinning or skidding under the brakes; smear = how sideways
    // each tyre moves.
    s.grip_usage = usage;
    let mark_from = p.mark_start.clamp(0.0, 0.99);
    let mark = smoothstep(mark_from, 1.0, usage).max(smoothstep(mark_from, 1.0, long_usage)).max(wheelspin);
    let kinematic = if speed_plane > 2.0 { libm::atanf(wheelbase * s.path_rate / speed_plane) } else { 0.0 };
    let drift_look = (s.drift_angle.abs() / 20f32.to_radians()).min(1.0);
    let display = (-s.steer * DISPLAY_LOCK * (1.0 - 0.6 * drift_look) - s.drift_angle).clamp(-0.7, 0.7);
    for i in 0..4 {
        let w = &mut s.wheels[i];
        w.anchor = anchors[i];
        w.steer = if i < 2 { steer_angle } else { 0.0 };
        w.steer_display = if i < 2 { display } else { 0.0 };
        match contacts[i] {
            Some(c) => {
                w.contact = true;
                w.surface = Some(c.hit.surface);
                w.suspension = c.len.clamp(0.0, travel);
                w.contact_point = c.hit.point;
                w.contact_normal = c.hit.normal;
                w.spin_rate = v_long / radius + wheelspin * gas * 8.0;
                // Rear tyres slide by the drift angle; front tyres by it plus any steering beyond
                // what the path needs (path assumed through the rear axle).
                let slip_angle = if i < 2 { s.drift_angle + steer_angle - kinematic } else { s.drift_angle };
                w.smear = libm::fabsf(sinf(slip_angle));
                let sg = p.surface(c.hit.surface);
                // Skis never brake.
                let skid = if p.front_skis && i < 2 { 0.0 } else { s.skid };
                w.mark = mark.max(sg.trail).max(skid);
                let sink = if p.front_skis && i < 2 { sg.sink * SKI_SINK } else { sg.sink };
                w.sink += (sink - w.sink) * (dt / SINK_TIME).min(1.0);
                w.slip = w.smear.max(wheelspin).max(skid);
            }
            None => {
                w.contact = false;
                w.surface = None;
                w.suspension = travel;
                w.sink += (0.0 - w.sink) * (dt / SINK_TIME).min(1.0);
                w.slip = 0.0;
                w.load = 0.0;
                w.mark = 0.0;
                w.smear = 0.0;
                let target = if gas > 0.0 { vmax / radius * gas } else { w.spin_rate };
                let target = if brake > 0.0 { 0.0 } else { target };
                w.spin_rate += (target - w.spin_rate) * (3.0 * dt);
            }
        }
        if p.front_skis && i < 2 {
            w.spin_rate = 0.0;
        }
        w.spin = (w.spin + w.spin_rate * dt) % TWO_PI;
    }
    s.tick += 1;
}

/// Pushes the body out of the triangles it touches and applies the impulses.
/// Returns the hardest impact speed and whether a wall was touched.
fn collide_body(
    p: &CarParams,
    s: &mut CarState,
    world: &World,
    cands: &[u32],
    spheres: &[(Vec3, f32)],
    m: f32,
    inv_i: Vec3,
) -> (f32, bool) {
    const MAX: usize = 24;
    let mut contacts = [BodyContact::default(); MAX];
    let mut count = 0;
    for &(offset, r) in spheres {
        let c = s.position + s.rotation * offset;
        let first = count;
        for &ti in cands {
            let tri = &world.tris[ti as usize];
            if tri.n == Vec3::ZERO {
                continue;
            }
            let plane = (c - tri.a).dot(tri.n);
            if plane.abs() >= r {
                continue;
            }
            let q = closest_point(tri, c);
            let d = c - q;
            let d2 = d.dot(d);
            if d2 >= r * r {
                continue;
            }
            let dist = sqrtf(d2);
            let normal = if dist > 1e-5 {
                d / dist
            } else if (s.position - q).dot(tri.n) >= 0.0 {
                tri.n
            } else {
                -tri.n
            };
            let depth = r - dist;
            let wall = normal.y < 0.55 || (tri.surface == Surface::Wall && normal.y < 0.8);
            // Merge with a contact of this sphere facing the same way (tessellated surfaces).
            let mut merged = false;
            for existing in &mut contacts[first..count] {
                if existing.normal.dot(normal) > 0.995 {
                    if depth > existing.depth {
                        *existing = BodyContact { point: q, normal, depth, wall };
                    }
                    merged = true;
                    break;
                }
            }
            if !merged && count < MAX {
                contacts[count] = BodyContact { point: q, normal, depth, wall };
                count += 1;
            }
        }
    }
    if count == 0 {
        return (0.0, false);
    }

    // Position: one translation satisfying the contacts in turn.
    let mut push = Vec3::ZERO;
    for c in &contacts[..count] {
        let residual = c.depth - push.dot(c.normal);
        if residual > 0.0 {
            push += c.normal * residual;
        }
    }
    s.position += push;

    // Velocity: sequential impulses.
    let body = Body { m, inv_i, rot: s.rotation };
    let mut impact: f32 = 0.0;
    let mut wall_hit = false;
    for pass in 0..4 {
        for c in &contacts[..count] {
            let mut r = c.point - s.position;
            let (ang, yaw) = if c.wall {
                // Walls act at the CG height: they can yaw the car but never roll or flip it.
                r.y = 0.0;
                (p.wall_rotation.clamp(0.0, 1.0), 1.0)
            } else {
                // Scrapes on the ground may roll or pitch the car but barely turn it.
                (1.0, SCRAPE_YAW)
            };
            if c.wall {
                wall_hit = true;
            }
            let vp = s.velocity + s.angular_velocity.cross(r);
            let vn = vp.dot(c.normal);
            if vn >= 0.0 {
                continue;
            }
            if pass == 0 {
                impact = impact.max(-vn);
            }
            let bounce = if c.wall { p.wall_bounce } else { p.body_bounce }.clamp(0.0, 1.0);
            let e = if -vn > 1.5 && pass == 0 { bounce } else { 0.0 };
            let kn = body.eff_mass(r, c.normal, ang, yaw);
            let jn = -(1.0 + e) * vn * kn;
            s.velocity += c.normal * (jn / m);
            s.angular_velocity += body.inv_inertia(body.torque(r, c.normal * jn, ang, yaw));

            // Friction.
            let vp2 = s.velocity + s.angular_velocity.cross(r);
            let vt = vp2 - c.normal * vp2.dot(c.normal);
            let vt_len = vt.length();
            if vt_len < 1e-4 {
                continue;
            }
            let t = -vt / vt_len;
            let kt = body.eff_mass(r, t, ang, yaw);
            let jt = if c.wall {
                // Modern walls: lose a share of the sliding speed proportional to the impact sine.
                let sine = (-vn / vp.length().max(0.1)).min(1.0);
                vt_len * (p.wall_speed_loss.max(0.0) * sine).min(1.0) * kt
            } else {
                (vt_len * kt).min(p.body_friction.max(0.0) * jn)
            };
            s.velocity += t * (jt / m);
            s.angular_velocity += body.inv_inertia(body.torque(r, t * jt, ang, yaw));
        }
    }
    (impact, wall_hit)
}
