//! Electric drivetrain sound from a real recording kept whole (an electric UTV's motor whine and
//! the gravel it rolled on, assets/ev_drive.wav): the seamless loop is pitched by speed and by a
//! virtual gearbox (it sweeps up within each gear). The surface only changes how it is filtered:
//! on asphalt the top is softened so the gravel crunch in the recording fades back, on dirt the
//! recording plays open and a little louder. An upshift drops the pitch to the next gear and cuts
//! the power for a moment. Louder and brighter on throttle.

const DRIVE_WAV: &[u8] = include_bytes!("../assets/ev_drive.wav");

/// Minimal reader for the PCM WAV above: returns (samples, rate).
fn decode_wav(bytes: &[u8]) -> (Vec<f32>, u32) {
    let u16le = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32le = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut rate = 48_000;
    let mut channels = 1usize;
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32le(i + 4) as usize;
        let body = i + 8;
        if id == b"fmt " {
            channels = u16le(body + 2) as usize;
            rate = u32le(body + 4);
        } else if id == b"data" {
            let end = (body + len).min(bytes.len());
            let samples = bytes[body..end]
                .chunks_exact(2 * channels)
                .map(|f| i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0)
                .collect();
            return (samples, rate);
        }
        i = body + len + (len & 1);
    }
    (Vec::new(), rate)
}

/// A looping sample read at a variable rate with linear interpolation.
struct Looper {
    samples: Vec<f32>,
    pos: f64,
}

impl Looper {
    fn new(bytes: &[u8]) -> (Self, u32) {
        let (samples, rate) = decode_wav(bytes);
        (Self { samples, pos: 0.0 }, rate)
    }

    fn next(&mut self, step: f64) -> f32 {
        let n = self.samples.len();
        if n < 2 {
            return 0.0;
        }
        self.pos += step;
        while self.pos >= n as f64 {
            self.pos -= n as f64;
        }
        let i = self.pos as usize;
        let frac = (self.pos - i as f64) as f32;
        self.samples[i] * (1.0 - frac) + self.samples[(i + 1) % n] * frac
    }
}

pub struct ElectricVoice {
    drive: Looper,
    /// Time since the last upshift, seconds (large when none is playing).
    shift_t: f32,
    /// Loop samples advanced per output sample at playback rate 1.
    step: f64,
    gain: f32,
    tone: f32,
    lp: f32,
    surface: f32,
    out_rate: f32,
    smooth: f32,
}

impl ElectricVoice {
    pub fn new(out_rate: f32) -> Self {
        let (drive, rate) = Looper::new(DRIVE_WAV);
        Self {
            drive,
            shift_t: 10.0,
            step: rate as f64 / out_rate as f64,
            gain: 0.0,
            tone: 0.0,
            lp: 0.0,
            surface: 0.0,
            out_rate,
            smooth: 1.0 - (-1.0 / (0.06 * out_rate)).exp(),
        }
    }

    /// Plays the upshift: a short power cut (the pitch drop comes from the gear change itself).
    pub fn shift(&mut self) {
        self.shift_t = 0.0;
    }

    /// `speed` in m/s, `rpm` 0..1 within the current gear, `load` 0 (off throttle) .. 1,
    /// `dirt` 0..1 share of the wheels rolling on dirt or off-track ground, `slide` 0..1.
    pub fn next(&mut self, speed: f32, rpm: f32, load: f32, dirt: f32, slide: f32) -> f32 {
        // Pitch: a slow rise with speed times a sweep within the gear (×1.7 from shift to shift).
        let v = (speed * 3.6 / 300.0).clamp(0.0, 1.2);
        let rate = (0.8 + 0.9 * v) * (0.78 + 0.55 * rpm.clamp(0.0, 1.1));
        let s = self.drive.next(self.step * rate as f64);

        // Throttle: louder and brighter; off throttle softer (regen). Quieter when cruising so the
        // steady whine does not wear on the ear; it comes forward under acceleration.
        let k = self.smooth;
        // Surface: 0 asphalt .. 1 dirt (gravel crunch fully audible), slowly blended.
        self.surface += ((dirt * (1.0 + 0.5 * slide)).min(1.0) - self.surface) * k * 0.5;
        let target_gain = (0.28 + 0.72 * load) * (0.55 + 0.45 * v.min(1.0)) * (1.0 + 0.2 * self.surface);
        self.gain += (target_gain - self.gain) * k;
        self.tone += (load - self.tone) * k;
        // Asphalt: the top is softened (the recording's gravel and hiss fade back). Dirt: open.
        let cutoff = (1700.0 + 2600.0 * self.tone) * (1.0 + 1.6 * self.surface);
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / self.out_rate).exp();
        self.lp += a * (s - self.lp);
        let mut out = self.lp * self.gain;

        // Upshift: a short power cut.
        if self.shift_t < 0.25 {
            let t = self.shift_t;
            let dip = if t < 0.045 { 0.3 } else { 1.0 - 0.7 * (-(t - 0.045) / 0.05).exp() };
            out *= dip;
            self.shift_t += 1.0 / self.out_rate;
        }
        out
    }
}
