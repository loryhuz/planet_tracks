//! The phone's attitude, from CoreMotion (iOS): how far it leans to the side, for the track
//! editor's glance in 3D. Reading the device's motion needs no permission.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::msg_send;

#[link(name = "CoreMotion", kind = "framework")]
unsafe extern "C" {}

/// A motion manager sending the device's attitude 60 times a second.
pub struct Motion(Retained<AnyObject>);

impl Motion {
    /// `None` where the device has no motion sensors (the simulator).
    pub fn start() -> Option<Self> {
        let class = AnyClass::get(c"CMMotionManager")?;
        // SAFETY: on the main thread; CMMotionManager's documented initialiser and methods, with
        // the argument types they declare.
        unsafe {
            let manager: *mut AnyObject = msg_send![class, alloc];
            let manager: *mut AnyObject = msg_send![manager, init];
            let manager = Retained::from_raw(manager)?;
            let available: bool = msg_send![&*manager, isDeviceMotionAvailable];
            if !available {
                return None;
            }
            let _: () = msg_send![&*manager, setDeviceMotionUpdateInterval: 1.0f64 / 60.0];
            let _: () = msg_send![&*manager, startDeviceMotionUpdates];
            Some(Self(manager))
        }
    }

    /// The device's roll, radians: turned about its long side, positive leaning right.
    pub fn roll(&self) -> Option<f32> {
        // SAFETY: `deviceMotion` and its `attitude` are nil until the first update, checked.
        unsafe {
            let motion: *mut AnyObject = msg_send![&*self.0, deviceMotion];
            if motion.is_null() {
                return None;
            }
            let attitude: *mut AnyObject = msg_send![motion, attitude];
            if attitude.is_null() {
                return None;
            }
            let roll: f64 = msg_send![attitude, roll];
            Some(roll as f32)
        }
    }
}
