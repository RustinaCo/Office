//! Phone layout: launch splash, slide-out panels, the start page and the two-row status bar.
//!
//! Everything here early-outs unless this is an Android build, or `CADCRAFT_PHONE=1` asks for the
//! phone layout in a desktop window (that is how the layout is tested and screenshotted). The
//! desktop window keeps the docking layout and the wide status bar exactly as they were.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use egui::{Align2, Color32, Rect, Stroke, Ui, vec2};
use serde_json::json;

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// Rustina Draw's brand colour: AutoCAD's own red (Pantone 1925 C).
pub const BRAND_BG: Color32 = Color32::from_rgb(0xE5, 0x10, 0x50);
/// The name on the splash, the start page and Android's launcher.
pub const APP_NAME: &str = "Rustina Draw";
/// The credit line the splash and About share.
pub const CREDIT: &str = "Using CADCraft by the ArtCraft team";

/// First-paint time (ms bits); `u64::MAX` once the splash is done.
static SPLASH_T0: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The left tool drawer and the right layers/properties drawer.
static TOOLS_OPEN: AtomicBool = AtomicBool::new(false);
static PALETTES_OPEN: AtomicBool = AtomicBool::new(false);
/// Whether the app wants the shell's keyboard up. The command line asks for it when it is
/// tapped, the drawing area lets it go; a text field in a dialog asks through egui's own IME
/// request, which the shell reads too. There is no keyboard button: tapping a field is the
/// gesture, and a button for it was one thing too many in the tool bar.
static KEYBOARD_WANTED: AtomicBool = AtomicBool::new(false);
/// The About page, in front of the start page (like the other three Rustina apps).
static ABOUT_OPEN: AtomicBool = AtomicBool::new(false);
/// The right drawer's tab: 0 Layers, 1 Properties.
static PALETTE_TAB: AtomicU8 = AtomicU8::new(0);

const SPLASH_MS: f64 = 1600.0;

/// Whether the phone layout is in charge. Read once: the layout must not change mid-run.
pub fn mobile() -> bool {
    if cfg!(target_os = "android") {
        return true;
    }
    static PHONE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PHONE.get_or_init(|| std::env::var_os("CADCRAFT_PHONE").is_some_and(|v| v == "1" || v == "true"))
}

/// Whether the About page is in front.
pub fn about_open() -> bool {
    ABOUT_OPEN.load(Ordering::Relaxed)
}

/// Leave the About page.
pub fn close_about() {
    ABOUT_OPEN.store(false, Ordering::Relaxed);
}

/// Bring the About page up (and get the drawers out of the way).
fn open_about() {
    TOOLS_OPEN.store(false, Ordering::Relaxed);
    PALETTES_OPEN.store(false, Ordering::Relaxed);
    ABOUT_OPEN.store(true, Ordering::Relaxed);
}

/// A text field was tapped: the shell should raise the keyboard.
pub fn request_keyboard() {
    KEYBOARD_WANTED.store(true, Ordering::Relaxed);
}

/// The drawing area was tapped: the keyboard has nothing to type into any more.
pub fn dismiss_keyboard() {
    KEYBOARD_WANTED.store(false, Ordering::Relaxed);
}

/// Whether the app is waiting for typing (the shell raises the keyboard while this is true).
pub fn keyboard_wanted() -> bool {
    KEYBOARD_WANTED.load(Ordering::Relaxed)
}

/// Whether the left drawer is open (the shell closes it on Back).
pub fn close_drawers() -> bool {
    TOOLS_OPEN.swap(false, Ordering::Relaxed) | PALETTES_OPEN.swap(false, Ordering::Relaxed)
}

/// Called from the Tool Sets palette when a tool is chosen: a drawer has done its job.
pub fn tools_picked() {
    if mobile() {
        TOOLS_OPEN.store(false, Ordering::Relaxed);
    }
}

