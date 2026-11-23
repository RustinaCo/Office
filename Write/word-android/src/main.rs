//! Desktop harness for the Android app struct: exercises the same
//! `WordAndroidApp` on Linux before going near a device.
#[cfg(not(target_os = "android"))]
use wordcraft_android::WordAndroidApp;

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions::default();
    let is_phone = std::env::var_os("WORDCRAFT_PHONE").is_some_and(|v| v == "1" || v == "true");
    if is_phone {
        options.viewport = egui::ViewportBuilder::default()
            .with_inner_size([412.0, 892.0])
            .with_title("WordCraft (Mobile Phone Mode)");
    }
    eframe::run_native(
        "WordCraft (Android-backend harness)",
        options,
        Box::new(|cc| Ok(Box::new(WordAndroidApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
fn main() {}
