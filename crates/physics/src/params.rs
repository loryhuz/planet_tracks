//! Car parameters: everything that defines how a profile drives, the tuning-panel view of it, and
//! the gameplay presets.
//!
//! Grip is expressed in g (1 g = 9.81 m/s²) of force per unit of static load, independent of the
//! profile's gravity: changing the gravity changes jumps and suspension sag, not the cornering
//! power. The wheel load modulates grip only through `load_sensitivity`, and smoothed over time.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use track::Surface;

/// One tunable parameter, exposed to the in-game tuning panel.
pub struct Tunable<'a> {
    pub group: &'static str,
    pub name: &'static str,
    pub value: &'a mut f32,
    pub min: f32,
    pub max: f32,
}

/// How the car behaves on one kind of surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SurfaceGrip {
    /// Lateral grip, in g: the tightest path the car can follow at a given speed.
    pub grip: f32,
    /// Traction and braking grip, in g.
    pub long_grip: f32,
    /// Largest drift angle on this surface (reached when the steering asks for `drift_excess_full`
    /// more than the grip), degrees. 0 = the body never swings out (the car just runs wider).
    pub drift_angle_deg: f32,
    /// Rolling resistance (coasting deceleration), m/s².
    pub rolling: f32,
    /// Extra speed-proportional drag (1/s): what makes off-track terrain slow.
    pub drag: f32,
    /// Engine top speed multiplier on this surface (also caps the reverse speed).
    pub top_speed: f32,
    /// Engine force multiplier on this surface.
    pub traction: f32,
    /// Turn-rate multiplier at full lock (1 everywhere by default: a different value changes the
    /// line when a corner crosses from one surface to another).
    pub yaw: f32,
}

impl Default for SurfaceGrip {
    fn default() -> Self {
        fidele().road
    }
}

