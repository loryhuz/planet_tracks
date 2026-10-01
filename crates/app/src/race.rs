//! One run on a track: countdown, checkpoints, respawn, finish. Deterministic, so a recorded
//! list of frames replays the same run (that is how ghosts work).

use glam::Vec3;
use physics::{Car, CarParams, CarState, Input, World};
use serde::{Deserialize, Serialize};
use track::{Track, Trigger};

use crate::car_model::Look;

/// Ticks before the start (1.5 s at 100 Hz).
pub const COUNTDOWN_TICKS: u32 = 150;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub input: Input,
    /// Go back to the last checkpoint before this tick.
    pub respawn: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RaceEvent {
    Checkpoint { index: usize, tick: u32 },
    Finish { tick: u32 },
    /// Fell below the track or asked to respawn before any checkpoint: the app decides.
    Fell,
    RespawnAtStart,
}

pub struct Run {
    pub car: Car,
    /// State before the last tick, for render interpolation.
    pub prev: CarState,
    /// Countdown ticks left.
    pub countdown: u32,
    /// Race ticks since the start (10 ms each).
    pub tick: u32,
    pub crossed: Vec<bool>,
    /// Race tick at each checkpoint, in crossing order.
    pub splits: Vec<u32>,
    last_checkpoint: Option<CarState>,
    pub finished: Option<u32>,
    pub frames: Vec<Frame>,
    pub respawns: u32,
    /// Render-side suspension, after the last tick and before it.
    pub look: Look,
    pub look_prev: Look,
}

impl Run {
    pub fn new(params: CarParams, world: &World, track: &Track) -> Self {
        let car = Car::new(params, world, track.start);
        let prev = car.state.clone();
        let look = Look::new(&car.state);
        Self {
            car,
            prev,
            look,
            look_prev: look,
            countdown: COUNTDOWN_TICKS,
            tick: 0,
            crossed: vec![false; track.checkpoints.len()],
            splits: Vec::new(),
            last_checkpoint: None,
            finished: None,
            frames: Vec::new(),
            respawns: 0,
        }
    }

    pub fn racing(&self) -> bool {
        self.countdown == 0 && self.finished.is_none()
    }

    pub fn step(&mut self, world: &World, track: &Track, frame: Frame) -> Vec<RaceEvent> {
        let respawns = self.respawns;
        let events = self.advance(world, track, frame);
        self.look_prev = self.look;
        if self.respawns != respawns || events.contains(&RaceEvent::RespawnAtStart) {
            self.look = Look::new(&self.car.state);
            self.look_prev = self.look;
        }
        self.look.step(&self.car.state, &self.car.params, physics::DT);
        events
    }

    fn advance(&mut self, world: &World, track: &Track, frame: Frame) -> Vec<RaceEvent> {
        let mut events = Vec::new();
        self.prev = self.car.state.clone();
        if self.countdown > 0 {
            self.countdown -= 1;
            self.car.step(world, Input::default());
            return events;
        }
        if self.finished.is_some() {
            // Coast after the line.
            self.car.step(world, Input { brake: 0.3, ..Default::default() });
            return events;
        }

        self.frames.push(frame);
        if frame.respawn {
            match &self.last_checkpoint {
                Some(state) => {
                    self.car.state = state.clone();
                    self.prev = state.clone();
                    self.respawns += 1;
                }
                None => {
                    self.car.respawn(world, track.start);
                    self.prev = self.car.state.clone();
                    self.respawns += 1;
                    events.push(RaceEvent::RespawnAtStart);
                }
            }
        }

        let from = self.car.state.position;
        self.car.step(world, frame.input);
        self.tick += 1;
        let to = self.car.state.position;

        for (i, cp) in track.checkpoints.iter().enumerate() {
            if !self.crossed[i] && swept(cp, from, to) {
                self.crossed[i] = true;
                self.splits.push(self.tick);
                self.last_checkpoint = Some(self.car.state.clone());
                events.push(RaceEvent::Checkpoint { index: i, tick: self.tick });
            }
        }
        if self.crossed.iter().all(|&c| c) && swept(&track.finish, from, to) {
            self.finished = Some(self.tick);
            events.push(RaceEvent::Finish { tick: self.tick });
        }
        if to.y < track.fall_limit_y || !to.is_finite() {
            events.push(RaceEvent::Fell);
        }
        events
    }
}

/// Whether the car's centre passed through the trigger during the tick (sampled every 20 cm,
/// so nothing is missed at 400 km/h).
fn swept(trigger: &Trigger, from: Vec3, to: Vec3) -> bool {
    let steps = (((to - from).length() / 0.2).ceil() as usize).clamp(1, 64);
    (0..=steps).any(|i| trigger.contains(from.lerp(to, i as f32 / steps as f32)))
}

/// Race time as m:ss.cc (1 tick = 1 centisecond).
pub fn format_time(ticks: u32) -> String {
    let cs = ticks % 100;
    let s = (ticks / 100) % 60;
    let m = ticks / 6000;
    format!("{m}:{s:02}.{cs:02}")
}

pub fn format_delta(ticks: i64) -> String {
    let sign = if ticks < 0 { '-' } else { '+' };
    let t = ticks.unsigned_abs() as u32;
    format!("{sign}{}.{:02}", t / 100, t % 100)
}
