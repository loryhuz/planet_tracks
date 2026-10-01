//! Engine voice built like a real engine: each cylinder firing is a short, slightly irregular
//! pressure pulse, and the pulses ring through an exhaust (two pipe resonances and a muffler)
//! instead of being summed sine harmonics.

/// Character of an engine.
#[derive(Clone, Copy, Debug)]
pub struct EngineKind {
    pub name: &'static str,
    pub cylinders: u32,
    pub idle_rpm: f32,
    pub redline_rpm: f32,
    /// Exhaust pipe lengths (metres): their resonances colour the sound.
    pub pipes: [f32; 2],
    /// Muffler formants (Hz).
    pub formants: [f32; 2],
    /// 0 smooth .. 1 rough (firing-to-firing variation).
    pub roughness: f32,
    /// Brightness of the pulses (Hz of the burst noise).
    pub brightness: f32,
}

pub const KINDS: [EngineKind; 3] = [
    // Air-cooled flat four, the classic dune-buggy burble.
    EngineKind {
        name: "flat4",
        cylinders: 4,
        idle_rpm: 900.0,
        redline_rpm: 6800.0,
        pipes: [1.1, 0.45],
        formants: [160.0, 430.0],
        roughness: 0.35,
        brightness: 1800.0,
    },
    // Big single-cylinder off-road thumper.
    EngineKind {
        name: "single",
        cylinders: 1,
        idle_rpm: 1300.0,
        redline_rpm: 9500.0,
        pipes: [0.9, 0.3],
        formants: [120.0, 650.0],
        roughness: 0.5,
        brightness: 2400.0,
    },
    // High-revving inline four, raspy race engine.
    EngineKind {
        name: "inline4",
        cylinders: 4,
        idle_rpm: 1100.0,
        redline_rpm: 9000.0,
        pipes: [0.7, 0.35],
        formants: [260.0, 900.0],
        roughness: 0.2,
        brightness: 3400.0,
    },
];

struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(2)], pos: 0 }
    }
    fn tap(&self) -> f32 {
        self.buf[self.pos]
    }
    fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
    }
}

#[derive(Default)]
struct OnePole {
    y: f32,
}

impl OnePole {
    fn lp(&mut self, x: f32, a: f32) -> f32 {
        self.y += a * (x - self.y);
        self.y
    }
}

#[derive(Default)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    fn band(&mut self, x: f32, f: f32, q: f32) -> f32 {
        let high = x - self.low - self.band / q;
        self.band += f * high;
        self.low += f * self.band;
        self.band
    }
}

pub struct EngineVoice {
    kind: EngineKind,
    rate: f32,
    seed: u32,
    fire_phase: f32,
    pulse: f32,
    pulse_amp: f32,
    pulse_bright: OnePole,
    pipes: [Delay; 2],
    pipe_lp: [OnePole; 2],
    formants: [Svf; 2],
    out_lp: OnePole,
    pop: f32,
}

impl EngineVoice {
    pub fn new(kind: EngineKind, rate: f32) -> Self {
        let speed_of_sound = 340.0;
        let delay = |len: f32| Delay::new((2.0 * len / speed_of_sound * rate) as usize);
        Self {
            kind,
            rate,
            seed: 0x51ed_270b,
            fire_phase: 0.0,
            pulse: 0.0,
            pulse_amp: 0.0,
            pulse_bright: OnePole::default(),
            pipes: [delay(kind.pipes[0]), delay(kind.pipes[1])],
            pipe_lp: [OnePole::default(), OnePole::default()],
            formants: [Svf::default(), Svf::default()],
            out_lp: OnePole::default(),
            pop: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32
    }

    /// `rpm01`: 0 idle .. 1 redline; `load`: 0 off-throttle .. 1 full throttle.
    pub fn next(&mut self, rpm01: f32, load: f32) -> f32 {
        let k = self.kind;
        let rpm = k.idle_rpm + (k.redline_rpm - k.idle_rpm) * rpm01.clamp(0.0, 1.1);
        // Four-stroke: each cylinder fires once every two crank turns.
        let firing_hz = rpm / 60.0 * k.cylinders as f32 / 2.0;
        self.fire_phase += firing_hz / self.rate;
        if self.fire_phase >= 1.0 {
            // Irregular firings: amplitude and a little timing jitter.
            self.fire_phase -= 1.0 - (self.rand() - 0.5) * 0.06 * k.roughness;
            let strength = 0.35 + 0.65 * load;
            self.pulse_amp = strength * (1.0 - k.roughness * 0.5 * self.rand());
            self.pulse = 1.0;
            // Off-throttle at high revs: occasional exhaust pops.
            if load < 0.1 && rpm01 > 0.5 && self.rand() < 0.04 {
                self.pop = 1.0;
            }
        }
        // The pulse: a fast attack then a decay of a few milliseconds of noisy pressure.
        let decay = (-1.0 / (0.0022 * self.rate)).exp();
        self.pulse *= decay;
        let noise = self.rand() * 2.0 - 1.0;
        let bright_a = 1.0 - (-std::f32::consts::TAU * k.brightness * (0.5 + 0.5 * load) / self.rate).exp();
        let mut excitation = self.pulse_bright.lp(noise * 0.6 + 0.4, bright_a) * self.pulse * self.pulse_amp;
        // Pops go through the exhaust too, so they crack like backfires instead of hissing.
        if self.pop > 0.01 {
            excitation += (self.rand() * 2.0 - 1.0) * self.pop * 1.5;
            self.pop *= (-1.0 / (0.006 * self.rate)).exp();
        }

        // Exhaust: two pipes with an inverting, damped reflection at the open end.
        let mut exhaust = 0.0;
        for i in 0..2 {
            let back = self.pipes[i].tap();
            let damped = self.pipe_lp[i].lp(back, 0.35);
            let y = excitation - 0.62 * damped;
            self.pipes[i].push(y);
            exhaust += y;
        }

        // Muffler formants plus a little of the raw exhaust, then a gentle top roll-off.
        let f0 = 2.0 * (std::f32::consts::PI * k.formants[0] / self.rate).sin();
        let f1 = 2.0 * (std::f32::consts::PI * k.formants[1] / self.rate).sin();
        let body = self.formants[0].band(exhaust, f0, 0.9) * 1.6 + self.formants[1].band(exhaust, f1, 1.4) * 0.8 + exhaust * 0.25;
        let out = self.out_lp.lp(body, 0.35);
        (out * 1.4).tanh()
    }
}