/// Everything that defines how a car drives. Serialized to JSON by the tuning panel.
///
/// Every field has a default (the "Fidèle" preset), so profiles saved by an older version still
/// load (unknown fields are ignored).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CarParams {
    pub name: String,
    pub description: String,

    // --- World
    /// m/s², positive.
    pub gravity: f32,
    /// Gravity multiplier while no wheel touches the ground.
    pub air_gravity: f32,
    /// Share of the gravity felt along the ground on slopes, while on the wheels.
    pub slope_gravity: f32,

    // --- Chassis
    /// kg. Forces are designed per kg, so the mass mainly weighs against collisions.
    pub mass: f32,
    pub inertia_pitch: f32,
    pub inertia_yaw: f32,
    pub inertia_roll: f32,
    /// Height of the centre of gravity above the suspension anchors, m.
    pub cg_height: f32,
    pub wheel_radius: f32,
    /// Distance between front and rear axles.
    pub wheelbase: f32,
    /// Distance between left and right wheels.
    pub track_width: f32,
    /// Share of the weight on the rear axle.
    pub weight_rear: f32,

    // --- Suspension
    /// Suspension travel below the anchor points.
    pub suspension_travel: f32,
    /// Natural frequency of the springs, Hz.
    pub spring_hz: f32,
    /// Damping ratio (1 = critical).
    pub damping: f32,
    /// Bump-stop stiffness, as a multiple of the spring rate.
    pub bump_stop: f32,
    /// Length of the bump-stop zone at the top of the travel, as a fraction of the travel.
    pub bump_zone: f32,
    /// Anti-roll bar stiffness, as a multiple of the spring rate.
    pub anti_roll: f32,
    /// Lateral load transfer: 0 applies the cornering forces at the CG height (no roll), 1 at the
    /// ground.
    pub roll_transfer: f32,
    /// Longitudinal load transfer: 0 = no dive or squat, 1 = full.
    pub pitch_transfer: f32,
    /// 0: grip ignores the wheel loads; 1: grip follows them (always smoothed over time).
    pub load_sensitivity: f32,
    /// Share of the tipping moment the tyres may apply (below 1, cornering and braking can never
    /// roll or flip the car).
    pub anti_rollover: f32,

    // --- Engine
    /// Push (m/s²) in each speed band, from standstill up.
    pub accel_steps: [f32; 6],
    /// Upper end of the first five bands, km/h (the last one ends at the top speed).
    pub accel_speeds: [f32; 5],
    /// No more push above this speed, km/h.
    pub top_speed_kmh: f32,
    /// 0 = clean steps between the bands, 1 = linear blend from one band to the next.
    pub accel_smooth: f32,
    pub reverse_accel: f32,
    pub reverse_speed_kmh: f32,
    /// Below this forward speed the brake also pushes backward (km/h).
    pub reverse_engage_kmh: f32,

    // --- Brakes
    /// Braking deceleration, m/s².
    pub brake: f32,

    // --- Steering
    /// Maximum wheel angle at standstill, degrees.
    pub steer_low_deg: f32,
    /// Maximum wheel angle at very high speed, degrees.
    pub steer_high_deg: f32,
    /// Speed at which the maximum angle is halfway between the two, km/h.
    pub steer_speed_kmh: f32,
    /// Steering speed toward the input, full lock per second.
    pub steer_rate: f32,
    /// Steering speed back toward the centre, full lock per second.
    pub steer_return: f32,
    /// Maximum yaw rate at full lock, rad/s (limits the wheel angle at speed).
    pub yaw_cap: f32,
    /// Increase of that maximum per 100 km/h, rad/s.
    pub yaw_cap_gain: f32,
    /// How fast the path's turn rate follows the steering, 1/s.
    pub yaw_response: f32,
    /// How fast the turn rate dies when the steering is released, 1/s.
    pub yaw_release: f32,

    // --- Grip
    pub road: SurfaceGrip,
    pub dirt: SurfaceGrip,
    pub ground: SurfaceGrip,
    /// Multiplies every grip coefficient.
    pub grip_scale: f32,
    /// How much traction or braking eats into the lateral grip (0 none, 1 friction circle).
    pub combined_grip: f32,
    /// Speed lost while turning: deceleration (m/s²) per (rad/s)² of turn rate.
    pub turn_drag: f32,
    /// How long grip and steering authority survive a brief loss of contact (bumps, crests), s.
    pub grip_memory: f32,

    // --- Drift (the body turning further than its path: the rear stepping out)
    //
    // Grip usage u = lateral acceleration the steering asks for / grip. Below `mark_start` the
    // tyres just grip; from there to 1 they still grip and stay aligned but leave marks; above 1
    // the path bends at the grip limit and the excess becomes a drift angle.
    /// Grip usage from which the tyres leave marks.
    pub mark_start: f32,
    /// Excess of grip usage (above 1) that gives the surface's full drift angle.
    pub drift_excess_full: f32,
    /// Time for the drift angle to build toward what the steering asks, s (63 %).
    pub drift_build_s: f32,
    /// Time for the drift angle to come back when the steering eases off, s (63 %).
    pub drift_release_s: f32,
    /// Speed lost while drifting, m/s² at a 30° drift angle (proportional to the angle).
    pub drift_bleed: f32,
    /// How much tighter a drift lets the car turn: 0 = no tighter than the grip, 1 = as tight as
    /// the steering asks (reached as the drift angle develops).
    pub drift_turn: f32,
    /// Speed lost for turning beyond the grip in a drift: m/s² of deceleration per m/s² of
    /// lateral acceleration beyond the grip.
    pub drift_turn_cost: f32,
    /// Fastest the body swings out (and settles back once the steering is released) relative to
    /// its path, rad/s.
    pub drift_swing_rate: f32,
    /// While the steering still asks for the turn, the share of the path's turn rate the body may
    /// give up to close the angle (below 1, the body always keeps turning inward).
    pub drift_hold_turn: f32,
    /// A mismatch between the body and its motion beyond this angle (walls, landings, grip losses)
    /// is turned into drift angle, so the car never slides sideways, degrees.
    pub drift_catch_deg: f32,

    // --- Assists
    /// Torque keeping the body parallel to the ground on its wheels, 1/s².
    pub ground_level: f32,

    // --- Aero
    /// Air drag, m/s² of deceleration at 360 km/h (scaled with speed²).
    pub air_drag: f32,
    /// Downforce in g at 200 km/h, on the ground only (scaled with speed²).
    pub downforce: f32,

    // --- Air
    /// Control in the air, rad/s²: the steering yaws, the brake pulls the nose up.
    pub air_control: f32,
    /// Torque bringing the car level in the air, 1/s².
    pub air_level: f32,
    /// Linear angular damping in the air, 1/s.
    pub air_damping: f32,
    /// Quadratic angular damping in the air, 1/rad.
    pub air_damping_quad: f32,

    // --- Walls and body
    /// Share of the speed along the wall lost per unit of impact sine.
    pub wall_speed_loss: f32,
    /// Share of the horizontal speed lost per tick of wall contact.
    pub wall_drag: f32,
    /// Restitution of wall impacts.
    pub wall_bounce: f32,
    /// How much a wall impact rotates the car (0 = never, 1 = physical).
    pub wall_rotation: f32,
    /// Friction of the body against the ground.
    pub body_friction: f32,
    /// Restitution of the body against the ground.
    pub body_bounce: f32,
}