/// The Rustina Draw mark: the app's colour square with its initial, exactly like the launcher
/// icon. Ours, drawn in code (shared by the splash, the start page and About).
pub fn brand_mark(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, egui::CornerRadius::ZERO, BRAND_BG);
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        "D",
        egui::FontId::new(r.width() * 0.62, egui::FontFamily::Proportional),
        Color32::WHITE,
    );
}

/// The launch splash: brand background, the mark, the name and the credits, for ~1.6 s or until
/// the first tap. Call at the very top of `ui()`; `true` means "painted, skip everything else".
pub fn splash(ui: &mut Ui) -> bool {
    if !mobile() {
        return false;
    }
    let now = crate::now_ms();
    let t0 = SPLASH_T0.load(Ordering::Relaxed);
    if t0 == u64::MAX {
        return false;
    }
    if t0 == 0 {
        SPLASH_T0.store(now.max(1.0).to_bits(), Ordering::Relaxed);
        ui.ctx().request_repaint();
        return true;
    }
    if now - f64::from_bits(t0) >= SPLASH_MS || ui.input(|i| i.pointer.any_pressed()) {
        SPLASH_T0.store(u64::MAX, Ordering::Relaxed);
        return false;
    }
    let r = ui.max_rect();
    ui.painter().rect_filled(r, 0.0, BRAND_BG);
    let c = r.center() + vec2(0.0, -40.0);
    brand_mark(ui.painter(), Rect::from_center_size(c, vec2(132.0, 132.0)));
    ui.painter().text(c + vec2(0.0, 96.0), Align2::CENTER_CENTER, APP_NAME, egui::FontId::proportional(24.0), Color32::WHITE);
    ui.painter().text(
        c + vec2(0.0, 122.0),
        Align2::CENTER_CENTER,
        crate::tf!("Version {version}", version = env!("CARGO_PKG_VERSION")),
        egui::FontId::proportional(12.0),
        Color32::from_white_alpha(190),
    );
    ui.painter().text(
        egui::pos2(r.right() - 14.0, r.bottom() - 30.0),
        Align2::RIGHT_BOTTOM,
        "created by sina khanzadeh",
        egui::FontId::proportional(10.5),
        Color32::from_white_alpha(170),
    );
    ui.painter().text(
        egui::pos2(r.right() - 14.0, r.bottom() - 17.0),
        Align2::RIGHT_BOTTOM,
        CREDIT,
        egui::FontId::proportional(10.5),
        Color32::from_white_alpha(170),
    );
    ui.ctx().request_repaint();
    true
}

/// The two edge rails the canvas keeps: a tools button on the left, a layers/properties button on
/// the right. Both open overlays instead of shrinking the drawing (a phone has no width to spare).
pub fn rails(ui: &mut Ui) {
    let t = Tokens::get();
    egui::Panel::left("cc_phone_rail_left").exact_size(30.0).resizable(false).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let tools = TOOLS_OPEN.load(Ordering::Relaxed);
        let br = Rect::from_center_size(egui::pos2(r.center().x, r.top() + 22.0), vec2(26.0, 26.0));
        let resp = ui.interact(br, ui.id().with("phone_tools"), egui::Sense::click());
        if tools || resp.hovered() {
            ui.painter().rect_filled(br, 3.0, t.toggle_on.gamma_multiply(0.35));
        }
        icons::paint(ui.painter(), br.shrink(3.0), Icon::Menu, false);
        icons::describe_control(ui, &resp, "Tool Sets", Some(tools), false);
        if resp.on_hover_text(crate::tl!("Tool Sets")).clicked() {
            let open = !tools;
            TOOLS_OPEN.store(open, Ordering::Relaxed);
            if open {
                PALETTES_OPEN.store(false, Ordering::Relaxed);
            }
        }
    });
    egui::Panel::right("cc_phone_rail_right").exact_size(30.0).resizable(false).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let open = PALETTES_OPEN.load(Ordering::Relaxed);
        let br = Rect::from_center_size(egui::pos2(r.center().x, r.top() + 22.0), vec2(26.0, 26.0));
        let resp = ui.interact(br, ui.id().with("phone_palettes"), egui::Sense::click());
        if open || resp.hovered() {
            ui.painter().rect_filled(br, 3.0, t.toggle_on.gamma_multiply(0.35));
        }
        icons::paint(ui.painter(), br.shrink(3.0), Icon::Properties, false);
        icons::describe_control(ui, &resp, "Properties", Some(open), false);
        if resp.on_hover_text(crate::tl!("Properties")).clicked() {
            let open = !open;
            PALETTES_OPEN.store(open, Ordering::Relaxed);
            if open {
                TOOLS_OPEN.store(false, Ordering::Relaxed);
            }
        }
    });
}

