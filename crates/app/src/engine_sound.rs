//! Combustion engine from two generated recordings held at steady revs (assets/audio/engine_mid.wav
//! and engine_high.wav, see tools/audio/prepare.py). Both loops play at once in a fixed blend, both
//! pitched together to the engine's revs: the mid one sped up carries the engine, the high one
//! slowed down adds its growl. The blend is the one heard in top gear above 250 km/h, the most
//! natural: crossfading toward the high recording at its own pitch as the revs climbed sounded like
//! another, harsher engine. Only near idle does the mid one play alone. The revs come from a
//! virtual gearbox (`revs`): first gear climbs from idle to the redline, every other gear from
//! SHIFT_REVS, so each upshift drops the pitch, and the upshift cuts the ignition for a moment. Off
//! throttle the engine is quieter and darker.

use crate::sample::Looper;

const MID_WAV: &[u8] = include_bytes!("../assets/audio/engine_mid.wav");
const HIGH_WAV: &[u8] = include_bytes!("../assets/audio/engine_high.wav");

/// Firing frequency of each recording relative to the high one's (about 108 Hz and 236 Hz).
const MID_PITCH: f32 = 0.46;
const HIGH_PITCH: f32 = 1.0;
/// The pitch at the redline, on that scale: the mid recording plays at most about ×1.85 and the high
/// one never at its own pitch.
const REDLINE_PITCH: f32 = 0.8;
/// Blend of the two recordings (equal power, they are not in phase).
const MID_GAIN: f32 = 0.8;
const HIGH_GAIN: f32 = 0.6;
/// Idle, and where every gear after the first starts, relative to the redline.
const IDLE_REVS: f32 = 0.45;
const SHIFT_REVS: f32 = 0.8;

/// Engine revs relative to the redline for `rpm` 0..1 within `gear` (0 first) and the throttle
/// `load`: in first gear the throttle alone raises them a little, as with a slipping clutch.
pub fn revs(rpm: f32, gear: u32, load: f32) -> f32 {
    let rpm = rpm.clamp(0.0, 1.0);
    if gear == 0 {
        (IDLE_REVS + (1.0 - IDLE_REVS) * rpm).max(IDLE_REVS + 0.3 * load)
    } else {
        SHIFT_REVS + (1.0 - SHIFT_REVS) * rpm
    }
}

pub struct EngineVoice {
    mid: Looper,
    high: Looper,
    revs: f32,
    load: f32,
    gain: f32,
    lp: f32,
    /// Time since the last upshift, seconds (large when none is playing).
    shift_t: f32,
    out_rate: f32,
    /// Per-sample smoothing of the revs (50 ms) and of the throttle (60 ms).
    k_revs: f32,
    k: f32,
}

impl EngineVoice {
    pub fn new(out_rate: f32) -> Self {
        Self {
            mid: Looper::new(MID_WAV, out_rate),
            high: Looper::new(HIGH_WAV, out_rate),
            revs: IDLE_REVS,
            load: 0.0,
            gain: 0.0,
            lp: 0.0,
            shift_t: 10.0,
            out_rate,
            k_revs: 1.0 - (-1.0 / (0.05 * out_rate)).exp(),
            k: 1.0 - (-1.0 / (0.06 * out_rate)).exp(),
        }
    }

    /// Plays the upshift's ignition cut (the pitch drop comes from the gear change itself).
    pub fn shift(&mut self) {
        self.shift_t = 0.0;
    }

    /// `revs` from [`revs`] (smoothed here), `speed` in m/s, `load` 0 (off throttle) .. 1.
    pub fn next(&mut self, revs: f32, speed: f32, load: f32) -> f32 {
        self.revs += (revs - self.revs) * self.k_revs;
        self.load += (load - self.load) * self.k;
        // A slight rise with speed, so the higher gears sound faster.
        let pitch = REDLINE_PITCH * self.revs * (0.94 + 0.12 * (speed * 3.6 / 300.0).clamp(0.0, 1.0));
        let mid = self.mid.next(pitch / MID_PITCH).0;
        let high = self.high.next(pitch / HIGH_PITCH).0;
        // Slowed down further than about ×0.5 the high recording turns to mud: it fades out
        // toward idle.
        let t = ((pitch - 0.36) / 0.24).clamp(0.0, 1.0);
        let s = mid * MID_GAIN + high * HIGH_GAIN * t * t * (3.0 - 2.0 * t);

        // Louder on throttle and at high revs; darker off throttle.
        let target_gain = (0.35 + 0.65 * self.load) * (0.7 + 0.3 * self.revs);
        self.gain += (target_gain - self.gain) * self.k;
        let cutoff = 2200.0 + 7000.0 * self.load;
        let c = 1.0 - (-std::f32::consts::TAU * cutoff / self.out_rate).exp();
        self.lp += c * (s - self.lp);
        let mut out = self.lp * self.gain;

        // Upshift: the ignition cut, 60 ms, then the power comes back.
        if self.shift_t < 0.25 {
            let t = self.shift_t;
            let dip = if t < 0.005 {
                1.0 - 0.85 * t / 0.005
            } else if t < 0.06 {
                0.15
            } else {
                1.0 - 0.85 * (-(t - 0.06) / 0.04).exp()
            };
            out *= dip;
            self.shift_t += 1.0 / self.out_rate;
        }
        out
    }
}