impl Default for CarParams {
    fn default() -> Self {
        fidele()
    }
}

fn surface_tunables<'a>(group: &'static str, s: &'a mut SurfaceGrip, out: &mut Vec<Tunable<'a>>) {
    let t = |name, value, min, max| Tunable { group, name, value, min, max };
    out.push(t("Adhérence latérale (g)", &mut s.grip, 0.5, 40.0));
    out.push(t("Motricité et freinage (g)", &mut s.long_grip, 0.2, 20.0));
    out.push(t("Angle de dérive max (°)", &mut s.drift_angle_deg, 0.0, 60.0));
    out.push(t("Roue libre (m/s²)", &mut s.rolling, 0.0, 10.0));
    out.push(t("Freinage du sol (1/s)", &mut s.drag, 0.0, 2.0));
    out.push(t("Vitesse max (×)", &mut s.top_speed, 0.2, 1.5));
    out.push(t("Force moteur (×)", &mut s.traction, 0.1, 2.0));
    out.push(t("Vitesse de virage (×)", &mut s.yaw, 0.2, 1.5));
}

const STEP_NAMES: [&str; 6] = [
    "Poussée palier 1 (m/s²)",
    "Poussée palier 2 (m/s²)",
    "Poussée palier 3 (m/s²)",
    "Poussée palier 4 (m/s²)",
    "Poussée palier 5 (m/s²)",
    "Poussée palier 6 (m/s²)",
];
const SPEED_NAMES: [&str; 5] = [
    "Fin du palier 1 (km/h)",
    "Fin du palier 2 (km/h)",
    "Fin du palier 3 (km/h)",
    "Fin du palier 4 (km/h)",
    "Fin du palier 5 (km/h)",
];

impl CarParams {
    pub fn tunables(&mut self) -> Vec<Tunable<'_>> {
        let mut v = Vec::with_capacity(96);
        macro_rules! t {
            ($g:expr, $n:expr, $f:expr, $min:expr, $max:expr) => {
                v.push(Tunable { group: $g, name: $n, value: &mut $f, min: $min, max: $max })
            };
        }
        t!("Monde", "Gravité (m/s²)", self.gravity, 1.0, 60.0);
        t!("Monde", "Gravité en l'air (×)", self.air_gravity, 0.2, 3.0);
        t!("Monde", "Gravité dans les pentes (×)", self.slope_gravity, 0.0, 1.5);

        t!("Moteur", "Vitesse max (km/h)", self.top_speed_kmh, 60.0, 600.0);
        for (value, name) in self.accel_steps.iter_mut().zip(STEP_NAMES) {
            v.push(Tunable { group: "Moteur", name, value, min: 0.0, max: 60.0 });
        }
        for (value, name) in self.accel_speeds.iter_mut().zip(SPEED_NAMES) {
            v.push(Tunable { group: "Moteur", name, value, min: 5.0, max: 500.0 });
        }
        t!("Moteur", "Lissage entre paliers", self.accel_smooth, 0.0, 1.0);
        t!("Moteur", "Marche arrière (m/s²)", self.reverse_accel, 1.0, 60.0);
        t!("Moteur", "Marche arrière max (km/h)", self.reverse_speed_kmh, 10.0, 200.0);
        t!("Moteur", "Le frein recule sous (km/h)", self.reverse_engage_kmh, 0.0, 80.0);

