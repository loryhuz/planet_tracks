//! iOS glue that winit 0.30 does not provide: the app's scene and the screen's safe area.

use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Puts winit's window in the app's scene and shows it. iOS 27 stops apps that do not adopt the
/// scene life cycle at launch, so Info.plist declares a scene; winit makes its window without one,
/// which would leave it hidden.
pub fn attach_to_scene(window: &Window) {
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else { return };
    // SAFETY: on the main thread (windows are made there), with a live UIView from winit;
    // `connectedScenes` holds the app's one window scene by the time it becomes active.
    unsafe {
        let view: &AnyObject = handle.ui_view.cast().as_ref();
        let ui_window: *mut AnyObject = msg_send![view, window];
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        let scenes: *mut AnyObject = msg_send![app, connectedScenes];
        let scene: *mut AnyObject = msg_send![scenes, anyObject];
        if ui_window.is_null() || scene.is_null() {
            return;
        }
        let _: () = msg_send![ui_window, setWindowScene: scene];
        let _: () = msg_send![ui_window, makeKeyAndVisible];
    }
}

/// The screen's notch, rounded corners and home indicator, in egui points. winit's inner
/// rectangle is the safe area; egui-winit's insets would ignore the menu's zoom.
pub fn safe_area(window: &Window, zoom: f32) -> egui::SafeAreaInsets {
    let (outer, inner) = (window.outer_size(), window.inner_size());
    let at = window.inner_position().unwrap_or_default();
    let k = 1.0 / (window.scale_factor() as f32 * zoom);
    egui::SafeAreaInsets(egui::epaint::MarginF32 {
        left: at.x as f32 * k,
        top: at.y as f32 * k,
        right: (outer.width as i32 - inner.width as i32 - at.x) as f32 * k,
        bottom: (outer.height as i32 - inner.height as i32 - at.y) as f32 * k,
    })
}