/// The overlay drawers, over the drawing area `rect`: tapping outside closes them, and the tool
/// drawer closes itself as soon as a tool is chosen.
pub fn drawers(app: &mut CadApp, ctx: &egui::Context, rect: Rect) {
    let tools = TOOLS_OPEN.load(Ordering::Relaxed);
    let palettes = PALETTES_OPEN.load(Ordering::Relaxed);
    if !tools && !palettes {
        return;
    }
    let close = || {
        TOOLS_OPEN.store(false, Ordering::Relaxed);
        PALETTES_OPEN.store(false, Ordering::Relaxed);
    };
    egui::Area::new(egui::Id::new("phone_drawer_backdrop")).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(90));
        if resp.clicked() {
            close();
        }
    });
    let t = Tokens::get();
    let w = (rect.width() * 0.88).min(360.0);
    // The drawer paints its own background first, in its own layer, before its content: painted
    // there it can never end up in front of the drawer, and the drawing can never show through it.
    let panel = |ui: &Ui, dr: Rect, edge: f32| {
        let p = ui.painter();
        p.rect_filled(dr, 0.0, t.panel);
        p.rect_stroke(dr, 0.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        p.vline(edge, dr.y_range(), Stroke::new(1.0, t.border));
    };
    if tools {
        let dr = Rect::from_min_max(rect.min, egui::pos2(rect.left() + w, rect.bottom()));
        egui::Area::new(egui::Id::new("phone_drawer_tools")).order(egui::Order::Foreground).fixed_pos(dr.min).show(ctx, |ui| {
            panel(ui, dr, dr.right());
            // An Area's own `max_rect` is the whole screen, not the drawer: the content is laid out
            // inside the drawer rect.
            ui.scope_builder(egui::UiBuilder::new().max_rect(dr), |ui| {
                ui.set_min_size(dr.size());
                ui.shrink_clip_rect(dr);
                crate::palettes::toolsets(app, ui);
            });
        });
    }
    if palettes {
        let dr = Rect::from_min_max(egui::pos2(rect.right() - w, rect.top()), rect.max);
        let tab_h = 28.0;
        egui::Area::new(egui::Id::new("phone_drawer_palettes")).order(egui::Order::Foreground).fixed_pos(dr.min).show(ctx, |ui| {
            panel(ui, dr, dr.left());
            ui.scope_builder(egui::UiBuilder::new().max_rect(dr), |ui| {
                ui.shrink_clip_rect(dr);
                // The same tab row as the Tool Sets drawer, mirrored: tabs from the left edge, the
                // collapse chevron on the right one, one tab body under it.
                let row = Rect::from_min_size(dr.min, vec2(dr.width(), tab_h));
                let p = ui.painter().clone();
                ui.allocate_rect(row, egui::Sense::click());
                p.rect_filled(row, 0.0, t.chrome);
                let tab = PALETTE_TAB.load(Ordering::Relaxed);
                let mut x = row.left();
                let mut picked = None;
                for (i, name) in ["Layers", "Properties"].iter().enumerate() {
                    let label = crate::i18n::t(name);
                    let g = p.layout_no_wrap(label.to_string(), egui::FontId::proportional(13.5), t.text);
                    let tw = (g.size().x + 28.0).max(76.0).min(row.width() / 2.0);
                    let tr = Rect::from_min_size(egui::pos2(x, row.top()), vec2(tw, tab_h));
                    let active = tab == i as u8;
                    let resp = ui.interact(tr, ui.id().with(("pt", i)), egui::Sense::click());
                    p.rect_filled(tr, 0.0, if active { t.tab_active } else { t.chrome });
                    p.text(
                        tr.center(),
                        Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(13.5),
                        if active { t.text } else { t.text_dim },
                    );
                    icons::describe_control(ui, &resp, name, Some(active), true);
                    if resp.clicked() {
                        picked = Some(i as u8);
                    }
                    x += tw;
                }
                if let Some(i) = picked {
                    PALETTE_TAB.store(i, Ordering::Relaxed);
                }
                p.rect_filled(Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(row.right() - 26.0, row.bottom())), 0.0, t.chrome);
                let cr = Rect::from_center_size(egui::pos2(row.right() - 13.0, row.center().y), vec2(24.0, 22.0));
                let cresp = ui.interact(cr, ui.id().with("palettes_close"), egui::Sense::click());
                icons::paint(&p, cr.shrink(5.0), Icon::ChevronRight, false);
                icons::describe_control(ui, &cresp, "Collapse Palettes", None, false);
                if cresp.on_hover_text(crate::tl!("Collapse Palettes")).clicked() {
                    close();
                }
                let body = Rect::from_min_max(egui::pos2(dr.left(), row.bottom()), dr.max);
                ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        if PALETTE_TAB.load(Ordering::Relaxed) == 0 {
                            crate::palettes::layers_section(app, ui);
                        } else {
                            crate::shortcut_menus::palette(app, ui, crate::palettes::properties_section, crate::shortcut_menus::properties_entries);
                        }
                    });
                });
            });
        });
    }
}