        t!("Freins", "Décélération (m/s²)", self.brake, 2.0, 120.0);

        t!("Direction", "Braquage à l'arrêt (°)", self.steer_low_deg, 3.0, 50.0);
        t!("Direction", "Braquage à haute vitesse (°)", self.steer_high_deg, 0.5, 50.0);
        t!("Direction", "Vitesse de mi-braquage (km/h)", self.steer_speed_kmh, 20.0, 400.0);
        t!("Direction", "Vitesse de braquage (/s)", self.steer_rate, 0.5, 40.0);
        t!("Direction", "Vitesse de retour (/s)", self.steer_return, 0.5, 40.0);
        t!("Direction", "Virage max (rad/s)", self.yaw_cap, 0.5, 10.0);
        t!("Direction", "Virage max en plus par 100 km/h (rad/s)", self.yaw_cap_gain, -2.0, 3.0);
        t!("Direction", "Réactivité du virage (/s)", self.yaw_response, 0.5, 40.0);
        t!("Direction", "Retour au relâché (/s)", self.yaw_release, 0.5, 40.0);

        t!("Adhérence", "Adhérence globale (×)", self.grip_scale, 0.2, 3.0);
        t!("Adhérence", "Couplage motricité / virage", self.combined_grip, 0.0, 1.0);
        t!("Adhérence", "Freinage en virage (m/s² par (rad/s)²)", self.turn_drag, 0.0, 5.0);
        t!("Adhérence", "Mémoire de l'adhérence sur les bosses (s)", self.grip_memory, 0.01, 1.0);
        surface_tunables("Route", &mut self.road, &mut v);
        surface_tunables("Terre", &mut self.dirt, &mut v);
        surface_tunables("Hors-piste", &mut self.ground, &mut v);

        t!("Dérive", "Traces de pneus dès (× adhérence)", self.mark_start, 0.3, 1.0);
        t!("Dérive", "Excès d'adhérence pour l'angle max (×)", self.drift_excess_full, 0.05, 3.0);
        t!("Dérive", "Montée de l'angle (s)", self.drift_build_s, 0.02, 2.0);
        t!("Dérive", "Retour de l'angle (s)", self.drift_release_s, 0.02, 2.0);
        t!("Dérive", "Perte de vitesse à 30° (m/s²)", self.drift_bleed, 0.0, 60.0);
        t!("Dérive", "Virage serré en dérive (part du braquage)", self.drift_turn, 0.0, 1.0);
        t!("Dérive", "Coût du virage serré (m/s² par m/s² en plus)", self.drift_turn_cost, 0.0, 1.0);
        t!("Dérive", "Vitesse de pivot de la caisse (rad/s)", self.drift_swing_rate, 0.1, 6.0);
        t!("Dérive", "Fin de dérive : rotation cédée (× trajectoire)", self.drift_hold_turn, 0.0, 1.0);
        t!("Dérive", "Glissement toléré avant rattrapage (°)", self.drift_catch_deg, 0.5, 30.0);

        t!("Châssis", "Masse (kg)", self.mass, 200.0, 4000.0);
        t!("Châssis", "Inertie en tangage (×)", self.inertia_pitch, 0.2, 5.0);
        t!("Châssis", "Inertie en lacet (×)", self.inertia_yaw, 0.2, 5.0);
        t!("Châssis", "Inertie en roulis (×)", self.inertia_roll, 0.2, 5.0);
        t!("Châssis", "Centre de gravité au-dessus des ancrages (m)", self.cg_height, -0.3, 1.0);
        t!("Châssis", "Poids sur l'arrière (part)", self.weight_rear, 0.3, 0.7);
        t!("Châssis", "Rayon des roues (m)", self.wheel_radius, 0.2, 0.8);
        t!("Châssis", "Empattement (m)", self.wheelbase, 1.5, 4.0);
        t!("Châssis", "Voie (m)", self.track_width, 1.0, 2.6);

