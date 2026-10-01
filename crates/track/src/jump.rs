//! Jumps: the flight off a ramp lip and the landing descent shaped to catch it.
//!
//! Work in the frame of the lip: `x` is the horizontal distance past the lip, heights are
//! relative to the lip, and the lip grade is `t = tan α`. A car leaving the lip at speed `v`
//! under gravity `g` flies
//!
//! ```text
//! y(x) = t·x − k·x²,   k = g / (2 v² cos² α)
//! ```
//!
//! so every (speed, gravity) pair is a single number `k`: slow cars and heavy gravity have a
//! large `k` and come down early, fast cars and light gravity a small `k` and fly far.
//!
//! The landing surface is `y_s(x) = (t − ε)·x − C·x²` from the end of the gap to the knee.
//! The trajectory `k` meets it at `x* = ε / (k − C)`, and there its slope is exactly `ε`
//! steeper than the surface's, whatever `k`: every car that lands on that part touches down at
//! the same small angle (ε = 0.075 is 4.3°). That part catches `k ∈ [C + ε/knee, C + ε/gap]`.
//! Past the knee an outrun (a sag) rounds the descent into the flat level below; cars that fly
//! beyond the knee land on it at a growing angle.

/// Shape of a landing descent, relative to the lip of the ramp that feeds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandingProfile {
    /// Grade of the ramp at the lip (tan of the launch angle).
    pub lip_grade: f32,
    /// Empty distance between the lip and the top of the landing.
    pub gap: f32,
    /// Slope mismatch every catch on the parabolic part lands with.
    pub epsilon: f32,
    /// Curvature of the parabolic part.
    pub c: f32,
    /// End of the parabolic part, start of the outrun.
    pub knee: f32,
    /// Horizontal length from the lip to the end of the outrun (flat from there on).
    pub length: f32,
    /// Height of the lip above the flat level the outrun ends on.
    pub drop: f32,
}

impl LandingProfile {
    /// Solves the curvature so that the outrun ends flat exactly `drop` below the lip.
    pub fn new(lip_grade: f32, gap: f32, epsilon: f32, outrun: f32, length: f32, drop: f32) -> Self {
        let knee = length - outrun;
        let c = ((lip_grade - epsilon) * (knee + 0.5 * outrun) + drop) / (knee * (knee + outrun));
        Self { lip_grade, gap, epsilon, c, knee, length, drop }
    }

    /// Height of the landing surface relative to the lip (meaningful from `gap` on; flat past
    /// `length`).
    pub fn height(&self, x: f32) -> f32 {
        let t = self.lip_grade - self.epsilon;
        if x <= self.knee {
            t * x - self.c * x * x
        } else if x <= self.length {
            let a = self.length - self.knee;
            let gk = t - 2.0 * self.c * self.knee;
            let d = x - self.knee;
            t * self.knee - self.c * self.knee * self.knee + gk * d - gk * d * d / (2.0 * a)
        } else {
            -self.drop
        }
    }

    /// Grade (dy/dx) of the landing surface.
    pub fn grade(&self, x: f32) -> f32 {
        let t = self.lip_grade - self.epsilon;
        if x <= self.knee {
            t - 2.0 * self.c * x
        } else if x <= self.length {
            let a = self.length - self.knee;
            let gk = t - 2.0 * self.c * self.knee;
            gk * (1.0 - (x - self.knee) / a)
        } else {
            0.0
        }
    }

    /// Range of the ballistic parameter `k = g / (2 v² cos² α)` that lands on the parabolic part
    /// at exactly the design mismatch: `(k_min, k_max)`.
    pub fn k_range(&self) -> (f32, f32) {
        (self.c + self.epsilon / self.knee, self.c + self.epsilon / self.gap)
    }

    /// Speed (m/s) that gives a ballistic parameter `k` under gravity `g`.
    pub fn speed_for_k(&self, k: f32, g: f32) -> f32 {
        let cos2 = 1.0 / (1.0 + self.lip_grade * self.lip_grade);
        libm::sqrtf(g / (2.0 * k * cos2))
    }
}

