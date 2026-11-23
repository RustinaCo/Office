//! Desktop harness for the Android app struct: exercises the same
//! `DeckAndroidApp` on Linux before going near a device.
#[cfg(not(target_os = "android"))]
use deckcraft_android::DeckAndroidApp;

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions::default();
    let is_phone = std::env::var_os("DECKCRAFT_PHONE").is_some_and(|v| v == "1" || v == "true");
    if is_phone {
        options.viewport = egui::ViewportBuilder::default()
            .with_inner_size([412.0, 892.0])
            .with_title("DeckCraft (Mobile Phone Mode)");
    }
    eframe::run_native(
        "DeckCraft (Android-backend harness)",
        options,
        Box::new(|cc| Ok(Box::new(DeckAndroidApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
fn main() {}
