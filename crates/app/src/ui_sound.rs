//! Menu sounds, synthesized on the audio thread: short cues (a click on hover, two rising notes
//! to confirm, a buzz when something is locked, the electric drive winding up before a race) and
//! a looping ambience per screen (Martian wind, a slow pad in space, the hum of the base).
//! Each cue is a few enveloped oscillators and filtered noise bursts, like a tiny modular synth.

use std::f32::consts::{PI, TAU};
use std::sync::mpsc::Receiver;

/// A sound the menu asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// Pointer over a button.
    Tick,
    /// Selection moved (a circuit, a mode).
    Select,
    /// Screen change: a whoosh under the livery band, `1.0` forward, `-1.0` back.
    Wipe(f32),
    /// Carousel turned towards the next (`1.0`) or previous (`-1.0`) planet.
    Swipe(f32),
    /// Arrived on a locked planet: scrambled radio.
    Static,
    Confirm,
    Back,
    /// Something locked or not there yet.
    Deny,
    Tab,
    SheetOpen,
    SheetClose,
    /// The title screen starts.
    Boot,
    /// The electric drive winds up while the circuit loads.
    Launch,
    /// The circuit is ready.
    Go,
}

/// Looping background of the menu screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Ambience {
    #[default]
    Off = 0,
    /// Title screen: wind and a far pad.
    Title = 1,
    /// Planet choice: a slow breathing pad, lighter wind.
    Space = 2,
    /// Modes and circuits: stronger wind and the hum of the installations.
    Base = 3,
}

impl Ambience {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Title,
            2 => Self::Space,
            3 => Self::Base,
            _ => Self::Off,
        }
    }

    /// Levels of wind, whistle, pad and hum.
    fn levels(self) -> [f32; 4] {
        match self {
            Self::Off => [0.0; 4],
            Self::Title => [0.5, 0.5, 0.1, 0.0],
            Self::Space => [0.35, 0.6, 0.13, 0.0],
            Self::Base => [0.6, 0.3, 0.0, 0.06],
        }
    }
}

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Triangle,
    Square,
    Saw,
}

#[derive(Clone, Copy)]
enum Filter {
    Low,
    Band,
    High,
}

/// Zavalishin's state-variable filter (stable at any cutoff). The band-pass is scaled to unity
/// gain at its centre, as a Web Audio band-pass is.
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn run(&mut self, x: f32, freq: f32, q: f32, rate: f32, kind: Filter) -> f32 {
        let g = (PI * freq.clamp(10.0, rate * 0.45) / rate).tan();
        let k = 1.0 / q.max(0.05);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match kind {
            Filter::Low => v2,
            Filter::Band => k * v1,
            Filter::High => x - k * v1 - v2,
        }
    }
}

/// Exponential attack to `peak` then exponential decay, like a Web Audio gain ramp.
#[derive(Clone, Copy)]
struct Env {
    /// Seconds before the note starts.
    delay: f32,
    attack: f32,
    /// Seconds held at the peak before the decay.
    hold: f32,
    decay: f32,
    peak: f32,
}

const FLOOR: f32 = 1e-4;

impl Env {
    fn gain(&self, t: f32) -> Option<f32> {
        let t = t - self.delay;
        if t < 0.0 {
            return Some(0.0);
        }
        if t < self.attack {
            return Some(FLOOR * (self.peak / FLOOR).powf(t / self.attack));
        }
        let d = t - self.attack - self.hold;
        if d < 0.0 {
            return Some(self.peak);
        }
        if d < self.decay {
            return Some(self.peak * (FLOOR / self.peak).powf(d / self.decay));
        }
        None
    }

    fn length(&self) -> f32 {
        self.attack + self.hold + self.decay
    }
}

#[derive(Clone, Copy)]
enum Source {
    Tone { wave: Wave, f0: f32, f1: f32, phase: f32 },
    Noise { kind: Filter, f0: f32, f1: f32, q: f32, svf: Svf },
}

