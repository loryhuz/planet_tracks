//! Deterministic noise and random numbers for map generation.
//!
//! Integer hashing and f32 arithmetic with `libm` only, so a map generates the same mesh, bit for
//! bit, on every platform (the mesh feeds the physics).

/// Mixes a 32-bit value (lowbias32).
pub(crate) fn mix(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

pub(crate) fn hash2(seed: u32, x: i32, z: i32) -> u32 {
    mix(mix(mix(seed) ^ x as u32) ^ (z as u32).wrapping_mul(0x9e37_79b9))
}

/// Uniform in `[0, 1)` from a hash.
pub(crate) fn unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Gradient noise on the integer lattice, roughly in `[-1, 1]`, 0 on lattice points.
pub(crate) fn perlin(seed: u32, x: f32, z: f32) -> f32 {
    let (xf, zf) = (libm::floorf(x), libm::floorf(z));
    let (ix, iz) = (xf as i32, zf as i32);
    let (fx, fz) = (x - xf, z - zf);
    let grad = |i: i32, k: i32, dx: f32, dz: f32| -> f32 {
        const D: f32 = core::f32::consts::FRAC_1_SQRT_2;
        match hash2(seed, i, k) & 7 {
            0 => dx,
            1 => -dx,
            2 => dz,
            3 => -dz,
            4 => D * (dx + dz),
            5 => D * (dx - dz),
            6 => D * (-dx + dz),
            _ => D * (-dx - dz),
        }
    };
    let (u, v) = (fade(fx), fade(fz));
    let a = grad(ix, iz, fx, fz);
    let b = grad(ix + 1, iz, fx - 1.0, fz);
    let c = grad(ix, iz + 1, fx, fz - 1.0);
    let d = grad(ix + 1, iz + 1, fx - 1.0, fz - 1.0);
    1.6 * lerp(lerp(a, b, u), lerp(c, d, u), v)
}

/// Fractal sum of `octaves` gradient noises, each twice the frequency and half the amplitude of
/// the previous one, normalised to roughly `[-1, 1]`.
pub(crate) fn fbm(seed: u32, x: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for o in 0..octaves {
        // Rotate each octave a little so lattice artefacts do not line up.
        let (rx, rz) = (x * f, z * f);
        let (sx, sz) = if o % 2 == 0 { (rx, rz) } else { (0.8 * rx - 0.6 * rz, 0.6 * rx + 0.8 * rz) };
        sum += amp * perlin(seed.wrapping_add(o.wrapping_mul(1013)), sx + 17.3 * o as f32, sz - 9.1 * o as f32);
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

/// A small deterministic generator (SplitMix64).
pub(crate) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / 16_777_216.0)
    }

    /// Uniform in `[a, b)`.
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }
}