        t!("Suspension", "Débattement (m)", self.suspension_travel, 0.05, 1.0);
        t!("Suspension", "Fréquence des ressorts (Hz)", self.spring_hz, 0.3, 6.0);
        t!("Suspension", "Amortissement (× critique)", self.damping, 0.02, 2.0);
        t!("Suspension", "Butée (× ressort)", self.bump_stop, 1.0, 60.0);
        t!("Suspension", "Zone de butée (× débattement)", self.bump_zone, 0.0, 0.5);
        t!("Suspension", "Anti-roulis (× ressort)", self.anti_roll, 0.0, 3.0);
        t!("Suspension", "Transfert de charge en virage", self.roll_transfer, 0.0, 1.0);
        t!("Suspension", "Transfert de charge avant/arrière", self.pitch_transfer, 0.0, 1.0);
        t!("Suspension", "Adhérence selon la charge", self.load_sensitivity, 0.0, 1.0);
        t!("Suspension", "Anti-tonneau", self.anti_rollover, 0.0, 2.0);
        t!("Suspension", "Maintien à plat au sol (/s²)", self.ground_level, 0.0, 100.0);

        t!("Aérodynamique", "Traînée à 360 km/h (m/s²)", self.air_drag, 0.0, 30.0);
        t!("Aérodynamique", "Appui à 200 km/h (g)", self.downforce, 0.0, 5.0);

        t!("En l'air", "Contrôle en l'air (rad/s²)", self.air_control, 0.0, 20.0);
        t!("En l'air", "Mise à niveau automatique (/s²)", self.air_level, 0.0, 50.0);
        t!("En l'air", "Amortissement de rotation (/s)", self.air_damping, 0.0, 10.0);
        t!("En l'air", "Amortissement quadratique (/rad)", self.air_damping_quad, 0.0, 10.0);

        t!("Murs", "Perte de vitesse au choc", self.wall_speed_loss, 0.0, 1.5);
        t!("Murs", "Frottement par tick de contact", self.wall_drag, 0.0, 0.2);
        t!("Murs", "Rebond", self.wall_bounce, 0.0, 1.0);
        t!("Murs", "Rotation au choc", self.wall_rotation, 0.0, 1.0);
        t!("Murs", "Frottement de la caisse", self.body_friction, 0.0, 2.0);
        t!("Murs", "Rebond de la caisse", self.body_bounce, 0.0, 1.0);
        v
    }

    /// Suspension anchor points (top of travel) in the car frame, wheel order FL, FR, RL, RR.
    ///
    /// The car frame's origin is the centre of gravity: the axles sit around it according to
    /// `weight_rear`, and the anchors `cg_height` below it.
    pub fn wheel_anchors(&self) -> [Vec3; 4] {
        let x = self.track_width * 0.5;
        let front = self.wheelbase * self.weight_rear.clamp(0.1, 0.9);
        let rear = front - self.wheelbase;
        let y = -self.cg_height;
        [Vec3::new(x, y, front), Vec3::new(-x, y, front), Vec3::new(x, y, rear), Vec3::new(-x, y, rear)]
    }

    /// The spheres (car-frame centre, radius) that make up the body for collisions.
    pub fn body_spheres(&self) -> [(Vec3, f32); 9] {
        let x = self.track_width * 0.5 - 0.05;
        let front = self.wheelbase * self.weight_rear.clamp(0.1, 0.9);
        let rear = front - self.wheelbase;
        let y = -self.cg_height;
        let r = (self.wheel_radius * 1.05).clamp(0.3, 0.8);
        let low = y + 0.05;
        let mid = (front + rear) * 0.5;
        [
            (Vec3::new(x, low, front), r),
            (Vec3::new(-x, low, front), r),
            (Vec3::new(x, low, rear), r),
            (Vec3::new(-x, low, rear), r),
            (Vec3::new(0.0, low + 0.05, front + r * 0.9), r * 0.9),
            (Vec3::new(0.0, low + 0.05, rear - r * 0.9), r * 0.9),
            (Vec3::new(0.0, low + 0.1, mid), 0.55),
            (Vec3::new(0.0, y + 0.6, mid + 0.4), 0.5),
            (Vec3::new(0.0, y + 0.6, mid - 0.6), 0.5),
        ]
    }

    pub fn surface(&self, s: Surface) -> &SurfaceGrip {
        match s {
            Surface::Road | Surface::Wall => &self.road,
            Surface::Dirt => &self.dirt,
            Surface::Ground => &self.ground,
        }
    }

    /// Maximum steering angle (radians) at a forward speed (m/s): the speed curve, limited so the
    /// kinematic yaw rate stays under the yaw cap.
    pub fn max_steer(&self, speed: f32) -> f32 {
        let v = speed.abs();
        let x = v / (self.steer_speed_kmh / 3.6).max(1.0);
        let lo = self.steer_low_deg.to_radians();
        let hi = self.steer_high_deg.to_radians();
        let curve = hi + (lo - hi) / (1.0 + x * x);
        let cap = self.yaw_cap_at(v);
        if v > 0.5 {
            curve.min(libm::atanf(cap * self.wheelbase.max(0.5) / v))
        } else {
            curve
        }
    }

    /// Suspension length (anchor to wheel centre) with the car at rest on flat ground, m.
    pub fn rest_suspension(&self) -> f32 {
        let w = core::f32::consts::TAU * self.spring_hz.max(0.1);
        let travel = self.suspension_travel.max(0.01);
        let sag = self.gravity.max(0.0) / (w * w);
        (travel - sag).clamp(travel * 0.05, travel)
    }

    /// Yaw-rate cap at a speed (m/s).
    pub fn yaw_cap_at(&self, speed: f32) -> f32 {
        (self.yaw_cap + self.yaw_cap_gain * speed.abs() * 0.036).max(0.1)
    }

    /// Engine push (m/s², before rolling resistance and surface multipliers) at a forward speed (m/s).
    pub fn engine_push(&self, speed: f32) -> f32 {
        let kmh = speed.max(0.0) * 3.6;
        let top = self.top_speed_kmh.max(1.0);
        if kmh >= top {
            return 0.0;
        }
        let mut start = 0.0;
        for i in 0..6 {
            let end = if i < 5 { self.accel_speeds[i].min(top) } else { top };
            if kmh < end || i == 5 {
                let a = self.accel_steps[i];
                let next = if i < 5 { self.accel_steps[i + 1] } else { 0.0 };
                let t = ((kmh - start) / (end - start).max(1e-3)).clamp(0.0, 1.0);
                let smooth = self.accel_smooth.clamp(0.0, 1.0);
                return a + (next - a) * t * smooth;
            }
            start = end;
        }
        0.0
    }
}

