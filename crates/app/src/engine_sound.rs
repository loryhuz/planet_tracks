//! Electric drive from two generated recordings held at a steady speed (assets/audio/drive_whine.wav
//! and drive_body.wav, see tools/audio/prepare.py): the motor's whine, played faster with the motor's
//! speed over its whole range (a whine scales with the speed as a whole, so a loop played faster
//! sounds right), and the drivetrain's body, a low rumble pitched over a narrower range and growing
//! with the torque. The motor's speed comes from a virtual gearbox (`revs`): first gear climbs from
//! IDLE_REVS to the redline, every other gear from SHIFT_REVS, so each change drops the whine and
//! cuts the torque for a moment. Standing still the motor is all but silent. Off throttle the drive
//! is quieter and darker, and lifting off at speed sets off a few crackling pops (synthesized: short
//! bursts of noise over a thump).

use std::f32::consts::{PI, TAU};

use crate::sample::{Looper, asset};

const WHINE_WAV: &[u8] = include_bytes!("../assets/audio/drive_whine.wav");
const BODY_WAV: &[u8] = include_bytes!("../assets/audio/drive_body.wav");

/// The motor's speed (relative to the redline) at which each recording plays as recorded.
const WHINE_REVS: f32 = 0.8;
const BODY_REVS: f32 = 0.8;
/// How much of the speed's variation the body follows (1 would be all of it, like the whine).
const BODY_SPREAD: f32 = 0.5;
/// Levels of the two recordings in the drive.
const WHINE_GAIN: f32 = 0.8;
const BODY_GAIN: f32 = 0.6;
/// Where first gear starts, and every other gear, relative to the redline.
const IDLE_REVS: f32 = 0.45;
const SHIFT_REVS: f32 = 0.7;
/// Level of the pops on lifting off.
const POP_GAIN: f32 = 1.4;

/// The motor's speed relative to the redline for `rpm` 0..1 within `gear` (0 first) and the
/// throttle `load`: in first gear the throttle alone raises it a little.
pub fn revs(rpm: f32, gear: u32, load: f32) -> f32 {
    let rpm = rpm.clamp(0.0, 1.0);
    if gear == 0 {
        (IDLE_REVS + (1.0 - IDLE_REVS) * rpm).max(IDLE_REVS + 0.3 * load)
    } else {
        SHIFT_REVS + (1.0 - SHIFT_REVS) * rpm
    }
}

pub struct EngineVoice {
    whine: Looper,
    body: Looper,
    revs: f32,
    load: f32,
    gain: f32,
    lp: f32,
    /// The throttle was down (more than 0.7) and has not been lifted since.
    pressed: bool,
    /// Time since the last gear change, seconds (large when none is playing).
    shift_t: f32,
    /// Pops still to come after a lift-off, and output samples until the next one.
    pops: u32,
    pop_wait: f32,
    /// The pop sounding: its crack's and its thump's levels (decaying), its noise's band-pass state
    /// and its thump's phase.
    pop: f32,
    thump: f32,
    pop_low: f32,
    pop_band: f32,
    pop_phase: f32,
    seed: u32,
    out_rate: f32,
    /// Per-sample smoothing of the motor's speed (50 ms) and of the throttle (60 ms).
    k_revs: f32,
    k: f32,
}

impl EngineVoice {
    pub fn new(out_rate: f32) -> Self {
        Self {
            whine: Looper::new(&asset("drive_whine.wav", WHINE_WAV), out_rate),
            body: Looper::new(&asset("drive_body.wav", BODY_WAV), out_rate),
            revs: IDLE_REVS,
            load: 0.0,
            gain: 0.0,
            lp: 0.0,
            pressed: false,
            shift_t: 10.0,
            pops: 0,
            pop_wait: 0.0,
            pop: 0.0,
            thump: 0.0,
            pop_low: 0.0,
            pop_band: 0.0,
            pop_phase: 0.0,
            seed: 0x9e37_79b9,
            out_rate,
            k_revs: 1.0 - (-1.0 / (0.05 * out_rate)).exp(),
            k: 1.0 - (-1.0 / (0.06 * out_rate)).exp(),
        }
    }

