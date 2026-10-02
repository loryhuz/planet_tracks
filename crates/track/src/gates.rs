//! The gates over the start, the checkpoints and the finish, built like the rest of the colony's
//! roads (art/roads/moodboard/07-depart-camp.jpg): an arch of fat red plastic tubes, two side by
//! side up each leg and along the top, bent round at the corners and held by grey collars; white
//! fabric sleeves hanging down the upper legs; under the top, a banner, white with checkered ends
//! and "PLANET TRACKS" across it (scene.wgsl letters it from the signs texture); a ring of big
//! sandbags round each foot; and orange straps from the top of each leg down to stakes in the
//! ground. The gate's line and its word ("DÉPART", "CHECKPOINT", "ARRIVÉE") are painted on the
//! deck (see [`kit::gate_deck_colour`]).
//!
//! The car meets the plain posts and beam the gates always had, as invisible hulls; the arch,
//! the banner and the fittings are drawn only.

use glam::Vec3;

use crate::Surface;
use crate::kit::{self, Frame, GATE_BEAM_BOTTOM, GATE_BEAM_HEIGHT, GATE_POST_HALF, color};
use crate::mesh::{MeshBuilder, add_box, add_hose, add_sandbag, add_tube};
use crate::noise::{hash2, unit};

/// Radius of the arch's tubes, and how far either tube of a pair stands from the middle of the
/// leg along the road, metres.
const TUBE_R: f32 = 0.28;
const PAIR: f32 = 0.25;
/// Radius of the bends at the top corners, metres.
const BEND: f32 = 0.9;
/// The banner: its top under the arch's top, its height, metres (scene.wgsl lays its lettering
/// for this height).
const BANNER_GAP: f32 = 0.07;
pub const BANNER_HEIGHT: f32 = 1.8;
/// The fabric sleeves on the upper legs: from under the bend, this long, this wide (radius).
const SLEEVE_LENGTH: f32 = 3.4;
const SLEEVE_R: f32 = 0.62;