/// The gameplay profiles offered in the game: only the player's pick, "Combo", for now. The other
/// profiles of the comparison (Fidèle, Grip arcade, Drift, Buggy lourd, Basse gravité, Équilibre)
/// stay below, out of the game; Combo is built from two of them.
pub fn presets() -> Vec<CarParams> {
    vec![combo()]
}

/// The player's pick after the profile comparison: "Fidèle" on the road (its steering, grip and
/// engine), "Grip arcade" on dirt and off-track (grippier, smaller and later drifts), with Grip
/// arcade's drift settings, which only matter where drifts happen, i.e. on dirt.
pub fn combo() -> CarParams {
    let road = fidele();
    let dirt = grip_arcade();
    let mut p = road.clone();
    p.name = "Combo".into();
    p.description = "La route de Fidèle et la terre de Grip arcade : le meilleur des deux.".into();
    p.dirt = dirt.dirt.clone();
    p.ground = dirt.ground.clone();
    p.drift_excess_full = dirt.drift_excess_full;
    p.drift_turn = dirt.drift_turn;
    p.mark_start = dirt.mark_start;
    p
}

/// Profile 1, the reference. Road: the measured TrackMania snow car on road (feel targets). Dirt:
/// grips, but the rear steps out into a visible drift angle while the car follows the steered path
/// and bleeds speed. Off-track ground: costs speed, never slides.
pub fn fidele() -> CarParams {
    CarParams {
        name: "Fidèle".into(),
        description: "Calé sur les mesures : route de la SnowCar, terre de la Rally (sur des rails, puis l'arrière chasse si on braque trop)."
            .into(),
        gravity: 40.0,
        air_gravity: 1.0,
        slope_gravity: 0.45,

        mass: 1000.0,
        inertia_pitch: 1.0,
        inertia_yaw: 1.0,
        inertia_roll: 1.0,
        cg_height: 0.05,
        wheel_radius: 0.45,
        wheelbase: 2.6,
        track_width: 1.8,
        weight_rear: 0.5,

        suspension_travel: 0.5,
        spring_hz: 2.0,
        damping: 0.3,
        bump_stop: 20.0,
        bump_zone: 0.1,
        anti_roll: 0.0,
        roll_transfer: 0.22,
        pitch_transfer: 0.0,
        load_sensitivity: 0.2,
        anti_rollover: 0.9,

        // The snow car's launch (measured: 30 m/s² up to 55 km/h), then more punch than it has at
        // speed (0→200 in ~7 s instead of 10), up to 300 km/h. Pushes include the 1 m/s² rolling
        // resistance.
        accel_steps: [31.0, 18.85, 12.0, 8.0, 4.5, 2.5],
        accel_speeds: [55.0, 80.0, 115.0, 160.0, 250.0],
        top_speed_kmh: 300.0,
        accel_smooth: 0.0,
        reverse_accel: 31.0,
        reverse_speed_kmh: 150.0,
        reverse_engage_kmh: 33.0,

        brake: 50.0,

        steer_low_deg: 15.6,
        steer_high_deg: 15.6,
        steer_speed_kmh: 200.0,
        steer_rate: 16.0,
        steer_return: 16.0,
        yaw_cap: 2.5,
        yaw_cap_gain: 0.07,
        yaw_response: 9.0,
        yaw_release: 9.5,

        // Road: the snow car's measured turns (17 g at 228 km/h, no slip) sit just below the limit,
        // where the tyres start marking; only the fastest full-lock turns drift, a little.
        road: SurfaceGrip {
            grip: 18.0,
            long_grip: 10.0,
            drift_angle_deg: 12.0,
            rolling: 1.0,
            drag: 0.0,
            top_speed: 1.0,
            traction: 1.0,
            yaw: 1.0,
        },
        // Dirt: on rails like TrackMania's Rally car well below the limit, marks near it, then a
        // drift angle that grows with how far the steering asks beyond the grip.
        dirt: SurfaceGrip {
            grip: 6.5,
            long_grip: 5.5,
            drift_angle_deg: 32.0,
            rolling: 1.0,
            drag: 0.0,
            top_speed: 0.92,
            traction: 0.75,
            yaw: 1.0,
        },
        // Off-track: costs speed (rolling, drag, traction, top speed) but never slides.
        ground: SurfaceGrip {
            grip: 14.0,
            long_grip: 3.0,
            drift_angle_deg: 0.0,
            rolling: 2.0,
            drag: 0.15,
            top_speed: 0.65,
            traction: 0.6,
            yaw: 1.0,
        },
        grip_scale: 1.0,
        combined_grip: 0.5,
        turn_drag: 1.0,
        grip_memory: 0.2,

        mark_start: 0.85,
        drift_excess_full: 0.5,
        drift_build_s: 0.3,
        drift_release_s: 0.18,
        drift_bleed: 3.5,
        drift_turn: 0.9,
        drift_turn_cost: 0.12,
        drift_swing_rate: 1.2,
        drift_hold_turn: 0.5,
        drift_catch_deg: 1.0,

        ground_level: 2.0,

        air_drag: 0.0,
        downforce: 0.0,

        air_control: 0.0,
        air_level: 0.0,
        air_damping: 0.96,
        air_damping_quad: 1.96,

        wall_speed_loss: 0.05,
        wall_drag: 0.012,
        wall_bounce: 0.0,
        wall_rotation: 0.3,
        body_friction: 0.1,
        body_bounce: 0.0,
    }
}

