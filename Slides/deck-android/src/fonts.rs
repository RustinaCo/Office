//! Bundled fonts for Android: the app only scans system font dirs (absent on
//! Android), so without these every non-Latin script — and any workbook font
//! we don't ship — falls back to tofu. Files are copied to `$HOME/.fonts` on
//! first launch (the app's `system_font_dirs` already scans there, and the
//! `sys-arabic` fallback chain looks for them by name).
//!
//! English (metric-compatible workhorses): Carlito ≈ Calibri, Caladea ≈
//! Cambria, Arimo ≈ Arial, Tinos ≈ Times New Roman, Cousine ≈ Courier New.
//!
//! Persian/Arabic: Vazirmatn, Noto Naskh Arabic, Noto Sans Arabic, Cairo,
//! Tajawal, plus the popular open Persian families a presentation app user reaches for by
//! name — Sahel, Samim, Shabnam, Tanha, Gandom, Nahid, Parastoo, Estedad
//! (text; the open stand-ins for the commercial B Nazanin/B Yekan) and
//! Lalezar, Katibeh, Aref Ruqaa, Amiri, Markazi Text, Mirza, Noto Nastaliq
//! Urdu, Scheherazade New, Almarai (display and calligraphic). All OFL 1.1;
//! see `../../fonts/ATTRIBUTION.md`. Commercial Iranian families (B Nazanin,
//! B Titr, IRANSans…) stay out; a font the user drops into the app's own
//! `$HOME/.fonts` folder is picked up at launch.

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
    "Tajawal-Regular.ttf",
    "Sahel-Regular.ttf",
    "Sahel-Bold.ttf",
    "Samim-Regular.ttf",
    "Samim-Bold.ttf",
    "Shabnam-Regular.ttf",
    "Shabnam-Bold.ttf",
    "Tanha-Regular.ttf",
    "Gandom-Regular.ttf",
    "Nahid-Regular.ttf",
    "Parastoo-Regular.ttf",
    "Parastoo-Bold.ttf",
    "Estedad-Regular.ttf",
    "Estedad-Bold.ttf",
    "Lalezar-Regular.ttf",
    "Katibeh-Regular.ttf",
    "ArefRuqaa-Regular.ttf",
    "ArefRuqaa-Bold.ttf",
    "Amiri-Regular.ttf",
    "Amiri-Bold.ttf",
    "MarkaziText-Regular.ttf",
    "Mirza-Regular.ttf",
    "NotoNastaliqUrdu-Regular.ttf",
    "ScheherazadeNew-Regular.ttf",
    "Almarai-Regular.ttf",
    "Almarai-Bold.ttf"
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