/// Where and how a car leaving the lip comes down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touchdown {
    /// Horizontal distance from the lip.
    pub x: f32,
    /// Height relative to the lip.
    pub y: f32,
    /// How much steeper the car comes down than the surface, degrees.
    pub mismatch_deg: f32,
    /// Velocity component into the surface at touchdown, m/s.
    pub normal_speed: f32,
    /// Seconds in the air.
    pub air_time: f32,
}

/// Point-mass flight from the lip at `speed` (m/s, along the lip) under `gravity` (m/s²).
/// `None` when the car comes down before the landing starts (it falls into the gap).
pub fn fly(profile: &LandingProfile, speed: f32, gravity: f32) -> Option<Touchdown> {
    let t = profile.lip_grade;
    let cos_a = 1.0 / libm::sqrtf(1.0 + t * t);
    let vx = speed * cos_a;
    let vy0 = speed * t * cos_a;
    let traj = |x: f32| t * x - gravity * x * x / (2.0 * vx * vx);
    let above = |x: f32| traj(x) - profile.height(x);
    if above(profile.gap) < 0.0 {
        return None;
    }
    let step = 0.25;
    let mut x0 = profile.gap;
    loop {
        let x1 = x0 + step;
        if above(x1) <= 0.0 {
            let (mut lo, mut hi) = (x0, x1);
            for _ in 0..40 {
                let m = 0.5 * (lo + hi);
                if above(m) > 0.0 {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            let x = hi;
            let tau = x / vx;
            let vy = vy0 - gravity * tau;
            let s = profile.grade(x);
            let mismatch = libm::atanf(s) - libm::atanf(vy / vx);
            let normal_speed = (vx * s - vy) / libm::sqrtf(1.0 + s * s);
            return Some(Touchdown {
                x,
                y: profile.height(x),
                mismatch_deg: mismatch.to_degrees(),
                normal_speed,
                air_time: tau,
            });
        }
        x0 = x1;
        if x0 > profile.length + 2000.0 {
            return None;
        }
    }
}

/// Speeds at the lip (km/h, whole numbers) that clear the gap and touch down at no more than
/// `max_mismatch_deg`, under `gravity`: `(slowest, fastest)`.
pub fn envelope_kmh(profile: &LandingProfile, gravity: f32, max_mismatch_deg: f32) -> Option<(f32, f32)> {
    let mut range: Option<(f32, f32)> = None;
    for kmh in 20..=600 {
        let v = kmh as f32 / 3.6;
        let clean = fly(profile, v, gravity).is_some_and(|td| td.mismatch_deg <= max_mismatch_deg);
        match (clean, range) {
            (true, None) => range = Some((kmh as f32, kmh as f32)),
            (true, Some((lo, _))) => range = Some((lo, kmh as f32)),
            (false, Some(_)) => break,
            (false, None) => {}
        }
    }
    range
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> LandingProfile {
        LandingProfile::new(libm::tanf(5f32.to_radians()), 12.0, 0.075, 48.0, 192.0, 17.4)
    }

    #[test]
    fn outrun_ends_flat_at_the_drop() {
        let p = profile();
        assert!((p.height(p.length) + p.drop).abs() < 1e-3);
        assert!(p.grade(p.length).abs() < 1e-6);
        // Continuous at the knee.
        let e = 1e-3;
        assert!((p.height(p.knee - e) - p.height(p.knee + e)).abs() < 1e-3);
        assert!((p.grade(p.knee - e) - p.grade(p.knee + e)).abs() < 1e-3);
    }

    #[test]
    fn parabolic_part_lands_at_the_design_angle() {
        let p = profile();
        let (k_min, k_max) = p.k_range();
        for i in 1..10 {
            let k = k_min + (k_max - k_min) * i as f32 / 10.0;
            let v = p.speed_for_k(k, 9.81);
            let td = fly(&p, v, 9.81).expect("lands");
            let expected = (libm::atanf(p.grade(td.x)) - libm::atanf(p.grade(td.x) - p.epsilon)).to_degrees();
            assert!((td.mismatch_deg - expected).abs() < 0.05, "k {k}: {td:?}");
            assert!(td.x >= p.gap && td.x <= p.knee + 0.1);
        }
    }

    #[test]
    fn too_slow_falls_into_the_gap() {
        assert!(fly(&profile(), 10.0, 30.0).is_none());
    }
}