// Every other profile is "Fidèle" with one design axis changed, so they all keep its steering
// feel, its engine and its brakes.

/// Profile 2, axis: grip. Turns a bit sharper; the same drift, but later and smaller.
pub fn grip_arcade() -> CarParams {
    let mut p = fidele();
    p.name = "Grip arcade".into();
    p.description = "Comme Fidèle mais plus d'adhérence : tourne plus sec, ne dérive que tard et peu.".into();
    p.steer_low_deg = 17.0;
    p.steer_high_deg = 17.0;
    p.yaw_cap = 2.8;
    p.road.grip = 21.0;
    p.dirt.grip = 8.0;
    p.dirt.long_grip = 6.0;
    p.dirt.traction = 0.9;
    p.dirt.top_speed = 1.0;
    p.ground.long_grip = 4.0;
    p.ground.grip = 16.0;
    // Same mechanism, later and smaller: a few degrees on road, about 9 on dirt.
    p.road.drift_angle_deg = 4.0;
    p.dirt.drift_angle_deg = 9.0;
    p.drift_excess_full = 0.35;
    p.drift_turn = 0.6;
    p.mark_start = 0.9;
    p
}

/// Profile 3, axis: drift. Less lateral grip everywhere, so the rear steps out on road and dirt,
/// and slides keep their speed.
pub fn drift() -> CarParams {
    let mut p = fidele();
    p.name = "Drift".into();
    p.description = "Comme Fidèle mais moins d'adhérence : l'arrière chasse vite et large, même sur la route, sans trop perdre de vitesse.".into();
    // Less lateral grip and wider angles: full lock drifts from ~120 km/h on road and ~75 on
    // dirt, the angle comes quickly and the slides keep their speed.
    p.road.grip = 10.0;
    p.road.drift_angle_deg = 22.0;
    p.dirt.grip = 5.5;
    p.dirt.drift_angle_deg = 42.0;
    p.drift_excess_full = 0.4;
    p.drift_build_s = 0.3;
    p.drift_swing_rate = 1.4;
    p.drift_bleed = 2.5;
    p.drift_turn = 1.0;
    p.drift_turn_cost = 0.08;
    p.turn_drag = 0.5;
    p
}