    /// Plays a gear change's torque cut (the drop of the whine comes from the gear change itself).
    pub fn shift(&mut self) {
        self.shift_t = 0.0;
    }

    /// 0..1.
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }

    /// `revs` from [`revs`] (smoothed here), `speed` in m/s, `load` 0 (off throttle) .. 1.
    pub fn next(&mut self, revs: f32, speed: f32, load: f32) -> f32 {
        self.revs += (revs - self.revs) * self.k_revs;
        self.load += (load - self.load) * self.k;
        // A slight rise with speed, so the higher gears sound faster.
        let pitch = self.revs * (0.94 + 0.12 * (speed * 3.6 / 300.0).clamp(0.0, 1.0));
        let whine = self.whine.next(pitch / WHINE_REVS).0;
        let body = self.body.next(1.0 + BODY_SPREAD * (pitch / BODY_REVS - 1.0)).0;
        // The motor is heard from walking pace; the body grows with the torque.
        let moving = (speed / 6.0).clamp(0.0, 1.0);
        let s = whine * WHINE_GAIN * (0.15 + 0.85 * moving) + body * BODY_GAIN * (0.3 + 0.7 * self.load) * (0.4 + 0.6 * moving);

        // Louder on throttle and fast; darker off throttle.
        let target_gain = (0.4 + 0.6 * self.load) * (0.6 + 0.4 * self.revs);
        self.gain += (target_gain - self.gain) * self.k;
        let cutoff = 2500.0 + 9000.0 * self.load;
        let c = 1.0 - (-TAU * cutoff / self.out_rate).exp();
        self.lp += c * (s - self.lp);
        let mut out = self.lp * self.gain;

        // Gear change: the torque cut, 60 ms, then the power comes back.
        if self.shift_t < 0.25 {
            let t = self.shift_t;
            let dip = if t < 0.005 {
                1.0 - 0.7 * t / 0.005
            } else if t < 0.06 {
                0.3
            } else {
                1.0 - 0.7 * (-(t - 0.06) / 0.04).exp()
            };
            out *= dip;
            self.shift_t += 1.0 / self.out_rate;
        }

        // Lifting off fast: a few crackling pops, 30 to 150 ms apart.
        if self.load > 0.7 {
            self.pressed = true;
        } else if self.pressed && self.load < 0.3 {
            self.pressed = false;
            if self.revs > 0.75 && speed > 15.0 {
                self.pops = 3 + (self.random() * 4.0) as u32;
                self.pop_wait = (0.02 + 0.08 * self.random()) * self.out_rate;
            }
        }
        if self.pops > 0 {
            self.pop_wait -= 1.0;
            if self.pop_wait <= 0.0 {
                self.pops -= 1;
                self.pop = 0.4 + 0.6 * self.random();
                self.thump = self.pop;
                self.pop_phase = 0.0;
                self.pop_wait = (0.03 + 0.12 * self.random()) * self.out_rate;
            }
        }
        if self.thump > 0.001 {
            let n = self.random() * 2.0 - 1.0;
            // A crack of noise ringing around 2 kHz (state-variable band-pass, Q 0.7), 8 ms, over a
            // 75 Hz thump, 45 ms.
            let f = 2.0 * (PI * 2000.0 / self.out_rate).sin();
            let high = n - self.pop_low - self.pop_band / 0.7;
            self.pop_band += f * high;
            self.pop_low += f * self.pop_band;
            self.pop_phase += 75.0 / self.out_rate;
            out += (self.pop_band * self.pop + (self.pop_phase * TAU).sin() * 0.6 * self.thump) * POP_GAIN;
            self.pop *= (-1.0 / (0.008 * self.out_rate)).exp();
            self.thump *= (-1.0 / (0.045 * self.out_rate)).exp();
        }
        out
    }
}
