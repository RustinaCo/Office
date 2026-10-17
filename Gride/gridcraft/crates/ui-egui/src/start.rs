//! Mobile launch splash + start screen (new / import / settings + credits).
//!
//! Desktop rendering is untouched: everything here early-outs unless this is
//! an Android build showing its first frames.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use egui::{Align2, Color32, Ui, vec2};

use crate::{BRAND_FULL, mobile, SheetApp};

/// Brand green background for the splash.
pub const BRAND_BG: Color32 = Color32::from_rgb(0x1E, 0x7A, 0x3C);

/// First-paint time (ms); `u64::MAX` once the splash is done.
static SPLASH_T0: AtomicU64 = AtomicU64::new(0);
/// The start screen goes away once the user picks a destination.
static START_DONE: AtomicBool = AtomicBool::new(false);

const SPLASH_MS: f64 = 1600.0;

/// Whether the splash covers this frame (first ~1.6 s on phones, or until
/// the first tap). Call at the very top of `ui()`: `true` means "painted,
/// skip everything else".
pub fn splash(ui: &mut egui::Ui) -> bool {
    if !mobile() {
        return false;
    }
    let now = crate::now_ms();
    let t0bits = SPLASH_T0.load(Ordering::Relaxed);
    if t0bits == u64::MAX {
        return false;
    }
    if t0bits == 0 {
        SPLASH_T0.store(now.max(1.0).to_bits(), Ordering::Relaxed);
        ui.ctx().request_repaint();
        return true;
    }
    let t0 = f64::from_bits(t0bits);
    if now - t0 >= SPLASH_MS || ui.input(|i| i.pointer.any_pressed()) {
        SPLASH_T0.store(u64::MAX, Ordering::Relaxed);
        return false;
    }
    let r = ui.max_rect();
    ui.painter().rect_filled(r, 0.0, BRAND_BG);
    let c = r.center() + vec2(0.0, -40.0);
    ui.painter().circle_filled(c, 64.0, Color32::WHITE);
    ui.painter().text(
        c,
        Align2::CENTER_CENTER,
        "G",
        egui::FontId::new(56.0, egui::FontFamily::Proportional),
        BRAND_BG,
    );
    ui.painter().text(
        c + vec2(0.0, 96.0),
        Align2::CENTER_CENTER,
        BRAND_FULL,
        egui::FontId::new(24.0, egui::FontFamily::Proportional),
        Color32::WHITE,
    );
    ui.painter().text(
        egui::pos2(r.right() - 16.0, r.bottom() - 34.0),
        Align2::RIGHT_BOTTOM,
        "created by sina khanzadeh",
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
        Color32::from_white_alpha(180),
    );
    ui.painter().text(
        egui::pos2(r.right() - 16.0, r.bottom() - 20.0),
        Align2::RIGHT_BOTTOM,
        "Using ArtCraft \u{2014} the ArtCraft team's design system",
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
        Color32::from_white_alpha(180),
    );
    ui.ctx().request_repaint();
    true
}

/// Whether the start screen replaces the main UI this frame.
pub fn start_active() -> bool {
    mobile() && !START_DONE.load(Ordering::Relaxed)
}

/// Leave the start screen for the main UI.
pub fn dismiss() {
    START_DONE.store(true, Ordering::Relaxed);
}

/// Developer credits, shared by the start screen and About.
pub fn credits(ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.label(egui::RichText::new("Developer: Sina Khanzadeh").strong().color(t.text));
    ui.label(egui::RichText::new("Discord · Telegram · Instagram: @programmer_1998").small().color(t.text_dim));
    ui.label(egui::RichText::new("GitHub: @programmer-1998").small().color(t.text_dim));
    ui.label(egui::RichText::new("Email: khanzadeh.1377@gmail.com").small().color(t.text_dim));
    ui.hyperlink_to("sina-khanzadeh.ir", "https://sina-khanzadeh.ir");
}

/// Full-screen launch menu: new workbook, import, settings, recents, credits.
pub fn show_start(app: &mut SheetApp, ui: &mut Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.window)).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(48.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(BRAND_FULL).font(crate::theme::ui_bold(26.0)).color(t.text));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Spreadsheets, rebuilt in Rust").font(crate::theme::ui_font(14.0)).color(t.text_dim));
            });
            ui.add_space(28.0);
            let w = (ui.available_width() - 64.0).clamp(220.0, 420.0);
            ui.vertical_centered(|ui| {
                if ui.add(egui::Button::new("  New workbook  ").min_size(vec2(w, 56.0))).clicked() {
                    app.run_or_alert("file.new", serde_json::json!({}));
                    dismiss();
                }
                ui.add_space(10.0);
                if ui.add(egui::Button::new("  Import / Open…  ").min_size(vec2(w, 56.0))).clicked() {
                    app.open_backstage(crate::backstage::Page::Open);
                    dismiss();
                }
                ui.add_space(10.0);
                if ui.add(egui::Button::new("  Settings  ").min_size(vec2(w, 56.0))).clicked() {
                    app.open_backstage(crate::backstage::Page::Options);
                    dismiss();
                }
            });
            let recent: Vec<String> = app.ui.recent.iter().take(5).cloned().collect();
            if !recent.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Recent").strong().color(t.text));
                });
                ui.add_space(6.0);
                for path in recent {
                    let short = path.rsplit('/').next().unwrap_or(&path).to_string();
                    if ui.add(egui::Button::new(short).min_size(vec2(w, 48.0))).clicked() {
                        app.open_path(&path);
                        dismiss();
                    }
                    ui.add_space(4.0);
                }
            }
            ui.add_space(32.0);
            ui.vertical_centered(|ui| {
                credits(ui);
            });
            ui.add_space(24.0);
        });
    });
}
