//! Desktop harness for the Rustina Draw (CADCraft-on-Android) shell.
//!
//! It runs the phone shell on the desktop so the host code — `install_io`, the settings
//! files, the [`CadAndroidApp`] frame loop — builds and runs on a workstation. The
//! Android-only pieces (JNI, SAF, the keyboard overlay, the safe-area spacers) are
//! compiled out, and the app gets no file pickers.
//!
//! Usage: `cargo run --release --bin cadcraft-android-desktop`

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rustina Draw")
            .with_inner_size([1280.0, 800.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Rustina Draw",
        options,
        Box::new(|cc| Ok(Box::new(cadcraft_android::CadAndroidApp::new(cc)))),
    )
}
