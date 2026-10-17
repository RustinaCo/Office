//! Status bar (slide counter, language, notes/comments toggles, view buttons, zoom slider) and the
//! notes pane under the slide.

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};
use crate::{SlideApp, ViewMode};

pub fn status_bar(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status").exact_size(28.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let rect = ui.max_rect();
        let Some(st) = app.session.active() else { return };
        let n = st.doc.slides.len();
        let left = match st.selection.target {
            deckcraft_engine::Target::Slides if crate::mobile() => {
                format!("{}/{}", (st.selection.slide + 1).min(n.max(1)), n)
            }
            deckcraft_engine::Target::Slides => format!("Slide {} of {}", (st.selection.slide + 1).min(n.max(1)), n),
            _ => "Slide Master".to_string(),
        };
        let p = ui.painter();
        p.text(pos2(rect.min.x + 14.0, rect.center().y), Align2::LEFT_CENTER, &left, theme::font(12.0), t.text_dim);
        if !crate::mobile() {
            p.text(pos2(rect.min.x + 120.0, rect.center().y), Align2::LEFT_CENTER, "English (United States)", theme::font(12.0), t.text_dim);
        }
        // Transient status message.
        if let Some((msg, at)) = app.status.clone() {
            if crate::now_ms() - at < 6000.0 {
                p.text(pos2(rect.min.x + 290.0, rect.center().y), Align2::LEFT_CENTER, &msg, theme::font(12.0), t.accent);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            } else {
                app.status = None;
            }
        }
        let right_w = if crate::mobile() { 480.0 } else { 560.0 };
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(rect.max.x - right_w, rect.min.y), pos2(rect.max.x - 8.0, rect.max.y)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 4.0;
        if small_icon(&mut right, Icon::FitSmall, "Fit slide to current window", false).clicked() {
            app.ui.zoom = None;
        }
        let pct = (app.ui.zoom.unwrap_or(app.canvas_scale) * 100.0).round();
        let lbl = right.add(egui::Label::new(egui::RichText::new(format!("{pct}%")).size(12.0).color(t.text_dim)).sense(Sense::click()));
        if lbl.clicked() {
            app.dialog = Some(crate::dialogs::Dialog::new("zoom"));
        }
        if small_icon(&mut right, Icon::ZoomIn, "Zoom In", false).clicked() {
            let _ = app.run("view.zoomIn", json!({}));
        }
        // Slider 10%–400% (log scale).
        let mut z = app.ui.zoom.unwrap_or(app.canvas_scale).clamp(0.1, 4.0);
        let mut lz = z.ln();
        let s = right.add_sized(vec2(110.0, 18.0), egui::Slider::new(&mut lz, (0.1f32).ln()..=(4.0f32).ln()).show_value(false));
        if s.changed() {
            z = lz.exp();
            app.ui.zoom = Some(z);
        }
        if small_icon(&mut right, Icon::ZoomOut, "Zoom Out", false).clicked() {
            let _ = app.run("view.zoomOut", json!({}));
        }
        right.add_space(8.0);
        let view = app.ui.view;
        if small_icon(&mut right, Icon::ViewShowSmall, "Slide Show", false).clicked() {
            let i = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
            app.start_show(i, false);
        }
        if small_icon(&mut right, Icon::ViewReadingSmall, "Reading View", false).clicked() {
            let _ = app.run("view.reading", json!({}));
        }
        if small_icon(&mut right, Icon::ViewSorterSmall, "Slide Sorter", view == ViewMode::Sorter).clicked() {
            app.ui.view = ViewMode::Sorter;
        }
        if small_icon(&mut right, Icon::ViewNormalSmall, "Normal", view == ViewMode::Normal).clicked() {
            app.ui.view = ViewMode::Normal;
        }
        right.add_space(8.0);
        let c_on = app.ui.pane.as_deref() == Some("comments");
        if labeled(&mut right, Icon::CommentsSmall, "Comments", c_on).clicked() {
            let _ = app.run("view.pane", json!({"pane": "comments", "toggle": true}));
        }
        let n_on = app.ui.notes;
        if labeled(&mut right, Icon::NotesSmall, "Notes", n_on).clicked() {
            app.ui.notes = !n_on;
        }
        // The slide pane's own toggle, the footer's first icon from the left: it reflects whether
        // the pane is showing (the phone filmstrip in portrait, the side list in landscape). The
        // old floating button over the canvas is gone.
        let slides_on = !app.ui.thumbs_collapsed;
        if small_icon(&mut right, Icon::List, if slides_on { "Hide slides" } else { "Show slides" }, slides_on).clicked() {
            app.ui.thumbs_collapsed = !app.ui.thumbs_collapsed;
        }
        let _ = Stroke::NONE;
    });
}

