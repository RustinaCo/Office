//! Desktop harness for the Android app struct: exercises the same
//! `WordAndroidApp` on Linux before going near a device.
#[cfg(not(target_os = "android"))]
use wordcraft_android::WordAndroidApp;

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "WordCraft (Android-backend harness)",
        options,
        Box::new(|cc| Ok(Box::new(WordAndroidApp::new(cc)))),
    )
}

#[cfg(target_os = "android")]
fn main() {}
