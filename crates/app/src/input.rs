//! Keyboard and gamepad → driving input and game actions.

use std::collections::HashSet;

use gilrs::{Axis, Button, EventType, Gilrs};
use physics::Input;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Respawn,
    Restart,
    Profile(usize),
    NextProfile,
    PrevProfile,
    Eliminate,
    TogglePanel,
    Camera,
    Fullscreen,
    Mute,
    NextEngine,
}

pub struct Controls {
    held: HashSet<KeyCode>,
    gilrs: Option<Gilrs>,
    actions: Vec<Action>,
    pub gamepad_name: Option<String>,
}

const STICK_DEAD_ZONE: f32 = 0.12;

impl Controls {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().ok();
        let gamepad_name = gilrs.as_ref().and_then(|g| g.gamepads().next().map(|(_, p)| p.name().to_string()));
        Self { held: HashSet::new(), gilrs, actions: Vec::new(), gamepad_name }
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        if pressed {
            self.held.insert(code);
            if !repeat {
                let action = match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => Some(Action::Respawn),
                    KeyCode::Backspace | KeyCode::Delete => Some(Action::Restart),
                    KeyCode::Digit1 => Some(Action::Profile(0)),
                    KeyCode::Digit2 => Some(Action::Profile(1)),
                    KeyCode::Digit3 => Some(Action::Profile(2)),
                    KeyCode::Digit4 => Some(Action::Profile(3)),
                    KeyCode::Digit5 => Some(Action::Profile(4)),
                    KeyCode::Digit6 => Some(Action::Profile(5)),
                    KeyCode::Digit7 => Some(Action::Profile(6)),
                    KeyCode::Digit8 => Some(Action::Profile(7)),
                    KeyCode::KeyX => Some(Action::Eliminate),
                    KeyCode::Tab => Some(Action::TogglePanel),
                    KeyCode::KeyC => Some(Action::Camera),
                    KeyCode::KeyF => Some(Action::Fullscreen),
                    KeyCode::KeyM => Some(Action::Mute),
                    KeyCode::KeyE => Some(Action::NextEngine),
                    KeyCode::PageDown => Some(Action::NextProfile),
                    KeyCode::PageUp => Some(Action::PrevProfile),
                    _ => None,
                };
                self.actions.extend(action);
            }
        } else {
            self.held.remove(&code);
        }
    }

    /// Forget held keys (window lost focus).
    pub fn clear(&mut self) {
        self.held.clear();
    }

    fn down(&self, codes: &[KeyCode]) -> bool {
        codes.iter().any(|c| self.held.contains(c))
    }

    /// Polls the gamepad; call once per frame.
    pub fn poll(&mut self) {
        let Some(gilrs) = self.gilrs.as_mut() else { return };
        while let Some(ev) = gilrs.next_event() {
            match ev.event {
                EventType::ButtonPressed(button, _) => {
                    let action = match button {
                        Button::East => Some(Action::Respawn),
                        Button::North | Button::Select => Some(Action::Restart),
                        Button::RightTrigger => Some(Action::NextProfile),
                        Button::LeftTrigger => Some(Action::PrevProfile),
                        Button::Start => Some(Action::TogglePanel),
                        Button::DPadUp => Some(Action::Camera),
                        _ => None,
                    };
                    self.actions.extend(action);
                }
                EventType::Connected => {
                    self.gamepad_name = Some(gilrs.gamepad(ev.id).name().to_string());
                }
                _ => {}
            }
        }
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// The driving input for the next tick: keyboard, overridden by the gamepad when it is used.
    pub fn driving(&self) -> Input {
        let left = self.down(&[KeyCode::ArrowLeft, KeyCode::KeyA]);
        let right = self.down(&[KeyCode::ArrowRight, KeyCode::KeyD]);
        let mut input = Input {
            steer: (right as i32 - left as i32) as f32,
            gas: self.down(&[KeyCode::ArrowUp, KeyCode::KeyW]) as i32 as f32,
            brake: self.down(&[KeyCode::ArrowDown, KeyCode::KeyS]) as i32 as f32,
        };
        if let Some(gilrs) = &self.gilrs {
            for (_, pad) in gilrs.gamepads() {
                let x = pad.value(Axis::LeftStickX);
                if x.abs() > STICK_DEAD_ZONE {
                    input.steer = (x.signum() * (x.abs() - STICK_DEAD_ZONE) / (1.0 - STICK_DEAD_ZONE)).clamp(-1.0, 1.0);
                }
                let trigger = |b: Button| pad.button_data(b).map(|d| d.value()).unwrap_or(0.0);
                let gas = trigger(Button::RightTrigger2).max(if pad.is_pressed(Button::South) { 1.0 } else { 0.0 });
                let brake = trigger(Button::LeftTrigger2).max(if pad.is_pressed(Button::West) { 1.0 } else { 0.0 });
                input.gas = input.gas.max(gas);
                input.brake = input.brake.max(brake);
            }
        }
        input
    }
}
