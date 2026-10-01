//! Vehicle sound: the combustion engine from two recordings (engine_sound.rs) and the Martian
//! ambience from a third, plus synthesized wind, tyre squeal at the grip limit, gravel on dirt and
//! thumps on impacts. The game writes a few values per frame; the audio thread reads them through
//! atomics and smooths them per sample.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// What the game tells the synth each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct SoundFrame {
    /// 0..1 within the current gear.
    pub rpm: f32,
    /// Virtual gear (0 first), from the speeds where the acceleration steps down.
    pub gear: u32,
    /// Throttle 0..1.
    pub load: f32,
    /// m/s.
    pub speed: f32,
    /// Tyre squeal on road, 0..1.
    pub squeal: f32,
    /// Wheels rolling on dirt or ground, 0..1.
    pub gravel: f32,
    /// Sliding on dirt, 0..1.
    pub scrub: f32,
    pub airborne: bool,
}

#[derive(Default)]
struct Shared {
    rpm: AtomicU32,
    gear: AtomicU32,
    load: AtomicU32,
    speed: AtomicU32,
    squeal: AtomicU32,
    gravel: AtomicU32,
    scrub: AtomicU32,
    airborne: AtomicBool,
    /// Incremented for each impact; the thread plays one thump per increment.
    impacts: AtomicU32,
    impact_strength: AtomicU32,
    muted: AtomicBool,
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

pub struct Audio {
    shared: Arc<Shared>,
    _stream: Option<cpal::Stream>,
}

impl Audio {
    /// None when there is no output device; the game runs silent.
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let mut config = supported.config();
        // A slightly larger buffer (about 20 ms) so a busy frame never starves the audio thread.
        if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
            config.buffer_size = cpal::BufferSize::Fixed(1024u32.clamp(*min, *max));
        }
        let channels = config.channels as usize;
        let rate = config.sample_rate as f32;
        let shared = Arc::new(Shared::default());
        let started = std::time::Instant::now();
        let mut synth = Synth::new(rate, shared.clone());
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |out: &mut [f32], _| synth.fill(out, channels),
                move |e| eprintln!("audio ({:.1} s after start): {e}", started.elapsed().as_secs_f32()),
                None,
            ),
            _ => return None,
        }
        .ok()?;
        stream.play().ok()?;
        Some(Self { shared, _stream: Some(stream) })
    }

    pub fn update(&self, f: &SoundFrame) {
        let s = &self.shared;
        store(&s.rpm, f.rpm);
        s.gear.store(f.gear, Ordering::Relaxed);
        store(&s.load, f.load);
        store(&s.speed, f.speed);
        store(&s.squeal, f.squeal);
        store(&s.gravel, f.gravel);
        store(&s.scrub, f.scrub);
        s.airborne.store(f.airborne, Ordering::Relaxed);
    }

    pub fn impact(&self, strength: f32) {
        store(&self.shared.impact_strength, strength.clamp(0.0, 1.0));
        self.shared.impacts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn toggle_mute(&self) -> bool {
        let m = !self.shared.muted.load(Ordering::Relaxed);
        self.shared.muted.store(m, Ordering::Relaxed);
        m
    }
}

/// Renders the synth offline: one SoundFrame per 10 ms tick, impacts as (tick, strength).
pub fn render_offline(frames: &[SoundFrame], impacts: &[(usize, f32)], rate: u32) -> Vec<f32> {
    let shared = Arc::new(Shared::default());
    let mut synth = Synth::new(rate as f32, shared.clone());
    let per_tick = (rate / 100) as usize;
    let mut out = vec![0.0f32; frames.len() * per_tick];
    for (i, f) in frames.iter().enumerate() {
        let a = Audio { shared: shared.clone(), _stream: None };
        a.update(f);
        for &(t, strength) in impacts {
            if t == i {
                a.impact(strength);
            }
        }
        synth.fill(&mut out[i * per_tick..(i + 1) * per_tick], 1);
    }
    out
}

/// One-pole low-pass.
#[derive(Default)]
struct LowPass {
    y: f32,
}

impl LowPass {
    fn run(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / rate).exp();
        self.y += a * (x - self.y);
        self.y
    }
}

/// State-variable band-pass.
#[derive(Default)]
struct BandPass {
    low: f32,
    band: f32,
}

impl BandPass {
    fn run(&mut self, x: f32, freq: f32, q: f32, rate: f32) -> f32 {
        let f = 2.0 * (std::f32::consts::PI * freq / rate).sin();
        let high = x - self.low - self.band / q;
        self.band += f * high;
        self.low += f * self.band;
        self.band
    }
}

const AMBIENCE_WAV: &[u8] = include_bytes!("../assets/audio/ambience_mars.wav");

struct Synth {
    rate: f32,
    shared: Arc<Shared>,
    seed: u32,
    // smoothed controls
    load: f32,
    speed: f32,
    squeal: f32,
    gravel: f32,
    scrub: f32,
    air: f32,
    master: f32,
    engine: crate::engine_sound::EngineVoice,
    seen_gear: u32,
    ambience: crate::sample::Looper,
    // oscillators and filters
    wobble: f32,
    wind_lp: LowPass,
    squeal_bp: BandPass,
    grain_lo: BandPass,
    grain_hi: BandPass,
    rumble_lp: LowPass,
    scrub_bp: BandPass,
    seen_impacts: u32,
    thump: f32,
    thump_phase: f32,
    // high-pass state (removes infrasound and DC)
    hp_x: f32,
    hp_y: f32,
}

