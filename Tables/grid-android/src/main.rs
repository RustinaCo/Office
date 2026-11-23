//! Desktop harness for the Android app struct: exercises the same
//! `GridAndroidApp` on Linux before going near a device.
#[cfg(not(target_os = "android"))]
use gridcraft_android::GridAndroidApp;

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions::default();
    let is_phone = std::env::var_os("GRIDCRAFT_PHONE").is_some_and(|v| v == "1" || v == "true");
    if is_phone {
        options.viewport = egui::ViewportBuilder::default()
            .with_inner_size([412.0, 892.0])
            .with_title("GridCraft (Mobile Phone Mode)");
    }
    eframe::run_native(
        "GridCraft (Android-backend harness)",
        options,
        Box::new(|cc| Ok(Box::new(GridAndroidApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
fn main() {}