/// Landscape rail: the footer as a vertical strip on the right — slide
/// position, zoom and views, all 44pt touch targets. Mirrors `status_bar`.
pub fn side_rail(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("footer_rail").exact_size(56.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        // Owned snapshot first: the rows below borrow `app` mutably.
        let snap = app.session.active().map(|st| {
            let n = st.doc.slides.len();
            let pos = match st.selection.target {
                deckcraft_engine::Target::Slides => format!("{}/{}", (st.selection.slide + 1).min(n.max(1)), n),
                _ => "M".to_string(),
            };
            let slide = st.selection.slide;
            (pos, slide)
        });
        let Some((pos, slide)) = snap else { return };
        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(pos).size(11.0).color(t.text_dim)));
            ui.add_space(2.0);
            let btn = |ui: &mut Ui, icon: Icon, tip: &str, on: bool| {
                let (r, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
                if on || resp.hovered() {
                    ui.painter().rect_filled(r, 6.0, if on { t.selected } else { t.hover });
                }
                icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(18.0, 18.0)), icon, t.text_dim, false);
                resp.on_hover_text(tip)
            };
            // The slide pane's toggle, as in the portrait footer.
            let slides_on = !app.ui.thumbs_collapsed;
            if btn(ui, Icon::List, if slides_on { "Hide slides" } else { "Show slides" }, slides_on).clicked() {
                app.ui.thumbs_collapsed = !app.ui.thumbs_collapsed;
            }
            if btn(ui, Icon::ZoomIn, "Zoom In", false).clicked() {
                let _ = app.run("view.zoomIn", json!({}));
            }
            let pct = (app.ui.zoom.unwrap_or(app.canvas_scale) * 100.0).round();
            let lbl = ui.add(egui::Label::new(egui::RichText::new(format!("{pct}%")).size(11.0).color(t.text_dim)).sense(Sense::click()));
            if lbl.clicked() {
                app.dialog = Some(crate::dialogs::Dialog::new("zoom"));
            }
            if btn(ui, Icon::ZoomOut, "Zoom Out", false).clicked() {
                let _ = app.run("view.zoomOut", json!({}));
            }
            ui.separator();
            let view = app.ui.view;
            if btn(ui, Icon::ViewShowSmall, "Slide Show", false).clicked() {
                app.start_show(slide, false);
            }
            if btn(ui, Icon::ViewReadingSmall, "Reading View", false).clicked() {
                let _ = app.run("view.reading", json!({}));
            }
            if btn(ui, Icon::ViewSorterSmall, "Slide Sorter", view == ViewMode::Sorter).clicked() {
                app.ui.view = ViewMode::Sorter;
            }
            if btn(ui, Icon::ViewNormalSmall, "Normal", view == ViewMode::Normal).clicked() {
                app.ui.view = ViewMode::Normal;
            }
        });
    });
}

fn small_icon(ui: &mut Ui, icon: Icon, tip: &str, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
    if on || resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), if on { t.selected } else { t.hover });
    }
    icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(16.0, 16.0)), icon, t.text_dim, false);
    resp.on_hover_text(tip)
}

fn labeled(ui: &mut Ui, icon: Icon, label: &str, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let w = 22.0 + ui.fonts_mut(|f| f.layout_no_wrap(label.into(), theme::font(12.0), t.text).size().x) + 8.0;
    let (r, resp) = ui.allocate_exact_size(vec2(w, 20.0), Sense::click());
    if on || resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(4), if on { t.selected } else { t.hover });
    }
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.min.x + 11.0, r.center().y), vec2(15.0, 15.0)), icon, t.text_dim, false);
    ui.painter().text(pos2(r.min.x + 22.0, r.center().y), Align2::LEFT_CENTER, label, theme::font(12.0), t.text_dim);
    resp
}

/// The speaker notes editor under the slide.
pub fn notes_pane(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(st) = app.session.active() else { return };
    if st.selection.target != deckcraft_engine::Target::Slides {
        return;
    }
    let idx = st.selection.slide;
    let uid = st.uid;
    let current = st.current_slide().map(|s| s.notes_text()).unwrap_or_default();
    egui::Panel::bottom("notes")
        .resizable(true)
        .default_size(app.ui.notes_height)
        .size_range(36.0..=400.0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(8, 6)))
        .show(ui, |ui| {
            app.ui.notes_height = ui.available_height() + 12.0;
            let rect = ui.max_rect();
            ui.painter()
                .line_segment([pos2(rect.min.x - 8.0, rect.min.y - 6.0), pos2(rect.max.x + 8.0, rect.min.y - 6.0)], Stroke::new(1.0, t.separator));
            // Keep a local buffer per slide; commit when it changes and the field loses focus.
            let id = egui::Id::new("notes_edit");
            let focused = ui.ctx().memory(|m| m.has_focus(id));
            // Focus can leave in the frame before the field is drawn (Escape, a click on a
            // thumbnail): what was typed is still committed, to the slide it was typed for.
            let was_focused = ui.ctx().memory(|m| m.had_focus_last_frame(id));
            if app.notes_buf.0 != uid || app.notes_buf.1 != idx {
                let (old_uid, old_idx, typed) = std::mem::replace(&mut app.notes_buf, (uid, idx, current.clone()));
                let old = app.session.active().and_then(|d| d.doc.slides.get(old_idx)).map(|s| s.notes_text());
                if was_focused && old_uid == uid && old.is_some_and(|o| o != typed) {
                    let _ = app.run("slide.notes", json!({"text": typed, "index": old_idx}));
                }
            }
            if !focused && !was_focused && app.notes_buf.2 != current {
                app.notes_buf.2 = current.clone();
            }
            let mut buf = app.notes_buf.2.clone();
            let r = ui.add(
                egui::TextEdit::multiline(&mut buf)
                    .id(id)
                    .hint_text("Click to add notes")
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .font(theme::font(13.0)),
            );
            app.notes_buf.2 = buf.clone();
            if r.lost_focus() && buf != current {
                let _ = app.run("slide.notes", json!({"text": buf, "index": idx}));
            }
        });
}