/// Profile 4, axis: suspension and weight. Long soft bouncy suspension, visible roll, dive and squat.
pub fn buggy_lourd() -> CarParams {
    let mut p = fidele();
    p.name = "Buggy lourd".into();
    p.description = "Comme Fidèle mais sur une suspension longue et molle : ça penche, ça plonge et ça rebondit.".into();
    p.mass = 1400.0;
    p.cg_height = 0.15;
    p.wheel_radius = 0.5;
    p.suspension_travel = 0.8;
    p.spring_hz = 1.5;
    p.damping = 0.2;
    p.bump_stop = 15.0;
    p.bump_zone = 0.12;
    p.anti_roll = 0.35;
    p.roll_transfer = 0.4;
    p.pitch_transfer = 0.5;
    p.anti_rollover = 0.75;
    p.ground_level = 3.0;
    p.wall_bounce = 0.1;
    p.body_bounce = 0.1;
    p
}

/// Profile 5, axis: gravity. Same car on the ground, long floaty jumps.
///
/// Gravity is "Fidèle"'s 40 m/s² scaled by the Mars/Earth ratio (0.38): 15 m/s². The springs and
/// the lateral load transfer are scaled so the car sits, rolls and grips like "Fidèle", and a little
/// downforce keeps it on the ground over crests.
pub fn basse_gravite() -> CarParams {
    let mut p = fidele();
    p.name = "Basse gravité".into();
    p.description = "Comme Fidèle au sol, mais gravité martienne (×0,38) : sauts longs et flottants, bien reçus.".into();
    p.gravity = 15.0;
    p.spring_hz = 2.0 * libm::sqrtf(15.0 / 40.0);
    p.roll_transfer = 0.22 * 15.0 / 40.0;
    p.downforce = 0.5;
    p.air_level = 3.0;
    p.air_damping = 1.5;
    p
}

/// Profile 6, the compromise: Fidèle's dirt, a more forgiving road that drifts a little too, and a
/// little help in the air.
pub fn equilibre() -> CarParams {
    let mut p = fidele();
    p.name = "Équilibre".into();
    p.description = "Fidèle avec une route plus tolérante qui glisse un peu elle aussi, et un peu d'aide en l'air.".into();
    // Fidèle's dirt (which was this profile's), a forgiving road (less grip, small drifts there
    // too), and a little help in the air.
    p.road.grip = 15.0;
    p.road.drift_angle_deg = 8.0;
    p.air_control = 1.5;
    p.air_level = 1.5;
    p
}