/// The phone's start page: the brand, the four ways to begin, About, and recent drawings.
/// Standard widgets in a scroll area: the page must fit under a 300-point-wide window with a
/// finger-sized target for every row, and About must still be reachable when there are recents.
pub fn start_page(app: &mut CadApp, ui: &mut Ui) {
    let t = Tokens::get();
    let r = ui.max_rect();
    ui.painter().rect_filled(r, 0.0, t.chrome);
    let mut action = None;
    let mut recent = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let w = (ui.available_width() - 44.0).clamp(180.0, 340.0);
        ui.vertical_centered(|ui| {
            ui.add_space(22.0);
            let (rect, _) = ui.allocate_exact_size(vec2(84.0, 84.0), egui::Sense::hover());
            brand_mark(ui.painter(), rect);
            ui.add_space(12.0);
            ui.label(egui::RichText::new(APP_NAME).size(28.0).color(t.text));
            ui.label(
                egui::RichText::new(crate::tf!("Version {version}", version = env!("CARGO_PKG_VERSION")))
                    .size(12.0)
                    .color(t.text_faint),
            );
            ui.add_space(22.0);
        });
        // Centred column of buttons.
        ui.horizontal(|ui| {
            let pad = ((ui.available_width() - w) / 2.0).max(0.0);
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(w);
                for (label, cmd) in [
                    ("Open...", "open"),
                    ("New", "new"),
                    ("New (metric)", "new.metric"),
                    ("Open sample drawing", "sample"),
                    ("About", "about"),
                ] {
                    let button = egui::Button::new(egui::RichText::new(crate::i18n::t(label)).size(16.0))
                        .fill(t.panel)
                        .stroke(Stroke::new(1.0, t.border));
                    if ui.add_sized([w, 46.0], button).clicked() {
                        action = Some(cmd);
                    }
                    ui.add_space(8.0);
                }
                if !app.ui.recent_files.is_empty() {
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new(crate::tl!("Recent")).size(17.0).color(t.text));
                    ui.add_space(4.0);
                    for path in app.ui.recent_files.iter().take(6) {
                        let label = crate::menus::file_name(path);
                        let button = egui::Button::new(egui::RichText::new(label).size(14.0)).fill(t.panel).stroke(Stroke::new(1.0, t.border));
                        let resp = ui.add_sized([w, 40.0], button).on_hover_text(path);
                        if resp.clicked() {
                            recent = Some(path.clone());
                        }
                        ui.add_space(6.0);
                    }
                }
                ui.add_space(18.0);
            });
        });
    });
    if let Some(path) = recent {
        app.open_path(&path);
    } else {
        match action {
            Some("open") => app.start("ui.open"),
            Some("new") => {
                app.start("new");
                app.ui.start_tab = false;
            }
            Some("new.metric") => {
                let _ = app.run("new", json!({ "metric": true }));
                app.ui.start_tab = false;
            }
            Some("sample") => app.start("ui.sample"),
            Some("about") => open_about(),
            _ => {}
        }
    }
}