/// A gate across the deck at frame `f` (the same arch for every kind of gate; the deck says which
/// it is): the posts and beam the car meets into `b` (their feet at `ground(p)` for a post at
/// horizontal position `p`), what the eye sees into `decor` (standing on the ground at
/// `terrain(p)`).
pub(crate) fn gate(b: &mut MeshBuilder, decor: &mut MeshBuilder, f: &Frame, ground: impl Fn(Vec3) -> f32, terrain: impl Fn(Vec3) -> f32) {
    let post_u = kit::gate_post_u(f.deck);
    let deck_y = f.centre().y;
    let top = deck_y + GATE_BEAM_BOTTOM + GATE_BEAM_HEIGHT;
    // The hulls: the posts and the beam.
    for side in [1.0, -1.0] {
        let base = f.horiz + f.left * (side * post_u);
        let foot = ground(base);
        let h = 0.5 * (top - foot);
        let centre = Vec3::new(base.x, foot + h, base.z);
        add_box(b, centre, Vec3::new(GATE_POST_HALF, h, GATE_POST_HALF), f.forward, Surface::Wall, color::HULL, false);
    }
    let beam = f.horiz + Vec3::Y * (deck_y + GATE_BEAM_BOTTOM + 0.5 * GATE_BEAM_HEIGHT);
    let half = Vec3::new(post_u + GATE_POST_HALF, 0.5 * GATE_BEAM_HEIGHT, 0.6);
    add_box(b, beam, half, f.forward, Surface::Wall, color::HULL, true);

    // The arch's line: the middle of its tubes, at the height of the beam's middle.
    let arch_y = deck_y + GATE_BEAM_BOTTOM + 0.5 * GATE_BEAM_HEIGHT;
    let at = |u: f32, along: f32, y: f32| Vec3::new(f.horiz.x, y, f.horiz.z) + f.left * u + f.forward * along;
    let feet = [1.0f32, -1.0].map(|side| terrain(at(side * post_u, 0.0, 0.0)));
    let seed = hash2(0x6a7e, libm::floorf(f.horiz.x * 4.0) as i32, libm::floorf(f.horiz.z * 4.0) as i32);
    // Two hoses side by side: up the left leg, round the corner, across, round, down the right.
    for along in [-PAIR, PAIR] {
        let mut pts = vec![at(post_u, along, feet[0] - 0.2), at(post_u, along, arch_y - BEND)];
        for k in 1..=6 {
            let a = core::f32::consts::FRAC_PI_2 * k as f32 / 6.0;
            pts.push(at(post_u - BEND * (1.0 - libm::cosf(a)), along, arch_y - BEND + BEND * libm::sinf(a)));
        }
        for k in 0..=6 {
            let a = core::f32::consts::FRAC_PI_2 * k as f32 / 6.0;
            pts.push(at(-post_u + BEND * (1.0 - libm::sinf(a)), along, arch_y - BEND + BEND * libm::cosf(a)));
        }
        pts.push(at(-post_u, along, feet[1] - 0.2));
        add_hose(decor, &pts, TUBE_R, 14, Surface::Wall, color::TUBE);
    }
    for (i, side) in [1.0f32, -1.0].into_iter().enumerate() {
        let u = side * post_u;
        let foot = feet[i];
        // Collars round each tube every 1.5 to 2 m up the leg, under the sleeve.
        let sleeve_top = arch_y - BEND - 0.05;
        let sleeve_bottom = (sleeve_top - SLEEVE_LENGTH).max(foot + 1.2);
        let mut y = foot + 0.9;
        let mut n = 0;
        while y < sleeve_bottom - 0.2 {
            for along in [-PAIR, PAIR] {
                add_tube(decor, at(u, along, y), at(u, along, y + 0.1), TUBE_R + 0.022, 14, Surface::Wall, color::COLLAR);
            }
            n += 1;
            y += 1.5 + 0.5 * unit(hash2(seed, i as i32, n));
        }
        // The fabric sleeve over the upper leg.
        if sleeve_top - sleeve_bottom > 0.5 {
            add_hose(decor, &[at(u, 0.0, sleeve_top), at(u, 0.0, sleeve_bottom)], SLEEVE_R, 12, Surface::Wall, color::SLEEVE);
        }
        // Big sandbags round the foot: before and after the leg, and two outside it.
        let out = f.left * side;
        let bags = [(f.forward, 1.05, 0.0), (-f.forward, 1.05, 0.0), (out, 0.9, 0.55), (out, 0.9, -0.55)];
        for (k, (dir, reach, shift)) in bags.into_iter().enumerate() {
            let r = |m: i32| unit(hash2(seed, 10 + 4 * i as i32 + k as i32, m));
            let spot = at(u, 0.0, 0.0) + dir * reach + f.forward * shift;
            let lie = if dir == out { f.forward } else { f.left };
            let yaw = 0.3 * (r(0) - 0.5);
            let along = (lie * libm::cosf(yaw) + Vec3::Y.cross(lie) * libm::sinf(yaw)).normalize();
            let size = Vec3::new(1.3 + 0.3 * r(1), 0.42 + 0.08 * r(2), 0.8 + 0.12 * r(3));
            let foot_at = Vec3::new(spot.x, terrain(spot) - 0.04, spot.z);
            add_sandbag(decor, foot_at, along, Vec3::Y, size, hash2(seed, 20 + k as i32, i as i32), true, Surface::Wall, color::SANDBAG, [0.24 + 0.24 * r(4), 0.3 + 0.7 * r(5)]);
        }
        // Orange straps from the top of the leg, out and down to stakes, one fore, one aft.
        for (k, along) in [-1.0f32, 1.0].into_iter().enumerate() {
            let r = |m: i32| unit(hash2(seed, 30 + 2 * i as i32 + k as i32, m));
            let from = at(u + side * (TUBE_R + 0.02), along * PAIR, arch_y - BEND - 0.3);
            let reach = 4.0 + 1.5 * r(0);
            let spot = at(u + side * reach, along * (2.5 + 1.0 * r(1)), 0.0);
            let anchor = Vec3::new(spot.x, terrain(spot), spot.z);
            add_tube(decor, from, anchor + Vec3::Y * 0.1, 0.03, 4, Surface::Wall, color::STRAP);
            kit::stake(decor, anchor, out, f.forward, hash2(seed, 40 + 2 * i as i32 + k as i32, 3));
        }
    }
    // The banner under the top, both faces lettered to read the right way round: from a car
    // coming through (facing -forward) and from beyond (facing +forward). Its uv: metres to the
    // viewer's right of the middle, metres down from its top.
    let w = post_u - TUBE_R - 0.05;
    let y0 = arch_y - TUBE_R - BANNER_GAP;
    let y1 = y0 - BANNER_HEIGHT;
    for (face, facing) in [(-0.004f32, -f.forward), (0.004, f.forward)] {
        // The viewer's right: -left from the front, +left from behind.
        let right = if face < 0.0 { -1.0 } else { 1.0 };
        let q = [(w, y0), (-w, y0), (-w, y1), (w, y1)].map(|(u, y)| decor.vertex_facing(at(u, face, y), facing, color::BANNER, [right * u, y0 - y]));
        let [p0, p1, p2] = [q[0], q[1], q[2]].map(|v| decor.position(v));
        if (p1 - p0).cross(p2 - p0).dot(facing) >= 0.0 {
            decor.tri(q[0], q[1], q[2], Surface::Wall);
            decor.tri(q[0], q[2], q[3], Surface::Wall);
        } else {
            decor.tri(q[0], q[2], q[1], Surface::Wall);
            decor.tri(q[0], q[3], q[2], Surface::Wall);
        }
    }
}
