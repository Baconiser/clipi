//! macOS menu-bar activation. Accessory apps cannot show windows in front
//! until they become Regular and call activateIgnoringOtherApps.
//!
//! Do not use std::panic::catch_unwind around AppKit. An NSException is a
//! foreign exception; catch_unwind aborts via ___rust_foreign_exception.
//! Catch ObjC exceptions with objc2::exception::catch instead.

#[cfg(target_os = "macos")]
pub fn set_accessory() {
    let _ = objc2::exception::catch(set_accessory_inner);
}

#[cfg(not(target_os = "macos"))]
pub fn set_accessory() {}

#[cfg(target_os = "macos")]
pub fn bring_to_front() {
    let _ = objc2::exception::catch(bring_to_front_inner);
}

#[cfg(not(target_os = "macos"))]
pub fn bring_to_front() {}

#[cfg(target_os = "macos")]
pub fn resign_accessory() {
    set_accessory();
}

#[cfg(not(target_os = "macos"))]
pub fn resign_accessory() {}

#[cfg(target_os = "macos")]
fn set_accessory_inner() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

#[cfg(target_os = "macos")]
fn bring_to_front_inner() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    // Required for menu-bar apps. NSApp.activate() will not raise a window
    // when nothing is already focused.
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}