#[derive(Clone, Copy)]
struct Voice {
    source: Source,
    env: Env,
    /// Optional low-pass on a tone: start and end cutoff, resonance.
    lowpass: Option<(f32, f32, f32)>,
    lp: Svf,
    t: f32,
}

fn sweep(f0: f32, f1: f32, x: f32) -> f32 {
    if (f1 - f0).abs() < 1e-3 { f0 } else { f0 * (f1 / f0).powf(x.clamp(0.0, 1.0)) }
}

pub struct UiSynth {
    rate: f32,
    rx: Option<Receiver<Cue>>,
    voices: Vec<Voice>,
    seed: u32,
    // ambience
    levels: [f32; 4],
    brown: [f32; 2],
    wind_lp: Svf,
    whistle_bp: Svf,
    pad_lp: Svf,
    lfo: [f32; 2],
    pad_phase: [f32; 4],
    tremolo: [f32; 4],
    hum_phase: [f32; 3],
}

const PAD: [(f32, Wave, f32); 4] =
    [(110.0, Wave::Sine, -9.0), (164.81, Wave::Triangle, -3.0), (220.6, Wave::Sine, 3.0), (277.18, Wave::Triangle, 9.0)];

impl UiSynth {
    pub fn new(rate: f32, rx: Option<Receiver<Cue>>) -> Self {
        Self {
            rate,
            rx,
            voices: Vec::with_capacity(64),
            seed: 0x9e37_79b9,
            levels: [0.0; 4],
            brown: [0.0; 2],
            wind_lp: Svf::default(),
            whistle_bp: Svf::default(),
            pad_lp: Svf::default(),
            lfo: [0.0; 2],
            pad_phase: [0.0; 4],
            tremolo: [0.0; 4],
            hum_phase: [0.0; 3],
        }
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn rand(&mut self) -> f32 {
        0.5 + 0.5 * self.noise()
    }

    fn push(&mut self, v: Voice) {
        if self.voices.len() < self.voices.capacity() {
            self.voices.push(v);
        }
    }

    /// A tone sweeping from `f0` to `f1` over its whole length.
    #[allow(clippy::too_many_arguments)]
    fn tone(&mut self, wave: Wave, f0: f32, f1: f32, delay: f32, decay: f32, peak: f32, attack: f32, lowpass: Option<(f32, f32, f32)>) {
        self.push(Voice {
            source: Source::Tone { wave, f0, f1, phase: 0.0 },
            env: Env { delay, attack, hold: 0.0, decay, peak },
            lowpass,
            lp: Svf::default(),
            t: 0.0,
        });
    }

    /// Filtered white noise whose cutoff sweeps from `f0` to `f1`.
    #[allow(clippy::too_many_arguments)]
    fn hiss(&mut self, kind: Filter, f0: f32, f1: f32, q: f32, delay: f32, decay: f32, peak: f32, attack: f32) {
        self.push(Voice {
            source: Source::Noise { kind, f0, f1, q, svf: Svf::default() },
            env: Env { delay, attack, hold: 0.0, decay, peak },
            lowpass: None,
            lp: Svf::default(),
            t: 0.0,
        });
    }

    pub fn trigger(&mut self, cue: Cue) {
        use Filter::*;
        use Wave::*;
        match cue {
            Cue::Tick => {
                self.tone(Sine, 2400.0, 1900.0, 0.0, 0.03, 0.06, 0.005, None);
                self.hiss(High, 6000.0, 6000.0, 0.7, 0.0, 0.012, 0.035, 0.005);
            }
            Cue::Select => {
                self.tone(Sine, 1500.0, 1420.0, 0.0, 0.05, 0.07, 0.005, None);
                self.tone(Triangle, 3000.0, 3000.0, 0.0, 0.015, 0.02, 0.003, None);
            }
            Cue::Wipe(dir) => {
                let (a, b) = if dir >= 0.0 { (300.0, 2400.0) } else { (2400.0, 300.0) };
                self.hiss(Band, a, b, 0.8, 0.0, 0.42, 0.13, 0.16);
            }
            Cue::Swipe(dir) => {
                let (a, b) = if dir >= 0.0 { (500.0, 2600.0) } else { (2600.0, 500.0) };
                self.hiss(Band, a, b, 1.4, 0.0, 0.3, 0.2, 0.06);
                self.tone(Sine, 220.0, if dir >= 0.0 { 330.0 } else { 165.0 }, 0.0, 0.25, 0.04, 0.005, None);
            }
            Cue::Static => {
                for i in 0..6 {
                    let f = 1600.0 + self.rand() * 2600.0;
                    self.hiss(Band, f, f, 6.0, 0.2 + i as f32 * 0.045, 0.03, 0.09 * (1.0 - i as f32 / 7.0), 0.01);
                }
                self.tone(Square, 62.0, 58.0, 0.2, 0.28, 0.025, 0.005, Some((300.0, 300.0, 0.7)));
            }
            Cue::Confirm => {
                self.tone(Triangle, 660.0, 660.0, 0.0, 0.08, 0.15, 0.005, None);
                self.tone(Triangle, 990.0, 990.0, 0.07, 0.16, 0.15, 0.005, None);
                self.tone(Sine, 1980.0, 1980.0, 0.07, 0.2, 0.035, 0.005, None);
                self.tone(Sine, 140.0, 48.0, 0.0, 0.16, 0.32, 0.005, None);
                self.hiss(High, 3000.0, 3000.0, 0.7, 0.0, 0.04, 0.06, 0.005);
            }
            Cue::Back => {
                self.tone(Triangle, 880.0, 880.0, 0.0, 0.07, 0.12, 0.005, None);
                self.tone(Triangle, 587.0, 587.0, 0.06, 0.12, 0.12, 0.005, None);
                self.tone(Sine, 120.0, 60.0, 0.0, 0.1, 0.14, 0.005, None);
            }
            Cue::Deny => {
                self.tone(Saw, 150.0, 140.0, 0.0, 0.08, 0.12, 0.005, Some((700.0, 700.0, 0.7)));
                self.tone(Saw, 150.0, 130.0, 0.11, 0.12, 0.12, 0.005, Some((700.0, 700.0, 0.7)));
            }
            Cue::Tab => {
                self.tone(Square, 1320.0, 1320.0, 0.0, 0.025, 0.04, 0.003, Some((3500.0, 3500.0, 0.7)));
                self.tone(Sine, 880.0, 1100.0, 0.02, 0.06, 0.07, 0.005, None);
            }
            Cue::SheetOpen => {
                self.hiss(Band, 300.0, 1800.0, 0.9, 0.0, 0.35, 0.15, 0.12);
                self.tone(Sine, 330.0, 660.0, 0.0, 0.3, 0.05, 0.05, None);
            }
            Cue::SheetClose => {
                self.hiss(Band, 1800.0, 300.0, 0.9, 0.0, 0.26, 0.13, 0.04);
                self.tone(Sine, 600.0, 300.0, 0.0, 0.2, 0.04, 0.005, None);
            }
            Cue::Boot => {
                self.tone(Sine, 55.0, 55.0, 0.0, 2.2, 0.22, 0.6, None);
                self.tone(Sine, 82.4, 82.4, 0.0, 2.2, 0.12, 0.8, None);
                self.hiss(Band, 200.0, 3000.0, 0.7, 0.0, 1.1, 0.1, 0.6);
                for (i, f) in [440.0, 659.25, 987.77, 1318.5].into_iter().enumerate() {
                    let at = 0.55 + i as f32 * 0.09;
                    self.tone(Sine, f, f, at, 1.2, 0.08, 0.005, None);
                    self.tone(Triangle, f * 2.004, f * 2.004, at, 0.5, 0.015, 0.005, None);
                }
            }
            Cue::Launch => {
                // Three detuned saws winding up under a resonant low-pass that opens with them,
                // held at full level until the cut.
                let d = 1.7;
                for (m, det, v) in [(1.0, 1.0, 0.6), (1.5, 1.004, 0.22), (2.0, 0.997, 0.18)] {
                    self.tone(Saw, 140.0 * m * det, 1250.0 * m * det, 0.0, 0.4, 0.15 * v, 0.25, Some((500.0, 5200.0, 1.6)));
                    if let Some(last) = self.voices.last_mut() {
                        last.env.hold = d - 0.4;
                    }
                }
                self.tone(Sine, 900.0, 4200.0, 0.0, d - 0.3, 0.025, 0.3, None);
                self.hiss(High, 800.0, 4000.0, 0.5, 0.0, d - 0.4, 0.07, 0.4);
            }
            Cue::Go => {
                for (i, f) in [523.25, 659.25, 783.99, 1046.5].into_iter().enumerate() {
                    self.tone(Triangle, f, f, i as f32 * 0.035, 0.5, 0.1, 0.005, None);
                }
                self.tone(Sine, 150.0, 45.0, 0.0, 0.25, 0.4, 0.005, None);
                self.hiss(High, 2500.0, 2500.0, 0.7, 0.0, 0.3, 0.12, 0.005);
            }
        }
    }

    /// The next sample of cues and ambience (`ambience` is the screen's background, faded in and
    /// out over about a second).
    pub fn next(&mut self, ambience: Ambience) -> f32 {
        if let Some(rx) = &self.rx {
            let mut cues = [None; 8];
            for slot in cues.iter_mut() {
                match rx.try_recv() {
                    Ok(c) => *slot = Some(c),
                    Err(_) => break,
                }
            }
            for c in cues.into_iter().flatten() {
                self.trigger(c);
            }
        }
        let rate = self.rate;
        let dt = 1.0 / rate;
        let mut out = 0.0;
        let mut i = 0;
        while i < self.voices.len() {
            let n = if matches!(self.voices[i].source, Source::Noise { .. }) { self.noise() } else { 0.0 };
            let Some(g) = self.voices[i].env.gain(self.voices[i].t) else {
                self.voices.swap_remove(i);
                continue;
            };
            let v = &mut self.voices[i];
            let x = ((v.t - v.env.delay) / v.env.length().max(1e-3)).clamp(0.0, 1.0);
            let s = match &mut v.source {
                Source::Tone { wave, f0, f1, phase } => {
                    let f = sweep(*f0, *f1, x);
                    *phase = (*phase + f * dt).fract();
                    let p = *phase;
                    match wave {
                        Wave::Sine => (p * TAU).sin(),
                        Wave::Triangle => 4.0 * (p - 0.5).abs() - 1.0,
                        Wave::Square => {
                            if p < 0.5 { 0.7 } else { -0.7 }
                        }
                        Wave::Saw => 2.0 * p - 1.0,
                    }
                }
                Source::Noise { kind, f0, f1, q, svf } => svf.run(n, sweep(*f0, *f1, x), *q, rate, *kind),
            };
            let s = match v.lowpass {
                Some((a, b, q)) => v.lp.run(s, sweep(a, b, x), q, rate, Filter::Low),
                None => s,
            };
            if v.t >= v.env.delay {
                out += s * g;
            }
            v.t += dt;
            i += 1;
        }
        out + self.ambience(ambience) * 0.55
    }

    fn ambience(&mut self, ambience: Ambience) -> f32 {
        let target = ambience.levels();
        let k = 1.0 - (-1.0 / (0.6 * self.rate)).exp();
        for (l, t) in self.levels.iter_mut().zip(target) {
            *l += (t - *l) * k;
        }
        if self.levels.iter().all(|&l| l < 1e-4) {
            return 0.0;
        }
        let rate = self.rate;
        let dt = 1.0 / rate;
        // Brown noise: a leaky integral of white noise (two of them, one brighter).
        let w1 = self.noise();
        let w2 = self.noise();
        self.brown[0] = (self.brown[0] + 0.02 * w1) / 1.02;
        self.brown[1] = (self.brown[1] + 0.034 * w2) / 1.034;
        self.lfo[0] = (self.lfo[0] + 0.07 * dt).fract();
        self.lfo[1] = (self.lfo[1] + 0.045 * dt).fract();
        let wind_f = 420.0 + 260.0 * (self.lfo[0] * TAU).sin();
        let wind = self.wind_lp.run(self.brown[0] * 8.0, wind_f, 0.8, rate, Filter::Low);
        let whistle_f = 900.0 + 380.0 * (self.lfo[1] * TAU).sin();
        let whistle = self.whistle_bp.run(self.brown[1] * 8.0, whistle_f, 9.0, rate, Filter::Band);
        let mut pad = 0.0;
        for (i, (f, wave, cents)) in PAD.iter().enumerate() {
            let f = f * 2f32.powf(cents / 1200.0);
            self.pad_phase[i] = (self.pad_phase[i] + f * dt).fract();
            self.tremolo[i] = (self.tremolo[i] + (0.11 + 0.05 * i as f32) * dt).fract();
            let p = self.pad_phase[i];
            let s = match wave {
                Wave::Triangle => 4.0 * (p - 0.5).abs() - 1.0,
                _ => (p * TAU).sin(),
            };
            let gain = (0.25 + 0.12 * (self.tremolo[i] * TAU).sin()) / (i as f32 + 1.0);
            pad += s * gain;
        }
        let pad = self.pad_lp.run(pad, 900.0, 0.7, rate, Filter::Low);
        let mut hum = 0.0;
        for (i, (f, g)) in [(50.0, 0.5), (100.0, 0.22), (150.0, 0.08)].into_iter().enumerate() {
            self.hum_phase[i] = (self.hum_phase[i] + f * dt).fract();
            hum += (self.hum_phase[i] * TAU).sin() * g;
        }
        let [lw, lh, lp, lu] = self.levels;
        wind * lw + whistle * lh + pad * lp + hum * lu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(synth: &mut UiSynth, ambience: Ambience, seconds: f32) -> Vec<f32> {
        (0..(seconds * synth.rate) as usize).map(|_| synth.next(ambience)).collect()
    }

    #[test]
    fn every_cue_sounds_and_ends() {
        let cues = [
            Cue::Tick,
            Cue::Select,
            Cue::Wipe(1.0),
            Cue::Wipe(-1.0),
            Cue::Swipe(1.0),
            Cue::Swipe(-1.0),
            Cue::Static,
            Cue::Confirm,
            Cue::Back,
            Cue::Deny,
            Cue::Tab,
            Cue::SheetOpen,
            Cue::SheetClose,
            Cue::Boot,
            Cue::Launch,
            Cue::Go,
        ];
        for cue in cues {
            let mut synth = UiSynth::new(44_100.0, None);
            synth.trigger(cue);
            let out = render(&mut synth, Ambience::Off, 3.5);
            let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(out.iter().all(|s| s.is_finite()), "{cue:?} produced NaN");
            assert!(peak > 0.005, "{cue:?} is silent (peak {peak})");
            assert!(peak < 1.2, "{cue:?} is too loud (peak {peak})");
            assert!(synth.voices.is_empty(), "{cue:?} still playing after 3.5 s");
        }
    }

    #[test]
    fn ambiences_are_quiet_beds() {
        for ambience in [Ambience::Title, Ambience::Space, Ambience::Base] {
            let mut synth = UiSynth::new(44_100.0, None);
            let out = render(&mut synth, ambience, 4.0);
            let tail = &out[out.len() / 2..];
            let rms = (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt();
            assert!(out.iter().all(|s| s.is_finite()));
            assert!(rms > 0.002 && rms < 0.2, "{ambience:?} rms {rms}");
        }
        let mut synth = UiSynth::new(44_100.0, None);
        assert!(render(&mut synth, Ambience::Off, 1.0).iter().all(|&s| s == 0.0));
    }
}
