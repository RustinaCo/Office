//! Desktop harness for the Android app struct: exercises the same
//! `GridAndroidApp` on Linux before going near a device.
#[cfg(not(target_os = "android"))]
use gridcraft_android::GridAndroidApp;

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "GridCraft (Android-backend harness)",
        options,
        Box::new(|cc| Ok(Box::new(GridAndroidApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
fn main() {}
