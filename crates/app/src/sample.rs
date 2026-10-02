//! Recordings: a minimal reader for the 16-bit PCM WAV files the app embeds (assets/audio/, made
//! by tools/audio/prepare.py), a loop played at a variable rate, and a sound played once.

/// A file of assets/audio/ the app embeds, or `DIR/<file>` when `MARS_SOUNDS=DIR` is set and the
/// file is there: to try other takes in the game, or in `MARS_AUDIO_WAV`'s render, without
/// rebuilding.
pub fn asset(file: &str, embedded: &'static [u8]) -> std::borrow::Cow<'static, [u8]> {
    if let Some(dir) = std::env::var_os("MARS_SOUNDS") {
        if let Ok(bytes) = std::fs::read(std::path::Path::new(&dir).join(file)) {
            return bytes.into();
        }
    }
    embedded.into()
}

/// Reads a 16-bit PCM WAV: returns (interleaved samples, channels, rate).
fn decode_wav(bytes: &[u8]) -> (Vec<f32>, usize, u32) {
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
            channels = (u16le(body + 2) as usize).max(1);
            rate = u32le(body + 4);
        } else if id == b"data" {
            let end = (body + len).min(bytes.len());
            let samples = bytes[body..end].chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect();
            return (samples, channels, rate);
        }
        i = body + len + (len & 1);
    }
    (Vec::new(), channels, rate)
}

/// A looping recording read at a variable rate with linear interpolation.
pub struct Looper {
    /// Interleaved, `channels` per frame.
    samples: Vec<f32>,
    channels: usize,
    frames: usize,
    pos: f64,
    /// Recording frames per output sample at the natural speed.
    step: f64,
}

impl Looper {
    pub fn new(bytes: &[u8], out_rate: f32) -> Self {
        let (samples, channels, rate) = decode_wav(bytes);
        let frames = samples.len() / channels;
        Self { samples, channels, frames, pos: 0.0, step: rate as f64 / out_rate as f64 }
    }

    /// The next (left, right) sample, `rate` times the natural speed (2 is an octave up); a mono
    /// recording gives the same value on both sides.
    pub fn next(&mut self, rate: f32) -> (f32, f32) {
        let n = self.frames;
        if n < 2 {
            return (0.0, 0.0);
        }
        self.pos += self.step * rate as f64;
        while self.pos >= n as f64 {
            self.pos -= n as f64;
        }
        let i = self.pos as usize;
        let frac = (self.pos - i as f64) as f32;
        let (a, b) = (i * self.channels, (i + 1) % n * self.channels);
        let at = |c: usize| self.samples[a + c] * (1.0 - frac) + self.samples[b + c] * frac;
        let right = if self.channels > 1 { 1 } else { 0 };
        (at(0), at(right))
    }
}

/// A recording played once from its start each time it is triggered (a new trigger restarts it).
pub struct OneShot {
    /// Interleaved, `channels` per frame.
    samples: Vec<f32>,
    channels: usize,
    frames: usize,
    /// Where it is playing, in recording frames; `None` once it has ended.
    pos: Option<f64>,
    step: f64,
}

impl OneShot {
    pub fn new(bytes: &[u8], out_rate: f32) -> Self {
        let (samples, channels, rate) = decode_wav(bytes);
        let frames = samples.len() / channels;
        Self { samples, channels, frames, pos: None, step: rate as f64 / out_rate as f64 }
    }

    pub fn trigger(&mut self) {
        self.pos = Some(0.0);
    }

    /// The next (left, right) sample, silence once the recording has ended.
    pub fn next(&mut self) -> (f32, f32) {
        let Some(pos) = self.pos else { return (0.0, 0.0) };
        let i = pos as usize;
        if i + 1 >= self.frames {
            self.pos = None;
            return (0.0, 0.0);
        }
        let frac = (pos - i as f64) as f32;
        let (a, b) = (i * self.channels, (i + 1) * self.channels);
        let at = |c: usize| self.samples[a + c] * (1.0 - frac) + self.samples[b + c] * frac;
        let right = if self.channels > 1 { 1 } else { 0 };
        self.pos = Some(pos + self.step);
        (at(0), at(right))
    }
}
