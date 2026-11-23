//! Desktop harness for the Rustina Draw (CADCraft-on-Android) shell.
//!
//! It runs the phone shell on the desktop so the host code — `install_io`, the settings
//! files, the [`CadAndroidApp`] frame loop — builds and runs on a workstation. The
//! Android-only pieces (JNI, SAF, the keyboard overlay, the safe-area spacers) are
//! compiled out, and the app gets no file pickers.
//!
//! Usage: `cargo run --release --bin cadcraft-android-desktop`

fn main() -> eframe::Result {
    // `RUSTINA_SIZE=430x900` opens the harness at a phone's proportions (the phone layout
    // follows `CADCRAFT_PHONE=1`), which is how the touch chrome is checked without a device.
    let size = std::env::var("RUSTINA_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some(egui::vec2(w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or(egui::vec2(1280.0, 800.0));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rustina Draw")
            .with_inner_size(size),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Rustina Draw",
        options,
        Box::new(|cc| Ok(Box::new(cadcraft_android::CadAndroidApp::new(cc)))),
    )
}