impl Synth {
    fn new(rate: f32, shared: Arc<Shared>) -> Self {
        Self {
            rate,
            shared,
            seed: 0x1234_5678,
            load: 0.0,
            speed: 0.0,
            squeal: 0.0,
            gravel: 0.0,
            scrub: 0.0,
            air: 0.0,
            master: 0.0,
            engine: crate::engine_sound::EngineVoice::new(rate),
            seen_gear: 0,
            ambience: crate::sample::Looper::new(AMBIENCE_WAV, rate),
            wobble: 0.0,
            wind_lp: LowPass::default(),
            squeal_bp: BandPass::default(),
            grain_lo: BandPass::default(),
            grain_hi: BandPass::default(),
            rumble_lp: LowPass::default(),
            scrub_bp: BandPass::default(),
            seen_impacts: 0,
            thump: 0.0,
            thump_phase: 0.0,
            hp_x: 0.0,
            hp_y: 0.0,
        }
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn fill(&mut self, out: &mut [f32], channels: usize) {
        let s = self.shared.clone();
        let target = (
            load(&s.rpm),
            load(&s.load),
            load(&s.speed),
            load(&s.squeal),
            load(&s.gravel),
            load(&s.scrub),
            if s.airborne.load(Ordering::Relaxed) { 1.0 } else { 0.0 },
            if s.muted.load(Ordering::Relaxed) { 0.0 } else { 0.7 },
        );
        let gear = s.gear.load(Ordering::Relaxed);
        let revs = crate::engine_sound::revs(target.0, gear, target.1);
        if gear > self.seen_gear && target.1 > 0.3 {
            self.engine.shift();
        }
        self.seen_gear = gear;
        let impacts = s.impacts.load(Ordering::Relaxed);
        if impacts != self.seen_impacts {
            self.seen_impacts = impacts;
            self.thump = self.thump.max(load(&s.impact_strength));
            self.thump_phase = 0.0;
        }
        let rate = self.rate;
        let k = 1.0 - (-1.0 / (0.03 * rate)).exp();
        for frame in out.chunks_mut(channels) {
            self.load += (target.1 - self.load) * k;
            self.speed += (target.2 - self.speed) * k;
            self.squeal += (target.3 - self.squeal) * k;
            self.gravel += (target.4 - self.gravel) * k;
            self.scrub += (target.5 - self.scrub) * k;
            self.air += (target.6 - self.air) * k;
            self.master += (target.7 - self.master) * k * 0.2;

            let engine = self.engine.next(revs, self.speed, self.load) * 0.45;

            // Gravel under the tyres on dirt (a stand-in for a recording): random grains of grit
            // ringing at two pitches over a low rumble, denser with speed; sliding adds a hiss.
            let ground = self.gravel * (1.0 - self.air) * (self.speed / 25.0).clamp(0.0, 1.0);
            let mut gravel = 0.0;
            if ground > 0.001 {
                let density = 300.0 + 2700.0 * (self.speed / 60.0).min(1.0) * (1.0 + self.scrub);
                let grain = if self.noise() * 0.5 + 0.5 < density / rate {
                    let a = self.noise();
                    a * a.abs()
                } else {
                    0.0
                };
                let lo = self.grain_lo.run(grain, 1800.0, 2.0, rate);
                let hi = self.grain_hi.run(grain, 4200.0, 2.5, rate);
                let n = self.noise();
                let rumble = self.rumble_lp.run(n, 250.0, rate);
                let n = self.noise();
                let hiss = self.scrub_bp.run(n, 3000.0, 0.8, rate) * self.scrub;
                gravel = (lo + 0.35 * hi + 0.5 * rumble + 0.12 * hiss) * ground * 0.4;
            }

            // Wind grows with speed squared.
            let v = (self.speed / 90.0).clamp(0.0, 1.5);
            let n = self.noise();
            // Kept low and dark: a constant broadband whoosh gets tiring ("aspirateur").
            let wind = self.wind_lp.run(n, 150.0 + 350.0 * v, rate) * 0.05 * v * v;

            // Tyre squeal on road, with a slow wobble.
            self.wobble = (self.wobble + 6.0 / rate).fract();
            let squeal_freq = 950.0 + 120.0 * (self.wobble * std::f32::consts::TAU).sin();
            let n3 = self.noise();
            let squeal = self.squeal_bp.run(n3, squeal_freq, 14.0, rate) * self.squeal * (1.0 - self.air) * 0.5;

            // Impact thump.
            let mut thump = 0.0;
            if self.thump > 0.001 {
                self.thump_phase += 1.0 / rate;
                thump = (self.thump_phase * 70.0 * std::f32::consts::TAU).sin() * self.thump * 0.6
                    + self.noise() * self.thump * 0.15;
                self.thump *= (-1.0 / (0.08 * rate)).exp();
            }

            let raw = (engine + gravel + wind + squeal + thump) * self.master;
            // ~30 Hz high-pass: nothing below what speakers can play.
            let a = (-std::f32::consts::TAU * 30.0 / rate).exp();
            let mix = a * (self.hp_y + raw - self.hp_x);
            self.hp_x = raw;
            self.hp_y = mix;
            // The ambience is the only stereo sound.
            let (al, ar) = self.ambience.next(1.0);
            let (l, r) = ((mix + al * 0.35 * self.master).tanh(), (mix + ar * 0.35 * self.master).tanh());
            match frame {
                [mono] => *mono = 0.5 * (l + r),
                [left, right, rest @ ..] => {
                    *left = l;
                    *right = r;
                    for c in rest {
                        *c = 0.5 * (l + r);
                    }
                }
                [] => {}
            }
        }
    }
}
