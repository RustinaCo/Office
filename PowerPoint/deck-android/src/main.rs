//! Desktop harness for the Android app struct: exercises the same
//! `DeckAndroidApp` on Linux before going near a device.
#[cfg(not(target_os = "android"))]
use deckcraft_android::DeckAndroidApp;

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "DeckCraft (Android-backend harness)",
        options,
        Box::new(|cc| Ok(Box::new(DeckAndroidApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
fn main() {}
