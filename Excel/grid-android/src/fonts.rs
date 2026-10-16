//! Bundled fonts for Android: the app only scans system font dirs (absent on
//! Android), so without these every non-Latin script — and any workbook font
//! we don't ship — falls back to tofu. Files are copied to `$HOME/.fonts` on
//! first launch (the app's `system_font_dirs` already scans there, and the
//! `sys-arabic` fallback chain looks for them by name).
//!
//! English (metric-compatible workhorses): Carlito ≈ Calibri, Caladea ≈
//! Cambria, Arimo ≈ Arial, Tinos ≈ Times New Roman, Cousine ≈ Courier New.
//! Persian/Arabic (all OFL, popular): Vazirmatn, Noto Naskh Arabic,
//! Noto Sans Arabic, Cairo, Tajawal.

macro_rules! bundled {
    ($($name:literal),*) => {
        const BUNDLED: &[(&str, &[u8])] = &[
            $(( $name, include_bytes!(concat!("../../fonts/", $name)) as &[u8] ),)*
        ];
    };
}

bundled!(
    "Carlito-Regular.ttf",
    "Carlito-Bold.ttf",
    "Caladea-Regular.ttf",
    "Arimo-Regular.ttf",
    "Tinos-Regular.ttf",
    "Cousine-Regular.ttf",
    "Vazirmatn-Regular.ttf",
    "Vazirmatn-Bold.ttf",
    "NotoNaskhArabic-Regular.ttf",
    "NotoSansArabic-Regular.ttf",
    "Cairo-Regular.ttf",
    "Tajawal-Regular.ttf"
);

/// Copy missing bundled fonts into `$HOME/.fonts`. Idempotent and cheap
/// after the first launch (skips files that already match in size).
pub fn install() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::Path::new(&home).join(".fonts");
    std::fs::create_dir_all(&dir).ok();
    for (name, bytes) in BUNDLED {
        let dest = dir.join(name);
        let fresh = std::fs::metadata(&dest)
            .map(|m| m.len() != bytes.len() as u64)
            .unwrap_or(true);
        if fresh {
            std::fs::write(&dest, bytes).ok();
        }
    }
}