/// The About page: the same frame as the other three Rustina apps — the brand, the version, what
/// the app is, who made it, the ArtCraft credit it stands on and the licence — with a back bar so
/// it sits in front of the start page instead of floating over it.
pub fn about_page(app: &mut CadApp, ui: &mut Ui) {
    let _ = app;
    let t = Tokens::get();
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let bar = Rect::from_min_size(r.min, vec2(r.width(), 46.0));
        let p = ui.painter().clone();
        p.rect_filled(bar, 0.0, t.chrome_dark);
        p.hline(bar.x_range(), bar.bottom(), Stroke::new(1.0, t.border));
        let br = Rect::from_min_size(egui::pos2(bar.left() + 6.0, bar.top() + 9.0), vec2(28.0, 28.0));
        let resp = ui.interact(br, ui.id().with("about_back"), egui::Sense::click());
        if resp.hovered() {
            p.rect_filled(br, 4.0, t.control_hover.gamma_multiply(0.6));
        }
        icons::paint(&p, br.shrink(4.0), Icon::ChevronLeft, false);
        icons::describe_control(ui, &resp, "Back", None, false);
        if resp.on_hover_text(crate::tl!("Back")).clicked() {
            close_about();
        }
        p.text(
            egui::pos2(bar.left() + 40.0, bar.center().y),
            Align2::LEFT_CENTER,
            crate::tl!("About"),
            egui::FontId::proportional(17.0),
            t.text,
        );
        ui.add_space(46.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(72.0, 72.0), egui::Sense::hover());
                brand_mark(ui.painter(), rect);
            });
            ui.add_space(12.0);
            let w = (ui.available_width() - 44.0).clamp(180.0, 340.0);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.label(egui::RichText::new(APP_NAME).size(22.0).color(t.text));
                    ui.label(
                        egui::RichText::new(crate::tf!("Version {version}", version = env!("CARGO_PKG_VERSION")))
                            .color(t.text_dim),
                    );
                    ui.label(crate::tl!("A free, open-source CAD app written from scratch in Rust."));
                    ui.label(crate::tl!("Part of Rustina Office."));
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Developer: Sina Khanzadeh").color(t.text));
                    ui.label(egui::RichText::new("Discord \u{b7} Telegram \u{b7} Instagram: @programmer_1998").small().color(t.text_dim));
                    ui.label(egui::RichText::new("GitHub: @programmer-1998").small().color(t.text_dim));
                    ui.label(egui::RichText::new("Email: khanzadeh.1377@gmail.com").small().color(t.text_dim));
                    ui.hyperlink_to("sina-khanzadeh.ir", "https://sina-khanzadeh.ir");
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(CREDIT).color(t.text_dim));
                    ui.hyperlink_to("getartcraft.com", "https://getartcraft.com");
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(crate::tl!(
                            "MIT OR Apache-2.0. Copyright (c) 2026 ArtCraft Team and the CADCraft contributors."
                        ))
                        .small()
                        .color(t.text_faint),
                    );
                    ui.add_space(24.0);
                });
            });
        });
    });
}

