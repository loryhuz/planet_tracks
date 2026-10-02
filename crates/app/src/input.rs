//! Keyboard and gamepad → driving input, game actions and menu navigation.

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
    NextMap,
    /// Pause the race on the settings sheet (and resume), or leave a finished one for the menu.
    Menu,
}

/// A menu move, from the keyboard or a gamepad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    /// Any other key or button (the title screen starts on any).
    Any,
}

pub struct Controls {
    held: HashSet<KeyCode>,
    gilrs: Option<Gilrs>,
    actions: Vec<Action>,
    nav: Vec<Nav>,
    /// Left stick past the threshold on each axis (-1, 0, 1), for one move per push.
    stick: (i32, i32),
    pub gamepad_name: Option<String>,
    /// The on-screen touch controls' input (set by the HUD each frame).
    pub touch: Input,
    /// Touch controls: always on the throttle, except while braking (so the brake can reverse).
    pub auto_gas: bool,
}

const STICK_DEAD_ZONE: f32 = 0.12;

impl Controls {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().ok();
        let gamepad_name = gilrs.as_ref().and_then(|g| g.gamepads().next().map(|(_, p)| p.name().to_string()));
        Self { held: HashSet::new(), gilrs, actions: Vec::new(), nav: Vec::new(), stick: (0, 0), gamepad_name, touch: Input::default(), auto_gas: false }
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        if pressed {
            self.held.insert(code);
            // Arrows repeat while held, as in any menu.
            let nav = match code {
                KeyCode::ArrowLeft | KeyCode::KeyA => Some(Nav::Left),
                KeyCode::ArrowRight | KeyCode::KeyD => Some(Nav::Right),
                KeyCode::ArrowUp | KeyCode::KeyW => Some(Nav::Up),
                KeyCode::ArrowDown | KeyCode::KeyS => Some(Nav::Down),
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => (!repeat).then_some(Nav::Confirm),
                KeyCode::Escape | KeyCode::Backspace => (!repeat).then_some(Nav::Back),
                KeyCode::ShiftLeft | KeyCode::ShiftRight | KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::AltLeft
                | KeyCode::AltRight | KeyCode::SuperLeft | KeyCode::SuperRight | KeyCode::Tab | KeyCode::CapsLock => None,
                _ => (!repeat).then_some(Nav::Any),
            };
            self.nav.extend(nav);
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
                    KeyCode::KeyN => Some(Action::NextMap),
                    KeyCode::PageDown => Some(Action::NextProfile),
                    KeyCode::PageUp => Some(Action::PrevProfile),
                    KeyCode::Escape => Some(Action::Menu),
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
                        Button::Start => Some(Action::Menu),
                        Button::DPadUp => Some(Action::Camera),
                        _ => None,
                    };
                    self.actions.extend(action);
                    self.nav.push(match button {
                        Button::DPadLeft => Nav::Left,
                        Button::DPadRight => Nav::Right,
                        Button::DPadUp => Nav::Up,
                        Button::DPadDown => Nav::Down,
                        Button::South | Button::Start => Nav::Confirm,
                        Button::East | Button::Select => Nav::Back,
                        _ => Nav::Any,
                    });
                }
                EventType::AxisChanged(axis, value, _) => {
                    let side = if value > 0.6 { 1 } else if value < -0.6 { -1 } else if value.abs() < 0.3 { 0 } else { 2 };
                    let (slot, moves) = match axis {
                        Axis::LeftStickX => (&mut self.stick.0, [Nav::Left, Nav::Right]),
                        // gilrs reports up as positive.
                        Axis::LeftStickY => (&mut self.stick.1, [Nav::Down, Nav::Up]),
                        _ => continue,
                    };
                    if side != 2 && side != *slot {
                        *slot = side;
                        if side != 0 {
                            self.nav.push(moves[(side + 1) as usize / 2]);
                        }
                    }
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

    pub fn take_nav(&mut self) -> Vec<Nav> {
        std::mem::take(&mut self.nav)
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
        if self.touch.steer != 0.0 {
            input.steer = self.touch.steer;
        }
        input.gas = input.gas.max(self.touch.gas);
        input.brake = input.brake.max(self.touch.brake);
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
        if self.auto_gas {
            input.gas = if input.brake > 0.0 { 0.0 } else { 1.0 };
        }
        input
    }
}