/// The phone status bar in two rows: the drafting toggles on top, the layout tabs and the
/// coordinate readout below. A single 26-point row was a wall of twenty icons.
pub fn status_bar(app: &mut CadApp, ui: &mut Ui) {
    let t = Tokens::get();
    egui::Panel::bottom("cc_status_phone").exact_size(54.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        if let Some((_, at)) = &app.status
            && crate::now_ms() - at < 5000.0
        {
            let left = std::time::Duration::try_from_secs_f64((5000.0 - (crate::now_ms() - at)) / 1000.0).unwrap_or_default();
            ui.ctx().request_repaint_after(left);
        }
        // Row 1: the toggles, icon only, in one line that scrolls when the language is long.
        let row1 = Rect::from_min_size(r.min, vec2(r.width(), 26.0));
        p.hline(row1.x_range(), row1.bottom(), Stroke::new(1.0, t.border));
        let s = app.session.settings.clone();
        let anno_all = app.session.doc().is_ok_and(|d| d.anno_all_visible());
        use cadcraft_engine::snap::mode;
        let toggles: Vec<(Icon, bool, &str, &str)> = vec![
            (Icon::Grid, s.gridmode, "grid", "Grid (F7)"),
            (Icon::Snap, s.snapmode, "snap", "Snap Mode (F9)"),
            (Icon::Ortho, s.orthomode, "ortho", "Ortho Mode (F8)"),
            (Icon::Polar, s.polarmode, "polar", "Polar Tracking (F10)"),
            (Icon::Isodraft, s.isodraft, "isodraft", "Isometric Drafting"),
            (Icon::OTrack, s.otrack, "otrack", "Object Snap Tracking (F11)"),
            (Icon::Osnap, s.osmode & mode::OFF == 0 && s.osmode != 0, "osnap", "Object Snap (F3)"),
            (Icon::Lineweight, s.lwdisplay, "lwdisplay", "Show/Hide Lineweight"),
            (Icon::Transparency, s.transparency_display, "transparencydisplay", "Show/Hide Transparency"),
            (Icon::DynInput, s.dynmode, "dynmode", "Dynamic Input (F12)"),
            (Icon::QuickProps, s.qpmode, "qpmode", "Quick Properties"),
            (Icon::Annotation, anno_all, "annovisible.toggle", "Annotation Visibility"),
            (Icon::Gear, false, "ui.dialog.dsettings", "Drafting Settings"),
        ];
        let size = 21.0;
        let mut x = row1.left() + 3.0;
        let mut toggle_cmd = None;
        for (icon, on, cmd, tip) in toggles {
            if x + size > row1.right() - 4.0 {
                break;
            }
            let br = Rect::from_min_size(egui::pos2(x, row1.center().y - size / 2.0), vec2(size, size));
            let resp = ui.interact(br, ui.id().with(("ptg", tip)), egui::Sense::click());
            if on {
                p.rect_filled(br, 3.0, t.toggle_on.gamma_multiply(0.35));
            } else if resp.hovered() {
                p.rect_filled(br, 3.0, t.control_hover.gamma_multiply(0.5));
            }
            icons::paint(&p, br.shrink(2.5), icon, false);
            if on {
                p.hline(br.x_range().shrink(4.0), br.bottom() - 1.0, Stroke::new(1.5, t.toggle_on));
            }
            icons::describe_control(ui, &resp, tip, Some(on), false);
            if resp.on_hover_text(crate::i18n::t(tip)).clicked() {
                toggle_cmd = Some(cmd);
            }
            x += size + 1.0;
        }
        match toggle_cmd {
            Some(c) if c.starts_with("ui.") => app.start(c),
            Some(c) => crate::menus::run_transparent(app, c),
            None => {}
        }
        // Row 2: layouts on the left, the readout on the right.
        let row2 = Rect::from_min_max(egui::pos2(r.left(), row1.bottom()), r.max);
        let mut tab_x = row2.left() + 6.0;
        let mut tabs: Vec<String> = vec!["Model".into()];
        let cur = app.session.state().map(|s| match &s.space {
            cadcraft_doc::Space::Model => "Model".to_string(),
            cadcraft_doc::Space::Paper(n) => n.clone(),
        });
        if let Ok(st) = app.session.state() {
            let mut ls: Vec<_> = st.doc.layouts.iter().collect();
            ls.sort_by_key(|l| l.tab_order);
            tabs.extend(ls.iter().map(|l| l.name.clone()));
        }
        let coord = app.canvas.cursor.map(|c| {
            let (lu, lp) = app.session.doc().map(|d| (d.header.i64("LUNITS", 2), d.header.i64("LUPREC", 4))).unwrap_or((2, 4));
            format!(
                "{}, {}",
                cadcraft_engine::units::format_distance(c.x, lu, lp),
                cadcraft_engine::units::format_distance(c.y, lu, lp)
            )
        });
        let readout = app.status.as_ref().filter(|(_, at)| crate::now_ms() - at < 5000.0).map(|(m, _)| m.clone()).or(coord);
        let readout_w = readout.as_ref().map(|s| p.layout_no_wrap(s.clone(), crate::theme::small(), t.text).size().x + 12.0).unwrap_or(0.0);
        let limit = (row2.right() - readout_w - 6.0).max(tab_x);
        let active_tab = tabs.iter().position(|n| cur.as_deref().is_ok_and(|c| c == n));
        let mut switch = None;
        for (i, name) in tabs.iter().enumerate() {
            let label = if i == 0 { crate::tl!("Model").to_string() } else { name.clone() };
            let g = p.layout_no_wrap(label.clone(), crate::theme::body(), t.text);
            let w = (g.size().x + 22.0).min(120.0);
            let hidden = i > 0 && tab_x + w + 26.0 > limit;
            if hidden && i > 1 {
                break;
            }
            let tr = Rect::from_min_size(egui::pos2(tab_x, row2.top() + 2.0), vec2(w, row2.height() - 6.0));
            let active = active_tab == Some(i);
            let resp = ui.interact(tr, ui.id().with(("phone_lt", i)), egui::Sense::click());
            if active {
                p.rect_filled(tr, 2.0, t.tab_active);
            } else if resp.hovered() {
                p.rect_filled(tr, 2.0, t.control.gamma_multiply(0.5));
            }
            let clip = p.with_clip_rect(tr.shrink(2.0));
            clip.text(tr.center(), Align2::CENTER_CENTER, label, crate::theme::body(), if active { t.text } else { t.text_dim });
            if resp.clicked() {
                switch = Some(name.clone());
            }
            crate::shortcut_menus::attach(app, &resp, |app| crate::shortcut_menus::layout_tab_entries(app, name));
            tab_x += w + 2.0;
            if i == 0 {
                let pr = Rect::from_min_size(egui::pos2(tab_x, row2.center().y - 8.0), vec2(20.0, 20.0));
                let presp = ui.interact(pr, ui.id().with("phone_layout_plus"), egui::Sense::click());
                icons::paint(&p, pr.shrink(3.0), Icon::Plus, false);
                if presp.clicked()
                    && let Ok(v) = app.run("layout.new", json!({}))
                    && let Some(n) = v.get("name").and_then(serde_json::Value::as_str)
                {
                    switch = Some(n.to_string());
                }
                tab_x += 22.0;
            }
        }
        if let Some(n) = switch {
            let _ = app.run("layout.set", json!({ "name": n }));
        }
        if let Some(text) = readout {
            let color = if app.status.as_ref().is_some_and(|(_, at)| crate::now_ms() - at < 5000.0) { t.warn } else { t.text_dim };
            let rect = Rect::from_min_max(egui::pos2(limit, row2.top()), egui::pos2(row2.right() - 6.0, row2.bottom()));
            let clip = p.with_clip_rect(rect);
            clip.text(egui::pos2(rect.right(), row2.center().y), Align2::RIGHT_CENTER, text.clone(), crate::theme::small(), color);
            ui.interact(rect, ui.id().with("phone_readout"), egui::Sense::hover()).on_hover_text(text);
        }
    });
}
